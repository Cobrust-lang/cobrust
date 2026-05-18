---
doc_kind: dispatch
dispatch_id: 2026-05-18-phase-j-lsp-interface-scoping
title: "Pre-Phase-J LSP interface scoping spike"
parent_adr: 0054
target_adr: 0057 (frame) + 0057a..d (sub-batch)
status: design-only-spike
date: 2026-05-18
last_verified_commit: bc10842
relates_to: [adr:0054, adr:0052b, adr:0051, adr:0029]
host_routing: Mac-local (doc-only; no `cargo build`)
---

# Pre-Phase-J LSP interface scoping spike (2026-05-18)

## 1. Goal

Cobrust ships a `cobrust-lsp` binary implementing the Language Server Protocol surface, consumable verbatim by Cursor / Continue / Cody / Aider / VSCode / Neovim / IntelliJ. The binary lives in a new crate `crates/cobrust-lsp/`, depends on `tower-lsp` or `lsp-server`, and re-uses the existing `cobrust check` pipeline (`cobrust-frontend` → `cobrust-hir` → `cobrust-types` → `cobrust-mir`) for diagnostics + symbol resolution. MVP surface: `textDocument/{didOpen,didChange,hover,completion,definition,diagnostic,codeAction,rename}`. No `workspace/*` beyond `workspace/symbol` (Phase J+).

## 2. §2.5 LLM-amplifier rationale (load-bearing)

ADR-0054 §2 reranked Phase J above K/L/I because in-editor agent-LLMs (Cursor, Continue, Cody, Aider — the entire IDE-coding agent ecosystem) **consume LSP diagnostics + suggestion strings + code-actions directly**. ADR-0052b shipped `suggestion: Option<&'static str>` on every `TypeError::*` / `MirError::*` / `LoweringError::*` variant. Today that field is the LLM-amplifier private contract inside `cobrust check`. Phase J wires the field across the LSP envelope so it reaches the editor agent without prose-stripping or re-extraction — the §2.5 binding becomes operational across the entire IDE tooling layer.

Without Phase J, ADR-0052b's structured `suggestion:` field remains stranded in `cobrust check` stderr. With Phase J, every IDE-driven agent-LLM session benefits from the same structured fix-path that the CLI renderer already prints. This is the largest agent multiplier in the H-L roadmap.

## 3. Diagnostic to LSP wire format (the heart of Phase J)

Three canonical mappings demonstrate the full surface:

### 3.1 `TypeError::ImplicitTruthiness` → `Diagnostic`

```
TypeError::ImplicitTruthiness {
  actual: Ty::Int, span, suggestion: Some("change to `if x != 0:` (use `.is_some()` for Option)")
}
```
→
```
Diagnostic {
  range: span_to_lsp_range(span),
  severity: DiagnosticSeverity::ERROR,
  source: Some("cobrust"),
  message: "implicit truthiness on type `Int`",
  related_information: Some(vec![DiagnosticRelatedInformation {
    location: { uri, range },
    message: "change to `if x != 0:` (use `.is_some()` for Option)",
  }]),
}
```
The structured `suggestion` field becomes `related_information[0].message` verbatim; the primary `message` carries the diagnosis only. LLM consumer reads both.

### 3.2 `MirError::UseAfterMove` → `Diagnostic` + `CodeAction`

```
MirError::UseAfterMove {
  local: 7, span, suggestion: Some("change to `&s` to borrow without consuming (ADR-0052a)")
}
```
→ One `Diagnostic` (severity ERROR) plus one `CodeAction { kind: QuickFix, title: "borrow with `&s` instead of consuming", edit: WorkspaceEdit { changes: { uri: [TextEdit { range: span_before_local, new_text: "&" }] } } }`. The agent-LLM in Cursor sees the suggested edit + can apply it via `workspace/applyEdit` without composing the diff itself.

### 3.3 `TypeError::UnknownMethod` → `Diagnostic` + `CompletionItem`

```
TypeError::UnknownMethod {
  type_name: "Str", method_name: "splt", span, suggestion: Some("did you mean `split`?")
}
```
→ `Diagnostic` (severity ERROR; primary message `method 'splt' not found on 'Str'`) + on subsequent `textDocument/completion` at that span, a `CompletionItem { label: "split", kind: METHOD, sort_text: "0_split", documentation: ..., text_edit: TextEdit { range: method_name_span, new_text: "split" } }` ranks first.

### 3.4 Span conversion (shared helper)

`span_to_lsp_range(span: cobrust_frontend::span::Span) -> lsp_types::Range` lives in `cobrust-lsp/src/span.rs`. Requires a source-text map (LSP `Range` is line/character, Cobrust `Span` is byte offsets). Hooks the same source-map work deferred by `cobrust-cli/src/error_ux.rs:343-352` (`span_to_line_col`); Phase J finally pays the deferred M15 source-map cost.

## 4. LSP feature surface (MVP scope)

Ordered by §2.5 ROI:

1. **textDocument/publishDiagnostics** (PRIORITY 1 — direct §2.5 payoff). Every `cobrust check` error round-trips through `Diagnostic`. Incremental: re-runs on `didChange` debounced 250ms. ADR-0057a owns the wire mapping.
2. **textDocument/hover** (PRIORITY 2 — type info + doc on cursor). Consumes existing `Session::step()` `:type` directive path from ADR-0029 §"Tab completion sources"; emits `Hover { contents: MarkupContent { kind: Markdown, value: format!("```cobrust\n{ty}\n```\n\n{doc}") } }`. ADR-0057b owns hover + completion.
3. **textDocument/completion** (PRIORITY 3 — PRELUDE-fn + method-form completion). Sources: keywords, stdlib PRELUDE (per ADR-0034), in-scope `let` bindings, method-form chains (per ADR-0052d). Triggers: `.` (method completion), identifier-prefix typing. ADR-0057b.
4. **textDocument/definition** (PRIORITY 4 — goto-def for `fn` / `let` / type). Consumes `cobrust-hir` `DefId` → original-AST span map. Cross-file: requires Phase I workspace symbol table (§5). ADR-0057c.
5. **textDocument/codeAction** (PRIORITY 5 — apply `suggestion` as fix). Each diagnostic with a `suggestion` may emit a paired `CodeAction { kind: QuickFix }`. ADR-0057d. Wave-1 (`MirError::UseAfterMove`) is the canonical example; method-form typo-fix is the second canonical example.
6. **textDocument/rename** (PRIORITY 6 — symbol rename via HIR `DefId`). Cross-file workspace rename. ADR-0057c. Heaviest surface; may slip to Phase J+ window per ADR-0054 §5.4.

Out-of-MVP: `textDocument/signatureHelp`, `textDocument/documentSymbol`, `textDocument/foldingRange`, `textDocument/semanticTokens`, all `workspace/*` beyond `workspace/symbol`. Phase J+ micro-ADRs.

## 5. Incremental compile architecture

LSP demands <100ms per-keystroke type-check (Cursor / VSCode latency budget for diagnostics). The naïve approach (re-run full `cobrust check` on every `didChange`) blows the budget on any non-trivial file.

Phase J reuses the incremental `TypeCheckCtx` shaken out by Phase I REPL JIT (ADR-0056). The REPL Session state machine keeps a persistent `Subst` + `Env` + `MoveSet` across statements; Phase J generalises this to per-file `LspFileCtx { source_version, hir_tree, type_check_ctx, mir_funcs, diagnostics }`. On `didChange`:

- Diff the AST against the previous version (per-toplevel-item identity).
- Re-lower changed items only (HIR / MIR re-build is per-fn already per ADR-0011).
- Re-type-check changed items using cached `Subst` for unchanged items.
- Compute `Diagnostic` diff vs. previous publish — push only new + removed.

This architecture blocks on Phase I shipping the incremental `TypeCheckCtx`. Hence ADR-0054 §9 ordering: Phase I before Phase J.

## 6. Sub-ADR roster

Per ADR-0054 §5.3, Phase J ships as one frame ADR + four sub-batches:

- **ADR-0057** (frame) — Crate split, `tower-lsp` binding, LSP protocol version pin (v3.17 baseline), server lifecycle, `Initialize` capabilities advertised, `LspFileCtx` architecture, source-map cost-pay-down (§3.4).
- **ADR-0057a** — Diagnostics wire mapping. `From<TypeError> for Vec<Diagnostic>`, `From<MirError> for Vec<Diagnostic>`, `From<LoweringError> for Vec<Diagnostic>`. `related_information` carries `suggestion` field; primary `message` carries diagnosis. 6 example cases (3 in §3 here + 3 more in ADR).
- **ADR-0057b** — Hover + completion. `:type`-path reuse for hover; PRELUDE-fn + method-form + in-scope-binding completion sources. Trigger characters `.` + identifier.
- **ADR-0057c** — Definition + rename. `DefId` → original-span map; workspace symbol table; cross-file rename via `WorkspaceEdit`.
- **ADR-0057d** — CodeAction. Each `suggestion`-bearing diagnostic emits a paired `QuickFix` CodeAction. Wave-1 `UseAfterMove` + method-typo-fix as canonical examples.

## 7. Crate proposal

New `crates/cobrust-lsp/` (binary `cobrust-lsp` + library lib). `Cargo.toml`:

- `tower-lsp` v0.20.x (mature, MIT, async-tokio-based; alternative `lsp-server` v0.7 + `lsp-types` v0.95 — frame ADR picks one). Recommend `tower-lsp` for async + streaming-diagnostic ergonomics.
- `lsp-types` v0.95.x (shared with `tower-lsp`).
- `cobrust-frontend`, `cobrust-hir`, `cobrust-types`, `cobrust-mir` (workspace path deps).
- `tokio` v1.x (runtime).
- `tracing` (per CLAUDE.md §9 logging tokens).

Library exposes `LspServer`, `LspFileCtx`, `From<...> for Diagnostic` impls. Binary is a thin `tokio::main` wrapper.

No dependency on `cobrust-cli`; the LSP path replaces (does not call) the CLI rendering layer.

## 8. Risk register

Top three risks blocking Phase J success:

1. **`&'static str` suggestion vs LSP dynamic format.** ADR-0052b §2 + §11 pinned suggestions as `Option<&'static str>` (compile-time literal). LSP needs line number / variable name / type name interpolated into `related_information.message` for some cases (e.g. `TypeMismatch` "change to `: i64`" vs "change to `: str`"). Phase J likely ships **TypeError v2** with `suggestion: Option<String>` field — either by (a) breaking change to `cobrust-types::TypeError`, (b) adding a sibling `suggestion_dynamic: Option<String>` field, or (c) a renderer-side `SuggestionTemplate` enum that materialises at LSP-emission time. Frame ADR-0057 picks one. Decision criterion: minimum churn against ADR-0052b's 62 construction sites.

2. **Incremental type-context reuse from Phase I REPL JIT must support multi-file projects.** Phase I (ADR-0056) is a single-REPL-session state machine. Phase J is multi-file: edit one file, watch diagnostics across the dependent file set. The `LspFileCtx` per-file design (§5) addresses this, but requires Phase I's `TypeCheckCtx` to be `Clone + Send` and the cross-file `DefId` resolution to support per-file invalidation. Phase J frame ADR-0057 must verify Phase I delivers these properties before sub-batch dispatch.

3. **Editor compat matrix (Cursor / VSCode / Neovim / IntelliJ).** LSP is in principle vendor-neutral, but each editor enforces slightly different `Initialize` capabilities + `ServerCapabilities` shape compliance. Cursor is a VSCode fork — start with VSCode, verify Cursor by hand. Neovim (`nvim-lspconfig`) is the second baseline. IntelliJ (LSP-aware via `lsp4ij` or vendor LSP4IJ) is the riskiest — defer to Phase J+. Per §2.5, Cursor + VSCode are the highest-ROI editor agents; success criterion is `cobrust-lsp` connects to both and surfaces diagnostics in the editor UI for the s0052b corpus.

## 9. Pre-dispatch acceptance gate

Phase J frame ADR-0057 dispatch may proceed only when:

- [ ] Phase I REPL JIT (ADR-0056) shipped + incremental `TypeCheckCtx` confirmed `Clone + Send` + per-file-invalidation API exists.
- [ ] TypeError suggestion v2 design (Risk #1) decided: pick one of {breaking `String` migration, sibling `suggestion_dynamic`, `SuggestionTemplate` enum}. Documented in ADR-0057 §"Decision".
- [ ] `tower-lsp` v0.20 dep added to workspace `Cargo.toml` (or `lsp-server` v0.7 if frame ADR chooses that path).
- [ ] Source-map (`span_to_lsp_range`) cost-pay-down scoped — either inline in `cobrust-lsp` or as a shared utility in `cobrust-frontend`.
- [ ] M15 (proper diagnostic renderer) status confirmed — Phase J either co-ships M15 or explicitly defers.

Out of scope this spike: no implementation, no `Cargo.toml` edits, no `src/` touch. Spike output is design-only.

## 10. Acceptance criteria for the spike

This dispatch ships when:

- The §3 wire-format examples cover the three §2.5-canonical variants (`ImplicitTruthiness`, `UseAfterMove`, `UnknownMethod`) and demonstrate the `suggestion` → `related_information` / `CodeAction` / `CompletionItem` triad.
- §4 prioritises the 6 LSP features by §2.5 ROI; diagnostics is PRIORITY 1, rename is PRIORITY 6 (admittedly the weakest §2.5 payoff but completeness binding).
- §6 sub-ADR roster matches ADR-0054 §5.3 prediction (4 sub-ADRs under one frame).
- §8 risk register identifies the three risks that frame ADR-0057 must resolve before sub-batch dispatch — first risk (static-vs-dynamic suggestion field) is the largest non-deterministic design decision.

## 11. Forward references

- ADR-0054 §5 — Phase J framing + 2-3 week wall-time prediction.
- ADR-0054 §10 — pre-Phase-H prep work bullet "LSP interface scoping spike (~1 day, Mac-local doc-only)" — this dispatch is its operationalisation.
- ADR-0052b §11 + §"Out of scope" — JSON serialisation + LSP integration deferred to Phase J.
- ADR-0029 — REPL `:type` directive path reused by `textDocument/hover`.
- ADR-0034 — PRELUDE-fn list consumed by `textDocument/completion`.
- ADR-0052d — method-form chain consumed by completion + UnknownMethod CodeAction.

— P9 Tech Lead, 2026-05-18
