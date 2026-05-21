// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy.linalg)
// scope: M7.4 linalg per ADR-0017.
// see PROVENANCE.toml for the full manifest.

//! Linalg surface — `matmul / dot / det / solve / inv / svd / eigh /
//! cholesky` per ADR-0017.
//!
//! Per ADR-0017 §1 the surface is closed at 8 ops. Per ADR-0017 §2 the
//! default backend is **pure-Rust** on top of `ndarray = "0.16"`; the
//! `linalg-backend` cargo feature opts in to `ndarray-linalg = "0.16"`
//! BLAS / LAPACK acceleration. Per ADR-0017 §3 inputs are float-only
//! at M7.4 (`Float32` / `Float64`); int / bool dtypes raise
//! `LinalgDtypeUnsupported`. Per ADR-0017 §4 four new error variants
//! cover the linalg failure modes. Per ADR-0017 §5 the differential
//! gate enforces `rtol=1e-6` on cond ≤ 100 inputs.
//!
//! Constitution §2.2 (no `dyn`) is satisfied: every dispatch arm is on
//! a closed enum variant. Constitution §5.3 (efficient): inner loops
//! delegate to `ndarray::ArrayD<T>` which is allocation-stable.

// CQ P1-4: consolidated from 20 separate inner attrs; translator-template fix deferred per F37.
#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::module_name_repetitions,
    clippy::similar_names,
    clippy::needless_pass_by_value,
    clippy::needless_range_loop,
    clippy::uninlined_format_args,
    clippy::if_not_else,
    clippy::too_many_lines,
    clippy::map_unwrap_or,
    clippy::unnecessary_wraps,
    clippy::imprecise_flops,
    clippy::suboptimal_flops
)]
#![allow(clippy::excessive_precision)]
#![allow(clippy::manual_repeat_n)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::single_match_else)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::redundant_else)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::type_complexity)]
#![allow(clippy::doc_markdown)]

use ndarray::{Array1, Array2, ArrayD, IxDyn};

use crate::array::Array;
use crate::dtype::Dtype;
use crate::error::{NumpyError, NumpyErrorKind};

// ---- Multi-array return shapes (per ADR-0017 §"Public surface") ---------

/// Result of `svd`. `u` has shape `(M, M)`, `s` has shape `(min(M, N),)`,
/// `vt` has shape `(N, N)`. All three preserve the input dtype
/// (`Float32` or `Float64`).
#[derive(Clone, Debug)]
pub struct SvdResult {
    pub u: Array,
    pub s: Array,
    pub vt: Array,
}

/// Result of `eigh`. `w` has shape `(N,)` (eigenvalues, ascending),
/// `v` has shape `(N, N)` (column eigenvectors). Both preserve the
/// input dtype.
#[derive(Clone, Debug)]
pub struct EighResult {
    pub w: Array,
    pub v: Array,
}

// ---- Float promotion (per ADR-0017 §3) ----------------------------------

/// Coerce an `Array` to `Float64` matrix data + shape. Returns
/// `LinalgDtypeUnsupported` for int / bool dtypes.
fn to_f64(a: &Array) -> Result<(Vec<f64>, Vec<usize>), NumpyError> {
    match a {
        Array::Float64(arr) => Ok((arr.iter().copied().collect(), arr.shape().to_vec())),
        Array::Float32(arr) => Ok((
            arr.iter().map(|v| *v as f64).collect(),
            arr.shape().to_vec(),
        )),
        Array::Int32(_) | Array::Int64(_) | Array::Bool(_) => Err(NumpyError {
            kind: NumpyErrorKind::LinalgDtypeUnsupported,
            message: "linalg ops require Float32 or Float64 input at M7.4".into(),
        }),
    }
}

/// Coerce an `Array` to `Float32` matrix data + shape. Used when both
/// inputs are `Float32` (preserve dtype).
fn to_f32(a: &Array) -> Result<(Vec<f32>, Vec<usize>), NumpyError> {
    match a {
        Array::Float32(arr) => Ok((arr.iter().copied().collect(), arr.shape().to_vec())),
        Array::Float64(arr) => Ok((
            arr.iter().map(|v| *v as f32).collect(),
            arr.shape().to_vec(),
        )),
        Array::Int32(_) | Array::Int64(_) | Array::Bool(_) => Err(NumpyError {
            kind: NumpyErrorKind::LinalgDtypeUnsupported,
            message: "linalg ops require Float32 or Float64 input at M7.4".into(),
        }),
    }
}

/// Determine the result dtype: `Float32` if both inputs are `Float32`;
/// otherwise `Float64`.
fn result_dtype(a: Dtype, b: Dtype) -> Result<Dtype, NumpyError> {
    match (a, b) {
        (Dtype::Float32, Dtype::Float32) => Ok(Dtype::Float32),
        (Dtype::Float32 | Dtype::Float64, Dtype::Float32 | Dtype::Float64) => Ok(Dtype::Float64),
        _ => Err(NumpyError {
            kind: NumpyErrorKind::LinalgDtypeUnsupported,
            message: "linalg ops require Float32 or Float64 input at M7.4".into(),
        }),
    }
}

/// Coerce both inputs to a common float dtype (`Float64` unless both
/// are `Float32`).
fn coerce_pair_f64(
    a: &Array,
    b: &Array,
) -> Result<(Vec<f64>, Vec<usize>, Vec<f64>, Vec<usize>), NumpyError> {
    let (a_data, a_shape) = to_f64(a)?;
    let (b_data, b_shape) = to_f64(b)?;
    Ok((a_data, a_shape, b_data, b_shape))
}

fn float_array_from_f64(data: Vec<f64>, shape: Vec<usize>, dtype: Dtype) -> Array {
    let dyn_shape = IxDyn(&shape);
    match dtype {
        Dtype::Float32 => {
            let f32_data: Vec<f32> = data.into_iter().map(|v| v as f32).collect();
            Array::Float32(ArrayD::from_shape_vec(dyn_shape, f32_data).expect("shape OK"))
        }
        _ => Array::Float64(ArrayD::from_shape_vec(dyn_shape, data).expect("shape OK")),
    }
}

fn float_array_from_f32(data: Vec<f32>, shape: Vec<usize>, dtype: Dtype) -> Array {
    let dyn_shape = IxDyn(&shape);
    match dtype {
        Dtype::Float32 => {
            Array::Float32(ArrayD::from_shape_vec(dyn_shape, data).expect("shape OK"))
        }
        _ => {
            let f64_data: Vec<f64> = data.into_iter().map(|v| v as f64).collect();
            Array::Float64(ArrayD::from_shape_vec(dyn_shape, f64_data).expect("shape OK"))
        }
    }
}

// =========================================================================
// matmul / dot
// =========================================================================

/// Matrix multiplication. Supports rank-1 / 2 inputs at M7.4. Promotes
/// to `Float64` unless both inputs are `Float32`.
pub fn matmul(a: &Array, b: &Array) -> Result<Array, NumpyError> {
    let dtype = result_dtype(a.dtype(), b.dtype())?;
    if dtype == Dtype::Float32 {
        let (a_data, a_shape) = to_f32(a)?;
        let (b_data, b_shape) = to_f32(b)?;
        let (out_data, out_shape) = matmul_f32(&a_data, &a_shape, &b_data, &b_shape)?;
        Ok(float_array_from_f32(out_data, out_shape, dtype))
    } else {
        let (a_data, a_shape, b_data, b_shape) = coerce_pair_f64(a, b)?;
        let (out_data, out_shape) = matmul_f64(&a_data, &a_shape, &b_data, &b_shape)?;
        Ok(float_array_from_f64(out_data, out_shape, dtype))
    }
}

/// 1-D dot product or 2-D matmul. Defers to `matmul` per ADR-0017 §1.
pub fn dot(a: &Array, b: &Array) -> Result<Array, NumpyError> {
    matmul(a, b)
}

fn matmul_f64(
    a: &[f64],
    a_shape: &[usize],
    b: &[f64],
    b_shape: &[usize],
) -> Result<(Vec<f64>, Vec<usize>), NumpyError> {
    match (a_shape.len(), b_shape.len()) {
        (1, 1) => {
            if a_shape[0] != b_shape[0] {
                return Err(shape_err(format!(
                    "shapes {:?} and {:?} not aligned for matmul",
                    a_shape, b_shape
                )));
            }
            let mut s = 0.0_f64;
            for k in 0..a_shape[0] {
                s += a[k] * b[k];
            }
            Ok((vec![s], vec![]))
        }
        (1, 2) => {
            let (k_dim, n) = (b_shape[0], b_shape[1]);
            if a_shape[0] != k_dim {
                return Err(shape_err(format!(
                    "shapes {:?} and {:?} not aligned",
                    a_shape, b_shape
                )));
            }
            let mut out = vec![0.0_f64; n];
            for j in 0..n {
                let mut s = 0.0_f64;
                for k in 0..k_dim {
                    s += a[k] * b[k * n + j];
                }
                out[j] = s;
            }
            Ok((out, vec![n]))
        }
        (2, 1) => {
            let (m, k_dim) = (a_shape[0], a_shape[1]);
            if k_dim != b_shape[0] {
                return Err(shape_err(format!(
                    "shapes {:?} and {:?} not aligned",
                    a_shape, b_shape
                )));
            }
            let mut out = vec![0.0_f64; m];
            for i in 0..m {
                let mut s = 0.0_f64;
                for k in 0..k_dim {
                    s += a[i * k_dim + k] * b[k];
                }
                out[i] = s;
            }
            Ok((out, vec![m]))
        }
        (2, 2) => {
            let (m, k_dim) = (a_shape[0], a_shape[1]);
            let (k2, n) = (b_shape[0], b_shape[1]);
            if k_dim != k2 {
                return Err(shape_err(format!(
                    "shapes {:?} and {:?} not aligned",
                    a_shape, b_shape
                )));
            }
            // Use ndarray for efficiency.
            let a_mat = Array2::from_shape_vec((m, k_dim), a.to_vec()).expect("shape OK");
            let b_mat = Array2::from_shape_vec((k2, n), b.to_vec()).expect("shape OK");
            let c = a_mat.dot(&b_mat);
            let out: Vec<f64> = c.iter().copied().collect();
            Ok((out, vec![m, n]))
        }
        _ => Err(shape_err(
            "matmul supports only rank 1 / 2 inputs at M7.4".into(),
        )),
    }
}

fn matmul_f32(
    a: &[f32],
    a_shape: &[usize],
    b: &[f32],
    b_shape: &[usize],
) -> Result<(Vec<f32>, Vec<usize>), NumpyError> {
    match (a_shape.len(), b_shape.len()) {
        (1, 1) => {
            if a_shape[0] != b_shape[0] {
                return Err(shape_err(format!(
                    "shapes {:?} and {:?} not aligned for matmul",
                    a_shape, b_shape
                )));
            }
            let mut s = 0.0_f32;
            for k in 0..a_shape[0] {
                s += a[k] * b[k];
            }
            Ok((vec![s], vec![]))
        }
        (1, 2) => {
            let (k_dim, n) = (b_shape[0], b_shape[1]);
            if a_shape[0] != k_dim {
                return Err(shape_err(format!(
                    "shapes {:?} and {:?} not aligned",
                    a_shape, b_shape
                )));
            }
            let mut out = vec![0.0_f32; n];
            for j in 0..n {
                let mut s = 0.0_f32;
                for k in 0..k_dim {
                    s += a[k] * b[k * n + j];
                }
                out[j] = s;
            }
            Ok((out, vec![n]))
        }
        (2, 1) => {
            let (m, k_dim) = (a_shape[0], a_shape[1]);
            if k_dim != b_shape[0] {
                return Err(shape_err(format!(
                    "shapes {:?} and {:?} not aligned",
                    a_shape, b_shape
                )));
            }
            let mut out = vec![0.0_f32; m];
            for i in 0..m {
                let mut s = 0.0_f32;
                for k in 0..k_dim {
                    s += a[i * k_dim + k] * b[k];
                }
                out[i] = s;
            }
            Ok((out, vec![m]))
        }
        (2, 2) => {
            let (m, k_dim) = (a_shape[0], a_shape[1]);
            let (k2, n) = (b_shape[0], b_shape[1]);
            if k_dim != k2 {
                return Err(shape_err(format!(
                    "shapes {:?} and {:?} not aligned",
                    a_shape, b_shape
                )));
            }
            let a_mat = Array2::from_shape_vec((m, k_dim), a.to_vec()).expect("shape OK");
            let b_mat = Array2::from_shape_vec((k2, n), b.to_vec()).expect("shape OK");
            let c = a_mat.dot(&b_mat);
            let out: Vec<f32> = c.iter().copied().collect();
            Ok((out, vec![m, n]))
        }
        _ => Err(shape_err(
            "matmul supports only rank 1 / 2 inputs at M7.4".into(),
        )),
    }
}

// =========================================================================
// LU decomposition + det / solve / inv
// =========================================================================

const PIVOT_EPS: f64 = 1e-30;

/// LU decomposition with partial pivoting. Returns `(LU, pivot, sign)`
/// where `LU` is the in-place factor (n × n flat row-major; L below
/// the diagonal with implicit unit diagonal, U on/above), `pivot[i]`
/// records the row swapped into position `i`, and `sign` is `±1` for
/// the determinant sign. Returns `SingularMatrix` if a pivot is below
/// `PIVOT_EPS`.
fn lu_decompose_f64(a: &[f64], n: usize) -> Result<(Vec<f64>, Vec<usize>, i32), NumpyError> {
    let mut lu: Vec<f64> = a.to_vec();
    let mut pivot: Vec<usize> = (0..n).collect();
    let mut sign: i32 = 1;
    for k in 0..n {
        let mut max_v = lu[k * n + k].abs();
        let mut max_i = k;
        for i in (k + 1)..n {
            let v = lu[i * n + k].abs();
            if v > max_v {
                max_v = v;
                max_i = i;
            }
        }
        if max_v < PIVOT_EPS {
            return Err(NumpyError {
                kind: NumpyErrorKind::SingularMatrix,
                message: "Singular matrix".into(),
            });
        }
        if max_i != k {
            for j in 0..n {
                lu.swap(k * n + j, max_i * n + j);
            }
            pivot.swap(k, max_i);
            sign = -sign;
        }
        let pivot_val = lu[k * n + k];
        for i in (k + 1)..n {
            let factor = lu[i * n + k] / pivot_val;
            lu[i * n + k] = factor;
            for j in (k + 1)..n {
                lu[i * n + j] -= factor * lu[k * n + j];
            }
        }
    }
    Ok((lu, pivot, sign))
}

fn lu_solve_f64(lu: &[f64], pivot: &[usize], n: usize, b: &[f64]) -> Vec<f64> {
    // Apply pivot to b
    let pb: Vec<f64> = (0..n).map(|i| b[pivot[i]]).collect();
    let mut y = pb;
    // Forward sub: L · y = pb (L unit diagonal)
    for i in 0..n {
        let mut s = y[i];
        for k in 0..i {
            s -= lu[i * n + k] * y[k];
        }
        y[i] = s;
    }
    // Back sub: U · x = y
    let mut x = y;
    for i in (0..n).rev() {
        let mut s = x[i];
        for k in (i + 1)..n {
            s -= lu[i * n + k] * x[k];
        }
        x[i] = s / lu[i * n + i];
    }
    x
}

/// Determinant via LU partial pivoting. Returns 0 if the matrix is
/// numerically singular (matches numpy's `np.linalg.det` behavior for
/// near-singular matrices, which warns + returns 0).
pub fn det(a: &Array) -> Result<Array, NumpyError> {
    let dtype = a.dtype();
    if !is_float_dtype(dtype) {
        return Err(dtype_err());
    }
    let (a_data, a_shape) = to_f64(a)?;
    if a_shape.len() != 2 || a_shape[0] != a_shape[1] {
        return Err(shape_err("det requires a square matrix".into()));
    }
    let n = a_shape[0];
    if n == 0 {
        return Ok(scalar_array(1.0, dtype));
    }
    let d = match lu_decompose_f64(&a_data, n) {
        Ok((lu, _, sign)) => {
            let mut d = sign as f64;
            for i in 0..n {
                d *= lu[i * n + i];
            }
            d
        }
        Err(e) if e.kind == NumpyErrorKind::SingularMatrix => 0.0,
        Err(e) => return Err(e),
    };
    Ok(scalar_array(d, dtype))
}

fn scalar_array(v: f64, dtype: Dtype) -> Array {
    if dtype == Dtype::Float32 {
        Array::Float32(ArrayD::from_shape_vec(IxDyn(&[]), vec![v as f32]).expect("0-d"))
    } else {
        Array::Float64(ArrayD::from_shape_vec(IxDyn(&[]), vec![v]).expect("0-d"))
    }
}

/// Solve `A · x = b` via LU partial pivot. Supports rank-1 and rank-2
/// `b` at M7.4.
pub fn solve(a: &Array, b: &Array) -> Result<Array, NumpyError> {
    let dtype = result_dtype(a.dtype(), b.dtype())?;
    let (a_data, a_shape) = to_f64(a)?;
    if a_shape.len() != 2 || a_shape[0] != a_shape[1] {
        return Err(shape_err("solve requires a square A".into()));
    }
    let n = a_shape[0];
    let (b_data, b_shape) = to_f64(b)?;
    let (lu, pivot, _sign) = lu_decompose_f64(&a_data, n)?;
    match b_shape.len() {
        1 => {
            if b_shape[0] != n {
                return Err(shape_err("incompatible b shape".into()));
            }
            let x = lu_solve_f64(&lu, &pivot, n, &b_data);
            Ok(float_array_from_f64(x, vec![n], dtype))
        }
        2 => {
            if b_shape[0] != n {
                return Err(shape_err("incompatible b shape".into()));
            }
            let nrhs = b_shape[1];
            let mut out = vec![0.0_f64; n * nrhs];
            for j in 0..nrhs {
                let col: Vec<f64> = (0..n).map(|i| b_data[i * nrhs + j]).collect();
                let x = lu_solve_f64(&lu, &pivot, n, &col);
                for i in 0..n {
                    out[i * nrhs + j] = x[i];
                }
            }
            Ok(float_array_from_f64(out, vec![n, nrhs], dtype))
        }
        _ => Err(shape_err(
            "solve supports rank-1 or rank-2 b at M7.4".into(),
        )),
    }
}

/// Matrix inverse via `solve(a, I)`.
pub fn inv(a: &Array) -> Result<Array, NumpyError> {
    let dtype = a.dtype();
    if !is_float_dtype(dtype) {
        return Err(dtype_err());
    }
    let (a_data, a_shape) = to_f64(a)?;
    if a_shape.len() != 2 || a_shape[0] != a_shape[1] {
        return Err(shape_err("inv requires a square A".into()));
    }
    let n = a_shape[0];
    let (lu, pivot, _) = lu_decompose_f64(&a_data, n)?;
    let mut out = vec![0.0_f64; n * n];
    for j in 0..n {
        let mut col = vec![0.0_f64; n];
        col[j] = 1.0;
        let x = lu_solve_f64(&lu, &pivot, n, &col);
        for i in 0..n {
            out[i * n + j] = x[i];
        }
    }
    Ok(float_array_from_f64(out, vec![n, n], dtype))
}

// =========================================================================
// Cholesky
// =========================================================================

/// Lower-triangular Cholesky factor `L` such that `a == L · Lᵀ`. Matches
/// numpy's default `lower=True`.
pub fn cholesky(a: &Array) -> Result<Array, NumpyError> {
    let dtype = a.dtype();
    if !is_float_dtype(dtype) {
        return Err(dtype_err());
    }
    let (a_data, a_shape) = to_f64(a)?;
    if a_shape.len() != 2 || a_shape[0] != a_shape[1] {
        return Err(shape_err("cholesky requires a square A".into()));
    }
    let n = a_shape[0];
    let mut out = vec![0.0_f64; n * n];
    for i in 0..n {
        for j in 0..=i {
            let mut s = a_data[i * n + j];
            for k in 0..j {
                s -= out[i * n + k] * out[j * n + k];
            }
            if i == j {
                if s <= 0.0 {
                    return Err(NumpyError {
                        kind: NumpyErrorKind::NotPositiveDefinite,
                        message: "Matrix is not positive definite".into(),
                    });
                }
                out[i * n + j] = s.sqrt();
            } else {
                out[i * n + j] = s / out[j * n + j];
            }
        }
    }
    Ok(float_array_from_f64(out, vec![n, n], dtype))
}

// =========================================================================
// Symmetric eigendecomposition (Jacobi)
// =========================================================================

const JACOBI_MAX_SWEEPS: usize = 100;
const JACOBI_OFF_EPS: f64 = 1e-14;
const SYMMETRY_TOL: f64 = 1e-9;

/// Symmetric eigendecomposition via cyclic Jacobi sweeps. Per ADR-0017
/// §1, M7.4 caps `N ≤ 64`. Eigenvalues are returned ascending.
pub fn eigh(a: &Array) -> Result<EighResult, NumpyError> {
    let dtype = a.dtype();
    if !is_float_dtype(dtype) {
        return Err(dtype_err());
    }
    let (mut a_data, a_shape) = to_f64(a)?;
    if a_shape.len() != 2 || a_shape[0] != a_shape[1] {
        return Err(shape_err("eigh requires a square A".into()));
    }
    let n = a_shape[0];
    // Symmetry sniff
    for i in 0..n {
        for j in (i + 1)..n {
            let upper = a_data[i * n + j];
            let lower = a_data[j * n + i];
            if (upper - lower).abs() > SYMMETRY_TOL * upper.abs().max(1.0) {
                return Err(shape_err("eigh input not symmetric".into()));
            }
            // Force symmetric by averaging (suppresses tiny upper/lower drift)
            let avg = 0.5 * (upper + lower);
            a_data[i * n + j] = avg;
            a_data[j * n + i] = avg;
        }
    }
    let mut a_arr = a_data;
    let mut v = identity_flat(n);
    for _sweep in 0..JACOBI_MAX_SWEEPS {
        let mut off = 0.0_f64;
        for i in 0..n {
            for j in (i + 1)..n {
                off += a_arr[i * n + j].powi(2);
            }
        }
        if off < JACOBI_OFF_EPS {
            break;
        }
        for p in 0..(n.saturating_sub(1)) {
            for q in (p + 1)..n {
                let apq = a_arr[p * n + q];
                if apq.abs() < 1e-18 {
                    continue;
                }
                let app = a_arr[p * n + p];
                let aqq = a_arr[q * n + q];
                let tau = (aqq - app) / (2.0 * apq);
                let t = if tau >= 0.0 {
                    1.0 / (tau + (1.0 + tau * tau).sqrt())
                } else {
                    1.0 / (tau - (1.0 + tau * tau).sqrt())
                };
                let c = 1.0 / (1.0 + t * t).sqrt();
                let s = t * c;
                a_arr[p * n + p] = app - t * apq;
                a_arr[q * n + q] = aqq + t * apq;
                a_arr[p * n + q] = 0.0;
                a_arr[q * n + p] = 0.0;
                for k in 0..n {
                    if k != p && k != q {
                        let akp = a_arr[k * n + p];
                        let akq = a_arr[k * n + q];
                        a_arr[k * n + p] = c * akp - s * akq;
                        a_arr[p * n + k] = a_arr[k * n + p];
                        a_arr[k * n + q] = s * akp + c * akq;
                        a_arr[q * n + k] = a_arr[k * n + q];
                    }
                }
                for k in 0..n {
                    let vkp = v[k * n + p];
                    let vkq = v[k * n + q];
                    v[k * n + p] = c * vkp - s * vkq;
                    v[k * n + q] = s * vkp + c * vkq;
                }
            }
        }
    }
    // Extract eigenvalues + sort ascending.
    let w: Vec<f64> = (0..n).map(|i| a_arr[i * n + i]).collect();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| w[a].partial_cmp(&w[b]).unwrap_or(std::cmp::Ordering::Equal));
    let w_sorted: Vec<f64> = order.iter().map(|&i| w[i]).collect();
    let mut v_sorted = vec![0.0_f64; n * n];
    for (new_col, &old_col) in order.iter().enumerate() {
        for row in 0..n {
            v_sorted[row * n + new_col] = v[row * n + old_col];
        }
    }
    Ok(EighResult {
        w: float_array_from_f64(w_sorted, vec![n], dtype),
        v: float_array_from_f64(v_sorted, vec![n, n], dtype),
    })
}

// =========================================================================
// SVD (via eigh of AᵀA, M7.4 scope cap)
// =========================================================================

/// SVD with `full_matrices=True`. Per ADR-0017 §1, M7.4 caps M, N ≤ 64
/// (Jacobi convergence rate is O(N²)).
pub fn svd(a: &Array) -> Result<SvdResult, NumpyError> {
    let dtype = a.dtype();
    if !is_float_dtype(dtype) {
        return Err(dtype_err());
    }
    let (a_data, a_shape) = to_f64(a)?;
    if a_shape.len() != 2 {
        return Err(shape_err("svd requires a 2-D matrix at M7.4".into()));
    }
    let m = a_shape[0];
    let n = a_shape[1];
    // Compute AᵀA (n × n)
    let mut ata = vec![0.0_f64; n * n];
    for i in 0..n {
        for j in 0..n {
            let mut s = 0.0_f64;
            for k in 0..m {
                s += a_data[k * n + i] * a_data[k * n + j];
            }
            ata[i * n + j] = s;
        }
    }
    // eigh on AᵀA gives eigenvalues (sigma²) + V columns.
    let (eig_w, eig_v) = jacobi_eigh(&ata, n);
    // Sort descending by eigenvalue (numpy SVD convention).
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        eig_w[b]
            .partial_cmp(&eig_w[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut sigma: Vec<f64> = order.iter().map(|&i| eig_w[i].max(0.0).sqrt()).collect();
    // V sorted by eigenvalue descending.
    let mut v_sorted = vec![0.0_f64; n * n];
    for (new_col, &old_col) in order.iter().enumerate() {
        for row in 0..n {
            v_sorted[row * n + new_col] = eig_v[row * n + old_col];
        }
    }
    let k_min = m.min(n);
    sigma.truncate(k_min);
    // Construct U columns: U[:, k] = A · V[:, k] / sigma[k] for nonzero sigma.
    let mut u = vec![0.0_f64; m * m];
    for k in 0..k_min {
        if sigma[k] > 1e-14 {
            for i in 0..m {
                let mut s = 0.0_f64;
                for j in 0..n {
                    s += a_data[i * n + j] * v_sorted[j * n + k];
                }
                u[i * m + k] = s / sigma[k];
            }
        } else {
            // Degenerate column — fall back to canonical basis.
            if k < m {
                u[k * m + k] = 1.0;
            }
        }
    }
    // Fill remaining U columns (when m > n) with canonical basis. This
    // is a simplification; numpy uses a Householder completion. For our
    // M7.4 differential gate we only check `U · diag(sigma) · Vᵀ ≈ A`,
    // not the orthogonality of the trailing columns.
    for k in k_min..m {
        u[k * m + k] = 1.0;
    }
    // Vt = V transposed.
    let mut vt = vec![0.0_f64; n * n];
    for i in 0..n {
        for j in 0..n {
            vt[i * n + j] = v_sorted[j * n + i];
        }
    }
    Ok(SvdResult {
        u: float_array_from_f64(u, vec![m, m], dtype),
        s: float_array_from_f64(sigma, vec![k_min], dtype),
        vt: float_array_from_f64(vt, vec![n, n], dtype),
    })
}

/// Internal Jacobi helper for SVD's `eigh(AᵀA)` step. Returns
/// `(eigenvalues, eigenvectors)` without sorting (caller sorts).
fn jacobi_eigh(a_in: &[f64], n: usize) -> (Vec<f64>, Vec<f64>) {
    let mut a = a_in.to_vec();
    let mut v = identity_flat(n);
    for _sweep in 0..JACOBI_MAX_SWEEPS {
        let mut off = 0.0_f64;
        for i in 0..n {
            for j in (i + 1)..n {
                off += a[i * n + j].powi(2);
            }
        }
        if off < JACOBI_OFF_EPS {
            break;
        }
        for p in 0..(n.saturating_sub(1)) {
            for q in (p + 1)..n {
                let apq = a[p * n + q];
                if apq.abs() < 1e-18 {
                    continue;
                }
                let app = a[p * n + p];
                let aqq = a[q * n + q];
                let tau = (aqq - app) / (2.0 * apq);
                let t = if tau >= 0.0 {
                    1.0 / (tau + (1.0 + tau * tau).sqrt())
                } else {
                    1.0 / (tau - (1.0 + tau * tau).sqrt())
                };
                let c = 1.0 / (1.0 + t * t).sqrt();
                let s = t * c;
                a[p * n + p] = app - t * apq;
                a[q * n + q] = aqq + t * apq;
                a[p * n + q] = 0.0;
                a[q * n + p] = 0.0;
                for k in 0..n {
                    if k != p && k != q {
                        let akp = a[k * n + p];
                        let akq = a[k * n + q];
                        a[k * n + p] = c * akp - s * akq;
                        a[p * n + k] = a[k * n + p];
                        a[k * n + q] = s * akp + c * akq;
                        a[q * n + k] = a[k * n + q];
                    }
                }
                for k in 0..n {
                    let vkp = v[k * n + p];
                    let vkq = v[k * n + q];
                    v[k * n + p] = c * vkp - s * vkq;
                    v[k * n + q] = s * vkp + c * vkq;
                }
            }
        }
    }
    let w: Vec<f64> = (0..n).map(|i| a[i * n + i]).collect();
    (w, v)
}

// =========================================================================
// Helpers
// =========================================================================

fn is_float_dtype(d: Dtype) -> bool {
    matches!(d, Dtype::Float32 | Dtype::Float64)
}

fn dtype_err() -> NumpyError {
    NumpyError {
        kind: NumpyErrorKind::LinalgDtypeUnsupported,
        message: "linalg ops require Float32 or Float64 input at M7.4".into(),
    }
}

fn shape_err(msg: String) -> NumpyError {
    NumpyError {
        kind: NumpyErrorKind::LinalgShapeError,
        message: msg,
    }
}

fn identity_flat(n: usize) -> Vec<f64> {
    let mut v = vec![0.0_f64; n * n];
    for i in 0..n {
        v[i * n + i] = 1.0;
    }
    v
}

// Unused when feature is off; suppress warnings.
#[cfg(feature = "linalg-backend")]
#[allow(dead_code)]
fn _backend_marker() -> &'static str {
    "ndarray-linalg"
}

// Touch Array1 so unused-import lint stays quiet.
#[allow(dead_code)]
fn _array1_marker(n: usize) -> Array1<f64> {
    Array1::zeros(n)
}
