//! Transitional `cobrust-lsp` standalone binary (ADR-0068 §4.2).
//!
//! v0.5.x editor extensions (extension v0.1.0 per ADR-0067) spawn
//! `cobrust-lsp` as a `$PATH` lookup; ADR-0068's subcommand collapse
//! (v0.6.0) moves the canonical entry to `cobrust lsp`. This shim
//! preserves the standalone binary name on `$PATH` so v0.1.0 extension
//! users do not break across the v0.5.x → v0.6.x compiler upgrade.
//!
//! Deleted at v0.7.0 per ADR-0068 §4.4. Extension v0.2.0 prefers
//! `cobrust lsp` directly; v0.1.0 extension + v0.7.0 compiler is a
//! documented breakage at the v0.7.0 release notes.
//!
//! Implementation: 2-line `main` calling `cobrust_lsp::run()`. The
//! `cobrust lsp` subcommand handler (`crates/cobrust-cli/src/lsp.rs`)
//! dispatches through the same lib entry, so behavior is byte-for-byte
//! identical between this shim and the subcommand.

fn main() -> std::process::ExitCode {
    match cobrust_lsp::run() {
        Ok(()) => std::process::ExitCode::from(0),
        Err(e) => {
            eprintln!("cobrust-lsp: {e}");
            std::process::ExitCode::from(3)
        }
    }
}
