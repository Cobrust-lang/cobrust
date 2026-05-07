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
