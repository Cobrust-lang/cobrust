---
doc_kind: finding
finding_id: M4-real-llm-tomli-2026-05-30
title: "Live real-LLM (gpt-5.5) tomli L0→L2 run: single-function closed loop PROVEN; full-surface behavioral gate is VACUOUS (0 cases) — an honest split"
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

## Honest takeaway

- **REAL, proven live:** a frontier LLM (gpt-5.5) translates real Python (tomli) into
  Rust that *compiles* (single fn + full 12-fn surface) and, for `_parse_bool`, *matches
  CPython on a real differential oracle* (12/12). The core mechanism is not synthetic.
- **NOT proven:** (a) full-surface *behavioral* parity — the fuzz gate that once ran 1024
  cases now runs 0; (b) the **production `pipeline::translate` repair loop** closing on a
  *real* divergence — still never exercised (the live evidence above comes from the
  bespoke audit/full_pipeline harnesses, not the production path, whose default
  `BehaviorVerifier` is `AcceptAll`→`Skip`; see ADR-0040 + translator-real-vs-synthetic-
  status). The narrative-vs-reality gap the review named is now pinned with precision: it
  is the *behavioral verification + repair loop at full surface*, not the translation itself.

## Follow-ups (the genuinely-new work this surfaces)

1. **F-candidate — vacuous behavior gate must FAIL-LOUD.** `full_pipeline`'s G3 counts a
   0-case smoke/fuzz/perf run as PASS. A 0-case gate is not a pass; the harness must hard-
   fail (or refuse to promote) when the oracle yields 0 cases. Then root-cause why
   smoke/fuzz case-generation dropped to 0 between the 0.1.0-beta run and `8b1a1fe`.
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
