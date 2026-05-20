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
//! Public surface (wave-1):
//! - [`Backend`] — the `tower_lsp::LanguageServer` implementation.
//! - [`span_convert`] — `Span` → LSP `Range` via `LineMap`.
//! - [`diagnostic`] — `From<&TypeError/&MirError/&LoweringError> for
//!   lsp_types::Diagnostic` impls.
//!
//! Wave-2+ extends this surface to hover / completion / definition /
//! rename / codeAction per ADR-0057 §4.

#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::enum_glob_use)]

use std::collections::HashMap;
use std::sync::Mutex;

use cobrust_frontend::span::FileId;
use tower_lsp::Client;
use tower_lsp::LanguageServer;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    Diagnostic, DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    InitializeParams, InitializeResult, InitializedParams, MessageType, ServerCapabilities,
    ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind, Url,
};

pub mod code_action;
pub mod diagnostic;
pub mod span_convert;

pub use code_action::{
    code_action_kind_for_fix_safety, code_action_kind_for_lowering_error,
    code_action_kind_for_mir_error, code_action_kind_for_type_error, fix_safety_from_code,
};
pub use diagnostic::{
    lowering_error_to_diagnostic, mir_error_to_diagnostic, type_error_to_diagnostics,
};
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
/// `did_open`, `did_change`, `did_close`, `shutdown`.
pub struct Backend {
    /// Tower-LSP client handle used to push notifications
    /// (`publish_diagnostics`, `log_message`, etc.).
    client: Client,
    /// URI → per-document state. Wrapped in a `Mutex` because
    /// `tower-lsp` calls handlers from multiple tokio tasks and the
    /// LSP `Backend` is `Sync`.
    docs: Mutex<HashMap<Url, DocState>>,
}

impl Backend {
    /// Construct a new `Backend` bound to a `Client`.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            docs: Mutex::new(HashMap::new()),
        }
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
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> LspResult<InitializeResult> {
        // ADR-0057a §4: wave-1 publishes full diagnostic vector per
        // URI on every `did_change`. Use FULL sync (Phase J+ may
        // upgrade to INCREMENTAL after the §6 LineMap helper proves
        // out the byte-offset accounting under partial edits).
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
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
                "cobrust-lsp wave-1 initialized (ADR-0057a textDocument/publishDiagnostics)",
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
        let state = DocState::new(source, version);
        let diagnostics = Backend::compile_diagnostics(&state.source, &state.line_map);
        {
            let mut docs = self.docs.lock().expect("docs mutex poisoned");
            docs.insert(uri.clone(), state);
        }
        self.client
            .publish_diagnostics(uri, diagnostics, Some(version))
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        // Wave-1 uses FULL sync, so the last content-change carries
        // the entire source text.
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        let Some(change) = params.content_changes.into_iter().last() else {
            return;
        };
        let state = DocState::new(change.text, version);
        let diagnostics = Backend::compile_diagnostics(&state.source, &state.line_map);
        {
            let mut docs = self.docs.lock().expect("docs mutex poisoned");
            docs.insert(uri.clone(), state);
        }
        self.client
            .publish_diagnostics(uri, diagnostics, Some(version))
            .await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        let mut docs = self.docs.lock().expect("docs mutex poisoned");
        docs.remove(&uri);
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
