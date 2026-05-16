//! M-F.3.4 — Dict end-to-end corpus (ADR-0050d sub-sprint a TEST half).
//!
//! Sub-sprint a's TEST corpus covers parser + AST + HIR + types for
//! dict literal `{k: v, ...}` and dict indexing `d[k]`. This Tier C
//! E2E file locks the user-facing surface that the codegen-side
//! sub-sprints (c, d, e) implement — every test exercises a
//! `cobrust build` → run → stdout-assert loop, mirroring the
//! `list_str_e2e.rs` precedent (per `feedback_p9_two_phase_dispatch.md`
//! sub-sprint d's verified pattern).
//!
//! Pre-impl status (sub-sprint a TEST corpus baseline at HEAD
//! `0ddcd27`; verified via Mac `cargo test -p cobrust-cli --test
//! dict_e2e --locked`):
//!
//!   - The M12.x stub at `cobrust-stdlib/src/collections.rs:534-636`
//!     ships an untyped `__cobrust_dict_{new,set,get,len,drop}` C-ABI
//!     for `Dict<i64,i64>`. The codegen path that wires
//!     `Aggregate(Dict)` to these symbols is NOT yet wired (a stub
//!     returns null per ADR-0027 §1 deferral and ADR-0050d §A2 table
//!     row "Codegen: Aggregate::Dict Cranelift lowering ... ❌ stub").
//!     Therefore most tests here SHOULD FAIL pre-impl (build OK, run
//!     fails at runtime or stdout mismatch). DEV's sub-sprints c+d+e
//!     close the gap.
//!
//!   - A subset of tests target the lighter sub-sprint a deliverable:
//!     "the existing surface compiles + accepts the documented
//!     syntax" — these pass pre-impl (compile-only `cargo cobrust
//!     check`-style smoke, no run). They live in family `f3d_chk_*`
//!     (post-`f3d20`).
//!
//! Test families:
//!
//! - `f3d01..f3d05` — literal build + index read → print value.
//!   Run-time pass requires sub-sprints c (MIR intrinsic-rewrite for
//!   `__cobrust_dict_get_<K_V>`) + d (typed shims with `indexmap`
//!   backing). Pre-impl: ignored (will FAIL at run).
//!
//! - `f3d06..f3d10` — insertion + lookup + len + is_empty. Same
//!   sub-sprint c+d dependency.
//!
//! - `f3d11..f3d15` — `for k, v in d.items()` iteration with
//!   insertion-order semantics (Decision 6A). Sub-sprint e desugar.
//!
//! - `f3d16..f3d20` — `key in d` membership tests + `.get(k)`
//!   fallback. Sub-sprint c (BinOp::In dispatch) + sub-sprint e
//!   (`.get()` intrinsic).
//!
//! - `f3d_chk_*` — pre-impl check-only tests. `cobrust check` exits 0
//!   for dict-surface programs that already type-check at HEAD;
//!   these turn green TODAY.
//!
//! - `f3d_bug_*` — bug-witness regressions per `findings/predicate-flip-
//!   cascade-discovery-deficit.md` F30 SOP. One regression per Wave 2
//!   cascade-bug class, transposed onto dict surface so it can't
//!   regress under future ADR-0050c symmetry walk-backs.
//!
//! Per `feedback_p9_clippy_stall_pattern.md` 2026-05-09:
//! 18-lint clippy module-level allow header at the TOP of every test
//! file authored under this corpus.

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

fn run_check(src: &Path) -> (i32, String) {
    let bin = cobrust_binary();
    let out = Command::new(&bin)
        .arg("check")
        .arg(src)
        .output()
        .expect("invoke cobrust check");
    let code = out.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stderr)
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

fn assert_check_ok(name: &str, src: &str) {
    let path = write_cb(name, src);
    let (code, stderr) = run_check(&path);
    assert_eq!(code, 0, "{name}: check failed; stderr={stderr}");
}

// =====================================================================
// f3d01..f3d05 — literal build + index → print specific value
//
// Each test materialises a `Dict[i64, i64]` literal (the M12.x stub
// shape), reads a value via `d[k]`, and prints it via `print_int`.
// Run-time pass requires sub-sprint c (intrinsic-rewrite for
// `__cobrust_dict_get_i64_i64`) + sub-sprint d (typed shim with
// insertion-order backing).
//
// Pre-impl: ignored (build may emit warning about Dict aggregate
// returning null per ADR-0027 §1 stub, or the Index lowering may
// route to a list-shaped helper that misbehaves at runtime). DEV
// removes the `#[ignore]` after sub-sprints c+d close.
// =====================================================================

#[test]
#[ignore = "sub-sprint c+d wire Aggregate(Dict) codegen + __cobrust_dict_get_i64_i64; remove ignore post-impl"]
fn f3d01_dict_i64_i64_literal_index_read_print() {
    // Three-entry Dict[i64, i64], read d[2] → expect "20\n".
    assert_build_run(
        "f3d01_dict_index_read",
        "fn main() -> i64:\n    let d: Dict[i64, i64] = {1: 10, 2: 20, 3: 30}\n    print_int(d[2])\n    return 0\n",
        &[],
        b"",
        "20\n",
    );
}

#[test]
#[ignore = "sub-sprint c+d wire Aggregate(Dict) codegen + __cobrust_dict_get_i64_i64; remove ignore post-impl"]
fn f3d02_dict_i64_i64_first_key_index() {
    // Read the first inserted entry.
    assert_build_run(
        "f3d02_dict_first_index",
        "fn main() -> i64:\n    let d: Dict[i64, i64] = {7: 70, 8: 80}\n    print_int(d[7])\n    return 0\n",
        &[],
        b"",
        "70\n",
    );
}

#[test]
#[ignore = "sub-sprint c+d wire Aggregate(Dict) codegen; remove ignore post-impl"]
fn f3d03_dict_i64_i64_last_key_index() {
    // Read the last inserted entry — insertion-order semantic per
    // Decision 6A: backing-store choice should not affect lookup.
    assert_build_run(
        "f3d03_dict_last_index",
        "fn main() -> i64:\n    let d: Dict[i64, i64] = {1: 100, 2: 200, 3: 300, 4: 400}\n    print_int(d[4])\n    return 0\n",
        &[],
        b"",
        "400\n",
    );
}

#[test]
#[ignore = "sub-sprint c+d wire Aggregate(Dict) codegen; remove ignore post-impl"]
fn f3d04_dict_i64_i64_arith_two_reads() {
    // Two reads + arith — exercises codegen lifetime of the dict
    // pointer (read twice without intervening drop).
    assert_build_run(
        "f3d04_dict_arith_reads",
        "fn main() -> i64:\n    let d: Dict[i64, i64] = {1: 10, 2: 20}\n    print_int((d[1] + d[2]))\n    return 0\n",
        &[],
        b"",
        "30\n",
    );
}

#[test]
#[ignore = "sub-sprint c+d wire Aggregate(Dict) codegen; remove ignore post-impl"]
fn f3d05_dict_str_i64_literal_index_read_print() {
    // Str-keyed dict — blocks on ADR-0050c Str-keyed shim
    // (`__cobrust_dict_get_str_i64`). Sub-sprint d's str-keyed shape.
    assert_build_run(
        "f3d05_dict_str_index",
        "fn main() -> i64:\n    let d: Dict[str, i64] = {\"a\": 1, \"b\": 2, \"c\": 3}\n    print_int(d[\"b\"])\n    return 0\n",
        &[],
        b"",
        "2\n",
    );
}

// =====================================================================
// f3d06..f3d10 — insertion + lookup + len + is_empty
//
// `d[k] = v` rebind-or-insert (Decision 3A); `dict_len` and
// `dict_is_empty` (Decision 5A + 5-addendum). Pre-impl: same
// codegen-side ignore.
// =====================================================================

#[test]
#[ignore = "sub-sprint c wires __cobrust_dict_set; remove ignore post-impl"]
fn f3d06_dict_insert_new_key_read() {
    // `d["new"] = 99; d["new"]` — write-then-read. Sub-sprint c
    // wires the `dict[k] = v` HIR Stmt::IndexAssign → intrinsic-rewrite.
    assert_build_run(
        "f3d06_dict_insert_read",
        "fn main() -> i64:\n    let d: Dict[i64, i64] = {1: 10}\n    d[2] = 20\n    print_int(d[2])\n    return 0\n",
        &[],
        b"",
        "20\n",
    );
}

#[test]
#[ignore = "sub-sprint c wires rebind; remove ignore post-impl"]
fn f3d07_dict_rebind_existing_key() {
    // `d[k] = new_v` rebinds. Verifies Decision 3A's rebind/insert
    // unification (insert-vs-rebind is decided at runtime by the
    // hashmap backing).
    assert_build_run(
        "f3d07_dict_rebind",
        "fn main() -> i64:\n    let d: Dict[i64, i64] = {1: 10}\n    d[1] = 99\n    print_int(d[1])\n    return 0\n",
        &[],
        b"",
        "99\n",
    );
}

#[test]
#[ignore = "sub-sprint d wires __cobrust_dict_len intrinsic; remove ignore post-impl"]
fn f3d08_dict_len_returns_count() {
    // `len(d)` via the `dict_len` intrinsic. The current M12.x stub
    // ships `__cobrust_dict_len(d) -> i64` but the source-level wire-up
    // (PRELUDE recognition) is sub-sprint e's intrinsic-rewrite work.
    // Pre-impl uses a free-fn stub or fails.
    assert_build_run(
        "f3d08_dict_len",
        "fn main() -> i64:\n    let d: Dict[i64, i64] = {1: 10, 2: 20, 3: 30}\n    print_int(len(d))\n    return 0\n",
        &[],
        b"",
        "3\n",
    );
}

#[test]
#[ignore = "sub-sprint d wires __cobrust_dict_len for the empty case; remove ignore post-impl"]
fn f3d09_dict_empty_len_zero() {
    // `len({})` — the empty literal must produce a non-null empty dict;
    // `dict_len(empty) -> 0`.
    assert_build_run(
        "f3d09_dict_empty_len",
        "fn main() -> i64:\n    let d: Dict[i64, i64] = {}\n    print_int(len(d))\n    return 0\n",
        &[],
        b"",
        "0\n",
    );
}

#[test]
#[ignore = "sub-sprint d ships __cobrust_dict_is_empty; remove ignore post-impl"]
fn f3d10_dict_is_empty_after_insert_returns_false() {
    // `dict_is_empty(d)` returns true/false. The dict_is_empty
    // intrinsic is the Decision-5-addendum surface — sub-sprint d
    // C-ABI + sub-sprint e source-level wiring.
    //
    // Print: "0\n" if False (bool printed as 0/1 per existing
    // print_int dispatch on a bool-typed operand; sub-sprint e
    // confirms the source path).
    assert_build_run(
        "f3d10_dict_is_empty",
        "fn main() -> i64:\n    let d: Dict[i64, i64] = {1: 10}\n    if dict_is_empty(d):\n        print_int(1)\n    else:\n        print_int(0)\n    return 0\n",
        &[],
        b"",
        "0\n",
    );
}

// =====================================================================
// f3d11..f3d15 — `for k, v in d.items()` iteration with insertion order
//
// Decision 6A's insertion-order guarantee is the load-bearing surface
// for the Python 3.7+ wedge audience. These tests are *deliberately*
// expected to produce specific stdout in insertion order; the
// `indexmap::IndexMap` backing in sub-sprint d guarantees this. If
// a refactor accidentally swaps to `std::collections::HashMap`, these
// tests fail (intentional regression detection).
//
// Sub-sprint e desugar shape (per ADR-0050d §"Iteration desugar"):
//   for (k, v) in d.items():
//     body
//   ↓
//   let __it = __cobrust_dict_iter_init(d, 2)  # mode=2: items
//   while __cobrust_dict_iter_next(__it) == 1:
//     let k = __cobrust_dict_iter_key_<K>(__it)
//     let v = __cobrust_dict_iter_val_<V>(__it)
//     body
//   __cobrust_dict_iter_drop(__it)
// =====================================================================

#[test]
#[ignore = "sub-sprint e wires for-loop dict desugar + indexmap insertion order; remove ignore post-impl"]
fn f3d11_for_keys_in_dict_print_keys_insertion_order() {
    // `for k in d:` iterates keys in insertion order.
    assert_build_run(
        "f3d11_for_keys_order",
        "fn main() -> i64:\n    let d: Dict[i64, i64] = {3: 30, 1: 10, 2: 20}\n    for k in d:\n        print_int(k)\n    return 0\n",
        &[],
        b"",
        "3\n1\n2\n",
    );
}

#[test]
#[ignore = "sub-sprint e + d.items() intrinsic; remove ignore post-impl"]
fn f3d12_for_items_in_dict_print_values() {
    // `for k, v in d.items():` — destructure tuple at HIR `Stmt::For`.
    assert_build_run(
        "f3d12_for_items_values",
        "fn main() -> i64:\n    let d: Dict[i64, i64] = {1: 10, 2: 20, 3: 30}\n    for (k, v) in d.items():\n        print_int(v)\n    return 0\n",
        &[],
        b"",
        "10\n20\n30\n",
    );
}

#[test]
#[ignore = "sub-sprint e + d.keys() intrinsic; remove ignore post-impl"]
fn f3d13_for_in_keys_explicit_method() {
    // `for k in d.keys():` — explicit method form, semantically same
    // as `for k in d:` (Decision 6A — keys mode is default).
    assert_build_run(
        "f3d13_for_keys_explicit",
        "fn main() -> i64:\n    let d: Dict[i64, i64] = {10: 1, 20: 2, 30: 3}\n    for k in d.keys():\n        print_int(k)\n    return 0\n",
        &[],
        b"",
        "10\n20\n30\n",
    );
}

#[test]
#[ignore = "sub-sprint e + d.values() intrinsic; remove ignore post-impl"]
fn f3d14_for_in_values_explicit_method() {
    // `for v in d.values():` — iterates values in insertion order.
    assert_build_run(
        "f3d14_for_values_explicit",
        "fn main() -> i64:\n    let d: Dict[i64, i64] = {1: 100, 2: 200, 3: 300}\n    for v in d.values():\n        print_int(v)\n    return 0\n",
        &[],
        b"",
        "100\n200\n300\n",
    );
}

#[test]
#[ignore = "sub-sprint e wires str-keyed iteration + indexmap order; remove ignore post-impl"]
fn f3d15_for_items_str_keyed_dict_print_pair() {
    // Str-keyed dict iteration — blocks on sub-sprint d's str-keyed
    // shim + sub-sprint e's iter desugar.
    assert_build_run(
        "f3d15_for_items_str",
        "fn main() -> i64:\n    let d: Dict[str, i64] = {\"a\": 1, \"b\": 2, \"c\": 3}\n    for (k, v) in d.items():\n        let _ = print(k)\n        print_int(v)\n    return 0\n",
        &[],
        b"",
        "a\n1\nb\n2\nc\n3\n",
    );
}

// =====================================================================
// f3d16..f3d20 — `key in d` membership + `.get(k)` safe lookup
//
// Decision 4A: `key in d -> bool`. Decision 2A: `d[k]` panics on
// missing, `.get(k) -> Option[V]` safe escape. Per ADR-0050d
// §"Surface coverage matrix" caveat: `.get(k) -> Option[V]` may
// scope-cap to a sentinel-pair return at Phase F.3 if typed Option
// is not yet wired; the corpus here writes the Option-shape and
// DEV's sub-sprint e decides which path lands.
// =====================================================================

#[test]
#[ignore = "sub-sprint c wires __cobrust_dict_contains; remove ignore post-impl"]
fn f3d16_key_in_dict_present_returns_true() {
    // `if k in d: print 1 else print 0` — present case.
    assert_build_run(
        "f3d16_in_present",
        "fn main() -> i64:\n    let d: Dict[i64, i64] = {1: 10, 2: 20}\n    if (1 in d):\n        print_int(1)\n    else:\n        print_int(0)\n    return 0\n",
        &[],
        b"",
        "1\n",
    );
}

#[test]
#[ignore = "sub-sprint c wires __cobrust_dict_contains; remove ignore post-impl"]
fn f3d17_key_in_dict_absent_returns_false() {
    // Absent case.
    assert_build_run(
        "f3d17_in_absent",
        "fn main() -> i64:\n    let d: Dict[i64, i64] = {1: 10}\n    if (99 in d):\n        print_int(1)\n    else:\n        print_int(0)\n    return 0\n",
        &[],
        b"",
        "0\n",
    );
}

#[test]
#[ignore = "sub-sprint c wires negated membership via BinOp::NotIn dispatch; remove ignore post-impl"]
fn f3d18_key_not_in_dict_via_unary_not() {
    // `not (k in d)` — unary-not over membership (parser surfaces
    // BinOp::NotIn as a single op at `parser.rs:946`; well_typed
    // corpus revealed a parse-gap on bare-context `not in`. Sub-
    // sprint b sub-task: confirm BinOp::NotIn surface OR keep
    // `not (k in d)` as the canonical workaround).
    assert_build_run(
        "f3d18_not_in_via_unary",
        "fn main() -> i64:\n    let d: Dict[i64, i64] = {1: 10}\n    if not (99 in d):\n        print_int(1)\n    else:\n        print_int(0)\n    return 0\n",
        &[],
        b"",
        "1\n",
    );
}

#[test]
#[ignore = "sub-sprint e wires d.get() intrinsic (Option or sentinel-pair); remove ignore post-impl"]
fn f3d19_dict_get_present_via_intrinsic() {
    // `d.get(k)` for present key returns Some(V) / sentinel-pair
    // (present=True, value). Sub-sprint e ratifies which return shape
    // ships pre-Option lowering. This test uses a sentinel-pair shape
    // workaround (`dict_get_or` with a default arg) that DEV
    // recognizes; if sub-sprint e picks the Option shape, the test
    // graduates to a match-on-Option form.
    assert_build_run(
        "f3d19_dict_get_present",
        "fn main() -> i64:\n    let d: Dict[i64, i64] = {1: 10, 2: 20}\n    print_int(d.get(1, 0))\n    return 0\n",
        &[],
        b"",
        "10\n",
    );
}

#[test]
#[ignore = "sub-sprint e wires d.get() default-fallback; remove ignore post-impl"]
fn f3d20_dict_get_absent_returns_default() {
    // `d.get(absent_k, default)` returns the default — Decision 2A's
    // safe-escape contract. Same scope-cap caveat as f3d19.
    assert_build_run(
        "f3d20_dict_get_absent",
        "fn main() -> i64:\n    let d: Dict[i64, i64] = {1: 10}\n    print_int(d.get(99, -1))\n    return 0\n",
        &[],
        b"",
        "-1\n",
    );
}

// =====================================================================
// f3d_chk_* — pre-impl check-only smoke tests
//
// These tests use `cobrust check` (type-check only, no codegen) on
// dict programs that the HEAD scaffolding already accepts. They turn
// green TODAY and serve as a smoke baseline so DEV can verify nothing
// in sub-sprint b's type-checker amendments accidentally rejects the
// happy-path surface.
// =====================================================================

#[test]
fn f3d_chk01_dict_literal_str_i64_check_ok() {
    // `Dict[str, i64] = {"a":1, "b":2}` already type-checks at HEAD.
    assert_check_ok(
        "f3d_chk01_str_i64_literal",
        "fn f() -> Dict[str, i64]:\n    return {\"a\": 1, \"b\": 2}\n",
    );
}

#[test]
fn f3d_chk02_dict_index_into_str_keyed_check_ok() {
    // `d["a"]` type-checks at HEAD (returns V=i64).
    assert_check_ok(
        "f3d_chk02_index_str_keyed",
        "fn f(d: Dict[str, i64]) -> i64:\n    return d[\"a\"]\n",
    );
}

#[test]
fn f3d_chk03_dict_in_membership_check_ok() {
    // `"a" in d` returns bool at HEAD.
    assert_check_ok(
        "f3d_chk03_in_membership",
        "fn f(d: Dict[str, i64]) -> bool:\n    return (\"a\" in d)\n",
    );
}

#[test]
fn f3d_chk04_dict_comp_check_ok() {
    // `{x: x*x for x in xs}` type-checks at HEAD.
    assert_check_ok(
        "f3d_chk04_dict_comp",
        "fn f(xs: List[i64]) -> Dict[i64, i64]:\n    return {x: (x * x) for x in xs}\n",
    );
}

#[test]
fn f3d_chk05_nested_dict_check_ok() {
    // `Dict[str, Dict[str, i64]]` type-checks at HEAD.
    assert_check_ok(
        "f3d_chk05_nested_dict",
        "fn f() -> Dict[str, Dict[str, i64]]:\n    let inner: Dict[str, i64] = {\"a\": 1}\n    let outer: Dict[str, Dict[str, i64]] = {\"x\": inner}\n    return outer\n",
    );
}

// =====================================================================
// f3d_bug_* — bug-witness regression tests (per F30 SOP)
//
// `findings/predicate-flip-cascade-discovery-deficit.md` recommends:
//   "for each cascade-bug class surfaced in Wave 2, add ≥ 1 regression
//    test that locks the dict equivalent."
//
// Wave 2 cascade-bug classes (per ADR-0050c §"Consequences" closed):
//   1. f-string with Str-typed hole / fix at lower.rs::is_str branch
//      iterator-advance. Locked here as f-string dict-index-of-Str
//      composition: `f"got={d["k"]}"`.
//   2. lower_constant(Str) zero-pointer fix — Str literal copied into
//      dict slot must not be the same zero-page reinterpret bug from
//      Wave 1 W2 reinterpret. Locked here as `d["k"] = v` where
//      v is a Str local.
//   3. Multi-Move dict literal — `{s: s, "x": s}` triple-free fix
//      symmetry. ADR-0050c Phase 2a List walk-back's Str-side
//      symmetry remains the honest-debt baseline.
//   4. Nested-aggregate drop — `Dict[str, List[str]]` must drop the
//      list[str] V cleanly when the dict drops.
//
// Each test correctly fails pre-impl (codegen sub-sprint not closed)
// but documents the WIN gate DEV's sub-sprint d must satisfy at close.
// =====================================================================

#[test]
#[ignore = "sub-sprint c+d wire codegen; bug-witness for F30 fstring + Str-hole + dict[k] composition"]
fn f3d_bug01_fstring_dict_str_index_hole() {
    // F30 bug-witness 1: f-string with `{d["k"]}` Str-typed hole.
    // Wave 2 fix at lower.rs is_str-branch iterator-advance must
    // hold when the f-string hole is a dict index expression.
    assert_build_run(
        "f3d_bug01_fstring_dict_index",
        "fn main() -> i64:\n    let d: Dict[str, str] = {\"k\": \"hello\"}\n    let s: str = f\"got={d[\\\"k\\\"]}\"\n    let _ = print(s)\n    return 0\n",
        &[],
        b"",
        "got=hello\n",
    );
}

#[test]
#[ignore = "sub-sprint c+d wire codegen; bug-witness for F30 lower_constant(Str) via dict insertion"]
fn f3d_bug02_lower_constant_str_via_dict_insert() {
    // F30 bug-witness 2: `let v: str = "literal"; d["k"] = v; print(d["k"])`.
    // Wave 2 fix in lower_constant(Str) for the zero-pointer collision
    // must hold when the Str is dropped into a dict slot rather than
    // a list slot.
    assert_build_run(
        "f3d_bug02_lower_const_dict_insert",
        "fn main() -> i64:\n    let v: str = \"literal\"\n    let d: Dict[str, str] = {}\n    d[\"k\"] = v\n    let _ = print(d[\"k\"])\n    return 0\n",
        &[],
        b"",
        "literal\n",
    );
}

#[test]
#[ignore = "sub-sprint c+d wire codegen; bug-witness for F30 multi-Move dict literal triple-free regression"]
fn f3d_bug03_multi_move_dict_literal_str_str() {
    // F30 bug-witness 3: `{s: s, "x": s}` — same Str local used 3
    // times in literal positions (as key, as value, as second value).
    // Wave 2 triple-free fix in MIR drop-pass must hold when the
    // moves are dict-literal operands rather than list-literal
    // operands.
    //
    // NOTE: this test deliberately AVOIDS the LC-100 honest-debt
    // pattern `let n = dict_len(d); let v = dict_get(d, k)` — the
    // local `s` is moved into the dict literal exactly once at the
    // literal-eval site, then never touched. The dict owns the Strs;
    // drop fires at scope exit cleanly.
    assert_build_run(
        "f3d_bug03_multi_move_dict",
        "fn main() -> i64:\n    let s: str = \"tag\"\n    let d: Dict[str, str] = {s: s, \"x\": s}\n    let _ = print(d[\"x\"])\n    return 0\n",
        &[],
        b"",
        "tag\n",
    );
}

#[test]
#[ignore = "sub-sprint d+f wire nested-aggregate drop; bug-witness for F30 dict[str,list[str]] drop"]
fn f3d_bug04_nested_dict_of_list_str_drop_clean() {
    // F30 bug-witness 4: `Dict[str, List[str]]` — when the outer dict
    // drops at scope exit, every inner `List[str]`'s elements + the
    // list itself must drop. Sub-sprint f wires the recursive drop
    // schedule for `Ty::Dict(K, V)` where V is itself an aggregate.
    //
    // Per ADR-0050d Decision 7A footnote: V ∈ {list[T], dict[K, V]}
    // is Phase G extension; this test is the Phase F.3-late gate
    // (sub-sprint f stretch goal) — DEV decides whether to ship
    // nested-aggregate-V at Phase F.3 close or defer to Phase G.
    assert_build_run(
        "f3d_bug04_nested_drop",
        "fn main() -> i64:\n    let xs: List[str] = [\"a\", \"b\"]\n    let d: Dict[str, List[str]] = {\"k\": xs}\n    let _ = print(\"done\")\n    return 0\n",
        &[],
        b"",
        "done\n",
    );
}
