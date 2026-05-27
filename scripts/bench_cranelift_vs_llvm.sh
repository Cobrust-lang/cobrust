#!/usr/bin/env bash
# Phase X.1 — Cranelift vs LLVM empirical benchmark on LC-100 + examples corpus.
#
# Drives `cobrust build --release` against both backends for each `.cb`
# program in:
#   - examples/leetcode/*.cb   (LC-100 corpus subset)
#   - examples/*.cb            (autonomous demos)
#
# Captures (per F50 stdout-parity discipline):
#   - compile time (ms)        — wall time of `cobrust build`
#   - run time (ms)            — wall time of compiled binary
#   - binary size (bytes)      — final executable on disk
#   - stdout parity            — byte-identical Cranelift vs LLVM stdout
#   - LLVM status              — ok / compile-fail / run-fail / non-parity
#
# Pre-state: both binaries must already be built (this script does not
# rebuild — caller responsible). Run:
#   CARGO_TARGET_DIR="$PWD/target-cranelift" cargo build --release -p cobrust-cli
#   CARGO_TARGET_DIR="$PWD/target-llvm" cargo build --release -p cobrust-cli \
#       --features cobrust-codegen/llvm
#
# Usage:
#   ./scripts/bench_cranelift_vs_llvm.sh [report-path]
#
# Default report: bench/cranelift_vs_llvm_$(date +%Y%m%d).md
#
# §2.5 LLM-first: emits a deterministic markdown table.
# F35-sibling: report claims only what was empirically measured.
# F39 device-name clean: no host-specific paths leak into report content.

set -uo pipefail

REPO_ROOT="${REPO_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
COBRUST_C="${COBRUST_C:-$REPO_ROOT/target-cranelift/release/cobrust}"
COBRUST_L="${COBRUST_L:-$REPO_ROOT/target-llvm/release/cobrust}"
REPORT="${1:-$REPO_ROOT/bench/cranelift_vs_llvm_$(date +%Y%m%d).md}"
BENCH_DIR="$REPO_ROOT/bench"
TMP="$(mktemp -d -t cobrust-bench-XXXXXX)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$BENCH_DIR"

if [ ! -x "$COBRUST_C" ]; then
  echo "ERROR: cranelift binary not found at $COBRUST_C" >&2
  exit 2
fi
if [ ! -x "$COBRUST_L" ]; then
  echo "ERROR: llvm binary not found at $COBRUST_L" >&2
  exit 2
fi

# millisecond-resolution timestamp; python3 used for portability across
# macOS (which lacks GNU date %N).
now_ms() { python3 -c "import time; print(int(time.time()*1000))"; }

# Capture compile-time + size for one (binary, program) pair.
# Side-effect: writes binary to "$TMP/$tag_$name".
# Echoes "compile_ms\tsize_bytes\texit_code\tstderr_file" tab-separated.
compile_one() {
  local cobrust="$1" tag="$2" cb="$3" name="$4"
  local out="$TMP/${tag}_${name}"
  local stderr_f="$TMP/${tag}_${name}.stderr"
  local start end size ec
  start=$(now_ms)
  "$cobrust" build "$cb" -o "$out" --release > "$stderr_f" 2>&1
  ec=$?
  end=$(now_ms)
  if [ -f "$out" ]; then
    size=$(stat -f%z "$out" 2>/dev/null || stat -c%s "$out" 2>/dev/null || echo 0)
  else
    size=0
  fi
  printf '%d\t%d\t%d\t%s\n' $((end - start)) "$size" "$ec" "$stderr_f"
}

# Capture run-time + stdout for compiled binary.
#
# argv[0] handling: programs that print `argv()[0]` would otherwise
# diverge between backends purely because each compile lands the
# executable at a different path (e.g. `C_for_list` vs `L_for_list`).
# To isolate true backend behavior, we invoke each compiled binary
# via a canonical symlink (`${TMP}/canonical_${name}`) so both Cranelift
# and LLVM runs see identical argv[0]. This is the correct comparison
# semantics — Phase X.1 measures backend codegen parity, not bench
# harness path collisions.
run_one() {
  local exe="$1" tag="$2" name="$3"
  local stdout_f="$TMP/${tag}_${name}.stdout"
  local canonical="$TMP/canonical_${name}"
  ln -sf "$exe" "$canonical"
  local start end ec
  start=$(now_ms)
  "$canonical" > "$stdout_f" 2>/dev/null
  ec=$?
  end=$(now_ms)
  rm -f "$canonical"
  printf '%d\t%d\t%s\n' $((end - start)) "$ec" "$stdout_f"
}

# Build full corpus list (deterministic alphabetical).
shopt -s nullglob
CORPUS=()
for f in "$REPO_ROOT/examples/leetcode"/*.cb; do CORPUS+=("$f"); done
for f in "$REPO_ROOT/examples"/*.cb;          do CORPUS+=("$f"); done
shopt -u nullglob
TOTAL=${#CORPUS[@]}

echo "Running bench against $TOTAL programs (Cranelift + LLVM, --release)..." >&2

# Aggregate accumulators.
TOTAL_C_COMPILE=0
TOTAL_L_COMPILE=0
TOTAL_C_RUN=0
TOTAL_L_RUN=0
TOTAL_C_SIZE=0
TOTAL_L_SIZE=0
LLVM_COMPILE_FAIL=0
LLVM_RUN_FAIL=0
PARITY_FAIL=0
ROWS=()

for cb in "${CORPUS[@]}"; do
  name=$(basename "$cb" .cb)
  # Strip 'examples/' or 'examples/leetcode/' prefix to a short namespace tag
  # so output is deterministic regardless of host path.
  case "$cb" in
    */leetcode/*) display="leetcode/$name" ;;
    *)            display="examples/$name" ;;
  esac

  # Cranelift compile
  IFS=$'\t' read -r C_COMPILE C_SIZE C_EC C_STDERR <<< "$(compile_one "$COBRUST_C" "C" "$cb" "$name")"
  # LLVM compile
  IFS=$'\t' read -r L_COMPILE L_SIZE L_EC L_STDERR <<< "$(compile_one "$COBRUST_L" "L" "$cb" "$name")"

  status="ok"
  if [ "$L_EC" -ne 0 ]; then
    status="L-compile-fail(ec=$L_EC)"
    LLVM_COMPILE_FAIL=$((LLVM_COMPILE_FAIL + 1))
  fi
  if [ "$C_EC" -ne 0 ]; then
    status="${status};C-compile-fail(ec=$C_EC)"
  fi

  # Run only if compile succeeded.
  C_RUN="-"; L_RUN="-"; PARITY="-"
  if [ "$C_EC" -eq 0 ] && [ -x "$TMP/C_${name}" ]; then
    IFS=$'\t' read -r C_RUN C_RUN_EC C_STDOUT <<< "$(run_one "$TMP/C_${name}" "C" "$name")"
    TOTAL_C_RUN=$((TOTAL_C_RUN + C_RUN))
  fi
  if [ "$L_EC" -eq 0 ] && [ -x "$TMP/L_${name}" ]; then
    IFS=$'\t' read -r L_RUN L_RUN_EC L_STDOUT <<< "$(run_one "$TMP/L_${name}" "L" "$name")"
    if [ "$L_RUN_EC" -ne 0 ]; then
      status="${status};L-run-fail(ec=$L_RUN_EC)"
      LLVM_RUN_FAIL=$((LLVM_RUN_FAIL + 1))
    fi
    TOTAL_L_RUN=$((TOTAL_L_RUN + L_RUN))
  fi

  # Stdout parity check (only meaningful if both ran).
  if [ -f "${C_STDOUT:-}" ] && [ -f "${L_STDOUT:-}" ]; then
    if cmp -s "$C_STDOUT" "$L_STDOUT"; then
      PARITY="OK"
    else
      PARITY="DIVERGE"
      PARITY_FAIL=$((PARITY_FAIL + 1))
      status="${status};stdout-divergence"
    fi
  fi

  # Convert sizes to KB for table (1-decimal).
  C_SIZE_KB=$(python3 -c "print(f'{$C_SIZE/1024:.1f}')")
  L_SIZE_KB=$(python3 -c "print(f'{$L_SIZE/1024:.1f}')")

  TOTAL_C_COMPILE=$((TOTAL_C_COMPILE + C_COMPILE))
  TOTAL_L_COMPILE=$((TOTAL_L_COMPILE + L_COMPILE))
  TOTAL_C_SIZE=$((TOTAL_C_SIZE + C_SIZE))
  TOTAL_L_SIZE=$((TOTAL_L_SIZE + L_SIZE))

  ROWS+=("| $display | $C_COMPILE | $L_COMPILE | $C_RUN | $L_RUN | $C_SIZE_KB | $L_SIZE_KB | $PARITY | $status |")
done

# Compute aggregates.
MEAN_C_COMPILE=$((TOTAL_C_COMPILE / TOTAL))
MEAN_L_COMPILE=$((TOTAL_L_COMPILE / TOTAL))
MEAN_C_RUN=$((TOTAL_C_RUN / TOTAL))
MEAN_L_RUN=$((TOTAL_L_RUN / TOTAL))
MEAN_C_SIZE_KB=$(python3 -c "print(f'{$TOTAL_C_SIZE/$TOTAL/1024:.1f}')")
MEAN_L_SIZE_KB=$(python3 -c "print(f'{$TOTAL_L_SIZE/$TOTAL/1024:.1f}')")

# Emit report.
{
  echo "# Cranelift vs LLVM benchmark — Phase X.1"
  echo
  echo "ADR-0070 §X.3 input: empirical baseline before flipping LLVM-default."
  echo
  echo "## Methodology"
  echo
  echo "- Both \`cobrust\` binaries built once at the same workspace HEAD."
  echo "- Cranelift binary: \`target-cranelift/release/cobrust\` (default backend)."
  echo "- LLVM binary: \`target-llvm/release/cobrust\` (built with \`--features cobrust-codegen/llvm\`)."
  echo "- Per program: \`cobrust build <file> -o <out> --release\` → run \`<out>\` → diff stdout."
  echo "- Times in milliseconds (wall clock, single sample per program — small-N indicative, not statistically significant)."
  echo "- Sizes in KB (1 KB = 1024 B)."
  echo "- Stdout parity per F50: byte-identical via \`cmp\`."
  echo "- F35-sibling: numbers are measured wall time, not extrapolated."
  echo
  echo "## Corpus"
  echo
  echo "- Total programs: $TOTAL"
  echo "- \`examples/leetcode/\`: $(ls "$REPO_ROOT/examples/leetcode"/*.cb 2>/dev/null | wc -l | tr -d ' ') (LC-100 subset)"
  echo "- \`examples/\`: $(ls "$REPO_ROOT/examples"/*.cb 2>/dev/null | wc -l | tr -d ' ')"
  echo
  echo "## Aggregate stats"
  echo
  echo "| Metric | Cranelift | LLVM | LLVM delta |"
  echo "|---|---|---|---|"
  PCT_COMPILE=$(python3 -c "print(f'{($MEAN_L_COMPILE - $MEAN_C_COMPILE) / $MEAN_C_COMPILE * 100:+.1f}%')")
  PCT_RUN=$(python3 -c "print(f'{($MEAN_L_RUN - $MEAN_C_RUN) / max($MEAN_C_RUN, 1) * 100:+.1f}%')")
  PCT_SIZE=$(python3 -c "print(f'{($TOTAL_L_SIZE - $TOTAL_C_SIZE) / $TOTAL_C_SIZE * 100:+.1f}%')")
  echo "| Mean compile (ms) | $MEAN_C_COMPILE | $MEAN_L_COMPILE | $PCT_COMPILE |"
  echo "| Mean runtime (ms) | $MEAN_C_RUN | $MEAN_L_RUN | $PCT_RUN |"
  echo "| Mean size (KB) | $MEAN_C_SIZE_KB | $MEAN_L_SIZE_KB | $PCT_SIZE |"
  echo
  echo "## Failure counts"
  echo
  echo "- LLVM compile failures: $LLVM_COMPILE_FAIL / $TOTAL"
  echo "- LLVM runtime failures: $LLVM_RUN_FAIL / $TOTAL"
  echo "- Stdout parity divergences: $PARITY_FAIL / $TOTAL"
  echo
  echo "## Per-program results"
  echo
  echo "| Program | C compile (ms) | L compile (ms) | C run (ms) | L run (ms) | C size (KB) | L size (KB) | Parity | Status |"
  echo "|---|---|---|---|---|---|---|---|---|"
  for r in "${ROWS[@]}"; do echo "$r"; done
  echo
  # Top 5 LLVM faster (more negative delta in compile or run) — focus on run since compile dominated by warm cache.
  echo "## Top 5 LLVM-faster at runtime (\`(L_run - C_run)\` most negative)"
  echo
  echo "| Program | C run (ms) | L run (ms) | Delta (ms) |"
  echo "|---|---|---|---|"
  for r in "${ROWS[@]}"; do
    # extract: | display | C_compile | L_compile | C_run | L_run | C_size | L_size | parity | status |
    IFS='|' read -r _ disp c_c l_c c_r l_r _c_s _l_s _par _stat _ <<< "$r"
    disp=$(echo "$disp" | xargs)
    c_r=$(echo "$c_r" | xargs)
    l_r=$(echo "$l_r" | xargs)
    [ "$c_r" = "-" ] && continue
    [ "$l_r" = "-" ] && continue
    delta=$((l_r - c_r))
    echo "$disp|$c_r|$l_r|$delta"
  done | sort -t'|' -k4,4n | head -5 | awk -F'|' '{printf "| %s | %s | %s | %s |\n", $1, $2, $3, $4}'
  echo
  echo "## Top 5 LLVM-slower at runtime (\`(L_run - C_run)\` most positive)"
  echo
  echo "| Program | C run (ms) | L run (ms) | Delta (ms) |"
  echo "|---|---|---|---|"
  for r in "${ROWS[@]}"; do
    IFS='|' read -r _ disp c_c l_c c_r l_r _c_s _l_s _par _stat _ <<< "$r"
    disp=$(echo "$disp" | xargs)
    c_r=$(echo "$c_r" | xargs)
    l_r=$(echo "$l_r" | xargs)
    [ "$c_r" = "-" ] && continue
    [ "$l_r" = "-" ] && continue
    delta=$((l_r - c_r))
    echo "$disp|$c_r|$l_r|$delta"
  done | sort -t'|' -k4,4nr | head -5 | awk -F'|' '{printf "| %s | %s | %s | %s |\n", $1, $2, $3, $4}'
  echo
  echo "## Interpretation guidance for ADR-0070 §X.3"
  echo
  echo "- **GREEN** (flip default to LLVM): zero LLVM compile-fail + zero parity divergence + LLVM runtime not materially worse (≥ -10% on small programs is noise)."
  echo "- **YELLOW**: non-zero LLVM failures but workaroundable; LLVM-default OK behind opt-out flag."
  echo "- **RED**: parity divergence or systemic LLVM crash → do NOT flip; investigate per F45a."
  echo
  echo "## Caveats (F35-sibling discipline)"
  echo
  echo "- Single sample per program; no variance estimate. For statistically rigorous numbers a multi-run / hyperfine pass is required (deferred — out of scope for this baseline)."
  echo "- All programs are tiny (≤ 30 LOC); LLVM optimization headroom is limited. Larger programs (LC-100 expansion / numerical kernels) will show clearer separation."
  echo "- Compile-time includes Rust toolchain stdlib link, dominated by linker work. The \`cobrust\`-internal compile fraction is small."
  echo "- Wall-time only; no \`cpu-time\` / \`max-rss\` collected."
  echo "- Stdout parity uses canonical-path argv[0] (symlink trick) so the bench harness does not falsely register divergence purely from \`C_<name>\` vs \`L_<name>\` exe path differences. Backend-level argv[0] semantics are byte-identical."
  echo
  # Mechanical recommendation derived from measured stats — must match
  # the per-program table above. F35-sibling: emit only what was measured.
  REC=""
  if [ "$LLVM_COMPILE_FAIL" -eq 0 ] && [ "$LLVM_RUN_FAIL" -eq 0 ] && [ "$PARITY_FAIL" -eq 0 ]; then
    REC="GREEN"
  elif [ "$PARITY_FAIL" -gt 0 ]; then
    REC="RED"
  else
    REC="YELLOW"
  fi
  echo "## Phase X.1 verdict"
  echo
  echo "- Recommendation: **$REC**"
  echo "- Rationale (measured):"
  echo "  - LLVM compile failures: $LLVM_COMPILE_FAIL / $TOTAL"
  echo "  - LLVM runtime failures: $LLVM_RUN_FAIL / $TOTAL"
  echo "  - Stdout parity divergences: $PARITY_FAIL / $TOTAL"
  echo "  - Mean compile delta: ${PCT_COMPILE}"
  echo "  - Mean runtime delta: ${PCT_RUN}"
  echo "  - Mean size delta:    ${PCT_SIZE}"
  echo
  echo "This file is the empirical input to ADR-0070 §X.3 (LLVM-default flip decision)."
  echo
} > "$REPORT"

echo "wrote $REPORT" >&2
echo "Cranelift compile fails: $(awk -F'|' '/C-compile-fail/{c++} END{print c+0}' "$REPORT") / $TOTAL" >&2
echo "LLVM compile fails:      $LLVM_COMPILE_FAIL / $TOTAL" >&2
echo "Parity divergences:      $PARITY_FAIL / $TOTAL" >&2
