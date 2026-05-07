"""Upstream-style pytest cases for the parser_core subset (M5 scope window)."""

from corpus.dateutil.upstream.parser_core import parse_iso, ParserError


def test_date_only():
    assert parse_iso("2026-04-30") == (2026, 4, 30, 0, 0, 0, 0, 0, 10)


def test_naive_datetime():
    assert parse_iso("2026-04-30T12:34:56") == (2026, 4, 30, 12, 34, 56, 0, 0, 19)


def test_zulu_datetime():
    assert parse_iso("2026-04-30T12:34:56Z") == (2026, 4, 30, 12, 34, 56, 1, 0, 20)


def test_positive_offset():
    assert parse_iso("2026-04-30T12:34:56+05:30") == (
        2026, 4, 30, 12, 34, 56, 2, 330, 25,
    )


def test_negative_offset():
    assert parse_iso("2026-04-30T12:34:56-08:00") == (
        2026, 4, 30, 12, 34, 56, 2, -480, 25,
    )


def test_empty_string_rejected():
    raised = False
    try:
        parse_iso("")
    except ParserError:
        raised = True
    assert raised


def test_short_string_rejected():
    raised = False
    try:
        parse_iso("2026-04")
    except ParserError:
        raised = True
    assert raised


def test_bad_month_rejected():
    raised = False
    try:
        parse_iso("2026-13-30")
    except ParserError:
        raised = True
    assert raised


def test_trailing_garbage_rejected():
    raised = False
    try:
        parse_iso("2026-04-30T12:34:56X")
    except ParserError:
        raised = True
    assert raised


if __name__ == "__main__":
    test_date_only()
    test_naive_datetime()
    test_zulu_datetime()
    test_positive_offset()
    test_negative_offset()
    test_empty_string_rejected()
    test_short_string_rejected()
    test_bad_month_rejected()
    test_trailing_garbage_rejected()
    print("ok")
