---
finding_id: F71
title: wasm32 strict typed-call signature mismatch — codegen externs hardcode i64 for C usize
status: resolved
date: 2026-05-29
discovered_during: "ADR-0075 Phase 2 wasm32-cross-smoke (CI run 26619321358) — hello_wasm.wasm runtime trap"
related: [F38, ADR-0025, ADR-0064, ADR-0075]
---

# F71 — wasm32 strict typed-call signature mismatch

## What happened

ADR-0075 Sprint E solved the wasm32-wasip1 sysroot — `hello_wasm.wasm`
**compiles + links + runs** under `wasmtime`. But it trapped at runtime:

```
0x307 hello_wasm.wasm!signature_mismatch:__cobrust_println
→ wasm trap: unreachable
```

The `.wasm` reached `main`, then trapped the instant it called the
`__cobrust_println` runtime helper.

## Root cause — i64-vs-i32 ABI mismatch on the `usize` parameter

The runtime helper's **definition** (the ABI contract) and codegen's
**extern declaration** disagreed on the second parameter's width.

| | Source | Signature |
|---|---|---|
| Runtime **definition** (truth) | `crates/cobrust-stdlib/src/io.rs:72` | `extern "C" fn __cobrust_println(ptr: *const u8, len: usize)` |
| Codegen **declaration** (before) | `crates/cobrust-codegen/src/llvm_backend.rs:1416` | `void_ty.fn_type(&[ptr_ty, i64_ty], false)` |

`usize` is **target-pointer-width**:

- x86_64 / aarch64 / riscv64 → 64-bit → LLVM `i64`
- **wasm32-wasip1 → 32-bit → LLVM `i32`**

So on wasm32 the Rust runtime exports `__cobrust_println : (i32, i32) -> ()`
while codegen emitted a call against a declared `(ptr, i64) -> ()` import.
wasm32 enforces **strict typed function tables**: every `call` /
`call_indirect` checks the callee's exact functype. A `(i32, i64)`
call against an `(i32, i32)` definition is a `signature_mismatch` →
`unreachable` trap.

Native ELF linkers (x86 / arm / riscv) silently tolerate the i64-vs-i64
match (there `usize == i64`, so the bug was *invisible* on every native
target). **wasm is the first target where the latent mismatch surfaced.**

## The generalized lesson — wasm is a free ABI-correctness fuzzer

Every `__cobrust_*` runtime extern whose Rust `extern "C"` definition
takes a `usize` but whose codegen declaration hardcodes `i64` is a
wasm32 time-bomb. The native CI never catches it; the wasm32 strict
typed-call check catches **all of them at once**. Treat a wasm32 smoke
run as a signature-fuzzer over the entire runtime ABI surface.

### Full audit of `usize`-typed runtime externs (io.rs / panic.rs / array.rs)

11 runtime functions take a `usize`. Cross-checked each against its
codegen declaration:

| Runtime symbol | Runtime def | Codegen decl before | Declared in LLVM backend? | Action |
|---|---|---|---|---|
| `__cobrust_println` | `(ptr, usize)` | `(ptr, i64)` | yes | **aligned → usize** (the hello-world trap) |
| `__cobrust_print` | `(ptr, usize)` | `(ptr, i64)` | yes | **aligned → usize** (`print_no_nl_lit` path) |
| `__cobrust_print_no_nl_lit` | `(ptr, usize)` | `(ptr, i64)` | yes | **aligned → usize** |
| `__cobrust_panic` | `(ptr, usize)` | `(ptr, i64)` | yes | **aligned → usize** |
| `__cobrust_input` | `(ptr, usize) -> ptr` | `(ptr, i64) -> ptr` | yes | **aligned → usize** |
| `__cobrust_array_get_i64` | `(ptr, usize, usize) -> i64` | `(ptr, i64, i64)` | yes | **aligned → usize** |
| `__cobrust_array_get_i32` | `(ptr, usize, usize) -> i32` | `(ptr, i64, i64)` | yes | **aligned → usize** |
| `__cobrust_array_get_i8` | `(ptr, usize, usize) -> i8` | `(ptr, i64, i64)` | yes | **aligned → usize** |
| `__cobrust_array_get_bool` | `(ptr, usize, usize) -> i64` | `(ptr, i64, i64)` | yes | **aligned → usize** |
| `__cobrust_assert` | `(bool, ptr, usize)` | — | **no** (Assert lowers to `unreachable`) | n/a — not emitted, cannot trap |
| `__cobrust_result_err_panic` | `(ptr, usize)` | — | **no** (not wired in LLVM backend) | deferred — declare with `usize` if/when wired |

Note: `__cobrust_str_push_static(buf, ptr, len: i64)` (`fmt.rs:88`) and
`__cobrust_fmt_float_prec(.., spec_len: i64)` (`fmt.rs:143`) genuinely
take `i64` in **both** the runtime def and the codegen decl — they are
consistent and were **correctly left alone**. The fix is not "make
everything pointer-width"; it is "make the declaration match the
definition", and these two were already matched.

## The fix

`crates/cobrust-codegen/src/llvm_backend.rs`. Derive the C `usize`
width from the target machine's data layout instead of hardcoding `i64`.

1. **New cached emitter field** `usize_ty: IntType<'ctx>`, computed in
   `LlvmEmitter::new` via
   `ctx.ptr_sized_int_type(&target_machine.get_target_data(), None)`
   — `i64` natively, `i32` on wasm32.
2. **Declarations**: the 9 emitted `usize`-typed externs above now
   declare their `usize` params as `usize_ty`.
3. **Call sites** (the value passed must match the now-target-width
   param, else LLVM rejects an i64→i32 arg even before wasm sees it):
   - The `(ptr, len)` expansion path in `lower_call`
     (`expand_str_to_ptr_len` / `expand_trailing_str_len`) coerces the
     i64 length from `materialize_str_data` to the **callee's declared
     len-param type** via the existing `coerce_value_to` helper. This
     covers `__cobrust_println`, `__cobrust_print_no_nl_lit`,
     `__cobrust_panic`, `__cobrust_input`.
   - The dynamic-array-index path coerces the static `N` length and the
     runtime index to `usize_ty` for the `__cobrust_array_get_*` calls.
   - `materialize_str_data` deliberately keeps returning an **i64**
     length: its other three consumers (`materialize_str_buffer`,
     the format-string literal push, `__cobrust_fmt_float_prec`) all
     feed genuinely-`i64` runtime externs. Coercion happens per-call.

On native targets `usize_ty == i64`, so the emitted IR + objects are
bit-identical to before (verified: `hello` / `fib` / `fizzbuzz` run and
the codegen diff-corpus fixtures all unchanged-pass).

## Verification

- **Local wasm signature probe** (throwaway, not committed): emitted a
  `__cobrust_println("hi")` module for `wasm32-wasip1`, hand-parsed the
  wasm Type section. `__cobrust_println` functype params went from the
  buggy `[i32, i64]` to the correct **`[i32, i32]`** — matching the
  runtime's `(i32, i32)` export. This is the strongest local evidence
  short of running wasmtime (unavailable on the macOS dev host).
- `cargo build --workspace`, `cargo clippy --workspace --all-targets
  -D warnings`, `cargo fmt --check`: green.
- `cargo test -p cobrust-codegen`: all pass (diff-corpus println /
  array fixtures unchanged). `cargo test -p cobrust-cli`: all pass
  (`hello_world_compiles_and_runs`, `s07_run_hello_world_end_to_end`,
  fib/fizzbuzz, coil/dora hello round-trips). `cargo test -p
  cobrust-stdlib`: 0 failures.
- **The wasm RUN itself is CI-verified, NOT verified locally** —
  the macOS dev host has no `wasi-sdk` / `wasmtime` / rustup wasm32
  target, so `cross_compile_wasm32_e2e` SKIPS locally. This finding
  claims: the emitted `__cobrust_println` signature now matches the
  runtime definition `(i32, i32)`, and all host gates are green; the
  CI wasm32-cross-smoke job under `wasmtime` is the authoritative
  confirmation that the `signature_mismatch` trap is gone.

## Status

`resolved` — hello-world's path (`__cobrust_println` + the str/print/
array `usize` family) is aligned to the runtime ABI. `__cobrust_assert`
+ `__cobrust_result_err_panic` are not emitted by the LLVM backend today
so cannot trap; when either is wired, declare its `usize` arg with
`usize_ty` (one-line, same pattern).
