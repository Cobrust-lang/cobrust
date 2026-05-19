//! Cobrust DAP server binary entrypoint (ADR-0059b wave-2).
//!
//! Stdio DAP server wrapping the [`cobrust_dap::Adapter`] handler. Editor
//! integrations (VSCode / Cursor / Neovim DAP / Emacs DAP-mode / …) launch
//! `cobrust-dap` as a child process and pipe DAP frames over stdin/stdout
//! per DAP §"Base Protocol" (Content-Length framing, JSON body).

use cobrust_dap::Adapter;
use cobrust_dap::run_stdio_loop;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let adapter = Adapter::new();
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    run_stdio_loop(adapter, stdin, stdout).await?;

    Ok(())
}
