//! `import re` (regular expressions) — `.cb` end-to-end proof for the
//! ADR-0084 addition: the `regex`-crate-backed stateless subset of
//! Python's `re`. VERY HIGH-USE — string/regex processing is one of the
//! most-reached-for Python capabilities, and the translation pipeline
//! needs it. These tests compile to REAL binaries, link, spawn, and
//! assert stdout / exit code, proving the str / list[str] / bool returns
//! are usable END-TO-END.
//!
//! ## The four functions (all stateless, NO Match-object groups)
//!
//! - `re.sub(pattern, repl, s) -> str` — replace ALL non-overlapping
//!   matches (`re.sub("a", "X", "banana") == "bXnXnX"`). The Str-return
//!   shape of `string`'s `replace` shim.
//! - `re.findall(pattern, s) -> list[str]` — every non-overlapping FULL
//!   match (`re.findall("[0-9]+", "a1b22c333") == ["1", "22", "333"]`;
//!   `[]` on no match). The list[str]-return shape of redis `smembers`.
//! - `re.match(pattern, s) -> bool` — START-anchored (CPython `re.match`).
//! - `re.search(pattern, s) -> bool` — match ANYWHERE (CPython
//!   `re.search`). The bool-return shape of `math.isnan`.
//!
//! ## The load-bearing semantics (confirmed via python3.11)
//!
//! - `re.sub` replaces ALL occurrences, not just the first
//!   (`re.sub("a", "X", "banana") == "bXnXnX"`, three replacements).
//! - `re.findall` is ITERATED here in a `.cb` `for` loop — proving the
//!   `list[str]` return is a first-class, drop-scheduled, usable list.
//! - `re.match` vs `re.search` DIFFER on the anchor — the distinguishing
//!   test: `re.match("bc", "abc")` is False (the pattern is NOT at the
//!   start) but `re.search("bc", "abc")` is True (it IS present later).
//!   The `if re.search(...):` branch proves the bool is a real condition.
//! - An INVALID runtime pattern (`"["`) TRAPS — the binary exits NON-ZERO
//!   with a clean `cobrust panic` (CPython raises `re.error`); it is NOT
//!   a silent no-match and NOT a Rust unwind across the C-ABI.
//!
//! ## @py_compat tier: Semantic (a documented divergence)
//!
//! The Rust `regex` flavor matches Python `re` for the common patterns
//! (classes, quantifiers, alternation, anchors, groups) but has NO
//! backreferences and NO lookaround. `re.findall` returns the FULL
//! matches (== CPython for the no-group form); CPython's group-capture
//! behavior is a documented deferral (ADR-0084).
//!
//! Mirrors the compile->spawn->assert-stdout harness of `coil_arange_e2e`.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative doc comments read as "lazy" list items to clippy; they
// are intentional explanatory prose, not lint targets. (This `#![allow]` is
// the lesson math-part2 learned the hard way — the doc-lint on the e2e header
// only surfaces in `-p cobrust-cli --all-targets` clippy.)
#![allow(clippy::doc_lazy_continuation)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; panic with stderr on build
/// failure. Mirrors `coil_arange_e2e::compile_source`.
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
    let out = Command::new(exe).output().expect("spawn re prog");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

// =====================================================================
// POSITIVE — `re.sub` replaces ALL non-overlapping matches.
// `re.sub("a", "X", "banana") == "bXnXnX"` — THREE replacements, not one.
// A "first-only" bug would print `bXnana`.
// =====================================================================

/// `re.sub("a", "X", "banana")` -> `bXnXnX`. The Str-return path, printed.
///
/// Oracle (python3.11): `re.sub('a','X','banana') == 'bXnXnX'`.
#[test]
fn test_e2e_sub_replaces_all() {
    let source = concat!(
        "import re\n",
        "\n",
        "fn main() -> i64:\n",
        "    let out: str = re.sub(\"a\", \"X\", \"banana\")\n",
        "    print(out)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "bXnXnX",
        "expected ALL 'a' replaced (a first-only bug prints 'bXnana'); got stdout=\n{stdout}",
    );
}

/// `re.sub("[0-9]+", "#", "a1b22c333")` -> `a#b#c#` — a CHARACTER-CLASS
/// pattern (not just a literal) collapses each digit-run to one `#`.
///
/// Oracle (python3.11): `re.sub('[0-9]+','#','a1b22c333') == 'a#b#c#'`.
#[test]
fn test_e2e_sub_class_pattern() {
    let source = concat!(
        "import re\n",
        "\n",
        "fn main() -> i64:\n",
        "    let out: str = re.sub(\"[0-9]+\", \"#\", \"a1b22c333\")\n",
        "    print(out)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "a#b#c#",
        "expected each digit-RUN collapsed to one '#'; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `re.findall` returns a list[str] that is ITERATED in a `.cb`
// for-loop (proving the list return is a first-class, usable list).
// `re.findall("[0-9]+", "a1b22c333") == ["1", "22", "333"]`.
// =====================================================================

/// `re.findall("[0-9]+", "a1b22c333")` -> `["1", "22", "333"]`, each
/// printed on its own line by a `for n in nums:` loop. THIS proves the
/// list[str] return is iterable from `.cb`, end-to-end.
///
/// Oracle (python3.11): `re.findall('[0-9]+','a1b22c333') == ['1','22','333']`.
#[test]
fn test_e2e_findall_iterated() {
    let source = concat!(
        "import re\n",
        "\n",
        "fn main() -> i64:\n",
        "    let nums: list[str] = re.findall(\"[0-9]+\", \"a1b22c333\")\n",
        "    for n in nums:\n",
        "        print(n)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "1\n22\n333",
        "expected the three matched runs ITERATED in order; got stdout=\n{stdout}",
    );
}

/// `re.findall("[0-9]+", "abcdef")` -> `[]`. The empty-list edge: the
/// for-loop body NEVER runs, so a sentinel line after it is the only
/// output. Proves no-match mints an empty (usable) list, not a trap.
///
/// Oracle (python3.11): `re.findall('[0-9]+','abcdef') == []`.
#[test]
fn test_e2e_findall_empty_no_match() {
    let source = concat!(
        "import re\n",
        "\n",
        "fn main() -> i64:\n",
        "    let nums: list[str] = re.findall(\"[0-9]+\", \"abcdef\")\n",
        "    for n in nums:\n",
        "        print(n)\n",
        "    print(\"done\")\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "done",
        "expected an EMPTY list (loop body never runs, only 'done' prints); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `re.match` vs `re.search` DIFFER on the anchor. THE
// load-bearing test: same pattern + haystack, opposite truth values.
// The `if re.search(...):` / `if re.match(...):` branches prove the bool
// returns are real condition values.
// =====================================================================

/// `re.match("bc", "abc")` is False (NOT start-anchored at 0) but
/// `re.search("bc", "abc")` is True (present later). Both drive `if`
/// branches; the output `search-yes\nmatch-no` proves they DIFFER.
///
/// Oracle (python3.11): `bool(re.match('bc','abc')) == False`,
/// `bool(re.search('bc','abc')) == True`.
#[test]
fn test_e2e_match_vs_search_differ() {
    let source = concat!(
        "import re\n",
        "\n",
        "fn main() -> i64:\n",
        "    if re.search(\"bc\", \"abc\"):\n",
        "        print(\"search-yes\")\n",
        "    else:\n",
        "        print(\"search-no\")\n",
        "    if re.match(\"bc\", \"abc\"):\n",
        "        print(\"match-yes\")\n",
        "    else:\n",
        "        print(\"match-no\")\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "search-yes\nmatch-no",
        "the load-bearing distinction: search('bc','abc') is True but \
         match('bc','abc') is False; got stdout=\n{stdout}",
    );
}

/// `re.match("ab", "abc")` is True (start-anchored, `ab` IS at index 0).
/// The positive `match` case (complements the False case above).
///
/// Oracle (python3.11): `bool(re.match('ab','abc')) == True`.
#[test]
fn test_e2e_match_true_at_start() {
    let source = concat!(
        "import re\n",
        "\n",
        "fn main() -> i64:\n",
        "    if re.match(\"ab\", \"abc\"):\n",
        "        print(\"match-yes\")\n",
        "    else:\n",
        "        print(\"match-no\")\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "match-yes",
        "expected match('ab','abc')=True (anchored at 0); got stdout=\n{stdout}",
    );
}

// =====================================================================
// NEGATIVE (RUNTIME TRAP) — an INVALID pattern (`"["`) makes the shim's
// `regex::Regex::new` return Err, which becomes a CLEAN `__cobrust_panic`
// (NON-ZERO exit). NOT a silent no-match, NOT a Rust unwind. The build
// SUCCEEDS (the pattern is a runtime str); the trap is at RUN time.
// CPython raises `re.error` — Cobrust traps. (A compile-time check for a
// LITERAL pattern is an ADR-0084 §"Deferred" follow-up.)
// =====================================================================

/// `re.sub("[", "X", "abc")` builds fine but TRAPS at runtime (non-zero
/// exit) with a `cobrust panic` naming the invalid pattern. THIS proves
/// the invalid-pattern policy: a clean trap, never a silent no-match.
#[test]
fn test_e2e_invalid_pattern_traps_nonzero() {
    let source = concat!(
        "import re\n",
        "\n",
        "fn main() -> i64:\n",
        "    let out: str = re.sub(\"[\", \"X\", \"abc\")\n",
        "    print(out)\n",
        "    return 0\n",
    );
    // The build MUST succeed — the pattern is a runtime string, so the
    // malformation is a RUNTIME error (the literal-pattern compile-check
    // is an ADR-0084 deferral).
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(
        !ok,
        "expected a NON-ZERO exit (clean trap) on an invalid pattern, \
         NOT a silent no-match; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    assert!(
        stderr.contains("cobrust panic") && stderr.contains("invalid pattern"),
        "expected a clean `cobrust panic: ... invalid pattern` on stderr \
         (CPython raises re.error; Cobrust traps); got stderr=\n{stderr}",
    );
}
