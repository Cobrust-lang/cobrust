# SPDX-License-Identifier: Apache-2.0 OR MIT
# Auto-generated for cobrust-den. DO NOT EDIT BY HAND.
#
# To build the native extension:
#   cargo build -p cobrust-den --features pyo3 --release
#
# The resulting `target/release/libden.{dylib,so,dll}` can
# be loaded directly via `ctypes` or wrapped as a Python wheel via
# maturin. The Z.7.c sprint ships only the build path; `setup.py` stays
# a placeholder until wheel publication is formalised.

from setuptools import setup

setup(
    name="cobrust-den",
    version="0.0.1.dev0",
    py_modules=["den_init"],
)
