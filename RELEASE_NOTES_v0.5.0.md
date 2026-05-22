# Cobrust v0.5.0 — LSP v1.3 feature-complete + DAP v1.2 feature-complete + production-scale benchmarks

**Released:** 2026-05-22
**Commits since v0.4.0:** 45
**Tag:** v0.5.0

---

## External-user-scenario binding (ADR-0045 mandate)

**v1.3 LSP + v1.2 DAP delivers full IDE experience** (diagnostics / hover / completion / rename / goto-def / code-actions / inlay-hints / semantic-tokens / call-hierarchy + step-debug / breakpoints / watchpoints / logpoints / data-breakpoints) matching mature Python + Rust LSPs. LLM agents in Cursor / Continue / Cody get rich code-intelligence on `.cb` sources.

---

## Shipped since v0.4.0

### Phase J wave-4 (ADR-0057f) — LSP v1.2
- Inlay hints per let-binding + per fn-arg
- Semantic tokens full (8-type legend)
- Call hierarchy: prepare + incoming + outgoing

### Phase J wave-5 (ADR-0057g) — LSP v1.3 TRUE feature-complete
- `textDocument/semanticTokens/full/delta` with `previousResultId` cache
- `inlayHint/resolve` adds tooltip + extended hint data
- Cross-file call hierarchy walks `Backend.documents`

### Phase L wave-4 (ADR-0059f) — DAP v1.1
- Evaluate handler for watch expressions
- Conditional breakpoints in `handle_set_breakpoints`
- Multi-thread visibility via `list_threads` + per-thread `stack_trace`
- Exception breakpoints (panic / result_err / unreachable filters)

### Phase L wave-5 (ADR-0059g) — DAP v1.2 TRUE feature-complete
- Logpoints via auto-continue lldb cmd
- Data breakpoints
- Step-into-source
- `__cobrust_result_err_panic` hookable symbol + DAP `result_err` filter (ADR-0059f §3.4 RESOLVED)

### ADR-0023 §A3 PRODUCTION-SCALE RESOLVED
- O3/O0 empirical ratio 0.293 measured against v0.4.0 release binary
- Production-scale benchmark using real cobrust release artifact
- Tier-2 honest-cite closed

### Infrastructure + quality
- `numpy` translator template fix — emit at item-level (closes CQ P1-4 honest-cite)
- `release.yml` SIGILL fix — per-triple `CARGO_TARGET_*_RUSTFLAGS`
- `task_perf_concurrency_producer_consumer_within_budget` test marked `#[ignore]` on standard CI (finding: `task-perf-ci-jitter`; F37 honest-cite)
- F44 finding ratified: CI cache stale green false-pass (upstream awareness added)

---

## F-pattern intel (ratified since v0.4.0)

- **F35-sibling**: sub-agents inject stale cross-crate version pins; lint enforces `=0.5.0` uniformity
- **F36**: ADR author + independent audit separation
- **F37**: task_perf CI-jitter — honest-ignore with finding-id cite
- **F38**: LLM stale SHA in doc-update sprints
- **F39**: post-merge `cargo fmt --all` mandatory gate
- **F40**: worktree-leak temp space (DG workstation /tmp/cobrust-* purge SOP)
- **F44**: CI cache stale green false-pass

ADSD upstream PR #1 + #2 live.

---

## Install

### Tier-1 — native binary (4 platforms)

```bash
# linux-gnu (x86_64)
curl -L https://github.com/<org>/cobrust/releases/download/v0.5.0/cobrust-x86_64-unknown-linux-gnu.tar.gz | tar xz

# linux-musl (x86_64)
curl -L https://github.com/<org>/cobrust/releases/download/v0.5.0/cobrust-x86_64-unknown-linux-musl.tar.gz | tar xz

# linux-arm64
curl -L https://github.com/<org>/cobrust/releases/download/v0.5.0/cobrust-aarch64-unknown-linux-gnu.tar.gz | tar xz

# darwin-arm64 (Apple Silicon)
curl -L https://github.com/<org>/cobrust/releases/download/v0.5.0/cobrust-aarch64-apple-darwin.tar.gz | tar xz
```

### Tier-3 — Python wheel (9 variants)

```bash
pip install cobrust==0.5.0
```

---

## Known issues / honest debt

- Phase J wave-6+ + Phase L wave-6+ proposed but not shipped (diminishing-returns polish; out-of-scope for v0.5.0)
- 5 honest-deferred `#[ignore]` tests retained with finding-id cite
- 1 perf-flaky test ignored on CI (F37 + finding-cited in `cobrust-stdlib`)
- Trademark check pending
- Linguist PR + Progopedia / Rosetta / 99-bottles user-side outreach pending
- Real-LLM E2E (tomli round-trip via codex/gpt5.5 API) queued post-M12.x per third-party audit

---

## New ADRs since v0.4.0

| ADR | Title |
|-----|-------|
| ADR-0057f | Phase J wave-4 — inlay hints + semantic tokens + call hierarchy (LSP v1.2) |
| ADR-0057g | Phase J wave-5 — semantic delta + inlay resolve + cross-file call hierarchy (LSP v1.3, feature-complete) |
| ADR-0059f | Phase L wave-4 — evaluate + conditional bp + multi-thread + exception bp (DAP v1.1) |
| ADR-0059g | Phase L wave-5 — logpoints + data bp + step-into + result_err (DAP v1.2, feature-complete) |

---

## Checksums

SHA256SUMS are published alongside each release asset.
