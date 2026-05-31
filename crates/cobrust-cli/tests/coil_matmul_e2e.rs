//! coil buffer-buffer MATRIX MULTIPLY `a @ b` — `.cb` end-to-end proof for
//! the ADR-0077 §"@-operator" addition: the `@` operator (`BinOp::MatMul`)
//! on two `coil.Buffer`s, wired to numpy `matmul`.
//!
//! ## The load-bearing semantic: `@` is MATRIX matmul, `Buffer @ Buffer -> Buffer`
//!
//! `a @ b` on two arrays is numpy MATRIX multiplication (`np.matmul`): it
//! CONTRACTS the inner dimensions — `(m,k)@(k,n) -> (m,n)`,
//! `(m,k)@(k,) -> (m,)`, `(k,)@(k,n) -> (n,)`. It is NOT element-wise (that
//! is `*`). The result is ALWAYS a `coil.Buffer`: the 1-D·1-D `(k,)@(k,)`
//! degenerate case yields numpy's 0-d scalar, but Cobrust has no 0-d scalar
//! type (ADR-0077 Q2), so the f64-returning `a.dot(b)` METHOD is the surface
//! for that case and `@` always types to `coil.Buffer`. Shape conformability
//! (inner-dim alignment, valid ranks) is a RUNTIME check (panic-on-mismatch,
//! like `a + b`'s broadcast guard — ADR-0077 Q4); Cobrust static types carry
//! no shape.
//!
//! ## Where this sits in the chain (reuses the `+`/`-`/`*`/`/` machinery)
//!
//!   - typecheck `synth_bin` ARITHMETIC arm — `a @ b` (both Buffer) resolves
//!     through `lookup_buffer_binop`'s new `MatMul` arm to `coil_buffer_ty()`;
//!     a `Buffer @ scalar` / `scalar @ Buffer` is rejected with a §2.5 FIX
//!     (matmul needs two arrays);
//!   - MIR retarget (`lower.rs`) — the SAME `lookup_buffer_binop` array-array
//!     path as `+`/`-`/`*`/`/`; `@` reaches it unintercepted (the scalar-shim
//!     guards return `None` for `MatMul`) and retargets to
//!     `__cobrust_coil_buffer_matmul`;
//!   - codegen externs (`llvm_backend.rs`) — one `(ptr, ptr) -> ptr` row;
//!   - the cabi shim (`cabi.rs`) — a DEDICATED `__cobrust_coil_buffer_matmul`
//!     (NOT the shared `buffer_binop`, whose `broadcast_shape` pre-check would
//!     wrongly reject a valid non-broadcastable `(2,3)@(3,4)`); forwards
//!     straight to `Array::matmul` and `coil_panic`s on its shape `Err` (NEVER
//!     unwinding across the C-ABI);
//!   - manifest `lookup_buffer_binop` (`ecosystem.rs`) — one new arm.
//!
//! Mirrors the compile→spawn→assert-stdout harness of `coil_compare_e2e.rs`.
//! Results are observed via `coil.print_buffer`, whose float64 repr renders
//! `array([[19, 22], [43, 50]], dtype=float64)` (the values + the
//! `dtype=float64` token).

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
// Dense ADR-narrative module/test doc comments read as "lazy" list items to
// clippy; they are intentional explanatory prose, not lint targets.
#![allow(clippy::doc_lazy_continuation)]

use std::path::PathBuf;
use std::process::Command;

/// Compile a `.cb` source into an executable; panic with stderr on build
/// failure. The caller spawns + asserts. Mirrors `coil_compare_e2e.rs`.
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

/// `cobrust check` helper — `(ok, exit_code, stderr)`. For the negative
/// (out-of-scope) assertions that must reject at typecheck.
fn try_check(source: &str) -> (bool, Option<i32>, String) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("prog.cb");
    std::fs::write(&src_path, source).unwrap();
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_cobrust"));
    let out = Command::new(&bin)
        .arg("check")
        .arg(&src_path)
        .output()
        .unwrap();
    (
        out.status.success(),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// =====================================================================
// POSITIVE — 2-D @ 2-D matrix product (the headline case). The values are
// asymmetric so a wrong contraction (transpose / element-wise / swapped
// operands) yields a DIFFERENT matrix and FAILS.
// =====================================================================

/// Positive #1 (2x2 @ 2x2) — `[[1,2],[3,4]] @ [[5,6],[7,8]]` →
/// `[[19,22],[43,50]]` (19 = 1·5+2·7, 22 = 1·6+2·8, 43 = 3·5+4·7,
/// 50 = 3·6+4·8). The result is bound as a `coil.Buffer` (proving `@` types
/// to a Buffer) and printed. Element-wise `*` would give `[[5,12],[21,32]]`
/// (all four different) and a swapped `b @ a` gives `[[23,34],[31,46]]`, so
/// the exact quad pins the matmul contraction.
///
/// Oracle (numpy 2.0.2):
/// `np.array([[1.,2.],[3.,4.]]) @ np.array([[5.,6.],[7.,8.]])` →
/// `array([[19., 22.], [43., 50.]])`.
#[test]
fn test_e2e_buffer_matmul_2x2() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(1.0, 2.0, 3.0, 4.0)\n",
        "    let b: coil.Buffer = coil.array2x2(5.0, 6.0, 7.0, 8.0)\n",
        "    let c: coil.Buffer = a @ b\n",
        "    let _ = coil.print_buffer(c)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn matmul-2x2");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    // The exact product matrix (float64 repr). The full bracketed body pins
    // BOTH the values AND the (2,2) shape — a 1-D / transposed / element-wise
    // result would not contain this substring.
    assert!(
        stdout.contains("[[19, 22], [43, 50]]") && stdout.contains("dtype=float64"),
        "expected [[1,2],[3,4]]@[[5,6],[7,8]] = [[19, 22], [43, 50]] (float64); \
         got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// POSITIVE — matrix @ vector (the matrix-vector case): `(2,2)@(2,) -> (2,)`.
// Pins that `@` handles the mixed-rank matrix-vector contraction, not only
// the square 2-D·2-D case, and that the RESULT is a 1-D Buffer.
// =====================================================================

/// Positive #2 (2x2 @ vec2) — `[[1,2],[3,4]] @ [5,6]` → `[17, 39]`
/// (17 = 1·5+2·6, 39 = 3·5+4·6). The result is a 1-D `(2,)` buffer (the
/// matrix-vector product), distinct from the 2-D case above.
///
/// Oracle (numpy 2.0.2): `np.array([[1.,2.],[3.,4.]]) @ np.array([5.,6.])` →
/// `array([17., 39.])`.
#[test]
fn test_e2e_buffer_matmul_matrix_vector() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(1.0, 2.0, 3.0, 4.0)\n",
        "    let v: coil.Buffer = coil.array1d2(5.0, 6.0)\n",
        "    let mv: coil.Buffer = a @ v\n",
        "    let _ = coil.print_buffer(mv)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn matmul-matvec");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    // `[17, 39]` as a 1-D float64 array — single brackets (NOT `[[...]]`),
    // pinning the (2,) matrix-vector result shape.
    assert!(
        stdout.contains("array([17, 39]") && stdout.contains("dtype=float64"),
        "expected [[1,2],[3,4]]@[5,6] = [17, 39] (1-D float64); \
         got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// POSITIVE — `@` against the identity is the original matrix (a numerically
// trivial but shape-non-trivial 2-D·2-D check that needs only `coil.eye`).
// =====================================================================

/// Positive #3 (`a @ eye(2) == a`) — multiplying by the 2x2 identity returns
/// the original matrix. A cheap end-to-end sanity that `@` against
/// `coil.eye(n)` (the only other `.cb`-constructible 2-D matrix) is wired the
/// same way as the explicit-data constructors.
///
/// Oracle (numpy 2.0.2): `np.array([[1.,2.],[3.,4.]]) @ np.eye(2)` →
/// `array([[1., 2.], [3., 4.]])`.
#[test]
fn test_e2e_buffer_matmul_identity() {
    let source = concat!(
        "import coil\n",
        "\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(1.0, 2.0, 3.0, 4.0)\n",
        "    let i: coil.Buffer = coil.eye(2)\n",
        "    let c: coil.Buffer = a @ i\n",
        "    let _ = coil.print_buffer(c)\n",
        "    return 0\n",
    );
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn matmul-identity");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "binary non-zero exit ({:?}); stdout=\n{stdout}\nstderr=\n{stderr}",
        out.status,
    );
    assert!(
        stdout.contains("[[1, 2], [3, 4]]") && stdout.contains("dtype=float64"),
        "expected a @ eye(2) == a = [[1, 2], [3, 4]]; got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// CHECK-only POSITIVE — the EXPLICIT-borrow form `&a @ &b` (the LLM-idiomatic
// non-Copy reuse pattern, ADR-0052a) typechecks identically to bare `a @ b`.
// =====================================================================

/// Positive #4 (borrow form) — `&a @ &b` typechecks and binds to a
/// `coil.Buffer` (the explicit-borrow form resolves through the same
/// `lookup_buffer_binop` Ref-unwrap as the bare `a @ b`). A `check`-only
/// assertion (the runtime values are pinned by #1).
#[test]
fn test_check_buffer_matmul_borrow_form() {
    let (ok, code, stderr) = try_check(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(1.0, 2.0, 3.0, 4.0)\n",
        "    let b: coil.Buffer = coil.array2x2(5.0, 6.0, 7.0, 8.0)\n",
        "    let c: coil.Buffer = &a @ &b\n",
        "    let _ = coil.print_buffer(c)\n",
        "    return 0\n",
    ));
    assert!(
        ok,
        "`&a @ &b` (explicit-borrow matmul) must typecheck to a coil.Buffer; \
         code={code:?}; stderr=\n{stderr}",
    );
}

// =====================================================================
// RUNTIME TRAP — a non-conformable `a @ b` (inner dims not aligned) must
// ABORT CLEANLY (non-zero exit, a numpy-style diagnostic, no UB, no C-ABI
// unwind). Shape is a runtime property (ADR-0077 Q4 panic-on-violation).
// =====================================================================

/// Runtime trap — `(2,3) @ (2,2)` has misaligned inner dims (3 != 2) and must
/// trap at runtime (the static types are both `coil.Buffer` — shape is NOT in
/// the type, so this PASSES `check`/`build` and aborts only when run). The
/// abort goes through the cabi `coil_panic` → `__cobrust_panic` path (the
/// `Array::matmul` shape `Err` is converted, NEVER unwound across the C-ABI).
///
/// Oracle (numpy 2.0.2): `np.zeros((2,3)) @ np.zeros((2,2))` raises
/// `ValueError: matmul: ... shapes ... not aligned`.
#[test]
fn test_e2e_buffer_matmul_shape_mismatch_traps() {
    // (2,3) @ (2,2): a's inner dim is 3, b's outer dim is 2 — not aligned.
    let source = concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0)\n",
        "    let b: coil.Buffer = coil.array2x2(1.0, 2.0, 3.0, 4.0)\n",
        "    let c: coil.Buffer = a @ b\n",
        "    let _ = coil.print_buffer(c)\n",
        "    return 0\n",
    );
    // It MUST compile + build (shape is not a static property).
    let (_dir, exe) = compile_source(source);
    let out = Command::new(&exe).output().expect("spawn matmul-mismatch");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    // Clean trap: NON-zero exit (aborted), NOT a silent wrong answer.
    assert!(
        !out.status.success(),
        "a non-conformable (2,3)@(2,2) matmul must TRAP (non-zero exit), not return a value; \
         stdout=\n{stdout}\nstderr=\n{stderr}",
    );
    // The abort carries the numpy-style "not aligned" diagnostic (the cabi
    // `coil_panic` message), proving it is the matmul shape check that fired
    // (not an unrelated crash). The diagnostic is on stderr (the panic path).
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("not aligned") && combined.contains("matmul"),
        "expected a numpy-style matmul shape-mismatch diagnostic ('not aligned'); \
         got stdout=\n{stdout}\nstderr=\n{stderr}",
    );
}

// =====================================================================
// NEGATIVE / OUT-OF-SCOPE — `@` with ONE scalar operand must reject at
// typecheck with a §2.5 FIX (matmul needs TWO arrays). Pins that `@` did NOT
// accidentally route through a scalar-broadcast shim like `*` does.
// =====================================================================

/// Negative #1 — `a @ 2` (Buffer @ scalar) must be REJECTED at typecheck.
/// Matrix multiplication requires two arrays; numpy raises on `array @ 3`.
/// Expect exit 2 AND a §2.5 fix-printing diagnostic that names the
/// requirement (both operands must be a coil.Buffer) AND the fix (use `*` to
/// scale) — so the LLM knows the FIX, not just the diagnosis.
#[test]
fn test_neg_buffer_matmul_scalar_rejected_with_fix() {
    let (ok, code, stderr) = try_check(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(1.0, 2.0, 3.0, 4.0)\n",
        "    let c: coil.Buffer = a @ 2\n",
        "    let _ = coil.print_buffer(c)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "`a @ 2` (buffer @ scalar matmul) must be rejected (matmul needs two arrays); \
         stderr=\n{stderr}",
    );
    assert_eq!(
        code,
        Some(2),
        "expected TYPE_ERROR exit 2 for `a @ 2`; got {code:?}; stderr=\n{stderr}",
    );
    // §2.5-B — the diagnostic must PRINT THE FIX, not just the diagnosis: it
    // names BOTH operands must be a coil.Buffer AND points at `*` for scaling.
    assert!(
        stderr.contains("coil.Buffer") && stderr.contains("`*`"),
        "expected a §2.5 fix naming the two-Buffer matmul requirement + the `*` scale fix; \
         got stderr=\n{stderr}",
    );
}

/// Negative #2 (mirror) — `2 @ a` (scalar @ Buffer) is the LEFT-operand
/// mirror and must ALSO reject with a §2.5 fix. Pins that the guard catches a
/// Buffer on EITHER side (not only the LHS), exactly like the `*`/`-` scalar
/// forms commute but `@` rejects either-sided scalars.
#[test]
fn test_neg_scalar_matmul_buffer_rejected() {
    let (ok, code, stderr) = try_check(concat!(
        "import coil\n",
        "fn main() -> i64:\n",
        "    let a: coil.Buffer = coil.array2x2(1.0, 2.0, 3.0, 4.0)\n",
        "    let c: coil.Buffer = 2 @ a\n",
        "    let _ = coil.print_buffer(c)\n",
        "    return 0\n",
    ));
    assert!(
        !ok,
        "`2 @ a` (scalar @ buffer matmul) must be rejected (matmul needs two arrays); \
         stderr=\n{stderr}",
    );
    assert_eq!(
        code,
        Some(2),
        "expected TYPE_ERROR exit 2 for `2 @ a`; got {code:?}; stderr=\n{stderr}",
    );
    assert!(
        stderr.contains("coil.Buffer") && stderr.contains("`*`"),
        "expected a §2.5 fix for the scalar-@-buffer case too; got stderr=\n{stderr}",
    );
}
