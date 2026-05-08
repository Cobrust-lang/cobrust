//! `cobrust check` — type-check-only subcommand.
//!
//! Runs lex → parse → HIR-lower → type-check. Emits diagnostics to
//! stderr; prints "ok" to stdout on success. No codegen.

use std::path::Path;

use cobrust_frontend::{parse_str, span::FileId};
use cobrust_hir::{Session, lower as hir_lower};
use cobrust_types::check as type_check;

use crate::exit_codes;

/// Run `cobrust check <file.cb>`.
///
/// Returns the appropriate [`exit_codes`] value.
pub fn run(file: &Path, quiet: bool) -> u8 {
    let user_source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cobrust check: cannot read {}: {e}", file.display());
            return exit_codes::USER_ERROR;
        }
    };
    let source = format!("{}{user_source}", crate::build::PRELUDE);

    let module = match parse_str(&source, FileId::SYNTHETIC) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("cobrust check: parse error: {e:?}");
            return exit_codes::TYPE_ERROR;
        }
    };

    let mut sess = Session::new();
    let hir = match hir_lower(&module, &mut sess) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("cobrust check: HIR lower error: {e:?}");
            return exit_codes::TYPE_ERROR;
        }
    };

    match type_check(&hir) {
        Ok(_) => {
            if !quiet {
                println!("ok");
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            eprintln!("cobrust check: type error: {e:?}");
            exit_codes::TYPE_ERROR
        }
    }
}
