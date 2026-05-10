# SPDX-License-Identifier: Apache-2.0 OR MIT
# Cobrust-tomli — Python package setup (0.1.0-beta T1.1).
#
# Auto-generated for cobrust-tomli (per ADR-0007). DO NOT EDIT BY HAND.
#
# 0.1.0-beta ships a subprocess wrapper that calls the
# `cobrust-tomli-json` Rust binary built from
# `crates/cobrust-tomli/src/bin/cobrust_tomli_json.rs`. The wrapper
# package is `cobrust_tomli` and lives next to this file; install via
# `pip install -e .` from this directory after running
# `cargo build --release -p cobrust-tomli` from the workspace root.
#
# A native PyO3 extension is queued for M-batch+ per ADR-0011; the
# subprocess bridge keeps 0.1.0-beta shippable on stock Rust toolchains.
from setuptools import setup, find_packages

setup(
    name="cobrust-tomli",
    version="2.0.1.dev0+cobrust.0.1.0-beta",
    description="Cobrust-translated tomli 2.0.1 — drop-in API-compatible wrapper.",
    long_description=(
        "Cobrust 0.1.0-beta — LLM-translated `tomli` 2.0.1 in Rust, "
        "exposed via a Python subprocess wrapper.\n\n"
        "Supports `cobrust_tomli.loads(s) -> dict` and "
        "`cobrust_tomli.load(fp) -> dict` per the upstream tomli API."
    ),
    long_description_content_type="text/plain",
    license="Apache-2.0 OR MIT",
    author="The Cobrust Project",
    packages=find_packages(),  # finds `cobrust_tomli` package
    python_requires=">=3.9",
)
