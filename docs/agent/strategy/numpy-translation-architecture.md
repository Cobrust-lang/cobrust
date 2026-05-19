---
doc_kind: strategy
strategy_id: numpy-translation-architecture
title: numpy translation — wrapper-first architecture insight
status: strategic-anchor
date: 2026-05-19
last_verified_commit: 1e57b85
relates_to: [adr:0007, adr:0011, adr:0012, adr:0013, adr:0021, adr:0032, adr:0036]
sourced_from: machine-local memory port 2026-05-19 (machine-loss-resilient copy)
---

# numpy Translation Architecture Insight

> READ THIS FIRST before any M7 / Phase N numpy sprint dispatch.

## Core insight (user 2026-05-19)

**numpy = a "wrapper translation" project, NOT a "numerical kernel rewrite" project.**

The community generally over-estimates numpy translation difficulty because they're
scared by the C layer — but the C layer is NEVER the translation target.

## numpy's actual LOC distribution

| Layer | LOC share | Translate? |
|---|---|---|
| Python upper (`numpy/_core/numeric.py`, `fromnumeric.py`, `lib/*.py`, `ma/*.py`, `random/*.py`) | ~30% | **YES** → Cobrust |
| C middle (`numpy/_core/src/multiarray`, `umath`) | ~50% | **NO** — keep as `.so` / `.dylib`, FFI from Cobrust |
| Fortran bottom (BLAS / LAPACK; not in numpy repo) | ~20% | **NO** — external dep, untouched |

C/Fortran are pure numerical kernels. Cobrust calls them via FFI exactly as CPython
does — only the "caller" changes from CPython to Cobrust.

## Translation targets (the 30%)

- `numpy/_core/numeric.py` — `array()`, `asarray()`, `zeros()` factory wrappers
- `numpy/_core/fromnumeric.py` — `sum/mean/argmax/sort` ufunc dispatch
- `numpy/lib/*.py` — `np.linalg.solve`, `np.fft.fft`, `index_tricks`, polynomial
- `numpy/ma/*.py` — masked array
- `numpy/random/*.py` — RNG high-level wrappers (Mersenne Twister itself is C)

Per-major-module LOC is similar to tomli (~800 LOC translation, audit #1 PASS
12/12 strict — ADR-0032). Same order of magnitude → same proven L0-L3 pipeline.

## Why this is the right path for Cobrust

1. **§1.2 AI-native compiler killer feature** — Python ↔ Cobrust is structural
   syntax diff (`def`/indent → static-type + `Result`), NOT semantic gulf. LLMs
   translate Python-wrappers 10× easier than C-kernels.
2. **ADR-0032 audit #1 already proved it** — tomli 5/5 PASS real-LLM E2E.
   `numpy/_core/numeric.py` ~1500 LOC is same difficulty grade.
3. **C extension untouched = zero perf loss** — `np.linalg.solve(A, b)` still
   hits same LAPACK. Cobrust may even be FASTER because Python wrapper
   boxing/PyObject overhead disappears (AOT + no GIL).
4. **L0-L3 closed-loop verification natural** — CPython running same numpy =
   oracle. Differential testing: 1000 fuzzer inputs per fn vs `np.allclose`
   tolerance per §4.2 L2 verification gate.

## Non-trivial design surfaces (deep-protocol coupling)

These are "detail-hard" not "path-wrong":

- `ndarray.__array_interface__` / `__array__` / `__array_function__` — hook
  protocols for pandas/scipy/PyTorch. Signatures must be bit-identical or
  downstream breaks.
- dtype object metaclass dance — Cobrust must simulate via trait +
  monomorphization (cleaner long-term but design needed; ADR required).
- `np.frompyfunc` / `np.vectorize` — dynamic ufunc generation. May need deferral
  OR a Cobrust-native `@ufunc` decorator (design ADR required).

## Prerequisite

PyO3 reverse-binding Cobrust runtime ([[adr:0011]] build path) — lets Cobrust
functions call C symbols from `multiarray.so` / `umath.so`. Mechanism proven in
M6 `cobrust-msgpack` native-extension demo.

## Scope estimate

- 1-2 phases (not 1-2 years).
- Phase ordering: PyO3 reverse-binding maturation → numeric.py spike → ndarray
  protocols → fromnumeric/lib batch → ma + random → numpy-as-tier-1 release
  validation.

## When to act on this insight

NOT pressing. M7 is post-Phase N (Phase K/L/M/N ahead). When the next "let's start
numpy" dispatch happens, this doc anchors scope correctly and prevents the "C layer
is the work" misframe.

## Cross-references

- [[adr:0007]] translator pipeline (the L0-L3 mechanism)
- [[adr:0011]] PyO3 build path (FFI to C extensions)
- [[adr:0012]] M7 numpy plan (high-level phase frame) — **see also §"See also"**
- [[adr:0013]] M7.0-M7.5 numpy sub-milestones (specific module roster)
- [[adr:0021]] M7.6 numpy expansion (Complex dtype tier)
- [[adr:0032]] audit #1 — tomli proof of concept for Python-wrapper translation
- [[adr:0036]] audit #3a — production rich-prompt builder + stateful
  `tomli::_parse_int` (this is the prompt-design level numpy needs)

## See also — hardware tiering

`docs/agent/strategy/numerical-compute-hardware-tiering.md` — CPU tier 0-3
(baseline → runtime-dispatch → native → multi-wheel) + GPU paths (cuBLAS FFI vs
NVPTX codegen) + §2.5 LLM-friendly ranking. Read before any numpy-cb SIMD or
`cobrust.gpu` dispatch.
