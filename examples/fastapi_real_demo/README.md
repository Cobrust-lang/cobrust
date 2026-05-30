# FastAPI-real capstone demo — `examples/fastapi_real_demo/`

A single, real, fully **type-driven validated REST API** in `.cb` that
exercises the **whole #156 Phase-1 surface together** (ADR-0080 +
ADR-0081). The per-feature unit E2Es each pass in isolation; this demo is
the **completeness-critic check** that proves they **compose** in one
running server.

## What it demonstrates (every Phase-1 feature, in ONE handler)

| Feature | Where in the demo | ADR |
|---|---|---|
| Validated body `class` with typed fields | `class CreateUser` | 0080 |
| **Int-range** refinement | `age: i64 where 0 <= self and self <= 150` | 0080 |
| **String-length** refinement | `name: str where 1 <= len(self) and len(self) <= 50` | 0080 |
| **String-pattern** refinement | `email: str where pattern(self, ".+@.+")` | 0080 |
| Typed `route_validated` handler (2-arg) | `fn create_user(req, body: CreateUser)` | 0080 |
| Runtime body **field read** | `let a: i64 = body.age` | 0081 |
| Body re-serialise (echo) | `pit.json_response(201, body)` | 0081 |
| Business-rule branch on a read field | `if a >= 18` → 201 else `text_response(403, ...)` | 0081 |
| Auto **OpenAPI** schema (cannot drift) | `app.serve_openapi("/openapi.json")` | 0080 |

The key elegance property (the elegance-law, no legacy debt):

- **Structure is caught at compile time.** Field presence + field type are
  the `class` field table — a typo'd `body.aeg` is a *compile error*, not a
  runtime `KeyError`. You cannot ship a handler that reads a field that
  isn't there.
- **Value constraints are ONE boundary guard → a typed 422.** `range` /
  `length` / `pattern` run once, at the request boundary, and render a
  `Result` to a 422 — never a thrown exception, never an in-handler
  re-check.
- **`body.age` is a typed read**, never a stringly-typed `body["age"]`.
- **The OpenAPI schema is a projection of the same field table** the
  validator reads — there is no second, hand-kept schema to fall out of
  sync (the utoipa / drf-spectacular drift footgun, dropped).

## The API

```python
class CreateUser:
    name:  str where 1 <= len(self) and len(self) <= 50
    age:   i64 where 0 <= self and self <= 150
    email: str where pattern(self, ".+@.+")

fn create_user(req: pit.Request, body: CreateUser) -> pit.Response:
    let a: i64 = body.age                       # runtime field read
    if a >= 18:
        return pit.json_response(201, body)     # echo the validated body
    return pit.text_response(403, "must be 18 or older")   # business-rule branch
```

| Method | Path             | Behavior |
|--------|------------------|----------|
| POST   | `/users`         | validate body → if `age >= 18` then **201** + echoed JSON, else **403**; any refinement violation → **422** |
| GET    | `/openapi.json`  | **200** + the OpenAPI 3.1 doc derived from `CreateUser` |

Request shape: `{"name":"...","age":N,"email":"..."}`. Every declared field
must be present, the right type, and satisfy its refinement — or the boundary
returns 422 and the handler is never entered.

## Behavior at the boundary

- **valid adult** `{"name":"Ada","age":42,"email":"ada@x.com"}` → **201** +
  body `{"age":42,"email":"ada@x.com","name":"Ada"}`, `content-type:
  application/json`. (all three refinements pass; `json_response` echoes the
  validated body)
- **valid minor** `{"name":"Kid","age":15,"email":"kid@x.com"}` → **403**
  `must be 18 or older`. (15 passes validation, so this is the *business-rule*
  branch — it proves `body.age` was genuinely read at runtime; an un-read
  branch could not return both 201 for an adult and 403 for a minor)
- **too-long name** (51 chars) → **422** (string-length), handler not entered.
- **age 200** (> 150) → **422** (int-range), handler not entered.
- **email "nope"** (no `@`) → **422** (string-pattern), handler not entered.
- `GET /openapi.json` → **200** + a schema showing
  `name: {type:string, minLength:1, maxLength:50}`,
  `age: {type:integer, minimum:0, maximum:150}`,
  `email: {type:string, pattern:".+@.+"}`.

## How to run

Build + run on the default port 8080:

```bash
LLVM_SYS_181_PREFIX=/opt/homebrew/opt/llvm@18 \
  cargo run -p cobrust-cli --bin cobrust -- build \
    examples/fastapi_real_demo/main.cb -o /tmp/users

/tmp/users &
SERVER=$!

# valid adult → 201 + echoed body
curl -s -X POST http://127.0.0.1:8080/users \
  -H 'Content-Type: application/json' \
  -d '{"name":"Ada","age":42,"email":"ada@x.com"}'
# {"age":42,"email":"ada@x.com","name":"Ada"}

# valid minor → 403 (business rule on the read age field)
curl -s -w ' [%{http_code}]\n' -X POST http://127.0.0.1:8080/users \
  -H 'Content-Type: application/json' \
  -d '{"name":"Kid","age":15,"email":"kid@x.com"}'
# must be 18 or older [403]

# refinement violations → 422 (handler never entered)
curl -s -w ' [%{http_code}]\n' -X POST http://127.0.0.1:8080/users \
  -H 'Content-Type: application/json' \
  -d '{"name":"Ada","age":200,"email":"a@x.com"}'   # age out of range → [422]

# the derived OpenAPI doc
curl -s http://127.0.0.1:8080/openapi.json
# {"openapi":"3.1.0","components":{"schemas":{"CreateUser":{...}}},...}

kill $SERVER
```

The demo accepts an explicit port from argv (for E2E harnesses running in
parallel):

```bash
/tmp/users 9099   # binds 127.0.0.1:9099 instead of 8080
```

## E2E test

`crates/cobrust-cli/tests/fastapi_real_demo_e2e.rs` ships two tests:

- `test_e2e_fastapi_real_demo_compiles` — floor smoke (`cobrust build` of the
  demo source passes).
- `test_e2e_fastapi_real_demo_all_features_compose` — the completeness-critic
  round-trip: one running server, six probes asserting **every** Phase-1
  feature (valid adult 201 + echoed body; valid minor 403 proving the field
  read drives the branch; each of the three refinement kinds enforced as 422
  with the handler not entered; the OpenAPI schema showing all three kinds).

Run with:

```bash
LLVM_SYS_181_PREFIX=/opt/homebrew/opt/llvm@18 \
  cargo test -p cobrust-cli --test fastapi_real_demo_e2e
```

## See also

- `docs/human/en/import-pit.md` (and `zh/`) — the full `pit` surface guide.
- `docs/agent/adr/0080-*.md` — type-driven request validation + OpenAPI.
- `docs/agent/adr/0081-*.md` — validated-body field read + `json_response`.
- `examples/z8_rest_blog/` — the den (SQLite) + pit REST demo (manual body
  parsing); this demo is its type-driven successor for the request body.

## Today's limits (inherited from #156 Phase-1)

- Field reads are `i64` + `str` only (`f64` / `bool` and nested-class / list
  fields are later phases); they work only on a body parameter your handler
  received from `route_validated` (a hand-constructed `CreateUser()` value
  does not yet carry field storage — the native-struct follow-up).
- The OpenAPI `paths` entry advertises the generic validated/422 responses;
  the per-status 201/403 response shapes are a follow-up. The body schema
  (`components/schemas/CreateUser`) is fully derived from the type.
- The route *path* string remains stringly-typed (typed path-params are a
  named follow-up, ADR-0080 §9); the body is typed end-to-end.
