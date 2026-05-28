---
doc_kind: reference
module_id: cb-ecosystem-import
title: .cb ecosystem-import wiring (ADR-0072 first proof — den)
last_verified_commit: HEAD
relates_to: [adr:0072, adr:0019, adr:0028, adr:0050c, adr:0071]
dependencies: [cobrust-types, cobrust-mir, cobrust-codegen, cobrust-den, cobrust-cli]
---

# `.cb` ecosystem-import wiring — `import den` end-to-end

Status: ADR-0072 first proof landed. `import den` + `den.connect` /
`Connection.execute` / `Cursor.fetchall` compile → link → run.

## Surface (manifest-defined)

| Source form | Retargeted symbol | Signature |
|---|---|---|
| `den.connect(path)` | `__cobrust_den_connect` | `(str) -> den.Connection` |
| `conn.execute(sql)` | `__cobrust_den_connection_execute` | `(den.Connection, str) -> den.Cursor` |
| `cur.fetchall()` | `__cobrust_den_cursor_fetchall` | `(den.Cursor) -> str` |
| scope-exit drop | `__cobrust_den_connection_drop` | `(den.Connection) -> ()` |
| scope-exit drop | `__cobrust_den_cursor_drop` | `(den.Cursor) -> ()` |

- `den.Connection` / `den.Cursor` are **nominal handle types**:
  `Ty::Adt(AdtId)` with reserved ids `>= 0xE000_0000`
  (`cobrust_types::ecosystem::ECO_ADT_BASE`). Non-`Copy`, drop-scheduled.
- `fetchall` returns a `str` rendering for the first proof
  (`[(42,)]`); `row -> list[tuple]` is the immediate follow-up.
- Tier: `den` first proof = `strict` (Q6; L2-verifier bind deferred).

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
- `locate_ecosystem_archive(module, release)` finds (or dev-builds)
  `lib<mod>.a`; the link line appends only the imported modules'
  archives, AFTER `libcobrust_stdlib.a` (both are Rust staticlibs that
  embed libstd; this order de-dups it). On Linux the stdlib + ecosystem
  archives are wrapped in `--start-group/--end-group` for single-pass
  GNU ld. den crate-type gains `staticlib`. Only imported modules link
  (risk 3: no link bloat).

## Done-means (ADR-0072 §4) — verification state

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
