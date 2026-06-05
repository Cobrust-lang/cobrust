//! `import random` (pseudo-random sampling) — `.cb` end-to-end proof for
//! the ADR-0086 addition: the thread-local `rand_pcg::Pcg64` module-global
//! RNG. UNIVERSAL — sampling / simulation / randomized-testing is one of
//! the most-reached-for Python capabilities, and the translation pipeline
//! needs it. These tests compile to REAL binaries, link, spawn, and assert
//! stdout / exit code, proving the scalar f64 / i64 returns + the seed
//! side effect are usable END-TO-END.
//!
//! ## The four functions (all scalar, NO list arg / mutation)
//!
//! - `random.random() -> f64` — a uniform float in `[0, 1)` (0-arg; the
//!   FIRST 0-arg scalar stdlib fn, mirroring `math.sqrt`'s f64 return).
//! - `random.randint(a, b) -> int` — INCLUSIVE `[a, b]` (CPython
//!   `randint(1, 6)` can return 6; `randint(5, 5)` is always 5).
//! - `random.uniform(a, b) -> f64` — a uniform float in `[a, b]`.
//! - `random.seed(n)` — re-seed; the SAME seed yields an IDENTICAL
//!   subsequent stream (reproducible). Returns a discarded i64 sentinel
//!   (CPython returns None; the dora `event.send_output` discard pattern).
//!
//! ## The load-bearing semantics
//!
//! - SEED REPRODUCIBILITY is the load-bearing, ASSERTABLE property: a raw
//!   draw is non-deterministic (OS-entropy un-seeded), so the e2e asserts
//!   `seed(42); a = random(); seed(42); b = random(); a == b`. This is the
//!   whole point of `random.seed` (constitution §5.2) and is the only
//!   thing one CAN assert about an RNG's exact output.
//! - `randint(5, 5)` is ALWAYS 5 (inclusive single point); `randint(1, 6)`
//!   stays in `[1, 6]` inclusive — a half-open `[a, b)` bug would NEVER
//!   reach the upper end (the classic off-by-one biased die).
//! - `random()` lands in `[0, 1)` — asserted via a range check (the value
//!   itself is not assertable).
//!
//! ## @py_compat tier: Semantic (a documented divergence)
//!
//! CPython's `random` uses the Mersenne Twister (MT19937); Cobrust uses
//! `Pcg64`. The two produce DIFFERENT streams for the same seed — Cobrust
//! does NOT reproduce CPython's exact values. The CONTRACT is the
//! DISTRIBUTION + Cobrust-internal seed-reproducibility, NOT bit-identical
//! agreement with CPython (ADR-0086 §"Divergence"; mirrors `coil.random`'s
//! honest "distribution-level, not bit-identical vs numpy" posture).
//!
//! Mirrors the compile->spawn->assert-stdout harness of `re_e2e` /
//! `math_e2e`.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative doc comments read as "lazy" list items to clippy; they
// are intentional explanatory prose, not lint targets. (This `#![allow]` is
// the lesson math-part2 / re learned the hard way — the doc-lint on the e2e
// header only surfaces in `-p cobrust-cli --all-targets` clippy.)
#![allow(clippy::doc_lazy_continuation)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; panic with stderr on build
/// failure. Mirrors `re_e2e::compile_source`.
fn compile_source(source: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let exe = dir.path().join("prog");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let build = Command::new(&bin)
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&exe)
        .arg("--quiet")
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "build failed: {}\nstderr: {}",
        build.status,
        String::from_utf8_lossy(&build.stderr)
    );
    (dir, exe)
}

/// Spawn a compiled program; return `(stdout, stderr, success)`.
fn run(exe: &PathBuf) -> (String, String, bool) {
    let out = Command::new(exe).output().expect("spawn random prog");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

// =====================================================================
// THE LOAD-BEARING TEST — seed reproducibility. `seed(42); a = random();
// seed(42); b = random()` => a == b. A raw draw is non-deterministic, so
// this equality is the ONLY assertable property of the RNG's exact output
// and is the entire reason `random.seed` exists (constitution §5.2). A
// broken `seed` (a no-op, or a fresh-entropy reseed) would print
// `DIVERGED` instead.
// =====================================================================

/// `seed(42); a = random(); seed(42); b = random()` re-seeds with the
/// SAME value and draws again; `a == b` proves the stream is reproducible.
/// THE load-bearing determinism proof, end-to-end through codegen.
#[test]
fn test_e2e_seed_makes_random_reproducible() {
    let source = concat!(
        "import random\n",
        "\n",
        "fn main() -> i64:\n",
        "    let _ = random.seed(42)\n",
        "    let a: f64 = random.random()\n",
        "    let _ = random.seed(42)\n",
        "    let b: f64 = random.random()\n",
        "    if a == b:\n",
        "        print(\"reproducible\")\n",
        "    else:\n",
        "        print(\"DIVERGED\")\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "reproducible",
        "same seed MUST reproduce the same random() draw (a broken seed \
         prints DIVERGED); got stdout=\n{stdout}",
    );
}

/// A SECOND seeded draw after the FIRST also matches under re-seed —
/// proving the whole stream (not just draw #1) is reproducible. Draws two
/// values per seed and compares both pairs.
#[test]
fn test_e2e_seed_reproduces_a_sequence() {
    let source = concat!(
        "import random\n",
        "\n",
        "fn main() -> i64:\n",
        "    let _ = random.seed(7)\n",
        "    let a1: f64 = random.random()\n",
        "    let a2: f64 = random.random()\n",
        "    let _ = random.seed(7)\n",
        "    let b1: f64 = random.random()\n",
        "    let b2: f64 = random.random()\n",
        "    if a1 == b1:\n",
        "        if a2 == b2:\n",
        "            print(\"seq-reproducible\")\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "seq-reproducible",
        "the FULL seeded stream (both draws) must reproduce; got stdout=\n{stdout}",
    );
}

// =====================================================================
// `random()` is in [0, 1) — a range check (the raw value is not
// assertable, but its membership in the unit interval IS).
// =====================================================================

/// `random()` lands in `[0, 1)`: the nested `if x >= 0.0` / `if x < 1.0`
/// branches print `unit-ok` only when BOTH hold. Seeded so the build is
/// deterministic, but the assertion is the range invariant, not the value.
#[test]
fn test_e2e_random_in_unit_interval() {
    let source = concat!(
        "import random\n",
        "\n",
        "fn main() -> i64:\n",
        "    let _ = random.seed(123)\n",
        "    let x: f64 = random.random()\n",
        "    if x >= 0.0:\n",
        "        if x < 1.0:\n",
        "            print(\"unit-ok\")\n",
        "        else:\n",
        "            print(\"TOO-BIG\")\n",
        "    else:\n",
        "        print(\"NEGATIVE\")\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "unit-ok",
        "random() must be in [0, 1); got stdout=\n{stdout}",
    );
}

// =====================================================================
// `randint` INCLUSIVE on BOTH ends. `randint(5, 5)` is ALWAYS 5 (the
// single-point case proves inclusivity directly: a half-open [5, 5) is
// EMPTY and would panic / diverge). A `for`-loop draws randint(1, 6)
// repeatedly and asserts every draw is in [1, 6].
// =====================================================================

/// `randint(5, 5)` is ALWAYS 5 — printed directly. The single-point case
/// is the cleanest inclusivity proof: `[5, 5]` (inclusive) contains 5,
/// whereas the half-open `[5, 5)` is empty.
#[test]
fn test_e2e_randint_single_point_is_that_point() {
    let source = concat!(
        "import random\n",
        "\n",
        "fn main() -> i64:\n",
        "    let _ = random.seed(0)\n",
        "    let d: i64 = random.randint(5, 5)\n",
        "    print(d)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "5",
        "randint(5, 5) must always be 5 (inclusive single point); got stdout=\n{stdout}",
    );
}

/// `randint(1, 6)` drawn 200 times in a `while` loop stays in `[1, 6]`
/// EVERY time (the die-roll invariant). Prints `die-in-range` iff no draw
/// ever escaped — proving the INCLUSIVE upper bound 6 is respected and no
/// value falls below 1. (A `[a, b)` bug could still pass this, which is
/// why the single-point test above is the primary inclusivity proof; this
/// adds the range-stability guarantee over many draws.)
#[test]
fn test_e2e_randint_stays_in_inclusive_range() {
    let source = concat!(
        "import random\n",
        "\n",
        "fn main() -> i64:\n",
        "    let _ = random.seed(2024)\n",
        "    let i: i64 = 0\n",
        "    let bad: i64 = 0\n",
        "    while i < 200:\n",
        "        let r: i64 = random.randint(1, 6)\n",
        "        if r < 1:\n",
        "            bad = bad + 1\n",
        "        if r > 6:\n",
        "            bad = bad + 1\n",
        "        i = i + 1\n",
        "    if bad == 0:\n",
        "        print(\"die-in-range\")\n",
        "    else:\n",
        "        print(\"OUT-OF-RANGE\")\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "die-in-range",
        "every randint(1, 6) draw must be in [1, 6] inclusive; got stdout=\n{stdout}",
    );
}

// =====================================================================
// `uniform(a, b)` is in [a, b]. A negative-spanning range exercises the
// general case.
// =====================================================================

/// `uniform(-5.0, -1.0)` lands in `[-5, -1]`: the nested branches print
/// `uniform-ok` only when `-5.0 <= x <= -1.0`. Proves the f64 two-arg
/// scalar return flows and the bounds hold for a negative range.
#[test]
fn test_e2e_uniform_in_negative_range() {
    let source = concat!(
        "import random\n",
        "\n",
        "fn main() -> i64:\n",
        "    let _ = random.seed(314)\n",
        "    let x: f64 = random.uniform(-5.0, -1.0)\n",
        "    if x >= -5.0:\n",
        "        if x <= -1.0:\n",
        "            print(\"uniform-ok\")\n",
        "        else:\n",
        "            print(\"TOO-BIG\")\n",
        "    else:\n",
        "        print(\"TOO-SMALL\")\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "uniform-ok",
        "uniform(-5, -1) must be in [-5, -1]; got stdout=\n{stdout}",
    );
}
