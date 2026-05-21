---
doc_kind: adr
adr_id: 0057e
parent_adr: 0057
title: "Phase J wave-3 — goto-def + codeAction (FixSafety-gated) + cross-file rename"
status: accepted
date: 2026-05-21
last_verified_commit: cb86fbd
ratified_at: pending-merge
ratified_on: 2026-05-21
phase: "Phase J wave-3"
supersedes: []
superseded_by: []
relates_to: [adr:0057, adr:0057a, adr:0057b, adr:0057c, adr:0057d, adr:0056b, adr:0062]
discovered_by: ADR-0057 §8 sub-ADR roster (wave-3 row); user dispatch 2026-05-21
ratification_path: P9 sub-ADR review under ADR-0057 frame; ratifies on impl merge; closes Phase J wave-3 (v1.1 LSP)
---

# ADR-0057e: Phase J wave-3 — goto-def + codeAction + cross-file rename

## 1. Motivation

Phase J wave-2 (ADR-0057a/b/c/d) shipped the v1 LSP server with five handlers:
`publishDiagnostics`, incremental `didChange`, `hover`, `completion`,
`prepareRename` + intra-file `rename`. Wave-3 polishes the v1 → **v1.1** by
adding the three productivity essentials editors expect from any modern
language server:

1. **`textDocument/definition`** — F12 / Cmd+click navigation. Without it,
   the agent-LLM cannot follow a use-site to a def-site through the editor
   keybinding; it must re-grep the workspace by hand. This is the largest
   single navigation deficit in v1.
2. **`textDocument/codeAction`** — quick-fix UI driven by ADR-0062 §3.5
   `FixSafety` tier gating. ADR-0062 §3.5 already maps `FixSafety` →
   `CodeActionKind`; wave-3 wires the **emission path** end-to-end so the
   editor's quick-fix UI displays the suggestion text from
   `Diagnostic.relatedInformation[0]` as an actionable `CodeAction` with
   the appropriate auto-apply behaviour per tier.
3. **Cross-file `rename`** — extends ADR-0057d's intra-file rename to
   walk every **open** document in the workspace. ADR-0057d §4 explicitly
   defers this to wave-3; closing the gap completes the symbol-refactor
   verb.

After wave-3, all six top-priority LSP features from the scoping spike
ship complete. v0.5.0 binds on this closure per ADR-0054 §9.

## 2. §2.5 LLM-first audit

Per CLAUDE.md §2.5 + ADR-0051, wave-3's three features each pass the
training-data-overlap and compile-time-catch tests:

- **goto-def (training-data overlap)** — every Python `pyright` / `pylsp`
  / Rust `rust-analyzer` LSP training corpus contains `textDocument/
  definition` round-trips. The agent-LLM's first-try prediction for
  "navigate to definition" matches this exact LSP shape. Matching the
  shape maximises first-try correctness; emitting `GotoDefinitionResponse::
  Scalar(Location)` is the canonical Cursor / VSCode wire format.

- **codeAction (compile-time-catch realised in UI)** — ADR-0052b shipped
  structured `suggestion: Option<&'static str>` across ~62 error sites.
  ADR-0062 §3.5 mapped `FixSafety` → `CodeActionKind`. Wave-3 surfaces
  the result in the editor's quick-fix UI. The agent-LLM sees diagnosis +
  fix path + actionable UI without prose-stripping. Compile-time-catch
  → user-facing-experience round-trip is now complete: type-check fails
  → diagnostic published → quick-fix CodeAction visible → user (or
  agent-via-`workspace/applyEdit`) clicks apply → fixed source.

- **cross-file rename (closes ADR-0057d wave-2 limitation)** — agent-LLM
  refactor workflows extend across the open project, not a single file.
  ADR-0057d §4 acknowledged this as the wave-3 gap; closing it completes
  the rename verb. Honest scope: limited to **open** documents tracked by
  `Backend.docs: Mutex<HashMap<Url, DocState>>` (the existing wave-2.1
  store from ADR-0057b §3.4). True filesystem-wide workspace search is
  deferred to a follow-up sub-ADR (see §4).

## 3. Scope

### 3.1 `textDocument/definition`

`crates/cobrust-lsp/src/goto_def.rs` (new) exposes:

```rust
pub fn resolve_definition(
    source: &str,
    line_map: &LineMap,
    position: Position,
    ctx: &TypeCheckCtx,
    uri: Url,
) -> Option<GotoDefinitionResponse>;
```

Algorithm:

1. Position → byte offset via `LineMap::position_to_byte`.
2. `word_at_offset` (re-used from `hover.rs`) finds the identifier word.
3. `ctx.lookup(name)?` confirms the binding exists.
4. Find the first textual occurrence of `name` as a standalone word in
   `source` (wave-3 scope: same-document def-site resolution via word
   scan; cross-file def-site indexing deferred to wave-4 — true
   `DefId`-span map requires HIR-side span tracking that ADR-0057
   §5.4's `span_to_lsp_range` stub doesn't yet plumb through).
5. Return `GotoDefinitionResponse::Scalar(Location { uri, range })`.

Returns `None` if the cursor is not on an identifier, the name is a
keyword, or the binding is unknown.

### 3.2 `textDocument/codeAction`

Extend `crates/cobrust-lsp/src/code_action.rs` (existing wave-2.2 FixSafety
gating module — keep all existing public surface) with:

```rust
pub fn build_code_actions(
    diagnostics: &[Diagnostic],
    uri: &Url,
) -> Vec<CodeActionOrCommand>;
```

Algorithm: for each `Diagnostic` in `params.context.diagnostics`:

- Extract suggestion text from `diagnostic.related_information[0].message`
  (the wave-1 emission site, ADR-0057a §3.1).
- Extract `FixSafety` tier from the diagnostic's `code` field (set by
  `diagnostic.rs::make_diagnostic` to the variant name; we encode the
  tier via a `Diagnostic.data` JSON object `{"fix_safety": u8}` written
  at emission time — wave-3 amends `diagnostic.rs` to include this).
- Use `code_action_kind_for_fix_safety` (existing wave-2.2 function) to
  determine the `CodeActionKind`.
- If `Some(kind)`:
  - For tiers `FormatOnly` / `BehaviorPreserving` / `LocalEdit`: emit a
    `CodeAction` with `WorkspaceEdit` containing a single `TextEdit`
    replacing the diagnostic's range with `suggestion`. (Wave-3 honest
    scope: the suggestion text *is* the replacement text — this works
    for the §2.5-canonical `ImplicitTruthiness` case "change to `if x
    != 0:`" — but is naive for cases where the suggestion is a hint not
    a verbatim replacement. Conservative-default: only emit
    `WorkspaceEdit` for `BehaviorPreserving` + `LocalEdit`. For
    `FormatOnly` + `ApiChanging` emit a CodeAction with no edit
    payload, suggestion-only via `title`.)
  - For `ApiChanging`: emit a CodeAction kind = REFACTOR with
    `title` = suggestion, no `edit` payload.
- If `None` (TargetChanging / RequiresHumanReview): skip — no CodeAction
  emitted, diagnostic message-only.

### 3.3 Cross-file `rename` (extended)

Extend `crates/cobrust-lsp/src/rename.rs::rename_symbol` to ALSO walk other
open documents. The wave-2.3 implementation already accepts a single URI;
wave-3 adds:

```rust
pub fn rename_symbol_cross_file(
    primary_source: &str,
    primary_line_map: &LineMap,
    position: Position,
    new_name: &str,
    ctx: &TypeCheckCtx,
    primary_uri: Url,
    other_docs: &[(Url, String, LineMap)],
) -> Option<WorkspaceEdit>;
```

Algorithm:

1. Run `resolve_rename_symbol` on the primary doc (same wave-2.3 guards).
2. Run `collect_occurrences` on the primary doc → primary TextEdits.
3. For each `(uri, source, line_map)` in `other_docs`:
   - Run `collect_occurrences(source, old_name, new_name, line_map)`.
   - If non-empty, insert into the changes map under `uri`.
4. Build `WorkspaceEdit { changes: HashMap<Url, Vec<TextEdit>> }`.

The `Backend::rename` handler is updated to gather `other_docs` from
`self.docs` (excluding the primary URI) before calling the new function.

Note on scope-blindness: wave-3 retains the wave-2.3 word-boundary
heuristic across all open docs. If a name `x` shadows another `x` in a
different file's scope, both are renamed. This is the same limitation
wave-2.3 acknowledged for single-file shadowing — true scope-aware
rename requires HIR `DefId` resolution that deferred to wave-4.

## 4. Non-goals

- **NO inlay hints** (`textDocument/inlayHint`) — separate sub-ADR.
- **NO semantic tokens** (`textDocument/semanticTokens`) — separate
  sub-ADR.
- **NO call hierarchy** (`textDocument/prepareCallHierarchy` +
  `callHierarchy/incomingCalls` + `callHierarchy/outgoingCalls`) —
  separate sub-ADR (heaviest surface, deferred).
- **NO type hierarchy** — separate sub-ADR.
- **NO filesystem-walk workspace symbol search**. Cross-file scope is
  LIMITED to documents currently OPEN in `Backend.docs` (the wave-2.1
  `Mutex<HashMap<Url, DocState>>`). True workspace-wide indexing is
  deferred to a follow-up sub-ADR — it requires either:
  - A persistent filesystem index, or
  - The client supplying every workspace file via `workspace/symbol`
    + per-file `didOpen` (which Cursor / VSCode do for already-open
    files only).
- **NO `DefId`-span-map cross-file definition lookup** — wave-3
  goto-def uses same-document word-scan only. HIR-side `DefId` span
  tracking deferred to wave-4.
- **NO `workspace/applyEdit` reverse direction** — wave-3 emits
  `WorkspaceEdit` on rename + codeAction; the client owns the apply.
- **NO `CodeAction.command` payload** — wave-3 uses `CodeAction.edit`
  only (declarative edit). Imperative commands (e.g. "run formatter")
  deferred to a future sub-ADR.

## 5. Acceptance gate

15 tests total (9 integration + 6 snapshot):

| # | Surface | Category | Description |
|---|---|---|---|
| 1 | goto_def | integration | local var def — cursor on use returns Location of def-site |
| 2 | goto_def | integration | function def — cursor on call returns Location of fn-def-site |
| 3 | goto_def | integration | unresolved symbol — cursor on unknown name returns None |
| 4 | code_action | integration | BehaviorPreserving suggestion → CodeAction with WorkspaceEdit |
| 5 | code_action | integration | ApiChanging suggestion → CodeAction REFACTOR, no edit |
| 6 | code_action | integration | RequiresHumanReview → no CodeAction emitted |
| 7 | rename cross-file | integration | rename in file-A propagates to file-B (both open) |
| 8 | rename cross-file | integration | symbol not in file-C → file-C unchanged in WorkspaceEdit |
| 9 | rename cross-file | integration | WorkspaceEdit.changes aggregates all URIs correctly |
| 10 | snapshot | goto_def | known symbol Location shape |
| 11 | snapshot | goto_def | unresolved symbol None |
| 12 | snapshot | code_action | quickfix CodeAction shape (BehaviorPreserving) |
| 13 | snapshot | code_action | refactor CodeAction shape (ApiChanging) |
| 14 | snapshot | rename cross-file | WorkspaceEdit.changes 2-URI shape |
| 15 | snapshot | rename cross-file | WorkspaceEdit.changes 3-URI partial-match shape |

## 6. Implementation plan

Estimated ~500-800 LOC across:

- `crates/cobrust-lsp/src/goto_def.rs` (new) — `resolve_definition`
  (~80 LOC).
- `crates/cobrust-lsp/src/code_action.rs` (extend) — add
  `build_code_actions` (~80 LOC).
- `crates/cobrust-lsp/src/rename.rs` (extend) — add
  `rename_symbol_cross_file` (~60 LOC).
- `crates/cobrust-lsp/src/diagnostic.rs` (amend) — write `FixSafety`
  tier into `Diagnostic.data` JSON for codeAction extraction (~30 LOC).
- `crates/cobrust-lsp/src/lib.rs` (extend) — add `goto_definition` +
  `code_action` LSP handlers; extend `rename` to gather cross-file
  docs (~80 LOC).
- `crates/cobrust-lsp/tests/wave_3_e2e.rs` (new) — 15 tests per §5
  (~400 LOC).

Per-phase commits (6 atomic):

1. Author this ADR.
2. Implement `goto_def.rs` + `Backend::goto_definition` handler.
3. Extend `diagnostic.rs` with FixSafety data + extend `code_action.rs`
   with `build_code_actions` + `Backend::code_action` handler.
4. Extend `rename.rs` with `rename_symbol_cross_file` + extend
   `Backend::rename` handler to gather cross-file docs.
5. Add 15 tests in `wave_3_e2e.rs`.
6. Dual-track docs (zh, en, agent) update + ADR status flip to accepted.

## 7. ADR-0057 frame relation

This ADR closes Phase J wave-3 (final wave). Wave-3 row:

| Sub-ADR | Feature | Status |
|---|---|---|
| 0057e | goto-def + codeAction + cross-file rename | **this ADR** |

Post-wave-3: v1.1 LSP server shipped; Phase J full roster closed; v0.5.0
binds per ADR-0054 §9.

## 8. Consequences

### 8.1 Positive

- Editor parity: F12 navigation works; quick-fix UI works; cross-file
  rename works. The three v1→v1.1 productivity essentials all land.
- ADR-0062 §3.5 `FixSafety` tier gating becomes user-visible in editor
  UI, not just an internal `code_action_kind_for_fix_safety` API.
- ADR-0057d wave-2.3 honest scope limit (single-file rename) closed.
- Wave-3 = final Phase J wave; v0.5.0 unblocks.

### 8.2 Negative

- goto-def uses same-document word-scan only (not HIR `DefId` span
  map) — best-effort fallback; cross-file definition lookup deferred
  to wave-4.
- codeAction `WorkspaceEdit` emission is conservative: only
  `BehaviorPreserving` + `LocalEdit` get auto-apply edits; other tiers
  get message-only CodeActions. Verbatim suggestion-as-replacement-text
  works for §2.5-canonical cases but is naive for hint-style
  suggestions.
- Cross-file rename is scope-blind across open docs (wave-2.3 word-scan
  limitation extended cross-file). True scope-aware rename requires
  HIR `DefId` cross-file resolution.
- Filesystem-walk workspace search deferred (open-doc scope only).

### 8.3 Neutral

- ~500-800 LOC across 6 files; matches ADR-0057 §13 wave-3 budget
  estimate (10 TEST + 18 DEV hrs ≈ 4-5 days agent-velocity, compressed
  via 6-commit chain).
- `Diagnostic.data` JSON encoding adds a stable
  `{"fix_safety": u8}` shape consumed by codeAction extraction;
  forward-compatible with future codeAction extensions (e.g. quickfix
  IDs).

## 9. Why this ADR now

- ADR-0057 §8 wave-3 row scheduled; wave-2 closed at `9023f9d` (ADR-0057d
  ratified 2026-05-21). User dispatch 2026-05-21 explicitly directs
  Phase J wave-3 dispatch.
- Closes the §2.5 ROI #1 LSP surface; the in-editor agent-LLM gains all
  three productivity essentials.
- v0.5.0 binds on Phase J full roster closure per ADR-0054 §9 — wave-3
  is the last sub-ADR blocking that bind.

— P9 Tech Lead, 2026-05-21
