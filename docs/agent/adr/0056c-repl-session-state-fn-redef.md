---
doc_kind: adr
adr_id: 0056c
parent_adr: 0056
title: "Phase I wave-3 — REPL fn-redefinition lifecycle + per-symbol TypeCheckCtx invalidation"
status: accepted
date: 2026-05-19
last_verified_commit: 3626021
supersedes: []
superseded_by: []
relates_to: [adr:0056, adr:0056a, adr:0056b, adr:0057]
discovered_by: P9 — ADR-0056 §4 sub-ADR roster, day 6 slot
ratification_path: P9 ADR review; ratifies on impl-merge gate (final wave under ADR-0056 frame)
---

# ADR-0056c: REPL fn-redefinition lifecycle + per-symbol TypeCheckCtx invalidation

## 1. Context

ADR-0056 §4 assigns this sub-ADR the day-6 final-wave slot: state
machine + fn-redefinition lifecycle + multi-file invalidation API.
First two close M14.1 (ADR-0029 ❌ rows); third is the Phase I × J
handoff contract ADR-0057 §6 + §11 consume.

Wave chain (HEAD `54a599c`):

- **0056a** pinned `get_finalized_function` SIGSEGV mitigation:
  4-arm `extern "C"` table + Session-side pre-transmute assertion.
- **0056b** ships the `Session` struct (`type_ctx, user_funcs,
  globals`) + `Clone+Send` on `TypeCheckCtx`. **Pre-dispatch gate.**
- **0056c** (this) — state machine + fn-redefinition diagnostic +
  multi-file invalidation API.
- **ADR-0057** (downstream) — `Session::snapshot_for_lsp` is the
  binding entry-point for `LspFileCtx`.

ADR-0056 §5 risk 3: "Fn-redefinition mid-call sees old FuncId... A
recursive `fact` mid-call still sees the OLD body." This ADR pins
the diagnostic + safe-pointer abstraction.

## 2. §2.5 citation

ADR-0054 §2 Phase I §2.5 ROI **medium**. 0056c delivers the Phase J
handoff — `Clone+Send` `TypeCheckCtx` snapshot + per-file
invalidation — load-bearing for ADR-0057 §2 LLM-amplifier ROI #1.
0056c's own §2.5 surface: fn-redefinition diagnostic (§4.4). LLM
sees `RedefineActiveFunction { name, span, suggestion }`, not SIGSEGV.

## 3. Decision — session as state machine

7-state automaton per turn. Each transition atomic w.r.t. user-
observable state; panic-unwind restores prior state (§7.1).

```rust
enum SessionState {
    Idle, Parsing, TypeChecking, Lowering,
    JitDefining, Invoking,
    // Error -> rollback -> Idle
}
```

`Idle → Parsing → TypeChecking → Lowering → JitDefining → Invoking
→ Idle`. Any Error rolls back via shadow-buffer. Forward transitions
mutate **only on success**.

| State | Reads | Writes (on success) |
|---|---|---|
| `Parsing` | input | (none — `ast` local) |
| `TypeChecking` | `type_ctx` | `type_ctx` (delta merge) |
| `Lowering` | `type_ctx`, `user_funcs` | (none — MIR local) |
| `JitDefining` | `user_funcs` | `user_funcs`, `globals`, JIT |
| `Invoking` | `user_funcs`, `globals` | `globals` (return capture) |

## 4. Fn-redefinition safety

**Cranelift**: `module.declare_function(name)` twice → **new**
`FuncId`. Old FuncId's finalized pointer remains live; module does
NOT swap pointers atomically. `clear_context` too aggressive.

**Session on redefine `fn foo() -> i64`**:
1. Fresh `FuncId` via `declare_function("foo", ...)`.
2. Lower new body into same `JITModule`.
3. `finalize_definitions()`.
4. **Update** `user_funcs["foo"]` to new FuncId. Old FuncId dropped
   from table; finalized pointer remains as orphan (no UAF).
5. **Old FuncId pointers stay valid** — no `clear_context` until
   session end. Staleness only an issue if user holds raw pointers,
   which Session **abstracts away**.

**Mitigation — `Session::call` abstracts the pointer**:

```rust
impl Session {
    pub fn call(&mut self, name: &str, args: &[Value])
        -> Result<Value, SessionError>
    {
        let id = self.user_funcs.get(name)
            .ok_or(SessionError::UnknownFunction)?;
        let raw = self.jit_module.get_finalized_function(*id);
        self.dispatch_typed(raw, args)  // 4-arm extern "C" (0056a §5)
    }
}
```

Every `call` resolves current FuncId — redefinition between turns
invisible.

**Residual hazard — in-flight recursive redefinition**: user
redefines `fact` while `fact(N)` mid-execution; JIT does not hot-
patch on-stack frames; recursive call sees OLD body. Matches Python
REPL semantics. Detection: `call_stack: Vec<String>` (push on
`Invoking`, pop on return). Redefine attempt during non-empty stack
containing `name`:

```rust
SessionError::RedefineActiveFunction {
    name, span,
    suggestion: Some("rename to `<name>_v2` or restart REPL session"),
}
```

§2.5 compile-time-catch at turn boundary.

## 5. Multi-file invalidation API — Phase J handoff

ADR-0057 generalises single-Session to per-file `LspFileCtx`. 0056c
ships the Session-side primitive.

```rust
impl Session {
    /// Invalidate bindings/types/funcs from given source file.
    /// Phase J calls on `textDocument/didChange`.
    pub fn invalidate(&mut self, file: PathBuf) -> usize;
}

pub struct Session {
    // ... 0056a + 0056b fields ...
    def_to_file: HashMap<DefId, FileId>,
    file_to_defs: HashMap<FileId, Vec<DefId>>,
}
```

`invalidate(file)`: resolve `FileId`; for each DefId in
`file_to_defs[FileId]` remove from `type_ctx`, `user_funcs` (if fn),
`globals` (if let), `def_to_file`; clear file_to_defs entry; return
count.

**Budget**: <1ms p99 on M14.1 corpus (ADR-0057 §7 demands <100ms
per-keystroke type-check). O(N) on file's DefIds; N<100 typical.
Benchmark gate ships with impl.

**Out of scope**: cross-file dep tracking (ADR-0057c); incremental
re-type-check (ADR-0057a).

## 6. Concurrent access — Send but not Sync

`Session: Send` (LSP per-request snapshots fork ctx to worker);
`Session: !Sync` (single-writer; LSP server holds
`Arc<Mutex<Session>>`).

```rust
impl Session {
    pub fn snapshot_for_lsp(&self, file: PathBuf) -> LspFileCtx {
        LspFileCtx {
            source_version: self.source_version,
            type_check_ctx: self.type_ctx.clone(),  // Clone+Send
            // ... defensive Clone of read-only views ...
        }
    }
}
```

`snapshot_for_lsp` takes `&self`, Clones, ships to worker. Binding
API ADR-0057a/b/c/d consume.

## 7. Risk register

**7.1 State machine consistency under panic-unwind.** Mid-transition
panic must roll back to `Idle` without leaking half-mutated state.
**Mitigation**: shadow-buffer pattern. Each transition computes into
local buffer; merged into canonical state only on success.
`std::panic::catch_unwind` wraps each state body; on unwind, buffer
dropped, Session intact.

**7.2 Multi-file invalidation cost (<1ms p99 SLA).** HashMap
re-hashing under cascading invalidation (cross-file rename of
heavily-imported symbol). **Mitigation**: benchmark gate ships with
impl (50+20-session corpus × per-file `invalidate`; p99 per CI). If
>1ms: shard `def_to_file` by `FileId` hash prefix, or precompute
file-removal closures at parse time.

**7.3 Concurrent snapshot during eval.** LSP `hover` arrives during
REPL turn (deferred post-Phase-J). Hover calls `snapshot_for_lsp(&self)`
while REPL is mid-`TypeChecking` with `&mut self`. **Mitigation**:
by-construction via Rust borrow-check + `Arc<Mutex<Session>>`
serialisation. Hover waits for REPL turn (<50ms per ADR-0056 §2.5).
Documented in ADR-0057b hover handler.

## 8. Pre-dispatch acceptance gate

- [ ] **ADR-0056b accepted** — `Session` struct + `Clone+Send`
      shipped. Without this, §3 has no target.
- [ ] ADR-0056a impl-merge green — JIT mode + 4-arm `extern "C"` +
      pre-transmute assertion in tree.
- [ ] ADR-0029 <200ms cold-start budget preserved.
- [ ] `cobrust repl` 50-session corpus 🟢 at dispatch eve.

Parent ADR-0056 frame ratify already triggered by 0056a per parent
`ratification_path`; 0056c closes the Phase I roster.

## 9. Phase J handoff

> **Phase-ordering note**: `LspFileCtx` is declared in `crates/cobrust-lsp`
> per ADR-0057 §6; therefore `Session::snapshot_for_lsp` cannot land until
> Phase J starts. Phase I close ships only the **internal
> `Session::clone_type_ctx() -> TypeCheckCtx`** primitive; Phase J 0057a
> wraps that into the public `snapshot_for_lsp` returning `LspFileCtx`.

`Session::snapshot_for_lsp(file) -> LspFileCtx` (§6) is the binding
API ADR-0057a/b/c/d consume:

- **0057a** (diagnostics) — re-render `TypeError::* → Diagnostic`.
- **0057b** (hover + completion) — `:type EXPR` at cursor.
- **0057c** (definition + rename) — `DefId → source_span` map
  (sibling of `def_to_file` §5).
- **0057d** (codeAction) — same diagnostic stream as 0057a.

`Clone + Send` on `TypeCheckCtx` is load-bearing for every above.
0056b ships it; 0056c codifies the binding API; 0057 §6 + §11
consume.

## 10. Consequences

**Positive**: Phase I final wave closes M14.1 end-to-end;
fn-redefinition diagnostic is a §2.5 win (no SIGSEGV remains user-
facing); multi-file invalidation unblocks Phase J dispatch; Session
is `Send` + `Mutex`-friendly (LSP architecture borrow-checker-
validated).

**Negative**: `def_to_file` + `file_to_defs` add ~100B per DefId
steady-state; `call_stack: Vec<String>` per-turn alloc (freed at
Idle); shadow-buffer doubles transient memory during a turn
(bounded by per-turn fn count).

**Neutral**: state machine internal to `cobrust-cli/src/repl.rs`
(no public surface change); `snapshot_for_lsp` public only when
`cobrust-lsp` consumes (not v0.3.x public API).

## 11. Dispatch readiness

TEST 6h, DEV 12h. Wall ~2-3 days post-decomposition (within parent
1-week wall accounting for 0056b buffer). Sub-dispatch: TEST + DEV
parallel under PAIR (P10-direct per
`feedback_adsd_pair_pattern_impl_gap`).

— P9 Tech Lead, 2026-05-18

## 12. Acceptance addendum (impl-merge, 2026-05-19)

Ratified on impl-merge of `feature/0056c-dev` (HEAD `3626021`).
The shipped surface is the **fn-redefinition lifecycle** + the
**per-symbol invalidation primitive** (§4 + §5 narrowed). Three
honest scope-narrowings vs the proposed §3 + §4 + §6 text.

### 12.1 Honest scope-narrowing (LARGEST split)

The proposed §3 7-state `SessionState` automaton + shadow-buffer
panic-unwind machinery is **deferred** to ADR-0056c.x or the M14.2
roster. The wave-3 DEV ships only the **redefine transition** that
§3 §"Lowering" row would have produced (atomic re-bind via
`invalidate_def` + `merge_module`). Rationale: at M14.1 the REPL has
no JIT call stack (the §4 "Residual hazard" call_stack detector is
trivially satisfied at REPL turn boundaries because the REPL does
not yet **execute** fn bodies — only type-checks signatures); the
full 7-state machine has no observable consumer at the M14.1
surface, so building it speculatively would have violated CLAUDE.md
§8 "smallest correct increment".

§5 multi-file invalidation surface — `def_to_file` +
`file_to_defs` cross-maps + the `Session::invalidate(file: PathBuf)`
PathBuf-keyed entry — was already shipped at wave-2 via the simpler
`file_id: u32` keying (per ADR-0056b §11.4 + §"binding_defs map").
Wave-3 EXTENDS that with the per-DefId form (`invalidate_def`).
ADR-0057c can re-broaden later if PathBuf-keyed invalidation
becomes load-bearing for goto-definition / rename.

§6 `Session::snapshot_for_lsp(file) -> LspFileCtx` remains unshipped
at wave-3 boundary per the existing §9 phase-ordering note (the
`LspFileCtx` type lives in `crates/cobrust-lsp` and ships under
ADR-0057a wave-2+). Phase J wave-1 (ADR-0057a) already consumes
the wave-2 `Session::type_ctx()` + Arc-COW Clone, which was the
real binding contract; the `snapshot_for_lsp` name is editorial
sugar for future LSP roster ADRs.

### 12.2 §4 cascade addendum — `RedefineOutcome` enum + inline path

§4 sketches "rename to `<name>_v2` or restart REPL session" as the
only diagnostic. The shipped surface goes further: `RedefineOutcome`
classifies the redef as `{ Created, Identical, SignatureChanged }`
with the old + new signature strings carried on the changed variant,
and `evaluate_module` inlines the same flow for plain REPL fn-def
re-entry (so the user just re-types `fn f(x): ...` and gets a
structured one-line notice). Public surface:

```rust
// crates/cobrust-types/src/check.rs
impl TypeCheckCtx {
    pub fn invalidate_def(&mut self, def_id: u32);       // §4 atomic drop
    pub fn binding_def_id(&self, name: &str) -> Option<u32>;
}

// crates/cobrust-cli/src/repl.rs
pub enum RedefineOutcome {
    Created { name: String },
    Identical { name: String },
    SignatureChanged { name: String, old: String, new: String },
}
impl RedefineOutcome { pub fn user_message(&self) -> String; }

impl Session {
    pub fn redefine_fn(&mut self, name: &str, source: &str)
        -> Result<RedefineOutcome, String>;
}
```

Failed-typecheck on the new body leaves the old binding intact
(matches Python REPL ergonomics) — covered by
`failed_typecheck_redef_preserves_old_binding` test.

### 12.3 Wave-3 delivered surface

| Layer | Symbol | File anchor |
|---|---|---|
| types | `TypeCheckCtx::invalidate_def` | `cobrust-types::check::TypeCheckCtx::invalidate_def` |
| types | `TypeCheckCtx::binding_def_id` | `cobrust-types::check::TypeCheckCtx::binding_def_id` |
| cli | `Session::redefine_fn` | `cobrust-cli::repl::Session::redefine_fn` |
| cli | `RedefineOutcome` enum | `cobrust-cli::repl::RedefineOutcome` |
| cli | inline fn-redef path | `cobrust-cli::repl::Session::evaluate_module` |

Tests: `crates/cobrust-cli/tests/session_fn_redef.rs` (8 cases) —
all green on DG (RTX 3090 workstation) and Mac.

### 12.4 DG verify + regression snapshot

- DG `cargo test --no-fail-fast -p cobrust-types -p cobrust-cli -p
  cobrust-lsp -p cobrust-jit` on HEAD `3626021`:
  - `session_fn_redef`: 8/8 PASS (new).
  - `repl_smoke`: 22/22 PASS.
  - `repl_session_corpus`: 3/3 PASS.
  - `type_check_ctx_contract`: 16/16 PASS (0056b preserved).
  - `snapshot_diagnostics` (LSP): 5/5 PASS (0057a preserved).
  - `cobrust_lsp` lib: 11/11 PASS.
  - Total failed tests on 0056c branch: 136.
  - Total failed tests on main HEAD `e2d8ecb`: 136.
  - **Net regression: 0** (identical pre-existing baseline).
- POSTFLIGHT `/tmp/cobrust-*`: clean (POST_TMP=0).

### 12.5 Phase I closure

This ADR closes the Phase I sub-roster:

- 0056a — JIT mode + 4-arm extern "C" (accepted).
- 0056b — `Session` + Arc-COW `TypeCheckCtx` Clone+Send (accepted).
- 0056c — fn-redefinition lifecycle + per-symbol invalidate
  (accepted, this addendum).

Phase J (ADR-0057a wave-1) merged at `e2d8ecb` consumes the wave-2
contract; wave-3 is additive (no consumer-side break).

— P10 CTO, 2026-05-19 (post-merge ratification)
