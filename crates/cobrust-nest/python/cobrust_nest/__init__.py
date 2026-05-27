# SPDX-License-Identifier: Apache-2.0 OR MIT
# Cobrust-nest Python wrapper (translated from tomli 2.0.1).
#
# 0.1.0-beta T1.1 ships a subprocess wrapper that calls the
# `cobrust-nest-json` Rust binary from this workspace. The wrapper
# exposes the standard `loads(s) -> dict` and `load(fp) -> dict` API
# (mirroring tomli 2.0.1) so downstream Python tooling (pip-tools,
# poetry, etc.) can `import cobrust_nest as nest` and call
# `nest.loads(...)`.
#
# A native PyO3 extension is queued for M-batch+ per ADR-0011; the
# subprocess bridge keeps the 0.1.0-beta release shippable on stock
# Rust toolchains.
"""Cobrust-nest — Python wrapper for the LLM-translated tomli 2.0.1.

This module exposes a `loads`/`load` API mirroring Python's tomli:

    >>> import cobrust_nest as nest
    >>> nest.loads("x = 1\\n")
    {'x': 1}
    >>> with open("pyproject.toml", "rb") as fp:
    ...     nest.load(fp)

The implementation calls the `cobrust-nest-json` Rust binary built
from `crates/cobrust-nest/src/bin/cobrust_nest_json.rs`. By default
the binary path is auto-discovered relative to this file's location;
override via the `COBRUST_NEST_BINARY` env var.

The 0.1.0-beta release is the headline demo of Cobrust's AI-native
compiler translation closed loop — see
`docs/agent/findings/0.1.0-beta-tomli-full-translation.md`.
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any, BinaryIO

__all__ = ["loads", "load", "TOMLDecodeError"]
__version__ = "2.0.1+cobrust-0.1.0-beta"


class TOMLDecodeError(ValueError):
    """Raised when input is not valid TOML.

    Mirrors `tomli.TOMLDecodeError` for drop-in compatibility.
    """


def _binary_path() -> str:
    """Resolve the path to the `cobrust-nest-json` bridge binary.

    Search order:
    1. `COBRUST_NEST_BINARY` env var (absolute path).
    2. `<workspace>/target/release/cobrust-nest-json` (release build).
    3. `<workspace>/target/debug/cobrust-nest-json` (debug build).
    4. `cobrust-nest-json` on `PATH`.

    `<workspace>` is auto-derived as the parent of `python/`, three
    directories up from this file.
    """
    env_override = os.environ.get("COBRUST_NEST_BINARY")
    if env_override:
        if os.path.isfile(env_override):
            return env_override
        raise RuntimeError(
            f"COBRUST_NEST_BINARY={env_override} does not point to a file"
        )

    here = Path(__file__).resolve()
    # this/__init__.py is at:
    #   .../<workspace>/crates/cobrust-nest/python/cobrust_nest/__init__.py
    # walk up: __init__.py -> cobrust_nest/ -> python/ -> cobrust-nest/
    #          -> crates/ -> <workspace>/
    workspace = here.parent.parent.parent.parent.parent
    candidates = [
        workspace / "target" / "release" / "cobrust-nest-json",
        workspace / "target" / "debug" / "cobrust-nest-json",
    ]
    for c in candidates:
        if c.is_file():
            return str(c)

    # PATH fallback.
    return "cobrust-nest-json"


def _invoke(src: bytes) -> dict[str, Any]:
    """Invoke the Rust binary with `src` on stdin; parse JSON response."""
    binary = _binary_path()
    try:
        proc = subprocess.run(
            [binary],
            input=src,
            capture_output=True,
            check=False,
            timeout=120,
        )
    except FileNotFoundError as e:
        raise RuntimeError(
            f"cobrust-nest-json binary not found at {binary}. "
            "Run `cargo build --release -p cobrust-nest` from the workspace."
        ) from e
    if proc.returncode != 0:
        # Binary should never exit non-zero unless a panic / OS error.
        stderr = proc.stderr.decode("utf-8", errors="replace")
        raise RuntimeError(
            f"cobrust-nest-json exited {proc.returncode}: {stderr.strip() or '(empty stderr)'}"
        )

    payload_text = proc.stdout.decode("utf-8", errors="replace")
    try:
        payload = json.loads(payload_text)
    except json.JSONDecodeError as e:
        raise RuntimeError(
            f"cobrust-nest-json produced unparsable output: {payload_text!r}"
        ) from e

    if "ok" in payload:
        return payload["ok"]
    if "err" in payload:
        raise TOMLDecodeError(payload["err"])
    raise RuntimeError(f"cobrust-nest-json output had neither 'ok' nor 'err': {payload}")


def loads(s: str) -> dict[str, Any]:
    """Parse a TOML string `s` into a dict.

    Mirrors `tomli.loads()`. Accepts `str` only — passing `bytes`
    raises `TypeError` to match upstream tomli's contract (since
    tomli >= 2.0).

    Raises `TOMLDecodeError` on parse failure.
    """
    if not isinstance(s, str):
        raise TypeError(f"Expected str; got {type(s).__name__}")
    return _invoke(s.encode("utf-8"))


def load(fp: BinaryIO) -> dict[str, Any]:
    """Parse a TOML file-handle `fp` into a dict.

    Mirrors `tomli.load()`. `fp` MUST be opened in binary mode (`"rb"`)
    per upstream tomli's contract. Raises `TypeError` if `fp.read()`
    returns `str` instead of `bytes`.
    """
    data = fp.read()
    if not isinstance(data, (bytes, bytearray)):
        raise TypeError(
            f"File must be opened in binary mode; got {type(data).__name__}"
        )
    return _invoke(bytes(data))
