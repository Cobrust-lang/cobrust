//! PyO3 bindings for cobrust-dateutil.
//!
//! Gated by `--features pyo3` per ADR-0011 §3. When compiled with the
//! feature, this module exposes a `cobrust_dateutil` Python extension
//! whose public functions are `parse_iso(src: str) -> tuple` and
//! `relativedelta_add(*args: int) -> tuple`. The Python tuple shape
//! mirrors the cobrust [`crate::DateTuple`] field order.
//!
//! M6 lights this up to discharge ADR-0007 §"PyO3 wrapper layout"'s
//! "M5 will flip this on" comment — alongside the M5 dateutil L3 gate
//! that already validated the wrapper's behaviour via subprocess.

#![allow(clippy::needless_pass_by_value)]

use pyo3::prelude::*;

use crate::{parse_iso as native_parse_iso, relativedelta_add as native_relativedelta_add};

#[pyfunction]
fn parse_iso(
    _py: Python<'_>,
    src: &str,
) -> PyResult<(i32, i32, i32, i32, i32, i32, i32, i32, usize)> {
    let dt = native_parse_iso(src)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("{e}")))?;
    Ok((
        dt.year,
        dt.month,
        dt.day,
        dt.hour,
        dt.minute,
        dt.second,
        dt.has_tz,
        dt.tz_offset_minutes,
        dt.consumed,
    ))
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn relativedelta_add(
    _py: Python<'_>,
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    minute: i32,
    second: i32,
    add_years: i32,
    add_months: i32,
    add_weeks: i32,
    add_days: i32,
    add_hours: i32,
    add_minutes: i32,
    add_seconds: i32,
) -> PyResult<(i32, i32, i32, i32, i32, i32, i32, i32, usize)> {
    let dt = native_relativedelta_add(
        year,
        month,
        day,
        hour,
        minute,
        second,
        add_years,
        add_months,
        add_weeks,
        add_days,
        add_hours,
        add_minutes,
        add_seconds,
    );
    Ok((
        dt.year,
        dt.month,
        dt.day,
        dt.hour,
        dt.minute,
        dt.second,
        dt.has_tz,
        dt.tz_offset_minutes,
        dt.consumed,
    ))
}

#[pymodule]
fn cobrust_dateutil(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_iso, m)?)?;
    m.add_function(wrap_pyfunction!(relativedelta_add, m)?)?;
    Ok(())
}
