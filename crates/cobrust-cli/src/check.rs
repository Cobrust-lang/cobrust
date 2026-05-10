//! `cobrust check` — type-check-only subcommand.
//!
//! Runs lex → parse → HIR-lower → type-check. Emits diagnostics to
//! stderr via the `error_ux` layer; prints "ok" to stdout on success.
//! No codegen.

use std::path::Path;

use cobrust_frontend::{parse_str, span::FileId};
use cobrust_hir::{Session, lower as hir_lower};
use cobrust_types::check as type_check;

use crate::error_ux::UserError;
use crate::exit_codes;

/// Run `cobrust check <file.cb>`.
///
/// Returns the appropriate [`exit_codes`] value.
pub fn run(file: &Path, quiet: bool) -> u8 {
    let user_source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            // File I/O failure → USER_ERROR (1), not TYPE_ERROR (2).
            // ADR-0024 §"Exit-code scheme": bad input path is a user error.
            let ue = UserError::Syntax {
                file: file.to_path_buf(),
                line: 0,
                col: 0,
                msg: format!("cannot read file: {e}"),
                hint: Some("check the path and permissions".to_owned()),
            };
            eprintln!("{ue}");
            return exit_codes::USER_ERROR;
        }
    };
    let source = format!("{}{user_source}", crate::build::PRELUDE);

    let module = match parse_str(&source, FileId::SYNTHETIC) {
        Ok(m) => m,
        Err(e) => {
            // Convert the frontend error through the UX layer and patch in
            // the real file path (the From impl uses "<source>" as a
            // placeholder; callers with a real path should overwrite it).
            let mut ue = UserError::from(e);
            set_ue_file(&mut ue, file);
            return ue.report_and_exit_code();
        }
    };

    let mut sess = Session::new();
    let hir = match hir_lower(&module, &mut sess) {
        Ok(h) => h,
        Err(e) => {
            let mut ue = UserError::from(e);
            set_ue_file(&mut ue, file);
            return ue.report_and_exit_code();
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
            let mut ue = UserError::from(e);
            set_ue_file(&mut ue, file);
            ue.report_and_exit_code()
        }
    }
}

/// Patch the `file` field of a `UserError::Syntax` or `UserError::Type`
/// variant to the real source path.
fn set_ue_file(ue: &mut UserError, file: &Path) {
    match ue {
        UserError::Syntax { file: f, .. } | UserError::Type { file: f, .. } => {
            *f = file.to_path_buf();
        }
        UserError::Runtime { .. } | UserError::Internal { .. } => {}
    }
}
