"""Upstream-style pytest cases for the relativedelta_core subset (M5 scope window)."""

from corpus.dateutil.upstream.relativedelta_core import relativedelta_add


def test_add_years():
    assert relativedelta_add(2026, 4, 30, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0)[0:3] == (2027, 4, 30)


def test_add_months_overflows_to_next_year():
    assert relativedelta_add(2026, 11, 1, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0)[0:3] == (2027, 2, 1)


def test_february_clamp_on_leap_year():
    # 2028 is a leap year — Feb 29 exists.
    assert relativedelta_add(2026, 1, 31, 0, 0, 0, 2, 1, 0, 0, 0, 0, 0)[0:3] == (2028, 2, 29)


def test_add_weeks():
    assert relativedelta_add(2026, 4, 1, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0)[0:3] == (2026, 4, 15)


def test_add_seconds_cascades_minutes():
    assert relativedelta_add(2026, 4, 30, 0, 0, 0, 0, 0, 0, 0, 0, 0, 125)[3:6] == (0, 2, 5)


def test_negative_days_underflow_month():
    assert relativedelta_add(2026, 5, 1, 0, 0, 0, 0, 0, 0, -1, 0, 0, 0)[0:3] == (2026, 4, 30)


if __name__ == "__main__":
    test_add_years()
    test_add_months_overflows_to_next_year()
    test_february_clamp_on_leap_year()
    test_add_weeks()
    test_add_seconds_cascades_minutes()
    test_negative_days_underflow_month()
    print("ok")
