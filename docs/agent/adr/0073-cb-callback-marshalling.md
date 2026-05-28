---
doc_kind: adr
adr_id: 0073
title: .cb↔Rust callback marshalling — fn-ptr passing across the ecosystem boundary (pit/hood prerequisite)
status: accepted
date: 2026-05-28
last_verified_commit: caa0510
relates_to: [adr:0034, adr:0050d, adr:0072, "claude.md:§2.2", "claude.md:§2.5"]
---

# ADR-0073: `.cb`↔Rust callback marshalling

## 1. Context

ADR-0072's `.cb` ecosystem-import chain (5 modules CI-verified: den/nest/strike/scale/molt)
handles value-in-value-out + opaque-handle patterns. **It does NOT yet handle Rust→`.cb`
cross-boundary calls** — the prerequisite for `cobrust-pit` (`@app.route("/path")` +
handler fns) and `cobrust-hood` (`@click.command` + handler fns) `.cb` surfaces.

Current state (mapped by the design Plan agent 2026-05-28):
- `Ty::Fn(FnTy)` exists end-to-end (`cobrust-types/src/ty.rs:69`); top-level fn names
  carry their signature through `lookup_local_for_resolved` + `def_types`.
- `Constant::FnRef(def_id)` exists in MIR (`cobrust-mir/src/tree.rs:320`), used for
  recursive/forward-decl callee resolution. **But as a VALUE**, codegen emits a
  **zero pointer** (`llvm_backend.rs:3876` + `cranelift_backend.rs:1414` — ADR-0034
  preserved stubs explicitly for this future use).
- `declare_body` (`llvm_backend.rs:2991`) declares every user fn with `Linkage::External`
  + the default C calling convention; every `.cb` fn already has a stable C-ABI symbol
  address (the proven `_cobrust_user_main` ↔ `cobrust_main.c` path).
- Decorators (`@expr`) parse + HIR-bind but **MIR walks through `Decorated` as a no-op**
  (`cobrust-mir/src/lower.rs:115/169`). Today `@pit.route("/x")` is a runtime no-op.
- Closures: `ExprKind::Lambda` exists at the parser, but MIR lower emits
  `Constant::FnRef(0)` (placeholder) — **closures are absent at the codegen level**.

## 2. Decision — reuse `Ty::Fn` + promote `Constant::FnRef` to first-class fn-ptr

Same design as the Plan output's D1–D8:

- **D1 (callback type)**: reuse `Ty::Fn`; no new `Ty::FnPtr`. Manifest carries the
  expected `FnTy` via a new `EcoParam::Callback(FnTy)` variant.
- **D2 (MIR)**: keep `Constant::FnRef`; the ecosystem-call rewrite recognises an
  `EcoParam::Callback` slot and emits `Operand::Constant(Constant::FnRef(def_id))`
  for the source `ExprKind::Name` resolving to a `DefKind::Function`.
- **D3 (codegen)**: replace the zero-pointer stubs at `llvm_backend.rs:3876` +
  `cranelift_backend.rs:1414` with real fn-pointer materialisation
  (`function_ids.get(id).as_global_value().as_pointer_value()` / Cranelift's
  `declare_func_in_func + func_addr`).
- **D4 (Rust trampoline)**: every callback has the **fixed C-ABI**
  `unsafe extern "C" fn(*mut u8) -> *mut u8`. The trampoline in `cobrust-pit/src/cabi.rs`
  transmutes the `*const c_void` arg into `CbHandlerAbi` and wraps it in a
  `move |req| { Box::into_raw(Box::new(req)); raw(...); Box::from_raw both; }` closure
  satisfying axum's `Arc<dyn Fn(Request) -> Response + Send + Sync + 'static>` bound.
  fn-pointer `Send + Sync` is the blanket impl; `'static` follows because the `.cb`
  fn lives in the binary's text segment for the process lifetime.
- **D5 (decorator desugar)**: in HIR lower, when a decorator head resolves to an
  ecosystem alias for `pit`/`hood`, synthesize a register-call sibling item
  (`_ = app.route("GET", "/ping", handle_ping)`) into the module's init body. Other
  decorators stay no-ops. **Deferred from ADR-0073 to ADR-0074 (per Q3 below)** — this
  ADR ships the explicit `app.route(...)` form; the decorator sugar lands next.
- **D6 (ownership)**: Rust runtime owns the Request box (Box::into_raw → `.cb` borrows →
  Box::from_raw on return); `.cb` handler constructs the Response via
  `__cobrust_pit_response_new` (Boxed in Rust, the `.cb` Response handle local is the
  same `Ty::Adt` pattern as den's Connection), returns it to Rust which unboxes.
  **Return-of-handle is the only allowed scope-escape**; codegen suppresses drop on
  operands feeding directly into `Terminator::Return` (verify; small drop-pass fix if
  not already).
- **D7 (first proof)**: pit "pong" — a `.cb` program that imports pit, defines
  `fn handle_ping(req) -> Response: return pit.text_response(200, "pong")`,
  `app.route("GET", "/ping", handle_ping)`, `app.serve_in_background(host, 0)`, then
  the E2E test issues a real `GET /ping` via reqwest and asserts body `pong` + 200.
- **D8 (scope cap)**: top-level fns only. No closures. No fn-typed-local intermediaries.
  Type-checker rejects each with a clear "expected a top-level fn name" diagnostic.

## 3. Q1–Q6 decisions (CTO ratification of the Plan recommendations)

- **Q1 (manifest signature shape)**: **inline `FnTy` in the EcoParam::Callback entry**.
- **Q2 (synthetic register-call placement)**: **module init body**. Decorator on `app`
  declared inside a fn is unsupported in the first proof; rejected at HIR with a clear
  message.
- **Q3 (decorator sugar scope)**: **deferred to ADR-0074**. ADR-0073 ships the explicit
  `app.route(method, path, fn_name)` callback chain — the load-bearing technical work.
- **Q4 (lambda promotion)**: **stay rejected for v0.7.0**. Existing `ExprKind::Lambda`
  MIR placeholder stays; type-checker rejects lambdas in callback slots with a fix
  suggestion ("use a top-level `fn` name instead").
- **Q5 (panic across boundary)**: **abort-on-panic** via the existing `__cobrust_panic`
  shim (`cobrust-codegen/src/llvm_backend.rs:1123`). NOT migrating to `extern "C-unwind"`
  (workspace-wide cascade).
- **Q6 (hood handler arg shape)**: **boxed `RunResult` handle**. Mirrors pit's Request
  → ONE callback ABI shape (`extern "C" fn(*mut u8) -> *mut u8`) → ONE trampoline
  pattern for both pit and hood.

## 4. Implementation per layer

Per Plan output §c — critical files:
- HIR: `cobrust-hir/src/lower.rs:242-266` extend `Decorated` lowering (only when D5
  desugar lands per Q3 in ADR-0074; ADR-0073 leaves decorators no-op).
- Types: `cobrust-types/src/ecosystem.rs` — add `EcoParam` enum, pit ADT ids in a new
  `0xE000_0400/0500` block (Plan used pit `0xE000_0400`, hood `0xE000_0500` — adopt).
  `check.rs:2101` `check_eco_sig` dispatches Value vs Callback. New `TypeError` kinds.
- MIR: `cobrust-mir/src/lower.rs:1882` `try_lower_ecosystem_call` — `EcoParam::Callback`
  arm emits `Constant::FnRef`. `drop.rs` confirm no drop on `Constant::FnRef` operands.
- Codegen: `llvm_backend.rs:3876` + `cranelift_backend.rs:1414` materialise fn-ptr;
  `declare_runtime_helpers` add pit/hood externs.
- CLI: `intrinsics.rs:1353` recognizer arm for `__cobrust_pit_*` + `__cobrust_hood_*`.
- pit runtime: NEW `cobrust-pit/src/cabi.rs` with the D4 trampoline + shims; Cargo.toml
  add `"staticlib"`.
- hood runtime: handler storage extension (`decorators.rs:208/276`) + NEW cabi.rs.
- ADR-0073 first proof scope = pit only; hood follows the same chain in a paired
  follow-up sprint (ADR-0073 also lays the chain for hood — only hood's handler
  invocation path needs adding).

## 5. Risks (from Plan §e)

1. **Trampoline `'static` claim** — sound under AOT (binary text segment is process-
   lifetime). Test asserts the closure satisfies `Handler` bound. Future dynamic-loaded
   modules would invalidate; out-of-scope for v0.7.0.
2. **Panic across C boundary** — `__cobrust_panic` aborts (Q5); safe by termination.
3. **Lifetime of stored closures** — safe under the existing `App::run`/
   `serve_in_background` flow; no API exposes route handles outliving `App`.
4. **Type-check error UX** — ≥5 negative-test corpus cases required (rejected shapes:
   lambda, call-result, parenthesized, method-ref, fn-typed local).
5. **Cranelift/LLVM parity** — **N/A in this repo** (drift caught by the impl agent
   2026-05-28). `cobrust-codegen/src/cranelift_backend.rs` was removed in ADR-0070 §X.4
   (commit `09f57ba`); LLVM is the sole AOT backend. Cranelift survives only as the
   `cobrust-jit` IR substrate (`lowering.rs`), which doesn't materialise `Constant::FnRef`.
   So the fn-ptr materialisation lands in LLVM only. The "lock-step" wording in the
   original draft is doc drift from pre-X.4 — kept here as a process note (catch by the
   first-proof impl agent).
6. **Manifest drift** — accepted per ADR-0072 §5 R4; pit/hood add ~15 entries.

## 6. Done-means (first proof — pit only, hood ADR-0073-sibling sprint follows)

1. `cobrust check examples/pit_pong/main.cb` — 0 errors. ≥3 negative-callback cases
   reject with clear diagnostics.
2. MIR shows `app.route` callee retargeted + third arg `Constant::FnRef(handle_ping)`.
3. `cobrust build` links `libcobrust_stdlib.a + libpit.a`; `nm` shows handle_ping
   + relocation from `_cobrust_user_main` to `__cobrust_pit_app_route`.
4. E2E (`crates/cobrust-cli/tests/pit_pong_e2e.rs`): real `GET /ping` round-trip
   → body `pong` + status 200; `GET /missing` → 404.
5. `pit::cabi::DROP_COUNT` shows exactly-once Request+Response drops per call.
6. Workspace gates green (fmt/clippy/build/test/doc-coverage + cargo test --workspace
   --locked run to completion).

After pit first proof + audit + push:
- ADR-0074: decorator desugar (Q3 deferred) — `@app.route("/x")` → `_ = app.route(...)`.
- ADR-0073-sibling: hood `.cb` wiring on the proven trampoline pattern.
- Z.8 REST demo (network MUST-ship §5) — unblocked by pit's `.cb` surface + den + json.
