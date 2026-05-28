---
doc_kind: reference
module_id: cb-ecosystem-import
title: .cb ecosystem-import wiring (ADR-0072 first proof — den; second-module proof — nest; third-module proof — strike)
last_verified_commit: HEAD
relates_to: [adr:0072, adr:0019, adr:0028, adr:0050c, adr:0071]
dependencies: [cobrust-types, cobrust-mir, cobrust-codegen, cobrust-den, cobrust-nest, cobrust-strike, cobrust-cli]
---

# `.cb` ecosystem-import wiring — `import den` / `import nest` / `import strike` end-to-end

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

- `den.Connection` / `den.Cursor` / `strike.Response` are **nominal
  handle types**: `Ty::Adt(AdtId)` with reserved ids `>= 0xE000_0000`
  (`cobrust_types::ecosystem::ECO_ADT_BASE`). Non-`Copy`, drop-scheduled.
  Per-module reservation convention: each module gets a 256-slot block
  starting at `ECO_ADT_BASE + N*0x100` (`den`: 0xE000_0000..0xE000_00FF;
  `strike`: 0xE000_0100..0xE000_01FF; new handle-typed modules take the
  next block).
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
  behavior-equivalent rather than identical).
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
  `__cobrust_nest_*` → `nest`, `__cobrust_strike_*` → `strike`. New
  modules extend `ecosystem_module_for_symbol`.
- `locate_ecosystem_archive(module, release)` finds (or dev-builds)
  `lib<mod>.a`; the link line appends only the imported modules'
  archives, AFTER `libcobrust_stdlib.a` (both are Rust staticlibs that
  embed libstd; this order de-dups it). On Linux the stdlib + ecosystem
  archives are wrapped in `--start-group/--end-group` for single-pass
  GNU ld. `cobrust-den` / `cobrust-nest` / `cobrust-strike` crate-types
  include `staticlib`. Only imported modules link (risk 3: no link bloat).

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
