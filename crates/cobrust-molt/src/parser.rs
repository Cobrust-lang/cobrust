// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: dateutil 2.9.0.post0
// oracle: cpython 3.11 (module: dateutil)
// functions translated: 8
// see PROVENANCE.toml for the full manifest.

//! Translated parser body.
//!
//! Each emitted block carries its own per-function provenance comment.

// fn:days_in_month provider=synthetic model=dateutil-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1
// Module preamble emitted with the first alphabetic function
// (days_in_month). Subsequent entries emit only their function body.

use std::fmt;

/// One ISO-style 9-tuple as returned by both `parse_iso` and
/// `relativedelta_add`. The shape mirrors the Python tuple exactly so
/// the L0 differential gate can compare element-wise without further
/// decoding.
///
/// `has_tz`: 0 = naive, 1 = Zulu (UTC), 2 = explicit offset.
/// `tz_offset_minutes`: signed minutes east of UTC.
/// `consumed`: bytes of `src` consumed (only meaningful for
/// `parse_iso`; `relativedelta_add` returns 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTuple {
    pub year: i32,
    pub month: i32,
    pub day: i32,
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
    pub has_tz: i32,
    pub tz_offset_minutes: i32,
    pub consumed: usize,
}

impl DateTuple {
    /// Convert to a serde_json array — the L3 differential gate uses
    /// this to compare against CPython's vendored harness output.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!([
            self.year,
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second,
            self.has_tz,
            self.tz_offset_minutes,
            self.consumed,
        ])
    }
}

/// Single error type for dateutil parse failures. Mirrors
/// `parser_core.ParserError` (ValueError subclass in CPython).
#[derive(Clone, Debug)]
pub struct ParserError {
    pub message: String,
    pub pos: usize,
}

impl fmt::Display for ParserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "dateutil parse error at byte {}: {}",
            self.pos, self.message
        )
    }
}

impl std::error::Error for ParserError {}

impl ParserError {
    pub(crate) fn new(message: impl Into<String>, pos: usize) -> Self {
        Self {
            message: message.into(),
            pos,
        }
    }
}

const DAYS_IN_MONTH_NORMAL: [i32; 13] = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
const DAYS_IN_MONTH_LEAP: [i32; 13] = [0, 31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// Days-in-month accounting for leap years. Mirrors
/// `relativedelta_core._days_in_month`. Out-of-range months return 0.
#[must_use]
pub fn days_in_month(year: i32, month: i32) -> i32 {
    let table = if is_leap_year(year) {
        &DAYS_IN_MONTH_LEAP
    } else {
        &DAYS_IN_MONTH_NORMAL
    };
    if (1..=12).contains(&month) {
        // SAFETY-equivalent: month is in 1..=12, fits in usize on every supported platform.
        table[usize::try_from(month).unwrap_or(0)]
    } else {
        0
    }
}

// fn:expect_char provider=synthetic model=dateutil-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1

/// Assert `bytes[pos] == ch` and advance, raising ParserError otherwise.
/// Mirrors `parser_core._expect`.
pub(crate) fn expect_char(bytes: &[u8], pos: usize, ch: u8) -> Result<usize, ParserError> {
    if pos >= bytes.len() || bytes[pos] != ch {
        return Err(ParserError::new(
            format!("expected {:?}", char::from(ch)),
            pos,
        ));
    }
    Ok(pos + 1)
}

// fn:is_digit provider=synthetic model=dateutil-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1

/// ASCII digit predicate. Mirrors `parser_core._is_digit`.
#[must_use]
pub fn is_digit(ch: u8) -> bool {
    ch.is_ascii_digit()
}

// fn:is_leap_year provider=synthetic model=dateutil-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1

/// Gregorian leap-year predicate. Mirrors `relativedelta_core._is_leap_year`.
#[must_use]
pub fn is_leap_year(year: i32) -> bool {
    if year % 4 != 0 {
        return false;
    }
    if year % 100 != 0 {
        return true;
    }
    year % 400 == 0
}

// fn:normalize_datetime provider=synthetic model=dateutil-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1

/// Cascade overflow / underflow upward through datetime fields until
/// each is in range. Mirrors `relativedelta_core._normalize`.
#[must_use]
pub fn normalize_datetime(
    mut year: i32,
    mut month: i32,
    mut day: i32,
    mut hour: i32,
    mut minute: i32,
    mut second: i32,
) -> (i32, i32, i32, i32, i32, i32) {
    while second < 0 {
        minute -= 1;
        second += 60;
    }
    while second >= 60 {
        minute += 1;
        second -= 60;
    }
    while minute < 0 {
        hour -= 1;
        minute += 60;
    }
    while minute >= 60 {
        hour += 1;
        minute -= 60;
    }
    while hour < 0 {
        day -= 1;
        hour += 24;
    }
    while hour >= 24 {
        day += 1;
        hour -= 24;
    }
    while month < 1 {
        year -= 1;
        month += 12;
    }
    while month > 12 {
        year += 1;
        month -= 12;
    }
    while day < 1 {
        month -= 1;
        if month < 1 {
            year -= 1;
            month += 12;
        }
        day += days_in_month(year, month);
    }
    while day > days_in_month(year, month) {
        day -= days_in_month(year, month);
        month += 1;
        if month > 12 {
            year += 1;
            month -= 12;
        }
    }
    (year, month, day, hour, minute, second)
}

// fn:parse_iso provider=synthetic model=dateutil-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1
// Note: this is the corrected (attempt = 2) emission. The attempt = 1
// canned response is intentionally broken for the M5 repair-loop demo
// — see `corpus/dateutil/canned_llm_responses.toml` and ADR-0008 §5.

/// Parse a strict ISO-8601 date or datetime. Mirrors
/// `parser_core.parse_iso`; returns a [`DateTuple`] whose layout
/// mirrors the Python 9-tuple element-wise.
///
/// Accepted forms:
/// - `YYYY-MM-DD`
/// - `YYYY-MM-DDTHH:MM:SS`
/// - `YYYY-MM-DDTHH:MM:SSZ`
/// - `YYYY-MM-DDTHH:MM:SS+HH:MM`
/// - `YYYY-MM-DDTHH:MM:SS-HH:MM`
///
/// # Errors
/// Returns [`ParserError`] for any malformed input. The differential
/// gate ensures CPython `parser_core.parse_iso` rejects the same
/// inputs.
pub fn parse_iso(src: &str) -> Result<DateTuple, ParserError> {
    let bytes = src.as_bytes();
    if bytes.is_empty() {
        return Err(ParserError::new(
            "empty string is not a valid ISO datetime",
            0,
        ));
    }
    let (year, pos) = take_digits(bytes, 0, 4)?;
    let pos = expect_char(bytes, pos, b'-')?;
    let (month, pos) = take_digits(bytes, pos, 2)?;
    let pos = expect_char(bytes, pos, b'-')?;
    let (day, pos) = take_digits(bytes, pos, 2)?;
    if !(1..=12).contains(&month) {
        return Err(ParserError::new("month out of range", pos));
    }
    if !(1..=31).contains(&day) {
        return Err(ParserError::new("day out of range", pos));
    }
    let mut hour = 0;
    let mut minute = 0;
    let mut second = 0;
    let mut has_tz = 0;
    let mut tz_offset_minutes = 0;
    if pos == bytes.len() {
        return Ok(DateTuple {
            year,
            month,
            day,
            hour,
            minute,
            second,
            has_tz,
            tz_offset_minutes,
            consumed: pos,
        });
    }
    let pos = expect_char(bytes, pos, b'T')?;
    let (h, pos) = take_digits(bytes, pos, 2)?;
    let pos = expect_char(bytes, pos, b':')?;
    let (m, pos) = take_digits(bytes, pos, 2)?;
    let pos = expect_char(bytes, pos, b':')?;
    let (s, mut pos) = take_digits(bytes, pos, 2)?;
    hour = h;
    minute = m;
    second = s;
    if hour > 23 || minute > 59 || second > 60 {
        return Err(ParserError::new("time component out of range", pos));
    }
    if pos == bytes.len() {
        return Ok(DateTuple {
            year,
            month,
            day,
            hour,
            minute,
            second,
            has_tz,
            tz_offset_minutes,
            consumed: pos,
        });
    }
    let ch = bytes[pos];
    if ch == b'Z' {
        has_tz = 1;
        pos += 1;
    } else if ch == b'+' || ch == b'-' {
        let sign: i32 = if ch == b'+' { 1 } else { -1 };
        pos += 1;
        let (oh, p2) = take_digits(bytes, pos, 2)?;
        let p3 = expect_char(bytes, p2, b':')?;
        let (om, p4) = take_digits(bytes, p3, 2)?;
        if oh > 23 || om > 59 {
            return Err(ParserError::new("tz offset out of range", p4));
        }
        tz_offset_minutes = sign * (oh * 60 + om);
        has_tz = 2;
        pos = p4;
    } else {
        return Err(ParserError::new("unexpected trailing character", pos));
    }
    if pos != bytes.len() {
        return Err(ParserError::new("trailing characters after datetime", pos));
    }
    Ok(DateTuple {
        year,
        month,
        day,
        hour,
        minute,
        second,
        has_tz,
        tz_offset_minutes,
        consumed: pos,
    })
}

// fn:relativedelta_add provider=synthetic model=dateutil-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1

/// Add a relative delta to a base date. Mirrors
/// `relativedelta_core.relativedelta_add`. Returns a normalised
/// [`DateTuple`] whose `has_tz`, `tz_offset_minutes`, `consumed`
/// fields are 0 — the shape is preserved so the L0 differential gate
/// can compare element-wise against the Python harness output.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn relativedelta_add(
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
) -> DateTuple {
    let mut year = year + add_years;
    let mut month = month + add_months;
    while month < 1 {
        year -= 1;
        month += 12;
    }
    while month > 12 {
        year += 1;
        month -= 12;
    }
    let cap = days_in_month(year, month);
    let mut day = day;
    if day > cap {
        day = cap;
    }
    let day = day + add_weeks * 7 + add_days;
    let hour = hour + add_hours;
    let minute = minute + add_minutes;
    let second = second + add_seconds;
    let (year, month, day, hour, minute, second) =
        normalize_datetime(year, month, day, hour, minute, second);
    DateTuple {
        year,
        month,
        day,
        hour,
        minute,
        second,
        has_tz: 0,
        tz_offset_minutes: 0,
        consumed: 0,
    }
}

// fn:take_digits provider=synthetic model=dateutil-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1

/// Read exactly `count` ASCII digits from `bytes` starting at `pos`.
/// Mirrors `parser_core._take_digits`.
pub(crate) fn take_digits(
    bytes: &[u8],
    pos: usize,
    count: usize,
) -> Result<(i32, usize), ParserError> {
    if pos + count > bytes.len() {
        return Err(ParserError::new(format!("expected {count} digits"), pos));
    }
    let mut value: i32 = 0;
    for i in 0..count {
        let b = bytes[pos + i];
        if !is_digit(b) {
            return Err(ParserError::new("non-digit in expected numeric run", pos));
        }
        value = value * 10 + i32::from(b - b'0');
    }
    Ok((value, pos + count))
}
