#!/usr/bin/env python3
"""M-AI.0 α Phase 2 — verify.py oracle for `cobrust.llm` Tier 1 tests.

ADR-0047a (Tier B P7-TEST mandate) requires every deterministic Tier 1
helper test to ship a sibling Python reference invocation that confirms
the Cobrust impl is faithful to the fixture contract — not just self-
consistent.

For M-AI.0 the Synthetic provider is a deterministic canned-response
fixture, so the Python "oracle" simply prints the same canned text the
Rust-side `SyntheticDouble` returns for the matching case. The Rust
test (Tier 1 #16) shells out to:

    python3 tests/llm_corpus_verify.py <test_case_name>

and asserts that the printed text matches the Cobrust output. This is
the ADR-0047a oracle-independence check: Cobrust and Python both report
the same fixture-anchored value, ruling out single-agent self-execution
defects (LC-100 F23-A pattern).

Contract:
  - Argv[1] is one of the Tier 1 test-case keys below.
  - Prints the deterministic fixture text to stdout (no trailing
    newline beyond what `print()` adds), exits 0.
  - Unknown case → prints nothing, exits non-zero.

Python 3.11+. No third-party deps (project standard per ADR-0047a §2.b).

Cross-references:
  - docs/agent/spike/m-ai-0-cobrust-llm-spike.md §"Test plan" Tier 1.
  - docs/agent/adr/0047a-verify-py-mandate.md §"verify.py contract".
  - crates/cobrust-stdlib/tests/llm_corpus.rs §Tier 1 #16
    (`test_llm_complete_blocking_verify_py_oracle_matches_synthetic_fixture`).
"""

import sys


# Deterministic synthetic-fixture text per Tier 1 case key.
#
# Keys mirror the Rust test names in `tests/llm_corpus.rs`. The text
# must match `Scripted::Ok(<text>)` value the Rust-side double is
# constructed with — DEV keeps these in lockstep when authoring the
# canned-response stubs.
CASES: dict[str, str] = {
    # Tier 1 #1
    "test_llm_complete_blocking_returns_synthetic_canned_text": "hello-from-synth",
    # Tier 1 #5
    "test_llm_complete_blocking_rate_limit_retries_then_succeeds": "after-retry",
    # Tier 1 #7
    "test_llm_dispatch_blocking_valid_task_routes_to_first_preferred": "from-alpha",
    # Tier 1 #9
    "test_llm_dispatch_blocking_consensus_strategy_routes_through_consensus_path": "consensus-answer",
    # Tier 1 #10 — concatenated stream-delta text
    "test_llm_stream_blocking_returns_ordered_chunks": "hello-world",
    # Tier 1 #13
    "test_llm_complete_blocking_utf8_multibyte_round_trip": "你好世界",
    # Tier 1 #14 / #15
    "test_llm_complete_blocking_concurrent_32_parallel_calls_safe": "parallel-ok",
    "test_llm_complete_blocking_writes_outcome_ok_to_ledger_after_32_parallel_calls": "ledger-ok",
}


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print(
            "usage: llm_corpus_verify.py <test_case_name>",
            file=sys.stderr,
        )
        return 2
    case = argv[1]
    text = CASES.get(case)
    if text is None:
        print(f"unknown case: {case}", file=sys.stderr)
        return 1
    # No trailing newline beyond what `print()` adds — the Rust assertion
    # uses `output.stdout.trim_end()` (or equivalent) when comparing.
    print(text)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
