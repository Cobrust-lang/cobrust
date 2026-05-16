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
/// ADR-0050b M-F.3.1: `range(start, stop) -> list[i64]` is a real
/// Cobrust fn body (not an intrinsic stub) — it materializes a
/// `list[i64]` of `stop - start` slots using `list_new` / `list_set`
/// (both intrinsic-rewritten on every callsite). The body survives
/// the intrinsic-rewrite pass and is compiled through normal MIR /
/// codegen. The for-loop then iterates the returned list via the
/// ADR-0050b length-bound index lowering (`__cobrust_list_len` +
/// `__cobrust_list_get` per `crates/cobrust-mir/src/lower.rs`
/// `LoopKind::For`), NOT the ADR-0044 W2 Phase 2 `__cobrust_iter_*`
/// runtime path. The iter-protocol path remains shipped for
/// comprehension lowering (see `lower.rs:1493-1576` and finding
/// `comp-lowering-zero-sentinel-collision.md` for the open scope
/// gap that Phase G will close).
pub const PRELUDE: &str = "fn print(s: str) -> i64:\n    return 0\n\nfn print_int(n: i64) -> i64:\n    return 0\n\nfn input(prompt: str) -> str:\n    return \"\"\n\nfn input_no_prompt() -> str:\n    return \"\"\n\nfn read_line() -> str:\n    return \"\"\n\nfn argv() -> list[str]:\n    let xs: list[str] = []\n    return xs\n\nfn parse_int(s: str) -> i64:\n    return 0\n\nfn str_len(s: str) -> i64:\n    return 0\n\nfn str_at(s: str, i: i64) -> str:\n    return \"\"\n\nfn str_eq(a: str, b: str) -> i64:\n    return 0\n\nfn str_eq_lit(s: str, lit: str) -> i64:\n    return 0\n\nfn str_ord(s: str) -> i64:\n    return 0\n\nfn parse_int_tok(line: str, i: i64) -> i64:\n    return 0\n\nfn count_toks(line: str) -> i64:\n    return 0\n\nfn list_set(lst: list[i64], i: i64, v: i64) -> i64:\n    return 0\n\nfn list_get(lst: list[i64], i: i64) -> i64:\n    return 0\n\nfn list_len(lst: list[i64]) -> i64:\n    return 0\n\nfn list_is_empty(lst: list[i64]) -> bool:\n    return False\n\nfn list_new(capacity: i64) -> list[i64]:\n    let xs: list[i64] = []\n    return xs\n\nfn print_no_nl(s: str) -> i64:\n    return 0\n\nfn range(start: i64, stop: i64) -> list[i64]:\n    let n: i64 = stop - start\n    let xs: list[i64] = list_new(n)\n    let i: i64 = 0\n    while i < n:\n        let _ = list_set(xs, i, start + i)\n        i = i + 1\n    return xs\n\nfn sqrt(x: f64) -> f64:\n    return 0.0\n\nfn floor(x: f64) -> f64:\n    return 0.0\n\nfn ceil(x: f64) -> f64:\n    return 0.0\n\nfn round(x: f64) -> f64:\n    return 0.0\n\nfn abs(x: f64) -> f64:\n    return 0.0\n\nfn pow(base: f64, exp: f64) -> f64:\n    return 0.0\n\nfn sin(x: f64) -> f64:\n    return 0.0\n\nfn cos(x: f64) -> f64:\n    return 0.0\n\nfn tan(x: f64) -> f64:\n    return 0.0\n\nfn log(x: f64) -> f64:\n    return 0.0\n\nfn exp(x: f64) -> f64:\n    return 0.0\n\nfn llm_complete(provider: str, model: str, prompt: str) -> str:\n    return \"\"\n\nfn llm_dispatch(task: str, prompt: str) -> str:\n    return \"\"\n\nfn llm_stream(provider: str, model: str, prompt: str) -> list[str]:\n    let xs: list[str] = []\n    return xs\n\nfn prompt_render(system: str, user: str, vars: list[str]) -> str:\n    return \"\"\n\nfn prompt_format_few_shot(examples_in: list[str], examples_out: list[str], current_input: str) -> str:\n    return \"\"\n\nfn prompt_format_system_user(system: str, user: str) -> str:\n    return \"\"\n\nfn prompt_escape_braces(text: str) -> str:\n    return \"\"\n\nfn llm_complete_structured(prompt: str, schema_json: str) -> str:\n    return \"\"\n\nfn tool_schema(name: str, description: str, parameters_json: str, return_type: str) -> str:\n    return \"\"\n\nfn tool_registry_new() -> str:\n    return \"\"\n\nfn tool_registry_register(registry_json: str, schema_json: str) -> str:\n    return \"\"\n\nfn tool_invoke(tool_name: str, args_json: str) -> str:\n    return \"\"\n\nfn llm_complete_with_tools(prompt: str, registry_json: str) -> str:\n    return \"\"\n\nfn split(s: str, sep: str) -> list[str]:\n    let xs: list[str] = []\n    return xs\n\nfn join(parts: list[str], sep: str) -> str:\n    return \"\"\n\nfn replace(s: str, old: str, new: str) -> str:\n    return \"\"\n\nfn trim(s: str) -> str:\n    return \"\"\n\nfn find(s: str, needle: str) -> i64:\n    return -1\n\nfn contains(s: str, needle: str) -> bool:\n    return False\n\nfn starts_with(s: str, prefix: str) -> bool:\n    return False\n\nfn ends_with(s: str, suffix: str) -> bool:\n    return False\n\nfn lower(s: str) -> str:\n    return \"\"\n\nfn upper(s: str) -> str:\n    return \"\"\n\nfn clone(s: str) -> str:\n    return s\n\n";

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
) -> u8 {
    match build(file, output, emit_kind, release, target, quiet) {
        Ok(_) => exit_codes::SUCCESS,
        Err(e) => {
            eprintln!("cobrust build: {e}");
            e.exit_code()
        }
    }
}

/// Underlying `Result`-returning driver. Used by `cobrust run` to grab
/// the produced artifact path before invoking it.
pub fn build(
    file: &Path,
    output: Option<&Path>,
    emit_kind: EmitKind,
    release: bool,
    target: Option<&str>,
    quiet: bool,
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

    let output_dir = match output {
        Some(p) => p
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
        None => PathBuf::from("target/cobrust"),
    };
    std::fs::create_dir_all(&output_dir)
        .map_err(|e| BuildError::Internal(format!("mkdir {}: {e}", output_dir.display())))?;

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

    let spec = TargetSpec {
        triple,
        opt_level,
        backend,
        artifact: artifact_kind,
        output_dir: output_dir.clone(),
        module_name: module_name.clone(),
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
            let user_obj_path = output_dir.join(format!("{module_name}.o"));
            // The previous emit may have left the `.o` next to the exe
            // (cranelift_backend::emit always writes `<module_name>.o`
            // first, then links). Confirm + re-link.
            if !user_obj_path.exists() {
                return Err(BuildError::Internal(format!(
                    "expected user object at {} but it was not produced",
                    user_obj_path.display()
                )));
            }

            let runtime_obj = ensure_runtime_object(&output_dir)?;
            let exe_path = match output {
                Some(p) => p.to_path_buf(),
                None => {
                    let mut p = output_dir.join(&module_name);
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

fn locate_runtime_source() -> Result<PathBuf, BuildError> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let p = Path::new(manifest_dir).join("runtime/cobrust_main.c");
    if p.exists() {
        return Ok(p);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let q = dir.join("../share/cobrust/runtime/cobrust_main.c");
            if q.exists() {
                return Ok(q);
            }
        }
    }
    Err(BuildError::Internal(format!(
        "cannot locate runtime/cobrust_main.c (checked {})",
        p.display()
    )))
}

/// Locate the prebuilt `libcobrust_stdlib.a` static archive.
///
/// T1.3 (install-path): the `build.rs` build script bakes the archive path
/// into the binary via `COBRUST_STDLIB_ARCHIVE_PATH` at compile time so
/// `cargo install cobrust-cli` produces a self-contained binary that never
/// needs a separate `cargo build -p cobrust-stdlib` step.
///
/// Fallback chain (for development builds where `build.rs` may not have run
/// or the baked-in path no longer exists):
///
/// 1. `COBRUST_STDLIB_ARCHIVE_PATH` compile-time env var (baked in by `build.rs`).
/// 2. `COBRUST_STDLIB_ARCHIVE` runtime env var override (for CI / test harness).
/// 3. Walk workspace `target/{release,debug}/libcobrust_stdlib.a`.
fn locate_stdlib_archive(release: bool) -> Result<PathBuf, BuildError> {
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
         install via `cargo install cobrust-cli` \
         or run `cargo build -p cobrust-stdlib` in the workspace first",
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
