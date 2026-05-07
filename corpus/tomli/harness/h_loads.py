# SPDX-License-Identifier: MIT
# L0 differential harness for tomli.loads().
#
# Usage:
#   python3 corpus/tomli/harness/h_loads.py [--cases N]
#
# Output: JSON to stdout, one record per case:
#   {"case": "name", "input": "...", "expected": <dict|null>,
#    "ok": bool, "error": "..."?}
#
# This harness is invoked by the Rust integration test
# (`crates/cobrust-translator/tests/tomli_pipeline.rs`) and by the L3
# downstream gate (`crates/cobrust-tomli/tests/tomli_downstream.rs`) to
# obtain ground-truth oracle outputs. CPython's `tomllib` is the
# oracle.

import json
import sys
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
TESTS = HERE.parent / "upstream_tests"
sys.path.insert(0, str(TESTS))
import test_loads as TC   # noqa: E402


def run_positive():
    out = []
    for name, src, expected in TC.CASES:
        try:
            got = tomllib.loads(src)
            ok = (got == expected)
            out.append({
                "case": name,
                "kind": "positive",
                "input": src,
                "expected": expected,
                "oracle": got,
                "ok": ok,
            })
        except Exception as e:
            out.append({
                "case": name,
                "kind": "positive",
                "input": src,
                "expected": expected,
                "oracle": None,
                "error": str(e),
                "ok": False,
            })
    return out


def run_negative():
    out = []
    for name, src in TC.NEGATIVE_CASES:
        raised = False
        msg = None
        try:
            tomllib.loads(src)
        except Exception as e:
            raised = True
            msg = str(e)
        out.append({
            "case": name,
            "kind": "negative",
            "input": src,
            "raised": raised,
            "error": msg,
            "ok": raised,
        })
    return out


def main():
    records = run_positive() + run_negative()
    print(json.dumps(records, indent=2))


if __name__ == "__main__":
    main()
