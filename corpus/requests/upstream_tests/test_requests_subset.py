"""Upstream-derived test subset for the requests M-batch oracle.

These cases exercise the surface in `corpus/requests/upstream/
requests_subset.py`. The cobrust-requests Rust crate is L0-verified
against these cases; the bytes-identical guarantee is loose here
(HTTP responses include timestamps + non-deterministic Server
headers), so we assert structural identity via `Response.json()` /
`Response.status_code` / `Response.text()`.
"""

import sys
import os

HERE = os.path.dirname(os.path.abspath(__file__))
SHIPPED = os.path.join(HERE, "..", "upstream")
sys.path.insert(0, SHIPPED)

from requests_subset import HttpError, Response, get  # type: ignore


def test_response_observers_project_inner_state():
    r = Response(200, {"content-type": "application/json"}, b'{"x":1}')
    assert r.status_code == 200
    assert r.ok()
    assert r.headers["content-type"] == "application/json"


def test_response_text_decodes_utf8():
    r = Response(200, {}, "héllo".encode("utf-8"))
    assert r.text() == "héllo"


def test_response_json_decodes_payload():
    r = Response(200, {}, b'{"a":1,"b":"x"}')
    v = r.json()
    assert v["a"] == 1 and v["b"] == "x"


def test_response_json_rejects_malformed_payload():
    r = Response(200, {}, b"not json")
    try:
        r.json()
    except HttpError as e:
        assert e.kind == "decode body"
        return
    raise AssertionError("must raise HttpError")


def test_invalid_url_routes_to_invalid_url_kind():
    try:
        get("not a url")
    except HttpError as e:
        assert e.kind == "invalid url"
        return
    raise AssertionError("must raise HttpError")


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
