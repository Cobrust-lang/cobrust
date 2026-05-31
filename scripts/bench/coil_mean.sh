#!/usr/bin/env bash
# Hardware-tagged one-command wrapper for the coil full-array reduction (mean)
# 3-tier benchmark (the THIRD increment of the Cobrust perf-benchmark suite,
# after elementwise-add and matmul).
#
# Methodology (single source of truth): docs/agent/benchmarks/README.md
# Report:                               docs/agent/benchmarks/coil-mean.md
# Bench source:                         crates/cobrust-coil/benches/reduce.rs
#
# What this adds over `cargo bench -p cobrust-coil --bench reduce`:
#   - stamps a HARDWARE TAG block (CPU / cores / OS-kernel / arch / rustc /
#     python+numpy) so a report run is self-describing (honesty rule (d)).
#   - prints the bench's KEY=value + table output beneath the tag.
#
# F39 device-name clean: this script captures ONLY the CPU model, core count,
# OS kernel version, arch, and toolchain versions. It does NOT emit
# `uname -a` (which leaks a hostname) or any user/home path. The numbers are
# dev-laptop numbers — indicative, not a controlled rig (honesty rule (d)).
#
# Usage:
#   ./scripts/bench/coil_mean.sh
#   COIL_REDUCE_SIZES=1000,100000,10000000 COIL_REDUCE_ITERS=101 \
#       ./scripts/bench/coil_mean.sh

set -uo pipefail

REPO_ROOT="${REPO_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"

echo "## Hardware tag (honesty rule (d) — dev-laptop, indicative, not a controlled rig)"
echo

# --- CPU + cores + OS-kernel + arch (no hostname). -------------------------
case "$(uname -s)" in
  Darwin)
    CPU="$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo unknown)"
    CORES="$(sysctl -n hw.logicalcpu 2>/dev/null || echo '?')"
    ;;
  Linux)
    CPU="$(grep -m1 'model name' /proc/cpuinfo 2>/dev/null | cut -d: -f2- | sed 's/^ *//' || echo unknown)"
    CORES="$(nproc 2>/dev/null || echo '?')"
    ;;
  *)
    CPU="unknown"; CORES="?";
    ;;
esac
echo "CPU=${CPU}"
echo "CORES=${CORES}"
# `uname -srm` = kernel-name + kernel-release + machine (arch). NO -n (node
# name / hostname), NO -a (which includes the hostname).
echo "OS=$(uname -srm)"
echo "RUSTC=$(rustc --version 2>/dev/null || echo unknown)"

# --- T1 interpreter + numpy version (best-effort; informational). ----------
# numpy `mean` is a C/SIMD-backed pairwise sum (NOT a BLAS call), so the
# BLAS flavour is less load-bearing than for matmul — but we still print it
# for parity with the matmul wrapper (a SIMD-tuned numpy is the T3/T1 story).
PY=""
for c in /opt/homebrew/bin/python3.11 /opt/homebrew/bin/python3 \
         /usr/local/bin/python3.11 /usr/local/bin/python3 /usr/bin/python3 python3; do
  if "$c" -c 'import numpy' >/dev/null 2>&1; then PY="$c"; break; fi
done
if [ -n "$PY" ]; then
  PYV="$("$PY" -c 'import sys,numpy; print("python "+sys.version.split()[0]+", numpy "+numpy.__version__)' 2>/dev/null)"
  echo "T1_PYTHON_TAG=${PY} (${PYV})"
  # numpy's BLAS backend — printed for parity with coil_matmul.sh (informational
  # for a reduction, which does not dispatch to GEMM).
  BLAS="$("$PY" -c 'import numpy; print(numpy.__config__.show(mode="dicts")["Build Dependencies"]["blas"]["name"])' 2>/dev/null || echo unknown)"
  echo "T1_NUMPY_BLAS=${BLAS}"
else
  echo "T1_PYTHON_TAG=none-with-numpy (T1 will self-skip; T2/T3 still run)"
fi
echo
echo "## Benchmark output"
echo

cargo bench --manifest-path "$REPO_ROOT/Cargo.toml" \
  -p cobrust-coil --bench reduce 2>/dev/null
RC="${PIPESTATUS[0]}"
exit "$RC"
