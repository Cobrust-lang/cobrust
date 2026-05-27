# SPDX-License-Identifier: Apache-2.0 OR MIT
# Auto-generated for cobrust-pit. DO NOT EDIT BY HAND.
#
# To build the native extension:
#   cargo build -p cobrust-pit --features pyo3 --release
#
# The resulting `target/release/libpit.{dylib,so,dll}` can be loaded
# directly via `ctypes` or wrapped as a Python wheel via maturin (M9+
# tooling). The ecosystem-batch sprint ships only the build path;
# `setup.py` stays a placeholder until M9+ formalises wheel publication.

from setuptools import setup

setup(
    name="cobrust-pit",
    version="0.0.1.dev0",
    py_modules=["pit_init"],
)
