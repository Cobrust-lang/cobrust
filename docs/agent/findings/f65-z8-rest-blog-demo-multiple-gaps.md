---
finding_id: F65
title: Z.8 REST blog demo doesn't compile + multiple manifest/state gaps block end-to-end demo
status: candidate
date: 2026-05-28
discovered_by: P9 (Z.8 E2E harness retry sprint)
related_commit: a6ee367 (Z.8 demo draft) → discovered while authoring the E2E harness in z8_rest_blog_e2e.rs
sibling_findings: [F36, F37]
ratification_criteria: |
  Promote candidate→ratified once the demo compiles + serves a real HTTP request
  end-to-end (i.e., gaps G1-G5 below close in follow-up sprints) and the
  z8_rest_blog_e2e.rs primary tests pass without #[ignore].
---

# F65 — Z.8 REST blog demo `main.cb` has 5 distinct gaps blocking end-to-end execution

## Summary

The `examples/z8_rest_blog/main.cb` REST blog demo (committed in `a6ee367` as
the v0.7.0 §5 网络 MUST-ship example) fails type-checking with `UnknownMethod`
on the first `cobrust build` attempt. Beyond that surface failure, a deeper
audit reveals **five distinct gaps** between what the demo source ASSUMES is
wired and what the runtime / type system / ecosystem manifest currently expose.
Each gap is independently blocking — closing only the first surface
`UnknownMethod` would expose the next gap immediately.

The Z.8 E2E harness sprint (this finding's discovery context) thus ships its
primary E2E test for `examples/z8_rest_blog/main.cb` `#[ignore]`'d with this
finding cited; an inline "scaffolded variant" test proves the harness pattern
works once the gaps close.

## Reproduction

From a clean tree at HEAD `3530b49`:

```bash
LLVM_SYS_181_PREFIX=/opt/homebrew/opt/llvm@18 \
  cargo run -p cobrust-cli --bin cobrust -- build \
    examples/z8_rest_blog/main.cb -o /tmp/blog --quiet
```

Result:

```
cobrust build: type error: UnknownMethod {
  type_name: "Adt#3758097409",
  method_name: "body",
  span: Span { file: FileId(0), start: 5694, end: 5704 },
  suggestion: Some("this method is not on this ecosystem handle (den: Connection.execute, Cursor.fetchall)")
}
```

`Adt#3758097409` is `PIT_REQUEST_ADT` (`crates/cobrust-types/src/ecosystem.rs:85`).
The error confirms gap G1 below.

## Gaps

### G1 — `req.body()` is not in the `pit.Request` ecosystem manifest

`main.cb:34` calls `req.body()`. The `pit.Request` ADT has **no handle methods
at all** in `lookup_handle_method` (`crates/cobrust-types/src/ecosystem.rs`).
This is documented and ratified at the type-system layer:
`crates/cobrust-types/src/ecosystem.rs::pit_request_has_no_methods_today`
explicitly asserts `lookup_handle_method(&pit_request_ty(), "path").is_none()`,
calling out that "reading Request fields (path/method/body) lands in a paired
follow-up sprint along with the borrow shims" (ADR-0073 §5 / first proof scope).

The Rust `Request` struct DOES expose `.body() -> &[u8]` and `.json() ->
serde_json::Value` (`crates/cobrust-pit/src/request.rs:117-130`); the gap is
purely at the `.cb`-source ↔ manifest binding.

**Follow-up tier**: ADR-0073 follow-up sprint — add `Request.body()`,
`Request.method()`, `Request.path()`, `Request.path_param(name)` to
`lookup_handle_method`, with paired C-ABI shims in `cobrust-pit/src/cabi.rs`
returning Cobrust `Str` buffers. The Request borrow shim is the load-bearing
piece since Request is Rust-owned (ADR-0073 §2 D6).

### G2 — `app.run(host, port)` is not in the `pit.App` manifest

`main.cb:47` calls `app.run("127.0.0.1", 8080)`. The `pit.App` ADT exposes only
`route` and `serve_in_background` in `lookup_handle_method`
(`crates/cobrust-types/src/ecosystem.rs:498-513`). An explicit test
`assert!(lookup_handle_method(&pit_app_ty(), "run").is_none())`
(`ecosystem.rs:1031`) ratifies the gap.

Even though the underlying Rust `App::run(host, port)` exists
(`crates/cobrust-pit/src/app.rs:176`), the `.cb` surface only ships
`serve_in_background` (which is what the proven `pit_pong_e2e.rs` uses, with a
busy-wait `while i < 10000000000` keep-alive in `main`).

**Follow-up tier**: Trivial — add `(PIT_APP_ADT, "run")` to
`lookup_handle_method` returning `Ty::None` (matching the established `route ->
Ty::None` discipline to avoid double-drop), wire a `__cobrust_pit_app_run`
shim. Or — preferred — keep the surface clean with only `serve_in_background`
and edit the demo to match. Decision recorded in this finding's resolution
ADR.

### G3 — `:memory:` per handler-call yields a fresh empty database

`main.cb:22, 33` each call `den.connect(":memory:")` inside the handler.
SQLite's `:memory:` opens an **isolated, fresh DB** on every connect — there is
NO state shared across handler invocations. Even if G1+G2 closed, `list_posts`
would always see an empty result set; `create_post` would always be inserting
into a freshly-created connection that gets dropped at handler return.

The ADR-0072 first-proof manifest exposes only `den.connect(path) ->
Connection` + `Connection.execute(sql) -> Cursor` + `Cursor.fetchall() ->
str`. There is no `:memory:`-with-shared-cache idiom wired (`?cache=shared` in
the URI would work for SQLite-native code but the den manifest takes a raw
path string, not URI semantics), nor a Cobrust-side App-scoped connection
fixture pattern.

**Follow-up tier**: Either (a) demo switches to a per-process tempfile path
(`/tmp/z8_blog.db`) with explicit `CREATE TABLE IF NOT EXISTS posts` in
`main()` before `serve_in_background`, or (b) `den` exposes a long-lived
connection handle pattern compatible with the trampoline closure capture
(harder — Connection is `!Send` per ADR-0072 §5 risk 2; would need an
Arc<Mutex<Connection>> wrapper at the ecosystem layer).

### G4 — Missing `CREATE TABLE posts (...)` in startup

`main.cb` references `posts` in SELECT / INSERT statements but never CREATEs
the table. Even closing G3 (e.g., via per-process tempfile) would surface
"no such table: posts" at the first request.

**Follow-up tier**: Trivial — edit `main()` to call `conn.execute("CREATE TABLE
IF NOT EXISTS posts (...)")` before `serve_in_background`. Pairs naturally
with G3's resolution.

### G5 — Demo only implements `list_posts` + `create_post`; harness done-means requires `GET /posts/<id>` + `DELETE /posts/<id>`

`README.md:25-26` explicitly says "demo 第一版只做 list + create, 不做 by-id
GET". The Z.8 E2E harness dispatch's done-means table requires:
- `POST /posts` ✓ (demo has it)
- `GET /posts/<id>` ✗ (demo does not)
- `GET /posts` ✓ (demo has it)
- `DELETE /posts/<id>` ✗ (demo does not)
- `GET /posts/<id>` after DELETE → 404 ✗

So the demo and the harness done-means are not co-designed; the harness
authoring sprint discovers this scope mismatch.

**Follow-up tier**: Expand demo to include by-id GET + DELETE handlers once
G1-G4 close (path params via `req.path_param("id")` are wired in
`crates/cobrust-pit/src/request.rs:73` at the Rust level but NOT at the `.cb`
surface — see G1).

## Why this slipped through

Sibling F36 (fixture name vs behavior drift) — the demo file's filename +
title promise "REST blog" but the body doesn't deliver a working REST blog;
no test exercises the demo, so the gap stayed silent until the E2E harness
sprint forced a real compilation attempt. Sibling F37 (silent rot on
accepted debt) — the README's "等所有依赖在 origin 绿后" hedge in the run-
instructions section deflected the gap-audit obligation; the demo was
committed without anyone trying `cobrust build` on it.

The discovered-while-authoring pattern is the canonical right route: a
follow-up E2E harness sprint surfaces the demo gaps the demo-authoring sprint
didn't notice. F65 ratifies this as a sibling rot pattern (committed example
file without a paired smoke test).

## Resolution plan

The fastest path to closing F65 is a paired sprint:

1. **Demo repair sprint** (closes G1-G5 together):
   - Add `Request.body()` / `Request.method()` / `Request.path()` /
     `Request.path_param(name)` to pit manifest + C-ABI shims (closes G1).
   - Either drop `app.run` from the demo in favor of `serve_in_background`
     (preferred), or add `app.run` to manifest (closes G2).
   - Switch demo to per-process tempfile path + explicit `CREATE TABLE IF
     NOT EXISTS posts` in `main` before serve (closes G3+G4).
   - Add `GET /posts/<id>` + `DELETE /posts/<id>` handlers using path-param
     surface (closes G5).
2. **E2E re-enable sprint** (removes `#[ignore]` from this finding's tests):
   - Re-run `crates/cobrust-cli/tests/z8_rest_blog_e2e.rs` after demo
     repair.
   - Remove the `#[ignore = "finding:f65-..."]` markers.
   - Update F65 status candidate → ratified.

ETA: 2-3 sprints (G1 is the load-bearing piece; G2-G5 are mechanical once G1
lands).

## Test artifact

`crates/cobrust-cli/tests/z8_rest_blog_e2e.rs` ships:
- `test_e2e_z8_demo_full_round_trip` (`#[ignore]` — cites F65 G1+G2+G3+G4)
- `test_e2e_z8_demo_compiles` (`#[ignore]` — cites F65 G1+G2: even the build
  fails today)
- Inline "scaffolded variant" tests that DO pass today, proving the harness
  pattern (compile a minimal pit-only `.cb` source + port-baked-in + real
  HTTP round-trip) works once the demo gaps close.

## Cross-references

- ADR-0073 (pit first proof) — manifest scope
- ADR-0072 (den first proof) — `:memory:` per-call semantics
- ADR-0074 (decorator desugar) — landed, decorator form is the v0.7.0 §5
  follow-up after demo repair
- Sibling F36 (fixture-name vs behavior drift), F37 (silent rot on accepted
  debt)
