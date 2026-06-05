//! `coil.array([list])` — `.cb` end-to-end proof for the FUNDAMENTAL numpy
//! constructor `np.array([...])`: the BRIDGE from real `.cb` list data to a
//! coil `Buffer` (parse → list → array → stats). The FIRST coil constructor
//! that CONSUMES a Cobrust `list[T]` argument (every prior ctor — `zeros(n)`
//! / `arange(n)` — is all-scalar-arg). These tests compile → link → spawn
//! REAL binaries (F73) and assert the produced Buffer's `print_buffer` repr +
//! exit code, proving list-data → Buffer construction works END-TO-END.
//!
//! ## The list-CONSUME (borrow-read) reuse (ADR-0090)
//!
//! `coil.array(xs)` REUSES the ADR-0090 list-consume mechanism: the `.cb`
//! list passes by POINTER (`is_copy_type(Ty::List)` — Copy-at-call, the
//! `.cb` scope retains ownership + drops it ONCE at scope exit), the shim
//! BORROWS it via the stdlib `__cobrust_list_len` / `__cobrust_list_get`
//! shared-reference accessors (NEVER `Box::from_raw` / free — EXACTLY like
//! `__cobrust_min_int`). The `list_usable_after_array` test LOCKS the borrow
//! (the list is `len()`-read AFTER `coil.array`, and the program exits 0 —
//! the list dropped exactly once, NO double-free).
//!
//! ## The ELEMENT-DTYPE dispatch (ADR-0089/0090 lesson)
//!
//! The MIR ecosystem-call lowering reads the list arg's STATIC element type
//! (`Ty::List(elem)` via `synth_expr_ty` — the resolved type, NOT a fragile
//! arg-temp read) and retargets onto `__cobrust_coil_array_int` (`list[int]`
//! → int64 Buffer) vs `__cobrust_coil_array_float` (`list[float]` → float64
//! Buffer). The `array_of_computed_int_list` test is the ADR-0089 miscompile
//! proof: a list BUILT in a fn then passed routes by its real element type.
//!
//! ## The load-bearing semantics (numpy 2.x, oracle python3.11 numpy)
//!
//! - `coil.array([1, 2, 3]) == array([1, 2, 3], dtype=int64)`
//!   (`np.array([1,2,3]).dtype == int64`).
//! - `coil.array([1.0, 2.5]) == array([1. , 2.5], dtype=float64)`
//!   (`np.array([1.0,2.5]).dtype == float64`; coil prints whole floats
//!   without the trailing `.0`, a pre-existing repr note — ADR-0089).
//! - `coil.array([]) == array([], dtype=float64)` — an EMPTY list defaults
//!   to float64 (`np.array([]).dtype == float64`); NOT a trap, the binary
//!   exits ZERO. An empty `list[int]` → an empty int64 Buffer.
//! - The CHAIN `coil.mean(coil.array([1, 2, 3, 4])) == 2.5` proves the
//!   produced Buffer flows into coil ops (the parse → array → stats payoff).
//! - A non-list arg (`coil.array(5)`) is a COMPILE-TIME `NotIterable`; a
//!   non-numeric-element list (`coil.array(["a"])`) is a COMPILE-TIME
//!   `TypeMismatch` (the §2.5 compile-time-catch path).
//!
//! The NESTED 2-D form (`coil.array([[1,2],[3,4]])` from a `list[list]`) is a
//! DOCUMENTED DEFERRAL (needs a recursive list read; ADR-0091 ships the 1-D
//! form). Mirrors the compile→spawn→assert-stdout harness of
//! `coil_arange_e2e`.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative doc comments read as "lazy" list items to clippy; they
// are intentional explanatory prose, not lint targets.
#![allow(clippy::doc_lazy_continuation)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; panic with stderr on build
/// failure. Mirrors `coil_arange_e2e.rs`.
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
    let out = Command::new(exe).output().expect("spawn coil-array prog");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// Compile-only helper for negative typecheck cases — returns (success?,
/// stderr).
fn try_build(source: &str) -> (bool, String) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let exe = dir.path().join("prog");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let out = Command::new(&bin)
        .arg("build")
        .arg(&src_path)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// =====================================================================
// POSITIVE — the core proof: `coil.array([1, 2, 3])` is an INT64 buffer.
// The `dtype=int64` assertion fails any Float64 mutation; the exact-value
// assertion fails any slot-read bug.
// =====================================================================

/// `coil.array([1, 2, 3])` → `array([1, 2, 3], dtype=int64)`. THIS is the
/// list-int → Buffer bridge, end-to-end.
///
/// Oracle (numpy 2.x): `np.array([1, 2, 3]) == [1, 2, 3]`, dtype `int64`.
#[test]
fn test_e2e_array_int_list_is_int64() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let xs: list[i64] = [1, 2, 3]\n",
        "    let a: coil.Buffer = coil.array(xs)\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1, 2, 3], dtype=int64)"),
        "expected an int64-dtype Buffer [1, 2, 3] (float64 would diverge from numpy); got stdout=\n{stdout}",
    );
}

/// `coil.array([...])` from an INLINE int literal list (no intermediate
/// binding) — the literal is built fresh, borrowed by the shim, dropped once.
///
/// Oracle (numpy 2.x): `np.array([7, 8, 9]) == [7, 8, 9]`, dtype `int64`.
#[test]
fn test_e2e_array_int_literal_inline() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array([7, 8, 9])\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([7, 8, 9], dtype=int64)"),
        "expected int64 [7, 8, 9] from an inline literal; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `coil.array([1.0, 2.5])` is a FLOAT64 buffer. The float slots
// are stored as `to_bits()` and read back via `from_bits` (ADR-0090). The
// `dtype=float64` assertion fails any int-shim misroute.
// =====================================================================

/// `coil.array([1.0, 2.5])` → `array([1, 2.5], dtype=float64)` (coil prints
/// whole floats without `.0`). THIS is the list-float → Buffer bridge.
///
/// Oracle (numpy 2.x): `np.array([1.0, 2.5])` is float64 `[1. , 2.5]`.
#[test]
fn test_e2e_array_float_list_is_float64() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let xs: list[f64] = [1.0, 2.5]\n",
        "    let a: coil.Buffer = coil.array(xs)\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("dtype=float64"),
        "expected a float64-dtype Buffer (an int-shim misroute would print int64 garbage); got stdout=\n{stdout}",
    );
    assert!(
        stdout.contains("2.5"),
        "expected the fractional value 2.5 preserved (from_bits slot read); got stdout=\n{stdout}",
    );
}

// =====================================================================
// COMPUTED-LIST DISPATCH (ADR-0089 miscompile proof) — a list BUILT in a fn
// then passed to coil.array must route by its REAL element type (int), not a
// fragile arg-temp read. `make_ints()` returns a `list[i64]`; the dispatch
// reads `synth_expr_ty` → Ty::List(Int) and picks the int shim.
// =====================================================================

/// `coil.array(make_ints())` where `make_ints() -> list[i64]` → an int64
/// Buffer. The COMPUTED-list dispatch: the list is built then passed, and the
/// int shim is selected from the resolved element type (NOT the arg's MIR
/// temp). A miscompile here would reinterpret the i64 slots as f64.
///
/// Oracle (numpy 2.x): `np.array([10, 20, 30]) == [10, 20, 30]`, dtype int64.
#[test]
fn test_e2e_array_of_computed_int_list() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn make_ints() -> list[i64]:\n",
        "    let xs: list[i64] = [10, 20, 30]\n",
        "    return xs\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array(make_ints())\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([10, 20, 30], dtype=int64)"),
        "expected int64 [10, 20, 30] from a COMPUTED list (the int shim must be picked from the resolved element type, ADR-0089); got stdout=\n{stdout}",
    );
}

// =====================================================================
// EMPTY — `coil.array([])` is the EMPTY FLOAT64 buffer (numpy's empty-list
// default dtype is float64). A non-erroring zero case (the binary exits
// ZERO). NOT a trap.
// =====================================================================

/// `coil.array([])` → `array([], dtype=float64)` (numpy's empty default).
/// The binary exits ZERO — NOT a trap.
///
/// Oracle (numpy 2.x): `np.array([]) == array([], dtype=float64)`.
#[test]
fn test_e2e_array_empty_is_empty_float64() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array([])\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(
        ok,
        "empty array must EXIT ZERO (empty, not a trap); stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    assert!(
        stdout.contains("array([], dtype=float64)"),
        "expected an empty float64 buffer (numpy's empty-list default dtype); got stdout=\n{stdout}",
    );
}

/// `coil.array(xs)` where `xs: list[i64] = []` → an EMPTY INT64 buffer (the
/// annotated empty list matches the static element type). NOT a trap.
///
/// Oracle (numpy 2.x): `np.array([], dtype=np.int64)` is `array([],
/// dtype=int64)`.
#[test]
fn test_e2e_array_empty_int_list_is_empty_int64() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let xs: list[i64] = []\n",
        "    let a: coil.Buffer = coil.array(xs)\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([], dtype=int64)"),
        "expected an empty int64 buffer (the annotated empty list[int] keeps the int dtype); got stdout=\n{stdout}",
    );
}

// =====================================================================
// CHAIN — the produced Buffer flows into a coil op. `coil.mean(coil.array(
// [1, 2, 3, 4]))` == 2.5. THIS is the parse → array → stats payoff: the
// fresh Buffer is a first-class drop-scheduled handle a downstream reduction
// consumes.
// =====================================================================

/// `coil.mean(coil.array([1, 2, 3, 4]))` → `2.5`. The array → stats chain:
/// the int64 Buffer feeds `coil.mean` (which always returns an f64, like
/// numpy `np.mean([1,2,3,4]) == 2.5`).
///
/// Oracle (numpy 2.x): `np.mean(np.array([1, 2, 3, 4])) == 2.5`.
#[test]
fn test_e2e_array_then_mean_chain() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array([1, 2, 3, 4])\n",
        "    let m: f64 = coil.mean(a)\n",
        "    print(m)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("2.5"),
        "expected mean∘array == 2.5 (the parse→array→stats payoff); got stdout=\n{stdout}",
    );
}

/// `coil.mean(coil.array([1.5, 2.5, 3.5]))` → `2.5`. The float-list array
/// also feeds the stats chain (the from_bits slot read is correct).
///
/// Oracle (numpy 2.x): `np.mean(np.array([1.5, 2.5, 3.5])) == 2.5`.
#[test]
fn test_e2e_array_float_then_mean_chain() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let xs: list[f64] = [1.5, 2.5, 3.5]\n",
        "    let a: coil.Buffer = coil.array(xs)\n",
        "    let m: f64 = coil.mean(a)\n",
        "    print(m)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("2.5"),
        "expected mean∘array(float) == 2.5; got stdout=\n{stdout}",
    );
}

// =====================================================================
// BORROW LOCK — the list is BORROWED by coil.array (read, NOT freed); the
// `.cb` scope drops it ONCE. A `len(xs)` read AFTER `coil.array(xs)` returns
// the right length AND the program exits 0 (the list dropped exactly once —
// a shim-free + scope-drop double-free would crash). Mirrors the ADR-0090
// list-reused-after-reduce lock.
// =====================================================================

/// `coil.array(xs)` then `len(xs)` — the list is reused after the shim. The
/// BORROW lock: the list reads length 3 after coil.array AND the program
/// exits 0 (no double-free).
///
/// Oracle: `len([1, 2, 3]) == 3` after the array build.
#[test]
fn test_e2e_list_usable_after_array() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let xs: list[i64] = [1, 2, 3]\n",
        "    let a: coil.Buffer = coil.array(xs)\n",
        "    let _ = coil.print_buffer(a)\n",
        "    let n: i64 = len(xs)\n",
        "    print(n)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(
        ok,
        "the list must be BORROWED (not freed) by coil.array — a double-free would crash; stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    assert!(
        stdout.contains("array([1, 2, 3], dtype=int64)") && stdout.trim_end().ends_with('3'),
        "expected the array repr AND len(xs)==3 after coil.array (the borrow lock); got stdout=\n{stdout}",
    );
}

// =====================================================================
// NEGATIVE (TYPECHECK) — a non-list arg + a non-numeric-element list are
// rejected at the element-poly special-case (the §2.5 compile-time-catch
// path). coil.array has a runtime empty path but NO error path; these
// compile-time rejects are the negative gates.
// =====================================================================

/// `coil.array(5)` is rejected — `coil.array` needs a list. The canonical
/// `NotIterable` (no new error variant), caught BEFORE codegen.
#[test]
fn test_neg_array_rejects_non_list_arg() {
    let (ok, stderr) = try_build(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array(5)\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "coil.array(5) must be rejected (a list is required); stderr=\n{stderr}"
    );
    assert!(
        stderr.contains("NotIterable"),
        "expected the canonical NotIterable diagnostic; stderr=\n{stderr}"
    );
}

/// `coil.array(["a", "b"])` is rejected — a `list[str]` is neither int nor
/// float. The canonical `TypeMismatch` (the element unifies against Float),
/// caught BEFORE codegen.
#[test]
fn test_neg_array_rejects_str_element_list() {
    let (ok, stderr) = try_build(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let xs: list[str] = [\"a\", \"b\"]\n",
        "    let a: coil.Buffer = coil.array(xs)\n",
        "    let _ = coil.print_buffer(a)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "coil.array([\"a\", \"b\"]) must be rejected (str is not a numeric element); stderr=\n{stderr}"
    );
    assert!(
        stderr.contains("TypeMismatch"),
        "expected the canonical TypeMismatch diagnostic; stderr=\n{stderr}"
    );
}
