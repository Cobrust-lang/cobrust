---
doc_kind: adr
adr_id: 0086
title: random — pseudo-random sampling stdlib module (import random) via a thread-local rand_pcg::Pcg64 module-global RNG + cobrust-stdlib shims
status: accepted
date: 2026-06-05
last_verified_commit: 731ee7d
supersedes: []
superseded_by: []
---

# ADR-0086: `random` — the pseudo-random sampling stdlib module (`import random`)

## Context

Sampling / simulation / randomized-testing is universal in Python, and the
L0–L3 translation pipeline needs it. `random` is the **3rd core stdlib
module** after `math` (ADR-0083) and `re` (ADR-0084), wired through the
SAME generic ecosystem-call path.

`random` introduces ONE thing with no precedent in Cobrust: a **module-global
RNG**. Python's module-level `random.random()` / `random.randint()` /
`random.seed()` all share ONE hidden, process-global `Random` instance. This
is **distinct** from `coil.random`'s explicit `Generator` HANDLE (ADR-0018):
there, the user holds `rng = coil.default_rng(seed)` and calls
`rng.random(...)`; here, there is no handle — the state is ambient.

Everything else is already-shipped:

- The four functions are **scalar-in / scalar-out** — the SIMPLEST
  ecosystem-call shape (no Str/list buffer marshalling, unlike `re`; no
  Buffer, unlike `coil`).
- The PRNG backend (`rand_pcg::Pcg64`) is ALREADY a workspace dependency via
  `cobrust-coil`'s `coil.random` (ADR-0018) — reused verbatim.

### Scope (the scalar core — 4 functions)

| `.cb` call | Signature | Semantics |
|---|---|---|
| `random.random()` | `[] -> f64` | uniform float in `[0, 1)` (0-arg) |
| `random.randint(a, b)` | `[Int, Int] -> Int` | uniform int in `[a, b]`, INCLUSIVE both ends |
| `random.uniform(a, b)` | `[Float, Float] -> Float` | uniform float in `[a, b]` |
| `random.seed(n)` | `[Int] -> Int` (sentinel) | re-seed; reproducible stream |

### Deferred (NOT in this cut)

`choice(seq)`, `shuffle(seq)`, `sample(seq, k)`, `randrange`, `gauss`,
`betavariate`, … — every list-taking / list-mutating / non-uniform form.
`choice` / `shuffle` / `sample` need a `list` argument crossing the C-ABI
(and `shuffle` mutates it in place — an `&mut list` marshalling surface). The
manifest returns `None` for these, so a `.cb` `random.shuffle(xs)` is a
compile-time `UnknownName` (§2.5 compile-time-catch), NOT a false-green
binding. They are a tracked follow-up once the list-arg ecosystem-call
surface lands.

## Options considered

### Q1 — the global-RNG cell

1. **`thread_local! { static RNG: RefCell<Pcg64> }`** (CHOSEN). One PRNG per
   thread, OS-seeded on first borrow (`Pcg64::from_entropy()`), re-seeded in
   place by `seed`. Each shim does `RNG.with(|cell| cell.borrow_mut().<draw>())`
   — single-threaded per call (one `borrow_mut`, released before return), so
   no double-borrow and no cross-thread lock.
2. **`static RNG: Mutex<Pcg64>`** (rejected). A process-wide lock on every
   draw — needless contention, and it fights constitution §2.2's no-GIL /
   no-global-lock posture. Cobrust's codegen-emitted `main` + callbacks run
   the shims on the calling thread; a `thread_local` is the contention-free
   fit.
3. **`static RNG: OnceCell<…>` + atomics** (rejected). A lock-free PRNG needs
   either an atomic-state generator (not `Pcg64`) or a CAS loop — complexity
   with no benefit for the single-threaded-per-call reality.

`RefCell` (not `Cell`) because `Pcg64` is not `Copy` and a draw needs `&mut`;
the borrow is taken and dropped entirely inside the closure, so re-entrancy
is impossible (a shim never calls back into another shim while holding it).

### Q2 — what does `seed` return?

CPython `random.seed(n)` returns `None`.

1. **`Ty::Int` i64 sentinel, discarded** (CHOSEN). The shim returns `0`; the
   `.cb` caller discards it (`let _ = random.seed(n)` or a bare expression
   statement). This is the EXACT dora `event.send_output` pattern (ADR-0076,
   `ecosystem.rs` `send_output` returns `Ty::Int` "a 0 sentinel for the
   `let _ = event.send_output(...)` discard").
2. **`Ty::None`** (rejected). `Ty::None` lowers to `i64` in codegen
   (`llvm_backend.rs` `lower_ty`: `Ty::None => i64_type`), so the `_ecoret`
   destination is i64 — but a Rust shim returning `()` (void) declared as an
   i64-returning extern is an ABI mismatch. The existing `Ty::None` eco-returns
   (`pit` `route` / `use_cors`) sidestep this by returning a `*mut u8 = null`
   POINTER, not void — i.e. they ALSO return a real value, just a discarded
   pointer. The i64 sentinel is the cleaner scalar analogue and is already
   proven by `send_output`.

### Q3 — backend & cross-host determinism

`rand_pcg::Pcg64` (the SAME generator `coil.random` pins, ADR-0018 §2). PCG64's
transition function is algebraic with NO host-endianness state, so **the same
seed yields the same byte stream on every host architecture** — the
reproducibility contract holds cross-platform, not just within one binary.

## Decision

Add `import random` as a `cobrust-stdlib` module (`src/random.rs`) exposing a
process-`thread_local!` `RefCell<rand_pcg::Pcg64>`, OS-seeded lazily via
`Pcg64::from_entropy()` and re-seeded by `random.seed(n)`
(`*cell.borrow_mut() = Pcg64::seed_from_u64(n as u64)`). Four C-ABI shims
(`__cobrust_random_{random,randint,uniform,seed}`) are the cabi; each borrows
the cell once. The four manifest rows in `cobrust-types::ecosystem` drive the
generic ecosystem-call path (NO new MIR arm); codegen declares the four
externs (scalar `f64`/`i64` fn-types). `random.randint` uses
`gen_range(a..=b)` — INCLUSIVE on BOTH ends. `random.seed` returns a discarded
i64 sentinel (CPython `None`).

### The seed-reproducibility contract (constitution §5.2)

The seed makes the stream **reproducible**: `seed(k); x; seed(k); y` gives
`x == y`, every time, on every host. This is the entire point of `random.seed`
and the ONLY assertable property of an RNG's exact output (a raw draw is
non-deterministic — OS-entropy seeded when un-seeded, which is the FEATURE).
The `.cb` e2e + the stdlib unit tests both pin this equality as the
load-bearing test.

### `@py_compat` tier: Semantic — the NOT-bit-identical divergence

CPython's `random` uses the **Mersenne Twister** (MT19937); Cobrust uses
**`Pcg64`**. For the same seed the two produce **DIFFERENT streams** — Cobrust
does NOT, and does not try to, reproduce CPython's exact values. The contract
is:

- **Distribution** — `random()` is uniform on `[0, 1)`, `randint(a, b)` is
  uniform on the inclusive integers `[a, b]`, `uniform(a, b)` is uniform on
  `[a, b]`.
- **Cobrust-internal seed-reproducibility** — same seed → identical stream,
  cross-host.

NOT — bit-identical agreement with CPython's MT19937 output. This is the SAME
honest posture `coil.random` takes vs numpy ("distribution-level, not
bit-identical vs numpy" — `coil/src/random.rs` §doc; numpy ALSO uses PCG64 but
a different `SeedSequence` layout). Tier `Semantic` records the divergence
explicitly rather than implying a Strict parity that does not hold.

## Consequences

- **Positive**
  - 3rd core stdlib module ships the universal sampling surface; the
    translation pipeline gains `random`.
  - Reuses `coil.random`'s `rand_pcg::Pcg64` backend (no new crate download —
    `rand` / `rand_pcg` / transitive `getrandom` are already in the lock); the
    new deps are EDGES only.
  - The FIRST 0-arg scalar stdlib fn (`random.random()`) proves the generic
    ecosystem-call path lowers a no-argument f64-returning call end-to-end.
  - Seed-reproducibility is deterministic + assertable (the load-bearing e2e),
    satisfying §5.2 despite the values being impl-defined.
  - No new MIR arm, no new codegen fn-type mechanism (scalar fn-types reuse
    the `math` shapes), no new runtime-link recognizer (the stdlib staticlib
    is always linked).

- **Negative**
  - Streams are NOT CPython-bit-identical (Pcg64 vs MT19937) — a `random`
    translation cannot be differential-tested against CPython for exact
    output, only for distribution + Cobrust-internal reproducibility. Recorded
    as the `Semantic` tier.
  - `choice` / `shuffle` / `sample` (the list forms) are deferred — the
    common "pick a random element / shuffle a deck" idiom is not yet covered.
  - A `thread_local` RNG means a multi-threaded `.cb` program (M13 `task`)
    gets a PER-THREAD stream; `random.seed(n)` on one thread does not reseed
    another. This matches CPython's actual behavior is NOT identical (CPython's
    module RNG is process-global under the GIL); for Cobrust's no-GIL model
    per-thread is the correct, lock-free choice, and a single-threaded program
    (the overwhelming common case) is unaffected. Documented, not a bug.

- **Neutral / unknown**
  - `randint(a, b)` with `a > b` panics inside `gen_range` (a clean Rust
    abort, never a silent wrong value or UB across the C-ABI) — CPython raises
    `ValueError`. A compile-time check for literal `a > b` is a possible §2.5
    follow-up.

## Evidence

- `crates/cobrust-stdlib/src/random.rs` — the `thread_local` `Pcg64` + the
  four shims + 12 unit tests (seed reproducibility incl. full-sequence,
  randint inclusive-both-ends incl. the `[0,1]` both-endpoints-appear test,
  range checks for `random`/`uniform`).
- `crates/cobrust-cli/tests/random_e2e.rs` — 6 compile→link→spawn e2es: seed
  reproducibility (single draw + sequence), `random()` in `[0,1)`,
  `randint(5,5)==5`, `randint(1,6)` range-stability over 200 draws, `uniform`
  negative-range bounds.
- `crates/cobrust-types/src/ecosystem.rs` — the four `("random", _)` manifest
  rows + `is_ecosystem_module("random")` + 6 manifest unit tests.
- `crates/cobrust-codegen/src/llvm_backend.rs` — the four extern declarations.
- Prior art: ADR-0018 (`coil.random` `Generator` + the PCG64 / not-bit-
  identical-vs-numpy decision), ADR-0083 (`math` scalar stdlib precedent),
  ADR-0084 (`re` ecosystem-call wiring precedent), ADR-0076 (dora
  `send_output` i64-sentinel-discard precedent).
