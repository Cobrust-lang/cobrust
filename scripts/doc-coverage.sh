#!/usr/bin/env bash
# Cobrust documentation coverage check.
#
# Enforces the doc-coverage rule (constitution §3.3): every public
# item that we promise in the agent-tree module spec must also be
# discussed in the zh and en human-tree docs.
#
# - M0 added the directory + ADR-0001 baseline.
# - M1 added public-surface coverage for the frontend crate.
# - M2 extended to mod:hir and mod:types.
# - M3 extended to mod:llm_router.
# - M4 (this revision) extends to mod:translator and mod:tomli, plus
#   ADR-0007 acceptance + the synthetic-mode contract terms.
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
    tomli
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
        # Skip dotfiles (e.g. transient .cobrust cache from integration tests).
        [[ "$crate_name" == .* ]] && continue
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

# --- 6. M2 HIR surface coverage --------------------------------------------
m2_hir_terms=(
    "lower"
    "Session"
    "DefId"
    "ResolvedName"
    "LoweringError"
    "Module"
)

m2_hir_files=(
    "docs/agent/modules/hir.md"
    "docs/human/en/architecture.md"
    "docs/human/zh/architecture.md"
)

if grep -q '^- \*\*M2 — delivered.\*\*' "docs/agent/modules/hir.md"; then
    for term in "${m2_hir_terms[@]}"; do
        for f in "${m2_hir_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M2 hir surface term '${term}' missing from ${f}"
            fi
        done
    done
    adr_five="docs/agent/adr/0005-hir-shape.md"
    [[ -f "$adr_five" ]] || fail "ADR-0005 (HIR shape) is required for M2"
    if ! grep -q '^status: accepted$' "$adr_five"; then
        fail "ADR-0005 must be 'status: accepted' for M2 to be done"
    fi
fi

# --- 7. M2 type-checker surface coverage -----------------------------------
m2_types_terms=(
    "check"
    "Ty"
    "TypeError"
    "TypedModule"
)

m2_types_files=(
    "docs/agent/modules/types.md"
    "docs/human/en/architecture.md"
    "docs/human/zh/architecture.md"
)

if grep -q '^- \*\*M2 — delivered.\*\*' "docs/agent/modules/types.md"; then
    for term in "${m2_types_terms[@]}"; do
        for f in "${m2_types_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M2 types surface term '${term}' missing from ${f}"
            fi
        done
    done
    adr_six="docs/agent/adr/0006-type-system.md"
    [[ -f "$adr_six" ]] || fail "ADR-0006 (type system) is required for M2"
    if ! grep -q '^status: accepted$' "$adr_six"; then
        fail "ADR-0006 must be 'status: accepted' for M2 to be done"
    fi
fi

# --- 8. M4 translator + tomli surface coverage -----------------------------
# When the translator module declares M4 delivered, every public surface
# term + the synthetic-mode contract + the manifest schema must appear in
# all three doc trees.

m4_translator_terms=(
    "translate"
    "PyLibrary"
    "TranslatedCrate"
    "TranslatorConfig"
    "TranslatorError"
    "ProvenanceManifest"
    "SyntheticProvider"
    "deterministic_id"
    "synthetic-miss"
    "synthetic-stale"
    "source_sha16"
    "PROVENANCE.toml"
)

m4_translator_files=(
    "docs/agent/modules/translator.md"
    "docs/human/en/architecture.md"
    "docs/human/zh/architecture.md"
)

if grep -q '^- \*\*M4 — delivered.\*\*' "docs/agent/modules/translator.md"; then
    for term in "${m4_translator_terms[@]}"; do
        for f in "${m4_translator_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M4 translator surface term '${term}' missing from ${f}"
            fi
        done
    done

    m4_tomli_terms=(
        "loads"
        "Value"
        "TomliError"
        "table_to_json"
        "to_json"
    )
    m4_tomli_files=(
        "docs/agent/modules/tomli.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${m4_tomli_terms[@]}"; do
        for f in "${m4_tomli_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M4 tomli surface term '${term}' missing from ${f}"
            fi
        done
    done

    adr_seven="docs/agent/adr/0007-translator-pipeline.md"
    [[ -f "$adr_seven" ]] || fail "ADR-0007 (translator pipeline) is required for M4"
    if ! grep -q '^status: accepted$' "$adr_seven"; then
        fail "ADR-0007 must be 'status: accepted' for M4 to be done"
    fi

    # PROVENANCE.toml must exist on the generated crate.
    if [[ -d crates/cobrust-tomli ]]; then
        [[ -f crates/cobrust-tomli/PROVENANCE.toml ]] \
            || fail "crates/cobrust-tomli/PROVENANCE.toml missing — regenerate via COBRUST_REGENERATE_TOMLI=1"
    fi

    # Corpus directory layout per ADR-0007.
    [[ -f corpus/tomli/spec.toml ]] || fail "corpus/tomli/spec.toml missing"
    [[ -f corpus/tomli/canned_llm_responses.toml ]] || fail "corpus/tomli/canned_llm_responses.toml missing"
    [[ -d corpus/tomli/upstream ]] || fail "corpus/tomli/upstream missing"
    [[ -d corpus/tomli/upstream_tests ]] || fail "corpus/tomli/upstream_tests missing"
fi

echo "doc-coverage: M0 + M1 + M2 + M4 checks passed"
