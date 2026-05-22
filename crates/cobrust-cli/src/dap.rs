//! `cobrust dap` — subcommand dispatch wrapper for the Cobrust DAP server.
//!
//! ADR-0068 §4.1: thin wrapper around [`cobrust_dap::run`]. The lib
//! entry handles tokio runtime setup, tracing-subscriber init, and the
//! stdio DAP server loop; this file maps the `cobrust dap` `Commands`
//! variant to that entry and translates any error into the CLI exit
//! code surface.
//!
//! Independent of `cobrust debug --dap`: that path (ADR-0059c) is the
//! editor-side DAP launcher which forwards stdio to a sibling
//! `cobrust-dap` process. The `cobrust dap` subcommand IS the server
//! process — editors that previously spawned `cobrust-dap` directly
//! now spawn `cobrust dap` (or transitionally the `cobrust-dap` shim
//! binary, ADR-0068 §4.2, deleted at v0.7.0).

use crate::exit_codes;

/// Run the Cobrust DAP stdio server.
///
/// Returns an exit code suitable for the CLI's `ExitCode::from(u8)`
/// dispatch: 0 on graceful disconnect, [`exit_codes::INTERNAL_PANIC`] on
/// runtime build failure or stdio loop error.
pub fn run() -> u8 {
    match cobrust_dap::run() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("cobrust dap: {e}");
            exit_codes::INTERNAL_PANIC
        }
    }
}
