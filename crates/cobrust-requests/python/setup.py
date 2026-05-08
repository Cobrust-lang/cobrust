# SPDX-License-Identifier: Apache-2.0 OR MIT
# Auto-generated for cobrust-requests. DO NOT EDIT BY HAND.
#
# To build the native extension:
#   cargo build -p cobrust-requests --features pyo3 --release
#
# The resulting `target/release/libcobrust_requests.{dylib,so,dll}` can
# be loaded directly via `ctypes` or wrapped as a Python wheel via
# maturin (M9+ tooling). The ecosystem-batch sprint ships only the
# build path; `setup.py` stays a placeholder until M9+ formalises wheel
# publication.

from setuptools import setup

setup(
    name="cobrust-requests",
    version="0.0.1.dev0",
    py_modules=["requests_init"],
)
