---
finding_id: F65
title: Z.8 REST blog demo doesn't compile + multiple manifest/state gaps block end-to-end demo
status: ratified_2026-05-29
date: 2026-05-28
last_verified_commit: head-of-branch (F65 resolution commit; circular SHA-pin issue resolved by referencing the commit's title)
discovered_by: P9 (Z.8 E2E harness retry sprint)
resolved_by: P9 (F65 resolution sprint, 2026-05-29)
related_commit: a6ee367 (Z.8 demo draft) → discovered while authoring the E2E harness in z8_rest_blog_e2e.rs
sibling_findings: [F36, F37]
ratification_criteria: |
  Promote candidate→ratified once the demo compiles + serves a real HTTP request
  end-to-end (i.e., gaps G1-G5 below close in follow-up sprints) and the
  z8_rest_blog_e2e.rs primary tests pass without #[ignore].
ratification_evidence: |
  - examples/z8_rest_blog/main.cb compiles via `cobrust build` (LLVM backend);
  - 4/4 tests in crates/cobrust-cli/tests/z8_rest_blog_e2e.rs pass live,
    NONE ignored;
  - manual `curl` smoke (see §Resolution below) confirms the full
    POST→GET-by-id→GET-list→DELETE→GET-404 sequence.
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

## Resolution (2026-05-29, F65 demo-repair sprint — single sprint, not paired)

The two phases were collapsed into one sprint because the manifest-side
work + demo rewrite + harness re-enablement all touched the same files.

### G1 — Request.body() (+ path_param) shipped
- `crates/cobrust-pit/src/cabi.rs` — new `__cobrust_pit_request_body` +
  `__cobrust_pit_request_path_param` shims (the alloc_str_buffer helper
  graduated from `#[cfg(test)]` to production since the shim produces a
  Cobrust Str buffer the .cb caller owns).
- `crates/cobrust-types/src/ecosystem.rs` — `Request.body()` +
  `Request.path_param(name)` rows in `lookup_handle_method`. The
  ratified `pit_request_has_no_methods_today` test was replaced by
  positive `pit_request_body_method_returns_str` +
  `pit_request_path_param_method_takes_name_returns_str` tests; the
  `pit_request_still_has_no_drop_symbol` test preserves the Rust-
  ownership invariant (ADR-0073 §2 D6 — Request still does NOT get a
  .cb drop schedule).
- `crates/cobrust-codegen/src/llvm_backend.rs` — extern decls for the
  two new symbols, mirroring the existing pit decl block.

ADR-0073 §5 follow-ups for `Request.method()` / `Request.path()` are
NOT in F65 scope (the demo only needs `body` + `path_param`); they
land when the next proof needs them.

### G2 — App.run shipped
- `crates/cobrust-pit/src/cabi.rs` — new `__cobrust_pit_app_run` shim
  using the same `mem::take` App-interior consume pattern as
  `serve_in_background` (so the .cb scope-exit `_drop` still frees a
  clean empty App).
- `crates/cobrust-types/src/ecosystem.rs` — `App.run(host: str, port:
  i64) -> i64` row. The hood cross-handle isolation test
  (`hood_methods_only_match_command_receiver`) had been asserting
  `lookup_handle_method(&pit_app_ty(), "run").is_none()` as a side
  invariant; F65 graduates `App.run` to a real pit method, so the
  invariant test was rewritten to probe `handler` (hood-only, not in
  pit) instead — preserves the isolation proof.
- `crates/cobrust-codegen/src/llvm_backend.rs` — extern decl
  (`i64_ty.fn_type(&[ptr_ty, ptr_ty, i64_ty], false)`).

### G3 + G4 — file-backed DB + table init
- `examples/z8_rest_blog/main.cb` — schema (re-)created in `main()`
  via `den.connect("/tmp/z8_blog.sqlite3")` + `DROP TABLE IF EXISTS`
  + `CREATE TABLE`. Each handler reopens its own Connection to the
  same path (Connection is `!Send` per ADR-0072 §5 risk 2; pit's
  trampoline requires `Send + Sync + 'static`, so we can't capture a
  shared Connection — but SQLite's file-backed committed-state
  semantics make reopen-per-call equivalent for the demo's load).

### G5 — by-id GET + DELETE
- Path params consumed via the F65 G1 sibling `req.path_param("id")`
  shim. Two new handlers + two new `app.route(...)` calls. The demo's
  E2E harness exercises POST → GET-by-id → GET-list → DELETE →
  GET-by-id-404 covering all four routes plus the negative.

### Sub-finding NOT filed (path-param surface IS supported)

The dispatch spec hedged: "If pit doesn't support `<id>` path params
today, that's an additional architectural gap (F67-candidate). In
that case, use query-string fallback." It turns out pit's underlying
Rust `Request::path_param(name)` IS wired (the routing engine
captures `<name>` segments into `path_params`); only the `.cb`
manifest binding was missing. F65 G1's `path_param` shim addition
closes the surface gap entirely, so NO F67-candidate is filed.

### Demo encoding workarounds (queued as follow-ups, NOT scope creep)

These five quirks the demo navigates were discovered IN-FLIGHT and
are documented in the demo's header + README §Known limitations:

1. F-string lexer does not accept `\"` inside braced interpolations
   — the demo uses `let qN = "\""` helper variables for response
   JSON building. F-string-escape sub-sprint is queued (separate
   finding when its proof case demands closure).
2. `den.execute(sql)` takes a bare string with no `?` params — the
   demo substitutes via `replace("ID_PLACEHOLDER", id)` etc. A
   parameterised SQL API on den is the canonical follow-up.
3. `cur.fetchall()` returns canonical Python-tuple-list str render
   `[(N, 'X', 'Y')]` — the demo strips the wrapping via three
   `replace`s back into JSON. A `fetchall_json()` / `fetchall_rows()`
   shape is the follow-up.
4. JSON body parsing accepts ONLY the exact flat shape
   `{"title":"X","body":"Y"}` (via replace + split). Real structured
   `dict[str, str]` json_loads needs the coil-deep type work.
5. PRELUDE str fns consume by Move; for multi-use variables the demo
   uses `&var` explicit-shared-borrow shortcut (ADR-0052a) — the
   canonical LLM-first §2.5-A pattern. No `clone` calls remain in the
   demo (per the constitution's "compile-time-catch + training-data-
   overlap" rule).

### Test artifact — UN-IGNORED

`crates/cobrust-cli/tests/z8_rest_blog_e2e.rs` ships four LIVE tests
(0 ignored):
- `test_e2e_z8_demo_compiles` — floor smoke (cobrust build passes).
- `test_e2e_z8_demo_full_round_trip` — full POST→GET-by-id→GET-list→
  DELETE→GET-404 round-trip against the real demo binary.
- `test_e2e_z8_harness_pattern_proof_inline` — pit-only minimal
  scaffolded harness (regression floor — if pit's chain breaks while
  the demo's storage layer is also broken, this test fires first).
- `test_e2e_z8_harness_method_mismatch_returns_404` — negative-sanity
  (GET-on-POST-only + POST-on-GET-only → 404 / 405).

The harness invokes the demo with `port` as `argv[1]` (the demo
falls back to 8080 when absent, for the standalone-runnable README
flow). Each test run picks an ephemeral port to avoid parallel-run
collisions.

### Resolution commit

The commit whose title is `fix(pit+demo): F65 resolution — Z.8 demo
lives (req.body + app.run + tempfile + table init + by-id)` —
referenced by title rather than by SHA to avoid the circular pin
issue (amending the commit to record its own SHA changes the SHA).
Locate via `git log --oneline --grep='F65 resolution'`.

## Test artifact

(see Resolution above — `crates/cobrust-cli/tests/z8_rest_blog_e2e.rs`
ships 4 LIVE tests, 0 ignored. The "scaffolded variant" tests stay
as the regression floor below the primary demo round-trip.)

## Cross-references

- ADR-0073 (pit first proof) — manifest scope
- ADR-0072 (den first proof) — `:memory:` per-call semantics
- ADR-0074 (decorator desugar) — landed, decorator form is the v0.7.0 §5
  follow-up after demo repair
- Sibling F36 (fixture-name vs behavior drift), F37 (silent rot on accepted
  debt)
