//! Transitional `cobrust-dap` standalone binary (ADR-0068 §4.2).
//!
//! v0.5.x editor extensions and `cobrust debug --dap` paths spawn
//! `cobrust-dap` as a `$PATH` lookup; ADR-0068's subcommand collapse
//! (v0.6.0) moves the canonical entry to `cobrust dap`. This shim
//! preserves the standalone binary name on `$PATH` so existing
//! integrations do not break across the v0.5.x → v0.6.x compiler
//! upgrade.
//!
//! Deleted at v0.7.0 per ADR-0068 §4.4.
//!
//! Implementation: 2-line `main` calling `cobrust_dap::run()`. The
//! `cobrust dap` subcommand handler (`crates/cobrust-cli/src/dap.rs`)
//! dispatches through the same lib entry, so behavior is byte-for-byte
//! identical between this shim and the subcommand.

fn main() -> std::process::ExitCode {
    match cobrust_dap::run() {
        Ok(()) => std::process::ExitCode::from(0),
        Err(e) => {
            eprintln!("cobrust-dap: {e}");
            std::process::ExitCode::from(3)
        }
    }
}
