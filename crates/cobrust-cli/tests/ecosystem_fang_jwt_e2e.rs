//! ADSD TEST-first corpus — end-to-end `.cb` source → compile → link →
//! run for the `fang` JWT surface (HS256-signed JSON Web Tokens), the
//! second value-fn family on the `fang` auth/security module.
//!
//! Twin of `ecosystem_fang_e2e.rs` (the argon2 hash/verify proof). The
//! three new fns mirror that EXACT flat str value-fn shape — no handles,
//! no callbacks, str-in/str-out (or `-> bool`), the den/strike/scale
//! template:
//!
//! - `fang.jwt_encode(claims_json: str, secret: str) -> str` — an HS256
//!   JWT for the given claims (a JSON object string, e.g.
//!   `{"sub":"alice"}`), signed with `secret`.
//! - `fang.jwt_verify(token: str, secret: str) -> bool` — TRUE iff the
//!   token's HS256 signature validates against `secret` (and any standard
//!   claims present); FALSE for a tampered / wrong-secret / malformed /
//!   `alg:none` token. NEVER panics.
//! - `fang.jwt_decode(token: str, secret: str) -> str` — the claims JSON
//!   if the token is valid, else the empty-string sentinel (mirroring
//!   `fang.hash_password`'s empty-sentinel fail-clean convention).
//!
//! The manifest rows the impl must add (twins of the `fang.hash_password`
//! / `fang.verify_password` rows in `cobrust-types/src/ecosystem.rs`):
//!
//! ```text
//! ("fang","jwt_encode") => EcoSig::from_values("__cobrust_fang_jwt_encode", vec![Ty::Str, Ty::Str], Ty::Str, …)
//! ("fang","jwt_verify") => EcoSig::from_values("__cobrust_fang_jwt_verify", vec![Ty::Str, Ty::Str], Ty::Bool, …)
//! ("fang","jwt_decode") => EcoSig::from_values("__cobrust_fang_jwt_decode", vec![Ty::Str, Ty::Str], Ty::Str, …)
//! ```
//!
//! The codegen externs (`llvm_backend.rs`) + the `__cobrust_fang_` build
//! recognizer (`cobrust-cli/src/build/intrinsics.rs:1393`) already cover
//! the `__cobrust_fang_*` prefix, so the JWT shims link via `libfang.a`
//! with NO new linker wiring.
//!
//! ```text
//! `import fang` + `fang.jwt_encode/jwt_verify/jwt_decode`
//!   → cobrust-types ecosystem manifest (typecheck, no AmbiguousType)
//!   → cobrust-mir lowering (retarget → __cobrust_fang_jwt_*)
//!   → cobrust-codegen extern + existing Str drop schedule
//!   → cobrust-fang C-ABI shim (libfang.a) wrapping HS256 sign/verify
//!   → cobrust-cli build.rs per-import static link
//!   → stdout
//! ```
//!
//! Bool idiom: a `bool` value is surfaced to stdout via the canonical
//! Cobrust `if b:\n    print(1)\nelse:\n    print(0)` form (NOT
//! `print(bool)` directly), so the oracle is a stable `"1\n"` / `"0\n"`.
//! The SECURITY cases (wrong-secret, append-tamper, malformed) assert the
//! exact rejection bit `"0\n"`.
//!
//! SECURITY NOTE on test coverage split: the two byte-precision JWT
//! security footguns — the **payload-segment byte tamper** (flip a byte
//! in the MIDDLE dot-separated part of a genuinely-signed token) and the
//! **`alg:none` forgery** (a hand-built `{"alg":"none"}` token a naive
//! verifier would trust) — live in the sibling Rust integration test
//! `crates/cobrust-fang/tests/jwt_cabi_security.rs`, which has byte-level
//! control over the token AND access to the real signing shim as an
//! oracle. THIS file covers the security cases expressible in pure `.cb`:
//! wrong-secret rejection, an append-tamper signature break, and the
//! malformed/empty no-panic path. Both surfaces are RED at HEAD.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::process::Command;

/// Compile + link + run a `.cb` source, returning its stdout. Asserts
/// the build and the run both succeed. Mirrors
/// `ecosystem_fang_e2e.rs::build_and_run_source`.
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

/// Like [`build_and_run_source`] but does NOT assert the run succeeded —
/// returns `(run_succeeded, stdout)`. For the malformed-input cases that
/// assert the program EXITS 0 (no panic / abort) even on garbage token
/// input: the build must still succeed, but we inspect the run's exit
/// status explicitly rather than asserting it up front.
fn build_then_run_capture(source: &str) -> (bool, String) {
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
    (
        run.status.success(),
        String::from_utf8_lossy(&run.stdout).into_owned(),
    )
}

/// CASE 1 — round-trip verify: a token minted with `jwt_encode` for the
/// secret `"s3cret"` verifies TRUE against the SAME secret. This is the
/// core JWT contract (sign-then-verify is the universal auth idiom). The
/// `bool` is surfaced as `1` via the explicit-bool-print idiom; oracle
/// `"1\n"` asserts TRUE.
#[test]
fn test_e2e_fang_jwt_round_trip_verify_true() {
    let stdout = build_and_run_source(concat!(
        "import fang\n",
        "\n",
        "fn main() -> i64:\n",
        "    let t: str = fang.jwt_encode(\"{\\\"sub\\\":\\\"alice\\\"}\", \"s3cret\")\n",
        "    let ok: bool = fang.jwt_verify(t, \"s3cret\")\n",
        "    if ok:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "1\n");
}

/// CASE 2 — round-trip decode: a valid token decodes (via `jwt_decode`
/// with the right secret) to its claims JSON, which CONTAINS the subject
/// `alice`. Uses `str.contains` (a supported `.cb` method, check.rs:2118)
/// surfaced via the explicit-bool-print idiom; oracle `"1\n"` asserts the
/// decoded claims carry the original subject. (A JWT payload may be
/// re-serialised — key order / whitespace are not guaranteed — so the
/// assertion is `contains("alice")`, not full-string equality.)
#[test]
fn test_e2e_fang_jwt_decode_contains_subject() {
    let stdout = build_and_run_source(concat!(
        "import fang\n",
        "\n",
        "fn main() -> i64:\n",
        "    let t: str = fang.jwt_encode(\"{\\\"sub\\\":\\\"alice\\\"}\", \"s3cret\")\n",
        "    let claims: str = fang.jwt_decode(t, \"s3cret\")\n",
        "    let has_sub: bool = claims.contains(\"alice\")\n",
        "    if has_sub:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "1\n");
}

/// CASE 3 (SECURITY, load-bearing) — wrong-secret rejects: a token minted
/// with `"s3cret"` must NOT verify against the secret `"wrong"`. This is
/// the property that MATTERS — an HS256 token's signature is keyed on the
/// secret, so a different secret yields a different MAC and the
/// verification fails. `jwt_verify(t, "wrong")` is FALSE; oracle `"0\n"`
/// asserts the rejection.
#[test]
fn test_e2e_fang_jwt_wrong_secret_rejects_false() {
    let stdout = build_and_run_source(concat!(
        "import fang\n",
        "\n",
        "fn main() -> i64:\n",
        "    let t: str = fang.jwt_encode(\"{\\\"sub\\\":\\\"alice\\\"}\", \"s3cret\")\n",
        "    let ok: bool = fang.jwt_verify(t, \"wrong\")\n",
        "    if ok:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "0\n");
}

/// CASE 4 (SECURITY, load-bearing) — append-tamper breaks the signature:
/// take a genuinely-signed token `t` and append a byte (`t + "X"`). The
/// trailing byte corrupts the base64url signature segment, so the
/// recomputed MAC no longer matches the (now-mutated) signature →
/// `jwt_verify` is FALSE. This is the pure-`.cb`-expressible tamper (the
/// byte-precise MIDDLE-payload-segment tamper lives in the sibling
/// `jwt_cabi_security.rs`). A naive verifier that only base64url-decodes
/// without re-checking the MAC would wrongly accept it. Oracle `"0\n"`
/// asserts the tampered token is rejected.
#[test]
fn test_e2e_fang_jwt_append_tamper_rejects_false() {
    let stdout = build_and_run_source(concat!(
        "import fang\n",
        "\n",
        "fn main() -> i64:\n",
        "    let t: str = fang.jwt_encode(\"{\\\"sub\\\":\\\"alice\\\"}\", \"s3cret\")\n",
        "    let tampered: str = t + \"X\"\n",
        "    let ok: bool = fang.jwt_verify(tampered, \"s3cret\")\n",
        "    if ok:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "0\n");
}

/// CASE 5 (SECURITY) — malformed token, NO panic: `jwt_verify` of a
/// not-a-JWT string `"not.a.jwt"` and of the empty string `""` must each
/// be a clean FALSE, and the program must EXIT 0 (the shim NEVER panics /
/// aborts on garbage input — CLAUDE.md §2.2: a malformed token is normal
/// control flow, not an exceptional condition). Asserts BOTH the exit
/// code (0) AND the oracle (`"0\n0\n"`, both rejected).
#[test]
fn test_e2e_fang_jwt_malformed_is_false_no_panic_exit0() {
    let (run_ok, stdout) = build_then_run_capture(concat!(
        "import fang\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: bool = fang.jwt_verify(\"not.a.jwt\", \"s3cret\")\n",
        "    if a:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    let b: bool = fang.jwt_verify(\"\", \"s3cret\")\n",
        "    if b:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    return 0\n",
    ));
    assert!(
        run_ok,
        "program must EXIT 0 on malformed token (no panic/abort)"
    );
    assert_eq!(stdout, "0\n0\n");
}

/// CASE 6 — decode of an invalid token yields the empty sentinel (and
/// verify is FALSE). `jwt_decode("not.a.jwt", secret)` returns the
/// empty-string sentinel (mirroring `fang.hash_password`'s fail-clean
/// empty-sentinel convention, cabi.rs:157), so `claims == ""` is TRUE;
/// AND `jwt_verify` of the same garbage is FALSE. Asserts the
/// conjunction via two print lines; oracle `"1\n0\n"`:
///   - line 1 (`1`): the decoded claims string IS empty (sentinel);
///   - line 2 (`0`): verify of the invalid token is FALSE.
///
/// Emptiness is tested via the `str == ""` NATURAL operator (the
/// always-linked `__cobrust_str_eq` path added alongside the argon2
/// chain), NOT a `str.is_empty()` method — `.cb`'s str method-table
/// (`len`/`split`/`replace`/`trim`/`find`/`contains`/`starts_with`/
/// `ends_with`/`lower`/`upper`) does not yet carry `is_empty`; adding it
/// is its own intrinsic-rewrite-pipeline language task, out of scope for
/// the JWT shim sprint. `claims == ""` tests the identical property.
#[test]
fn test_e2e_fang_jwt_decode_invalid_is_empty_sentinel() {
    let stdout = build_and_run_source(concat!(
        "import fang\n",
        "\n",
        "fn main() -> i64:\n",
        "    let claims: str = fang.jwt_decode(\"not.a.jwt\", \"s3cret\")\n",
        "    let empty: bool = claims == \"\"\n",
        "    if empty:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    let ok: bool = fang.jwt_verify(\"not.a.jwt\", \"s3cret\")\n",
        "    if ok:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    return 0\n",
    ));
    assert_eq!(stdout, "1\n0\n");
}
