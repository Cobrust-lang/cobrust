# SPDX-License-Identifier: Apache-2.0 OR MIT
# Auto-generated for cobrust-den. DO NOT EDIT BY HAND.
"""Cobrust den — translated PEP 249 DB-API 2.0 surface (PyO3 placeholder); translated from CPython sqlite3."""

# PEP 249 module globals.
apilevel = "2.0"
threadsafety = 1
paramstyle = "qmark"

# When built with `cargo build -p cobrust-den --features pyo3`, the
# extension exposes `connect(path)` returning a `Connection`, whose
# `.cursor()` yields a `Cursor` with `.execute(sql, params)`,
# `.fetchone() / .fetchmany(n) / .fetchall()`, `.rowcount`, and
# `.lastrowid` from the native module `den`. Without the
# feature, this stub is the only Python-side surface; the Rust lib is
# still importable from Rust crates.
