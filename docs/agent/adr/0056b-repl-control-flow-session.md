---
doc_kind: adr
adr_id: 0056b
parent_adr: 0056
title: "Phase I day 3-5 — Control-flow + stdlib lowering reuse + `Session` struct + incremental `TypeCheckCtx`"
status: accepted
date: 2026-05-18
last_verified_commit: b0e1e9e
supersedes: []
superseded_by: []
relates_to: [adr:0056, adr:0056a, adr:0029, adr:0034, adr:0057, adr:0057a]
discovered_by: P9 — ADR-0056 §4 sub-ADR roster, day 3-5 slot
ratification_path: P9 ADR review; ratifies on impl-merge gate
---

# ADR-0056b: Control-flow + stdlib lowering reuse + `Session` struct + incremental `TypeCheckCtx`

## 1. Context

Wave-2 of Phase I per ADR-0056 §"Sub-ADR roster" (HEAD `54a599c`).
Wave-1 ADR-0056a wires `cranelift-jit = "0.131"` + `CodegenMode { Aot,
Jit }` + a minimal arithmetic round-trip. This wave consumes that
switch and removes the ADR-0029 §"Negative" one-shot AOT delegation
entirely.

Two reuses are load-bearing:

- **MIR control-flow lowering** — M11.1 `lower_condition` shared root
  (`cobrust-codegen::cranelift_backend::lower_condition`, ADR-0035)
  handles `if/elif/else` + `while`; ADR-0027 for-protocol handles
  `for`. JIT rebinds against `JITModule` per ADR-0056a §6
  (`cranelift_module::Module` trait bound; both impls satisfy it).
- **M11.2 FnRef Call lowering** — ADR-0034 §"Decision Option 3"
  forward-declaration pass (`ObjectModuleBackend::user_funcs`,
  `accepted` at `ea15683`). Generalises to `JITModule` with **zero
  MIR or HIR surface change** — `declare_function` +
  `define_function` are trait methods.

This ADR ratifies the `Session` struct skeleton parent §6 sketched
and binds the `TypeCheckCtx: Clone + Send` contract ADR-0057 §6 + §11
consume — **the** Phase I × J handoff primitive.

## 2. §2.5 citation

ADR-0054 §2 ranks Phase I §2.5 ROI **medium**. Wave-2 delivers the
payoff: removing AOT one-shot (~50ms/turn) drops L1 closed-loop to
<5ms warm JIT dispatch — the agent-velocity multiplier ADR-0054 §8.5
predicts. §2.5 also binds `Clone + Send` `TypeCheckCtx`: Phase J's
<100ms-per-keystroke IDE budget (ADR-0057 §7) is unmeetable if every
LSP request re-derives the ctx. Phase I produces; Phase J consumes.

## 3. Decision

Three coordinated deliverables:

### 3.1 Control-flow JIT lowering — reuse, no new opcodes

Wire `if/elif/else`, `while`, `for` against `JITModule` via the
existing `lower_module<M: ClifModule>` helper from ADR-0056a §3.2.
The proof obligation is **zero** MIR/HIR-surface change:

- **`if/elif/else`**: lowers through `lower_condition` shared root
  (ADR-0035). Branch / merge blocks materialise via
  `FunctionBuilder::ins().brif` + `FunctionBuilder::switch_to_block` —
  no `ObjectModule`-specific paths.
- **`while`**: same MIR `Loop { header, body, exit }` shape per M11.1;
  block-header phi reconstruction unchanged.
- **`for`**: ADR-0027 for-protocol — `iter()` + `next()` lowered as
  intrinsic calls; `Constant::Str` callee path already JIT-compatible
  per ADR-0056a §6.

Acceptance: `examples/fib.cb` (recursive form per ADR-0034), the
M14.1 corpus's 20 control-flow sessions (ADR-0056 §8.3 corpus
extension), and the M11.1 200-fuzz harness all green under JIT mode.

### 3.2 Stdlib + PRELUDE in REPL — intrinsic rewrite reuse

Stdlib top-level fns (`print`, `len`, `int`, `str`, `float`, `bool`,
`panic`, `assert`, `args`, `var`, `read_line`, `print_err` per
ADR-0029 §"Tab completion sources") and PRELUDE-fn dispatch (ADR-0034
+ ADR-0050a-f) are **rewritten at type-check time** into intrinsic
calls. Mode-agnostic: JIT codegen sees the same MIR call shape as AOT
and lowers via the same `extern_funcs` / `runtime_funcs` path.

Method-form (ADR-0052d-prereq) `s.split(",")` desugars at type-check
to `split(s, ",")`; JIT lowering inherits the desugared form. No
JIT-specific method-dispatch infra in this ADR.

### 3.3 `Session` struct skeleton (Phase J contract)

Final shape ratified here; ADR-0056c populates the `user_funcs` /
`globals` cross-turn semantics. The struct lives in
`crates/cobrust-cli/src/repl.rs::Session` (extending the ADR-0029
HEAD `54a599c` definition at `repl.rs::Session`):

```rust
pub struct Session {
    /// Incremental type-check context; survives across REPL turns.
    /// MUST be `Clone + Send` per ADR-0057 §6 + §11 (Phase J consumer).
    type_ctx: TypeCheckCtx,
    /// User-defined fns; FuncId obtained via JIT `declare_function`.
    user_funcs: HashMap<String, FuncId>,
    /// Mutable globals; JIT-DataId-addressed for cross-turn `let` writes.
    globals: HashMap<String, JitGlobalSlot>,
    /// ADR-0029 §"Public surface" fields preserved (bindings, history, …).
    bindings: HashMap<String, Value>,
}

impl Session {
    pub fn type_ctx(&self) -> &TypeCheckCtx;       // Phase J snapshot input
    pub fn eval(&mut self, line: &str) -> EvalResult; // see §5
}
```

`TypeCheckCtx` is a new struct landing in
`crates/cobrust-types/src/check.rs::TypeCheckCtx` (no prior
construct at HEAD `54a599c`); the impl is incremental per §6.

## 4. Session lifecycle

`Session::new()` is cheap (ADR-0056 §"Negative" <200ms cold-start
holds via lazy `JITModule` init at first non-introspection turn):

- `JITModule` is **not** allocated at `::new()` — only on first
  JIT-bound turn. Introspection (`:type`, `:mir`, `:ast`, `:bindings`)
  never triggers JIT.
- Empty `type_ctx`, `user_funcs`, `globals`; `bindings` ADR-0029-compat.

`Session::eval(line)` happy path:

1. **Parse** — `cobrust-frontend::parse_str` per ADR-0024.
2. **HIR-lower** — `cobrust-hir::lower` per ADR-0011.
3. **Type-check (incremental)** — merge new bindings into `type_ctx`;
   redefinition replaces + invalidates downstream per §6.
4. **MIR-lower** — `cobrust-mir::lower` (existing pipeline).
5. **JIT lower** — `cobrust-codegen::emit_jit` per ADR-0056a §3.3;
   adds synthetic `fn __repl_eval_NNNN()` to the live `JITModule`.
6. **Finalize + invoke** — `module.finalize_definitions()` once per
   turn; pre-transmute signature assertion per ADR-0056a §5; invoke
   via the 4-arm `extern "C"` fn-ptr table.
7. **Return value handle** — `EvalResult::Value(Value)` or
   `EvalResult::Diagnostic(Vec<TypeError>)`.

`Session::drop()` drops the `JITModule`; RWX `memmap2` pages reclaim.

## 5. Incremental `TypeCheckCtx`

`TypeCheckCtx` carries unification `Subst` + symbol-table + per-`DefId`
dependency-map across turns. Per-turn protocol:

- **Add**: `let x = ...` adds `(x: Ty)` to `type_ctx.bindings`;
  `Subst` extended in place.
- **Redefine**: `let x = ...` on existing `x` replaces the entry.
  Dependency-map (`DefId → Vec<DefId>`) drives downstream
  invalidation: any `DefId` body that referenced old `x: Ty` is
  re-type-checked at next reference. (Phase J reuses for multi-file
  invalidation per ADR-0057 §7.)
- **Fn redef**: rejects re-def of on-stack fns per parent §5 risk 3.

`Clone + Send` is the Phase J binding contract. Default-derived
`Clone` on `Subst` is O(n) per turn — kills LSP budget. Inner
structures (`Subst`, `bindings`, `dependency_map`) are `Arc<...>`-shared
with COW: `Clone` is O(1) `Arc::clone`; write-path clones the Arc on
first mutation per turn. Phase J snapshots pay only Arc-bump cost.

## 6. Phase J handoff contract (binding)

ADR-0057 §6 + §11 pin; this ADR ships:

- **`Clone`**: LSP forks per-`hover` snapshot (ADR-0057 §6); without
  `Clone` LSP must serialise via mutex — blows <100ms IDE budget.
- **`Send`**: LSP runtime is `tokio` async (ADR-0057 §9). `Send`
  permits per-request handlers to own a snapshot across `.await`.
- **Lock-free read**: `Session::type_ctx() -> &TypeCheckCtx`; readers
  (LSP `hover` / `completion`) never block writers (REPL `eval`).
  Writers Arc-COW internal state; live ref reflects pre- or post-write
  — Phase J accepts both via per-snapshot freshness versioning
  (deferred to ADR-0057a).

Pre-dispatch gate ADR-0057 §11 ("Phase I shipped + `Clone + Send` +
per-file-invalidation API") closes on this ADR's impl-merge.

## 7. No new MIR / HIR surface

Reuse-only by construction:

- M11.1 `lower_condition` shared root — unchanged.
- M11.2 FnRef::Call path (ADR-0034) — unchanged; rebinds against
  `JITModule` per ADR-0056a §6.
- ADR-0027 for-protocol intrinsic — unchanged.
- ADR-0050a-f PRELUDE / method-form rewrites — unchanged surface;
  type-check-time rewrite is mode-agnostic.

The `lower_module<M: ClifModule>` helper from ADR-0056a §3.2 is the
single mode-agnostic entry point; both AOT (`ObjectModule`) and JIT
(`JITModule`) callers reuse it bytewise.

## 8. Risk register

Top 3 (parent §5 narrowed to wave-2):

1. **MIR locals don't persist across turns.** Parent §5 risk 2. Each
   `__repl_eval_NNNN` is a fresh JIT compilation; locals do NOT
   persist — only `globals` slots do. REPL HIR-lower rewrites bare
   `x` → `__globals.x` when `x ∈ type_ctx.bindings`. Silent
   stale-value risk on shadow-miss. **Mitigation:** shadowing corpus
   (let-rebind, scoped if-block-binding, fn-arg shadowing) lands
   with ADR-0056a §10 acceptance-gate corpus extension; this ADR pins
   the rewrite at HIR-lower entry.

2. **FuncId staleness on fn redefinition.** Parent §5 risk 3.
   `declare_function` returns a **new** FuncId on redef; old
   `get_finalized_function` pointer is stale; in-flight recursive
   calls see OLD body. **Mitigation:** `Session::user_funcs` rejects
   re-def of on-stack fns at type-check entry (per parent §5 risk 3
   + ADR-0056a §5). Clear diagnostic; matches Python REPL.

3. **`TypeCheckCtx::clone()` cost across turns.** Default-derived
   `Clone` on `Subst` + symbol-table is O(n) per turn — kills LSP
   per-keystroke budget on deep-source files. **Mitigation:** inner
   structures Arc-shared (§5); `Clone` is O(1) `Arc::clone`; write
   COW once per mutation. Per-snapshot pays only Arc-bump.

## 9. Pre-dispatch acceptance gate

- ADR-0056a `accepted` (impl-merged); `cranelift-jit = "0.131"`
  builds clean; `CodegenMode { Aot, Jit }` lands; arithmetic
  round-trip green.
- ADR-0034 (M11.2 FnRef::Call) `accepted` at `ea15683`;
  `examples/fib.cb` recursive green.
- ADR-0035 (`lower_condition`) `accepted`; ADR-0027 (for-protocol)
  `accepted`; AOT-mode `if`/`while`/`for` green on M11.1 200-fuzz.
- `cobrust check` + `build` + `repl` smoke on M14 50-session +
  M14.1 20-session control-flow extension all green at dispatch eve.

If any gate fails: defer wave-2 one wave per CLAUDE.md §6.

## 10. Consequences & dispatch readiness

### 10.1 Positive

- Removes ADR-0029 §"Negative" one-shot AOT delegation entirely; L1
  closed-loop tightens ~50ms → <5ms warm.
- Closes ADR-0029 §"Evaluation surface" ❌ rows (loops, if-else,
  comprehensions) via reuse-only.
- Ships `Session::type_ctx: Clone + Send` — Phase J unblocks on
  impl-merge.
- Arc-COW keeps `Clone` O(1); Phase J <100ms keystroke budget holds.

### 10.2 Negative

- Arc-COW infra adds ~200 LOC to `cobrust-types::check` + dependency
  tracking ~150 LOC; non-trivial test surface.
- Sharing-induced versioning subtlety (§6): Phase J snapshot may
  reflect pre- or post-write; per-snapshot version tag deferred to
  ADR-0057a.

### 10.3 Neutral

- No new MIR/HIR/parser surface; reuse-only on 4 prior ADRs (0027,
  0034, 0044, 0056a).
- 50-session ADR-0029 corpus stays green (introspection never JITs).

### 10.4 Dispatch readiness

Per ADR-0056 §9 row 3: TEST 8h, DEV 16h, wall ~3-4 days. TEST
sonnet + DEV opus per MEMORY.md `feedback_subagent_model_tier`.
Two-phase dispatch SOP per `feedback_p9_two_phase_dispatch`.

— P9 Tech Lead, 2026-05-18

## 11. Acceptance addendum (impl-merge, 2026-05-18)

Ratified on impl-merge of feature/0056b-dev (HEAD `b0e1e9e`). The
shipped surface is the **Phase J handoff carrier** of §3.3 + §5 + §6
(the load-bearing Phase J `Clone + Send + invalidate(file_id)`
contract). Two scope refinements vs the proposed text:

### 11.1 Honest scope-narrowing (LARGEST split)

§3.1 (control-flow JIT lowering reuse) + §3.2 (stdlib/PRELUDE in REPL)
remain **proposed**, not delivered in this DEV. The load-bearing
Phase J × J wave-1 dependency (ADR-0057a §10) is the
`TypeCheckCtx: Clone + Send` + `Session::type_ctx()` accessor + the
`invalidate(file_id)` API, all of which §3.3 + §5 + §6 ship. The
JIT control-flow + stdlib eval-side wiring is hand-rolled into the
ADR-0056c roster (which §3.3 already names as the cross-turn
state-machine sub-sprint). This split is deliberate: ADR-0057a's
pre-dispatch gate §10 requires only the §3.3 + §6 surface — so the
wave-1 LSP DEV unblocks without waiting on §3.1/§3.2.

### 11.2 §5 cascade addendum — `binding_defs` map

`TypeCheckCtx` ships with one additional internal Arc field NOT in
the original §3.3 sketch: `binding_defs: Arc<HashMap<String, u32>>`
(name → owning DefId). Required because invalidate must drop
name-keyed rows whose owner belongs to the invalidated file —
a row like `let x = 0` carries `Ty::Int` payload that has NO
DefId reference, so the §5 "type_refs_any" filter alone leaks it.

The addition is invisible to the public surface (no new accessor;
all reads + writes go through existing `lookup` / `invalidate` /
`merge_module`). It preserves §5 Risk 3 Arc-COW O(1) Clone — the
new map is wrapped in the same `Arc<...>` discipline as the four
existing inner structures. Caught by `invalidate_drops_file_owned_rows`
contract test.

### 11.3 Wave-2 delivered surface (the LSP-unblocker)

Concrete public types/methods committed at HEAD `b0e1e9e`:

```rust
// crates/cobrust-types/src/check.rs — new
pub struct TypeCheckCtx { /* Arc-COW internals */ }
impl TypeCheckCtx {
    pub fn new() -> Self;                          // O(1)
    pub fn lookup(&self, name: &str) -> Option<&Ty>;
    pub fn def_type(&self, def_id: u32) -> Option<&Ty>;
    pub fn alias(&self, name: &str) -> Option<&Ty>;
    pub fn subst(&self) -> &Subst;
    pub fn version(&self) -> u64;
    pub fn binding_count(&self) -> usize;
    pub fn bindings(&self) -> impl Iterator<Item = (&String, &Ty)>;
    pub fn invalidate(&mut self, file_id: u32);    // ADR-0057a §4
    pub fn merge_module(&mut self, &TypedModule, file_id: u32);
}
// Clone + Default + Send + 'static (compile-time assertions in tests)

pub fn check_incremental(
    ctx: &mut TypeCheckCtx,
    module: &Module,
    file_id: u32,
) -> Result<TypedModule, TypeError>;

// crates/cobrust-cli/src/repl.rs — extended (ADR-0029 carrier)
impl Session {
    pub fn type_ctx(&self) -> &TypeCheckCtx;       // Phase J snapshot
    pub fn invalidate(&mut self, file_id: u32);    // ADR-0057a §4
}
// + #[derive(Clone)]
```

Test coverage 23 new (16 contract + 7 collocated session). DG verify
PASS green on RTX 3090 workstation; zero new regressions on the
existing failing surface (124 pre-existing F-findings tracked under
MEMORY audit pipeline are identical on Mac main + DG main baselines).
Phase H 188 cobrust-types-cb regression PASS on 8 target test binaries
(unit, type_check_ctx_contract, repl_smoke, repl_session_corpus,
repl_types_integration, borrow_phase_g_e2e [pre-existing 3 skip],
tomli_e2e, parity_wave1).

### 11.4 Phase J wave-1 pre-dispatch gate closure

Per ADR-0057a §10:

- [x] Phase I `Session::type_ctx: Clone + Send` contract shipped.
- [x] `TypeCheckCtx: Clone + Send + 'static` compile-time asserted.
- [x] `Session::invalidate(file_id: u32)` per-file API.
- [x] ADR-0056 + ADR-0056a accepted; this ADR ratifies on merge.
- [x] No regressions on ADR-0052b snapshot corpus (delta tests run
      clean — 25/25 PASS on repl_smoke + repl_session_corpus).

ADR-0057a wave-1 (`textDocument/publishDiagnostics`) is now
dispatchable.

— P10 CTO, 2026-05-18 (post-merge ratification)
