//! Package-mode `cobrust build` / `cobrust test` driver (M12, ADR-0026).
//!
//! When the user runs `cobrust build` with no `.cb` argument (or a path
//! pointing at a directory), we:
//!
//! 1. Locate the nearest `cobrust.toml` walking up from cwd (or the
//!    explicit directory).
//! 2. Parse + validate via `cobrust_pkg::Manifest::parse_str`.
//! 3. Open the user-global registry at `~/.cobrust/registry/`.
//! 4. Resolve the dep graph; emit/refresh `cobrust.lock` next to the
//!    manifest.
//! 5. Compile the `[bin]` (and/or `[lib]`) target via the M11 single-file
//!    pipeline (`build::build`), passing the manifest-relative path.
//!
//! M12 treats each pkg's source `.path` as a single-file Cobrust program.
//! Multi-file modules + import resolution are M12.x scope (out of this
//! milestone). In practice: notebook example bundles one main.cb that
//! the parser walks; module files coexist as siblings + are referenced
//! via `import` statements which the parser already accepts at M11.
//!
//! `cobrust test` extends the same flow: after resolving the manifest,
//! every `[[test]]` entry is built + invoked; test pass/fail is the
//! union of (build-success ∧ exit-status==0).

use std::path::{Path, PathBuf};
use std::process::Command;

use cobrust_pkg::{Lockfile, Registry, find_manifest, load_manifest, resolve_and_lock};

use crate::build::{self, BuildError, EmitKind};
use crate::exit_codes;

/// Run `cobrust build` in package mode (no `.cb` argument or a directory
/// argument). Walks up to `cobrust.toml`, resolves deps, builds the
/// `[bin]` (or `[lib]`) target.
pub fn run_build(
    file_or_dir: Option<&Path>,
    output: Option<&Path>,
    emit_kind: EmitKind,
    release: bool,
    target: Option<&str>,
    quiet: bool,
) -> u8 {
    let cwd = match start_from(file_or_dir) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("cobrust build: {e}");
            return exit_codes::USER_ERROR;
        }
    };

    let manifest_path = match find_manifest(&cwd) {
        Some(p) => p,
        None => {
            eprintln!(
                "cobrust build: no `cobrust.toml` found walking up from {}",
                cwd.display()
            );
            return exit_codes::USER_ERROR;
        }
    };

    let workspace_root = manifest_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let manifest = match load_manifest(&manifest_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("cobrust build: {e}");
            return exit_codes::USER_ERROR;
        }
    };

    if !quiet {
        eprintln!(
            "cobrust build: package `{}` v{} ({})",
            manifest.package.name,
            manifest.package.version,
            manifest_path.display()
        );
    }

    // Resolve + lock.
    let registry = match Registry::open_default() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("cobrust build: cannot open registry: {e}");
            return exit_codes::INTERNAL_PANIC;
        }
    };
    let lockfile = match resolve_and_lock(&manifest, &workspace_root, &registry) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cobrust build: dep resolution failed: {e}");
            return exit_codes::USER_ERROR;
        }
    };

    let lock_path = workspace_root.join("cobrust.lock");
    if let Err(e) = cobrust_pkg::save_lockfile(&lockfile, &lock_path) {
        eprintln!("cobrust build: cannot write {}: {e}", lock_path.display());
        return exit_codes::INTERNAL_PANIC;
    }
    if !quiet {
        eprintln!(
            "cobrust build: wrote {} ({} packages)",
            lock_path.display(),
            lockfile.packages.len()
        );
    }

    // Per ADR-0026 §"Public CLI surface impact": resolve `[bin]` first,
    // fall back to `[lib]` (M12 doesn't link multiple targets in one
    // invocation; that's the M12.x scope).
    let bin_or_lib = match (&manifest.bin, &manifest.lib) {
        (Some(b), _) => Target::Bin(b.path.clone(), b.name.clone()),
        (None, Some(l)) => Target::Lib(l.path.clone(), l.name.clone()),
        (None, None) => {
            eprintln!("cobrust build: manifest declares neither [bin] nor [lib]");
            return exit_codes::USER_ERROR;
        }
    };

    let target_path = workspace_root.join(bin_or_lib.path());
    if !target_path.is_file() {
        eprintln!(
            "cobrust build: target source `{}` does not exist",
            target_path.display()
        );
        return exit_codes::USER_ERROR;
    }

    // Resolve output: respect user override; otherwise emit under
    // `<workspace_root>/target/cobrust/<name>`.
    let resolved_output = match output {
        Some(o) => Some(o.to_path_buf()),
        None => {
            let dir = workspace_root.join("target").join("cobrust");
            if let Err(e) = std::fs::create_dir_all(&dir) {
                eprintln!("cobrust build: cannot create {}: {e}", dir.display());
                return exit_codes::INTERNAL_PANIC;
            }
            Some(dir.join(bin_or_lib.name()))
        }
    };

    match build::build(
        &target_path,
        resolved_output.as_deref(),
        emit_kind,
        release,
        target,
        quiet,
        None, // enable_runtime_dispatch: use default (true on --release)
    ) {
        Ok(_) => exit_codes::SUCCESS,
        Err(e) => {
            eprintln!("cobrust build: {e}");
            match e {
                BuildError::User(_) => exit_codes::USER_ERROR,
                BuildError::Type(_) => exit_codes::TYPE_ERROR,
                BuildError::Internal(_) => exit_codes::INTERNAL_PANIC,
            }
        }
    }
}

/// Run `cobrust test` in package mode. Builds + invokes every
/// `[[test]]` entry. Returns SUCCESS iff all tests pass.
pub fn run_test(file_or_dir: Option<&Path>, release: bool, quiet: bool) -> u8 {
    let cwd = match start_from(file_or_dir) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("cobrust test: {e}");
            return exit_codes::USER_ERROR;
        }
    };

    let manifest_path = match find_manifest(&cwd) {
        Some(p) => p,
        None => {
            eprintln!(
                "cobrust test: no `cobrust.toml` found walking up from {}",
                cwd.display()
            );
            return exit_codes::USER_ERROR;
        }
    };
    let workspace_root = manifest_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let manifest = match load_manifest(&manifest_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("cobrust test: {e}");
            return exit_codes::USER_ERROR;
        }
    };

    // Refresh the lockfile (matches `cobrust build`).
    let registry = match Registry::open_default() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("cobrust test: cannot open registry: {e}");
            return exit_codes::INTERNAL_PANIC;
        }
    };
    let lock = match resolve_and_lock(&manifest, &workspace_root, &registry) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cobrust test: dep resolution failed: {e}");
            return exit_codes::USER_ERROR;
        }
    };
    let _ = persist_lockfile(&lock, &workspace_root, quiet);

    if manifest.tests.is_empty() {
        if !quiet {
            eprintln!("cobrust test: no [[test]] entries declared in manifest");
        }
        return exit_codes::SUCCESS;
    }

    let target_dir = workspace_root.join("target").join("cobrust").join("tests");
    if let Err(e) = std::fs::create_dir_all(&target_dir) {
        eprintln!("cobrust test: cannot create {}: {e}", target_dir.display());
        return exit_codes::INTERNAL_PANIC;
    }

    let mut passes = 0usize;
    let mut fails = 0usize;
    for t in &manifest.tests {
        let src = workspace_root.join(&t.path);
        if !src.is_file() {
            eprintln!(
                "cobrust test: [[test]] `{}` source `{}` not found",
                t.name,
                src.display()
            );
            fails += 1;
            continue;
        }
        let bin_path = target_dir.join(&t.name);
        let result = build::build(
            &src,
            Some(&bin_path),
            EmitKind::Executable,
            release,
            None,
            true,
            None, // enable_runtime_dispatch: use default
        );
        match result {
            Ok(_) => {}
            Err(e) => {
                eprintln!("cobrust test: build of `{}` failed: {e}", t.name);
                fails += 1;
                continue;
            }
        }
        let status = Command::new(&bin_path).status();
        match status {
            Ok(s) if s.success() => {
                passes += 1;
                if !quiet {
                    eprintln!("test {} ... ok", t.name);
                }
            }
            Ok(s) => {
                fails += 1;
                eprintln!("test {} ... FAILED (exit {s:?})", t.name);
            }
            Err(e) => {
                fails += 1;
                eprintln!("test {} ... FAILED (invoke: {e})", t.name);
            }
        }
    }

    if !quiet {
        eprintln!(
            "\ntest result: {}. {} passed; {} failed",
            if fails == 0 { "ok" } else { "FAILED" },
            passes,
            fails
        );
    }
    if fails == 0 {
        exit_codes::SUCCESS
    } else {
        exit_codes::USER_ERROR
    }
}

fn persist_lockfile(lock: &Lockfile, workspace_root: &Path, quiet: bool) -> std::io::Result<()> {
    let path = workspace_root.join("cobrust.lock");
    let _ = cobrust_pkg::save_lockfile(lock, &path);
    if !quiet {
        eprintln!("cobrust: refreshed {}", path.display());
    }
    Ok(())
}

fn start_from(arg: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(p) = arg {
        if p.is_dir() {
            return Ok(p.to_path_buf());
        }
        if p.is_file() {
            return Err(format!(
                "expected a directory or no argument; got file `{}`",
                p.display()
            ));
        }
        return Err(format!("path `{}` does not exist", p.display()));
    }
    std::env::current_dir().map_err(|e| format!("cannot read cwd: {e}"))
}

enum Target {
    Bin(String, String),
    Lib(String, String),
}

impl Target {
    fn path(&self) -> &str {
        match self {
            Self::Bin(p, _) | Self::Lib(p, _) => p.as_str(),
        }
    }
    fn name(&self) -> &str {
        match self {
            Self::Bin(_, n) | Self::Lib(_, n) => n.as_str(),
        }
    }
}
