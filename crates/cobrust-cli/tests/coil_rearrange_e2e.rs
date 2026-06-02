//! coil REARRANGE / REPEAT ops (`diff` / `flip` / `roll` / `repeat` /
//! `tile`) — `.cb` end-to-end proof for the #145 BATCH-10 addition. Each
//! is a Buffer-RETURNING op over the C-order FLATTENED array, split on
//! arity + the output-shape contract:
//!
//! - `coil.diff(a)` / `coil.flip(a)` are 1-arg, wired EXACTLY like the
//!   BATCH-2 reshape ops (`transpose` / `flatten` / `ravel`) + the unary
//!   ufuncs: borrow-Buffer-arg → fresh-Buffer-return, riding the shared
//!   `cabi::buffer_unary` body + the `coil_shape_ty` `(ptr) -> ptr` extern.
//! - `coil.roll(a, k)` / `coil.repeat(a, n)` / `coil.tile(a, n)` take a
//!   trailing i64 SCALAR — the i64-scalar mirror of the BATCH-6
//!   `coil.clip(a, lo, hi)` / `coil.power(a, p)` f64 scalar, riding a
//!   `(ptr, i64) -> ptr` extern. The `.cb` int literal lowers DIRECTLY as
//!   an i64 (the `EcoSig` param `Ty::Int` — no f64 cast, UNLIKE
//!   `percentile`'s `q`), so there is NO new MIR arm.
//!
//! ## The load-bearing semantics (numpy 2.x, oracle `python3.11`, 2.4.6)
//!
//! - `coil.diff(a)` — `a[1:] - a[:-1]` over the flattened array, 1-D
//!   length `max(size - 1, 0)` (`np.diff([1,4,9,16]) == [3,5,7]`).
//! - `coil.flip(a)` — reverse the flattened array, 1-D same length
//!   reversed (`np.flip([1,2,3]) == [3,2,1]`).
//! - `coil.roll(a, k)` — cyclic shift by `k`, reshaped BACK to the
//!   ORIGINAL shape (`np.roll([1,2,3,4],1) == [4,1,2,3]`; SAME shape).
//! - `coil.repeat(a, n)` — repeat EACH element `n` times, 1-D length
//!   `n * size` (`np.repeat([1,2],2) == [1,1,2,2]`).
//! - `coil.tile(a, n)` — tile the WHOLE flattened array `n` times, 1-D
//!   length `n * size` (`np.tile([1,2],2) == [1,2,1,2]`).
//!
//! All five are DTYPE-PRESERVING; every `.cb` Buffer constructor builds a
//! Float64 buffer, so the printed repr renders `dtype=float64` (the
//! float64 repr prints integer-valued floats WITHOUT a `.0` suffix).
//!
//! ## Where this sits in the chain (reuses the ecosystem-call machinery)
//!
//!   - typecheck `try_synth_ecosystem_call` → `lookup_module_fn("coil",
//!     <op>)` resolves the `Buffer(...) -> Buffer` `EcoSig`;
//!   - MIR `try_lower_ecosystem_call` Case-1 → the GENERIC borrow-arg →
//!     fresh-Buffer-return path (the SAME path `coil.transpose(a)` proves
//!     for the 1-arg ops + `coil.clip(a, lo, hi)` proves for the
//!     scalar-arg ops; NO batch-specific MIR arm, NO `_=>"any"` gap);
//!   - codegen externs (`llvm_backend.rs`) — `(ptr) -> ptr` for diff /
//!     flip, `(ptr, i64) -> ptr` for roll / repeat / tile;
//!   - the cabi shims (`cabi.rs`) — `__cobrust_coil_<op>` returning a
//!     fresh Boxed `Buffer` (TOTAL — a null handle is the only abort).
//!
//! Mirrors the compile→spawn→assert-stdout harness of
//! `coil_scalararg_e2e.rs`. Results are observed via `coil.print_buffer`.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

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

fn run(exe: &PathBuf) -> (String, String, bool) {
    let out = Command::new(exe)
        .output()
        .expect("spawn coil-rearrange prog");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

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
// POSITIVE — diff / flip (1-arg Buffer -> Buffer).
// =====================================================================

/// `coil.diff(array1d2(1.0, 4.0))` → `[3]` (one element, `4 - 1`). Oracle:
/// `np.diff([1., 4.])` → `array([3.])`. The result length is size-1 = 1.
#[test]
fn test_e2e_diff_adjacent_difference() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 4.0)\n",
        "    let r: coil.Buffer = coil.diff(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([3], dtype=float64)"),
        "expected diff([1,4])=[3] (len size-1); got stdout=\n{stdout}",
    );
}

/// `coil.flip(array1d2(1.0, 2.0))` → `[2, 1]` (same length, reversed).
/// Oracle: `np.flip([1., 2.])` → `array([2., 1.])`.
#[test]
fn test_e2e_flip_reverses() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 2.0)\n",
        "    let r: coil.Buffer = coil.flip(a)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([2, 1], dtype=float64)"),
        "expected flip([1,2])=[2, 1]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — roll / repeat / tile (Buffer + i64-scalar -> Buffer). The
// trailing arg is a `.cb` INT literal (lowers DIRECTLY as i64).
// =====================================================================

/// `coil.roll(array1d2(1.0, 2.0), 1)` → `[2, 1]` (SAME shape, cyclic
/// shift). Oracle: `np.roll([1., 2.], 1)` → `array([2., 1.])`. The trailing
/// `1` is a `.cb` int literal — the FIRST coil call with an i64 scalar arg.
#[test]
fn test_e2e_roll_cyclic_shift() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 2.0)\n",
        "    let r: coil.Buffer = coil.roll(a, 1)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([2, 1], dtype=float64)"),
        "expected roll([1,2],1)=[2, 1] (SAME shape); got stdout=\n{stdout}",
    );
}

/// `coil.roll(array1d2(1.0, 2.0), -1)` → `[2, 1]` (negative k rolls LEFT).
/// Oracle: `np.roll([1., 2.], -1)` → `array([2., 1.])`. Pins the
/// negative-i64-scalar path (a `.cb` `-1` literal).
#[test]
fn test_e2e_roll_negative_k_rolls_left() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 2.0)\n",
        "    let r: coil.Buffer = coil.roll(a, -1)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([2, 1], dtype=float64)"),
        "expected roll([1,2],-1)=[2, 1] (left-roll); got stdout=\n{stdout}",
    );
}

/// `coil.repeat(array1d2(1.0, 2.0), 2)` → `[1, 1, 2, 2]` (each element
/// twice, len n*size = 4). Oracle: `np.repeat([1., 2.], 2)` →
/// `array([1., 1., 2., 2.])`.
#[test]
fn test_e2e_repeat_each_element() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 2.0)\n",
        "    let r: coil.Buffer = coil.repeat(a, 2)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1, 1, 2, 2], dtype=float64)"),
        "expected repeat([1,2],2)=[1, 1, 2, 2]; got stdout=\n{stdout}",
    );
}

/// `coil.tile(array1d2(1.0, 2.0), 2)` → `[1, 2, 1, 2]` (whole array twice,
/// len n*size = 4). Oracle: `np.tile([1., 2.], 2)` →
/// `array([1., 2., 1., 2.])`. Contrasts `repeat` (whole-repeat vs.
/// per-element interleave).
#[test]
fn test_e2e_tile_whole_array() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 2.0)\n",
        "    let r: coil.Buffer = coil.tile(a, 2)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1, 2, 1, 2], dtype=float64)"),
        "expected tile([1,2],2)=[1, 2, 1, 2]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — chain: flip(diff(a)). Proves a fresh Buffer feeds the next
// op (the `_ecoret` handle of `diff` becomes the borrowed arg of `flip`).
// =====================================================================

/// `coil.flip(coil.diff(array2x2(1, 4, 9, 16)))` flattens the 2x2 to
/// `[1,4,9,16]`, diffs to `[3,5,7]`, flips to `[7,5,3]`. Oracle:
/// `np.flip(np.diff([1,4,9,16])) == [7,5,3]`. The `array2x2` constructor
/// proves the 2-D → flatten path inside `diff`.
#[test]
fn test_e2e_chain_flip_of_diff() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(1.0, 4.0, 9.0, 16.0)\n",
        "    let d: coil.Buffer = coil.diff(a)\n",
        "    let r: coil.Buffer = coil.flip(d)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([7, 5, 3], dtype=float64)"),
        "expected flip(diff([1,4,9,16]))=[7, 5, 3]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// NEGATIVE — manifest-driven arg-type rejection. `roll` / `repeat` /
// `tile` take an i64 scalar; a `str` scalar must be rejected at the
// type-check arm (not reach codegen). Uses the SAME `coil` surface, so
// these are real type-error proofs, not fake-symbol stubs.
// =====================================================================

/// `coil.roll(a, "x")` — the i64 scalar slot rejects a `str` arg (the
/// `(Buffer, Int) -> Buffer` manifest signature). Compile must FAIL.
#[test]
fn test_e2e_roll_rejects_str_scalar() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 2.0)\n",
        "    let r: coil.Buffer = coil.roll(a, \"x\")\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (ok, _stderr) = try_build(source);
    assert!(
        !ok,
        "coil.roll must reject a str scalar (Int-typed slot); build unexpectedly succeeded",
    );
}

/// `coil.tile(a, 2.5)` — the i64 scalar slot rejects a `float` arg (the
/// `count` is an `Int`, NOT a `Float` like `power`'s exponent). Compile
/// must FAIL — pins the `Ty::Int` (not `Ty::Float`) param choice.
#[test]
fn test_e2e_tile_rejects_float_scalar() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(1.0, 2.0)\n",
        "    let r: coil.Buffer = coil.tile(a, 2.5)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (ok, _stderr) = try_build(source);
    assert!(
        !ok,
        "coil.tile must reject a float scalar (Int-typed count slot); build unexpectedly succeeded",
    );
}
