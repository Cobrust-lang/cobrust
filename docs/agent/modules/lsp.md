---
module_id: lsp
last_verified_commit: feature/0057a-dev
milestone: J.wave1
dependencies:
  - crates/cobrust-frontend/src/lib.rs       # parse_str entrypoint
  - crates/cobrust-hir/src/lower.rs          # HIR Session + lower
  - crates/cobrust-types/src/check.rs        # check entrypoint + TypeCheckCtx
  - crates/cobrust-types/src/error.rs        # 25 TypeError variants
  - crates/cobrust-mir/src/error.rs          # 11 MirError variants
  - crates/cobrust-hir/src/error.rs          # 6 LoweringError variants
  - crates/cobrust-frontend/src/span.rs      # Span { file, start, end }
adr:
  - 0057   # Phase J frame
  - 0057a  # Wave-1 publishDiagnostics
  - 0052b  # suggestion: Option<&'static str> field
  - 0056b  # TypeCheckCtx Clone + Send + invalidate
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
| `Backend::new(Client) -> Self` | `crates/cobrust-lsp/src/lib.rs::Backend::new` | constructor |
| `Backend::compile_diagnostics(&str, &LineMap) -> Vec<Diagnostic>` | `crates/cobrust-lsp/src/lib.rs::Backend::compile_diagnostics` | static method |
| `LineMap` | `crates/cobrust-lsp/src/span_convert.rs::LineMap` | byte-offset → UTF-16 position |
| `LineMap::from_source(&str) -> LineMap` | `crates/cobrust-lsp/src/span_convert.rs::LineMap::from_source` | constructor |
| `LineMap::byte_to_position(u32) -> Position` | `crates/cobrust-lsp/src/span_convert.rs::LineMap::byte_to_position` | accessor |
| `span_to_lsp_range(&Span, &LineMap) -> Range` | `crates/cobrust-lsp/src/span_convert.rs::span_to_lsp_range` | helper |
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

## Pipeline dispatch (per ADR-0057a §4)

`did_open` and `did_change` paths:

```text
1. parse_str(source, FileId::SYNTHETIC) -> Result<AstModule, FrontendError>
   - on Err(FrontendError) -> emit one Diagnostic, return
2. cobrust_hir::lower::lower(&ast, &mut Session::new()) -> Result<Module, LoweringError>
   - on Err(LoweringError) -> emit one Diagnostic, return
3. cobrust_types::check(&hir) -> Result<TypedModule, TypeError>
   - on Err(TypeError) -> flatten (handles ::Multiple) and emit Vec<Diagnostic>
4. client.publish_diagnostics(uri, diagnostics, Some(version))
```

Wave-1 publishes the full `Diagnostic` vector per URI on every
`did_change` (FULL text sync). Delta-diff publishing is deferred per
ADR-0057a §4 + §9 Risk 3.

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
- `cargo test -p cobrust-lsp` PASS for ≥ 16 tests (11 unit + 5 snapshot).
- 5 snapshot tests in `tests/snapshot_diagnostics.rs` cover canonical
  variants: `TypeMismatch`, `OccursCheck`, `UnknownName`,
  `ImplicitTruthiness`, `ArityMismatch`.
- ADR-0057a status flips `proposed → accepted` with
  `last_verified_commit` and `ratified_on: 2026-05-18`.

## Non-goals (wave-1)

- No hover (ADR-0057b).
- No completion (ADR-0057b).
- No definition / rename (ADR-0057c).
- No codeAction / quickfix (ADR-0057d).
- No incremental text sync — FULL sync only.
- No multi-file invalidation — `LspFileCtx` per-URI scope only.
- No `TypeCheckCtx` reuse — every `did_change` re-runs the pipeline
  end-to-end. Phase I × J handoff (ADR-0056b §3.3 + §6) is wired in
  ADR-0057a wave-2.

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
- ADR-0057a — this wave-1 spec.
- ADR-0052b — `suggestion` field shape (`Option<&'static str>`).
- ADR-0056b — Phase I × J handoff (`TypeCheckCtx` Clone + Send).
- ADR-0062 — FixSafety ladder (CodeAction gating + JSON wire field).
- `docs/human/{zh,en}/editor-setup.md` — user-facing setup guide.
- `docs/human/{zh,en}/error-reference.md` — six-tier fix-safety table.
