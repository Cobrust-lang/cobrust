---
doc_kind: adr
adr_id: 0056a
parent_adr: 0056
title: "Phase I day 1-2 — Cranelift JIT crate wire (`CodegenMode { Aot, Jit }` switch + minimal arithmetic round-trip)"
status: accepted
date: 2026-05-18
last_verified_commit: 710fadd
supersedes: []
superseded_by: []
relates_to: [adr:0056, adr:0054, adr:0034]
discovered_by: P9 — ADR-0056 §4 sub-ADR roster, day 1-2 slot
ratification_path: P9 ADR review; ratifies on impl-merge gate (also triggers ADR-0056 frame ratify per parent §"ratification_path")
---

# ADR-0056a: Cranelift JIT crate wire — `cranelift-jit = "0.131"` add + `CodegenMode { Aot, Jit }` switch

## 1. Context

ADR-0056 §3 calls for a `CodegenMode { Aot, Jit }` switch at the
codegen entry, both modes sharing the `Function`-level MIR-lowering
loop. Today `cobrust-codegen/src/cranelift_backend.rs` (HEAD
`1fbed82` lines 39-86) instantiates `ObjectModule` unconditionally
and writes `.o`. No JIT path.

ADR-0056 §4 assigns this ADR the day 1-2 slot: wire
`cranelift-jit = "0.131"`, add `CodegenMode`, demonstrate arithmetic
round-trip (`1 + 2 * 3` → `i64`). Siblings 0056b (control-flow +
stdlib) and 0056c (session state machine) consume it.

Pre-dispatch gate per ADR-0056 §7: `cranelift-jit = "0.131"` adds
clean to the `cranelift-{codegen,frontend,module,object} = "0.131"`
quartet at `Cargo.toml` lines 28-31. 30-min cargo POC on DG verified
no version-skew.

## 2. §2.5 citation

ADR-0054 §2 ranks Phase I §2.5 ROI **medium** — L1 pipeline speedup,
no new LLM-surface contract. 0056a delivers no §2.5 user-visible
surface; JIT switch is internal. §2.5 ROI accrues at 0056b (AOT-
delegation removal) + 0056c (Phase J ctx).

## 3. Decision

Three coordinated changes to `crates/cobrust-codegen`:

### 3.1 Dependency add

Add to `crates/cobrust-codegen/Cargo.toml` `[dependencies]`:

```toml
cranelift-jit = "0.131"
```

Pinned to the quartet minor. Cold-build delta <30s per ADR-0056 §7.

### 3.2 `CodegenMode` enum + entry switch

In `cobrust-codegen/src/lib.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodegenMode { Aot, Jit }
```

`Aot` = `ObjectModule` (ADR-0023, unchanged); `Jit` = `JITModule`
(finalized fn pointer via `get_finalized_function`, no `.o`).
`cranelift_backend::emit` splits: `emit(...)` → AOT (compat-
preserved, `Aot` default); `emit_with_mode(..., mode)` → dispatched.

Helper `lower_module<M: ClifModule>(module, &mut clif_mod, …)`
factors out the MIR→Cranelift lowering loop (current lines 64-72:
declare-then-define). Both modes call it with their `ClifModule`
impl.

### 3.3 JIT module construction + finalize

New `crates/cobrust-codegen/src/cranelift_jit.rs`:

```rust
pub struct JitArtifact {
    pub module: JITModule,
    pub entry: HashMap<String, FuncId>,
    pub finalized: bool,
}

pub fn emit_jit(module: &Module, spec: &TargetSpec) -> Result<JitArtifact, CodegenError> {
    let isa = build_isa(spec)?;
    let jit_builder = JITBuilder::with_isa(isa, default_libcall_names());
    let mut jit_module = JITModule::new(jit_builder);
    // … reuses lower_module helper …
    jit_module.finalize_definitions()?;
    Ok(JitArtifact { module: jit_module, entry, finalized: true })
}
```

`JitArtifact::module` is returned by-value to the REPL Session caller
(ADR-0056c). `get_finalized_function(id)` is **not** called inside
`emit_jit` — that's the caller's job once it knows the exact
`extern "C"` fn sig to transmute against.

## 4. `JITModule` lifetime

**Owned by the REPL Session, not per-eval.** Per ADR-0056 §3 the
synthetic `fn __repl_eval_NNNN()` is added incrementally to the SAME
`JITModule` instance across REPL turns:

- Session-init: lazy `JITModule::new` on first non-introspection
  statement (preserves ADR-0029 <200ms cold-start budget).
- Per-turn: `module.define_function(id, sig, body)` then
  `module.finalize_definitions()` once per turn. New fns add to the
  same module; module **survives** across REPL turns.
- Session-exit: `JITModule` is dropped (frees RWX `memmap2` pages).

`get_finalized_function(id) -> *const u8` returns a raw pointer.
**Safety contract** (load-bearing): only safe to `transmute` to a fn
signature that EXACTLY matches the MIR-lowered Cranelift `Signature`
for that FuncId. Signature mismatch = SIGSEGV (parent §5 risk 1, the
top Phase I risk). This ADR pins the validation surface:

- One `extern "C"` fn per primitive return type. 4-arm table:
  `() -> i64` / `() -> f64` / `() -> *const u8` / `() -> ()`.
- Any return type outside the 4-arm table → fall back to AOT one-
  shot path (ADR-0029 §"Negative") for that REPL turn.

## 5. Mode dispatch + signature contract

`Codegen::new(mode: CodegenMode)` constructor exposed at
`cobrust-codegen/src/lib.rs`. The `lower_module<M: ClifModule>`
helper is mode-agnostic; only the finalize step differs (JIT:
`finalize_definitions`; AOT: `finish().emit()`).

The REPL Session owns a `(FuncId, Signature)` map (full shape in
ADR-0056c) for two reasons:

1. **Pre-transmute signature assertion.** Before
   `get_finalized_function(id)` + transmute, the session asserts the
   Cranelift `Signature` matches the chosen `extern "C"` fn sig.
   Mismatch = early type-error diagnostic, not SIGSEGV.
2. **Redefinition invalidation.** When a user redefines a fn,
   `module.declare_function` returns a **new** FuncId. Any pointer
   from `get_finalized_function` for the old FuncId is **stale**;
   in-flight calls still see the old body (parent §5 risk 3). This
   ADR pins the contract; ADR-0056c implements the diagnostic
   (reject re-def of currently-on-stack fns).

## 6. No new MIR opcodes

JIT mode is **purely a codegen-output backend switch**. MIR is
identical to the AOT path:

- M11.2 FnRef::Call (ADR-0034 §"Decision Option 3" forward-decl)
  re-binds against `JITModule` without form-level rewrites — both
  modules implement `cranelift_module::Module`.
- M11.3 `lower_condition` shared root (ADR-0035) — unchanged.
- ADR-0050d dict / mapping lowering — unchanged.
- All intrinsic / PRELUDE rewrites (ADR-0027 + ADR-0050a-f) —
  unchanged surface.

If a MIR form lowers correctly under `ObjectModule` today, it lowers
identically under `JITModule` post-0056a. The
`lower_module<M: ClifModule>` extraction makes this formal: the
bound is `cranelift_module::Module`, both impls satisfy it.

## 7. Sub-ADR roster

Single ADR. No further sub-sprints under 0056a; day 3-5 lands as
sibling ADR-0056b (control-flow + stdlib), day 6 as ADR-0056c
(session state machine).

## 8. Risk register

Top 3 (parent §5 narrowed to wire stage):

1. **Cranelift JIT API drift between minor versions.** `cranelift-jit`
   tracks the family's `0.131`-minor cadence; API breakage between
   `0.131` and `0.132` is the documented cranelift CHANGELOG cadence.
   **Mitigation:** pin `0.131.x`. Bump only via a dedicated ADR with
   full re-validation.

2. **`get_finalized_function` raw-pointer transmute = SIGSEGV on
   signature mismatch.** Parent §5 risk 1, wire-stage narrowed.
   **Mitigation:** Session pre-transmute assertion against the 4-arm
   `extern "C"` table (§5); fall-back to AOT one-shot for any non-
   4-arm return type. ABI validation tests land with impl.

3. **`JITModule` memory growth across REPL turns.** Each
   `__repl_eval_NNNN` allocates fresh JIT pages; no GC; module-drop
   is the sole reclaim. **Mitigation:** opt-in `--repl-max-mem <MiB>`
   deferred to a post-Phase-I ADR. Not blocking — 50+20-session M14.1
   corpus is ~70MiB worst-case (~1MiB × ~70 fns).

## 9. Phase J handoff

`cranelift-jit` produces **no debug-info** today.
`JITModule::finalize_definitions` does not emit DWARF; DWARF/CodeView
is a Phase K LLVM deliverable per ADR-0054 §6.

This is fine for Phase J: LSP `textDocument/hover` + `completion`
consume the **incremental `TypeCheckCtx`** that ADR-0056c ships
(parent §6). LSP never consumes codegen debug-info; source-position-
to-type lives in type-check ctx, not in any backend artifact. Phase
K LLVM ships DWARF for native-binary debugger flow; JIT-mode debug-
info stays out-of-scope until then.

## 10. Pre-dispatch acceptance gate

Per ADR-0056 §7 (HEAD `1fbed82` lines 192-210):

- Phase G closed at v0.3.0 (`8b4366c`) ✓
- M11.2 FnRef::Call verified (ADR-0034 `ea15683`; `examples/fib.cb`
  recursive green) ✓
- `cranelift-jit = "0.131"` 30-min cargo POC on DG green; no
  version-skew at `1fbed82` ✓
- `cobrust check` + `build` + `repl` smoke on M14 50-session
  corpus 🟢 ✓

Parent ADR-0056 is `proposed`; per its `ratification_path` this
ADR's impl-merge triggers the frame ratify.

## 11. Consequences

### 11.1 Positive

- Day 1-2 of Phase I wall consumed; 0056b unblocks on merge.
- `CodegenMode` is internal — no public surface change. AOT path
  bit-identical to pre-0056a behaviour.
- `lower_module<M: ClifModule>` extraction is a healthy refactor —
  future `WasmModule` plug-ins become trivial.

### 11.2 Negative

- `cranelift-jit` adds ~150KB to release `cobrust repl` binary
  (parent §8.2). ADR-0029 <200ms cold-start held via lazy init.
- ABI validation test surface non-trivial (4-arm fn-sig table +
  redefinition-invalidation corpus). Lands with impl.

### 11.3 Neutral

- `JITModule` lifetime is REPL-Session-owned (§4); no `unsafe` leaks
  to callers — `JitArtifact` owns the module by-value.
- No new MIR opcodes (§6); reuse-only on M11.2 / M11.3 / 0050d.

## 12. Dispatch readiness

TEST 4h, DEV 8h, wall ~2 days. Matches ADR-0056 §9 row 2.

— P9 Tech Lead, 2026-05-18

## 13. Impl-time amendment — separate `cobrust-jit` crate vs. in-place
##     `cobrust-codegen/src/cranelift_jit.rs` (2026-05-18)

The pre-dispatch design (§3.3) placed `emit_jit` + `JitArtifact`
inside `cobrust-codegen` as a sibling to
`cranelift_backend::emit`. The P10 dispatcher for wave-1 instead
specified a NEW workspace crate `cobrust-jit` with public surface
`JitEngine` / `JitHandle` / `JitError`. Rationale (per P10
dispatcher):

- **PIC divergence (load-bearing).** Discovered at first DG test
  run: cranelift-jit asserts `is_pic=false` (panics at
  `cranelift-jit-0.131.1/src/backend.rs:353`). AOT (cranelift-
  object) requires `is_pic=true` for ELF/Mach-O. Two ISA flag
  sets that **cannot coexist** in one `build_isa` helper without
  a mode parameter; cleaner factored across crates.
- **REPL Session ergonomics.** ADR-0056c needs to own a long-
  lived `JitEngine` across REPL turns. Embedding inside the
  CodegenError taxonomy adds a leaky surface to AOT callers who
  don't want the JIT failure modes (NoSuchFunction,
  SignatureMismatch).
- **Wave-1 vs wave-2 boundary.** ADR-0056b plans to extract a
  `lower_module<M: ClifModule>` helper shared between
  cobrust-codegen + cobrust-jit (per parent §3.2). Having two
  crates makes the shared-helper extraction a true sibling-
  refactor, not an internal module split.

Concretely shipped (impl-time):

- New crate `crates/cobrust-jit/` registered in workspace `[members]`.
- `JitEngine::new()` — host ISA via `cranelift-native::builder`,
  `is_pic=false`, `opt_level=none`.
- `JitEngine::compile_mir(self, &Module) -> Result<JitHandle>` —
  consumes engine; two-pass declare/define against `JITModule`;
  one `finalize_definitions` call.
- `JitHandle::call::<R, A: ArgsList>(name, args) -> Result<R>` —
  unsafe (one of two unsafe surfaces in the project alongside
  cobrust-llm-router's HTTP client) with pre-transmute signature
  validation against the 4-arm extern table at §4.
- `JitHandle::function_names()` / `signature(name)` —
  introspection for the REPL Session caller.
- `JitError` — 9-variant taxonomy including
  `UnsupportedMirFeature` for the AOT-fallback signal (parent §4
  4-arm table).

§3.2's `CodegenMode { Aot, Jit }` enum is **deferred** to ADR-
0056b's `lower_module<M: ClifModule>` extraction sprint —
wave-1's standalone `JitEngine` is the cleaner first step.

§3.1's `cranelift-jit = "0.131"` dependency ships clean (the
quartet pinning held; no version-skew).

Wave-1 lowering surface (per `crates/cobrust-jit/src/lower.rs`):
`Constant::Int`, `BinOp::{Add,Sub,Mul}`, `UnOp::{Neg,Plus}`,
`Place::local` (no projections), `Terminator::{Return,Goto,
Unreachable}`. ADR-0056b grows to the AOT-parity surface.

DG verify at HEAD `710fadd` — `cargo test -p cobrust-jit`:
1 unit + 11 integration tests PASS, POSTFLIGHT clean.

The §3.2 enum + lower_module helper still ship — ADR-0056b is
the binding sub-ADR for them. This amendment narrows wave-1 to
the standalone crate path; no semantic deviation from the parent
ADR-0056 frame (the `Module` trait gating is still the
abstraction, just sourced from `cranelift-module` directly
rather than through a Cobrust-side enum).

**Cross-arch note (Tier-1 audit A1, 2026-05-18)**: JIT-compiled code from `cobrust-jit::JitEngine` is host-ISA-bound. A `JitHandle` produced on Mac aarch64 (via `cranelift_native::builder()` auto-detect) cannot be shipped to DG x86_64 for execution. Practical implication: `cargo test -p cobrust-jit` on Mac aarch64 is a valid CI gate for JIT correctness on macOS; DG x86_64 must re-run the same tests natively to validate Linux-host JIT path. Both arches are required for the green signal. AOT artifacts (cobrust-codegen) remain cross-arch portable per ADR-0023; JIT is host-only by Cranelift contract.

**Follow-on amendments deferred to 0056b sprint** (Tier-1 audit A2 + A3):
- **A2**: Remove blanket `#![allow(clippy::must_use_candidate)]` in `crates/cobrust-jit/src/lib.rs` and apply explicit `#[must_use]` to `JitEngine::compile_mir` and `JitHandle::call` return types per §5.1 engineering standard.
- **A3**: Add 2 rejection-path tests for `Rvalue::Aggregate` + `Rvalue::Cast` (each 2-line MIR body, expect `JitError::UnsupportedMirFeature`). Prevents silent regression when 0056b extends lowering.
