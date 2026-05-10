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

mod add;
mod build;
mod check;
pub mod error_ux;
mod exit_codes;
mod fmt;
mod new;
mod pkg_build;
mod repl;
mod report_bug;
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
    /// Compile a `.cb` source file or a package (when omitted, walks up to the
    /// nearest `cobrust.toml`) to an object or executable.
    Build {
        /// Input `.cb` file or package directory. Omit to detect a package
        /// from the current working directory.
        file: Option<PathBuf>,
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
    /// Add a dependency to the nearest `cobrust.toml` (M12).
    Add {
        /// Dependency name (must match the manifest schema).
        name: String,
        /// Path source: `cobrust add <name> --path ../foo`.
        #[arg(long, conflicts_with_all = ["git", "version"])]
        path: Option<PathBuf>,
        /// Git source: `cobrust add <name> --git URL --rev REV`.
        #[arg(long, requires = "rev", conflicts_with_all = ["path", "version"])]
        git: Option<String>,
        /// Git revision (used with --git).
        #[arg(long)]
        rev: Option<String>,
        /// Registry version: `cobrust add <name> --version 1.2`.
        #[arg(long, conflicts_with_all = ["path", "git"])]
        version: Option<String>,
        /// Add to `[dev-dependencies]` instead of `[dependencies]`.
        #[arg(long)]
        dev: bool,
    },
    /// Collect a compiler bug report and print a GitHub issue link.
    ///
    /// Run this command immediately after a compiler crash or unexpected
    /// `error[Internal]` message.  The collected report can be attached
    /// to a new GitHub issue.
    ReportBug {
        /// Include the last MIR dump (`.cobrust/last_mir.txt`) in the report.
        #[arg(long)]
        include_mir: bool,
        /// Attach this Cobrust source file to the report.
        #[arg(long)]
        source_file: Option<PathBuf>,
        /// Write the report tarball to this directory (default: current dir).
        #[arg(long)]
        out_dir: Option<PathBuf>,
    },
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
        } => match file {
            // M11 single-file mode: explicit `.cb` argument.
            Some(p) if p.is_file() && p.extension().is_some_and(|e| e == "cb") => build::run(
                &p,
                output.as_deref(),
                emit.into(),
                release,
                target.as_deref(),
                quiet,
            ),
            // M12 package mode: directory or no argument → walk for cobrust.toml.
            other => pkg_build::run_build(
                other.as_deref(),
                output.as_deref(),
                emit.into(),
                release,
                target.as_deref(),
                quiet,
            ),
        },
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
        Command::Add {
            name,
            path,
            git,
            rev,
            version,
            dev,
        } => add::run(
            &name,
            path.as_deref(),
            git.as_deref(),
            rev.as_deref(),
            version.as_deref(),
            dev,
        ),
        Command::ReportBug {
            include_mir,
            source_file,
            out_dir,
        } => report_bug::run(include_mir, source_file.as_deref(), out_dir.as_deref()),
    };
    ExitCode::from(code)
}
