//! `cobrust-lsp` — Cobrust Language Server Protocol implementation.
//!
//! Phase J wave-1 (ADR-0057a) — `textDocument/publishDiagnostics` wire
//! mapping. Per ADR-0057 §2, the highest §2.5 ROI surface in the H-L
//! roadmap: every in-editor agent-LLM (Cursor / VSCode / Cody / Aider
//! / Continue) consumes published `Diagnostic` arrays as the primary
//! fix-path signal. ADR-0052b shipped `suggestion: Option<&'static str>`
//! across the 25 + 11 + 6 = 42 `TypeError + MirError + LoweringError`
//! variants; this crate wires the field to LSP `Diagnostic.related-
//! Information[0].message` so the agent-LLM consumes diagnosis + fix-
//! path without prose-stripping `cobrust check` stderr.
//!
//! Phase J wave-2.1 (ADR-0057b) — `textDocument/didChange` incremental
//! + Session reuse. Live diagnostics on each keystroke via:
//!   - LSP `INCREMENTAL` sync (range-splice + full-replace branches).
//!   - Per-URI text-store with `LineMap` rebuild after every batch.
//!   - Bounded ~100ms debounce so rapid edits coalesce into one
//!     pipeline re-run + one `publish_diagnostics` emission.
//!   - Shared `TypeCheckCtx` (ADR-0056b Arc-COW Clone+Send contract)
//!     across calls, with per-URI `FileId` allocation + `invalidate`
//!     dropping stale type rows before the next re-check.
//!
//! Public surface:
//! - [`Backend`] — the `tower_lsp::LanguageServer` implementation.
//! - [`span_convert`] — `Span` → LSP `Range` via `LineMap`.
//! - [`diagnostic`] — `From<&TypeError/&MirError/&LoweringError> for
//!   lsp_types::Diagnostic` impls.
//! - [`debounce`] — ADR-0057b §3.5 bounded debounce token.
//!
//! Wave-2+ extends this surface to hover / completion / definition /
//! rename / codeAction per ADR-0057 §4.

#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::enum_glob_use)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cobrust_frontend::span::FileId;
use cobrust_types::{TypeCheckCtx, check_incremental};
use tower_lsp::Client;
use tower_lsp::LanguageServer;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    CompletionOptions, CompletionParams, CompletionResponse, Diagnostic,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams, Hover,
    HoverParams, HoverProviderCapability, InitializeParams, InitializeResult, InitializedParams,
    MessageType, OneOf, PrepareRenameResponse, RenameOptions, RenameParams, ServerCapabilities,
    ServerInfo, TextDocumentContentChangeEvent, TextDocumentPositionParams,
    TextDocumentSyncCapability, TextDocumentSyncKind, Url, WorkspaceEdit,
};

pub mod code_action;
pub mod completion;
pub mod debounce;
pub mod diagnostic;
pub mod hover;
pub mod rename;
pub mod span_convert;

pub use code_action::{
    code_action_kind_for_fix_safety, code_action_kind_for_lowering_error,
    code_action_kind_for_mir_error, code_action_kind_for_type_error, fix_safety_from_code,
};
pub use completion::{
    build_completion_response, keyword_items, prefix_at_offset, prelude_items, scope_items,
};
pub use debounce::{DEFAULT_DEBOUNCE_MS, DebounceTokens};
pub use diagnostic::{
    lowering_error_to_diagnostic, mir_error_to_diagnostic, type_error_to_diagnostics,
};
pub use hover::{render_hover_markdown, resolve_hover, word_at_offset};
pub use rename::{prepare_rename, rename_symbol};
pub use span_convert::{LineMap, span_to_lsp_range};

/// Per-document state cached by the LSP server.
///
/// Wave-1 keeps the cached source + `LineMap` so `did_change` can
/// rebuild diagnostics without re-scanning the file's bytes for line
/// offsets on every keystroke. Wave-2+ extends this with `hir_tree`,
/// `mir_funcs`, and the per-file `TypeCheckCtx` snapshot per ADR-0057
/// §6.
#[derive(Clone, Debug)]
pub struct DocState {
    /// Latest source text the client sent us.
    pub source: String,
    /// Byte-offset → line/column map built from `source`.
    pub line_map: LineMap,
    /// Client-supplied version number; echoed back on `publish_diagnostics`.
    pub version: i32,
}

impl DocState {
    /// Build a fresh `DocState` from a full source snapshot.
    pub fn new(source: String, version: i32) -> Self {
        let line_map = LineMap::from_source(&source);
        Self {
            source,
            line_map,
            version,
        }
    }
}

/// The Cobrust LSP `Backend`.
///
/// `Backend` implements [`tower_lsp::LanguageServer`] and routes
/// every `textDocument/*` request through the Cobrust frontend +
/// HIR + types pipeline. Wave-1 surface: `initialize`, `initialized`,
/// `did_open`, `did_change`, `did_close`, `shutdown`. Wave-2.1 adds:
///   - incremental `did_change` with per-URI text-store mutation;
///   - shared cross-call `TypeCheckCtx` (ADR-0056b Clone+Send Arc-COW);
///   - per-URI `FileId` allocation;
///   - bounded ~100ms debounce.
pub struct Backend {
    /// Tower-LSP client handle used to push notifications
    /// (`publish_diagnostics`, `log_message`, etc.).
    client: Client,
    /// URI → per-document state. Wrapped in a `Mutex` because
    /// `tower-lsp` calls handlers from multiple tokio tasks and the
    /// LSP `Backend` is `Sync`. Wave-2.1 mutates `DocState.source`
    /// in-place via the §3.3 range-splice path.
    docs: Mutex<HashMap<Url, DocState>>,
    /// Shared incremental type-check context per ADR-0057b §3.4. Arc-COW
    /// snapshots (per ADR-0056b §6) keep `Clone` O(1); the mutex
    /// serialises writes so per-URI `invalidate` → `check_incremental`
    /// cannot interleave. Phase J+ hover / completion will migrate to a
    /// lock-free read path via `Arc<TypeCheckCtx>` snapshots.
    session_ctx: Arc<Mutex<TypeCheckCtx>>,
    /// URI → opaque per-document `FileId` (a u32). Allocated on first
    /// `did_open`; passed to `check_incremental` + `TypeCheckCtx::invalidate`
    /// so cross-file type rows don't collide. Per ADR-0057b §3.4 the
    /// counter is per-`Backend` (not global), so multiple `Backend`s in
    /// a process don't conflict.
    uri_file_ids: Mutex<UriFileIdPool>,
    /// Per-URI debounce token map per ADR-0057b §3.5. Each entry records
    /// the latest scheduled version; a spawned debounce task checks
    /// against the map before running the pipeline.
    debounce_tokens: Arc<DebounceTokens>,
}

/// Per-`Backend` allocator that assigns a stable `u32` `FileId` to each
/// open URI. Wave-2.1 uses this to scope `TypeCheckCtx` rows per-URI
/// (so an `invalidate(file_id)` only drops the URI's own type cache).
#[derive(Debug, Default)]
struct UriFileIdPool {
    /// URI → assigned `FileId` (a u32).
    map: HashMap<Url, u32>,
    /// Next free `FileId`. Allocated lazily; never reused on
    /// `did_close` to keep IDs monotonic across the backend lifetime
    /// (recycling would risk leaking stale rows into a future doc).
    next: u32,
}

impl UriFileIdPool {
    /// Get the `FileId` for `uri`, allocating a fresh one if needed.
    /// Skips `0` (reserved for `FileId::SYNTHETIC`) by starting at 1.
    fn intern(&mut self, uri: &Url) -> u32 {
        if let Some(&id) = self.map.get(uri) {
            return id;
        }
        // Skip 0 (SYNTHETIC). Start at 1.
        if self.next == 0 {
            self.next = 1;
        }
        let id = self.next;
        self.next = self.next.saturating_add(1);
        self.map.insert(uri.clone(), id);
        id
    }
}

impl Backend {
    /// Construct a new `Backend` bound to a `Client`.
    pub fn new(client: Client) -> Self {
        Self::with_debounce_ms(client, DEFAULT_DEBOUNCE_MS)
    }

    /// Construct a `Backend` with a custom debounce window (in ms).
    /// Tests pass `0` to bypass debounce entirely.
    pub fn with_debounce_ms(client: Client, debounce_ms: u64) -> Self {
        Self {
            client,
            docs: Mutex::new(HashMap::new()),
            session_ctx: Arc::new(Mutex::new(TypeCheckCtx::new())),
            uri_file_ids: Mutex::new(UriFileIdPool::default()),
            debounce_tokens: Arc::new(DebounceTokens::new(Duration::from_millis(debounce_ms))),
        }
    }

    /// Read-only accessor for the shared `TypeCheckCtx`. Phase J+
    /// hover / completion will consume this for cross-file symbol
    /// lookups; tests assert on it after `did_change`.
    pub fn session_ctx_snapshot(&self) -> TypeCheckCtx {
        self.session_ctx
            .lock()
            .expect("session_ctx poisoned")
            .clone()
    }

    /// Intern a URI → `FileId` allocation. Public for tests; production
    /// callers go through `did_open` / `did_change`.
    pub fn file_id_for(&self, uri: &Url) -> u32 {
        self.uri_file_ids
            .lock()
            .expect("uri_file_ids poisoned")
            .intern(uri)
    }

    /// Run the Cobrust compile pipeline against `source`, returning
    /// the per-URI `Diagnostic` vector ready for
    /// `publish_diagnostics`.
    ///
    /// Pipeline order (per ADR-0057a §4):
    /// 1. `cobrust_frontend::parse_str` — lex + parse.
    /// 2. `cobrust_hir::lower` — AST → HIR.
    /// 3. `cobrust_types::check` — HIR → typed module.
    ///
    /// Each stage's error variants are mapped to LSP `Diagnostic`s
    /// via the `From`-impls in [`diagnostic`]. Wave-1 emits `Error`
    /// severity only per ADR-0057a §5.
    ///
    /// Wave-1 stateless variant kept for snapshot tests + smoke tools.
    /// Wave-2.1's stateful path is [`Self::compile_diagnostics_with_session`].
    pub fn compile_diagnostics(source: &str, line_map: &LineMap) -> Vec<Diagnostic> {
        use cobrust_frontend::parse_str;

        let mut diagnostics: Vec<Diagnostic> = Vec::new();

        // Stage 1: parse.
        let ast_module = match parse_str(source, FileId::SYNTHETIC) {
            Ok(m) => m,
            Err(err) => {
                diagnostics.push(diagnostic::frontend_error_to_diagnostic(&err, line_map));
                return diagnostics;
            }
        };

        // Stage 2: HIR lowering.
        let mut hir_sess = cobrust_hir::lower::Session::new();
        let hir_module = match cobrust_hir::lower::lower(&ast_module, &mut hir_sess) {
            Ok(m) => m,
            Err(err) => {
                diagnostics.push(diagnostic::lowering_error_to_diagnostic(&err, line_map));
                return diagnostics;
            }
        };

        // Stage 3: type-check.
        if let Err(err) = cobrust_types::check(&hir_module) {
            diagnostics.extend(diagnostic::type_error_to_diagnostics(&err, line_map));
        }

        diagnostics
    }

    /// Wave-2.1 stateful pipeline per ADR-0057b §3.4.
    ///
    /// Calls `TypeCheckCtx::invalidate(file_id)` BEFORE re-checking so
    /// stale rows from the previous version are dropped; then runs
    /// `check_incremental(&mut ctx, &hir, file_id)` which merges fresh
    /// types back into the shared ctx. Diagnostics are produced from
    /// the same error path as wave-1 — only the symbol-table reuse is
    /// new.
    pub fn compile_diagnostics_with_session(
        source: &str,
        line_map: &LineMap,
        ctx: &mut TypeCheckCtx,
        file_id: u32,
    ) -> Vec<Diagnostic> {
        use cobrust_frontend::parse_str;

        let mut diagnostics: Vec<Diagnostic> = Vec::new();

        // Drop stale type-cache rows for this file BEFORE re-checking
        // (per ADR-0057b §3.4 step 3).
        ctx.invalidate(file_id);

        // Use the URI's `FileId` for span tracking so future cross-file
        // queries don't collide.
        let frontend_file_id = FileId(file_id);
        let ast_module = match parse_str(source, frontend_file_id) {
            Ok(m) => m,
            Err(err) => {
                diagnostics.push(diagnostic::frontend_error_to_diagnostic(&err, line_map));
                return diagnostics;
            }
        };

        let mut hir_sess = cobrust_hir::lower::Session::new();
        let hir_module = match cobrust_hir::lower::lower(&ast_module, &mut hir_sess) {
            Ok(m) => m,
            Err(err) => {
                diagnostics.push(diagnostic::lowering_error_to_diagnostic(&err, line_map));
                return diagnostics;
            }
        };

        // Stage 3: incremental type-check merges fresh rows into ctx.
        if let Err(err) = check_incremental(ctx, &hir_module, file_id) {
            diagnostics.extend(diagnostic::type_error_to_diagnostics(&err, line_map));
        }

        diagnostics
    }

    /// Apply LSP content-change events (incremental or full-replace) to
    /// a source string. Returns the spliced `String`.
    ///
    /// Per ADR-0057b §3.2 + §3.3:
    ///   - If `change.range` is `Some`, splice `change.text` at the
    ///     UTF-16 range. We map each [`Position`] back to a byte offset
    ///     using a freshly-built [`LineMap`] over the *current* source.
    ///   - If `change.range` is `None`, replace the entire source.
    ///
    /// Events are applied in array order. The `LineMap` is rebuilt after
    /// each event because subsequent ranges are relative to the
    /// post-edit source.
    pub fn apply_content_changes(
        mut source: String,
        changes: &[TextDocumentContentChangeEvent],
    ) -> String {
        for change in changes {
            let Some(range) = change.range else {
                source.clone_from(&change.text);
                continue;
            };
            let line_map = LineMap::from_source(&source);
            let Some(start) = line_map.position_to_byte(range.start) else {
                // Out-of-bounds position; client mis-spec'd. Fall back
                // to clamping at EOF.
                source.push_str(&change.text);
                continue;
            };
            let Some(end) = line_map.position_to_byte(range.end) else {
                source.push_str(&change.text);
                continue;
            };
            let start = (start as usize).min(source.len());
            let end = (end as usize).min(source.len()).max(start);
            source.replace_range(start..end, &change.text);
        }
        source
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> LspResult<InitializeResult> {
        // ADR-0057b §3.2 — advertise INCREMENTAL sync.
        // ADR-0057c §6 — advertise hover + completion capabilities.
        // ADR-0057d §3 — advertise prepareRename + rename capabilities.
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".to_string(), "_".to_string()]),
                    ..Default::default()
                }),
                // Advertise rename with prepare_rename support so clients
                // pre-flight the request (ADR-0057d §3.1).
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                ..ServerCapabilities::default()
            },
            server_info: Some(ServerInfo {
                name: "cobrust-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        self.client
            .log_message(
                MessageType::INFO,
                "cobrust-lsp wave-2.1 initialized (ADR-0057b textDocument/didChange + Session reuse)",
            )
            .await;
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let source = params.text_document.text;
        let version = params.text_document.version;
        let file_id = self.file_id_for(&uri);
        let state = DocState::new(source, version);
        let diagnostics = {
            let mut ctx = self.session_ctx.lock().expect("session_ctx poisoned");
            Backend::compile_diagnostics_with_session(
                &state.source,
                &state.line_map,
                &mut ctx,
                file_id,
            )
        };
        {
            let mut docs = self.docs.lock().expect("docs mutex poisoned");
            docs.insert(uri.clone(), state);
        }
        self.client
            .publish_diagnostics(uri, diagnostics, Some(version))
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        let changes = params.content_changes;

        // Step 1: apply content changes to the per-URI text store.
        // Handler holds the docs mutex only across the splice +
        // LineMap rebuild — released before the pipeline re-run.
        let new_state = {
            let mut docs = self.docs.lock().expect("docs mutex poisoned");
            let prev_source = docs.get(&uri).map(|s| s.source.clone()).unwrap_or_default();
            let new_source = Backend::apply_content_changes(prev_source, &changes);
            let state = DocState::new(new_source, version);
            docs.insert(uri.clone(), state.clone());
            state
        };

        // Step 2: schedule a debounced pipeline re-run. If the version
        // we recorded is overtaken by a later `did_change` within the
        // debounce window, the spawned task self-cancels.
        let token = self.debounce_tokens.schedule(uri.clone(), version);
        let client = self.client.clone();
        let session_ctx = Arc::clone(&self.session_ctx);
        let file_id = self.file_id_for(&uri);
        let docs_arc: Arc<()> = Arc::new(()); // placeholder for future shared ref
        let _ = docs_arc;
        let debounce_tokens = Arc::clone(&self.debounce_tokens);
        let uri_clone = uri.clone();

        tokio::spawn(async move {
            // Wait the debounce window. If a later event supersedes us
            // before we wake, the spawned task for *that* event will
            // see its token as latest; we bail.
            debounce::wait_for_token(token).await;
            if !debounce_tokens.is_latest(&uri_clone, version) {
                return;
            }

            // Wake → pipeline re-run + publish. Note: we re-read the
            // current `DocState` (the source recorded for `version`)
            // rather than capturing the `new_state` clone, because a
            // *non-debounced* superseding event might have already
            // mutated the store. We bailed above if so; reaching here
            // means our version is latest, so the store still has our
            // text.
            let diagnostics = {
                let mut ctx = session_ctx.lock().expect("session_ctx poisoned");
                Backend::compile_diagnostics_with_session(
                    &new_state.source,
                    &new_state.line_map,
                    &mut ctx,
                    file_id,
                )
            };
            client
                .publish_diagnostics(uri_clone, diagnostics, Some(version))
                .await;
        });
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        // Drop per-URI cached state.
        {
            let mut docs = self.docs.lock().expect("docs mutex poisoned");
            docs.remove(&uri);
        }
        // Drop per-URI type-cache rows so a future re-open starts clean.
        let file_id = self.file_id_for(&uri);
        let mut ctx = self.session_ctx.lock().expect("session_ctx poisoned");
        ctx.invalidate(file_id);
        // The `uri_file_ids` mapping is intentionally retained per
        // ADR-0057b §3.4 (monotonic FileId allocation across reopens).
        // `debounce_tokens` for the URI is left in place too — stale
        // entries are harmless (every new event re-keys them).
    }

    /// ADR-0057c §3.1 — hover handler.
    ///
    /// Resolves the identifier at the cursor from the shared
    /// `TypeCheckCtx` and returns a Markdown hover card with the
    /// inferred type. Returns `Ok(None)` for unknown identifiers,
    /// punctuation, or positions with no surrounding identifier.
    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        // Read the doc state (no long-held lock).
        let (source, line_map) = {
            let docs = self.docs.lock().expect("docs mutex poisoned");
            match docs.get(uri) {
                Some(s) => (s.source.clone(), s.line_map.clone()),
                None => return Ok(None),
            }
        };

        // Snapshot the type context (O(1) Arc clone per ADR-0056b).
        let ctx = self.session_ctx_snapshot();

        Ok(hover::resolve_hover(&source, &line_map, position, &ctx))
    }

    /// ADR-0057c §3.2 — completion handler.
    ///
    /// Returns PRELUDE functions + in-scope bindings + keywords,
    /// filtered by the identifier prefix at the cursor.
    async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        // Read the doc state.
        let (source, line_map) = {
            let docs = self.docs.lock().expect("docs mutex poisoned");
            match docs.get(uri) {
                Some(s) => (s.source.clone(), s.line_map.clone()),
                // No doc open yet — still return PRELUDE + keywords from
                // an empty context so clients connected before `did_open`
                // get a useful list.
                None => {
                    let ctx = self.session_ctx_snapshot();
                    let resp = completion::build_completion_response("", 0, &ctx);
                    return Ok(Some(resp));
                }
            }
        };

        // Convert LSP Position → byte offset.
        let byte_offset = line_map
            .position_to_byte(position)
            .map_or(0, |b| b as usize);

        let ctx = self.session_ctx_snapshot();
        let resp = completion::build_completion_response(&source, byte_offset, &ctx);
        Ok(Some(resp))
    }

    /// ADR-0057d §3.1 — prepareRename handler.
    ///
    /// Pre-flight check before a rename: returns `Some(Range)` covering
    /// the symbol if it is rename-able, or `None` if it is not (keyword,
    /// punctuation, unknown binding).
    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> LspResult<Option<PrepareRenameResponse>> {
        let uri = &params.text_document.uri;
        let position = params.position;

        let (source, line_map) = {
            let docs = self.docs.lock().expect("docs mutex poisoned");
            match docs.get(uri) {
                Some(s) => (s.source.clone(), s.line_map.clone()),
                None => return Ok(None),
            }
        };

        let ctx = self.session_ctx_snapshot();
        Ok(rename::prepare_rename(&source, &line_map, position, &ctx))
    }

    /// ADR-0057d §3.2 — rename handler.
    ///
    /// Returns a `WorkspaceEdit` with `TextEdit[]` replacing every
    /// occurrence of the symbol at `position` with `params.new_name`,
    /// or `None` if the symbol is not rename-able.
    ///
    /// Single-document scope only (ADR-0057d §4 non-goal). Cross-file
    /// rename deferred to ADR-0057e wave-3.
    async fn rename(&self, params: RenameParams) -> LspResult<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let position = params.text_document_position.position;
        let new_name = params.new_name;

        let (source, line_map) = {
            let docs = self.docs.lock().expect("docs mutex poisoned");
            match docs.get(&uri) {
                Some(s) => (s.source.clone(), s.line_map.clone()),
                None => return Ok(None),
            }
        };

        let ctx = self.session_ctx_snapshot();
        Ok(rename::rename_symbol(
            &source, &line_map, position, &new_name, &ctx, uri,
        ))
    }
}

// Note: the `Mutex<HashMap<...>>` field above carries `MirError` /
// `LoweringError` indirectly through the diagnostic conversion path
// only — the cached `DocState` itself does not own any error data.
// Re-export the error types at the module root so downstream test
// crates can build synthetic diagnostics without re-importing each
// upstream crate.
pub use cobrust_hir::LoweringError as ReExportLoweringError;
pub use cobrust_mir::MirError as ReExportMirError;
pub use cobrust_types::TypeError as ReExportTypeError;
