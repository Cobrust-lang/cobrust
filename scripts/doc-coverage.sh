#!/usr/bin/env bash
# Cobrust documentation coverage check.
#
# Enforces the doc-coverage rule (constitution §3.3): every public
# item that we promise in the agent-tree module spec must also be
# discussed in the zh and en human-tree docs.
#
# M0 added the directory + ADR-0001 baseline.
# M1 (this revision) adds public-surface coverage for the frontend
# crate by checking that the canonical entrypoint names appear in
# every doc tree.
#
# Future milestones extend the `m_<milestone>_checks` block.
#
# See `docs/agent/conventions.md` and constitution `CLAUDE.md` §3.

set -euo pipefail

cd "$(dirname "$0")/.."

fail() {
    echo "doc-coverage: $1" >&2
    exit 1
}

# --- 1. Required directories with README anchors -----------------------------
required_readmes=(
    "docs/human/zh/README.md"
    "docs/human/en/README.md"
    "docs/agent/README.md"
    "docs/agent/adr/README.md"
    "docs/agent/findings/README.md"
)

for path in "${required_readmes[@]}"; do
    [[ -f "$path" ]] || fail "missing $path"
done

# --- 2. Human-tree parity (zh/en filename one-to-one) ------------------------
zh_files=$(cd docs/human/zh && find . -name '*.md' | sort)
en_files=$(cd docs/human/en && find . -name '*.md' | sort)

if [[ "$zh_files" != "$en_files" ]]; then
    echo "doc-coverage: zh/en parity broken — file lists differ" >&2
    diff <(printf '%s\n' "$zh_files") <(printf '%s\n' "$en_files") >&2 || true
    exit 1
fi

# --- 3. Agent module specs match crate list ---------------------------------
expected_modules=(
    cli
    frontend
    hir
    types
    mir
    codegen
    llm-router
    translator
)

for mod in "${expected_modules[@]}"; do
    [[ -f "docs/agent/modules/${mod}.md" ]] \
        || fail "missing docs/agent/modules/${mod}.md"
done

# Reverse check: every workspace member crate has a matching module spec.
if [[ -d crates ]]; then
    while IFS= read -r crate_dir; do
        crate_name="$(basename "$crate_dir")"
        mod_name="${crate_name#cobrust-}"
        if [[ ! -f "docs/agent/modules/${mod_name}.md" ]]; then
            fail "crate ${crate_name} has no docs/agent/modules/${mod_name}.md"
        fi
    done < <(find crates -mindepth 1 -maxdepth 1 -type d)
fi

# --- 4. ADR-0001 must be accepted -------------------------------------------
adr_one="docs/agent/adr/0001-license.md"
[[ -f "$adr_one" ]] || fail "ADR-0001 (license) is required"
if ! grep -q '^status: accepted$' "$adr_one"; then
    fail "ADR-0001 must be 'status: accepted' for M0 to be done"
fi

# --- 5. M1 frontend surface coverage -----------------------------------------
# Every named public entrypoint of `cobrust-frontend` must appear in
# the agent-tree module spec, the en-tree architecture doc, and the
# zh-tree architecture doc.
#
# This is the §3.3 sync rule, mechanized.

m1_frontend_terms=(
    "lex"
    "lex_bytes"
    "parse"
    "parse_str"
    "unparse"
    "FileId"
    "Span"
)

m1_frontend_files=(
    "docs/agent/modules/frontend.md"
    "docs/human/en/architecture.md"
    "docs/human/zh/architecture.md"
)

# Skip the strict M1 coverage gate when the frontend module spec
# still declares "M0 — empty stub" — that means M1 has not been
# delivered yet on this branch.
if grep -q '^- M0 — empty stub.$' "docs/agent/modules/frontend.md"; then
    echo "doc-coverage: M0 baseline checks passed (M1 surface check skipped)"
    exit 0
fi

for term in "${m1_frontend_terms[@]}"; do
    for f in "${m1_frontend_files[@]}"; do
        if ! grep -q -F "${term}" "$f"; then
            fail "M1 frontend surface term '${term}' missing from ${f}"
        fi
    done
done

# ADR-0003 must be accepted now that M1 has landed.
adr_three="docs/agent/adr/0003-core-30-forms.md"
[[ -f "$adr_three" ]] || fail "ADR-0003 (core 30 forms) is required for M1"
if ! grep -q '^status: accepted$' "$adr_three"; then
    fail "ADR-0003 must be 'status: accepted' for M1 to be done"
fi

# Findings index must reference m1-fuzz-method since the gate uses it.
findings_index="docs/agent/findings/README.md"
if ! grep -q -F "m1-fuzz-method" "$findings_index"; then
    fail "findings/README.md must index m1-fuzz-method (M1 fuzz gate evidence)"
fi

echo "doc-coverage: M0 + M1 checks passed"
