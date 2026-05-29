---
doc_kind: module
module_id: mod:fang
crate: cobrust-fang
last_verified_commit: HEAD
dependencies: [mod:types, mod:mir, mod:codegen, mod:stdlib]
---

# Module: fang

## Purpose

`cobrust-fang` bridges `.cb` source programs to password hashing /
verification, wrapping the [`argon2`](https://docs.rs/argon2) crate.
TENTH ecosystem-module proof on the ratified `.cb` ecosystem-import
chain (ADR-0072) and the FIRST ADR-0078 backend Phase-2 crate.

`fang` (cobra-themed name for the auth/security toolkit) is a **pure
value-pattern** module — like `nest` (TOML) and `scale` (msgpack): no
handles, no `AdtId`, no callbacks. It exposes two flat value-functions
a compiled `.cb` program binds onto when it does `import fang`:

- `fang.hash_password(pw: str) -> str` — argon2id PHC hash.
- `fang.verify_password(pw: str, hash: str) -> bool` — constant-time
  verify. The FIRST `-> bool` value-fn return on the ecosystem chain.

## Status

- **ADR-0078 backend Phase 2 — delivered.** Two C-ABI shims over
  `argon2` 0.5; argon2id PHC hash + constant-time verify. 4/4 E2E +
  5/5 in-crate cabi tests green.
- **Phase 2 (tuning surface) — proposed.** A per-deployment cost knob
  (`Argon2::new` with explicit `Params` — memory / time / parallelism
  for slow hardware or high-security tiers). Deliberately OUT of the
  first surface so the secure default cannot be weakened by accident
  (elegance law — no weak-params footgun).

## Public surface (Phase 1)

C-ABI symbols (`#[no_mangle] extern "C"`) declared in
`crates/cobrust-fang/src/cabi.rs`:

```text
__cobrust_fang_hash_password(pw: *mut Str) -> *mut Str
__cobrust_fang_verify_password(pw: *mut Str, hash: *mut Str) -> bool  // i1
```

Manifest entries (`crates/cobrust-types/src/ecosystem.rs`):

- `fang.hash_password(pw: str) -> str` (tier `semantic`) — argon2id PHC
  string (`$argon2id$v=…$m=…,t=…,p=…$<salt>$<hash>`) with a fresh
  random salt embedded.
- `fang.verify_password(pw: str, hash: str) -> bool` (tier `semantic`)
  — `true` iff `pw` produced `hash`; constant-time; a wrong / malformed
  hash is a clean `false`.

NO ADT slot is allocated — `fang` is value-pattern only (no handles).
The `0xE000_0800` block is NOT reserved (a future fang handle, if any,
takes the next free block).

## The `-> bool` value-fn return (first on the chain)

`verify_password` is the FIRST ecosystem value-fn returning `Ty::Bool`.
The chain carries it with NO new mechanism:

- `cobrust-mir` lowers via the existing `emit_ecosystem_call` with
  `sig.ret = Ty::Bool`, so the `_ecoret` destination local is a bool.
- `cobrust-codegen` declares the extern with an `i1` (`bool_type()`)
  return that stores into the i1 alloca; `write_place` →
  `coerce_value_to` bridges any i1/i8 width gap. Sibling of the
  existing `i64`-return externs (`strike.status_code`,
  `molt.unix_timestamp`).

## Chain-logic change: `str == str` / `str != str` natural operator

The fang E2E corpus asserts `h1 != h2` (two PHC strings differ by
salt). This is the FIRST `.cb` test exercising the NATURAL
string-equality OPERATOR on two `Ty::Str` LOCALS. It previously crashed
codegen — the `cobrust-codegen` `lower_binop` `Eq`/`NotEq` arms call
`into_int_value()`, but two str locals are `ptr` values ("Found
PointerValue but expected IntValue"). Only the explicit `str_eq(a, b)`
builtin had a lowering; the operator form had none.

Fix (`cobrust-mir/src/lower.rs` `lower_bin`, sibling of the Dict
`in`/`not in` arm): when `op ∈ {Eq, NotEq}` and the LHS resolves to
`Ty::Str`, retarget to the always-linked `__cobrust_str_eq(a, b) -> i64`
(0/1) then materialise the bool (`!= 0` for Eq, `== 0` for NotEq). Both
operands are BORROWED (Move→Copy upgrade — `__cobrust_str_eq` reads but
does not consume, so the source `str` locals survive for later uses and
drop ONCE at scope exit, per the `Str` non-Copy discipline).
String-LITERAL comparisons keep flowing through the existing
`str_eq_lit` PRELUDE path (the guard fires only when the LHS resolves
to a `Ty::Str` value). This is a GENERAL capability gap surfaced by the
corpus, not fang-specific plumbing.

## Security choices (elegance law — no auth footguns)

- **argon2id only, defaults baked in.** `hash_password` always uses
  `argon2::Argon2::default()` = argon2id with OWASP-recommended params.
  NO algorithm / cost knob in Phase 1 → a weak algo (argon2i/argon2d,
  unsalted digest) or weak params cannot be picked by accident.
- **Full PHC string out.** The returned `str` is self-describing — the
  random salt + parameters travel WITH the hash. No separate-salt
  management footgun.
- **Constant-time verify** (`argon2::Argon2::verify_password`) — no
  timing-attack footgun.
- **A wrong / malformed-hash password is a normal `false`**, NOT a
  panic / error across the boundary (CLAUDE.md §2.2: errors are not the
  default control path). No plaintext password is ever logged.

## Scope window (Phase 1)

In scope:

- argon2id hash + constant-time verify over `argon2` 0.5 defaults.
- Pure value pattern (str→str, (str,str)→bool); no handles, no
  callbacks — the `nest`/`scale` template.
- The empty-Str sentinel on the (effectively unreachable) hashing
  error, matching the std.json / F59 fail-clean convention.

Out of scope (Phase 2 follow-ups):

- A cost-parameter tuning surface (`Params` knob).
- A typed `Result[str, FangError]` surface (Phase 1 uses the empty-Str
  sentinel).
- Other primitives (HMAC, token signing, symmetric encryption) — each
  its own sub-ADR off this proven value chain.
- A PyO3 native module (the `cdylib` crate-type ships for it, but no
  `#[pymodule]` is wired in Phase 1).

## Invariants

- **No silent translations.** Every shim has a per-function doc comment
  citing its ADR-0078 origin + the argon2 API it wraps.
- **No panic across the C ABI.** `hash_password` returns the empty-Str
  sentinel on error; `verify_password` returns `false` on a malformed /
  empty hash. A wrong password is expected control flow, never an error.
- **Nondeterministic salt.** Two `hash_password` calls on the same
  password MUST differ (fresh `SaltString::generate(&mut OsRng)` per
  call) — verified by the in-crate + E2E tests.
- **No plaintext logging.** The wrapper never prints / logs the
  password or the hash.

## Gates (Phase 1 — none optional)

| Stage | Gate | Pass criteria | Status |
|---|---|---|---|
| L1 | typecheck manifest | `fang.*` resolve, no AmbiguousType | passes |
| L2.build | `cargo build -p cobrust-fang` | zero warnings | passes |
| L2.behavior | in-crate cabi tests | 5/5 — round-trip / wrong-pw / PHC-prefix / nondeterministic / malformed-is-false | passes |
| L3.e2e | compile + link + run | `cargo test -p cobrust-cli --test ecosystem_fang_e2e` 4/4 | passes |

## Done means (Phase 1 — DONE)

- [x] Workspace member `crates/cobrust-fang/` with crate-type rlib +
      cdylib + staticlib; `argon2 = "0.5"` + `password-hash` (getrandom).
- [x] 2 shims (`__cobrust_fang_hash_password` / `_verify_password`).
- [x] Manifest entries in `cobrust-types/src/ecosystem.rs`
      (`hash_password` str→str, `verify_password` str,str→bool) +
      `is_ecosystem_module("fang")`.
- [x] codegen extern declarations in
      `cobrust-codegen/src/llvm_backend.rs` (the `verify_password`
      extern declares an `i1` return).
- [x] Intrinsic prefix recognizer arm in
      `cobrust-cli/src/build/intrinsics.rs::ecosystem_module_for_symbol`
      (`__cobrust_fang_*` → `fang`).
- [x] `str == str` / `str != str` natural-operator lowering in
      `cobrust-mir/src/lower.rs` (general gap surfaced by the corpus).
- [x] E2E test `crates/cobrust-cli/tests/ecosystem_fang_e2e.rs` (4/4).

## Non-goals

- **Not** a re-implementation of argon2 in Cobrust — the chain is
  C-ABI shim FFI per ADR-0072 §3, wrapping the upstream `argon2` crate.
- **Not** a general crypto toolkit — Phase 1 is password hashing only;
  other primitives are separate sub-ADRs.
- **Not** an algorithm-selection surface — argon2id is THE default and
  the only option in Phase 1 (no weak-algo footgun).

## Cross-references

- `mod:types` — ecosystem manifest at `crates/cobrust-types/src/ecosystem.rs`.
- `mod:mir` — `try_lower_ecosystem_call` chain + the `str ==`/`!=`
  operator rewrite in `lower_bin`.
- `mod:codegen` — extern declarations (`i1`-return for verify).
- `mod:stdlib` — `__cobrust_str_*` primitives the cabi shims bind to
  (incl. `__cobrust_str_eq` for the operator rewrite).
- `mod:nest` — sister value-pattern module (TOML, second module).
- `mod:scale` — sister value-pattern module (msgpack, fourth module).
- [adr:0078](../adr/0078-backend-rust-crate-import-strategy.md) — backend crate-import strategy / Phase plan.
- [adr:0072](../adr/0072-cb-ecosystem-import-wiring.md) — L1→L5 chain.
- argon2 upstream — https://docs.rs/argon2.
