---
doc_kind: adr
adr_id: 0080
title: Cobrust-native type-driven request validation + OpenAPI emission — the FastAPI-real elegance-law PRIME target; declarative body `class` + per-field refinement side-table (NOT pydantic runtime-revalidation, NOT a derive-macro, NOT utoipa); structure is a type (compile-time), a value-predicate is a guard (runtime); schema + validator are two projections of ONE field table (cannot drift)
status: draft
date: 2026-05-30
decision_owner: cto
last_verified_commit: 5bfab21
relates_to: [adr:0006, adr:0050d, adr:0052a, adr:0060b, adr:0072, adr:0073, adr:0074, adr:0077, adr:0078, "claude.md:§2.2", "claude.md:§2.5", "claude.md:§5.1", "finding:F64", "feedback:elegant_ecosystem_surface_no_legacy_debt"]
---

# ADR-0080: Cobrust-native type-driven request validation + OpenAPI emission

## 1. Context + the #156 thesis

ADR-0078 surveyed the FastAPI-real backend surface and landed the load-bearing
honest headline (§8): the FLAT crate-wraps (tower-http / argon2 / redis / jwt /
sqlx-runtime) ship an LLM-friendly *supporting cast*, but **the two features that
*define* FastAPI vs Flask — typed request-body validation (pydantic) and auto
OpenAPI / `/docs` — are precisely the two DEEP crates** (`validator`'s
`#[derive(Validate)]`, `utoipa`'s `#[derive(ToSchema)]`), and ADR-0078 §4.a / §8 /
§9 concluded that their §2.5-correct form is **not a crate-wrap at all** but a
**Cobrust-native type-driven capability**: Cobrust already owns the type
information utoipa's derive reflects, so re-deriving it through a Rust proc-macro is
a detour (ADR-0078 §9 Phase-3 strategy (a) — "Cobrust generates the schema from its
OWN type info").

Task **#156** is that capability, and it is the **PRIME target of the
elegance-law** (`feedback_elegant_ecosystem_surface_no_legacy_debt`): the `.cb`
backend surface must be a *clean re-design that drops the footguns Flask / FastAPI /
Express accumulated*, NOT a mechanical clone. #156's thesis:

> A `.cb` web handler declares its request body as a typed value. The type IS the
> contract. Field *presence* + field *type* are checked at compile time (the strong
> LLM signal, §2.5). The value-level constraints a type cannot express
> (range / length / pattern) are a single runtime guard at the request boundary
> that returns a `Result`, rendered to a typed 422 — never a thrown exception,
> never an in-handler re-check. The OpenAPI schema is *derived from the same type*,
> so the schema and the validator cannot drift.

**This ADR is DESIGN ONLY (doc, zero src).** It picks the validation/OpenAPI
mechanism, states the elegance-law footgun ledger, weighs three approaches, maps the
buildable seams, and phases a Phase-1 that is the smallest end-to-end-real increment
on the type system **as it actually is today**. It does not implement, and it is
honest (§4, §8) that the cleanest design is *gated on a type-checker capability that
does not exist yet* — and makes Phase-1 the part of that capability buildable now.

### 1.1 Ground truth — verified at `5bfab21` (NO-OVERCLAIM)

The single most decisive fact, read from source, that shapes every option below:
**the static core does not track class/ADT field types.** All three approaches in
§4 converge on needing this prerequisite; it is the real gate, not the constraint
syntax. Verified seams:

- **A `class` registers as a ZERO-ARG constructor, fields untracked.**
  `check.rs:519-530` (`prebind_item`, `ItemKind::Class`) records the class `def_id`
  as `Ty::Fn(FnTy { positional: [], …, return_ty: Ty::Adt(AdtId(def_id), []) })` —
  a `() -> Adt` ctor — then recurses members. `check_class` (`check.rs:757-762`) is
  a **stub**: it only `check_item`s each member; it records **no field types** into
  the Adt. A class body field `name: str = …` is an `ItemKind::Let`
  (`cobrust-hir/src/tree.rs:42/52/74` — `ClassBody.members: Vec<Item>`,
  `ItemKind::Let`), and that `Let` is checked as an ordinary statement, **not**
  registered as an Adt field.
- **Attribute access on a class instance returns `fresh_var()`.** The `Attr` arm
  (`check.rs:1250-1291`) resolves tuple-field projection (`Ty::Tuple` + integer
  name) and ecosystem-handle attributes (`lookup_handle_attr`, ADR-0077 Q3), then
  for any other base falls through to `Ok(self.fresh_var())` (`check.rs:1291`) with
  the verbatim comment **"the static core does not yet track ADT fields"**
  (`check.rs:1260-1261`, `1283-1285`). So today `body.name` on a user class is an
  unconstrained type variable — there is no field type to type-check against and no
  type from which to derive a schema.
- **`Ty::Record { fields: BTreeMap<String, Ty> }` EXISTS but is unreachable from
  `.cb` source.** `ty.rs:106-120` defines a closed structural record with typed
  fields (it already unifies field-wise per ADR-0006). But `lower_type`
  (`check.rs:2749-2788`) has **no arm that constructs `Ty::Record`** — the only
  `TypeKind` variants are `Name / Generic / Union / Fn / Tuple / Ref / Array`
  (`ast.rs:255-277`); there is **no record/struct type-literal surface**.
  `Ty::Record` appears only in internal traversals (e.g. `check.rs:346`
  `type_refs_any`). **The typed-field machine exists in the type representation but
  is not wired to any source surface.**
- **`Ty` has NO refinement carrier.** `ty.rs:39-101` (`pub enum Ty`) has `Int /
  Float / Str / Bool / None / Adt / Record / List / Dict / Set / Tuple / Fn / Ref /
  Array / Var / Never / Alias` (abbreviated; the full enum also carries `Imag / Bytes /
  IntN / Generic`) — none of which is a slot for "an `Int` with a `0 <= x <= 100`
  predicate." A value-range constraint cannot be a type today.
- **pit handler ABI + the body-param hook.** `pit_handler_fn_ty()`
  (`ecosystem.rs:198`) is `fn(pit.Request) -> pit.Response`; the manifest wires it
  as `EcoParam::Callback(pit_handler_fn_ty())` (`ecosystem.rs:877`) on
  `runtime_symbol: "__cobrust_pit_app_route"`. `EcoParam::Callback(FnTy)`
  (`ecosystem.rs:344`) + `TypeError::CallbackSignatureMismatch`
  (`error.rs:259`) already type-check a handler's arity/param shape. The route
  trampoline `__cobrust_pit_app_route` (`cabi.rs:246-310`) **owns** the boxed
  `Request`, `catch_unwind`s across the C ABI, and frees the box exactly once in the
  closure (`cabi.rs:303`) — the body's `handle_drop_symbol` returns `None` so the
  `.cb` side never drops it (ADR-0073 §2 D6). `Request::json()` returns
  `Result<serde_json::Value, PitError>` (`request.rs:137`).
- **The pain #156 removes is REAL.** `examples/z8_rest_blog/main.cb:72-78` parses a
  POST body by hand: `let raw = req.body()` then chained `replace(...)` +
  `split(...)` into pieces, with the comment (line 42) *"Structured JSON-dict access
  lands with the coil-deep type work."* This is exactly the Express/Flask
  stringly-typed-body footgun the elegance-law forbids.
- **`TypeError` carries a FIX channel.** Every variant has
  `suggestion: Option<&'static str>` (`error.rs:8/26/…`), satisfying §2.5-B
  (errors print the fix). `MutableDefault` (`error.rs:104`) +
  `ImplicitTruthiness` (`error.rs:87`) already exist.

**Conclusion of the ground-truth read:** #156's elegance (compile-time structure +
drift-free schema) is *contingent on first making class field types real* — the
explicitly-absent piece (`check.rs:1260` "does not yet track ADT fields"). This is a
**core type-checker capability**, not a manifest row. The mechanism choice (§4) is
therefore decided less by the constraint syntax and more by *which approach adds the
smallest net-new surface on top of that one shared prerequisite.*

## 2. Decision (summary)

| # | Question | Decision |
|---|---|---|
| Q1 | The validated-body surface | **A declarative body `class` whose fields carry types, plus a per-field refinement clause `field: ty where <pred>` (Approach B, §4).** The `class … : field: type` body is verbatim the pydantic / dataclass idiom (§2.5 ~0.9, the single highest-overlap shape, ADR-0078 §8). The handler declares the body as a **typed second parameter** (`fn create(req: pit.Request, body: CreatePost) -> pit.Response`), type-checked through the EXISTING `EcoParam::Callback(FnTy)` + `CallbackSignatureMismatch` machinery. |
| Q2 | Where refinements live in the type system | **In a side-table keyed by `(AdtId, field)`, NOT in `Ty`.** `Ty` has no refinement carrier (§1.1) and widening `Ty::Int` would ripple through every `unify`/`subst`/`is_hashable` arm (high blast radius). The predicate is **metadata beside the field**, read by the guard-emitter AND the OpenAPI-emitter. This is the key scoping decision that keeps the increment buildable. |
| Q3 | Compile-time vs runtime split | **Structure is a type (compile-time); a value-predicate is a guard (runtime).** Field presence + field base-type + every `body.field` access are type-checked (the §2.5 strong signal). `range`/`length`/`pattern` lower to ONE boundary validator `validate_<Body>(serde_json::Value) -> Result<<Body>, ValidationError>` that runs once, at the trampoline, on the already-typed value. Cobrust has no dependent-type machinery, so these are **honestly runtime** — never re-checked in the handler. |
| Q4 | OpenAPI derivation | **A compile-time Cobrust-native emitter pass walks the SAME field table + refinement side-table the validator reads — NOT a derive macro, NOT utoipa, NOT a hand-kept schema.** Schema and validator are two projections of ONE source; there is no second declaration to drift from. This realises ADR-0078 §9 Phase-3 strategy (a). |
| Q5 | pit integration | **A sibling manifest row `app.route_validated(method, path, handler)` whose callback `FnTy` is `fn(pit.Request, <Body>) -> pit.Response`** (2-arg, type-checked by the existing `CallbackSignatureMismatch` gate). The trampoline deserializes+validates into `<Body>`; on `Ok(v)` it calls the `.cb` handler with the boxed value, on `Err` it short-circuits a typed 422 and **never enters the handler**. The Result-error path lives entirely in Rust, surfaced as a `Response` — mirroring `__cobrust_pit_app_route`'s Rust-owned-Request ownership split (ADR-0073 §2 D6). |
| Q6 | Refinement-language scope | **A FIXED grammar in v1 — `lo <= self <= hi` (and `lo <= self`, `self <= hi`) for ints/floats; `len(self) <= n` / `len(self) >= n` for str; `pattern(self, "<re>")` for str — rejecting anything else with a clear `TypeError`.** NO general SMT-style refinement *checking* (a LARGE separate type-system project, §8). All value-predicates stay runtime guards; the parser admits only the fixed forms. |
| Q7 | Phasing + Phase-1 | **Phase-1 = the load-bearing prerequisite + ONE refinement kind end-to-end:** land class field tracking (so `body.name` type-checks), parse `where lo <= self <= hi` on an `i64` field into the side-table, emit the boundary validator (range-check only), wire `app.route_validated` + the 422 path, and emit the body's OpenAPI schema with `minimum`/`maximum`. `len`/`pattern`, nested bodies, lists, and the declarative-shape sugar are later phases (§5). |

## 3. Design principle — the elegance-law footgun ledger (each of the 6, named, with the decision that drops it)

The elegance-law (`feedback_elegant_ecosystem_surface_no_legacy_debt`) mandates an
explicit ledger: name each accumulated Flask/FastAPI/Express footgun and the
specific design decision that drops it. No hand-waving — each row cites the seam.

| # | Footgun (Flask / FastAPI / Express / pydantic) | Cobrust #156 decision that DROPS it |
|---|---|---|
| **1** | **pydantic runtime-only validation** — even field *presence* + *type* are re-checked on every request at runtime; nothing is known statically. | **DROPPED.** Field presence + field base-type are the **`class` field table (compile-time, Q3)**: once `check_class` records `title: str`, `rank: i64` as real Adt fields and the `Attr` arm reads them (replacing the `fresh_var()` at `check.rs:1291`), `body.rank + 1` type-checks and `body.rank + "x"` is a `TypeError`. The boundary deserialization into `<Body>` is **total** — it yields a value whose fields match the declared types or it fails — so a missing/extra key or wrong JSON type is *structurally unable to reach the handler*. The runtime guard runs ONLY the value-predicates a type cannot express (`range`/`len`/`pattern`). No in-handler re-validation. |
| **2** | **validation errors as exceptions** (pydantic `ValidationError` raised; Flask aborts; control-flow-by-exception). | **DROPPED.** The boundary returns `Result<<Body>, ValidationError>` (Q3). CLAUDE.md §2.2 already makes `Result` the default and reserves exceptions for the unrecoverable. A failed guard becomes a **typed 422 `pit.Response` value** synthesised in the trampoline (Q5); the error never unwinds into the handler. Extends the existing `PitError` closed-enum / `Result`-default discipline (`error.rs`). |
| **3** | **FastAPI `Depends` DI magic** — bodies/params injected by a hidden registry resolved by import-time side effects; the wiring is invisible at the call site. | **DROPPED.** The body is an **explicit typed second parameter** on the handler signature (`fn create(req, body: CreatePost)`, Q1), wired at the explicit `app.route_validated(...)` call site (Q5). No global registry, no import-time side effect — ADR-0074 keeps the register-call in the module-init body, not a hidden DI container. The wiring is type-checked and visible. |
| **4** | **utoipa / drf-spectacular parallel-annotation drift** — the OpenAPI schema is a *second* hand-kept declaration (a `#[derive(ToSchema)]` copy of the field types, or YAML) that silently falls out of sync with the model. | **DROPPED.** The schema is **derived by walking the one field table + refinement side-table** the validator reads (Q4). One source, three consumers (type-check, validate, schema-emit) — **there is no second declaration to drift from.** This is the load-bearing elegance property of #156, and it is exactly why #156 *requires* the typed field table (without it there is no single source to derive from — only the parallel annotation the elegance-law forbids; ADR-0078 §9 Phase-3 (a)). |
| **5** | **Express / Flask stringly-typed routes + untyped bodies** — `req.body` is `Any`; you hand-parse it (the z8 `replace`/`split`, §1.1); typos in field access are runtime `KeyError`s. | **DROPPED for the body** (the #156 scope). `body.title` is statically `str`; a typo'd field is a `TypeError`, not a runtime `KeyError`; the z8 hand-parse vanishes. **Honest residual:** the route *path* string + path-params remain stringly-typed (out of #156 scope — a follow-up, §9). The body is typed end-to-end; the path is the named residual, not a silent gap. |
| **6** | **mutable default arguments** (`def f(items=[])` — the classic shared-mutable-default bug). | **KEPT-as-error (inherited).** A class field `items: list[i64] = []` already trips `TypeError::MutableDefault` (`error.rs:104`) via the existing default-arg rule (`Ty` mutable-container check). #156 inherits this for free; CLAUDE.md §2.2 already forbids it. |

**Net:** all six footguns are dropped or kept-as-error with a cited seam. The one
*honest residual* (footgun #5's route-path string) is named, not hidden, and scoped
out of #156.

## 4. Options considered — the three approaches, scored

All three approaches were independently grounded against `5bfab21` and **converge on
the same prerequisite** (class field tracking, §1.1) and the **same six-footgun
ledger** (§3) and the **same ~0.9 §2.5 score** on the declarative-class shape. They
differ only in *how the per-field constraint is spelled and where it lives* — which
decides how much net-new surface rides on top of the shared prerequisite. Judged in
the mandated priority order: **(1) elegance-law cleanliness → (2) §2.5 first-try →
(3) feasibility on the type system as-is → (4) grounding.**

### Approach A — field-constraint attributes (`name: str @length(1, 80)`)

A postfix `@constraint(...)` on a field's type annotation; the body is a `class`,
the handler takes it as a typed param, the trampoline validates.

- **Elegance (1):** clean — drops all six footguns identically to B/C (the ledger is
  approach-independent). 1.0.
- **§2.5 (2):** the declarative `class` core is ~0.9 (verbatim pydantic). **Deficit:**
  the constraint *spelling* — pydantic writes `Field(max_length=80)` / `conint(le=150)`;
  A's postfix `@length(1,80)` / `@range(0,150)` is **new surface an LLM will not
  produce first-try** (recovered from one §2.5-B FIX diagnostic).
- **Feasibility (3) — the decisive miss:** A's `@`-on-a-field grammar **does not
  exist and contradicts the existing grammar**. `parse_decorated` (`parser.rs:319-351`)
  attaches `@`-decorators *only* immediately before `fn`/`class`, and **explicitly
  rejects** any other target with `"decorators must precede `fn` or `class`"` +
  a suggestion. A field-level postfix `@` is a **net-new grammar surface that
  collides conceptually with the one decorator rule the parser already enforces** —
  it is the largest net-new *parser* surface of the three.
- **Grounding (4):** honest; A's own memo concedes the `@`-on-field form is net-new
  parser surface (parser.rs:319 attaches to fn/class only).
- **Verdict:** correct + elegant, but pays a §2.5 spelling cost AND the most
  net-new/conflicting grammar. **Runner-up.**

### Approach B — refinement clause (`field: ty where <pred>`) [CHOSEN]

The body is a `class`; each field's type annotation may carry a `where`-clause
(`title: str where 1 <= len(self) and len(self) <= 255`); the predicate is stored in
a **side-table keyed by `(AdtId, field)`**, leaving `Ty` untouched.

- **Elegance (1):** identical clean six-footgun drop (§3). 1.0.
- **§2.5 (2):** the declarative `class` core is ~0.9 (verbatim pydantic). The
  `where`-predicate is the one novelty, but it **reads like Python's `assert` /
  comprehension-guard and the Rust refinement-type literature** (high prior) — a
  closer training-data match than A's `@length` or C's `where len(...)` *call*
  form, because `where <boolean expr over self>` is the most "obvious" spelling of a
  constraint. Slightly ahead of A/C on the constraint spelling.
- **Feasibility (3) — the decisive win:** B adds the **smallest net-new surface on
  top of the shared prerequisite.** (i) The refinement is a **side-table**, so `Ty`
  is untouched — zero blast radius on `unify`/`subst_var`/`is_hashable` (contrast a
  `Ty`-widening that A/C would tempt). (ii) The body-param-as-typed-parameter wiring
  reuses the EXISTING `EcoParam::Callback(FnTy)` + `CallbackSignatureMismatch` gate
  verbatim (`ecosystem.rs:344`, `error.rs:259`) — the 2-arg handler shape type-checks
  for free. (iii) The `where`-clause grammar is a **postfix on an annotation inside a
  class field**, which is a *local* parser addition, not a collision with the
  decorator rule. (iv) The boxed-body crosses the C ABI on the same
  `Box::into_raw`/`from_raw` + `handle_drop_symbol → None` discipline as `Request`
  (`cabi.rs:284/303`). The only genuinely-new *compiler* work is the shared
  prerequisite (field tracking) + a bounded `where`-parse + two emitter passes.
- **Grounding (4):** fully grounded; the side-table scoping is the explicit move that
  keeps it buildable without touching `Ty`.
- **Verdict:** smallest correct increment, untouched `Ty`, reuses the proven callback
  machinery, and the `where`-spelling is the closest constraint-syntax to LLM priors.
  **CHOSEN.**

### Approach C — `#[derive(Validate)]`-analog + a `record` keyword + codegen-emits-Rust

A new `record` keyword/`TypeKind`; a `Validate` trait generated *from the type*;
schema reflected from the type — the utoipa/validator shape, done natively.

- **Elegance (1):** identical clean six-footgun drop (§3). The "derive-shaped, schema
  reflected from the type" framing is the cleanest *statement* of the cannot-drift
  property. 1.0.
- **§2.5 (2):** the declarative shape is ~0.9; the `where len(1,120)` *call* form is
  the same novelty cost as A's `@length`.
- **Feasibility (3) — the decisive miss:** C as literally specified needs **two**
  net-new pieces beyond the shared prerequisite: (i) a `record` surface — a new
  `TypeKind`/grammar OR a class-with-field-annotations path (the prerequisite either
  way), AND (ii) **the codegen-emits-Rust-source stage** (ADR-0078 §4a-i) — Cobrust
  has **never** emitted Rust source to be re-compiled (today it emits LLVM IR + links
  pre-built archives, ADR-0078 §4.a/§9). That is a *major new `cobrust build`
  capability*, not a bounded addition. C's own memo states the trap plainly: *"a 'C'
  that skips that gate is a B wearing C's name"* — i.e. without the Rust-emit stage,
  C collapses to B (runtime-schema-as-data), at which point B is the honest spine.
- **Grounding (4):** honest — C explicitly flags that its defining derive-shaped claim
  is gated on a build stage that does not exist.
- **Verdict:** the §2.5-right *end-state framing*, but its load-bearing piece (codegen
  emits Rust) is the single largest unbuilt capability of the three and is NOT needed
  to ship #156's elegance. **Runner-up; its best idea is grafted (below).**

### The choice + what is grafted from the runners-up

**Adopt B as the spine.** It drops the six footguns as cleanly as A/C, its
`where`-spelling is the closest constraint syntax to LLM priors (§2.5), and it has
the **smallest correct first increment buildable on the type system as it actually is**
— the refinement side-table leaves `Ty` untouched, the body-param wiring reuses the
proven `Callback(FnTy)` gate, and the only genuinely-new compiler work is the
shared prerequisite (field tracking) that *all three* need anyway.

Grafted from the runners-up (named, with why):

- **From Approach A — the split statement:** *"shape is a type (compile-time); a
  predicate over a value is a guard (runtime)"* is the single cleanest articulation
  of Q3 and is adopted verbatim as the design's organizing line. A's framing of the
  boundary deserialization as **total** (yields a typed value or fails) is also
  adopted — it is *why* footgun #1's in-handler re-check vanishes.
- **From Approach C — the cannot-drift framing + the sequencing honesty:** C's
  "schema reflected from the *one* type, derive-shaped, zero duplication" is the
  cleanest statement of Q4/footgun #4 and is adopted as the OpenAPI rationale. C's
  explicit warning — *the schema-derivation property is **gated** on building the
  field table first; a design that skips that gate is a B wearing C's name* — is
  adopted as §8's central honesty and the §5 phasing constraint (Phase-1 IS that
  gate). C's eventual codegen-emits-Rust path (§4a-i) is recorded as a §9 sub-ADR for
  a future where a `.cb`-type genuinely must reach a Rust derive (it does not, for
  #156 — Cobrust owns the type, Q4).

## 5. The chosen mechanism — buildable detail

### 5.1 Surface syntax (the `.cb` an LLM writes)

```python
# A validated request body is a `class` (= Ty::Adt) whose fields are typed,
# with an optional refinement `where`-clause per field (Q1, Q6).
class CreatePost:
    title: str where 1 <= len(self) and len(self) <= 255
    body:  str where len(self) <= 65535
    rank:  i64 where 0 <= self and self <= 100

# The handler declares the body as a TYPED second parameter (Q5). pit parses +
# validates the JSON body into it BEFORE the handler runs. No req.body() surgery.
fn create_post(req: pit.Request, post: CreatePost) -> pit.Response:
    # `post` is ALREADY validated. Field access is statically typed:
    let t = post.title            # : str  (compile-time known; non-empty/<=255 at entry)
    let r = post.rank             # : i64  (0..=100 guaranteed at entry)
    return pit.json_response(201, post)   # re-serializes from its known type

# Registration is explicit (no DI magic, footgun #3):
fn main() -> i64:
    let app = pit.App()
    app.route_validated("POST", "/posts", create_post)   # 2-arg handler shape (Q5)
    app.run("127.0.0.1", 8080)
    return 0
```

The single new grammar token is a **postfix `where <pred>` on a class-field type
annotation**, admitted only inside a `class`-body field (`Let`). `<pred>` is the
FIXED grammar of Q6 (`lo <= self <= hi` for ints/floats; `len(self) <= n` for str;
`pattern(self, "<re>")`) — anything else is a `TypeError` with a FIX suggestion.

### 5.2 Compile-time vs runtime split (Q3 — the organizing line, grafted from A)

**Shape is a type (compile-time); a value-predicate is a guard (runtime).**

- **Compile-time (type/borrow checker):**
  - *Field presence + base-type.* Once `check_class` records `CreatePost`'s fields as
    `{title: Str, body: Str, rank: Int}` into a field table, the `Attr` arm
    (`check.rs:1287`) returns the recorded field `Ty` for a class-instance base
    instead of `fresh_var()` (`check.rs:1291`). `post.rank + 1` type-checks;
    `post.rank + "x"` is a `TypeError` (the §2.5 strong signal). The boundary
    deserialization into `CreatePost` is **total** — it yields a value whose fields
    match the declared types or it fails — so missing/extra keys + wrong JSON types
    are *structurally unable to reach the handler* (drops footgun #1's in-handler
    re-check).
  - *Statically-decidable refinements (the few).* A contradiction (`self > 0 and
    self < 0`) or a trivially-true predicate is flagged at parse/check time. Most
    refinements are NOT statically decidable on arbitrary input (Q6 keeps them runtime).
  - *The handler-arity/param gate.* `app.route_validated`'s callback `FnTy` is
    `fn(pit.Request, <Body>) -> pit.Response`; a 1-arg handler, or a 2nd param that is
    not a class with a field table, is a `CallbackSignatureMismatch` (`error.rs:259`)
    with a FIX suggestion.
- **Runtime guard (emitted code, returns `Result`):** every refinement whose truth
  depends on the *value* — `len(self) <= 255`, `0 <= self <= 100`, `pattern`. The
  checker lowers each surviving predicate to a single boundary validator
  `validate_<Body>(serde_json::Value) -> Result<<Body>, ValidationError>` run once at
  the trampoline. **Why honestly runtime:** `Ty` has no refinement carrier (§1.1) and
  Cobrust has no dependent-type / SMT machinery, so a value-range predicate over
  arbitrary input is *provably* not statically decidable here. Structure IS statically
  decidable (once fields are tracked), so it is never re-checked at runtime.

### 5.3 OpenAPI derivation (Q4 — cannot drift, framing grafted from C)

A **compile-time Cobrust-native emitter pass** (NOT a derive macro — Cobrust has no
`.cb` proc-macro surface, ADR-0078 §4.a; NOT utoipa) walks the **same** recorded
field table + refinement side-table the validator reads, and emits the OpenAPI
`components/schemas/<Body>` JSON directly:

```
title: str where 1 <= len(self) <= 255   →  {"type":"string","minLength":1,"maxLength":255}
rank:  i64 where 0 <= self <= 100         →  {"type":"integer","minimum":0,"maximum":100}
body:  str                                →  {"type":"string"}
```

`str → {"type":"string"}`, `i64 → {"type":"integer"}`, `f64 → {"type":"number"}`,
`bool → {"type":"boolean"}`; `len(self) <= n` on a `str` → `maxLength` (the array-length
`maxItems` form is deferred to the Phase-4 list-field work); `lo <= self` → `minimum`;
`pattern(self, re)` → `pattern`. Because the validator AND the schema are
generated from **one source — the field table + side-table** — they are provably the
same artifact; **there is no second hand-written schema to drift** (drops footgun #4).
This realises ADR-0078 §9 Phase-3 strategy (a) verbatim: *Cobrust owns the type, so it
re-derives the schema from its own type info rather than through a Rust macro.*

### 5.4 pit integration (Q5)

`app.route_validated(method, path, handler)` is a **sibling manifest row** of
`app.route` (ADR-0073/0074), whose callback `FnTy` is
`fn(pit.Request, <Body>) -> pit.Response` — the existing `EcoParam::Callback(FnTy)` +
`CallbackSignatureMismatch` machinery type-checks the 2-arg shape for free
(`ecosystem.rs:344`, `error.rs:259`). At the trampoline (a sibling of
`__cobrust_pit_app_route`, `cabi.rs:246-310`):

1. box the `Request` (`Box::into_raw`, exactly as `cabi.rs:284`);
2. call the generated `validate_<Body>(req.json())` (`request.rs:137` supplies the
   `serde_json::Value`);
3. on `Ok(v)` box `v` (Rust-owned, same `Box::into_raw`/`from_raw` +
   `handle_drop_symbol → None` discipline as `Request`) and pass **both** `*mut u8`
   pointers to the `.cb` handler (`raw(req_raw, body_raw)`);
4. on `Err(ve)` synthesise a **typed 422 `pit.Response`** from the `ValidationError`
   *without entering the handler* (drops footgun #2 — the Result-error path stays in
   Rust, surfaced as a `Response`);
5. free both boxes exactly once on the way out (mirroring `cabi.rs:303`),
   `catch_unwind` across the C ABI (`cabi.rs:287`).

The handler body therefore only ever sees a fully valid typed value; the
Result-error path lives entirely in the trampoline, matching pit's Rust-owned-Request
discipline (ADR-0073 §2 D6).

## 6. Phased plan + Done-means per phase

Each phase: scope, layers, done-means, tractability. The phasing constraint (grafted
from C): **Phase-1 IS the field-tracking gate** — without it, the schema-derivation +
typed-body properties collapse to a runtime-schema degrade (ADR-0078 §4a-ii), which is
explicitly *not* #156's elegance.

### Phase 1 (the smallest end-to-end-real increment) — field tracking + ONE refinement kind (int range) + one validated route + its OpenAPI

The load-bearing prerequisite + one refinement kind, wired end-to-end:

```python
class CreateScore:
    name: str
    rank: i64 where 0 <= self and self <= 100

fn create_score(req: pit.Request, body: CreateScore) -> pit.Response:
    return pit.json_response(201, body)

fn main() -> i64:
    let app = pit.App()
    app.route_validated("POST", "/scores", create_score)
    app.run("127.0.0.1", 8080)
    return 0
```

**Scope (the slice):**
1. **(load-bearing) class field tracking** — `check_class` (`check.rs:757`) records
   `{name: Str, rank: Int}` into a per-Adt field table; the `Attr` arm
   (`check.rs:1287`) returns the recorded field `Ty` for a class-instance base instead
   of `fresh_var()` (`check.rs:1291`). *This is most of the work and the true gate.*
2. **`where lo <= self <= hi` parse** on an `i64` field → stored predicate AST in the
   `(AdtId, field)` side-table; reject any non-fixed-grammar predicate with a FIX-text
   `TypeError` (Q6).
3. **boundary validator** — emit `validate_CreateScore(serde_json::Value) ->
   Result<CreateScore, ValidationError>` doing the deserialize + the range-check only.
4. **`app.route_validated` manifest row** + the 2-arg callback `FnTy` (Q5) + the
   trampoline validate-or-422 + the dual-box ownership (§5.4).
5. **OpenAPI emit** — the `CreateScore` schema with `minimum`/`maximum` (§5.3).
6. **one E2E**.

**Layers touched** (mirrors ADR-0077 §9 / ADR-0078 §6.1; anchors at `5bfab21`, the
impl sprint re-greps):

| Layer | File | Site | Edit |
|---|---|---|---|
| **Typecheck (prereq)** | `crates/cobrust-types/src/check.rs` | `check_class` @757; `Attr` arm @1287/1291; `prebind_item` Class @519 | record class fields into a field table; `Attr` on a class-instance base returns the field `Ty` (not `fresh_var()`); (the zero-arg ctor @527 may stay for Phase-1 — the body is constructed Rust-side by the validator, not via a `.cb` ctor call) |
| **Refinement parse** | `crates/cobrust-frontend/src/parser.rs` (field-annotation parse) + a side-table in `cobrust-types` | the class-body field `Let` annotation | admit a postfix `where <fixed-pred>` on a field annotation; store the predicate AST keyed `(AdtId, field)`; reject non-fixed forms with a `TypeError` + suggestion |
| **Manifest** | `crates/cobrust-types/src/ecosystem.rs` | the pit `App` block (the `route` row @873-877) | add `route_validated` → `__cobrust_pit_app_route_validated`, callback `EcoParam::Callback(fn(pit.Request, <Body>) -> pit.Response)`; tier `PyCompatTier::Semantic` |
| **MIR** | `crates/cobrust-mir/src/lower.rs` | `try_lower_ecosystem_call` / `emit_ecosystem_call` (the handle-method retarget, ADR-0073/0078 anchors) | `app.route_validated(...)` retargets to `Constant::Str("__cobrust_pit_app_route_validated")` — no new mechanism, a different symbol string |
| **Codegen** | `crates/cobrust-codegen/src/llvm_backend.rs` | the pit extern block (ADR-0073 §4) | declare `__cobrust_pit_app_route_validated`; emit the `validate_<Body>` shim call shape (the body is a 2nd `*mut u8`) |
| **CLI build** | `crates/cobrust-cli/src/build/intrinsics.rs` | the `__cobrust_pit_*` recognizer | confirm `__cobrust_pit_app_route_validated` matches the existing pit prefix |
| **Runtime** | `crates/cobrust-pit/src/cabi.rs` + a new `cobrust-validator`-style shim | sibling of `__cobrust_pit_app_route` @246 | the 2-arg trampoline: deserialize+validate (`req.json()` @`request.rs:137`) → `Ok` dual-box dispatch / `Err` typed-422; dual `Box::from_raw` drop exactly once (mirror @303); `catch_unwind` @287 |
| **OpenAPI emit** | a Cobrust-native emitter pass (new, small) | walks the field table + side-table | emit `components/schemas/CreateScore` with `minimum`/`maximum`; served via pit (`GET /openapi.json`) |
| **Cargo** | `crates/cobrust-pit/Cargo.toml` (+ maybe a `cobrust-validator` crate) | `[dependencies]` | if a validation crate or `serde` derive is added, **stage `Cargo.lock`** (finding F64) |
| **Docs** | `docs/{agent,human/zh,human/en}` pit specs | the validated-route + body-class surface | per CLAUDE.md §3.3, in the impl commit |
| **Tests** | `crates/cobrust-pit/src/cabi.rs` `#[cfg(test)]` + a CLI E2E | mirror `trampoline_invokes_handler` + a `pit_validated_body_e2e.rs` | the §6-Phase-1 program + the negatives below |

**Done-means (Phase 1):**
- `POST /scores {"name":"a","rank":50}` → 201; `body.rank` type-checks as `i64` and
  `body.name` as `str` (a `body.rank + "x"` in the handler is a compile-time
  `TypeError`; a `body.nonexistent` is a compile-time error, not a runtime `KeyError`).
- `POST /scores {"name":"a","rank":200}` → **422** with a structured error body (the
  guard's `range` failure), and the handler is **never entered**.
- `POST /scores {"rank":50}` (missing `name`) and `{"name":"a","rank":"x"}` (wrong
  type) → 422 at the boundary (total deserialization), handler never entered.
- `GET /openapi.json` returns a valid OpenAPI doc whose `components/schemas/CreateScore`
  shows `{"name":{"type":"string"}, "rank":{"type":"integer","minimum":0,"maximum":100}}`,
  and that schema is *the same source* the validator used (a single field-table read —
  no second declaration).
- ≥3 negatives: `app.route_validated("POST","/s", h)` where `h` is 1-arg
  (`CallbackSignatureMismatch` + FIX); a field `where self ~ weird` non-fixed predicate
  (`TypeError` + FIX); a 2nd param that is not a field-tracked class
  (`CallbackSignatureMismatch` + FIX).
- every boxed `Request` + boxed body dropped exactly once (a `DROP_COUNT`-style
  assertion, mirroring den/pit); the existing pit route/pong E2E suite still passes
  (no regression); workspace gates green; `Cargo.lock` staged if a dep was added (F64).

**Tractability:** the body-param wiring + trampoline + OpenAPI-emit are bounded
(reuse the proven `Callback(FnTy)` gate + the `__cobrust_pit_app_route` ownership
template). **The honest cost is concentrated in item 1 — class field tracking — a
genuine type-checker capability, not a manifest row** (§8).

### Phase 2 — `where len(self) <= n` / `>= n` on `str` (+ `minLength`/`maxLength`)

Second refinement kind. **Done-means:** a `str` field with a length bound rejects
out-of-bound input with 422; `GET /openapi.json` shows `minLength`/`maxLength`; the
length is read from the *same* side-table the validator uses.

### Phase 3 — `where pattern(self, "<re>")` on `str` (+ `pattern`)

Regex refinement (a vetted Rust `regex` dep → **stage `Cargo.lock`**, F64).
**Done-means:** a pattern-failing body → 422; schema shows `pattern`.

### Phase 4 — the declarative-shape ergonomics + nested bodies + list fields

(a) optionally a lighter declarative spelling if the `class … where …` body proves
verbose; (b) a body field that is itself a field-tracked class (nested `$ref` in the
schema); (c) `list[T]` fields (`{"type":"array","items":…}` + `minItems`/`maxItems`).
**Done-means:** a nested body validates + its schema emits `$ref`; a list field with a
length bound validates + emits `items`/`maxItems`.

### Phase 5+ (deferred, §9 sub-ADRs) — the §2.5-superior compile-time-checked refinements; arbitrary cross-field constraints; the codegen-emits-Rust path

General refinement *checking* (proving `0 <= self <= 100` statically), cross-field
predicates (`start <= end`), and the C-style codegen-emits-Rust derive — each its own
sub-ADR (§9). These are the genuinely-large type-system / build investments;
Phase-1..4 deliberately keep all value-predicates as runtime guards.

## 7. §2.5 compliance note

| Surface | Python idiom (pydantic/FastAPI) | Cobrust shape | §2.5 overlap | Forced divergence |
|---|---|---|---|---|
| Body model (Ph1) | `class CreatePost(BaseModel): title: str` | `class CreatePost: title: str` | **~0.9** | no `(BaseModel)` base; the `class … : field: type` body is verbatim — the single highest-overlap shape (ADR-0078 §8) |
| Field access (Ph1) | `post.title` (runtime attr) | `post.title` (statically `str`) | **1.0** | none — and *stronger* than pydantic (compile-time-typed) |
| Int range (Ph1) | `rank: conint(ge=0, le=100)` / `Field(ge=0, le=100)` | `rank: i64 where 0 <= self and self <= 100` | **~0.6** | the `where`-predicate is novel; an LLM may not write it first-try (recovered from one §2.5-B FIX) — but it reads like `assert`/comprehension-guard, the closest spelling to LLM priors among the three approaches |
| Str length (Ph2) | `title: constr(max_length=255)` / `Field(max_length=255)` | `title: str where len(self) <= 255` | **~0.6** | same `where` novelty |
| Handler body param (Ph1) | `def create(post: CreatePost)` (FastAPI DI) | `fn create(req: pit.Request, post: CreatePost)` | **~0.85** | explicit `req` param (no DI magic, footgun #3) — the shape (a typed body param) matches; the explicitness is the deliberate divergence |
| Auto `/docs` (Ph1+) | FastAPI: free, derived from type hints + model | `GET /openapi.json` derived from the field table | **~0.8** | rises toward 0.9 as the schema is *exactly* the type the handler consumes (the FastAPI "free `/docs`" feel, now drift-free) |

**Compile-time-catch (§2.5-A):** STRONG and **strictly better than pydantic** — a
wrong field type, a missing field, a typo'd `body.field`, or a 1-arg handler on
`route_validated` is a **`TypeError` with a `suggestion` FIX string**
(`error.rs:8`, §2.5-B), the strongest LLM correction signal. pydantic catches the same
structure only at *runtime*. **The honest deficit (§2.5-B residual):** the *value*
refinements (`range`/`len`/`pattern`) surface at **runtime (422), not type-check** —
the same deficit ADR-0078 §8 / ADR-0077 Q4 record (handles/values carry no
shape/refinement in the type). The LLM still gets the *structure* caught at compile
time, which is the larger win; the §2.5-superior compile-time-checked-refinement form
is the Phase-5 sub-ADR (§9).

**Training-data-overlap (§2.5-A):** the declarative `class`-body is the
single-highest-overlap shape (ADR-0078 §8 scored the runtime-builder alternative ~0.5
and named the declarative class the §2.5-correct target — this IS that target). The
`where`-spelling is the one novelty, and is chosen over A's `@length` / C's
`where len(...)` *call* form precisely because `where <boolean expr over self>` is the
closest constraint syntax to an LLM's `assert`/comprehension-guard priors.

## 8. Honesty — what cannot be built yet, and why Phase-1 is the part buildable now

Stated plainly, per the dispatch's HONESTY mandate:

- **The load-bearing prerequisite — class/ADT field tracking — does NOT exist today**
  (`check.rs:1260` "the static core does not yet track ADT fields"; `check_class` @757
  is a stub; the `Attr` arm @1291 returns `fresh_var()` for a class-instance base;
  `Ty::Record` @`ty.rs:106` is unreachable from `lower_type` @`check.rs:2749`). #156's
  *entire elegance* — compile-time structure (footgun #1) + drift-free schema (footgun
  #4) + typed-body access (footgun #5) — is **contingent on first making the class
  field table real.** This touches `check_class`, the `Attr` arm, and (for full ctor
  support) the Adt constructor (real field args, not the zero-arg `() -> Adt` @527),
  borrow-check on `body.field`, and eventually MIR/codegen struct layout. **This is a
  language-feature sprint, not a crate-wrap.** ADR-0078 §3/§4.a rated validator/utoipa
  DEEP for exactly this reason ("there is no `.cb` type for the derive to reflect
  over"). Phase-1 IS this gate, scoped to the minimum (field *read* in `Attr`; the body
  value is constructed Rust-side by the validator, so a full `.cb` ctor-call path can
  be deferred).
- **General refinement-type *checking* does NOT exist and is NOT in scope.** Proving
  `0 <= self <= 100` statically (SMT-style) is a LARGE separate type-system project.
  The honest v1 move (Q6) is to keep **all** value-predicates as runtime guards and
  parse only a *fixed grammar*, rejecting anything else with a clear `TypeError`. A
  beautiful general refinement checker that stalls is worse than this narrow, real
  slice. The field-tracking work is the true gate; the refinements ride on top as a
  side-table (which is why the increment is feasible without touching `Ty`).
- **The codegen-emits-Rust path (Approach C / ADR-0078 §4a-i) is NOT used and NOT
  needed for #156.** Cobrust owns the type (Q4), so the schema is derived natively;
  emitting Rust source to feed a Rust derive is a detour. It is recorded as a §9
  sub-ADR for a hypothetical future where a `.cb` type must genuinely reach a Rust
  proc-macro — not a #156 dependency.
- **The runtime-schema degrade is the fallback, and it forfeits the point.** If field
  tracking proves too large for a sprint, the cheaper fallback is ADR-0078 §4a-ii
  (runtime-schema-as-data: `schema.field("rank", v.range(0,100))`). But that fallback
  **forfeits the compile-time-catch (footgun #1) and the cannot-drift property (footgun
  #4) that are the whole point of #156** — and (grafted from Approach C's honesty) *a
  "#156" that ships the runtime degrade is a B-shaped supporting feature wearing the
  FastAPI-DEFINING name.* **Sequence #156 behind — or AS — the "class fields are typed"
  milestone (Phase-1), or it collapses to the degrade.** This is the central honesty of
  this ADR.

**No overclaim:** there are no benchmarks in this ADR (none were run); the §7 §2.5
scores are estimates in the ADR-0078 §8 tradition, not measurements; the feasibility
claims are each tied to a verified source seam (§1.1).

## 9. Open questions for sub-ADRs

- **Compile-time-checked refinements (the §2.5-superior future).** Can Cobrust gain a
  *bounded* static refinement check (interval analysis for int ranges; not full SMT) so
  `rank: i64 where 0 <= self <= 100` is partly compile-time-caught? The §2.5-A upgrade
  for the value-predicate deficit (§7). The hardest type-system investment in the set.
- **The declarative-shape sugar.** If `class … where …` proves verbose, a lighter
  spelling (closer to pydantic's bare `Field(...)`) — but only if it raises §2.5 overlap
  without re-introducing a footgun. A bounded follow-up.
- **Cross-field + collection constraints.** `where start <= end` (two fields),
  `list[T] where len(self) <= n` (Phase-4), unique-items. Needs the predicate grammar to
  reference sibling fields — a side-table-shape extension.
- **The `.cb`-value ↔ serde bridge** (shared with ADR-0078 §9). The validator
  deserializes `serde_json::Value → <Body>` and re-serializes `<Body> → json` for the
  response; a general `.cb`-struct↔serde bridge unblocks this + arbitrary-claims JWT +
  utoipa-style reflection at once (ADR-0078 §9).
- **The codegen-emits-Rust derive (Approach C / ADR-0078 §4a-i).** Recorded for the
  hypothetical future where a `.cb` type must reach a Rust proc-macro. NOT a #156
  dependency (Cobrust owns the type, Q4); kept as a cross-reference so the option is not
  re-litigated.
- **Typed route paths + path-params (footgun #5 residual).** The body is typed
  end-to-end; the route *path* string + path-params remain stringly-typed (§3, scoped
  out of #156). A follow-up: a typed path-param surface
  (`@app.get("/posts/{id}") fn(id: i64, …)`).
- **The full Adt constructor.** Phase-1 constructs the body value Rust-side (the
  validator builds it); a `.cb` `CreatePost("a", 50)` ctor-call path (replacing the
  zero-arg `() -> Adt` @`check.rs:527` with a real field-args ctor) is the natural
  follow-up once field tracking lands, and is independently valuable for every future
  `.cb` struct.

## 10. Consequences

- **Positive:** picks the FastAPI-DEFINING capability's mechanism with the
  elegance-law footgun ledger made explicit (§3 — all six named with a cited seam);
  chooses the approach (B) with the **smallest correct first increment on the type
  system as it actually is** (refinement side-table leaves `Ty` untouched; body-param
  reuses the proven `Callback(FnTy)` gate); states the compile-time/runtime split as a
  clean law ("shape is a type, a value-predicate is a guard", grafted from A); makes the
  OpenAPI schema a *projection of the one field table* so it **cannot drift** (footgun
  #4, framing grafted from C); and realises ADR-0078 §9 Phase-3 strategy (a)
  (Cobrust-native schema emit, not a crate-wrap).
- **Negative / accepted:** (1) the **load-bearing prerequisite (class field tracking)
  does not exist** (§1.1/§8) — Phase-1 IS that gate, a language-feature sprint not a
  manifest row. (2) The *value* refinements are **runtime-only** (422), not
  compile-time-caught — the §2.5-B residual (§7), the §2.5-superior form deferred to a
  §9 sub-ADR. (3) The refinement language is a **fixed grammar** in v1 (Q6) — no general
  refinement checking. (4) The route *path* stays stringly-typed (footgun #5 residual,
  §3). (5) If field tracking proves too large, the runtime-schema degrade is the
  fallback — but it **forfeits #156's whole point** (§8).
- **Risk — the degrade trap (the central one):** a "#156" that ships the ADR-0078 §4a-ii
  runtime-schema degrade is a supporting-cast B wearing the FastAPI-DEFINING name (§8,
  grafted from C). The mitigation is sequencing: **Phase-1 IS the field-tracking gate**;
  do not declare #156 done on the degrade.
- **Risk — manifest drift:** `route_validated` joins the hand-maintained manifest
  (ADR-0072 §5 R4 accepted debt; generation still deferred).
- **Risk — Cargo.lock staging (F64):** Phases 2/3 add deps (a validator crate / `regex`
  / `serde` derive); **stage `Cargo.lock`** or `--locked` CI cluster-fails
  build/clippy/test.
- **Risk — borrow-check on `body.field`:** once `Attr` returns a real field `Ty`, moves
  out of `body.field` interact with the borrow checker (ADR-0060b `&`/let-rebind) — a
  Phase-1 done-means check, not a new mechanism.
- **Follow-up:** ratify draft→accepted when the Phase-1 field-tracking + one-refinement
  + `route_validated` impl sprint lands + passes the §6 done-means + a paired ADSD
  audit; open the §9 sub-ADRs (the compile-time-checked-refinement upgrade + the
  `.cb`↔serde bridge first, as they unblock both the §2.5-A residual and several DEEP
  surfaces at once).

## 11. Evidence

- **Source ground truth (verified at `5bfab21`):**
  - `crates/cobrust-types/src/check.rs` — `prebind_item` Class @519-530 (class = zero-arg
    ctor `() -> Adt(AdtId, [])`); `check_class` @757-762 (stub — recurses members, records
    no fields); `Attr` arm @1250-1291 (tuple-field + `lookup_handle_attr`, else
    `fresh_var()` @1291); the verbatim "the static core does not yet track ADT fields"
    @1260-1261/1283-1285; `lower_type` @2749-2788 (no `Ty::Record` arm).
  - `crates/cobrust-types/src/ty.rs` — `pub enum Ty` @39-101 (no refinement carrier);
    `Record { fields: BTreeMap<String, Ty> }` @106-120 (exists, unifies field-wise per
    ADR-0006); `Ty::Adt(AdtId, Vec<Ty>)` @71.
  - `crates/cobrust-frontend/src/ast.rs` — `pub enum TypeKind` @255-277 (Name / Generic /
    Union / Fn / Tuple / Ref / Array — **no record/struct literal**).
  - `crates/cobrust-frontend/src/parser.rs` — `parse_decorated` @319-351 (decorators
    attach ONLY before `fn`/`class`; explicit reject + suggestion otherwise — grounds the
    Approach-A grammar-collision finding).
  - `crates/cobrust-hir/src/tree.rs` — `ClassBody.members: Vec<Item>` @69-74; `ItemKind`
    @30 incl `Class(ClassBody)` @42 + `Let(LetBody)` @52 (a class field is an `ItemKind::Let`).
  - `crates/cobrust-types/src/ecosystem.rs` — `pit_handler_fn_ty()` @198
    (`fn(pit.Request)->pit.Response`); `EcoParam::Callback(FnTy)` @344; `EcoSig` @349;
    `PyCompatTier` @316; the pit `route` row @873-877 (`EcoParam::Callback(pit_handler_fn_ty())`,
    `runtime_symbol: "__cobrust_pit_app_route"`).
  - `crates/cobrust-types/src/error.rs` — `suggestion: Option<&'static str>` on every
    `TypeError` variant @8/26/…; `ImplicitTruthiness` @87; `MutableDefault` @104;
    `CallbackSignatureMismatch` @259.
  - `crates/cobrust-pit/src/request.rs` — `body()` @118; `json() -> Result<serde_json::Value,
    PitError>` @137.
  - `crates/cobrust-pit/src/cabi.rs` — `__cobrust_pit_app_route` @246-310 (Rust-owned boxed
    Request @284, `catch_unwind` @287, freed exactly once @303; handler `CbHandlerAbi`
    transmute @268).
  - `examples/z8_rest_blog/main.cb` — @42 ("Structured JSON-dict access lands with the
    coil-deep type work"), @72-78 (`req.body()` + chained `replace`/`split` body-parse —
    the real pain #156 removes).
- **ADRs:** ADR-0078 (the parent — DEEP validator/utoipa, §4.a derive-blocker + 4a-i/4a-ii
  fork, §8 declarative-class §2.5-correct target, §9 Phase-3 strategy (a) Cobrust-native
  emitter — this ADR is that strategy made concrete); ADR-0072 (ecosystem-import chain +
  flat manifest); ADR-0073 (callback marshalling — the pit trampoline + Rust-owned-Request
  ownership split §2 D6 — the `route_validated` template); ADR-0074 (decorator desugar —
  explicit register-call, no DI); ADR-0077 (operator/attr surface — §9 impl-map structure +
  §2.5/§Q4 shape-correctness-is-runtime honesty mirrored here); ADR-0006 (type system —
  `Ty::Record` field-wise unification); ADR-0050d / ADR-0052a / ADR-0060b (`&`/let-rebind
  borrow surface — the `body.field` move interaction).
- **Constitution:** CLAUDE.md §2.2 (`Result`-default + no exceptions-as-control-flow +
  mutable-default-arg error — footguns #2/#6); §2.5 (LLM-first: compile-time-catch §7 +
  training-data-overlap; §2.5-B error-UX FIX text — the `suggestion` channel); §5.1
  (elegant — newtypes only where invariants exist, the side-table-not-`Ty`-widening call,
  Q2).
- **Findings:** F64 (dev-dep `Cargo.lock` staging — Phases 2/3 add deps).
- **Feedback:** `feedback_elegant_ecosystem_surface_no_legacy_debt` (the elegance-law — the
  mandatory §3 footgun ledger; #156 named as the most-governed backend surface).
- **External refs:** pydantic models
  (https://docs.pydantic.dev/latest/concepts/models/), FastAPI request body +
  validation (https://fastapi.tiangolo.com/tutorial/body/), OpenAPI 3.1 schema object
  (https://spec.openapis.org/oas/v3.1.0#schema-object), Rust `validator` crate
  (https://docs.rs/validator/latest/validator/), `utoipa`
  (https://docs.rs/utoipa/latest/utoipa/).

## Phase-3a (f64 / `FloatRange`) — amendment

> **Status:** landed (impl + tests + dual docs). The precise MIRROR of the
> Phase-1 `Refinement::IntRange` on an `f64` field. This amendment records the
> CTO-decided design (D1–D6) and the cannot-drift encode↔decode contract. It is
> a **pure addition** — the `i64`/`str` surface stays byte-identical.

Phase-3a adds value-range validation to an `f64` field via a single new
refinement variant `Refinement::FloatRange { lo: Option<f64>, hi: Option<f64> }`.
`bool` value-validation is OUT of this scope (deferred to a later 3b).

### Design (D1–D6)

- **D1 — drop `Eq` from `Refinement`'s derive** (`#[derive(Clone, Debug, PartialEq)]`).
  `f64` is `PartialEq` but not `Eq` (IEEE-754 `NaN != NaN`), so a `FloatRange`
  carrying `f64` bounds forces `Refinement` to `PartialEq` only. **This is SAFE
  and was verified at impl time:** `Refinement` is used EXCLUSIVELY as a
  `HashMap` VALUE (`adt_refinements: HashMap<(AdtId, String), Refinement>` —
  `check.rs:52/477/576` + `TypedModule`); it is NEVER a `HashMap`/`HashSet` key,
  no site bounds it `: Eq`/`: Hash`, and the enclosing `TypedModule`/`Ctx`/
  `TypeCheckCtx` derive only `Clone, Debug, Default` (no transitive `Eq`
  requirement). The SAME `Eq`-drop applies to `cobrust-pit`'s `ValidationError`
  (which now carries `FloatOutOfRange { value: f64, … }`) for the identical
  reason — it is only `==`/`matches!`-compared in tests + flows through
  `Result`/`Err`, never a key. Were any future site to need `Refinement: Eq`, it
  would fail to compile loudly at that site (no silent semantic change).
- **D2 — `Refinement::FloatRange { lo, hi }`**, INCLUSIVE, ≥1 bound `Some`
  (both-`None` is meaningless and is rejected at parse-interpretation, exactly as
  `IntRange`). The fixed grammar admits ONLY the inclusive operators `<=`/`>=`
  on an `f64` field; a STRICT `<`/`>` bound is **rejected** with the §2.5-B FIX.
  Rationale: the integer grammar rewrites `S < N` to `<= N-1`, but the reals are
  dense — a float strict bound has no clean inclusive ±1 rewrite, so inventing an
  epsilon would be a silent footgun. `NaN`/`inf` are not producible by the
  grammar (`literal_float_value` rejects a non-finite literal), so the validator's
  partial-order comparison is total in practice.
- **D3 — the descriptor encode↔decode (cannot-drift pair, footgun #4).**
  `descriptor_payload("f64", FloatRange{lo,hi})` → `f64:<lo>:<hi>` via a new
  `float_suffix(lo, hi: Option<f64>)` dual to `int_suffix`, rendering each bound
  with `f64`'s `Display` (shortest round-trippable decimal). Examples:
  `0 <= self and self <= 100` → `f64:0:100`; one-sided `0 <= self` → `f64:0:`;
  `self <= 100` → `f64::100`; a fractional `0.5 <= self <= 99.9` → `f64:0.5:99.9`.
  `cobrust-pit`'s `parse_schema` DECODEs the `f64` kind with a new
  `parse_float_suffix` (`str::parse::<f64>()`), which accepts every string `f64`
  `Display` emits — the two halves round-trip exactly. The `f64` bounds live in a
  SEPARATE `FieldSpec` pair (`lo_f`/`hi_f: Option<f64>`) because a fractional
  bound (`0.5`) is not representable in the integer `lo`/`hi`.

  **The encode↔decode contract string (the cannot-drift pair):**

  ```text
  cobrust-types  Refinement::FloatRange{lo,hi}.descriptor_payload("f64")
                 = format!("f64{}", float_suffix(lo, hi))
                 = "f64:" + lo.map(f64::Display).unwrap_or("") + ":" + hi.map(f64::Display).unwrap_or("")
  cobrust-pit    parse_schema → FieldKind::F64 → parse_float_suffix(suffix)
                 = (suffix[0].parse::<f64>().ok(), suffix[1].parse::<f64>().ok())
  ```

  round-trips because `∀ x: f64.is_finite() ⇒ x.to_string().parse::<f64>() == Ok(x)`.

- **D4 — OpenAPI projection.** An `f64` `FloatRange` → `{"type":"number",
  "minimum":lo, "maximum":hi}` (an absent bound is omitted), emitted from the
  SAME `parse_schema` output the validator reads (`field_schema`'s new
  `FieldKind::F64` arm reads `lo_f`/`hi_f` and emits them as JSON numbers via
  `serde_json::Number::from_f64`, which only fails on NaN/inf — never produced).
  Concrete: `ratio: f64 where 0 <= self <= 1 → {"type":"number","minimum":0,
  "maximum":1}`; `score: f64 where 0.5 <= self → {"type":"number","minimum":0.5}`.
- **D5 — the `where`-parse front-end (`check.rs`).** `interpret_refinement`
  gains a `Some(&Ty::Float)` arm dispatching to a new `interpret_float_range`,
  which threads a new `parse_bound_predicate_f64` (the `f64` dual of
  `parse_bound_predicate`, identical contradiction-detection) over a new
  `parse_subject_bound_f64` + `literal_float_value`. The float bound-parser
  accepts BOTH float literals (`0.0`) and integer literals (`0` widens to `0.0`,
  the natural spelling that matches LLM priors, §2.5). Without this front-end arm
  no `.cb` source could ever produce a `FloatRange`.
- **D6 — §2.5.** A range violation renders a typed 422 whose detail PRINTS THE
  FIX (mirroring `IntRange`): `field \`ratio\` value 1.5 must be in [0, 1]`. A
  refinement on the wrong base type (a `len(self)`/`pattern` on an `f64` field, a
  strict `<`, an arbitrary fn call) is a **compile-time** `TypeError::Unsupported​Refinement`
  with a FIX suggestion — the strong §2.5-A compile-time-catch signal.

### Worked `.cb` examples

```python
# (1) two-sided inclusive float range — the canonical form
class Reading:
    name: str
    ratio: f64 where 0.0 <= self and self <= 1.0
# descriptor:  ratio<TAB>f64:0:1
# OpenAPI:     ratio → {"type":"number","minimum":0,"maximum":1}
# 422 detail:  field `ratio` value 1.5 must be in [0, 1]

# (2) one-sided bounds + integer-literal bounds (widen to f64)
class Sensor:
    low:  f64 where 0.5 <= self          # descriptor f64:0.5:  → {minimum:0.5}
    high: f64 where self <= 100          # descriptor f64::100  → {maximum:100}

# (3) the validated route, end-to-end
fn submit(req: pit.Request, body: Reading) -> pit.Response:
    return pit.text_response(201, "ok")   # reached ONLY if 0.0 <= ratio <= 1.0
fn main() -> i64:
    let app = pit.App()
    app.route_validated("POST", "/readings", submit)
    app.serve_openapi("/openapi.json")
    app.run("127.0.0.1", 8080)
    return 0
```

### Done-means (Phase-3a) — all green

- `POST /readings {"name":"a","ratio":0.5}` → 201, handler entered; an
  integer-valued `ratio:1` → 201 (an integer is a valid f64).
- `ratio:1.5` (> max) and `ratio:-0.5` (< min) → **422** with a FIX-printing
  detail, handler **never entered**; `ratio:"x"` → 422 wrong type
  (`must be of type number`).
- `GET /openapi.json` shows `ratio → {"type":"number","minimum":0,"maximum":1}`
  (NOT `integer`, NOT `minLength`/`maxLength`) — the SAME source the validator
  used (cannot-drift, asserted by a paired 422-and-`maximum:1` test).
- Negatives compile-rejected with a FIX: `len(self)`/`pattern` on an `f64`
  field; a strict `<` bound (D2); an arbitrary `weird(self)` call.
- `i64`/`str` surface byte-identical (pure addition); the encode↔decode pair
  preserved; workspace gates green.

### Evidence (Phase-3a)

- **Encode:** `crates/cobrust-types/src/refinement.rs` — `Refinement::FloatRange`,
  `float_suffix`, the `descriptor_payload` `f64` arm; the `#[derive(…, PartialEq)]`
  Eq-drop + its safety rationale.
- **Front-end:** `crates/cobrust-types/src/check.rs` — `interpret_refinement`
  `Ty::Float` arm, `interpret_float_range`, `parse_bound_predicate_f64`,
  `parse_subject_bound_f64`, `literal_float_value`.
- **Decode + validate:** `crates/cobrust-pit/src/validation.rs` —
  `ValidationError::FloatOutOfRange` (+ its FIX-printing `detail`), `FieldSpec`'s
  `lo_f`/`hi_f`, `parse_float_suffix`, the `FieldKind::F64` range-check arm,
  `float_out_of_range`; the `ValidationError` Eq-drop.
- **OpenAPI:** `crates/cobrust-pit/src/openapi.rs` — `field_schema`'s
  `FieldKind::F64` `minimum`/`maximum` arm.
- **MIR:** unchanged — `lower.rs:2324` already computes `kind="f64"` for
  `Ty::Float` and calls `descriptor_payload(kind)` generically, so the FloatRange
  suffix renders with no MIR edit.
- **Tests:** `crates/cobrust-types/src/refinement.rs` (`float_range_*` unit
  tests); `crates/cobrust-pit/src/{validation,openapi}.rs` (`float_*` /
  `f64_*` unit tests); `crates/cobrust-types/tests/well_typed.rs` (w211–w214);
  `crates/cobrust-types/tests/ill_typed.rs` (i165–i168);
  `crates/cobrust-cli/tests/pit_float_refinement_e2e.rs` (3 live-server
  round-trips + 2 compile negatives).
