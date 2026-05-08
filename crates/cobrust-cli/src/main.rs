//! Cobrust CLI entrypoint (M10).
//!
//! Subcommand registry per ADR-0024 §"Public surface (binding)".
//! See `docs/agent/modules/cli.md` for the agent-facing spec.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::single_match_else)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::similar_names)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::needless_pass_by_ref_mut)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::format_push_string)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::if_not_else)]
#![allow(clippy::redundant_else)]
#![allow(clippy::result_large_err)]

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

mod build;
mod check;
mod exit_codes;
mod fmt;
mod new;
mod repl;
mod run;
mod test_runner;
mod translate;

#[derive(Parser, Debug)]
#[command(
    name = "cobrust",
    version,
    about = "Cobrust — Python ergonomics + Rust safety + AI-native compiler",
    long_about = None,
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Compile a `.cb` source file to an object or executable.
    Build {
        /// Input `.cb` file.
        file: PathBuf,
        /// Output path (defaults to `target/cobrust/<basename>{,.o}`).
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// What to emit: `obj` (relocatable `.o`) or `exe` (linked executable).
        #[arg(long, value_enum, default_value_t = EmitKindArg::Exe)]
        emit: EmitKindArg,
        /// Build with optimizations (M9 LLVM tier when available).
        #[arg(long)]
        release: bool,
        /// Override the host triple (e.g. `aarch64-apple-darwin`).
        #[arg(long)]
        target: Option<String>,
        /// Suppress informational stderr.
        #[arg(short, long)]
        quiet: bool,
    },
    /// Compile + invoke a `.cb` source file.
    Run {
        /// Input `.cb` file.
        file: PathBuf,
        /// Build with optimizations.
        #[arg(long)]
        release: bool,
        /// Override the host triple.
        #[arg(long)]
        target: Option<String>,
        /// Suppress informational stderr.
        #[arg(short, long)]
        quiet: bool,
    },
    /// Type-check a `.cb` source file (no codegen).
    Check {
        file: PathBuf,
        #[arg(short, long)]
        quiet: bool,
    },
    /// Format a `.cb` source file via the unparser.
    Fmt {
        file: PathBuf,
        /// Don't write; exit non-zero (5) if file would change.
        #[arg(long)]
        check: bool,
    },
    /// Translate a Python library into a Cobrust crate.
    Translate {
        /// Library name (looked up under `corpus/<library>/`).
        library: String,
        /// Output directory (defaults to `target/cobrust/crates/`).
        #[arg(long)]
        out_dir: Option<PathBuf>,
        #[arg(short, long)]
        quiet: bool,
    },
    /// Scaffold a new Cobrust package directory.
    New {
        /// Package name.
        name: String,
        /// Parent directory (defaults to cwd).
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Compile + run every `.cb` file under `tests/`.
    Test {
        #[arg(short, long)]
        quiet: bool,
    },
    /// Interactive REPL (M14 stub).
    Repl,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum EmitKindArg {
    Obj,
    Exe,
}

impl From<EmitKindArg> for build::EmitKind {
    fn from(a: EmitKindArg) -> Self {
        match a {
            EmitKindArg::Obj => build::EmitKind::Object,
            EmitKindArg::Exe => build::EmitKind::Executable,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let code: u8 = match cli.command {
        Command::Build {
            file,
            output,
            emit,
            release,
            target,
            quiet,
        } => build::run(
            &file,
            output.as_deref(),
            emit.into(),
            release,
            target.as_deref(),
            quiet,
        ),
        Command::Run {
            file,
            release,
            target,
            quiet,
        } => run::run(&file, release, target.as_deref(), quiet),
        Command::Check { file, quiet } => check::run(&file, quiet),
        Command::Fmt { file, check } => fmt::run(&file, check),
        Command::Translate {
            library,
            out_dir,
            quiet,
        } => translate::run(&library, out_dir.as_deref(), quiet),
        Command::New { name, path } => new::run(&name, path.as_deref()),
        Command::Test { quiet } => test_runner::run(quiet),
        Command::Repl => repl::run(),
    };
    ExitCode::from(code)
}
