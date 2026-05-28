---
doc_kind: reference
module_id: cb-ecosystem-import
title: .cb ecosystem-import wiring (ADR-0072 — 5 data modules; ADR-0073 — pit callback marshalling 6th module + hood 7th module second proof)
last_verified_commit: HEAD
relates_to: [adr:0072, adr:0073, adr:0019, adr:0028, adr:0050c, adr:0071, adr:0034]
dependencies: [cobrust-types, cobrust-mir, cobrust-codegen, cobrust-den, cobrust-nest, cobrust-strike, cobrust-scale, cobrust-molt, cobrust-pit, cobrust-hood, cobrust-cli]
---

# `.cb` ecosystem-import wiring — `import den` / `nest` / `strike` / `scale` / `molt` end-to-end

Status:
- ADR-0072 **first proof** landed. `import den` + `den.connect` /
  `Connection.execute` / `Cursor.fetchall` compile → link → run.
- ADR-0072 **second-module generalization** landed. `import nest` +
  `nest.loads_str` compile → link → run, proving the chain is not
  den-specific. The second wiring touched only the manifest + the new
  shim crate + the per-symbol-prefix recognizer in
  `collect_ecosystem_modules`; the typecheck / MIR / drop / link-locate
  layers stayed untouched.
- ADR-0072 **third-module generalization** landed. `import strike` +
  `strike.get` / `Response.text` / `Response.status_code` /
  `Response.json` compile → link → run, proving the chain supports a
  SECOND handle-pattern module (independent of `den`'s) and that the
  reserved-AdtId `0xE000_0000+N*0x100` block convention scales. The
  third wiring again touched only the manifest, the codegen extern
  block, the recognizer alternation, and the new shim crate.
- ADR-0072 **fourth-module generalization** landed. `import scale` +
  `scale.dumps_str` / `scale.loads_str` compile → link → run, proving
  the chain handles a SECOND value-pattern module (independent of
  `nest`'s) — msgpack JSON round-trip via the proven str→str shape.
  Touched manifest + codegen extern + recognizer alternation + new
  shim crate; the chain-logic layers stayed untouched.
- ADR-0072 **fifth-module generalization** landed. `import molt` +
  `molt.now()` + `DateTime.isoformat` / `DateTime.unix_timestamp`
  compile → link → run, proving the chain handles a THIRD
  handle-pattern module — datetime/RFC3339 via the proven Box-into-raw
  / Box-from-raw + drop-once instrument pattern. Touched the same
  surfaces as scale + reserved a new 256-slot AdtId block (the FOURTH
  block; scale stays in the THIRD block reserved for its future
  bytes-ABI handles).

## Surface (manifest-defined)

| Source form | Retargeted symbol | Signature |
|---|---|---|
| `den.connect(path)` | `__cobrust_den_connect` | `(str) -> den.Connection` |
| `conn.execute(sql)` | `__cobrust_den_connection_execute` | `(den.Connection, str) -> den.Cursor` |
| `cur.fetchall()` | `__cobrust_den_cursor_fetchall` | `(den.Cursor) -> str` |
| scope-exit drop | `__cobrust_den_connection_drop` | `(den.Connection) -> ()` |
| scope-exit drop | `__cobrust_den_cursor_drop` | `(den.Cursor) -> ()` |
| `nest.loads_str(toml)` | `__cobrust_nest_loads_str` | `(str) -> str` |
| `strike.get(url)` | `__cobrust_strike_get` | `(str) -> strike.Response` |
| `strike.post(url, body)` | `__cobrust_strike_post` | `(str, str) -> strike.Response` |
| `resp.text()` | `__cobrust_strike_response_text` | `(strike.Response) -> str` |
| `resp.status_code()` | `__cobrust_strike_response_status_code` | `(strike.Response) -> i64` |
| `resp.json()` | `__cobrust_strike_response_json` | `(strike.Response) -> str` |
| scope-exit drop | `__cobrust_strike_response_drop` | `(strike.Response) -> ()` |
| `scale.dumps_str(json_input)` | `__cobrust_scale_dumps_str` | `(str) -> str` |
| `scale.loads_str(packed)` | `__cobrust_scale_loads_str` | `(str) -> str` |
| `molt.now()` | `__cobrust_molt_now` | `() -> molt.DateTime` |
| `dt.isoformat()` | `__cobrust_molt_datetime_isoformat` | `(molt.DateTime) -> str` |
| `dt.unix_timestamp()` | `__cobrust_molt_datetime_unix_timestamp` | `(molt.DateTime) -> i64` |
| scope-exit drop | `__cobrust_molt_datetime_drop` | `(molt.DateTime) -> ()` |
| `hood.Command(name, help)` | `__cobrust_hood_command_new` | `(str, str) -> hood.Command` |
| `cmd.handler(fn)` | `__cobrust_hood_command_handler` | `(hood.Command, Callback(fn() -> i64)) -> i64` |
| `cmd.run()` | `__cobrust_hood_command_run` | `(hood.Command) -> i64` |
| scope-exit drop | `__cobrust_hood_command_drop` | `(hood.Command) -> ()` |

- `den.Connection` / `den.Cursor` / `strike.Response` / `molt.DateTime`
  / `pit.App` / `pit.Request` / `pit.Response` / `pit.ServerHandle`
  / `hood.Command` are **nominal handle types**: `Ty::Adt(AdtId)` with
  reserved ids `>= 0xE000_0000` (`cobrust_types::ecosystem::ECO_ADT_BASE`).
  Non-`Copy`, drop-scheduled. Per-module reservation convention: each
  module gets a 256-slot block starting at `ECO_ADT_BASE + N*0x100`
  (`den`: 0xE000_0000..0xE000_00FF;
  `strike`: 0xE000_0100..0xE000_01FF;
  `scale`: 0xE000_0200..0xE000_02FF (reserved for a future bytes-ABI
  handle; no handles in the first proof);
  `molt`: 0xE000_0300..0xE000_03FF;
  `pit`: 0xE000_0400..0xE000_04FF;
  `hood`: 0xE000_0500..0xE000_05FF;
  new handle-typed modules take the next block).
- `fetchall` returns a `str` rendering for the first proof
  (`[(42,)]`); `row -> list[tuple]` is the immediate follow-up.
- `nest.loads_str` is **pure value-in-value-out** (`str -> str`): the
  TOML source goes in, its canonical-JSON rendering comes out. No
  handles, no callbacks; the returned `Str` is freed by the existing
  Str drop schedule. Parse errors are returned as a JSON sentinel
  `{"err":"…"}` (matching the `cobrust-nest-json` subprocess bridge);
  a typed `Result[str, E]` surface is a follow-up.
- Tier: `den` first proof = `strict`; `nest.loads_str` = `semantic`
  (TOML→JSON canonicalization, Q6; L2-verifier bind deferred);
  `strike` = `semantic` (HTTP is not a bit-for-bit parity surface —
  timing, header ordering, connection-pool side effects are
  behavior-equivalent rather than identical);
  `scale` = `semantic` (msgpack canonical-form behavioral parity for
  the unpack value tree; the HEX wrapper is Cobrust-specific);
  `molt` = `semantic` (datetime parsing / formatting variants are
  behavior-equivalent rather than bit-for-bit CPython parity).
- `strike.get` / `strike.post` and the Response methods all fail
  **cleanly** at the C-ABI boundary: any network error / invalid URL
  / non-JSON body returns a sentinel Response (`status_code == 0`,
  empty `text()`, `{}` for `json()`). NO panic, NO null — the `.cb`
  caller checks `resp.status_code() == 0` to detect failure. Mirrors
  the std.json / F59 empty-Str sentinel convention.

## Layer map (the proven flat-intrinsic chain, keyed on ecosystem alias)

```mermaid
flowchart TD
  A["`.cb`: import den; den.connect(:memory:)"] --> B
  B["cobrust-types: ecosystem manifest<br/>check.rs try_synth_ecosystem_call"] --> C
  C["cobrust-mir: lower.rs try_lower_ecosystem_call<br/>retarget func = Constant::Str(&quot;__cobrust_den_*&quot;)"] --> D
  D["cobrust-codegen: declare_runtime_helpers externs<br/>+ emit_drop_for_ty handle drop"] --> E
  E["cobrust-den: cabi.rs #[no_mangle] shims (libden.a)"] --> F
  F["cobrust-cli build.rs: locate_ecosystem_archive<br/>per-import static link (libden.a after stdlib; Linux --start-group)"]
```

### L1 — typecheck (`cobrust-types`)
- `src/ecosystem.rs` — the Rust-table manifest (Q2): `lookup_module_fn`,
  `lookup_handle_method`, `den_connection_ty`/`den_cursor_ty`,
  `handle_drop_symbol`, `is_ecosystem_module`/`is_ecosystem_handle`.
- `src/check.rs`:
  - `prebind_item` Import arm records ecosystem aliases in
    `ecosystem_module_defs` (def_id → module name) and records the
    alias's value-type as `Ty::None` (not a fresh var → no
    `AmbiguousType` leak at finalize).
  - `synth_call` → `try_synth_ecosystem_call` fires first: Case 1
    `Name(import-alias).attr(...)` → `lookup_module_fn`; Case 2
    `<handle>.attr(...)` → `lookup_handle_method`. Arity + arg types
    checked by `check_eco_sig`.

### L2 — MIR lowering (`cobrust-mir/src/lower.rs`)
- `try_lower_ecosystem_call` (called first in `lower_call`) emits a
  `Terminator::Call` with `func = Constant::Str(sig.runtime_symbol)` and
  a `_ecoret` destination carrying the manifest return type.
- Receiver of a handle method is `Move → Copy`-upgraded
  (`upgrade_move_to_copy_handle`) — the shim BORROWS it, so the local
  must stay live for its single scope-exit drop. Str args
  (`upgrade_move_to_copy_for_str`) are borrow-not-move too.
- `synth_expr_ty` extended so a chained `conn.execute(sql).fetchall()`
  resolves the inner call to its handle `Ty::Adt`.

### L3 — codegen (`cobrust-codegen/src/llvm_backend.rs`)
- `declare_runtime_helpers` declares the 5 `__cobrust_den_*` externs
  over `{ptr, ptr}` / `{ptr}` and registers their param counts.
- `emit_drop_for_ty`: `Ty::Adt(id, _) => handle_drop_symbol(id)` — the
  reserved-id handle gets its foreign drop symbol, emitted once at scope
  exit by the same drop schedule that handles Str/List.

### L4 — runtime shims (`cobrust-den/src/cabi.rs`)
- `#[no_mangle] extern "C"` shims over the opaque-pointer ABI. Handles
  are `Box::into_raw`'d (connect/execute) and `Box::from_raw`'d once
  (the `_drop` shims). execute/fetchall BORROW (`&*` / `&mut *`).
- `__cobrust_str_*` declared `extern "C"`, resolved from
  `libcobrust_stdlib.a` at link (Q5 — no Rust dep on cobrust-stdlib).
- `DROP_COUNT` instrument: the cabi round-trip test asserts exactly 4
  drops (3 cursors + 1 connection), each once.

### L5 — link (`cobrust-cli/src/build.rs`)
- `collect_ecosystem_modules(&mir)` (in `build/intrinsics.rs`) scans
  retargeted `Constant::Str` callees for the `__cobrust_<mod>_*` prefix.
  Currently recognized prefixes: `__cobrust_den_*` → `den`,
  `__cobrust_nest_*` → `nest`, `__cobrust_strike_*` → `strike`,
  `__cobrust_scale_*` → `scale`, `__cobrust_molt_*` → `molt`. New
  modules extend `ecosystem_module_for_symbol`.
- `locate_ecosystem_archive(module, release)` finds (or dev-builds)
  `lib<mod>.a`; the link line appends only the imported modules'
  archives, AFTER `libcobrust_stdlib.a` (both are Rust staticlibs that
  embed libstd; this order de-dups it). On Linux the stdlib + ecosystem
  archives are wrapped in `--start-group/--end-group` for single-pass
  GNU ld. `cobrust-den` / `cobrust-nest` / `cobrust-strike` /
  `cobrust-scale` / `cobrust-molt` crate-types include `staticlib`.
  Only imported modules link (risk 3: no link bloat).

## Done-means (ADR-0072 §4) — verification state

### `den` first proof
1. Type-checks against the manifest, no `AmbiguousType`. ✅
2. MIR retargets to `__cobrust_den_*`. ✅ (`nm` shows all 5 symbols)
3. `cc` links `prog.o + cobrust_main.o + libcobrust_stdlib.a + libden.a`
   (ecosystem after stdlib so each archive's embedded libstd de-dups;
   Linux wraps them in `--start-group/--end-group` so single-pass GNU
   ld still resolves den's `__cobrust_str_*` back-references), no
   unresolved symbols. ✅
4. Binary opens `:memory:`, CREATE/INSERT/SELECT, prints `[(42,)]`,
   exit 0. ✅ (`crates/cobrust-cli/tests/ecosystem_den_e2e.rs`)
5. No leak/UAF — handle drops fire once at scope exit. ✅
   (`cobrust-den::cabi::tests::cabi_round_trip_prints_42_and_drops_once`)

### `nest` second-module proof
1. Type-checks against the manifest, no `AmbiguousType`. ✅
2. MIR retargets to `__cobrust_nest_loads_str`. ✅
3. `cc` links `prog.o + cobrust_main.o + libcobrust_stdlib.a + libnest.a`
   (same link policy as den). ✅
4. Binary parses `title = "hello"` + `[server]\nport=8080`, prints the
   canonical JSON `{"title":"hello"}` / `{"server":{"port":8080}}`,
   exit 0. ✅ (`crates/cobrust-cli/tests/ecosystem_nest_e2e.rs`)
5. Drop correctness: no handles in this surface; the input + output
   `Str` buffers are freed by the existing Str drop schedule (the
   "easy case" the chain handles natively — ADR-0072 §5 risk 1 is a
   non-concern for pure value-in-value-out shims). ✅ (cabi unit
   tests in `cobrust-nest/src/cabi.rs`)

### `strike` third-module proof
1. Type-checks against the manifest, no `AmbiguousType`. ✅ (the
   `strike.Response` handle is a fresh reserved-AdtId block; receiver
   inference for `resp.text()` / `.status_code()` / `.json()` routes
   through `lookup_handle_method` exactly like den's Cursor methods).
2. MIR retargets to `__cobrust_strike_*`. ✅
3. `cc` links `prog.o + cobrust_main.o + libcobrust_stdlib.a + libstrike.a`
   (same link policy as den / nest). ✅
4. The compiled `.cb` binary issues a real HTTP `GET` over loopback
   against a `pit::App` axum server, prints `pong\n200\n` for `/ping`
   and `{"x":42}\n200\n` for `/json` (canonical-JSON rendering, same
   shape as den's `fetchall() -> str`), and falls back to `\n0\n` for
   an unreachable URL — the fail-clean sentinel survives the full
   compile → link → run path with NO panic. ✅
   (`crates/cobrust-cli/tests/ecosystem_strike_e2e.rs`)
5. Drop correctness: the `Response` handle drops exactly once at
   scope exit via `__cobrust_strike_response_drop`. ✅ (cabi unit
   tests in `cobrust-strike/src/cabi.rs::DROP_COUNT` instrument;
   `cabi_round_trip_borrows_then_drops_once` +
   `cabi_get_with_invalid_url_returns_status_zero_sentinel` both
   assert `delta == 1` under a serialized counter lock).

### `scale` fourth-module proof
1. Type-checks against the manifest, no `AmbiguousType`. ✅
2. MIR retargets to `__cobrust_scale_*`. ✅
3. `cc` links `prog.o + cobrust_main.o + libcobrust_stdlib.a + libscale.a`
   (same link policy as den / nest / strike). ✅
4. The compiled `.cb` binary round-trips `{"key":"value"}` and
   `{"items":[1,2,3],"name":"x"}` through `scale.dumps_str` (JSON →
   msgpack-HEX) → `scale.loads_str` (HEX → canonical JSON) and prints
   the inputs back unchanged. ✅
   (`crates/cobrust-cli/tests/ecosystem_scale_e2e.rs`)
5. Drop correctness: no handles in this surface; the input + output
   `Str` buffers are freed by the existing Str drop schedule (the
   "easy case" the chain handles natively, same as `nest`). ✅
   (cabi unit tests in `cobrust-scale/src/cabi.rs`)

### `molt` fifth-module proof
1. Type-checks against the manifest, no `AmbiguousType`. ✅ (the
   `molt.DateTime` handle is a fresh reserved-AdtId block in the
   FOURTH 256-slot range; method inference for `now.isoformat()` /
   `.unix_timestamp()` routes through `lookup_handle_method`).
2. MIR retargets to `__cobrust_molt_*`. ✅
3. `cc` links `prog.o + cobrust_main.o + libcobrust_stdlib.a + libmolt.a`
   (same link policy as den / strike). ✅
4. The compiled `.cb` binary captures the current UTC time, prints
   the RFC3339 isoformat + UNIX epoch seconds, and a twin-invocation
   variant proves the wall clock is monotone across two scope-local
   handles. ✅ (`crates/cobrust-cli/tests/ecosystem_molt_e2e.rs`)
5. Drop correctness: the `DateTime` handle drops exactly once at
   scope exit via `__cobrust_molt_datetime_drop`. ✅
   (cabi unit tests in `cobrust-molt/src/cabi.rs::DROP_COUNT`
   instrument; `cabi_round_trip_drops_once` asserts `delta == 1`
   under a serialized counter lock).

### Generalization finding

The second-module (nest) wiring touched 4 source files and added 2 (the
new shim crate + its E2E test). Of those edits:
- 3 were strictly additive (manifest row, codegen extern block,
  collected-module recognizer) — pure data, no logic change.
- 1 was a true generalization: `ecosystem_module_for_symbol` in
  `cobrust-cli/src/build/intrinsics.rs` was den-specific (single
  `starts_with("__cobrust_den_")` branch). Generalized to an alternation
  per recognized module prefix. New modules extend this in one place.

The third-module (strike) wiring confirmed the chain is FULLY general
for the handle pattern too — strike pairs handle methods (`Response.text`
/ `.status_code` / `.json`, like den's `Cursor.fetchall`) with free-
function entrypoints (`get`/`post`, like `den.connect`). The wiring
needed:
- A new manifest block (`STRIKE_RESPONSE_ADT` + `strike_response_ty()` +
  drop-symbol row + `lookup_module_fn` arms + `lookup_handle_method`
  arms + `is_ecosystem_module` alternation) — pure data.
- A new codegen extern block (6 symbols: `get` / `post` / 3 borrowing
  Response accessors / `_drop`) — pure data.
- One line in `ecosystem_module_for_symbol` (the alternation already
  generalized for nest accepted a strike prefix without touching shape).
- The new shim crate (`cobrust-strike/src/cabi.rs`) — the L4 runtime
  shim with the Box::into_raw / Box::from_raw + `&*` borrow pattern,
  drop-count instrument, fail-clean sentinel returns, mirroring
  `cobrust-den/src/cabi.rs` line for line.
- The new E2E test (`crates/cobrust-cli/tests/ecosystem_strike_e2e.rs`)
  which spins a loopback `pit::App` (workspace member) for an in-process
  HTTP endpoint, then compiles + runs the `.cb` binary against it.

NO chain-logic changes were needed: `check.rs` `try_synth_ecosystem_call`,
`lower.rs` `try_lower_ecosystem_call`, `emit_drop_for_ty`,
`locate_ecosystem_archive`, the link policy, and the
`upgrade_move_to_copy_handle` receiver-borrow pass all stayed UNTOUCHED.
The reserved-AdtId block convention (`ECO_ADT_BASE + N*0x100`) lets new
handle-typed modules coexist with den without collision.

### `scale` + `molt` 5-module proof — generalization finding

The fourth (`scale`) + fifth (`molt`) wiring landed in ONE batch and
**confirms the chain is fully general** after the strike third-module
proof. Per-layer cost:

- `cobrust-types/src/ecosystem.rs`: pure additive — 2 free-fn rows for
  scale (`dumps_str` + `loads_str`), 1 handle-id constant
  (`MOLT_DATETIME_ADT`, FOURTH 256-slot block), 1 handle-`Ty`
  constructor, 1 drop-symbol arm, 1 free-fn row + 2 method rows for
  molt, and `is_ecosystem_module` alternation extended from 3 → 5.
  9 new unit tests.
- `cobrust-codegen/src/llvm_backend.rs`: pure additive — 2 extern
  decls for scale (str → str), 4 extern decls for molt
  (`now() -> ptr`, `isoformat(ptr) -> ptr`, `unix_timestamp(ptr) -> i64`,
  `drop(ptr)`). `emit_drop_for_ty` picks up `MOLT_DATETIME_ADT` via
  `handle_drop_symbol` with no code change.
- `cobrust-cli/src/build/intrinsics.rs`: 2 lines added to the
  `ecosystem_module_for_symbol` alternation (`__cobrust_scale_*` /
  `__cobrust_molt_*` prefix arms).
- `cobrust-scale/src/cabi.rs` (new) + `cobrust-molt/src/cabi.rs` (new):
  the L4 runtime shims, mirroring nest (scale, value pattern) and
  den/strike (molt, handle pattern + DROP_COUNT). Both add
  `staticlib` to crate-type + `cobrust-stdlib` dev-dep + macOS
  cdylib `-Wl,-undefined,dynamic_lookup` build.rs.
- 2 new E2E tests (`ecosystem_scale_e2e.rs` + `ecosystem_molt_e2e.rs`),
  compile → link → run, both passing. den/nest/strike E2E regression
  green.

**Chain-logic edits this batch**: ZERO. The chain genuinely supports
N modules off pure-data additions; the only generalization step
required was the recognizer alternation (one new line per module, same
as nest already established). The 256-slot AdtId block convention also
extends to a "block-per-module-even-if-no-handles-yet" rule (scale
reserves the THIRD block without populating it, so a future raw-bytes
ABI handle can land without renumbering molt's block) — this is the
honest finding from a 5-module proof: when the chain is general, the
constraint that shows up next is **address-space reservation
discipline**, not generalization debt.

### Honest finding — source-level `<module>.<HandleType>` annotation gap

The example program in the ADR-0072 sprint brief used an explicit type
annotation: `let resp: strike.Response = strike.get(...)`. This fails
to type-check today — the typechecker resolves `strike.Response` as a
`Ty::Alias` (it goes through the alias-path resolver before the
ecosystem manifest lookup), so it doesn't unify with the `Ty::Adt`
returned by the manifest-driven `strike.get(...)`. The strike E2E
sidesteps this by relying on type inference (`let resp = strike.get(...)`,
no annotation) — exactly like `den`'s E2E does for `let conn = den.connect(...)`.

This is a real generalization gap: source-level path annotations for
ecosystem handle types are not yet routed through the manifest. It is
NOT specific to strike — it would affect any user writing
`let conn: den.Connection = den.connect(...)` today. The minimal fix is
in `cobrust-types/src/check.rs` where the type-expression resolver
synthesizes `Ty::Alias` for any unrecognized `<base>.<attr>` path; that
path should consult `is_ecosystem_module(base) && lookup_handle_method`
/ a new `lookup_handle_ty(base, attr)` first. Tracked as a follow-up
to ADR-0072; not blocking the third-module proof (the no-annotation
form works identically and is what real-LLM-written code tends to use,
per CLAUDE.md §2.5 training-data-overlap).

## ADR-0073 — `pit` first proof (the SIXTH module, FIRST with a callback)

After the 5-module data-only generalization, `pit` (Flask web-server,
ADR-0071 rebrand) brings the next qualitatively new pattern: a
**callback parameter** crossing the C ABI. `App.route(method, path,
handler)` takes a top-level `.cb` fn as its third argument; the
codegen materialises the fn pointer via the `function_ids` table
(ADR-0073 §2 D3) and the Rust trampoline transmutes it back into a
`move |req| -> resp` closure satisfying axum's `Send + Sync + 'static`
handler bound.

### New machinery (ADR-0073 §4)

- `cobrust-types/src/ecosystem.rs`: new `EcoParam { Value(Ty),
  Callback(FnTy) }` enum; `EcoSig::params` migrated from `Vec<Ty>` to
  `Vec<EcoParam>`. pit handles reserved in the FIFTH 256-slot AdtId
  block (`0xE000_0400..0xE000_04FF`). 4 handle ids (App, Request,
  Response, ServerHandle) + 6 drop symbols + 4 manifest rows
  (`pit.App`, `pit.text_response`, `App.route`, `App.serve_in_background`).
  `PIT_REQUEST_ADT` deliberately returns `None` from
  `handle_drop_symbol` — Rust owns the Request box around each callback
  invocation; the `.cb` side must not drop it (ADR-0073 §2 D6).
- `cobrust-types/src/check.rs::check_eco_sig`: dispatches on `EcoParam`
  per slot. `Callback(expected_fn)` requires the source arg to be a
  bare `ExprKind::Name(rn)` whose `DefKind == Fn`; unifies the resolved
  `Ty::Fn(actual)` against `expected_fn`. New TypeError variants
  `CallbackArgMustBeFnName` + `CallbackSignatureMismatch`.
- `cobrust-types/src/check.rs::lower_named_type`: recognises dotted
  ecosystem-handle annotations (`pit.Request`, `pit.Response`, etc.)
  so `fn handle(req: pit.Request) -> pit.Response: …` lowers to the
  matching `Ty::Adt` ids the manifest emits.
- `cobrust-mir/src/lower.rs::try_lower_ecosystem_call`: per-slot
  dispatch via new `lower_eco_arg(b, arg, kind)` helper. `Callback`
  slot extracts `rn.def_id.0` from the source `Name` and emits
  `Operand::Constant(Constant::FnRef(def_id))` directly.
- `cobrust-codegen/src/llvm_backend.rs:3876` (the ADR-0034-preserved
  zero stub): now materialises `Constant::FnRef(id)` as
  `function_ids[id].as_global_value().as_pointer_value()`. Unknown ids
  (lambda placeholder `FnRef(0)`, await placeholder `FnRef(u32::MAX)`)
  keep the legacy i64-zero stub for defense in depth.
- `cobrust-codegen/src/llvm_backend.rs::declare_runtime_helpers`:
  7 new `__cobrust_pit_*` extern decls — `app_new`, `text_response`,
  `app_route` (4 args incl fn-ptr slot), `app_serve_in_background`,
  `app_drop`, `response_drop`, `server_handle_drop`.
- `cobrust-pit/src/cabi.rs` (NEW): the load-bearing trampoline. The
  closure captures only the raw fn pointer (auto-`Send + Sync + Copy`),
  satisfies `'static` because the `.cb` fn lives in the binary text
  segment for the process lifetime (ADR-0073 §5 risk 1), and wraps
  the callback in `std::panic::catch_unwind` to abort cleanly on
  cross-boundary unwinding (ADR-0073 §3 Q5).
- `cobrust-pit/Cargo.toml`: `staticlib` added to crate-type for
  `libpit.a`; `cobrust-stdlib` as dev-dep for cabi unit-test linkage.
- `cobrust-pit/build.rs` (NEW): macOS `-Wl,-undefined,dynamic_lookup`
  for `__cobrust_str_*` extern resolution at PyO3 cdylib build time.
- `cobrust-cli/src/build/intrinsics.rs::ecosystem_module_for_symbol`:
  `__cobrust_pit_*` recognizer arm (one-line; the chain stays
  module-agnostic otherwise).

### `App.route` returns `Ty::None` (NOT App handle)

The trampoline mutates the receiver in place; identity-returning the
App pointer would alias it into a second drop-eligible local
(`let app2 = app.route(...)`), causing `__cobrust_pit_app_drop` to
fire twice at scope exit. The canonical .cb shape is
`let _ = app.route("GET", "/x", handler)`.

### Negative-callback corpus (ADR-0073 §5 R4 — ≥5 cases)

`crates/cobrust-cli/tests/pit_pong_e2e.rs` ships 5 negatives:
lambda / 0-arg fn / wrong-return / non-fn name / call-result — each
prints either `CallbackArgMustBeFnName` or `CallbackSignatureMismatch`
with a §2.5-B fix suggestion.

### E2E (ADR-0073 §6 done-means)

`crates/cobrust-cli/tests/pit_pong_e2e.rs::test_e2e_pit_pong_full_round_trip`:
picks a free port, compiles + runs the .cb pong program as a subprocess,
polls until the server binds, issues `GET /ping` via `reqwest::blocking`,
asserts body == "pong" + status 200, then asserts `GET /missing` → 404.

`cobrust-pit/src/cabi.rs::tests::trampoline_invokes_handler_and_drops_handles_once`:
drives the trampoline directly (not through .cb), proving the
transmute + closure-wrap + drop discipline in isolation.

## ADR-0073 second proof — `hood` (the SEVENTH module, SECOND with a callback)

After pit proved the callback chain crosses a `fn(Request) -> Response`
through the C ABI, `hood` (click-rebrand, CLI commands) reuses the
SAME chain for a different callback shape: `fn() -> i64`. Same
trampoline pattern, same drop discipline, same compile-time-catch
gate. The MIR / typecheck / drop / link-locate layers are
**unchanged** — chain generality holds.

### New machinery (mirrors ADR-0073 §4 for hood)

- `cobrust-types/src/ecosystem.rs`: hood handles reserved in the SIXTH
  256-slot AdtId block (`0xE000_0500..0xE000_05FF`). 1 handle id
  (`HOOD_COMMAND_ADT`) + 1 drop symbol + 3 manifest rows
  (`hood.Command(name, help)`, `Command.handler(fn)`, `Command.run()`).
  `Command.handler` is the load-bearing site — uses the existing
  `EcoParam::Callback(FnTy)` variant with a `fn() -> i64` FnTy.
- `cobrust-types/src/check.rs::lower_named_type`: adds `hood.Command`
  arm so the (rare today, future-proof) annotation
  `fn x(cmd: hood.Command) -> ...:` lowers correctly.
- `cobrust-codegen/src/llvm_backend.rs::declare_runtime_helpers`:
  4 new `__cobrust_hood_*` extern decls — `command_new`,
  `command_handler` (2 args incl fn-ptr slot), `command_run`,
  `command_drop`.
- `cobrust-hood/src/cabi.rs` (NEW): the trampoline. Stores the bound
  callback as a `Box<dyn Fn() -> i64 + Send + Sync + 'static>` closure
  capturing `raw: CbHandlerAbi` (auto-`Send + Sync + Copy`). Same
  panic-abort + `'static` AOT text-segment claim as pit. The closure
  invokes the fn-ptr with a null `*mut u8` placeholder per ADR-0073
  §5.1's zero-arg-zero-result pattern (the source-level `-> i64`
  return is the user's exit-code intent; the handler's printf side-
  effect IS the value for the first proof).
- `cobrust-hood/Cargo.toml`: `staticlib` added to crate-type for
  `libhood.a`; `cobrust-stdlib` as dev-dep for cabi unit-test linkage.
- `cobrust-hood/build.rs` (NEW): macOS `-Wl,-undefined,dynamic_lookup`
  for `__cobrust_str_*` extern resolution at PyO3 cdylib build time.
- `cobrust-cli/src/build/intrinsics.rs::ecosystem_module_for_symbol`:
  `__cobrust_hood_*` recognizer arm (one-line; the chain stays
  module-agnostic otherwise — `locate_ecosystem_archive` picks up
  `libhood.a` out of the box).

### `Command.handler` returns `Ty::Int` (NOT Command)

Same discipline as pit's `App.route -> Ty::None`: the trampoline
mutates the receiver in place; identity-returning the Command pointer
would alias it into a second drop-eligible local
(`let cmd2 = cmd.handler(...)`), causing `__cobrust_hood_command_drop`
to fire twice at scope exit. The canonical .cb shape is
`let _ = cmd.handler(handle_greet)`. Zero is the sentinel.

### E2E (ADR-0073 second-proof done-means)

`crates/cobrust-cli/tests/hood_cmd_e2e.rs::test_e2e_hood_cmd_handler_round_trip`:
compiles + runs the .cb greet program as a subprocess via
`std::process::Command`, asserts stdout contains "hello from hood"
+ exit code 0. 3 negative-callback cases ship alongside
(wrong-arity / wrong-return / lambda); each reuses the SHARED
`check_callback_arg` gate so the diagnostic phrasing matches pit's.

`cobrust-hood/src/cabi.rs::tests::trampoline_invokes_handler_and_drops_once`:
drives the trampoline directly (not through .cb), proves the
transmute + closure-wrap + drop discipline in isolation.

### Chain-generality metric

`git diff --stat crates/cobrust-{mir,hir,codegen}/` after the hood
sprint: zero hir changes, zero MIR changes, ~30 lines codegen
(extern decls only). The mir / hir / drop / link-locate layers are
unchanged — proving the chain generalizes off pit's pattern, the
same way ADR-0072's data-modules generalized off den.

## Constraints / follow-ups

- The milestone program must be wrapped in `fn main() -> i64:` — bare
  module-level execution is a pre-existing toolchain limitation (the AOT
  entry `_cobrust_user_main` is emitted from `fn main`), not specific to
  this wiring.
- Handles are scope-local only (Q4): no return/store/capture escape.
  Escape-transfer (move/borrow) semantics are tracked follow-ups.
- `!Send` (risk 2): single-threaded only; do not cross handles into
  spawned tasks.
- Follow-ups: `row -> list[tuple]` marshalling, handle-escape rules,
  `coil.Array` ABI, and generalizing the remaining cobra modules off
  this proven chain.
