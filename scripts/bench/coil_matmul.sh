#!/usr/bin/env bash
# Hardware-tagged one-command wrapper for the coil matrix-multiply 3-tier
# benchmark (the SECOND increment of the Cobrust perf-benchmark suite, after
# elementwise-add).
#
# Methodology (single source of truth): docs/agent/benchmarks/README.md
# Report:                               docs/agent/benchmarks/coil-matmul.md
# Bench source:                         crates/cobrust-coil/benches/matmul.rs
#
# What this adds over `cargo bench -p cobrust-coil --bench matmul`:
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
#   ./scripts/bench/coil_matmul.sh
#   COIL_MATMUL_SIZES=32,128,512 COIL_MATMUL_ITERS=101 \
#       ./scripts/bench/coil_matmul.sh

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
# numpy `@` is BLAS-backed; the BLAS flavour (OpenBLAS / Accelerate) matters
# for the T3/T1 headline, so we also try to print numpy's BLAS config.
PY=""
for c in /opt/homebrew/bin/python3.11 /opt/homebrew/bin/python3 \
         /usr/local/bin/python3.11 /usr/local/bin/python3 /usr/bin/python3 python3; do
  if "$c" -c 'import numpy' >/dev/null 2>&1; then PY="$c"; break; fi
done
if [ -n "$PY" ]; then
  PYV="$("$PY" -c 'import sys,numpy; print("python "+sys.version.split()[0]+", numpy "+numpy.__version__)' 2>/dev/null)"
  echo "T1_PYTHON_TAG=${PY} (${PYV})"
  # numpy's BLAS backend (the load-bearing detail for the headline ratio).
  BLAS="$("$PY" -c 'import numpy; print(numpy.__config__.show(mode="dicts")["Build Dependencies"]["blas"]["name"])' 2>/dev/null || echo unknown)"
  echo "T1_NUMPY_BLAS=${BLAS}"
else
  echo "T1_PYTHON_TAG=none-with-numpy (T1 will self-skip; T2/T3 still run)"
fi
echo
echo "## Benchmark output"
echo

cargo bench --manifest-path "$REPO_ROOT/Cargo.toml" \
  -p cobrust-coil --bench matmul 2>/dev/null
RC="${PIPESTATUS[0]}"
exit "$RC"
