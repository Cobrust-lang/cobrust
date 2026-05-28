# Z.8 REST demo — `examples/z8_rest_blog/`

v0.7.0 §5 网络 MUST-ship 演示落地: a minimal REST blog service that wires
**pit (HTTP/axum) + den (SQLite/rusqlite)** end-to-end in a single
`.cb` source. F65 closed the 5 demo gaps; the demo now compiles + runs
+ passes its E2E harness.

## 状态

**Live as of F65 resolution sprint** (2026-05-29). 4/4 E2E tests in
`crates/cobrust-cli/tests/z8_rest_blog_e2e.rs` pass without
`#[ignore]`. Underlying chains:

- ADR-0073 pit "pong" first proof (commit `5153b35` + `8a3e8bf`);
- ADR-0072 den first proof (commit `b5b7318`);
- F65 G1 — `Request.body() -> str` + `Request.path_param(name) -> str`
  borrow shims;
- F65 G2 — `App.run(host, port) -> i64` blocking serve;
- F65 G3 / G4 — file-backed SQLite (`/tmp/z8_blog.sqlite3`) +
  `DROP TABLE IF EXISTS` + `CREATE TABLE` schema init in `main()`;
- F65 G5 — by-id GET + DELETE handlers using path-param shim.

## Routes

| Method | Path           | Behavior                                                       |
|--------|----------------|----------------------------------------------------------------|
| GET    | `/posts`       | 200 + JSON array `[{id,title,body},...]`                       |
| POST   | `/posts`       | 201 + JSON `{id,title,body}`, INSERTs the body's title/body    |
| GET    | `/posts/<id>`  | 200 + JSON `{id,title,body}`, or 404 + `{"error":"not found"}` |
| DELETE | `/posts/<id>`  | 204 (idempotent: deleting a missing id still 204)              |

POST request shape: `{"title":"X","body":"Y"}` — exact flat JSON shape
only (structured-dict JSON lands with the coil-deep type work).
Validation guards: empty title rejects with 400 + `{"error":"title is
required"}`; malformed body rejects with 400 +
`{"error":"invalid json body"}`.

## 跑法

Build + run on the default port 8080:

```bash
LLVM_SYS_181_PREFIX=/opt/homebrew/opt/llvm@18 \
  cargo run -p cobrust-cli --bin cobrust -- build \
    examples/z8_rest_blog/main.cb -o /tmp/blog

/tmp/blog &
SERVER=$!

curl -s http://127.0.0.1:8080/posts                                    # []
curl -s -X POST http://127.0.0.1:8080/posts \
  -H 'Content-Type: application/json' \
  -d '{"title":"hello","body":"world"}'                                # {"id":1,...}
curl -s http://127.0.0.1:8080/posts/1                                  # {"id":1,...}
curl -s -X DELETE http://127.0.0.1:8080/posts/1                        # (empty 204)
curl -s -w '%{http_code}' http://127.0.0.1:8080/posts/1; echo          # 404

kill $SERVER
```

The demo also accepts an explicit port from argv (for E2E harnesses
running in parallel):

```bash
/tmp/blog 9099   # binds 127.0.0.1:9099 instead of 8080
```

## E2E test

`crates/cobrust-cli/tests/z8_rest_blog_e2e.rs` ships four tests:

- `test_e2e_z8_demo_compiles` — floor smoke test (`cobrust build` passes).
- `test_e2e_z8_demo_full_round_trip` — full round-trip against the
  expected POST→GET→GET-list→DELETE→GET-404 sequence.
- `test_e2e_z8_harness_pattern_proof_inline` — pit-only minimal
  scaffolded harness (proves the harness shape works without the den
  + body-parse chain).
- `test_e2e_z8_harness_method_mismatch_returns_404` — negative-sanity:
  POST on a GET-only path + GET on a POST-only path → 404.

Run with:

```bash
cargo test -p cobrust-cli --test z8_rest_blog_e2e
```

## 已知限制 (post-F65 follow-ups)

1. `den.Connection.execute(sql)` takes a bare SQL string (no
   parameterised `?` placeholders), so the POST handler builds SQL via
   `replace` placeholder substitution. Single-quote escaping in user
   input is NOT done — first-proof scope. A parameterised-statement
   API + escape pass is the canonical follow-up.
2. `cur.fetchall()` returns den's canonical Python-tuple-list str
   render (`[(1, 'hello', 'world')]`). The demo strips the tuple
   wrapping via `replace` to produce a real JSON array; a future
   `fetchall_json()` / `fetchall_rows()` shape would obviate this.
3. JSON request body parsing accepts ONLY the exact flat shape
   `{"title":"X","body":"Y"}` (replace + split decomposition). When
   coil-deep type work lands a real `dict[str, str]` surface on
   `json_loads`, the handler can do `let body = json_loads(req.body());
   let title = body["title"]`.
4. F-string lexer doesn't accept `\"` inside `{...}` interpolations;
   the response JSON uses helper variables `let q1 = "\""` etc. A
   lexer follow-up (`\` escape inside f-string interior) is queued.
5. `Connection` is `!Send` per ADR-0072 §5 risk 2, so handlers cannot
   capture a single shared Connection across requests — each handler
   does its own `den.connect("/tmp/z8_blog.sqlite3")`. SQLite's
   committed-state semantics make this work; a future
   `Arc<Mutex<Connection>>` ecosystem wrapper would enable shared-
   handle patterns (and connection pooling).
6. Decorator form (`@app.route("GET", "/posts")`) is supported by
   ADR-0074 for single-method routes; multi-route + DELETE-/HEAD-/
   PATCH-method decorator forms are a follow-up. The demo uses
   explicit `app.route(method, path, handler)` calls for now to keep
   the four-route + path-param surface uniform.

## 后续 sprint

- Real parameterised SQL on den (`execute(sql, params)`).
- Structured `dict[str, str]` returns from `json_loads` for body
  parsing without `replace` + `split`.
- Decorator form rewrite once ADR-0074 generalizes to all four
  routes.
