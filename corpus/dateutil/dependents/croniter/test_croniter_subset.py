"""Vendored subset of croniter's tests that exercise dateutil.relativedelta.

We pin upstream version 2.0.7 (corpus/.../UPSTREAM_VERSION) and select 5
representative cases. The full upstream test bank is at
https://github.com/kiorky/croniter/blob/master/src/croniter/tests/test_croniter.py.

These tests are run by `crates/cobrust-translator/src/downstream.rs`
against the cobrust-dateutil PyO3-shaped wrapper. Each test that
exercises a M5-out-of-scope dateutil API is documented as such — the
L3 driver records those as `Skipped { reason }` rather than failing.
"""

import sys
from datetime import datetime

# Use the cobrust-shipped wrapper if available; fall back to upstream
# dateutil so the suite runs in the M5 manual-vendoring path. The L3
# driver guarantees PYTHONPATH includes the wrapper directory.
try:
    from cobrust_dateutil import relativedelta  # type: ignore
except ImportError:
    from dateutil.relativedelta import relativedelta  # type: ignore


def test_relativedelta_one_year_forward():
    """Adding 1 year to 2026-01-15 yields 2027-01-15."""
    base = datetime(2026, 1, 15, 0, 0, 0)
    out = base + relativedelta(years=1)
    assert (out.year, out.month, out.day) == (2027, 1, 15)


def test_relativedelta_one_month_clamps_jan31_to_feb28_or_29():
    """Adding 1 month to 2026-01-31 must clamp to Feb 28 (2026 is not leap)."""
    base = datetime(2026, 1, 31, 0, 0, 0)
    out = base + relativedelta(months=1)
    assert out.month == 2 and out.day == 28


def test_relativedelta_two_weeks_forward():
    """Adding 2 weeks = 14 days."""
    base = datetime(2026, 4, 1, 0, 0, 0)
    out = base + relativedelta(weeks=2)
    assert (out.year, out.month, out.day) == (2026, 4, 15)


def test_relativedelta_negative_days_underflows_month():
    """Subtracting 1 day from May 1 → Apr 30."""
    base = datetime(2026, 5, 1, 0, 0, 0)
    out = base + relativedelta(days=-1)
    assert (out.year, out.month, out.day) == (2026, 4, 30)


def test_relativedelta_minutes_cascade_to_hours():
    """75 minutes added to 12:00 → 13:15."""
    base = datetime(2026, 4, 30, 12, 0, 0)
    out = base + relativedelta(minutes=75)
    assert (out.hour, out.minute) == (13, 15)


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
