---
doc_kind: adr
adr_id: 0057b
parent_adr: 0057
title: "Phase J wave-2.1 — LSP `textDocument/didChange` incremental + Session reuse"
status: proposed
date: 2026-05-21
last_verified_commit: 05aa137
supersedes: []
superseded_by: []
relates_to: [adr:0057, adr:0057a, adr:0056b]
discovered_by: ADR-0057a §10 cascade — didChange wave-2 explicit defer
ratification_path: P9 sub-ADR review under ADR-0057 frame; ratifies on impl merge
---

# ADR-0057b: Phase J wave-2.1 — LSP `textDocument/didChange` incremental + Session reuse

## 1. Motivation

ADR-0057a wave-1 shipped publishDiagnostics on `did_open` (and a
minimal FULL-sync `did_change` re-runs the pipeline from scratch).
The agent-LLM editor surface today only refreshes diagnostics when a
file is (re-)opened — between opens the user can edit for many
keystrokes without the LLM seeing fresh structured fix-paths.

Wave-2.1 (this ADR) makes diagnostics **LIVE** as the user types:

- Incremental edits via `contentChanges[].range` (LSP standard).
- Full-replace edits via single `contentChanges[]` without `range`.
- Bounded debounce (~100ms) so rapid keystrokes do not stampede the
  type-check pipeline.
- Session reuse via the ADR-0056b `Clone + Send` Arc-COW contract:
  clone the Session once on `did_open`, mutate per URI via
  `Session::invalidate(file_id)` on each `did_change`, re-run
  parse → check → publish_diagnostics against the **shared**
  incremental `TypeCheckCtx`.

The §2.5 binding: every Cursor / VSCode / Cody / Aider / Continue
session benefits from per-keystroke fix-path feedback. Without this
wire the agent-LLM has no "fix loop" between save points.

## 2. §2.5 LLM-first audit

**Compile-time-catch via live diagnostics.** ADR-0057a wired the
catch surface to the LSP envelope; wave-2.1 makes the catch
**realtime**. The agent-LLM's fix-path latency drops from
"save → re-open → diagnose" (multi-second) to
"keystroke → debounce 100ms → diagnose" (per-keystroke). Every
`TypeError::*` / `MirError::*` / `LoweringError::*` variant becomes
a feedback loop the LLM consumes in-line.

**Training-data overlap with `textDocument/didChange`.** The LSP
`didChange` notification is one of the highest-frequency shapes in
modern IDE-LLM training corpora — every editor implementation
(VSCode, Neovim, Helix, Cursor, Continue, Cody, Aider's LSP bridge)
emits `DidChangeTextDocumentParams` with `contentChanges` arrays.
Matching this shape byte-for-byte (including the standard
incremental `Range` + `text` pair) maximises LLM correctness on
client-side editor integrations.

## 3. Scope

### 3.1 `did_change` handler shape

```rust
async fn did_change(&self, params: DidChangeTextDocumentParams) { ... }
```

Reads `params.text_document.uri`, `params.text_document.version`,
and `params.content_changes: Vec<TextDocumentContentChangeEvent>`.

### 3.2 Two modes — incremental + full-replace

LSP spec `textDocument/didChange` admits both:

- **Incremental**: each `TextDocumentContentChangeEvent` carries
  `range: Some(Range)` + `text: String`. The server splices `text`
  into the current document at `range`. Multiple events apply in
  array order (LSP §"DidChangeTextDocumentParams").
- **Full-replace**: a single event carries `range: None` (or
  `range_length: None`) + `text: String` containing the entire new
  document.

Wave-2.1 declares `TextDocumentSyncKind::INCREMENTAL` in
`initialize.capabilities` and supports BOTH branches at the handler
level (servers MUST handle full-replace as a fallback per spec).

### 3.3 Per-URI text-store (in-memory)

Extend the existing `Backend.docs: Mutex<HashMap<Url, DocState>>`
(wave-1 already caches `source + line_map + version`). Wave-2.1
mutates `DocState.source` in-place via range-splice and rebuilds the
`LineMap` after each event batch. The handler is the single writer
per URI; `Mutex` is held for the splice + LineMap rebuild only —
released before the pipeline re-run so concurrent reads (Phase J+
hover / completion) do not block.

### 3.4 Session reuse via `Session::invalidate(file_id)`

Wave-1 spins up a fresh `cobrust_hir::lower::Session` and a fresh
`cobrust_types::check` call on every `did_change`. Wave-2.1
introduces a **shared** `cobrust_cli::repl::Session` on `Backend`:

```rust
pub struct Backend {
    client: Client,
    docs: Mutex<HashMap<Url, DocState>>,
    /// ADR-0056b §3.3 + §6 — Clone+Send Arc-COW Session.
    /// One Session per Backend; per-URI mutation via
    /// Session::invalidate(file_id).
    session: Arc<Mutex<Session>>,
}
```

On `did_change`:

1. Apply content changes to `DocState.source` (§3.2 + §3.3).
2. Map URI → `file_id` (stable per URI via FileId pool).
3. `session.lock().invalidate(file_id)` to drop stale type-cache rows.
4. Re-run pipeline: parse → HIR-lower → type-check (via
   `session.type_ctx()`-aware path) → produce diagnostics.
5. `publish_diagnostics(uri, diagnostics, Some(version))`.

The `Arc<Mutex<Session>>` is held for the `invalidate` + pipeline
re-run window. Wave-2.1 keeps the mutex coarse — fine-grained
read/write split is a follow-up (§6 Risk 2).

**URI → FileId mapping.** Wave-2.1 uses a per-URI counter inside
`Backend` (a `Mutex<HashMap<Url, u32>>` with the next free `u32`
allocated on first `did_open`). This avoids depending on the
`cobrust-frontend` FileId pool (which is per-pipeline-invocation).
Mapping is stable within a Backend lifetime.

### 3.5 Bounded debounce (~100ms)

Per ADR-0057a §9 Risk 3, keystroke cadences can exceed 50ms in
Cursor. Wave-2.1 introduces a **per-URI debounce token**:

- On each `did_change`, record the latest event timestamp + version
  in a `HashMap<Url, DebounceState>`.
- Spawn a `tokio::time::sleep(Duration::from_millis(100))` task.
- After the sleep, check if the recorded version is still the
  latest; if yes, run the pipeline. If no, the next event's task
  will run instead.
- This coalesces N events arriving within 100ms into ONE pipeline
  re-run + ONE `publish_diagnostics` emission.

Wave-2.1 implements this via a `tokio::sync::Mutex<HashMap<Url, i32>>`
holding the "last scheduled version" — the spawned task self-checks
against this value before running the pipeline.

## 4. Non-goals (wave-2.1)

- **NO incremental parse**: full re-parse on each pipeline re-run.
  AST-cache + incremental parse is ADR-0057e wave-2.2 scope.
- **NO incremental type-check**: full re-check via `session.invalidate
  + merge_module`. True per-DefId incremental check (avoid re-checking
  unchanged DefIds) is ADR-0056c follow-up scope.
- **NO hover / completion / definition / rename**: separate Phase J
  sub-ADRs (0057b-historical was hover+completion; this 0057b is
  the didChange wave per ADR-0057a §10 cascade).
- **NO Code Action emission on didChange**: code actions surface on
  `textDocument/codeAction` request, not on `didChange` push.
- **NO multi-file invalidation**: wave-2.1 invalidates only the URI
  whose source changed. Cross-file dependency invalidation is
  ADR-0056c (Session multi-file path) future scope.

## 5. Acceptance gate (5 integration + 5 snapshot tests)

5 integration tests covering the full handler surface:

1. **`did_change_incremental_refreshes_diagnostics`** — open with
   error → send incremental edit that fixes the error → verify
   `publish_diagnostics` is called twice (open + after debounce)
   with the second emission carrying an empty diagnostic vector.
2. **`did_change_full_replace_diagnostics`** — open with valid
   source → send full-replace edit introducing a `TypeMismatch` →
   verify the second emission carries 1 `Diagnostic` with code
   `type_mismatch`.
3. **`did_change_debounce_coalesces`** — fire 5 incremental events
   within 50ms → verify exactly ONE pipeline re-run + ONE
   `publish_diagnostics` emission (in addition to the initial open).
4. **`did_change_invalidate_session_drops_stale_types`** — open
   `let x: i64 = 1` → edit to `let x: str = "hi"` → verify the
   downstream `:type x` (queried via `session.type_ctx().lookup`)
   reports `Str`, not stale `Int`.
5. **`did_change_concurrent_serialized_no_race`** — fire 10
   concurrent `did_change` calls via `tokio::spawn` → assert final
   `DocState.source` matches the last-applied event AND no panic
   from poisoned mutex. (`Backend` mutexes serialise writes.)

5 insta snapshot tests for diagnostics-after-edit JSON shape:

1. `snapshot_after_incremental_type_mismatch`
2. `snapshot_after_full_replace_unbound_name`
3. `snapshot_after_incremental_implicit_truthiness`
4. `snapshot_after_full_replace_arity_mismatch`
5. `snapshot_after_incremental_clears_diagnostics`

## 6. Risk register

1. **Tower-LSP client-side ordering vs concurrent edits**.
   `tower-lsp` dispatches handlers from multiple tokio tasks, so two
   `did_change` events MAY interleave at the await boundary.
   **Mitigation**: §3.4 holds the `Backend.session` mutex across the
   entire pipeline + publish call. The §3.5 debounce additionally
   funnels rapid edits into a single re-run. Test 5 verifies the
   serialisation invariant.

2. **Coarse Session mutex blocks readers**. Phase J+ hover /
   completion (ADR-0057b-historical was this surface) reads
   `Session::type_ctx()` and would block on writers under the
   wave-2.1 coarse `Mutex<Session>`. **Mitigation**: ADR-0056b §6
   already declares the lock-free read contract via Arc-COW; wave-2.1
   uses `Mutex<Session>` only as a temporary funnel until hover /
   completion lands. Migration to `Arc<Session>` + interior
   `Arc<TypeCheckCtx>` snapshot is a follow-up sub-ADR.

3. **Large file performance (>10K LOC re-parse cost)**. §4 punts
   incremental parse to wave-2.2. Wave-2.1 may exceed the 100ms
   p99 budget on files >5K LOC. **Mitigation**: the 100ms debounce
   absorbs some of the cost; if Cursor smoke surfaces UX
   regressions, follow-up sub-ADR ships AST cache + incremental
   parse without breaking the `did_change` wire shape.

## 7. Implementation plan (~500-800 LOC)

| Phase | Surface | LOC |
|---|---|---|
| 1. Backend struct extension | `lib.rs` Backend fields + `new()` | ~30 |
| 2. `did_change` handler | `lib.rs` LanguageServer impl | ~80 |
| 3. Range-application helper | `lib.rs` or new module | ~80 |
| 4. Per-URI FileId pool | `lib.rs` Backend method | ~30 |
| 5. Bounded debounce | `src/debounce.rs` new module | ~100 |
| 6. Session reuse wire | `lib.rs` integration | ~50 |
| 7. 5 integration tests | `tests/did_change_e2e.rs` | ~250 |
| 8. 5 snapshot tests | extend `tests/snapshot_diagnostics.rs` | ~150 |
| **Total** | | **~770 LOC** |

Branch: `feature/0057b-didchange`. Mac single-crate verify: `cargo
test -p cobrust-lsp` PASS (existing 16 + new 10 = 26 tests).

## 8. Consequences

### Positive

- §2.5 LLM-amplifier ROI #2 surface delivered: per-keystroke
  diagnostic refresh closes the agent-LLM fix-loop latency.
- ADR-0056b Arc-COW Session contract realised end-to-end — the
  Phase I × J handoff primitive ships its first consumer.
- ADR-0057a §10 cascade addendum ("`Session::type_ctx` Clone+Send
  handoff is consumed in ADR-0057a wave-2 (deferred)") RESOLVED.

### Negative

- Wave-2.1 cannot deliver per-DefId incremental type-check (§4);
  full re-check runs on every debounced batch. Acceptable for
  small files; large-file deferral noted in §6 Risk 3.
- Coarse `Mutex<Session>` blocks future hover / completion readers
  until the follow-up sub-ADR migrates to lock-free read (§6 Risk 2).

### Neutral

- The `tokio::sync::Mutex<HashMap<Url, i32>>` debounce-token map
  adds ~40 bytes per open URI; negligible memory footprint.
- LSP `INCREMENTAL` sync mode advertised on `initialize`; clients
  default to full-replace if they cannot compute deltas. Backend
  handles both branches transparently.

## 9. Ratification

This ADR ratifies on `feature/0057b-didchange` impl merge. Per
ADR-0057 §13, sub-ADR ratification rolls up to parent Phase J
status.
