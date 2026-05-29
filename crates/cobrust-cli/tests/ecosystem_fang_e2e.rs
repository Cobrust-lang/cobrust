//! ADR-0078 Phase-1b proof — end-to-end `.cb` source → compile → link →
//! run for the `fang` ecosystem-import wiring (argon2, the auth/security
//! toolkit; CTO-chosen cobra-themed name).
//!
//! Twin of `ecosystem_nest_e2e.rs` / `ecosystem_scale_e2e.rs` (the
//! value-pattern proofs). Confirms the SAME chain the
//! `den`/`nest`/`strike`/`scale`/`molt` proofs exercise generalizes to a
//! FLAT VALUE-FUNCTION security module — `fang.hash_password(str) -> str`
//! and `fang.verify_password(str, str) -> bool`. No handles, no
//! callbacks; ADR-0078 §3 rates argon2 FLAT (str-in/str-out, sync,
//! CPU-bound), the den/strike template exactly. This adds the FIRST
//! `-> bool` value-fn return on the chain (prior value-fns are `str ->
//! str`); the manifest row the impl must add is
//! `("fang","verify_password") => EcoSig::from_values(sym, vec![Ty::Str,
//! Ty::Str], Ty::Bool, …)`.
//!
//! ```text
//! `import fang` + `fang.hash_password(pw) -> str` + `fang.verify_password(pw, hash) -> bool`
//!   → cobrust-types ecosystem manifest (typecheck, no AmbiguousType)
//!   → cobrust-mir lowering (retarget → __cobrust_fang_*)
//!   → cobrust-codegen extern + existing Str drop schedule
//!   → cobrust-fang C-ABI shim (libfang.a) wrapping argon2 (argon2id PHC)
//!   → cobrust-cli build.rs per-import static link (after libcobrust_stdlib.a)
//!   → stdout
//! ```
//!
//! Robustness pattern (coil/pit E2E precedent + `method_call_e2e.rs:215`):
//! a `bool` value is surfaced to stdout via the explicit
//! `if b:\n    print(1)\nelse:\n    print(0)` idiom (the canonical
//! Cobrust bool-print form — `print(bool)` directly is avoided), so the
//! oracle is a stable `"1\n"` / `"0\n"`. The security cases assert the
//! exact bit (TRUE for the right pw, FALSE for a wrong pw).

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::process::Command;

/// Compile + link + run a `.cb` source, returning its stdout. Asserts
/// the build and the run both succeed.
fn build_and_run_source(source: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let exe = dir.path().join("prog");
    let bin = std::path::PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
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
        "build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(&exe).current_dir(dir.path()).output().unwrap();
    assert!(
        run.status.success(),
        "run failed: {:?}\nstderr: {}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8_lossy(&run.stdout).into_owned()
}

/// ADR-0078 Phase-1b proof — the round-trip security property: a password
/// hashed with `fang.hash_password` verifies TRUE against itself via
/// `fang.verify_password`. This is the core argon2 contract (hash-then-
/// verify is the universal auth idiom). The `bool` is surfaced as `1`
/// via the explicit-bool-print idiom; oracle `"1\n"` asserts TRUE.
#[test]
fn test_e2e_fang_hash_then_verify_round_trip_true() {
    let stdout = build_and_run_source(concat!(
        "import fang\n",
        "\n",
        "fn main() -> i64:\n",
        "    let h: str = fang.hash_password(\"hunter2\")\n",
        "    let ok: bool = fang.verify_password(\"hunter2\", h)\n",
        "    if ok:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "1\n");
}

/// Wrong-password-rejects — the security property that MATTERS: a hash of
/// `"hunter2"` must NOT verify against the password `"wrong"`.
/// `fang.verify_password("wrong", h)` is FALSE; oracle `"0\n"` asserts
/// the rejection (only the right pw verifies — argon2's whole point).
#[test]
fn test_e2e_fang_wrong_password_rejects_false() {
    let stdout = build_and_run_source(concat!(
        "import fang\n",
        "\n",
        "fn main() -> i64:\n",
        "    let h: str = fang.hash_password(\"hunter2\")\n",
        "    let ok: bool = fang.verify_password(\"wrong\", h)\n",
        "    if ok:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "0\n");
}

/// Hash-is-PHC — the returned hash string starts with the argon2id PHC
/// prefix `$argon2id$`. Proves the algorithm is argon2id (not a weak
/// algo like argon2i/argon2d or an unsalted digest) AND that the salt is
/// embedded in the self-describing PHC string. The prefix test uses
/// `str.starts_with` (a supported `.cb` str method) surfaced via the
/// explicit-bool-print idiom; oracle `"1\n"` asserts the prefix matches.
#[test]
fn test_e2e_fang_hash_is_argon2id_phc() {
    let stdout = build_and_run_source(concat!(
        "import fang\n",
        "\n",
        "fn main() -> i64:\n",
        "    let h: str = fang.hash_password(\"hunter2\")\n",
        "    let is_phc: bool = h.starts_with(\"$argon2id$\")\n",
        "    if is_phc:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "1\n");
}

/// Hash-is-nondeterministic — two hashes of the SAME password `"x"` must
/// differ (a fresh random salt per call, the PHC discipline) AND both
/// must still verify TRUE. Asserts the conjunction:
///   - the two hash strings are UNEQUAL (`h1 != h2`) — random salt;
///   - `verify_password("x", h1)` is TRUE;
///   - `verify_password("x", h2)` is TRUE.
///
/// Prints one line per property (`1` = property holds); oracle
/// `"1\n1\n1\n"` asserts all three. A deterministic (salt-less) impl
/// would emit `0` on line 1 and fail the security contract.
#[test]
fn test_e2e_fang_hash_is_nondeterministic_both_verify() {
    let stdout = build_and_run_source(concat!(
        "import fang\n",
        "\n",
        "fn main() -> i64:\n",
        "    let h1: str = fang.hash_password(\"x\")\n",
        "    let h2: str = fang.hash_password(\"x\")\n",
        "    if h1 != h2:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    let ok1: bool = fang.verify_password(\"x\", h1)\n",
        "    if ok1:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    let ok2: bool = fang.verify_password(\"x\", h2)\n",
        "    if ok2:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "1\n1\n1\n");
}
