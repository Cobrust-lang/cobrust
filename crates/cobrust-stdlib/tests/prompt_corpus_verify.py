#!/usr/bin/env python3
"""M-AI.1 α Phase 3 — verify.py oracle for `cobrust.prompt` Tier 1 tests.

ADR-0047a (Tier B P7-TEST mandate) requires every deterministic Tier 1
helper test to ship a sibling Python reference invocation that confirms
the Cobrust impl is faithful to the fixture contract — not just self-
consistent.

For M-AI.1 the five `prompt_*` helpers are pure string-manipulation
functions, so the Python oracle re-implements the same algorithm in
Python and prints the deterministic expected output for each fixture
case. The Rust test (Tier 1 #20) shells out to:

    python3 tests/prompt_corpus_verify.py <test_case_name>

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
  - docs/agent/spike/m-ai-1-cobrust-prompt-spike.md §"Test plan" Tier 1.
  - docs/agent/adr/0047a-verify-py-mandate.md §"verify.py contract".
  - crates/cobrust-stdlib/tests/prompt_corpus.rs §Tier 1 #20
    (`test_verify_py_oracle_matches_prompt_render_helper_output`).
"""

import sys


# =====================================================================
# Python reference implementations of the five prompt helpers.
# These match the spike §Decision 4 + 5 algorithms for the pure-fn cases.
# For llm_complete_structured, the synthetic fixture text is used
# (same pattern as llm_corpus_verify.py CASES dict).
# =====================================================================


def prompt_render(system: str, user: str, vars: list[str]) -> str:
    """Re-implements spike §Decision 4 algorithm in Python.

    Combines system + '\\n' + user, then applies single-pass {key}
    interpolation using even-indexed [k1, v1, k2, v2, ...] vars list.
    Unknown keys remain literal. '{{' / '}}' render as literal '{' / '}'.
    """
    # Build map from even-indexed pairs; later same-key overrides earlier.
    kv: dict[str, str] = {}
    i = 0
    while i + 1 < len(vars):
        kv[vars[i]] = vars[i + 1]
        i += 2
    # Silently drop trailing odd key (Decision 3 + 7).

    combined = system + "\n" + user
    out = []
    idx = 0
    while idx < len(combined):
        ch = combined[idx]
        if ch == '{' and idx + 1 < len(combined) and combined[idx + 1] == '{':
            out.append('{')
            idx += 2
        elif ch == '}' and idx + 1 < len(combined) and combined[idx + 1] == '}':
            out.append('}')
            idx += 2
        elif ch == '{':
            # Scan to matching '}'.
            end = idx + 1
            while end < len(combined) and combined[end] != '}':
                end += 1
            if end >= len(combined):
                # Unterminated '{' — emit rest as literal.
                out.append(combined[idx:])
                break
            key = combined[idx + 1:end]
            if key in kv:
                out.append(kv[key])
            else:
                out.append(combined[idx:end + 1])  # keep literal {key}
            idx = end + 1
        else:
            out.append(ch)
            idx += 1
    return "".join(out)


def prompt_format_few_shot(
    examples_in: list[str],
    examples_out: list[str],
    current_input: str,
) -> str:
    """Re-implements spike §Decision 5 canonical few-shot format.

    Renders "Input: <in>\\nOutput: <out>\\n\\n" for each min-length pair,
    then appends "Input: <current>\\nOutput:" trailer (no trailing newline).
    """
    n = min(len(examples_in), len(examples_out))
    out = []
    for i in range(n):
        out.append(f"Input: {examples_in[i]}\nOutput: {examples_out[i]}\n\n")
    out.append(f"Input: {current_input}\nOutput:")
    return "".join(out)


def prompt_format_system_user(system: str, user: str) -> str:
    """Returns '<system>\\n\\n<user>' verbatim."""
    return f"{system}\n\n{user}"


def prompt_escape_braces(text: str) -> str:
    """Escapes '{' → '{{' and '}' → '}}' in text."""
    return text.replace("{", "{{").replace("}", "}}")


# =====================================================================
# CASES dict — maps Rust test function names to deterministic expected
# output. Keys mirror the Rust test names in `tests/prompt_corpus.rs`.
# Pure-fn cases reuse the Python reference implementations above.
# For llm_complete_structured cases, the synthetic fixture text is used.
# =====================================================================

CASES: dict[str, str] = {
    # Tier 1 #1 — empty vars, just system+"\n"+user
    "test_prompt_render_helper_empty_vars_returns_system_newline_user": (
        prompt_render("sys", "usr", [])
    ),
    # Tier 1 #2 — single key/value substitution (the oracle test case)
    "test_prompt_render_helper_single_key_value_substitutes_correctly": (
        prompt_render(
            "You are an expert.",
            "Translate: {code}",
            ["code", "def foo(): pass"],
        )
    ),
    # Tier 1 #3 — multiple pairs substitution
    "test_prompt_render_helper_multiple_pairs_substitutes_all": (
        prompt_render(
            "Convert {lang} code.",
            "Input: {code}",
            ["lang", "python", "code", "x = 1"],
        )
    ),
    # Tier 1 #4 — unknown placeholder kept literal
    "test_prompt_render_helper_unknown_placeholder_keeps_literal": (
        prompt_render("sys", "Hello {unknown}", [])
    ),
    # Tier 1 #5 — double-brace escape renders literal braces
    "test_prompt_render_helper_double_brace_escape_renders_literal_braces": (
        prompt_render("sys", "{{literal}} braces", [])
    ),
    # Tier 1 #6 — odd-length vars: trailing key silently dropped
    "test_prompt_render_helper_odd_length_vars_drops_trailing_key_silently": (
        prompt_render(
            "sys",
            "Value: {key1} orphan: {orphan}",
            ["key1", "val1", "orphan"],
        )
    ),
    # Tier 1 #7 — empty system + empty user → "\n"
    "test_prompt_render_helper_empty_system_user_returns_newline": (
        prompt_render("", "", ["k", "v"])
    ),
    # Tier 1 #10 — one example pair produces canonical format
    "test_prompt_format_few_shot_helper_one_example_produces_canonical_format": (
        prompt_format_few_shot(["x = 1"], ["let x: i64 = 1"], "y = 2")
    ),
    # Tier 1 #11 — multiple examples
    "test_prompt_format_few_shot_helper_multiple_examples_emits_n_blocks_plus_trailer": (
        prompt_format_few_shot(
            ["x = 1", "y = 2"],
            ["let x: i64 = 1", "let y: i64 = 2"],
            "z = 3",
        )
    ),
    # Tier 1 #12 — empty examples → trailer only
    "test_prompt_format_few_shot_helper_empty_examples_emits_just_trailer": (
        prompt_format_few_shot([], [], "z = 3")
    ),
    # Tier 1 #15 — format_system_user canonical "<system>\n\n<user>"
    "test_prompt_format_system_user_helper_produces_canonical_format": (
        prompt_format_system_user(
            "You are a Cobrust expert.",
            "Translate this code.",
        )
    ),
    # Tier 1 #16 — escape_braces escapes correctly
    "test_prompt_escape_braces_helper_escapes_braces_correctly": (
        prompt_escape_braces("hello {world}")
    ),
    # Tier 1 #18 — llm_complete_structured synthetic fixture text
    "test_llm_complete_structured_helper_synthetic_provider_returns_canned_response": (
        "structured-canned"
    ),
    # Tier 1 #20 — oracle test case (same as #2 above)
    "test_verify_py_oracle_matches_prompt_render_helper_output": (
        prompt_render(
            "You are an expert.",
            "Translate: {code}",
            ["code", "def foo(): pass"],
        )
    ),
}


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print(
            "usage: prompt_corpus_verify.py <test_case_name>",
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
