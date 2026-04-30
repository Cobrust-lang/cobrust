#!/usr/bin/env bash
# Cobrust documentation coverage check.
#
# M0 placeholder: enforces that the three documentation trees exist with
# the expected README anchors and that ADR-0001 has landed. Future
# milestones extend this to a real "public-item ↔ triple-doc" mapping.
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
# (Matches `cobrust-<name>` directory under `crates/` against module file.)
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

echo "doc-coverage: M0 checks passed"
