#!/usr/bin/env python3.11
# SPDX-License-Identifier: Apache-2.0 OR MIT
# Cobrust 0.1.0-beta T1.1 — pip-tools downstream verification.
#
# Replays the tomli call surface pip-tools uses (`tomli.load(fp)` on
# pyproject.toml) against the LLM-translated cobrust-tomli wrapper.
# Compares each fixture's parsed structure to CPython's `tomllib`.
"""Drive cobrust_tomli through pip-tools-shaped fixtures and verify
behaviorally equivalent to CPython's tomllib."""
from __future__ import annotations

import json
import os
import sys
import tomllib
from pathlib import Path
from typing import Any

# Make `cobrust_tomli` importable from the workspace `python/`
# directory without requiring `pip install -e .`.
_HERE = Path(__file__).resolve().parent
_WORKSPACE = _HERE.parent.parent.parent
_PYTHON_PKG = _WORKSPACE / "crates" / "cobrust-tomli" / "python"
sys.path.insert(0, str(_PYTHON_PKG))

import cobrust_tomli  # noqa: E402

FIXTURES = [
    "pyproject.toml",
    "setuptools-style.toml",
    "poetry-style.toml",
    "pdm-style.toml",
]


def normalize(value: Any) -> Any:
    """Make values comparable across two TOML implementations.

    cobrust-tomli keeps order alphabetical (BTreeMap); CPython's
    tomllib uses dict insertion order. Convert dicts → sorted dicts
    recursively for comparison.
    """
    if isinstance(value, dict):
        return {k: normalize(value[k]) for k in sorted(value)}
    if isinstance(value, list):
        return [normalize(v) for v in value]
    return value


def run() -> int:
    """Run every fixture; return 0 on PASS, 1 on any divergence."""
    results: list[dict[str, Any]] = []
    failures = 0
    for fix_name in FIXTURES:
        fix_path = _HERE / "fixtures" / fix_name
        with fix_path.open("rb") as fp:
            cobrust_parsed = cobrust_tomli.load(fp)
        with fix_path.open("rb") as fp:
            oracle_parsed = tomllib.load(fp)

        cob_norm = normalize(cobrust_parsed)
        ora_norm = normalize(oracle_parsed)
        passed = cob_norm == ora_norm
        results.append(
            {
                "fixture": fix_name,
                "pass": passed,
                "size_bytes": fix_path.stat().st_size,
                "cobrust_keys": sorted(cobrust_parsed.keys()),
                "oracle_keys": sorted(oracle_parsed.keys()),
            }
        )
        if not passed:
            failures += 1
            print(f"FAIL fixture={fix_name}", file=sys.stderr)
            print(f"  cobrust : {json.dumps(cob_norm, indent=2)}", file=sys.stderr)
            print(f"  oracle  : {json.dumps(ora_norm, indent=2)}", file=sys.stderr)
        else:
            print(f"PASS fixture={fix_name} ({fix_path.stat().st_size} bytes)")

    # Also exercise loads() on a stringified fixture.
    sample_str = (_HERE / "fixtures" / "pyproject.toml").read_text(encoding="utf-8")
    cob_via_loads = cobrust_tomli.loads(sample_str)
    ora_via_loads = tomllib.loads(sample_str)
    loads_pass = normalize(cob_via_loads) == normalize(ora_via_loads)
    results.append(
        {
            "fixture": "loads-on-pyproject.toml",
            "pass": loads_pass,
            "size_bytes": len(sample_str),
            "cobrust_keys": sorted(cob_via_loads.keys()),
            "oracle_keys": sorted(ora_via_loads.keys()),
        }
    )
    if not loads_pass:
        failures += 1
        print("FAIL loads-on-pyproject.toml diverged", file=sys.stderr)
    else:
        print(f"PASS loads-on-pyproject.toml ({len(sample_str)} chars)")

    # Write the per-fixture verdict to result.md so the finding can
    # link to it.
    result_md = _HERE / "result.md"
    with result_md.open("w", encoding="utf-8") as fh:
        fh.write("# pip-tools downstream verdict — 0.1.0-beta T1.1\n\n")
        fh.write(f"OUTCOME: {'PASS' if failures == 0 else f'FAIL ({failures} divergences)'}\n\n")
        fh.write("| Fixture | Result | Bytes | cobrust keys | oracle keys |\n")
        fh.write("|---|---|---|---|---|\n")
        for r in results:
            fh.write(
                f"| `{r['fixture']}` | {'PASS' if r['pass'] else 'FAIL'} | "
                f"{r['size_bytes']} | `{', '.join(r['cobrust_keys'])}` | "
                f"`{', '.join(r['oracle_keys'])}` |\n"
            )
        fh.write("\n## Method\n\n")
        fh.write(
            "Each fixture is loaded through `cobrust_tomli.load(fp)` and "
            "through CPython's `tomllib.load(fp)`. Outputs are normalised "
            "(recursive dict-key sort) then compared. A FAIL row means the "
            "LLM-translated cobrust-tomli would have produced a different "
            "parse than CPython's tomllib for a real pyproject.toml shape "
            "pip-tools encounters in the wild.\n"
        )

    print(f"\nresult.md written to {result_md}")
    print(
        f"OUTCOME: {'PASS' if failures == 0 else f'FAIL ({failures} divergences)'}"
    )
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    sys.exit(run())
