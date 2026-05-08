---
doc_kind: module
module_id: mod:stdlib
crate: cobrust-stdlib
last_verified_commit: TBD
dependencies: [mod:codegen, mod:mir, mod:hir]
---

# Module: stdlib

## Purpose

Cobrust's standard library — the seven binding modules from
ADR-0019 §"M11" + the runtime shim that codegen-emitted programs
link against. Constitution §1.1 dual mandate: the runtime half of
"a statically-typed language implemented in Rust".

## Status

- **M11 — delivered.** ADR-0025 binds the seven module surfaces
  (io / collections / string / math / panic / env / fmt), the
  runtime ABI (mimalloc allocator, panic handler, main shim),
  the print-intrinsic lift superseding ADR-0024 §"Hello-world
  contract", and codegen amendments materializing `Constant::Str`
  via `.rodata`.

## Public surface (M11)

```rust
// crates/cobrust-stdlib/src/lib.rs

pub mod io;
pub mod collections;
pub mod string;
pub mod math;
pub mod panic;
pub mod env;
pub mod fmt;
pub mod runtime;

pub use runtime::{Error, ErrorKind};
pub use collections::{Dict, List, Set};
```

### `std.io`

```rust
pub fn print(s: &str);
pub fn println(s: &str);
pub fn read_line() -> Result<String, Error>;
pub fn read_file(path: &str) -> Result<String, Error>;
pub fn write_file(path: &str, contents: &str) -> Result<(), Error>;
pub fn stdin() -> Stdin;
pub fn stdout() -> Stdout;
pub fn stderr() -> Stderr;

// C ABI (codegen targets these):
pub unsafe extern "C" fn __cobrust_print(ptr: *const u8, len: usize);
pub unsafe extern "C" fn __cobrust_println(ptr: *const u8, len: usize);
```

### `std.collections`

```rust
pub struct List<T> { /* Vec<T>-backed */ }
pub struct Dict<K, V> { /* HashMap<K, V>-backed; K: Eq + Hash */ }
pub struct Set<T> { /* HashSet<T>-backed; T: Eq + Hash */ }

impl<T> List<T> {
    pub fn new() -> Self;
    pub fn with_capacity(n: usize) -> Self;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;     // Constitution §2.2: no implicit truthiness.
    pub fn push(&mut self, value: T);
    pub fn pop(&mut self) -> Option<T>;
    pub fn get(&self, idx: usize) -> Result<&T, Error>;
    pub fn iter(&self) -> std::slice::Iter<'_, T>;
}
impl<T: Ord> List<T> { pub fn sort(&mut self); }
impl<T: PartialEq> List<T> { pub fn contains(&self, target: &T) -> bool; }
```

`Dict<K, V>` and `Set<T>` follow the same shape with the obvious
method differences (`insert`/`get`/`contains_key`/`remove`).

### `std.string`

```rust
pub fn len(s: &str) -> usize;
pub fn find(s: &str, pat: &str) -> Option<usize>;
pub fn replace(s: &str, from: &str, to: &str) -> String;
pub fn split(s: &str, sep: &str) -> Vec<String>;
pub fn strip(s: &str) -> &str;
pub fn lower(s: &str) -> String;
pub fn upper(s: &str) -> String;
pub fn format(template: &str, args: &[FormatArg<'_>]) -> String;

pub enum FormatArg<'a> { Str(&'a str), Int(i64), Float(f64), Bool(bool) }
```

### `std.math`

```rust
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
```

### `std.panic`

```rust
pub fn panic(msg: &str) -> !;
pub fn assert(cond: bool, msg: &str);

pub unsafe extern "C" fn __cobrust_panic(ptr: *const u8, len: usize) -> !;
pub unsafe extern "C" fn __cobrust_assert(cond: bool, ptr: *const u8, len: usize);
```

ADR-0024 §"Exit-code scheme" — `panic` exits with code 3
(`INTERNAL_PANIC`).

### `std.env`

```rust
pub fn args() -> Vec<String>;
pub fn var(name: &str) -> Option<String>;
```

### `std.fmt`

```rust
pub fn format_int(i: i64) -> String;
pub fn format_float(x: f64) -> String;
pub fn format_bool(b: bool) -> String;
pub fn format_str(s: &str) -> String;
```

### Cobrust source-level surface

The seven binding modules project onto Cobrust source-level imports
(M11 ships the runtime + Rust shim; the source-level Cobrust import
machinery is M12 scope per ADR-0019 §"M12 — Package format"). The
canonical paths a user will write at M12+:

- `std.io.println(s)` / `std.io.print(s)` / `std.io.read_line()` / `std.io.read_file(path)` / `std.io.write_file(path, contents)`
- `std.collections.List<T>` / `std.collections.Dict<K, V>` / `std.collections.Set<T>`
- `std.string.format(template, args)` / `std.string.split(s, sep)` / `std.string.find(s, pat)` / `std.string.replace(s, from, to)`
- `std.math.sqrt(x)` / `std.math.PI` / `std.math.E` / `std.math.sin(x)` / `std.math.pow(x, y)`
- `std.panic.panic(msg)` / `std.panic.assert(cond, msg)`
- `std.env.args()` / `std.env.var(name)`
- `std.fmt.format_int(i)` / `std.fmt.format_float(x)` / `std.fmt.format_bool(b)`

At M11 these resolve through the `cobrust-stdlib` Rust crate; M12 will
bind the source-level `import std.X` machinery to the same Rust shim.

### `runtime`

```rust
pub enum ErrorKind { Io, Parse, Custom, OutOfBounds, KeyNotFound, Runtime }
pub struct Error { /* kind + message */ }

pub mod exit_codes {
    pub const SUCCESS: u8 = 0;
    pub const USER_ERROR: u8 = 1;
    pub const TYPE_ERROR: u8 = 2;
    pub const INTERNAL_PANIC: u8 = 3;
    pub const RUNTIME_PANIC: u8 = 4;
}

// Heap allocator (gated by feature `mimalloc-alloc`, default on).
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

// C-ABI argv capture (called by the C entry shim cobrust_main.c).
pub unsafe extern "C" fn __cobrust_capture_argv(argc: i32, argv: *const *const u8);
pub unsafe extern "C" fn _cobrust_drop_str(_place: *mut u8);

// User entry-point symbol — codegen exports the user's `fn main` Body
// as `_cobrust_user_main`. The C entry shim (cobrust_main.c) provides
// the platform `int main(int, char**)` and dispatches here.
extern "C" { pub fn _cobrust_user_main() -> i64; }
```

## Invariants

- **No implicit truthiness** — every collection has `is_empty()`;
  there is no `bool` coercion path through `List` / `Dict` / `Set`.
- **Result<T, E> is the default error path** for all fallible
  operations (constitution §2.2). Panic is reserved for "truly
  unrecoverable" via `std.panic.panic`.
- **No `dyn` in the public surface** (constitution §5.1) — every
  trait bound is a generic parameter.
- **C ABI symbols are stable** — the runtime ABI between codegen
  and `cobrust-stdlib` is closed-set + documented (this file +
  ADR-0025 §"Runtime ABI").
- **String literals are `.rodata` interned** at codegen time;
  `_cobrust_drop_str` is a no-op for `.rodata` strings (they don't
  own heap state at M11). Heap-allocated strings are M12+.

## Done means (M11)

- [x] Seven binding modules ship: io, collections, string, math,
      panic, env, fmt.
- [x] Runtime shim (mimalloc allocator + main entry +
      __cobrust_capture_argv) ships.
- [x] C-ABI symbols (__cobrust_print, __cobrust_println,
      __cobrust_panic, __cobrust_assert, _cobrust_drop_str)
      exported from libcobrust_stdlib.a.
- [x] hello.cb regression: PASS through the M11 lift.
- [x] 10 representative example programs build + run + match
      expected stdout + exit 0 (per ADR-0025 §"Examples (binding)").
- [x] ≥ 200 stdlib unit tests + integration tests:
      262 passing (133 unit + 11 example gate +118 integration).
- [x] ADR-0025 accepted.

## Non-goals

- **Full closure / iteration-protocol lowering through MIR** —
  for-loops over `List<T>` and friends are M12 scope. M11 ships
  the stdlib API + runtime ABI; the codegen end-to-end iteration
  arrives later.
- **Heap-allocated `Str`** — M11 strings live in `.rodata`. M12+
  add the heap-`String` path with `_cobrust_drop_str` materializing.
- **Async / sync coloring** — constitution §2.2 forbids it; the
  structured-concurrency runtime is M13.
- **REPL** — M14.
- **Full Unicode case-folding** in `string::lower`/`upper` —
  ASCII fast-path at M11; full case-folding is M11.x.

## Cross-references

- `mod:codegen` — emits calls into the C ABI symbols this module
  provides; ADR-0025 §"Codegen amendments" pins the contract.
- `mod:cli` — links against `libcobrust_stdlib.a` at every
  `cobrust build` invocation per ADR-0025 §"Runtime ABI".
- `mod:hir` — the print-intrinsic lift superseding ADR-0024.
- ADR-0019 §"M11" — milestone scope.
- ADR-0023 §"Drop-handler ABI" — Drop terminator materialization
  delegated to M11.
- ADR-0024 §"Hello-world contract" — M10 supersedes pinned here.
- ADR-0025 — M11 design (this milestone).
