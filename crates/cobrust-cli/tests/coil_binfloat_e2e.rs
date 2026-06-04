//! coil 2-Buffer FLOAT ufuncs (`coil.arctan2` / `coil.hypot` /
//! `coil.logaddexp`) — `.cb` end-to-end proof for the #145 BATCH-15
//! addition: 2-Buffer ELEMENTWISE ufuncs that are FLOAT-PROMOTING (int->f64,
//! f32->f32). They ride the IDENTICAL borrow-two-Buffers → fresh-Buffer-return
//! value-handle ABI as the BATCH-13 min/max family (`maximum` / `minimum` /
//! `fmax` / `fmin`) and the `concatenate` / `vstack` / `hstack` combine ops —
//! same `(ptr, ptr) -> ptr` `coil_binop_ty` extern, same `buffer_combine`
//! cabi body. The ONLY new behaviour is the per-op float math + the float
//! promotion (below).
//!
//! ## The load-bearing semantics (numpy 2.4.6 oracle)
//!
//! - `coil.arctan2(y, x)` — the angle (radians) of the point `(x, y)`.
//!   **numpy ARG ORDER IS `(y, x)` — Y FIRST**: `arctan2(1,0)=pi/2`, NOT `0`
//!   (a swapped y/x would compute `atan2(0,1)=0`). The four-quadrant test
//!   pins all of `arctan2(1,1)=pi/4`, `arctan2(1,0)=pi/2`,
//!   `arctan2(0,-1)=pi`, `arctan2(-1,0)=-pi/2`.
//! - `coil.hypot(x, y)` — the Euclidean norm `sqrt(x*x + y*y)` (hypotenuse),
//!   OVERFLOW-SAFE. `hypot(3,4)=5`.
//! - `coil.logaddexp(a, b)` — `log(exp(a) + exp(b))`, NUMERICALLY STABLE:
//!   `logaddexp(0,0)=ln(2)`; `logaddexp(1000,1000)=1000+ln(2)` is FINITE
//!   (a naive `log(exp+exp)` overflows to `+inf`).
//!
//! - Same-shape + same-dtype required (the `concatenate` equal-shape /
//!   equal-dtype combine contract; numpy broadcasts + promotes — a tracked
//!   follow-up). A non-conformable pair `coil_panic`s (numpy raises
//!   `ValueError`) = a clean trap, NEVER a C-ABI unwind.
//!
//! ## Where this sits in the chain (reuses the ecosystem-call machinery)
//!
//!   - typecheck `try_synth_ecosystem_call` → `lookup_module_fn("coil",
//!     "arctan2")` resolves the `(Buffer, Buffer) -> Buffer` `EcoSig`;
//!   - MIR `try_lower_ecosystem_call` Case-1 (module free fn) → the GENERIC
//!     `emit_ecosystem_call` borrow-args → fresh-Buffer-return path (the
//!     SAME path as the 2-Buffer `coil.maximum(a, b)`, iterating
//!     `sig.params` regardless of arity; NO batch-specific MIR arm — ZERO
//!     new MIR code);
//!   - codegen externs (`llvm_backend.rs`) — the `(ptr,ptr)->ptr`
//!     `coil_binop_ty` shape, reused verbatim from `maximum`;
//!   - the cabi shims (`cabi.rs`) — `__cobrust_coil_{arctan2,hypot,
//!     logaddexp}` borrow BOTH handles via the shared `buffer_combine` body,
//!     return a fresh Boxed `Buffer`, and `coil_panic` on a non-conformable
//!     / dtype-mismatch pair (NEVER unwinding across the C-ABI).
//!
//! Mirrors the compile→spawn→assert-stdout harness of `coil_minmax_e2e.rs`.
//! Results are observed via `coil.print_buffer`, whose float64 repr renders
//! integer-valued floats WITHOUT a `.0` suffix (e.g. `array([5, 13],
//! dtype=float64)`) and a non-integer float via Rust's shortest round-trip
//! `Display` (e.g. `pi/4` → `0.7853981633974483`).

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative module/test doc comments read as "lazy" list items to
// clippy; they are intentional explanatory prose, not lint targets.
#![allow(clippy::doc_lazy_continuation)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; panic with stderr on build
/// failure. Mirrors `coil_minmax_e2e.rs`.
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
    let out = Command::new(exe)
        .output()
        .expect("spawn coil-binfloat prog");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

// =====================================================================
// POSITIVE — `arctan2` ALL FOUR QUADRANTS + the (y, x) ARG ORDER. Built as
// (2,2) buffers (coil has `array2x2` but no `array1d4`; arctan2 is
// elementwise over any shape). y = [[1,1],[0,-1]], x = [[1,0],[-1,0]]:
//   arctan2(1,1)=pi/4, arctan2(1,0)=pi/2  (row 0)
//   arctan2(0,-1)=pi,  arctan2(-1,0)=-pi/2 (row 1)
// The [0][1] value pi/2 (NOT 0) is THE assertion: a swapped (x,y) call would
// compute atan2(0,1)=0 at that lane — this pins Y FIRST.
// =====================================================================

/// `coil.arctan2([[1,1],[0,-1]], [[1,0],[-1,0]]) = [[pi/4, pi/2], [pi,
/// -pi/2]]` — all four quadrants, ARG ORDER `(y, x)` Y FIRST.
///
/// Oracle (numpy 2.4.6): `np.arctan2([[1,1],[0,-1]],[[1,0],[-1,0]]) =
/// [[0.7853981633974483, 1.5707963267948966], [3.141592653589793,
/// -1.5707963267948966]]`. The `[0][1]` `pi/2` proves Y FIRST (a swapped y/x
/// → `0`).
#[test]
fn test_e2e_arctan2_four_quadrants_arg_order() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let y: coil.Buffer = coil.array2x2(1.0, 1.0, 0.0, -1.0)\n",
        "    let x: coil.Buffer = coil.array2x2(1.0, 0.0, -1.0, 0.0)\n",
        "    let r: coil.Buffer = coil.arctan2(y, x)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains(
            "array([[0.7853981633974483, 1.5707963267948966], \
             [3.141592653589793, -1.5707963267948966]], dtype=float64)"
        ),
        "expected arctan2(y,x) over 4 quadrants = [[pi/4, pi/2], [pi, -pi/2]] \
         ([0][1] = pi/2 proves Y FIRST); got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — the arg order is load-bearing: arctan2(y=1,x=0) != arctan2(y=0,x=1).
// arctan2([1,0],[0,1]) = [pi/2, 0]  (Y FIRST: lane 0 has y=1,x=0 -> pi/2;
// lane 1 has y=0,x=1 -> 0). If the shim forwarded (x, y) instead, lane 0
// would be atan2(0,1)=0 — so this lane-0 pi/2 is the swap detector.
// =====================================================================

/// `coil.arctan2([1,0], [0,1]) = [pi/2, 0]` — within ONE call, two lanes
/// with swapped operands give divergent results, proving the `(y, x)` order.
///
/// Oracle (numpy 2.4.6): `np.arctan2([1,0],[0,1]) = [1.5707963267948966, 0.]`.
#[test]
fn test_e2e_arctan2_swap_detector() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let y: coil.Buffer = coil.array1d2(1.0, 0.0)\n",
        "    let x: coil.Buffer = coil.array1d2(0.0, 1.0)\n",
        "    let r: coil.Buffer = coil.arctan2(y, x)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([1.5707963267948966, 0], dtype=float64)"),
        "expected arctan2([1,0],[0,1]) = [pi/2, 0]; a swapped (x,y) shim \
         would give [0, pi/2]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `hypot`. hypot([3,5],[4,12]) = [5, 13] (the classic Pythagorean
// triples; lane-mixed, rules out a degenerate fill). f64 result renders
// integer-valued floats without `.0`.
// =====================================================================

/// `coil.hypot([3,5], [4,12]) = [5, 13]` — the Euclidean norm over two real
/// Buffers.
///
/// Oracle (numpy 2.4.6): `np.hypot([3.,5.],[4.,12.]) = [5., 13.]`.
#[test]
fn test_e2e_hypot_pythagorean() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let x: coil.Buffer = coil.array1d2(3.0, 5.0)\n",
        "    let y: coil.Buffer = coil.array1d2(4.0, 12.0)\n",
        "    let r: coil.Buffer = coil.hypot(x, y)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([5, 13], dtype=float64)"),
        "expected hypot([3,5],[4,12]) = [5, 13]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// POSITIVE — `logaddexp` value + STABILITY. logaddexp([0,1000],[0,1000]) =
// [ln2, 1000+ln2]. The lane-1 value (1000+ln2 ≈ 1000.693) is FINITE — a naive
// log(exp(1000)+exp(1000)) overflows to +inf. The finite lane-1 IS the
// stability proof (the whole point of the stable formula).
// =====================================================================

/// `coil.logaddexp([0,1000], [0,1000]) = [ln2, 1000+ln2]` — the log-sum-exp,
/// with the large-input lane STABLE (finite, not `+inf`).
///
/// Oracle (numpy 2.4.6): `np.logaddexp([0,1000],[0,1000]) =
/// [0.6931471805599453, 1000.6931471805599]`. A naive `log(exp+exp)` gives
/// `+inf` at lane 1; the `max + ln1p(exp(-|d|))` form stays finite.
#[test]
fn test_e2e_logaddexp_stable() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(0.0, 1000.0)\n",
        "    let b: coil.Buffer = coil.array1d2(0.0, 1000.0)\n",
        "    let r: coil.Buffer = coil.logaddexp(a, b)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([0.6931471805599453, 1000.6931471805599], dtype=float64)"),
        "expected logaddexp([0,1000],[0,1000]) = [ln2, 1000+ln2] (lane 1 \
         FINITE = STABLE; a naive log(exp+exp) → inf); got stdout=\n{stdout}",
    );
    // Defensive: the stable result must NOT contain `inf` (the naive-overflow
    // failure mode).
    assert!(
        !stdout.contains("inf"),
        "logaddexp must be STABLE — no `inf` in the output; got stdout=\n{stdout}",
    );
}

// =====================================================================
// CHAIN — a hypot-result feeds the next op (proving the result handle is a
// first-class drop-scheduled Buffer). sqrt(hypot([3],[4])) chains a 2-Buffer
// op into a 1-Buffer op. hypot([9,40],[12,9]) = [15, 41], then transpose
// (a no-op on rank 1, but proves the fresh Buffer is a valid follow-on handle).
// =====================================================================

/// `coil.transpose(coil.hypot(x, y))` — the hypot result is a first-class
/// Buffer consumed by the next op; both temporaries drop.
/// `hypot([9,40],[12,9]) = [15, 41]`; transpose of a 1-D is unchanged.
#[test]
fn test_e2e_transpose_of_hypot() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let x: coil.Buffer = coil.array1d2(9.0, 40.0)\n",
        "    let y: coil.Buffer = coil.array1d2(12.0, 9.0)\n",
        "    let r: coil.Buffer = coil.hypot(x, y)\n", // [15, 41]
        "    let t: coil.Buffer = coil.transpose(r)\n",
        "    let _ = coil.print_buffer(t)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, stderr, ok) = run(&exe);
    assert!(ok, "non-zero exit; stdout=\n{stdout}\nstderr=\n{stderr}");
    assert!(
        stdout.contains("array([15, 41], dtype=float64)"),
        "expected transpose∘hypot(1-D unchanged) = [15, 41]; got stdout=\n{stdout}",
    );
}

// =====================================================================
// NEGATIVE (RUNTIME) — non-conformable pair aborts cleanly (numpy raises
// ValueError; the shim `coil_panic`s = a clean trap, never a C-ABI unwind).
// a is (2,) but b is (2,2): the shape mismatch aborts.
// =====================================================================

/// `coil.hypot(a(2,), b(2,2))` traps (non-conformable: equal-shape combine
/// contract). The binary exits NON-zero (the `__cobrust_panic` abort path)
/// rather than producing a garbage buffer or unwinding across the C-ABI.
/// numpy would broadcast — coil raises (a tracked follow-up).
#[test]
fn test_e2e_hypot_nonconformable_traps() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array1d2(3.0, 4.0)\n", // (2,)
        "    let b: coil.Buffer = coil.array2x2(1.0, 2.0, 3.0, 4.0)\n", // (2,2) — mismatch
        "    let r: coil.Buffer = coil.hypot(a, b)\n",
        "    let _ = coil.print_buffer(r)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let (stdout, _stderr, ok) = run(&exe);
    assert!(
        !ok,
        "expected non-conformable hypot to TRAP (non-zero exit); \
         got success with stdout=\n{stdout}",
    );
}
