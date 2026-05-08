//! `cobrust fmt` — format a `.cb` file via parse → unparse.
//!
//! The unparser (`cobrust_frontend::unparse`) is M1 surface and round-trips
//! the AST exactly per ADR-0003. M10 wires it as the formatter:
//!
//! - default mode: rewrite the file in place with the canonical form
//! - `--check` mode: exit non-zero (FMT_DIFF) if rewrite would change the
//!   file, leaving it untouched

use std::path::Path;

use cobrust_frontend::{parse_str, span::FileId, unparse};

use crate::exit_codes;

/// Run `cobrust fmt <file.cb> [--check]`.
pub fn run(file: &Path, check_only: bool) -> u8 {
    let user_source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cobrust fmt: cannot read {}: {e}", file.display());
            return exit_codes::USER_ERROR;
        }
    };
    // For `fmt` we parse the user source alone (no prelude); the unparse
    // round-trip is the user's text, not the prelude-prepended form.
    let source = user_source.clone();

    let module = match parse_str(&source, FileId::SYNTHETIC) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("cobrust fmt: parse error: {e:?}");
            return exit_codes::TYPE_ERROR;
        }
    };

    let formatted = unparse(&module);

    if check_only {
        if formatted == source {
            exit_codes::SUCCESS
        } else {
            eprintln!("cobrust fmt: file would be reformatted");
            exit_codes::FMT_DIFF
        }
    } else {
        match std::fs::write(file, &formatted) {
            Ok(()) => exit_codes::SUCCESS,
            Err(e) => {
                eprintln!("cobrust fmt: cannot write {}: {e}", file.display());
                exit_codes::USER_ERROR
            }
        }
    }
}
