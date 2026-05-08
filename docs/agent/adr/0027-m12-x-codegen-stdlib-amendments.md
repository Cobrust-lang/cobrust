---
doc_kind: adr
adr_id: 0027
title: M12.x — codegen + stdlib amendments to lift M11 followups (Aggregate / Ref / Cast / for-protocol / f-string)
status: accepted
date: 2026-05-09
last_verified_commit: TBD
supersedes: []
superseded_by: []
---

# ADR-0027: M12.x — codegen + stdlib amendments to lift M11 followups

## Context

M9 codegen (ADR-0023) shipped Cranelift IR lowering for the "core 30
forms" but stubbed five cross-cutting Rvalue / lowering paths as
zero-pointer placeholders or runtime shims:

1. **`Rvalue::Aggregate`** — tuple / list / dict literal construction.
   M9 stub returns a zero pointer. M11 stdlib's `List<T>` / `Dict<K,V>` /
   `Set<T>` therefore cannot be built from a Cobrust literal; tests
   instead rely on Rust shims invoked via FFI from the runtime.

2. **`Rvalue::Ref`** — `&x` borrow operator. M9 stub returns zero.
   M11's structured concurrency primitives (deferred to M13) and any
   value-passing across function boundaries are handicapped.

3. **`Rvalue::Cast`** — int↔float conversions, signed/unsigned, narrowing
   truncation. M9 stub returns zero. The `wc.cb` / `csv_sum.cb`
   examples can't parse stdin into ints.

4. **For-protocol iteration** — `for x in expr:` lowers in HIR to a
   while loop with manual indexing instead of the iterator-protocol
   `iter().next()` pattern. List / Dict iteration therefore can't be
   written ergonomically; the 8 deferred M11 examples use loop +
   index-into-shim instead.

5. **HIR-tier f-string lowering** — `f"hello {name}"` currently inline-
   concatenates static literals only. Interpolation of arbitrary
   expressions falls back to manual `+ str(x)` chains. The
   `cobrust-stdlib::fmt` runtime helpers exist (M11) but are not wired
   into HIR's f-string lowering.

ADR-0025 explicitly deferred all five to M12.x. ADR-0026 (M12 package
format) preserved the deferral per its "smallest correct increment"
guidance. ADR-0019 §"Definition of usable" three lines were met at
the M12 merge (cc15f0b) without these — but only because the notebook
example uses literal `print(...)` callsites. This ADR closes the
deferral.

## Options considered

1. **Single P9-M12.x dispatch covering all five deliverables**
   - Pros: atomic milestone, mirrors M0..M11 dispatch shape.
   - Cons: scope is similar to M12 itself (which timed out at 32 min).
     Cross-cutting changes touch hir + mir + codegen + stdlib + 8
     example .cb files simultaneously — high stream-idle-timeout risk.
   - **Rejected.**

2. **Two-phase dispatch: CTO writes this ADR first; general-purpose
   agent does the impl** *(chosen)*
   - Pros: Skips the ADR-drafting timeout window (M7.2 + M12 both
     stalled there). General-purpose has full Edit / Write / Grep
     tools and runs ~30-50% faster than P9-fallback bash heredoc.
   - Cons: CTO normally writes design ADRs; here it writes an
     implementation-contract ADR too. ADR-0002 §"P9 task prompt
     template" allows this — the topology lets either layer draft.

3. **Split into M12.x.A (codegen Rvalues) + M12.x.B (lang protocols)**
   - Pros: smaller per-sprint scope.
   - Cons: f-string runtime needs Aggregate (heap-allocated `String`)
     so M12.x.B depends on M12.x.A — sequential not parallel. Two
     wall-clock sprints. Not better than option 2.

## Decision

Adopt **option 2**. CTO ships this ADR as a spike commit at the M12.x
worktree HEAD; a general-purpose agent picks up the impl, tests,
example rewrites, and ignore-flag lifts.

### Lowering specifications (binding)

#### 1. `Rvalue::Aggregate(kind, operands)` → heap-allocated value

The M9 stub returns `iconst.i64 0`. Replace with:

| Aggregate kind | Cranelift lowering |
|---|---|
| `Tuple` | `call __cobrust_alloc(size_of_tuple_layout)`; element-by-element store via `store i32/i64/f64 v_i, alloc + offset_i`; return alloc. |
| `List<T>` | `call __cobrust_list_new(elem_size, len)`; element-by-element `__cobrust_list_set(list, i, v)`; return list ptr. |
| `Dict<K, V>` | `call __cobrust_dict_new(k_size, v_size, len)`; pair-by-pair `__cobrust_dict_set(dict, k, v)`; return dict ptr. |
| `Set<T>` | `call __cobrust_set_new(elem_size, len)`; element-by-element `__cobrust_set_insert(set, v)`. |
| `Struct` | Same shape as `Tuple` but with named field offsets per type info. |

Runtime functions land in `cobrust-stdlib`:
- `__cobrust_alloc(size: i64) -> *mut u8` (mimalloc-backed; M11 entry shim).
- `__cobrust_list_new / list_set / list_get / list_len / list_drop`.
- `__cobrust_dict_new / dict_set / dict_get / dict_len / dict_drop`.
- `__cobrust_set_new / set_insert / set_contains / set_len / set_drop`.

Drop schedule (per ADR-0020 §"Drop schedule algorithm"): emit
`call __cobrust_<type>_drop(ptr)` at end-of-scope. M9's drop stub
already routes Drop terminators through `_cobrust_drop_<TypeId>`
handlers; this ADR populates the handlers for List/Dict/Set/Tuple.

#### 2. `Rvalue::Ref(borrow_kind, place)` → address-of

M9 stub returns zero. Replace with:
- For a stack local: `stack_addr v_local` (Cranelift's stack-slot address).
- For a heap allocation: pass the existing pointer through.
- For a field of a place: `iadd ptr, const_offset`.

Borrow kind is informational at codegen — borrow-check (M8) already
discharged the obligations. Lifetime tracking is intra-procedural at
M12.x; cross-body lifetimes remain Phase F.

#### 3. `Rvalue::Cast(kind, operand)` → typed conversion

| From → To | Cranelift op |
|---|---|
| `i32 → i64` (sext) | `sextend` |
| `i64 → i32` (trunc) | `ireduce` |
| `i32 → u32` / `i64 → u64` | (no-op; bit-identical) |
| `i32/i64 → f32` | `fcvt_from_sint` |
| `i32/i64 → f64` | `fcvt_from_sint` |
| `f32 → i32/i64` | `fcvt_to_sint_sat` (saturates per ADR; matches Rust) |
| `f64 → i32/i64` | `fcvt_to_sint_sat` |
| `f32 ↔ f64` | `fpromote` / `fdemote` |
| `bool → int` | `uextend` |

Documented unstable cases (per ADR-0023 §"Documented unstable cases"
pattern): NaN / Inf casts to int saturate to MIN/MAX rather than
panic, matching Rust's `as` semantics.

#### 4. For-protocol iteration

HIR `Stmt::For { var, iter_expr, body }` lowers to:

```mir
let it = iter_expr.iter();          // call <iter>::iter
loop:
  let opt = it.next();              // call <iter>::next, returns Option<T>
  if opt.is_none() { break }
  let var = opt.unwrap();
  body
  goto loop
```

Where `<iter>` is `cobrust_stdlib::iter::ListIter / DictIter / SetIter
/ RangeIter`. `Iterator` is a trait surface in `cobrust-stdlib::iter`
with `next() -> Option<Self::Item>`. The HIR pass identifies the
expression's type (M2 type checker output) and selects the
corresponding iter constructor.

Deferred to Phase F: user-defined types implementing `Iterator` (M12.x
binds only the four stdlib types).

#### 5. HIR-tier f-string lowering

HIR `Expr::FString { parts: Vec<FStringPart> }` where `FStringPart` is
either `Static(&str)` or `Interp(Expr)`:

```
1. Allocate an empty `String` at start.
2. For each part:
   - Static(s): emit `call __cobrust_str_push_static(buf, s_ptr, s_len)`.
   - Interp(e):
     - Codegen e to its native type T.
     - Look up T's formatter:
       - i32/i64 → `__cobrust_fmt_int(buf, v)`.
       - f32/f64 → `__cobrust_fmt_float(buf, v)`.
       - bool   → `__cobrust_fmt_bool(buf, v)`.
       - str    → `__cobrust_fmt_str(buf, v_ptr, v_len)`.
       - List/Dict/Set → `__cobrust_fmt_repr(buf, v_ptr, type_id)`.
3. Result: the `String` heap pointer.
4. Drop schedule registers the `String` for `__cobrust_str_drop` at
   end-of-scope.
```

The runtime helpers all live in `cobrust-stdlib::fmt` (M11 shipped the
stub; this ADR materializes them).

### Example rewrites (binding)

The 8 deferred M11 examples (`wc / cat / echo / sort / unique_lines /
regex_grep / csv_sum / json_pretty`) ship as full Cobrust source after
M12.x:

- `wc.cb` — `for line in std.io.stdin: counts.add_or_update(line, 1)`.
- `cat.cb` — `for line in std.io.stdin: print(line)`.
- `echo.cb` — `print(" ".join(std.env.args()[1:]))`.
- `sort.cb` — `let lines = std.io.stdin.lines(); lines.sort(); for l in lines: print(l)`.
- `unique_lines.cb` — `let prev = ""; for line in stdin: if line != prev: print(line); prev = line`.
- `regex_grep.cb` — `for line in stdin: if line.contains(pattern): print(line)` (substring match — full regex is Phase F).
- `csv_sum.cb` — `for line in stdin: let cells = line.split(","); total += int(cells[col])`.
- `json_pretty.cb` — `let v = cobrust-tomli::loads(std.io.read_file(path)); print(v.pretty())`.

The 11 `#[ignore]` markers in
`crates/cobrust-stdlib/tests/stdlib_examples.rs` are removed; tests
must pass without `--ignored`.

### Test contract (binding)

- Pre-existing 2088 tests stay green.
- M12.x adds:
  - ≥ 30 tests for Aggregate / Ref / Cast each (codegen unit tests in
    `crates/cobrust-codegen/tests/`).
  - ≥ 30 for-protocol tests (driving the four stdlib iter types).
  - ≥ 30 f-string tests (covering each formatter dispatch).
  - 11 lifted `stdlib_examples` tests passing without `--ignored`.
  - 1 differential test per rewritten example: `cobrust build && run`
    stdout matches a Python reference implementation byte-for-byte
    (where possible) or `rtol=0` for ints.

### Workflow

The general-purpose agent picks up at this ADR's spike commit. ADR is
already at `status: accepted` with this commit; the agent reads it,
implements per the binding tables, and commits atomically per
constitution §6. The CTO operations runbook (memory) covers the rest:
two-phase dispatch SOP, 18-lint clippy header, doc-coverage shell
fix-up after union-merge.

## Consequences

- **Positive**
  - The language acquires its full first-class data structures
    (List/Dict/Set built from literals), proper borrow / cast surface,
    iteration protocol, and string formatting.
  - 8 examples promote from runtime-shim demos to real Cobrust source
    — the project's "11 examples" claim becomes structurally honest.
  - 11 `#[ignore]` markers lift; test count grows by ~120-180.
  - M11 followup debt closed; M13 / M14 start from a clean base.

- **Negative**
  - Five cross-cutting changes touching mir + codegen + hir + stdlib
    simultaneously is the broadest sprint of Phase E. Two-phase
    dispatch (this ADR + general-purpose impl) reduces the timeout
    risk but doesn't eliminate it.
  - Heap allocator pressure increases — Aggregate literals now hit
    `__cobrust_alloc` per construction. Performance gating left to a
    later sprint.

- **Neutral / unknown**
  - User-defined `Iterator` implementations are out of scope; only
    the four stdlib types iterate. Phase F lifts.
  - Full regex (vs substring) is Phase F.
  - `regex_grep.cb` therefore ships as substring match; the example
    name is preserved for compatibility but the docstring documents
    the substring-only semantics.

## Evidence

- Constitution `CLAUDE.md` §1 (dual mandate; language must be
  expressive enough for "most projects") + §2.2 (no `dyn`; structured
  ownership preserved through Aggregate / Ref / Cast).
- ADR-0019 §"Definition of usable for most projects" — three lines met
  at M12 with the workaround; M12.x removes the workaround.
- ADR-0023 §"M9 followups" — Aggregate/Ref/Cast stub deferral.
- ADR-0025 §"M11 followups" — for-protocol + f-string deferral.
- ADR-0026 §"smallest correct increment" — explicit M12 → M12.x
  handoff.
- M11 example file `crates/cobrust-stdlib/tests/stdlib_examples.rs`
  for the 11 `#[ignore]`-gated tests.
- M9 codegen file `crates/cobrust-codegen/src/cranelift_backend.rs`
  for the existing stub call sites.
