---
doc_kind: finding
finding_id: m9-cross-arch-linux-x86_64-validation
last_verified_commit: 60243ab
dependencies: [adr:0023, adr:0033]
status: closed-by-fix
---

# Finding: M9 cross-architecture validation — Linux x86_64

## Hypothesis

M9 Cranelift codegen + cobrust-stdlib runtime ABI (ADR-0023's
SystemV AMD64 binding) produce bit-identical stdout to the macOS
arm64 baseline at HEAD `b83ea80`.

## Method

SSH into 2×3090 workstation (Linux x86_64, Ubuntu 22.04, kernel
5.15.0-176), rsync HEAD `b83ea80` to `~/cobrust-cross-arch`, then
run the 5-gate workspace tests + the 4 canonical example binaries.

```
workstation: <internal Linux x86_64 validator host>
ssh: <redacted user@host -p port>
toolchain: rustc 1.94.1 / cargo 1.94.1 (bootstrapped, matches rust-toolchain.toml)
arch: x86_64-unknown-linux-gnu
```

## Result

### Gate table

| Gate | macOS arm64 | Linux x86_64 |
|---|---|---|
| `cargo build --workspace --locked` | exit 0 | **exit 0** |
| `cargo test --workspace --locked` | exit 0 (2430+ pass) | **FAIL: 314 pass, 4 fail** |
| `cargo clippy --workspace -D warnings` | exit 0 | **exit 0** |
| `hello.cb` stdout | `hello, world` | **PASS: identical** |
| `fizzbuzz.cb` stdout | algorithm 1..15 | **PASS: identical** |
| `fib.cb` stdout | `fib(10) =\n55` | **PASS: identical** |
| `notebook diff vs expected.txt` | empty | **PASS: diff exit 0** |

### Failing tests (all in `cobrust-codegen::codegen_well_formed`)

```
p008_const_float_neg   fn f() -> f64: return -2.71
p017_fadd              fn f(a: f64, b: f64) -> f64: return a + b
p018_fsub              fn f(a: f64, b: f64) -> f64: return a - b
p019_fmul              fn f(a: f64, b: f64) -> f64: return a * b
```

All 4 failures panic inside Cranelift's x64 emitter:

```
cranelift_codegen::isa::x64::inst::emit::emit::{{closure}}
  at cranelift-codegen-0.131.1/src/isa/x64/inst/emit.rs:1057:26
internal error: entered unreachable code
```

The panic is inside `Inst::CvtFloatToSintSeq`'s `cvtt_op` closure,
which handles `(*src_size, *dst_size)` pairs for float-to-int
conversion. The closure panics because neither size matches `(Size32,
Size32|Size64)` or `(Size64, Size32|Size64)`.

## Root-cause analysis

### Type inference bug in `cranelift_backend.rs`

MIR lowering introduces `Ty::None`-typed synthetic temporaries (e.g.
`_un` for a unary-negation result, `_bin` for a binary-op result).
The Cobrust codegen has a two-pass type inference:

- **`infer_local_types`**: scans every `Statement::Assign` and
  infers each `Ty::None` local's type from the RHS `rvalue_ty`.
  Works correctly: `_1 = UnaryOp(Neg, Constant::Float(2.71))` →
  `rvalue_ty` → `operand_ty(Constant::Float)` = `F64`. `_1` gets
  `F64`. ✓

- **`infer_return_type`**: scans statements that assign to the return
  local `_0`. For `return -2.71`, MIR emits:
  `_0 = Use(Copy(Place(_1)))`.
  `infer_return_type` → `rvalue_ty(Use(Copy(_1)))` →
  `operand_ty(Copy(_1))` → `body.locals[1].ty = Ty::None` →
  `cranelift_scalar_ty(Ty::None)` = `Some(I8)`. **WRONG: infers
  I8 instead of F64.** ✗

The `operand_ty` function for `Operand::Copy(place)` looks up the
local's **declared type**, not the **inferred type**. It doesn't
consult `inferred_locals`. When the return value is a chain
`_0 = Copy(_1)` where `_1.ty = Ty::None`, the return type is
inferred as `I8` (the cranelift-scalar-ty of `Ty::None`).

As a result:
- `_0` Variable is declared as `I8` (wrong)
- `_1` Variable is declared as `F64` (correct, via `infer_local_types`)
- Cranelift IR: `_0 := ireduce.i8(fneg.f64(...))` or similar
  implicit F64→I8 coercion
- On x86_64 (System V), Cranelift lowers this via `CvtFloatToSintSeq`
- `CvtFloatToSintSeq` receives `src_size=F64=Size64`, `dst_size=I8=Size8`
- The match in `cvtt_op` closure only handles `(Size32, Size32|Size64)`
  and `(Size64, Size32|Size64)`; `Size8` is not covered → `unreachable!()`

### Why macOS arm64 does NOT fail

On AAPCS64 (AppleAarch64), the Cranelift aarch64 backend lowers
F64→I8 differently — the truncation path does not have the same
size-exhaustive match. The mismatched type survives silently, and
the function produces incorrect-but-non-panicking output. (The 4
tests pass because they only verify compile-to-object success,
not value correctness; the runtime values would be wrong.)

This means the bug is present on both platforms but is **latent on
macOS arm64** and **fatal on Linux x86_64**.

### Affected scope

- Any Cobrust function where the return type flows through one or
  more `Ty::None`-typed temporary locals (common for all non-trivial
  float expressions).
- All float arithmetic (`+`, `-`, `*`, `/`) and unary negation on
  floats where the result chain passes through a `Ty::None` temp.
- Integer operations are unaffected because `Ty::None → I8` is only
  wrong for float result types.

### Fix direction (for CTO / fix-sprint)

**Option A (minimal):** In `infer_return_type`, after resolving
`operand_ty` for a `Copy` of a `Ty::None` local, also check the
`inferred_locals` map for that local's inferred type.

**Option B (comprehensive):** Merge the two inference passes into a
single worklist-based inference that propagates types transitively:
if `_0 = Copy(_1)` and `_1`'s inferred type is `F64`, then `_0`
is also `F64`. This handles arbitrary-depth copy chains.

Option B is the correct long-term fix; Option A is a targeted patch
that covers the observed failure pattern.

## Conclusion

**PARTIAL PASS / LINUX-ONLY BUG CONFIRMED**

- `cargo build`, `cargo clippy`, and all 4 example binaries: PASS
- `cargo test`: FAIL — 4 tests in `cobrust-codegen::codegen_well_formed`
  all involving float arithmetic / float negation
- Root cause: type inference in `cranelift_backend::infer_return_type`
  fails to consult `inferred_locals` when the return chain passes
  through a `Ty::None` temp, causing Cranelift to generate an I8 return
  type, which on x86_64 triggers a fatal `unreachable!()` in
  `CvtFloatToSintSeq` emission
- This is a **latent bug on macOS arm64** (wrong value, no panic) and
  a **fatal panic on Linux x86_64** — therefore this is correctly
  classified as a Linux-only observable failure

**CTO action required: dispatch a fix sprint for `infer_return_type` /
`operand_ty` before proceeding with tomli real-LLM E2E (audit #1).**

## Resolution

ADR-0033 (`docs/agent/adr/0033-codegen-float-return-fix.md`) closes
the bug via Option C — fixed-point inference + threading the
inferred-locals map through `operand_ty` and `rvalue_ty`.

Verified post-fix on both delivery-scope architectures:

| Arch                  | `cargo test --workspace --locked`     | float corpus (16 cases) | 4 named tests   |
|-----------------------|---------------------------------------|-------------------------|-----------------|
| macOS aarch64         | passes (count up by 16 vs 2,430+ baseline) | 16 / 16 pass        | all 4 PASS      |
| Linux x86_64          | passes (codegen / cli / stdlib gates) | 16 / 16 pass            | all 4 PASS      |

Linux x86_64 verification went through the
`<internal Linux x86_64 validator host>` workstation per
`~/.claude/projects/-Users-hakureirm-codespace-Study-Cobrust/memory/reference_x86_workstation.md`,
synced via `rsync` of the `feature/codegen-float-return-fix`
branch tree. The `cobrust-msgpack::msgpack_fuzz` test failed on
x86_64 with a 190 GiB allocation request — that is a separate,
pre-existing fuzz-knob issue unrelated to ADR-0033 and not gated
by this finding.

## Cross-references

- ADR-0023 (M9 codegen target matrix)
- ADR-0033 (the fix)
- `crates/cobrust-codegen/src/cranelift_backend.rs`: `infer_return_type`,
  `operand_ty`, `infer_local_types`, `rvalue_ty`
- `crates/cobrust-codegen/tests/float_return_corpus.rs` — the
  16-case regression net.
- Local memory: `reference_x86_workstation.md` (workstation access)
- review-claude 三轮反馈 ① B (2026-05-09)
- `cranelift-codegen-0.131.1/src/isa/x64/inst/emit.rs:1057`
  (`CvtFloatToSintSeq` closure)
