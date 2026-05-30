// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy)
// scope: M7.1 ufuncs per ADR-0014 §1.
// see PROVENANCE.toml for the full manifest.

//! Universal-function dispatch + per-dtype monomorphic inner loops.
//!
//! Per ADR-0014 §1: dispatch is monomorphic via the `for_each_dtype!`
//! macro. The public-API (`Array::add`, `Array::sin`, etc.) matches
//! once on `(self.dtype(), other.dtype())`, picks the promoted dtype
//! via `result_type` (ADR-0014 §3), and dispatches into a per-dtype
//! monomorphic helper. The inner helper calls
//! `ndarray::Zip::from(...).and(...).map_collect(...)` on a concrete
//! `ndarray::ArrayD<T>`; LLVM inlines and the Zip iterator
//! vectorises naturally.
//!
//! Constitution §2.2 (no `dyn`) is satisfied: every dispatch arm is
//! on a closed enum variant. Constitution §5.3 (efficient): inner
//! loops are auto-vectorisable.

// CQ P1-4 + template-fix: single consolidated block; future emits use #[allow] at item level.
#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::if_not_else,
    clippy::map_unwrap_or,
    clippy::missing_errors_doc,
    clippy::module_name_repetitions,
    clippy::needless_bitwise_bool,
    clippy::needless_pass_by_value,
    clippy::similar_names,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unnecessary_wraps
)]

use ndarray::{ArrayD, Zip};

use crate::array::Array;
use crate::broadcast::broadcast_shape;
use crate::dtype::Dtype;
use crate::error::{NumpyError, NumpyErrorKind};
use crate::promote::{result_type, unary_math_dtype};

// ---- Cast helpers --------------------------------------------------------

/// Cast an `Array` to a specific dtype, returning an `ArrayD<T>`. Used by
/// the monomorphic inner loops to coerce both operands to the promoted
/// dtype before dispatching the element-wise op.
fn cast_to_i32(arr: &Array) -> ArrayD<i32> {
    match arr {
        Array::Int32(a) => a.clone(),
        Array::Int64(a) => a.mapv(|v| v as i32),
        Array::Float32(a) => a.mapv(|v| v as i32),
        Array::Float64(a) => a.mapv(|v| v as i32),
        Array::Bool(a) => a.mapv(i32::from),
    }
}

fn cast_to_i64(arr: &Array) -> ArrayD<i64> {
    match arr {
        Array::Int32(a) => a.mapv(i64::from),
        Array::Int64(a) => a.clone(),
        Array::Float32(a) => a.mapv(|v| v as i64),
        Array::Float64(a) => a.mapv(|v| v as i64),
        Array::Bool(a) => a.mapv(i64::from),
    }
}

fn cast_to_f32(arr: &Array) -> ArrayD<f32> {
    match arr {
        Array::Int32(a) => a.mapv(|v| v as f32),
        Array::Int64(a) => a.mapv(|v| v as f32),
        Array::Float32(a) => a.clone(),
        Array::Float64(a) => a.mapv(|v| v as f32),
        Array::Bool(a) => a.mapv(|v| f32::from(u8::from(v))),
    }
}

fn cast_to_f64(arr: &Array) -> ArrayD<f64> {
    match arr {
        Array::Int32(a) => a.mapv(f64::from),
        Array::Int64(a) => a.mapv(|v| v as f64),
        Array::Float32(a) => a.mapv(f64::from),
        Array::Float64(a) => a.clone(),
        Array::Bool(a) => a.mapv(|v| f64::from(u8::from(v))),
    }
}

fn cast_to_bool(arr: &Array) -> ArrayD<bool> {
    match arr {
        Array::Int32(a) => a.mapv(|v| v != 0),
        Array::Int64(a) => a.mapv(|v| v != 0),
        Array::Float32(a) => a.mapv(|v| v != 0.0),
        Array::Float64(a) => a.mapv(|v| v != 0.0),
        Array::Bool(a) => a.clone(),
    }
}

/// Cast an `Array` to the requested promoted dtype, returning a fresh
/// `Array`. Used by the public-API binary-op entrypoints.
fn cast_to(arr: &Array, target: Dtype) -> Array {
    match target {
        Dtype::Int32 => Array::Int32(cast_to_i32(arr)),
        Dtype::Int64 => Array::Int64(cast_to_i64(arr)),
        Dtype::Float32 => Array::Float32(cast_to_f32(arr)),
        Dtype::Float64 => Array::Float64(cast_to_f64(arr)),
        Dtype::Bool => Array::Bool(cast_to_bool(arr)),
        Dtype::Complex64 | Dtype::Complex128 => {
            // Per ADR-0021 §3 + §5 the M7.6 sub-milestone widens `Dtype`
            // to seven variants but defers the `Array` tagged-union
            // widening (and therefore complex-cast routing) to a
            // follow-up sprint. Reaching this arm at M7.6 means a caller
            // passed a complex target_dtype to a code path that did not
            // pre-validate it; every consumer in the M7.6 surface filters
            // complex via `Dtype::is_complex` before calling `cast_to`.
            unreachable!(
                "ufunc::cast_to: target Complex dtype routed through real-only path;                  callers must filter via Dtype::is_complex before reaching here                  (M7.6 ADR-0021 §3 + §5)"
            );
        }
    }
}

// ---- Broadcast view helpers ---------------------------------------------

/// Materialise a broadcast `ArrayD<T>` from `arr` to `target_shape`.
/// `ndarray::ArrayBase::broadcast` returns a view; we own the result
/// so we materialise to an owned `ArrayD<T>` via `.to_owned()`.
fn broadcast_owned<T: Clone>(arr: &ArrayD<T>, target: &[usize]) -> ArrayD<T> {
    arr.broadcast(ndarray::IxDyn(target))
        .map(|view| view.to_owned())
        .unwrap_or_else(|| arr.clone())
}

// ---- Dispatch model ----------------------------------------------------
//
// Per ADR-0014 §1: dispatch is monomorphic. The public-API entrypoints
// match once on Array variants, cast both operands to the promoted
// dtype, and dispatch into a per-variant inner loop. We intentionally
// inline the per-dtype arms in `binary_dispatch` and `cmp_dispatch`
// rather than introduce a `for_each_dtype!` macro: the explicit match
// is easier to read, easier to debug under perf profilers, and the
// LLVM optimiser inlines the closures regardless. A macro would add
// indirection without removing any code.

// ---- Binary-op core dispatch --------------------------------------------

/// Dispatch a binary-op closure across two operands after broadcasting
/// + promotion. The op closure runs in the **promoted dtype** space.
///
/// `op_*` are per-dtype monomorphic closures; only the promoted dtype's
/// closure runs. The other dtypes' closures are still in the binary
/// (because they're compiled), but they are dead code at runtime per
/// the dispatch arm — LLVM eliminates them.
#[allow(clippy::too_many_arguments)]
fn binary_dispatch(
    a: &Array,
    b: &Array,
    promoted: Dtype,
    op_i32: impl Fn(i32, i32) -> Result<i32, NumpyError>,
    op_i64: impl Fn(i64, i64) -> Result<i64, NumpyError>,
    op_f32: impl Fn(f32, f32) -> f32,
    op_f64: impl Fn(f64, f64) -> f64,
    op_bool: impl Fn(bool, bool) -> bool,
) -> Result<Array, NumpyError> {
    let a_cast = cast_to(a, promoted);
    let b_cast = cast_to(b, promoted);
    let target_shape = broadcast_shape(&a.shape(), &b.shape())?;
    let target_ix = ndarray::IxDyn(&target_shape);

    match (a_cast, b_cast) {
        (Array::Int32(av), Array::Int32(bv)) => {
            let av_b = broadcast_owned(&av, &target_shape);
            let bv_b = broadcast_owned(&bv, &target_shape);
            let mut out = ArrayD::<i32>::zeros(target_ix);
            let mut err: Option<NumpyError> = None;
            Zip::from(&mut out)
                .and(&av_b)
                .and(&bv_b)
                .for_each(|o, &x, &y| {
                    if err.is_some() {
                        return;
                    }
                    match op_i32(x, y) {
                        Ok(v) => *o = v,
                        Err(e) => err = Some(e),
                    }
                });
            if let Some(e) = err {
                return Err(e);
            }
            Ok(Array::Int32(out))
        }
        (Array::Int64(av), Array::Int64(bv)) => {
            let av_b = broadcast_owned(&av, &target_shape);
            let bv_b = broadcast_owned(&bv, &target_shape);
            let mut out = ArrayD::<i64>::zeros(target_ix);
            let mut err: Option<NumpyError> = None;
            Zip::from(&mut out)
                .and(&av_b)
                .and(&bv_b)
                .for_each(|o, &x, &y| {
                    if err.is_some() {
                        return;
                    }
                    match op_i64(x, y) {
                        Ok(v) => *o = v,
                        Err(e) => err = Some(e),
                    }
                });
            if let Some(e) = err {
                return Err(e);
            }
            Ok(Array::Int64(out))
        }
        (Array::Float32(av), Array::Float32(bv)) => {
            let av_b = broadcast_owned(&av, &target_shape);
            let bv_b = broadcast_owned(&bv, &target_shape);
            let mut out = ArrayD::<f32>::zeros(target_ix);
            Zip::from(&mut out)
                .and(&av_b)
                .and(&bv_b)
                .for_each(|o, &x, &y| {
                    *o = op_f32(x, y);
                });
            Ok(Array::Float32(out))
        }
        (Array::Float64(av), Array::Float64(bv)) => {
            let av_b = broadcast_owned(&av, &target_shape);
            let bv_b = broadcast_owned(&bv, &target_shape);
            let mut out = ArrayD::<f64>::zeros(target_ix);
            Zip::from(&mut out)
                .and(&av_b)
                .and(&bv_b)
                .for_each(|o, &x, &y| {
                    *o = op_f64(x, y);
                });
            Ok(Array::Float64(out))
        }
        (Array::Bool(av), Array::Bool(bv)) => {
            let av_b = broadcast_owned(&av, &target_shape);
            let bv_b = broadcast_owned(&bv, &target_shape);
            let mut out = ArrayD::<bool>::from_elem(target_ix, false);
            Zip::from(&mut out)
                .and(&av_b)
                .and(&bv_b)
                .for_each(|o, &x, &y| {
                    *o = op_bool(x, y);
                });
            Ok(Array::Bool(out))
        }
        _ => unreachable!("cast_to must produce matching variants"),
    }
}

// ---- Comparison-op core dispatch (always returns Bool) -----------------

#[allow(clippy::too_many_arguments)]
fn cmp_dispatch(
    a: &Array,
    b: &Array,
    cmp_i32: impl Fn(i32, i32) -> bool,
    cmp_i64: impl Fn(i64, i64) -> bool,
    cmp_f32: impl Fn(f32, f32) -> bool,
    cmp_f64: impl Fn(f64, f64) -> bool,
    cmp_bool: impl Fn(bool, bool) -> bool,
) -> Result<Array, NumpyError> {
    // Comparison promotes operands per result_type but the **output** is
    // always Bool (matches numpy).
    let promoted = result_type(a.dtype(), b.dtype());
    let a_cast = cast_to(a, promoted);
    let b_cast = cast_to(b, promoted);
    let target_shape = broadcast_shape(&a.shape(), &b.shape())?;
    let target_ix = ndarray::IxDyn(&target_shape);

    let out = match (a_cast, b_cast) {
        (Array::Int32(av), Array::Int32(bv)) => {
            let av_b = broadcast_owned(&av, &target_shape);
            let bv_b = broadcast_owned(&bv, &target_shape);
            let mut out = ArrayD::<bool>::from_elem(target_ix, false);
            Zip::from(&mut out)
                .and(&av_b)
                .and(&bv_b)
                .for_each(|o, &x, &y| {
                    *o = cmp_i32(x, y);
                });
            out
        }
        (Array::Int64(av), Array::Int64(bv)) => {
            let av_b = broadcast_owned(&av, &target_shape);
            let bv_b = broadcast_owned(&bv, &target_shape);
            let mut out = ArrayD::<bool>::from_elem(target_ix, false);
            Zip::from(&mut out)
                .and(&av_b)
                .and(&bv_b)
                .for_each(|o, &x, &y| {
                    *o = cmp_i64(x, y);
                });
            out
        }
        (Array::Float32(av), Array::Float32(bv)) => {
            let av_b = broadcast_owned(&av, &target_shape);
            let bv_b = broadcast_owned(&bv, &target_shape);
            let mut out = ArrayD::<bool>::from_elem(target_ix, false);
            Zip::from(&mut out)
                .and(&av_b)
                .and(&bv_b)
                .for_each(|o, &x, &y| {
                    *o = cmp_f32(x, y);
                });
            out
        }
        (Array::Float64(av), Array::Float64(bv)) => {
            let av_b = broadcast_owned(&av, &target_shape);
            let bv_b = broadcast_owned(&bv, &target_shape);
            let mut out = ArrayD::<bool>::from_elem(target_ix, false);
            Zip::from(&mut out)
                .and(&av_b)
                .and(&bv_b)
                .for_each(|o, &x, &y| {
                    *o = cmp_f64(x, y);
                });
            out
        }
        (Array::Bool(av), Array::Bool(bv)) => {
            let av_b = broadcast_owned(&av, &target_shape);
            let bv_b = broadcast_owned(&bv, &target_shape);
            let mut out = ArrayD::<bool>::from_elem(target_ix, false);
            Zip::from(&mut out)
                .and(&av_b)
                .and(&bv_b)
                .for_each(|o, &x, &y| {
                    *o = cmp_bool(x, y);
                });
            out
        }
        _ => unreachable!("cast_to must produce matching variants"),
    };
    Ok(Array::Bool(out))
}

// ---- Public binary ops --------------------------------------------------

/// Element-wise add: `a + b`. Promotes per `result_type`, broadcasts
/// per numpy rules, integer overflow wraps (matches numpy default).
pub fn add(a: &Array, b: &Array) -> Result<Array, NumpyError> {
    let promoted = result_type(a.dtype(), b.dtype());
    binary_dispatch(
        a,
        b,
        promoted,
        |x, y| Ok(x.wrapping_add(y)),
        |x, y| Ok(x.wrapping_add(y)),
        |x, y| x + y,
        |x, y| x + y,
        |x, y| x | y,
    )
}

pub fn sub(a: &Array, b: &Array) -> Result<Array, NumpyError> {
    let promoted = result_type(a.dtype(), b.dtype());
    binary_dispatch(
        a,
        b,
        promoted,
        |x, y| Ok(x.wrapping_sub(y)),
        |x, y| Ok(x.wrapping_sub(y)),
        |x, y| x - y,
        |x, y| x - y,
        // bool - bool: numpy raises, we return XOR-flavored to keep total.
        |x, y| x != y,
    )
}

pub fn mul(a: &Array, b: &Array) -> Result<Array, NumpyError> {
    let promoted = result_type(a.dtype(), b.dtype());
    binary_dispatch(
        a,
        b,
        promoted,
        |x, y| Ok(x.wrapping_mul(y)),
        |x, y| Ok(x.wrapping_mul(y)),
        |x, y| x * y,
        |x, y| x * y,
        |x, y| x & y,
    )
}

/// Element-wise division. Integer dtypes raise
/// `NumpyErrorKind::IntegerDivisionByZero` on divisor==0; float dtypes
/// follow IEEE 754 (`x/0.0 → ±inf`, `0.0/0.0 → NaN`). Per ADR-0014 §4.
pub fn div(a: &Array, b: &Array) -> Result<Array, NumpyError> {
    let promoted = result_type(a.dtype(), b.dtype());
    binary_dispatch(
        a,
        b,
        promoted,
        |x, y| {
            if y == 0 {
                Err(NumpyError {
                    kind: NumpyErrorKind::IntegerDivisionByZero,
                    message: "integer division by zero".into(),
                })
            } else {
                Ok(x.wrapping_div(y))
            }
        },
        |x, y| {
            if y == 0 {
                Err(NumpyError {
                    kind: NumpyErrorKind::IntegerDivisionByZero,
                    message: "integer division by zero".into(),
                })
            } else {
                Ok(x.wrapping_div(y))
            }
        },
        |x, y| x / y,
        |x, y| x / y,
        |x, y| {
            // bool/bool: matches numpy's float-division promotion path
            // semantically but with our tier we keep it total: y=false →
            // would normally be IntegerDivisionByZero, but bool/bool is
            // not a numpy ufunc; we return x AND y as a placeholder to
            // keep totality. M7.x callers should not rely on this.
            x & y
        },
    )
}

/// Element-wise NumPy **true division** (`/`, the `true_divide` ufunc).
///
/// Unlike [`div`] (which dispatches in the dtype-preserving promoted
/// dtype, so int/int floor-divides and raises on int/0), `true_div`
/// ALWAYS yields a floating result: integer / boolean operands are cast
/// to `Float64` BEFORE dividing (per [`true_div_dtype`]). Therefore
///   - `int / int → float64` true-division (`[1,2,3]/[2] → [0.5,1,1.5]`,
///     NOT integer `[0,1,1]`);
///   - `int / 0 → IEEE +inf` and `0 / 0 → NaN` (NumPy's RuntimeWarning,
///     NEVER an `IntegerDivisionByZero` error);
///   - `float32 / float32 → float32`, any `float64 → float64`, all IEEE.
///
/// This is the operator surface for `a / b` on `coil.Buffer` (ADR-0077
/// Phase 1 completion): `/` is `true_divide`, matching NumPy's `/`. The
/// `f32`/`f64` arms are total (IEEE division never errors), so `true_div`
/// is infallible for the floating promoted dtype — the `Result` is kept
/// only for the shared `binary_dispatch` signature + the broadcast-shape
/// `Err` (an incompatible shape is the ONLY error path).
pub fn true_div(a: &Array, b: &Array) -> Result<Array, NumpyError> {
    let promoted = crate::promote::true_div_dtype(a.dtype(), b.dtype());
    binary_dispatch(
        a,
        b,
        promoted,
        // Integer arms are UNREACHABLE: `true_div_dtype` promotes every
        // integer/bool operand to Float64, so `binary_dispatch` only ever
        // takes the f32/f64 arm. They are present solely to satisfy the
        // shared closure arity; if a caller somehow reached them (it
        // cannot — the promoted dtype is always floating), fall back to
        // wrapping integer division to stay total (no panic).
        |x, y| Ok(if y == 0 { 0 } else { x.wrapping_div(y) }),
        |x, y| Ok(if y == 0 { 0 } else { x.wrapping_div(y) }),
        // The live arms — IEEE 754 true-division. `x / 0.0 → ±inf`,
        // `0.0 / 0.0 → NaN`, never a trap (constitution §2.2 / numpy `/`).
        |x, y| x / y,
        |x, y| x / y,
        // bool/bool is unreachable (bool promotes to Float64); keep total.
        |x, _y| x,
    )
}

/// Element-wise power. Float follows `f64::powf` / `f32::powf`. Integer
/// follows numpy: `0**0 = 1`, negative exponents on integer dtypes
/// yield 0 (numpy behavior — int**negative truncates to 0).
pub fn pow(a: &Array, b: &Array) -> Result<Array, NumpyError> {
    let promoted = result_type(a.dtype(), b.dtype());
    binary_dispatch(
        a,
        b,
        promoted,
        |x, y| {
            // Numpy: int ** negative returns 0 (truncated).
            if y < 0 {
                Ok(0)
            } else {
                Ok(x.wrapping_pow(u32::try_from(y).unwrap_or(0)))
            }
        },
        |x, y| {
            if y < 0 {
                Ok(0)
            } else {
                Ok(x.wrapping_pow(u32::try_from(y).unwrap_or(0)))
            }
        },
        f32::powf,
        f64::powf,
        // bool ** bool: numpy promotes both to int, but on our tier
        // we keep it within bool: 0**0 = 1 = true; otherwise x.
        |x, y| if y { x } else { true },
    )
}

// ---- Public comparison ops ---------------------------------------------

pub fn eq(a: &Array, b: &Array) -> Result<Array, NumpyError> {
    cmp_dispatch(
        a,
        b,
        |x, y| x == y,
        |x, y| x == y,
        |x, y| x == y,
        |x, y| x == y,
        |x, y| x == y,
    )
}

pub fn ne(a: &Array, b: &Array) -> Result<Array, NumpyError> {
    cmp_dispatch(
        a,
        b,
        |x, y| x != y,
        |x, y| x != y,
        |x, y| x != y,
        |x, y| x != y,
        |x, y| x != y,
    )
}

pub fn lt(a: &Array, b: &Array) -> Result<Array, NumpyError> {
    cmp_dispatch(
        a,
        b,
        |x, y| x < y,
        |x, y| x < y,
        |x, y| x < y,
        |x, y| x < y,
        |x, y| !x & y,
    )
}

pub fn le(a: &Array, b: &Array) -> Result<Array, NumpyError> {
    cmp_dispatch(
        a,
        b,
        |x, y| x <= y,
        |x, y| x <= y,
        |x, y| x <= y,
        |x, y| x <= y,
        |x, y| !x | y,
    )
}

pub fn gt(a: &Array, b: &Array) -> Result<Array, NumpyError> {
    cmp_dispatch(
        a,
        b,
        |x, y| x > y,
        |x, y| x > y,
        |x, y| x > y,
        |x, y| x > y,
        |x, y| x & !y,
    )
}

pub fn ge(a: &Array, b: &Array) -> Result<Array, NumpyError> {
    cmp_dispatch(
        a,
        b,
        |x, y| x >= y,
        |x, y| x >= y,
        |x, y| x >= y,
        |x, y| x >= y,
        |x, y| x | !y,
    )
}

// ---- Public unary math ops ---------------------------------------------

/// Element-wise unary math. Integer inputs are promoted to Float64
/// per `unary_math_dtype`; float inputs preserve dtype.
fn unary_float_op(
    arr: &Array,
    op_f32: impl Fn(f32) -> f32,
    op_f64: impl Fn(f64) -> f64,
) -> Result<Array, NumpyError> {
    let target = unary_math_dtype(arr.dtype());
    let casted = cast_to(arr, target);
    Ok(match casted {
        Array::Float32(a) => Array::Float32(a.mapv(op_f32)),
        Array::Float64(a) => Array::Float64(a.mapv(op_f64)),
        _ => unreachable!("unary_math_dtype always returns Float32 or Float64"),
    })
}

pub fn sin(a: &Array) -> Result<Array, NumpyError> {
    unary_float_op(a, f32::sin, f64::sin)
}

pub fn cos(a: &Array) -> Result<Array, NumpyError> {
    unary_float_op(a, f32::cos, f64::cos)
}

pub fn exp(a: &Array) -> Result<Array, NumpyError> {
    unary_float_op(a, f32::exp, f64::exp)
}

pub fn log(a: &Array) -> Result<Array, NumpyError> {
    unary_float_op(a, f32::ln, f64::ln)
}

pub fn sqrt(a: &Array) -> Result<Array, NumpyError> {
    unary_float_op(a, f32::sqrt, f64::sqrt)
}

// ---- Stream W item 7: is* predicates (numpy `lib/_type_check_impl.py`) ------
//
// `@py_compat(strict)` — these are exact boolean predicates, no
// tolerance. Each returns a `Dtype::Bool` array of the same shape as
// the input (scalar-shape is preserved). Per numpy:
//   - `isnan(x)`  : element is NaN.
//   - `isinf(x)`  : element is +inf or -inf.
//   - `iscomplex(x)`: element has a nonzero imaginary part.
//   - `isreal(x)` : element has a zero imaginary part.
//
// The cobrust-coil `Array` tagged-union is real-only (the M7.6
// `Dtype::Complex*` widening did not extend `Array` — see ADR-0021 §3).
// Therefore `iscomplex` always yields all-`false` and `isreal` always
// all-`true` on any `Array` we can hold, which is exactly numpy's
// answer for real-dtype inputs. A complex-`Array` widening is a
// deferred follow-on (would make `iscomplex` check `imag != 0` per
// element); flagged in the module doc + report.

/// Map every element of a float array through a `f64 -> bool` predicate,
/// producing a `Dtype::Bool` array of the same shape. Integer / bool
/// inputs (which can never be NaN or inf) short-circuit to all-`false`.
fn float_predicate(arr: &Array, pred: impl Fn(f64) -> bool) -> Array {
    match arr {
        Array::Float32(a) => Array::Bool(a.mapv(|v| pred(f64::from(v)))),
        Array::Float64(a) => Array::Bool(a.mapv(pred)),
        // Integers and bools are always finite, never NaN.
        Array::Int32(a) => Array::Bool(a.mapv(|_| false)),
        Array::Int64(a) => Array::Bool(a.mapv(|_| false)),
        Array::Bool(a) => Array::Bool(a.mapv(|_| false)),
    }
}

/// `numpy.isnan(x)`-equivalent. Element-wise NaN test. Integer / bool
/// inputs are always `false` (matches numpy). Returns a `Dtype::Bool`
/// array of the same shape.
///
/// `@py_compat(strict)`.
///
/// # Errors
/// Currently total — never errors.
pub fn isnan(a: &Array) -> Result<Array, NumpyError> {
    Ok(float_predicate(a, f64::is_nan))
}

/// `numpy.isinf(x)`-equivalent. Element-wise `±inf` test. Integer /
/// bool inputs are always `false`. Returns a `Dtype::Bool` array.
///
/// `@py_compat(strict)`.
///
/// # Errors
/// Currently total.
pub fn isinf(a: &Array) -> Result<Array, NumpyError> {
    Ok(float_predicate(a, f64::is_infinite))
}

/// `numpy.iscomplex(x)`-equivalent. Element-wise "has nonzero imaginary
/// part" test. The cobrust-coil `Array` is real-only, so this always
/// yields all-`false` — which matches numpy for every real-dtype input
/// (`np.iscomplex([1,2,3])` → `[False, False, False]`). Returns a
/// `Dtype::Bool` array of the same shape.
///
/// `@py_compat(strict)` (for the real-dtype inputs `Array` can hold).
///
/// # Errors
/// Currently total.
pub fn iscomplex(a: &Array) -> Result<Array, NumpyError> {
    Ok(constant_bool(a, false))
}

/// `numpy.isreal(x)`-equivalent. Element-wise "has zero imaginary part"
/// test. The cobrust-coil `Array` is real-only, so this always yields
/// all-`true` — which matches numpy for every real-dtype input
/// (`np.isreal([1,2,3])` → `[True, True, True]`; note numpy also treats
/// `NaN` as real). Returns a `Dtype::Bool` array of the same shape.
///
/// `@py_compat(strict)` (for the real-dtype inputs `Array` can hold).
///
/// # Errors
/// Currently total.
pub fn isreal(a: &Array) -> Result<Array, NumpyError> {
    Ok(constant_bool(a, true))
}

/// Produce a `Dtype::Bool` array of `a`'s shape filled with `value`.
fn constant_bool(a: &Array, value: bool) -> Array {
    Array::Bool(ndarray::ArrayD::<bool>::from_elem(
        ndarray::IxDyn(&a.shape()),
        value,
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::cast_possible_truncation)]
    #![allow(clippy::cast_possible_wrap)]
    #![allow(clippy::cast_precision_loss)]
    #![allow(clippy::cast_sign_loss)]
    #![allow(clippy::format_push_string)]
    #![allow(clippy::let_unit_value)]
    #![allow(clippy::ignored_unit_patterns)]
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::expect_used)]
    #![allow(clippy::float_cmp)]
    #![allow(clippy::imprecise_flops)]
    #![allow(clippy::suboptimal_flops)]
    #![allow(clippy::similar_names)]
    #![allow(clippy::approx_constant)]
    #![allow(clippy::uninlined_format_args)]
    use super::*;
    use crate::constructors::{array_f32, array_f64, array_i32, array_i64};

    #[test]
    fn add_int32_int32_preserves_int32() {
        let a = array_i32(&[1, 2, 3], &[3]).unwrap();
        let b = array_i32(&[10, 20, 30], &[3]).unwrap();
        let c = add(&a, &b).unwrap();
        assert_eq!(c.dtype(), Dtype::Int32);
        if let Array::Int32(arr) = c {
            assert_eq!(arr.iter().copied().collect::<Vec<i32>>(), vec![11, 22, 33]);
        } else {
            panic!("expected Int32");
        }
    }

    #[test]
    fn add_int32_float64_promotes_to_float64() {
        let a = array_i32(&[1, 2, 3], &[3]).unwrap();
        let b = array_f64(&[0.5, 1.5, 2.5], &[3]).unwrap();
        let c = add(&a, &b).unwrap();
        assert_eq!(c.dtype(), Dtype::Float64);
        if let Array::Float64(arr) = c {
            let v: Vec<f64> = arr.iter().copied().collect();
            assert_eq!(v, vec![1.5, 3.5, 5.5]);
        } else {
            panic!("expected Float64");
        }
    }

    #[test]
    fn div_int_by_zero_errors() {
        let a = array_i32(&[1, 2, 3], &[3]).unwrap();
        let b = array_i32(&[1, 0, 3], &[3]).unwrap();
        let err = div(&a, &b).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::IntegerDivisionByZero);
    }

    #[test]
    fn div_float_by_zero_yields_inf() {
        let a = array_f64(&[1.0, -1.0, 0.0], &[3]).unwrap();
        let b = array_f64(&[0.0, 0.0, 0.0], &[3]).unwrap();
        let c = div(&a, &b).unwrap();
        if let Array::Float64(arr) = c {
            let v: Vec<f64> = arr.iter().copied().collect();
            assert!(v[0].is_infinite() && v[0] > 0.0);
            assert!(v[1].is_infinite() && v[1] < 0.0);
            assert!(v[2].is_nan());
        } else {
            panic!("expected Float64");
        }
    }

    #[test]
    fn broadcast_shape_size_one_axis_works() {
        // [3, 1] + [1, 4] → [3, 4]
        let a = array_i32(&[1, 2, 3], &[3, 1]).unwrap();
        let b = array_i32(&[10, 20, 30, 40], &[1, 4]).unwrap();
        let c = add(&a, &b).unwrap();
        assert_eq!(c.shape(), vec![3, 4]);
    }

    #[test]
    fn broadcast_shape_mismatch_errors() {
        let a = array_i32(&[1, 2, 3], &[3]).unwrap();
        let b = array_i32(&[10, 20, 30, 40], &[4]).unwrap();
        let err = add(&a, &b).unwrap_err();
        assert_eq!(err.kind, NumpyErrorKind::BroadcastShapeMismatch);
    }

    #[test]
    fn sin_int_promotes_to_f64() {
        let a = array_i32(&[0, 1, 2], &[3]).unwrap();
        let c = sin(&a).unwrap();
        assert_eq!(c.dtype(), Dtype::Float64);
    }

    #[test]
    fn lt_returns_bool_array() {
        let a = array_i32(&[1, 2, 3], &[3]).unwrap();
        let b = array_i32(&[2, 2, 2], &[3]).unwrap();
        let c = lt(&a, &b).unwrap();
        assert_eq!(c.dtype(), Dtype::Bool);
        if let Array::Bool(arr) = c {
            let v: Vec<bool> = arr.iter().copied().collect();
            assert_eq!(v, vec![true, false, false]);
        } else {
            panic!("expected Bool");
        }
    }

    // ---- Stream W item 7: is* predicates --------------------------------
    // Oracle: numpy 2.0.2.

    fn as_bool(a: &Array) -> Vec<bool> {
        if let Array::Bool(arr) = a {
            arr.iter().copied().collect()
        } else {
            panic!("expected Bool dtype, got {:?}", a.dtype());
        }
    }

    #[test]
    fn isnan_mixed_array() {
        // np.isnan([1,nan,inf,-inf,0]) -> [F,T,F,F,F]
        let a = array_f64(
            &[1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0],
            &[5],
        )
        .unwrap();
        let r = isnan(&a).unwrap();
        assert_eq!(r.dtype(), Dtype::Bool);
        assert_eq!(as_bool(&r), vec![false, true, false, false, false]);
    }

    #[test]
    fn isnan_int_array_all_false() {
        // np.isnan(int array) -> all False
        let a = array_i32(&[1, 2, 3], &[3]).unwrap();
        let r = isnan(&a).unwrap();
        assert_eq!(as_bool(&r), vec![false, false, false]);
    }

    #[test]
    fn isnan_f32_array() {
        let a = array_f32(&[1.0, f32::NAN, 2.0], &[3]).unwrap();
        let r = isnan(&a).unwrap();
        assert_eq!(as_bool(&r), vec![false, true, false]);
    }

    #[test]
    fn isinf_mixed_array() {
        // np.isinf([1,nan,inf,-inf,0]) -> [F,F,T,T,F]
        let a = array_f64(
            &[1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0],
            &[5],
        )
        .unwrap();
        let r = isinf(&a).unwrap();
        assert_eq!(as_bool(&r), vec![false, false, true, true, false]);
    }

    #[test]
    fn isinf_int_array_all_false() {
        let a = array_i64(&[1, 2, 3], &[3]).unwrap();
        let r = isinf(&a).unwrap();
        assert_eq!(as_bool(&r), vec![false, false, false]);
    }

    #[test]
    fn iscomplex_real_array_all_false() {
        // np.iscomplex([1,2,3]) -> [F,F,F]
        let a = array_i32(&[1, 2, 3], &[3]).unwrap();
        let r = iscomplex(&a).unwrap();
        assert_eq!(r.dtype(), Dtype::Bool);
        assert_eq!(as_bool(&r), vec![false, false, false]);
        // float array too
        let f = array_f64(&[1.0, 2.0], &[2]).unwrap();
        assert_eq!(as_bool(&iscomplex(&f).unwrap()), vec![false, false]);
    }

    #[test]
    fn isreal_real_array_all_true() {
        // np.isreal([1,2,3]) -> [T,T,T]
        let a = array_i32(&[1, 2, 3], &[3]).unwrap();
        let r = isreal(&a).unwrap();
        assert_eq!(r.dtype(), Dtype::Bool);
        assert_eq!(as_bool(&r), vec![true, true, true]);
        // numpy treats NaN as real too
        let f = array_f64(&[1.0, f64::NAN], &[2]).unwrap();
        assert_eq!(as_bool(&isreal(&f).unwrap()), vec![true, true]);
    }

    #[test]
    fn is_predicates_preserve_shape() {
        let a = array_f64(&[1.0, f64::NAN, 2.0, 3.0], &[2, 2]).unwrap();
        assert_eq!(isnan(&a).unwrap().shape(), vec![2, 2]);
        assert_eq!(isinf(&a).unwrap().shape(), vec![2, 2]);
        assert_eq!(iscomplex(&a).unwrap().shape(), vec![2, 2]);
        assert_eq!(isreal(&a).unwrap().shape(), vec![2, 2]);
    }
}
