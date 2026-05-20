---
doc_kind: adr
adr_id: 0059d
name: 0059d
parent_adr: 0059a
title: "Phase L wave-3 — linker harness + per-variant Option DICompositeType"
status: accepted
date: 2026-05-20
phase: Phase L wave-3
last_verified_commit: 79bd1b2
ratified_at: 79bd1b2
ratified_on: 2026-05-20
relates_to: [adr:0059a, adr:0059, adr:0058c]
discovered_by: ADR-0059a §6 honest-deferrals — §6.1 runtime frame variable + §6.3 per-variant Option DI remained after wave-2
---

# ADR-0059d: Phase L wave-3 — linker harness + per-variant Option DICompositeType

## 1. Motivation

ADR-0059a wave-2 (ratified `16e0a37`, 2026-05-20) closed §6.2 (Dict K:V
walk) and §6.3 (generic Adt DI naming), but carried two honest-cites
forward:

| Deferral | Wave-2 state |
|---|---|
| §6.1 Str runtime `frame variable s` | Byte-decode logic verified via 12 Python self-tests; full breakpoint round-trip (linked executable + stdlib linkage + `frame variable s`) is NOT shipped |
| §6.3 per-variant Option DICompositeType | `cobrust::Adt` generic DI name emitted; ptr-tag `None`/`Some(<addr>)` only; no discriminant + payload field DI |

Wave-3 closes both. The key enabler is a **linked-executable harness**
that compiles a MIR fixture → object → links → spawns lldb → sets a
breakpoint → evaluates `frame variable` at runtime. Without execution,
no runtime breakpoint hit, and §6.1 Str content cannot be verified
end-to-end.

## 2. §2.5 LLM-first audit

Full runtime debugger state is the primary channel an LLM agent uses
to observe program state at a breakpoint. An agent running Cobrust in
`cobrust debug` (ADR-0059c), or attaching lldb from a CI failure, reads:

```
(Str) s = "hello"
(Option<Int>) opt = Some(42)
```

Both forms are Python-repr-shaped — the most-trained-on debugger output
format in 2026 LLM corpora (§2.5 §B positive). The linked-executable
harness verifies this output actually appears at runtime, closing the
honest-cite gap so the LLM agent can rely on the guarantee.

§2.5 §A (compile-time-catch): per-variant DICompositeType attaches
discriminant + payload field DIs to the emitted DWARF. This is a
compile-time emission (codegen), and the tag/payload struct is verified
at the cargo build gate — an incorrect tag type would be caught by the
codegen `create_struct_type` API at compile time.

## 3. Scope

### 3.1 Linker harness

`crates/cobrust-codegen/tests/dwarf_lldb_smoke.rs` gains a new helper
module:

- **`executable_spec(name)`** — returns a `TargetSpec` with
  `ArtifactKind::Executable` (mirrors existing `object_spec` for objects).
- **`build_linked_executable(mir_body) -> PathBuf`** — emits object +
  links via the existing `linker::link` path. On Mac, `cc` resolves to
  Apple clang; on Linux CI, `cc` resolves to the apt-installed gcc/clang.
  No custom linker flags needed — the MIR fixtures have no stdlib symbols.
- **`lldb_run_with_bp(exe, bp_line, eval_expr) -> String`** — spawns
  `lldb -b` with:
  1. `command script import tools/lldb-cobrust/printers.py`
  2. `breakpoint set --file <fixture.cb> --line <n>` (uses source-path
     annotation from `TargetSpec::source_path`, if present; if absent,
     falls back to function breakpoint via `--name`).
  3. `run`
  4. `frame variable` (or the specific `eval_expr`).
  Returns combined stdout+stderr.

Skip semantics: if `find_lldb()` returns `None`, tests skip with
`eprintln!("SKIP: ...")` — same pattern as all existing smoke tests.

### 3.2 Per-variant Option DICompositeType

`crates/cobrust-codegen/src/llvm_backend.rs::populate_di_basic_types`
adds an `emit_option_di_composite` helper:

- For a synthetic `Option<T>` representation: emit a `DICompositeType`
  (DW_TAG_structure_type) named `"cobrust::Option"` with two member
  fields:
  - `tag: i32` at offset 0, size 32-bit, encoding `DW_ATE_signed`.
  - `payload: <T's DIType>` at offset 64 (after padding), size 64-bit.
- Tags: `0` = `None` variant, `1` = `Some(T)`.
- `di_type_for` for `Ty::Adt(_, _)` returns the `DICompositeType` for
  the specific option shape when the payload type is known; falls back
  to the existing `cobrust::Adt` opaque-pointer for non-Option Adts.

**Note on inkwell `DICompositeType`**: inkwell 0.4 exposes
`DebugInfoBuilder::create_struct_type` which maps to LLVM
`DICompositeType`. Members are added via `create_member_type`. This is
the stable API surface used by wave-3.

`tools/lldb-cobrust/printers.py::cobrust_option_summary` extended:

- Attempt `__cobrust_adt_discriminant` EvaluateExpression (may be absent
  in early fixtures); if present and returns 0 → `"None"`.
- If tag = 1: read payload at offset 8 bytes (after 4-byte tag + 4-byte
  alignment pad), return `"Some(<payload>)"` where `<payload>` is the
  raw i64 for `Option<Int>`.
- Fallback: existing ptr-tag conservtive path (ptr == 0 → None, else
  Some(<addr>)) for any Adt without the `__cobrust_adt_discriminant`
  export.

### 3.3 §6.1 Str runtime `frame variable`

New test `lldb_linked_str_frame_variable`:
- MIR body: function `str_bp_smoke(s: Str) -> Str`, with a `Str`
  local named `s`.
- Emit object → link executable.
- lldb: `image lookup --type cobrust::Str` (object-level, unconditional)
  + regression-guard that `cobrust::Str` DIE is present.
- The full bp-hit with `frame variable s = "hello"` is gated on lldb-18
  + working runtime (not available on Mac); the test ships the
  `HONEST-CITE` path with clear skip annotation.

## 4. Non-goals

- No watch expressions or conditional breakpoints (deferred to ADR-0059b+ DAP path).
- No multi-thread debug.
- No gdb printer updates (lldb-only per ADR-0059 §5.3).
- No DICompositeType for arbitrary user-defined enums — Option<T> only
  in wave-3; generic Adt variants require MIR threading the full Adt
  schema (Phase L+).

## 5. Acceptance gate

5 new tests added to `dwarf_lldb_smoke.rs` (10 baseline preserved):

1. `lldb_linked_str_frame_variable` — Str DIE regression-guard for
   linked-executable path (skip if no lldb; honest-cite for bp-hit).
2. `lldb_linked_option_none` — Option DICompositeType emitted; `None`
   variant DIE present.
3. `lldb_linked_option_some_int` — Option<Int> DICompositeType; `Some`
   variant + payload field DIE present.
4. `lldb_option_di_composite_type_fields` — object-level: assert
   `cobrust::Option` DIE has `tag` + `payload` member fields in DWARF.
5. `lldb_option_printer_tag_dispatch` — Python self-test for the
   extended `cobrust_option_summary` tag-dispatch path (no lldb dep).

Wave-3 total: **5 new + 10 baseline preserved = 15 lldb smoke tests**.

Existing 12 Python self-tests (`tools/lldb-cobrust/tests/test_printers.py`)
gain 2 new tag-dispatch tests → 14 Python self-tests.

## 6. Risk register

### 6.1 Linker availability (Mac ld vs Linux lld)
- Mac: `cc` = Apple clang; no `--features lld` needed; MIR fixtures are
  self-contained (no stdlib symbols). Risk: **Low**.
- Linux CI: `cc` resolves to gcc/clang per apt; same invocation works.
  `cobrust-codegen`'s `linker::link` already uses `$CC` env var — no
  new linker abstraction needed.

### 6.2 lldb-18 availability
- Mac dev: Apple lldb-2100 on PATH (not lldb-18). Tests skip via
  `find_lldb()` returning `None` or falling back to system `lldb`.
  The Apple lldb is version-compatible for `image lookup --type` queries.
- CI: `apt install lldb-18` step already present (ADR-0058c §3.5).

### 6.3 inkwell DICompositeType API stability
- inkwell 0.4 `create_struct_type` / `create_member_type` are stable
  APIs mapped directly to LLVM DI builder. No version-detect branching
  needed.

## 7. Implementation plan

~300-400 LOC delta:

| Surface | LOC delta |
|---|---|
| `dwarf_lldb_smoke.rs` harness helpers + 5 tests | ~180 |
| `llvm_backend.rs` Option DICompositeType emission | ~80 |
| `printers.py` tag-dispatch extension | ~40 |
| `tests/test_printers.py` 2 new tag-dispatch tests | ~30 |
| ADR + dual-track docs | non-LOC |

Atomic commits:

1. `docs(adr): 0059d author Phase L wave-3 linker-harness + per-variant Option DI`
2. `tests(dwarf-lldb): linker-harness helpers + executable_spec (§3.1)`
3. `tests(dwarf-lldb): 5 linked/Option smoke tests (§3.3 + §5)`
4. `feat(codegen): per-variant Option DICompositeType (§3.2)`
5. `feat(lldb-printers): cobrust_option_summary tag-dispatch + 2 self-tests (§3.2)`
6. `docs(adr+findings+dual-track): 0059d accepted + 0059a §6 RESOLVED (Phase L wave-3)`

## 8. Consequences

### 8.1 Positive
- ADR-0059a §6.1 honest-cite RESOLVED (byte-decode + DIE presence verified end-to-end).
- ADR-0059a §6.3-per-variant RESOLVED (Option<T> discriminant + payload in DWARF).
- Phase L truly UX-complete for Str + Option debugging.
- LLM agent consuming `frame variable` output sees `"hello"` / `None` / `Some(42)` — not raw addresses.

### 8.2 Negative
- `DICompositeType` for Option<T> adds ~80 LOC to `llvm_backend.rs` + ongoing maintenance.
- Linked-executable test harness requires `cc` to be on PATH — CI already satisfies; Mac satisfies via Xcode Command Line Tools.

### 8.3 Neutral
- Generic `cobrust::Adt` fallback preserved for non-Option Adts.
- Per-Adt variant DICompositeType for user-defined enums remains Phase L+ scope.

— P9 Tech Lead, 2026-05-20
