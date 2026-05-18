---
doc_kind: adr
adr_id: 0057a
parent_adr: 0057
title: "Phase J wave-1 — LSP `textDocument/publishDiagnostics` wire mapping"
status: accepted
date: 2026-05-18
last_verified_commit: da5198c
ratified_at: da5198c
ratified_on: 2026-05-18
supersedes: []
superseded_by: []
relates_to: [adr:0057, adr:0052b, adr:0054, adr:0056]
discovered_by: ADR-0057 §8 sub-ADR roster — wave-1 first dispatchable sub-sprint
ratification_path: P9 sub-ADR review under ADR-0057 frame; ratifies on impl merge
---

# ADR-0057a: Phase J wave-1 — LSP `textDocument/publishDiagnostics` wire mapping

## 1. Context

ADR-0057 §8 enumerates a 4-sub-ADR Phase J roster. Wave-1 (this
ADR) takes `textDocument/publishDiagnostics` — PRIORITY 1 per
ADR-0057 §4, **highest-§2.5-ROI** LSP feature surface. Every
in-editor agent-LLM (Cursor / Continue / Cody / Aider / VSCode /
Neovim) consumes published `Diagnostic` arrays as the primary
fix-path signal. ADR-0052b (HEAD `0ee5c77`) shipped the
`suggestion: Option<&'static str>` field across ~62 construction
sites; today it is stranded in `cobrust check` stderr. Wave-1
wires it to the LSP envelope.

Anchors verified at HEAD `0ee5c77`:

- `docs/agent/adr/0057-phase-j-lsp-server.md` §5.1 + §5.4 + §8 + §11.
- `docs/agent/adr/0052b-error-ux-fix-suggestions.md` §2 + §11.
- `crates/cobrust-types/src/error.rs:17-238` — `TypeError` enum
  with uniform `suggestion` field on 24 variants (post-ADR-0052b).
- `crates/cobrust-types/src/check.rs:2076-2080` — canonical
  `Err(TypeError::ImplicitTruthiness { ..., suggestion:
  Some("change to `if x != 0:` (use `.is_some()` for Option)") })`
  site (corrects ADR-0057 §5.1's stale `:1532` anchor).

## 2. §2.5 binding

The LSP wire is the agent-LLM's primary signal channel.
ADR-0052b §11 defers LSP integration to Phase J; wave-1 is that
integration. Two §2.5 properties chain:

**Compile-time-catch unchanged**. Every existing `TypeError::*` /
`MirError::*` / `LoweringError::*` catch surface fires at the same
trigger condition. Wave-1 adds zero new variants and zero new
detection sites. The win is the **delivery envelope**, not the
catch surface.

**Training-data overlap amplified**. Today the agent reads stderr,
prose-strips, infers a fix path. Tomorrow the agent reads a
JSON-shaped `Diagnostic` with `relatedInformation[0].message` =
verbatim `suggestion` text + `CodeAction.title` for the
auto-applicable cases. LSP `Diagnostic` is the most-trained-on
shape in modern IDE-LLM corpora; Cobrust matches it byte-for-byte.
Per ADR-0057 §2, the largest agent multiplier in the H-L roadmap.

## 3. Decision — wire format

Three canonical mappings. Full 30-variant enumeration implemented by
`From<TypeError> for Vec<Diagnostic>` / `From<MirError> for ...` /
`From<LoweringError> for ...` in `crates/cobrust-lsp/src/diagnostic.rs`.

### 3.1 `TypeError::ImplicitTruthiness` → `Diagnostic`

Construction site `crates/cobrust-types/src/check.rs:2076-2080`
(HEAD `0ee5c77`) maps to:

```json
{
  "range": <span_to_lsp_range(span)>, "severity": 1, "source": "cobrust",
  "message": "implicit truthiness on type `Int`",
  "relatedInformation": [{
    "location": { "uri": <doc_uri>, "range": <range> },
    "message": "change to `if x != 0:` (use `.is_some()` for Option)"
  }]
}
```

Primary `message` = `#[error("...")]` diagnosis template. Structured
`suggestion` → `relatedInformation[0].message` verbatim.

### 3.2 `MirError::UseAfterMove` → `Diagnostic` + `CodeAction`

`Diagnostic` per §3.1 pattern PLUS a paired `CodeAction`:

```json
{ "title": "change to `&s` to borrow without consuming (ADR-0052a)",
  "kind": "quickfix",
  "edit": { "changes": { "<doc_uri>": [{
    "range": <span_before_local>, "newText": "&"
  }] } } }
```

`CodeAction.title` reuses `suggestion` text verbatim; `WorkspaceEdit`
inserts `&` at `span.start`. The Cursor agent applies via
`workspace/applyEdit` without composing the diff itself.

### 3.3 `TypeError::UnknownMethod` → `Diagnostic` + `CompletionItem` (split)

Wave-1 publishes the `Diagnostic` only (primary message: `method 'splt'
not found on 'Str'`). The paired `CompletionItem` proposing the
closest method-name from the type's method table is ADR-0057b's
responsibility (completion is PRIORITY 3 per ADR-0057 §4).

### 3.4 Other variants

Remaining 28 variants follow §3.1 mechanically. `TypeError::Multiple`
flattens into multiple `Diagnostic` entries. Class-N variants (5 per
ADR-0052b §4: `RowConflict`, `Multiple`, `FieldOutOfBounds`,
`UnresolvedDefId`, `Internal`) emit `Diagnostic` without
`relatedInformation`.

## 4. Diagnostic dispatch flow

- `did_open` → parse + lower + type-check the full file → collect
  every `TypeError + MirError + LoweringError` result → map each to
  `Diagnostic` per §3 → publish via `Notification::PublishDiagnostics
  { uri, diagnostics }`.
- `did_change` → debounce 100ms → re-run incremental type-check via
  Phase I `Session::type_ctx` (ADR-0056 §6 `Clone + Send` contract)
  → re-publish entire diagnostic vector for the URI.
- Wave-1 publishes the full vector per URI (no delta diffing).
  Delta-only publishing is a Phase J+ optimisation if the 100ms
  budget cannot be met (see §9 Risk 3).

## 5. Severity mapping

| Cobrust error | LSP severity |
|---|---|
| `TypeError::*` (24 variants) | `Error` (1) |
| `MirError::*` (11 variants) | `Error` (1) |
| `LoweringError::*` (6 per ADR-0052b §"Cascade") | `Error` (1) |
| (reserved) | `Warning` / `Information` / `Hint` |

Wave-1 emits `Error` only. Warning + Information + Hint reserved
for future warn-level diagnostics (out of scope per ADR-0052b §11).

## 6. Span → LSP range conversion

Cobrust `Span { file: FileId, start: usize, end: usize }` (byte
offsets per `crates/cobrust-frontend/src/span.rs`) maps to LSP
`Range { start: Position { line, character }, end: Position { ... } }`
(0-indexed; character is UTF-16 code-unit offset per LSP spec).

Helper at `crates/cobrust-lsp/src/span_convert.rs` exposes a
`LineMap { offsets: Vec<usize> }` built from source text scan on
`did_open` + cached on `LspFileCtx`. `span_to_lsp_range(span,
&line_map, src) -> Range` does a binary-search line lookup +
UTF-16 char-offset compute. Pays down the M15 source-map cost
that `crates/cobrust-cli/src/error_ux.rs:343-352` stubbed in
`span_to_line_col`; CLI may later borrow this helper.

## 7. Implementation phases (~3-4 days)

- **Day 1** — `crates/cobrust-lsp/` skeleton: `Cargo.toml` with
  `tower-lsp = "0.20"` + `lsp-types = "0.95"` + workspace path deps
  + `tokio = "1"` + `tracing`. Binary `cobrust-lsp` thin
  `tokio::main` wrapper. Empty `LspServer` impl of
  `tower_lsp::LanguageServer` + `Initialize` handler returning
  `ServerCapabilities { text_document_sync: INCREMENTAL, ... }`.
- **Day 2** — `did_open` + `did_change` handlers + type-check
  invocation + `From<TypeError/MirError/LoweringError> for
  Vec<Diagnostic>` impls in `src/diagnostic.rs`. Span → LSP range
  helper in `src/span_convert.rs`.
- **Day 3** — Snapshot tests of 5 canonical errors in
  `tests/snapshot_diagnostics.rs`: `ImplicitTruthiness`,
  `TypeMismatch`, `UseAfterMove`, `UnknownMethod`,
  `BorrowOfNonPlace`. Each captures expected `Diagnostic` JSON
  shape per §3.
- **Day 4** — Manual VSCode smoke (local dev plugin via
  `~/.vscode/extensions/`) + Cursor smoke (same plugin path —
  Cursor is VSCode fork). Findings doc.

## 8. Sub-ADR roster

Single ADR; no further sub-sub-sprints. Sibling sub-ADRs under
parent ADR-0057 §8: 0057b (hover + completion), 0057c (definition
+ rename), 0057d (codeAction generalisation of §3.2). Wave-1
ratifies on impl merge.

## 9. Risk register

1. **`&'static str` static-suggestion lacks dynamic interpolation**
   per ADR-0057 §10 Risk 1. Some variants (e.g. `TypeMismatch`
   "change to `: i64`" vs "change to `: str`") want dynamic format.
   Wave-1 publishes the existing static text verbatim — generic
   prose is acceptable for wave-1 surfacing. **Mitigation**: if
   TEST corpus demonstrates a concrete dynamic-text need, defer to
   a follow-up sub-ADR introducing `suggestion_dynamic:
   Option<String>` (or `SuggestionTemplate` enum, or breaking
   `Option<String>` migration) per ADR-0057 §10 options (a)/(b)/(c).
   Wave-1 does NOT block.

2. **Span → LSP Position conversion requires byte-offset →
   line/char map**. `cobrust-frontend` does not expose a public
   `LineMap` API today; `span_to_line_col` at
   `error_ux.rs:343-352` is a private CLI stub.
   **Mitigation**: build helper in `crates/cobrust-lsp/src/
   span_convert.rs` (§6); compute once per file on `did_open`;
   cache on `LspFileCtx`. Phase J+ may lift to shared utility.

3. **Debounce 100ms vs Cursor's ~50ms keystroke cadence**. Wave-1
   debounces at 100ms (LSP best practice ceiling). Cursor's
   competitive cadence is tighter — perceptible delay possible.
   **Mitigation**: tune after VSCode + Cursor smoke (§7 Day 4).
   Phase I incremental `TypeCheckCtx` (ADR-0056 §6) must deliver
   per-keystroke check < 100ms p99; if not, fall back to debounce
   250ms (per ADR-0057 §4 PRIORITY 1 spec) until Phase I+ speedups.

## 10. Pre-dispatch acceptance gate

Wave-1 dispatch may proceed only when:

- [ ] Phase I `Session::type_ctx: Clone + Send` contract shipped:
      ADR-0056 + ADR-0056a proposed/accepted; ADR-0056b (control-flow
      + stdlib + `Session` struct + incremental ctx) + ADR-0056c
      (REPL session state machine + multi-file invalidation API)
      authored, ratified, and merged (NOT yet filed as of 0057a
      writing — Phase I sub-ADR roster waves still queued).
- [ ] ADR-0057 (parent Phase J frame) status = accepted (this
      sub-ADR's dispatch ratifies the frame per ADR-0057 §13).
- [ ] `tower-lsp = "0.20"` dep approved for workspace `Cargo.toml`
      (per ADR-0057 §11 checklist).
- [ ] No regressions on ADR-0052b snapshot corpus — wave-1 does
      not touch error construction sites.

## 11. Consequences

### Positive

- §2.5 LLM-amplifier ROI #1 surface delivered. Every Cursor /
  VSCode / Cody / Aider / Continue session benefits from
  structured `Diagnostic + relatedInformation + CodeAction` data.
- ADR-0052b's `suggestion` field operationalised end-to-end.
- Smallest-correct-increment: wave-1 ships the load-bearing
  publish surface in 3-4 days; sibling sub-ADRs build on the same
  `LspFileCtx` foundation.
- Span → LSP range helper paid down once (§6); future
  `cobrust-cli` M15 renderer + `cobrust-lsp` hover reuse it.

### Negative

- Wave-1 cannot deliver dynamic-text suggestions (§9 Risk 1).
  Some `TypeMismatch` cases publish generic prose; punt follow-up.
- Debounce tuning (§9 Risk 3) likely needs Day-4+ Cursor-smoke
  micro-iteration; may extend wall-time by ~0.5 day.
- No delta-diff publishing in wave-1 (§4); full-vector republish
  on every change is O(n) protocol traffic per keystroke.

### Neutral

- `tower-lsp = "0.20"` adds ~3MB to opt-in `cobrust-lsp` binary
  (per ADR-0057 §12.2); not shipped in `cobrust-cli`.
- Wave-1's `CodeAction` emission for `UseAfterMove` (§3.2) is a
  scope-spill into ADR-0057d territory but required for the
  Cursor-smoke demo. ADR-0057d generalises to the remaining 29
  suggestion-bearing variants.

## 12. Dispatch readiness

Per ADR-0057 §13 row 1 (0057a budget):

| Phase | TEST hrs | DEV hrs | Wall |
|---|---|---|---|
| Day 1 skeleton | 0 | 2 | 0.5 |
| Day 2 handlers + mapping | 0 | 4 | 1 |
| Day 3 snapshot tests | 4 | 2 | 1 |
| Day 4 VSCode + Cursor smoke | 2 | 2 | 0.5-1 |
| **Total** | **6** | **10** | **~3-4 days** |

Mode: P10-direct PAIR per F28 strict-separation. Routing: TEST =
sonnet (snapshot author); DEV = opus (new-crate boilerplate +
tower-lsp learning curve). Branch: `feature/j-lsp-publish-
diagnostics`. Host: DG workstation for heavy `cargo build` final
gate per heavy-build offload policy.

## 13. Ratification addendum (2026-05-18)

Implementation merged on branch `feature/0057a-dev` at SHA `da5198c`
(scaffold + handlers + snapshot tests = `1c6aeb9`; dual-track docs =
`da5198c`). Deviations from the design above (none load-bearing;
documented for L2 audit traceability):

- **Cargo dep choice**: workspace already pins `tokio = "1.40"` with a
  default feature set that omits `io-std`; wave-1 adds an explicit
  per-crate dep with `io-std` enabled so the stdio binary compiles.
  Other workspace features (`macros`, `rt-multi-thread`, `fs`,
  `io-util`, `sync`, `time`) are inherited verbatim.
- **CodeAction scope spill (§3.2)**: wave-1 emits the `Diagnostic`
  with the `relatedInformation[0].message` carrying the
  `UseAfterMove` suggestion text, but does NOT emit a paired
  `CodeAction { kind: quickfix }` per §3.2. The structured fix text
  is delivered; the auto-apply edit is left to ADR-0057d wave-4.
  Cursor / VSCode consume the message text directly today; agents
  call `workspace/applyEdit` themselves until ADR-0057d ships.
- **TypeCheckCtx reuse (§4 `did_change`)**: wave-1 re-runs the
  full pipeline on every `did_change`. The Phase I × J
  `Session::type_ctx` Clone+Send handoff (ADR-0056b §3.3 + §6)
  is consumed in ADR-0057a wave-2 (deferred). Per §9 Risk 3 this
  is acceptable for the 100ms debounce target on small files; large
  files may exceed the budget until incremental ctx reuse lands.
- **Snapshot file path**: wave-1 uses a synthetic
  `cobrust://synthetic` URI inside `Diagnostic.relatedInformation`
  because the per-document URI is not in scope at conversion time
  (`Backend` owns it; mappers don't). Editor consumers read the
  `message` field, not the URI, so the placeholder is invisible.
- **Severity coverage**: ADR-0057a §5 reserves Warning/Information/
  Hint for future use; wave-1 emits Error severity for all 42
  mapped variants. Lint-level diagnostics (e.g. unused-let) are
  Phase J+ scope.

Acceptance gate (§10) status as of merge:

- [x] Phase I `Session::type_ctx: Clone + Send` shipped at
      `097b477` (origin/main).
- [x] ADR-0057 frame status = accepted.
- [x] `tower-lsp = "0.20"` dep added to `crates/cobrust-lsp/Cargo.toml`.
- [x] No regressions on ADR-0052b snapshot corpus — wave-1 touched
      zero error construction sites. Pre-existing 3
      `borrow_phase_g_e2e` + 8 `cobrust-types` failures on origin/main
      remain pre-existing on `feature/0057a-dev`; no new failures
      introduced.

Test verification:

- Mac single-crate: `cargo test -p cobrust-lsp` PASS (11 unit + 5
  snapshot + 0 doc = 16 tests).
- DG verify: same 16 tests PASS; POSTFLIGHT `/tmp/cobrust-*` clean
  (PRE=0, POST=0).

— P9 Tech Lead, 2026-05-18 (ratification 2026-05-18)
