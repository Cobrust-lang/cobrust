---
module_id: lsp
last_verified_commit: feature/0057b-didchange
milestone: J.wave2.1
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
  - 0052b  # suggestion: Option<&'static str> field
  - 0056b  # TypeCheckCtx Clone + Send + invalidate (Arc-COW contract)
---

# cobrust-lsp

## Purpose

Cobrust Language Server Protocol (LSP) implementation.

Wave-1 (ADR-0057a) ships `textDocument/publishDiagnostics`. Every
`TypeError + MirError + LoweringError + FrontendError` produced by the
Cobrust compile pipeline is mapped to an LSP `Diagnostic` and pushed
to the editor via `Client::publish_diagnostics`. Wave-2+ (ADR-0057b/c/d)
extends to hover, completion, definition, rename, codeAction.

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
- `cargo test -p cobrust-lsp` PASS for 47 tests:
  - 32 unit (code_action + debounce + diagnostic + span_convert).
  - 5 integration in `tests/did_change_e2e.rs` covering ADR-0057b §5
    gate (incremental refresh, full-replace, debounce coalesce,
    invalidate session, concurrent serialisation).
  - 10 snapshot in `tests/snapshot_diagnostics.rs` (5 wave-1 +
    5 wave-2.1 after-edit JSON shapes).
- ADR-0057b status flips `proposed → accepted` on impl merge.

## Non-goals (wave-2.1)

- No incremental parse — full re-parse on each debounced batch.
  AST-cache + incremental parse is wave-2.2 scope.
- No per-DefId incremental type-check — full re-check via
  `TypeCheckCtx::invalidate + merge_module`. True incremental
  check is an ADR-0056c follow-up.
- No hover / completion / definition / rename — separate sub-ADRs.
- No CodeAction emission on `did_change` push — code actions surface
  on `textDocument/codeAction` request only.
- No multi-file invalidation propagation — wave-2.1 invalidates
  only the URI whose source changed. Cross-file dependency
  invalidation is future scope.

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
- ADR-0057b — wave-2.1 didChange + Session reuse (this milestone).
- ADR-0052b — `suggestion` field shape (`Option<&'static str>`).
- ADR-0056b — Phase I × J handoff (`TypeCheckCtx` Clone + Send Arc-COW).
- ADR-0062 — FixSafety ladder (CodeAction gating + JSON wire field).
- `docs/human/{zh,en}/editor-setup.md` — user-facing setup guide.
- `docs/human/{zh,en}/error-reference.md` — six-tier fix-safety table.
