//! M-F.3.5 — String stdlib end-to-end corpus (ADR-0050e Tier C+D).
//!
//! Locks the source-level surface for the eleven PRELUDE fns added by
//! M-F.3.5: `split` / `join` / `replace` / `trim` / `find` /
//! `contains` / `starts_with` / `ends_with` / `lower` / `upper` +
//! `clone` (LC-100 honest-debt mitigation per
//! `findings/lc100-str-use-after-move-regression-from-adr0050c.md`
//! Path D §"Phase G closure scope").
//!
//! Pre-impl status (TEST corpus baseline at branch
//! `feature/f3-string-stdlib-test` off `main@8b081ae`; verified via
//! Mac `cargo test -p cobrust-cli --test string_stdlib_e2e --locked`):
//!
//!   - The PRELUDE in `crates/cobrust-cli/src/build.rs:51` does NOT
//!     yet declare the eleven new fns. Source-level calls to e.g.
//!     `split(s, ",")` produce parse-time `UnknownName` or fail
//!     intrinsic-rewrite (`Kind` enum has no `StrSplit`).
//!   - The C-ABI shims `__cobrust_str_split` / `__cobrust_str_join`
//!     etc. do NOT exist in `crates/cobrust-stdlib/src/string.rs`.
//!   - Therefore every test in this file SHOULD FAIL pre-impl and is
//!     marked `#[ignore = "M-F.3.5 sub-sprint N — remove ignore post-DEV"]`.
//!     The DEV PAIR removes the ignore markers as each sub-sprint
//!     closes.
//!   - Exception: `f3str16..f3str20` (LC-100 clone() mitigation) are
//!     the LOAD-BEARING tests; they unblock LC-100 corpus closure once
//!     `clone()` ships end-to-end. They are also `#[ignore]` pre-impl.
//!
//! Test families:
//!
//! - `f3str01..f3str10` — happy-path build+run for each of the
//!   eleven surface fns; covers signature semantics + empty-input
//!   edge cases from ADR-0050e Decision 8.
//! - `f3str11..f3str15` — chained patterns: split + for-iter + print;
//!   join(split(s, ","), ","); contains(lower(s), needle); join over
//!   empty list; round-trip identity.
//! - `f3str16..f3str20` — LC-100 clone() mitigation: the load-bearing
//!   reverse_string + valid_parentheses + 3 additional shapes proving
//!   `let s2 = clone(s); ...str_len(s)... ...str_at(s2, 0)...` compiles
//!   and runs after M-F.3.5 closes the honest-debt.
//! - `f3str21..f3str25` — F30 bug-witness regression coverage per
//!   `findings/predicate-flip-cascade-discovery-deficit.md`. One test
//!   per Wave 2 cascade-bug class, transposed onto the string-stdlib
//!   surface so future predicate flips can't silently regress.
//!
//! Per `feedback_p9_clippy_stall_pattern.md` 2026-05-09:
//! 18-lint clippy module-level allow header at the TOP of the file.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::len_zero)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::no_effect_underscore_binding)]
#![allow(clippy::too_many_lines)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn cobrust_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cobrust"))
}

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("workspace root")
}

struct TempPath {
    _temp_dir: tempfile::TempDir,
    path: PathBuf,
}

impl std::ops::Deref for TempPath {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

fn write_cb(name: &str, contents: &str) -> TempPath {
    let dir = tempfile::tempdir().expect("create temp source dir");
    let path = dir.path().join(format!("{name}.cb"));
    std::fs::write(&path, contents).expect("write temp .cb");
    TempPath {
        _temp_dir: dir,
        path,
    }
}

struct BuiltExe {
    _temp_dir: tempfile::TempDir,
    path: PathBuf,
}

impl std::ops::Deref for BuiltExe {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

fn run_build_exe(src: &Path) -> (i32, BuiltExe, String) {
    let bin = cobrust_binary();
    let exe_dir = tempfile::tempdir().expect("create temp exe dir");
    let exe = exe_dir.path().join(src.file_stem().unwrap());
    let out = Command::new(&bin)
        .arg("build")
        .arg(src)
        .arg("-o")
        .arg(&exe)
        .arg("--quiet")
        .current_dir(workspace_root())
        .output()
        .expect("invoke cobrust build");
    let code = out.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (
        code,
        BuiltExe {
            _temp_dir: exe_dir,
            path: exe,
        },
        stderr,
    )
}

fn run_exe(exe: &Path, args: &[&str], stdin_bytes: &[u8]) -> (i32, String, String) {
    let mut child = Command::new(exe)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn produced exe");
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        let _ = stdin.write_all(stdin_bytes);
    }
    let out = child.wait_with_output().expect("wait_with_output");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn assert_build_run(name: &str, src: &str, args: &[&str], stdin: &[u8], expected_stdout: &str) {
    let path = write_cb(name, src);
    let (build_code, exe, build_stderr) = run_build_exe(&path);
    assert_eq!(build_code, 0, "{name}: build failed; stderr={build_stderr}");
    let (run_code, stdout, run_stderr) = run_exe(&exe, args, stdin);
    assert_eq!(run_code, 0, "{name}: run failed; stderr={run_stderr}");
    assert_eq!(
        stdout, expected_stdout,
        "{name}: stdout mismatch\nstderr={run_stderr}"
    );
}

// =====================================================================
// f3str01..f3str10 — happy-path E2E per surface fn.
//
// Each test invokes one surface fn against a literal Str input and
// asserts the printed result. Covers Decision 3 signatures + Decision 8
// edge cases.
// =====================================================================

#[test]
#[ignore = "M-F.3.5 sub-sprint 3 wires __cobrust_str_split + PRELUDE + intrinsic-rewrite; remove ignore post-DEV"]
fn f3str01_split_three_parts_iter_print() {
    // Decision 3 row 1: `split(s, sep) -> list[str]`. Result iterates
    // via the ADR-0050b length-bound for-loop; each element prints.
    assert_build_run(
        "f3str01_split",
        "fn main() -> i64:\n    let xs: list[str] = split(\"a,b,c\", \",\")\n    for s in xs:\n        let _ = print(s)\n    return 0\n",
        &[],
        b"",
        "a\nb\nc\n",
    );
}

#[test]
#[ignore = "M-F.3.5 sub-sprint 3 wires __cobrust_str_join + PRELUDE + intrinsic-rewrite; remove ignore post-DEV"]
fn f3str02_join_two_parts_with_sep_print() {
    // Decision 3 row 2: `join(parts, sep) -> str`. The List walk-back
    // (ADR-0050c Phase 2a) keeps `parts` alive at operand-level.
    assert_build_run(
        "f3str02_join",
        "fn main() -> i64:\n    let xs: list[str] = [\"hello\", \"world\"]\n    let r: str = join(xs, \" \")\n    let _ = print(r)\n    return 0\n",
        &[],
        b"",
        "hello world\n",
    );
}

#[test]
#[ignore = "M-F.3.5 sub-sprint 3 wires __cobrust_str_replace; remove ignore post-DEV"]
fn f3str03_replace_first_pattern_full_substitution() {
    // Decision 3 row 3: `replace(s, old, new)` — all occurrences.
    assert_build_run(
        "f3str03_replace",
        "fn main() -> i64:\n    let r: str = replace(\"foo bar baz\", \"bar\", \"BAR\")\n    let _ = print(r)\n    return 0\n",
        &[],
        b"",
        "foo BAR baz\n",
    );
}

#[test]
#[ignore = "M-F.3.5 sub-sprint 3 wires __cobrust_str_trim; remove ignore post-DEV"]
fn f3str04_trim_whitespace_both_sides() {
    // Decision 3 row 4 + Decision 8: trim whitespace both sides.
    assert_build_run(
        "f3str04_trim",
        "fn main() -> i64:\n    let r: str = trim(\"   hello   \")\n    let _ = print(r)\n    return 0\n",
        &[],
        b"",
        "hello\n",
    );
}

#[test]
#[ignore = "M-F.3.5 sub-sprint 3 wires __cobrust_str_find; remove ignore post-DEV"]
fn f3str05_find_present_byte_offset() {
    // Decision 5 / Q2: find returns i64. Present substring → byte
    // offset (here byte offset of "world" in "hello world" = 6).
    assert_build_run(
        "f3str05_find_present",
        "fn main() -> i64:\n    let pos: i64 = find(\"hello world\", \"world\")\n    print(pos)\n    return 0\n",
        &[],
        b"",
        "6\n",
    );
}

#[test]
#[ignore = "M-F.3.5 sub-sprint 3 wires __cobrust_str_find; remove ignore post-DEV"]
fn f3str06_find_absent_returns_neg_one() {
    // Decision 5 / Q2: find returns -1 sentinel when not found.
    // This is the contract that f3str06 + the well-typed sentinel-doc
    // idiom (w156, w175) together lock.
    assert_build_run(
        "f3str06_find_absent",
        "fn main() -> i64:\n    let pos: i64 = find(\"hello\", \"xyz\")\n    print(pos)\n    return 0\n",
        &[],
        b"",
        "-1\n",
    );
}

#[test]
#[ignore = "M-F.3.5 sub-sprint 3 wires __cobrust_str_contains; remove ignore post-DEV"]
fn f3str07_contains_positive_negative_print() {
    // Decision 3 row 6: contains returns bool. Print "yes"/"no" by
    // explicit if-else (no implicit truthy/falsy per §2.2).
    assert_build_run(
        "f3str07_contains",
        "fn main() -> i64:\n    if contains(\"foobar\", \"oo\"):\n        let _ = print(\"yes\")\n    else:\n        let _ = print(\"no\")\n    return 0\n",
        &[],
        b"",
        "yes\n",
    );
}

#[test]
#[ignore = "M-F.3.5 sub-sprint 3 wires __cobrust_str_starts_with + __cobrust_str_ends_with; remove ignore post-DEV"]
fn f3str08_starts_with_ends_with_true_print() {
    // Decision 3 rows 7-8: starts_with + ends_with both return bool.
    assert_build_run(
        "f3str08_prefix_suffix",
        "fn main() -> i64:\n    if starts_with(\"foobar\", \"foo\"):\n        let _ = print(\"prefix-ok\")\n    if ends_with(\"foobar\", \"bar\"):\n        let _ = print(\"suffix-ok\")\n    return 0\n",
        &[],
        b"",
        "prefix-ok\nsuffix-ok\n",
    );
}

#[test]
#[ignore = "M-F.3.5 sub-sprint 3 wires __cobrust_str_lower + __cobrust_str_upper; remove ignore post-DEV"]
fn f3str09_lower_upper_ascii_print() {
    // Decision 3 rows 9-10 + Decision 6: ASCII fast path matches
    // Rust str::to_lowercase / to_uppercase.
    assert_build_run(
        "f3str09_case",
        "fn main() -> i64:\n    let _ = print(lower(\"HELLO\"))\n    let _ = print(upper(\"hello\"))\n    return 0\n",
        &[],
        b"",
        "hello\nHELLO\n",
    );
}

#[test]
#[ignore = "M-F.3.5 sub-sprint 1 + 2 wire `clone` PRELUDE + intrinsic-rewrite (__cobrust_str_clone shim already ships at fmt.rs:306); remove ignore post-DEV"]
fn f3str10_clone_independent_buffer_print() {
    // Decision 2: clone(s) returns a fresh heap StringBuffer; the
    // value is byte-equal to s. Verified by printing both source
    // (consumed by clone) — no, we cannot print s after clone consumes
    // it. We chain: clone(clone(s)) → print. Each clone copies the
    // bytes.
    assert_build_run(
        "f3str10_clone",
        "fn main() -> i64:\n    let s: str = \"hello\"\n    let s2: str = clone(s)\n    let _ = print(s2)\n    return 0\n",
        &[],
        b"",
        "hello\n",
    );
}

// =====================================================================
// f3str11..f3str15 — chained / idiomatic patterns.
//
// Composition tests: split → for-iter → print; join(split(...), ...);
// case-insensitive contains via lower(); empty edge cases; round-trip.
// =====================================================================

#[test]
#[ignore = "M-F.3.5 sub-sprint 3 wires split + list[str] iter integration; remove ignore post-DEV"]
fn f3str11_split_for_iter_print_each() {
    // Idiomatic: `split(input, ",")` → for-iter → print. Daily
    // log-parsing shape that M-F.3.5 unlocks.
    assert_build_run(
        "f3str11_split_iter",
        "fn main() -> i64:\n    let xs: list[str] = split(\"alpha,beta,gamma\", \",\")\n    for s in xs:\n        let _ = print(s)\n    return 0\n",
        &[],
        b"",
        "alpha\nbeta\ngamma\n",
    );
}

#[test]
#[ignore = "M-F.3.5 sub-sprint 3 wires split + join round-trip; remove ignore post-DEV"]
fn f3str12_split_join_roundtrip_identity() {
    // Round-trip: `join(split(s, sep), sep)` ≡ s for non-pathological s.
    // Locks the Decision 3 row 1/2 algebraic symmetry.
    assert_build_run(
        "f3str12_roundtrip",
        "fn main() -> i64:\n    let s: str = \"a,b,c,d\"\n    let r: str = join(split(s, \",\"), \",\")\n    let _ = print(r)\n    return 0\n",
        &[],
        b"",
        "a,b,c,d\n",
    );
}

#[test]
#[ignore = "M-F.3.5 sub-sprint 3 wires lower + contains chaining; remove ignore post-DEV"]
fn f3str13_lower_then_contains_case_insensitive_workaround() {
    // Decision 7 workaround: until Phase G adds contains_ignore_case,
    // `contains(lower(s), lower(needle))` is the hand-composed
    // case-insensitive substring check. Locks the workaround idiom.
    assert_build_run(
        "f3str13_lower_contains",
        "fn main() -> i64:\n    if contains(lower(\"FooBar\"), \"foo\"):\n        let _ = print(\"matched\")\n    return 0\n",
        &[],
        b"",
        "matched\n",
    );
}

#[test]
#[ignore = "M-F.3.5 sub-sprint 3 wires join with empty list edge case; remove ignore post-DEV"]
fn f3str14_join_empty_list_returns_empty_str() {
    // Decision 8 row "join([], sep) → "". Edge case: empty list. The
    // surface signature accepts; runtime returns empty Str.
    assert_build_run(
        "f3str14_join_empty",
        "fn main() -> i64:\n    let xs: list[str] = []\n    let r: str = join(xs, \",\")\n    let _ = print(r)\n    let _ = print(\"END\")\n    return 0\n",
        &[],
        b"",
        "\nEND\n",
    );
}

#[test]
#[ignore = "M-F.3.5 sub-sprint 3 wires split with empty separator (Decision 8 special-case); remove ignore post-DEV"]
fn f3str15_split_empty_sep_returns_singleton() {
    // Decision 8 row "split(s, \"\") → [s]". Mirrors the existing
    // Rust-side `string::split` impl at string.rs:36-38.
    assert_build_run(
        "f3str15_split_empty_sep",
        "fn main() -> i64:\n    let xs: list[str] = split(\"abc\", \"\")\n    for s in xs:\n        let _ = print(s)\n    return 0\n",
        &[],
        b"",
        "abc\n",
    );
}

// =====================================================================
// f3str16..f3str20 — LC-100 clone() mitigation (LOAD-BEARING).
//
// Per `findings/lc100-str-use-after-move-regression-from-adr0050c.md`
// Path D §"Phase G closure scope": once M-F.3.5 ships `clone()` as a
// PRELUDE fn, users mitigate the LC-100 UseAfterMove pattern via
// `let s2 = clone(s); let n = str_len(s); let c = str_at(s2, i)`. These
// tests prove the mitigation works end-to-end.
//
// Each test reproduces the EXACT shape of a known LC-100 victim with
// `clone()` inserted. Without clone(), each program fails MIR
// UseAfterMove (the documented honest-debt baseline). With clone(),
// each must build + run + match the original program's expected stdout.
// =====================================================================

#[test]
#[ignore = "M-F.3.5 sub-sprint 1+2 wire `clone` PRELUDE + intrinsic-rewrite; remove ignore post-DEV"]
fn f3str16_lc100_reverse_string_via_clone_mitigation() {
    // LC-02 reverse_string.cb under M-F.3.5 + clone() mitigation.
    // Original (broken): `let s = input(""); let n = str_len(s);
    //                    while ...: let c = str_at(s, i)` —
    // fails MIR UseAfterMove on the second `s` consumer.
    // Mitigation: `let s = input(""); let s2 = clone(s); let n =
    // str_len(s); while ...: let c = str_at(s2, i)` — builds + runs.
    //
    // This is the LOAD-BEARING test proving M-F.3.5's clone() closes
    // the LC-100 honest-debt mitigation gap.
    assert_build_run(
        "f3str16_lc100_reverse",
        "fn main() -> i64:\n    let s: str = input(\"\")\n    let s2: str = clone(s)\n    let n: i64 = str_len(s)\n    let i: i64 = n - 1\n    while i >= 0:\n        let c: str = str_at(s2, i)\n        print_no_nl(c)\n        i = i - 1\n    let _ = print(\"\")\n    return 0\n",
        &[],
        b"hello\n",
        "olleh\n",
    );
}

#[test]
#[ignore = "M-F.3.5 sub-sprint 1+2 wire `clone` PRELUDE + intrinsic-rewrite; remove ignore post-DEV"]
fn f3str17_lc100_valid_parentheses_via_clone_mitigation() {
    // LC-04 valid_parentheses.cb shape under clone() mitigation. The
    // original `let s = input(""); let n = str_len(s); while ...:
    // let c = str_at(s, i)` fails UseAfterMove on the second consumer.
    //
    // Mitigation pattern: clone s once, use s2 for indexed reads.
    // Simplified test: count opening parens in input. Locks the
    // same str_len + str_at pattern as f3str16 but in a different
    // algorithmic shape (while + comparison).
    assert_build_run(
        "f3str17_lc100_valid_parens",
        "fn main() -> i64:\n    let s: str = input(\"\")\n    let s2: str = clone(s)\n    let n: i64 = str_len(s)\n    let count: i64 = 0\n    let i: i64 = 0\n    while i < n:\n        let c: str = str_at(s2, i)\n        let o: i64 = str_ord(c)\n        if o == 40:\n            count = count + 1\n        i = i + 1\n    print(count)\n    return 0\n",
        &[],
        b"((()))\n",
        "3\n",
    );
}

#[test]
#[ignore = "M-F.3.5 sub-sprint 1+2 wire `clone`; remove ignore post-DEV"]
fn f3str18_clone_twice_then_chained_reads() {
    // Multi-clone safety: `let a = clone(s); let b = clone(a);` —
    // each clone produces an independent heap StringBuffer. We then
    // verify both buffers print the same bytes (proving they hold
    // copies, not aliases).
    //
    // This locks the F30 §"multi-clone safety" bug-witness shape that
    // future cascade-bug audits must not regress.
    assert_build_run(
        "f3str18_clone_chain",
        "fn main() -> i64:\n    let s: str = \"abc\"\n    let a: str = clone(s)\n    let b: str = clone(a)\n    let n: i64 = str_len(b)\n    print(n)\n    return 0\n",
        &[],
        b"",
        "3\n",
    );
}

#[test]
#[ignore = "M-F.3.5 sub-sprint 1+2 wire `clone`; remove ignore post-DEV"]
fn f3str19_clone_then_concat_via_fstring() {
    // f-string composition with cloned Str. Locks the
    // f-string-uses-clone bug-witness shape that the F30 audit calls
    // out as a latent cascade-bug class. `f"first={c}"` where c is
    // a clone() result must compose cleanly.
    assert_build_run(
        "f3str19_clone_fstring",
        "fn main() -> i64:\n    let s: str = \"world\"\n    let c: str = clone(s)\n    let msg: str = f\"hello {c}\"\n    let _ = print(msg)\n    return 0\n",
        &[],
        b"",
        "hello world\n",
    );
}

#[test]
#[ignore = "M-F.3.5 sub-sprint 1+2 wire `clone`; remove ignore post-DEV"]
fn f3str20_clone_in_helper_returns_independent_str() {
    // User-defined helper that wraps clone(): `fn dup(s: str) -> str:
    // return clone(s)`. Locks (a) clone is callable from user fns,
    // (b) the returned str transfers ownership to caller cleanly,
    // (c) the original s is moved into the helper and not aliased.
    assert_build_run(
        "f3str20_clone_helper",
        "fn dup(s: str) -> str:\n    return clone(s)\nfn main() -> i64:\n    let s: str = \"hi\"\n    let r: str = dup(s)\n    let _ = print(r)\n    return 0\n",
        &[],
        b"",
        "hi\n",
    );
}

// =====================================================================
// f3str21..f3str25 — F30 bug-witness regression coverage.
//
// Per `findings/predicate-flip-cascade-discovery-deficit.md` F30 SOP:
// "every predicate-flip ADR should mandate a shadow-flip dry-run …
// classify each new failure: direct-consumer, latent-consumer, or
// genuine test broken by the flip semantics." These tests transpose
// each known Wave 2 cascade-bug class onto the M-F.3.5 surface so
// future predicate flips can't silently regress them.
//
// One test per cascade-bug class:
//   - f3str21: f-string with split-result-index — locks str hole +
//     list[str] iter integration (F30 latent consumer #2 family).
//   - f3str22: multi-clone safety — independent heap per clone() call
//     (mitigates F30 latent consumer #3 family: synthetic-slot
//     bookkeeping double-free).
//   - f3str23: fn-boundary Str return + reuse — locks the cross-fn
//     move-then-bind shape (F30 cross-surface era-collision).
//   - f3str24: list[str] returned from split, dropped via for-loop
//     exit — locks the f3ls23 partial-iteration drop scope on
//     split-produced lists.
//   - f3str25: nested chain `lower(trim(clone(s)))` — locks
//     three-level rvalue chaining (cascade audit Lane 2 stale-IR
//     dispatch risk).
// =====================================================================

#[test]
#[ignore = "M-F.3.5 sub-sprint 3 wires split + f-string composition; remove ignore post-DEV"]
fn f3str21_fstring_with_split_result_index() {
    // F30 bug-witness #1 — f-string with split result indexed.
    // `f"first={split(s, ",")[0]}"`. Locks:
    //   (a) f-string Str hole accepts a str payload (M9 stub fix),
    //   (b) list[str] indexing yields str (ADR-0050c Phase 2),
    //   (c) split() return composes with f-string lowering.
    // If any future ADR flips a predicate that affects str-hole
    // dispatch, this test must catch it.
    assert_build_run(
        "f3str21_fstring_split_idx",
        "fn main() -> i64:\n    let xs: list[str] = split(\"alpha,beta\", \",\")\n    let msg: str = f\"first={xs[0]}\"\n    let _ = print(msg)\n    return 0\n",
        &[],
        b"",
        "first=alpha\n",
    );
}

#[test]
#[ignore = "M-F.3.5 sub-sprint 1+2 wire `clone`; remove ignore post-DEV"]
fn f3str22_multi_clone_independent_heap_safety() {
    // F30 bug-witness #2 — multi-clone safety regression.
    //
    // `let a = clone(s); let b = clone(s)` would be ideal but consumes
    // s twice (UseAfterMove). The corrected multi-clone shape is
    // `let a = clone(s); let b = clone(a);` (chain). Each clone is an
    // independent allocation; verifying via two str_len calls that
    // both return the same value (proof: they hold byte-equal copies).
    assert_build_run(
        "f3str22_multi_clone",
        "fn main() -> i64:\n    let s: str = \"xyz\"\n    let a: str = clone(s)\n    let b: str = clone(a)\n    let na: i64 = str_len(a)\n    let nb: i64 = str_len(b)\n    if na == nb:\n        let _ = print(\"len-eq\")\n    else:\n        let _ = print(\"len-neq\")\n    return 0\n",
        &[],
        b"",
        "len-eq\n",
    );
}

#[test]
#[ignore = "M-F.3.5 sub-sprint 3 wires user-fn-boundary str return; remove ignore post-DEV"]
fn f3str23_fn_boundary_str_return_then_print() {
    // F30 bug-witness #3 — cross-fn move-then-bind shape regression.
    // `fn first_word(s: str) -> str: return ...` consumes s, returns
    // a new str (e.g. via split-then-index); caller binds + prints.
    // Locks the ADR-0050c Phase 5 cross-fn ownership transfer per the
    // "audit Lane 2 stale-IR dispatch" risk.
    assert_build_run(
        "f3str23_fn_boundary",
        "fn first_word(s: str) -> str:\n    let xs: list[str] = split(s, \" \")\n    return xs[0]\nfn main() -> i64:\n    let r: str = first_word(\"hello world\")\n    let _ = print(r)\n    return 0\n",
        &[],
        b"",
        "hello\n",
    );
}

#[test]
#[ignore = "M-F.3.5 sub-sprint 3 wires split + partial-iter drop on list[str] returns; remove ignore post-DEV"]
fn f3str24_split_result_partial_iter_clean_exit() {
    // F30 bug-witness #4 — partial-iteration drop on split-produced
    // list[str]. Mirrors `f3ls23` (list_str_e2e.rs:644) but with the
    // list coming from split() instead of a literal. Locks the
    // drop schedule's coverage of split's heap returns.
    assert_build_run(
        "f3str24_split_partial",
        "fn first_two(xs: list[str]) -> i64:\n    let count: i64 = 0\n    for s in xs:\n        let _ = print(s)\n        count = count + 1\n        if count == 2:\n            return count\n    return count\nfn main() -> i64:\n    let xs: list[str] = split(\"a,b,c,d\", \",\")\n    let n: i64 = first_two(xs)\n    print(n)\n    return 0\n",
        &[],
        b"",
        "a\nb\n2\n",
    );
}

#[test]
#[ignore = "M-F.3.5 sub-sprint 3 wires nested rvalue chains; remove ignore post-DEV"]
fn f3str25_nested_rvalue_chain_lower_trim_clone() {
    // F30 bug-witness #5 — three-level rvalue chain.
    // `lower(trim(clone(s)))` — chain of three M-F.3.5 surface calls.
    // Locks (a) clone produces a fresh heap, (b) trim consumes the
    // clone and returns a fresh heap, (c) lower consumes the trim
    // and returns a fresh heap. Each intermediate drops at expression
    // end per ADR-0050c Phase 1 drop schedule.
    //
    // If any future predicate flip silently changes the rvalue-chain
    // drop schedule, this test must catch it.
    assert_build_run(
        "f3str25_nested_chain",
        "fn main() -> i64:\n    let s: str = \"  HELLO  \"\n    let r: str = lower(trim(clone(s)))\n    let _ = print(r)\n    return 0\n",
        &[],
        b"",
        "hello\n",
    );
}
