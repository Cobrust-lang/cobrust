---
doc_kind: module
module_id: mod:codegen
crate: cobrust-codegen
last_verified_commit: TBD
dependencies: [mod:mir, mod:types, adr:0023]
---

# Module: codegen

## Purpose

Lower MIR to native code. Two backends behind a feature flag;
default depends on the build profile (per ADR-0023).

## Status

- **M9 — delivered.** ADR-0023 pinned the design; implementation
  matches. 158 tests across `codegen_well_formed /
  codegen_ill_formed / codegen_diff_corpus /
  codegen_object_layout / codegen_release_smoke` pass.

## Backend matrix

| Backend | Default for | Pros | Cons |
|---|---|---|---|
| Cranelift (`Backend::Cranelift`) | `cargo build` (dev) | Pure Rust, fast compile, no system deps | Less mature optimization |
| LLVM (`Backend::Llvm`) — `--features llvm` | `cargo build --release` (when feature on) | Best codegen quality, broad target support | Slow build, large dep tree, requires system LLVM |

`Backend::default_for_dev()` always returns `Cranelift`.
`Backend::default_for_release()` returns `Llvm` if `cfg!(feature =
"llvm")` is on, otherwise `Cranelift`.

## Public surface (M9)

```rust
// emit a MIR module to native artifact
pub fn emit(module: &cobrust_mir::Module, spec: TargetSpec) -> Result<Artifact, CodegenError>;

// target specification
pub struct TargetSpec {
    pub triple: target_lexicon::Triple,
    pub opt_level: OptLevel,
    pub backend: Backend,
    pub artifact: ArtifactKind,
    pub output_dir: PathBuf,
    pub module_name: String,
}
impl TargetSpec {
    pub fn host_dev(output_dir: PathBuf, module_name: impl Into<String>) -> Self;
    pub fn host_release(output_dir: PathBuf, module_name: impl Into<String>) -> Self;
    pub fn host_object(output_dir: PathBuf, module_name: impl Into<String>) -> Self;
}

pub enum OptLevel { None, Speed, SpeedAndSize }

pub enum Backend { Cranelift, Llvm }
impl Backend {
    pub fn default_for_dev() -> Self;       // always Cranelift
    pub fn default_for_release() -> Self;   // Llvm if feature else Cranelift
}

pub enum ArtifactKind { Object, Executable, DynamicLibrary }
impl ArtifactKind { pub fn extension(self, triple: &Triple) -> &'static str; }

pub enum Artifact {
    Object(PathBuf),
    Executable(PathBuf),
    DynamicLibrary(PathBuf),
}
impl Artifact {
    pub fn path(&self) -> &Path;
    pub fn is_executable(&self) -> bool;
}

pub enum CodegenError {
    UnsupportedBackend(Backend),
    UnsupportedTarget(String),
    InvalidMir(String),
    CraneliftError(String),
    LlvmError(String),
    ObjectEmission(String),
    LinkerFailed { exit_code: i32, stderr: String },
    Io(String),
    Internal(String),
}

// ABI helpers exposed for tests + downstream consumers (M10 driver).
pub mod abi {
    pub fn cranelift_call_conv(triple: &Triple) -> CallConv;
    pub fn cranelift_scalar_ty(ty: &Ty) -> Option<ir::Type>;
    pub fn is_copy_ty(ty: &Ty) -> bool;
    pub fn pointer_ty(triple: &Triple) -> ir::Type;
}

// Linker helpers
pub mod linker {
    pub fn link(object: &Path, output: &Path, kind: ArtifactKind) -> Result<PathBuf, CodegenError>;
    pub fn linker_available() -> bool;
}
```

## ABI (per ADR-0023 §"Calling convention details")

| Aspect | AMD64 (Linux) | AArch64 (macOS) |
|---|---|---|
| Cranelift call conv | `CallConv::SystemV` | `CallConv::AppleAarch64` |
| Integer arg regs | rdi rsi rdx rcx r8 r9 | x0 x1 x2 x3 x4 x5 x6 x7 |
| Float arg regs | xmm0..xmm7 | d0..d7 |
| Integer return | rax (rdx for 128-bit) | x0 (x1 for 128-bit) |
| Float return | xmm0 | d0 |
| Stack alignment at call | 16 bytes | 16 bytes |
| Red zone | 128 bytes | none |
| Pointer width | 64 bits | 64 bits |

## Linker delegation

- **Default**: invoke `cc` (via `$CC` env or `cc` on `$PATH`).
- **`--features lld`**: pass `-fuse-ld=lld` to `cc`.
- **No bundled linker**.
- **Captures** stderr + exit code into [`CodegenError::LinkerFailed`].

## Per-MIR-form lowering rules

| MIR construct | Cranelift | LLVM (M9 feature stub) |
|---|---|---|
| `Body` | `Function` with `Signature` | `FunctionValue` |
| `LocalDecl` | `Variable` bound via `declare_var` + `def_var` | stack alloca + load/store |
| `BasicBlock` | `Block` via `FunctionBuilder::create_block` | `BasicBlock` via `LLVMAppendBasicBlock` |
| `Statement::Assign` | RHS lowered → `def_var(LHS)` | `build_store` |
| `Terminator::Goto(b)` | `ins().jump(b, &[])` | `build_unconditional_branch` |
| `Terminator::SwitchInt` | brif chain (bool) / br_table (int) | `build_switch` |
| `Terminator::Return` | `ins().return_(&[ret])` | `build_return` |
| `Terminator::Call` | `ins().call(callee, &args)` | `build_call` |
| `Terminator::Drop` | call to `_cobrust_drop_<TypeId>` | same |
| `Terminator::Unreachable` | `ins().trap(TrapCode::User(1))` | `build_unreachable` |
| `Terminator::Assert` | conditional jump → trap | conditional jump → call panic |
| `Rvalue::BinaryOp(Add)` | `iadd / fadd` | `build_int_add / build_float_add` |
| `Rvalue::Aggregate(Tuple, ...)` | M9 stub: zero-pointer (M12 lifts) | M9 stub: zero-pointer |
| `Rvalue::Ref` | M9 stub: zero-pointer (M12 lifts) | M9 stub: zero-pointer |
| `Operand::Constant(Int)` | `iconst.i64` | `i64_type.const_int` |
| `Operand::Constant(Constant::Str(s))` (in `Terminator::Call` `args[0]` slot whose `func` is `Constant::Str(_)`) | **M11**: intern `s` as `.rodata` data symbol; emit `(ptr, len)` Cranelift values | M11 forward |
| `Terminator::Call { func: Constant::Str(name), args: [Constant::Str(payload), ...] }` | **M11**: declare `name` as `Linkage::Import` with `(*const u8, usize)` signature; emit real `call` with payload | M11 forward |
| User `fn main` | **M11**: exported as `_cobrust_user_main`; the C entry shim provides platform `main(argc, argv)` | M11 forward |

## Type inference for unresolved MIR locals

The MIR's `_return` slot is declared `Ty::None`; sub-expression
spill temps may also carry `Ty::None`. The Cranelift backend
runs a pre-pass that walks every `Statement::Assign`; the first
rvalue assigned to a local gives that local's effective Cranelift
type. This recovers the function's actual return type and
intermediate-temp widths without modifying the MIR.

## Differential corpus

For every "core 30" form we exercise the M9 in-scope subset
(arithmetic, comparison, branching, looping, recursion). For each
form, the `tests/codegen_diff_corpus.rs` set:
- compiles the Cobrust source through `emit`,
- compiles a hand-written Rust reference (when `rustc` is on
  `$PATH`),
- asserts both produce non-empty relocatable object files with
  matching arity / signature shape.

Out-of-scope-at-M9 forms (f-string, collections, lambda capture,
slice / attr access, await/yield) are recorded as `#[ignore]`'d
M10/M11 follow-up cases.

## Object layout

| Format | Triple OS | Sections | Symbol prefix |
|---|---|---|---|
| ELF | linux | `.text`, `.rodata`, `.data` | none |
| Mach-O | darwin / ios | `__text`, `__cstring`, `__data` | `_` |

`object` crate parsing: every emitted file is parseable by
`object::File::parse(&bytes)` with a non-empty symbol table that
includes every exported function name (with `_`-prefix on Mach-O).

## Invariants (M9)

- `emit` is total over MIR — every `cobrust_mir::Module` either
  yields an `Artifact` or yields a structured `CodegenError`.
- The Cranelift backend never panics on a well-formed MIR module.
- Linker invocation is captured + structured: no transient
  process error escapes as a panic.
- Pointer width matches the host triple (64 bits on x86_64 +
  aarch64).
- Backend selection is deterministic given `(spec.backend,
  cfg!(feature = "llvm"))`.

## M11 amendments (per ADR-0025)

Per ADR-0025 §"Codegen amendments":

- **Constant::Str** payloads referenced as the first argument of a
  `Terminator::Call` whose `func` is `Constant::Str(name)` are interned
  via `ObjectModule::declare_data` + `define_data` (boxed-slice payload).
  At the call site, the data symbol is materialized via
  `declare_data_in_func` + `symbol_value(pointer_type, gv)`, paired with
  an `iconst.i64` length value, and passed to the runtime helper.
- The runtime-helper signature widens from M10's `void(void)` to
  `(*const u8, usize)`. M10 hello-world callsite path remains green
  via the lifted intrinsic in `cobrust-cli/src/build/intrinsics.rs`
  passing the literal payload through.
- The user's top-level `main` Body is emitted as `_cobrust_user_main`
  (instead of `main`); the C entry shim (`cobrust-cli/runtime/cobrust_main.c`)
  provides platform `int main(int, char**)` per ADR-0025 §G.
- All amendments are **additive**; the M9 158-test baseline remains
  green.

## Done means (M9 — DONE)

- [x] `Backend::Cranelift` produces correct object files for ≥ 60
      well-formed MIR programs.
- [x] `Backend::Llvm` is feature-gated; without `--features llvm`,
      the backend returns `CodegenError::UnsupportedBackend`.
- [x] Object files parse via the `object` crate; symbols + sections
      match expected tables.
- [x] Linker delegation invokes `cc` and captures stderr.
- [x] System V AMD64 + AAPCS64 calling conventions selected by
      triple.
- [x] Differential corpus covers the M9 in-scope subset of the
      "core 30"; M11+ forms tracked as `#[ignore]`'d follow-ups.
- [x] `adr:0023` accepted; implementation matches.

## Non-goals (M9)

- LLVM full lowering (M9.1 follow-up; the surface ships, the
  body stubs return `CodegenError::LlvmError`).
- Drop handler materialization (M11 stdlib).
- Aggregate / collection / list / dict layout (M11 stdlib).
- f-string runtime (M11 stdlib).
- Lambda + closure capture (M10 alongside calling convention).
- Generator state-machine lowering (M13 structured concurrency).
- Cross-compilation matrix beyond x86_64 + aarch64.
- WASM target (Phase F).

## Cross-references

- `adr:0023` — backend feature flags, ABI, linker delegation,
  target matrix (authoritative).
- `adr:0019` — Phase E roadmap; M9 row.
- `adr:0020` — MIR shape; M9 is the consumer.
- `adr:0012` — "translate the surface, bind the core"; Cranelift +
  inkwell are bound, not reimplemented.
- `mod:mir` — input.
- `mod:cli` — future M10 consumer of `emit`.
- Constitution `CLAUDE.md` §4.1 (compiler layers), §5.3 (efficient:
  AOT default).
