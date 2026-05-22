---
doc_kind: adr
adr_id: 0057
parent_adr: 0054
title: "Phase J frame — LSP server (highest §2.5 ROI post-Phase-G)"
status: proposed
date: 2026-05-18
last_verified_commit: 2a710d3
supersedes: []
superseded_by: []
relates_to: [adr:0054, adr:0052b, adr:0056, adr:0051]
discovered_by: P10/user 2026-05-18 — pre-Phase-J LSP scoping spike `docs/agent/dispatches/2026-05-18-phase-j-lsp-interface-scoping.md` ratifies here
ratification_path: P9 frame-ADR review; ratifies on first sub-ADR (0057a) dispatch
---

# ADR-0057: Phase J frame — LSP server (highest §2.5 ROI post-Phase-G)

## 1. Context

Per ADR-0054 §5 (HEAD `1fbed82`), Phase J ships `cobrust-lsp` as a
new workspace crate. ADR-0054 §2 ranks Phase J **rank-1 by §2.5 ROI**
across H-L (2-3w agent-velocity, 4-sub-ADR composition). Scoping spike
`docs/agent/dispatches/2026-05-18-phase-j-lsp-interface-scoping.md`
expands to 6 prioritised features + 5 sub-ADRs + 3 risks; this frame
ADR ratifies that decomposition verbatim and binds the Phase I × J
handoff contract that ADR-0056 §6 anticipates.

Constitutional anchors:

- **CLAUDE.md §2.5** (HEAD `1fbed82`) — LSP is the §2.5 §B
  "training-data overlap" realisation at the IDE tooling layer.
- **ADR-0054 §2** — Phase J rank 1, highest §2.5 ROI.
- **ADR-0052b §"Out of scope" L259** — JSON / LSP integration of
  structured `suggestion` field deferred to Phase J.
- **ADR-0056 §6** — Phase I × J handoff codifies `Session::type_ctx:
  Clone + Send` as the load-bearing primitive.

## 2. §2.5 LLM-amplifier rationale

Cursor / Continue / Cody / Aider — the entire in-editor agent-LLM
ecosystem — consume LSP diagnostics + suggestions + code-actions as
their primary fix-path signal. ADR-0052b shipped `suggestion:
Option<&'static str>` on every `TypeError::*` / `MirError::*` /
`LoweringError::*` variant (~62 sites per ADR-0052b §12). Today the
field is a private contract inside `cobrust check` stderr — the
structured shape exists but no machine consumer reads it.

Phase J wires the field across the LSP envelope: `suggestion` becomes
`Diagnostic.relatedInformation[0].message` (read by the agent) and
`CodeAction.title` (applied via `workspace/applyEdit`). The agent-LLM
sees diagnosis + fix path without prose-stripping. §2.5 compile-time-
catch + training-data-overlap compound: every catch surfaces with a
fix the agent applies mechanically.

Without Phase J, ADR-0052b's structured shape stays stranded.
With Phase J, every IDE-driven LLM dev session benefits. **Largest
agent multiplier in the H-L roadmap** per ADR-0054 §2 — §2.5
constitutional binding (ADR-0051) operational in the agent's daily
editing surface, not just `cobrust check` stderr.

## 3. Decision

Adopt the scoping spike's 6-feature ROI ordering, 5-sub-ADR roster,
and 3-risk register verbatim. Pin `tower-lsp` v0.20.x as the LSP
framework (mature, MIT, async-tokio). Pin LSP protocol v3.17. Create
new workspace crate `crates/cobrust-lsp/` (binary + library). Library
exposes `LspServer`, `LspFileCtx`, `From<...> for Diagnostic` impls;
binary is a thin `tokio::main` wrapper.

## 4. Six LSP feature priorities (per §2.5 ROI)

Per scoping spike §4:

1. **`textDocument/publishDiagnostics`** — **PRIORITY 1, highest §2.5
   payoff.** Every `cobrust check` error round-trips through `Diagnostic`.
   ADR-0052b `suggestion` → `relatedInformation[0].message`. Incremental:
   debounced 250ms on `didChange`. Owned by ADR-0057a.
2. **`textDocument/hover`** — **PRIORITY 2.** Type-of-expression on
   cursor; consumes ADR-0029 `:type` path; emits Markdown `Hover`.
   Owned by ADR-0057b.
3. **`textDocument/completion`** — **PRIORITY 3.** Sources: keywords,
   PRELUDE (ADR-0034), in-scope `let`, method-form (ADR-0052d).
   Triggers: `.`, identifier prefix. Owned by ADR-0057b.
4. **`textDocument/definition`** — **PRIORITY 4.** Goto-def via HIR
   `DefId` → original-AST span map; cross-file via workspace symbol
   table (§7). Owned by ADR-0057c.
5. **`textDocument/codeAction`** — **PRIORITY 5.** Each `suggestion`-
   bearing diagnostic emits paired `QuickFix`. Canonical examples:
   `UseAfterMove` `&s` borrow auto-fix; method-typo `splt`→`split` via
   `WorkspaceEdit`. Owned by ADR-0057d.
6. **`textDocument/rename`** — **PRIORITY 6.** Cross-file symbol
   rename via HIR `DefId`. Heaviest surface; may slip per ADR-0054
   §5.4. Owned by ADR-0057c.

Out-of-MVP: `signatureHelp`, `documentSymbol`, `foldingRange`,
`semanticTokens`, all `workspace/*` beyond `workspace/symbol`. Phase J+
micro-ADRs.

## 5. Diagnostic → LSP wire format

Three canonical mappings; full enumeration in ADR-0057a.

### 5.1 `TypeError::ImplicitTruthiness` → `Diagnostic` (§2.5-canonical)

Construction site (`crates/cobrust-types/src/check.rs:2076`, HEAD
`1fbed82` — corrected from stale `:1532` per audit `a0ed6f54b8f05e1cb`): `Err(TypeError::ImplicitTruthiness { actual: Ty::Int, span,
suggestion: Some("change to `if x != 0:` (use `.is_some()` for Option)") })`.

Maps to:

```json
{
  "range": <span_to_lsp_range(span)>, "severity": 1, "source": "cobrust",
  "message": "implicit truthiness on type `Int`",
  "relatedInformation": [{
    "location": { "uri": <uri>, "range": <range> },
    "message": "change to `if x != 0:` (use `.is_some()` for Option)"
  }]
}
```

Primary `message` = diagnosis only; structured `suggestion` →
`relatedInformation[0].message` verbatim.

### 5.2 `MirError::UseAfterMove` → `Diagnostic` + `CodeAction`

One ERROR `Diagnostic` plus one `CodeAction`:

```json
{ "title": "borrow with `&s` instead of consuming", "kind": "quickfix",
  "edit": { "changes": { "<uri>": [{ "range": <span_before_local>,
  "newText": "&" }] } } }
```

Agent in Cursor applies via `workspace/applyEdit` without composing
the diff itself.

### 5.3 `TypeError::UnknownMethod` → `Diagnostic` + `CompletionItem`

Primary `method 'splt' not found on 'Str'`; on subsequent
`textDocument/completion` at that span, `{ "label": "split", "kind":
2, "sortText": "0_split", "textEdit": { "range": <method_name_span>,
"newText": "split" } }` ranks first.

### 5.4 Span conversion

`span_to_lsp_range(span) -> lsp_types::Range` lives in
`crates/cobrust-lsp/src/span.rs`. Pays down the M15 source-map cost
that `crates/cobrust-cli/src/error_ux.rs:343-352` (`span_to_line_col`)
inlined as a stub.

## 6. Phase I × J handoff (codified)

Per ADR-0056 §6, Phase J consumes Phase I's **incremental `TypeCheckCtx`**:

```rust
pub struct Session {
    type_ctx: TypeCheckCtx,            // Clone + Send required
    user_funcs: HashMap<String, FuncId>,
    globals: HashMap<String, JitGlobalSlot>,
}
```

Phase J generalises single-REPL-session to multi-file project semantic
via `LspFileCtx { source_version, hir_tree, type_check_ctx,
mir_funcs, diagnostics }`. The LSP server forks per-`hover` snapshots
of `type_check_ctx` without contending on the live file context.
Phase I's `Clone + Send` property is load-bearing — failure to ship
it forces Phase J to re-derive the entire ctx per LSP request,
defeating incremental typing.

## 7. Incremental compile architecture

LSP demands <100ms per-keystroke type-check (Cursor / VSCode budget).
Naïve full `cobrust check` per `didChange` blows the budget on any
non-trivial file. Phase J reuses Phase I's incremental `TypeCheckCtx`.
On `didChange`: diff AST per-toplevel-item identity; re-lower changed
items only (HIR/MIR per-fn already per ADR-0011); re-type-check changed
with cached `Subst` for unchanged; compute `Diagnostic` delta, push
only new + removed. Multi-file invalidation: track `DefId` →
dependent-`DefId`s; on change invalidate dependents cross-file. Reuses
types-side dependency tracking. **Blocks on Phase I shipping
incremental `TypeCheckCtx`** — ADR-0054 §9 ordering.

## 8. Sub-ADR roster

Five sub-ADRs (extends ADR-0054 §5.3's 4-sub-ADR prediction by +1 per
scoping spike §6):

| ADR | Role | Day budget |
|---|---|---|
| **0057** | Phase J frame (this ADR) — crate split, `tower-lsp` bind, v3.17 pin, `Initialize` capabilities, `LspFileCtx` arch. | day 14 ratify |
| **0057a** | Diagnostics wire mapping (PRIORITY 1) — `From<TypeError/MirError/LoweringError> for Vec<Diagnostic>`; 6 example cases. **ACCEPTED** | day 1-3 |
| **0057b** | didChange incremental + Session reuse (wave-2.1) — INCREMENTAL sync; 100ms debounce; shared TypeCheckCtx; FileId pool. **ACCEPTED** | day 4-5 |
| **0057c** | Hover + completion (PRIORITY 2 + 3, wave-2.2) — word-boundary hover; PRELUDE + scope + keyword completion. **ACCEPTED** | day 6-7 |
| **0057d** | Rename (wave-2.3) — `prepareRename` + `rename` (WorkspaceEdit); single-document scope; 9-test gate. **ACCEPTED `e5bb708`** — Phase J wave-2 FULL CLOSED | day 8 |
| **0057e** | Definition + codeAction (wave-3) — go-to-def via `DefId`; paired `QuickFix` per `suggestion`; cross-file rename. **ACCEPTED** — Phase J wave-3 SHIPPED, v1.1 LSP server. Honest scope: same-doc word-scan goto-def + OPEN-doc cross-file rename; HIR `DefId` cross-file resolution deferred to wave-4. | day 9-14 |
| **0057f** | Inlay hints + semantic tokens + call hierarchy (wave-4) — `textDocument/inlayHint` per let-binding + per fn-arg; `semanticTokens/full` 8-type legend (keyword/string/number/comment/operator/variable/function/type); `prepareCallHierarchy` + `incomingCalls` + `outgoingCalls` same-document fn graph. **ACCEPTED `52d8315`** — Phase J wave-4 SHIPPED, **v1.2 LSP server**. Honest scope: same-document only; modifier bitmask flat zero; cross-file call-graph deferred to wave-5. 20 e2e tests + 6 insta snapshots. | day 15-20 |

Sub-ADRs ratify sequentially. Frame ratifies on 0057a dispatch.

## 9. Crate proposal

New `crates/cobrust-lsp/` (binary `cobrust-lsp` + library). `Cargo.toml`:

- `tower-lsp = "0.20"` (mature, MIT, async-tokio; chosen over
  `lsp-server` v0.7 for async + streaming-diagnostic ergonomics).
- `lsp-types = "0.95"` (shared).
- `cobrust-frontend`, `cobrust-hir`, `cobrust-types`, `cobrust-mir`
  (workspace path deps).
- `tokio = "1"`, `tracing` (per CLAUDE.md §9).

Library exposes `LspServer`, `LspFileCtx`, `From<...> for Diagnostic`.
Binary is a thin `tokio::main` wrapper. No `cobrust-cli` dep — LSP
path replaces (does not call) CLI rendering.

## 10. Risk register

Three top risks per scoping spike §8. Frame ADR must resolve each
before sub-batch dispatch.

1. **`&'static str` static-suggestion vs LSP dynamic-format need.**
   ADR-0052b §2 + §11 pinned `suggestion: Option<&'static str>` (62
   sites). LSP needs interpolated values (variable / type name / line)
   for some cases (e.g. `TypeMismatch` "change to `: i64`" vs "change
   to `: str`"). **Phase J ships TypeError v2** via one of: (a)
   breaking `cobrust-types::TypeError` to `Option<String>`, (b)
   sibling `suggestion_dynamic: Option<String>` field added alongside,
   (c) renderer-side `SuggestionTemplate` enum materialising at
   LSP-emission time. Frame leans (b) sibling-field path: minimum
   churn against 62 sites; existing static stays; new dynamic lands
   on new field. Final decision deferred to ADR-0057a dispatch eve
   once TEST corpus surfaces concrete dynamic-text needs.

2. **Phase I `TypeCheckCtx` must be `Clone + Send` + multi-file
   invalidation.** Phase I (ADR-0056) is single-REPL-session; Phase J
   is multi-file. `LspFileCtx` per-file design (§6) addresses this
   but requires `Clone + Send` + `DefId` per-file invalidation API.
   §11 pre-dispatch gate explicitly checks this before sub-batch
   dispatch.

3. **Editor compat matrix (Cursor / VSCode / Neovim / IntelliJ).**
   LSP is vendor-neutral in principle; each editor enforces slightly
   different `Initialize` / `ServerCapabilities` compliance. Cursor
   is a VSCode fork — VSCode baseline first, Cursor verify by hand.
   Neovim (`nvim-lspconfig`) is the second baseline. IntelliJ
   (`lsp4ij` / LSP4IJ) is riskiest — defer to Phase J+. Per §2.5,
   Cursor + VSCode are highest-ROI editor agents; success criterion
   is `cobrust-lsp` connects to both and surfaces diagnostics for the
   s0052b corpus.

## 11. Pre-dispatch acceptance gate

Phase J frame ADR-0057 dispatch may proceed only when:

- [ ] Phase I REPL JIT (ADR-0056) shipped + incremental `TypeCheckCtx`
      confirmed `Clone + Send` + per-file-invalidation API exists.
- [ ] TypeError suggestion v2 design (Risk 1) decided: pick one of
      {breaking `String`, sibling `suggestion_dynamic`,
      `SuggestionTemplate` enum}. Documented in ADR-0057a §"Decision".
- [ ] `tower-lsp = "0.20"` dep approved for workspace `Cargo.toml`.
- [ ] Source-map (`span_to_lsp_range`) cost-pay-down scoped — inline
      in `cobrust-lsp` or shared utility in `cobrust-frontend`.

**NOTE per user 2026-05-18 directive**: Phase J frame author may land
this ADR even with Phase I in flight. Frame design vs impl distinction
holds — the design contract (§§4-10) is ratifiable now; the impl
dispatch gate (§11) blocks on Phase I closure.

## 12. Consequences

### 12.1 Positive

- §2.5 LLM-amplifier ROI #1 surface delivered: every in-editor
  agent-LLM consumes `Diagnostic.relatedInformation` +
  `CodeAction.title` end-to-end without prose-stripping.
- ADR-0052b structured `suggestion` field operationalised; no longer
  stranded in `cobrust check` stderr.
- Phase I × J handoff codified (§6) — `Clone + Send` is a hard
  contract Phase I must honour.
- Incremental compile architecture (§7) generalises Phase I's
  single-session pattern to multi-file IDE features.
- Cursor + VSCode prioritised (highest §2.5 ROI editor agents);
  Neovim + IntelliJ deferred.

### 12.2 Negative

- 5-sub-ADR roster vs ADR-0054 §5.3's 4-sub-ADR prediction (+1) — ~2-4
  extra days in 2-3w wall.
- TypeError v2 design (Risk 1) deferred to ADR-0057a dispatch eve.
- `tower-lsp` v0.20 adds ~3MB to opt-in `cobrust-lsp` binary.
- LSP protocol-revision risk (v3.17 → v3.18); pinned major mitigates.

### 12.3 Neutral

- Crate split (§9) follows `cobrust-cli` pattern; no public API change.
- PRIORITY 6 rename is weakest §2.5 payoff but completeness binding.
- v0.4.0 binds on Phase H+I joint closure (ADR-0054 §9); v0.5.0 on
  Phase J full roster closure.

## 13. Dispatch readiness

Per ADR-0054 §5.2 (~5x compression vs ~3-month human estimate):

| Sub-ADR | TEST hrs | DEV hrs | Wall |
|---|---|---|---|
| 0057a (diagnostics) | 6 | 10 | 2-3 days |
| 0057b (hover+completion) | 8 | 14 | 3-4 days |
| 0057c (def+rename) | 10 | 18 | 4-5 days |
| 0057d (codeAction) | 4 | 6 | 1-2 days |
| 0057 frame ratify | 2 | 0 | 1 day |
| **Total** | **30** | **48** | **~2-3 weeks** |

Matches ADR-0054 §5.2 "2-3 weeks agent-velocity".

## 14. Why this ADR now

- §2.5 ROI #1 post-Phase-G per ADR-0054 §2. Codifying the frame now
  unblocks scoped sub-ADR dispatch the moment Phase I ships `Clone +
  Send`.
- User explicit directive 2026-05-18: "ADR-0057 — Phase J frame: LSP
  server (highest §2.5 ROI post-Phase-G)".
- ADR-0052b's `suggestion` field has been stranded since `365181a`
  (2026-05-17); Phase J is its one consumer. Field-sit delay raises
  TypeError v2 design drift risk (Risk 1).
- ADR-0056 §6 anticipates this ADR as the `Clone + Send` consumer.
  Codifying closes the Phase I × J handoff loop before either ships.
- Scoping spike `2026-05-18-phase-j-lsp-interface-scoping.md` ratifies
  6-feature scope + 5-sub-ADR roster + 3-risk register; frame binds
  them as decision.

— P9 Tech Lead, 2026-05-18
