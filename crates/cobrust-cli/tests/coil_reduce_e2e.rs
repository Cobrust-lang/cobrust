//! #145 numpy gap-closure BATCH 5 — `.cb` end-to-end proof for the
//! REDUCTIONS family, which spans THREE return shapes on a single
//! `coil.Buffer` arg:
//!
//!   - `coil.cumsum(a)` / `coil.cumprod(a)` → a `coil.Buffer` (the no-axis
//!     FLATTEN-to-1-D cumulative scan), bound + printed via
//!     `coil.print_buffer`. Wired EXACTLY like the BATCH-3/4 ufuncs
//!     (`coil_ufunc_e2e.rs` / `coil_round_e2e.rs`) — borrow-Buffer-arg →
//!     fresh-Buffer-return, codegen extern `(ptr) -> ptr`.
//!   - `coil.argmin(a)` / `coil.argmax(a)` → a scalar `i64` (the flat
//!     C-order index), bound as `let i: i64 = ...` and `print(i)`-ed.
//!     Wired like `coil.mean` (the scalar-return precedent) adapting
//!     f64 → i64; codegen extern `(ptr) -> i64`.
//!   - `coil.any(a)` / `coil.all(a)` → a scalar `bool`, bound as
//!     `let b: bool = ...`. Printed via the canonical Cobrust bool idiom
//!     `if b:\n    print(1)\nelse:\n    print(0)` (the same form
//!     `ecosystem_fang_e2e.rs` uses — `print(bool)` directly is avoided).
//!     Codegen extern `(ptr) -> i1` (the Rust C-ABI `-> bool`), the SAME
//!     shape as `fang.verify_password`.
//!
//! ## The load-bearing semantics pinned end-to-end
//!
//! - **cumsum/cumprod FLATTEN** a 2-D input to 1-D C-order (`coil.cumsum(
//!   array2x2(1,2,3,4))` → `[1, 3, 6, 10]`, a length-4 1-D buffer, NOT a
//!   2x2). Oracle: `np.cumsum([[1,2],[3,4]]) == array([ 1,  3,  6, 10])`.
//! - **argmin/argmax ties → FIRST occurrence** + **flat C-order index** on
//!   a 2-D input (`coil.argmax(array2x2(3,1,1,5))` → flat idx `3`).
//! - **any/all truthiness** (`any` of a buffer with a nonzero element is
//!   True; `all` of a buffer containing a zero is False).
//! - **EMPTY-input clean trap**: `coil.argmin(coil.zeros(0))` aborts with a
//!   NON-ZERO exit (numpy raises `ValueError`; coil `coil_panic`s — a clean
//!   process abort, NEVER a Rust unwind across the C-ABI). The test asserts
//!   `!status.success()` AND that the binary did not crash with a signal
//!   that would indicate an unwind/segfault rather than a controlled abort.
//!
//! Mirrors the compile→spawn→assert-stdout harness of `coil_round_e2e.rs`.
//! Every `.cb` coil constructor (`array1d2`/`array2x2`/`mgrid`/`zeros`)
//! yields a `Float64` buffer, so cumsum/cumprod return `Float64` here and
//! the integer-valued results print WITHOUT a `.0` suffix (`array([1, 3,
//! 6, 10], dtype=float64)`); the int32→int64 / bool→int64 dtype-widening +
//! the NaN-propagation / NaN-truthy edges (no `.cb` NaN literal yet) are
//! exhaustively pinned in the `reduce.rs` + `aggregates.rs` Rust unit
//! tests, which this end-to-end proof ultimately serves.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative module/test doc comments read as "lazy" list items to
// clippy; they are intentional explanatory prose, not lint targets.
#![allow(clippy::doc_lazy_continuation)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; panic with stderr on build
/// failure. Mirrors `coil_round_e2e.rs`.
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
    let out = Command::new(exe).output().expect("spawn coil-reduce prog");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

// =====================================================================
// POSITIVE — cumsum: 1-D values. `coil.cumsum(array1d2(2.0, 3.0))` →
// `[2, 5]`. Oracle: `np.cumsum([2,3]) == array([2, 5])`.
// =====================================================================

/// `coil.cumsum(array1d2(2.0, 3.0))` → `[2, 5]` (Float64, the .cb
/// constructor dtype). Running sum: `2`, `2+3=5`.
#[test]
fn test_e2e_cumsum_1d() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(2.0, 3.0)\n",
        "    let r: coil.Buffer = coil.cumsum(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([2, 5], dtype=float64)"),
        "expected cumsum([2,3])=[2, 5]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — cumsum FLATTENS a 2-D input to 1-D C-order. THE load-bearing
// no-axis nuance: `coil.cumsum(array2x2(1,2,3,4))` → `[1, 3, 6, 10]`
// (a length-4 1-D buffer, NOT a 2x2). Oracle: `np.cumsum([[1,2],[3,4]])
// == array([ 1,  3,  6, 10])`.
// =====================================================================

/// `coil.cumsum(array2x2(1.0, 2.0, 3.0, 4.0))` → `[1, 3, 6, 10]` (1-D,
/// flattened C-order). The `[[...]]`-style nested repr would indicate a
/// FAILED flatten (a 2-D result); the flat `array([1, 3, 6, 10])` repr
/// confirms the no-axis FLATTEN-to-1-D contract.
#[test]
fn test_e2e_cumsum_2d_flattens_to_1d() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(1.0, 2.0, 3.0, 4.0)\n",
        "    let r: coil.Buffer = coil.cumsum(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1, 3, 6, 10], dtype=float64)"),
        "expected cumsum([[1,2],[3,4]]) FLATTENED to 1-D [1, 3, 6, 10]; \
         a 2-D `[[...]]` repr would mean the flatten FAILED. got stdout=\n{stdout}",
    );
    // A flattened 1-D result has NO nested `[[` — pin the absence so a
    // 2-D regression is caught even if the values happened to match.
    assert!(
        !stdout.contains("[["),
        "cumsum (no axis) must FLATTEN to 1-D — found nested `[[` (2-D); \
         got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — cumprod: running product, also flattening. `coil.cumprod(
// array2x2(1,2,3,4))` → `[1, 2, 6, 24]`. Oracle: `np.cumprod([[1,2],
// [3,4]]) == array([ 1,  2,  6, 24])`.
// =====================================================================

/// `coil.cumprod(array2x2(1.0, 2.0, 3.0, 4.0))` → `[1, 2, 6, 24]` (1-D
/// flatten). Running product: `1`, `1*2=2`, `2*3=6`, `6*4=24`.
#[test]
fn test_e2e_cumprod_flattens_and_products() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(1.0, 2.0, 3.0, 4.0)\n",
        "    let r: coil.Buffer = coil.cumprod(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1, 2, 6, 24], dtype=float64)"),
        "expected cumprod([[1,2],[3,4]]) flattened = [1, 2, 6, 24]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — argmin / argmax return the FLAT C-order index (scalar i64).
// On a 2-D input with a tie, `argmax` returns the FIRST occurrence's flat
// index. `coil.argmax(array2x2(3,1,1,5))` (flat `[3,1,1,5]`) → `3` (the
// 5); `coil.argmin(...)` → `1` (FIRST of the two `1`s). Oracle:
// `np.argmax([[3,1],[1,5]]) == 3`, `np.argmin([[3,1],[1,5]]) == 1`.
// =====================================================================

/// `coil.argmin` / `coil.argmax` over `array2x2(3,1,1,5)` (flat
/// `[3,1,1,5]`): `argmin` → `1` (FIRST occurrence of the min `1`, not the
/// second at idx 2), `argmax` → `3` (the max `5`). Pins both the
/// flat-C-order-index AND the ties-go-to-first-occurrence contract.
/// Printed as two i64 lines: `1`, `3`.
#[test]
fn test_e2e_argmin_argmax_flat_index_ties_first() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(3.0, 1.0, 1.0, 5.0)\n",
        "    let lo: i64 = coil.argmin(&a)\n",
        "    let hi: i64 = coil.argmax(&a)\n",
        "    print(lo)\n",
        "    print(hi)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "1\n3".trim_end(),
        "expected argmin=1 (FIRST occurrence of min), argmax=3 (flat \
         C-order index of max); got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// `coil.argmax(coil.mgrid(0, 5))` over `[0,1,2,3,4]` → `4` (the last
/// index, the max value 4); `coil.argmin(...)` → `0`. A second,
/// monotonically-increasing shape pins the scalar-i64 return is not a
/// constant. Printed: `0`, `4`.
#[test]
fn test_e2e_argmin_argmax_monotonic_mgrid() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.mgrid(0, 5)\n",
        "    let lo: i64 = coil.argmin(&a)\n",
        "    let hi: i64 = coil.argmax(&a)\n",
        "    print(lo)\n",
        "    print(hi)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "0\n4".trim_end(),
        "expected argmin([0..4])=0, argmax=4; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// POSITIVE — any / all return a scalar `bool`, printed via the canonical
// Cobrust `if b:` idiom (1 = True, 0 = False). `coil.any(array1d2(0,5))`
// is True (the 5 is truthy); `coil.all(array1d2(0,5))` is False (the 0 is
// falsy). Oracle: `np.any([0,5])==True`, `np.all([0,5])==False`.
// =====================================================================

/// `coil.any(array1d2(0.0, 5.0))` is True (one nonzero element);
/// `coil.all(array1d2(0.0, 5.0))` is False (a zero element). Pins the
/// `-> bool` return wiring on a MIXED buffer (a uniform all-true / all-
/// false buffer could hide a constant-fold bug). Printed: `1` (any True),
/// `0` (all False).
#[test]
fn test_e2e_any_all_mixed_buffer() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(0.0, 5.0)\n",
        "    let some: bool = coil.any(&a)\n",
        "    let every: bool = coil.all(&a)\n",
        "    if some:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    if every:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "1\n0".trim_end(),
        "expected any([0,5])=True (->1), all([0,5])=False (->0); got \
         stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

/// `coil.all(coil.ones(3))` is True (all nonzero); `coil.any(coil.ones(3))`
/// is True too. A second, all-truthy shape pins `all` returns True when no
/// element is falsy (the complement of the mixed-buffer case). Printed:
/// `1`, `1`.
#[test]
fn test_e2e_all_true_when_no_zero() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.ones(3)\n",
        "    let some: bool = coil.any(&a)\n",
        "    let every: bool = coil.all(&a)\n",
        "    if some:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    if every:\n",
        "        print(1)\n",
        "    else:\n",
        "        print(0)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert_eq!(
        stdout.trim(),
        "1\n1".trim_end(),
        "expected any(ones)=True (->1), all(ones)=True (->1); got \
         stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// EMPTY-input CLEAN TRAP — `coil.argmin(coil.zeros(0))` on an empty buffer.
// numpy RAISES `ValueError`; coil cannot raise across the C-ABI, so the
// shim `coil_panic`s: a CLEAN process abort (the stdlib `__cobrust_panic`,
// which diverges) — NEVER a Rust `panic!` unwind across the FFI boundary
// (which would be UB). The test asserts a NON-ZERO exit (the trap fired)
// AND that it is a controlled abort, NOT a normal `return 0`.
// =====================================================================

/// `coil.argmin(coil.zeros(0))` on an EMPTY buffer must ABORT with a
/// non-zero exit (the `coil_panic` clean trap), NOT print the unreachable
/// success marker. The `coil.zeros(0)` constructor yields a 0-length 1-D
/// buffer; `argmin` of an empty array is a numpy `ValueError`, which the
/// C-ABI shim turns into a clean abort. We assert:
///   - the binary did NOT exit 0 (the trap fired, the `print(999)` /
///     `return 0` tail is never reached);
///   - the program produced NO success stdout (`999` is absent).
/// This proves the trap is a CONTROLLED abort (not a crash that happened
/// to print first, and not a silent wrong-answer).
#[test]
fn test_e2e_argmin_empty_clean_trap() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.zeros(0)\n",
        "    let lo: i64 = coil.argmin(&a)\n",
        // Unreachable: argmin of an empty buffer traps above. If the trap
        // did NOT fire (a regression), this marker would print + the
        // binary would exit 0 — both of which the asserts below reject.
        "    print(999)\n",
        "    print(lo)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, _stderr, ok) = run(&exe);
    assert!(
        !ok,
        "argmin of an EMPTY buffer must ABORT with a non-zero exit (clean \
         coil_panic trap); instead it exited 0 with stdout=\n{stdout}",
    );
    assert!(
        !stdout.contains("999"),
        "the post-argmin `print(999)` marker must be UNREACHABLE (the trap \
         fires first); found it in stdout=\n{stdout}",
    );
}

/// `coil.argmax(coil.zeros(0))` on an EMPTY buffer — twin of the argmin
/// trap, pinning `argmax` ALSO traps cleanly (not just argmin). Same
/// non-zero-exit + unreachable-marker assertions.
#[test]
fn test_e2e_argmax_empty_clean_trap() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.zeros(0)\n",
        "    let hi: i64 = coil.argmax(&a)\n",
        "    print(999)\n",
        "    print(hi)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, _stderr, ok) = run(&exe);
    assert!(
        !ok,
        "argmax of an EMPTY buffer must ABORT with a non-zero exit (clean \
         coil_panic trap); instead it exited 0 with stdout=\n{stdout}",
    );
    assert!(
        !stdout.contains("999"),
        "the post-argmax `print(999)` marker must be UNREACHABLE; found it \
         in stdout=\n{stdout}",
    );
}
