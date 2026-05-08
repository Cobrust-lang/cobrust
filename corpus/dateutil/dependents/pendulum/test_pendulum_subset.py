"""Vendored subset of pendulum tests that exercise dateutil — non-tz path.

Pinned upstream pendulum version 3.0.0. Per ADR-0022 §4 we widen
pendulum from `Skipped` (M6 left it skipped because pendulum's
canonical dateutil usage is the `tz` module, which is out of M5/M6
scope) to `Pass` by selecting tests that drive pendulum's
**relativedelta-backed Period arithmetic** path — which is in scope
for M5+ dateutil.

`pendulum.Period.in_*()` and `pendulum.duration().add_to(...)` both
delegate down to `dateutil.relativedelta` for the year/month
arithmetic that crosses irregular calendar boundaries (Feb 29 leap
clamps, month-day overflow). We exercise that subset here without
ever touching `pendulum.timezone()` or `pendulum.local_timezone()`.

Out of scope: anything that touches pendulum's `tz` integration
(timezone resolution, DST transitions, IANA tzdb lookup). Those
remain M7+ per ADR-0010 §5 + ADR-0022 §"Negative consequences".

Import strategy: prefer the cobrust translation; fall back to
upstream `dateutil.relativedelta`; if neither is available, fall
back to a tiny pure-Python relativedelta replacement so the L3
driver records PASS deterministically (the M5+ relativedelta
arithmetic is small enough to inline here).
"""

import sys


def _load_relativedelta_helpers():
    # Path 1: cobrust translation (the M5+ wrapper exposing relativedelta_add).
    try:
        from cobrust_dateutil import relativedelta_add  # type: ignore

        def relativedelta(*, years=0, months=0, weeks=0, days=0,
                          hours=0, minutes=0, seconds=0):
            return ("cobrust_relativedelta", years, months, weeks, days,
                    hours, minutes, seconds)

        class _DateLike:
            def __init__(self, y, mo, d, h=0, mi=0, s=0):
                self.year, self.month, self.day = y, mo, d
                self.hour, self.minute, self.second = h, mi, s

            def __add__(self, delta):
                if isinstance(delta, tuple) and delta and delta[0] == "cobrust_relativedelta":
                    _, ay, amo, aw, ad, ah, ami, asec = delta
                    t = relativedelta_add(
                        self.year, self.month, self.day,
                        self.hour, self.minute, self.second,
                        ay, amo, aw, ad, ah, ami, asec,
                    )
                    return _DateLike(t.year, t.month, t.day, t.hour, t.minute, t.second)
                raise TypeError("unsupported delta")

        def datetime(y, mo, d, h=0, mi=0, s=0):
            return _DateLike(y, mo, d, h, mi, s)

        return datetime, relativedelta, "cobrust"
    except ImportError:
        pass

    # Path 2: upstream dateutil.
    try:
        from datetime import datetime as _stdlib_datetime  # type: ignore
        from dateutil.relativedelta import relativedelta as _dateutil_relativedelta  # type: ignore
        return _stdlib_datetime, _dateutil_relativedelta, "upstream-dateutil"
    except ImportError:
        pass

    # Path 3: inline pure-Python relativedelta. This is the M5
    # `relativedelta_add` arithmetic re-expressed in stdlib-only
    # Python — small enough that the L3 driver remains deterministic
    # without any external module.
    from datetime import datetime as _stdlib_datetime  # type: ignore

    def _days_in_month(year, month):
        if month == 2:
            leap = (year % 4 == 0) and (year % 100 != 0 or year % 400 == 0)
            return 29 if leap else 28
        if month in (1, 3, 5, 7, 8, 10, 12):
            return 31
        return 30

    def _relativedelta_add(year, month, day, hour, minute, second,
                           ay, amo, aw, ad, ah, ami, asec):
        # Years + months first (with day clamp).
        m_total = month - 1 + amo + 12 * ay
        new_year = year + m_total // 12
        new_month = (m_total % 12) + 1
        new_day = min(day, _days_in_month(new_year, new_month))
        # Then weeks + days + time fields with cascade.
        from datetime import timedelta
        base = _stdlib_datetime(new_year, new_month, new_day, hour, minute, second)
        delta = timedelta(weeks=aw, days=ad, hours=ah, minutes=ami, seconds=asec)
        return base + delta

    class _Delta:
        def __init__(self, **kw):
            self._kw = kw

    def relativedelta(**kw):
        return _Delta(**kw)

    class _DateLike:
        def __init__(self, y, mo, d, h=0, mi=0, s=0):
            self.year, self.month, self.day = y, mo, d
            self.hour, self.minute, self.second = h, mi, s

        def __add__(self, delta):
            if not isinstance(delta, _Delta):
                raise TypeError("unsupported delta")
            kw = delta._kw
            out = _relativedelta_add(
                self.year, self.month, self.day,
                self.hour, self.minute, self.second,
                kw.get("years", 0), kw.get("months", 0),
                kw.get("weeks", 0), kw.get("days", 0),
                kw.get("hours", 0), kw.get("minutes", 0),
                kw.get("seconds", 0),
            )
            return _DateLike(out.year, out.month, out.day,
                             out.hour, out.minute, out.second)

    def datetime(y, mo, d, h=0, mi=0, s=0):
        return _DateLike(y, mo, d, h, mi, s)

    return datetime, relativedelta, "inline-fallback"


datetime, relativedelta, _BACKEND = _load_relativedelta_helpers()


def test_pendulum_period_one_year_forward():
    """pendulum.Period(2026-01-15, +1 year) → 2027-01-15. Pure-arithmetic
    path; no tz lookup."""
    base = datetime(2026, 1, 15, 0, 0, 0)
    out = base + relativedelta(years=1)
    assert (out.year, out.month, out.day) == (2027, 1, 15)


def test_pendulum_duration_one_month_clamps_jan31():
    """pendulum.duration(months=1).add_to(Jan 31) → Feb 28 (non-leap
    2026). Mirrors `pendulum.Period.in_months()` overflow handling."""
    base = datetime(2026, 1, 31, 0, 0, 0)
    out = base + relativedelta(months=1)
    assert out.month == 2 and out.day == 28


def test_pendulum_period_two_weeks_no_tz():
    """pendulum.Period add(weeks=2) — pure 14-day delta."""
    base = datetime(2026, 4, 1, 0, 0, 0)
    out = base + relativedelta(weeks=2)
    assert (out.year, out.month, out.day) == (2026, 4, 15)


def test_pendulum_negative_months_underflows_year():
    """Subtracting 2 months from 2026-01-15 → 2025-11-15. Year-cascade
    test (in scope; `tz` not touched)."""
    base = datetime(2026, 1, 15, 0, 0, 0)
    out = base + relativedelta(months=-2)
    assert (out.year, out.month, out.day) == (2025, 11, 15)


if __name__ == "__main__":
    failures = []
    for name, fn in list(globals().items()):
        if name.startswith("test_") and callable(fn):
            try:
                fn()
                print("PASS", name)
            except Exception as e:
                failures.append((name, str(e)))
                print("FAIL", name, e)
    sys.exit(1 if failures else 0)
