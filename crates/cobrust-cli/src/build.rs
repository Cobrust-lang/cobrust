//! `cobrust build` — end-to-end driver per ADR-0024 §"Hello-world contract".
//!
//! Stitches:
//!
//! 1. `parse_str` → `hir_lower` → `type_check` → `mir_lower` (M1..M8).
//! 2. M10 intrinsic rewrite (`intrinsics::rewrite_print`): recognizes
//!    `print("hello, world")` callsites and rewrites the MIR `Call`'s
//!    `func` operand to point at the runtime helper symbol
//!    `__cobrust_println_static`. Per ADR-0024, M10 narrows the
//!    intrinsic to the literal `"hello, world"`; M11 stdlib supersedes.
//! 3. `cobrust_codegen::emit` (M9 surface; the Cranelift backend's
//!    Call-amendment emits a real call when `func` is `Constant::Str`).
//! 4. Linker stage: invoke `cc <user>.o <runtime>.o -o <out>` with the
//!    M10 runtime helper compiled from
//!    `crates/cobrust-cli/runtime/m10_runtime.c`.

use std::path::{Path, PathBuf};
use std::process::Command;

use cobrust_codegen::{Artifact, ArtifactKind, Backend, OptLevel, TargetSpec, emit};
use cobrust_frontend::{parse_str, span::FileId};
use cobrust_hir::{Session, lower as hir_lower};
use cobrust_mir::{Module as MirModule, lower as mir_lower};
use cobrust_types::check as type_check;
use target_lexicon::Triple;

use crate::exit_codes;

pub mod intrinsics;

/// M10 prelude prepended to every Cobrust source before parsing.
/// Declares `print(s: str) -> i64` so user programs can call it; the
/// intrinsic-rewrite pass (per ADR-0024 §"Hello-world contract") then
/// retargets the MIR Call from this stub Body to the runtime helper
/// `__cobrust_println_static`. M11 stdlib supersedes by lifting this
/// declaration into `std.io`.
///
/// Re-exported from [`cobrust_frontend::PRELUDE`] per F50
/// (2026-05-22): the LSP `cobrust-lsp` crate also prepends the same
/// source before invoking the frontend so `textDocument/publishDiagnostics`
/// reaches diagnostic parity with `cobrust check`. The single source-
/// of-truth lives in `crates/cobrust-frontend/src/prelude.rs`; this
/// re-export keeps the `crate::build::PRELUDE` call sites stable.
pub use cobrust_frontend::PRELUDE;

/// What `cobrust build` should emit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmitKind {
    /// Relocatable object file (`.o`); no link step.
    Object,
    /// Linked executable (`.exe` on Windows, no extension elsewhere).
    Executable,
}

/// Run `cobrust build <file.cb>`.
#[allow(clippy::too_many_arguments)]
pub fn run(
    file: &Path,
    output: Option<&Path>,
    emit_kind: EmitKind,
    release: bool,
    target: Option<&str>,
    quiet: bool,
    enable_runtime_dispatch: Option<bool>,
    target_cpu: Option<&str>,
) -> u8 {
    match build(
        file,
        output,
        emit_kind,
        release,
        target,
        quiet,
        enable_runtime_dispatch,
        target_cpu,
    ) {
        Ok(_) => exit_codes::SUCCESS,
        Err(e) => {
            eprintln!("cobrust build: {e}");
            e.exit_code()
        }
    }
}

/// Underlying `Result`-returning driver. Used by `cobrust run` to grab
/// the produced artifact path before invoking it.
///
/// `enable_runtime_dispatch` — when `Some`, overrides the default
/// Tier-1 runtime-dispatch setting (default: `true` on `--release`).
/// Pass `None` to accept the default.
///
/// `target_cpu` — Tier 2 host-specific CPU tuning
/// (numerical-compute-hardware-tiering.md §Tier 2).
/// `"native"` auto-detects the host CPU; any other string is passed
/// directly to LLVM as the CPU name (e.g. `"skylake"`, `"apple-m1"`).
/// `None` keeps the generic LLVM baseline.
#[allow(clippy::too_many_arguments)]
pub fn build(
    file: &Path,
    output: Option<&Path>,
    emit_kind: EmitKind,
    release: bool,
    target: Option<&str>,
    quiet: bool,
    enable_runtime_dispatch: Option<bool>,
    target_cpu: Option<&str>,
) -> Result<Artifact, BuildError> {
    let user_source = std::fs::read_to_string(file)
        .map_err(|e| BuildError::User(format!("cannot read {}: {e}", file.display())))?;
    let source = format!("{PRELUDE}{user_source}");

    let module = parse_str(&source, FileId::SYNTHETIC)
        .map_err(|e| BuildError::Type(format!("parse error: {e:?}")))?;

    let mut sess = Session::new();
    let hir = hir_lower(&module, &mut sess)
        .map_err(|e| BuildError::Type(format!("HIR lower error: {e:?}")))?;
    let typed = type_check(&hir).map_err(|e| BuildError::Type(format!("type error: {e:?}")))?;

    let mut mir = mir_lower(&typed).map_err(|e| BuildError::Type(format!("MIR error: {e:?}")))?;

    intrinsics::rewrite_print(&mut mir).map_err(|e| BuildError::Type(format!("{e}")))?;

    // --- target spec ----------------------------------------------------
    let triple = match target {
        Some(t) => t
            .parse::<Triple>()
            .map_err(|e| BuildError::User(format!("invalid target triple `{t}`: {e}")))?,
        None => Triple::host(),
    };

    let module_name = file
        .file_stem()
        .and_then(|s| s.to_str())
        .map_or_else(|| String::from("a"), String::from);

    // Final output directory — where the user wants the *final* artifact
    // to land. For `EmitKind::Object` this is also the codegen output dir.
    // For `EmitKind::Executable` we route codegen + runtime intermediates
    // through a scoped `tempfile::TempDir` (see `intermediate_scope`
    // below) so the `.o` files don't pile up next to the final executable.
    let final_output_dir = match output {
        Some(p) => p
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
        None => PathBuf::from("target/cobrust"),
    };
    std::fs::create_dir_all(&final_output_dir)
        .map_err(|e| BuildError::Internal(format!("mkdir {}: {e}", final_output_dir.display())))?;

    // For Executable emit, ask codegen for the Object only; we link
    // ourselves with the runtime helper. Codegen's own link step would
    // call `cc <user>.o` without runtime.o, leaving
    // `__cobrust_println_static` undefined.
    let artifact_kind = match emit_kind {
        EmitKind::Object => ArtifactKind::Object,
        EmitKind::Executable => ArtifactKind::Object,
    };

    let opt_level = if release {
        OptLevel::Speed
    } else {
        OptLevel::None
    };
    let backend = if release {
        Backend::default_for_release()
    } else {
        Backend::default_for_dev()
    };

    // Tempdir RAII for intermediate .o artifacts in the Executable path
    // (2026-05-18 leak fix; supersedes the 81a2433 test-only patch).
    // `intermediate_scope` is `Some(TempDir)` when we're building an
    // executable — its Drop runs at function exit (success OR
    // panic-unwind) and removes the temp directory + all the
    // intermediate `.o` files inside. For Object emit, intermediates
    // ARE the final output, so we keep `intermediate_scope = None` and
    // codegen writes directly to `final_output_dir`.
    //
    // CRITICAL: do NOT call `intermediate_scope.into_path()` anywhere
    // below — that consumes the TempDir and leaks the directory.
    // Tempdir is RAII-owned by this scope; see the commit
    // `findings: diagnose CLI tempdir leak root cause` for rationale.
    let intermediate_scope = match emit_kind {
        EmitKind::Object => None,
        EmitKind::Executable => Some(
            tempfile::Builder::new()
                .prefix("cobrust-build-")
                .tempdir_in(&final_output_dir)
                .or_else(|_| tempfile::Builder::new().prefix("cobrust-build-").tempdir())
                .map_err(|e| BuildError::Internal(format!("create intermediate tempdir: {e}")))?,
        ),
    };

    let codegen_output_dir: PathBuf = match &intermediate_scope {
        Some(td) => td.path().to_path_buf(),
        None => final_output_dir.clone(),
    };

    let spec = TargetSpec {
        triple,
        opt_level,
        backend,
        artifact: artifact_kind,
        output_dir: codegen_output_dir.clone(),
        module_name: module_name.clone(),
        source_path: None,
        // Tier 1 runtime-dispatch: default true on --release, false on debug.
        // `enable_runtime_dispatch` overrides when explicitly set.
        runtime_dispatch: enable_runtime_dispatch.unwrap_or(release),
        // Tier 2: pass caller-supplied CPU string (or None for generic baseline).
        target_cpu: target_cpu.map(str::to_owned),
    };

    // Emit the user's object file.
    let user_artifact = emit(&mir, spec).map_err(|e| BuildError::Internal(format!("{e}")))?;

    // For Executable kind, the codegen layer already invoked `cc`. But our
    // M10 hello-world contract requires the runtime helper to be linked
    // alongside; codegen's link step doesn't know about runtime helpers.
    // So when `emit_kind == Executable`, we re-link manually with the
    // runtime helper appended.
    match emit_kind {
        EmitKind::Object => {
            // Caller asked for an `.o` only; the linker isn't invoked.
            // The runtime helper isn't relevant here.
            if let Some(target_path) = output {
                if user_artifact.path() != target_path {
                    std::fs::copy(user_artifact.path(), target_path).map_err(|e| {
                        BuildError::Internal(format!(
                            "cannot copy {} → {}: {e}",
                            user_artifact.path().display(),
                            target_path.display()
                        ))
                    })?;
                }
            }
            if !quiet {
                eprintln!("cobrust build: wrote {}", user_artifact.path().display());
            }
            Ok(user_artifact)
        }
        EmitKind::Executable => {
            // Codegen emitted an executable, but it lacks the runtime
            // helper. Re-link: produce a fresh `.o` then link with
            // runtime/m10_runtime.c.
            //
            // The user object + runtime helper object both live under
            // `codegen_output_dir` (= `intermediate_scope.path()` for
            // EmitKind::Executable, owned by the TempDir created above).
            // The final exe is written to `final_output_dir` so it
            // survives the TempDir Drop at function exit.
            let user_obj_path = codegen_output_dir.join(format!("{module_name}.o"));
            // The previous emit may have left the `.o` next to the exe
            // (cranelift_backend::emit always writes `<module_name>.o`
            // first, then links). Confirm + re-link.
            if !user_obj_path.exists() {
                return Err(BuildError::Internal(format!(
                    "expected user object at {} but it was not produced",
                    user_obj_path.display()
                )));
            }

            let runtime_obj = ensure_runtime_object(&codegen_output_dir)?;
            let exe_path = match output {
                Some(p) => p.to_path_buf(),
                None => {
                    // No `-o` flag: write the final exe into the
                    // user-visible `final_output_dir` (= `target/cobrust`
                    // by default), NOT the TempDir — otherwise it would
                    // be deleted on Drop before `cobrust run` could
                    // spawn it.
                    let mut p = final_output_dir.join(&module_name);
                    let ext = ArtifactKind::Executable.extension(&Triple::host());
                    if !ext.is_empty() {
                        p.set_extension(ext);
                    }
                    p
                }
            };

            let stdlib_archive = locate_stdlib_archive(release)?;
            let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
            // Per ADR-0025 §"Runtime ABI": link order matters — user
            // object provides forward references resolved by stdlib
            // archive (which provides `__cobrust_print`,
            // `__cobrust_println`, etc.). On macOS the linker resolves
            // archives lazily; on Linux we use --whole-archive only if
            // strictly needed (M11 helpers are referenced at every
            // print/panic callsite, so plain archive resolution
            // suffices).
            let mut cmd = Command::new(&cc);
            cmd.arg(&user_obj_path)
                .arg(&runtime_obj)
                .arg(&stdlib_archive)
                .arg("-o")
                .arg(&exe_path);
            // Platform: macOS doesn't need --no-as-needed; Linux needs
            // libpthread + libdl + libm pulled in for std + mimalloc.
            if cfg!(target_os = "linux") {
                cmd.arg("-lpthread").arg("-ldl").arg("-lm");
            }
            let status = cmd
                .status()
                .map_err(|e| BuildError::Internal(format!("invoking {cc}: {e}")))?;
            if !status.success() {
                return Err(BuildError::Internal(format!(
                    "linker `{cc}` exited with status {status:?}"
                )));
            }

            if !quiet {
                eprintln!("cobrust build: linked {}", exe_path.display());
            }
            Ok(Artifact::Executable(exe_path))
        }
    }
}

/// Compile `runtime/cobrust_main.c` into an object file (idempotent;
/// caches at `<output_dir>/cobrust_main.o`). Per ADR-0025 §G this
/// shim provides the platform `main(argc, argv)` entry, captures
/// argv via `__cobrust_capture_argv`, then dispatches to the user's
/// codegen-emitted `_cobrust_user_main`.
///
/// T1.3: checks the compile-time baked `COBRUST_RUNTIME_OBJ_PATH` env
/// (set by `build.rs`) before falling back to compiling from source.
fn ensure_runtime_object(output_dir: &Path) -> Result<PathBuf, BuildError> {
    // T1.3: use the pre-compiled object baked in at build time (set by build.rs).
    // Uses option_env! so the crate compiles even without the build script having run.
    if let Some(baked) = option_env!("COBRUST_RUNTIME_OBJ_PATH") {
        if !baked.is_empty() {
            let p = PathBuf::from(baked);
            if p.exists() {
                return Ok(p);
            }
        }
    }

    // Fallback: runtime env override (CI / test harness).
    if let Ok(p) = std::env::var("COBRUST_RUNTIME_OBJ") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Ok(p);
        }
    }

    // Development fallback: compile from source.
    let runtime_obj = output_dir.join("cobrust_main.o");
    if runtime_obj.exists() {
        return Ok(runtime_obj);
    }
    let runtime_src = locate_runtime_source()?;
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let status = Command::new(&cc)
        .arg("-c")
        .arg(&runtime_src)
        .arg("-O0")
        .arg("-o")
        .arg(&runtime_obj)
        .status()
        .map_err(|e| BuildError::Internal(format!("compiling runtime helper: {e}")))?;
    if !status.success() {
        return Err(BuildError::Internal(format!(
            "runtime-helper compilation failed: status {status:?}"
        )));
    }
    Ok(runtime_obj)
}

/// Locate the runtime C entrypoint (`runtime/cobrust_main.c`).
///
/// ADR-0069 wheel-layout-aware lookup chain (v0.6.0+):
///
/// - **Phase 0 (wheel-layout, NEW)** — derive `<install_prefix>` from the running binary's own path via `current_exe()`. The wheel extracts to `cobrust-vX.Y.Z/{bin,lib/cobrust,share/cobrust/runtime}/`, so a binary at `<prefix>/bin/cobrust` finds its runtime sources at `<prefix>/share/cobrust/runtime/cobrust_main.c`.
/// - **Phase 1 (dev fallback)** — `CARGO_MANIFEST_DIR/runtime/cobrust_main.c` bakes the workspace path at compile time. Works for `cargo install` + source-tree `cargo build`; broken for wheels per F46 (the GH Actions runner workspace path is gone by user run-time).
/// - **Phase 2 (legacy `current_exe`-rooted)** — kept for compat with any future relocation experiment. Same shape as Phase 0 but the Phase 0 path is the canonical wheel-layout target.
fn locate_runtime_source() -> Result<PathBuf, BuildError> {
    let mut checked: Vec<PathBuf> = Vec::new();

    // Phase 0: wheel-layout lookup (ADR-0069 §4.2).
    if let Some(p) = locate_wheel_share_file("runtime/cobrust_main.c") {
        if p.exists() {
            return Ok(p);
        }
        checked.push(p);
    }

    // Phase 1: workspace dev path (CARGO_MANIFEST_DIR compile-time const).
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let p = Path::new(manifest_dir).join("runtime/cobrust_main.c");
    if p.exists() {
        return Ok(p);
    }
    checked.push(p);

    // Phase 2: legacy current_exe-rooted relative-to-bin lookup.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let q = dir.join("../share/cobrust/runtime/cobrust_main.c");
            if q.exists() {
                return Ok(q);
            }
            checked.push(q);
        }
    }

    Err(BuildError::Internal(format!(
        "cannot locate runtime/cobrust_main.c (ADR-0069 wheel-layout lookup); checked: {}",
        checked
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    )))
}

/// ADR-0069 §4.2 Phase 0 helper: derive `<install_prefix>/share/cobrust/`
/// from the running binary via `current_exe()`.
///
/// Wheel layout: `cobrust-vX.Y.Z/bin/cobrust` → prefix is the parent of
/// `bin/`. Returns `Some(<prefix>/share/cobrust/<rel_path>)` if the
/// derivation succeeds; the caller is responsible for `.exists()`
/// confirmation. Returns `None` if `current_exe()` or the parent walks
/// fail (typically only on unusual sandboxed installs).
fn locate_wheel_share_file(rel_path: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let bin_dir = exe.parent()?;
    let prefix = bin_dir.parent()?;
    Some(prefix.join("share").join("cobrust").join(rel_path))
}

/// ADR-0069 §4.2 Phase 0 helper: derive `<install_prefix>/lib/cobrust/`
/// from the running binary via `current_exe()`.
///
/// Same shape as [`locate_wheel_share_file`] but targets the `lib/`
/// sibling (where `libcobrust_stdlib.a` lives in the wheel layout).
fn locate_wheel_lib_file(rel_path: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let bin_dir = exe.parent()?;
    let prefix = bin_dir.parent()?;
    Some(prefix.join("lib").join("cobrust").join(rel_path))
}

/// Locate the prebuilt `libcobrust_stdlib.a` static archive.
///
/// T1.3 (install-path): the `build.rs` build script bakes the archive path
/// into the binary via `COBRUST_STDLIB_ARCHIVE_PATH` at compile time so
/// `cargo install cobrust-cli` produces a self-contained binary that never
/// needs a separate `cargo build -p cobrust-stdlib` step.
///
/// ADR-0069 wheel-layout-aware lookup chain (v0.6.0+):
///
/// - **Phase 0 (wheel-layout, NEW)** — `<install_prefix>/lib/cobrust/libcobrust_stdlib.a` derived from `current_exe()`. Wheel users get a zero-config path (F46 closure).
/// - **Phase 1** — `COBRUST_STDLIB_ARCHIVE_PATH` compile-time env var (baked in by `build.rs`).
/// - **Phase 2** — `COBRUST_STDLIB_ARCHIVE` runtime env var override (for CI / test harness).
/// - **Phase 3** — walk workspace `target/{release,debug}/libcobrust_stdlib.a`.
fn locate_stdlib_archive(release: bool) -> Result<PathBuf, BuildError> {
    // 0. ADR-0069 §4.2 Phase 0 — wheel-layout lookup. Fires first so
    //    wheel users (F46 closure) hit a working path without env vars.
    if let Some(p) = locate_wheel_lib_file("libcobrust_stdlib.a") {
        if p.exists() {
            return Ok(p);
        }
    }

    // 1. Compile-time baked-in path from build.rs (preferred — works after
    //    `cargo install`). Uses option_env! so the crate compiles without build.rs.
    if let Some(baked) = option_env!("COBRUST_STDLIB_ARCHIVE_PATH") {
        if !baked.is_empty() {
            let p = PathBuf::from(baked);
            if p.exists() {
                return Ok(p);
            }
        }
    }

    // 2. Runtime override (useful for tests / CI that swap the archive).
    if let Ok(p) = std::env::var("COBRUST_STDLIB_ARCHIVE") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Ok(p);
        }
    }

    // 3. Workspace-relative fallback for development builds.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace = Path::new(manifest_dir)
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| {
            BuildError::Internal("cannot derive workspace root from CARGO_MANIFEST_DIR".into())
        })?;
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| workspace.join("target"), PathBuf::from);
    let profile = if release { "release" } else { "debug" };
    let candidate = target_dir.join(profile).join("libcobrust_stdlib.a");
    if candidate.exists() {
        return Ok(candidate);
    }
    let other = if release { "debug" } else { "release" };
    let alt = target_dir.join(other).join("libcobrust_stdlib.a");
    if alt.exists() {
        return Ok(alt);
    }
    Err(BuildError::Internal(format!(
        "cannot locate libcobrust_stdlib.a \
         (looked under {cand} and {alt_}); \
         install via `cargo install cobrust-cli`, \
         run `cargo build -p cobrust-stdlib` in the workspace first, \
         or download a v0.6.0+ wheel tarball (ADR-0069)",
        cand = candidate.display(),
        alt_ = alt.display(),
    )))
}

/// Build-stage error taxonomy.
#[derive(Debug)]
pub enum BuildError {
    /// User-facing error (bad path, malformed flag).
    User(String),
    /// Type-check-tier error (parse / HIR / types / MIR).
    Type(String),
    /// Internal error (codegen / linker / I/O).
    Internal(String),
}

impl BuildError {
    pub fn exit_code(&self) -> u8 {
        match self {
            BuildError::User(_) => exit_codes::USER_ERROR,
            BuildError::Type(_) => exit_codes::TYPE_ERROR,
            BuildError::Internal(_) => exit_codes::INTERNAL_PANIC,
        }
    }
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::User(s) | BuildError::Type(s) | BuildError::Internal(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for BuildError {}

#[allow(dead_code)] // ADR-0024 public surface; consumed by future LSP / IDE integrations.
/// Convenience helper for callers that just want a MIR module from a path,
/// for diagnostics or programmatic use. Used by `cobrust run` to skip the
/// link step when only the build artifact is needed.
pub fn lower_to_mir(file: &Path) -> Result<MirModule, BuildError> {
    let user_source = std::fs::read_to_string(file)
        .map_err(|e| BuildError::User(format!("cannot read {}: {e}", file.display())))?;
    let source = format!("{PRELUDE}{user_source}");
    let module = parse_str(&source, FileId::SYNTHETIC)
        .map_err(|e| BuildError::Type(format!("parse error: {e:?}")))?;
    let mut sess = Session::new();
    let hir = hir_lower(&module, &mut sess)
        .map_err(|e| BuildError::Type(format!("HIR lower error: {e:?}")))?;
    let typed = type_check(&hir).map_err(|e| BuildError::Type(format!("type error: {e:?}")))?;
    let mut mir = mir_lower(&typed).map_err(|e| BuildError::Type(format!("MIR: {e:?}")))?;
    intrinsics::rewrite_print(&mut mir).map_err(|e| BuildError::Type(format!("{e}")))?;
    Ok(mir)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn emit_kind_default_executable() {
        // Smoke: kind enum is correctly compared.
        assert_ne!(EmitKind::Object, EmitKind::Executable);
    }
}
