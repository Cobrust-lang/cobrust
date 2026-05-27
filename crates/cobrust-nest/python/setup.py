# SPDX-License-Identifier: Apache-2.0 OR MIT
# Cobrust-nest — Python package setup (0.1.0-beta T1.1).
#
# Auto-generated for cobrust-nest (per ADR-0007). DO NOT EDIT BY HAND.
#
# 0.1.0-beta ships a subprocess wrapper that calls the
# `cobrust-nest-json` Rust binary built from
# `crates/cobrust-nest/src/bin/cobrust_nest_json.rs`. The wrapper
# package is `cobrust_nest` and lives next to this file; install via
# `pip install -e .` from this directory after running
# `cargo build --release -p cobrust-nest` from the workspace root.
#
# A native PyO3 extension is queued for M-batch+ per ADR-0011; the
# subprocess bridge keeps 0.1.0-beta shippable on stock Rust toolchains.
from setuptools import setup, find_packages

setup(
    name="cobrust-nest",
    version="2.0.1.dev0+cobrust.0.1.0-beta",
    description="Cobrust-translated tomli 2.0.1 — drop-in API-compatible wrapper.",
    long_description=(
        "Cobrust 0.1.0-beta — LLM-translated `tomli` 2.0.1 in Rust, "
        "exposed via a Python subprocess wrapper.\n\n"
        "Supports `cobrust_nest.loads(s) -> dict` and "
        "`cobrust_nest.load(fp) -> dict` per the upstream tomli API."
    ),
    long_description_content_type="text/plain",
    license="Apache-2.0 OR MIT",
    author="The Cobrust Project",
    packages=find_packages(),  # finds `cobrust_nest` package
    python_requires=">=3.9",
)
