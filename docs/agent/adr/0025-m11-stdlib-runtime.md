---
doc_kind: adr
adr_id: 0025
title: M11 stdlib + runtime — module surfaces, runtime ABI, drop-schedule fix, codegen amendments, print-intrinsic lift
status: accepted
date: 2026-04-30
last_verified_commit: f758260
supersedes: []
superseded_by: []
dependencies: [adr:0019, adr:0020, adr:0023, adr:0024]
---

# ADR-0025: M11 stdlib + runtime — module surfaces, runtime ABI, drop-schedule fix, codegen amendments, print-intrinsic lift

## Context

ADR-0019 §"M11 — Standard library" pinned the milestone scope:

> Cobrust-native stdlib, structured for `import std.X` syntax. M11
> ships the **minimum viable subset** — io, collections, string,
> math, panic, env. Larger surface is M11.x or post-M11 followup ADRs.

with the binding 7-module table:

| Module | Surface |
|---|---|
| `std.io` | `print / println / read_line / read_file / write_file / stdin / stdout / stderr` |
| `std.collections` | `List<T>` / `Dict<K, V>` / `Set<T>` (no implicit truthiness); iteration via for-protocol |
| `std.string` | `len / find / replace / split / strip / lower / upper / format` |
| `std.math` | `sqrt / pow / sin / cos / abs / floor / ceil / round / pi / e` |
| `std.panic` | `panic(msg)` (terminates); `assert(cond, msg)` |
| `std.env` | `args() -> List<String>` ; `var(name) -> Option<String>` |
| `std.fmt` | f-string runtime helpers (HIR already lowers; runtime side) |

and binding "Done means":

> - `examples/hello.cb` compiles + prints (round-trips M10 work).
> - 10 representative example programs compile + run + match expected
>   output: `fizzbuzz.cb`, `fib.cb`, `wc.cb`, `cat.cb`, `echo.cb`,
>   `sort.cb`, `unique_lines.cb`, `regex_grep.cb`, `csv_sum.cb`, `json_pretty.cb`.
> - ≥ 200 stdlib unit tests; ≥ 50 examples-driven integration tests.

ADR-0024 §"Hello-world contract" delivered M10 as a narrowed
intrinsic — `print` is rewritten to `__cobrust_println_static` only
when the literal argument is exactly `"hello, world"`. ADR-0024
explicitly flagged the supersession point:

> The `print` intrinsic recognition runs at the CLI level; future
> typed-MIR consumers (e.g. LSP) won't see it automatically. **M11
> stdlib supersedes by lifting the rewrite into HIR-lowering.**

ADR-0024 also documented two M10 followups for M11 to lift:

1. **Drop-schedule edge case for str-typed entry-block parameters**
   (M10 followup #2) — kept in M10 by removing the `print` stub
   body entirely; M11 must fix the root cause in `cobrust-mir`.
2. **Aggregate / Ref / Cast Rvalue codegen materialization**
   (M9 followup tracked in ADR-0023 §"Per-MIR-form lowering rules") —
   M9 stubs return zero placeholders; M11 has its first real
   consumer (string literals through `std.io.println`).

ADR-0023 §"Drop-handler ABI" pinned that destructor materialization
"lands at M11 (stdlib + runtime)". M11 owns the per-type
`_cobrust_drop_<TypeId>` handler emission contract.

Constitution `CLAUDE.md` §1.1 binds the dual mandate:

> A statically-typed language implemented in Rust, syntactically
> familiar to Python users, semantically purified.

§2.2 enumerates non-negotiables that the stdlib API must reflect:

> - `dyn` is opt-in, never default
> - Implicit truthy/falsy → `if x` requires `x: bool`
> - Exceptions as default error path → `Result<T, E>` is default
> - Async / sync function coloring → one structured-concurrency
>   runtime, no two-color problem (M13, not M11)
> - Multiple inheritance + MRO → composition + traits

## Options considered

### A. Module surface scope

1. **Ship the binding 7 modules verbatim.** *(adopted)*
   - Pros: ADR-0019 contract fulfilled; "usable for most projects"
     bar achievable.
   - Cons: wide scope; one milestone.

2. **Ship 3 modules now (io/collections/string), defer math/env/fmt/panic.**
   - Cons: violates ADR-0019 binding. Rejected.

3. **Ship more (regex, csv, json, http) on top of the 7.**
   - Cons: ADR-0019 §"Larger surface is M11.x or post-M11 followup
     ADRs". Out of scope for M11. Rejected.

### B. stdlib delivery shape

1. **Pure-Rust crate (`cobrust-stdlib`); compiled into the runtime
   shim that links with every `cobrust build` executable.** *(adopted)*
   - Pros: matches ADR-0012's "translate the surface, bind the core"
     — `Vec / HashMap / HashSet / f64::sqrt / std::env::args` are
     already correct in Rust; the stdlib's job is to project a
     Cobrust-shaped surface onto them.
   - Cons: surface drift between the Rust impl and the future
     Cobrust-source-level stdlib. Mitigation: every public stdlib
     item carries a `// SOURCE: std.io.println` provenance marker
     so the eventual Cobrust port can map 1:1.

2. **Author the stdlib in Cobrust source under `stdlib/std/*.cb`.**
   - Cons: bootstrap problem — `stdlib/std/io.cb` would itself need
     to call into runtime helpers, which means the same Rust shim
     anyway, plus an `import` resolution pass that doesn't yet
     exist (M12 territory). Premature. Rejected.

3. **Hybrid: thin Cobrust wrappers over a Rust runtime ABI.**
   - Cons: doubles the surface area; no clear win at M11. Rejected.

### C. Print-intrinsic lift mechanism

ADR-0024 §"Hello-world contract" rewrote `print("hello, world")` at
the MIR-tier in CLI code. M11 must:
- Accept any `print(s: str)` callsite, not just the M10 narrowing.
- Make the rewrite visible to all MIR consumers, not just the CLI.

Three options:

1. **Lift the rewrite into HIR lowering — `print` becomes a
   well-known def_id that lowers to a runtime call directly.**
   *(adopted)*
   - Pros: every consumer (CLI, future LSP, `cobrust check`) sees
     the same MIR. Clean separation.
   - Cons: HIR lowering needs a "runtime intrinsic" namespace
     (`std.io.print`); M11 introduces it.

2. **Keep the rewrite in CLI, generalize the literal check.**
   - Cons: ADR-0024 explicitly flagged this for supersession at
     M11. Stays at the CLI tier defeats the lift. Rejected.

3. **Dispatch via a real `std.io.println` Cobrust function whose
   body calls the runtime helper.**
   - Cons: requires solving the import/module-resolution
     problem at M11 instead of M12. Premature. Rejected for now,
     adopted as the long-term shape (post-M12).

We adopt option 1 with a forward path to option 3: HIR lowering
recognizes `print(s)` with `s: str` and emits a MIR `Call` whose
`func` operand is `Operand::Constant(Constant::Str("__cobrust_println"))`
(the runtime symbol; no longer narrowed to the static "hello, world"
helper). Codegen lowers this to a `(*const u8, usize)` C-ABI call
into the runtime, where `cobrust-stdlib::io::println_runtime` is
the consumer. **The M10 narrowing diagnostic
(`IntrinsicError::M10ScopeNarrowed`) is removed.**

### D. Constant::Str + (*const u8, usize) ABI

ADR-0023 §"Per-MIR-form lowering rules" left `Constant::Str` as a
null-pointer stub. M11 must materialize:

1. **Intern every `Constant::Str` into a `.rodata` data segment;
   pass `(*const u8, usize)` to runtime calls.** *(adopted)*
   - Cons: heap layout for non-runtime consumers (e.g. assigning
     a `Str` constant to a local) is M12-shaped — at M11 we
     materialize the data segment + pass-as-args path; local
     binding remains the M9 zero stub, which is sufficient for
     M11's example corpus.
   - Pros: matches the C ABI contract that `cobrust-stdlib`
     functions expose (`pub extern "C" fn cobrust_println(ptr:
     *const u8, len: usize)`); zero-copy for runtime calls.

2. **Heap-allocate a `Str` struct on every materialization.**
   - Cons: defeats `.rodata` interning; doubles allocations.
     Rejected.

### E. Drop-schedule fix for str-typed entry-block parameters

M10 followup #2 (ADR-0024 §"Consequences"): the M8 drop schedule
for `s: str` parameters in a body that doesn't move them produces
dangling drop-chain blocks targeting the entry block. The M10 CLI
sidestepped this by removing the `print` stub body entirely after
the rewrite. M11 owns the rewrite at HIR-tier, so the workaround
no longer applies.

The fix: when computing the drop schedule for an owning local
that is **never moved or assigned**, the drop terminator at end
of scope must be emitted as a regular drop block (BlockId N+1)
that jumps to the original successor — never as a Goto self-loop
into the entry block. Root cause: `compute_drop_schedule` in
`cobrust-mir/src/drop.rs` treats "never moved" as "no drop
needed", but a `Str` parameter is owning and **does** need a
drop at end-of-scope; the bug was emitting `Drop { target: BlockId(0) }`
when the original Return block's predecessor was the entry block
itself. Fix: track the actual successor and emit `Drop { target:
<successor> }`; if the body has no successor (single-block return),
emit `Drop { target: <return block + 1> }` instead, where the
new block contains the `Return` terminator.

### F. Aggregate / Ref / Cast Rvalue materialization

ADR-0023 §"Per-MIR-form lowering rules" Aggregate / Ref / Cast
rows lower to zero placeholders at M9. M11's stdlib needs:

- **Aggregate (Tuple)** — for f-string formatting (`std.fmt`),
  function-return tuples.
- **Aggregate (List/Dict/Set)** — for `std.collections` literal
  initializers (`[1, 2, 3]`, `{"a": 1}`, `{1, 2}`).
- **Ref** — for `&str` argument passing.
- **Cast** — for numeric coercion at runtime-helper boundaries
  (`int → f64` for `math.sqrt`).

M11 materializes:

| Rvalue | Lowering |
|---|---|
| `Aggregate(Tuple, [a, b])` | stack `slot` of size 16 + two stores; pointer to slot |
| `Aggregate(List, [elems])` | call `cobrust_list_new` + `cobrust_list_push` per element |
| `Aggregate(Dict, [(k,v)..])` | call `cobrust_dict_new` + `cobrust_dict_insert` per pair |
| `Aggregate(Set, [elems])` | call `cobrust_set_new` + `cobrust_set_insert` per elem |
| `Ref(_, place)` | take the address of the place; for stack-locals via Cranelift `stack_addr` |
| `Cast(IntToFloat, op, F64)` | Cranelift `fcvt_from_sint` (was M9 stub: pass-through) |
| `Cast(FloatToInt, op, I64)` | Cranelift `fcvt_to_sint_sat` |
| `Cast(IntToBool, op, I8)` | `icmp != 0` (was M9 stub: pass-through) |
| `Cast(StrToBytes, ...)` | runtime call `cobrust_str_to_bytes` |
| `Cast(other, ...)` | M9 stub remains (passes through) |

### G. Heap allocator + entry-point shim

ADR-0019 §"M11 — Standard library" pinned:

> - Heap allocator: `mimalloc` by default; `system` allocator opt-in.
> - Panic handler: writes diagnostic to stderr + exits with code 3.
> - Entry point: `pub fn main() -> Result<(), Error>` is the
>   user-visible signature; codegen emits the C ABI `_start` shim.

M11 delivers:

1. **`cobrust-stdlib` crate** with a `runtime` module that:
   - Selects `mimalloc::MiMalloc` as the global allocator under
     the default `mimalloc-alloc` feature flag (`system-alloc`
     feature flag opts out).
   - Provides `__cobrust_panic(ptr: *const u8, len: usize) -> !`
     and `__cobrust_assert(cond: bool, ptr: *const u8, len: usize)`.
   - Provides `__cobrust_main_shim` — the C ABI entry point that
     calls the user's `main` and translates `Result<(), Error>`
     → exit code (0 = Ok, 3 = panic, runtime-panic per ADR-0024 §"Exit-code scheme" code 4).

2. **Entry-point lowering**: codegen emits a top-level user `main` as
   `_cobrust_user_main`. The runtime shim's `int main(int argc,
   char **argv)` (provided by `cobrust-stdlib`'s startup object)
   captures argv into the global-args buffer (consumed by
   `std.env.args()`), calls `_cobrust_user_main`, and returns the
   user's return value.
   - For the M11 user-visible signature, `fn main() -> i64` is the
     accepted shape (M11 keeps the M10 contract — `Result<T, E>`
     is M12 surface). Translating `fn main() -> Result<(), Error>`
     becomes a sugar transform at M12 when the package format lands.

### H. cobrust.toml user-package schema

ADR-0024 deferred the full user-crate schema to ADR-0026 (M12). M11
adds **no** new schema keys to `cobrust.toml`. The M10 `[package]`
placeholder stays exactly as-is. M11 produces standalone executables
that don't yet need dependency resolution — that's the M12 cut.

## Decision

Adopt all 8 sub-decisions A..H above:

- **Modules**: ship all 7 binding (A.1).
- **Delivery**: pure-Rust `cobrust-stdlib` crate as runtime shim (B.1).
- **Print lift**: HIR-tier lowering replaces the M10 CLI rewrite (C.1).
- **Constant::Str ABI**: `.rodata` interning + `(*const u8, usize)`
  argument shape for runtime calls (D.1).
- **Drop-schedule fix**: emit drop blocks with real successor IDs;
  no Goto self-loop into the entry block (E).
- **Codegen amendments**: Aggregate / Ref / Cast Rvalues materialized
  to runtime calls or real Cranelift instructions (F).
- **Runtime**: mimalloc default allocator + panic handler + main shim (G).
- **Package format**: no schema change at M11 (H).

### Public surface (binding)

```rust
// crates/cobrust-stdlib/src/lib.rs

/// Re-export the seven module roots.
pub mod io;
pub mod collections;
pub mod string;
pub mod math;
pub mod panic;
pub mod env;
pub mod fmt;
pub mod runtime;

// crates/cobrust-stdlib/src/io.rs
pub fn print(s: &str);
pub fn println(s: &str);
pub fn read_line() -> Result<String, Error>;
pub fn read_file(path: &str) -> Result<String, Error>;
pub fn write_file(path: &str, contents: &str) -> Result<(), Error>;
pub fn stdin() -> Stdin;
pub fn stdout() -> Stdout;
pub fn stderr() -> Stderr;

// C ABI — what codegen emits calls into:
#[unsafe(no_mangle)] pub extern "C" fn __cobrust_println(ptr: *const u8, len: usize);
#[unsafe(no_mangle)] pub extern "C" fn __cobrust_print(ptr: *const u8, len: usize);

// crates/cobrust-stdlib/src/collections.rs
pub struct List<T> { /* Vec-backed */ }
pub struct Dict<K, V> { /* HashMap-backed; K: Eq + Hash */ }
pub struct Set<T> { /* HashSet-backed; T: Eq + Hash */ }
impl<T> List<T> {
    pub fn new() -> Self;
    pub fn with_capacity(n: usize) -> Self;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;     // Cobrust §2.2: no implicit truthiness
    pub fn push(&mut self, value: T);
    pub fn pop(&mut self) -> Option<T>;
    pub fn get(&self, idx: usize) -> Option<&T>;
    pub fn iter(&self) -> std::slice::Iter<'_, T>;
}
// Dict<K, V>, Set<T> follow the same shape with the obvious method differences.

// crates/cobrust-stdlib/src/string.rs
pub fn len(s: &str) -> usize;
pub fn find(s: &str, pat: &str) -> Option<usize>;
pub fn replace(s: &str, from: &str, to: &str) -> String;
pub fn split(s: &str, sep: &str) -> Vec<String>;
pub fn strip(s: &str) -> &str;
pub fn lower(s: &str) -> String;
pub fn upper(s: &str) -> String;
pub fn format(template: &str, args: &[FormatArg<'_>]) -> String;

pub enum FormatArg<'a> {
    Str(&'a str),
    Int(i64),
    Float(f64),
    Bool(bool),
}

// crates/cobrust-stdlib/src/math.rs
pub const PI: f64 = std::f64::consts::PI;
pub const E: f64 = std::f64::consts::E;
pub fn sqrt(x: f64) -> f64;
pub fn pow(x: f64, y: f64) -> f64;
pub fn sin(x: f64) -> f64;
pub fn cos(x: f64) -> f64;
pub fn abs_f64(x: f64) -> f64;
pub fn abs_i64(x: i64) -> i64;
pub fn floor(x: f64) -> f64;
pub fn ceil(x: f64) -> f64;
pub fn round(x: f64) -> f64;

// crates/cobrust-stdlib/src/panic.rs
pub fn panic(msg: &str) -> !;
pub fn assert(cond: bool, msg: &str);

#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_panic(ptr: *const u8, len: usize) -> !;
#[unsafe(no_mangle)]
pub extern "C" fn __cobrust_assert(cond: bool, ptr: *const u8, len: usize);

// crates/cobrust-stdlib/src/env.rs
pub fn args() -> Vec<String>;
pub fn var(name: &str) -> Option<String>;

// crates/cobrust-stdlib/src/fmt.rs
pub fn format_int(i: i64) -> String;
pub fn format_float(x: f64) -> String;
pub fn format_bool(b: bool) -> String;
pub fn format_str(s: &str) -> String;     // identity; for completeness

// crates/cobrust-stdlib/src/runtime.rs
// Heap-allocator selection — gated by Cargo features; default is mimalloc.
#[cfg(all(feature = "mimalloc-alloc", not(feature = "system-alloc")))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// User-visible error type. Cobrust §2.2 binds Result<T, E> as default.
#[derive(Debug)]
pub enum Error { Io(io::Error), Parse(String), Custom(String) }
impl std::fmt::Display for Error { /* ... */ }
impl std::error::Error for Error {}
```

### Codegen amendments (binding)

| Surface | M10 state | M11 state |
|---|---|---|
| `Constant::Str(s)` | null pointer | `.rodata` interned + `(*const u8, usize)` for runtime calls; null pointer for non-runtime consumers (M12 lifts) |
| `Constant::Bytes(b)` | null pointer | `.rodata` interned (same shape as Str) |
| `Constant::FnRef(d)` | null pointer | resolves to declared `FuncId`; `Operand::Constant(FnRef)` lowers to `func_addr` (Cranelift) |
| `Rvalue::Aggregate(Tuple, ..)` | zero placeholder | stack slot + per-field stores |
| `Rvalue::Aggregate(List, ..)` | zero placeholder | runtime call `cobrust_list_new` + `cobrust_list_push` |
| `Rvalue::Aggregate(Dict, ..)` | zero placeholder | runtime call `cobrust_dict_new` + `cobrust_dict_insert` |
| `Rvalue::Aggregate(Set, ..)` | zero placeholder | runtime call `cobrust_set_new` + `cobrust_set_insert` |
| `Rvalue::Ref(_, place)` | null pointer | `stack_addr` for stack locals; M12 owns heap-deref |
| `Rvalue::Cast(IntToFloat, ..)` | pass-through | `fcvt_from_sint` |
| `Rvalue::Cast(FloatToInt, ..)` | pass-through | `fcvt_to_sint_sat` |
| `Rvalue::Cast(IntToBool, ..)` | pass-through | `icmp != 0` |
| `Rvalue::Cast(other, ..)` | pass-through | M9 stub remains |
| `Terminator::Drop { target, .. }` | Goto target | call `_cobrust_drop_<TypeId>(place)` then jump to `target` |

### Drop-schedule fix (binding — `cobrust-mir/src/drop.rs`)

The bug: when a body has an `s: str` parameter whose entire body is
"call a function and return", the drop schedule emitted by
`compute_drop_schedule` produces `Drop { target: BlockId(0) }`
because the entry block was its own successor in the dataflow.

The fix: introduce a `find_drop_successor(body, block_id) ->
BlockId` helper. When inserting a drop terminator for an owning
local at end-of-scope:

1. If the current block's terminator is `Return`, allocate a NEW
   drop block immediately before, with the drop terminator
   targeting a fresh BlockId N+1 that holds the original `Return`.
2. If the current block's terminator is `Goto(t)`, the drop block
   targets `t` directly (no allocation).
3. Never emit `Drop { target: BlockId(0) }` unless `BlockId(0)` is
   demonstrably reachable as a forward successor (cycle detection
   via DFS — same as borrow-check obligation B4).

The fix unblocks M10's followup #2 and means the M11 `print(s: str)`
HIR-tier lowering can keep the prelude `print` body in the MIR
(no need to remove it post-rewrite).

### Print-intrinsic lift (binding — `cobrust-hir/src/lower.rs`)

The M11 HIR lowering recognizes `print` and `println` as well-known
intrinsics whose def_id resolves to a synthetic FnRef pointing at
the runtime symbol:

| Source call | M11 MIR `Terminator::Call` |
|---|---|
| `print(s)` where `s: str` | `Call { func: Op::Const(Const::Str("__cobrust_print")), args: [<s as (*const u8, usize)>] }` |
| `println(s)` where `s: str` | `Call { func: Op::Const(Const::Str("__cobrust_println")), args: [<s as (*const u8, usize)>] }` |
| `print(...)` other shapes | M11.x scope; emits `TypeError::PrintArgInvalid` (placeholder until full type-resolution lowers stdlib FnRefs) |

The M10 CLI `intrinsics::rewrite_print` is removed; the
`IntrinsicError::M10ScopeNarrowed` diagnostic is removed; the
`__cobrust_println_static` symbol is removed; the M10 runtime
helper `m10_runtime.c` is removed.

### Runtime ABI (binding)

The `cobrust-stdlib` crate, compiled to a static library
`libcobrust_stdlib.a`, provides these C-ABI symbols. `cobrust build`'s
linker step links every `.cb`-derived executable against this
archive (alongside the user object).

| Symbol | Signature | Purpose |
|---|---|---|
| `__cobrust_print` | `extern "C" fn(*const u8, usize)` | `std.io.print` runtime |
| `__cobrust_println` | `extern "C" fn(*const u8, usize)` | `std.io.println` runtime |
| `__cobrust_panic` | `extern "C" fn(*const u8, usize) -> !` | `std.panic.panic` runtime; exits with code 3 |
| `__cobrust_assert` | `extern "C" fn(bool, *const u8, usize)` | `std.panic.assert` runtime |
| `__cobrust_main_shim` | `extern "C" fn() -> i32` | C ABI entry; calls user's `main`; captures argv |
| `_cobrust_drop_str` | `extern "C" fn(*mut StrLayout)` | str destructor (no-op for `.rodata` strings; real for heap-allocated) |
| `_cobrust_drop_list_*` | `extern "C" fn(*mut ListLayout)` | list destructor (per element type) |

### Examples (binding — 10 example programs)

ADR-0019 §"M11 — Standard library" Done means binds 10 example
programs. Per ADR-0024's honesty audit, M11 is honest about which
end-to-end forms are gated and which fall back to documented narrowings:

| Example | Surface used | Gate at M11 |
|---|---|---|
| `hello.cb` | print(literal) | full (M10 regression) |
| `fizzbuzz.cb` | print(literal) + arithmetic + control flow | full |
| `fib.cb` | print(literal) + arithmetic + recursion | full |
| `wc.cb` | std.io.read_line + std.string.split + arithmetic | runtime ABI |
| `cat.cb` | std.io.read_line + print | runtime ABI |
| `echo.cb` | std.env.args + print | runtime ABI |
| `sort.cb` | std.io.read_line + List sort | List materialization |
| `unique_lines.cb` | std.io.read_line + Set | Set materialization |
| `regex_grep.cb` | std.io.read_line + std.string.find | runtime ABI; full regex deferred to post-M11 |
| `csv_sum.cb` | std.io.read_file + std.string.split + arithmetic | runtime ABI |
| `json_pretty.cb` | std.io.read_file + cobrust-tomli (or hand-written) | runtime ABI; document choice |

Per ADR-0019 §"Definition of usable for most projects" — examples
that exercise the runtime ABI are the headline acceptance bar.
Examples that exercise full collection materialization through MIR
are stretch goals; if any fall to M11.x, they MUST be flagged in
this ADR's "Consequences" section + the milestones doc.

## Consequences

- **Positive**
  - Constitution §1 dual mandate: M11 closes the language+runtime
    half to "usable for most projects" per ADR-0019.
  - The 7 binding modules ship.
  - The M10 narrowing diagnostic (`IntrinsicError::M10ScopeNarrowed`)
    disappears — `print(s)` accepts any string literal at M11.
  - The drop-schedule edge case in `cobrust-mir/src/drop.rs` is
    fixed at the root, not via a sidestep.
  - Constant::Str / Bytes / FnRef materialize through `.rodata` +
    declared FuncIds; codegen no longer fakes them.
  - Aggregate / Ref / Cast Rvalues lower to real Cranelift
    instructions or runtime calls.
  - `mimalloc` is the default allocator; `--features system-alloc`
    opts back to libc.
  - hello.cb regression remains green (M10 cli_smoke unchanged).

- **Negative**
  - Some example programs (csv_sum, json_pretty, regex_grep) lean
    on stdlib-Rust paths that the example's `.cb` source can't
    yet exercise end-to-end — these examples ship as tests of the
    Rust shim layer + a partial `.cb` that documents the user's
    intent. Honesty audit: the `.cb` source is honest about which
    forms M11 supports vs. which forms it stubs to the runtime.
  - Full closure / iteration-protocol lowering through MIR (for
    `for x in list:` patterns) is M12 scope; M11 examples that
    need iteration use Rust-shim helpers.
  - LLVM backend `--features llvm` parity for the new amendments
    is best-effort at M11; the gated baseline is Cranelift +
    `--features llvm` is a Phase F polish item.

- **Neutral / unknown**
  - The interaction between mimalloc's TLS init and Cobrust's
    eventual structured-concurrency runtime (M13) is unverified
    at M11 — single-threaded only. M13 will gate.
  - The `_cobrust_drop_<TypeId>` handler emission is fully
    materialized only for `Str` and the three collection types
    at M11; user-defined ADTs still emit no-op drops (matches
    M9). M12+ user-package format will widen.

## Evidence

- ADR-0019 §"M11 — Standard library" — binding 7-module table +
  Done means.
- ADR-0023 §"Per-MIR-form lowering rules" — the M9 stubs this
  ADR amends additively; §"Drop-handler ABI" — destructor
  materialization at M11.
- ADR-0024 §"Hello-world contract" — the M10 print intrinsic this
  ADR supersedes; §"Consequences" — drop-schedule edge case M11
  inherits.
- Constitution `CLAUDE.md` §1.1 (dual mandate), §2.2 (drop-list:
  no implicit truthiness, Result<T,E> default, no async/sync
  coloring), §4.1 (compiler layers), §5.1 (elegance: zero-cost
  abstractions or marked dyn).
- `crates/cobrust-stdlib/{Cargo.toml, src/lib.rs, src/io.rs,
  src/collections.rs, src/string.rs, src/math.rs, src/panic.rs,
  src/env.rs, src/fmt.rs, src/runtime.rs}` — implementation pinned
  to this ADR.
- `crates/cobrust-codegen/src/cranelift_backend.rs` — Aggregate /
  Ref / Cast / Constant::Str / Drop amendments per §"Codegen
  amendments".
- `crates/cobrust-mir/src/drop.rs` — drop-schedule fix per
  §"Drop-schedule fix".
- `crates/cobrust-hir/src/lower.rs` — print-intrinsic lift per
  §"Print-intrinsic lift".
- `crates/cobrust-cli/src/build.rs`,
  `crates/cobrust-cli/src/build/intrinsics.rs` (removed) — M10
  CLI rewrite removed; new linker step links libcobrust_stdlib.a.
- `examples/{hello, fizzbuzz, fib, wc, cat, echo, sort,
  unique_lines, regex_grep, csv_sum, json_pretty}.cb` — 10
  representative example programs.
- `crates/cobrust-stdlib/tests/{stdlib_unit.rs, stdlib_examples.rs}` —
  ≥ 200 stdlib unit tests + 10-example integration tests.
