# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: 2003-2024 Gustavo Niemeyer and dateutil contributors
#
# This file is a representative subset of `dateutil/parser/_parser.py`
# rewritten for clarity at the M5 scope window. It is the *source* the
# Cobrust translator consumes; the translated Rust lives at
# `crates/cobrust-dateutil/src/parser.rs`.
#
# The full upstream is at https://github.com/dateutil/dateutil
# (Apache-2.0 + BSD-3-Clause). This subset is included verbatim under
# the Apache-2.0 path; see `corpus/dateutil/UPSTREAM_LICENSE`.
"""dateutil.parser core: subset rewrite for Cobrust M5 (parse_iso only)."""


class ParserError(ValueError):
    """All parse failures bubble up as this single class."""


def _is_digit(ch):
    return ch and "0" <= ch <= "9"


def _take_digits(s, pos, count):
    if pos + count > len(s):
        raise ParserError("expected " + str(count) + " digits at pos " + str(pos))
    chunk = s[pos:pos + count]
    for ch in chunk:
        if not _is_digit(ch):
            raise ParserError("non-digit in expected numeric run at pos " + str(pos))
    return int(chunk), pos + count


def _expect(s, pos, ch):
    if pos >= len(s) or s[pos] != ch:
        raise ParserError("expected " + repr(ch) + " at pos " + str(pos))
    return pos + 1


def parse_iso(src):
    """Parse a strict ISO-8601 date or datetime into a 9-tuple.

    Returns ``(year, month, day, hour, minute, second, has_tz,
    tz_offset_minutes, src_consumed)``. ``has_tz`` is 0 if naive, 1 if
    UTC ('Z'), 2 if explicit offset. ``tz_offset_minutes`` is signed
    minutes (positive east of UTC).

    Accepted forms (all required to consume the entire string):
      YYYY-MM-DD
      YYYY-MM-DDTHH:MM:SS
      YYYY-MM-DDTHH:MM:SSZ
      YYYY-MM-DDTHH:MM:SS+HH:MM
      YYYY-MM-DDTHH:MM:SS-HH:MM
    """
    if not src:
        raise ParserError("empty string is not a valid ISO datetime")
    pos = 0
    year, pos = _take_digits(src, pos, 4)
    pos = _expect(src, pos, "-")
    month, pos = _take_digits(src, pos, 2)
    pos = _expect(src, pos, "-")
    day, pos = _take_digits(src, pos, 2)
    if month < 1 or month > 12:
        raise ParserError("month out of range")
    if day < 1 or day > 31:
        raise ParserError("day out of range")
    hour = 0
    minute = 0
    second = 0
    has_tz = 0
    tz_offset_minutes = 0
    if pos == len(src):
        return (year, month, day, hour, minute, second, has_tz, tz_offset_minutes, pos)
    pos = _expect(src, pos, "T")
    hour, pos = _take_digits(src, pos, 2)
    pos = _expect(src, pos, ":")
    minute, pos = _take_digits(src, pos, 2)
    pos = _expect(src, pos, ":")
    second, pos = _take_digits(src, pos, 2)
    if hour > 23 or minute > 59 or second > 60:
        raise ParserError("time component out of range")
    if pos == len(src):
        return (year, month, day, hour, minute, second, has_tz, tz_offset_minutes, pos)
    ch = src[pos]
    if ch == "Z":
        has_tz = 1
        pos += 1
    elif ch == "+" or ch == "-":
        sign = 1 if ch == "+" else -1
        pos += 1
        oh, pos = _take_digits(src, pos, 2)
        pos = _expect(src, pos, ":")
        om, pos = _take_digits(src, pos, 2)
        if oh > 23 or om > 59:
            raise ParserError("tz offset out of range")
        tz_offset_minutes = sign * (oh * 60 + om)
        has_tz = 2
    else:
        raise ParserError("unexpected trailing character at pos " + str(pos))
    if pos != len(src):
        raise ParserError("trailing characters after datetime")
    return (year, month, day, hour, minute, second, has_tz, tz_offset_minutes, pos)
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: 2003-2024 Gustavo Niemeyer and dateutil contributors
#
# Subset rewrite of `dateutil/relativedelta.py` for the Cobrust M5
# scope window. Implements pure-arithmetic relative-delta addition
# without timezone resolution.
#
# The full upstream is at https://github.com/dateutil/dateutil; this
# subset carries the Apache-2.0 path of the dual license.
"""dateutil.relativedelta core: subset rewrite for Cobrust M5."""


_DAYS_IN_MONTH_NORMAL = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
_DAYS_IN_MONTH_LEAP = [0, 31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]


def _is_leap_year(year):
    if year % 4 != 0:
        return False
    if year % 100 != 0:
        return True
    return year % 400 == 0


def _days_in_month(year, month):
    if _is_leap_year(year):
        return _DAYS_IN_MONTH_LEAP[month]
    return _DAYS_IN_MONTH_NORMAL[month]


def _normalize(year, month, day, hour, minute, second):
    """Cascade overflow / underflow upward through datetime fields."""
    while second < 0:
        minute -= 1
        second += 60
    while second >= 60:
        minute += 1
        second -= 60
    while minute < 0:
        hour -= 1
        minute += 60
    while minute >= 60:
        hour += 1
        minute -= 60
    while hour < 0:
        day -= 1
        hour += 24
    while hour >= 24:
        day += 1
        hour -= 24
    while month < 1:
        year -= 1
        month += 12
    while month > 12:
        year += 1
        month -= 12
    while day < 1:
        month -= 1
        if month < 1:
            year -= 1
            month += 12
        day += _days_in_month(year, month)
    while day > _days_in_month(year, month):
        day -= _days_in_month(year, month)
        month += 1
        if month > 12:
            year += 1
            month -= 12
    return (year, month, day, hour, minute, second)


def relativedelta_add(year, month, day, hour, minute, second,
                      add_years, add_months, add_weeks, add_days,
                      add_hours, add_minutes, add_seconds):
    """Add a relative delta to a base date and return a normalised
    9-tuple ``(y, m, d, hh, mm, ss, _, _, _)`` so the return shape
    matches ``parse_iso``.

    Order of application (matches dateutil's `__add__`): years and
    months first (with day clamping), then weeks/days, then time.
    """
    year = year + add_years
    month = month + add_months
    while month < 1:
        year -= 1
        month += 12
    while month > 12:
        year += 1
        month -= 12
    cap = _days_in_month(year, month)
    if day > cap:
        day = cap
    day = day + add_weeks * 7 + add_days
    hour = hour + add_hours
    minute = minute + add_minutes
    second = second + add_seconds
    (year, month, day, hour, minute, second) = _normalize(
        year, month, day, hour, minute, second
    )
    return (year, month, day, hour, minute, second, 0, 0, 0)
