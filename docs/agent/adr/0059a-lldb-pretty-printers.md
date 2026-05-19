---
doc_kind: adr
adr_id: 0059a
parent_adr: 0059
title: "Phase L wave-1 — lldb pretty-printers for 6 Cobrust types"
status: proposed
date: 2026-05-19
last_verified_commit: 3c9382c
supersedes: []
superseded_by: []
relates_to: [adr:0059, adr:0058c, adr:0050c, adr:0050d]
discovered_by: ADR-0059 §3.1 sub-ADR roster — wave-1 first dispatchable sub-sprint
ratification_path: P9 sub-ADR review under ADR-0059 frame; ratifies on impl merge
---

# ADR-0059a: Phase L wave-1 — lldb pretty-printers

## 1. Motivation

Post-ADR-0058c DWARF v5 emission (ratified `a46fe85`), `lldb-18 frame
variable` correctly locates Cobrust locals via DWARF lines + types, but
shows the **raw struct internals** of every non-primitive type:

```
(lldb) frame variable s
(struct cobrust_str) s = {
  ptr = 0x000060000123a000
  len = 5
  cap = 8
}
```

Cobrust user expectation (Python ergonomics, CLAUDE.md §1):

```
(lldb) frame variable s
(Str) s = "hello"
```

ADR-0058c §4 explicitly deferred this gap: *"Source-level variable
inspection — `DILocalVariable` / `DIFormalParameter` entries for `lldb
frame variable` to walk Cobrust locals. Per ADR-0058 §7 wave-3 ships
DWARF lines + types; full local-variable inspection is Phase L UX, not
Phase K codegen."*

Wave-1 closes the deferral via **lldb pretty-printers** — Python scripts
hosted in lldb's embedded interpreter that translate the raw struct
shape to Cobrust source-level appearance. Pretty-printers are NOT a
DWARF emission extension (no codegen surface change); they are a
debugger-side display layer.

Anchors verified at HEAD `3c9382c`:

- `docs/agent/adr/0058c-llvm-dwarf-debug-info.md` §3.1-§3.4 (DI scaffold
  + DISubprogram + DILocation + finalize shipped).
- `docs/agent/adr/0058c-llvm-dwarf-debug-info.md` §4 (variable
  inspection deferral to Phase L).
- `crates/cobrust-codegen/tests/dwarf_lldb_smoke.rs` (4 lldb smoke
  fixtures; wave-1 extends with +3 type-pretty-printer smoke).
- `docs/agent/adr/0050c-str-ownership.md` (Str shape + non-Copy).
- `docs/agent/adr/0050d-dict-design.md` (Dict shape + `indexmap`
  insertion-order).

## 2. Scope — 6-type pretty-printer roster

Wave-1 ships pretty-printers for the 6 non-primitive Cobrust types
that user-written code most frequently inspects. Primitive types
(`Int` / `Float` / `Bool`) need no pretty-printer — lldb's native
DWARF-driven display already prints them correctly per ADR-0058c §3.2
DIBasicType bindings.

| Type | Cobrust shape | Pretty-printed display | Implementation notes |
|---|---|---|---|
| `Str` | `{ptr: *mut u8, len: usize, cap: usize}` (non-Copy per ADR-0050c) | `"<utf-8 content>"` | Decode bytes `[ptr, ptr+len)` as UTF-8; bound by `len` |
| `List<T>` | `{ptr: *mut T, len: usize, cap: usize}` | `[t1, t2, ...]` | Recurse pretty-printer on each element by index |
| `Dict<K, V>` | `indexmap::IndexMap<K, V>` shape | `{k1: v1, k2: v2, ...}` | Walk indexmap entries in insertion order; recurse K + V |
| `Tuple` | heterogeneous struct of N fields | `(t1, t2, ..., tN)` | Walk by field index; recurse per element type |
| `Set<T>` | `indexmap::IndexSet<T>` | `{t1, t2, ...}` | Walk in insertion order; recurse on element |
| `Option<T>` | `enum { None, Some(T) }` | `None` / `Some(t)` | Discriminant-driven; if Some, recurse on payload |

For non-printable elements (e.g. a `List<RawPtr>` for some future
opaque pointer type, or a Dict with a type we haven't yet registered a
printer for), fall back to lldb's default display for that element —
do NOT crash the printer.

### 2.1 Recursion depth + cycle safety

Pretty-printers recurse on element types. For pathological cases
(`List<List<List<...>>>` deep nesting, or a Dict containing itself via
Ref types in the future), wave-1 caps recursion at depth 8. Beyond
depth 8, print `[...]` placeholder. Cycle detection deferred to a
follow-up sub-ADR if surface demand arises (Cobrust's static ownership
model makes runtime cycles via Ref hard to construct in idiomatic
code).

### 2.2 Large container truncation

For containers > 32 elements, print first 32 + `, ...` + total count
suffix. e.g. `[1, 2, ..., 32, ..., (1024 total)]`. lldb's default
`frame variable --depth N --count M` flags can be overridden by the
pretty-printer's `update()` call.

## 3. Implementation

### 3.1 File location

Single Python file `tools/lldb-cobrust/printers.py`. Top-level layout:

```python
#!/usr/bin/env python3
"""lldb pretty-printers for Cobrust types (ADR-0059a)."""

import lldb

# Per-type provider classes (synthetic + summary providers).
class StrProvider:
    def __init__(self, valobj, internal_dict): ...
    def num_children(self): return 0  # leaf type
    def get_summary(self): ...        # "<utf-8 content>"

class ListProvider:
    def __init__(self, valobj, internal_dict): ...
    def num_children(self): ...
    def get_child_at_index(self, idx): ...
    def update(self): ...

class DictProvider:
    def __init__(self, valobj, internal_dict): ...
    # similar

# ... TupleProvider / SetProvider / OptionProvider ...

def __lldb_init_module(debugger, internal_dict):
    """Auto-called when 'command script import' loads this file."""
    debugger.HandleCommand(
        'type summary add -F printers.StrProvider.get_summary cobrust_str'
    )
    debugger.HandleCommand(
        'type synthetic add -l printers.ListProvider --regex '
        '"^cobrust_list<.+>$"'
    )
    # ... Dict / Tuple / Set / Option ...
```

### 3.2 Loading mechanism

Wave-1 supports three load paths:

- **`command script import` in a `.lldbrc` file** (smoke test path).
  `dwarf_lldb_smoke.rs` extends each fixture's `lldb_batch` invocation
  to prepend `command script import tools/lldb-cobrust/printers.py`
  before any `frame variable` query.
- **`~/.lldbinit-cobrust` user snippet** (manual user load path).
  Documented in `docs/human/{zh,en}/debug.md` (added in wave-1 commit).
  Users append `command source ~/.lldbinit-cobrust` to their
  `~/.lldbinit`.
- **Future: `cobrust debug` auto-load** (wave-3 ADR-0059c path). Not
  wave-1 scope — wave-1 only ships printers + smoke harness wiring.

### 3.3 Type-name pattern matching

lldb's `type summary add` and `type synthetic add` support both
literal type-name matching (`cobrust_str`) and regex matching
(`--regex "^cobrust_list<.+>$"`). Wave-1 uses literal for `cobrust_str`
+ regex for generic containers (`List<T>` / `Dict<K, V>` / `Set<T>` /
`Tuple` / `Option<T>`) since their DWARF type names contain the
parametrized payload.

The exact DWARF type names emitted by ADR-0058c §3.2 are basic types
only (Int / Float / Bool / opaque-ptr); the struct-level type names
(`cobrust_str`, `cobrust_list<i64>`, etc.) come from the per-MIR-Ty
DIType lowering that ADR-0058c §4 explicitly defers ("full
local-variable inspection is Phase L UX"). **Wave-1 must coordinate
type-name emission with the codegen layer** — either:

- **Option A**: extend `LlvmEmitter`'s DIType emission to attach
  Cobrust-source type names to each struct DIType. This is a small
  codegen-side delta (~50 LOC) but does cross the wave boundary into
  Phase K territory.
- **Option B**: pretty-printers infer the Cobrust type from the
  surrounding `DIFormalParameter` / `DILocalVariable` entries (also
  not emitted by ADR-0058c §4). Same crossover issue.

**Decision deferred to wave-1 dispatch eve**: the ADR author's frame
intention is Option A (small codegen delta + clean type-name surface
to match on); the impl P7 sprint will validate Option A's LOC cost
matches the +50 LOC estimate before committing. If LOC explodes >+150,
escalate to a sibling sub-ADR splitting "DIType naming" off
ADR-0059a. This deferral is the wave-1 §5 Risk 5.1 mitigation.

### 3.3.1 Wave-1 dispatch-eve decision: **Option A chosen** (2026-05-19)

Empirical spike at HEAD `f8c459f` (P7 DEV pre-impl read):

- `llvm_backend.rs::populate_di_basic_types` (lines 421-449) emits
  ONLY 4 `DIBasicType` entries: `i64`, `f64`, `bool`, `ptr`.
- `llvm_backend.rs::di_type_for` (lines 454-462) collapses every
  non-primitive `Ty::Str / List / Dict / Set / Tuple / Adt` to the
  same `"Ptr"` opaque-pointer DI key.
- `llvm_backend.rs::lower_ty` (lines 536-548) collapses every
  container type to the SAME LLVM `i8*` opaque pointer.

**Consequence**: under Option B (printer-infer-from-DIE-structure),
lldb sees every Cobrust container local as identically `(ptr) x = ...`
— no struct DIE, no field offsets, no shape distinction. Three
distinct types `List<Int>` / `Dict<Int, Str>` / `Set<Int>` are
indistinguishable to the lldb pretty-printer dispatcher. Option B
is **structurally infeasible** at ADR-0058c's current DWARF emission
level.

**Option A — chosen.** Smallest viable codegen delta: extend
`populate_di_basic_types` with 5 NEW named `DIBasicType` entries
(`Str`, `List`, `Dict`, `Set`, `Tuple`), all 64-bit opaque-pointer
storage (`DW_ATE_ADDRESS`) but with **distinct DWARF type-names**.
Then `di_type_for` dispatches Cobrust `Ty` variant → name. Pretty-
printers register on these names (`type summary add cobrust::Str`,
`type synthetic add -l ListProvider --regex '^cobrust::List'`).

Option per type:

- `Ty::Str` → "cobrust::Str"
- `Ty::List(_)` → "cobrust::List" (regex match; Phase L+ may add
  inner element type-name e.g. "cobrust::List<Int>" once MIR carries
  the element type through to DI; wave-1 ships the un-parametrized
  matcher).
- `Ty::Dict(_, _)` → "cobrust::Dict"
- `Ty::Set(_)` → "cobrust::Set"
- `Ty::Tuple(_)` → "cobrust::Tuple"
- `Option<T>` is modeled via `Ty::Adt(...)` in HIR/MIR. Wave-1 ships
  the OptionProvider Python class as scaffolding, but the smoke gate
  exercises only the 3 directly-typeable container shapes (Str / List
  / Dict per §6) since `Option<T>` requires Adt → DI naming which is
  a Phase L+ sub-ADR. The printer registration is filed with a
  conservative `cobrust::Option` matcher; if no MIR Local carries
  that name, lldb simply never invokes the printer.

**LOC cost**: ~35 LOC delta in `populate_di_basic_types` (5 new
`create_basic_type` calls) + ~20 LOC delta in `di_type_for`
(Cobrust-aware variant dispatch). Total +55 LOC. Within the +150
escalation threshold. Wave-1 proceeds without sibling sub-ADR split.

**Reproducibility**: spike performed against worktree
`/Users/hakureirm/codespace/Study/cobrust-0059a-dev` at branch
`feature/0059a-dev`. The 4 existing baseline smoke tests
(`lldb_smoke_hello_world_subprogram_resolves` /
`lldb_smoke_fib_function_visible` /
`lldb_smoke_multi_fn_module_lists_both` /
`lldb_smoke_line_table_present`) are guaranteed to PASS regardless
of Option A or B (their fixtures use only `Ty::Int` locals which
already map to a working `DIBasicType`).

## 4. Non-goals

Explicitly out of wave-1 scope:

- **Source-level type-name printing in Rust style** (e.g. don't render
  `Vec<String>` Rust-style). Wave-1 uses Cobrust source syntax: `List<Str>`
  not `Vec<String>`; `Dict<Int, Str>` not `HashMap<i64, String>` or
  `IndexMap<i64, String>`. The user wrote Cobrust, not Rust.
- **Inline expression evaluation in printer** (e.g. printing
  `arr[i] + 1` rather than `arr[i]`). Pretty-printers display raw
  values; computation is out of scope. Phase L+ may add this if user
  demand arises.
- **Struct field access without DWARF DI** (e.g. user-defined record
  types from ADR-0006 / future record-types ADR). Wave-1 ships only
  the 6 built-in types in §2; user-defined types fall back to lldb
  default display. Phase L+ may add a generic record-type printer
  once user-record DWARF emission lands (parked behind a future ADR).
- **gdb pretty-printers** (per ADR-0059 §5.3 Risk Linux gdb compat).
  Wave-1 is lldb-only; gdb is Phase L+ followup.
- **REPL-style mutation from inspector** (per ADR-0059 §4 non-goal
  "Variable rewrite from the debugger"). Read-only display.

## 5. §2.5 LLM-first audit

Pretty-printer output IS **LLM-consumable**. The agent reading lldb
output (Cursor's terminal panel, an `agent --command "lldb-18 ..."`
session, CI failure attached debug log) understands:

```
(lldb) frame variable d
(Dict<Int, Str>) d = {1: "a", 2: "b"}
```

…better than:

```
(lldb) frame variable d
(struct __cobrust_dict_t) d = {
  ptr = 0x600003a40080
  len = 2
  capacity = 4
  hashes = 0x600003a40100
  entries = 0x600003a40180
}
```

The Python-`repr`-shaped format is the most-trained-on debugger output
format in 2026 LLM corpora. §2.5 §B (training-data overlap) modest
positive.

§2.5 §A (compile-time-catch-errors): neutral. Pretty-printers operate
at runtime; type errors / borrow errors already caught upstream. The
pretty-printer itself catches NO new error class.

§2.5 §B positive does NOT promote Phase L from its rank-5 ROI slot;
ADR-0059 §2 binding is preserved. The §2.5 §B win is modest but real,
and it is enumerated here so future audit doesn't claim Phase L is
"§2.5-zero" (it is §2.5-low, distinct from zero).

## 6. Acceptance gate — 4 lldb smoke tests

`crates/cobrust-codegen/tests/dwarf_lldb_smoke.rs` extends with 3 NEW
smoke tests + 1 REGRESSION-GUARD test:

1. **`lldb_smoke_str_variable_inspection_pretty`** — compile a fixture
   that declares `let s: Str = "hello"`; assert
   `frame variable s` output matches `(Str) s = "hello"` (or pattern
   `Str.*=.*"hello"`).
2. **`lldb_smoke_list_int_variable_inspection_pretty`** — compile a
   fixture declaring `let xs: List<Int> = [1, 2, 3]`; assert
   `frame variable xs` output matches `[1, 2, 3]` shape.
3. **`lldb_smoke_dict_int_str_variable_inspection_pretty`** — compile
   a fixture declaring `let d: Dict<Int, Str> = {1: "a", 2: "b"}`;
   assert `frame variable d` matches `{1: "a", 2: "b"}` shape.
4. **REGRESSION-GUARD**: existing 4 ADR-0058c smoke tests
   (`lldb_smoke_hello_world_subprogram_resolves` /
   `lldb_smoke_fib_function_visible` /
   `lldb_smoke_multi_fn_module_lists_both` /
   `lldb_smoke_line_table_present`) still PASS. Pretty-printer loading
   must NOT break baseline DWARF inspection.

Wave-1 ships **3 new + 4 preserved = 7 total** lldb smoke tests. The
"4 lldb smoke tests in `crates/cobrust-codegen/tests/dwarf_lldb_smoke.rs`
extended" wording from the parent ADR-0059 §7 was a frame-author
overcount; the precise count is 3 new + 4 baseline preserved = 7
total, of which 3 are pretty-printer-driven.

Skip behaviour: when `lldb-18` is not on `$PATH` (Mac dev machines
without `brew install llvm@18`), the 3 new tests skip cleanly per
existing `find_lldb()` helper — mirroring ADR-0058c §3.5 behaviour.

## 7. Risk register

Three concrete risks tracked for wave-1 dispatch.

### 7.1 DIType naming coordination (§3.3 deferred decision)

- **Risk**: per §3.3, lldb pretty-printers match by DWARF type name.
  ADR-0058c §3.2 emits only basic types (Int / Float / Bool /
  opaque-ptr); the struct-level type names (`cobrust_str`,
  `cobrust_list<i64>`, etc.) needed for `type summary add` matching
  do NOT exist in the current DWARF. Option A (extend LlvmEmitter
  +~50 LOC) or Option B (printer infers from sibling DI entries) is
  the decision point.
- **Mitigation**: dispatch eve, the impl P7 spike validates Option A
  LOC cost. If ≤ +150 LOC delta, accept inline. If >+150 LOC, split
  to a sibling sub-ADR (ADR-0059a-prereq DIType naming) before
  wave-1 main dispatch. The sibling ADR would be a ~3-day codegen
  delta independent of pretty-printers proper.

### 7.2 lldb Python API version churn

- **Risk**: lldb's Python API (`SBValue.GetSummary` /
  `SBSyntheticValueProvider`) drifted between LLVM-17 and LLVM-18.
  Future LLVM-19 may drift further. ADR-0059 §5.1 already enumerates
  this. Wave-1 pretty-printers written against LLVM-18 are pinned to
  that version surface.
- **Mitigation**: stick to the most stable Python lldb subset
  (per ADR-0059 §5.1 enumeration: `SBValue.GetChildAtIndex`,
  `SBValue.GetData`, `SBData.ReadRawData`,
  `SBSyntheticValueProvider.{num_children, get_child_at_index, update}`).
  If LLVM-19 churns, version-detect at script load + branch
  per-version code paths. Wave-1 is single-version (LLVM-18) baseline;
  multi-version support is Phase L+ followup.

### 7.3 UTF-8 decode robustness for Str

- **Risk**: `Str` ptr+len reads raw bytes; arbitrary content might
  contain non-UTF-8 byte sequences (e.g. partially-corrupted memory
  during a crash-investigation debug session, or test fixtures
  exercising the codegen path with intentionally-invalid bytes). A
  naive `bytes.decode('utf-8')` raises an exception that breaks the
  printer.
- **Mitigation**: wave-1 uses `bytes.decode('utf-8', errors='replace')`
  for fallback safety. Invalid UTF-8 byte sequences render as the
  Unicode replacement character `�`. The printer continues
  rather than crashing. Print a `(non-UTF-8 detected)` suffix in
  ambiguous cases.

## 8. Implementation plan

~500 LOC Python (`tools/lldb-cobrust/printers.py`) + ~50 LOC Rust test
wiring (`dwarf_lldb_smoke.rs` extensions).

Day-by-day breakdown:

- **Day 1** — `tools/lldb-cobrust/printers.py` scaffold + StrProvider
  + ListProvider (~200 LOC, 2 of 6 types). Manual lldb smoke on a
  hand-built fixture.
- **Day 2** — DictProvider + TupleProvider + SetProvider +
  OptionProvider (~300 LOC, remaining 4 types). Lookup-table for
  type-name regex patterns. Manual smoke on multi-type fixtures.
- **Day 3** — Resolve §3.3 DIType naming decision (Option A vs B).
  If Option A, +~50 LOC LlvmEmitter delta. Extend
  `dwarf_lldb_smoke.rs` with 3 new pretty-printer smoke tests +
  verify existing 4 baseline tests still PASS.
- **Day 4** — `docs/human/{zh,en}/debug.md` new doc page (debugger
  setup + pretty-printer load instructions); `docs/agent/` agent-doc
  for the pretty-printer surface. CLAUDE.md §3 dual-track binding
  enforced. Mac smoke + <self-hosted-runner> cross-verify.

Wall time estimate: ~3-4 days. Risk 7.1 may extend to 5 days if
DIType naming requires sibling ADR.

## 9. Sub-ADR roster

Single ADR (wave-1 of Phase L). No further sub-sub-sprints under
ADR-0059a. Sibling sub-ADRs under parent ADR-0059 §9: ADR-0059b (DAP
server), ADR-0059c (`cobrust debug` CLI). Wave-1 ratifies on impl
merge per ADR-0059 §10 pre-dispatch acceptance gate.

## 10. Pre-dispatch acceptance gate

Wave-1 dispatch may proceed only when:

- [ ] ADR-0058c (Phase K wave-3 DWARF emission) status = accepted.
      Verified at `a46fe85` ✓.
- [ ] ADR-0059 (Phase L frame) authored + frontmatter declares
      relates_to: [adr:0058c]. Verified ✓ (this ADR-0059a's parent).
- [ ] lldb-18 available on dispatch host (Mac dev: `brew install
      llvm@18`; DG: already installed per ADR-0058c).
- [ ] §3.3 DIType naming decision (Option A vs B) resolved at dispatch
      eve — impl P7 spike validates LOC cost before main dispatch.
- [ ] §10 acceptance: 3 new lldb smoke tests + 4 baseline preserved.

## 11. Consequences

### 11.1 Positive

- ADR-0058c §4 variable-inspection deferral CLOSED.
- ADR-0019 Phase E "debugger out of scope" deferral CLOSED.
- 6-type pretty-printer surface delivered with ~500 LOC Python (small
  maintenance burden).
- §2.5 §B modest positive: pretty-printer output is LLM-consumable
  Python-`repr` shape.
- `dwarf_lldb_smoke.rs` test corpus expanded from 4 → 7 (regression
  guard for future codegen drift).

### 11.2 Negative

- ~500 LOC Python adds a new toolchain (lldb embedded Python) to the
  Cobrust maintenance surface. lldb Python API churn (Risk 7.2) carries
  forward.
- DIType naming (Risk 7.1) may push a +50 LOC codegen-side delta into
  wave-1 — cross-wave scope spill if Option A taken.
- gdb users get no support in wave-1 (per ADR-0059 §5.3 Risk).

### 11.3 Neutral

- `tools/lldb-cobrust/` is a new top-level dir; CLAUDE.md §3
  doc-coverage applies.
- Wave-1 is lldb-18-pinned; future LLVM-19 may require version-detect
  branching (Risk 7.2 mitigation deferred to Phase L+).
- Cycle detection (§2.1) deferred until pathological cases surface.

## 12. Dispatch readiness

Per ADR-0059 §13 row 1 (0059a budget):

| Phase | TEST hrs | DEV hrs | Wall |
|---|---|---|---|
| Day 1 Str + List printers | 0 | 4 | 1 |
| Day 2 Dict + Tuple + Set + Option printers | 0 | 6 | 1 |
| Day 3 DIType decision + 3 new smoke tests | 3 | 3 | 1 |
| Day 4 dual-track docs + Mac + DG verify | 1 | 1 | 0.5-1 |
| **Total** | **~4** | **~14** | **~3-4 days** |

Mode: P10-direct PAIR per F28 strict-separation. Routing: TEST =
sonnet (lldb smoke author + verify-baseline); DEV = opus (~500 LOC
Python printers + lldb Python API learning curve + DIType decision
option-validation). Branch: `feature/0059a-dev`. Host: self-hosted runner
for final `cargo test -p cobrust-codegen --features llvm` + lldb-18
smoke baseline + `tests/dwarf_lldb_smoke.rs` 7-test PASS.

— P9 Tech Lead, 2026-05-19
