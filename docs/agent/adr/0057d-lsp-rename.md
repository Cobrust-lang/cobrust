---
doc_kind: adr
adr_id: 0057d
parent_adr: 0057
title: "Phase J wave-2.3 — LSP rename (textDocument/prepareRename + textDocument/rename)"
status: accepted
date: 2026-05-21
last_verified_commit: feature/0057d-rename
ratified_at: feature/0057d-rename
ratified_on: 2026-05-21
phase: "Phase J wave-2.3"
supersedes: []
superseded_by: []
relates_to: [adr:0057, adr:0057a, adr:0057b, adr:0057c, adr:0056b]
discovered_by: ADR-0057 §4 sub-ADR roster (PRIORITY 4 after diagnostics/didChange/hover/completion)
ratification_path: P9 sub-ADR review under ADR-0057 frame; ratifies on impl merge; closes Phase J wave-2
---

# ADR-0057d: Phase J wave-2.3 — LSP rename

## 1. Motivation

Wave-2.2 (ADR-0057c) delivered hover type inspection and completion — the two highest §2.5 ROI
features for read-only agent-LLM workflows. Wave-2.3 ships **rename**, the primary _refactor_
verb in every IDE. Without rename, an agent-LLM that wants to improve a symbol name must emit
a manual find/replace prompt (fragile, scope-blind, no undo). With rename:

- The agent emits `textDocument/rename { position, newName }` and the LSP server returns a
  `WorkspaceEdit` containing every `TextEdit` needed — the client applies them atomically.
- The agent receives `PrepareRenameResponse` pre-flight so it can confirm a symbol is
  rename-able before committing the operation.

This closes **Phase J wave-2** entirely. After wave-2.3, all five high-ROI LSP features
(diagnostics, incremental sync, hover, completion, rename) are shipped.

## 2. §2.5 LLM-first audit

**Rename = compile-time-catch via type/scope analysis.**
The rename handler queries the same `TypeCheckCtx` + AST span machinery used by hover. If a
name is unknown in scope (not in `ctx.bindings()`), `prepare_rename` returns `None` — the
client refuses to offer the action. This is a **compile-time-catch**: the agent cannot rename
a phantom symbol and later discover the edit was a no-op.

**Rename shortcut is LLM training-data saturated.**
Every IDE training corpus (VSCode, Cursor, Neovim LSP, IntelliJ) contains F2 / "Rename Symbol"
workflows. The agent's prediction for "what to call next after deciding to rename" is `rename()`
— not a bespoke Cobrust API. Matching the standard LSP shape maximises first-try correctness.

**Single-document scope is safe for the current training corpus.**
LLM agents operating in single-file sessions (the dominant use case for Cobrust 0.x) need only
local rename. Cross-file workspace rename deferred to wave-3 does not penalise the 0.x agent
workflow.

## 3. Scope

### 3.1 `prepare_rename` (`PrepareRenameParams`)

At cursor position, return a `PrepareRenameResponse::Range(range)` covering the symbol's byte
span if the symbol is rename-able, or `None` if:

- Cursor is not on an identifier character (punctuation, whitespace, EOF).
- Identifier is a keyword (`let`, `def`, `if`, etc.) — keywords are not rename-able.
- Identifier is not present in `TypeCheckCtx::bindings()` (unknown / unbound symbol).

Implementation reuses `word_at_offset` from `hover.rs`. Keyword check uses the same
`KEYWORDS` static slice from `completion.rs`.

### 3.2 `rename` (`RenameParams`)

At cursor position with `new_name`:

1. Resolve the symbol name at position via `word_at_offset`.
2. Confirm the symbol is rename-able (same guards as §3.1).
3. Scan the entire source for all occurrences of the same identifier word (same `word_at_offset`
   heuristic applied at every byte position where the source byte equals the first char of
   `old_name`).
4. Build `WorkspaceEdit { changes: { uri: vec![TextEdit{ range, new_text: new_name }…] } }`.
5. Return `Some(WorkspaceEdit)`.

Scope: single-document only. Cross-file rename deferred to wave-3.

## 4. Non-goals

- **Cross-file / workspace rename** — deferred to ADR-0057e (wave-3). Only the open document's
  URI is included in `WorkspaceEdit.changes`.
- **Type-aware duplicate-name rejection** — if `new_name` collides with an existing binding,
  `rename` returns the edit anyway; the client's LSP infrastructure will surface a subsequent
  type error via `publishDiagnostics`. We do not pre-validate `new_name` uniqueness.
- **Non-ASCII identifiers** — out of scope; the same ASCII-only heuristic as hover/completion
  applies. Non-ASCII support deferred to wave-3.
- **Structural pattern rename** — renaming a binding that appears in a `match` pattern arm
  follows the same word-scan; no special AST traversal needed for single-file scope.

## 5. Acceptance gate

9 tests total (per §8 engineering standards: test-first; acceptance must be written before impl
is considered complete):

| # | Category | Description |
|---|---|---|
| 1 | prepare_rename integration | Local var at cursor → returns Range covering the word |
| 2 | prepare_rename integration | Keyword at cursor (`let`) → returns None |
| 3 | prepare_rename integration | Undefined symbol / space → returns None |
| 4 | rename integration | `let x = 42; x + 1` rename x → y → 2 TextEdits (def + use) |
| 5 | rename integration | Rename def-only symbol (single occurrence) → 1 TextEdit |
| 6 | rename integration | Rename with multi-occurrence symbol across multiple lines |
| 7 | snapshot | `prepare_rename` on known symbol |
| 8 | snapshot | `rename` result WorkspaceEdit (edits serialised) |
| 9 | snapshot | `prepare_rename` on keyword → None |

## 6. Implementation plan

Estimated ~300–500 LOC across:

- `crates/cobrust-lsp/src/rename.rs` (new) — `prepare_rename` + `rename` pure functions
- `crates/cobrust-lsp/src/lib.rs` — import rename module, add `rename_provider` capability,
  wire `prepare_rename` + `rename` handlers to `LanguageServer` impl
- `crates/cobrust-lsp/tests/rename_e2e.rs` (new) — 9 tests per §5

### 6.1 `rename.rs` public surface

```rust
/// Return the LSP Range covering the rename-able symbol at `byte_offset`,
/// or `None` if the cursor is not on a rename-able symbol.
pub fn prepare_rename(
    source: &str,
    line_map: &LineMap,
    position: Position,
    ctx: &TypeCheckCtx,
) -> Option<PrepareRenameResponse>;

/// Find all occurrences of `old_name` as a standalone word in `source`
/// and return a WorkspaceEdit replacing each with `new_name`.
/// Returns `None` if `old_name` is not rename-able.
pub fn rename_symbol(
    source: &str,
    line_map: &LineMap,
    position: Position,
    new_name: &str,
    ctx: &TypeCheckCtx,
    uri: Url,
) -> Option<WorkspaceEdit>;
```

### 6.2 Occurrence scan algorithm

Word-boundary scan in O(n) source length:

```
for i in 0..source.len():
    if source[i] matches first byte of old_name:
        if word_at_offset(source, i) == Some((i, i + old_name.len())):
            if source[i..i+old_name.len()] == old_name:
                record TextEdit for that range
```

This reuses the existing `word_at_offset` boundary checker — no new AST traversal needed
for single-file scope.

## 7. ADR-0057 frame relation

This ADR closes the Phase J wave-2 row. Wave-2 sub-ADRs:

| Sub-ADR | Feature | Status |
|---|---|---|
| 0057b | `textDocument/didChange` incremental + Session reuse | accepted |
| 0057c | `textDocument/hover` + `textDocument/completion` | accepted |
| 0057d | `textDocument/prepareRename` + `textDocument/rename` | **this ADR** |

Wave-3 (ADR-0057e) will cover: go-to-definition, cross-file rename, workspace symbol search.
