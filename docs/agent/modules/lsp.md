---
module_id: lsp
last_verified_commit: 9023f9d
milestone: J.wave2.3
dependencies:
  - crates/cobrust-frontend/src/lib.rs       # parse_str entrypoint
  - crates/cobrust-hir/src/lower.rs          # HIR Session + lower
  - crates/cobrust-types/src/check.rs        # check entrypoint + TypeCheckCtx + check_incremental
  - crates/cobrust-types/src/error.rs        # 25 TypeError variants
  - crates/cobrust-mir/src/error.rs          # 11 MirError variants
  - crates/cobrust-hir/src/error.rs          # 6 LoweringError variants
  - crates/cobrust-frontend/src/span.rs      # Span { file, start, end }
adr:
  - 0057   # Phase J frame
  - 0057a  # Wave-1 publishDiagnostics
  - 0057b  # Wave-2.1 didChange incremental + Session reuse
  - 0057c  # Wave-2.2 hover + completion
  - 0057d  # Wave-2.3 prepareRename + rename
  - 0052b  # suggestion: Option<&'static str> field
  - 0056b  # TypeCheckCtx Clone + Send + invalidate (Arc-COW contract)
---

# cobrust-lsp

## Purpose

Cobrust Language Server Protocol (LSP) implementation.

Wave-1 (ADR-0057a) ships `textDocument/publishDiagnostics`. Every
`TypeError + MirError + LoweringError + FrontendError` produced by the
Cobrust compile pipeline is mapped to an LSP `Diagnostic` and pushed
to the editor via `Client::publish_diagnostics`.

Wave-2.1 (ADR-0057b) adds per-keystroke live diagnostics via
`textDocument/didChange` incremental sync + 100ms debounce + shared
`TypeCheckCtx`.

Wave-2.2 (ADR-0057c) adds `textDocument/hover` (inferred type at cursor)
+ `textDocument/completion` (PRELUDE + scope + keywords).

Wave-2.3 (ADR-0057d) adds `textDocument/prepareRename` + `textDocument/rename`
(symbol rename with `WorkspaceEdit` response). Closes Phase J wave-2.

Wave-3 (ADR-0057e) adds `textDocument/definition` (go-to-def via
same-document word-scan fallback), `textDocument/codeAction` (ADR-0062
`FixSafety`-tier-gated Quick Fix emission), and extends `rename` to
walk every OPEN document in `Backend.docs` for cross-file `WorkspaceEdit`
aggregation. Closes Phase J wave-3 — v1.1 LSP server shipped.

### Wave-3 public surface (ADR-0057e)

| Symbol | Location | Shape |
|---|---|---|
| `goto_def::resolve_definition` | `crates/cobrust-lsp/src/goto_def.rs` | `(source: &str, line_map: &LineMap, position: Position, ctx: &TypeCheckCtx, uri: Url) -> Option<GotoDefinitionResponse>` |
| `code_action::build_code_actions` | `crates/cobrust-lsp/src/code_action.rs` | `(diagnostics: &[Diagnostic], uri: &Url) -> Vec<CodeActionOrCommand>` |
| `code_action::fix_safety_from_diagnostic_data` | `crates/cobrust-lsp/src/code_action.rs` | `(diag: &Diagnostic) -> Option<FixSafety>` |
| `rename::rename_symbol_cross_file` | `crates/cobrust-lsp/src/rename.rs` | `(primary_source: &str, primary_line_map: &LineMap, position: Position, new_name: &str, ctx: &TypeCheckCtx, primary_uri: Url, other_docs: &[(Url, String, LineMap)]) -> Option<WorkspaceEdit>` |
| `diagnostic::DIAG_DATA_FIX_SAFETY_KEY` | `crates/cobrust-lsp/src/diagnostic.rs` | `&'static str = "fix_safety"` |

### Wave-3 wire-shape (`Diagnostic.data`)

ADR-0057e §3.2 stamps the ADR-0062 `FixSafety` tier into
`Diagnostic.data` as a JSON object `{"fix_safety": <u8>}` (key is
`DIAG_DATA_FIX_SAFETY_KEY`). The codeAction handler reads this to
route per-tier behaviour without re-classifying the original error.

| Tier code (u8) | `FixSafety` | `CodeActionKind` | Edit payload |
|---|---|---|---|
| 0 | FormatOnly | SOURCE_FIX_ALL | — (title-only) |
| 1 | BehaviorPreserving | QUICKFIX | `WorkspaceEdit` (suggestion text) |
| 2 | LocalEdit | QUICKFIX | `WorkspaceEdit` (suggestion text) |
| 3 | ApiChanging | REFACTOR | — (title-only) |
| 4 | TargetChanging | — | no CodeAction emitted |
| 5 | RequiresHumanReview | — | no CodeAction emitted |
| other | (out-of-range) | RequiresHumanReview default | no CodeAction emitted |

### Wave-3 dispatch paths

**goto_definition (ADR-0057e §3.1):**

1. Read `(source, line_map)` from `docs.lock()` for the URI; return
   `Ok(None)` if absent.
2. Snapshot `TypeCheckCtx` via `session_ctx_snapshot` (Arc-clone, O(1)).
3. Call `goto_def::resolve_definition(...)`.
4. `resolve_definition`:
   a. `position → byte_offset` via `LineMap::position_to_byte`.
   b. `word_at_offset` resolves the cursor word.
   c. Guard: not in `KEYWORDS`, present in `ctx.lookup`.
   d. `first_word_occurrence(source, name)` scans for the first
      word-boundary occurrence (the def-site under wave-3 same-doc scope).
   e. Range conversion → `GotoDefinitionResponse::Scalar(Location)`.

**code_action (ADR-0057e §3.2):**

1. Read `params.context.diagnostics` + `params.text_document.uri`.
2. For each diagnostic:
   a. `fix_safety_from_diagnostic_data(diag)?` — read tier from `data`.
   b. `code_action_kind_for_fix_safety(tier)?` — skip if `None`.
   c. Read suggestion from `related_information[0].message` — skip if absent.
   d. For `BehaviorPreserving` / `LocalEdit`: emit CodeAction with
      `WorkspaceEdit { changes: {uri: [TextEdit{range, new_text: suggestion}]} }`.
   e. For `ApiChanging` / `FormatOnly`: emit CodeAction with no `edit`,
      `title = suggestion`.
   f. For `TargetChanging` / `RequiresHumanReview`: skip emission.
3. Return `Ok(Some(actions))` or `Ok(None)` if vec empty.

**rename (extended, ADR-0057e §3.3):**

1. Under a single `docs.lock()`, gather `(primary_source, primary_line_map)`
   for the cursor URI AND a `Vec<(Url, String, LineMap)>` of every OTHER
   open URI. Release lock before scan.
2. Call `rename::rename_symbol_cross_file(...)`.
3. Function reuses wave-2.3 `resolve_rename_symbol` guards on the
   primary doc, then runs `collect_occurrences` for every URI; URIs
   with zero occurrences are omitted from the `changes` map.
4. Returns `WorkspaceEdit { changes: HashMap<Url, Vec<TextEdit>> }`.

## Public surface

| Item | Anchor | Kind |
|---|---|---|
| `Backend` | `crates/cobrust-lsp/src/lib.rs::Backend` | struct (LanguageServer impl) |
| `Backend::new(Client) -> Self` | `crates/cobrust-lsp/src/lib.rs::Backend::new` | constructor (100ms debounce) |
| `Backend::with_debounce_ms(Client, u64) -> Self` | `crates/cobrust-lsp/src/lib.rs::Backend::with_debounce_ms` | constructor (tests pass `0`) |
| `Backend::session_ctx_snapshot() -> TypeCheckCtx` | `crates/cobrust-lsp/src/lib.rs::Backend::session_ctx_snapshot` | Arc-COW snapshot |
| `Backend::file_id_for(&Url) -> u32` | `crates/cobrust-lsp/src/lib.rs::Backend::file_id_for` | URI → FileId interning |
| `Backend::compile_diagnostics(&str, &LineMap) -> Vec<Diagnostic>` | `crates/cobrust-lsp/src/lib.rs::Backend::compile_diagnostics` | wave-1 stateless |
| `Backend::compile_diagnostics_with_session(&str, &LineMap, &mut TypeCheckCtx, u32) -> Vec<Diagnostic>` | `crates/cobrust-lsp/src/lib.rs::Backend::compile_diagnostics_with_session` | wave-2.1 stateful (invalidate + check_incremental) |
| `Backend::apply_content_changes(String, &[TextDocumentContentChangeEvent]) -> String` | `crates/cobrust-lsp/src/lib.rs::Backend::apply_content_changes` | range-splice + full-replace |
| `word_at_offset(&str, usize) -> Option<(usize, usize)>` | `crates/cobrust-lsp/src/hover.rs::word_at_offset` | byte-range of ident at cursor |
| `render_hover_markdown(&str, &str) -> String` | `crates/cobrust-lsp/src/hover.rs::render_hover_markdown` | `**name**: \`Type\`` card |
| `resolve_hover(&str, &LineMap, Position, &TypeCheckCtx) -> Option<Hover>` | `crates/cobrust-lsp/src/hover.rs::resolve_hover` | hover dispatcher |
| `prefix_at_offset(&str, usize) -> &str` | `crates/cobrust-lsp/src/completion.rs::prefix_at_offset` | ident prefix at cursor |
| `prelude_items(&str) -> Vec<CompletionItem>` | `crates/cobrust-lsp/src/completion.rs::prelude_items` | PRELUDE fn catalogue |
| `scope_items(&TypeCheckCtx, &str) -> Vec<CompletionItem>` | `crates/cobrust-lsp/src/completion.rs::scope_items` | in-scope binding items |
| `keyword_items(&str) -> Vec<CompletionItem>` | `crates/cobrust-lsp/src/completion.rs::keyword_items` | keyword items |
| `build_completion_response(&str, usize, &TypeCheckCtx) -> CompletionResponse` | `crates/cobrust-lsp/src/completion.rs::build_completion_response` | full completion dispatcher |
| `KEYWORDS: &[&str]` | `crates/cobrust-lsp/src/completion.rs::KEYWORDS` | keyword list (pub, used by rename guard) |
| `prepare_rename(&str, &LineMap, Position, &TypeCheckCtx) -> Option<PrepareRenameResponse>` | `crates/cobrust-lsp/src/rename.rs::prepare_rename` | cursor → rename-able Range or None |
| `rename_symbol(&str, &LineMap, Position, &str, &TypeCheckCtx, Url) -> Option<WorkspaceEdit>` | `crates/cobrust-lsp/src/rename.rs::rename_symbol` | all-occurrence WorkspaceEdit builder |
| `LineMap` | `crates/cobrust-lsp/src/span_convert.rs::LineMap` | byte-offset → UTF-16 position |
| `LineMap::from_source(&str) -> LineMap` | `crates/cobrust-lsp/src/span_convert.rs::LineMap::from_source` | constructor |
| `LineMap::byte_to_position(u32) -> Position` | `crates/cobrust-lsp/src/span_convert.rs::LineMap::byte_to_position` | byte → position |
| `LineMap::position_to_byte(Position) -> Option<u32>` | `crates/cobrust-lsp/src/span_convert.rs::LineMap::position_to_byte` | position → byte (wave-2.1) |
| `span_to_lsp_range(&Span, &LineMap) -> Range` | `crates/cobrust-lsp/src/span_convert.rs::span_to_lsp_range` | helper |
| `DebounceTokens` | `crates/cobrust-lsp/src/debounce.rs::DebounceTokens` | wave-2.1 per-URI version tracker |
| `DEFAULT_DEBOUNCE_MS = 100` | `crates/cobrust-lsp/src/debounce.rs::DEFAULT_DEBOUNCE_MS` | const (ADR-0057b §3.5) |
| `wait_for_token(DebounceToken)` | `crates/cobrust-lsp/src/debounce.rs::wait_for_token` | async sleep helper |
| `type_error_to_diagnostics(&TypeError, &LineMap) -> Vec<Diagnostic>` | `crates/cobrust-lsp/src/diagnostic.rs::type_error_to_diagnostics` | mapper (25 variants, flattens Multiple) |
| `mir_error_to_diagnostic(&MirError, &LineMap) -> Diagnostic` | `crates/cobrust-lsp/src/diagnostic.rs::mir_error_to_diagnostic` | mapper (11 variants) |
| `lowering_error_to_diagnostic(&LoweringError, &LineMap) -> Diagnostic` | `crates/cobrust-lsp/src/diagnostic.rs::lowering_error_to_diagnostic` | mapper (6 variants) |
| `frontend_error_to_diagnostic(&FrontendError, &LineMap) -> Diagnostic` | `crates/cobrust-lsp/src/diagnostic.rs::frontend_error_to_diagnostic` | mapper (Lex + Parse) |

## Wire format (per ADR-0057a §3)

`Diagnostic` shape on every variant:

```text
Diagnostic {
  range:    span_to_lsp_range(span, &line_map),
  severity: DiagnosticSeverity::ERROR,
  code:     Some(NumberOrString::String("<discriminant>")),
  source:   Some("cobrust"),
  message:  err.to_string(),                      // thiserror diagnosis
  related_information: suggestion.map(|s| vec![
    DiagnosticRelatedInformation {
      location: Location { uri: synthetic, range },
      message:  s,                                // ADR-0052b suggestion verbatim
    }
  ]),
  tags: None,
  data: None,
}
```

`code` discriminants used by editor-side code-action routing:

| Variant kind | `code` string |
|---|---|
| `TypeError::TypeMismatch` | `"type-mismatch"` |
| `TypeError::ImplicitTruthiness` | `"implicit-truthiness"` |
| `TypeError::OccursCheck` | `"occurs-check"` |
| `TypeError::UnknownName` | `"unknown-name"` |
| `TypeError::ArityMismatch` | `"arity-mismatch"` |
| `MirError::UseAfterMove` | `"use-after-move"` |
| `MirError::Internal` | `"internal-mir"` (no span; `Range::default()`) |
| `LoweringError::UnknownName` | `"lower-unknown-name"` |
| ... | (one per variant; see `src/diagnostic.rs`) |

## Pipeline dispatch (per ADR-0057a §4 + ADR-0057b §3)

### `did_open` path (wave-1 + wave-2.1)

```text
1. Allocate file_id = backend.file_id_for(uri)
2. compile_diagnostics_with_session(source, line_map, &mut session_ctx, file_id):
   2a. session_ctx.invalidate(file_id)         // drop stale rows (no-op on first open)
   2b. parse_str(source, FileId(file_id))      // wave-2.1 uses URI's FileId
   2c. cobrust_hir::lower::lower(&ast, ...)
   2d. check_incremental(&mut session_ctx, &hir, file_id)
3. docs[uri] = DocState::new(source, version)
4. client.publish_diagnostics(uri, diagnostics, Some(version))
```

### `did_change` path (wave-2.1, ADR-0057b §3)

```text
1. Apply content changes (in-place, holding docs mutex):
   1a. prev_source = docs[uri].source.clone()  (else empty)
   1b. new_source = apply_content_changes(prev_source, &params.content_changes)
   1c. docs[uri] = DocState::new(new_source, version)
2. Schedule debounce token: debounce_tokens.schedule(uri, version)
3. tokio::spawn:
   3a. wait_for_token(token)                   // sleep DEFAULT_DEBOUNCE_MS
   3b. if !debounce_tokens.is_latest(uri, version): return    // overtaken
   3c. compile_diagnostics_with_session(new_state.source, &new_state.line_map,
                                       &mut session_ctx, file_id)
   3d. client.publish_diagnostics(uri, diagnostics, Some(version))
```

### `did_close` path (wave-2.1)

```text
1. docs.remove(uri)
2. session_ctx.invalidate(file_id_for(uri))    // drop type-cache rows
   (uri_file_ids / debounce_tokens entries are intentionally retained)
```

### apply_content_changes contract

For each event in `content_changes`:

- If `event.range` is `Some(Range)`: rebuild `LineMap` over current
  source, map `range.start` and `range.end` to byte offsets via
  `position_to_byte`, splice `event.text` in via `String::replace_range`.
- If `event.range` is `None`: replace entire source with `event.text`.

Events apply in array order. `LineMap` rebuilt after each event because
subsequent ranges are relative to the post-edit source.

## Span → LSP Range conversion (per ADR-0057a §6)

`LineMap` is built once per `did_open` and rebuilt on every
`did_change` (because FULL sync replaces the source). `LineMap`
records the byte offset of every `\n`; `byte_to_position` does a
`binary_search` on `line_starts`, then counts UTF-16 code units
between the line start and the target byte offset.

UTF-16 column semantics per LSP §"Position Encoding Kinds" default
(`utf-16`). ASCII source behaves identically under UTF-8 / UTF-16 /
codepoint counting; the distinction matters only for non-BMP
codepoints (e.g. 🦀 → 2 UTF-16 surrogates).

## Done means

- `cargo check -p cobrust-lsp` exits 0 on Mac single-crate scope.
- `cargo clippy -p cobrust-lsp --all-targets -- -D warnings` clean.
- `cargo test -p cobrust-lsp` PASS for 88 tests:
  - 52 unit (code_action + debounce + diagnostic + span_convert +
    hover + completion + rename unit tests).
  - 5 integration in `tests/did_change_e2e.rs` (ADR-0057b §5 gate).
  - 12 integration + snapshot in `tests/hover_completion_e2e.rs`
    (ADR-0057c §5 gate: 3 hover + 3 completion + 3 hover snap + 3 completion snap).
  - 9 integration + snapshot in `tests/rename_e2e.rs`
    (ADR-0057d §5 gate: 3 prepare_rename + 3 rename + 3 snapshot).
  - 10 snapshot in `tests/snapshot_diagnostics.rs` (wave-1 + wave-2.1).
- ADR-0057d status flips `proposed → accepted` on impl merge.

## Hover dispatch path (wave-2.2, ADR-0057c §3.1)

```text
hover(HoverParams):
  1. docs[uri].source + line_map snapshot (mutex held briefly)
  2. session_ctx_snapshot()           // O(1) Arc clone
  3. line_map.position_to_byte(pos)   // LSP Position → byte offset
  4. word_at_offset(source, offset)   // (start, end) or None
  5. source[start..end] → name
  6. ctx.lookup(name)                 // TypeCheckCtx name → Ty
  7. render_hover_markdown(name, ty)  // "**x**: `Int`\n\nInferred type."
  8. Return Hover { MarkupContent::Markdown, range: word_range }
```

Unknown name → `Ok(None)`.

## Completion dispatch path (wave-2.2, ADR-0057c §3.2)

```text
completion(CompletionParams):
  1. docs[uri].source + line_map snapshot
  2. line_map.position_to_byte(pos) → byte_offset
  3. prefix_at_offset(source, offset) → &str prefix
  4. build_completion_response(source, offset, ctx):
     4a. prelude_items(prefix)       // 23 hardcoded PRELUDE fns, sortText "0_*"
     4b. scope_items(ctx, prefix)    // TypeCheckCtx::bindings(), sortText "1_*"
     4c. keyword_items(prefix)       // 35 keywords, sortText "2_*"
  5. Return CompletionResponse::Array(items)
```

Prefix filtering is case-sensitive. Empty prefix returns all items.

## prepareRename dispatch path (wave-2.3, ADR-0057d §3.1)

```text
prepare_rename(TextDocumentPositionParams):
  1. docs[uri].source + line_map snapshot
  2. session_ctx_snapshot()
  3. resolve_rename_symbol(source, line_map, position, ctx):
     3a. line_map.position_to_byte(pos) → byte_offset
     3b. word_at_offset(source, byte_offset) → (start, end) or None
     3c. source[start..end] → name
     3d. KEYWORDS.contains(name) → None if keyword
     3e. ctx.lookup(name) → None if unknown binding
  4. line_map.byte_to_position(start/end) → LSP Range
  5. Return PrepareRenameResponse::Range(range)
```

Unknown name / keyword / whitespace → `Ok(None)`.

## rename dispatch path (wave-2.3, ADR-0057d §3.2)

```text
rename(RenameParams { position, new_name }):
  1. docs[uri].source + line_map snapshot
  2. session_ctx_snapshot()
  3. resolve_rename_symbol(source, line_map, position, ctx) → old_name
  4. collect_occurrences(source, old_name, new_name, line_map):
     - O(n) scan: for i where source[i] == old_name[0]:
         word_at_offset(source, i) == Some((i, i+len)) AND slice == old_name
         → TextEdit { range, new_text: new_name }
  5. WorkspaceEdit { changes: { uri: edits } }
  6. Return Some(WorkspaceEdit)
```

Single-document scope. `changes` always has exactly one URI key.

## Non-goals (wave-2.1 + wave-2.2 + wave-2.3)

- No incremental parse — full re-parse on each debounced batch.
  AST-cache + incremental parse is wave-2.3 scope.
- No per-DefId incremental type-check — full re-check via
  `TypeCheckCtx::invalidate + merge_module`. True incremental
  check is an ADR-0056c follow-up.
- No definition / codeAction — separate sub-ADRs (wave-3).
- No CodeAction emission on `did_change` push — code actions surface
  on `textDocument/codeAction` request only.
- No multi-file invalidation propagation — wave-2.1 invalidates
  only the URI whose source changed. Cross-file dependency
  invalidation is future scope.
- No full HIR span-indexed hover — wave-2.2 uses word-boundary
  heuristic. DefId-span hover is wave-3 scope (ADR-0057c §4).
- No PRELUDE introspection from `TypeCheckCtx` — completion uses a
  hardcoded 23-item catalogue. Live query is wave-3 per
  `TODO(#hover-prelude-sync)` in `src/completion.rs`.
- No cross-file / workspace rename — ADR-0057d §4 non-goal. Single-
  document only. Cross-file deferred to ADR-0057e (wave-3).
- No `new_name` uniqueness validation — duplicate-name check deferred;
  the subsequent `publishDiagnostics` cycle surfaces the collision.
- No non-ASCII identifier rename — ASCII heuristic only (same as hover).

## CodeAction gating by FixSafety tier (ADR-0062)

The `code_action` module (anchor: `crates/cobrust-lsp/src/code_action.rs`)
maps a diagnostic's `FixSafety` tier to the LSP `CodeActionKind` the
client should expose. Defined per ADR-0062 §3.5 gating matrix:

| Tier | CodeActionKind | Auto-apply behaviour |
|---|---|---|
| `FormatOnly` | `SOURCE_FIX_ALL` | Applied on save / format pass |
| `BehaviorPreserving` | `QUICKFIX` | Apply on user accept |
| `LocalEdit` | `QUICKFIX` | Apply on user accept |
| `ApiChanging` | `REFACTOR` | Suggest only, no quick apply |
| `TargetChanging` | `None` | Diagnostic-only, no code action |
| `RequiresHumanReview` | `None` | Diagnostic-only, no code action |

### Convenience helpers

- `code_action_kind_for_type_error(&TypeError) -> Option<CodeActionKind>`
- `code_action_kind_for_mir_error(&MirError) -> Option<CodeActionKind>`
- `code_action_kind_for_lowering_error(&LoweringError) -> Option<CodeActionKind>`
- `code_action_kind_for_fix_safety(FixSafety) -> Option<CodeActionKind>` (raw)
- `fix_safety_from_code(u8) -> FixSafety` — widens the opaque `u8` tier
  code emitted by `cobrust-mir` / `cobrust-hir` (which don't depend on
  `cobrust-types`) into the public `FixSafety` enum at this LSP-adapter
  boundary.

### Wire-form contract

The kebab-case Display impl on `FixSafety` is the JSON wire form per
ADR-0062 §1.2 Zero-language precedent. When `--emit-json` diagnostic
output ships, the field name is `"fix_safety": "behavior-preserving"`.

## See also

- ADR-0057 — Phase J frame.
- ADR-0057a — wave-1 publishDiagnostics spec.
- ADR-0057b — wave-2.1 didChange + Session reuse.
- ADR-0057c — wave-2.2 hover + completion (this milestone).
- ADR-0052b — `suggestion` field shape (`Option<&'static str>`).
- ADR-0056b — Phase I × J handoff (`TypeCheckCtx` Clone + Send Arc-COW).
- ADR-0062 — FixSafety ladder (CodeAction gating + JSON wire field).
- `docs/human/{zh,en}/editor-setup.md` — user-facing setup guide.
- `docs/human/{zh,en}/error-reference.md` — six-tier fix-safety table.
