# `import fang` — hash passwords and sign JSON Web Tokens from Cobrust

> Status: ADR-0078 backend Phase 2, the FIRST backend Phase-2 crate.
> After the nine cobra-batch modules (den / nest / strike / scale /
> molt / pit / hood / coil / dora), `fang` (the auth/security toolkit,
> a thin safe wrapper over the `argon2` and `jsonwebtoken` crates) is
> the TENTH module wired through the same ecosystem-import chain — a
> pure value-pattern module like `nest`/`scale`, and the FIRST one with
> a `-> bool` value-function return. It now offers two surfaces:
> **password hashing** (argon2id) and **JSON Web Tokens** (HS256).

## Example first

```python
import fang

fn main() -> i64:
    let h: str = fang.hash_password("hunter2")
    let ok: bool = fang.verify_password("hunter2", h)
    if ok:
        print(1)
    else:
        print(0)
    return 0
```

Build and run it:

```bash
cobrust build prog.cb -o prog
./prog
# 1
```

## What you get (Phase-2 first surface)

- **`fang.hash_password(pw: str) -> str`** — hash `pw` with **argon2id**
  (a fresh random salt each call) and return the full
  [PHC string](https://github.com/P-H-C/phc-string-format) — the
  `$argon2id$v=…$m=…,t=…,p=…$<salt>$<hash>` format, with the salt and
  cost parameters embedded. Store this whole string; there is nothing
  else to keep.
- **`fang.verify_password(pw: str, hash: str) -> bool`** — return
  `true` iff `pw` is the password that produced `hash`. The comparison
  is constant-time. A wrong password is a normal `false` — never an
  error you have to catch.

## JSON Web Tokens (HS256)

The second `fang` surface signs and verifies **JSON Web Tokens** — the
standard way to carry a small, signed set of claims (who the user is,
when the token expires) between your services.

```python
import fang

fn main() -> i64:
    let token: str = fang.jwt_encode("{\"sub\":\"alice\"}", "s3cret")
    let ok: bool = fang.jwt_verify(token, "s3cret")
    if ok:
        print(1)
    else:
        print(0)
    return 0
```

```bash
cobrust build prog.cb -o prog
./prog
# 1
```

- **`fang.jwt_encode(claims_json: str, secret: str) -> str`** — sign the
  JSON claims object in `claims_json` with `secret` using **HS256** and
  return the compact `header.payload.signature` token. If `claims_json`
  is not valid JSON you get the empty string back (never a crash).
- **`fang.jwt_verify(token: str, secret: str) -> bool`** — `true` iff
  `token` is a genuine HS256 token signed with `secret`. A tampered,
  wrong-secret, malformed, or `alg:none` token is a clean `false`.
- **`fang.jwt_decode(token: str, secret: str) -> str`** — verify `token`
  and, if it is genuine, return its claims JSON; otherwise return the
  empty string. A decode **never** hands you the claims of an unverified
  token.

```python
let claims: str = fang.jwt_decode(token, "s3cret")
# claims == "{\"sub\":\"alice\"}" (re-serialised; key order may differ)
# for a forged / tampered token, claims == "" (the empty sentinel)
```

That, plus the password round trip above, is the whole surface for now:
hash a password, and sign/verify a token — exercising the chain
top-to-bottom for the first security module.

## A real login check

```python
import fang

fn check_login(stored_hash: str, attempt: str) -> bool:
    return fang.verify_password(attempt, stored_hash)
```

`stored_hash` is whatever `fang.hash_password` returned when the user
set their password (you save the full string in your user table).

## Why this design? (no auth footguns)

Cobrust's ecosystem surface deliberately drops the traps that other
languages' auth libraries carry. `fang` is a clean re-design, not a
mechanical clone:

- **argon2id is the only algorithm, with safe defaults baked in.**
  `fang.hash_password` always uses argon2id (the OWASP-recommended
  password hash) with sound default parameters. Phase 1 exposes **no**
  algorithm or cost-parameter knob — so you cannot accidentally pick a
  weak hash (plain `argon2i`/`argon2d`, an unsalted SHA, a low work
  factor). The secure choice is the only choice.
- **The salt lives inside the hash.** The returned PHC string carries
  its own random salt and parameters. There is no separate salt to
  generate, store, or accidentally reuse — one of the most common
  password-storage bugs simply cannot happen here.
- **Verification is constant-time.** `fang.verify_password` uses
  argon2's constant-time comparison, so a timing side-channel cannot
  leak how much of a guess was correct.
- **A wrong password is a value, not an exception.** Verification
  returns `bool`. A mismatch is ordinary control flow (`false`), in
  keeping with Cobrust's rule that errors are not the default control
  path. Your code never wraps a login check in exception handling.
- **No plaintext is ever logged.** The wrapper never prints or logs the
  password or the hash.

### And for JWTs: the algorithm is pinned (the classic footgun, closed)

JSON Web Tokens have one infamous trap. A token carries its OWN
"algorithm" field in its header, and a naive verifier *trusts* it. Two
attacks follow:

- **`alg:none`** — an attacker sends a token whose header says
  `{"alg":"none"}` and leaves the signature empty. A verifier that obeys
  the header skips the signature check entirely and accepts *any* claims
  the attacker writes (admin, anyone). This is the CVE-2015-9235 family.
- **algorithm swap** — an attacker takes a service that verifies RS256
  (public-key) tokens and sends an HS256 token, tricking the verifier
  into using the *public* key as the HMAC *secret*.

`fang.jwt_verify` / `fang.jwt_decode` **pin the algorithm to HS256** and
never look at the token's own `alg` field to choose how to verify. An
`alg:none` token, an RS256-header token, a tampered payload, or the
wrong secret all come back as a clean `false` (or the empty string for
decode) — there is no API on this surface to disable signature checking,
so the footgun cannot be triggered even by accident. And because
`fang.jwt_encode` only ever emits HS256, you cannot mint a weak token
either.

## What happens with a bad hash string?

`fang.verify_password` **never panics**. If the `hash` argument is empty
or not a valid PHC string, verification simply returns `false` (it
cannot match). So the idiomatic check is just:

```python
let ok: bool = fang.verify_password(attempt, stored_hash)
if ok:
    print("welcome")
else:
    print("nope")
```

This matches Cobrust's runtime convention for the value-pattern
shims — fail cleanly, never panic across the boundary.

## Why two hashes of the same password differ

Run `fang.hash_password("x")` twice and you get two **different**
strings — each call draws a fresh random salt. Both still verify TRUE
against `"x"`. This is the whole point of salting: identical passwords
must not produce identical stored hashes, so a leaked database does not
reveal which users share a password.

```python
let h1: str = fang.hash_password("x")
let h2: str = fang.hash_password("x")
# h1 != h2 (different salts), yet both verify_password("x", …) == true
```

## Today's limits

- Wrap your code in `fn main() -> i64:` (bare top-level statements are
  a separate, not-yet-finished part of the toolchain).
- Phase 1 exposes argon2id with default parameters only. A tuning
  surface (per-deployment memory / time / parallelism cost, for slow
  hardware or high-security tiers) is a tracked follow-up — kept out of
  the first surface so the default cannot be weakened by accident.
- JWTs are **HS256** (a shared secret) only. Asymmetric algorithms
  (RS256 / ES256) are a tracked follow-up — they would be *added* to the
  pinned algorithm list, never replace the pin.
- The JWT verifier checks the **signature** only; it does not yet
  enforce an expiry (`exp`) claim, so a bare `{"sub":"alice"}` token
  round-trips. An expiry-policy surface is a tracked follow-up.
- The error path on `hash_password` / `jwt_encode` / `jwt_decode` (an
  invalid input) is the empty-string sentinel; a typed
  `Result[str, FangError]` surface is a tracked follow-up.

These are tracked follow-ups, not dead ends — the wiring generalizes
to the rest of the security surface from here.
