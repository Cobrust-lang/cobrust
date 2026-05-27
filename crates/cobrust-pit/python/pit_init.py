# SPDX-License-Identifier: Apache-2.0 OR MIT
# Auto-generated for cobrust-pit. DO NOT EDIT BY HAND.
"""Cobrust pit — translated Flask web-server surface (PyO3 placeholder); translated from flask 3.0."""

__version__ = "3.0.0+cobrust"

# When built with `cargo build -p cobrust-pit --features pyo3`, the
# extension exposes an `App` class from the native module `pit`:
#
#     from pit import App
#     app = App()
#     app.route("GET", "/", lambda req: (200, "hello, pit"))
#     app.run("127.0.0.1", 8080)
#
# Without the feature, this stub is the only Python-side surface; the
# Rust lib is still importable from Rust crates. The full Flask-parity
# wrapper (the `@app.route` decorator, Request/Response classes) lands
# with the `.cb`-source wiring follow-on.
