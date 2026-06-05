---
doc_kind: adr
adr_id: 0081
title: Validated-body field READ + `json_response(status, body)` — the `.cb` ↔ serde bridge (ADR-0080 §9 made concrete). A validated body is the `serde_json::Value` it already is; `body.field` reads via a TYPED accessor shim keyed on the field's declared `Ty` (Value-backed, NOT a native struct, NOT a stringly-typed `.get`); the accessor SEAM names a symbol + a `Ty` (never serde/a JSON key in MIR) so a future native-struct ABI swaps the backing with zero `.cb`-source churn. Native struct layout + `.cb`-constructed-class field storage are the honest long-term endpoint, explicitly deferred.
status: accepted
date: 2026-05-30
decision_owner: cto
last_verified_commit: ba8cca6
relates_to: [adr:0006, adr:0060b, adr:0072, adr:0073, adr:0074, adr:0077, adr:0078, adr:0080, "claude.md:§2.2", "claude.md:§2.5", "claude.md:§5.1", "finding:F64", "feedback:elegant_ecosystem_surface_no_legacy_debt"]
---

# ADR-0081: validated-body field READ + `json_response` — the `.cb` ↔ serde bridge

## 1. Context — the gap left by ADR-0080, and the runtime-no-field-storage finding

ADR-0080 landed the FastAPI-DEFINING #156 capability and **its Phase-1 + Phase-2
have SHIPPED** (verified, §1.1): a `.cb` `class` body declares typed fields
(`adt_fields`), per-field refinements live in a side-table (`adt_refinements`),
`app.route_validated` type-checks a 2-arg `fn(pit.Request, <Body>) -> pit.Response`
handler, the trampoline deserialize-or-422s the JSON body, and a Cobrust-native
OpenAPI pass emits the schema from the same field table (cannot drift). That is the
*validation + schema* half.

What ADR-0080 left open is the **consumption** half, recorded in its §9 (".cb-value ↔
serde bridge") and §6 Phase-1's `return pit.json_response(201, body)` line: **once a
validated body reaches the handler, the handler cannot actually USE it.** Two precise
gaps:

1. **`body.field` reads type-check but lower to garbage.** The checker's `Attr` arm
   (ADR-0080 Phase-1a) returns the *declared field `Ty`* for `body.rank`
   (`check.rs:1610-1614`) — so `body.rank` is statically `i64` and a typo is a
   compile-time `UnknownField` (`check.rs:1621`). But there is **no codegen/runtime
   backing**: the MIR `Attr` rvalue arm (`lower.rs:1445-1477`) checks
   `lookup_handle_attr` first (the coil.Buffer.shape seam), and for any *non-handle*
   base — which a user body class is — falls through to a placeholder
   `Projection::Field(0)` that **discards the field name** (`let _ = name;`,
   `lower.rs:1476`); the lvalue arm does the same (`lower.rs:672-674`). Codegen's
   `lower_place_load` (`llvm_backend.rs:4435`) has **no `Projection::Field` arm at
   all** — `Field(_)` falls into the final `else` → a bare-local stub load
   (`llvm_backend.rs:4564-4573`). So `body.rank` at runtime loads the wrong slot. The
   typed surface is real; the runtime read is a no-op stub.
2. **`json_response(status, body)` is unwired.** Only `__cobrust_pit_text_response`
   exists (`cabi.rs:193`); the handler-return reclaim assumes a `Response` from it
   (`cabi.rs:494`). The §6 Phase-1 handler's `return pit.json_response(201, body)`
   has nothing to call.

### 1.1 Ground truth — verified at `8dae584` (NO-OVERCLAIM)

The decisive finding, read from source, that shapes every option below: **a `.cb`
class instance has ZERO runtime field storage today.**

- **Adt construction = a null/opaque pointer.** Codegen lowers
  `AggregateKind::Adt(_, _)` to `self.emitter.opaque_ptr_ty.const_null()`
  (`llvm_backend.rs:5016`); `lower_ty` collapses every `Ty::Adt` to `opaque_ptr_ty`
  (`llvm_backend.rs:3486`). A class instance is, at runtime, an **opaque token over an
  empty allocation** — there is no record, no GEP, no field slot.
- **No `Projection::Field` load path, and `#![forbid(unsafe_code)]` blocks the obvious
  one.** `lower_place_load` materialises only the empty-projection load, a
  `[Deref]`, and a *constant-index* `Array` extract; `Field(_)` hits the stub-load
  `else` (`llvm_backend.rs:4564`). Codegen is `#![forbid(unsafe_code)]`
  (`cobrust-codegen/src/lib.rs`), which **already forced the dynamic-index Array path
  OFF inkwell's unsafe GEP onto a runtime helper** (`__cobrust_array_get_*`,
  `llvm_backend.rs:4476-4520`). A native struct-GEP field load would re-fight exactly
  this constraint.
- **The validated body is ALREADY a `serde_json::Value` behind the handler's 2nd
  `*mut u8`.** The shipped `__cobrust_pit_app_route_validated` trampoline
  (`cabi.rs:402`) validates `req.json()`, then `Box::into_raw(Box::new(value))`s the
  `serde_json::Value` (`cabi.rs:464`) and passes BOTH `req_raw` + `body_raw` to a
  2-arg `CbValidatedHandlerAbi` (`cabi.rs:352`, `:470`); it frees the box as a
  `serde_json::Value` exactly once (`cabi.rs:479`). **The body the handler holds is a
  boxed serde `Value`, not a `.cb` struct.**
- **The handler-body local DOES carry the real class id (not the sentinel).** The
  `route_validated` callback `FnTy`'s 2nd slot is the SENTINEL
  `PIT_VALIDATED_BODY_SENTINEL_ADT` (`ecosystem.rs:238`) — the manifest cannot name a
  user class — but the *route-shape gate* (`check_validated_body_param`,
  `ecosystem.rs:220-232`) substitutes "any field-tracked user `Ty::Adt` with id
  OUTSIDE the ecosystem-handle range." Inside the handler body, the param annotation
  `body: CreateScore` binds the local to the **real** `Ty::Adt(CreateScore)` (ADR-0080
  Phase-1b-i, commit `7c58bd5`) — which is *why* `body.rank` type-checks against
  `adt_fields` at all. So `synth_expr_ty(body)` in MIR yields the real class id; the
  sentinel lives only in the route-shape gate.
- **The `(ptr, ptr) -> ptr` accessor-shim template EXISTS and is in production.**
  `__cobrust_pit_request_path_param(req: *mut u8, name: *mut u8) -> *mut u8`
  (`cabi.rs:755-765`) does `read_str_buf(name)` → `req.path_param(&name).unwrap_or("")`
  → `alloc_str_buffer(...)`. This is the exact shape a body-field-read accessor clones.
- **The retarget SEAM EXISTS.** The MIR `Attr` arm already routes a handle base through
  `lookup_handle_attr → emit_ecosystem_call` with a borrowed receiver
  (`lower.rs:1457-1465`); the lvalue/callee Attr paths mirror it
  (`lower.rs:2017-2120`, `:3137-3140`). `lookup_handle_attr` today gates on
  `COIL_BUFFER_ADT` + `is_ecosystem_handle` (`ecosystem.rs:312` = `id.0 >=
  ECO_ADT_BASE`), so a user body class (id `< ECO_ADT_BASE`) returns `None` and falls
  through to the stub.
- **`Response::json` + `with_status` ALREADY give `json_response`.** `Response::json(&serde_json::Value)`
  sets `content-type: application/json` + serializes (`response.rs:49-58`);
  `.with_status(s)` overrides the code (`response.rs:74`). `json_response(status, body)`
  is `Response::json(&value).with_status(status)` — trivial.

**Conclusion.** The validated body's *structure* is already typed at compile time
(footgun #5 won by ADR-0080); the body's *runtime* is a serde `Value`. So the bridge
question is **not** "build a struct" — it is "expose typed reads + a typed re-serialize
over the `Value` that already crosses the boundary, behind a seam that does not weld the
`.cb` source to the `Value` representation." The native-struct option is *also* possible
but is gated on a real codegen subsystem (per-Adt LLVM struct type + `#![forbid(unsafe_code)]`-safe
field load + generated (de)serializer + a real field-args ctor) — the same M5+/§9-grade
investment ADR-0080 §9 ("the full Adt constructor") + ADR-0078 §9 (".cb↔serde bridge")
already point at. This ADR picks the buildable-now bridge and names the struct ABI as the
honest endpoint with an explicit migration seam.

## 2. Decision (summary)

| # | Question | Decision |
|---|---|---|
| Q1 | What a validated body IS at runtime | **The `serde_json::Value` it already is** (`cabi.rs:464`). NOT a native `.cb` struct (no field storage exists, §1.1), NOT re-boxed. A *typed view* over the boxed Value, exactly as the validator left it. |
| Q2 | How `body.field` reads | **A TYPED accessor shim per declared `Ty`** — `__cobrust_pit_body_get_{i64,f64,bool,str}(body: *mut u8, name: *mut u8) -> <ret>` — cloned bit-for-bit from the `(ptr,ptr)->ptr` `path_param` template (`cabi.rs:755`). The MIR `Attr` arm retargets `body.field` to the right shim **keyed on the field's declared `Ty`** (read from `adt_fields`), passing the receiver + the compiler-synthesized field-name `Str`. The shim borrows `&serde_json::Value`, does a typed get (`v.get(name).and_then(as_i64)` etc.), `alloc_str_buffer`s strings. Because validation already proved presence+type+range, the lookup is **total** — the fail-clean `unwrap_or` sentinel is unreachable on the validated path (mirrors `path_param`'s `unwrap_or("")`). |
| Q3 | `json_response(status, body)` | **A new manifest free-fn + `__cobrust_pit_json_response(status: i64, body: *mut u8) -> *Response`** — the sibling of `text_response` (`cabi.rs:193`), differing only in the 2nd param being the boxed `Value` and the body being `Response::json(&*body).with_status(status)` (`response.rs:49`/`74`). It **borrows** the body box; the trampoline (`cabi.rs:479`) still frees it exactly once. |
| Q4 | The dispatch gate (THE load-bearing risk, §4) | **REGISTRATION-DRIVEN.** Fire the serde-accessor retarget ONLY for a base local that the *checker recorded as a `route_validated` body-param* — NOT for any `Ty::Adt`-with-a-field-table. A `.cb`-*constructed* instance (`let s = Score()`) has the same `Ty::Adt(real-id)` + field table but its `*mut u8` is a null/opaque pointer (§1.1), so a serde shim would `cast::<Value>()` garbage → UB. **The "param origin" information does NOT exist at HEAD** (route-shape validation is call-site-only at `check_eco_sig` ~`check.rs:2641`, recorded nowhere a fn body can read; MIR `Body`/`LocalDecl` carry only `param_count` + a `Ty`, no origin flag — §5.2). So the gate is built as a concrete channel: the checker COLLECTS each accepted `app.route_validated(method, path, handler)` into a new `TypedModule.validated_handlers: HashMap<DefId, (usize /*body param idx*/, AdtId /*body class*/)>` (the SAME checker→MIR channel as `adt_fields`); MIR, lowering a fn whose `DefId` is in that map, MARKS the body-param local via a NEW `validated_body_of: Option<AdtId>` on `LocalDecl`/`Body`; the serde shim fires ONLY when the base resolves to a local with `validated_body_of == Some(id)` AND the field ∈ that class's `adt_fields`. See §5.2 for the exact channel. |
| Q5 | Where the representation choice lives (the SEAM) | **Behind the accessor symbol.** `body.field` lowers to *a borrowed-receiver call to an accessor symbol returning the field's declared `Ty`* — MIR names **a symbol + a `Ty`, NEVER `serde_json` or a JSON key.** Today the symbol indexes a `Value`; a future native-struct ABI emits a real struct + a `Projection::Field` load behind the SAME accessor — the `.cb` source and the MIR shape do not change. The Value-vs-struct choice is entirely behind the symbol. |
| Q6 | Native struct ABI / `.cb`-constructed-class field storage | **DEFERRED to a §7 sub-ADR (the honest long-term endpoint).** It is gated on a real codegen subsystem (§1.1) and is NOT needed to ship the §6-Phase-1 surface. This ADR does not pretend the Value-backed view is the struct story (§6 is explicit). |

## 3. Design principle — the elegance-law footgun ledger (the `.cb` ↔ serde bridge)

The elegance-law (`feedback_elegant_ecosystem_surface_no_legacy_debt`) mandates an
explicit ledger: name each footgun a JSON-body-consumption surface could re-introduce,
and the decision that drops it. Each row cites the seam.

| # | Footgun (Express / Flask / FastAPI / hand-rolled serde) | Cobrust #156-read decision that DROPS it |
|---|---|---|
| **1** | **Stringly-typed body access** — `req.body["rank"]` / `body.get("rank")`; the JSON key is author-written; a typo is silent or a runtime miss. | **DROPPED.** The author writes `body.rank` — a typed attribute, never a string. The `"rank"` key is **compiler-synthesized** from the resolved field name (Q2); it never appears in `.cb` source. A typo'd field is a compile-time `UnknownField` with a FIX (`check.rs:1621`, §2.5-B), caught by the field table ADR-0080 already records. |
| **2** | **Runtime `KeyError` / `None`-unwrap on a present-but-misnamed field.** | **DROPPED on the validated path.** Validation already proved presence+type+range *before* the handler ran (`validate_against_schema`, `cabi.rs:442`). The accessor's get is **total**; the `unwrap_or` sentinel is unreachable on a value that entered the handler (it is a fail-clean defense, mirroring `path_param`'s `unwrap_or("")` — not a `KeyError` surface). |
| **3** | **Silent numeric coercion on read** — `body.rank` reads `1.5` and truncates to `1` (the classic JS/`as f64`→`as i64` footgun §2.2 forbids). | **DROPPED.** The accessor for an `i64` field uses `serde_json::Value::as_i64` (integer-only), **never `as_f64`-then-truncate**. Validation already rejects a float for an `i64` field (the refinement/type check), so the shim inherits that guarantee — but the shim code is constrained (a Phase-1 done-means review item) to NOT widen it. CLAUDE.md §2.2 (no silent coercion) is honored at the read. |
| **4** | **Re-serialization drift** — a handler hand-builds the response JSON, diverging from the body's actual shape. | **DROPPED.** `json_response(201, body)` re-serializes the **same `serde_json::Value`** the validator produced (Q3) — no hand-rebuild, no field-by-field copy, so the response body cannot drift from the validated body. |
| **5** | **Exceptions as the error path on a malformed body.** | **DROPPED (inherited from ADR-0080).** The malformed-body case never reaches the handler — it is the trampoline's typed-422 `Result` arm (`cabi.rs:447-458`). The accessor only ever sees a validated value, so it has no error path to throw. |

**Honest residual (NOT a footgun, an accepted cost):** the Value-backed read is **not
zero-cost** — `body.rank` is a `serde_json::Value` map get + a typed coerce per access,
not a struct load (§5.1's zero-cost-abstraction ideal). For a handful of fields per
handler this matches the existing per-request cost profile (the validator already walks
the value once); it is honest debt the native-struct endpoint (Q6/§7) retires, **not** a
silently-introduced footgun. Named here, not hidden.

## 4. Options considered — the three approaches, scored

All three approaches were independently grounded against the codebase and **converge on
the same buildable runtime** — the body is a boxed `serde_json::Value`, read via accessor
shims over the `lookup_handle_attr → emit_ecosystem_call` seam, with `json_response` a
`text_response` sibling. They differ on *framing* and *how much object-model machinery
they claim to build*. Judged in the mandated priority order: **(1) feasibility/grounding
→ (2) §2.5 first-try → (3) elegance → (4) smallest-correct-increment.**

A correction the synthesis makes to all three memos: they were authored against a stale
`5bfab21` baseline and treat "class field tracking" as the absent gate. **At HEAD
(`8dae584`) ADR-0080 Phase-1 + Phase-2 have SHIPPED** — `adt_fields`,
`adt_refinements`, `route_validated`, the 422 path, and OpenAPI emission all exist
(§1.1). The *actual* gap is narrower: the field table exists in the **checker** but has
**no codegen/runtime backing for reads**, and `json_response` is unwired. This makes the
increment smaller than any memo assumed, and it sharpens the real risk (Q4's dispatch
gate) over the imagined one (field tracking).

### Approach A — serde-Value-backed typed view [the runtime spine; CHOSEN]

The body stays the boxed `serde_json::Value`; `body.field` retargets to a typed
accessor shim keyed on the field's `Ty`; `json_response` re-serializes the held Value.

- **Feasibility (1) — the decisive win:** every primitive EXISTS and is verified
  (§1.1) — the boxed `Value`, the `(ptr,ptr)->ptr` `path_param` shim template, the
  `lookup_handle_attr → emit_ecosystem_call` retarget seam, the field `Ty` from
  `adt_fields`, `Response::json`. **NO new object-model machinery, NO LLVM struct type,
  NO GEP, NO `#![forbid(unsafe_code)]` fight** — reads go through runtime calls exactly
  like `req.body()` / `path_param` / `xs[i]` already do. Buildable this sprint.
- **§2.5 (2):** `body.field` is verbatim pydantic/attrs attribute access (~0.95
  shipped surface — already type-checked by `adt_fields`); the serde lookup is
  invisible at the source layer. A typo is compile-time `UnknownField` (the strongest
  §2.5-A catch). 0.95.
- **Elegance (3):** drops all five footguns (§3) — no stringly access (key is
  compiler-synthesized), no runtime KeyError (total lookup), no silent coercion
  (`as_i64`). Honest caveat: NOT zero-cost (a Map get per access).
- **Grounding (4):** A's memo nailed the load-bearing facts — Adt = opaque pointer,
  `Field(0)` discards the name, `json_response` genuinely unwired, AND it correctly
  identified Q4 (the validated-body-vs-`.cb`-constructed discriminator) as "the
  load-bearing design decision most likely to be got subtly wrong." It is also the
  most honest about the limit: this is a **validated-body-ONLY** solution that does
  NOT generalize to `.cb`-constructed classes.
- **Verdict:** the buildable-now runtime + the most honest grounding. **Its runtime is
  the spine.** Its one weakness — A frames the read as a serde lookup *without* the
  swappable-seam abstraction — is fixed by grafting C's seam framing (below).

### Approach B — `.cb`-native struct ABI [the honest long-term endpoint; deferred]

A validated body becomes a **real heap struct** with a deterministic layout
(`adt_fields` BTreeMap order); `body.field` is a struct-GEP+load; the validator
deserializes JSON straight into the struct; `json_response` walks the layout to
re-serialize.

- **Feasibility (1) — the decisive miss for *this* sprint:** the "big four" B itself
  enumerates do NOT exist — (i) a per-Adt LLVM `StructType` (today every `Ty::Adt` is
  `opaque_ptr_ty`, `llvm_backend.rs:3486`), (ii) struct-GEP field load/store under
  `#![forbid(unsafe_code)]` (the codebase **already retreated from GEP** for Array
  dynamic-index onto runtime helpers, `llvm_backend.rs:4476`; B's own memo flags this
  as "the load-bearing unknown that must be spiked"), (iii) the real field index in MIR
  (vs the `Field(0)` stub), (iv) a generated (de)serializer + a field-args ctor (the
  current ctor is zero-arg `() -> Adt`). B's own §5 slice-1 collapses to **"Approach
  A's runtime underneath, with a field-index over the same `Value`"** — i.e. *not yet
  the native struct ABI*. That is the honest read: full B is a multi-sprint object-model
  build.
- **§2.5 (2):** identical `body.field` surface (~0.95) — the surface is
  approach-independent.
- **Elegance (3):** the *destination* is the cleanest — a real typed struct load IS
  zero-cost (§5.1), and `body.field` would work for `.cb`-constructed instances too.
  This is the genuine object model.
- **Grounding (4):** honest — B explicitly flags the `#![forbid(unsafe_code)]`-vs-GEP
  collision as the unspiked, sprint-betting risk, and concedes its slice-1 is A's
  runtime wearing B's name.
- **Verdict:** the **right long-term endpoint** and the only approach that makes `.cb`
  structs real everywhere (Q6/§7) — but its defining machinery is the single largest
  unbuilt capability of the three and is **not needed to ship the §6-Phase-1 surface.**
  Adopted as the deferred endpoint + the migration target the §5 seam points at.

### Approach C — hybrid/staged, Value-backed now behind a stable seam

The body stays the boxed `serde_json::Value`; `body.field` retargets through the SAME
`lookup_handle_attr` seam to a field-indexed accessor; the seam names an accessor
symbol + a `Ty`, so a future struct ABI swaps the backing with zero `.cb`-source churn.

- **Feasibility (1):** identical to A (same verified primitives) — buildable now.
- **§2.5 (2):** identical `body.field` (~0.95); compile-time-catch preserved via
  `adt_fields`.
- **Elegance (3):** identical five-footgun drop, same not-zero-cost caveat.
- **Grounding (4):** C correctly identified the `lookup_handle_attr` seam as the
  representation boundary and articulated the cleanest version of the
  migration-seam argument ("MIR names an accessor + a `Ty`, NOT `serde_json` or a JSON
  key — the Value-vs-struct choice lives entirely behind the symbol"). **C's stated
  biggest-risk (the body param is the sentinel) is partly mis-resolved** — at HEAD the
  handler-body local already carries the *real* class id via the Phase-1b-i annotation
  fix (`7c58bd5`), so the sentinel lives only in the route-shape gate, NOT the
  handler body; C's one-line "record the real AdtId" worry is already done. C's
  honesty about deferring `.cb`-constructed-class storage is correct.
- **Verdict:** C is **A's runtime + the explicit swappable seam** — the same buildable
  mechanism as A, but framed so the future does not get painted into a corner. Its seam
  framing is the missing piece A lacks.

### The choice + what is grafted

**Adopt A's runtime (the buildable-now serde-Value-backed view) AS the spine, framed
through C's swappable accessor seam, with B named as the honest long-term endpoint.**
The three are not really rivals: **A, B-slice-1, and C describe the SAME runtime
mechanism** (Value-backed accessor shims over the `lookup_handle_attr` seam). The
synthesis is therefore:

- **From A — the runtime + the honesty.** The body IS the `serde_json::Value`;
  `json_response` re-serializes it; reads go through typed accessor shims; this is a
  validated-body-ONLY solution that does NOT generalize to `.cb`-constructed classes.
  A's Q4 finding (the validated-body-vs-`.cb`-constructed discriminator is the
  load-bearing risk) is adopted verbatim as §4-Q4.
- **From C — the swappable seam (the key graft).** `body.field` lowers to *a
  borrowed-receiver call to an accessor symbol returning the field's declared `Ty`* —
  MIR names a symbol + a `Ty`, never `serde_json` or a JSON key (§2-Q5). This is what
  makes A's runtime forward-compatible with B's struct: the backing swaps behind the
  symbol. Without this graft A welds the source to the Value; with it, A's runtime is
  a deliberate, non-cornering stop-gap.
- **From B — the destination + the sequencing honesty.** The native struct ABI (real
  per-Adt LLVM struct + safe field load + generated (de)serializer + field-args ctor)
  is the genuine object model and the only path that makes `.cb`-constructed `Score()`
  field access real. It is recorded as the §7 sub-ADR endpoint and the migration target
  the §5 seam points at — NOT claimed as buildable this sprint, and the §5 increment is
  explicitly NOT oversold as the struct story (§6).

## 5. The chosen mechanism — buildable detail

### 5.1 Surface syntax (the `.cb` an LLM writes — unchanged from ADR-0080 §6)

```python
class CreateScore:
    name: str
    rank: i64 where 0 <= self and self <= 100

fn create_score(req: pit.Request, body: CreateScore) -> pit.Response:
    let r: i64 = body.rank        # typed read → __cobrust_pit_body_get_i64(body, "rank")
    let n: str = body.name        # typed read → __cobrust_pit_body_get_str(body, "name")
    return pit.json_response(201, body)   # re-serializes the held Value

fn main() -> i64:
    let app = pit.App()
    app.route_validated("POST", "/scores", create_score)
    app.serve_openapi("/openapi.json")
    app.run("127.0.0.1", 8080)
    return 0
```

No new `.cb` syntax — `body.field` + `json_response` are already what ADR-0080 §6
promised; this ADR makes them *execute*.

### 5.2 `body.field` read — the typed accessor shim + the gated MIR retarget

The body is the boxed `serde_json::Value` (`cabi.rs:464`). Reads go through typed shims
cloned from `path_param` (`cabi.rs:755`):

```rust
// cabi.rs — sibling of __cobrust_pit_request_path_param. (ptr, ptr) -> <ret>.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_pit_body_get_i64(body: *mut u8, name: *mut u8) -> i64 {
    if body.is_null() { return 0; }                       // fail-clean sentinel (unreachable on validated path)
    let v: &serde_json::Value = unsafe { &*body.cast::<serde_json::Value>() };
    let name_s = unsafe { read_str_buf(name) };
    v.get(&name_s).and_then(serde_json::Value::as_i64).unwrap_or(0)   // as_i64, NOT as_f64-truncate (footgun #3)
}
// __cobrust_pit_body_get_str    -> *mut u8 (alloc_str_buffer, like path_param)
// __cobrust_pit_body_get_f64    -> f64   (serde as_f64; LLVM `double` extern — math.sqrt precedent)
// __cobrust_pit_body_get_bool   -> bool  (serde as_bool, STRICT; LLVM `i1` extern via bool_type() — re.match precedent)
// __cobrust_pit_body_get_nested -> *mut u8 (BORROWED interior &Value for a nested object; body.inner.x recurses)
```

**The retarget.** A new MIR `Attr` sub-arm fires when the base is a **validated-body
param** (Q4 gate), reads the field's declared `Ty` from `adt_fields` to pick the shim,
and lowers to the borrowed-receiver `emit_ecosystem_call` already used for
`coil.Buffer.shape` (`lower.rs:1457-1465`), passing `(recv, Constant::Str(field_name))`.
The field name string is **compiler-synthesized** from the resolved field, never
author-written (footgun #1). The cleanest wiring is to extend the
`lookup_handle_attr`-style retarget (or add a sibling lookup) so it returns an accessor
`EcoSig { runtime_symbol, ret: <field Ty> }` for a validated-body base — which is
exactly C's seam (§2-Q5): **MIR names the symbol + the `Ty`, never serde or a JSON key.**

**The Q4 gate (load-bearing) — REGISTRATION-DRIVEN, the concrete channel.** The retarget
must fire for a `route_validated` body-param base but **NOT** for a `.cb`-constructed
`Ty::Adt` instance (same id, same field table, but a null/opaque `*mut u8` — §1.1 —
which would make the shim `cast::<Value>()` garbage → UB). The naive "key on param
origin" phrasing is a hand-wave: **that information does NOT exist at HEAD.** Verified at
`8dae584`:

- **Route-shape validation is call-site-only.** The validated-body acceptance lives in
  `check_eco_sig`'s callback loop (`check.rs` ~`:2641`): when the expected slot is the
  `PIT_VALIDATED_BODY_SENTINEL_ADT`, the checker accepts the handler's 2nd positional iff
  it is a field-tracked user `Ty::Adt` (`is_tracked_body` = id outside the handle range
  AND `self.adt_fields.contains_key(id)`). This is checked **at the
  `app.route_validated(...) call site** and recorded **NOWHERE** a per-fn body can read.
- **MIR carries no origin.** `Body` (`tree.rs:46`) has `param_count` but no
  handler-ness flag; `LocalDecl` (`tree.rs:89`) is `{id, name, ty, mutable, span}` — no
  per-param origin. `synth_expr_ty(Name)` yields only a `Ty`. So a validated-body param,
  a `let s = Score()` binding, and a non-handler-fn param (`fn helper(b: CreateScore)`)
  are all `Ty::Adt(same-id)` — **type-indistinguishable**. Firing on type alone is the UB.

So the gate is a NEW checker→MIR channel, built exactly like the existing
`adt_fields`/`adt_refinements`/`adt_names` reach MIR (`self.ctx.typed.adt_fields` is
already read at MIR by `validated_body_schema_for_handler`, `lower.rs` ~`:2195`):

1. **Checker COLLECTS registrations.** As each `app.route_validated(method, path,
   handler)` call is accepted in `check_eco_sig` (the `is_tracked_body` branch above is
   the proof point — the body param index + the body class `AdtId` are both in hand
   there), record it into a new field on `TypedModule` (the struct at `check.rs:34` that
   already carries `adt_fields`/`adt_refinements`/`adt_names`):

   ```rust
   // crates/cobrust-types/src/check.rs — TypedModule, sibling of adt_fields
   /// ADR-0081 — per-handler validated-body registration. Populated as each
   /// accepted `app.route_validated(_, _, handler)` is checked: the handler's
   /// `DefId` → (body-param positional index, body class `AdtId`). The SAME
   /// checker→MIR channel as `adt_fields`. The ONLY source of "this param is a
   /// validated body" — route-shape validation is otherwise call-site-only.
   pub validated_handlers: HashMap<DefId, (usize, AdtId)>,
   ```

   (`DefId(pub u32)` = `cobrust-hir/scope.rs:18`; `AdtId(pub u32)` = `cobrust-types/ty.rs:31`.)
2. **MIR MARKS the body-param local.** When lowering a fn whose `DefId` is in
   `validated_handlers`, MIR sets a NEW field on that fn's body-param `LocalDecl` (and/or
   `Body`):

   ```rust
   // crates/cobrust-mir/src/tree.rs — LocalDecl, NEW field
   /// ADR-0081 — `Some(id)` iff this local is the validated-body parameter of a
   /// `route_validated`-registered handler (from `TypedModule.validated_handlers`).
   /// The ONLY thing that authorises the serde-accessor shim (§5.2). `None` for
   /// every other local — incl. a `let s = Score()` binding and a non-registered
   /// fn's `b: CreateScore` param (both `Ty::Adt(same-id)`, NOT serde-backed).
   pub validated_body_of: Option<AdtId>,
   ```
3. **The shim fires gated on the MARK, not the type.** The `body.field` serde-accessor
   sub-arm fires **ONLY** when the base resolves to a local carrying `validated_body_of
   == Some(id)` **AND** the field is in that class's `adt_fields` (the shim is then picked
   by the field's declared `Ty`). Otherwise the **pre-existing** path is taken (the
   `Field(0)` no-runtime-field-storage stub, which Approach A defers to the native-struct
   phase — §6/§7).

This is the single most important design constraint and the thing the impl sprint must
get exactly right; it is a Phase-1 done-means + a paired ADSD audit focus.

**The no-UB invariant (stated explicitly).** A tracked-body class used as anything OTHER
than a registered handler's validated-body param — specifically (a) a NON-registered
fn param (`fn helper(b: CreateScore) -> i64: return b.rank`, where `helper` is never
`route_validated`-registered), or (b) a `let s = Score()` `.cb`-constructed binding — has
`validated_body_of == None` on its local, so the serde shim **never fires** and the base
is **never `cast::<Value>()`-ed**. It instead hits the PRE-EXISTING no-runtime-field-storage
path (the `Field(0)` stub, `lower.rs:1476` / `lower_place_load` `else`, `llvm_backend.rs:4564`)
which Approach A explicitly defers to the native-struct phase (§8). **Therefore the worst
case degrades to the already-documented "no field storage yet" limitation — a stub read,
NOT undefined behavior.** The serde-cast UB is structurally unreachable for any unmarked
local: this ADR introduces NO new serde-cast hazard beyond the registration-gated path.

### 5.3 `json_response(status, body)` — the `text_response` sibling

```rust
// cabi.rs — sibling of __cobrust_pit_text_response. The only delta: the body
// is the boxed serde_json::Value, re-serialized; the box is BORROWED (the
// route_validated trampoline still frees it once at cabi.rs:479).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_pit_json_response(status: i64, body: *mut u8) -> *mut u8 {
    if body.is_null() { return std::ptr::null_mut(); }
    let v: &serde_json::Value = unsafe { &*body.cast::<serde_json::Value>() };
    let status_u16 = u16::try_from(status).unwrap_or(200);
    let resp = crate::response::Response::json(v).with_status(status_u16);   // response.rs:49 + 74
    Box::into_raw(Box::new(resp)).cast::<u8>()
}
```

A new manifest free-fn row `pit.json_response(status: i64, body: <validated-body>) ->
pit.Response` (`PyCompatTier::Semantic`) + a codegen extern `fn_type([i64, ptr]) -> ptr`
(clone the `text_response` extern) + the shim above. **`json_response` is independent of
the field-read work and ships first**: it re-serializes the held Value, so the §6
Phase-1 handler's `return pit.json_response(201, body)` round-trips with ZERO field
reads needed.

### 5.4 What new machinery — built vs exists (file-level)

| Need | Exists? | Build |
|---|---|---|
| Body as boxed `serde_json::Value` behind `*mut u8` | **Yes** (`cabi.rs:464`) | — |
| `(ptr,ptr)->ptr` accessor-shim template | **Yes** (`path_param`, `cabi.rs:755`) | 2–4 typed shims `__cobrust_pit_body_get_{i64,str,f64,bool}` (clones) |
| `lookup_handle_attr → emit_ecosystem_call` retarget seam | **Partial** — gated on `is_ecosystem_handle`/`COIL_BUFFER_ADT` (`ecosystem.rs:312`); a user body class returns `None` → stub `Field(0)` | a validated-body Attr sub-arm + accessor `EcoSig` keyed on the field `Ty`, gated on the **registration mark** `validated_body_of` (Q4) |
| **Checker→MIR validated-body param-marking channel** (the Q4 gate's substrate) | **NO — must build (NEW).** The "this param is a validated body" fact does NOT exist at HEAD: route-shape validation is call-site-only (`check_eco_sig` ~`check.rs:2641`); `Body`/`LocalDecl` (`tree.rs:46`/`:89`) carry only `param_count` + a `Ty`, NO origin flag | `TypedModule.validated_handlers: HashMap<DefId, (usize, AdtId)>` populated in `check_eco_sig` (sibling of `adt_fields`, `check.rs:34/46`) + a NEW `LocalDecl.validated_body_of: Option<AdtId>` set when MIR lowers a registered handler's body param (§5.2) |
| Field `Ty` reachable at lowering | **Yes** — `adt_fields` (`check.rs:46`); already walked by the validator/OpenAPI passes (`validated_body_schema_for_handler`, `lower.rs` ~`:2195`, reads `self.ctx.typed.adt_fields` — the channel `validated_handlers` clones) | — |
| `json_response` manifest row + extern + shim | **No** (the §1 gap) | one `ecosystem.rs` row + one codegen extern (`[i64, ptr] -> ptr`) + one `cabi.rs` shim (`Response::json` + `with_status`, ~30 LOC) |
| Native LLVM struct type / GEP / field-args ctor / generated (de)serializer | **No** — Adt = `opaque_ptr_ty` null (`llvm_backend.rs:5016`), `#![forbid(unsafe_code)]` blocks GEP | **NOT built** — Q6/§7 deferred endpoint |

**No change to `lower_place_load`, no GEP, no per-Adt struct type, no
`#![forbid(unsafe_code)]` confrontation.** The reads + the re-serialize are runtime
calls, the same category as every shipped ecosystem read.

## 6. Phased plan + Done-means

The phasing constraint: **the seam (§2-Q5) is the load-bearing invariant** — every phase
keeps `body.field` lowering to *an accessor symbol + a `Ty`*, never to serde/a JSON key,
so the native-struct endpoint (§7) can swap the backing without `.cb`-source churn.

### Phase 1 (the smallest end-to-end-real increment) — `json_response` + `i64`/`str` field reads

The §6-Phase-1 handler from ADR-0080, now executing.

**Scope (the slice):**
1. **`json_response` (ship FIRST, independent, ~30 LOC):** manifest free-fn row +
   codegen extern (`[i64, ptr] -> ptr`) + `__cobrust_pit_json_response` shim
   (`Response::json(&*body).with_status(status)`, §5.3). **This alone makes the §6
   handler's `return pit.json_response(201, body)` round-trip** (no field reads needed).
2. **Two typed accessor shims:** `__cobrust_pit_body_get_i64` + `__cobrust_pit_body_get_str`
   (the §6 example uses only `i64` + `str`), cloned from `path_param` (§5.2).
3. **The new checker→MIR registration channel + the gated MIR `Attr` sub-arm:** the
   checker populates `TypedModule.validated_handlers` (`DefId → (body param idx, body
   `AdtId`)`) in `check_eco_sig`; MIR marks the body-param `LocalDecl.validated_body_of =
   Some(id)` when lowering a registered handler; the `Attr` sub-arm fires for a base local
   carrying `validated_body_of == Some(id)` (Q4 — the registration mark, NOT
   `Ty::Adt`-with-fields), picks the shim by the field's `Ty` from `adt_fields`, lowers
   via the existing borrowed-receiver `emit_ecosystem_call` (`lower.rs:1457`) passing
   `(recv, Constant::Str(field_name))`.

**Layers touched** (anchors at `8dae584`; the impl sprint re-greps):

| Layer | File | Site | Edit |
|---|---|---|---|
| **Manifest** | `crates/cobrust-types/src/ecosystem.rs` | the pit free-fn block (the `text_response` row) + an accessor-attr lookup | add `json_response` free-fn row → `__cobrust_pit_json_response`; add the validated-body accessor `EcoSig`s (keyed on the field `Ty`); `PyCompatTier::Semantic` |
| **Checker channel (NEW)** | `crates/cobrust-types/src/check.rs` | `TypedModule` @34 (sibling of `adt_fields` @46) + `check_eco_sig` validated-body branch ~@2641 | add `validated_handlers: HashMap<DefId, (usize, AdtId)>` to `TypedModule`; populate it in the `is_tracked_body` branch (the body param idx + body `AdtId` are in hand there); carry it out exactly like `adt_fields`/`adt_names` (`check.rs:439-463`) |
| **MIR mark (NEW)** | `crates/cobrust-mir/src/tree.rs` + `lower.rs` | `LocalDecl` @89 (+ `Body` @46); the `Attr` rvalue arm @1445-1477 (+ lvalue @672, + callee/load-attr @2017/@3137) | add `LocalDecl.validated_body_of: Option<AdtId>`; set it from `TypedModule.validated_handlers` when lowering a registered handler's body param; a validated-body Attr sub-arm BEFORE the `Field(0)` fallthrough gated on `validated_body_of == Some(id)` (Q4 mark, NOT type); retarget to the accessor symbol via `emit_ecosystem_call`; `json_response(...)` is a normal free-fn call (no new mechanism) |
| **Codegen** | `crates/cobrust-codegen/src/llvm_backend.rs` | the pit extern block (ADR-0073 §4) | declare `__cobrust_pit_json_response` (`[i64, ptr] -> ptr`) + the body-get externs (clone the `path_param`/`text_response` extern shapes); NO struct/GEP work |
| **CLI build** | `crates/cobrust-cli/src/build/intrinsics.rs` | the `__cobrust_pit_*` recognizer | confirm the new symbols match the existing pit prefix |
| **Runtime** | `crates/cobrust-pit/src/cabi.rs` | sibling of `path_param` @755 + `text_response` @193 | the 2–4 body-get shims + `__cobrust_pit_json_response`; `as_i64` not `as_f64`-truncate (footgun #3); shim borrows, does not free (trampoline frees @479) |
| **Docs** | `docs/{agent,human/zh,human/en}` pit specs | the body-field-read + `json_response` surface | per CLAUDE.md §3.3, in the impl commit |
| **Tests** | `crates/cobrust-pit/src/cabi.rs` `#[cfg(test)]` + a CLI E2E | mirror `text_response_round_trip_drops_once` + a `pit_validated_body_read_e2e.rs` | the §6 program + the negatives below |
| **Cargo** | (none expected — `serde_json` already a pit dep) | — | if any dep is added, **stage `Cargo.lock`** (F64) |

**Done-means (Phase 1):**
- `POST /scores {"name":"a","rank":50}` → **201** with body `{"name":"a","rank":50}`
  (the round-trip via `json_response(201, body)`).
- Inside the handler, `body.rank` **reads 50** (not a stub-load garbage value) and
  `body.name` reads `"a"`; a handler doing `let x: i64 = body.rank` compiles and the
  value is correct at runtime (a real read, not `Field(0)`).
- `body.rank + "x"` in the handler is a compile-time `TypeError` (ADR-0080, unchanged);
  `body.nonexistent` is a compile-time `UnknownField` (unchanged) — NOT a runtime
  `KeyError`.
- `POST /scores {"name":"a","rank":200}` → **422**, handler never entered (ADR-0080
  validation, unchanged — this ADR adds reads on TOP, touches neither validator nor
  schema).
- **The Q4 negative (critical — the registration-gate proof):** the serde shim is
  registration-gated (`validated_body_of == Some(id)`), NOT type-gated. Both unmarked
  cases must hold and a test asserts the serde-accessor arm fires ONLY for a
  registered handler's validated-body param:
  - **(a) non-registered fn param** — `fn helper(b: CreateScore) -> i64: return b.rank`
    where `helper` is NOT `route_validated`-registered: `b` has `validated_body_of ==
    None`, so `b.rank` must **NOT** emit the serde cast — it is either a clean
    compile-time error or hits the deferred no-field-storage path (the `Field(0)` stub),
    but it **MUST NOT** `cast::<Value>()` a null/opaque pointer (no UB). This is the proof
    that the shim is registration-gated, not type-gated.
  - **(b) `.cb`-constructed instance** — `let s = CreateScore()` used as `s.rank`: same
    `validated_body_of == None`, same deferred no-field-storage path, never a serde cast.
- `json_response`'s borrowed body box is still freed **exactly once** by the
  `route_validated` trampoline (`cabi.rs:479`); a `DROP_COUNT`-style assertion (mirror
  `text_response_round_trip_drops_once`) confirms no double-free / no leak.
- The shipped pit + `route_validated` + OpenAPI suites still pass (no regression);
  workspace gates green; `Cargo.lock` staged if a dep was added (F64).

### Phase 2 — `f64` / `bool` field reads + nested-body reads — **SHIPPED (`ba8cca6`→ this sprint)**

**Status: DELIVERED, all three.** `f64`, `bool`, AND nested-body reads land in one sprint
(`pit_body_field_read_e2e.rs` extended to 9 tests, GREEN).

- **`f64` / `bool` (mechanical):** two new arms in `lookup_validated_body_accessor`
  (`ecosystem.rs`) — `Ty::Float → __cobrust_pit_body_get_f64` (`as_f64`), `Ty::Bool →
  __cobrust_pit_body_get_bool` (`as_bool`, STRICT — no truthiness, §2.2). Two new shims in
  `cabi.rs` (mirror the i64/str pair, BORROW the box). Two new codegen externs in
  `llvm_backend.rs`: `f64` → LLVM `double` (the `math.sqrt` precedent), `bool` → LLVM `i1`
  via `bool_type()` (the `re.match` / `fang.verify_password` / `coil.any` precedent — the
  i1 lands in the `.cb` `_ecoret` Bool local, usable in `if body.flag:`). The
  `__cobrust_pit_*` prefix recognizer in `intrinsics.rs` already covered the new symbols
  (no edit).
- **Nested `body.inner.x` (the design-heavy half, also DELIVERED):** a body field typed as
  ANOTHER field-tracked validated class (`Ty::Adt(nested_adt, _)`, id OUTSIDE the
  ecosystem-handle range) resolves to `__cobrust_pit_body_get_nested`, which returns the
  **BORROWED interior** `&serde_json::Value` for the nested object (no allocation, no free
  — it lives inside the parent box the trampoline owns + frees once, `cabi.rs:530`). The
  MIR `Attr` base resolution became **recursive** (`resolve_validated_body_base`,
  `lower.rs`): it walks a `body.inner` chain down to the marked param, emitting a nested
  borrow at each hop and **re-marking each result temp** `validated_body_of =
  Some(nested_adt)`, so `.field` on it recurses through the SAME registration-gated arm.
  Verified at depth 1 (`body.inner.x`) AND depth 3 (`body.mid.leaf.v`, `body.mid.leaf.flag`).
- **Soundness of the borrowed-interior pointer (the load-bearing argument):** the nested
  pointer aliases the parent box, which the trampoline frees EXACTLY ONCE *after* the
  handler returns — so the borrow is valid for the whole handler. The `_ecoret` temp is
  typed `Ty::Adt(user_class)`, whose codegen drop is a **NO-OP**
  (`handle_drop_symbol(user_id) == None`, `llvm_backend.rs:5212`), so even if the drop
  schedule enumerates the temp, NO free is emitted on the borrowed pointer (no double-free,
  no UB).
- **No-UB gate preserved + extended:** the recursive resolver only succeeds when the chain
  bottoms out at a `validated_body_of`-marked param, so a non-registered helper reading
  `b.ratio` / `b.active` / `b.inner.x` (or a `.cb`-constructed instance) emits NEITHER the
  scalar NOR the nested accessor — verified by three new `nm`-on-`.o` codegen-property
  tripwires (the gate is REGISTRATION-driven, not type-driven; §5.2 / §10).
- **No new dep** (serde_json was already a pit dep); `Cargo.lock` unchanged.

### Phase 3 — list-field reads + `body` as a function argument

`body.tags` where `tags: list[str]` (accessor returns a `.cb` list built from the JSON
array); passing `body` (or a field) to another fn. **Done-means:** a list field reads +
iterates; the seam still names a symbol + a `Ty`.

### Phase 4+ (deferred, §7 sub-ADRs) — the native struct ABI + `.cb`-constructed-class field storage

The genuine object model (Approach B): a per-Adt LLVM struct type, a
`#![forbid(unsafe_code)]`-safe field load, a generated (de)serializer, and a field-args
ctor — so `let s = CreateScore("a", 50); s.rank` works for hand-built instances and the
Value-backed view is retired behind the SAME `body.field` seam (zero `.cb`-source churn).

## 7. Done-means (this ADR — design only)

- The chosen representation (Value-backed typed view behind a swappable accessor seam),
  the three options + scored rationale, the runtime-no-field-storage finding, the Q4
  dispatch-gate risk, the Phase-1 buildable slice, the honest deferrals, and the
  migration seam are all recorded, each tied to a verified source seam (§1.1).
- ADR row added to `docs/agent/adr/README.md`.
- `scripts/doc-coverage.sh` passes (design-only ADR adds no human-tree files → zh/en
  parity intact, matching ADR-0077/0078/0079/0080).
- Ratify draft→accepted when the Phase-1 `json_response` + `i64`/`str` field-read +
  Q4-gated retarget impl sprint lands, passes the §6 done-means, and clears a paired
  ADSD audit. **The audit's primary focus is the registration gate:** the serde shim
  fires ONLY for a local marked `validated_body_of == Some(id)` (a
  `route_validated`-registered handler's body param), and BOTH negatives hold — (a) a
  non-registered fn param `fn helper(b: CreateScore) -> i64: return b.rank` emits NO serde
  cast (clean error or deferred stub, never UB), and (b) a `.cb`-constructed
  `let s = Score()` emits NO serde cast. This is the ADR's central correctness risk
  (§2-Q4 / §5.2 / §10) and the Phase-1 impl's primary done-means.

## 8. Honesty — what is deferred, and why Phase-1 is the part buildable now

Stated plainly:

- **A native struct ABI is the honest long-term endpoint, and it is NOT built here.**
  Real field storage needs a per-Adt LLVM `StructType` (today every `Ty::Adt` is
  `opaque_ptr_ty`, `llvm_backend.rs:3486`/`:5016`), a struct-GEP field load that
  survives `#![forbid(unsafe_code)]` (the codebase **already retreated from GEP** for
  Array dynamic-index, `llvm_backend.rs:4476` — the same fight), a generated
  (de)serializer, and a real field-args ctor (the current ctor is zero-arg `() -> Adt`).
  That is the M5+/§9-grade object-model build ADR-0080 §9 ("the full Adt constructor")
  + ADR-0078 §9 (".cb↔serde bridge") already point at. **This ADR does not pretend the
  Value-backed view is that.**
- **The Value-backed view does NOT generalize to `.cb`-constructed classes (NOR to a
  non-registered fn's tracked-body param).** A hand-built `let s = CreateScore(); s.rank`
  — or a non-registered `fn helper(b: CreateScore): … b.rank` — has a null/opaque
  `*mut u8` (§1.1), not a serde `Value`, so the accessor shim has nothing to read. This
  is exactly why the Q4 gate is REGISTRATION-DRIVEN (`validated_body_of == Some(id)` on
  the local, §5.2), not type-driven: a serde shim over any unmarked tracked-body local
  would be UB. The no-UB invariant (§5.2 / §10) makes that structurally unreachable — an
  unmarked local degrades to the deferred no-field-storage stub, not UB. The Value-backed
  view is a **pragmatic, validated-body-ONLY bridge**; it buys the FastAPI-defining
  `body.field` + `json_response` round-trip cheaply and does not corner the future (the
  §5 seam), but it is **not** the general `.cb`-struct story. Naming this is the central
  honesty.
- **Why it is still the right Phase-1.** The validated body uniquely already HAS a
  serde `Value` (the validator left it); `body.field` already type-checks (ADR-0080
  `adt_fields`); the read is the only missing rung. Shipping the Value-backed read +
  `json_response` now — behind a seam that names a symbol + a `Ty`, never serde — is the
  smallest correct increment that makes the shipped #156 surface *usable*, and it leaves
  the native-struct endpoint reachable without re-litigating the `body.field` surface.
- **The seam is what makes the stop-gap honest, not a debt-trap.** Because MIR names an
  accessor symbol + a `Ty` (never `serde_json` / a JSON key, §2-Q5), the native-struct
  ABI (§7/Phase-4) swaps the backing behind the symbol — the `.cb` source and the MIR
  shape are unchanged. This is the difference between a deliberate increment and a
  corner.

**No overclaim:** no benchmarks (none run); §4 §2.5 scores are estimates in the
ADR-0078/0080 tradition; every feasibility claim is tied to a verified source seam
(§1.1); the native-struct ABI is explicitly NOT claimed buildable this sprint.

## 9. Open questions for sub-ADRs

- **The native struct ABI (Approach B — the §7/Phase-4 endpoint).** Per-Adt LLVM
  `StructType` + a `#![forbid(unsafe_code)]`-safe field load (spike `build_struct_gep`
  vs a generated runtime accessor wall — the load-bearing unknown B flagged) + a
  generated (de)serializer + a field-args ctor (replacing the zero-arg `() -> Adt`).
  Unifies validated bodies + `.cb`-constructed instances behind the §5 seam. The genuine
  object-model build; shared with ADR-0080 §9 + ADR-0078 §9.
- **`.cb`-constructed-class field storage + the field-args ctor** (a prerequisite of the
  above, independently valuable for every future `.cb` struct — ADR-0080 §9 "the full
  Adt constructor").
- **Body mutation + partial responses.** `json_response(201, body)` re-serializes the
  full validated body; a sub-ADR for returning a derived/filtered shape (without
  re-introducing footgun #4's hand-rebuild drift).
- **The accessor `EcoSig` vs a dedicated validated-body lookup.** Whether the
  `validated_body_of`-gated retarget (Q4) extends `lookup_handle_attr` (made
  mark-aware) or gets a sibling lookup — a bounded factoring decision for the impl
  sprint. (The gate's *substrate* — `validated_handlers` + `validated_body_of` — is
  fixed by §5.2; only its plumbing into the existing seam is open here.)

## 10. Consequences

- **Positive:** makes the shipped ADR-0080 #156 surface *usable* (the §6 Phase-1
  handler's `body.field` reads + `json_response(201, body)` round-trip now execute) with
  the **smallest correct increment on the runtime as it actually is** — the body is the
  serde `Value` it already crosses as, reads are typed accessor shims cloned from the
  proven `path_param` template, `json_response` is a `text_response` sibling, and the
  whole thing rides the existing `lookup_handle_attr → emit_ecosystem_call` seam with NO
  new object-model machinery, NO LLVM struct, NO GEP, NO `#![forbid(unsafe_code)]` fight.
  Drops five JSON-body-consumption footguns with cited seams (§3); keeps `body.field`
  lowering to a symbol + a `Ty` so the native-struct endpoint swaps the backing without
  `.cb`-source churn (§5 seam, the C graft).
- **Negative / accepted:** (1) the Value-backed read is **not zero-cost** (a serde Map
  get per access, §3 residual) — retired by the native-struct endpoint. (2) The view is
  **validated-body-ONLY** — it does NOT generalize to `.cb`-constructed classes (§8); a
  hand-built instance has no Value backing. (3) The native struct ABI (the honest object
  model) is **deferred** (§7/Phase-4) — a multi-sprint build gated on resolving the
  `#![forbid(unsafe_code)]`-vs-GEP question.
- **Risk — the Q4 dispatch gate (THE central correctness risk, and the Phase-1 impl's
  primary done-means):** the serde-accessor retarget MUST fire ONLY for a
  `route_validated`-registered handler's body-param local, NEVER for a `.cb`-constructed
  `Ty::Adt` instance NOR a non-registered fn's tracked-body param (all three share id +
  field table, but the latter two have a null/opaque `*mut u8` → `cast::<Value>()` UB).
  **The naive "key on param origin" is a hand-wave — that fact does not exist at HEAD**
  (route-shape validation is call-site-only at `check_eco_sig` ~`check.rs:2641`; MIR
  `Body`/`LocalDecl` carry no origin). So the gate is REGISTRATION-DRIVEN: a NEW
  `TypedModule.validated_handlers: HashMap<DefId, (usize, AdtId)>` populated in the
  checker + a NEW `LocalDecl.validated_body_of: Option<AdtId>` mark set in MIR; the shim
  gates on the mark, NOT the type (§2-Q4 / §5.2). **The no-UB invariant:** any unmarked
  tracked-body local (non-registered fn param OR `.cb`-constructed binding) hits the
  pre-existing no-field-storage path (a stub read deferred to the native-struct phase),
  so the worst case degrades to the documented "no field storage yet" limitation — NOT
  undefined behavior; this ADR introduces NO new serde-cast hazard beyond the
  registration-gated path. A Phase-1 done-means + a paired-ADSD-audit focus; the thing
  most likely to be got subtly wrong (A's finding, adopted + concretized).
- **Risk — borrow-check on `body.field`:** a borrowed-receiver `emit_ecosystem_call`
  (the coil.Buffer.shape discipline, `lower.rs:1458` `upgrade_move_to_copy_handle`)
  keeps the body live; moves out of `body.field` interact with the borrow checker
  (ADR-0060b) — a done-means check, not a new mechanism.
- **Risk — manifest drift:** `json_response` + the body-get accessors join the
  hand-maintained manifest (ADR-0072 §5 R4 accepted debt; generation still deferred).
- **Risk — Cargo.lock staging (F64):** none expected (`serde_json` is already a pit
  dep); if any phase adds a dep, **stage `Cargo.lock`** or `--locked` CI cluster-fails.
- **Follow-up:** ratify draft→accepted on the Phase-1 impl + §6 done-means + paired ADSD
  audit; open the §7 native-struct-ABI sub-ADR (it retires the not-zero-cost residual
  AND unblocks `.cb`-constructed-class field access at once — the highest-leverage
  follow-up).

## 11. Evidence

- **Source ground truth (verified at `8dae584`):**
  - `crates/cobrust-codegen/src/llvm_backend.rs` — `AggregateKind::Adt(_, _) =>
    opaque_ptr_ty.const_null()` @5016 (Adt = null, zero field storage); `lower_ty` Adt →
    `opaque_ptr_ty` @3486; `lower_place_load` @4435 (empty / `[Deref]` / const-index
    Array only; `Field(_)` → stub-load `else` @4564-4573); the Array dynamic-index GEP
    retreat onto `__cobrust_array_get_*` under `#![forbid(unsafe_code)]` @4476-4520.
  - `crates/cobrust-mir/src/lower.rs` — `Attr` rvalue arm @1445-1477 (`lookup_handle_attr →
    emit_ecosystem_call` borrowed-receiver @1457-1465; non-handle base → `Field(0)`
    placeholder discarding `name` via `let _ = name;` @1476); lvalue Attr `Field(0)`
    @672-674; callee/load-attr mirrors @2017-2120 / @3137-3140; the checker→MIR channel
    pattern the Q4 gate clones — `validated_body_schema_for_handler` reads
    `self.ctx.typed.adt_fields.get(body_adt)` @~2195 (re-derives the body class from the
    handler `Ty::Fn` slot AT THE CALL SITE — it does NOT mark per-fn-body which param is a
    validated body; that channel is NEW).
  - `crates/cobrust-mir/src/tree.rs` — `Body` @46 (`param_count` + `is_param`, NO
    handler-ness flag) + `LocalDecl` @89 (`{id, name, ty, mutable, span}`, NO per-param
    origin) — confirms a validated-body param, a `let s = Score()` binding, and a
    non-registered fn's `b: CreateScore` param are all `Ty::Adt(same-id)` +
    type-indistinguishable; the Q4 gate's `LocalDecl.validated_body_of: Option<AdtId>` is
    a NEW field here.
  - `crates/cobrust-types/src/check.rs` — `TypedModule` @34 (carries `adt_fields`/
    `adt_refinements`/`adt_names`; the Q4 gate adds `validated_handlers: HashMap<DefId,
    (usize, AdtId)>` as a sibling, carried out @439-463 like the others); `adt_fields`
    @46, `adt_refinements` @52; the `Attr` arm returning the declared field `Ty` from
    `adt_fields` @1610-1614 + `UnknownField` with FIX @1621 (ADR-0080 Phase-1a;
    `body.field` type-checks); `check_class` recording fields @874-929; **`check_eco_sig`
    @2521 — the validated-body acceptance branch ~@2641 (`is_tracked_body` = id outside
    handle range AND `adt_fields.contains_key`) is CALL-SITE-only and recorded NOWHERE a
    fn body can read — the populate point for `validated_handlers` (the Q4 gate's source).**
  - `crates/cobrust-hir/src/scope.rs` — `DefId(pub u32)` @18 (the `validated_handlers` key);
    `crates/cobrust-types/src/ty.rs` — `AdtId(pub u32)` @31 (the `validated_handlers` value
    + `validated_body_of` payload).
  - `crates/cobrust-pit/src/cabi.rs` — `__cobrust_pit_app_route_validated` @402 (validates
    `req.json()` @440-445, boxes the `serde_json::Value` @464, passes 2-arg
    `CbValidatedHandlerAbi` @470, frees the Value box once @479, 422 arm @447-458);
    `CbValidatedHandlerAbi` @352; `__cobrust_pit_text_response` @193-207 (the
    `json_response` template); `__cobrust_pit_request_path_param` @755-765 (the
    `(ptr,ptr)->ptr` accessor template — `read_str_buf` @107 + `alloc_str_buffer` @134);
    `__cobrust_pit_app_serve_openapi` @530.
  - `crates/cobrust-pit/src/response.rs` — `Response::json(&serde_json::Value)` @49-58
    (sets `content-type: application/json` + serializes); `with_status` @74; `from_parts`
    @63 (the 422 path).
  - `crates/cobrust-types/src/ecosystem.rs` — `pit_validated_handler_fn_ty` @234 (2nd slot
    = SENTINEL `PIT_VALIDATED_BODY_SENTINEL_ADT` @238); the route-shape gate doc
    @220-232 (substitutes "any field-tracked user class id OUTSIDE the handle range");
    `is_ecosystem_handle` @312 (`id.0 >= ECO_ADT_BASE`); `lookup_handle_attr` /
    `COIL_BUFFER_ADT` attr seam @1128/@1170; `PIT_*` reserved ids @85-103.
  - `crates/cobrust-types/src/refinement.rs` — `pub enum Refinement` @41 (the side-table
    ADR-0080 records; confirms `adt_refinements` is real).
  - `git log 5bfab21..8dae584` — `e66dcfb` field tracking, `7c58bd5` class-name-resolves-to-Adt
    (the Phase-1b-i fix that gives the handler-body local the REAL class id, resolving
    Approach C's stated sentinel risk), `a1c9d83` route_validated + 422, `ecf9298` OpenAPI
    emit, `8dae584` string refinements — ADR-0080 Phase-1 + Phase-2 SHIPPED.
- **ADRs:** ADR-0080 (the parent — this ADR makes its §6 `body.field` + `json_response`
  line execute + concretizes its §9 ".cb↔serde bridge"); ADR-0078 (§9 ".cb↔serde bridge"
  shared open question; "Cobrust IS Rust"); ADR-0077 (the `lookup_handle_attr →
  emit_ecosystem_call` attr-retarget seam this reuses; §Q4 shape-correctness-runtime
  honesty mirrored); ADR-0073 (the pit trampoline + Rust-owned ownership split §2 D6 —
  the `route_validated` body box + `json_response` Response box discipline); ADR-0072
  (ecosystem-import chain + flat manifest); ADR-0074 (explicit register-call, no DI);
  ADR-0060b (the `#![forbid(unsafe_code)]`-vs-GEP precedent + `body.field` borrow
  interaction); ADR-0006 (`Ty::Record` field-wise unify — the native-struct §7 endpoint
  reference).
- **Constitution:** CLAUDE.md §2.2 (no silent coercion — footgun #3 `as_i64` not
  `as_f64`-truncate; `Result`-default — footgun #5 the 422 path); §2.5 (LLM-first:
  `body.field` ~0.95 training-data overlap + compile-time-catch via `adt_fields` +
  §2.5-B FIX `suggestion`); §5.1 (elegant — the not-zero-cost residual named honestly;
  the native-struct endpoint as the zero-cost ideal).
- **Findings:** F64 (dev-dep `Cargo.lock` staging — none expected, but any added dep
  stages the lock).
- **Feedback:** `feedback_elegant_ecosystem_surface_no_legacy_debt` (the elegance-law —
  the §3 footgun ledger for the JSON-body-consumption surface).
- **External refs:** `serde_json::Value` API
  (https://docs.rs/serde_json/latest/serde_json/enum.Value.html — `get` / `as_i64` /
  `as_str`), pydantic model attribute access
  (https://docs.pydantic.dev/latest/concepts/models/), FastAPI returning a model
  (https://fastapi.tiangolo.com/tutorial/response-model/).
