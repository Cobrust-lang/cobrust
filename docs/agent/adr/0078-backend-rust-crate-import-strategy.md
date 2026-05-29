---
doc_kind: adr
adr_id: 0078
title: Backend Rust-crate import strategy — wrap-the-crate (ADR-0072 chain) over translate-the-Python for the FastAPI-real backend surface (tower-http / validator / utoipa / sqlx / sea-orm / diesel / jsonwebtoken / argon2 / redis)
status: draft
date: 2026-05-29
decision_owner: cto
last_verified_commit: c3caa88
relates_to: [adr:0019, adr:0028, adr:0050d, adr:0071, adr:0072, adr:0073, adr:0074, adr:0076, adr:0077, "claude.md:§2.2", "claude.md:§2.5", "claude.md:§4.2"]
---

# ADR-0078: Backend Rust-crate import strategy

## 1. Context

ADR-0072 wired the `.cb` ecosystem-import chain and made `import den` / `import strike`
/ `import pit` / `import coil` real. ADR-0073 extended it to cross-boundary callbacks
(the pit/hood/dora handler trampoline). ADR-0074 added decorator sugar (`@app.route`).
ADR-0077 began the operator/index/attribute surface for `coil.Buffer`. Stream Z's
MUST-ship is a **FastAPI-real** backend: a `.cb` web app with typed request validation,
auto-generated OpenAPI/`/docs`, a real DB layer, auth, and middleware — the surface an
LLM agent writes when asked for "a Python web service" today.

The question this ADR answers: **how do we get that surface?** Two roads:

- **Road A — wrap a correct Rust crate** through the ADR-0072 chain (manifest →
  MIR-retarget → codegen-extern → cabi-trampoline → build-link). This is what
  `den`/`strike`/`pit`/`coil` *already are*.
- **Road B — translate the Python library** (FastAPI/pydantic/SQLAlchemy/PyJWT/passlib)
  through the L0-L3 LLM-translation loop (CLAUDE.md §4.2).

This ADR establishes that **Road A is strictly cheaper and more correct** for the backend
surface (§2), surveys the candidate crates with an honest per-crate mapping-tractability
read (§3), confronts the genuinely hard design problem the survey exposes (§4 — how a flat
`.cb` manifest row expresses a derive-macro / compile-time-query-macro / deeply-generic-
trait Rust API), generalizes the async-收编 strategy pit already proves (§5), then
**recommends the Phase-1 first crate** with an ADR-0077-§9-style implementation map (§6),
phases toward FastAPI-real (§7), scores the §2.5 surface match (§8), and enumerates the
sub-ADR questions each hard class spawns (§9).

**This ADR is DESIGN ONLY (doc, zero src).** It picks the road, picks the first crate,
and maps the seams. It does not implement.

### 1.1 The proven-mechanism map (verified at `c3caa88`)

The load-bearing prior: the ADR-0072 chain is **already carrying async Rust web crates**.
The "new" backend crates are not a new mechanism — they are more rows on a chain with four
live witnesses:

- **`crates/cobrust-den/src/cabi.rs`** — rusqlite, **sync**. Opaque `Connection`/`Cursor`
  handles cross as `*mut u8` (`Box::into_raw`/`Box::from_raw`), borrow-arg / return-fresh-
  handle discipline (cabi.rs:189/212/239), `DROP_COUNT` exactly-once instrument
  (cabi.rs:121), `__cobrust_str_*` declared `extern "C"` + resolved from the always-linked
  `libcobrust_stdlib.a` (ADR-0072 Q5; no Rust dep). den's `Rc<RefCell<…>>` handles are
  `!Send` (cabi.rs §`!Send`) — the single-threaded constraint is recorded, not fixed.
- **`crates/cobrust-pit/src/cabi.rs` + `src/app.rs`** — axum, **async, already 收编'd into
  the single structured-concurrency model**. `app.rs:61` `runtime()` returns a
  `OnceLock<Runtime>` process-singleton tokio runtime; `App::run` (app.rs:184) does
  `runtime().block_on(async move { … })`; `serve_in_background` (app.rs:207) does
  `rt.block_on(bind) + rt.spawn(serve)`. The `.cb` surface sees **zero `async fn`** —
  CLAUDE.md §2.2 (no async/sync coloring) is honored *at the cabi boundary*, not by the
  user. pit also proves the **callback trampoline** (cabi.rs:246 `__cobrust_pit_app_route`
  transmutes a `Constant::FnRef` fn-ptr into axum's `Arc<dyn Fn(Request) -> Response +
  Send + Sync + 'static>` Handler bound), abort-on-panic across the C ABI
  (cabi.rs:287 `catch_unwind`), and Rust-owned-Request / `.cb`-owned-Response ownership
  split (ADR-0073 §2 D6).
- **`crates/cobrust-strike/src/cabi.rs` + `src/client.rs`** — reqwest, **async crate driven
  via the `blocking` feature** (Cargo.toml `features = ["json","blocking","rustls-tls"]`;
  client.rs:275 `reqwest::blocking::Client`). A *second* async-收编 strategy (let the crate
  ship a sync facade) distinct from pit's `block_on`. Both are valid; §5 names which a
  candidate crate forces.

So the backend candidate crates are the **same chain, lower-risk than the framing
suggests** — every layer has a verbatim template and pit already proves the hardest sub-
problems (async runtime + cross-boundary callbacks). What is genuinely *new* is §4: some
candidate crates expose their power through **derive macros / compile-time query macros /
deeply-generic traits**, and those do NOT map to a flat manifest row.

## 2. Thesis — wrap-the-crate is strictly cheaper + more correct (and Cobrust IS Rust)

The reason Road A wins is structural, not incidental: **Cobrust compiles to native code
through LLVM over a C-ABI link step** (the den/pit/strike/coil proof). A Rust backend
crate is *already* a correct-by-construction native artifact with a stable C-ABI reachable
via `#[no_mangle] extern "C"` shims. Importing it is a **direct link** to vetted code.
Translating the equivalent Python library means running the L0-L3 loop (spec-extract →
LLM-translate → differential-verify ≥1000 fuzzed inputs → downstream-validate) to
*reconstruct* behavior the Rust crate already has, correctly, today.

### 2.1 Cost / correctness comparison

| Axis | Road A — wrap a Rust crate (ADR-0072 chain) | Road B — translate the Python lib (L0-L3) |
|---|---|---|
| **Correctness source** | The crate's own test suite + ecosystem use (millions of downloads; e.g. axum/sqlx are battle-tested). Correct *by construction*. | Reconstructed by LLM; correctness must be *earned* per-fn via differential testing against the CPython oracle + ≥1000 fuzzed inputs (CLAUDE.md §4.2 L2, §8). |
| **Token cost** | ~0 LLM tokens. The work is manifest rows + cabi shims + 5-layer wiring (deterministic engineering). | High: L0 spec-extract + L1 consensus-translate + L2 repair-loop (≤50 retries/fn) over the *whole* library dependency graph. |
| **Time-to-surface** | Days-per-crate (den first proof ≈ 1 sprint; §1.1 proves the template). | Weeks-per-library (M4 `tomli` — a *tiny* lib — was a full milestone). FastAPI's transitive Python deps (pydantic-core in Rust already, starlette, anyio) dwarf `tomli`. |
| **Async / runtime** | One 收编 site (pit's `block_on` / strike's `blocking`); §2.2 honored at the boundary. | Must re-derive the async semantics in the no-coloring model AND match Python's asyncio behavior — strictly harder. |
| **Perf gate (§4.2 L2)** | Native Rust crate IS the perf baseline; ≥0.8× is trivially met (it's 1.0× — same code). | Must hit ≥0.8× of CPython; for C-accelerated libs (pydantic-core, orjson) the bar is a *Rust* bar. |
| **Maintenance** | Track upstream crate version in the manifest (`@py_compat` tier + version pin). | Re-run L0-L3 on every upstream Python release; divergences accumulate. |
| **§2.5 LLM-correctness** | The `.cb` surface is hand-shaped to match the Python idiom (manifest is small + deliberate). | The translated surface inherits whatever the LLM emitted; surface drift is a verification burden. |
| **Honest cost of Road A** | The §4 hard problem: derive-macro / compile-time-macro / generic-trait APIs do NOT map flat. Some crates need a *sub-ADR*, not a sprint. | n/a (Road B has its own, larger, hard problems). |

**Conclusion:** for the backend surface, Road A is the default. Road B (translation)
remains correct for **pure-Python libraries with no good Rust analog** (the original
`tomli`/`dateutil` thesis) — but FastAPI's entire stack has best-in-class Rust crates, so
the ecosystem-import chain is the right tool. This is consistent with ADR-0076's dora
decision ("FFI, NOT L0-L3 translation") and ADR-0012's "translate the surface, bind the
core" (pit binds axum; here we bind the whole crate).

The one caveat the thesis must carry honestly: **"wrap a Rust crate" is cheap only for
flat-API crates.** §3 + §4 show that the *most FastAPI-defining* features (typed
validation, OpenAPI gen) live behind Rust's derive-macro / compile-time-macro machinery,
which is exactly where the cheap-link story stops and a design sub-problem starts.

## 3. Candidate-crate survey — per-crate mapping-tractability (the load-bearing analysis)

Each crate rated on `.cb`-surface-mapping difficulty: **FLAT** (maps like
den/strike/pit — connect/execute, route/handler, a `Layer` registration — pure manifest
rows + cabi shims), **MEDIUM** (mostly flat but one sub-surface needs new marshalling, like
ADR-0077's tuple/kwarg-arg work), **DEEP** (the crate's *defining* API is a Rust derive
macro / compile-time macro / deeply-generic trait that has **no flat `.cb` surface** and
needs its own sub-ADR — the coil-operator-sub-ADR precedent). API shapes below were
verified against docs.rs at authoring time (ADSD §4 no-overclaim).

| Crate | FastAPI role | Rating | The specific Rust-API shape + why it maps (or doesn't) |
|---|---|---|---|
| **tower-http** | CORS / trace / compression / auth / timeout middleware | **FLAT** (mostly) | Ships ready-made `Layer` values — `CorsLayer`, `TraceLayer`, `CompressionLayer` — registered via `Router::layer(CorsLayer::permissive())` / `ServiceBuilder::layer(...)` (verified docs.rs). A `.cb` `app.use_cors()` / `app.use_trace()` is a **manifest row + a cabi shim that calls `router.layer(CorsLayer::…)` on pit's App** — the Layer construction stays entirely Rust-side; the `.cb` side only flips a registration. pit is **already built on tower** (axum's `Router::layer`), so the integration surface exists. The DEEP part is *only* if we expose `Layer`-construction *configuration* (`CorsLayer::new().allow_origin(...).allow_methods(...)`) builder-by-builder to `.cb`; the FLAT first proof exposes **canned presets** (`use_cors()` = `CorsLayer::permissive()`) and configuration is a follow-up. |
| **validator** | struct validation = pydantic-equiv | **DEEP** | The primary API is `#[derive(Validate)]` on a struct with field attributes `#[validate(email)]`, `#[validate(length(min=1))]`, `#[validate(range(min=18))]` → a `Validate` trait with `.validate() -> Result<(), ValidationErrors>` (verified docs.rs). **There is no `.cb` struct-derive surface** — `.cb` has no `#[derive(...)]` and no proc-macro user surface. The derive runs at *Rust*-compile-time over a *Rust* struct; a `.cb` program declares no such struct. (Escape: validator ALSO ships non-derive trait validators — `ValidateEmail`/`ValidateLength`/`validate_must_match()` — verified docs.rs; §4.a uses this.) This is the FastAPI-DEFINING feature and the hardest to map. **Own sub-ADR.** |
| **utoipa** | Rust-type → OpenAPI / Swagger gen | **DEEP** | `#[derive(ToSchema)]` on structs + `#[utoipa::path(...)]` proc-macro attributes on handler fns build an `OpenApi` document by **reflecting Rust types at compile time** (derive macro). Same blocker as validator squared: there is **no `.cb` type for the derive to reflect over**, and the OpenAPI doc is *generated from* the type system that the `.cb` program does not expose to Rust. **Own sub-ADR** (§4.c — likely "Cobrust generates the schema from its OWN manifest/type info, NOT via utoipa's derive"). |
| **sqlx** | compile-time-checked queries | **MEDIUM→DEEP** (split) | **Two APIs** (verified docs.rs): (1) `sqlx::query!`/`query_as!` **macros that check SQL at *Rust*-compile-time against a live DB via `DATABASE_URL` or a `.sqlx` offline cache** — this is **alien to `.cb`**: the SQL string lives in `.cb` source, the compile-time check fires in the *Rust* crate's build, and there is no `.cb`→`DATABASE_URL` reflection path. DEEP. (2) The runtime form `sqlx::query("SELECT …").bind(x).fetch_all(&pool).await` takes a `&str`, binds at runtime, **no compile-time check** — this is **FLAT-MEDIUM**, essentially den's `conn.execute(sql)` for Postgres/MySQL with a pool handle + an async-收编 (sqlx is fully async, `PgPool::connect(url).await`). So sqlx maps as the **runtime API (Road A FLAT-MEDIUM)**; the compile-time-macro superpower does not cross to `.cb` and is its own sub-ADR (or simply declined). |
| **sea-orm** | trait-heavy async ORM | **DEEP** | Entities are derive-generated (`#[derive(DeriveEntityModel)]`) and queries compose through generic `EntityTrait`/`ActiveModelTrait`/`QueryFilter` traits with associated types. The whole ergonomic surface is **generic-trait + derive-macro** — no flat row expresses `Entity::find().filter(Column::Id.eq(1)).all(&db).await`. **Own sub-ADR**, and a lower priority than the FLAT DB path (sqlx-runtime / den already cover "run a query"). |
| **diesel** | sync ORM + DSL | **DEEP** | `table!` macro + `#[derive(Queryable, Insertable)]` + a compile-time-typed query DSL (`users.filter(id.eq(1)).load::<User>(conn)`). The DSL is built from macro-generated typed column modules — **maximally derive/macro-bound**, the least `.cb`-mappable of the DB crates. Sync (unlike sqlx). **Own sub-ADR / likely declined** in favor of sqlx-runtime. |
| **jsonwebtoken** | JWT encode/decode | **MEDIUM** | Free fns `encode(header, &claims, &key) -> Result<String>` + `decode::<Claims>(token, &key, &validation) -> Result<TokenData>` (verified docs.rs), **sync**. FLAT-shaped (free fns, no async) BUT the generic `Claims` is a serde-`Serialize`/`Deserialize` struct — the `.cb` side has no such struct to parameterize over. **Maps MEDIUM** by fixing the claims shape: a `.cb` `jwt.encode_hs256(secret, subject, exp) -> str` / `jwt.decode_hs256(secret, token) -> JwtClaims` with a **fixed canned claims set** (sub/exp/iat) marshalled as scalars, hiding the generic. Arbitrary-claims (generic `Claims`) is a follow-up needing a `.cb`-struct↔serde bridge (shared with the validator/utoipa derive problem). |
| **argon2** | password hashing | **FLAT** | `Argon2::default().hash_password(pw_bytes, &salt) -> Result<PasswordHash>` + `verify_password(pw, &parsed_hash) -> Result<()>` (verified docs.rs), **sync, CPU-bound**, via the `password-hash` `PasswordHasher`/`PasswordVerifier` traits — but the *user-facing* surface is two function calls over strings. Maps as `auth.hash_password(pw) -> str` + `auth.verify_password(pw, hash) -> bool`: **pure str-in / str-out, the den/strike template exactly.** The cleanest FLAT win after tower-http. |
| **redis** | cache / KV | **FLAT** (sync path) | Sync path (verified docs.rs): `Client::open(url)?` → `client.get_connection()?` (a `Connection` handle, den-shaped) → `con.set(k, v)` / `con.get(k) -> Option<T>` via the `Commands` trait, or low-level `redis::cmd("SET").arg(k).arg(v).query(&mut con)`. **This is den's Connection-handle pattern verbatim** — opaque `*mut u8` Connection, borrow-arg scalar/str commands. Async (`aio`) exists but the sync path makes it FLAT. The DEEP part is only generic `T` return polymorphism on `get`; fix it to `get_str`/`get_int` first proof. |

### 3.1 What the survey reveals

Two clean buckets:

- **FLAT / MEDIUM (Road-A-cheap, maps like the existing modules):** **tower-http**
  (canned-preset middleware), **argon2** (str-in/str-out), **redis** (den-shaped
  Connection), **jsonwebtoken** (fixed-claims scalars), **sqlx-runtime** (den-shaped
  pool + runtime query string). These need **manifest rows + cabi shims + the standard
  5-layer wiring** — exactly the den/strike/pit/coil sprint shape, no compiler-internals
  change beyond new externs.
- **DEEP (the FastAPI-DEFINING features, need their own sub-ADR):** **validator**
  (`#[derive(Validate)]`), **utoipa** (`#[derive(ToSchema)]` + `#[utoipa::path]`),
  **sqlx-macros** (`query!` compile-time check), **sea-orm**/**diesel** (derive + generic-
  trait ORM DSL). These do NOT map to flat manifest rows. They are the §4 hard problem.

The painful irony, stated plainly: **the two features that *define* FastAPI vs Flask —
typed request-body validation (pydantic) and auto OpenAPI/`/docs` — are precisely the two
DEEP crates** (validator + utoipa). The cheap FLAT wins (middleware, auth, cache, DB) are
real backend value but are the *supporting cast*, not the headline.

## 4. The hard design problem — expressing a non-flat Rust API in a `.cb` manifest row

A `.cb` manifest row (ADR-0072 Q2) is `module → { fn → (params:[CbTy], ret:CbTy,
py_compat_tier) }` + handle types + drop symbols, extended by ADR-0073's
`EcoParam::Callback(FnTy)`. It expresses **a function call over scalars / strings / opaque
handles / a fn-pointer callback**. It cannot, today, express a Rust *type-level* construct.
Three distinct alien shapes, three distinct mapping strategies.

### 4.a Derive macros (`#[derive(Validate)]`, `#[derive(ToSchema)]`)

**The blocker.** A derive macro runs at *Rust*-compile-time over a *Rust* struct's fields,
generating a trait impl. A `.cb` program declares no Rust struct and has no `#[derive]`
surface. There is nothing for the derive to attach to.

**Two candidate mapping strategies:**

- **(Strategy 4a-i) Codegen synthesizes a Rust derive on a wrapper struct.** When a `.cb`
  program declares a validated record type (a future `.cb` `struct`/`record` surface) with
  validation annotations, **the Cobrust codegen emits a synthesized Rust source file** with
  `#[derive(Validate)]` + the field attributes, compiles it into the user's link unit, and
  the cabi calls `.validate()` on it. This is a *new codegen capability* (Cobrust has never
  emitted Rust source to be re-compiled; today it emits LLVM IR + links pre-built archives).
  Powerful but heavy: it adds a Rust-source-generation + re-compile stage to `cobrust
  build`, and the `.cb`→Rust-struct field mapping is itself a design (which `.cb` types map
  to which Rust validator-compatible types). **Likely the eventual answer for utoipa**
  (where the *type structure* genuinely must reach Rust), and the subject of its own sub-ADR.

- **(Strategy 4a-ii) Move validation to a runtime-checked, manifest-declared schema —
  NO derive.** Instead of a Rust derive, the `.cb` side declares a validation schema as
  **data**, and a cabi shim runs validator's **non-derive trait validators**
  (`ValidateEmail`/`ValidateLength`/`ValidateRange`/`validate_must_match()` — verified
  present on docs.rs) field-by-field at runtime. The `.cb` surface is e.g.
  `schema.field("email", v.email()).field("name", v.length(1, 80))` then
  `schema.validate(body_dict) -> Result<(), ValidationErrors>`. **This sidesteps the derive
  entirely** — it is the den/strike pattern (build a handle, call methods, get a Result) and
  needs NO new codegen stage. It loses compile-time-catch (a §2.5 cost — validation errors
  are runtime, not type-check-time) and loses the derive's auto-OpenAPI-schema linkage, but
  it ships on the existing chain. **Recommended as the Phase-2 validator first proof**; the
  derive path (4a-i) is the §2.5-superior follow-up when a `.cb` record-type surface exists.

The §2.5 tension is sharp here: 4a-ii ships now but defers errors to runtime (weakest LLM
correction signal); 4a-i gives compile-time-catch but needs a Cobrust-emits-Rust stage that
does not exist. **Decision: Phase 2 ships 4a-ii (runtime schema), and a sub-ADR
(§9) designs 4a-i (codegen-synthesized derive) for the §2.5-correct future.**

### 4.b Compile-time query macros (`sqlx::query!`)

`sqlx::query!("SELECT …")` checks the SQL against a live DB **at Rust-compile-time**
(`DATABASE_URL`) or a committed `.sqlx` offline cache (verified docs.rs). For `.cb` this is
**doubly alien**: (1) the SQL literal is in `.cb` source, and the check would have to fire
during the *Rust* crate's compile, with no path from `.cb` source into sqlx's macro input;
(2) the compile-time check needs a live DB at *build* time, which `cobrust build` has no
notion of.

**Decision: do NOT cross the compile-time-macro to `.cb`.** Map sqlx via its **runtime
API** (`sqlx::query(&str).bind(...).fetch_all(&pool).await`), which is den's
`conn.execute(sql)` shape for Postgres/MySQL + an async-收编 pool handle. The
compile-time-checked superpower is a Rust-only convenience that does not generalize to a
language whose source the Rust compiler never sees. A *future* sub-ADR could explore a
**Cobrust-native** compile-time SQL check (Cobrust's own toolchain reads `.cb` SQL literals
+ a build-time DB) — but that is a Cobrust feature, not an sqlx-macro wrap. Recorded as a
§9 open question, not a Phase target.

### 4.c Generic trait abstractions (tower's `Service<Request, Response>`, sea-orm's `EntityTrait`)

tower's `Service<Req>` (and the `Layer<S>` that wraps one) is a generic trait with
associated `Response`/`Error`/`Future` types. A `.cb` program cannot *implement* a generic
Rust trait (no trait-impl surface, no associated-type surface). **But it does not need to:**
the FLAT tower-http path consumes *pre-built* `Layer` values (`CorsLayer`, `TraceLayer`) —
the genericity is entirely Rust-internal, and the `.cb` side only triggers a registration
(`router.layer(layer)`). So **generic-trait-CONSUMPTION maps FLAT** (the cabi shim holds the
generic types); **generic-trait-IMPLEMENTATION (writing a custom `Service`/middleware in
`.cb`) is DEEP** and out of scope — a `.cb` custom middleware would need the ADR-0073
callback chain plumbed through tower's `Service::call`, a sub-ADR of its own. sea-orm is
DEEP for the same reason: its ergonomics require the `.cb` side to compose generic-trait
*queries*, which has no flat surface.

**Decision: consume pre-built generic-trait values (FLAT); do not let `.cb` implement
generic Rust traits (DEEP, deferred).** This is exactly why tower-http rates FLAT for
canned middleware and sea-orm rates DEEP.

### 4.d The unifying insight

All three hard shapes share one root: **the Rust crate's power lives at Rust's
*type/compile-time* layer (derives, query macros, generic traits), and `.cb`'s manifest row
lives at the *value/runtime/link* layer.** The chain trivially carries value/runtime/link
APIs (every FLAT crate). It carries type/compile-time APIs only by either (i) **degrading
them to a runtime form** (4a-ii runtime schema, 4b runtime query, 4c consume-pre-built) —
cheap, ships now, costs §2.5 compile-time-catch; or (ii) **teaching Cobrust to generate
Rust source** (4a-i) — §2.5-correct, but a major new codegen stage. The phasing (§7) ships
(i) and sub-ADRs (ii).

## 5. Async-收编 (§2.2) — keeping the `.cb` surface sync under one runtime

Every interesting backend crate is async (axum/sqlx/redis-aio/reqwest). CLAUDE.md §2.2
forbids async/sync coloring *at the user layer*. pit already proves the 收编: the async
crate is driven entirely **inside the cabi boundary**, and the `.cb` surface sees only sync
calls. Two proven strategies, plus the per-candidate cost:

- **Strategy S1 — `block_on` on a process-singleton runtime (pit's path).** A
  `static RT: OnceLock<Runtime>` (pit app.rs:61), and every cabi shim that calls an async
  crate method wraps it `runtime().block_on(async move { … })`. Best when the crate is
  async-only (axum, sqlx, redis-aio). The `.cb` call blocks the calling thread for the
  duration — correct under the single structured-concurrency model (there is one runtime,
  and the `.cb` "thread" is a runtime-driven task or the main thread).
- **Strategy S2 — use the crate's own sync/blocking facade (strike's path).** reqwest ships
  a `blocking` feature (strike Cargo.toml) presenting `reqwest::blocking::Client`; the cabi
  shim calls it directly, no `block_on`. Best when the crate offers a maintained blocking
  facade. sqlx does **not** ship one (fully async); redis's sync path IS this (its
  `get_connection()` non-`aio` path is a blocking facade — verified docs.rs).

**Per-candidate async-surface cost:**

| Crate | Async? | 收编 strategy | Cost |
|---|---|---|---|
| tower-http (via pit/axum) | Layers are sync constructs; serving is pit's existing async | **none new** — pit already 收编s axum (app.rs `block_on`) | **zero** — `router.layer(...)` is a sync call; the async serving loop is pit's existing `block_on` |
| argon2 | sync (CPU-bound) | **none** | zero |
| jsonwebtoken | sync | **none** | zero |
| redis | sync path available | **S2** (use the blocking `get_connection`) | low — den-shaped sync Connection handle |
| sqlx | fully async | **S1** (`block_on` per query on a singleton runtime) | medium — every query shim wraps `block_on`; pool handle constructed under `block_on` (mirror pit's `serve_in_background` rt-take pattern for the pool's lifetime) |
| sea-orm | fully async | **S1** | medium (atop its DEEP trait rating) |

**Generalization:** an async candidate crate is 收编'd by **either** reusing pit's
`OnceLock<Runtime>` + `block_on` (S1, for async-only crates) **or** the crate's blocking
facade (S2, when it ships one). The `.cb` surface stays sync; §2.2 is honored at the cabi
boundary exactly as pit proves today. Note the **`!Send`/runtime-affinity caveat** (ADR-0072
§5 risk 2, den's `Rc<RefCell>`): a handle constructed under one `block_on` must be used
under the same runtime — sqlx's `PgPool` is `Send + Sync + Clone` (fine), but a per-crate
check is part of each FLAT sprint's done-means.

## 6. Decision — Phase-1 first crate = **tower-http (canned-preset middleware)**

**Recommendation: tower-http.** Justification via tractability × FastAPI-value:

- **Most tractable real-backend win (FLAT):** tower-http's `Layer` values
  (`CorsLayer`/`TraceLayer`/`CompressionLayer`) register via `Router::layer(...)` — and
  **pit is already built on tower/axum** (`Router::layer` exists in pit's app). The
  integration is a manifest row + a cabi shim flipping a registration on pit's `App`. No new
  async-收编 (pit's `block_on` already serves), no derive-macro blocker, no compile-time-
  macro blocker, no generic-trait-implementation (we *consume* pre-built `CorsLayer`,
  §4.c). It is the den/pit sprint shape with the *lowest* new-surface count.
- **Real, demoable FastAPI feature:** CORS is a near-universal requirement for any real web
  service (browser clients); request tracing/logging is the second. `@app.middleware` /
  `app.use_cors()` is a surface an LLM writing a FastAPI/Flask app reaches for immediately.
- **Why not validator/utoipa first** (the FastAPI-DEFINING features): both are **DEEP**
  (§3 derive-macro blocker, §4.a). Leading with them means leading with the
  Cobrust-emits-Rust-source sub-problem (4a-i) or accepting the runtime-schema degrade
  (4a-ii) before the FLAT chain is even extended to a new crate. Wrong order: prove the FLAT
  backend-crate extension first (tower-http), *then* spend a sub-ADR on the DEEP defining
  features. ADR-0077 set this precedent — ship the tractable mechanism first, sub-ADR the
  hard surface.
- **Why not argon2** (the other clean FLAT): argon2 is even simpler (str-in/str-out, no
  handle) but it is a *leaf* utility, not a *web* feature — tower-http exercises the
  pit-integration seam (the chain's web spine) which is more architecturally load-bearing
  to prove first. argon2 is the ideal **Phase-1b** fast-follow (≈half a sprint).

### 6.1 Phase-1 Implementation map (tower-http via pit `app.use_cors()` / `app.use_trace()`)

Mirrors ADR-0077 §9. Line anchors are at `c3caa88`; the impl sprint re-greps the named
functions. The first proof exposes **canned presets** (`use_cors()` = `CorsLayer::permissive()`,
`use_trace()` = `TraceLayer::new_for_http()`); configurable builders are a follow-up.

| Layer | File | Function / site | Edit |
|---|---|---|---|
| **Manifest** | `crates/cobrust-types/src/ecosystem.rs` | the pit `App` handle-method block (the `0xE000_0400` pit ADT region per ADR-0073 §4) | add rows `use_cors` / `use_trace` / `use_compression` → `__cobrust_pit_app_use_cors` etc., receiver `pit.App`, zero value-args, return `Ty::None` (side-effect on receiver, mirror `app.route`'s None-return discipline — pit cabi.rs:233 rationale: avoid aliasing a second drop-eligible App handle) |
| **Typecheck** | `crates/cobrust-types/src/check.rs` | `try_synth_method_call` / the ecosystem handle-method dispatch (the `lookup_handle_method` consult) | no new mechanism — the new rows resolve through the existing pit `App`-method path; verify the zero-arg method-call shape type-checks |
| **MIR** | `crates/cobrust-mir/src/lower.rs` | `try_lower_ecosystem_call` @1931 + `emit_ecosystem_call` @1995 (the handle-method retarget) | no new mechanism — `app.use_cors()` retargets to `Constant::Str("__cobrust_pit_app_use_cors")` via `emit_ecosystem_call`, identical to `app.route` |
| **Codegen** | `crates/cobrust-codegen/src/llvm_backend.rs` | `declare_runtime_helpers` pit extern block (where `__cobrust_pit_app_route` etc. are declared, per ADR-0073 §4) | add extern decls `__cobrust_pit_app_use_cors` / `_use_trace` / `_use_compression` (`ptr -> ptr`, the App-receiver / None-return shape) |
| **CLI build** | `crates/cobrust-cli/src/build/intrinsics.rs` | the `__cobrust_pit_*` recognizer arm (ADR-0073 §4 added it) | confirm the new `__cobrust_pit_app_use_*` symbols match the existing pit-prefix recognizer (likely already prefix-matched; verify) |
| **Runtime (the real work)** | `crates/cobrust-pit/src/app.rs` + new shims in `src/cabi.rs` | `App` must hold a middleware-flag set; `App::run`/`serve_in_background` apply `Router::layer(...)` when building the router | (a) add `App` fields `cors: bool` / `trace: bool` / `compress: bool` (or a `Vec<MiddlewareKind>`); (b) cabi shims `__cobrust_pit_app_use_cors(app) -> ()` set the flag (borrow `&mut App`, mirror nothing-returned discipline); (c) in `serve`/`handle_any` router construction (app.rs:257/266), conditionally `.layer(tower_http::cors::CorsLayer::permissive())` / `.layer(tower_http::trace::TraceLayer::new_for_http())` / `CompressionLayer`. **Async-收编: zero new** — registration is sync, serving is pit's existing `block_on` |
| **Cargo** | `crates/cobrust-pit/Cargo.toml` | `[dependencies]` | add `tower-http = { version = "0.6", features = ["cors", "trace", "compression-full"] }` + `tower` (already transitively present via axum; verify the direct dep + **stage `Cargo.lock`** per finding F64) |
| **Drop** | n/a | — | **no new handle** — middleware flags live inside the existing `App` box; no new `*mut u8` handle, no new `_drop` shim, no `DROP_COUNT` change. This is *why* tower-http is the cheapest first proof |
| **Decorator sugar (optional Phase-1c)** | `crates/cobrust-hir/src/lower.rs` Decorated lowering (ADR-0074) | — | `@app.middleware("cors")` could desugar to `_ = app.use_cors()` via the ADR-0074 chain — BUT ADR-0074 §7 risk 4 flags class-method decorators (`@app.middleware`) as a *future-ADR* concern; first proof ships the **explicit `app.use_cors()` call form** (ADR-0073-precedent: ship the explicit form, sugar follows) |
| **Tests** | `crates/cobrust-pit/src/cabi.rs` `#[cfg(test)]` + a new CLI E2E | mirror pit's `trampoline_invokes_handler` + a `pit_cors_e2e.rs` | a `.cb` program does `app.use_cors(); app.route(...); app.serve_in_background(...)`, the E2E issues a real cross-origin `OPTIONS`/`GET` via reqwest and asserts the `Access-Control-Allow-Origin` header is present |
| **Docs** | `docs/{agent,human/zh,human/en}` pit module specs | add the middleware surface rows | per CLAUDE.md §3.3 sync rule, in the impl commit |

**Honest difficulty read:** this is the **lowest-risk new-crate sprint in the ecosystem
chain so far** — lower than den's first proof, because (1) it adds **no new handle type**
(middleware is a flag on the existing `App`), (2) it needs **no new async-收编** (pit's
`block_on` serving already exists), (3) it touches the compiler internals only as *new
manifest rows + new externs* (the MIR/typecheck/codegen *mechanisms* are unchanged — it
rides ADR-0073's pit-method chain verbatim). The *only* real work is pit-runtime-side:
threading a middleware-flag set through `App` into the `Router::layer(...)` calls. The one
gotcha is the Cargo dep + **Cargo.lock staging** (finding F64) when adding `tower-http`.

## 7. Phasing toward FastAPI-real

Each phase: scope, done-means, layers touched, tractability note.

### Phase 1 (≈1 sprint) — tower-http middleware [RECOMMENDED, §6]

**Scope:** `app.use_cors()` / `app.use_trace()` / `app.use_compression()` canned presets on
pit's `App`. **Done-means:** a `.cb` program registers CORS + serves; an E2E asserts the
`Access-Control-Allow-Origin` header on a real cross-origin request; the existing pit
pong/route E2E suite still passes (no regression); workspace gates green. **Layers:**
manifest rows + externs + pit-runtime `Router::layer` wiring (NO new compiler mechanism, NO
new handle, NO new async-收编). **Tractability:** FLAT — lowest-risk chain extension to date.

### Phase 1b (≈½ sprint) — argon2 [fast-follow FLAT]

**Scope:** `auth.hash_password(pw) -> str` + `auth.verify_password(pw, hash) -> bool`.
**Done-means:** round-trip `verify_password(pw, hash_password(pw)) == true`, wrong-pw ==
false, E2E + drop-once (no handle — pure str-in/str-out). **Layers:** NEW `cobrust-argon2`
crate (cabi.rs str-in/str-out shims, the den template) + manifest rows + externs + build-
link. **Tractability:** FLAT — the cleanest leaf-utility wrap; sync, no handle, no runtime.

### Phase 1c (≈½–1 sprint) — redis + jsonwebtoken + sqlx-runtime [FLAT/MEDIUM batch]

**Scope:** `redis` (den-shaped Connection + `get_str`/`set_str`/`get_int`); `jsonwebtoken`
(`jwt.encode_hs256(secret, sub, exp) -> str` / `jwt.decode_hs256(secret, token) ->
JwtClaims` fixed-claims); `sqlx-runtime` (Postgres pool handle + runtime `query(sql)`,
async-收编 S1). **Done-means:** per-crate round-trip E2E + handle-drop-once. **Layers:** NEW
`cobrust-redis` / extend an `cobrust-jwt` / NEW `cobrust-sqlx` crates on the den/strike
template; sqlx adds the S1 `block_on` 收编 (mirror pit's rt-singleton). **Tractability:**
FLAT (redis/jwt) → MEDIUM (sqlx-runtime async-收编 + pool lifetime).

### Phase 2 (≥1 sprint + its own sub-ADR) — typed body validation (validator) [DEEP — FastAPI-DEFINING]

**Scope:** a `.cb` web handler validates a JSON request body against a declared schema.
**First-proof strategy: 4a-ii (runtime-checked manifest-declared schema)** — `.cb` builds a
validation schema as data (`schema.field("email", v.email()).field("age", v.range(18,
120))`), a cabi shim runs validator's **non-derive trait validators** over the parsed body
dict, returning `Result<(), ValidationErrors>` rendered to the `.cb` `Result`/error surface.
**Done-means:** a `.cb` POST handler rejects `{"email":"bad"}` with a 422 + a structured
error body, accepts a valid body; E2E. **Layers:** NEW `cobrust-validator` crate (runtime
trait-validator shims over the parsed-body dict) + manifest rows; **NO codegen-emits-Rust
stage** (that is the 4a-i sub-ADR). **Tractability:** DEEP — needs a sub-ADR for the schema-
as-data surface design; ships on the existing chain via the runtime degrade (§4.a), at the
§2.5 cost of runtime-not-compile-time validation errors.

### Phase 3 (≥1 sprint + its own sub-ADR) — auto OpenAPI / `/docs` (utoipa or Cobrust-native) [DEEP — FastAPI-DEFINING]

**Scope:** a `.cb` web app serves a generated OpenAPI document + a Swagger `/docs` UI.
**Strategy decision deferred to the sub-ADR:** either (a) **Cobrust generates the OpenAPI
JSON from its OWN type/manifest info** (Cobrust knows the handler signatures + the Phase-2
validation schemas — it can emit OpenAPI *without* utoipa's derive, sidestepping §4.a
entirely), OR (b) the 4a-i codegen-synthesized-Rust-derive path feeding utoipa. **(a) is
likely correct** — Cobrust already owns the type information utoipa's derive reflects, so
re-deriving it through a Rust macro is a detour. **Done-means:** `GET /openapi.json` returns
a valid OpenAPI doc matching the app's routes + validated bodies; `/docs` renders.
**Layers:** depends on (a) vs (b) — (a) is a Cobrust-side OpenAPI emitter (no new crate-wrap)
+ a static Swagger-UI serve via pit; (b) is the 4a-i codegen stage. **Tractability:** DEEP —
the headline FastAPI feature, and the strongest argument that *some* of FastAPI-real is a
**Cobrust-native capability, not a crate-wrap** (Cobrust's type system is the schema source).

### Phase 4+ (deferred) — ORM ergonomics (sea-orm/diesel), custom `.cb` middleware, arbitrary-claims JWT

DEEP generic-trait / derive surfaces; each its own sub-ADR (§9). Lower priority — the FLAT
DB path (sqlx-runtime / den) already covers "run a query".

## 8. §2.5 analysis — does the resulting `.cb` surface match what an LLM writes?

Explicit scoring of each shipped surface against the Python idiom an LLM emits for a
FastAPI/Flask backend. (FastAPI uses decorators + pydantic models + dependency injection;
Flask uses `app` methods + manual validation. The `.cb` surface targets the *intersection*
an LLM reliably writes.)

| Surface | Python idiom (FastAPI/Flask) | Cobrust shape | §2.5 overlap | Forced divergence |
|---|---|---|---|---|
| CORS middleware (Ph1) | `app.add_middleware(CORSMiddleware, ...)` (FastAPI) / Flask-CORS `CORS(app)` | `app.use_cors()` | **~0.85** | method name differs (`use_cors` vs `add_middleware`); the *shape* (call a method on `app` to enable CORS) matches |
| Tracing/logging (Ph1) | middleware / `@app.middleware("http")` | `app.use_trace()` | **~0.8** | same shape, different name |
| Password hash (Ph1b) | `passlib`/`bcrypt`: `pwd_context.hash(pw)` / `verify(pw, h)` | `auth.hash_password(pw)` / `auth.verify_password(pw, h)` | **~0.9** | near-verbatim — hash-in/verify-out is universal |
| Redis (Ph1c) | `r = redis.Redis(); r.set(k, v); r.get(k)` | `con = redis.connect(url); con.set_str(k, v); con.get_str(k)` | **~0.9** | `set`/`get` → `set_str`/`get_str` (type-suffix, the §2.5 cost of no generic `T`); connect-then-command shape is verbatim |
| JWT (Ph1c) | PyJWT: `jwt.encode(payload, secret)` / `jwt.decode(token, secret)` | `jwt.encode_hs256(secret, sub, exp)` / `jwt.decode_hs256(secret, token)` | **~0.75** | fixed-claims scalars vs a free-form payload dict; the encode/decode verb matches, the claims shape is narrowed |
| SQL query (Ph1c) | `cur.execute("SELECT …"); cur.fetchall()` | `pool.query("SELECT …").fetchall()` (mirror den) | **~0.9** | matches den/DB-API shape; pool vs connection naming |
| Body validation (Ph2) | pydantic `class Item(BaseModel): email: EmailStr` — declarative model | `schema.field("email", v.email())` — builder | **~0.5** | **large divergence** — pydantic's *declarative class* is the iconic FastAPI shape; the runtime-builder (4a-ii) does NOT match it. This is the §2.5 weak point, and why 4a-i (a declarative `.cb` record + codegen-derive) is the §2.5-correct future |
| Auto `/docs` (Ph3) | FastAPI: **free** (derived from type hints + pydantic models) | `app.enable_docs()` + Cobrust-native OpenAPI emit | **~0.7** | FastAPI's "you get `/docs` for free from your types" is the magic; a Cobrust-native emitter can match it *if* it reads the type system (Phase-3 strategy (a)) — then overlap rises toward 0.9 |

**Aggregate read:** the **FLAT Phase-1/1b/1c surfaces score ~0.85 average** — strong
overlap, the divergences are name/type-suffix cosmetics an LLM recovers from a one-line
diagnostic (§2.5-B error-UX FIX text). The **DEEP Phase-2/3 surfaces are where §2.5 bites:**
pydantic's declarative model (Ph2, ~0.5 with the runtime degrade) and FastAPI's free-`/docs`
(Ph3) are *defining* idioms whose §2.5-correct form needs a declarative `.cb` record surface
+ either codegen-derive (4a-i) or a Cobrust-native schema/OpenAPI emitter. **The honest
headline: the cheap wins land an LLM-friendly *supporting* backend; the §2.5-defining
FastAPI feel requires Cobrust-native type-driven validation + OpenAPI, which is a
language-feature sub-ADR, not a crate-wrap.** This is the single most important finding for
roadmap honesty — and it argues the FastAPI-real endgame is **Cobrust's own type system as
the schema/OpenAPI source** (Phase-3 strategy (a)), not a deeper crate wrap.

**Compile-time-catch (§2.5) ledger:** Ph1/1b/1c surfaces are call-shaped and type-check at
the manifest layer (compile-time-caught arg/type errors — the strong signal). Ph2's runtime-
schema degrade (4a-ii) moves validation errors to **runtime** (weak signal) — the recorded
§2.5 cost, mitigated only by the 4a-i sub-ADR. This mirrors ADR-0077 Q4's honest "shape-
correctness is uncheckable at compile time" admission: some power genuinely cannot reach
the type checker without a larger investment.

## 9. Open questions for sub-ADRs

Each DEEP surface (§3.1) and each §4 hard shape spawns a future design pass:

- **Derive-macro mapping (the 4a-i sub-ADR)** — does Cobrust gain a codegen stage that
  emits Rust source (`#[derive(Validate)]` / `#[derive(ToSchema)]`) over a synthesized
  wrapper struct, re-compiled into the user link unit? This is a **major new `cobrust build`
  capability** (today it emits LLVM IR + links pre-built archives; it has never generated
  Rust source). Defines the §2.5-correct declarative-validation + auto-schema future.
  Prerequisite: a `.cb` declarative record/struct surface for fields to map from.
- **Compile-time-query-macro (the 4b sub-ADR / likely declined)** — should Cobrust ever
  offer a *Cobrust-native* compile-time SQL check (its toolchain reads `.cb` SQL literals +
  a build-time DB), or is the runtime-query form (Phase-1c) the permanent answer? Wrapping
  `sqlx::query!` itself is rejected (§4.b — no path from `.cb` source into the Rust macro).
- **OpenAPI reflection (the Phase-3 strategy sub-ADR)** — **strategy (a) Cobrust-native
  emitter** (read the type system + Phase-2 schemas, emit OpenAPI JSON directly) vs
  **(b) codegen-synthesized utoipa derive** (the 4a-i path). (a) is likely correct (Cobrust
  owns the type info) and reframes a chunk of FastAPI-real as a **Cobrust language feature**,
  not a crate-wrap. The most strategically important sub-ADR for the FastAPI-real headline.
- **ORM trait surface (sea-orm / diesel sub-ADR)** — can a generic-trait / derive ORM DSL
  ever get a flat `.cb` surface, or is the query-builder-as-data pattern (4a-ii-style) the
  only viable map? Lower priority — sqlx-runtime/den cover "run a query"; the ORM ergonomics
  (typed entity composition) are the deferred want.
- **Custom `.cb` middleware (the 4c-implementation sub-ADR)** — plumbing the ADR-0073
  callback chain through tower's `Service::call` so a `.cb` fn IS a middleware (vs Phase-1's
  consume-pre-built-`Layer`). Needs the generic-trait-implementation gap closed.
- **Arbitrary-claims JWT + the serde bridge** — generalizing jsonwebtoken's fixed-claims
  (Phase-1c) to a free-form claims dict needs a `.cb`-value↔serde-`Value` bridge, **shared
  with the validator/utoipa derive problem** (both want `.cb` structured data to reach a
  serde-driven Rust API). A common `.cb`↔serde marshalling sub-ADR could unblock several
  DEEP surfaces at once.
- **Configurable middleware builders** — Phase-1 ships canned presets (`CorsLayer::permissive()`);
  exposing the builder chain (`CorsLayer::new().allow_origin(...).allow_methods(...)`) to
  `.cb` needs either a builder-handle pattern (a `.cb` `cors = CorsConfig(); cors.allow_origin(...)`
  handle, den-shaped) or a config-dict marshalling. A bounded follow-up, not a full sub-ADR.

## 10. Consequences

- **Positive:** establishes wrap-the-crate as the default for the backend surface with a
  defensible cost/correctness case (§2); identifies the FLAT batch (tower-http/argon2/redis/
  jwt/sqlx-runtime) that ships on the *existing* chain with no compiler-internals change;
  recommends the lowest-risk first proof (tower-http — no new handle, no new async-收编, no
  new mechanism); and — critically — **names honestly** that the §2.5-defining FastAPI
  features (declarative validation + free `/docs`) are DEEP and likely a **Cobrust-native
  type-driven capability**, not a crate-wrap (§8).
- **Negative / accepted:** the two FastAPI-DEFINING features (validator/utoipa) are DEEP and
  deferred to Phases 2/3 + sub-ADRs; the Phase-2 runtime-schema degrade (4a-ii) costs §2.5
  compile-time-catch (validation errors are runtime) until the 4a-i codegen-derive sub-ADR
  lands; several DEEP surfaces (sea-orm/diesel/custom-middleware/arbitrary-claims) are
  out-of-scope follow-ups.
- **Risk — manifest drift:** each new crate adds hand-maintained manifest rows (ADR-0072 §5
  R4 accepted debt; generation still deferred).
- **Risk — Cargo.lock staging:** every new-crate sprint adds a dependency; **stage
  `Cargo.lock`** (finding F64 — `--locked` CI rejects an unstaged lockfile, cluster-failing
  build/clippy/test).
- **Risk — async-收编 runtime affinity:** S1 (`block_on`) handles constructed under the
  singleton runtime must be used under it (ADR-0072 §5 R2 `!Send` precedent); each FLAT
  sprint's done-means includes a per-crate `Send`/affinity check.
- **Risk — the FastAPI-real headline is bigger than a crate-wrap:** §8 shows the defining
  feel needs Cobrust-native type-driven validation + OpenAPI. The roadmap must not
  over-promise "FastAPI in Cobrust" from the FLAT phases alone; Phases 2/3 + their sub-ADRs
  carry the real headline.
- **Follow-up:** ratify draft→accepted when the Phase-1 tower-http impl sprint lands +
  passes its done-means + a paired ADSD audit; open the §9 sub-ADRs (4a-i derive-mapping +
  Phase-3 OpenAPI-strategy first, as the FastAPI-real critical path).
