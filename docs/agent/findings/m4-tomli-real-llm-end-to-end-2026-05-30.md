---
doc_kind: finding
finding_id: M4-real-llm-tomli-2026-05-30
title: "Live real-LLM (gpt-5.5) tomli closed loop PROVEN (12/12 single-fn + full-surface 99.51% fuzz parity vs CPython); the vacuous behavior gate was a rebrand-import regression — root-caused, fixed (f025dae), and the full-surface behavioral parity now measured"
status: candidate
empirical_date: 2026-05-30
model: gpt-5.5 (user-provided OpenAI-compatible endpoint; URL kept out-of-repo per security)
base_commit: 8b1a1fe
related: [adr:0032, adr:0036, adr:0039, adr:0040, finding:translator-real-vs-synthetic-status, F44]
---

# M4 — Live real-LLM tomli run (gpt-5.5, 2026-05-30): the honest split

## Why this run

External review (2026-05-30) flagged the project's flagship claim — CLAUDE.md §1.2's
**AI-native closed-loop translation** — as the most-advertised, least-verified surface:
a real LLM had only translated single tomli functions, never a recorded full-library
loop, and the production `cobrust translate` CLI is hardwired synthetic. This run drove
the *existing* real-LLM harnesses against a **live `gpt-5.5`** OpenAI-compatible endpoint
to record, honestly, exactly what does and does not hold end-to-end. (The harnesses pin
`BASE_URL` to a dead loopback by design — the live URL is never committed; it was set
locally for the run and reverted. Key passed via `USER_CODEX_API_KEY` env only.)

## What is GENUINELY PROVEN (real, live — not synthetic)

**audit-1 (`tomli._parse_bool`) — the single-function closed loop works end-to-end.**
`cargo test -p cobrust-translator --test audit_1_tomli_real_llm` against `gpt-5.5`:
- **G1 L1 dispatch** PASS — one real OpenAI round-trip, 3.38 s, 1482→1592 tokens,
  `cache_hit=false`, provider = a single `OpenAiProvider` (NO `SyntheticProvider`),
  isolated tempdir cache. The LLM genuinely emitted the Rust port.
- **G2 L2.build** PASS — the emitted Rust glued to the workspace preamble passes a real
  `cargo check` (6.56 s).
- **G3 L2.behavior** PASS — **12/12 strict-tier** differential cases vs the CPython 3.11
  `tomllib` oracle (true/false at offset, prefix-consume `trueX`, uppercase/titlecase
  rejection, empty rejection — every output byte-matches CPython).

This is the real claim in miniature, demonstrated live: **L0 spec → L1 real-LLM
translation → L2 build gate → L2 behavioral differential vs CPython**, all green, for one
real Python function, with a current frontier model.

**full_pipeline (12 functions) — the full surface TRANSLATES and COMPILES.**
`--test full_pipeline_tomli_real_llm`:
- **L1**: 12 functions, **12 OK / 0 ERR**, 167 s, **12 live OpenAI calls (no cache)**,
  12 ledger entries. gpt-5.5 translated the whole tomli 2.0.1 parser surface.
- **L2.build**: the 12 emitted functions ASSEMBLE into one crate that passes `cargo check`.
- Canonical 5 entrypoints (loads / parse_value / parse_array / parse_inline_table /
  parse_int): L1 + L2.build PASS.

## What is NOT proven — the VACUOUS behavior gate (the honest finding)

The full_pipeline harness reported **`OVERALL: PASS`**, but its behavioral gates ran
**ZERO cases**:
- `G3.smoke : 0/0 positive, 0/0 negative`
- `G3.fuzz  : 0 cases, 0 divergences, 0 panics`
- `G3.perf  : 0 ns / 0 ns / ratio 0.00`

A 0-case differential gate is **vacuously green** — it proves nothing about behavioral
parity, yet the harness counts it as PASS and **promoted** the gpt-5.5-emitted
`parser.rs` to `crates/cobrust-nest/` on that basis. (The promotion was NOT kept —
reverted, because the gate that approved it was empty.)

This is a **regression**, not a new gap: the *committed* `0.1.0-beta-tomli-full-
translation.md` finding records a prior run with a real **1024-input fuzz, 5 divergences,
0 panics, 99.51 %** behavioral result. Between that run and `8b1a1fe` the smoke/fuzz
**case-generation silently degraded to 0 cases**, and the harness's PASS verdict did not
notice — an F44-class "green that lies." So at HEAD the full-surface **behavioral parity
of the real-LLM translation is UNVERIFIED**, despite the green test.

## RESOLUTION (f025dae — root-caused, fixed, re-verified live)

**Root cause (precise).** Commit `0010653` (the ADR-0071 cobra-rebrand follow-up,
2026-05-28) renamed the in-tempdir synth-crate package `cobrust-tomli-llm-synth` →
`cobrust-nest-llm-synth` but left the harness's three embedded smoke/fuzz/perf test
strings importing the OLD name (`use cobrust_tomli_llm_synth::...`). Those generated
targets failed to compile (E0432); the `cargo test --test smoke/fuzz` subprocess exited
101 emitting no result lines; and `run_smoke_test`/`run_fuzz_test` — which read only
stdout and IGNORED the subprocess exit status — harvested 0 cases. The classifier then
reported 5/5 (it gates on smoke *failures*, empty when 0 ran) → vacuous PASS. A textbook
F44, born of the rebrand.

**Fix (f025dae).** (1) Corrected the three imports → `cobrust_nest_llm_synth`. (2) The
smoke/fuzz outcomes now capture the subprocess exit + stderr (a compile failure is
recorded, not swallowed) and expose `is_vacuous()`; a pure `derive_verdict(canonical_pass,
vacuous)` forces a 0-case gate to `FAIL-VACUOUS-BEHAVIOR-GATE`, refuses promotion, and
trips a hard assertion — proven by 6 synthetic, LLM-independent tests. A 0-case gate can
never silently pass again.

**Re-verified live (gpt-5.5, restored gate).** The full_pipeline re-run now runs REAL
behavioral cases against the gpt-5.5-emitted parser:
- **G3.smoke: 26/26 positive + 5/5 negative** (was 0/0).
- **G3.fuzz: 1024 inputs, 5 divergences, 0 panics — 99.51 % parity vs CPython `tomllib`**
  (was 0 cases; matches the pre-regression 0.1.0-beta baseline exactly).
- **G3.perf: 9.24×–14.57× faster than CPython** across 1KB / 100KB / 10MB (was 0 ns).
- Canonical 5/5; promotion now rests on a REAL gate.

So the full-surface **behavioral parity of the real-LLM translation IS now verified**:
gpt-5.5 translates the full tomli parser to Rust that is **99.51 % behaviorally equivalent
to CPython** (5 genuine divergences / 1024 fuzzed inputs) and ~10× faster. The 5
divergences are real and recorded honestly (the translation is not 100 %); the test
accepts them within tolerance (canonical 5/5 + 99.51 % ≥ threshold).

## Honest takeaway

- **REAL, proven live:** a frontier LLM (gpt-5.5) translates real Python (tomli) into
  Rust that *compiles* (single fn + full 12-fn surface) and, for `_parse_bool`, *matches
  CPython on a real differential oracle* (12/12). The core mechanism is not synthetic.
- **NOW proven too (after f025dae):** full-surface *behavioral* parity — gpt-5.5's
  full-tomli translation is **99.51 % equivalent to CPython** (5 div / 1024 fuzz) + ~10×
  faster, measured by the restored gate (see RESOLUTION).
- **STILL not proven:** the **production `pipeline::translate` repair loop** closing on a
  *real* divergence — the live evidence comes from the bespoke audit/full_pipeline
  harnesses, not the production path, whose default `BehaviorVerifier` is
  `AcceptAll`→`Skip` (ADR-0040 + translator-real-vs-synthetic-status). The
  narrative-vs-reality gap is now narrowed to exactly that — the *production repair loop* —
  not the translation and not the full-surface behavioral verification (both demonstrated).

## Follow-ups (the genuinely-new work this surfaces)

1. ✅ **DONE (f025dae) — vacuous behavior gate now FAILS-LOUD + case-gen restored.** The
   0-case regression was root-caused (the cobra-rebrand import drift, `0010653`) and fixed;
   a 0-case gate now hard-fails + refuses promotion; the restored gate was re-verified live
   (gpt-5.5, 99.51 % parity). See the RESOLUTION section above.
2. **Production closed loop.** Wire a real differential `BehaviorVerifier` (against
   `corpus/tomli/harness/h_loads.py`) + a real `cargo build` L2 gate into
   `pipeline::translate_with_verifiers`, and run tomli through it so the L1→repair→
   reconverge loop sees ≥1 real divergence with live diagnostic feedback (the manifest's
   `verification.divergences` is always `vec![]` today).

## Provenance

gpt-5.5 via the user's OpenAI-compatible endpoint (URL out-of-repo). Run 2026-05-30 at
`8b1a1fe`; harness `BASE_URL` edited locally and reverted; key via `USER_CODEX_API_KEY`.
Ledgers were in per-run tempdirs (not retained); the gate verdicts above are transcribed
from the live `--nocapture` output. No synthetic provider was registered in either run.
