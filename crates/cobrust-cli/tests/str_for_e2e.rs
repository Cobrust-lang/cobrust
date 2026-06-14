//! F88 / ADR-0101 end-to-end corpus: `for c in <str>:` iterates a string
//! CODEPOINT-by-CODEPOINT, binding each `c` to a fresh 1-codepoint owned
//! `str` (CPython semantics). §2.5 LLM-first idiom-overlap win.
//!
//! Background (docs/agent/findings/f88-str-for-codepoint-iteration.md):
//! before F88 `for c in "hi":` was a clean TYPE_ERROR (Phase-G-deferred per
//! ADR-0050b §"Iter source type checking") — never a silent miscompile. F88
//! lifts that deferral. The MIR lowers a str iter source to a length-bound
//! index walk whose bound is the CODEPOINT count (`__cobrust_str_char_count`,
//! NOT byte len) and whose per-iteration value is `__cobrust_str_char_at(s,
//! idx)` (codepoint-addressed, F79/ADR-0094) — so a multi-byte char yields
//! exactly ONE iteration.
//!
//! WATCHDOG DISCIPLINE (critical): str-for inherits the SAME length-bound
//! index iteration as the list arm, so it also inherits the F89/ADR-0100
//! `continue` increment latch. A `continue`-in-str-for regression must FAIL
//! the test, NOT stall CI — every run goes through `run_with_timeout`, which
//! kills + FAILS an exe that does not exit within `RUN_TIMEOUT`. The
//! `e07`/`e08` cases prove `continue` over a str loop terminates.
//!
//! ORACLE: CPython `for c in s: print(c)` prints one codepoint per line.
//! Note `len(str)` still returns the BYTE count (a separate pre-existing
//! divergence, NOT F88's scope) — these tests assert ITERATION COUNT via the
//! number of printed lines, which IS codepoint-accurate.
//!
//! 18-lint clippy allow header per `feedback_p9_clippy_stall_pattern.md`.

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
#![allow(clippy::too_many_lines)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::len_zero)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::no_effect_underscore_binding)]
#![allow(clippy::unnecessary_debug_formatting)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// Watchdog bound. These loops walk a handful of codepoints (the longest is
/// a 1000-char string); a correct exe exits in well under a second. A hang
/// (a `continue`-latch regression inherited from the list arm) trips this.
const RUN_TIMEOUT: Duration = Duration::from_secs(15);

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

struct Built {
    _temp: tempfile::TempDir,
    exe: PathBuf,
}

fn build_program(name: &str, src: &str) -> Built {
    let temp = tempfile::tempdir().expect("tempdir");
    let src_path = temp.path().join(format!("{name}.cb"));
    std::fs::write(&src_path, src).expect("write src");
    let exe = temp.path().join(name);
    let bin = cobrust_binary();
    let out = Command::new(&bin)
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&exe)
        .arg("--quiet")
        .current_dir(workspace_root())
        .output()
        .expect("invoke cobrust build");
    assert!(
        out.status.success(),
        "cobrust build failed for {name}:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Built { _temp: temp, exe }
}

/// Spawn `exe`, poll for exit, and KILL + FAIL if it does not terminate
/// within `RUN_TIMEOUT`. Returns trimmed stdout on success.
fn run_with_timeout(name: &str, exe: &Path) -> String {
    use std::fs::File;
    let temp = exe.parent().expect("exe parent");
    let stdout_path = temp.join(format!("{name}.stdout"));
    let stderr_path = temp.join(format!("{name}.stderr"));
    let out_file = File::create(&stdout_path).expect("create stdout file");
    let err_file = File::create(&stderr_path).expect("create stderr file");

    let mut child = Command::new(exe)
        .stdout(out_file)
        .stderr(err_file)
        .spawn()
        .expect("spawn exe");

    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            let stdout = std::fs::read_to_string(&stdout_path).unwrap_or_default();
            let stderr = std::fs::read_to_string(&stderr_path).unwrap_or_default();
            assert!(
                status.success(),
                "{name}: exe exited non-zero ({status:?})\nstdout={stdout}\nstderr={stderr}"
            );
            return stdout.trim_end_matches('\n').to_string();
        }
        if start.elapsed() >= RUN_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "{name}: exe did NOT terminate within {:?} — HANG \
                 (`for c in <str>:` is not advancing its index)",
                RUN_TIMEOUT
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Build, run-under-watchdog, and assert the exact stdout. Termination is
/// implicitly asserted by `run_with_timeout` (a hang panics).
fn assert_program(name: &str, src: &str, expected: &str) {
    let built = build_program(name, src);
    let stdout = run_with_timeout(name, &built.exe);
    assert_eq!(
        stdout,
        expected.trim_end_matches('\n'),
        "{name}: stdout mismatch\nsrc:\n{src}"
    );
}

// =====================================================================
// Section A — ASCII codepoint iteration (the textbook idiom)
// =====================================================================

#[test]
fn e01_ascii_literal_prints_one_codepoint_per_line() {
    // CPython: `for c in "hi": print(c)` → "h\ni\n".
    let src = "fn main() -> i64:\n    for c in \"hi\":\n        print(c)\n    return 0\n";
    assert_program("e01_ascii", src, "h\ni");
}

#[test]
fn e02_ascii_five_chars() {
    // CPython: `for c in "hello": print(c)` → h,e,l,l,o.
    let src = "fn main() -> i64:\n    for c in \"hello\":\n        print(c)\n    return 0\n";
    assert_program("e02_hello", src, "h\ne\nl\nl\no");
}

#[test]
fn e03_iter_over_str_name_binding() {
    // The iter source is a NAME (`for c in s:`). F88 reads `s` as a
    // shared BORROW (Operand::Copy, not Move) so it stays usable after
    // the loop — the second loop below re-iterates the SAME `s`.
    let src = "fn main() -> i64:\n    let s: str = \"abc\"\n    for c in s:\n        print(c)\n    for c in s:\n        print(c)\n    return 0\n";
    assert_program("e03_name_reuse", src, "a\nb\nc\na\nb\nc");
}

// =====================================================================
// Section B — multi-byte codepoints: the LOAD-BEARING test
// =====================================================================

#[test]
fn e04_multibyte_e_acute_is_one_codepoint() {
    // CPython: `for c in "héllo"` visits h, é, l, l, o — FIVE iterations,
    // NOT six. `é` (U+00E9) is 2 BYTES in UTF-8 but ONE codepoint; the
    // loop bound is `__cobrust_str_char_count` (codepoint count), so the
    // multi-byte char yields exactly one iteration with `c == "é"`.
    let src = "fn main() -> i64:\n    for c in \"héllo\":\n        print(c)\n    return 0\n";
    assert_program("e04_he_acute", src, "h\né\nl\nl\no");
}

#[test]
fn e05_multibyte_iteration_count_equals_codepoint_count() {
    // Count the iterations of "héllo" via an accumulator: it MUST be 5
    // (codepoint count), NOT 6 (byte length). This is the codepoint-vs-byte
    // guarantee stated numerically, independent of the printed glyph.
    let src = "fn main() -> i64:\n    let n: i64 = 0\n    for c in \"héllo\":\n        n = n + 1\n    print(n)\n    return 0\n";
    assert_program("e05_count", src, "5");
}

#[test]
fn e06_cjk_codepoints_each_one_iteration() {
    // Each CJK char is 3 UTF-8 bytes but ONE codepoint. "你好" → 2 lines.
    let src = "fn main() -> i64:\n    let n: i64 = 0\n    for c in \"你好\":\n        print(c)\n        n = n + 1\n    print(n)\n    return 0\n";
    assert_program("e06_cjk", src, "你\n好\n2");
}

// =====================================================================
// Section C — `continue` over a str loop must TERMINATE (F89 inherited)
// =====================================================================

#[test]
fn e07_continue_in_str_for_skips_and_terminates() {
    // CPython: `for c in "hello": if c == "l": continue; print(c)`
    // → h, e, o. The str loop reuses the list arm's increment LATCH
    // (F89/ADR-0100), so `continue` advances the codepoint index and the
    // loop TERMINATES (the watchdog fails a regression-hang).
    let src = "fn main() -> i64:\n    for c in \"hello\":\n        if c == \"l\":\n            continue\n        print(c)\n    return 0\n";
    assert_program("e07_continue", src, "h\ne\no");
}

#[test]
fn e08_continue_every_codepoint_terminates() {
    // `continue` on EVERY codepoint: body prints nothing, but the loop must
    // still advance and TERMINATE (the purest hang shape). Prints only the
    // post-loop sentinel.
    let src = "fn main() -> i64:\n    for c in \"abcd\":\n        continue\n        print(c)\n    print(-1)\n    return 0\n";
    assert_program("e08_continue_all", src, "-1");
}

// =====================================================================
// Section D — edge cases: empty string, loop-var usability, drop safety
// =====================================================================

#[test]
fn e09_empty_string_zero_iterations() {
    // CPython: `for c in "":` runs the body 0 times. The post-loop print
    // is the only output.
    let src =
        "fn main() -> i64:\n    for c in \"\":\n        print(c)\n    print(-1)\n    return 0\n";
    assert_program("e09_empty", src, "-1");
}

#[test]
fn e10_loop_var_is_usable_one_codepoint_str() {
    // Each `c` is a real 1-codepoint `str`: it concatenates (`c + "!"`)
    // and has `len(c) == 1` (ASCII byte len == codepoint len here). Proves
    // the loop var is a fully-formed owned str, not a raw scalar.
    let src = "fn main() -> i64:\n    for c in \"xy\":\n        print(c + \"!\")\n        print(len(c))\n    return 0\n";
    assert_program("e10_usable", src, "x!\n1\ny!\n1");
}

#[test]
fn e11_thousand_codepoints_no_double_free_exits_clean() {
    // Iterate a 1000-char string. Each `c` is a fresh OWNED 1-codepoint
    // str; the source is only READ via char_at (never consumed), so there
    // is NO double-free. (A per-iter LEAK exists under the pre-existing F82
    // loop-body-drop gap — NOT asserted here; this case asserts a CLEAN
    // exit, no abort.) The string is built by repeating "ab" 500×; the
    // accumulator counts exactly 1000 codepoints.
    let src = "fn main() -> i64:\n    let s: str = (\"ab\" * 500)\n    let n: i64 = 0\n    for c in s:\n        n = n + 1\n    print(n)\n    return 0\n";
    assert_program("e11_thousand", src, "1000");
}
