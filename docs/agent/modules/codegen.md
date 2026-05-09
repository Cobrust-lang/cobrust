---
doc_kind: module
module_id: mod:codegen
crate: cobrust-codegen
last_verified_commit: 078eab9
dependencies: [mod:mir, mod:types, adr:0023, adr:0027]
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
| `Rvalue::Aggregate(Tuple)` | **M12.x**: `__cobrust_tuple_new(n)` + per-slot `__cobrust_tuple_set` | M12.x mirror |
| `Rvalue::Aggregate(List)` | **M12.x**: `__cobrust_list_new(elem_size, len)` + per-elem `__cobrust_list_set` | M12.x mirror |
| `Rvalue::Aggregate(Dict)` | **M12.x**: `__cobrust_dict_new(k_size, v_size, len)` + per-pair `__cobrust_dict_set` | M12.x mirror |
| `Rvalue::Aggregate(Set)` | **M12.x**: `__cobrust_set_new(elem_size, len)` + per-elem `__cobrust_set_insert` | M12.x mirror |
| `Rvalue::Aggregate(FormatString)` | **M12.x**: `__cobrust_str_new` + per-part dispatch (`push_static / fmt_int / fmt_float / fmt_bool / fmt_str / fmt_repr`) | M12.x mirror |
| `Rvalue::Ref(_, place)` | **M12.x**: lazy stack-slot allocation; `stack_addr` + Field projections via `iadd` constant offset | M12.x mirror |
| `Rvalue::Cast(IntToFloat)` | **M12.x**: `fcvt_from_sint` | M12.x mirror |
| `Rvalue::Cast(FloatToInt)` | **M12.x**: `fcvt_to_sint_sat` (saturates per Rust `as`) | M12.x mirror |
| `Rvalue::Cast(BoolToInt)` | **M12.x**: `uextend` (i8 → i64) | M12.x mirror |
| `Rvalue::Cast(IntToBool)` | **M12.x**: `icmp NotEqual` against zero | M12.x mirror |
| `Rvalue::Cast(StrToBytes / BytesToStr)` | **M12.x**: pointer pass-through (layout identical) | M12.x mirror |
| `Stmt::For` (HIR-resident) | **M12.x** (ADR-0027 §4 for-protocol): MIR Calls dispatch to `__cobrust_iter_init` (iter() bind) → header loop calls `__cobrust_iter_next` (next() pull) against the four closed-world `cobrust-stdlib::iter` types (`ListIter / DictIter / SetIter / RangeIter`) implementing the `Iterator` trait | M12.x mirror |
| Heap allocation entry (`__cobrust_alloc(size) -> *mut u8`) | **M12.x** (ADR-0027 §1): stdlib runtime fn, mimalloc-backed when feature on; serves Aggregate constructors + f-string buffer + future user heap. The codegen emits `__cobrust_iter_init / __cobrust_fmt_int / __cobrust_str_new` calls that internally route allocations through this entry | M12.x mirror |
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

## M11.2 amendments (per ADR-0034)

Per ADR-0034 §"Decision" Option 3, the Cranelift backend now lowers
`Operand::Constant(Constant::FnRef(id))` callees in a `Terminator::Call`
to a real Cranelift `call` instruction. The mechanism is a classical
two-pass:

- **Pass 1 (declare)** — `emit()` already iterates `module.bodies`
  twice. The first iteration calls `declare_body` for every body,
  which calls `obj.declare_function(name, Linkage::Export, sig)` and
  records the resulting `FuncId` in `CraneliftCtx.function_ids` keyed
  on `body.def_id.0`. The same call records the body's declared
  return type in `CraneliftCtx.body_return_types` so cross-fn return
  types are queryable when the inferred-locals fixed-point runs in
  pass 2.
- **Pass 2 (define)** — the second iteration calls `define_body` for
  every body. Inside `define_body`, every entry of `function_ids` is
  converted to a `FuncRef` (per-builder scope) via
  `obj.declare_func_in_func` and stored in a per-body `user_funcs:
  HashMap<u32, ir::FuncRef>`. The `EmitCtx` carries a borrow of this
  map. `lower_call` consults `user_funcs` whenever the callee operand
  is `Constant::FnRef(id)` and emits the real `call` — args, return
  value, jump-to-continuation — exactly mirroring the existing
  `extern_funcs` branch.

This closes the M9 stub at `lower_call` line 870-878 (the
"M11 will materialize the FnRef path" comment) and unlocks recursion
+ mutual recursion + arbitrary user-defined cross-fn dispatch. The
`Constant::FnRef(_)` fallback in `lower_constant` (zero-pointer for
first-class FnRef use) is preserved unchanged.

### Interaction with ADR-0033 inferred_locals fixed-point

ADR-0033's per-fn `inferred_locals` runs at codegen time to type
`Ty::None`-declared locals (synthetic temps `_un` / `_bin` /
`_callret`). M11.2's forward-declaration pass operates at the
fn-signature boundary; the two layers are orthogonal. When a caller
contains `_callret = call FnRef(M)`, `infer_local_types` consults
`body_return_types[M]` to type `_callret` directly — closing the
interaction surface that would otherwise leave `_callret` defaulting
to `I8` and miscompiling any caller chain (e.g.
`print_int(fib(10))`). The mandatory regression case
`fnref_inferred_locals_recursive_chain` (in
`crates/cobrust-codegen/tests/fnref_call_corpus.rs`) exercises this
exact path.

### MIR-side amendment

To enable the codegen-level branch, MIR's `lower_call`
(`crates/cobrust-mir/src/lower.rs`) was extended: a `Name` callee
expression whose resolved type is `Ty::Fn(...)` lowers to
`Operand::Constant(Constant::FnRef(rn.def_id.0))` instead of the
generic `Operand::Move(Place::local(L))`. Non-fn-typed callees (e.g.
indirect-call locals storing fn pointers, lambdas) keep the existing
expression-lowering path. This is the single MIR change ADR-0034
requires; everything else lives in codegen.

## M11.3 lower_condition extraction (per ADR-0035)

Per ADR-0035 §"Decision" Option 2, the `if` and `while` heads now route
through a single shared `lower_condition` root primitive. The primitive
lives in MIR (`crates/cobrust-mir/src/lower.rs`), not codegen — the
ADR's hypothesis ("the divergence is in `cranelift_backend.rs`") was
diagnosed wrong; empirical CLIF + MIR dumps showed the `while` arm of
`lower_loop` was terminating the wrong block (the `header` block,
already terminated by a `Mod`-divassert) with `SwitchInt`, leaving the
condition's final assigns orphaned in a downstream block. The codegen
path was correct in both heads; only the MIR shape diverged.

**MIR-side change** (this section is informational for codegen
consumers; see `mod:mir` §"M11.3 lower_condition extraction" for the
authoritative description):

- New `lower_condition(expr) -> (Operand, BlockId)` helper. Returns
  the cond Operand and the `cond_end_block` (the block where the
  Operand's value is finally available). Caller terminates
  `cond_end_block` with `SwitchInt` (or `Assert` for div-style
  asserts).
- `lower_if` and `lower_loop`'s While arm both call `lower_condition`.
  Pre-fix `lower_if` already used the correct `cond_end_block` pattern
  inline; M11.3 hoists it into the shared primitive so future
  divergence cannot recur.

**Codegen impact**: zero. The fix is invisible to the Cranelift
backend — it just consumes the now-correctly-shaped MIR. Existing
ADR-0033 (`inferred_locals` fixed-point) and ADR-0034
(`Constant::FnRef` Call lowering) interactions are orthogonal because
`lower_condition` operates on block-flow shape, not on operand types
or fn-call lowering. Verified by corpus cases
`while_condition_through_inferred_locals_chain` (ADR-0033 cross) and
`while_binop_with_function_call` (ADR-0034 cross) in
`crates/cobrust-codegen/tests/while_condition_corpus.rs`.

**Why the bug was specific to `while` heads**: `lower_if` already
captured `cond_end_block = current_block_id()` after `lower_expr(cond)`
(see ADR-0030 M11.1 fix). `lower_loop`'s While arm had its own
hand-rolled equivalent that reset `cur_block` back to the loop
`header` after `lower_expr` returned, blindly assuming the cond was
materialised in `header`. For trivial conds (`n > 0`, `n == 5`) that
assumption held; for `<BinOp> == 0` style conds (LC 263 trigger), the
divassert chain split the cond eval across two blocks and the SwitchInt
ended up reading a stale value.

Cross-references: ADR-0035, ADR-0030 (M11.1 sibling fix), ADR-0033
(`inferred_locals` fixed-point — orthogonal), ADR-0034 (FnRef Call
— orthogonal), `findings/while-binop-eq-zero-condition-miscompile.md`,
`findings/two-bugs-one-fix-option-c-pattern.md`.

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
