#!/usr/bin/env python3
"""Standalone unit tests for `tools/lldb-cobrust/printers.py` (ADR-0059a
Phase L wave-2 §6.1 honest-cite + §6.2 fallback path).

Runs WITHOUT lldb installed. Mocks the lldb SBValue / SBProcess /
SBTarget surfaces the printer module touches, so the byte-decode +
fallback logic can be exercised in CI even when lldb-18 is not
available.

Why this matters:

- Mac dev hosts often lack lldb-18 (brew installs unversioned `lldb`).
- The `crates/cobrust-codegen/tests/dwarf_lldb_smoke.rs` tests skip
  when `lldb-18` is not on PATH (per `find_lldb()` helper).
- The printer's byte-decode logic (StringBuffer Vec<u8> layout walk,
  UTF-8 fallback, dict tag-dispatch fallback) is testable
  independently of lldb itself — this test exercises that surface.

Run:

    python3 tools/lldb-cobrust/tests/test_printers.py
"""

import os
import sys
import struct
import unittest
import importlib.util

# =====================================================================
# lldb mock — minimal SB API surface the printer touches.
# =====================================================================


class MockSBError:
    """Mocks the `lldb.SBError` API the printer touches."""

    def __init__(self, success=True):
        self._success = success

    def Success(self):
        return self._success

    def Fail(self):
        return not self._success


class MockSBProcess:
    """Mocks `lldb.SBProcess` — backing memory is a dict of byte ranges."""

    def __init__(self, memory):
        # memory: dict[int -> bytes], keyed by base address.
        self._memory = memory

    def _resolve(self, addr, count):
        # Find the smallest base >= 0 whose range covers [addr, addr+count).
        for base, blob in self._memory.items():
            if base <= addr < base + len(blob):
                offset = addr - base
                if offset + count <= len(blob):
                    return blob[offset : offset + count]
        return None

    def ReadPointerFromMemory(self, addr, err):
        raw = self._resolve(addr, 8)
        if raw is None:
            err._success = False
            return 0
        err._success = True
        return int.from_bytes(raw, "little")

    def ReadMemory(self, addr, count, err):
        raw = self._resolve(addr, count)
        if raw is None:
            err._success = False
            return None
        err._success = True
        return raw


class MockSBTarget:
    """Mocks `lldb.SBTarget` — supports EvaluateExpression returning
    a mock SBValue whose GetValueAsSigned / GetValueAsUnsigned read
    from a pre-populated `expressions` dict."""

    def __init__(self, expressions=None):
        self._expressions = expressions or {}

    def GetProcess(self):
        return getattr(self, "_process", None)

    def __bool__(self):
        return True

    def EvaluateExpression(self, expr):
        # Best-effort match: any registered expression key that is a
        # substring of the input is returned. Default to a failing
        # mock value.
        for needle, value in self._expressions.items():
            if needle in expr:
                return MockSBValue(value)
        return MockSBValue(None, success=False)


class MockSBValue:
    """Mocks `lldb.SBValue` — pointer-address-backed."""

    def __init__(self, value, success=True):
        self._value = value
        self._success = success

    def IsValid(self):
        return self._success and self._value is not None

    def GetError(self):
        return MockSBError(success=self._success)

    def GetTarget(self):
        return getattr(self, "_target", None)

    def GetValueAsUnsigned(self, err=None, default=0):
        if isinstance(err, MockSBError):
            err._success = self._success
        if not self._success or self._value is None:
            return default
        return self._value

    def GetValueAsSigned(self, default=0):
        if not self._success or self._value is None:
            return default
        # Reinterpret as signed i64.
        v = self._value & ((1 << 64) - 1)
        if v >= (1 << 63):
            v -= 1 << 64
        return v


# Inject a `lldb` stub module BEFORE importing printers.py so the
# `import lldb` line at the top of printers.py resolves.
class LLDBMock:
    SBError = MockSBError
    LLDB_INVALID_ADDRESS = 0xFFFFFFFFFFFFFFFF


sys.modules["lldb"] = LLDBMock

# Locate printers.py relative to this test file (regardless of cwd).
_PRINTERS_PATH = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "printers.py"
)
_spec = importlib.util.spec_from_file_location("printers", _PRINTERS_PATH)
printers = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(printers)


# =====================================================================
# Helpers — build fake StringBuffer / dict layouts.
# =====================================================================


def _build_string_buffer(bytes_payload):
    """Build a memory map containing a `StringBuffer { bytes: Vec<u8> }`
    at addr `0x10000`, with the inner Vec data at `0x20000`. Returns
    `(buffer_addr, memory_dict)`."""
    buffer_addr = 0x10000
    data_addr = 0x20000
    length = len(bytes_payload)
    # Vec<u8> layout: { *mut u8, usize cap, usize len } (24 bytes).
    sb = struct.pack("<QQQ", data_addr, length, length)
    return buffer_addr, {buffer_addr: sb, data_addr: bytes_payload}


# =====================================================================
# Tests
# =====================================================================


class TestStrSummary(unittest.TestCase):
    """ADR-0059a §6.1 honest-cite — the StringBuffer byte-decode logic
    inside `cobrust_str_summary` is exercised here against synthetic
    bytes (no lldb required)."""

    def test_decodes_ascii_hello(self):
        addr, memory = _build_string_buffer(b"hello")
        process = MockSBProcess(memory)
        target = MockSBTarget()
        target._process = process
        valobj = MockSBValue(addr)
        valobj._target = target
        summary = printers.cobrust_str_summary(valobj, {})
        self.assertEqual(summary, '"hello"')

    def test_decodes_utf8_multibyte(self):
        # 你好 is U+4F60 U+597D = e4 bd a0 e5 a5 bd in UTF-8.
        addr, memory = _build_string_buffer("你好".encode("utf-8"))
        process = MockSBProcess(memory)
        target = MockSBTarget()
        target._process = process
        valobj = MockSBValue(addr)
        valobj._target = target
        summary = printers.cobrust_str_summary(valobj, {})
        self.assertEqual(summary, '"你好"')

    def test_invalid_utf8_uses_replace_fallback(self):
        # Single invalid byte 0xff — should render as the replacement char.
        addr, memory = _build_string_buffer(b"\xff")
        process = MockSBProcess(memory)
        target = MockSBTarget()
        target._process = process
        valobj = MockSBValue(addr)
        valobj._target = target
        summary = printers.cobrust_str_summary(valobj, {})
        # The 'replace' fallback substitutes U+FFFD.
        self.assertIn("�", summary)
        # Must not crash, must wrap in quotes.
        self.assertTrue(summary.startswith('"'))
        self.assertTrue(summary.endswith('"'))

    def test_null_ptr_renders_empty_string(self):
        process = MockSBProcess({})
        target = MockSBTarget()
        target._process = process
        valobj = MockSBValue(0)
        valobj._target = target
        summary = printers.cobrust_str_summary(valobj, {})
        self.assertEqual(summary, '""')

    def test_escapes_embedded_quotes(self):
        addr, memory = _build_string_buffer(b'he said "hi"')
        process = MockSBProcess(memory)
        target = MockSBTarget()
        target._process = process
        valobj = MockSBValue(addr)
        valobj._target = target
        summary = printers.cobrust_str_summary(valobj, {})
        self.assertEqual(summary, '"he said \\"hi\\""')


class TestDictSummaryFallback(unittest.TestCase):
    """ADR-0059a §6.2 — verify the wave-1 placeholder fallback path
    fires when the runtime accessors are not resolvable (i.e. object
    file only, no executable, no process)."""

    def test_null_dict_renders_empty_braces(self):
        process = MockSBProcess({})
        target = MockSBTarget()
        target._process = process
        valobj = MockSBValue(0)
        valobj._target = target
        summary = printers.cobrust_dict_summary(valobj, {})
        self.assertEqual(summary, "{}")

    def test_unresolved_accessors_fall_back_to_placeholder(self):
        # Build a memory map where the first 8 bytes at the dict ptr
        # encode a length hint of 3 (the wave-1 best-effort path
        # reads this as the entry count). The mock target has NO
        # registered EvaluateExpression results — the printer should
        # fall back to `{<3 entries>}` per the wave-2 path 2 fallback.
        dict_addr = 0x30000
        memory = {dict_addr: struct.pack("<Q", 3)}
        process = MockSBProcess(memory)
        target = MockSBTarget(expressions={})  # No accessor symbols.
        target._process = process
        valobj = MockSBValue(dict_addr)
        valobj._target = target
        summary = printers.cobrust_dict_summary(valobj, {})
        self.assertEqual(summary, "{<3 entries>}")

    def test_resolved_accessors_render_kv_walk(self):
        # Mock: tag_key=0 (i64), tag_value=0 (i64), len=2, items 7→70, 5→50.
        dict_addr = 0x40000
        process = MockSBProcess({dict_addr: struct.pack("<Q", 2)})
        target = MockSBTarget(
            expressions={
                "__cobrust_dict_key_tag": 0,
                "__cobrust_dict_value_tag": 0,
                "__cobrust_dict_len": 2,
                # `iter_key_i64_at` / `iter_value_i64_at` — the printer
                # passes the index as part of the expression; we register
                # the substring once and the substring-match returns
                # the same value for both indices, so this is a
                # minimum-viable smoke that confirms the rendering
                # composes `{k: v, k: v}` correctly.
                "__cobrust_dict_iter_key_i64_at": 42,
                "__cobrust_dict_iter_value_i64_at": 100,
            }
        )
        target._process = process
        valobj = MockSBValue(dict_addr)
        valobj._target = target
        summary = printers.cobrust_dict_summary(valobj, {})
        # Two entries; same key/value each due to substring-mock.
        # The contract being verified: tag-dispatch path executes
        # (not the placeholder fallback) when accessors resolve.
        self.assertEqual(summary, "{42: 100, 42: 100}")


class TestOptionAdtSummary(unittest.TestCase):
    """ADR-0059a §6.3 — generic Adt printer ptr-tag rendering."""

    def test_null_ptr_renders_none(self):
        process = MockSBProcess({})
        target = MockSBTarget()
        target._process = process
        valobj = MockSBValue(0)
        valobj._target = target
        summary = printers.cobrust_option_summary(valobj, {})
        self.assertEqual(summary, "None")

    def test_non_null_ptr_renders_some_addr(self):
        process = MockSBProcess({})
        target = MockSBTarget()
        target._process = process
        valobj = MockSBValue(0xDEADBEEF)
        valobj._target = target
        summary = printers.cobrust_option_summary(valobj, {})
        self.assertEqual(summary, "Some(<0xdeadbeef>)")


class TestListSummary(unittest.TestCase):
    """Wave-1 List printer regression guard — verify the bracket walk
    still works after the wave-2 printer edits."""

    def test_empty_list_renders_brackets(self):
        process = MockSBProcess({})
        target = MockSBTarget()
        target._process = process
        valobj = MockSBValue(0)
        valobj._target = target
        summary = printers.cobrust_list_summary(valobj, {})
        self.assertEqual(summary, "[]")

    def test_list_with_three_i64_elements(self):
        # ListI64Layout { items_ptr, len, cap } — 24 bytes.
        list_addr = 0x50000
        items_addr = 0x60000
        layout = struct.pack("<qqq", items_addr, 3, 3)
        # Three i64s at items_addr.
        items_bytes = struct.pack("<qqq", 10, 20, 30)
        memory = {list_addr: layout, items_addr: items_bytes}
        process = MockSBProcess(memory)
        target = MockSBTarget()
        target._process = process
        valobj = MockSBValue(list_addr)
        valobj._target = target
        summary = printers.cobrust_list_summary(valobj, {})
        self.assertEqual(summary, "[10, 20, 30]")


# =====================================================================
# Entry point
# =====================================================================

if __name__ == "__main__":
    unittest.main(verbosity=2)
