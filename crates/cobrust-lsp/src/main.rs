//! Cobrust LSP server binary entrypoint (ADR-0057a wave-1).
//!
//! Stdio LSP server wrapping the [`cobrust_lsp::Backend`] tower-lsp
//! `LanguageServer` impl. Editor integrations (VSCode, Cursor,
//! Neovim, …) launch `cobrust-lsp` and pipe LSP frames over
//! stdin/stdout per LSP §"Base Protocol".

use cobrust_lsp::Backend;
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
