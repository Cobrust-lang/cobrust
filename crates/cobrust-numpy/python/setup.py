# SPDX-License-Identifier: Apache-2.0 OR MIT
# Auto-generated for cobrust-numpy. DO NOT EDIT BY HAND.
#
# To build the native extension (M7.0 ndarray foundation):
#   cargo build -p cobrust-numpy --features pyo3 --release
#
# The resulting `target/release/libcobrust_numpy.{dylib,so,dll}` can
# be loaded directly via `ctypes` or wrapped as a Python wheel via
# maturin (M7+ tooling). M7.0 ships only the build path; `setup.py`
# stays a placeholder until M7+ formalises the wheel publication.

from setuptools import setup

setup(
    name="cobrust-numpy",
    version="0.0.1.dev0",
    py_modules=["numpy_init"],
)
