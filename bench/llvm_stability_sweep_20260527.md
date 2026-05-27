# Phase X.2 LLVM stability sweep — program-level parity audit

ADR-0070 §X.3 input: program-level parity audit before flipping LLVM-default.

Generated: 2026-05-27
Workspace HEAD at sweep time: `ed6be8c`

## Methodology

- Both `cobrust` binaries built once at the same HEAD (`ed6be8c`):
  - Cranelift binary: `target-cranelift/release/cobrust` (default; `cfg(feature="llvm")` false)
  - LLVM binary: `target-llvm/release/cobrust` (built with `--features cobrust-codegen/llvm`)
- Per program: build with `--release` on each backend, then run, then categorize:
  - `LLVM_BUILD_FAIL`: LLVM rejected what Cranelift accepted
  - `LLVM_RUN_CRASH`: built but crashed at runtime under LLVM (rc != 0 while Cranelift rc == 0)
  - `STDOUT_DIVERGENCE`: both built and ran (rc == 0) but stdout differs byte-for-byte
  - `CRANELIFT_BUILD_FAIL`: Cranelift rejected what LLVM accepted (symmetric case)
  - `BOTH_BUILD_FAIL`: both rejected (parse / type / HIR before codegen)
  - `BOTH_RUN_FAIL`: both ran, both rc != 0 (stdin / argv shape mismatch, not a backend issue)
  - `PARITY_GREEN`: both built, both ran rc == 0, stdout byte-identical
- LC-100 stress programs (`examples/leetcode-stress/*/solution.cb`): stdin fed from
  first case of sibling `test.toml` (Python regex extract).
- Stdin-less programs: `</dev/null`.
- F35-sibling discipline: every non-PARITY_GREEN row is reported below. No silent failures.
- F49 §4.1 discipline: no device-specific name embedded.

## Corpus

| Subdir | Count |
|---|---|
| `examples/leetcode-stress/*/solution.cb` | 100 |
| `examples/*.cb` (top-level) | 15 |
| `examples/leetcode/*.cb` (small) | 10 |
| `examples/lc100_pattern_a_fixtures/*.cb` | 6 |
| `examples/leetcode_fixtures/*.cb` | 7 |
| `tests/syntax-corpus/*.cb` | 5 |
| `examples/notebook/` (multi-file package) | 1 |
| **Total** | **144** |

Notes on exclusions:

- `examples/notebook/src/*.cb` and `examples/notebook/tests/*.cb` are nested
  package files; only the package root is built (1 entry).
- `examples/notebook-config/src/lib.cb` is a config-package, skipped
  (no entry-point main).

## Aggregate

| Category | Count | % of corpus |
|---|---|---|
| PARITY_GREEN | 137 | 95.1% |
| STDOUT_DIVERGENCE (sweep artifact — see §"Investigated") | 2 | 1.4% |
| BOTH_BUILD_FAIL (pre-codegen reject) | 5 | 3.5% |
| LLVM_BUILD_FAIL | 0 | 0.0% |
| LLVM_RUN_CRASH | 0 | 0.0% |
| CRANELIFT_BUILD_FAIL | 0 | 0.0% |
| BOTH_RUN_FAIL | 0 | 0.0% |

**Real backend-asymmetry findings: zero.**

## LLVM_BUILD_FAIL

None.

## LLVM_RUN_CRASH

None.

## STDOUT_DIVERGENCE — investigated, not real

Two programs flagged. Both root-cause to the sweep harness, not the LLVM
backend.

| Program | Cranelift line-5 | LLVM line-5 |
|---|---|---|
| `examples/for_list.cb` | `/tmp/sweep_work/examples_for_list.cb_C` | `/tmp/sweep_work/examples_for_list.cb_L` |
| `examples/leetcode_fixtures/argv_dump.cb` | `/tmp/sweep_work/examples_leetcode_fixtures_argv_dump.cb_C` | `/tmp/sweep_work/examples_leetcode_fixtures_argv_dump.cb_L` |

### Why this is not a backend gap

Both programs call `argv()` and `print(a)`, which echoes `argv[0]` — the
**output binary path**. The sweep harness deliberately writes the two
backends to differently-named binaries (`_C` / `_L` suffix) so they can
co-exist on disk. Therefore the bytes that differ are exactly the
output-binary path string, not language semantics.

### Verification (control test)

```text
# Copy both binaries to the same path, then re-run:
cp .../examples_for_list.cb_C  /tmp/sweep_work/same_name
/tmp/sweep_work/same_name      → 10 / 20 / 30 / 40 / /tmp/sweep_work/same_name
cp .../examples_for_list.cb_L  /tmp/sweep_work/same_name
/tmp/sweep_work/same_name      → 10 / 20 / 30 / 40 / /tmp/sweep_work/same_name
diff → IDENTICAL
```

Both backends emit bit-identical output when the binary path is
identical. The LLVM backend is semantically correct for `argv()`.

## BOTH_BUILD_FAIL — pre-codegen reject

Five files in `tests/syntax-corpus/` are highlight / lint fixtures, **not
buildable programs**. They exercise features rejected by the type
checker, parser, or HIR before codegen runs. Both backends fail at
identical earlier stages, so no LLVM-specific signal.

| Program | Reject stage | First diagnostic |
|---|---|---|
| `tests/syntax-corpus/01_keywords.cb` | HIR | `UnknownName { name: "open" }` (stdlib not declared) |
| `tests/syntax-corpus/02_strings_and_fstrings.cb` | TypeCheck | `TypeMismatch { expected: Dict, actual: List(Int) }` |
| `tests/syntax-corpus/03_types_and_generics.cb` | Parse | `DroppedByConstitution { name: "walrus :=" }` (§2.2) |
| `tests/syntax-corpus/04_numbers_and_operators.cb` | TypeCheck | `TypeMismatch { expected: Float, actual: Imag }` |
| `tests/syntax-corpus/05_advanced_patterns.cb` | Parse | `Expected: Ident, found: KwLambda` |

These are **expected rejects** — the suite header comment on each file
identifies them as "syntax corpus" / "visual test" fixtures. They
correctly exercise §2.2 ("Drop from Python") rejections.

## Per-category breakdown

| Subdir | PARITY_GREEN | DIVERGENCE | BUILD_FAIL |
|---|---|---|---|
| `examples/leetcode-stress` | 100 | 0 | 0 |
| `examples/` (top-level) | 14 | 1 (argv artifact) | 0 |
| `examples/leetcode` | 10 | 0 | 0 |
| `examples/lc100_pattern_a_fixtures` | 6 | 0 | 0 |
| `examples/leetcode_fixtures` | 6 | 1 (argv artifact) | 0 |
| `examples/notebook` (package) | 1 | 0 | 0 |
| `tests/syntax-corpus` | 0 | 0 | 5 (both, pre-codegen) |

## Cross-reference: §4.2 L2 Behavior gate

Closed-loop verification on the corpus: LLVM backend matches Cranelift
backend on **100% of buildable programs**. No `@py_compat` divergences,
no numerical drift (LC-100 outputs include integer arithmetic, integer
overflow corner-cases, bit-tricks 81-90; all byte-identical).

## Recommendation for ADR-0070 §X.3

**GREEN** — recommend flipping LLVM-default for `--release` builds.

Rationale:

- Zero LLVM-only build failures across 144-program sweep.
- Zero LLVM-only runtime crashes.
- Zero real stdout divergences (the two flagged are harness-induced
  argv[0] differences; control-test bit-identical when path matches).
- All 100 LC-100 stress solutions match byte-for-byte under stdin from
  authoritative `test.toml` cases.
- Phase X.1 already established LLVM compile-time penalty is +15%
  (acceptable for one-time release builds) and runtime is -8% (faster).

§X.3 may proceed without further per-program triage. Future regressions
should be caught by a CI sweep similar to this one (next milestone).

## Reproducing this sweep

```bash
# From workspace root, at HEAD ed6be8c (or later):
CARGO_TARGET_DIR="$PWD/target-cranelift" cargo build --release -p cobrust-cli
CARGO_TARGET_DIR="$PWD/target-llvm"      cargo build --release -p cobrust-cli \
    --features cobrust-codegen/llvm

# Sweep script (bash 3-compatible; see commit message for /tmp/sweep.sh):
bash sweep.sh target-cranelift/release/cobrust target-llvm/release/cobrust results.txt
cut -d'|' -f1 results.txt | sort | uniq -c
```

## F35-sibling commit-msg-vs-diff discipline

This report content is exactly what the sweep recorded. The two
STDOUT_DIVERGENCE rows above were investigated to root cause before
classifying. Nothing reclassified to PARITY_GREEN; the divergence
records remain visible so future audits can verify the explanation.
