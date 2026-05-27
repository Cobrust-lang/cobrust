# SPDX-License-Identifier: Apache-2.0 OR MIT
# Auto-generated for cobrust-scale. DO NOT EDIT BY HAND.
#
# To build the native extension:
#   cargo build -p cobrust-scale --features pyo3 --release
#
# The resulting `target/release/libscale.{dylib,so,dll}` can
# be loaded directly via `ctypes` or wrapped as a Python wheel via
# maturin (M7+ tooling). M6 ships only the build path; `setup.py`
# stays a placeholder until M7+ formalises the wheel publication.

from setuptools import setup

setup(
    name="cobrust-scale",
    version="0.0.1.dev0",
    py_modules=["scale_init"],
)
