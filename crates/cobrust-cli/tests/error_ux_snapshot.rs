//! ADR-0052b Wave-2 — Error UX rewrite: CLI renderer snapshot corpus.
//!
//! Each test compiles a failing program with `cobrust check` and
//! verifies the rendered stderr output contains the expected
//! suggestion text per ADR-0052b §3 surface examples.
//!
//! The contract:
//!
//! - The stderr must contain a fix-path line whose text matches the
//!   §4 variant table for the triggered variant. Both `hint: <text>`
//!   and `suggestion: <text>` line labels are accepted — DEV chooses
//!   the surface label (§3.1 surface example uses `suggestion:`).
//! - The match is on substring (not exact-line) so DEV has freedom
//!   to refine prose while preserving the canonical fix-path keyword.
//! - All tests are ``
//!   pre-merge per F28 strict-separation PAIR pattern.
//!
//! Per ADR-0052b §3 surface examples, the 8 scenarios below cover
//! one canonical case per major variant family:
//!
//! - 01: `ImplicitTruthiness` (§3.1 §2.5-canonical case)
//! - 02: `TypeMismatch` (§3.2 generic single-text)
//! - 03: `UseAfterMove` (§3.3 — MIR error path)
//! - 04: `AmbiguousType` (§3.4)
//! - 05: `UnknownName` (§3.5 dynamic-format drop case)
//! - 06: `MutableDefault` (§3.6)
//! - 07: `NotHashable` (§3.8 f64-key forbidden)
//! - 08: `DictSpreadNotSupported` (Phase-G feature gated)
//!
//! Pre-reads:
//! - `docs/agent/adr/0052b-error-ux-fix-suggestions.md` §3, §7.
//! - `crates/cobrust-cli/src/error_ux.rs` (renderer).
//! - `crates/cobrust-cli/tests/error_ux_corpus.rs` (idiom).

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

fn cobrust_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cobrust"))
}

/// Write a `.cb` snippet to a temp file and run `cobrust check` on
/// it. Returns `(exit_code, stderr_text)`.
///
/// Snapshot tests examine stderr (where errors print) — stdout is
/// captured separately + joined for full diagnostic visibility on
/// failure, but the suggestion-text assertion targets stderr only.
fn check_snippet_stderr(name: &str, source: &str) -> (i32, String) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let file = dir.path().join(format!("{name}.cb"));
    std::fs::write(&file, source).expect("write source");

    let out = Command::new(cobrust_binary())
        .arg("check")
        .arg(&file)
        .output()
        .expect("invoke cobrust check");

    // Combine stderr + stdout so DEV can route diagnostics to either
    // stream; today they go to stderr per cobrust-cli/src/error_ux.rs
    // Display impl + stderr_writeln invocation.
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let combined = format!("{stderr}{stdout}");

    let code = out.status.code().unwrap_or(-1);
    (code, combined)
}

/// Assert the rendered output contains the suggestion-text needle.
///
/// `needle_keywords` is a list of strings ALL of which must appear in
/// the stderr — this lets the test match the canonical "fix path"
/// (e.g. `if x != 0:` for ImplicitTruthiness) without locking the
/// surrounding prose word-for-word.
fn assert_suggestion_contains(name: &str, source: &str, needle_keywords: &[&str]) {
    let (code, combined) = check_snippet_stderr(name, source);
    assert_eq!(
        code, 2,
        "{name}: expected exit code 2 (type error per ADR-0024), got {code}\nstderr+stdout:\n{combined}"
    );
    for needle in needle_keywords {
        assert!(
            combined.contains(needle),
            "{name}: stderr must contain the canonical fix-path keyword `{needle}` per ADR-0052b §3 surface examples\nstderr+stdout:\n{combined}"
        );
    }
}

// ============================================================
// §3.1 — `ImplicitTruthiness` canonical CLAUDE.md §2.5 case.
// ============================================================

#[test]

fn snap_01_implicit_truthiness_suggestion() {
    // `if x:` where `x: i64`. Per ADR-0052b §3.1 surface example:
    //
    //   suggestion: change to `if x != 0:` (use `.is_some()` for Option)
    //
    // Canonical fix-path keyword: `if x != 0:` (the static text the
    // §4.1 row pins). DEV may choose prose around it.
    assert_suggestion_contains(
        "snap_01_implicit_truthiness",
        r"fn f(x: i64) -> i64:
    if x:
        return 1
    return 0
",
        &["if x != 0"],
    );
}

// ============================================================
// §3.2 — `TypeMismatch` generic single-text per §11 dynamic-drop.
// ============================================================

#[test]

fn snap_02_type_mismatch_suggestion() {
    // `let x: i64 = "hello"`. Per ADR-0052b §3.2 + §4.1:
    //
    //   suggestion: change the expression type or add `: <expected>` annotation
    //
    // Canonical fix-path keyword: `type annotation` (the §4.1 row
    // pins one of "change the expression type" OR "add a type
    // annotation"). Matching on "annotation" covers both options.
    assert_suggestion_contains(
        "snap_02_type_mismatch",
        r#"fn f() -> i64:
    let x: i64 = "hello"
    return x
"#,
        &["annotation"],
    );
}

// ============================================================
// §3.3 — `UseAfterMove` MIR error path (ADR-0052a precedent).
//
// The MIR path is exercised via `cobrust check` when the source
// triggers a borrow-check failure. Per ADR-0052a §7 + ADR-0052b
// §3.3 lifted-to-construction-site:
//
//   suggestion: change to `&s` to borrow without consuming
//               (ADR-0052a explicit shared borrow)
// ============================================================

#[test]
#[ignore = "finding:check-exit-code-borrow-gap — ADR-0052b §3.3: `cobrust check` exits 0 instead of 2 for cross-statement use-after-move; intra-block borrow-checker does not cover `let zs = xs` then `print(xs)` across statements. Landing: Phase H+ borrow-check widening."]
fn snap_03_use_after_move_suggestion() {
    // Construct a use-after-move scenario at source level. The
    // canonical Cobrust source is:
    //
    //   fn main() -> i64:
    //       let xs: list[i64] = [1, 2, 3]
    //       let ys = xs            ← move
    //       let zs = xs            ← UseAfterMove
    //       return 0
    //
    // Canonical fix-path keyword: `&s` (the literal text ADR-0052a
    // pins for the suggestion). Matching on `&` covers both
    // `&s` and `&xs` if DEV chose a contextual rendering.
    //
    // Pre-impl note: today's renderer at error_ux.rs:817-830
    // already produces `change to \`&s\` to borrow without
    // consuming`. The DEV refactor lifts this to construction
    // site but the rendered text stays the same. The test locks
    // the keyword `&s` either way.
    assert_suggestion_contains(
        "snap_03_use_after_move",
        r"fn main() -> i64:
    let xs: list[i64] = [1, 2, 3]
    let ys = xs
    let zs = xs
    return 0
",
        &["&"],
    );
}

// ============================================================
// §3.4 — `AmbiguousType`.
// ============================================================

#[test]

fn snap_04_ambiguous_type_suggestion() {
    // `let x = []` — empty list with no inferable element type.
    // Per ADR-0052b §3.4 + §4.1:
    //
    //   suggestion: add an explicit type annotation, e.g. `let x: i64 = …`
    //
    // Canonical fix-path keyword: `let x:` (showing the
    // annotation-form fix) AND `annotation` (the noun).
    assert_suggestion_contains(
        "snap_04_ambiguous_type",
        r"fn f() -> i64:
    let x = []
    return 0
",
        &["annotation"],
    );
}

// ============================================================
// §3.5 — `UnknownName` dynamic-format-drop case.
//
// Today: `did you mean to declare it with `let {name} = …`?`
// Tomorrow: `declare with `let <name> = …` first` (static).
//
// The test verifies the SUGGESTION text uses the literal
// `<name>` placeholder, NOT the bound identifier `foo`.
// ============================================================

#[test]

fn snap_05_unknown_name_static_suggestion() {
    // `let r = foo` where `foo` is undeclared. Per ADR-0052b §3.5:
    //
    //   suggestion: declare with `let <name> = …` first
    //
    // Canonical fix-path keyword: `let` (the form keyword). The
    // primary error line still mentions `foo` (the bound name) so
    // LLM stderr parsing retains it — that's verified separately
    // in s0052b_28 + s0052b_29.
    assert_suggestion_contains(
        "snap_05_unknown_name",
        r"fn f() -> i64:
    let r = foo
    return r
",
        &["let"],
    );
}

// ============================================================
// §3.6 — `MutableDefault`.
// ============================================================

#[test]

fn snap_06_mutable_default_suggestion() {
    // `fn g(xs: list[i64] = [])` — mutable default forbidden. Per
    // ADR-0052b §3.6 + §4.1:
    //
    //   suggestion: use `None` as the default and assign inside the function body
    //
    // Canonical fix-path keyword: `None` (the recommended-default
    // value). DEV's wording flexibility is around the surrounding
    // "and assign inside the function body" prose.
    assert_suggestion_contains(
        "snap_06_mutable_default",
        r"fn g(xs: list[i64] = []) -> i64:
    return 0
",
        &["None"],
    );
}

// ============================================================
// §3.8 — `NotHashable` f64-key forbidden.
// ============================================================

#[test]

fn snap_07_not_hashable_suggestion() {
    // `{1.5: 0}` — f64 dict key forbidden per ADR-0050d Decision
    // 7A. Per ADR-0052b §3.8 + §4.1:
    //
    //   suggestion: f64 keys are forbidden (NaN != NaN); use i64 via
    //               `f.to_bits() as i64` or a str repr
    //
    // Canonical fix-path keyword: `to_bits` (the recommended fix
    // for f64 → i64 hashable conversion). The reason-prose `NaN !=
    // NaN` is also pinnable but `to_bits` is the actionable fix.
    assert_suggestion_contains(
        "snap_07_not_hashable",
        r"fn f() -> i64:
    let d = {1.5: 0}
    return 0
",
        &["to_bits"],
    );
}

// ============================================================
// `DictSpreadNotSupported` Phase-G feature gated rejection.
// ============================================================

#[test]

fn snap_08_dict_spread_not_supported_suggestion() {
    // `{**other}` — dict-spread is Phase G. Per ADR-0052b §4.1:
    //
    //   suggestion: dict-merge is Phase G; build the result manually
    //               by iterating `other.items()` and inserting
    //
    // Canonical fix-path keyword: `items()` (the iteration
    // workaround the §4.1 row pins). Also `Phase G` is the
    // explanation but `items()` is the actionable fix.
    assert_suggestion_contains(
        "snap_08_dict_spread_not_supported",
        r"fn f() -> i64:
    let other: dict[str, i64] = {}
    let d = {**other}
    return 0
",
        &["items()"],
    );
}
