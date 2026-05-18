---
doc_kind: adr
adr_id: 0058a
parent_adr: 0058
title: "Phase K wave-1 — MIR → LLVM IR lowering core (parallel to Cranelift backend)"
status: proposed
date: 2026-05-18
last_verified_commit: 54a599c
supersedes: []
superseded_by: []
relates_to: [adr:0058, adr:0023, adr:0046]
discovered_by: P10 Phase K wave-1 first sub-sprint per ADR-0058 §"Sub-ADR roster"
ratification_path: P9 sub-ADR review; ratifies on TEST+DEV PAIR landing the LLVM `emit` entry path + diff-gate parity
---

# ADR-0058a: Phase K wave-1 — MIR → LLVM IR lowering core

## 1. Context

ADR-0058 (Phase K frame, proposed `2a710d3`) un-defers the LLVM backend column
of ADR-0023 §"Per-MIR-form lowering rules". The frame enumerates three
sub-sprints — **0058a (this ADR)**, 0058b (optimization + multi-target), 0058c
(DWARF emission) — pinned **sequential**: optimization (0058b) operates on
emitted IR; DWARF intrinsics (0058c) interleave with `build_store` /
`build_call` in the same `Builder` cursor 0058a constructs.

ADR-0058a is the **first sub-sprint** under Phase K. It implements MIR → LLVM
IR lowering parallel to MIR → Cranelift CLIF (`cranelift_backend.rs`),
mirroring that file's structure but emitting via `inkwell`. **No optimization
pipeline. No DWARF emission. Wave-1 ships the core IR-construction pass only.**

The `Backend::Llvm` arm in `crates/cobrust-codegen/src/lib.rs` currently
errors `UnsupportedBackend` (stub at M9). ADR-0058a is the un-stubbing.

Constitutional anchors: CLAUDE.md §4.1 (pipeline `Codegen (LLVM / Cranelift)`);
§5.1 (elegant — `LlvmLowerCtx`-owned inkwell objects avoid raw `LLVMValueRef`
exposure); §6 (atomic commits — lowering + tests + sub-ADR + doc-coverage
ship together).

## 2. §2.5 LLM-first design — neutral

Per ADR-0058 §"§2.5 ROI position", Phase K is §2.5-neutral; ADR-0058a inherits
that neutrality. LLVM IR lowering is **product perf**, not LLM-amplifier:
the LLM does not write Cobrust source differently because of LLVM. Lowering
affects binary size and runtime perf, neither §2.5 §A (compile-time-catch)
nor §2.5 §B (training-data overlap). ADR-0058a introduces no new
`TypeError::*` variants — it consumes the typechecked `cobrust_mir::Module`
and emits IR. §2.5 audit: must not regress error UX.

## 3. Decision

Implement `crates/cobrust-codegen/src/llvm_backend.rs` mirroring
`cranelift_backend.rs` structure. The module is gated behind
`#[cfg(feature = "llvm")]` per the existing ADR-0023 feature-flag binding.

### 3.1 Public entry path (binding)

```rust
#[cfg(feature = "llvm")]
pub fn emit(module: &Module, spec: &TargetSpec) -> Result<Artifact, CodegenError> {
    let ctx = inkwell::context::Context::create();
    let llvm_module = ctx.create_module(&spec.module_name);
    let builder = ctx.create_builder();
    let mut lower_ctx = LlvmLowerCtx::new(&ctx, &llvm_module, &builder, spec)?;
    for body in &module.bodies { lower_ctx.declare_body(body)?; }
    for body in &module.bodies { lower_ctx.define_body(body)?; }
    lower_ctx.finalize_and_write_object()
}
```

`emit` is the **only public surface** added. The dispatch arm in
`crates/cobrust-codegen/src/lib.rs` switches from `UnsupportedBackend` to
`llvm_backend::emit(module, spec)` under `feature = "llvm"`.

### 3.2 Internal structure (binding)

| Type | Role |
|---|---|
| `LlvmLowerCtx<'ctx>` | Per-emit state (parallel to `CraneliftCtx`); borrows `&Context` / `&Module` / `&Builder` |
| `BodyLowerer<'ctx, 'l>` | Per-`Body` lowerer; borrows `LlvmLowerCtx` mutably |
| `function_ids: HashMap<DefId, FunctionValue<'ctx>>` | Body → LLVM function handle |
| `body_return_types: HashMap<DefId, Ty>` | Per-body return type cache |
| `runtime_helper_decls: HashMap<&'static str, FunctionValue<'ctx>>` | Declared runtime helpers (`__cobrust_drop_*`, etc.) |

`'ctx` is `inkwell::context::Context`'s arena lifetime. All `Type<'ctx>` /
`BasicValueEnum<'ctx>` / `FunctionValue<'ctx>` borrow from the same context
— no manual `drop_in_place`, no raw `LLVMValueRef`.

## 4. MIR → LLVM type mapping (binding)

The wave-1 lowering table covers M9 "core 30" forms. The LLVM column extends
ADR-0023 §"Per-MIR-form lowering rules" with concrete inkwell calls.

### 4.1 Scalar + reference types

| MIR `Ty` | LLVM type | inkwell construction |
|---|---|---|
| `Ty::Bool` | `i1` | `ctx.bool_type()` |
| `Ty::Int (i64)` | `i64` | `ctx.i64_type()` |
| `Ty::Float (f64)` | `double` | `ctx.f64_type()` |
| `Ty::Str (*mut u8)` | `i8*` (opaque ptr LLVM 15+) | `ctx.ptr_type(AddressSpace::default())` |
| `Ty::List[T]` | opaque `i8*` (heap-managed) | `ctx.ptr_type(...)` — element ty stays MIR-level |
| `Ty::Dict[K, V]` | opaque `i8*` (heap-managed) | `ctx.ptr_type(...)` |
| `Ty::Ref(T)` | same LLVM repr as `T` | transparent at LLVM level; lifetimes MIR-level (ADR-0020 B1..B5) |

Opaque pointers (LLVM 15+ default) mean `List[Int]` and `Dict[Str, Int]` both
lower to `i8*`. Element type is recovered via the MIR `Ty` on each `Place` /
`Operand`, not from the LLVM type.

### 4.2 Aggregate + function-shaped types

| MIR construct | LLVM lowering | inkwell call |
|---|---|---|
| `MirFunc` (a `Body`) | `LLVMFunction` (`FunctionValue<'ctx>`) | `module.add_function(name, fn_type, Some(External))` |
| `BasicBlock` | `LLVMBasicBlock` | `ctx.append_basic_block(fn_value, &label)` |
| `LocalDecl` | stack `alloca` + load/store | `builder.build_alloca(ty, &local_name)` |
| `Tuple(T1, T2, ...)` | `LLVMStructType` | `ctx.struct_type(&[t1, t2, ...], false)` |
| `Aggregate::List(elements)` | runtime helper → `i8*` | `builder.build_call(__cobrust_list_new, &args, "list_new")` |
| `Aggregate::Dict(pairs)` | runtime helper → `i8*` | same indirect pattern |

`Ty::List` / `Ty::Dict` aggregates do **not** map to LLVM aggregates; they
lower to runtime-helper calls returning opaque pointers. Helpers
(`__cobrust_list_new`, `__cobrust_list_push`, etc.) are declared as
`extern "C"` in the LLVM module; bodies live in `crates/cobrust-stdlib-rt/`
(M11).

## 5. Operand lowering (binding)

`Operand::Copy(Place)`, `Operand::Move(Place)`, `Operand::Constant(c)` lower
directly to LLVM `load` / `load` / `LLVMConstInt`-family calls. Drop
information stays MIR-level; LLVM does **not** model drop semantics.

### 5.1 Per-operand mapping

| MIR `Operand` | LLVM lowering | inkwell call |
|---|---|---|
| `Operand::Copy(place)` | load from `alloca`/GEP | `builder.build_load(ty, place_ptr, "copy")` |
| `Operand::Move(place)` | load from `alloca`/GEP | `builder.build_load(ty, place_ptr, "move")` (same as Copy at LLVM level) |
| `Operand::Constant(Int(i))` | `i64` constant | `ctx.i64_type().const_int(i as u64, true)` |
| `Operand::Constant(Float(bits))` | `double` constant | `ctx.f64_type().const_float_from_bits(bits)` |
| `Operand::Constant(Bool(b))` | `i1` constant | `ctx.bool_type().const_int(b as u64, false)` |
| `Operand::Constant(Str(s))` | global `i8*` ptr | `module.add_global(...)` + `builder.build_pointer_cast` |

`Copy` and `Move` produce **identical LLVM IR**. Move semantics (ownership
transfer) are enforced at MIR-time per ADR-0020 §"B1..B5 borrow obligations";
LLVM sees only a load. Drop schedules (ADR-0020 §"Drop schedule") are
pre-computed at MIR; LLVM emits the corresponding `Drop` terminator call (§6).
No LLVM-side reanalysis.

### 5.2 `Place` projection lowering

`Place { local, projections }` lowers to a GEP chain rooted at the local's
`alloca`. `Projection::Field(idx)` → `build_struct_gep`;
`Projection::Index(operand)` → `build_gep` with operand as offset;
`Projection::Deref` → load pointer-typed local then GEP into loaded pointer.

## 6. Terminator lowering (binding)

| MIR `Terminator` | LLVM lowering | inkwell call |
|---|---|---|
| `Goto(b)` | unconditional branch | `builder.build_unconditional_branch(target_block)` |
| `Return(operand)` | return value | `builder.build_return(Some(&operand_value))` |
| `SwitchInt { discr, targets, default }` | switch instr | `builder.build_switch(discr_value, default_block, &case_pairs)` |
| `Call { fn, args, dest, target }` | call instr + branch | `builder.build_call(...)` then `build_unconditional_branch(target)` |
| `Drop { place, target }` | runtime helper call + branch | `builder.build_call(__cobrust_drop_<TypeId>, &[place_ptr], "")` then branch |
| `Unreachable` | unreachable instr | `builder.build_unreachable()` |
| `Assert { cond, msg, target }` | conditional jump + panic call | `build_conditional_branch` + `build_call(__cobrust_panic, ...)` |

`Drop` lowers to runtime-helper calls (`__cobrust_str_drop`,
`__cobrust_list_drop`, etc.) — same ABI as Cranelift per ADR-0023
§"Drop-handler ABI". Wave-1 helpers are **no-op stubs** (M11 materializes).

## 7. Calling convention (binding)

C ABI (`ccc`, inkwell's `CallConv::C`) for runtime helpers (`__cobrust_*`)
and Cobrust-internal calls. Phase K wave-1 does **not** introduce a custom
LLVM-level convention — `extern "Cobrust"` per ADR-0023 matches platform C
ABI (System V AMD64 on Linux x86_64; AAPCS64 on macOS arm64 + Linux arm64),
which `CCallConv` already targets. inkwell exposes this via
`FunctionType::fn_type(...)` with default `CallConv::C`. Sub-ADR 0058b may
revisit if optimization motivates a custom convention.

## 8. Sub-ADR boundary — wave-1 IS lowering ONLY

What ADR-0058a **ships**:

- `llvm_backend::emit` entry path with `LlvmLowerCtx` + `BodyLowerer`.
- MIR → LLVM IR construction for every M9 "core 30" form.
- Object-file emission via `TargetMachine::write_to_file` direct path.
- Differential gate parity with Cranelift on the "core 30" diff corpus
  (`crates/cobrust-codegen/tests/codegen_diff_corpus.rs` extended).

What ADR-0058a does **not** ship — explicit non-goals:

- Optimization pass pipeline (`OptLevel::Speed` / `OptLevel::SpeedAndSize`):
  **sub-ADR 0058b**.
- DWARF debug-info emission (`DIBuilder`, `dbg.declare` / `dbg.value`):
  **sub-ADR 0058c**.
- Multi-target cross-compilation matrix (release.yml `cross` for Cranelift
  is ADR-0046; LLVM cross-target is sub-ADR 0058b).
- Binary-size acceptance bar (ADR-0023 §"LLVM `-O3` ≥ 30% smaller binary"):
  closes at sub-ADR 0058b under `OptLevel::SpeedAndSize`.

Wave-1's acceptance gate is **functional parity**, not optimization parity.

## 9. Risk register — 3 concrete risks

### 9.1 inkwell version pin vs LLVM system library mismatch

- **Risk**: current `Cargo.toml` HEAD pins `inkwell = "0.9"` (latest stable);
  ADR-0058 §4 keeps 0.9 and **adds** `features = ["llvm18-1"]` to activate
  LLVM 18. If lockfile pin and system LLVM diverge (LLVM 17 apt vs LLVM 18
  brew), build fails with confusing linker errors.
- **Mitigation**: 0058a's first commit modifies `crates/cobrust-codegen/Cargo.toml`
  line 33 to ADD `features = ["llvm18-1"]` to the existing
  `inkwell = { version = "0.9", optional = true }` line (NOT downgrade to 0.5);
  CI matrix in `release.yml` verifies LLVM 17 + 18 explicitly on tier-1
  hosts. `Cargo.lock` pins exact inkwell revision; sub-ADR captures pin
  commit in §"Evidence" at acceptance.

  > **Correction 2026-05-18 per audit `a8155e81cb212aaca` F1**: inkwell 0.9 IS
  > the latest stable on crates.io; `llvm18-1` feature only exists on inkwell
  > ≥ 0.6. A downgrade to 0.5 would fail cargo immediately. Phase K keeps 0.9
  > + enables `llvm18-1` feature.

### 9.2 LLVM IR verifier rejects mid-development output

- **Risk**: `LLVMVerifyModule` is strict — malformed GEP chains, mismatched
  call signatures, terminator-less blocks all reject. Verifier cascade can
  mask the actual lowering bug.
- **Mitigation**: gate `module.verify()` under `#[cfg(debug_assertions)]`.
  On failure, print offending IR via `module.print_to_stderr()` before
  panicking. Release mode skips verification. Dev-mode verifier becomes
  the primary feedback signal during diff-gate corpus expansion.

### 9.3 Memory leaks via inkwell's lifetime management

- **Risk**: inkwell wraps `LLVMContextRef` in Rust types parameterized by
  `'ctx`. Dangling references arise if `Context` drops before borrowed
  values, or `Module` outlives `Context`. Compiles but leaks (or
  double-frees) at drop.
- **Mitigation**: `LlvmLowerCtx<'ctx>` owns inkwell `Context` via
  single-arena — created at `emit` entry, dropped at return. All `'ctx`
  borrowers (Module / Builder / FunctionValue) share that arena; `'ctx`
  enforces drop ordering at compile time. Wave-1 acceptance includes Miri
  smoke pass over `emit` on `fib_50.cb` + `dotproduct_1k.cb` fixtures.

## 10. Pre-dispatch acceptance gate

ADR-0058a dispatches only when **all four** conditions hold:

- **Parent ADR-0058 accepted**: Phase K frame ratified (0058a is the first
  ratifier per §"ratification_path"). Frame + 0058a can land in one merge
  if frame is uncontested.
- **`inkwell` dep updated**: current HEAD `inkwell = "0.9"` stays at 0.9;
  first commit of the 0058a branch adds `features = ["llvm18-1"]` to
  `inkwell = { version = "0.9", optional = true }` per ADR-0058 §4.
- **LLVM toolchain installed**: Mac — `brew install llvm@18`;
  `LLVM_SYS_180_PREFIX=$(brew --prefix llvm@18)` exported.
  DG-Workstation — `apt install llvm-18-dev libpolly-18-dev`; ssh
  preflight verifies `llvm-config-18 --version` returns 18.x.
- **Cranelift baseline diff-gate green**: `cargo test -p cobrust-codegen
  --test codegen_diff_corpus` passes on M9 "core 30" forms at dispatch
  HEAD. LLVM acceptance is identical-stdout parity with Cranelift, so
  baseline must be green.

## 11. Consequences / Dispatch readiness

### 11.1 Positive

- Unblocks sub-ADR 0058b (optimization + multi-target) which depends on
  emitted IR existing.
- Unblocks sub-ADR 0058c (DWARF emission) which depends on the `Builder`
  cursor + `Span`-keyed lowering pass 0058a constructs.
- Activates ADR-0023's forward-compat LLVM column — un-stubs `Backend::Llvm`.

### 11.2 Negative

- ~1 week wall agent-velocity; biggest single sub-ADR in Phase K (~25h DEV
  + ~10h TEST per ADR-0058 §"Dispatch readiness").
- Maintains two backend lowering paths (ADR-0023 §"Consequences" accepted;
  not re-litigated here).
- inkwell pin (§9.1) becomes lockfile-tracked; LLVM 19 / inkwell 0.6 upgrade
  is a follow-up sub-ADR.

### 11.3 Dispatch composition

- **TEST opus (rare)**: LLVM IR snapshot tests need careful golden-file
  management; snapshot format canonicalizes against inkwell IR-print output
  ordering (varies across LLVM versions). TEST agent produces the
  diff-corpus parity harness + golden-file canonicalization scaffold.
  ~10h budget.
- **DEV opus (multi-week wall)**: 0058a is the biggest single sub-ADR in
  Phase K. DEV agent implements `LlvmLowerCtx` + `BodyLowerer` + all
  per-form lowerings + runtime-helper declarations. ~25h budget.

Total wall: **~1 week** per ADR-0058 §"Dispatch readiness" sub-ADR row.
Buffer +2 days if §9.1 inkwell version mismatch surfaces on DG-Workstation
apt LLVM-18 path.

## 12. Why this ADR now

ADR-0058 (Phase K frame) authored 2026-05-18 in the user's "fire all
post-Phase-G frames concurrently" batch; 0058a is wave-1 under that frame.

Per ADR-0054 §9.2, Phase K is impl-independent of Phase H + I — touches only
`crates/cobrust-codegen/`, no overlap with `crates/cobrust-types-cb/` (Phase
H) or `crates/cobrust-cli/src/repl.rs` (Phase I). Dispatches in parallel
with Phase H + I or after Phase I closes.

Authoring 0058a ex-ante codifies the **lowering core boundary** before
optimization or DWARF concerns accrete. The §8 non-goals list is the
audit-trail protection against scope creep at later sub-ADR authoring.

## 13. Evidence

- ADR-0058 (Phase K frame, proposed `2a710d3`) — §"Sub-ADR roster" enumerates
  0058a as MIR → LLVM IR core (~1 week wall); §"Wave plan" pins 0058a →
  0058b → 0058c sequential.
- ADR-0023 (M9 codegen, accepted `ec680bc`) — §"Per-MIR-form lowering rules"
  LLVM column; §"Backend feature flag layout" `--features llvm`;
  §"Calling convention details" System V AMD64 + AAPCS64.
- ADR-0046 (release.yml tier-1 contract, accepted `03c70f2`) — §"Tier-1
  platform contract" 3-target list that Phase K promotes to LLVM-backed
  delivery (0058b closes; 0058a unblocks).
- `crates/cobrust-codegen/src/cranelift_backend.rs`::`emit` (HEAD
  `54a599c`) — Cranelift entry symbol (`pub fn emit(module: &Module, spec: &TargetSpec)`); `llvm_backend.rs` mirrors structure.
- `crates/cobrust-codegen/Cargo.toml` (HEAD `54a599c`) — current
  `inkwell = "0.9"` stable dep; 0058a adds `features = ["llvm18-1"]`.
- inkwell crate docs — <https://docs.rs/inkwell/0.9.x>; LLVM 18 via
  `llvm18-1` feature.
- CLAUDE.md §2.5 (HEAD `54a599c`) — LLM-first principle; 0058a §2.5-neutral.
- CLAUDE.md §4.1 — pipeline `Codegen (LLVM / Cranelift)` anchor.

— P9 Tech Lead, 2026-05-18
