"""Vendored subset of requests 2.31.0 — the M-batch L0 oracle.

Per ADR-0022 §1, this file holds only the surface that the M-batch
translation covers. It is **not** the upstream `requests` package
verbatim — it is a hand-pinned oracle subset that the L0 differential
harness drives against the cobrust translation. We re-implement the
six-verb dispatcher + `Session` + `Response` shape with stdlib only
(`urllib.request` / `urllib.parse` / `json`) so the harness has zero
runtime deps.

The full upstream surface (cookie jar, auth shims, streaming bodies)
is documented in `corpus/requests/README.md` as out of scope.
"""

from __future__ import annotations

import json as _json
import urllib.error
import urllib.parse
import urllib.request


class HttpError(Exception):
    """Single error class — mirrors the cobrust HttpError taxonomy.

    Subclasses (`InvalidUrlError`, `NetworkError`, `TimeoutError`,
    `DecodeBodyError`) keep the upstream `requests.exceptions`
    feel without dragging in the full hierarchy.
    """

    def __init__(self, kind: str, message: str):
        self.kind = kind
        self.message = message
        super().__init__(f"http {kind} error: {message}")


class Response:
    def __init__(self, status: int, headers: dict, body: bytes):
        self._status = status
        self._headers = headers
        self._body = body

    @property
    def status_code(self) -> int:
        return self._status

    def ok(self) -> bool:
        return 200 <= self._status < 300

    @property
    def headers(self) -> dict:
        return self._headers

    def text(self) -> str:
        try:
            return self._body.decode("utf-8")
        except UnicodeDecodeError as e:
            raise HttpError("decode body", str(e)) from e

    def json(self):
        try:
            return _json.loads(self._body.decode("utf-8"))
        except (ValueError, UnicodeDecodeError) as e:
            raise HttpError("decode body", str(e)) from e


def _parse_url(url: str):
    if not url or url.isspace():
        raise HttpError("invalid url", "empty url")
    parsed = urllib.parse.urlparse(url)
    if not parsed.scheme or not parsed.netloc:
        raise HttpError("invalid url", url)
    if parsed.scheme not in ("http", "https"):
        raise HttpError("invalid url", f"unsupported scheme {parsed.scheme}")
    return parsed


def _dispatch(method: str, url: str, body: bytes | None = None) -> Response:
    _ = _parse_url(url)
    req = urllib.request.Request(url, data=body, method=method)
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            headers = {k.lower(): v for k, v in resp.getheaders()}
            data = resp.read()
            return Response(resp.status, headers, data)
    except urllib.error.URLError as e:
        raise HttpError("network", str(e)) from e
    except TimeoutError as e:
        raise HttpError("timeout", str(e)) from e


class Session:
    def __init__(self):
        # urllib lives at module scope so the "session" is just a
        # marker for keep-alive intent; real keep-alive happens in
        # cobrust-requests via reqwest::blocking::Client.
        self._opener = urllib.request.build_opener()

    def get(self, url: str) -> Response:
        return _dispatch("GET", url)

    def post(self, url: str, body: bytes) -> Response:
        return _dispatch("POST", url, body)

    def put(self, url: str, body: bytes) -> Response:
        return _dispatch("PUT", url, body)

    def patch(self, url: str, body: bytes) -> Response:
        return _dispatch("PATCH", url, body)

    def delete(self, url: str) -> Response:
        return _dispatch("DELETE", url)

    def head(self, url: str) -> Response:
        return _dispatch("HEAD", url)


def get(url: str) -> Response:
    return Session().get(url)


def post(url: str, body: bytes) -> Response:
    return Session().post(url, body)


def put(url: str, body: bytes) -> Response:
    return Session().put(url, body)


def patch(url: str, body: bytes) -> Response:
    return Session().patch(url, body)


def delete(url: str) -> Response:
    return Session().delete(url)


def head(url: str) -> Response:
    return Session().head(url)
