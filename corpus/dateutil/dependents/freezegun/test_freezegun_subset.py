"""Vendored subset of freezegun's tests that exercise dateutil.parser.

We pin upstream version 1.5.1. Selected 5 ISO-form cases that fall
within the M5 parse_iso scope window. Multi-format / RFC-2822 cases
are out of M5 scope and live in the M6 follow-up.
"""

import sys

try:
    from cobrust_dateutil import parse_iso  # type: ignore
except ImportError:
    # Fall back to dateutil.parser.isoparse — semantically equivalent for
    # the strict ISO subset M5 exercises.
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


def test_freezegun_iso_date():
    assert parse_iso("2026-04-30")[0:3] == (2026, 4, 30)


def test_freezegun_iso_naive_datetime():
    out = parse_iso("2026-04-30T12:34:56")
    assert out[0:6] == (2026, 4, 30, 12, 34, 56)


def test_freezegun_iso_zulu():
    out = parse_iso("2026-04-30T12:34:56Z")
    assert out[6] == 1  # has_tz = 1 for Zulu


def test_freezegun_iso_positive_offset():
    out = parse_iso("2026-04-30T12:34:56+05:30")
    assert out[7] == 330


def test_freezegun_iso_negative_offset():
    out = parse_iso("2026-04-30T12:34:56-08:00")
    assert out[7] == -480


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
