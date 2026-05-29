# `import fang` — hash and verify passwords from Cobrust

> Status: ADR-0078 backend Phase 2, the FIRST backend Phase-2 crate.
> After the nine cobra-batch modules (den / nest / strike / scale /
> molt / pit / hood / coil / dora), `fang` (the auth/security toolkit,
> a thin safe wrapper over the `argon2` crate) is the TENTH module
> wired through the same ecosystem-import chain — a pure value-pattern
> module like `nest`/`scale`, and the FIRST one with a `-> bool`
> value-function return.

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

That's the whole surface for now: the smallest-useful auth round trip
(hash a password, verify it later), exercising the chain top-to-bottom
for the first security module.

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
- The error path on `hash_password` (effectively unreachable) is the
  empty-string sentinel; a typed `Result[str, FangError]` surface is a
  tracked follow-up.

These are tracked follow-ups, not dead ends — the wiring generalizes
to the rest of the security surface from here.
