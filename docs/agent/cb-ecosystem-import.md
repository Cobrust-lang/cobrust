---
doc_kind: reference
module_id: cb-ecosystem-import
title: .cb ecosystem-import wiring (ADR-0072 — 5 data modules + coil 8th module first proof; ADR-0073 — pit callback marshalling 6th module + hood 7th module second proof; ADR-0076 — dora 9th module; ADR-0078 backend Phase 2 — fang 10th module, first -> bool value-fn)
last_verified_commit: HEAD
relates_to: [adr:0072, adr:0073, adr:0076, adr:0078, adr:0019, adr:0028, adr:0050c, adr:0071, adr:0034]
dependencies: [cobrust-types, cobrust-mir, cobrust-codegen, cobrust-den, cobrust-nest, cobrust-strike, cobrust-scale, cobrust-molt, cobrust-pit, cobrust-hood, cobrust-coil, cobrust-dora, cobrust-fang, cobrust-cli]
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
- ADR-0078 backend Phase 2 — **tenth-module generalization** landed.
  `import fang` + `fang.hash_password(pw) -> str` +
  `fang.verify_password(pw, hash) -> bool` compile → link → run, the
  FIRST backend Phase-2 crate (auth/security, wrapping the `argon2`
  crate). Pure value pattern like `nest`/`scale` (no handles, no
  `AdtId`), and the FIRST module with a `-> bool` value-fn return.
  Touched manifest + codegen extern (an `i1`-return extern) +
  recognizer alternation + the new `cobrust-fang` shim crate. One
  chain-logic change was required (NOT just data): a `str == str` /
  `str != str` natural-operator MIR rewrite in `lower_bin` — the corpus
  asserts `h1 != h2` (two PHC strings differ by salt), which previously
  crashed codegen ("Found PointerValue but expected IntValue") because
  the natural string-equality operator had no lowering (only the
  explicit `str_eq(a, b)` builtin did). See "fang tenth-module proof"
  below.

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
| `fang.hash_password(pw)` | `__cobrust_fang_hash_password` | `(str) -> str` |
| `fang.verify_password(pw, hash)` | `__cobrust_fang_verify_password` | `(str, str) -> bool` |

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

### ADR-0078 Phase-1 — `pit` tower-http middleware (`app.use_cors` / `use_trace` / `use_compression`)

`pit.App` gains three zero-value-arg, `Ty::None`-returning methods that
register a canned `tower_http` `Layer` preset on the axum `Router`:
`use_cors()` = `CorsLayer::permissive()`, `use_trace()` =
`TraceLayer::new_for_http()`, `use_compression()` =
`CompressionLayer::new()`. This is the cheapest ecosystem-chain
extension to date — it rides ADR-0073's pit-method chain VERBATIM (NO
new compiler mechanism, NO new handle, NO new async-收编):

- `cobrust-types/src/ecosystem.rs`: 3 new `PIT_APP_ADT` rows
  (`use_cors`/`use_trace`/`use_compression`) → `__cobrust_pit_app_use_*`,
  zero value-args, `ret = Ty::None`. `Ty::None` MIRRORS `App.route`'s
  discipline (side-effect on the receiver in place; returning an App
  pointer would alias a second drop-eligible handle and double-fire
  `__cobrust_pit_app_drop`). Canonical `.cb` shape: `let _ = app.use_cors()`.
- `cobrust-types/src/check.rs`: NO new code — the rows resolve through
  the existing `try_synth_ecosystem_call` Case-2 → `lookup_handle_method`
  → `check_eco_sig` path; a zero-`params` sig type-checks the zero-arg
  method call.
- `cobrust-mir/src/lower.rs`: NO new mechanism — `try_lower_ecosystem_call`
  Case-2 borrows the receiver (Move→Copy), zero positional args,
  `emit_ecosystem_call("__cobrust_pit_app_use_cors", Ty::None, [recv], …)`,
  identical to `app.route`.
- `cobrust-codegen/src/llvm_backend.rs`: 3 new extern decls in the pit
  block (shape `ptr -> ptr`, the App-receiver / None-return form,
  identical to `__cobrust_pit_request_body`).
- `cobrust-cli/src/build/intrinsics.rs`: NO change — the new symbols
  match the existing `__cobrust_pit_` prefix recognizer.
- `cobrust-pit/src/app.rs` (the real work): `App` gains `cors`/`trace`/
  `compress` `bool` flags (default `false`); `use_cors`/`use_trace`/
  `use_compression` setters flip them; `serve` reads the flags ONCE when
  building the `Router` and conditionally `.layer(...)`'s each preset.
- `cobrust-pit/src/cabi.rs` (the real work): 3 `__cobrust_pit_app_use_*`
  shims borrow `&mut App` (NOT consumed), call the setter, return null
  (Ty::None discard). No new `_drop` shim, no `DROP_COUNT` change.
- `cobrust-pit/Cargo.toml`: `tower-http = { version = "0.6", features =
  ["cors", "trace", "compression-full"] }`. tower-http 0.6.11 was already
  transitively in the lock (via reqwest) WITHOUT these features; making
  it a direct featured dep mutates `Cargo.lock` (+74 lines: brotli,
  async-compression, compression-codecs/core — staged per finding F64).

#### Before-serve contract (ADR-0078 §6.1 + audit LOW finding)

The flag is read at the moment the `Router` is constructed in `serve`.
A `use_cors()` call AFTER `serve`/`serve_in_background` has bound the
Router is a no-op. The cabi `serve_in_background`/`run` shims
`std::mem::take` the WHOLE `App` (flags included) into the value moved
into `serve`, so flags set before serve survive the take. The DEV impl
MUST NOT re-apply layers per-request (that would change the contract +
risk double-application on the hot path).

#### E2E (ADR-0078 §6.1 done-means)

`crates/cobrust-cli/tests/pit_middleware_e2e.rs` (6 cases): the
load-bearing PRIMARY proves `app.use_cors()` adds
`Access-Control-Allow-Origin` to a served response, with a paired
control (NO `use_cors` → header ABSENT) — the differential proof that
`use_cors` is the cause. Plus `use_compression` (4 KiB body round-trips
intact), `use_trace` (server still 200s), stacking (all three compose,
CORS survives), and a before-serve ordering invariant.
`cobrust-pit/src/cabi.rs::tests::use_middleware_flips_flag_and_drops_once`
proves the shims flip the flag + the App still drops exactly once (no
new handle).

#### Phase scope + deferred

Phase-1 ships the 3 canned presets ONLY. Configurable CORS origins /
custom-`.cb` middleware / validator / utoipa are ADR-0078 Phases 2/3 +
their own sub-ADRs (§9).

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

## ADR-0072 eighth-module proof — `coil` (the EIGHTH and FINAL cobra-batch module)

After den/nest/strike/scale/molt walked the value-handle chain and
pit/hood walked the callback chain, `coil` (numpy-rebrand) takes
the EIGHTH and final cobra-batch slot. Wired off the proven
value-handle precedent — pure value-handle (no callbacks) — coil
completes the workspace-vendored ecosystem chain v0.7.0 shipped.
The MIR / HIR / drop / link-locate layers are **unchanged** —
chain generality holds for the eighth time.

### Scope discipline (first proof only)

This proof intentionally scopes to constructors + repr ONLY. Three
explicit out-of-scope surfaces are deferred to a sub-ADR (per
ADR-0072 §"coil deep operator/index"):

- `a + b` (BinOp dispatch for Buffer — deep operator work; the
  `EcoParam` manifest doesn't model binary operators today, and the
  .cb-side BinOp dispatch needs a method-form lowering).
- `a[i]` (IndexExpr dispatch for Buffer — deep index work).
- `a.shape` (Attr access on handle — needs a handle-attr sub-ADR).
- `Buffer.dot(other)` (multi-handle methods — manifest extension).

Same scope discipline as nest's first proof (str→str only, no
structured TOML value surface).

### New machinery (mirrors ADR-0072 §4 for coil)

- `cobrust-types/src/ecosystem.rs`: coil handles reserved in the
  EIGHTH 256-slot AdtId block (`0xE000_0700..0xE000_07FF`); the
  SEVENTH block (`0xE000_0600..0xE000_06FF`) is claimed for dora
  per ADR-0076. 1 handle id (`COIL_BUFFER_ADT`) + 1 drop symbol +
  4 manifest rows (`coil.zeros(i64)`, `coil.ones(i64)`,
  `coil.eye(i64)`, `coil.print_buffer(Buffer)`).
- `cobrust-types/src/check.rs::lower_named_type`: adds `coil.Buffer`
  arm so the annotation `let a: coil.Buffer = …` lowers correctly.
- `cobrust-types/src/lib.rs`: re-exports `COIL_BUFFER_ADT` +
  `coil_buffer_ty`.
- `cobrust-codegen/src/llvm_backend.rs::declare_runtime_helpers`:
  5 new `__cobrust_coil_*` extern decls (`zeros`/`ones`/`eye`
  ctor shape `i64 -> *mut Buffer`; `print_buffer` shape
  `*mut Buffer -> i64`; `buffer_drop` shape `*mut Buffer -> void`).
- `cobrust-cli/src/build/intrinsics.rs::ecosystem_module_for_symbol`:
  `__cobrust_coil_*` recognizer arm (one-line; the chain stays
  module-agnostic otherwise — `locate_ecosystem_archive` picks up
  `libcoil.a` out of the box).
- `cobrust-coil/src/cabi.rs` (NEW): the value-handle shims. Each
  constructor calls the existing `coil::constructors::*` function
  with `Dtype::Float64` (zeros/ones/eye are all f64 in the first
  proof) and `Box::into_raw`s the resulting `Array`. `print_buffer`
  borrows the handle, calls `coil::print::array_repr`, and prints to
  stdout. `_drop` reclaims the Box (which owns the entire chain:
  Array → ArrayD → Vec<T>).
- `cobrust-coil/Cargo.toml`: `staticlib` added to crate-type for
  `libcoil.a`; `cobrust-stdlib` as dev-dep (reserved for future
  cabi unit tests that may need str-buffer allocations — e.g. when
  `Buffer.tolist() -> str` lands).
- `cobrust-coil/build.rs` (NEW): macOS `-Wl,-undefined,dynamic_lookup`
  for future `__cobrust_str_*` extern resolution at PyO3 cdylib
  build time (the first proof has no str args at the wire, but the
  flag is in place for the str-extension follow-up).

### `print_buffer` is the read primitive (NOT `Buffer.repr() -> str`)

The first-proof `print_buffer` directly prints via Rust-side
`println!` — it does NOT return a `.cb`-owned `Str` buffer. The
intent is the printed bytes (the user's side effect), matching pit's
"the handler's printf IS the user's intent" discipline for the
first-proof scope.

A future `Buffer.tolist() -> str` shape would lift the den-style
`__cobrust_str_*` extern wiring per ADR-0072 Q5. The `build.rs`
deferral flag is already in place for that extension, so the
follow-up sprint just adds the extern decls + the den-style
`read_str_buf` / `alloc_str_buffer` helpers without touching the
chain shape.

### E2E (ADR-0072 8/8 first-proof done-means)

`crates/cobrust-cli/tests/coil_hello_e2e.rs::test_e2e_coil_hello_zeros_round_trip`:
compiles + runs the .cb hello program as a subprocess via
`std::process::Command`, asserts stdout contains numpy's `array(`
prefix + `dtype=float64` marker + exit code 0. 2 negative cases
ship alongside:
- `test_neg_coil_rejects_unknown_function` — `coil.flatten(a)` is
  rejected at type-check (not in manifest).
- `test_neg_coil_zeros_rejects_str_argument` — `coil.zeros("three")`
  is rejected at type-check (wrong arg type).

`cobrust-coil/src/cabi.rs::tests`: 6 in-crate cabi unit tests
(zeros / ones / eye each prove drop-once via `DROP_COUNT`;
print_buffer borrows and returns 0 sentinel; null tolerance;
negative-n clamp).

### Chain-generality metric

`git diff --stat crates/cobrust-{mir,hir,codegen}/` after the coil
8/8 sprint: zero HIR changes, zero MIR changes, +40 lines codegen
(extern decls only). The mir / hir / drop / link-locate layers are
unchanged — proving the chain generalizes off the proven
value-handle pattern for the eighth time. Same metric profile as
nest/strike/scale/molt walked through the data-module chain off den.

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

## ADR-0076 ninth-module proof — `dora` (the NINTH module, THIRD with a callback; SYNTHETIC runtime Phase 1)

After pit/hood walked the callback chain through `fn(Request) ->
Response` and `fn() -> i64` respectively, `dora` (dora-rs robotics
dataflow runtime bridge) is the NINTH ecosystem module — and the
THIRD to cross a callback through the C ABI. The shape here is
`fn(dora.Event) -> i64`, mixing pit's Event-receiver borrow pattern
with hood's i64 exit-code intent. The MIR / HIR / drop / link-locate
layers are **unchanged** — chain generality holds for the ninth time.

Phase 1 is intentionally SYNTHETIC: `__cobrust_dora_node_run` mocks
one canned `("camera", "frame_001")` Event arrival without depending
on the real dora-rs daemon, mirroring F65's synthetic-LLM provider
precedent. The C-ABI chain is proven end-to-end; the real `dora-node-
api` integration is a Phase 2 deliverable.

### Scope discipline (Phase 1 first proof only)

This proof intentionally scopes to a 1-source 1-handler synthetic
runtime. Explicit out-of-scope surfaces are deferred to Phase 2 / 3:

- Real `dora-node-api` dependency + real coordinator orchestration
  (Phase 2).
- Multi-input / multi-output per-node handler vector (Phase 2).
- `@dora.node(inputs=[...], outputs=[...])` decorator desugar — extends
  ADR-0074 for module-receiver decorators (Phase 2, finding F68).
- Yaml-loaded dataflows (`dora.run("dataflow.yml")` — Phase 2).
- Arrow `RecordBatch` payload accessors beyond `data_str` (Phase 2 /
  sub-ADR 0076c).
- ROS2 bridge publish surface (sub-ADR 0076a — Phase 3).
- riscv64 cross-build of `cobrust-dora` (ADR-0075 Phase 1 dependency
  — Phase 3 stretch).
- Real-robotics CartPole simulation demo (Phase 3 deliverable).

Same scope discipline as nest's first proof (str→str only) and coil's
first proof (constructors + repr only).

### New machinery (mirrors ADR-0072 §4 for dora)

- `cobrust-types/src/ecosystem.rs`: dora handles reserved in the
  SEVENTH 256-slot AdtId block (`0xE000_0600..0xE000_06FF`); 2 handle
  ids (`DORA_NODE_ADT` 0x600 + `DORA_EVENT_ADT` 0x601) + 1 drop symbol
  (Event is Rust-owned per ADR-0073 §2 D6 — `handle_drop_symbol`
  returns `None` for `DORA_EVENT_ADT`, mirrors `PIT_REQUEST_ADT`) +
  `dora_event_handler_fn_ty()` returning `fn(dora.Event) -> i64` +
  6 manifest rows: `dora.Node(str) -> Node`, `dora.node(callback) -> i64`,
  `Node.run() -> i64`, `Node.shutdown() -> i64`, `Event.id() -> str`,
  `Event.data_str() -> str`. The Phase-2-reserved slots
  `0x602..0x6FF` stay open for ArrowArray / Metadata / Ros2Subscription
  follow-ups.
- `cobrust-types/src/check.rs::lower_named_type`: adds `dora.Node` +
  `dora.Event` arms so `fn detect(event: dora.Event) -> i64:` lowers
  correctly.
- `cobrust-types/src/lib.rs`: re-exports `DORA_NODE_ADT`,
  `DORA_EVENT_ADT`, `dora_node_ty`, `dora_event_ty`,
  `dora_event_handler_fn_ty`.
- `cobrust-codegen/src/llvm_backend.rs::declare_runtime_helpers`:
  8 new `__cobrust_dora_*` extern decls (`node_new` shape
  `*mut Str -> *mut Node`; `node_node` shape
  `*const c_void -> i64`; `node_run` / `node_shutdown` shape
  `*mut Node -> i64`; `event_id` / `event_data_str` shape
  `*mut Event -> *mut Str`; `node_drop` / `event_drop` shape
  `*mut Node -> void`).
- `cobrust-cli/src/build/intrinsics.rs::ecosystem_module_for_symbol`:
  `__cobrust_dora_*` recognizer arm (one-line; `locate_ecosystem_archive`
  picks up `libdora.a` out of the box).
- `cobrust-dora/src/cabi.rs` (NEW): the callback-bearing trampolines.
  `__cobrust_dora_node_node` stores the fn pointer in a process-global
  `AtomicPtr` slot; `__cobrust_dora_node_run` reads the slot, allocates
  a canned `DoraEventHandle { id: "camera", data_str: "frame_001" }`,
  invokes the handler via `std::mem::transmute` to the
  `CbHandlerAbi` shape (`unsafe extern "C" fn(*mut u8) -> *mut u8`),
  catches panics with `catch_unwind` + aborts per ADR-0073 §3 Q5, and
  frees the Event box on return. `event_id` / `event_data_str` are
  borrow shims that allocate fresh Cobrust `Str` buffers from the
  Rust-owned Event.
- `cobrust-dora/Cargo.toml`: `staticlib` + `cdylib` + `rlib` crate-type;
  `cobrust-stdlib` as dev-dep for in-crate cabi unit tests; no
  production deps in Phase 1 (`dora-node-api` is a Phase 2 add).
- `cobrust-dora/build.rs` (NEW): macOS
  `-Wl,-undefined,dynamic_lookup` for the `__cobrust_str_*` extern
  resolution at PyO3 cdylib build time.

### `dora.node(handler)` is the Phase 1 explicit-registration form

Phase 1 ships `dora.node(handler)` as a module-level free fn with
`EcoParam::Callback(dora_event_handler_fn_ty())`. The Phase 2 follow-
up replaces this with `@dora.node(inputs=[...], outputs=[...])`
decorator-form over the handler fn (extends ADR-0074 — see finding
F68 for the desugar-extension design). Until Phase 2 ships, the
explicit-form discards the i64-zero sentinel via
`let _ = dora.node(detect)`.

### `event.data_str()` is the Phase 1 payload primitive (NOT `event.data_arrow()`)

The first-proof `event.data_str()` returns a fresh Cobrust `Str`
buffer carrying the canned `"frame_001"` payload. A Phase 2
`event.data_arrow()` shape would surface Arrow `RecordBatch` accessors
for typed multi-element payloads (i64 array, f64 array, dict). Same
scope discipline as coil's `print_buffer`-not-`repr()`.

### E2E (ADR-0076 Phase 1 first-proof done-means)

`crates/cobrust-cli/tests/dora_hello_e2e.rs::test_e2e_dora_hello_synthetic_runtime_round_trip`:
compiles + runs the `.cb` hello program as a subprocess via
`std::process::Command`, asserts stdout contains
`"got frame: frame_001"` + exit code 0. 2 negative cases ship
alongside:

- `test_neg_dora_callback_rejects_zero_arity_fn` — `dora.node(bad)`
  where `bad() -> i64` (missing the `dora.Event` arg) is rejected at
  type-check via the SHARED `check_callback_arg` gate.
- `test_neg_dora_callback_rejects_wrong_return_type` — `dora.node(bad)`
  where `bad(event) -> str` is rejected at type-check.

`cobrust-dora/src/cabi.rs::tests`: 5 in-crate cabi unit tests
(node new/drop proves drop-once via `DROP_COUNT`; null-drop tolerance;
run-without-handler returns -1 sentinel; trampoline-invokes-handler
with canned event asserts the borrow shims surface the expected
strings; shutdown-returns-clean-sentinel idempotency).
`crates/cobrust-dora/tests/dora_pyo3_compiles.rs`: 1 shape-regression
test asserting the `crate-type = ["rlib", "cdylib", "staticlib"]`
literal in Cargo.toml.

### Chain-generality metric

`git diff --stat crates/cobrust-{mir,hir}/` after the dora Phase 1
sprint: **zero HIR changes, zero MIR changes**, ~40 lines codegen
(extern decls only). The mir / hir / drop / link-locate layers are
unchanged — proving the chain generalizes off the proven callback
pattern for the third time. Same metric profile as hood walked the
callback chain off pit's first proof.

## ADR-0078 backend Phase 2 — `fang` tenth-module proof (FIRST `-> bool` value-fn; auth/security over `argon2`)

`fang` is the TENTH ecosystem module and the FIRST ADR-0078 backend
Phase-2 crate. It is a **pure value-pattern** module (no handles, no
`AdtId`, no callbacks — the `nest`/`scale` template) wrapping the
`argon2` crate to expose two flat value-functions.

### Surface + security choices (elegance law — no auth footguns)

| Source form | Retargeted symbol | Signature | Tier |
|---|---|---|---|
| `fang.hash_password(pw)` | `__cobrust_fang_hash_password` | `(str) -> str` | semantic |
| `fang.verify_password(pw, hash)` | `__cobrust_fang_verify_password` | `(str, str) -> bool` | semantic |

- **argon2id only, defaults baked in.** `hash_password` always uses
  `argon2::Argon2::default()` = argon2id with OWASP-recommended params.
  NO algorithm / cost knob in Phase 1 → a weak algo/params cannot be
  picked by accident.
- **Full PHC string out.** The returned `str` is the self-describing
  `$argon2id$v=…$m=…,t=…,p=…$<salt>$<hash>` — the random salt + params
  travel WITH the hash. No separate-salt management.
- **Constant-time verify** (`argon2::Argon2::verify_password`) → no
  timing-attack footgun.
- **A wrong / malformed-hash password is a normal `false`**, NOT a
  panic / error across the boundary (CLAUDE.md §2.2: errors are not the
  default control path). No plaintext logging.
- Tier `semantic` — the PHC hash is nondeterministic (fresh salt per
  call), so this is behavioral parity (a hash verifies the password
  that produced it), NOT bit-for-bit output parity with any oracle.

### `-> bool` value-fn return (first on the chain)

`verify_password` is the FIRST ecosystem value-fn returning `Ty::Bool`.
The MIR lowering carries `sig.ret = Ty::Bool` through the existing
`emit_ecosystem_call` into the `_ecoret` bool local; codegen declares
the extern with an `i1` (`bool_type()`) return that lands in the i1
alloca (`write_place` → `coerce_value_to` bridges any i1/i8 width gap).
No new MIR/codegen mechanism — only an `i1`-return extern row alongside
the existing `i64`-return rows (`strike.status_code`, `molt.unix_timestamp`).

### One chain-logic change: `str == str` / `str != str` natural operator

The corpus asserts `h1 != h2` (two PHC strings differ by salt). This is
the FIRST `.cb` test exercising the NATURAL string-equality operator on
two `Ty::Str` LOCALS. It previously crashed codegen
(`llvm_backend.rs` `lower_binop` Eq/NotEq arms call `into_int_value()`,
but two str locals are `ptr` values → "Found PointerValue but expected
IntValue"). Only the explicit `str_eq(a, b)` builtin had a lowering;
the operator form had none.

Fix (`cobrust-mir/src/lower.rs` `lower_bin`, sibling of the Dict
`in`/`not in` arm): when `op ∈ {Eq, NotEq}` and the LHS resolves to
`Ty::Str`, retarget to the always-linked `__cobrust_str_eq(a, b) -> i64`
(0/1) then materialise the bool (`!= 0` for Eq, `== 0` for NotEq). Both
operands are BORROWED (Move→Copy upgrade — `__cobrust_str_eq` reads but
does not consume, so the source `str` locals survive for later uses and
drop ONCE at scope exit per the Str non-Copy discipline). String-LITERAL
comparisons keep flowing through the existing `str_eq_lit` PRELUDE path
(the guard fires only when the LHS resolves to a `Ty::Str` value).

### L4 runtime (`cobrust-fang/src/cabi.rs`)

Two `#[no_mangle] extern "C"` shims mirroring the scale/nest str-buffer
ABI (`__cobrust_str_new`/`_push_static`/`_ptr`/`_len` declared
`extern "C"`, resolved from `libcobrust_stdlib.a` at link; cobrust-stdlib
is a dev-dep only, for the in-crate cabi tests):

- `__cobrust_fang_hash_password(pw) -> *mut Str` — `SaltString::generate(&mut OsRng)`
  + `Argon2::default().hash_password(...)` → PHC string in a fresh Str
  buffer (empty-Str sentinel on the unreachable hashing error).
- `__cobrust_fang_verify_password(pw, hash) -> bool` —
  `PasswordHash::new(hash)` (malformed → `false`) +
  `Argon2::default().verify_password(...)` (`Ok` → `true`, any `Err` →
  `false`).

`cobrust-fang/Cargo.toml`: `[lib] crate-type = ["rlib", "cdylib",
"staticlib"]` (libfang.a for per-import link), `argon2 = "0.5"` +
`password-hash = { version = "0.5", features = ["getrandom"] }` (argon2's
`rand` feature only enables `password-hash/rand_core`, NOT
`/getrandom` — the explicit dep turns on `OsRng` via feature
unification). `build.rs` mirrors scale (macOS cdylib
`-undefined dynamic_lookup`).

### E2E (ADR-0078 backend Phase-2 done-means)

`crates/cobrust-cli/tests/ecosystem_fang_e2e.rs` (4 tests, all pass):

- `test_e2e_fang_hash_then_verify_round_trip_true` — hash then verify
  the same pw → `"1\n"` (TRUE).
- `test_e2e_fang_wrong_password_rejects_false` — verify a wrong pw
  against the hash → `"0\n"` (FALSE; the security property that
  matters).
- `test_e2e_fang_hash_is_argon2id_phc` — `h.starts_with("$argon2id$")`
  → `"1\n"` (argon2id, salt embedded).
- `test_e2e_fang_hash_is_nondeterministic_both_verify` — `h1 != h2`
  (random salt) AND both verify TRUE → `"1\n1\n1\n"`.

`cobrust-fang/src/cabi.rs::tests` (5 in-crate cabi unit tests):
round-trip TRUE; wrong-pw FALSE; argon2id-PHC-prefix;
nondeterministic-salt-both-verify; malformed/empty-hash-is-false-not-panic.

### Chain-generality metric

`git diff --stat crates/cobrust-hir/` after the fang sprint: **zero HIR
changes**. MIR changed by ONE arm (the `str ==`/`!=` operator rewrite —
a general capability gap surfaced by the corpus, not fang-specific
plumbing); codegen +~15 lines (the two extern rows, one with an `i1`
return). The drop / link-locate layers are unchanged — the per-import
`cobrust-<mod>` / `lib<mod>.a` locate logic resolved `fang`/`libfang.a`
with no edit. Tenth module on the chain; second value-pattern proof
after nest/scale.
