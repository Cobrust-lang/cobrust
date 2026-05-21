---
doc_kind: adr
adr_id: 0057c
parent_adr: 0057
title: "Phase J wave-2.2 — LSP hover + completion (textDocument/hover + textDocument/completion)"
status: accepted
date: 2026-05-21
last_verified_commit: 1149325
ratified_at: 1149325
ratified_on: 2026-05-21
phase: "Phase J wave-2.2"
supersedes: []
superseded_by: []
relates_to: [adr:0057, adr:0057a, adr:0057b, adr:0056b, adr:0052b, adr:0062]
discovered_by: ADR-0057 §4 sub-ADR roster (PRIORITY 2 + 3 after diagnostics/didChange)
ratification_path: P9 sub-ADR review under ADR-0057 frame; ratifies on impl merge
---

# ADR-0057c: Phase J wave-2.2 — LSP hover + completion

## 1. Motivation

Wave-2.1 (ADR-0057b) delivered per-keystroke diagnostic refresh — the agent-LLM's
fix-path latency is now per-keystroke. Wave-2.2 ships the next two highest-§2.5-ROI
features from ADR-0057 §4 PRIORITY ranking:

- **PRIORITY 2 `textDocument/hover`** — "I'm seeing `x: List[Int]` at this position."
  Surfaces the inferred type of the binding under the cursor as a Markdown hover card.
  The agent-LLM in Cursor/VSCode consumes this as its inline type-oracle, reducing the
  number of inference-loop turns to zero for simple type queries.

- **PRIORITY 3 `textDocument/completion`** — Surfaces PRELUDE functions, in-scope
  `let`-bindings, and keywords as LSP `CompletionItem[]`. The agent-LLM already has
  training-data priors on completion shape (Python + Rust IDEs both emit completion
  lists); matching the standard shape maximises first-try correctness on `.` or
  identifier-prefix triggers.

Without wave-2.2, the Cobrust LSP is a "diagnostics-only" server. With it, a Cursor
session can type-check, get hover types, and request completions in a single session
— the full trifecta an IDE-integrated LLM needs for context-free editing.

## 2. §2.5 LLM-first audit

**Hover = inline LLM-readable type info.**
The hover card renders `**x**: \`Int\`` as Markdown. The in-editor agent sees this in
its context window verbatim — no prose-stripping, no type inference round-trip. Every
hover hit shortens the agent's debugging loop by at least one generation.

**Compile-time-catch signal via hover.**
If a user sees `**x**: \`?T3\`` (an unresolved inference variable), the agent knows
there is ambiguity *before* a compile error fires. This is a pre-diagnostic signal the
agent can act on immediately.

**Completion = match training-data distribution.**
Python coders expect `print`, `len`, `range`, `list`, `dict` in completion lists.
Rust coders expect `let`, `fn`, `if`, `match`, `for`. Cobrust completion surfaces both
families. The LLM's training-data prior fires correctly for every item emitted.

**Completion triggers §2.5 §D method-call sugar priority.**
Method-form completion (e.g. `s.split`) is ranked before function-form. ADR-0057 §4 /
ADR-0052d method-form wins; the completion list is the LLM's first hint that method
syntax is preferred.

## 3. Scope

### 3.1 `textDocument/hover` handler

```rust
async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>>
```

Pipeline per request:

1. Extract URI + `Position` from `params.text_document_position_params`.
2. Read `DocState.source` + `DocState.line_map` from `self.docs` (no lock held beyond
   the read).
3. Convert LSP `Position` (0-based line + character) → byte offset via
   `LineMap::position_to_byte`.
4. Scan the AST at the byte offset for the innermost name/binding. Implementation
   strategy for wave-2.2: use `TypeCheckCtx::bindings()` to iterate all name→Ty pairs
   from the last check; find the lexically nearest name to the cursor offset by
   scanning `source[..offset]` backwards for a word boundary. This is a **heuristic
   hover** — sufficient for the LSP test suite and common Cursor/VSCode use-cases.
   Full DefId-resolved hover (requiring span-indexed HIR) is a follow-up
   (ADR-0057e wave-3).
5. Render Markdown:
   ```
   **name**: `TypeDisplay`

   Inferred type.
   ```
   If the name carries a doc-comment (deferred — no doc-comment map yet), append it.
6. Return `Hover { contents: HoverContents::Markup(MarkupContent { kind: Markdown,
   value: "..." }), range: Some(word_range) }`.
   Unknown name → return `Ok(None)`.

### 3.2 `textDocument/completion` handler

```rust
async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>>
```

Pipeline per request:

1. Extract URI + `Position`.
2. Read `DocState.source` from `self.docs`.
3. Extract the identifier prefix at the cursor position (scan backwards from the byte
   offset for `[a-zA-Z_][a-zA-Z0-9_]*`).
4. Build the candidate set:
   - **PRELUDE functions** (hardcoded list from `build.rs` PRELUDE — see §3.3): kind =
     `Function`, detail = function signature string, sort prefix `0_`.
   - **In-scope bindings** from `TypeCheckCtx::bindings()`: kind = `Variable`, detail
     = `Ty::to_string()`, sort prefix `1_`.
   - **Keywords**: `def`, `let`, `mut`, `if`, `else`, `for`, `while`, `break`,
     `continue`, `return`, `class`, `enum`, `match`, `with`, `and`, `or`, `not`,
     `True`, `False`, `None`; kind = `Keyword`, sort prefix `2_`.
5. Filter by prefix (case-sensitive prefix match; empty prefix returns all items).
6. Return `CompletionResponse::Array(items)`.

### 3.3 PRELUDE function catalogue (completion candidates)

Sourced from ADR-0034 / `build.rs` PRELUDE definitions; hardcoded in
`crates/cobrust-lsp/src/completion.rs` for wave-2.2 (dynamic PRELUDE introspection via
`TypeCheckCtx` is wave-3 scope):

| Name | Signature | Notes |
|---|---|---|
| `print` | `(s: Any) -> None` | ADR-0064 polymorphic |
| `len` | `(x: List[T] \| Str \| Bytes) -> Int` | |
| `range` | `(start: Int, stop: Int) -> List[Int]` | |
| `input` | `(prompt: Str = "") -> Str` | |
| `int` | `(x: Any) -> Int` | |
| `float` | `(x: Any) -> Float` | |
| `str` | `(x: Any) -> Str` | |
| `bool` | `(x: Any) -> Bool` | |
| `list` | `(x: Any) -> List[Any]` | |
| `dict` | `() -> Dict[Any, Any]` | |
| `set` | `(x: Any) -> Set[Any]` | |
| `abs` | `(x: Int \| Float) -> Int \| Float` | |
| `max` | `(a: T, b: T) -> T` | |
| `min` | `(a: T, b: T) -> T` | |
| `sum` | `(xs: List[Int \| Float]) -> Int \| Float` | |
| `sorted` | `(xs: List[T]) -> List[T]` | |
| `reversed` | `(xs: List[T]) -> List[T]` | |
| `enumerate` | `(xs: List[T]) -> List[(Int, T)]` | |
| `zip` | `(a: List[A], b: List[B]) -> List[(A, B)]` | |
| `map` | `(f: (T) -> U, xs: List[T]) -> List[U]` | |
| `filter` | `(f: (T) -> Bool, xs: List[T]) -> List[T]` | |
| `open` | `(path: Str, mode: Str) -> FileHandle` | |
| `argv` | `() -> List[Str]` | |

## 4. Non-goals

- **NO signature help** (`textDocument/signatureHelp`): separate ADR if needed.
- **NO definition jump** (`textDocument/definition`): deferred per ADR-0057 §4
  PRIORITY 4 — own sub-ADR (0057d).
- **NO semantic tokens** (`textDocument/semanticTokens`): deferred.
- **NO rename** (`textDocument/rename`): PRIORITY 6, own sub-ADR.
- **NO full HIR span-indexed hover**: wave-2.2 uses a word-boundary heuristic at the
  cursor. True DefId-indexed hover requiring a span→DefId map is wave-3 scope.
- **NO incremental completion** (per-keystroke re-ranking): completion fires on request
  only, not on `didChange`.

## 5. Acceptance gate

12 tests across 4 categories (3 per category):

### 5.1 Hover integration (3)

1. `hover_known_binding_returns_type` — source `let x: Int = 42`, cursor on `x` →
   hover contents contain `**x**: \`Int\``.
2. `hover_function_binding_returns_fn_type` — `def f(a: Int) -> Str: ...`, cursor on
   `f` → hover contents contain `**f**: \``.
3. `hover_unknown_name_returns_none` — cursor on an unresolved identifier → `Ok(None)`.

### 5.2 Completion integration (3)

1. `completion_empty_prefix_includes_prelude` — at file start, empty prefix →
   items include `print`, `len`, `range`.
2. `completion_prefix_filters_items` — prefix `"pri"` → only `print` matches.
3. `completion_includes_keywords` — no prefix → items include `let`, `def`, `if`.

### 5.3 Hover snapshot (3)

1. `snapshot_hover_int_binding` — `let x = 42` at `x`.
2. `snapshot_hover_str_binding` — `let s = "hi"` at `s`.
3. `snapshot_hover_none_on_unknown` — cursor at unknown token → serialised `null`.

### 5.4 Completion snapshot (3)

1. `snapshot_completion_prelude_items` — empty source, no prefix → snapshot the array.
2. `snapshot_completion_keyword_items` — empty prefix → keywords present in snapshot.
3. `snapshot_completion_prefix_print` — prefix `"pr"` → only `print` returned.

## 6. ServerCapabilities advertisement

`initialize` response must extend `ServerCapabilities` with:

```rust
hover_provider: Some(HoverProviderCapability::Simple(true)),
completion_provider: Some(CompletionOptions {
    trigger_characters: Some(vec![".".to_string(), "_".to_string()]),
    ..Default::default()
}),
```

This tells Cursor/VSCode to route hover requests to this server and to send
`completion` on `.` and `_` triggers.

## 7. Implementation plan (~400-600 LOC)

| Phase | Surface | LOC |
|---|---|---|
| 1. `src/hover.rs` | Word-boundary scanner + Markdown renderer + hover handler | ~120 |
| 2. `src/completion.rs` | PRELUDE catalogue + in-scope builder + keyword list + filter | ~160 |
| 3. `lib.rs` capabilities extension | `hover_provider` + `completion_provider` in `initialize` | ~15 |
| 4. `lib.rs` LanguageServer trait impl | `hover` + `completion` delegating to modules | ~20 |
| 5. `tests/hover_completion_e2e.rs` | 6 integration tests | ~200 |
| 6. Snapshot extension in `tests/` | 6 snapshot tests | ~80 |
| **Total** | | **~595 LOC** |

## 8. Consequences

### 8.1 Positive

- Live type-at-cursor for every in-scope binding: §2.5 §B "training-data overlap"
  realised at the IDE hover surface.
- PRELUDE + keyword completion: agents expecting Python/Rust completion priors get a
  matching list on first use.
- ADR-0057 §4 PRIORITY 2 + 3 delivered; remaining open features are PRIORITY 4-6
  (definition/codeAction/rename).
- ADR-0052b `suggestion` field already in diagnostics; hover can forward it as a
  "fix hint" on unknown-name hover (wave-2.2 scope).

### 8.2 Negative

- Word-boundary hover is a heuristic; it will return `Ok(None)` on punctuation tokens
  or expressions without a single-word name. Full DefId-indexed hover requires a
  span→name map not yet built; deferred to wave-3.
- PRELUDE catalogue hardcoded: will drift from `build.rs` if new intrinsics are added.
  A `TODO(#hover-prelude-sync)` comment in `completion.rs` documents the live-query
  path for wave-3.

### 8.3 Neutral

- No new crate deps; all required types (`lsp_types::Hover`, `CompletionItem`,
  `MarkupContent`, etc.) already ship with `lsp-types = "0.95"` in the existing
  `Cargo.toml`.
- No change to `did_change` debounce or Session invalidation paths.

## 9. Ratification

This ADR ratifies on `1149325` impl merge. Per ADR-0057 §13,
sub-ADR ratification rolls up to parent Phase J status.

— P9 Tech Lead, 2026-05-21

## 10. Ratification addendum (2026-05-21)

Implementation merged on branch `1149325`. Deviations from
the design above (none load-bearing; documented for L2 audit traceability):

- **Word-boundary heuristic scope**: design §3.1 step 4 mentioned scanning
  `source[..offset]` backwards. Impl uses a forward+backward scan from
  `byte_offset` — same semantics, cleaner implementation. Cursor on a non-ident
  byte (space, punctuation) returns `None` for hover (§3.1 contract) and `""`
  for completion prefix (§3.2 contract). Cursor-at-`offset-1` (ident char)
  forms the anchor for completion only (triggers on `.` or `_`), NOT for hover
  (hover requires cursor on the token, not after it).

- **`fn` keyword**: the integration test for function hover uses `fn f(a: i64)` —
  Cobrust's function keyword is `fn` (not `def` as listed in ADR §1 examples).
  `def` appears in the keyword completion list only.

- **Snapshot count**: 6 new snapshot files created (3 hover + 3 completion).
  Snapshot shapes are stable across test runs.

Acceptance gate (§5) status as of merge:

- [x] 3 hover integration tests in `tests/hover_completion_e2e.rs` PASS.
- [x] 3 completion integration tests PASS.
- [x] 3 hover snapshot tests PASS.
- [x] 3 completion snapshot tests PASS.
- [x] `cargo clippy -p cobrust-lsp --all-targets -- -D warnings` clean.
- [x] ADR-0057 §4 PRIORITY 2 (hover) + PRIORITY 3 (completion) delivered.

Test verification:

- Mac single-crate: `cargo test -p cobrust-lsp` PASS (48 lib + 5 did_change
  integration + 12 hover_completion + 10 snapshot_diagnostics = 75 tests).

— P9 Tech Lead, 2026-05-21 (ratification 2026-05-21)
