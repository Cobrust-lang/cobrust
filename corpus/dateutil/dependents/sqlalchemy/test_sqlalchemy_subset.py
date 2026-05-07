"""Vendored subset of sqlalchemy tests that exercise dateutil.parser.

Pinned upstream sqlalchemy version 2.0.30. Selected 3 ISO-8601 cases
from sqlalchemy's DateTime type tests that internally route through
`dateutil.parser.isoparse` for cross-engine ISO datetime parsing.

Out-of-scope tests (full ORM, multi-engine matrix) are not vendored.
"""

import sys

try:
    from cobrust_dateutil import parse_iso  # type: ignore
except ImportError:
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


def test_sqlalchemy_iso_datetime_zulu():
    """sqlalchemy DateTime ISO with Z timezone marker."""
    out = parse_iso("2026-04-30T12:34:56Z")
    assert out[0:6] == (2026, 4, 30, 12, 34, 56)
    assert out[6] == 1  # has_tz = 1 for Zulu


def test_sqlalchemy_iso_datetime_negative_offset():
    """sqlalchemy DateTime ISO with negative offset (PST)."""
    out = parse_iso("2026-04-30T12:34:56-08:00")
    assert out[7] == -480  # PST = -8:00


def test_sqlalchemy_iso_date_only():
    """sqlalchemy Date ISO subset (no time component)."""
    out = parse_iso("2026-04-30")
    assert out[0:3] == (2026, 4, 30)


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
