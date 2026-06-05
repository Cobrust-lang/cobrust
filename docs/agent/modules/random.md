---
doc_kind: module
module_id: mod:random
crate: none
last_verified_commit: TBD
dependencies: [mod:types, mod:codegen, mod:stdlib]
---

# Module: random (pseudo-random sampling stdlib surface)

## Purpose

`import random` — the pseudo-random sampling Python stdlib module wired
into Cobrust (per ADR-0086). Sampling / simulation / randomized-testing:
`random.random()`, `random.randint(a, b)`, `random.uniform(a, b)`,
`random.seed(n)`.

NOT a crate. There is no `cobrust-random`; `random` is a compiler
surface — a manifest in `cobrust-types` + four `__cobrust_random_*` shims
in `cobrust-stdlib/src/random.rs` (backed by a thread-local
`rand_pcg::Pcg64`) + the extern decls in `cobrust-codegen`.

The ONE novelty vs `math`/`re`: a **module-global RNG** (a process
`thread_local!` `RefCell<Pcg64>`), mirroring CPython's hidden module-level
`Random` instance — distinct from `coil.random`'s explicit `Generator`
HANDLE (ADR-0018). Everything else is scalar-in / scalar-out, the
SIMPLEST ecosystem-call shape — no new MIR arm, no new codegen fn-type.

## Status

- **ADR-0086 — delivered.** 4 scalar functions (random / randint /
  uniform / seed). 12 `cobrust-stdlib` shim unit tests + 6 `cobrust-types`
  manifest tests + 6 `.cb` e2e tests green.
- `choice` / `shuffle` / `sample` (list arg / list mutation), `randrange`,
  `gauss` and the non-uniform distributions are documented follow-ups
  (deferred until the list-arg ecosystem-call surface lands).

## Public surface — `lookup_module_fn("random", _)`

| `.cb` form | signature | runtime symbol | tier |
|---|---|---|---|
| `random.random()` | `[] -> Float` | `__cobrust_random_random` | Semantic |
| `random.randint(a, b)` | `[Int, Int] -> Int` | `__cobrust_random_randint` | Semantic |
| `random.uniform(a, b)` | `[Float, Float] -> Float` | `__cobrust_random_uniform` | Semantic |
| `random.seed(n)` | `[Int] -> Int` (sentinel) | `__cobrust_random_seed` | Semantic |

`is_ecosystem_module("random") == true`.

## Semantics

- `random.random()` — uniform float in **`[0, 1)`** (half-open: `0.0`
  attainable, `1.0` not). 0-arg — the FIRST 0-arg scalar stdlib fn.
- `random.randint(a, b)` — uniform int in **`[a, b]`, INCLUSIVE on BOTH
  ends**. `randint(5, 5) == 5`; `randint(1, 6)` can return 6 (CPython
  parity for the interval). Implemented with `gen_range(a..=b)` — the
  inclusive range is load-bearing; a half-open `a..b` would never yield
  `b` (the classic off-by-one biased die).
- `random.uniform(a, b)` — uniform float in **`[a, b]`** (closed both
  ends).
- `random.seed(n)` — re-seed the global RNG. The SAME `n` yields an
  IDENTICAL subsequent stream (reproducible). CPython returns `None`;
  Cobrust returns a discarded **i64 sentinel** (`0`) — the dora
  `event.send_output` discard pattern. The `.cb` form is
  `let _ = random.seed(n)`.

### The global RNG (the design crux)

A process `thread_local! { static RNG: RefCell<rand_pcg::Pcg64> }` in
`cobrust-stdlib/src/random.rs`:

- **OS-seeded lazily** on first borrow via `Pcg64::from_entropy()`
  (getrandom) — so an un-seeded program is non-deterministic (the
  feature).
- **Re-seeded** by `random.seed(n)`:
  `*cell.borrow_mut() = Pcg64::seed_from_u64(n as u64)`.
- Each shim borrows ONCE per call:
  `RNG.with(|cell| cell.borrow_mut().<draw>())` — single-threaded per
  call (one `borrow_mut`, released before return), so no double-borrow
  and no cross-thread lock. `thread_local` (not `Mutex`) honors §2.2's
  no-GIL / no-global-lock posture.

### Determinism contract (§5.2)

- Seed → **reproducible**: `seed(k); x; seed(k); y` ⇒ `x == y`, every
  time, on every host (`Pcg64`'s transition function is algebraic with no
  host-endianness state — same seed, same bytes cross-platform).
- No seed → OS-entropy (non-deterministic).
- A RAW draw is therefore NOT assertable; the seed-reproducibility
  EQUALITY is (the load-bearing `.cb` e2e + unit test).

### `@py_compat` tier: Semantic (the NOT-bit-identical divergence)

CPython's `random` uses the **Mersenne Twister** (MT19937); Cobrust uses
**`Pcg64`**. Same seed → DIFFERENT streams; Cobrust does NOT reproduce
CPython's exact values. The CONTRACT is the **distribution** (uniform on
the stated interval) + **Cobrust-internal seed-reproducibility**, NOT
bit-identical CPython agreement. Same honest posture `coil.random` takes
vs numpy (ADR-0018). Tier `Semantic`.

## ABI

The four `__cobrust_random_*` symbols are scalar-in / scalar-out (no
Str/list buffer marshalling, unlike `re`):

| symbol | C-ABI type |
|---|---|
| `__cobrust_random_random` | `() -> f64` (LLVM `double`) |
| `__cobrust_random_randint` | `(i64, i64) -> i64` |
| `__cobrust_random_uniform` | `(f64, f64) -> f64` |
| `__cobrust_random_seed` | `(i64) -> i64` (discarded sentinel) |

The generic ecosystem-call path (`cobrust-mir` `try_lower_ecosystem_call`
Case 1 → `emit_ecosystem_call`) drives args + return off the `EcoSig`;
the destination `_ecoret` local carries the manifest return type. Codegen
only declares the externs (the stdlib staticlib is always linked, so no
runtime-link recognizer is needed). 0-arg + scalar-arg + the seed
i64-sentinel side effect all lower through the EXISTING path — `'none'`
new MIR code.

## `seed` return-type rationale

CPython `random.seed` returns `None`. Cobrust types it `Ty::Int` (an i64
sentinel `0`), NOT `Ty::None`, because `Ty::None` lowers to `i64` in
codegen (`lower_ty`) while a Rust shim returning `()` would be an ABI
mismatch. The i64 sentinel is the scalar analogue of the `pit` `route` /
`use_cors` `Ty::None`-discard (which return `*mut u8 = null`) and is
exactly the dora `event.send_output` pattern. The caller discards it:
`let _ = random.seed(n)`.

## Deferred

- `choice(seq)` / `shuffle(seq)` / `sample(seq, k)` — list arg crossing
  the C-ABI; `shuffle` mutates in place (`&mut list` marshalling). The
  manifest returns `None`, so a `.cb` `random.shuffle(xs)` is a
  compile-time `UnknownName` (§2.5), not a false-green binding.
- `randrange`, `gauss`, `betavariate`, … — non-uniform / step forms.
- A compile-time `a > b` check for literal `randint(a, b)` (today: a clean
  runtime panic inside `gen_range`).

## Tests

- `crates/cobrust-stdlib/src/random.rs` `#[cfg(test)]` — 12 tests: seed
  reproducibility (single draw + full sequence + distinct-seeds-diverge +
  randint/uniform too), `random()` in `[0,1)` over 10k draws, randint
  INCLUSIVE both ends (`randint(5,5)==5`; both 0 and 1 appear over
  `randint(0,1)` draws; in-range over 10k), `uniform` bounds (positive +
  negative range), the shim delegation + sentinel.
- `crates/cobrust-cli/tests/random_e2e.rs` — 6 compile→link→spawn e2es.
- `crates/cobrust-types/src/ecosystem.rs` `#[cfg(test)]` — 6 manifest
  tests (each row's symbol + types + Semantic tier; `random` is a known
  module; the deferred fns resolve to `None`).

## See also

- `docs/agent/adr/0086-random-prng-stdlib-module.md` — the design ADR.
- `docs/human/en/import-random.md` / `docs/human/zh/import-random.md` —
  human-facing usage.
- `docs/agent/modules/stdlib.md` — the host crate (`cobrust-stdlib`).
- ADR-0018 (`coil.random` `Generator`), ADR-0083 (`math`), ADR-0084
  (`re`).
