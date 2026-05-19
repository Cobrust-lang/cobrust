//! Category A — ADR-0052c Wave 2 PyCompatTier parse + serde tests.
//!
//! Pinned by ADR-0052c §4 (type changes — `crates/cobrust-translator/src/spec.rs`)
//! and §11 (§2.5 compile-time-catch surface). Today's `py_compat: String`
//! field at `crates/cobrust-translator/src/spec.rs:48` silently accepts any
//! typo (`"strikt"`, `"sematic"`, malformed `"numerical(...)"`) and defers
//! the error to L2-gate-fail-time. After 0052c, the serde custom Deserialize
//! impl rejects at `SpecToml::read()` with `SpecError::Malformed(...)`.
//!
//! These tests are **failing-first** per F28 PAIR pattern: the matching
//! `PyCompatTier` enum + Deserialize + Display impls are DEV-side work
//! (sibling branch `feature/0052c-dev`). All eight tests are marked
//! `#[ignore = "ADR-0052c Wave-2 DEV impl pending"]`.
//!
//! ## Coverage matrix
//!
//! | # | Surface | Spec ref |
//! |---|---------|----------|
//! | A1 | `"strict"` → `PyCompatTier::Strict` | §4 |
//! | A2 | `"semantic"` → `PyCompatTier::Semantic` | §4 |
//! | A3 | `"numerical(rtol=1e-7)"` → `PyCompatTier::Numerical { rtol: 1e-7 }` | §4 |
//! | A4 | `"none"` → `PyCompatTier::None` | §4 |
//! | A5 | `"strikt"` → parse error (typo, compile-time-catch) | §11 |
//! | A6 | `"numerical(rtol=)"` → parse error (empty rtol) | §4 |
//! | A7 | `"numerical"` → parse error or default rtol (missing args) | §4 |
//! | A8 | Backward-compat: existing tomli/dateutil/msgpack spec.toml strings still load | §"Migration plan" |

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::missing_panics_doc,
    clippy::print_stdout,
    clippy::uninlined_format_args
)]

use cobrust_translator::spec::SpecToml;

/// Build a minimal SpecToml document with one function whose `py_compat`
/// field is the caller-supplied string. The rest of the schema is fixed
/// so the toml::from_str call exercises ONLY the py_compat parse path.
fn spec_with_py_compat(tier_string: &str) -> String {
    format!(
        r#"
schema_version = 1
library = "test_corpus"
upstream_version = "0.0.1"
oracle_module = "stub"
oracle_runtime = "cpython"
oracle_runtime_version = "3.11"

[function.f]
qualname = "stub.f"
public = true
signature = "f() -> None"
py_compat = {tier_value}
description = "Stub."
exemplars = []
errors_on = []

[verification]
seeds = [1]
fuzz_inputs_per_fn = 1
tolerance = "exact"
"#,
        // We pass the tier value as a quoted TOML string. Callers must
        // escape any internal quotes; the eight test cases below use
        // simple non-escapable strings (no internal `"`).
        tier_value = format_args!("\"{tier_string}\""),
    )
}

// ============================================================================
// A1 — "strict" round-trips to PyCompatTier::Strict
// ============================================================================

/// Per ADR-0052c §3 tier matrix row 1: `"strict"` is the canonical
/// byte-identical-oracle tier. The serde custom Deserialize must read
/// the TOML string `"strict"` and emit `PyCompatTier::Strict`. The
/// deterministic-id contract (BTreeMap iteration order pinning) requires
/// that the round-trip back to TOML produce the same string.
#[test]
fn a1_strict_string_parses_to_strict_variant() {
    let toml = spec_with_py_compat("strict");
    let spec: SpecToml = toml::from_str(&toml).expect("strict must parse");

    // The 0052c-DEV impl exposes `PyCompatTier` as a pub re-export of
    // `crate::spec::PyCompatTier`. This test asserts on the Debug form
    // because the enum lives in DEV-side code; once DEV ships, the
    // assertion tightens to a typed-match arm (see TEST author followup
    // §3.3 doc note in the sibling ADR-0052c §10 PAIR plan).
    let f = spec.function.get("f").expect("function f exists");

    // DEV-side contract: PyCompatTier::Strict's Debug form is exactly
    // "Strict" (no struct/payload). We test by Debug-format-match to
    // avoid coupling this TEST file to the not-yet-shipped enum type.
    let dbg = format!("{:?}", f.py_compat);
    assert_eq!(
        dbg, "Strict",
        "A1 contract: \"strict\" must parse to PyCompatTier::Strict; got Debug={:?}",
        dbg
    );
}

// ============================================================================
// A2 — "semantic" round-trips to PyCompatTier::Semantic
// ============================================================================

/// Per ADR-0052c §3 tier matrix row 2: `"semantic"` is the
/// structural-equivalence tier. Reserved for libraries where
/// dict-iteration-order / error-text drift is acceptable (e.g.
/// `urllib.parse` Python-3-vs-2 quirks). Today's `corpus/click/spec.toml`
/// already declares `py_compat = "semantic"` on at least one entry; that
/// callsite must still load post-0052c.
#[test]
fn a2_semantic_string_parses_to_semantic_variant() {
    let toml = spec_with_py_compat("semantic");
    let spec: SpecToml = toml::from_str(&toml).expect("semantic must parse");
    let f = spec.function.get("f").expect("function f exists");
    let dbg = format!("{:?}", f.py_compat);
    assert_eq!(
        dbg, "Semantic",
        "A2 contract: \"semantic\" must parse to PyCompatTier::Semantic; got Debug={:?}",
        dbg
    );
}

// ============================================================================
// A3 — "numerical(rtol=1e-7)" parses with explicit rtol payload
// ============================================================================

/// Per ADR-0052c §3 tier matrix row 3 + §4: `"numerical(rtol=1e-7)"`
/// must parse to `PyCompatTier::Numerical { rtol: 1e-7 }`. The rtol
/// payload is read via regex per §4 spec text. This is the canonical
/// numpy-test idiom (`numpy.testing.assert_allclose(rtol=...)`).
#[test]
fn a3_numerical_with_rtol_parses_payload() {
    let toml = spec_with_py_compat("numerical(rtol=1e-7)");
    let spec: SpecToml = toml::from_str(&toml).expect("numerical(rtol=1e-7) must parse");
    let f = spec.function.get("f").expect("function f exists");

    // The Debug form for PyCompatTier::Numerical { rtol: 1e-7 } is
    // expected to be `Numerical { rtol: 1e-7 }` per Rust derive(Debug)
    // default for tuple/struct enum variants. Float-printing varies by
    // platform (`1e-7` vs `0.0000001`), so we substring-check for both
    // the variant name and the rtol payload value.
    let dbg = format!("{:?}", f.py_compat);
    assert!(
        dbg.starts_with("Numerical"),
        "A3 contract: parse must produce a Numerical variant; got Debug={:?}",
        dbg
    );
    // The rtol payload must be readable. We check via substring because
    // Rust's float-Display picks one of "1e-7" / "0.0000001" / similar.
    assert!(
        dbg.contains("1e-7") || dbg.contains("0.0000001"),
        "A3 contract: Numerical variant must carry rtol=1e-7 payload; got Debug={:?}",
        dbg
    );
}

// ============================================================================
// A4 — "none" round-trips to PyCompatTier::None
// ============================================================================

/// Per ADR-0052c §3 tier matrix row 4: `"none"` is the gate-disabled
/// tier — VerifierVerdict::Accept unconditionally + GateOutcome::Skip
/// recorded honestly per ADR-0040. This is the opt-out path retained for
/// translations that intentionally have no correctness contract (e.g.
/// docstring-only or repair-loop failure footer per `repair.rs:233`).
#[test]
fn a4_none_string_parses_to_none_variant() {
    let toml = spec_with_py_compat("none");
    let spec: SpecToml = toml::from_str(&toml).expect("none must parse");
    let f = spec.function.get("f").expect("function f exists");
    let dbg = format!("{:?}", f.py_compat);
    assert_eq!(
        dbg, "None",
        "A4 contract: \"none\" must parse to PyCompatTier::None; got Debug={:?}",
        dbg
    );
}

// ============================================================================
// A5 — "strikt" (typo) is rejected at SpecToml::read time
// ============================================================================

/// Per ADR-0052c §11 §2.5 compile-time-catch contract: today,
/// `"strikt"` parses as `String("strikt")` and silently runs the L2 gate
/// as an unknown tier (effectively AcceptAll since no arm matches).
/// After 0052c, the serde custom impl MUST reject at parse time with
/// `SpecError::Malformed("py_compat: unknown tier 'strikt'; expected
/// strict|semantic|numerical(rtol=…)|none")`. This is the canonical
/// §2.5 compile-time-catch surface — typos surface at spec-load instead
/// of at L2-fail-time.
#[test]
fn a5_strikt_typo_rejected_at_parse_time() {
    let toml = spec_with_py_compat("strikt");
    let result: Result<SpecToml, _> = toml::from_str(&toml);
    let err = result.expect_err("A5 contract: \"strikt\" must reject at parse time");

    // The error message must name the offending tier so the spec author
    // sees the typo immediately. ADR-0052c §11 binds the exact diagnostic
    // shape: `"py_compat: unknown tier 'strikt'; expected
    // strict|semantic|numerical(rtol=…)|none"`. We substring-match the
    // tier name + the expected-variants list rather than the exact
    // string so DEV has a small editing budget.
    let msg = err.to_string();
    assert!(
        msg.contains("strikt"),
        "A5 contract: error must name the offending tier \"strikt\"; got {:?}",
        msg
    );
    assert!(
        msg.contains("strict") && (msg.contains("semantic") || msg.contains("numerical")),
        "A5 contract: error must list the expected variants; got {:?}",
        msg
    );
}

// ============================================================================
// A6 — "numerical(rtol=)" (empty rtol) is rejected at parse time
// ============================================================================

/// Per ADR-0052c §4: the numerical tier requires a non-empty rtol value.
/// `"numerical(rtol=)"` is malformed and MUST reject at parse time, NOT
/// silently default. This is the §2.5 compile-time-catch surface for the
/// numerical arm — the spec author must specify an explicit tolerance
/// or use `"strict"` instead.
#[test]
fn a6_numerical_with_empty_rtol_rejected_at_parse_time() {
    let toml = spec_with_py_compat("numerical(rtol=)");
    let result: Result<SpecToml, _> = toml::from_str(&toml);
    let err = result.expect_err("A6 contract: empty rtol must reject at parse time");
    let msg = err.to_string();
    assert!(
        msg.contains("rtol") || msg.contains("numerical"),
        "A6 contract: error must reference rtol/numerical; got {:?}",
        msg
    );
}

// ============================================================================
// A7 — "numerical" (no parens) is rejected (or defaults explicitly)
// ============================================================================

/// Per ADR-0052c §4: `"numerical"` without `(rtol=...)` is ambiguous.
/// The DEV impl has two acceptable dispositions:
/// 1. Reject at parse time with a "missing rtol argument" diagnostic.
/// 2. Accept and default to a documented baseline rtol (e.g. 1e-7).
///
/// Either is valid; what is NOT valid is silently treating `"numerical"`
/// as Strict (the v0 §2.4 stringly-typed behavior). This test asserts
/// the parse path produces EITHER an explicit error OR a Numerical
/// variant with a default rtol — never any other arm.
#[test]
fn a7_numerical_without_args_explicit_disposition() {
    let toml = spec_with_py_compat("numerical");
    let result: Result<SpecToml, _> = toml::from_str(&toml);

    match result {
        Err(e) => {
            // Disposition 1: explicit reject with rtol-naming diagnostic.
            let msg = e.to_string();
            assert!(
                msg.contains("rtol") || msg.contains("numerical"),
                "A7 contract: reject diagnostic must reference rtol/numerical; got {:?}",
                msg
            );
        }
        Ok(spec) => {
            // Disposition 2: accept with a default rtol. The variant
            // MUST be Numerical (never Strict / Semantic / None by accident).
            let f = spec.function.get("f").expect("function f exists");
            let dbg = format!("{:?}", f.py_compat);
            assert!(
                dbg.starts_with("Numerical"),
                "A7 contract: bare \"numerical\" must be either rejected OR \
                 produce a Numerical variant; got Debug={:?}",
                dbg
            );
        }
    }
}

// ============================================================================
// A8 — backward-compat: existing corpus spec.toml strings still parse
// ============================================================================

/// Per ADR-0052c §"Migration plan" + §1: all three production corpus
/// PROVENANCEs (`corpus/tomli/spec.toml`, `corpus/dateutil/spec.toml`,
/// `corpus/msgpack/spec.toml`) declare `py_compat = "strict"`. The DEV
/// impl MUST NOT break the backward-compat parse for these existing
/// callsites. This test exercises the canonical M4 tomli minimal-spec
/// pattern (`crates/cobrust-translator/src/spec.rs:122` fixture form)
/// and asserts it loads cleanly post-0052c.
#[test]
fn a8_backward_compat_existing_tomli_style_spec_loads() {
    let toml = r#"
schema_version = 1
library = "tomli"
upstream_version = "2.0.1"
oracle_module = "tomllib"
oracle_runtime = "cpython"
oracle_runtime_version = "3.11"

[function.loads]
qualname = "tomli_loads.loads"
public = true
signature = "loads(src: str) -> dict"
py_compat = "strict"
description = "Parse TOML."

[function.skip_whitespace]
qualname = "tomli_loads._skip_whitespace"
public = false
signature = "..."
py_compat = "strict"
description = "Skip whitespace."

[verification]
seeds = [42]
fuzz_inputs_per_fn = 100
tolerance = "exact"
"#;
    let spec: SpecToml =
        toml::from_str(toml).expect("A8 contract: existing tomli-style spec must still parse");
    assert_eq!(spec.function.len(), 2);

    let loads = spec.function.get("loads").expect("loads exists");
    let dbg = format!("{:?}", loads.py_compat);
    assert_eq!(
        dbg, "Strict",
        "A8 contract: existing \"strict\" entry must round-trip to Strict variant; \
         got Debug={:?}",
        dbg
    );

    let skip = spec
        .function
        .get("skip_whitespace")
        .expect("skip_whitespace exists");
    let dbg2 = format!("{:?}", skip.py_compat);
    assert_eq!(
        dbg2, "Strict",
        "A8 contract: second \"strict\" entry must also round-trip; got Debug={:?}",
        dbg2
    );
}
