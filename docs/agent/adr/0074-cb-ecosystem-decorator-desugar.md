---
doc_kind: adr
adr_id: 0074
title: .cb ecosystem-decorator desugar — @pit.route / @hood.command sugar on top of ADR-0073's callback chain
status: accepted
date: 2026-05-28
last_verified_commit: 8a3e8bf
relates_to: [adr:0072, adr:0073, "claude.md:§2.1", "claude.md:§2.5"]
---

# ADR-0074: `.cb` ecosystem-decorator desugar

## 1. Context

ADR-0073 shipped the explicit-form callback chain — `app.route("GET", "/ping", handle_ping)`
is a working end-to-end call (5 compiler layers + trampoline + drop). ADR-0073 §3 Q3
deferred the **decorator sugar** (`@app.route("/ping")` over the next-line fn def) to
ADR-0074. That sugar is the LLM-first §2.5 surface for Flask (`@app.route` is the
training-corpus shape) and Click (`@click.command`).

Current state:
- Parser captures `@expr` lines + attaches as `ast::StmtKind::Decorated { decorators, inner }`
  (`cobrust-frontend/src/parser.rs:321` / `ast.rs:48-53`).
- HIR wraps the inner item as `h::ItemKind::Decorated { decorators, inner }`
  (`cobrust-hir/src/lower.rs:242-266`).
- **MIR walks through `Decorated` as a no-op** (`cobrust-mir/src/lower.rs:115/169`).
- Net: `@pit.route("/x")` parses + typechecks + lowers ITS INNER FN cleanly, but the
  decorator itself is a **runtime no-op** — the route is never registered.

CLAUDE.md §2.1 keeps decorators (composition primitive). ADR-0074 gives them ecosystem
semantics.

## 2. Decision — HIR-level desugar, only for known ecosystem-alias decorators

When a `Decorated` item's decorator expression head resolves to an ecosystem alias
(`pit`/`hood`/etc.), emit a SYNTHETIC sibling register-call into the module's init body.
All other decorators stay no-ops (status quo — composition + reflection sugar; runtime
semantics are a separate concern).

**Example** (`pit`):
```python
app = pit.App()

@app.route("/ping")
fn handle_ping(req: pit.Request) -> pit.Response:
    return pit.text_response(200, "pong")
```
desugars in HIR (document order; the synthetic call appears AFTER the fn item so name
resolution sees `handle_ping`):
```
app = pit.App()                                   # original
fn handle_ping(req: pit.Request) -> pit.Response: # the inner Fn, lowered normally
    return pit.text_response(200, "pong")
_ = app.route("GET", "/ping", handle_ping)        # SYNTHETIC ExprStmt
```

**Example** (`hood` — parallel):
```python
cmd = hood.command("add")

@cmd.handler
fn add(a: i64, b: i64) -> i64:
    return a + b
```
→ `_ = cmd.set_handler(add)` synthetic ExprStmt.

The synthetic call goes through ADR-0073's existing `try_lower_ecosystem_call` chain:
manifest dispatch → `Constant::FnRef(handle_ping_def_id)` arg → codegen fn-ptr
materialisation → `__cobrust_pit_app_route` trampoline. **Zero new compiler infra** is
needed — the callback chain ADR-0073 ratified is the load-bearing path.

### Method resolution rule for the decorator head

The decorator expr's head can be:
- `Attr { base: Name(rn), name }` where `rn` resolves to an ecosystem `ImportAlias`
  (e.g. `@pit.command(...)` if pit ships a module-level decorator — currently it doesn't).
- `Attr { base: Name(rn), name }` where `rn` is an `app`-like binding whose type is an
  ecosystem handle (e.g. `@app.route("/x")` where `app: pit.App`). This is the common case.
- `Name(rn)` where `rn` is an ecosystem handle method bound via `@cmd.handler` (no call,
  just an attr ref).

For first proof: recognise `Attr(base, method)` where `base` typechecks to a known
ecosystem handle type (`pit.App` for `@app.route`, `hood.Command` for `@cmd.handler`)
AND the `method` exists in the manifest as an `EcoParam::Callback`-bearing fn. The HIR
pass synthesises `_ = base.method(<call_args>, <inner_fn_name>)` where `<call_args>` are
the decorator's call args (the `"/ping"` in `@app.route("/ping")`) and `<inner_fn_name>`
is the decorated fn's name as a `Constant::FnRef`-bearing operand.

## 3. Open-question decisions

- **Q1 (scope rule)**: synthetic register-call goes in the **module init body** (per
  ADR-0073 §3 Q2). Decorating a fn nested inside another fn is REJECTED at HIR with a
  clear "ecosystem decorators must be at module scope" diagnostic. Relaxing is a
  follow-up.
- **Q2 (decorator-with-args vs decorator-bare)**: support BOTH (`@app.route("/x")` AND
  `@cmd.handler`). The parser already accepts both; the HIR-resolve fork keys on whether
  the decorator expression is `Call(...)` (forward the call args) or bare `Attr(...)`
  (no call args).
- **Q3 (multiple decorators)**: ecosystem decorators are NOT stackable in the first
  proof — exactly one ecosystem decorator per fn. Stacking non-ecosystem decorators
  with one ecosystem decorator is fine (the non-ecosystem ones stay no-ops; the
  ecosystem one fires).
- **Q4 (kwargs in decorator call)**: support `@app.route("/x", methods=["GET","POST"])`
  if pit's manifest entry declares kwargs. The HIR desugar threads kwargs through to
  the synthetic call args. Defer if pit's manifest entry doesn't ship kwargs in this
  increment.
- **Q5 (the inner-fn's signature mismatch)**: caught by the existing ADR-0073 callback
  type-check — `try_synth_ecosystem_call` runs against the synthetic call, the
  `EcoParam::Callback(FnTy)` slot validates the decorated fn's `Ty::Fn` against the
  expected `FnTy`. Same error UX as today's explicit form.

## 4. Implementation per layer

Per ADR-0073's existing scaffolding — ADR-0074 is a **single-pass HIR addition**:

- `cobrust-hir/src/lower.rs:242-266` — extend `Decorated` lowering:
  - For each decorator expr: type-resolve the head. If it's
    `Attr(base: Name(rn), method)` AND `base.ty()` is a known ecosystem handle type AND
    `method` looks up in the manifest with at least one `EcoParam::Callback` slot →
    desugar.
  - Synthesize an `h::Item::ExprStmt(...)` with the synthetic call AFTER the inner fn,
    appended to the module's item list.
  - Otherwise: status-quo no-op (the `Decorated` wrapper is preserved, MIR walks
    through).
- `cobrust-types/src/ecosystem.rs` — no change required; the existing `EcoParam::Callback`
  slot already encodes the expected `FnTy`. May add a `is_decoratable: bool` hint per
  manifest entry to gate which methods accept the decorator sugar (defer; first proof
  recognises `pit.App.route` + `hood.Command.handler` by name).
- Everything downstream (typecheck, MIR retarget, codegen, link) reuses ADR-0073.

## 5. First proof — pit `@app.route` E2E

The `examples/pit_pong/` directory from ADR-0073 first proof is rewritten in decorator
form:
```python
import pit

app = pit.App()

@app.route("/ping")
fn handle_ping(req: pit.Request) -> pit.Response:
    return pit.text_response(200, "pong")

fn main() -> i64:
    let _ = app.serve_in_background("127.0.0.1", 0)
    return 0
```

The HIR desugar inserts `_ = app.route("GET", "/ping", handle_ping)` after the
`handle_ping` fn item. The synthetic call goes through `try_lower_ecosystem_call` →
`__cobrust_pit_app_route(app, "GET", "/ping", handle_ping_fn_ptr)`. The trampoline
registers + axum serves. Test client gets `pong` + 200.

**Done-means**: identical to ADR-0073 first proof (compile→link→run + 200 + drop-once),
PLUS a new negative-test: a decorator with mismatched fn signature
(`@app.route("/x") fn bad() -> i64:`) is rejected at typecheck with the same
`CallbackSignatureMismatch` error ADR-0073 ships.

## 6. Scope cap

- Top-level fns only (per ADR-0073 §2 D8). Lambdas, fn-typed locals, and call results
  still rejected.
- Single ecosystem decorator per fn. Non-ecosystem decorators stack as no-ops.
- HTTP method default `GET` when `@app.route("/path")` is used without `methods=`.
- Decorating a class is OUT OF SCOPE (no ecosystem currently exposes class-level
  decorators; revisit if/when one does).

## 7. Risks

1. **Name-resolution ordering**: the desugar inserts the synthetic register-call AFTER
   the inner fn item in document order, so HIR name resolution sees `handle_ping`
   defined. Verify the existing HIR is order-aware for synthesised items (it should be,
   per the standard pre-pass).
2. **kwargs threading**: if Q4 is enabled, kwargs `methods=["GET","POST"]` must be
   convertible to the synthetic positional args. Defer if pit's manifest doesn't
   currently take a methods list.
3. **Error UX**: a mistakenly-shaped decorator (`@app.route` without args, or with the
   wrong number of args) should produce a clear "expected `@app.route("/path")`"
   diagnostic, NOT a confusing chain error. Add a HIR-level shape check.
4. **Conflict with future class-method decorators**: when classes get ecosystem-handle
   methods (`@app.middleware`), the module-scope rule (Q1) bars them — that's a
   follow-up ADR's concern.

## 8. Implementation sequence (after pit pong CI lands)

1. Sprint A — ADR-0074 implementation: HIR desugar + ≥3 positive (route + command +
   methods=) + ≥3 negative (nested-fn decorator, multi-ecosystem stack, mismatched sig)
   E2E cases. Single focused sprint, ~half the size of ADR-0073's first proof (no new
   compiler infra; just HIR pattern-match + synth-item insertion).
2. Then `hood` `.cb` wiring (paired sprint: hood manifest + cabi.rs trampoline mirror of
   pit's, exercised via `@cmd.handler` decorator form).
3. Then **Z.8 REST demo** (network MUST-ship §5) — `.cb` source: `@app.route` GET/POST
   over `den.connect(":memory:")` + `json.dumps` formatting → real demoable REST
   service.
