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
use cobrust_frontend::{PRELUDE, PRELUDE_LINE_COUNT};
use cobrust_types::{TypeCheckCtx, check_incremental};
use tower_lsp::Client;
use tower_lsp::LanguageServer;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams, CallHierarchyItem,
    CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    CallHierarchyServerCapability, CodeActionOrCommand, CodeActionParams,
    CodeActionProviderCapability, CodeActionResponse, CompletionOptions, CompletionParams,
    CompletionResponse, Diagnostic, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams,
    HoverProviderCapability, InitializeParams, InitializeResult, InitializedParams, InlayHint,
    InlayHintOptions, InlayHintParams, InlayHintServerCapabilities, MessageType, OneOf,
    PrepareRenameResponse, RenameOptions, RenameParams, SemanticToken, SemanticTokensDeltaParams,
    SemanticTokensFullDeltaResult, SemanticTokensFullOptions, SemanticTokensOptions,
    SemanticTokensParams, SemanticTokensResult, SemanticTokensServerCapabilities,
    ServerCapabilities, ServerInfo, TextDocumentContentChangeEvent, TextDocumentPositionParams,
    TextDocumentSyncCapability, TextDocumentSyncKind, Url, WorkDoneProgressOptions, WorkspaceEdit,
};

pub mod call_hierarchy;
pub mod code_action;
pub mod completion;
pub mod debounce;
pub mod diagnostic;
pub mod goto_def;
pub mod hover;
pub mod inlay;
pub mod rename;
pub mod semantic_tokens;
pub mod span_convert;

pub use call_hierarchy::{
    build_incoming_calls, build_incoming_calls_cross_file, build_outgoing_calls,
    build_outgoing_calls_cross_file, prepare_call_hierarchy,
};
pub use code_action::{
    build_code_actions, code_action_kind_for_fix_safety, code_action_kind_for_lowering_error,
    code_action_kind_for_mir_error, code_action_kind_for_type_error, fix_safety_from_code,
    fix_safety_from_diagnostic_data,
};
pub use completion::{
    build_completion_response, keyword_items, prefix_at_offset, prelude_items, scope_items,
};
pub use debounce::{DEFAULT_DEBOUNCE_MS, DebounceTokens};
pub use diagnostic::{
    lowering_error_to_diagnostic, mir_error_to_diagnostic, type_error_to_diagnostics,
};
pub use goto_def::resolve_definition;
pub use hover::{render_hover_markdown, resolve_hover, word_at_offset};
pub use inlay::{build_inlay_hints, resolve_inlay_hint};
pub use rename::{prepare_rename, rename_symbol, rename_symbol_cross_file};
pub use semantic_tokens::{build_semantic_tokens, build_semantic_tokens_delta, token_legend};
pub use span_convert::{LineMap, span_to_lsp_range};

/// Run the Cobrust LSP server over stdio.
///
/// ADR-0068 §4.1: this is the unified entry point both the `cobrust lsp`
/// subcommand (`crates/cobrust-cli/src/lsp.rs`) and the transitional
/// `cobrust-lsp` shim binary (`crates/cobrust-lsp-shim/src/main.rs`)
/// dispatch through. Calls into the wave-2.1 `Backend` per ADR-0057b
/// and serves until stdin EOF.
///
/// Initializes a `tracing` subscriber that writes to stderr (LSP stdout
/// is reserved for JSON-RPC frames). Returns `Ok(())` on graceful
/// client disconnect.
///
/// # Errors
///
/// Returns the underlying tokio runtime build error if the multi-thread
/// runtime cannot be created; otherwise never returns an error in
/// normal operation (the LSP `Server` loop exits on stdin EOF and the
/// run function returns `Ok(())`).
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .with_writer(std::io::stderr)
            .init();

        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();

        let (service, socket) = tower_lsp::LspService::new(Backend::new);
        tower_lsp::Server::new(stdin, stdout, socket)
            .serve(service)
            .await;
    });
    Ok(())
}

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
    /// Per-URI semantic-tokens cache for `textDocument/semanticTokens/
    /// full/delta` (ADR-0057g §3.1). Each entry stores the last
    /// emitted `result_id` and the delta-encoded token vec; the delta
    /// handler reads the cache, computes a diff against the new
    /// tokens, and writes the new cache before responding. A miss on
    /// `previous_result_id` falls back to the full response.
    semantic_tokens_cache: Mutex<HashMap<Url, (String, Vec<SemanticToken>)>>,
    /// Monotone counter for `result_id` allocation. Each emission
    /// allocates a unique id so the cache lookup is well-defined.
    /// `String` form keeps the LSP wire shape; opaque to clients.
    semantic_tokens_result_counter: Mutex<u64>,
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
            semantic_tokens_cache: Mutex::new(HashMap::new()),
            semantic_tokens_result_counter: Mutex::new(0),
        }
    }

    /// Allocate a fresh monotone `result_id` for semantic-tokens
    /// responses. Each call returns a stringified u64 unique within
    /// this `Backend`'s lifetime.
    fn next_semantic_tokens_result_id(&self) -> String {
        let mut counter = self
            .semantic_tokens_result_counter
            .lock()
            .expect("semantic_tokens_result_counter poisoned");
        *counter = counter.saturating_add(1);
        format!("st-{counter}")
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
    /// Per F50 (2026-05-22) the LSP path prepends [`cobrust_frontend::
    /// PRELUDE`] to `source` BEFORE invoking the frontend, matching
    /// `crates/cobrust-cli/src/check.rs:36`. Without this every
    /// `print(...)` / `range(...)` / `parse_int(...)` callsite lit up
    /// as a `lower-unknown-name` red squiggle in Cursor while
    /// `cobrust check <file>` reported `ok`. Diagnostic spans emerging
    /// from the pipeline are in **composed-source** byte offsets;
    /// [`shift_diagnostic_into_user_coords`] subtracts
    /// [`cobrust_frontend::PRELUDE_LINE_COUNT`] from each
    /// `Diagnostic.range.{start,end}.line` so the LSP wire shape
    /// surfaces user-coordinate lines. Any diagnostic whose final line
    /// would underflow (i.e., the span lay inside the synthetic PRELUDE
    /// region) is filtered out as a defensive measure — PRELUDE stubs
    /// are always well-typed by construction.
    ///
    /// The caller-supplied `line_map` argument is no longer consulted
    /// for span conversion (the conversion now runs against a freshly
    /// built composed-source LineMap inside this function); the
    /// parameter is kept for source-API stability with existing tests.
    ///
    /// Each stage's error variants are mapped to LSP `Diagnostic`s
    /// via the `From`-impls in [`diagnostic`]. Wave-1 emits `Error`
    /// severity only per ADR-0057a §5.
    ///
    /// Wave-1 stateless variant kept for snapshot tests + smoke tools.
    /// Wave-2.1's stateful path is [`Self::compile_diagnostics_with_session`].
    pub fn compile_diagnostics(source: &str, _line_map: &LineMap) -> Vec<Diagnostic> {
        use cobrust_frontend::parse_str;

        // F50: prepend the synthetic PRELUDE so intrinsic names
        // (`print`, `range`, `parse_int`, ...) resolve identically to
        // the `cobrust check` CLI path. Build a composed-source
        // LineMap so `span_to_lsp_range` lookups land in composed
        // coordinates; we shift each emitted diagnostic back into
        // user coordinates via `shift_diagnostic_into_user_coords` at
        // the end.
        let composed = format!("{PRELUDE}{source}");
        let composed_line_map = LineMap::from_source(&composed);
        let mut diagnostics: Vec<Diagnostic> = Vec::new();

        // Stage 1: parse.
        let ast_module = match parse_str(&composed, FileId::SYNTHETIC) {
            Ok(m) => m,
            Err(err) => {
                diagnostics.push(diagnostic::frontend_error_to_diagnostic(
                    &err,
                    &composed_line_map,
                ));
                return shift_diagnostics_into_user_coords(diagnostics);
            }
        };

        // Stage 2: HIR lowering.
        let mut hir_sess = cobrust_hir::lower::Session::new();
        let hir_module = match cobrust_hir::lower::lower(&ast_module, &mut hir_sess) {
            Ok(m) => m,
            Err(err) => {
                diagnostics.push(diagnostic::lowering_error_to_diagnostic(
                    &err,
                    &composed_line_map,
                ));
                return shift_diagnostics_into_user_coords(diagnostics);
            }
        };

        // Stage 3: type-check.
        if let Err(err) = cobrust_types::check(&hir_module) {
            diagnostics.extend(diagnostic::type_error_to_diagnostics(
                &err,
                &composed_line_map,
            ));
        }

        shift_diagnostics_into_user_coords(diagnostics)
    }

    /// Wave-2.1 stateful pipeline per ADR-0057b §3.4.
    ///
    /// Calls `TypeCheckCtx::invalidate(file_id)` BEFORE re-checking so
    /// stale rows from the previous version are dropped; then runs
    /// `check_incremental(&mut ctx, &hir, file_id)` which merges fresh
    /// types back into the shared ctx. Diagnostics are produced from
    /// the same error path as wave-1 — only the symbol-table reuse is
    /// new.
    ///
    /// Per F50 (2026-05-22) prepends the same synthetic PRELUDE as
    /// [`Self::compile_diagnostics`] before parsing, shifts emitted
    /// diagnostic ranges back into user coordinates, and filters any
    /// span that landed inside the PRELUDE prefix.
    pub fn compile_diagnostics_with_session(
        source: &str,
        _line_map: &LineMap,
        ctx: &mut TypeCheckCtx,
        file_id: u32,
    ) -> Vec<Diagnostic> {
        use cobrust_frontend::parse_str;

        // F50: prepend PRELUDE for CLI-parity name resolution. See
        // [`Self::compile_diagnostics`] for the rationale; this path
        // applies the same construction so live `did_change` updates
        // (which call this function via the debounced pipeline) see
        // the same name table.
        let composed = format!("{PRELUDE}{source}");
        let composed_line_map = LineMap::from_source(&composed);
        let mut diagnostics: Vec<Diagnostic> = Vec::new();

        // Drop stale type-cache rows for this file BEFORE re-checking
        // (per ADR-0057b §3.4 step 3).
        ctx.invalidate(file_id);

        // Use the URI's `FileId` for span tracking so future cross-file
        // queries don't collide.
        let frontend_file_id = FileId(file_id);
        let ast_module = match parse_str(&composed, frontend_file_id) {
            Ok(m) => m,
            Err(err) => {
                diagnostics.push(diagnostic::frontend_error_to_diagnostic(
                    &err,
                    &composed_line_map,
                ));
                return shift_diagnostics_into_user_coords(diagnostics);
            }
        };

        let mut hir_sess = cobrust_hir::lower::Session::new();
        let hir_module = match cobrust_hir::lower::lower(&ast_module, &mut hir_sess) {
            Ok(m) => m,
            Err(err) => {
                diagnostics.push(diagnostic::lowering_error_to_diagnostic(
                    &err,
                    &composed_line_map,
                ));
                return shift_diagnostics_into_user_coords(diagnostics);
            }
        };

        // Stage 3: incremental type-check merges fresh rows into ctx.
        if let Err(err) = check_incremental(ctx, &hir_module, file_id) {
            diagnostics.extend(diagnostic::type_error_to_diagnostics(
                &err,
                &composed_line_map,
            ));
        }

        shift_diagnostics_into_user_coords(diagnostics)
    }

    /// Apply LSP content-change events (incremental or full-replace) to
    /// a source string. Returns the spliced `String`.
    //
    // (Helpers `shift_diagnostics_into_user_coords` /
    // `shift_diagnostic_into_user_coords` live as module-private free
    // functions below the impl block.)
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

/// F50 helper: shift every diagnostic's `Range` from composed-source
/// coordinates (PRELUDE + user) back into user-source coordinates.
///
/// Per `crates/cobrust-frontend/src/prelude.rs` the synthetic PRELUDE
/// always ends with a `\n`, so user content begins at line index
/// [`cobrust_frontend::PRELUDE_LINE_COUNT`] of the composed source.
/// Subtracting that constant from each `range.{start,end}.line` is a
/// pure line-shift; LSP `character` (UTF-16 column) is unchanged because
/// the PRELUDE ends at column 0 of the next line.
///
/// Diagnostics whose `start.line` lies inside the PRELUDE prefix are
/// filtered out as a defensive measure — PRELUDE stubs are always
/// well-typed by construction, so a span landing there indicates either
/// (a) an internal compiler bug in the PRELUDE source itself, or
/// (b) a name-conflict where the user shadowed a PRELUDE symbol and
/// the error referenced the PRELUDE declaration; in both cases
/// surfacing a phantom span in the user's editor view would be more
/// confusing than dropping it.
fn shift_diagnostics_into_user_coords(diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    diagnostics
        .into_iter()
        .filter_map(shift_diagnostic_into_user_coords)
        .collect()
}

/// Single-diagnostic shift; returns `None` when the span lay inside
/// the synthetic PRELUDE prefix (see [`shift_diagnostics_into_user_coords`]
/// for the policy rationale).
fn shift_diagnostic_into_user_coords(mut diag: Diagnostic) -> Option<Diagnostic> {
    let prelude_lines = PRELUDE_LINE_COUNT;
    if diag.range.start.line < prelude_lines {
        // Inside PRELUDE prefix — should never happen for well-formed
        // user source. Filter rather than surface a confusing range.
        return None;
    }
    diag.range.start.line -= prelude_lines;
    diag.range.end.line = diag.range.end.line.saturating_sub(prelude_lines);
    // related_information ranges (suggestion locations) also live in
    // composed coordinates. Shift them too so quick-fix overlays land
    // on the right user line.
    if let Some(related) = diag.related_information.as_mut() {
        for info in related.iter_mut() {
            info.location.range.start.line =
                info.location.range.start.line.saturating_sub(prelude_lines);
            info.location.range.end.line =
                info.location.range.end.line.saturating_sub(prelude_lines);
        }
    }
    // thiserror-rendered `#[error("... at {span}")]` messages embed the
    // raw byte offsets as `file#K@N..M`. Those offsets are in composed-
    // source coordinates; subtract `PRELUDE_BYTE_LEN` so the message
    // surface matches the LSP `Range` user coordinates. Returns the
    // original string unchanged when no `file#K@...` pattern is present.
    diag.message = shift_offsets_in_message(&diag.message);
    Some(diag)
}

/// Subtract [`cobrust_frontend::PRELUDE_BYTE_LEN`] from every
/// `file#K@N..M` byte-offset pair embedded in `msg`. Used to keep
/// thiserror-rendered error messages in user-source coordinates after
/// the F50 PRELUDE-prepend.
///
/// State machine: scan for the literal prefix `file#`, then digits, `@`,
/// digits, `..`, digits. Anything that doesn't match exactly is copied
/// through verbatim, so identifiers that happen to contain `@` or `..`
/// elsewhere in the message are unaffected.
fn shift_offsets_in_message(msg: &str) -> String {
    use cobrust_frontend::PRELUDE_BYTE_LEN;

    let bytes = msg.as_bytes();
    let mut out = String::with_capacity(msg.len());
    let mut i = 0usize;
    while i < bytes.len() {
        // Try to match `file#`.
        if bytes[i..].starts_with(b"file#") {
            // Find `@` after digits.
            let after_file_hash = i + b"file#".len();
            let mut k = after_file_hash;
            while k < bytes.len() && bytes[k].is_ascii_digit() {
                k += 1;
            }
            if k > after_file_hash && k < bytes.len() && bytes[k] == b'@' {
                // After `@`, parse `START..END`.
                let after_at = k + 1;
                let mut s = after_at;
                while s < bytes.len() && bytes[s].is_ascii_digit() {
                    s += 1;
                }
                if s > after_at && s + 1 < bytes.len() && &bytes[s..s + 2] == b".." {
                    let after_dotdot = s + 2;
                    let mut e = after_dotdot;
                    while e < bytes.len() && bytes[e].is_ascii_digit() {
                        e += 1;
                    }
                    if e > after_dotdot {
                        // Parse the two offsets, subtract PRELUDE_BYTE_LEN,
                        // emit the shifted form.
                        let start_offset: u32 = msg[after_at..s].parse().unwrap_or(0);
                        let end_offset: u32 = msg[after_dotdot..e].parse().unwrap_or(0);
                        out.push_str(&msg[i..after_at]); // "file#K@"
                        out.push_str(&start_offset.saturating_sub(PRELUDE_BYTE_LEN).to_string());
                        out.push_str("..");
                        out.push_str(&end_offset.saturating_sub(PRELUDE_BYTE_LEN).to_string());
                        i = e;
                        continue;
                    }
                }
            }
        }
        // Default: copy the current byte through. UTF-8 multi-byte
        // sequences cannot start with an ASCII byte we care about, so
        // byte-by-byte copy preserves UTF-8 well-formedness.
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

#[cfg(test)]
#[allow(
    clippy::items_after_test_module,
    reason = "F50 fix inserted offset-shift tests mid-file; lib internals follow"
)]
mod offset_shift_tests {
    use super::shift_offsets_in_message;

    #[test]
    fn shifts_single_span_in_message() {
        // PRELUDE_BYTE_LEN is a fixed constant; assert behavior by
        // pinning to its current value. If the PRELUDE grows the
        // assertion will need re-pinning — that's the desired signal.
        let prelude_len = cobrust_frontend::PRELUDE_BYTE_LEN;
        let composed = format!(
            "unknown name `print` at file#0@{}..{}",
            prelude_len + 22,
            prelude_len + 27
        );
        let shifted = shift_offsets_in_message(&composed);
        assert_eq!(shifted, "unknown name `print` at file#0@22..27");
    }

    #[test]
    fn leaves_non_matching_text_alone() {
        let input = "no spans here, just text with @ and ..";
        assert_eq!(shift_offsets_in_message(input), input);
    }

    #[test]
    fn handles_multiple_spans() {
        let prelude_len = cobrust_frontend::PRELUDE_BYTE_LEN;
        let composed = format!(
            "between file#1@{}..{} and file#2@{}..{}",
            prelude_len + 1,
            prelude_len + 5,
            prelude_len + 10,
            prelude_len + 20,
        );
        let shifted = shift_offsets_in_message(&composed);
        assert_eq!(shifted, "between file#1@1..5 and file#2@10..20");
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
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                })),
                // ADR-0057e §3.1 — goto-definition capability.
                definition_provider: Some(OneOf::Left(true)),
                // ADR-0057e §3.2 — codeAction capability (FixSafety-gated).
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                // ADR-0057f §3.1 + ADR-0057g §3.2 — inlay hint with resolve.
                // Wave-5 flips from `OneOf::Left(true)` to the Options form so
                // `resolve_provider: true` advertises the `inlayHint/resolve`
                // path; clients pre-flight resolve before requesting tooltips.
                inlay_hint_provider: Some(OneOf::Right(InlayHintServerCapabilities::Options(
                    InlayHintOptions {
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                        resolve_provider: Some(true),
                    },
                ))),
                // ADR-0057f §3.2 + ADR-0057g §3.1 — semantic tokens with
                // delta support. Wave-5 flips `full` from `Bool(true)` to
                // `Delta { delta: Some(true) }` so clients call the delta
                // path after the first full response, dropping wire bytes
                // from O(file) to O(edit).
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            work_done_progress_options: WorkDoneProgressOptions::default(),
                            legend: semantic_tokens::token_legend(),
                            range: Some(false),
                            full: Some(SemanticTokensFullOptions::Delta { delta: Some(true) }),
                        },
                    ),
                ),
                // ADR-0057f §3.3 — call hierarchy (prepare + incoming + outgoing).
                // ADR-0057g §3.3 — wave-5 broadens incoming + outgoing to walk
                // every OPEN document via `Backend.documents` (cross-file).
                call_hierarchy_provider: Some(CallHierarchyServerCapability::Simple(true)),
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
                "cobrust-lsp wave-5 initialized (ADR-0057g v1.3: semantic-tokens delta + inlayHint/resolve + cross-file call hierarchy — feature-complete)",
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
            if let Some(s) = docs.get(uri) {
                (s.source.clone(), s.line_map.clone())
            } else {
                // No doc open yet — still return PRELUDE + keywords from
                // an empty context so clients connected before `did_open`
                // get a useful list.
                let ctx = self.session_ctx_snapshot();
                let resp = completion::build_completion_response("", 0, &ctx);
                return Ok(Some(resp));
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
    /// ADR-0057e wave-3 extends this to walk OTHER open documents in
    /// `self.docs` and aggregate their `TextEdit`s into the
    /// `WorkspaceEdit.changes` map. Honest scope: cross-file rename is
    /// LIMITED to documents currently OPEN in the LSP session;
    /// filesystem-walk workspace search is deferred to a follow-up
    /// sub-ADR (ADR-0057e §4 non-goal).
    async fn rename(&self, params: RenameParams) -> LspResult<Option<WorkspaceEdit>> {
        let primary_uri = params.text_document_position.text_document.uri.clone();
        let position = params.text_document_position.position;
        let new_name = params.new_name;

        // Snapshot the primary doc + every OTHER open doc under a single
        // lock, then release before invoking the rename. Avoids holding
        // the lock across the (synchronous but potentially long) word
        // scan over cross-file sources.
        let (primary_source, primary_line_map, other_docs) = {
            let docs = self.docs.lock().expect("docs mutex poisoned");
            let Some(primary) = docs.get(&primary_uri) else {
                return Ok(None);
            };
            let primary_source = primary.source.clone();
            let primary_line_map = primary.line_map.clone();
            let other_docs: Vec<(Url, String, LineMap)> = docs
                .iter()
                .filter_map(|(uri, doc)| {
                    if *uri == primary_uri {
                        None
                    } else {
                        Some((uri.clone(), doc.source.clone(), doc.line_map.clone()))
                    }
                })
                .collect();
            (primary_source, primary_line_map, other_docs)
        };

        let ctx = self.session_ctx_snapshot();
        Ok(rename::rename_symbol_cross_file(
            &primary_source,
            &primary_line_map,
            position,
            &new_name,
            &ctx,
            primary_uri,
            &other_docs,
        ))
    }

    /// ADR-0057e §3.1 — go-to-definition handler.
    ///
    /// Returns `Some(GotoDefinitionResponse::Scalar(Location))` with
    /// the def-site of the symbol under the cursor, or `None` if the
    /// cursor is not on a known identifier (keyword, punctuation,
    /// unbound).
    ///
    /// Honest scope: wave-3 uses same-document word-scan only —
    /// cross-file def-site indexing via HIR `DefId` span map deferred
    /// to wave-4 (ADR-0057e §3.1 + §4).
    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let position = params.text_document_position_params.position;

        let (source, line_map) = {
            let docs = self.docs.lock().expect("docs mutex poisoned");
            match docs.get(&uri) {
                Some(s) => (s.source.clone(), s.line_map.clone()),
                None => return Ok(None),
            }
        };

        let ctx = self.session_ctx_snapshot();
        Ok(goto_def::resolve_definition(
            &source, &line_map, position, &ctx, uri,
        ))
    }

    /// ADR-0057e §3.2 — codeAction handler (FixSafety-tier gated).
    ///
    /// For each `Diagnostic` in `params.context.diagnostics`, emits a
    /// `CodeAction` whose `kind` is determined by ADR-0062 §3.5 tier
    /// gating (via [`code_action::code_action_kind_for_fix_safety`]).
    /// `BehaviorPreserving` + `LocalEdit` tiers attach a
    /// `WorkspaceEdit` with the suggestion as replacement text; other
    /// emitted tiers (`ApiChanging`, `FormatOnly`) attach a CodeAction
    /// with title-only (message-only). Tiers that map to `None`
    /// (`TargetChanging`, `RequiresHumanReview`) emit no CodeAction at
    /// all — the diagnostic stays message-only.
    async fn code_action(&self, params: CodeActionParams) -> LspResult<Option<CodeActionResponse>> {
        let uri = params.text_document.uri.clone();
        let actions: Vec<CodeActionOrCommand> =
            code_action::build_code_actions(&params.context.diagnostics, &uri);
        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(actions))
        }
    }

    /// ADR-0057f §3.1 — inlayHint handler (Phase J wave-4).
    ///
    /// Returns the inline type + parameter-name hints visible inside
    /// `params.range` for the document at `params.text_document.uri`.
    /// Returns `Ok(None)` for unknown URIs.
    async fn inlay_hint(&self, params: InlayHintParams) -> LspResult<Option<Vec<InlayHint>>> {
        let uri = &params.text_document.uri;
        let (source, line_map) = {
            let docs = self.docs.lock().expect("docs mutex poisoned");
            match docs.get(uri) {
                Some(s) => (s.source.clone(), s.line_map.clone()),
                None => return Ok(None),
            }
        };
        let ctx = self.session_ctx_snapshot();
        let hints = inlay::build_inlay_hints(&source, &line_map, params.range, &ctx);
        Ok(Some(hints))
    }

    /// ADR-0057f §3.2 + ADR-0057g §3.1 — semantic-tokens full-document
    /// handler.
    ///
    /// Wave-5 extends the response with a fresh `result_id` and writes
    /// the new `(result_id, tokens)` pair into the per-URI cache so a
    /// subsequent `semanticTokens/full/delta` can diff against it.
    /// Returns `Ok(None)` for unknown URIs.
    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> LspResult<Option<SemanticTokensResult>> {
        let uri = &params.text_document.uri;
        let (source, line_map) = {
            let docs = self.docs.lock().expect("docs mutex poisoned");
            match docs.get(uri) {
                Some(s) => (s.source.clone(), s.line_map.clone()),
                None => return Ok(None),
            }
        };
        let mut tokens = semantic_tokens::build_semantic_tokens(&source, &line_map);
        let result_id = self.next_semantic_tokens_result_id();
        tokens.result_id = Some(result_id.clone());
        // Cache the (result_id, tokens) pair for the upcoming delta request.
        {
            let mut cache = self
                .semantic_tokens_cache
                .lock()
                .expect("semantic_tokens_cache poisoned");
            cache.insert(uri.clone(), (result_id, tokens.data.clone()));
        }
        Ok(Some(SemanticTokensResult::Tokens(tokens)))
    }

    /// ADR-0057g §3.1 — `textDocument/semanticTokens/full/delta` handler.
    ///
    /// Reads `(previous_result_id, previous_tokens)` from the per-URI
    /// cache. If the client's `previous_result_id` matches the cached
    /// id, computes the minimal `SemanticTokensEdit` vec via the
    /// `build_semantic_tokens_delta` helper. Otherwise falls back to
    /// the full response (per ADR-0057g §4 honest-scope: no graceful
    /// partial-synthesis when the cache misses).
    async fn semantic_tokens_full_delta(
        &self,
        params: SemanticTokensDeltaParams,
    ) -> LspResult<Option<SemanticTokensFullDeltaResult>> {
        let uri = &params.text_document.uri;
        let (source, line_map) = {
            let docs = self.docs.lock().expect("docs mutex poisoned");
            match docs.get(uri) {
                Some(s) => (s.source.clone(), s.line_map.clone()),
                None => return Ok(None),
            }
        };

        let (cached_id, prev_tokens) = {
            let cache = self
                .semantic_tokens_cache
                .lock()
                .expect("semantic_tokens_cache poisoned");
            cache.get(uri).map_or((None, None), |(id, toks)| {
                (Some(id.clone()), Some(toks.clone()))
            })
        };

        let new_id = self.next_semantic_tokens_result_id();
        let prev_id_arg: Option<&str> = Some(params.previous_result_id.as_str());
        let cached_id_arg: Option<&str> = cached_id.as_deref();
        let prev_tokens_arg: Option<&[SemanticToken]> = prev_tokens.as_deref();

        let result = semantic_tokens::build_semantic_tokens_delta(
            &source,
            &line_map,
            prev_id_arg,
            cached_id_arg,
            prev_tokens_arg,
            new_id.clone(),
        );

        // Update the cache with the freshly computed token vec so the
        // next delta request can diff against this one. We have to
        // recompute the new tokens because the helper returns a delta
        // (which doesn't carry the full new stream).
        let new_tokens = semantic_tokens::build_semantic_tokens(&source, &line_map);
        {
            let mut cache = self
                .semantic_tokens_cache
                .lock()
                .expect("semantic_tokens_cache poisoned");
            cache.insert(uri.clone(), (new_id, new_tokens.data));
        }

        Ok(Some(result))
    }

    /// ADR-0057f §3.3 — `textDocument/prepareCallHierarchy` handler.
    ///
    /// Resolves the symbol at the cursor to a `CallHierarchyItem` if
    /// it names a same-document fn def, or returns `Ok(None)` if the
    /// cursor is not on a known fn name.
    async fn prepare_call_hierarchy(
        &self,
        params: CallHierarchyPrepareParams,
    ) -> LspResult<Option<Vec<CallHierarchyItem>>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let position = params.text_document_position_params.position;

        let (source, line_map) = {
            let docs = self.docs.lock().expect("docs mutex poisoned");
            match docs.get(&uri) {
                Some(s) => (s.source.clone(), s.line_map.clone()),
                None => return Ok(None),
            }
        };
        let ctx = self.session_ctx_snapshot();
        Ok(call_hierarchy::prepare_call_hierarchy(
            &source, &line_map, position, &ctx, uri,
        ))
    }

    /// ADR-0057f §3.3 + ADR-0057g §3.3 — `callHierarchy/incomingCalls`
    /// handler.
    ///
    /// Wave-5 broadens the walk to every OPEN document in
    /// `self.docs`: the wave-4 same-doc result is concatenated with
    /// the result of walking each other open URI for callers of the
    /// target fn. Honest scope: closed files are invisible
    /// (consistent with wave-3 cross-file rename).
    async fn incoming_calls(
        &self,
        params: CallHierarchyIncomingCallsParams,
    ) -> LspResult<Option<Vec<CallHierarchyIncomingCall>>> {
        let uri = params.item.uri.clone();
        let (source, line_map, other_docs) = {
            let docs = self.docs.lock().expect("docs mutex poisoned");
            let Some(primary) = docs.get(&uri) else {
                return Ok(None);
            };
            let primary_source = primary.source.clone();
            let primary_line_map = primary.line_map.clone();
            let other: Vec<(Url, String, LineMap)> = docs
                .iter()
                .filter_map(|(u, doc)| {
                    if *u == uri {
                        None
                    } else {
                        Some((u.clone(), doc.source.clone(), doc.line_map.clone()))
                    }
                })
                .collect();
            (primary_source, primary_line_map, other)
        };
        let calls = call_hierarchy::build_incoming_calls_cross_file(
            &source,
            &line_map,
            &params.item,
            &other_docs,
        );
        Ok(Some(calls))
    }

    /// ADR-0057f §3.3 + ADR-0057g §3.3 — `callHierarchy/outgoingCalls`
    /// handler.
    ///
    /// Wave-5 resolves callees against every OPEN document so a fn
    /// calling a helper defined in another file surfaces the correct
    /// `to` location instead of a zero-span placeholder.
    async fn outgoing_calls(
        &self,
        params: CallHierarchyOutgoingCallsParams,
    ) -> LspResult<Option<Vec<CallHierarchyOutgoingCall>>> {
        let uri = params.item.uri.clone();
        let (source, line_map, other_docs) = {
            let docs = self.docs.lock().expect("docs mutex poisoned");
            let Some(primary) = docs.get(&uri) else {
                return Ok(None);
            };
            let primary_source = primary.source.clone();
            let primary_line_map = primary.line_map.clone();
            let other: Vec<(Url, String, LineMap)> = docs
                .iter()
                .filter_map(|(u, doc)| {
                    if *u == uri {
                        None
                    } else {
                        Some((u.clone(), doc.source.clone(), doc.line_map.clone()))
                    }
                })
                .collect();
            (primary_source, primary_line_map, other)
        };
        let calls = call_hierarchy::build_outgoing_calls_cross_file(
            &source,
            &line_map,
            &params.item,
            &other_docs,
        );
        Ok(Some(calls))
    }

    /// ADR-0057g §3.2 — `inlayHint/resolve` handler.
    ///
    /// Lazily fills in the `tooltip` field on a hint that was emitted
    /// with `data` populated by `build_inlay_hints`. The resolve
    /// handler re-uses the shared `TypeCheckCtx` snapshot to render
    /// a Markdown tooltip with the inferred type (for `let` hints)
    /// or the callee signature (for param-name hints).
    async fn inlay_hint_resolve(&self, params: InlayHint) -> LspResult<InlayHint> {
        let ctx = self.session_ctx_snapshot();
        Ok(inlay::resolve_inlay_hint(params, &ctx))
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
