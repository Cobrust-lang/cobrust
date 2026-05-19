---
doc_kind: module
module_id: mod:codegen
crate: cobrust-codegen
last_verified_commit: 0590731
dependencies: [mod:mir, mod:types, adr:0023, adr:0027, adr:0041, adr:0058, adr:0058a, adr:0058d]
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
    /// ADR-0041 §H3 — `**`/`@`/`in`/`not in` no longer silently emit
    /// `iconst(I64, 0)`. Codegen returns this variant; M11.x integer
    /// pow + container-membership runtime closes the gap.
    UnimplementedBinOp { op: &'static str, note: &'static str },
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

## M-F.3.3 — f64 and `as`-cast codegen (ADR-0050 §A1)

| Feature | Location | Notes |
|---|---|---|
| `runtime_helper_signatures` math shims | `cranelift_backend.rs` — 11 new entries: `__cobrust_math_sqrt` … `__cobrust_math_exp` all `(f64) -> f64`; `__cobrust_math_pow` is `(f64, f64) -> f64` | M-F.3.3 gap (b) |
| `__cobrust_fmt_float_prec` signature | `cranelift_backend.rs` — `(buf: ptr, val: f64, spec_ptr: ptr, spec_len: i64) -> void` | M-F.3.3 gap (c) |
| `lower_aggregate_format_string` FMTSPEC | `cranelift_backend.rs` — detects `Constant::Str("FMTSPEC:<spec>")` sentinel after a float operand; routes to `__cobrust_fmt_float_prec` | M-F.3.3 gap (c) |
| FormatString Str intern scan | `cranelift_backend.rs` — scans `Rvalue::Aggregate(FormatString, ..)` operands in pre-pass to intern their Constant::Str payloads (including FMTSPEC sentinels and bare specs) | M-F.3.3 gap (c) |
| `inferred_locals` math call resolution | existing `runtime_helper_return_types` map — now includes math shim entries so `_callret` locals for math calls get inferred as `F64` | M-F.3.3 gap (b) |

Invariants:
- Math shim args are passed as `f64` directly (not via `coerce_to_i64` path); the codegen `lower_terminator` passes operands via `lower_operand` which preserves float types.
- The FMTSPEC sentinel string must be interned in the pre-pass; `materialize_str_data` fails if called with an uninternded payload.

## Cross-references

- `adr:0023` — backend feature flags, ABI, linker delegation,
  target matrix (authoritative).
- `adr:0019` — Phase E roadmap; M9 row.
- `adr:0020` — MIR shape; M9 is the consumer.
- `adr:0012` — "translate the surface, bind the core"; Cranelift +
  inkwell are bound, not reimplemented.
- `adr:0050` §A1 — M-F.3.3 f64 gap table.
- `adr:0050c` — M-F.3.2 Str ownership + list[str] drop schedule
  (codegen consumer of the MIR-level closure).
- `mod:mir` — input.
- `mod:cli` — future M10 consumer of `emit`.
- Constitution `CLAUDE.md` §4.1 (compiler layers), §5.3 (efficient:
  AOT default).

## ADR-0050c M-F.3.2 — Str + list[str] codegen surfaces

| Surface | Anchor |
|---|---|
| `emit_drop_for_ty(place, ty)` | `cranelift_backend.rs:965-1014` — polymorphic drop helper. `Ty::Str` → `__cobrust_str_drop`; `Ty::List(Ty::Str)` → `__cobrust_list_drop_elems(ptr, __cobrust_str_drop)`; `Ty::List(_)` (other elem types) → `__cobrust_list_drop`; anything else → no-op. |
| `Terminator::Drop` arm | `cranelift_backend.rs:1108-1124` — wired to `emit_drop_for_ty(place, &body.locals[place.local].ty)`. |
| Aggregate(List) Str slot | `cranelift_backend.rs:1396-1450` — `Constant::Str(payload)` materialised via `materialize_str_buffer`; non-literal Str-typed operand cloned via `__cobrust_str_clone`. Each slot owns a fresh heap copy. |
| Str literal interning pre-pass | `cranelift_backend.rs:761-810` — walks BOTH `Terminator::Call` args AND every statement's `Rvalue::Aggregate` operands for `Constant::Str(payload)`. Closes the "str payload not interned" codegen-time bug for list literals. |
| FnRef call arg Str materialise | `cranelift_backend.rs:1180-1212` — `Constant::Str` literal args to user-fn calls route through `materialize_str_buffer` so the callee's param receives a real heap pointer (was 0 under the M9 stub). |
| `let v: str = "literal"` materialise | `cranelift_backend.rs:1048-1075` — `Use(Constant::Str(_))` rvalue with a Str-typed destination routes through `materialize_str_buffer`. |
| `operand_ty` for indirect types | `cranelift_backend.rs:286-310` — Str / List / Tuple / Dict / Set / Record / Adt / Alias / Fn now resolve to `pointer_type` (i64) instead of None. Fixes signature inference for Str-returning fns so the Cranelift verifier accepts the call ABI. |
| f-string Str hole dispatch | `cranelift_backend.rs:1547-1626` — inspects MIR-declared type. `Ty::Str` operand extracts `(ptr, len)` via `__cobrust_str_ptr` + `__cobrust_str_len`, then calls `__cobrust_fmt_str(buf, ptr, len)`. Previously routed through `__cobrust_fmt_int` printing the pointer as a decimal number. |
| `__cobrust_list_is_empty` signature | `cranelift_backend.rs:2082` — `(p) -> i64` — symmetric to `__cobrust_dict_is_empty` (ADR-0050d Decision 5 addendum). |
| `__cobrust_str_clone` signature | `cranelift_backend.rs:2141` — `(p) -> p` — closes the Phase 4 explicit-clone path. |
| `__cobrust_list_drop_elems` signature | `cranelift_backend.rs:2068+` — `(p, p) -> ()` — the second `p` is the per-element fn pointer materialised via `func_addr` on `__cobrust_str_drop`. |

## Phase K wave-1 — LLVM backend MIR→LLVM IR core (ADR-0058a)

Status: **delivered at HEAD `4686192`**. DG verify: 355 tests PASS,
TEST_EXIT=0, 0 regressions.

### Public surface (LLVM backend)

```rust
// crates/cobrust-codegen/src/llvm_backend.rs
// Gated behind #[cfg(feature = "llvm")].

/// Public LLVM backend entrypoint. Mirrors cranelift_backend::emit.
pub fn emit(module: &Module, spec: &TargetSpec) -> Result<Artifact, CodegenError>;

/// Per-emit state. Borrows the inkwell Context for `'ctx`.
pub struct LlvmEmitter<'ctx> {
    pub module: inkwell::module::Module<'ctx>,
    // ... private fields ...
}

impl<'ctx> LlvmEmitter<'ctx> {
    /// Construct + pre-declare runtime-helper externs.
    pub fn new(
        ctx: &'ctx inkwell::context::Context,
        spec: &TargetSpec,
        target_machine: &inkwell::targets::TargetMachine,
    ) -> Result<Self, CodegenError>;

    /// First pass — declare function symbol so cross-body calls resolve.
    pub fn declare_body(&mut self, body: &Body) -> Result<(), CodegenError>;

    /// Second pass — emit the function body.
    pub fn define_body(&mut self, body: &Body) -> Result<(), CodegenError>;
}
```

### Lowering tables

#### §4 MIR Ty → LLVM type (wave-1 revised)

| MIR `Ty` | LLVM type | Notes |
|---|---|---|
| `Bool` | `i1` | `ctx.bool_type()` |
| `Int` | `i64` | `ctx.i64_type()` |
| `Float` / `Imag` | `double` | `ctx.f64_type()` |
| `None` | `i64` | mirrors Cranelift `pointer_type` fallback; revised from spec's `i8` |
| `Str` / `Bytes` | `i8*` (opaque ptr) | `ctx.i8_type().ptr_type(AddressSpace::default())` |
| `List[T]` / `Dict[K,V]` / `Set[T]` | `i8*` (opaque ptr) | heap-managed; element ty stays MIR-level |
| `Ref(T)` | same as `T` (transparent) | borrow tracked at MIR per ADR-0020 B1..B5 |
| `Tuple(...)` / `Record(_)` / `Adt(_,_)` / `Alias(_,_)` / `Fn(_)` | `i8*` opaque ptr | by-pointer at wave-1 |

#### §5 Operand lowering

| MIR `Operand` | LLVM | inkwell call |
|---|---|---|
| `Copy(place)` | `load ty, place_ptr` | `builder.build_load(ty, alloca, "load")` |
| `Move(place)` | `load ty, place_ptr` | same (Move semantics enforced at MIR) |
| `Constant(Int(i))` | `i64` constant | `ctx.i64_type().const_int(i as u64, true)` |
| `Constant(Float(bits))` / `Imag(bits)` | `double` constant | `ctx.f64_type().const_float(f64::from_bits(bits))` |
| `Constant(Bool(b))` | `i1` constant | `ctx.bool_type().const_int(b as u64, false)` |
| `Constant(None)` | `i64` zero | matches `Ty::None → i64` mapping |
| `Constant(Str(_))` / `Bytes(_)` | `i8*` null | wave-1 stub (M11 materialises rodata) |
| `Constant(FnRef(_))` | `i64` zero | wave-1 stub |

#### §6 Terminator lowering

| MIR `Terminator` | LLVM | inkwell call |
|---|---|---|
| `Goto(b)` | unconditional branch | `builder.build_unconditional_branch(b)` |
| `Return` | return value | `builder.build_return(Some(&load(_return)))` |
| `Unreachable` | unreachable instr | `builder.build_unreachable()` |
| `SwitchInt {...}` | switch instr | `builder.build_switch(discr, default, &cases)` |
| `Call { FnRef(id), args, dest, target }` | call + branch | `builder.build_call(callee, &args, "call")` then branch |
| `Call { Constant::Str(_) or unknown, ..}` | wave-1 stub (zero ret) | M11 wires runtime-helper path |
| `Drop { place, target }` | runtime helper call + branch | dispatched by `emit_drop_for_ty(place, &local_ty)` |
| `Assert { cond, expected, msg, target }` | conditional branch to trap block | trap block builds `build_unreachable` (wave-1 stub for panic) |

#### Drop lowering by Ty (mirrors ADR-0050c TD-1)

| `Ty` | Helper | inkwell args |
|---|---|---|
| `Ty::Str` | `__cobrust_str_drop` | `[ptr_arg]` |
| `Ty::List(Ty::Str)` | `__cobrust_list_drop_elems` | `[ptr_arg, &__cobrust_str_drop as fn ptr]` |
| `Ty::List(other)` | `__cobrust_list_drop` | `[ptr_arg]` |
| other | (no-op) | — |

#### §7 Calling convention

- `CallConv::C` (inkwell default).
- Linux x86_64: System V AMD64 — argument registers `rdi rsi rdx rcx r8 r9`, return in `rax`/`xmm0`.
- macOS arm64: AAPCS64 — argument registers `x0..x7`, return in `x0`/`d0`.
- No custom convention at wave-1; sub-ADR 0058b may add an opt-driven convention.

### F34 symbol anchors

| Anchor | Role |
|---|---|
| `llvm_backend::emit` | public entry (`llvm_backend.rs:75`) |
| `LlvmEmitter::new` | emitter constructor (`llvm_backend.rs:204`) |
| `LlvmEmitter::declare_body` | first-pass fn symbol declare (`llvm_backend.rs:331`) |
| `LlvmEmitter::define_body` | second-pass fn body emit (`llvm_backend.rs:371`) |
| `BodyLowerer::lower_terminator` | per-block terminator dispatch (`llvm_backend.rs:500`) |

### Wave-1 non-goals (deferred — wave-2 status noted inline)

- **Optimization pass pipeline** (`OptLevel::Speed` / `SpeedAndSize`):
  **DELIVERED at wave-2 (ADR-0058b)** via `pass_pipeline_for(OptLevel)` +
  `Module::run_passes`. See "Phase K wave-2" section below.
- **DWARF debug-info emission**: sub-ADR 0058c.
- **Multi-target cross-compilation matrix**: **DELIVERED at wave-2 (ADR-0058b §3.4)**
  via `supported_tier1_triples()` enumeration; cross-link stays `release.yml`-scope.
- **Binary-size acceptance bar** (ADR-0023 §"LLVM `-O3` ≥ 30% smaller"):
  **RESOLVED at wave-2** under `OptLevel::SpeedAndSize`; see `tests/binary_size_bench.rs`.
- **`Constant::Str` / `Bytes` runtime-helper Call lowering**: wave-2;
  Cranelift M11 `__cobrust_*` extern-helper Call path not yet ported.
- **Aggregate construction** (List/Dict/Set/Tuple/Record): wave-2 stub
  returns null pointer (matches Cranelift mid-M9 posture pre-M11).
- **`Projection::Field` / `Projection::Index` / `Projection::Discriminant`**:
  wave-2 stub (Deref handled; others fall back to bare-local load).
- **`Rvalue::Discriminant` / `Len` / `NullaryOp`**: wave-2 stub returns
  i64 zero (matches Cranelift M9 posture).

### Test counts (DG verify at HEAD `4686192`)

| Suite | Passed | Notes |
|---|---|---|
| llvm_backend wave-1 smoke (inline) | 5 | empty/return-42/binop-add/unop-neg-float/drop-str |
| aggregate_corpus | 31 | wave-1 stub matches Cranelift mid-M9 stub |
| cast_corpus | 31 | cast lowering parity |
| codegen_diff_corpus | 22 (6 ignored) | M9 "core 30" diff gate |
| codegen_ill_formed | 50 | error taxonomy |
| codegen_object_layout | 16 | symbol/section assertions |
| codegen_release_smoke | 10 | LLVM-backed release default; recursion (fib/ack) NOW PASSES |
| function_corpus | 70 | user-fn call surface |
| ip_corpus | 33 | int-precision parity |
| list_corpus | 16 | aggregate/index/len |
| mir_to_codegen | 10 | end-to-end |
| mut_corpus | 12 | mutability + place writes |
| placeholder_corpus | 30 | placeholder ADR coverage |
| str_corpus | 12 | str materialise + drop |
| while_corpus | 12 | while + nested binop |
| while_if_corpus | 7 | fizzbuzz/short |
| **Total** | **355** | TEST_EXIT=0 |

## Phase K wave-2 — LLVM opt pipeline + multi-target (ADR-0058b)

Status: **delivered**. Extends wave-1's `llvm_backend::emit` with the LLVM new-pass-manager pipeline and codifies the ADR-0046 tier-1 multi-target dispatch contract.

### Public surface added (wave-2)

```rust
// crates/cobrust-codegen/src/llvm_backend.rs
// All gated behind #[cfg(feature = "llvm")].

/// Map OptLevel to PassBuilder pipeline string (ADR-0058b §3.2).
/// Returns None for OptLevel::None (no passes run).
pub fn pass_pipeline_for(level: OptLevel) -> Option<&'static str>;

/// ADR-0046 tier-1 four-triple binding contract (ADR-0058b §3.4).
pub fn supported_tier1_triples() -> &'static [&'static str];
```

### Pipeline mapping (binding)

| `OptLevel` | `pass_pipeline_for` | `TargetMachine` opt | LLVM behavior |
|---|---|---|---|
| `None` | `None` | `OptimizationLevel::None` | wave-1 path; `run_passes` skipped |
| `Speed` | `Some("default<O2>")` | `OptimizationLevel::Default` | LLVM `-O2` equivalent |
| `SpeedAndSize` | `Some("default<O3>,default<Os>")` | `OptimizationLevel::Aggressive` | LLVM `-O3` then size overlay |

`PassBuilderOptions::create()` defaults preserved at wave-2 (no manual `set_loop_*` / `set_inline_threshold` flipping); follow-up sub-ADR if empirical bench fails.

### Tier-1 triple matrix (ADR-0046)

| Triple | Object format | Backend availability |
|---|---|---|
| `aarch64-apple-darwin` | Mach-O | brew `llvm@18` on Mac |
| `aarch64-unknown-linux-gnu` | ELF | apt `llvm-18-dev` on Linux + `cross` |
| `x86_64-unknown-linux-gnu` | ELF | apt `llvm-18-dev` on Linux |
| `x86_64-unknown-linux-musl` | ELF (static) | apt `llvm-18-dev` + musl sysroot |

Cross-link stays in `release.yml` + `cross` scope (linker delegation per ADR-0023 §"Linker delegation" unchanged).

### Wave-2 test surface

| Suite | Tests | Notes |
|---|---|---|
| llvm_backend inline (wave-2) | 5 added | `pass_pipeline_mapping_matches_spec`, `tier1_triple_matrix_has_four_entries`, `tier1_triples_parse_via_target_lexicon`, `smoke_opt_speed_pipeline`, `smoke_opt_speed_and_size_pipeline` |
| binary_size_bench | 2 | `bench_fixtures`, `o3_median_under_70pct` (ADR-0023 §A3 close) |

5-fixture bench corpus: `hello`, `fizzbuzz`, `fib`, `dot_product`, `nested_branch`. Per-fixture O3/O0 ratios printed on stderr via `--nocapture`; median ratio asserted ≤ 0.70.

### F34 symbol anchors (wave-2)

| Anchor | Role |
|---|---|
| `llvm_backend::pass_pipeline_for` | OptLevel → PassBuilder pipeline mapping |
| `llvm_backend::supported_tier1_triples` | ADR-0046 tier-1 enumeration |
| `binary_size_bench::o3_median_under_70pct` | ADR-0023 §A3 empirical close assertion |

### Non-goals (deferred per ADR-0058b §4)

- **DWARF emission**: **DELIVERED at wave-3 (ADR-0058c)**; see "Phase K wave-3" section below.
- **JIT opt-level changes**: cobrust-jit `lower.rs` unchanged at wave-2.
- **Cross-link**: linker stays at `cc`; cross-target executables are `release.yml` + `cross`-tool scope.
- **New MIR features**: wave-2 consumes wave-1's IR-construction pass.
- **Manual PassBuilder flag tuning**: defaults preserved; sub-ADR follow-up only if bench fails on tier-1 host.

## Phase K wave-3 — LLVM DWARF debug-info emission (ADR-0058c)

Status: **delivered**. Extends wave-1/2's `LlvmEmitter` with a
`DebugInfoBuilder` + per-function `DISubprogram` + per-Span
`DILocation` line-table, finalized before `Module::verify` + the opt
pipeline. Phase L Debugger (ADR-0059) consumes the emitted DWARF v5
via standard `lldb` / `gdb` / VS Code DAP (bind-the-core, ADR-0012).

### Public surface added (wave-3)

```rust
// crates/cobrust-codegen/src/target.rs
pub struct TargetSpec {
    // ... existing fields ...
    /// Optional source-file path for DWARF emission (ADR-0058c §3.3).
    pub source_path: Option<PathBuf>,
}

// crates/cobrust-codegen/src/llvm_backend.rs (gated --features llvm)
// LlvmEmitter::new signature unchanged; DI scaffold built internally.
// Per-fn DISubprogram + per-Span DILocation emitted in declare_body +
// BodyLowerer::lower_block respectively. No new pub fns.
```

### DI basic-type mapping (ADR-0058c §3.2)

| Cobrust `Ty` | DWARF basic type | DW_ATE | Size |
|---|---|---|---|
| `Int` | `int64_t` | `DW_ATE_signed` (5) | 64 bits |
| `Float` / `Imag` | `double` | `DW_ATE_float` (4) | 64 bits |
| `Bool` | `bool` | `DW_ATE_boolean` (2) | 8 bits (storage) |
| `Str` / `Bytes` / `List` / `Dict` / `Set` / `Ref` / `None` / `Tuple` / etc. | opaque `ptr` | `DW_ATE_address` (1) | 64 bits |

A single shared cache (`di_basic_types: HashMap<&'static str, DIBasicType<'ctx>>`) dedups the four basic types per module.

### Source-path resolution (ADR-0058c §3.3)

- `TargetSpec.source_path = Some(path)` → `LineMap::from_source(read_to_string(path))`; DILocation lines + columns match the on-disk file.
- `TargetSpec.source_path = None` → `LineMap::empty()`; every span resolves to `(line=1, col=1)`. DI structure still validates per `llvm-dwarfdump-18` but breakpoint resolution collapses to "the first line" of the synthetic file.

### F34 symbol anchors (wave-3)

| Anchor | Role |
|---|---|
| `LlvmEmitter::new` | Constructs DIBuilder + DICompileUnit + DIFile + cached DI basic types per source (`llvm_backend.rs`, ADR-0058c §3.1) |
| `LlvmEmitter::populate_di_basic_types` | Four-DI-basic-type cache builder (Int / Float / Bool / Ptr) keyed by ADR-0058c §3.2 short tag |
| `BodyLowerer::set_debug_loc` | Per-Span DILocation setter — root of ADR-0058c §3.3 line-table emission |

### Wave-3 test surface (per ADR-0058c §3.4 + §3.5)

| Suite | Tests | Notes |
|---|---|---|
| llvm_backend inline DWARF smoke | 5 added | `dwarf_empty_module_emits_well_formed_object`, `dwarf_return_42_emits_subprogram`, `dwarf_multi_fn_module_emits_per_fn_subprograms`, `dwarf_drop_emitting_fn_still_validates`, `dwarf_o3_pipeline_preserves_dwarf` |
| llvm_backend inline LineMap | 2 added | `linemap_empty_returns_1_1`, `linemap_ascii_lines` |
| `tests/dwarf_lldb_smoke.rs` | 4 fixtures | `lldb_smoke_hello_world_subprogram_resolves`, `lldb_smoke_fib_function_visible`, `lldb_smoke_multi_fn_module_lists_both`, `lldb_smoke_line_table_present` — skip cleanly when neither `lldb-18` nor `lldb` is on `$PATH` |

### Non-goals (deferred per ADR-0058c §4)

- **Source-level variable inspection** (`DILocalVariable` / `DIFormalParameter` entries for `lldb frame variable`): Phase L UX scope; wave-3 ships per-fn + per-line baseline only.
- **macOS dSYM packaging**: `dsymutil` invocation handled in `release.yml`, not `llvm_backend`.
- **Inlined-frame chains** (`DILocation::inlined_at`): Phase-L+ if debugger demand surfaces it.
- **DWARF v4 fallback**: LLVM-18 emits v5 by default; older toolchains must regenerate.

## Phase K Strand #4 — JIT/AOT lowering convergence (ADR-0058d)

`crates/cobrust-codegen/src/lowering.rs` is the module-generic MIR→Cranelift IR lowering substrate extracted in ADR-0058d. It anchors a single source of truth for the wave-1 lowering shape consumed by both the AOT path (`cranelift_backend::CraneliftCtx::define_body`'s stateful dispatcher) and the JIT path (`cobrust-jit::lower`, which is a thin wrapper consumer).

### Public surface (stable for wave-1 per ADR-0058d §5.1)

| Symbol | Signature | Wave-1 shape |
|---|---|---|
| `lowering::lower_ty_wave1` | `(&Ty) -> Result<ir::Type, CodegenError>` | `Int → I64`, `Bool → I8`, `None → INVALID` |
| `lowering::body_signature_wave1` | `(&Body, CallConv) -> Result<Signature, CodegenError>` | params=`I64`s, return=`I64` |
| `lowering::lower_constant` | `(&mut FunctionBuilder, &Constant, BlockId) -> Result<ir::Value, CodegenError>` | `Int(n)` + `Bool(b)` lifted to I64 |
| `lowering::lower_place` | `(&mut FunctionBuilder, &HashMap<LocalId, Variable>, &Place, BlockId) -> Result<ir::Value, CodegenError>` | bare local read; no projections |
| `lowering::lower_operand` | `(&mut FunctionBuilder, &HashMap<LocalId, Variable>, &Operand, BlockId) -> Result<ir::Value, CodegenError>` | `Copy`/`Move`/`Constant` dispatch |
| `lowering::lower_rvalue_wave1` | `(&mut FunctionBuilder, &HashMap<LocalId, Variable>, &Rvalue, BlockId) -> Result<ir::Value, CodegenError>` | `Use` + `BinaryOp::{Add,Sub,Mul}` + `UnaryOp::{Neg,Plus}` |
| `lowering::lower_statement_wave1` | `(&mut FunctionBuilder, &HashMap<LocalId, Variable>, &Statement, BlockId) -> Result<(), CodegenError>` | `Assign(Place::local, _)` + Storage{Live,Dead}/Nop |
| `lowering::lower_terminator_wave1` | `(&mut FunctionBuilder, &HashMap<LocalId, Variable>, &HashMap<BlockId, ir::Block>, &Terminator, BlockId, LocalId) -> Result<(), CodegenError>` | `Return` + `Goto` + `Unreachable` |
| `lowering::lower_body_wave1` | `(&Body, CallConv) -> Result<ir::Function, CodegenError>` | full Body → `ir::Function` |

Non-wave-1 MIR shapes return `CodegenError::InvalidMir` with a `"wave1:"` prefix; JIT callers narrow this to `JitError::UnsupportedMirFeature` / `UnsupportedType` via `From<CodegenError> for JitError` in `cobrust-jit::lower`.

### Stability contract

**Wave-1 surface is stable-for-wave-1** (ADR-0058d §5.1):
- Signature changes require a sub-ADR.
- Adding helpers (`lower_constant_float`, `lower_call_wave2`, etc.) is **non-breaking**.
- Removing or changing the wave-1 signature is **breaking**.

### What is NOT in the substrate (ADR-0058d §2.3 non-goals)

- AOT-specific surface stays in `cranelift_backend::CraneliftCtx::define_body`: runtime helpers, extern symbol declaration, drop schedules, dict/list/str intrinsics, `Place` projections, `Constant::FnRef` call lowering, str data symbols, `infer_local_types` chained inference.
- `cranelift_backend.rs` is unchanged by ADR-0058d. AOT-side delegation through the wave-1 helpers is reserved for a future ADR (hypothetical 0058e or 0056d) — see ADR-0058d §2.3 deferral rationale.

### Unit tests (wave-1 substrate)

`crates/cobrust-codegen/src/lowering.rs::tests`:
- `lower_body_wave1_int_add_round_trip` — `1 + 2` through full Body lowering.
- `lower_constant_str_rejected` — `Constant::Str(_)` returns `InvalidMir("wave1: ...")`.
