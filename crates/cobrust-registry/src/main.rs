//! `cobrust-registry-gen` — one-shot post-release index generator.
//!
//! Usage:
//! ```text
//! cobrust-registry-gen <pkg> <version> [--repo <owner/name>] [--out-dir <dir>]
//! ```
//!
//! Defaults:
//! - `--repo Cobrust-lang/cobrust`
//! - `--out-dir pkg-index/`
//!
//! The output file is `<out-dir>/<pkg>-<version>.json`.
//!
//! Exit codes:
//! - `0` — success, index written.
//! - `1` — error (HTTP failure, bad version tag, I/O error). Error is
//!   printed to stderr with a `suggestion:` field per §2.5 direction B.
//!
//! Designed to be called as a one-shot step in `release.yml` after all wheel
//! assets are uploaded. W4 will wire this into the CI pipeline.

use std::path::PathBuf;
use std::process::ExitCode;

use cobrust_registry::generator::fetch_release_assets;
use cobrust_registry::{generate_index, write_index_json};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    // Minimal arg parser — no clap dep to keep the registry crate lean.
    let mut pkg: Option<String> = None;
    let mut version: Option<String> = None;
    let mut repo = "Cobrust-lang/cobrust".to_owned();
    let mut out_dir = PathBuf::from("pkg-index");

    let mut iter = args.iter().skip(1).peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--repo" => {
                repo.clone_from(iter.next().expect("--repo requires a value"));
            }
            "--out-dir" => {
                out_dir = PathBuf::from(iter.next().expect("--out-dir requires a value"));
            }
            _ if pkg.is_none() => pkg = Some(arg.to_owned()),
            _ if version.is_none() => version = Some(arg.to_owned()),
            other => {
                eprintln!("error: unexpected argument '{other}'");
                eprintln!(
                    "usage: cobrust-registry-gen <pkg> <version> [--repo <owner/name>] [--out-dir <dir>]"
                );
                return ExitCode::FAILURE;
            }
        }
    }

    // Multiple eprintln! statements prevent the compiler from rewriting this as
    // a simple `let...else`, so we suppress the lint here.
    #[allow(clippy::manual_let_else)]
    let (pkg, version) = if let (Some(p), Some(v)) = (pkg, version) {
        (p, v)
    } else {
        eprintln!("error: missing required arguments");
        eprintln!(
            "usage: cobrust-registry-gen <pkg> <version> [--repo <owner/name>] [--out-dir <dir>]"
        );
        eprintln!(
            "  suggestion: provide both the package name and version, e.g. 'cobrust-registry-gen numpy-cb 0.1.0'"
        );
        return ExitCode::FAILURE;
    };

    let assets = match fetch_release_assets(&repo, &version) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let index = generate_index(&pkg, &version, &assets);
    let out_path = out_dir.join(format!("{pkg}-{version}.json"));

    if let Err(e) = write_index_json(&index, &out_path) {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }

    println!(
        "wrote {} wheel entries to {}",
        index.wheels.len(),
        out_path.display()
    );
    ExitCode::SUCCESS
}
