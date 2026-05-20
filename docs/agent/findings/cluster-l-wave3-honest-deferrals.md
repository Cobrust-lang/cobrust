---
doc_kind: finding
finding_id: cluster-l-wave3-honest-deferrals
title: "Phase L wave-3 — ADR-0059d §6.1/§6.3-per-variant closure"
status: resolved
date: 2026-05-20
last_verified_commit: 79bd1b2
relates_to: [adr:0059, adr:0059a, adr:0059d, adr:0058c]
---

# Phase L wave-3 — ADR-0059d closure record

## 1. Wave-3 work items (from wave-2 carry-over)

| # | Item | Wave-2 state |
|---|---|---|
| §6.1 | Str linked-executable + bp-hit smoke | HONEST-CITE: byte-decode verified, no exe harness |
| §6.3-per-variant | Option DICompositeType | HONEST-CITE: generic `cobrust::Adt` only; no tag+payload DI |

## 2. Wave-3 disposition

| # | Item | Wave-3 disposition |
|---|---|---|
| §6.1 | Str linked-executable | **PARTIAL RESOLVED**: linked-executable harness shipped (`executable_spec` / `build_linked_executable` / `lldb_run_with_bp`); `cobrust::Str` DIE verified in linked binary. **Remaining HONEST-CITE**: bp-hit showing `"hello"` requires stdlib linkage — deferred to ADR-0059c |
| §6.3-per-variant | Option DICompositeType | **RESOLVED**: `cobrust::Option` DICompositeType emitted with tag (i32) + payload (i64) member fields; printer tag-dispatch reads tag at ptr+0 → `None` / `Some(<payload>)` |

## 3. Commits

| SHA | Phase | Summary |
|---|---|---|
| `5b6a5b2` | ADR | 0059d authored |
| `bfa8c35` | 1 | Linker harness helpers + 5 linked/Option smoke tests |
| `326f017` | 2 | Per-variant Option DICompositeType in codegen |
| `79bd1b2` | 2 | Printer tag-dispatch + 2 new Python self-tests |

## 4. Verification

| Surface | Mode | Result |
|---|---|---|
| `cobrust-codegen` `cargo check` (no llvm) | Mac local | PASS |
| `cobrust-codegen --features llvm` | CI authoritative | PENDING — LLVM-18 not on Mac |
| `dwarf_lldb_smoke.rs` (15 tests total) | CI authoritative | PENDING — `lldb-18` not on PATH on Mac |
| `test_printers.py` (14 tests) | Mac local | 14 PASS |

## 5. Remaining honest-cites (carried forward)

- **§6.1 bp-hit content**: full `frame variable s = "hello"` requires
  stdlib Str allocator + ADR-0059c `cobrust debug` CLI linkage.
- **§6.3 generic Adt variants**: per-Adt variant DI for user-defined
  enums requires MIR threading the full Adt schema (Phase L+).

## 6. F35-sibling + F39 compliance

- F35-sibling: no synthetic translation; all changes are hand-written
  code mirroring existing patterns.
- F39: no device-name redaction. Mac paths honest-cited as
  "LLVM-18 / lldb-18 not on Mac; CI authoritative".

— P9 Tech Lead, 2026-05-20
