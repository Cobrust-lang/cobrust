---
doc_kind: adr
adr_id: 0057g
parent_adr: 0057
title: "Phase J wave-5 — semantic-tokens delta + inlayHint/resolve + cross-file call hierarchy (v1.3 LSP feature-complete)"
status: accepted
date: 2026-05-22
last_verified_commit: d521b77
ratified_at: d521b77
ratified_on: 2026-05-22
phase: "Phase J wave-5"
supersedes: []
superseded_by: []
relates_to: [adr:0057, adr:0057a, adr:0057b, adr:0057c, adr:0057d, adr:0057e, adr:0057f, adr:0056b, adr:0062]
discovered_by: ADR-0057 §8 sub-ADR roster (wave-5 row), ADR-0057f §4 honest-cite deferred items, user dispatch 2026-05-22
ratification_path: P9 sub-ADR review under ADR-0057 frame; ratifies on impl merge; closes Phase J wave-5 (v1.3 LSP feature-complete)
---

# ADR-0057g: Phase J wave-5 — semantic-tokens delta + inlayHint/resolve + cross-file call hierarchy

## 1. Motivation

Wave-4 (ADR-0057f, `52d8315`) shipped the v1.2 LSP server with inlay
hints, semantic tokens, and same-document call hierarchy, then folded
**three** honest-cite deferrals into wave-5 (per ADR-0057f §4):

1. **NO `inlayHint/resolve`** — wave-4 emitted `label` + `kind` only;
   no tooltip / extended hint data lazy-resolution.
2. **NO incremental semantic tokens** — wave-4 shipped only
   `textDocument/semanticTokens/full`; the LSP `full/delta` variant
   that minimises wire bytes on large workspaces was deferred.
3. **NO cross-file call hierarchy** — wave-4 limited prepare /
   incoming / outgoing to same-document scope; cross-doc fn-graph
   aggregation was deferred.

Wave-5 closes all three. After wave-5 the v1.3 LSP server is
**feature-complete** against the rust-analyzer + pyright minimum bar
for the surfaces ADR-0057 §4 prioritised: every LSP handler the
agent-LLM editor stack expects is wired, and ADR-0057f's three
deferrals are honestly resolved (no surfaces left "ship as full
fallback / same-doc only").

Per ADR-0057 §13 wall-time estimate, this is the wave-5 row (1-2 days
TEST + 4-6 hours DEV). Closure of wave-5 binds the Phase J
feature-complete declaration that ADR-0054 §9 v0.6.0 anticipates.

## 2. §2.5 LLM-first audit

Per CLAUDE.md §2.5 + ADR-0051, each wave-5 feature passes the binding
training-data-overlap + compile-time-catch tests:

- **semantic-tokens delta (training-data overlap + latency)**.
  rust-analyzer + pyright + tsserver all ship `semanticTokens/full/delta`.
  Cursor / VSCode call delta by default after the first `full`
  response so the request load on a large open file is `len(edit)`
  bytes, not `len(file)` bytes. For an agent-LLM doing rapid batch
  edits across a 1k-line file, the latency reduction is the difference
  between sub-50ms recoloring and visible flicker. §2.5 §B
  compile-time-catch path stays the same; the §A
  training-data-overlap (every LSP corpus contains delta round-trips)
  reinforces that the agent generates correct delta-aware code on
  first try because it's seen exactly this shape thousands of times.

- **inlay-hint resolve (hover-on-inlay UX)**. The
  `inlayHint/resolve` request carries the inlay's `data` field back to
  the server so it can lazily emit the tooltip — the agent hovering
  on an inlay sees an expanded Markdown explanation (the inferred
  type's derivation, the fn def-site, the param's documented role)
  without inflating the initial response payload. For a 100-line fn
  with 20 inlay hints, the initial response stays small; only when
  the agent actually hovers does the resolve path expand. This is
  §2.5 compile-time-catch surfaced *progressively* — the cheap signal
  ships eagerly, the rich tooltip ships lazily.

- **cross-file call hierarchy (workspace refactor visibility)**.
  Wave-4 limited prepare / incoming / outgoing to same-document. For
  an agent renaming a fn in a multi-file project, same-doc-only
  hierarchy means the agent has to grep cross-file callers by hand
  before applying the rename. Wave-5 walks every OPEN document in
  `Backend.documents` to aggregate cross-doc callers + callees, so
  the agent sees the full impact radius before edits. This is the
  §2.5 §A training-data-overlap (every rust-analyzer + pyright
  workspace corpus has cross-file call hierarchy) compounding with
  ADR-0057e's wave-3 cross-file rename — wave-5 makes "who calls
  this fn across the workspace?" answerable in one LSP request.

## 3. Scope

### 3.1 `textDocument/semanticTokens/full/delta`

`crates/cobrust-lsp/src/semantic_tokens.rs` extends with:

```rust
pub fn build_semantic_tokens_delta(
    source: &str,
    line_map: &LineMap,
    previous_result_id: Option<&str>,
    previous_tokens: Option<&[SemanticToken]>,
) -> SemanticTokensFullDeltaResult;
```

Algorithm (per LSP spec
`textDocument/semanticTokens/full/delta`):

1. Build the full `Vec<SemanticToken>` for `source` via the wave-4
   `build_semantic_tokens` path. Assign a fresh `result_id` (monotone
   per-URI; `Backend` tracks the last issued id + token vec).
2. If `previous_result_id` is `None` or out-of-cache (id ≠ stored
   id), fall back to `SemanticTokensFullDeltaResult::Tokens(SemanticTokens
   { result_id: Some(new_id), data: new_tokens })` — full response.
3. Otherwise diff `previous_tokens` against `new_tokens` token-by-token,
   building a minimal `Vec<SemanticTokensEdit>`. Each edit `(start,
   delete_count, data)` replaces a contiguous span of the previous
   delta-encoded token stream with new tokens.
4. Return `SemanticTokensFullDeltaResult::TokensDelta(SemanticTokensDelta
   { result_id: Some(new_id), edits })`.

`Backend` extension: a new `Mutex<HashMap<Url, (String, Vec<SemanticToken>)>>`
field stores per-URI `(last_result_id, last_tokens_data)` so the
handler reads the previous cache, runs the delta builder, and writes
the new cache before responding.

### 3.2 `inlayHint/resolve`

`crates/cobrust-lsp/src/inlay.rs` extends with:

```rust
pub fn resolve_inlay_hint(
    hint: InlayHint,
    ctx: &TypeCheckCtx,
) -> InlayHint;
```

Algorithm:

1. Read `hint.data` (set during the wave-5 emission pass) as
   `serde_json::Value`. The expected shape is
   `{"kind": "type", "name": <binder_name>}` for type hints or
   `{"kind": "param", "callee": <fn_name>, "param": <param_name>,
   "index": <u32>}` for param-name hints.
2. If the kind is `"type"`:
   - Look up `ctx.lookup(name)` again; if present, build a Markdown
     tooltip `**name**: \`Type\`\n\n_Inferred at let-binding._` and
     attach as `hint.tooltip = Some(InlayHintTooltip::MarkupContent(...))`.
3. If the kind is `"param"`:
   - Look up `ctx.lookup(callee)`; if the result is a `Ty::Fn { params,
     return_type, .. }`, build a Markdown tooltip
     `**\`callee(<sig>)\`**\n\nParameter \`param\` (slot N).`. Attach
     to `hint.tooltip`.
4. If neither lookup resolves (data missing, callee unbound), return
   the hint unchanged — wave-5 honest scope is a best-effort
   progressive enhancement, not a hard contract.

Wave-4 inlay emission is extended so every emitted hint carries its
`data` field populated for resolve to consume. The `tooltip` field
stays `None` in the initial response; resolve sets it.

### 3.3 Cross-file call hierarchy

`crates/cobrust-lsp/src/call_hierarchy.rs` extends three free functions
plus the three `Backend` handlers:

```rust
pub fn build_incoming_calls_cross_file(
    primary_source: &str,
    primary_line_map: &LineMap,
    item: &CallHierarchyItem,
    other_docs: &[(Url, String, LineMap)],
) -> Vec<CallHierarchyIncomingCall>;

pub fn build_outgoing_calls_cross_file(
    primary_source: &str,
    primary_line_map: &LineMap,
    item: &CallHierarchyItem,
    other_docs: &[(Url, String, LineMap)],
) -> Vec<CallHierarchyOutgoingCall>;
```

Algorithm:

1. Start with the same-doc result from the wave-4 `build_incoming_calls`
   / `build_outgoing_calls`.
2. For each `(other_uri, other_source, other_line_map)` in
   `other_docs`:
   - Parse the other doc's AST.
   - For incoming: walk every fn def in that doc; collect calls to
     `item.name`; build `CallHierarchyIncomingCall { from: <caller
     item in other_uri>, from_ranges: [<call-sites>] }`.
   - For outgoing: locate the `item.name` fn def in the primary doc
     (already done by `find_fn_def`); for each callee name referenced
     by its body, scan `other_docs` for a fn def matching that name —
     if found, attribute the callee item to that other_uri. Wave-5
     honest scope: callee resolution prefers same-doc, falls back to
     cross-doc only if same-doc has no matching def.
3. Concatenate per-doc results into the returned `Vec<...>`.

The `Backend` `incoming_calls` + `outgoing_calls` handlers extend to
gather `other_docs` snapshots under a single `docs.lock()` (same
pattern as wave-3 cross-file rename) before invoking the cross-file
function.

`prepare_call_hierarchy` itself stays same-doc (the cursor's URI
identifies the resolved fn's home document); only the incoming /
outgoing walks broaden cross-doc.

### 3.4 LSP handler registrations

`crates/cobrust-lsp/src/lib.rs::Backend` adds / extends:

```rust
async fn semantic_tokens_full_delta(
    &self,
    params: SemanticTokensDeltaParams,
) -> LspResult<Option<SemanticTokensFullDeltaResult>>;

async fn inlay_hint_resolve(&self, params: InlayHint) -> LspResult<InlayHint>;
```

`ServerCapabilities` updates:

- `inlay_hint_provider` flips from `Some(OneOf::Left(true))` to
  `Some(OneOf::Right(InlayHintServerCapabilities::Options(InlayHintOptions
  { resolve_provider: Some(true), .. })))` so clients pre-flight the
  resolve path.
- `semantic_tokens_provider` `full` option flips from
  `SemanticTokensFullOptions::Bool(true)` to
  `SemanticTokensFullOptions::Delta { delta: Some(true) }` so clients
  call the delta path after the first full response.

`incoming_calls` + `outgoing_calls` handlers gain `other_docs`
gathering under the same single-lock pattern wave-3 rename uses.

## 4. Non-goals

- **NO filesystem-walk for closed files** — cross-file call hierarchy
  + cross-file resolve consider OPEN documents only (consistent with
  wave-3 cross-file rename). Out-of-workspace = invisible.
- **NO graceful-degradation if `previousResultId` is out-of-sync** —
  if the cached id misses, fall back to the full response. The client
  retries on a fresh id; no partial delta synthesis.
- **NO trait-method call-hierarchy resolution** — `obj.method()` does
  not resolve to a trait def-site across files. Static dispatch only
  (the wave-4 honest scope persists).
- **NO inlayHint `text_edits` lazy-resolve** — wave-5 resolves
  `tooltip` only; `text_edits` (which would let the agent accept the
  inlay as a permanent source edit) stays `None` everywhere. Future
  sub-ADR may add.
- **NO `semanticTokens/range`** — wave-5 ships only `full` + `full/delta`.
- **NO type hierarchy / signature help / document symbols** — separate
  sub-ADRs per ADR-0057 §4 out-of-MVP list.
- **NO modifier bitmask refinement** — semantic tokens modifiers
  remain flat zero (wave-4 honest scope unchanged).

## 5. Acceptance gate

18 tests total (12 integration + 6 snapshot):

| # | Surface | Category | Description |
|---|---|---|---|
| 1 | delta | integration | first request with no previousResultId → full response (Tokens variant) |
| 2 | delta | integration | second request with matching previousResultId after single-token append → delta with one edit |
| 3 | delta | integration | second request with matching previousResultId after multi-token rewrite → delta with multiple edits |
| 4 | delta | integration | previousResultId out-of-sync (unknown id) → fallback to Tokens variant |
| 5 | resolve | integration | type-kind hint with name in ctx → resolve sets tooltip (Markdown contains type) |
| 6 | resolve | integration | param-kind hint with callee in ctx → resolve sets tooltip (Markdown contains callee + param) |
| 7 | resolve | integration | hint with absent data field → resolve returns unchanged (tooltip stays None) |
| 8 | crossfile | integration | incoming-from-other-file: 2-file doc, primary fn called from other → cross-file IncomingCall surfaces |
| 9 | crossfile | integration | outgoing-to-other-file: primary fn calls a callee defined in other doc → cross-file OutgoingCall surfaces with other_uri |
| 10 | crossfile | integration | both-directions: A->B and B->A patterns → incoming + outgoing both surface cross-file |
| 11 | crossfile | integration | no-cross-file-match: callee not present in any open doc → no cross-file OutgoingCall |
| 12 | crossfile | integration | cycle-detection: A calls B, B calls A → no infinite walk; both report exactly once |
| 13 | snapshot | delta | first-response Tokens variant shape |
| 14 | snapshot | delta | delta-response TokensDelta variant shape with at least one edit |
| 15 | snapshot | resolve | resolved type hint with tooltip shape |
| 16 | snapshot | resolve | resolved param hint with tooltip shape |
| 17 | snapshot | crossfile | IncomingCall vec shape across 2 files |
| 18 | snapshot | crossfile | OutgoingCall vec shape with mixed same-doc + cross-doc callees |

## 6. Implementation plan

Estimated ~400-700 LOC across:

- `crates/cobrust-lsp/src/semantic_tokens.rs` (extend) —
  `build_semantic_tokens_delta` + per-URI cache type (~150 LOC).
- `crates/cobrust-lsp/src/inlay.rs` (extend) — `resolve_inlay_hint`
  + `data` field population in wave-4 emit path (~100 LOC).
- `crates/cobrust-lsp/src/call_hierarchy.rs` (extend) —
  `build_incoming_calls_cross_file` + `build_outgoing_calls_cross_file`
  (~120 LOC).
- `crates/cobrust-lsp/src/lib.rs` (extend) —
  `semantic_tokens_full_delta` + `inlay_hint_resolve` handlers; cache
  field; capabilities update; `incoming_calls` + `outgoing_calls`
  other_docs gathering (~130 LOC).
- `crates/cobrust-lsp/tests/wave_5_e2e.rs` (new) — 18 tests per §5
  (~450 LOC).

Per-phase atomic commits (6):

1. Author this ADR.
2. Implement `build_semantic_tokens_delta` + `Backend::semantic_tokens_full_delta`
   handler + cache field + capabilities update.
3. Implement `resolve_inlay_hint` + extend `build_inlay_hints` to populate
   `data` + `Backend::inlay_hint_resolve` handler + capabilities update.
4. Implement cross-file call hierarchy helpers + extend `Backend::incoming_calls`
   / `outgoing_calls` to gather other_docs.
5. Add 18 tests in `wave_5_e2e.rs`.
6. Dual-track docs (zh + en + agent) + ADR status accepted + ADR-0057 frame §8 wave-5 row.

## 7. ADR-0057 frame relation

Wave-5 closes Phase J. ADR-0057 §8 row update:

| Sub-ADR | Feature | Status |
|---|---|---|
| 0057g | semantic-tokens delta + inlayHint/resolve + cross-file call hierarchy | **this ADR** |

Post-wave-5: v1.3 LSP server FEATURE-COMPLETE; thirteen handlers
operational (eleven from wave-4 + `semantic_tokens_full_delta` +
`inlay_hint_resolve`). ADR-0057f §4 honest-cite deferrals fully
closed.

## 8. Consequences

### 8.1 Positive

- v1.3 LSP server FEATURE-COMPLETE — the three ADR-0057f §4
  honest-cite deferrals are honestly closed.
- Wire-byte cost for semantic-tokens recoloring drops from O(file)
  to O(edit) on every keystroke after the first response.
- Inlay hints become progressively rich — initial response stays
  cheap, hover-on-inlay reveals Markdown tooltip with type
  derivation + callee signature.
- Cross-file call hierarchy unlocks workspace refactor confidence;
  the agent sees who-calls-whom across every open document before
  applying renames.
- Phase J SHIPPED. ADR-0054 §9 v0.6.0 binding cleared.

### 8.2 Negative

- Per-URI semantic-tokens cache + result_id allocation: ~32 bytes
  per token × 1k tokens × N open URIs = ~32KB per file held in
  `Backend`. Trivially bounded by open-doc count.
- Cross-file call hierarchy walks every OPEN doc per request.
  Worst-case O(N_docs × M_fn) for incoming aggregation. The wave-4
  same-doc path keeps O(M_fn). For typical 10-doc workspaces this
  is well within sub-50ms budget.
- The `data` field on every inlay hint adds ~50 bytes per hint to
  the wire payload (in the eager `textDocument/inlayHint` response).
  Offset by the resolve path's avoidance of always-emit tooltips.

### 8.3 Neutral

- ~400-700 LOC across 5 files; matches wave-4 envelope.
- No changes to `cobrust-types` / `cobrust-hir` / `cobrust-mir`;
  wave-5 is LSP-only.
- The semantic-tokens cache mutex is independent of `docs` and
  `session_ctx` mutexes; no lock-order considerations.

## 9. Why this ADR now

- ADR-0057 §8 wave-5 row scheduled; wave-4 closed at `52d8315`
  (ADR-0057f ratified 2026-05-22). User dispatch 2026-05-22
  directs Phase J wave-5 dispatch.
- v0.6.0 binds on Phase J wave-5 closure per ADR-0054 §9.
- ADR-0057f §4 deferred items become technical debt the longer they
  stay deferred; closing them now bounds the Phase J surface
  cleanly so cross-cutting Phase K work starts from a stable LSP
  feature-complete baseline.
- §2.5 audit (§2 above): every wave-5 feature passes the
  training-data-overlap + compile-time-catch binding rules.

— P9 Tech Lead, 2026-05-22
