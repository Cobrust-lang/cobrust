---
doc_kind: finding
finding_id: F68
title: ADR-0076 Phase 1 follow-ups — @dora.node decorator desugar gap + Event-id equality demo simplification
status: ratified
date: 2026-05-29
last_verified_commit: 971d4ce
relates_to: [adr:0074, adr:0076, finding:F35, finding:F36]
resolution_commit: 971d4ce
resolves: ["F68 §1 — @dora.node module-receiver decorator desugar"]
---

> **RESOLVED 2026-05-29 (§1 — decorator desugar gap).** The
> `@dora.node(inputs=[...], outputs=[...])` module-receiver decorator now
> desugars to a synthetic `dora.node(<handler>)` register-call. The
> resolution is **HIR-only** (extends ADR-0074's decorator desugar to
> module-alias receivers) — ZERO mir / codegen / dora-runtime change,
> ZERO `cobrust-types/src/ecosystem.rs` manifest change. See §6 below for
> the resolution detail. §2 (string-equality demo dispatch) remains a
> Phase 2 follow-up tracked here and is NOT resolved by this sprint.

# F68: ADR-0076 Phase 1 follow-ups (decorator-desugar gap + demo simplifications)

## Summary

ADR-0076 §Q4 picked the decorator-form `@dora.node(inputs=[...],
outputs=[...])` as the canonical user surface for handler registration.
Phase 1 ships with the **explicit form** `let _ = dora.node(detect)`
instead — extending ADR-0074's decorator desugar to recognize
**module-receiver decorators** (where the decorator's receiver is the
`dora` module alias, not a let-bound handle like `app`) is out of
Phase 1 scope and would compound scope creep. The Phase 1 manifest
ships `dora.node(handler)` as a module-level free fn with
`EcoParam::Callback`; this is functionally complete and proves the
callback chain end-to-end, but the user surface is one rung less
ergonomic than the ADR-promised decorator form.

This finding tracks two related Phase 1 follow-ups:

1. **`@dora.node` decorator desugar gap** — Phase 2 prerequisite.
2. **Demo simplification** — Phase 1 demo avoids `if event.id() ==
   "camera":` string-equality dispatch (would require `str_eq_lit`
   wiring at the user level); deferred to Phase 2 alongside multi-IO.

## 1. `@dora.node` decorator desugar gap

### Current state (Phase 1)

ADR-0074 §2 `is_ecosystem_decorator_shape` recognizes exactly TWO
decorator shapes today:

```rust
fn is_decoratable_call_method(name: &str) -> bool {
    matches!(name, "route")     // @app.route(...) for pit
}
fn is_decoratable_bare_method(name: &str) -> bool {
    matches!(name, "handler")   // @cmd.handler for hood
}
```

Both shapes assume the receiver is a **let-bound handle** (`app =
pit.App()` or `cmd = hood.Command(...)`). Peeling the decorator
expression in `peel_eco_decorator` requires `base_expr.kind ==
ast::ExprKind::Name(_)` AND the name must resolve to a `LetBinding`.
The HIR `inject_pending_eco_decorators` post-pass walks the lowered
`fn main()` body to find that let-binding and synthesise the
`app.route("GET", "/x", handler)` register-call.

`@dora.node(inputs=["camera"], outputs=["det"])` doesn't fit this
shape. Its receiver is the **`dora` module alias** (a `DefKind::
ImportAlias`, not a `LetBinding`), and the call has **keyword args**
(`inputs=...`, `outputs=...`) the existing desugar peeler explicitly
rejects per ADR-0074 §2 Q4 keyword-args defer.

### Phase 2 desugar extension design

Phase 2 needs the following ADR-0074 amendments:

| Change | Where | Risk |
|---|---|---|
| Add `"node"` to `is_decoratable_call_method` | `cobrust-hir/src/lower.rs` (1 line) | low — additive |
| Recognise module-alias receivers in `peel_eco_decorator` | `cobrust-hir/src/lower.rs` (~20 lines) — accept `base_name` whose def-kind is `ImportAlias` of an ecosystem module | medium — branches the receiver resolution; need a corresponding `inject_pending_eco_decorators` arm that builds a **free-fn call** synth (`dora.node(handler)`) instead of a method call (`app.route(...)`) |
| Thread keyword args through `build_eco_register_call` | `cobrust-hir/src/lower.rs` (~30 lines) — currently call-args-only | medium — need to extend `lower_eco_decorator_arg` to handle list-literal args (`["camera"]`) for inputs/outputs; this may surface ListExpr lowering issues |
| Manifest `dora.node` signature widens to accept inputs/outputs lists | `cobrust-types/src/ecosystem.rs` — current shape is `EcoParam::Callback(...)` single-arg; extend to `[Value(list[str]), Value(list[str]), Callback(...)]` | medium — Phase 2 sprint should land manifest + desugar together |

### Phase 2 user-facing target

```python
import dora

@dora.node(inputs=["camera"], outputs=["detections"])
fn detect(event: dora.Event) -> i64:
    let frame: str = event.data_str()
    print(frame)
    return 0

fn main() -> i64:
    let node = dora.Node("detector")
    return node.run()
```

HIR-desugared form (synthesised inside `fn main()` body before
`node.run()`):

```python
let __dora_decl_detect = dora.node(["camera"], ["detections"], detect)
```

### Why not extend ADR-0074 in Phase 1?

Three reasons:

1. **Scope creep risk** — the dispatch brief explicitly warned about
   this: *"If scope creep surfaces (e.g. ADR-0074 desugar doesn't
   handle the @dora.node decorator shape cleanly), file F68 candidate
   + work around it (e.g. fall back to explicit `dora.node(...)(detect)`
   form in the demo) so Phase 1 still ships."*
2. **Module-receiver decorators are a new shape** (vs let-binding
   receivers) — would deserve its own audit + design pass under
   ADR-0074 amendment.
3. **The chain is proven without it.** The Phase 1 explicit form
   `let _ = dora.node(detect)` exercises the same MIR / codegen /
   trampoline path as the future decorator form would. Adding the
   decorator desugar is pure HIR-layer sugar that lands cleanly atop
   the proven Phase 1 chain.

## 2. Demo simplification — string-equality dispatch deferred

### Current state (Phase 1 demo)

The ADR-0076 spec §Q4 example shows:

```python
@dora.node(inputs=["camera"], outputs=["detections"])
fn detect(event: dora.Event) -> i64:
    if event.id() == "camera":
        let frame: str = event.data_str()
        print(f"got frame: {frame}")
    return 0
```

The Phase 1 demo at `examples/dora_hello/main.cb` ships a **simpler
form** that always handles the message:

```python
fn detect(event: dora.Event) -> i64:
    let frame: str = event.data_str()
    print_no_nl("got frame: ")
    print(frame)
    return 0
```

This avoids:

- `if event.id() == "camera":` — requires `str` == `str_literal`
  comparison at the source-language level. Cobrust today does this
  via the explicit `str_eq_lit(s, "lit") == 1` form (see
  `examples/leetcode-stress/020-twoptr-backspace-compare/solution.cb`);
  the natural `==` operator on str is a Phase G+ language-level
  surface follow-up tracked elsewhere.
- `f"got frame: {frame}"` f-string — f-strings work in Cobrust today,
  but combining them with the runtime str (`event.data_str()`)
  through the formatter's existing `__cobrust_fmt_str` path adds one
  more chain link to verify. Replacing with `print_no_nl + print`
  keeps the demo's Phase-1 surface to ZERO unfamiliar primitives.

### Phase 2 demo target

Phase 2 wires the decorator desugar + the multi-input handler
dispatch and can return to the canonical:

```python
@dora.node(inputs=["camera", "lidar"], outputs=["detections"])
fn detect(event: dora.Event) -> i64:
    if event.id() == "camera":
        let frame: str = event.data_str()
        print(f"got frame: {frame}")
    if event.id() == "lidar":
        let scan: str = event.data_str()
        print(f"got lidar scan: {scan}")
    return 0
```

at which point the `if event.id() == "...":` dispatch is the entire
**point** of the multi-input demo (single-input Phase 1 has nothing
to dispatch on).

## 3. Status promotion criteria

This finding promotes from `candidate → ratified` when Phase 2 dispatch
either:

- (a) extends ADR-0074 for module-receiver decorators + manifest-side
  inputs/outputs lists, OR
- (b) ADR-0076 Phase 2 sprint dispatch explicitly accepts a
  module-receiver-decorator amendment scope and dispatches it as a
  paired ADR-0074 amendment.

If Phase 2 instead chooses to permanently ship the explicit-form
`dora.node(handler)` shape and DROP the decorator from ADR-0076 §Q4,
this finding promotes to `ratified` with a §"design pivot" note
recording the surface change rationale.

**OUTCOME (2026-05-29): path (a), HIR-only variant.** ADR-0074's desugar
was extended to module-receiver decorators (`@dora.node`). The
manifest-side inputs/outputs widening was **deferred** (manifest is the
Phase 2 dataflow-graph wiring's concern): the `inputs=`/`outputs=` kwargs
are validated as list-of-str literals at the desugar layer, then
**dropped**, so the synthesised call is a single-arg `dora.node(handler)`
— byte-identical to the explicit form, reusing the unchanged Phase 1
manifest row + runtime. This keeps the user surface at the ADR-0076 §Q4
idiomatic decorator form while the synthetic Phase 1 runtime stays
single-handler. The metadata becomes load-bearing when Phase 2 wires the
real `dora-node-api` dataflow graph + widens the manifest row.

## 4. Related findings

- **F35 (commit-msg vs diff drift)** — separate concern, not implicated
  here; the Phase 1 commit message scoped accurately to Phase 1
  surface (explicit form, no decorator) per the dispatch brief.
- **F36 (fixture-name vs behavior drift)** — the Phase 1 manifest test
  names (`dora_node_free_fn_carries_callback_slot`) match the actual
  shape tested (`dora.node` as a `lookup_module_fn` row with
  `EcoParam::Callback`); no drift.

## 5. Evidence

- `crates/cobrust-hir/src/lower.rs:2025-2060` (the existing
  `is_ecosystem_decorator_shape` predicate scope).
- `docs/agent/adr/0076-dora-cb-stream-y.md` §Q4 (the decorator-form
  decision).
- `docs/agent/adr/0074-cb-ecosystem-decorator-desugar.md` (the
  decorator desugar machinery).
- `examples/dora_hello/main.cb` (the Phase 1 explicit-form demo).
- `crates/cobrust-cli/tests/dora_hello_e2e.rs` (the Phase 1 E2E
  test asserting `got frame: frame_001` stdout).

## 6. §1 resolution detail (2026-05-29)

### What changed (HIR-only)

All in `crates/cobrust-hir/src/lower.rs`:

| Element | Role |
|---|---|
| `is_decoratable_module_method("node")` (new free fn) | structural recognition of the module-receiver method name; `is_ecosystem_decorator_shape` now ORs it into both the call-form and bare-form branches |
| `validate_module_node_decorator_shape` (new free fn) | shape gate for `@dora.node(...)`: rejects positional args (handler is the decorated fn), non-`inputs`/`outputs` kwargs, non-list / non-str-literal port values, `*args`/`**kwargs` — each with a §2.5 Direction B fix-suggesting diagnostic; accepts the bare `@dora.node` form |
| `build_eco_module_register_call` (new free fn) | synthesises a MODULE-FN call `dora.node(<fn_ref>)` whose receiver `Name` carries the import-alias `DefId` + `DefKind::ImportAlias` (the SAME id the typechecker registers in `ecosystem_module_defs`), so `try_synth_ecosystem_call` Case 1 (module free-fn) fires — NOT Case 2 (handle method). The `inputs=`/`outputs=` kwargs are dropped. |
| `inject_pending_eco_decorators` fork | resolves the receiver name against the module (top-level) scope; if it is an `ImportAlias` of a known ecosystem module + a module-decoratable method → module-fn synth at `main`'s **prologue** (index 0, so the handler installs before `node.run()` dispatches). Otherwise → the pre-existing handle-method synth (`let <recv>`-scan). |

`crates/cobrust-hir/src/error.rs` — **NO change.** The existing
`EcosystemDecoratorShape { detail, span, suggestion }` variant's
free-text fields absorbed every module-receiver diagnostic; no new
sub-variant was needed.

### How module-receiver recognition differs from handle-receiver

- **Handle receiver** (`@app.route(...)`): receiver is a `DefKind::
  LetBinding` (`app = pit.App()`). The post-pass scans `fn main()`'s
  body for `let app = ...`, resolves its `DefId`, and synthesises a
  handle-METHOD call `app.route("GET", "/x", handler)` inserted right
  after the `let`. Routes through `try_synth_ecosystem_call` **Case 2**.
- **Module receiver** (`@dora.node(...)`): receiver is a `DefKind::
  ImportAlias` (`import dora`). The post-pass resolves the alias `DefId`
  via `lookup_top_level("dora")` (no `let` scan), and synthesises a
  module-FN call `dora.node(handler)` inserted at `main`'s prologue.
  Routes through `try_synth_ecosystem_call` **Case 1**.

### Chain-generality (the §"why not Phase 1" prediction held)

`git diff --stat` over `crates/cobrust-mir/`, `crates/cobrust-codegen/`,
`crates/cobrust-dora/` is **EMPTY** — 0/0/0. Pure HIR sugar atop the
proven ADR-0073 callback chain + ADR-0076 Phase 1 synthetic runtime, as
§1 "Why not extend ADR-0074 in Phase 1?" point 3 predicted.

### Tests

- `crates/cobrust-cli/tests/decorator_dora_e2e.rs` (NEW): 2 positive
  (full `@dora.node(inputs=..., outputs=...)` form + bare `@dora.node`
  form, both print `got frame: frame_001` + exit 0) + 4 negative
  (nested-fn → "module scope"; wrong handler sig →
  `CallbackSignatureMismatch`; positional arg → rejected; non-list
  `inputs=` → rejected).
- `examples/dora_hello/main.cb` switched explicit-form
  `let _ = dora.node(detect)` → decorator-form
  `@dora.node(inputs=["camera"], outputs=["detections"])`.
- No regression: `dora_hello_e2e` (explicit form still supported),
  `decorator_pit_e2e`, `pit_pong_e2e`, `hood_cmd_e2e` all green.
