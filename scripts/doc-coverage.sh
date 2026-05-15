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
# - M4 extends to mod:translator and mod:tomli, plus ADR-0007
#   acceptance + the synthetic-mode contract terms.
# - M5 extends to mod:dateutil + ADR-0008 + ADR-0009.
# - M6 (this revision) extends to mod:msgpack + ADR-0010 + ADR-0011 +
#   the Cython shim + PerfVerifier surface + dateutil L3 widening.
# - ADR-0022 (M-batch) extends to mod:requests + mod:click + the
#   surface-translate / Rust-binding tier (0.8x) + dateutil L3 5/5
#   + msgpack L3 3/3 closures.
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
    dateutil
    msgpack
    requests
    click
    stdlib
    pkg
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

# --- 9. M5 translator + dateutil surface coverage ---------------------------
# When the translator module declares M5 delivered, every public surface
# term + the repair-loop contract + the perf-gate contract + the
# downstream-dependents contract must appear in all three doc trees.

m5_translator_terms=(
    "translate_with_verifier"
    "BehaviorVerifier"
    "VerifierVerdict"
    "GateFailure"
    "EscalationExceeded"
    "BenchmarkReport"
    "PerfTarget"
    "DownstreamReport"
    "DependentsSection"
    "repair_translation"
    "failure_report.md"
    "ADR-0008"
    "ADR-0009"
)

m5_translator_files=(
    "docs/agent/modules/translator.md"
    "docs/human/en/architecture.md"
    "docs/human/zh/architecture.md"
)

if grep -q '^- \*\*M5 — delivered.\*\*' "docs/agent/modules/translator.md"; then
    for term in "${m5_translator_terms[@]}"; do
        for f in "${m5_translator_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M5 translator surface term '${term}' missing from ${f}"
            fi
        done
    done

    m5_dateutil_terms=(
        "parse_iso"
        "relativedelta_add"
        "DateTuple"
        "ParserError"
    )
    m5_dateutil_files=(
        "docs/agent/modules/dateutil.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${m5_dateutil_terms[@]}"; do
        for f in "${m5_dateutil_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M5 dateutil surface term '${term}' missing from ${f}"
            fi
        done
    done

    adr_eight="docs/agent/adr/0008-l2-perf-and-repair-loop.md"
    [[ -f "$adr_eight" ]] || fail "ADR-0008 (L2.perf + repair loop) is required for M5"
    if ! grep -q '^status: accepted$' "$adr_eight"; then
        fail "ADR-0008 must be 'status: accepted' for M5 to be done"
    fi
    adr_nine="docs/agent/adr/0009-downstream-validation.md"
    [[ -f "$adr_nine" ]] || fail "ADR-0009 (downstream validation) is required for M5"
    if ! grep -q '^status: accepted$' "$adr_nine"; then
        fail "ADR-0009 must be 'status: accepted' for M5 to be done"
    fi

    # PROVENANCE.toml must exist on the generated dateutil crate.
    if [[ -d crates/cobrust-dateutil ]]; then
        [[ -f crates/cobrust-dateutil/PROVENANCE.toml ]] \
            || fail "crates/cobrust-dateutil/PROVENANCE.toml missing"
    fi

    # Corpus directory layout per ADR-0007 (extended for M5 dependents).
    [[ -f corpus/dateutil/spec.toml ]] || fail "corpus/dateutil/spec.toml missing"
    [[ -f corpus/dateutil/canned_llm_responses.toml ]] || fail "corpus/dateutil/canned_llm_responses.toml missing"
    [[ -d corpus/dateutil/upstream ]] || fail "corpus/dateutil/upstream missing"
    [[ -d corpus/dateutil/upstream_tests ]] || fail "corpus/dateutil/upstream_tests missing"
    [[ -d corpus/dateutil/dependents/croniter ]] || fail "corpus/dateutil/dependents/croniter missing"
    [[ -d corpus/dateutil/dependents/freezegun ]] || fail "corpus/dateutil/dependents/freezegun missing"
    [[ -f corpus/dateutil/perf.toml ]] || fail "corpus/dateutil/perf.toml missing"
fi

# --- 10. M6 translator + msgpack surface coverage --------------------------
# When the translator module declares M6 delivered, every public surface
# term + the Cython shim contract + the perf-verifier trait + ADR-0010
# anchors must appear in all three doc trees.

m6_translator_terms=(
    "translate_with_verifiers"
    "PerfVerifier"
    "PerfVerdict"
    "AcceptAllPerf"
    "translate_cython"
    "CythonSource"
    "CythonType"
    "ADR-0010"
    "ADR-0011"
)

m6_translator_files=(
    "docs/agent/modules/translator.md"
    "docs/human/en/architecture.md"
    "docs/human/zh/architecture.md"
)

if grep -q '^- \*\*M6 — delivered.\*\*' "docs/agent/modules/translator.md"; then
    for term in "${m6_translator_terms[@]}"; do
        for f in "${m6_translator_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M6 translator surface term '${term}' missing from ${f}"
            fi
        done
    done

    m6_msgpack_terms=(
        "pack_to_vec"
        "MsgValue"
        "MsgError"
        "pack_uint_cython"
        "unpack_uint_cython"
    )
    m6_msgpack_files=(
        "docs/agent/modules/msgpack.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${m6_msgpack_terms[@]}"; do
        for f in "${m6_msgpack_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M6 msgpack surface term '${term}' missing from ${f}"
            fi
        done
    done

    adr_ten="docs/agent/adr/0010-native-ext-translation.md"
    [[ -f "$adr_ten" ]] || fail "ADR-0010 (native-ext translation) is required for M6"
    if ! grep -q '^status: accepted$' "$adr_ten"; then
        fail "ADR-0010 must be 'status: accepted' for M6 to be done"
    fi
    adr_eleven="docs/agent/adr/0011-pyo3-build-path.md"
    [[ -f "$adr_eleven" ]] || fail "ADR-0011 (PyO3 build path) is required for M6"
    if ! grep -q '^status: accepted$' "$adr_eleven"; then
        fail "ADR-0011 must be 'status: accepted' for M6 to be done"
    fi

    # PROVENANCE.toml must exist on the generated msgpack crate.
    if [[ -d crates/cobrust-msgpack ]]; then
        [[ -f crates/cobrust-msgpack/PROVENANCE.toml ]]             || fail "crates/cobrust-msgpack/PROVENANCE.toml missing"
    fi

    # Corpus directory layout per ADR-0010.
    [[ -f corpus/msgpack/spec.toml ]] || fail "corpus/msgpack/spec.toml missing"
    [[ -f corpus/msgpack/canned_llm_responses.toml ]] || fail "corpus/msgpack/canned_llm_responses.toml missing"
    [[ -d corpus/msgpack/upstream ]] || fail "corpus/msgpack/upstream missing"
    [[ -d corpus/msgpack/upstream_tests ]] || fail "corpus/msgpack/upstream_tests missing"
    [[ -d corpus/msgpack/dependents/redis-py ]] || fail "corpus/msgpack/dependents/redis-py missing"
    [[ -d corpus/msgpack/dependents/msgpack-numpy ]] || fail "corpus/msgpack/dependents/msgpack-numpy missing"
    [[ -f corpus/msgpack/perf.toml ]] || fail "corpus/msgpack/perf.toml missing"

    # M6 dateutil widening: pandas + sqlalchemy + pendulum subsets.
    [[ -d corpus/dateutil/dependents/pandas ]] || fail "corpus/dateutil/dependents/pandas missing (M6 widening)"
    [[ -d corpus/dateutil/dependents/sqlalchemy ]] || fail "corpus/dateutil/dependents/sqlalchemy missing (M6 widening)"
    [[ -d corpus/dateutil/dependents/pendulum ]] || fail "corpus/dateutil/dependents/pendulum missing (M6 widening)"
fi

# --- 11. M7.0 translator + numpy surface coverage ---------------------------
# When the translator module declares M7.0 delivered, the cobrust-numpy
# surface terms + ADR-0012 + ADR-0013 anchors must appear in all three
# doc trees.

m7_0_translator_terms=(
    "translate the surface, bind the core"
    "ADR-0012"
    "ADR-0013"
)

m7_0_translator_files=(
    "docs/agent/modules/translator.md"
    "docs/human/en/architecture.md"
    "docs/human/zh/architecture.md"
)

if grep -q '^- \*\*M7.0 — delivered.\*\*' "docs/agent/modules/translator.md"; then
    for term in "${m7_0_translator_terms[@]}"; do
        for f in "${m7_0_translator_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M7.0 translator surface term '${term}' missing from ${f}"
            fi
        done
    done

    m7_0_numpy_terms=(
        "Array"
        "Dtype"
        "array"
        "zeros"
        "ones"
        "arange"
        "ndarray"
    )
    m7_0_numpy_files=(
        "docs/agent/modules/numpy.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${m7_0_numpy_terms[@]}"; do
        for f in "${m7_0_numpy_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M7.0 numpy surface term '${term}' missing from ${f}"
            fi
        done
    done

    adr_twelve="docs/agent/adr/0012-m7-numpy-plan.md"
    [[ -f "$adr_twelve" ]] || fail "ADR-0012 (M7 numpy plan) is required for M7.0"
    if ! grep -q '^status: accepted$' "$adr_twelve"; then
        fail "ADR-0012 must be 'status: accepted' for M7.0 to be done"
    fi
    adr_thirteen="docs/agent/adr/0013-m7-0-ndarray-foundation.md"
    [[ -f "$adr_thirteen" ]] || fail "ADR-0013 (M7.0 ndarray foundation) is required for M7.0"
    if ! grep -q '^status: accepted$' "$adr_thirteen"; then
        fail "ADR-0013 must be 'status: accepted' for M7.0 to be done"
    fi

    # PROVENANCE.toml must exist on the generated numpy crate.
    if [[ -d crates/cobrust-numpy ]]; then
        [[ -f crates/cobrust-numpy/PROVENANCE.toml ]] \
            || fail "crates/cobrust-numpy/PROVENANCE.toml missing"
    fi

    # Corpus directory layout per ADR-0013.
    [[ -f corpus/numpy/M7.0/spec.toml ]] || fail "corpus/numpy/M7.0/spec.toml missing"
    [[ -f corpus/numpy/M7.0/canned_llm_responses.toml ]] || fail "corpus/numpy/M7.0/canned_llm_responses.toml missing"
    [[ -d corpus/numpy/M7.0/upstream ]] || fail "corpus/numpy/M7.0/upstream missing"
    [[ -d corpus/numpy/M7.0/upstream_tests ]] || fail "corpus/numpy/M7.0/upstream_tests missing"
    [[ -d corpus/numpy/M7.0/harness ]] || fail "corpus/numpy/M7.0/harness missing"
    [[ -f corpus/numpy/M7.0/perf.toml ]] || fail "corpus/numpy/M7.0/perf.toml missing"
fi

echo "doc-coverage: M0 + M1 + M2 + M4 + M5 + M6 + M7.0 + M7.1 + M7.2 + M7.3 + M7.4 + M7.5 checks passed"

# --- 12. M7.1 ufunc + broadcasting + promotion surface coverage -----------
# When the numpy module declares M7.1 delivered, the ufunc + broadcasting
# + promotion surface terms + ADR-0014 anchors must appear in all three
# doc trees.

if grep -q '^- \*\*M7.1 — delivered.\*\*' "docs/agent/modules/numpy.md"; then
    m7_1_numpy_terms=(
        "add"
        "sub"
        "mul"
        "div"
        "broadcast_shape"
        "result_type"
        "sin"
        "cos"
        "exp"
        "log"
        "sqrt"
        "BroadcastShapeMismatch"
        "IntegerDivisionByZero"
        "NestedList"
        "array_i32"
        "array_f64"
        "ADR-0014"
    )
    m7_1_numpy_files=(
        "docs/agent/modules/numpy.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${m7_1_numpy_terms[@]}"; do
        for f in "${m7_1_numpy_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M7.1 numpy surface term '${term}' missing from ${f}"
            fi
        done
    done

    adr_fourteen="docs/agent/adr/0014-m7-1-ufuncs-broadcasting.md"
    [[ -f "$adr_fourteen" ]] || fail "ADR-0014 (M7.1 ufuncs + broadcasting) is required for M7.1"
    if ! grep -q '^status: accepted$' "$adr_fourteen"; then
        fail "ADR-0014 must be 'status: accepted' for M7.1 to be done"
    fi

    # Corpus directory layout per ADR-0014.
    [[ -f corpus/numpy/M7.1/spec.toml ]] || fail "corpus/numpy/M7.1/spec.toml missing"
    [[ -f corpus/numpy/M7.1/canned_llm_responses.toml ]] || fail "corpus/numpy/M7.1/canned_llm_responses.toml missing"
    [[ -d corpus/numpy/M7.1/upstream ]] || fail "corpus/numpy/M7.1/upstream missing"
    [[ -d corpus/numpy/M7.1/upstream_tests ]] || fail "corpus/numpy/M7.1/upstream_tests missing"
    [[ -d corpus/numpy/M7.1/harness ]] || fail "corpus/numpy/M7.1/harness missing"
    [[ -f corpus/numpy/M7.1/perf.toml ]] || fail "corpus/numpy/M7.1/perf.toml missing"
fi

echo "doc-coverage: M7.1 ufunc surface checks passed"

# --- 13. M7.2 indexing surface coverage -----------------------------------
# When the numpy module declares M7.2 delivered, the indexing + view + np.where
# surface terms + ADR-0015 anchors must appear in all three doc trees.

if grep -q '^- \*\*M7.2 — delivered.\*\*' "docs/agent/modules/numpy.md"; then
    m7_2_numpy_terms=(
        "Index"
        "SliceSpec"
        "ArrayView"
        "ArrayViewMut"
        "slice"
        "take"
        "mask"
        "np_where"
        "OutOfBoundsIndex"
        "BoolMaskShapeMismatch"
        "IndexDtypeNotInteger"
        "ADR-0015"
    )
    m7_2_numpy_files=(
        "docs/agent/modules/numpy.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${m7_2_numpy_terms[@]}"; do
        for f in "${m7_2_numpy_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M7.2 numpy surface term '${term}' missing from ${f}"
            fi
        done
    done

    adr_fifteen="docs/agent/adr/0015-m7-2-indexing.md"
    [[ -f "$adr_fifteen" ]] || fail "ADR-0015 (M7.2 indexing) is required for M7.2"
    if ! grep -q '^status: accepted$' "$adr_fifteen"; then
        fail "ADR-0015 must be 'status: accepted' for M7.2 to be done"
    fi

    # Corpus directory layout per ADR-0015.
    [[ -f corpus/numpy/M7.2/spec.toml ]] || fail "corpus/numpy/M7.2/spec.toml missing"
    [[ -f corpus/numpy/M7.2/canned_llm_responses.toml ]] || fail "corpus/numpy/M7.2/canned_llm_responses.toml missing"
    [[ -d corpus/numpy/M7.2/upstream ]] || fail "corpus/numpy/M7.2/upstream missing"
    [[ -d corpus/numpy/M7.2/harness ]] || fail "corpus/numpy/M7.2/harness missing"
    [[ -f corpus/numpy/M7.2/perf.toml ]] || fail "corpus/numpy/M7.2/perf.toml missing"
fi

echo "doc-coverage: M7.2 indexing surface checks passed"

# --- 14. M7.3 reduction surface coverage ----------------------------------
# When the numpy module declares M7.3 delivered, the reduction surface
# terms + ADR-0016 anchors must appear in all three doc trees.

if grep -q '^- \*\*M7.3 — delivered.\*\*' "docs/agent/modules/numpy.md"; then
    m7_3_numpy_terms=(
        "sum"
        "prod"
        "mean"
        "std"
        "var"
        "min"
        "max"
        "argmin"
        "argmax"
        "ReductionEmptyArray"
        "pairwise_sum"
        "ddof"
        "ADR-0016"
    )
    m7_3_numpy_files=(
        "docs/agent/modules/numpy.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${m7_3_numpy_terms[@]}"; do
        for f in "${m7_3_numpy_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M7.3 numpy surface term '${term}' missing from ${f}"
            fi
        done
    done

    adr_sixteen="docs/agent/adr/0016-m7-3-reductions.md"
    [[ -f "$adr_sixteen" ]] || fail "ADR-0016 (M7.3 reductions) is required for M7.3"
    if ! grep -q '^status: accepted$' "$adr_sixteen"; then
        fail "ADR-0016 must be 'status: accepted' for M7.3 to be done"
    fi

    # Corpus directory layout per ADR-0016.
    [[ -f corpus/numpy/M7.3/spec.toml ]] || fail "corpus/numpy/M7.3/spec.toml missing"
    [[ -f corpus/numpy/M7.3/canned_llm_responses.toml ]] || fail "corpus/numpy/M7.3/canned_llm_responses.toml missing"
    [[ -d corpus/numpy/M7.3/upstream ]] || fail "corpus/numpy/M7.3/upstream missing"
    [[ -d corpus/numpy/M7.3/harness ]] || fail "corpus/numpy/M7.3/harness missing"
    [[ -f corpus/numpy/M7.3/perf.toml ]] || fail "corpus/numpy/M7.3/perf.toml missing"
fi

echo "doc-coverage: M7.3 reduction surface checks passed"

# --- 15. M7.4 linalg surface coverage -------------------------------------
# When the numpy module declares M7.4 delivered, the linalg surface
# terms + ADR-0017 anchors must appear in all three doc trees.

if grep -q '^- \*\*M7.4 — delivered.\*\*' "docs/agent/modules/numpy.md"; then
    m7_4_numpy_terms=(
        "matmul"
        "dot"
        "det"
        "solve"
        "inv"
        "svd"
        "eigh"
        "cholesky"
        "SingularMatrix"
        "NotPositiveDefinite"
        "LinalgShapeError"
        "LinalgDtypeUnsupported"
        "SvdResult"
        "EighResult"
        "linalg-backend"
        "ADR-0017"
    )
    m7_4_numpy_files=(
        "docs/agent/modules/numpy.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${m7_4_numpy_terms[@]}"; do
        for f in "${m7_4_numpy_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M7.4 numpy surface term '${term}' missing from ${f}"
            fi
        done
    done

    adr_seventeen="docs/agent/adr/0017-m7-4-linalg.md"
    [[ -f "$adr_seventeen" ]] || fail "ADR-0017 (M7.4 linalg) is required for M7.4"
    if ! grep -q '^status: accepted$' "$adr_seventeen"; then
        fail "ADR-0017 must be 'status: accepted' for M7.4 to be done"
    fi

    # Corpus directory layout per ADR-0017.
    [[ -f corpus/numpy/M7.4/spec.toml ]] || fail "corpus/numpy/M7.4/spec.toml missing"
    [[ -f corpus/numpy/M7.4/canned_llm_responses.toml ]] || fail "corpus/numpy/M7.4/canned_llm_responses.toml missing"
    [[ -d corpus/numpy/M7.4/upstream ]] || fail "corpus/numpy/M7.4/upstream missing"
    [[ -d corpus/numpy/M7.4/harness ]] || fail "corpus/numpy/M7.4/harness missing"
    [[ -f corpus/numpy/M7.4/perf.toml ]] || fail "corpus/numpy/M7.4/perf.toml missing"
fi

echo "doc-coverage: M7.4 linalg surface checks passed"

# --- 16. M7.5 random surface coverage --------------------------------------
# When the numpy module declares M7.5 delivered, the random surface
# terms + ADR-0018 anchors must appear in all three doc trees.

if grep -q '^- \*\*M7.5 — delivered.\*\*' "docs/agent/modules/numpy.md"; then
    m7_5_numpy_terms=(
        "Generator"
        "default_rng"
        "integers"
        "normal"
        "uniform"
        "choice"
        "InvalidIntegerRange"
        "InvalidDistributionParams"
        "InvalidProbabilities"
        "EmptyChoicePopulation"
        "rand_pcg::Pcg64"
        "ADR-0018"
    )
    m7_5_numpy_files=(
        "docs/agent/modules/numpy.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${m7_5_numpy_terms[@]}"; do
        for f in "${m7_5_numpy_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M7.5 numpy surface term '${term}' missing from ${f}"
            fi
        done
    done

    adr_eighteen="docs/agent/adr/0018-m7-5-random.md"
    [[ -f "$adr_eighteen" ]] || fail "ADR-0018 (M7.5 random) is required for M7.5"
    if ! grep -q '^status: accepted$' "$adr_eighteen"; then
        fail "ADR-0018 must be 'status: accepted' for M7.5 to be done"
    fi

    # Corpus directory layout per ADR-0018.
    [[ -f corpus/numpy/M7.5/spec.toml ]] || fail "corpus/numpy/M7.5/spec.toml missing"
    [[ -f corpus/numpy/M7.5/canned_llm_responses.toml ]] || fail "corpus/numpy/M7.5/canned_llm_responses.toml missing"
    [[ -d corpus/numpy/M7.5/upstream ]] || fail "corpus/numpy/M7.5/upstream missing"
    [[ -d corpus/numpy/M7.5/harness ]] || fail "corpus/numpy/M7.5/harness missing"
    [[ -f corpus/numpy/M7.5/perf.toml ]] || fail "corpus/numpy/M7.5/perf.toml missing"
fi

echo "doc-coverage: M7.5 random surface checks passed"
# --- 17. ADR-0022 ecosystem-batch coverage --------------------------------
# When the requests module declares M-batch delivered, the public-surface
# terms + ADR-0022 anchors must appear in all three doc trees.

if grep -q '^- \*\*M-batch — delivered.\*\*' "docs/agent/modules/requests.md"; then
    mb_requests_terms=(
        "Session"
        "Response"
        "HttpError"
        "HttpErrorKind"
        "HttpMethod"
        "reqwest"
        "ADR-0022"
    )
    mb_requests_files=(
        "docs/agent/modules/requests.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${mb_requests_terms[@]}"; do
        for f in "${mb_requests_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M-batch requests surface term '${term}' missing from ${f}"
            fi
        done
    done
fi

echo "doc-coverage: M-batch requests surface checks passed"

# --- 17. M7.6 expansion surface coverage -----------------------------------
# When the numpy module declares M7.6 delivered, the M7.6 expansion surface
# terms + ADR-0021 anchors must appear in all three doc trees.

if grep -q '^- \*\*M7.6 — delivered.\*\*' "docs/agent/modules/numpy.md"; then
    m7_6_numpy_terms=(
        "Complex64"
        "Complex128"
        "ComplexNotOrderable"
        "PercentileOutOfRange"
        "EmptyAxisTuple"
        "fft"
        "ifft"
        "rfft"
        "irfft"
        "polyval"
        "polyfit"
        "cumsum"
        "cumprod"
        "median"
        "percentile"
        "nansum"
        "nanmean"
        "nanmin"
        "nanmax"
        "ADR-0021"
    )
    m7_6_numpy_files=(
        "docs/agent/modules/numpy.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${m7_6_numpy_terms[@]}"; do
        for f in "${m7_6_numpy_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M7.6 numpy surface term '${term}' missing from ${f}"
            fi
        done
    done
fi

echo "doc-coverage: M7.6 numpy surface checks passed"

# --- 18. M8 MIR surface coverage --------------------------------------------
# When the mir module declares M8 delivered, the MIR surface terms +
# ADR-0020 anchors must appear in all three doc trees.

if grep -q '^- \*\*M8 — delivered.\*\*' "docs/agent/modules/mir.md"; then
    m8_mir_terms=(
        "lower"
        "Module"
        "Body"
        "BasicBlock"
        "Terminator"
        "Place"
        "Rvalue"
        "Operand"
        "MirError"
    )
    m8_mir_files=(
        "docs/agent/modules/mir.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${m8_mir_terms[@]}"; do
        for f in "${m8_mir_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M8 mir surface term '${term}' missing from ${f}"
            fi
        done
    done

    adr_22="docs/agent/adr/0022-translation-ecosystem-batch.md"
    [[ -f "$adr_22" ]] || fail "ADR-0022 (translation ecosystem batch) is required for M-batch"
    if ! grep -q '^status: accepted$' "$adr_22"; then
        fail "ADR-0022 must be 'status: accepted' for M-batch to be done"
    fi

    [[ -f corpus/requests/spec.toml ]] || fail "corpus/requests/spec.toml missing"
    [[ -f corpus/requests/canned_llm_responses.toml ]] || fail "corpus/requests/canned_llm_responses.toml missing"
    [[ -d corpus/requests/upstream ]] || fail "corpus/requests/upstream missing"
    [[ -d corpus/requests/upstream_tests ]] || fail "corpus/requests/upstream_tests missing"
    [[ -d corpus/requests/harness ]] || fail "corpus/requests/harness missing"
    [[ -f corpus/requests/perf.toml ]] || fail "corpus/requests/perf.toml missing"

    if [[ -d crates/cobrust-requests ]]; then
        [[ -f crates/cobrust-requests/PROVENANCE.toml ]]             || fail "crates/cobrust-requests/PROVENANCE.toml missing"
    fi
fi

if grep -q '^- \*\*M-batch — delivered.\*\*' "docs/agent/modules/click.md"; then
    mb_click_terms=(
        "Command"
        "OptionSpec"
        "ArgumentSpec"
        "RunResult"
        "ClickError"
        "ParamType"
        "clap"
        "ADR-0022"
    )
    mb_click_files=(
        "docs/agent/modules/click.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${mb_click_terms[@]}"; do
        for f in "${mb_click_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M-batch click surface term '${term}' missing from ${f}"
            fi
        done
    done

    [[ -f corpus/click/spec.toml ]] || fail "corpus/click/spec.toml missing"
    [[ -f corpus/click/canned_llm_responses.toml ]] || fail "corpus/click/canned_llm_responses.toml missing"
    [[ -d corpus/click/upstream ]] || fail "corpus/click/upstream missing"
    [[ -d corpus/click/upstream_tests ]] || fail "corpus/click/upstream_tests missing"
    [[ -d corpus/click/harness ]] || fail "corpus/click/harness missing"
    [[ -f corpus/click/perf.toml ]] || fail "corpus/click/perf.toml missing"

    if [[ -d crates/cobrust-click ]]; then
        [[ -f crates/cobrust-click/PROVENANCE.toml ]]             || fail "crates/cobrust-click/PROVENANCE.toml missing"
    fi

    # Verify the L3 closure files exist with non-skipped pendulum + non-deferred pyspark.
    [[ -d corpus/dateutil/dependents/pendulum ]] || fail "corpus/dateutil/dependents/pendulum missing"
    [[ -f corpus/dateutil/dependents/pendulum/test_pendulum_subset.py ]]         || fail "pendulum subset missing"
    [[ -d corpus/msgpack/dependents/pyspark ]] || fail "corpus/msgpack/dependents/pyspark missing (M-batch closure)"
    [[ -f corpus/msgpack/dependents/pyspark/test_pyspark_subset.py ]]         || fail "pyspark subset missing (M-batch closure)"
fi

echo "doc-coverage: ADR-0022 ecosystem-batch surface checks passed"

# --- M7.6 numpy expansion (ADR-0021) -----------------------------------------
if [[ -f "docs/agent/adr/0021-m7-6-numpy-expansion.md" ]]; then
    adr_twentyone="docs/agent/adr/0021-m7-6-numpy-expansion.md"
    if ! grep -q '^status: accepted$' "$adr_twentyone"; then
        fail "ADR-0021 must be 'status: accepted' for M7.6 to be done"
    fi

    [[ -f corpus/numpy/M7.6/spec.toml ]] || fail "corpus/numpy/M7.6/spec.toml missing"
    [[ -f corpus/numpy/M7.6/canned_llm_responses.toml ]] || fail "corpus/numpy/M7.6/canned_llm_responses.toml missing"
    [[ -d corpus/numpy/M7.6/upstream ]] || fail "corpus/numpy/M7.6/upstream missing"
    [[ -d corpus/numpy/M7.6/harness ]] || fail "corpus/numpy/M7.6/harness missing"
    [[ -f corpus/numpy/M7.6/perf.toml ]] || fail "corpus/numpy/M7.6/perf.toml missing"
fi

echo "doc-coverage: M7.6 expansion surface checks passed"

# --- M8 MIR shape (ADR-0020) -------------------------------------------------
if [[ -f "docs/agent/adr/0020-m8-mir-shape.md" ]]; then
    adr_twenty="docs/agent/adr/0020-m8-mir-shape.md"
    if ! grep -q '^status: accepted$' "$adr_twenty"; then
        fail "ADR-0020 must be 'status: accepted' for M8 to be done"
    fi
fi

echo "doc-coverage: M8 MIR surface checks passed"


# --- 19. M9 codegen surface coverage --------------------------------------
# When the codegen module declares M9 delivered, the codegen surface
# terms + ADR-0023 anchors must appear in all three doc trees.

if grep -q '^- \*\*M9 — delivered.\*\*' "docs/agent/modules/codegen.md"; then
    m9_codegen_terms=(
        "emit"
        "TargetSpec"
        "Artifact"
        "ArtifactKind"
        "Backend"
        "OptLevel"
        "CodegenError"
        "Cranelift"
        "LLVM"
        "ADR-0023"
    )
    m9_codegen_files=(
        "docs/agent/modules/codegen.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${m9_codegen_terms[@]}"; do
        for f in "${m9_codegen_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M9 codegen surface term '${term}' missing from ${f}"
            fi
        done
    done

    adr_23="docs/agent/adr/0023-m9-codegen.md"
    [[ -f "$adr_23" ]] || fail "ADR-0023 (M9 codegen) is required for M9"
    if ! grep -q '^status: accepted$' "$adr_23"; then
        fail "ADR-0023 must be 'status: accepted' for M9 to be done"
    fi
fi

echo "doc-coverage: M9 codegen surface checks passed"

# --- 20. M10 CLI driver surface coverage ----------------------------------
# When the cli module declares M10 delivered, the M10 subcommand surface
# terms + ADR-0024 anchors must appear in all three doc trees.

if grep -q '^- \*\*M10 — delivered.\*\*' "docs/agent/modules/cli.md"; then
    m10_cli_terms=(
        "build"
        "run"
        "check"
        "fmt"
        "translate"
        "new"
        "test"
        "repl"
        "ADR-0024"
        "hello, world"
        "[package]"
    )
    m10_cli_files=(
        "docs/agent/modules/cli.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${m10_cli_terms[@]}"; do
        for f in "${m10_cli_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M10 cli surface term '${term}' missing from ${f}"
            fi
        done
    done

    adr_24="docs/agent/adr/0024-m10-cli-driver.md"
    [[ -f "$adr_24" ]] || fail "ADR-0024 (M10 CLI driver) is required for M10"
    if ! grep -q '^status: accepted$' "$adr_24"; then
        fail "ADR-0024 must be 'status: accepted' for M10 to be done"
    fi

    [[ -f examples/hello.cb ]] || fail "examples/hello.cb missing (M10 binding done-means)"
    # M11 superseded M10's runtime helper: m10_runtime.c lifted to
    # cobrust-stdlib::io::__cobrust_println per ADR-0025 §"Print-intrinsic
    # lift". The new entry shim is cobrust_main.c.
    [[ -f crates/cobrust-cli/runtime/cobrust_main.c ]] \
        || fail "crates/cobrust-cli/runtime/cobrust_main.c missing (M11 entry shim)"
fi

echo "doc-coverage: M10 CLI driver surface checks passed"

# --- 21. M11 stdlib + runtime surface coverage -----------------------------
# When the stdlib module declares M11 delivered, the M11 binding surface
# terms + ADR-0025 anchors must appear in all three doc trees.

if grep -q '^- \*\*M11 — delivered.\*\*' "docs/agent/modules/stdlib.md"; then
    m11_stdlib_terms=(
        "std.io.println"
        "std.collections.List"
        "std.string.format"
        "std.math.sqrt"
        "std.panic.panic"
        "std.env.args"
        "std.fmt"
        "ADR-0025"
        "__cobrust_print"
        "__cobrust_println"
        "__cobrust_panic"
        "__cobrust_capture_argv"
        "_cobrust_user_main"
        "mimalloc"
        # ADR-0044 W2 Phase 2 — source-level stdin/argv binding.
        "input"
        "read_line"
        "argv"
        "__cobrust_input"
        "__cobrust_read_line"
        "__cobrust_argv"
        "ADR-0044"
    )
    m11_stdlib_files=(
        "docs/agent/modules/stdlib.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${m11_stdlib_terms[@]}"; do
        for f in "${m11_stdlib_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M11 stdlib surface term '${term}' missing from ${f}"
            fi
        done
    done

    adr_25="docs/agent/adr/0025-m11-stdlib-runtime.md"
    [[ -f "$adr_25" ]] || fail "ADR-0025 (M11 stdlib + runtime) is required for M11"
    if ! grep -q '^status: accepted$' "$adr_25"; then
        fail "ADR-0025 must be 'status: accepted' for M11 to be done"
    fi

    # M11 binding done-means: 10 example programs + hello.cb regression.
    for example in hello fizzbuzz fib wc cat echo sort unique_lines regex_grep csv_sum json_pretty; do
        [[ -f "examples/${example}.cb" ]] || fail "examples/${example}.cb missing (M11 binding done-means)"
    done

    # M11 stdlib crate must exist with the seven binding modules.
    [[ -d crates/cobrust-stdlib ]] || fail "crates/cobrust-stdlib missing"
    for mod in io collections string math panic env fmt runtime; do
        [[ -f "crates/cobrust-stdlib/src/${mod}.rs" ]]             || fail "crates/cobrust-stdlib/src/${mod}.rs missing"
    done
fi

echo "doc-coverage: M11 stdlib + runtime surface checks passed"

# --- 22. M12 package format surface coverage -------------------------------
# When the pkg module declares M12 delivered, the M12 binding surface
# terms + ADR-0026 anchors must appear in all three doc trees.

if grep -q '^- \*\*M12 — delivered.\*\*' "docs/agent/modules/pkg.md"; then
    m12_pkg_terms=(
        "cobrust.toml"
        "cobrust.lock"
        "[package]"
        "[dependencies]"
        "[bin]"
        "[lib]"
        "[[test]]"
        "blake3"
        "content-addressed"
        "manifest_hash"
        "lockfile_version"
        "provenance_hash"
        "ADR-0026"
    )
    m12_pkg_files=(
        "docs/agent/modules/pkg.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${m12_pkg_terms[@]}"; do
        for f in "${m12_pkg_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M12 pkg surface term '${term}' missing from ${f}"
            fi
        done
    done

    adr_26="docs/agent/adr/0026-m12-package-format.md"
    [[ -f "$adr_26" ]] || fail "ADR-0026 (M12 package format) is required for M12"
    if ! grep -q '^status: accepted$' "$adr_26"; then
        fail "ADR-0026 must be 'status: accepted' for M12 to be done"
    fi

    # M12 binding done-means: pkg crate exists with the seven binding modules.
    [[ -d crates/cobrust-pkg ]] || fail "crates/cobrust-pkg missing"
    for mod in error manifest lockfile resolver registry sources tarball; do
        [[ -f "crates/cobrust-pkg/src/${mod}.rs" ]] \
            || fail "crates/cobrust-pkg/src/${mod}.rs missing"
    done

    # M12 binding done-means: notebook example exists + has ≥ 3 modules.
    [[ -d examples/notebook ]] || fail "examples/notebook missing (ADR-0019 line 3)"
    [[ -f examples/notebook/cobrust.toml ]] || fail "examples/notebook/cobrust.toml missing"
    [[ -f examples/notebook/src/main.cb ]] || fail "examples/notebook/src/main.cb missing"
    notebook_module_count=$(find examples/notebook/src -name '*.cb' | wc -l | tr -d ' ')
    if [[ "$notebook_module_count" -lt 3 ]]; then
        fail "examples/notebook/src has only ${notebook_module_count} .cb modules; ADR-0019 requires ≥ 3"
    fi
fi

echo "doc-coverage: M12 package format surface checks passed"

# --- 23. M12.x codegen + stdlib amendments surface coverage ----------------
# When ADR-0027 is accepted, the M12.x amendments surface terms must
# appear in the relevant doc trees.

adr_27="docs/agent/adr/0027-m12-x-codegen-stdlib-amendments.md"
if [[ -f "$adr_27" ]] && grep -q '^status: accepted$' "$adr_27"; then
    m12x_terms=(
        "Iterator"
        "__cobrust_alloc"
        "__cobrust_fmt_int"
        "__cobrust_iter_init"
        "__cobrust_str_new"
        "ADR-0027"
        "for-protocol"
    )
    m12x_files=(
        "docs/agent/modules/stdlib.md"
        "docs/agent/modules/codegen.md"
        "docs/agent/modules/mir.md"
    )
    for term in "${m12x_terms[@]}"; do
        for f in "${m12x_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M12.x surface term '${term}' missing from ${f}"
            fi
        done
    done

    # M12.x binding done-means: iter module exists + 8 examples have
    # the M12.x docstring banner + 11 #[ignore] markers gone.
    [[ -f crates/cobrust-stdlib/src/iter.rs ]] || fail "crates/cobrust-stdlib/src/iter.rs missing (M12.x ADR-0027 §4)"

    # All 8 example bodies must compile + run; the doc-coverage script
    # only checks the source presence + the no-ignore property.
    for ex in wc cat echo sort unique_lines regex_grep csv_sum json_pretty; do
        [[ -f "examples/${ex}.cb" ]] || fail "examples/${ex}.cb missing"
        if grep -q "M11 stub" "examples/${ex}.cb"; then
            fail "examples/${ex}.cb still contains 'M11 stub' (M12.x rewrite incomplete)"
        fi
    done

    if grep -q '#\[ignore = "requires staticlib + cli binary; gated under --ignored"\]' \
        crates/cobrust-stdlib/tests/stdlib_examples.rs; then
        fail "stdlib_examples still gates 11 tests behind #[ignore]; M12.x must lift them"
    fi

    echo "doc-coverage: M12.x codegen + stdlib amendments surface checks passed"
fi

# --- 24. M13 stdlib::task + ::sync surface coverage (ADR-0028) -------------
# When the stdlib module declares M13 delivered, the M13 binding surface
# terms + ADR-0028 anchors must appear in all three doc trees.

if grep -q '^- \*\*M13 — delivered.\*\*' "docs/agent/modules/stdlib.md"; then
    m13_stdlib_terms=(
        "std.task.spawn"
        "std.task.scope"
        "std.task.cancel"
        "JoinHandle"
        "JoinError"
        "std.sync.channel"
        "Sender"
        "Receiver"
        "SendError"
        "TrySendError"
        "TryRecvError"
        "tokio-runtime"
        "ADR-0028"
    )
    m13_stdlib_files=(
        "docs/agent/modules/stdlib.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${m13_stdlib_terms[@]}"; do
        for f in "${m13_stdlib_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M13 stdlib surface term '${term}' missing from ${f}"
            fi
        done
    done
fi

# --- M13 stdlib::task + ::sync surface coverage (ADR-0028) ----------------
if [[ -f "docs/agent/adr/0028-m13-concurrency-runtime.md" ]]; then
    adr_28="docs/agent/adr/0028-m13-concurrency-runtime.md"
    if ! grep -q '^status: accepted$' "$adr_28"; then
        fail "ADR-0028 must be 'status: accepted' for M13 to be done"
    fi

    # M13 binding done-means: task + sync sources exist.
    [[ -f "crates/cobrust-stdlib/src/task.rs" ]] \
        || fail "crates/cobrust-stdlib/src/task.rs missing (M13 ADR-0028 §C)"
    [[ -f "crates/cobrust-stdlib/src/sync.rs" ]] \
        || fail "crates/cobrust-stdlib/src/sync.rs missing (M13 ADR-0028 §C)"

    # M13 binding done-means: 4 test files exist.
    for tf in task_well_typed task_ill_typed task_corpus task_perf; do
        [[ -f "crates/cobrust-stdlib/tests/${tf}.rs" ]] \
            || fail "crates/cobrust-stdlib/tests/${tf}.rs missing (M13 ADR-0028 §F)"
    done

    # M13 finding doc must exist.
    [[ -f "docs/agent/findings/m13-sync-bridge-cost.md" ]] \
        || fail "docs/agent/findings/m13-sync-bridge-cost.md missing (ADR-0028 §F)"
fi

# --- M-AI.0 stdlib::llm surface coverage (ADR-0048) ------------------------
if grep -q '^- \*\*M-AI.0 — delivered\.\*\*' "docs/agent/modules/stdlib.md"; then
    mai0_llm_terms=(
        "llm_complete"
        "llm_dispatch"
        "llm_stream"
        "Decision 7"
        "ledger"
        "[routing.llm_complete_"
        "[routing.llm_stream_"
    )
    mai0_llm_files=(
        "docs/agent/modules/stdlib.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${mai0_llm_terms[@]}"; do
        for f in "${mai0_llm_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M-AI.0 llm surface term '${term}' missing from ${f}"
            fi
        done
    done

    [[ -f "crates/cobrust-stdlib/src/llm.rs" ]] \
        || fail "crates/cobrust-stdlib/src/llm.rs missing (M-AI.0 ADR-0048)"
    [[ -f "crates/cobrust-stdlib/tests/llm_corpus.rs" ]] \
        || fail "crates/cobrust-stdlib/tests/llm_corpus.rs missing (M-AI.0 tests)"
    [[ -f "crates/cobrust-cli/tests/intrinsics_llm.rs" ]] \
        || fail "crates/cobrust-cli/tests/intrinsics_llm.rs missing (M-AI.0 E2E tests)"

    if grep -q -F 'llm_dispatch(' "docs/human/en/architecture.md"; then
        grep -q -F '[routing.summarize_doc]' "cobrust.toml.example" \
            || fail "M-AI.0 dispatch examples are exposed but cobrust.toml.example lacks a sample [routing.<task>] entry"
    fi
fi

echo "doc-coverage: M-AI.0 llm surface checks passed"

# --- M-AI.1 stdlib::prompt surface coverage (ADR-0048) ---------------------
if grep -q '^- \*\*M-AI.1 — delivered\.\*\*' "docs/agent/modules/stdlib.md"; then
    mai1_prompt_terms=(
        "prompt_render"
        "prompt_format_few_shot"
        "prompt_format_system_user"
        "prompt_escape_braces"
        "llm_complete_structured"
        "task=\"structured\""
        "Input: <in_i>"
        "Respond with valid JSON"
    )
    mai1_prompt_files=(
        "docs/agent/modules/stdlib.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${mai1_prompt_terms[@]}"; do
        for f in "${mai1_prompt_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M-AI.1 prompt surface term '${term}' missing from ${f}"
            fi
        done
    done

    [[ -f "crates/cobrust-stdlib/src/prompt.rs" ]] \
        || fail "crates/cobrust-stdlib/src/prompt.rs missing (M-AI.1 ADR-0048)"
    [[ -f "crates/cobrust-stdlib/tests/prompt_corpus.rs" ]] \
        || fail "crates/cobrust-stdlib/tests/prompt_corpus.rs missing (M-AI.1 tests)"
    [[ -f "crates/cobrust-cli/tests/intrinsics_prompt.rs" ]] \
        || fail "crates/cobrust-cli/tests/intrinsics_prompt.rs missing (M-AI.1 E2E tests)"

    if grep -q -F 'llm_complete_structured' "crates/cobrust-stdlib/src/prompt.rs"; then
        grep -q -F '[routing.structured]' "cobrust.toml.example" \
            || fail "M-AI.1 structured routing is exposed but cobrust.toml.example lacks [routing.structured]"
    fi
fi

echo "doc-coverage: M-AI.1 prompt surface checks passed"

# --- M-AI.2 stdlib::tool surface coverage (ADR-0048) -----------------------
if grep -q '^- \*\*M-AI.2 — delivered\.\*\*' "docs/agent/modules/stdlib.md"; then
    mai2_tool_terms=(
        "tool_schema"
        "tool_registry_new"
        "tool_registry_register"
        "tool_invoke"
        "llm_complete_with_tools"
        "closed-world"
        "add_i64"
        "@cobrust.tool.expose"
        "Registry"
        "native provider tool-call"
    )
    mai2_tool_files=(
        "docs/agent/modules/stdlib.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${mai2_tool_terms[@]}"; do
        for f in "${mai2_tool_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M-AI.2 tool surface term '${term}' missing from ${f}"
            fi
        done
    done

    [[ -f "crates/cobrust-stdlib/src/tool.rs" ]] \
        || fail "crates/cobrust-stdlib/src/tool.rs missing (M-AI.2 ADR-0048)"
    [[ -f "crates/cobrust-stdlib/tests/tool_corpus.rs" ]] \
        || fail "crates/cobrust-stdlib/tests/tool_corpus.rs missing (M-AI.2 tests)"
    [[ -f "crates/cobrust-cli/tests/intrinsics_tool.rs" ]] \
        || fail "crates/cobrust-cli/tests/intrinsics_tool.rs missing (M-AI.2 E2E tests)"

    if grep -q -F 'llm_dispatch_blocking("tools"' "crates/cobrust-stdlib/src/tool.rs"; then
        grep -q -F '[routing.tools]' "cobrust.toml.example" \
            || fail "M-AI.2 tool routing is exposed via llm_dispatch(task=\"tools\") but cobrust.toml.example lacks [routing.tools]"
    fi
fi

echo "doc-coverage: M-AI.2 tool surface checks passed"
echo "doc-coverage: M13 stdlib task + sync surface checks passed"

# --- M-F.3.1 for-loop + range PRELUDE surface coverage (ADR-0050b) ---------
mf31_terms=(
    "range(start, stop)"
    "for-loop length-bound"
    "ADR-0050b"
    "length-bound index"
)
mf31_human_files=(
    "docs/human/en/getting-started.md"
    "docs/human/zh/getting-started.md"
)
# The human-tree section just needs the user-visible phrase
# "range(start, stop)" plus the §2.5 anchor.
for f in "${mf31_human_files[@]}"; do
    grep -q -F 'range(start, stop)' "$f" \
        || fail "M-F.3.1 user-facing term 'range(start, stop)' missing from ${f}"
    grep -q -F 'for i in range' "$f" \
        || fail "M-F.3.1 for-loop example 'for i in range' missing from ${f}"
done
# Agent-tree mir.md must document the length-bound index lowering.
grep -q -F 'length-bound index' "docs/agent/modules/mir.md" \
    || fail "M-F.3.1 'length-bound index' missing from docs/agent/modules/mir.md"
grep -q -F 'ADR-0050b' "docs/agent/modules/mir.md" \
    || fail "M-F.3.1 'ADR-0050b' cross-ref missing from docs/agent/modules/mir.md"
# Agent-tree cli.md must document the PRELUDE range body.
grep -q -F 'fn range(start: i64, stop: i64) -> list[i64]' "docs/agent/modules/cli.md" \
    || fail "M-F.3.1 prelude range signature missing from docs/agent/modules/cli.md"
# ADR-0050b must exist and reference the for-loop shape.
[[ -f "docs/agent/adr/0050b-for-loop-shape.md" ]] \
    || fail "ADR-0050b file missing (M-F.3.1 deliverable)"
grep -q -F 'M-F.3.1' "docs/agent/adr/0050b-for-loop-shape.md" \
    || fail "ADR-0050b must reference M-F.3.1 in its body"
# Test corpus + examples must exist.
[[ -f "crates/cobrust-cli/tests/for_range_e2e.rs" ]] \
    || fail "crates/cobrust-cli/tests/for_range_e2e.rs missing (M-F.3.1 corpus)"
[[ -f "examples/for_range.cb" ]] \
    || fail "examples/for_range.cb missing (M-F.3.1 deliverable)"
[[ -f "examples/for_list.cb" ]] \
    || fail "examples/for_list.cb missing (M-F.3.1 deliverable)"

echo "doc-coverage: M-F.3.1 for-loop + range surface checks passed"

# --- 23. M14 REPL surface coverage --------------------------------------
# When the cli module declares M14 delivered, the M14 binding surface
# terms + ADR-0029 anchors must appear in all three doc trees.

if grep -q '^- \*\*M14 — delivered.\*\*' "docs/agent/modules/cli.md"; then
    m14_cli_terms=(
        ":type"
        ":ast"
        ":hir"
        ":mir"
        ":clear"
        ":help"
        ":quit"
        "rustyline"
        "cobrust repl"
        "ADR-0029"
    )
    m14_cli_files=(
        "docs/agent/modules/cli.md"
        "docs/human/en/architecture.md"
        "docs/human/zh/architecture.md"
    )
    for term in "${m14_cli_terms[@]}"; do
        for f in "${m14_cli_files[@]}"; do
            if ! grep -q -F "${term}" "$f"; then
                fail "M14 cli surface term '${term}' missing from ${f}"
            fi
        done
    done

    adr_29="docs/agent/adr/0029-m14-repl.md"
    [[ -f "$adr_29" ]] || fail "ADR-0029 (M14 REPL) is required for M14"
    if ! grep -q '^status: accepted$' "$adr_29"; then
        fail "ADR-0029 must be 'status: accepted' for M14 to be done"
    fi

    # M14 binding done-means: 50-session corpus + at least one bin-tests file.
    [[ -f examples/repl-session.txt ]] || fail "examples/repl-session.txt missing (M14 binding done-means)"
    session_count=$(grep -c "^=== " examples/repl-session.txt || true)
    if [[ "$session_count" -lt 50 ]]; then
        fail "examples/repl-session.txt has only ${session_count} sessions; ADR-0029 requires ≥ 50"
    fi
    [[ -f crates/cobrust-cli/tests/repl_smoke.rs ]] || fail "crates/cobrust-cli/tests/repl_smoke.rs missing"
    [[ -f crates/cobrust-cli/tests/repl_session_corpus.rs ]] || fail "crates/cobrust-cli/tests/repl_session_corpus.rs missing"
fi

echo "doc-coverage: M14 REPL surface checks passed"

# --- M-F.3.0 break/continue contract seal (ADR-0050a) -----------------------
# When ADR-0050a is present, verify that:
# - Every doc tree (zh + en + agent) mentions `break` AND `continue`.
# - The four binding corpus files exist.
# - The canonical example exists.

adr_50a="docs/agent/adr/0050a-loop-control-flow.md"
if [[ -f "$adr_50a" ]]; then
    mf3_0_doc_files=(
        "docs/human/zh/getting-started.md"
        "docs/human/en/getting-started.md"
        "docs/agent/modules/frontend.md"
        "docs/agent/modules/hir.md"
        "docs/agent/modules/types.md"
        "docs/agent/modules/mir.md"
    )
    for f in "${mf3_0_doc_files[@]}"; do
        if ! grep -q "break" "$f"; then
            fail "M-F.3.0 (ADR-0050a) requires '${f}' to mention 'break'"
        fi
        if ! grep -q "continue" "$f"; then
            fail "M-F.3.0 (ADR-0050a) requires '${f}' to mention 'continue'"
        fi
    done

    mf3_0_test_files=(
        "crates/cobrust-frontend/tests/break_continue_parse_corpus.rs"
        "crates/cobrust-types/tests/break_continue_types_corpus.rs"
        "crates/cobrust-mir/tests/break_continue_mir_corpus.rs"
        "crates/cobrust-cli/tests/cli_break_continue_e2e.rs"
    )
    for f in "${mf3_0_test_files[@]}"; do
        [[ -f "$f" ]] || fail "M-F.3.0 (ADR-0050a) requires corpus file ${f}"
    done

    [[ -f "examples/early_exit.cb" ]] \
        || fail "M-F.3.0 (ADR-0050a) requires examples/early_exit.cb"
fi

echo "doc-coverage: M-F.3.0 break/continue contract checks passed"
