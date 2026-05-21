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
mod debug;
pub mod error_ux;
mod exit_codes;
mod fmt;
pub mod install;
mod new;
mod pkg_build;
mod repl;
mod report_bug;
mod run;
mod skills;
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
        /// Enable (true) or disable (false) Tier-1 LLVM runtime-dispatch
        /// multi-versioning (SSE2 / AVX2 / AVX-512 specialisations in one binary).
        /// Default: true on --release, false on debug builds.
        /// LLVM backend only; Cranelift ignores this flag. §2.5 LLM-first:
        /// LLM users do not need to specify this flag for --release builds.
        #[arg(long)]
        enable_runtime_dispatch: Option<bool>,
        /// Tier-2 host-specific CPU tuning: pass a CPU name to LLVM
        /// (e.g. `native`, `skylake`, `apple-m1`, `neoverse-v1`).
        /// `native` auto-detects the current host CPU and enables all
        /// available ISA extensions — zero dispatch overhead, host-only binary.
        /// Default: unset (LLVM targets generic baseline).
        /// LLVM backend only; Cranelift ignores this flag.
        #[arg(long)]
        target_cpu: Option<String>,
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
        /// Trailing args forwarded to the produced user program (after
        /// the `--` separator). ADR-0044 W2 Phase 2: lets
        /// `cobrust run prog.cb -- a b c` end up with `argv() == [exe_path, "a", "b", "c"]`
        /// inside the user program. `last = true` tells clap to treat
        /// every argument after `--` as part of this Vec without
        /// re-parsing as options.
        #[arg(last = true, allow_hyphen_values = true)]
        program_args: Vec<String>,
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
    /// Show or fetch agent-readable skill cheatsheets embedded in the binary.
    ///
    /// Skills are version-matched to this binary (rust-embed; ADR-0061).
    /// LLM agents call this mid-conversation to fetch Cobrust-specific
    /// idioms absent from training data (CLAUDE.md §2.5).
    ///
    /// Examples:
    ///   cobrust skills list
    ///   cobrust skills get cobrust-language
    ///   cobrust skills get cobrust-error-codes --json
    Skills {
        #[command(subcommand)]
        action: skills::SkillsArgs,
    },

    /// Install a Cobrust package from the wheel registry (ADR-0065 §3.3).
    ///
    /// Detects host CPU, fetches the wheel index, selects the highest-tier
    /// matching wheel, verifies SHA-256, and unpacks under
    /// `$COBRUST_HOME/pkgs/<name>-<version>/`.
    ///
    /// Examples:
    ///   cobrust install numpy-cb --version 0.1.0
    ///   cobrust install hello-cb --version 0.1.0 --registry-url https://example.com
    ///   cobrust install numpy-cb --version 0.1.0 --dry-run
    ///   cobrust install svecalc-cb --version 0.1.0 --allow-experimental
    Install {
        /// Package name (e.g. `numpy-cb`, `hello-cb`).
        pkg_name: String,
        /// Package version (e.g. `0.1.0`). Required in wave-2 — transitive
        /// resolution lands later.
        #[arg(long)]
        version: Option<String>,
        /// Override the wheel registry URL (advanced; default points at
        /// the official Cobrust GitHub Releases host).
        #[arg(long)]
        registry_url: Option<String>,
        /// Resolve + select but don't download or write to disk.
        #[arg(long)]
        dry_run: bool,
        /// Allow installing experimental wheels such as SVE (ADR-0065 §3.1 /
        /// §6.5). Experimental wheels may have unstable ABI or correctness
        /// gaps. Default is `false`; must be set explicitly.
        #[arg(long)]
        allow_experimental: bool,
    },

    /// Interactive lldb / DAP-stdio debugging launcher (Phase L wave-3,
    /// ADR-0059c). Builds the source with DWARF on, auto-imports the
    /// wave-1 lldb pretty-printers (`tools/lldb-cobrust/printers.py`),
    /// and spawns `lldb-18` with inherited stdio. With `--dap`, forwards
    /// stdio to the wave-2 `cobrust-dap` server.
    Debug {
        /// Source `.cb` file. Required in interactive mode; optional
        /// in `--dap` mode (the DAP `Launch` request carries the
        /// program path).
        file: Option<PathBuf>,
        /// Spawn the sibling `cobrust-dap` stdio server and forward
        /// stdin/stdout/stderr to it (editor DAP-stdio transport).
        #[arg(long)]
        dap: bool,
        /// Auto-set a line breakpoint in interactive mode. Repeatable
        /// (`--bp 5 --bp 12`).
        #[arg(long)]
        bp: Vec<u32>,
        /// Override the lldb binary path (default: `lldb-18`, fallback
        /// `lldb` on `$PATH`).
        #[arg(long)]
        lldb_path: Option<PathBuf>,
        /// Suppress informational stderr.
        #[arg(short, long)]
        quiet: bool,
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
            enable_runtime_dispatch,
            target_cpu,
        } => match file {
            // M11 single-file mode: explicit `.cb` argument.
            Some(p) if p.is_file() && p.extension().is_some_and(|e| e == "cb") => build::run(
                &p,
                output.as_deref(),
                emit.into(),
                release,
                target.as_deref(),
                quiet,
                enable_runtime_dispatch,
                target_cpu.as_deref(),
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
            program_args,
        } => run::run(&file, release, target.as_deref(), quiet, &program_args),
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
        Command::Skills { action } => skills::cmd_skills(&action),
        Command::Install {
            pkg_name,
            version,
            registry_url,
            dry_run,
            allow_experimental,
        } => install::run(install::InstallArgs {
            pkg_name,
            version,
            registry_url,
            dry_run,
            allow_experimental,
        }),
        Command::Debug {
            file,
            dap,
            bp,
            lldb_path,
            quiet,
        } => debug::run(debug::DebugArgs {
            file,
            dap,
            bp,
            lldb_path,
            quiet,
        }),
    };
    ExitCode::from(code)
}
