//! `cobrust lsp` — subcommand dispatch wrapper for the Cobrust LSP server.
//!
//! ADR-0068 §4.1: thin wrapper around [`cobrust_lsp::run`]. The lib
//! entry handles tokio runtime setup, tracing-subscriber init, and the
//! stdio LSP server loop; this file maps the `cobrust lsp` `Commands`
//! variant to that entry and translates any error into the CLI exit
//! code surface.
//!
//! No flags currently — the LSP server takes no arguments at v0.6.0.
//! Future flags (e.g. `--log-level`, `--port` for TCP transport) can
//! be added here without disturbing the lib entry.

use crate::exit_codes;

/// Run the Cobrust LSP stdio server.
///
/// Returns an exit code suitable for the CLI's `ExitCode::from(u8)`
/// dispatch: 0 on graceful disconnect, [`exit_codes::INTERNAL_PANIC`] on
/// runtime build failure or stdio loop error.
pub fn run() -> u8 {
    match cobrust_lsp::run() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("cobrust lsp: {e}");
            exit_codes::INTERNAL_PANIC
        }
    }
}
