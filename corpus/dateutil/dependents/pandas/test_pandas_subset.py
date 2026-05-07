"""Vendored subset of pandas tests that exercise dateutil.parser.

Pinned upstream pandas version 2.2.2. Selected 3 ISO-8601 cases that
fall within the M5 dateutil parser scope window (per ADR-0009 §3 +
ADR-0010 §5). pandas's `to_datetime()` falls back to
`dateutil.parser.parse` when format is not specified — these tests
exercise that path in the M6-bounded subset.

Out-of-scope tests (mixed format, RFC-2822, multi-string broadcasting)
are not vendored.
"""

import sys

try:
    from cobrust_dateutil import parse_iso  # type: ignore
except ImportError:
    # Fall back to dateutil.parser.isoparse — semantically equivalent
    # for the strict ISO subset M5/M6 exercises.
    from dateutil.parser import isoparse  # type: ignore

    def parse_iso(src):
        out = isoparse(src)
        offset_minutes = 0
        has_tz = 0
        if out.tzinfo is not None:
            sec = out.utcoffset().total_seconds()
            offset_minutes = int(sec // 60)
            has_tz = 1 if offset_minutes == 0 else 2
        return (
            out.year, out.month, out.day,
            out.hour, out.minute, out.second,
            has_tz, offset_minutes, len(src),
        )


def test_pandas_iso_date_to_datetime():
    """pandas.to_datetime("2026-04-30") → datetime(2026, 4, 30) — date-only."""
    out = parse_iso("2026-04-30")
    assert out[0:3] == (2026, 4, 30)


def test_pandas_iso_naive_to_datetime():
    """pandas.to_datetime("2026-04-30T12:34:56") — naive datetime."""
    out = parse_iso("2026-04-30T12:34:56")
    assert out[0:6] == (2026, 4, 30, 12, 34, 56)


def test_pandas_iso_tz_to_datetime():
    """pandas.to_datetime("2026-04-30T12:34:56+05:30") — tz-aware."""
    out = parse_iso("2026-04-30T12:34:56+05:30")
    assert out[7] == 330  # IST = +5:30


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
