// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: numpy 2.0.2
// oracle: cpython 3.11 (module: numpy)
// scope: M7.0 ndarray foundation per ADR-0013
// see PROVENANCE.toml for the full manifest.

//! numpy-compatible `repr()` formatting per ADR-0013 §4.
//!
//! Produces output of the form `array([nested_data], dtype=<py_name>)`
//! that is **informationally equivalent** to numpy's
//! `numpy.array_repr(arr)` (same dtype name, same shape, same values
//! to 17 significant figures for float64) but **does not** reproduce
//! numpy's column-aligned multi-line layout. The differential gate
//! uses `to_json` for bytewise comparison; `repr` is for human
//! display.

// CQ P1-4 + template-fix: single consolidated block; future emits use #[allow] at item level.
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

use crate::array::Array;

fn format_int_nested<T: core::fmt::Display>(data: &[T], shape: &[usize]) -> String {
    nested_format(data, shape, |v, out| out.push_str(&v.to_string()))
}

fn format_float_nested<T: core::fmt::Display>(data: &[T], shape: &[usize]) -> String {
    // numpy's float repr is intricate (auto-trims trailing zeros at
    // certain widths). M7.0 ships a stable cobrust-flavored repr per
    // ADR-0013 §4 that uses Rust's Display, which agrees on integer
    // values (`1.0`, `0.5`) and is bit-stable for float comparisons.
    nested_format(data, shape, |v, out| out.push_str(&v.to_string()))
}

fn format_bool_nested(data: &[bool], shape: &[usize]) -> String {
    nested_format(data, shape, |v, out| {
        let s = if *v { "True" } else { "False" };
        out.push_str(s);
    })
}

fn nested_format<T, F>(data: &[T], shape: &[usize], mut emit: F) -> String
where
    F: FnMut(&T, &mut String),
{
    let mut out = String::new();
    if shape.is_empty() {
        if let Some(v) = data.first() {
            emit(v, &mut out);
        }
        return out;
    }
    write_nested(data, shape, 0, &mut out, &mut emit);
    out
}

fn write_nested<T, F>(data: &[T], shape: &[usize], axis: usize, out: &mut String, emit: &mut F)
where
    F: FnMut(&T, &mut String),
{
    if axis + 1 == shape.len() {
        out.push('[');
        for (i, v) in data.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            emit(v, out);
        }
        out.push(']');
        return;
    }
    let dim = shape[axis];
    let inner: usize = shape[axis + 1..].iter().product();
    out.push('[');
    for i in 0..dim {
        if i > 0 {
            out.push_str(", ");
        }
        let lo = i * inner;
        let hi = lo + inner;
        write_nested(&data[lo..hi], shape, axis + 1, out, emit);
    }
    out.push(']');
}

/// Compose the cobrust-coil repr text for an `Array`. Format:
/// `array(<body>, dtype=<py_name>)`.
#[must_use]
pub fn array_repr(arr: &Array) -> String {
    let dtype = arr.dtype();
    let shape = arr.shape();
    let body = match arr {
        Array::Int32(a) => {
            let flat: Vec<i32> = a.iter().copied().collect();
            format_int_nested(&flat, &shape)
        }
        Array::Int64(a) => {
            let flat: Vec<i64> = a.iter().copied().collect();
            format_int_nested(&flat, &shape)
        }
        Array::Float32(a) => {
            let flat: Vec<f32> = a.iter().copied().collect();
            format_float_nested(&flat, &shape)
        }
        Array::Float64(a) => {
            let flat: Vec<f64> = a.iter().copied().collect();
            format_float_nested(&flat, &shape)
        }
        Array::Bool(a) => {
            let flat: Vec<bool> = a.iter().copied().collect();
            format_bool_nested(&flat, &shape)
        }
    };
    format!("array({body}, dtype={dtype})")
}
