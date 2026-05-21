---
doc_kind: adr
adr_id: 0057f
parent_adr: 0057
title: "Phase J wave-4 — inlay hints + semantic tokens + call hierarchy (v1.2 LSP polish)"
status: proposed
date: 2026-05-21
last_verified_commit: 657019e
ratified_at: pending-merge
ratified_on: 2026-05-21
phase: "Phase J wave-4"
supersedes: []
superseded_by: []
relates_to: [adr:0057, adr:0057a, adr:0057b, adr:0057c, adr:0057d, adr:0057e, adr:0056b, adr:0062]
discovered_by: ADR-0057 §8 sub-ADR roster (wave-4 row), user dispatch 2026-05-21
ratification_path: P9 sub-ADR review under ADR-0057 frame; ratifies on impl merge; closes Phase J wave-4 (v1.2 LSP server polish)
---

# ADR-0057f: Phase J wave-4 — inlay hints + semantic tokens + call hierarchy

## 1. Motivation

Phase J waves 1-3 (ADR-0057a/b/c/d/e) shipped the v1.1 LSP server with
eight handlers across publishDiagnostics, incremental didChange, hover,
completion, prepareRename + cross-file rename, goto-definition, and
codeAction (FixSafety-gated). The acceptance bar set by ADR-0057 §8
identified three further editor-expected features that polish v1.1 →
**v1.2**:

1. **`textDocument/inlayHint`** — inline type annotations on `let x =
   expr` and parameter-name hints at non-literal call-arg sites. Every
   modern Rust + Python LSP server (rust-analyzer, pyright,
   basedpyright) ships this. For the agent-LLM, inlay hints surface
   the inferred type that compile-time-catch otherwise discovers only
   at error time — the §2.5 compile-time-catch path made *visible* at
   the cursor, not behind an error message.
2. **`textDocument/semanticTokens/full`** — accurate identifier vs
   keyword vs string-content vs number-literal coloring. The agent-LLM
   reads source as text; consistent coloring + the LSP-published
   `tokenType` + `tokenModifier` legend make the structural reading
   path crisp where regex-based syntax highlight smears across
   structural boundaries.
3. **`textDocument/prepareCallHierarchy`** + `callHierarchy/incomingCalls`
   + `callHierarchy/outgoingCalls` — fn-level call-graph traversal at
   the cursor. The agent-LLM refactor workflow needs the impact radius
   of a change ex ante; today the agent must grep call sites by hand.

After wave-4, the v1.2 LSP server ships eleven LSP handlers. ADR-0054
§9 v0.5.0 bind already cleared at wave-3; wave-4 polishes the v0.5.0
LSP surface to the editor-feature-parity bar set against rust-analyzer
+ pyright minimum.

## 2. §2.5 LLM-first audit

Per CLAUDE.md §2.5 + ADR-0051, each of the three wave-4 features
passes the training-data-overlap and compile-time-catch tests:

- **inlay hints (compile-time-catch surfaced as UX)**. `let x = 42`
  parses + lowers + types in <100µs per ADR-0056b incremental ctx.
  The inferred type `Int` is already in `TypeCheckCtx::lookup("x")`.
  Wave-4 emits it as an inline `: Int` decoration at the byte after
  the binder. The agent-LLM looking at source sees the type ex ante;
  no error needed. This is the §2.5 §B compile-time-catch path's
  positive direction: type information surfaced before a misuse, not
  after.

- **semantic tokens (training-data overlap)**. Every rust-analyzer +
  pyright training corpus contains `textDocument/semanticTokens/full`
  round-trips. The wire shape is identical; the legend matches the
  established Token / Identifier / String / Number / Comment /
  Operator / Type / Namespace nine-type axis common to both. Matching
  the established legend maximises first-try correctness for the
  agent-LLM operating in any modern editor.

- **call hierarchy (refactor impact radius)**. Agent-LLM refactor
  workflows (`rename fn`, `change signature`) need to enumerate all
  call sites before applying the edit. Without call hierarchy, the
  agent re-greps the workspace. With call hierarchy, the impact set
  is precise (within same-document wave-4 scope) and the agent
  applies edits with full visibility. This is the §2.5 §A
  training-data-overlap (every rust-analyzer + pyright corpus
  has call hierarchy) compounding with ADR-0057e's cross-file rename
  — agents know who-calls-whom before they rename.

## 3. Scope

### 3.1 `textDocument/inlayHint`

`crates/cobrust-lsp/src/inlay.rs` (new) exposes:

```rust
pub fn build_inlay_hints(
    source: &str,
    line_map: &LineMap,
    range: Range,
    ctx: &TypeCheckCtx,
) -> Vec<InlayHint>;
```

Algorithm (walking the AST in `range`):

1. Parse `source` to an AST module (via the existing `parse_str` path).
   Walk module-level items; for each `Stmt::Let { target: Pattern,
   annot: None, value, .. }` whose span intersects `range`:
   - Resolve the binder name from `Pattern::Binding(name)` (wave-4
     scope: single-binding patterns only — tuple / sequence patterns
     deferred to wave-5).
   - Look up `ctx.lookup(&name)`. If `Some(ty)`, emit an `InlayHint`
     with `kind: InlayHintKind::TYPE`, `label: format!(": {ty}")`,
     positioned at the end of the binder span.
   - If the `let` already has `annot: Some(_)`, emit no hint
     (information is already in the source).

2. For each `Expr::Call { callee, args, .. }`:
   - Resolve the callee name (if `ExprKind::Name(name)`) → look up in
     `ctx.lookup(name)` → if `Ty::Fn { params, .. }`, iterate
     `(arg_idx, arg)` and the matching `Param` from the fn def.
   - For each `CallArg::Positional(expr)` where `expr` is NOT a
     literal (i.e. NOT `ExprKind::Literal` AND NOT `ExprKind::Name`
     equal to the param name), emit a parameter-name hint
     `<param_name>:` positioned before the arg span. Skip literal-
     named-after-param matches to avoid pointless decoration.

3. Honest scope: param-name hints require fn parameter names visible
   through `TypeCheckCtx`. Wave-4 walks the same-document AST for fn
   defs to look up param names; cross-file param-name resolution
   (via cross-doc AST cache) deferred to wave-5.

### 3.2 `textDocument/semanticTokens/full`

`crates/cobrust-lsp/src/semantic_tokens.rs` (new) exposes:

```rust
pub fn token_legend() -> SemanticTokensLegend;
pub fn build_semantic_tokens(source: &str, line_map: &LineMap)
    -> SemanticTokens;
```

**Legend** (`token_legend`):

| Index | tokenType | Source kind |
|---|---|---|
| 0 | `keyword` | `Token::Kw*` family (let / fn / if / while / for / match / class / return / break / continue / pass / raise / try / except / finally / with / and / or / not / in / is / lambda / yield / await / import / from / as / True / False / None) |
| 1 | `string` | `Token::Str { .. }` + `Token::Bytes { .. }` + `Token::FString { .. }` |
| 2 | `number` | `Token::Int(_)` + `Token::Float(_)` + `Token::Imag(_)` |
| 3 | `comment` | `#` line comments (re-lexed via byte scan since the parser strips trivia) |
| 4 | `operator` | `+`, `-`, `*`, `/`, `%`, `**`, `//`, `==`, `!=`, `<=`, `>=`, `<<`, `>>`, `=`, `:=`, `&`, `|`, `^`, `~`, `->`, `@`, and augmented-assign variants |
| 5 | `variable` | identifier-use site (default for `Token::Ident(_)`) |
| 6 | `function` | identifier-decl site at a fn def name (resolved via AST walk for `Stmt::Fn(FnDef { name, .. })`) |
| 7 | `type` | identifier in type-annotation position (resolved via AST walk for `Type::Name(_)`) |

**Modifiers** (one-bit flags) — wave-4 conservatively uses NONE
(empty modifier bitmask) on every token. Modifiers (`declaration` /
`readonly` / `static`) deferred to wave-5.

**Algorithm**:

1. Lex `source` with `cobrust_frontend::lex` to get `Vec<Token>`.
2. Parse `source` to an AST (best-effort; tokens emitted regardless
   of parse success — if parse fails, fall back to keyword + literal
   + operator + variable classification only, no function / type
   refinement).
3. For each `Token`, map its `TokenKind` to a `tokenType` per the
   legend table.
4. If the AST parsed, walk it to refine `variable` → `function` /
   `type` at known sites:
   - `Stmt::Fn(FnDef { name, .. })` → name span is `function`.
   - `Stmt::Class(ClassDef { name, .. })` → name span is `function`
     (no separate class token type in wave-4 to keep the legend
     compact — a future sub-ADR may split).
   - `Type::Name(path)` → each path segment is `type`.
5. Comments (`# .*\n`) are re-scanned by byte over `source` because
   the lexer strips them; emit `comment` tokens for each `#`-to-EOL
   range.
6. Emit `SemanticTokens` per LSP wire format: each token is the
   5-tuple `(delta_line, delta_start, length, tokenType, tokenModifier)`
   encoded as `u32`. Tokens MUST be sorted by `(line, start)`
   ascending before encoding.

### 3.3 Call hierarchy

`crates/cobrust-lsp/src/call_hierarchy.rs` (new) exposes three
functions plus three LSP handlers.

```rust
pub fn prepare_call_hierarchy(
    source: &str,
    line_map: &LineMap,
    position: Position,
    ctx: &TypeCheckCtx,
    uri: Url,
) -> Option<Vec<CallHierarchyItem>>;

pub fn incoming_calls(
    source: &str,
    line_map: &LineMap,
    item: &CallHierarchyItem,
) -> Vec<CallHierarchyIncomingCall>;

pub fn outgoing_calls(
    source: &str,
    line_map: &LineMap,
    item: &CallHierarchyItem,
) -> Vec<CallHierarchyOutgoingCall>;
```

**`prepare_call_hierarchy` algorithm**:

1. Position → byte offset → `word_at_offset` (re-used from `hover.rs`).
2. Confirm the word names a fn binding in `ctx.lookup(name)` and the
   resolved type is `Ty::Fn { .. }`.
3. Resolve the fn def-site span via the same-document word-scan
   already used by goto-def (`goto_def.rs::first_word_occurrence`).
   wave-4 limit: same-document only.
4. Build `CallHierarchyItem { name, kind: SymbolKind::FUNCTION, uri,
   range: <def-span>, selection_range: <name-span>, .. }`.

**`incoming_calls` algorithm**: walk the AST of `item.uri`'s source;
for each `Expr::Call { callee: Name(name), .. }` where `name == item.name`,
build a `CallHierarchyIncomingCall { from: <enclosing fn item>, from_ranges:
[<call-site span>] }`. Group multiple call-sites in the same caller by
extending `from_ranges`.

**`outgoing_calls` algorithm**: locate the AST fn def matching
`item.name`; walk its body's `Expr::Call { callee: Name(callee_name), .. }`
sites; for each unique callee, build a `CallHierarchyOutgoingCall { to:
<callee item>, from_ranges: [<call-site spans>] }`.

**Honest scope**: all three operate on the same-document AST only.
Cross-file call graph (calls from file-B to a fn defined in file-A)
deferred to wave-5. The `Backend` handler aggregates other-open-docs
similarly to wave-3 cross-file rename, but the wave-4 acceptance gate
verifies only the same-document path.

### 3.4 LSP handler registrations

`crates/cobrust-lsp/src/lib.rs::Backend` adds four `async fn`s:

```rust
async fn inlay_hint(&self, params: InlayHintParams)
    -> LspResult<Option<Vec<InlayHint>>>;
async fn semantic_tokens_full(&self, params: SemanticTokensParams)
    -> LspResult<Option<SemanticTokensResult>>;
async fn prepare_call_hierarchy(&self, params: CallHierarchyPrepareParams)
    -> LspResult<Option<Vec<CallHierarchyItem>>>;
async fn incoming_calls(&self, params: CallHierarchyIncomingCallsParams)
    -> LspResult<Option<Vec<CallHierarchyIncomingCall>>>;
async fn outgoing_calls(&self, params: CallHierarchyOutgoingCallsParams)
    -> LspResult<Option<Vec<CallHierarchyOutgoingCall>>>;
```

`ServerCapabilities` adds:

- `inlay_hint_provider: Some(OneOf::Left(true))`
- `semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(...))`
  with the legend from `semantic_tokens::token_legend()` + `full: Some(SemanticTokensFullOptions::Bool(true))`
- `call_hierarchy_provider: Some(CallHierarchyServerCapability::Simple(true))`

## 4. Non-goals

- **NO `inlayHint/resolve`** — wave-4 emits the `label` and `kind`
  in the initial response; tooltip / textEdits resolved-on-demand
  deferred to wave-5.
- **NO incremental semantic tokens** — `textDocument/semanticTokens/
  full/delta` deferred to wave-5; wave-4 ships only the full-document
  variant. Cursor / VSCode call `full` by default so this is
  unblocking for the primary editor targets.
- **NO cross-file call hierarchy** — wave-4 limits all three handlers
  to same-document scope. Cross-doc fn-graph aggregation deferred
  to wave-5.
- **NO type hierarchy** (`textDocument/prepareTypeHierarchy`) —
  separate sub-ADR.
- **NO signature help** (`textDocument/signatureHelp`) — separate
  sub-ADR.
- **NO document symbols / outline** (`textDocument/documentSymbol`)
  — separate sub-ADR.
- **NO modifier bitmask on semantic tokens** — wave-4 emits empty
  modifier on every token. Modifiers deferred to wave-5.

## 5. Acceptance gate

20 tests total (14 integration + 6 snapshot):

| # | Surface | Category | Description |
|---|---|---|---|
| 1 | inlay | integration | `let x = 42` without annot → `: Int` hint at binder end |
| 2 | inlay | integration | `let x: Int = 42` with explicit annot → no hint emitted |
| 3 | inlay | integration | fn-call with non-literal arg → param-name hint emitted |
| 4 | inlay | integration | nested `let` in fn body → hint at inner binder |
| 5 | inlay | integration | multi-fn doc → hints across all fns in range |
| 6 | semantic_tokens | integration | `let` / `fn` / `if` → `keyword` tokens |
| 7 | semantic_tokens | integration | `"hello"` string literal → `string` token |
| 8 | semantic_tokens | integration | `42` int literal → `number` token |
| 9 | semantic_tokens | integration | identifier at fn def site → `function` token |
| 10 | semantic_tokens | integration | type-annotation `: Int` → `type` token |
| 11 | call_hierarchy | integration | prepare on fn def → CallHierarchyItem with FUNCTION kind |
| 12 | call_hierarchy | integration | incoming calls — 2 callers in same doc → 2 IncomingCall items |
| 13 | call_hierarchy | integration | outgoing calls — fn body calls 3 callees → 3 OutgoingCall items |
| 14 | call_hierarchy | integration | unresolved symbol (unbound name) → prepare returns `None` |
| 15 | snapshot | inlay | `: Int` hint shape (single let) |
| 16 | snapshot | inlay | param-name hint shape (single call) |
| 17 | snapshot | semantic_tokens | encoded token vec for a 3-line program |
| 18 | snapshot | semantic_tokens | empty source → empty SemanticTokens |
| 19 | snapshot | call_hierarchy | CallHierarchyItem shape (prepare) |
| 20 | snapshot | call_hierarchy | OutgoingCall vec shape (multi-callee fn) |

## 6. Implementation plan

Estimated ~500-800 LOC across:

- `crates/cobrust-lsp/src/inlay.rs` (new) — `build_inlay_hints`
  walking AST `let` + `Call` sites (~180 LOC).
- `crates/cobrust-lsp/src/semantic_tokens.rs` (new) — `token_legend`
  + `build_semantic_tokens` with token-stream + AST-walk refinement
  (~220 LOC).
- `crates/cobrust-lsp/src/call_hierarchy.rs` (new) — three free
  functions + AST walker for caller / callee discovery (~180 LOC).
- `crates/cobrust-lsp/src/lib.rs` (extend) — five LSP handler `async
  fn`s + `ServerCapabilities` updates (~100 LOC).
- `crates/cobrust-lsp/tests/wave_4_e2e.rs` (new) — 20 tests per §5
  (~450 LOC).

Per-phase commits (6 atomic):

1. Author this ADR.
2. Implement `inlay.rs` + `Backend::inlay_hint` handler + `ServerCapabilities` update.
3. Implement `semantic_tokens.rs` + `Backend::semantic_tokens_full` handler + capabilities update.
4. Implement `call_hierarchy.rs` + three handlers + capabilities update.
5. Add 20 tests in `wave_4_e2e.rs`.
6. Dual-track docs (zh, en, agent) update + ADR status flip to accepted.

## 7. ADR-0057 frame relation

This ADR closes Phase J wave-4 (v1.2 LSP polish). Wave-4 row appended
to ADR-0057 §8:

| Sub-ADR | Feature | Status |
|---|---|---|
| 0057f | inlay hints + semantic tokens + call hierarchy | **this ADR** |

Post-wave-4: v1.2 LSP server shipped; eleven handlers operational.

## 8. Consequences

### 8.1 Positive

- Editor feature parity with rust-analyzer + pyright minimum:
  inlay hints, semantic tokens, call hierarchy all land.
- §2.5 compile-time-catch path made *visible* via inlay hints — the
  agent-LLM sees inferred types ex ante.
- §2.5 training-data-overlap reinforced via standardized semantic
  tokens legend matching established LSP server conventions.
- Refactor impact-radius visible via call hierarchy; the agent
  applies cross-fn edits with full visibility of dependents.
- v1.2 LSP server SHIPPED.

### 8.2 Negative

- Same-document scope for call hierarchy is honest but limits
  cross-file refactor confidence. Cross-doc aggregation deferred
  to wave-5.
- Semantic tokens modifier bitmask flat-zero — no readonly /
  declaration / static distinction. wave-5 may refine.
- Inlay hint param-name resolution is best-effort (requires
  same-document fn def to look up param names); cross-file param
  names not surfaced. wave-5 may refine.
- No incremental delta semantic tokens — every keystroke re-builds
  the full token vec. Performance is bounded by source size; an
  agent editing a 1k-line file sees ~5ms per `semantic_tokens/full`
  on Mac per benchmark target.

### 8.3 Neutral

- ~500-800 LOC across 5 files; matches ADR-0057 §13 wave-4 budget
  envelope.
- Three new modules align with ADR-0057a-e's per-feature module
  pattern.
- No changes to `cobrust-types` or `cobrust-hir`; wave-4 is
  LSP-only.

## 9. Why this ADR now

- ADR-0057 §8 wave-4 row scheduled; wave-3 closed at `9023f9d`
  (ADR-0057e ratified 2026-05-21). User dispatch 2026-05-21
  explicitly directs Phase J wave-4 dispatch.
- v0.5.0 ships with Phase J wave-3 closure (ADR-0054 §9); wave-4
  polish to v1.2 is the editor-parity bar before v0.6.0 planning.
- §2.5 compile-time-catch + training-data-overlap audit: each of
  the three wave-4 features passes the binding rules per §2 above.
- Wave-4 is the last polishing wave before Phase K planning; closing
  it now bounds the Phase J surface so cross-cutting Phase K work
  starts from a stable LSP baseline.

— P9 Tech Lead, 2026-05-21
