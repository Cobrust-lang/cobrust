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

    // F69 / CLAUDE.md §2.5 Direction-B — route the MIR-lowering error
    // (ownership / borrow / drop violations from `borrow_check`) through
    // the `error_ux` renderer instead of dumping the raw `{e:?}` Debug
    // repr (`UseAfterMove { local: 2, span: Span {..}, suggestion: ..}`).
    // `From<MirError> for UserError` (error_ux.rs) maps each variant's
    // construction-time `suggestion` to the rendered `hint:` line and
    // preserves the source span via `span_to_line_col`, so a use-after-move
    // prints the polished fix ("change to `&s` to borrow without
    // consuming") rather than internal field names. This honours
    // error_ux's own contract ("the raw internal representation … never
    // reaches the terminal") that the adjacent type / HIR / parse errors
    // already satisfy. The rendered text is carried in `BuildError::Type`
    // so the existing `{e}` print sites in `run` / `run.rs` / `pkg_build`
    // surface it verbatim (compiler-internal MirError variants —
    // UnresolvedDefId / Internal — route through `UserError::internal`
    // inside the From impl, keeping the bug-report path intact).
    let mut mir = mir_lower(&typed)
        .map_err(|e| BuildError::Type(crate::error_ux::UserError::from(e).to_string()))?;

    intrinsics::rewrite_print(&mut mir).map_err(|e| BuildError::Type(format!("{e}")))?;

    // ADR-0072 §2/§3 — the set of ecosystem modules this program imports
    // (drives per-import static linking of `lib<mod>.a`; risk 3 link bloat).
    let eco_modules = intrinsics::collect_ecosystem_modules(&mir);

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

    // ADR-0075 Phase 1 — F58-sibling guard: `target_cpu = native` resolves
    // to *host* CPU via LLVM `get_host_cpu_name`. That's meaningless when
    // cross-targeting (host == macOS-arm64 → "apple-m1"; target == riscv64
    // → must be a generic-rv64 baseline). When the triple isn't host AND
    // the caller passed `native`, rebind to `None` (generic baseline) and
    // emit a one-line stderr note so the override is auditable.
    let is_cross = triple != Triple::host();
    let effective_target_cpu: Option<String> = match (is_cross, target_cpu) {
        (true, Some("native")) => {
            if !quiet {
                eprintln!(
                    "cobrust build: ignoring `--target-cpu=native` on cross-target `{triple}` \
                     (host-CPU resolution is meaningless cross-arch); using generic baseline"
                );
            }
            None
        }
        (_, Some(s)) => Some(s.to_owned()),
        (_, None) => None,
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
        // ADR-0075 Phase 1 — runtime dispatch emits x86-feature dispatchers.
        // On cross-targets that aren't x86_64, the dispatcher is a no-op
        // already (`triple_is_x86_64` check); leaving the flag honest.
        runtime_dispatch: enable_runtime_dispatch.unwrap_or(release),
        // Tier 2: pass caller-supplied CPU string (or None for generic baseline).
        target_cpu: effective_target_cpu,
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

            // ADR-0075 Phase 1 — cross-target derived from `is_cross` computed
            // above the spec. Declared early so `ensure_runtime_object` +
            // `locate_stdlib_archive` + `locate_ecosystem_archive` +
            // `select_cc_resolved` all share the same value (single source of
            // truth — flipping host vs cross at any link-stage boundary
            // would leave host objects mixed with target objects).
            let cross_target: Option<&str> = if is_cross { target } else { None };
            let runtime_obj = ensure_runtime_object(&codegen_output_dir, cross_target)?;
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

            // `cross_target` was declared above ensure_runtime_object; reuse it.
            let stdlib_archive = locate_stdlib_archive(release, cross_target)?;
            let (cc, cc_prefix_args) = select_cc_resolved(cross_target)?;
            // Per ADR-0025 §"Runtime ABI": link order matters — user
            // object provides forward references resolved by stdlib
            // archive (which provides `__cobrust_print`,
            // `__cobrust_println`, etc.). On macOS the linker resolves
            // archives lazily; on Linux we use --whole-archive only if
            // strictly needed (M11 helpers are referenced at every
            // print/panic callsite, so plain archive resolution
            // suffices).
            let mut cmd = Command::new(&cc);
            for a in &cc_prefix_args {
                cmd.arg(a);
            }
            cmd.arg(&user_obj_path).arg(&runtime_obj);
            // ADR-0072 §2/§3 Q5 — per-import ecosystem static linking.
            // Both `libcobrust_stdlib.a` and each ecosystem `lib<mod>.a`
            // are Rust staticlibs that EACH embed a copy of libstd /
            // liballoc / panic runtime. Two ordering hazards therefore
            // collide:
            //   - ecosystem-AFTER-stdlib: macOS `ld` (multi-pass) resolves
            //     den's `__cobrust_str_*` back-references against the
            //     earlier stdlib fine, but single-pass GNU `ld` would not;
            //   - ecosystem-BEFORE-stdlib: both archives' embedded-libstd
            //     members get pulled → "duplicate symbols".
            // The portable fix: keep ecosystem archives AFTER the stdlib
            // archive (so the embedded-std de-dups against stdlib's, which
            // is pulled first) AND, on Linux only, wrap all archives in a
            // `--start-group/--end-group` so GNU ld iterates them to a
            // fixpoint (resolving den's `__cobrust_str_*` back-refs without
            // re-pulling duplicate std members). macOS `ld` is already
            // multi-pass and needs no group. Only imported modules link
            // (risk 3).
            let eco_archives = eco_modules
                .iter()
                .map(|m| locate_ecosystem_archive(m, release, cross_target))
                .collect::<Result<Vec<_>, _>>()?;
            // ADR-0075 Phase 1 — `target_os == "linux"` is a *host*-cfg
            // predicate; for cross-targets we must look at the *target*
            // OS instead so a macOS host targeting riscv64-linux-gnu
            // still emits the GNU `--start-group/--end-group` for the
            // archive ordering hazard.
            let target_is_linux = if let Some(t) = cross_target {
                t.contains("linux")
            } else {
                cfg!(target_os = "linux")
            };
            // ADR-0075 Phase 2 Sprint D — wasm32-wasip1 path. `wasm-ld`
            // (clang's default for wasm32) is single-pass but link order
            // is irrelevant: WASM is a self-contained module without the
            // archive ordering hazard GNU `ld` exhibits. Avoid emitting
            // GNU-ld-specific flags (`-Wl,--start-group/--end-group`)
            // and Linux-only libs (`-lpthread -ldl -lm`); the wasi-libc
            // sysroot bundled in clang's wasm32-wasip1 target already
            // provides the libc/mathlib surface, and there are no
            // threads in WASI preview 1.
            let target_is_wasm = if let Some(t) = cross_target {
                triple_is_wasm(t)
            } else {
                cfg!(target_arch = "wasm32") || cfg!(target_arch = "wasm64")
            };
            if target_is_linux && !target_is_wasm && !eco_archives.is_empty() {
                cmd.arg("-Wl,--start-group").arg(&stdlib_archive);
                for archive in &eco_archives {
                    cmd.arg(archive);
                }
                cmd.arg("-Wl,--end-group");
            } else {
                cmd.arg(&stdlib_archive);
                for archive in &eco_archives {
                    cmd.arg(archive);
                }
            }
            cmd.arg("-o").arg(&exe_path);
            // Platform: macOS doesn't need --no-as-needed; Linux needs
            // libpthread + libdl + libm pulled in for std + mimalloc.
            // ADR-0075 Phase 1 — apply Linux libs by *target* not host so a
            // macOS host targeting riscv64-linux-gnu pulls libpthread/dl/m.
            // ADR-0075 Phase 2 Sprint D — explicitly skip on wasm32 even
            // when the triple contains "linux"-ish substrings (none do
            // today, but the guard is cheap and protects future variants
            // like `wasm32-wasi-linux`).
            if target_is_linux && !target_is_wasm {
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
fn ensure_runtime_object(
    output_dir: &Path,
    cross_target: Option<&str>,
) -> Result<PathBuf, BuildError> {
    // T1.3: use the pre-compiled object baked in at build time (set by build.rs).
    // Uses option_env! so the crate compiles even without the build script having run.
    //
    // ADR-0075 Phase 1 — baked + env-override paths reflect the HOST
    // build's `cobrust_main.o`. A cross-target build must NEVER reuse
    // them; the embedded ELF header would mismatch the target arch.
    // Skip both fast paths when cross_target is set; fall through to a
    // fresh cross-cc compile under `output_dir`.
    if cross_target.is_none() {
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
    }

    // Development fallback (and cross-target path): compile from source.
    let runtime_obj_name = match cross_target {
        // Suffix the cross artifact so it doesn't collide with the host
        // shim if the same `output_dir` is reused by tests / repeat runs.
        Some(t) => format!("cobrust_main.{t}.o"),
        None => "cobrust_main.o".to_string(),
    };
    let runtime_obj = output_dir.join(&runtime_obj_name);
    if runtime_obj.exists() {
        return Ok(runtime_obj);
    }
    let runtime_src = locate_runtime_source()?;
    let (cc, cc_prefix_args) = select_cc_resolved(cross_target)?;
    let mut cmd = Command::new(&cc);
    for a in &cc_prefix_args {
        cmd.arg(a);
    }
    cmd.arg("-c")
        .arg(&runtime_src)
        .arg("-O0")
        .arg("-o")
        .arg(&runtime_obj);
    let status = cmd
        .status()
        .map_err(|e| BuildError::Internal(format!("compiling runtime helper with `{cc}`: {e}")))?;
    if !status.success() {
        return Err(BuildError::Internal(format!(
            "runtime-helper compilation failed via `{cc}`: status {status:?}; \
             (cross-target: {cross_target:?}; ensure the cross-cc is on PATH — \
             see docs/agent/setup/cross-toolchain.md)"
        )));
    }
    Ok(runtime_obj)
}

/// ADR-0075 Phase 1 — pick the C compiler driver for a given target.
///
/// Returns `(program, prefix_args)` so that `clang --target=<triple>`
/// (driver + leading args) can coexist with a plain `riscv64-linux-gnu-gcc`
/// driver (no prefix args).
///
/// Resolution order:
///
/// 1. **Per-target env override** — `COBRUST_CC_<TRIPLE>` where `<TRIPLE>` is
///    the upper-cased triple with `-` → `_` (e.g.
///    `COBRUST_CC_RISCV64GC_UNKNOWN_LINUX_GNU`). Highest priority so CI
///    can pin a specific cross-cc on a per-job basis.
/// 2. **Global `CC` env** — when set, used regardless of target. Matches
///    historical behaviour for host builds; on cross-targets, the user
///    is asserting "this CC handles the triple" (e.g.
///    `CC=clang` plus the appropriate `--target=` flag).
/// 3. **Convention-based cross-cc** — for cross-targets, derive from the
///    triple: `riscv64-linux-gnu-gcc` for `riscv64gc-unknown-linux-gnu`,
///    `aarch64-linux-gnu-gcc` for `aarch64-unknown-linux-gnu`, etc.
///    Probed with `--version`. Falls back to `clang --target=<triple>`.
/// 4. **Host default** — `cc`.
fn select_cc_resolved(cross_target: Option<&str>) -> Result<(String, Vec<String>), BuildError> {
    // ADR-0075 Phase 2 Sprint E — wasm32 needs a wasi-sysroot. Compute it
    // once here so EVERY cc-resolution branch below appends the same
    // `--sysroot=<path>` to its prefix args. Apt-installed `clang-18`
    // (`/usr/lib/llvm-18`) does NOT bundle a wasi-libc sysroot — it falls
    // back to host glibc headers (`bits/libc-header-start.h` not found),
    // which was the Sprint D live-CI failure. A real wasi-sysroot (from
    // wasi-sdk) fixes it; discovered via $COBRUST_WASI_SYSROOT / $WASI_SDK_PATH.
    let target_is_wasm = cross_target.is_some_and(triple_is_wasm);
    let wasm_sysroot: Option<String> = if target_is_wasm {
        Some(resolve_wasi_sysroot(
            cross_target.unwrap_or("wasm32-wasip1"),
        )?)
    } else {
        None
    };
    // Helper: fold the wasi `--sysroot` flag onto a base prefix-arg vec.
    let with_sysroot = |mut args: Vec<String>| -> Vec<String> {
        if let Some(sysroot) = &wasm_sysroot {
            args.push(format!("--sysroot={sysroot}"));
        }
        args
    };

    if let Some(triple) = cross_target {
        let env_key = format!("COBRUST_CC_{}", triple.replace('-', "_").to_uppercase());
        if let Ok(v) = std::env::var(&env_key) {
            if !v.is_empty() {
                // A user-set per-target CC may already embed `--sysroot`;
                // appending ours is still safe (clang takes the last
                // `--sysroot` wins / an identical path is idempotent).
                return Ok((v, with_sysroot(Vec::new())));
            }
        }
    }
    if let Ok(v) = std::env::var("CC") {
        if !v.is_empty() {
            return Ok((v, with_sysroot(Vec::new())));
        }
    }
    if let Some(triple) = cross_target {
        // ADR-0075 Phase 2 Sprint D/E — wasm32 targets short-circuit to
        // `clang --target=<triple> --sysroot=<wasi-sysroot>`. WASM has no
        // GNU cross-prefix convention (there is no `wasm32-wasi-gcc`);
        // clang is the canonical wasm32-wasip1 driver. The `wasm-ld`
        // linker ships with LLVM; the wasi-libc sysroot does NOT ship with
        // apt's clang-18, so `--sysroot` (resolved above) is mandatory.
        // Try `clang-18` (LLVM 18) first, then plain `clang` as fallback.
        if target_is_wasm {
            for cand in ["clang-18", "clang"] {
                if probe_cc_available(cand) {
                    return Ok((
                        cand.to_string(),
                        with_sysroot(vec![format!("--target={triple}")]),
                    ));
                }
            }
            return Err(BuildError::User(format!(
                "no wasm C compiler found for target `{triple}`. Tried: \
                 $COBRUST_CC_{} env, $CC env, `clang-18`, `clang`. \
                 Install LLVM 18+ clang (see docs/agent/setup/cross-toolchain.md) \
                 or set $CC.",
                triple.replace('-', "_").to_uppercase(),
            )));
        }
        // Convention: GNU cross-cc prefix derived from the triple's first 3
        // components (arch-vendor/os-libc → arch-libc/os-prefix style).
        //
        // riscv64gc-unknown-linux-gnu → riscv64-linux-gnu-gcc
        // aarch64-unknown-linux-gnu  → aarch64-linux-gnu-gcc
        if let Some(prefix) = derive_gnu_cross_prefix(triple) {
            let gcc = format!("{prefix}-gcc");
            if probe_cc_available(&gcc) {
                return Ok((gcc, Vec::new()));
            }
        }
        // Fallback: clang knows every triple as `--target=`. Caller will
        // hit a clean error if clang lacks the sysroot.
        if probe_cc_available("clang") {
            return Ok(("clang".to_string(), vec![format!("--target={triple}")]));
        }
        return Err(BuildError::User(format!(
            "no cross C compiler found for target `{triple}`. Tried: \
             $COBRUST_CC_{} env, $CC env, \
             `{}-gcc`, `clang --target={triple}`. \
             Install one (see docs/agent/setup/cross-toolchain.md) or set $CC.",
            triple.replace('-', "_").to_uppercase(),
            derive_gnu_cross_prefix(triple).unwrap_or_else(|| triple.to_string()),
        )));
    }
    Ok(("cc".to_string(), Vec::new()))
}

/// ADR-0075 Phase 2 Sprint E — resolve the wasi-libc sysroot for a wasm
/// cross-build.
///
/// Resolution order:
///
/// 1. **`COBRUST_WASI_SYSROOT`** — points directly at the sysroot dir
///    (the one containing `include/` + `lib/wasm32-wasi/`). Highest
///    priority so CI / users can pin an exact sysroot.
/// 2. **`WASI_SDK_PATH`** — the wasi-sdk install root; the sysroot lives
///    at `<WASI_SDK_PATH>/share/wasi-sysroot` (the canonical wasi-sdk
///    layout). Convenient when the whole SDK is installed.
///
/// Errors with an actionable message (pointing at the install doc) when
/// neither is set, OR when the resolved path doesn't exist on disk.
/// This converts the opaque clang `bits/libc-header-start.h file not
/// found` failure (Sprint D's live-CI break) into a clear, fix-shaped
/// diagnostic per CLAUDE.md §2.5-B.
fn resolve_wasi_sysroot(triple: &str) -> Result<String, BuildError> {
    if let Ok(p) = std::env::var("COBRUST_WASI_SYSROOT") {
        if !p.is_empty() {
            if Path::new(&p).is_dir() {
                return Ok(p);
            }
            return Err(BuildError::User(format!(
                "$COBRUST_WASI_SYSROOT is set to `{p}` but that directory does not \
                 exist. Point it at a real wasi-libc sysroot (the dir containing \
                 `include/` + `lib/wasm32-wasi/`); see docs/agent/setup/cross-toolchain.md."
            )));
        }
    }
    if let Ok(sdk) = std::env::var("WASI_SDK_PATH") {
        if !sdk.is_empty() {
            let sysroot = Path::new(&sdk).join("share").join("wasi-sysroot");
            if sysroot.is_dir() {
                return Ok(sysroot.to_string_lossy().into_owned());
            }
            return Err(BuildError::User(format!(
                "$WASI_SDK_PATH is `{sdk}` but `{}` (expected wasi-sysroot under the \
                 SDK) does not exist. Re-install wasi-sdk or set $COBRUST_WASI_SYSROOT \
                 to the sysroot dir directly; see docs/agent/setup/cross-toolchain.md.",
                sysroot.display(),
            )));
        }
    }
    Err(BuildError::User(format!(
        "cross-building for `{triple}` requires a wasi-libc sysroot, but neither \
         $COBRUST_WASI_SYSROOT nor $WASI_SDK_PATH is set. Apt's `clang-18` does NOT \
         bundle one (it falls back to host glibc headers and fails with \
         `bits/libc-header-start.h file not found`). Install wasi-sdk and set \
         $WASI_SDK_PATH (sysroot auto-derived at <SDK>/share/wasi-sysroot), or set \
         $COBRUST_WASI_SYSROOT directly; see docs/agent/setup/cross-toolchain.md."
    )))
}

/// Convert an LLVM-style triple `<arch>-<vendor>-<os>-<env>` to the
/// canonical GNU cross-cc prefix `<arch>-<os>-<env>` (drops `<vendor>`,
/// strips the rust-specific `gc` ISA-extension suffix on riscv).
///
/// `riscv64gc-unknown-linux-gnu` → `riscv64-linux-gnu`
/// `aarch64-unknown-linux-gnu`   → `aarch64-linux-gnu`
/// `x86_64-unknown-linux-gnu`    → `x86_64-linux-gnu`
fn derive_gnu_cross_prefix(triple: &str) -> Option<String> {
    let parts: Vec<&str> = triple.split('-').collect();
    if parts.len() < 4 {
        return None;
    }
    // Strip the rust-flavor ISA-extension suffix on riscv arches: `riscv64gc`
    // / `riscv32imc` etc. → `riscv64` / `riscv32` for the GNU prefix that the
    // Debian `gcc-riscv64-linux-gnu` package installs.
    let arch = parts[0];
    let arch_stripped = if arch.starts_with("riscv64") {
        "riscv64"
    } else if arch.starts_with("riscv32") {
        "riscv32"
    } else {
        arch
    };
    Some(format!("{}-{}-{}", arch_stripped, parts[2], parts[3]))
}

/// ADR-0075 Phase 2 — `true` when the cross triple targets WebAssembly
/// (`wasm32-*` / `wasm64-*`). Single predicate so the sysroot resolution,
/// the linker-flag guards, and the `--no-default-features` stdlib
/// cross-build all agree on what "wasm" means.
fn triple_is_wasm(triple: &str) -> bool {
    triple.starts_with("wasm32") || triple.starts_with("wasm64")
}

/// Returns `true` when `cc --version` runs successfully (binary present
/// + executable). Cheap probe; suppresses stdout/stderr.
fn probe_cc_available(cc: &str) -> bool {
    Command::new(cc)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
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
fn locate_stdlib_archive(release: bool, cross_target: Option<&str>) -> Result<PathBuf, BuildError> {
    // ADR-0075 Phase 1 — when cross-targeting, skip baked + wheel-layout
    // fast paths (host arch object code) and look directly under
    // `target/<triple>/<profile>/`. Build via subprocess when absent.
    if let Some(triple) = cross_target {
        return locate_or_build_cross_stdlib(release, triple);
    }

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

/// ADR-0075 Phase 1 — locate (or cross-build) `libcobrust_stdlib.a`
/// for a non-host target.
///
/// Lookup chain:
///
/// 1. Env override `COBRUST_STDLIB_ARCHIVE_<TRIPLE>` (CI prebuilt).
/// 2. `target/<triple>/<profile>/libcobrust_stdlib.a`.
/// 3. If missing: subprocess `cargo build -p cobrust-stdlib --target=<triple>
///    [--release]` and re-look. Requires the user to have already run
///    `rustup target add <triple>`; subprocess failure surfaces a clear
///    error pointing at the install command.
fn locate_or_build_cross_stdlib(release: bool, triple: &str) -> Result<PathBuf, BuildError> {
    let env_key = format!(
        "COBRUST_STDLIB_ARCHIVE_{}",
        triple.replace('-', "_").to_uppercase()
    );
    if let Ok(p) = std::env::var(&env_key) {
        let p = PathBuf::from(p);
        if p.exists() {
            return Ok(p);
        }
    }

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
    let candidate = target_dir
        .join(triple)
        .join(profile)
        .join("libcobrust_stdlib.a");
    if candidate.exists() {
        return Ok(candidate);
    }

    // Build via subprocess (dev convenience).
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut cmd = Command::new(&cargo);
    cmd.current_dir(workspace)
        .arg("build")
        .arg("-p")
        .arg("cobrust-stdlib")
        .arg("--target")
        .arg(triple);
    if release {
        cmd.arg("--release");
    }
    // ADR-0075 Phase 2 Sprint E / F70 — the stdlib default feature trio
    // (mimalloc-alloc / tokio-runtime / llm-router) does NOT build for
    // wasm32-wasip1 (native mimalloc + mio sockets + a TLS network stack
    // WASI preview 1 doesn't expose). The hello-world path needs none of
    // them, so drop them for wasm targets — otherwise this subprocess
    // fails on mimalloc and the user never reaches a working `.wasm`.
    // (CI pre-builds the archive with the same flag; this makes a clean
    // machine work too.) Full feature-on-wasm enablement is deferred per
    // F70.
    if triple_is_wasm(triple) {
        cmd.arg("--no-default-features");
    }
    let status = cmd
        .status()
        .map_err(|e| BuildError::Internal(format!("invoking `{cargo} build`: {e}")))?;
    if !status.success() {
        return Err(BuildError::User(format!(
            "cross-build of cobrust-stdlib for target `{triple}` failed \
             (`{cargo} build -p cobrust-stdlib --target {triple}` exited {status:?}). \
             Did you run `rustup target add {triple}` first? \
             See docs/agent/setup/cross-toolchain.md"
        )));
    }
    // macOS fs visibility race (CI #26580282088 2026-05-28): when parallel
    // `cobrust build` processes contend on `target/<triple>/.cargo-lock`, the
    // winning cargo's archive write may not be visible to the losing cargo's
    // `exists()` check for tens of milliseconds. Brief retry-with-backoff
    // before erroring; if still absent after 3×50ms, it really is missing.
    for _ in 0..3 {
        if candidate.exists() {
            return Ok(candidate);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Err(BuildError::Internal(format!(
        "cross-built cobrust-stdlib for `{triple}` but the archive is missing at {}",
        candidate.display()
    )))
}

/// ADR-0072 §2/§3 Q5 — locate (and, in a dev workspace, build) the
/// static archive `lib<module>.a` for an imported ecosystem module.
///
/// Mirrors [`locate_stdlib_archive`]'s lookup chain shape but for a
/// per-module archive:
///
/// - **Phase 0 (wheel-layout)** — `<install_prefix>/lib/cobrust/lib<mod>.a`.
/// - **Phase 1 (env override)** — `COBRUST_ECOSYSTEM_ARCHIVE_<MOD>`
///   (uppercased module name) for CI / test harnesses.
/// - **Phase 2 (workspace fallback)** — `target/{profile}/lib<mod>.a`;
///   if absent, run `cargo build -p cobrust-<mod>` to produce it (dev
///   convenience so `cobrust build prog.cb` works against a source tree
///   without a manual pre-build).
fn locate_ecosystem_archive(
    module: &str,
    release: bool,
    cross_target: Option<&str>,
) -> Result<PathBuf, BuildError> {
    let archive_name = format!("lib{module}.a");

    // ADR-0075 Phase 1 — cross-target ecosystem archives are sourced from
    // `target/<triple>/<profile>/`; built via subprocess when missing.
    if let Some(triple) = cross_target {
        let env_key = format!(
            "COBRUST_ECOSYSTEM_ARCHIVE_{}_{}",
            module.to_uppercase(),
            triple.replace('-', "_").to_uppercase()
        );
        if let Ok(p) = std::env::var(&env_key) {
            let p = PathBuf::from(p);
            if p.exists() {
                return Ok(p);
            }
        }
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
        let candidate = target_dir.join(triple).join(profile).join(&archive_name);
        if candidate.exists() {
            return Ok(candidate);
        }

        let crate_name = format!("cobrust-{module}");
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let mut cmd = Command::new(&cargo);
        cmd.current_dir(workspace)
            .arg("build")
            .arg("-p")
            .arg(&crate_name)
            .arg("--target")
            .arg(triple);
        if release {
            cmd.arg("--release");
        }
        let status = cmd
            .status()
            .map_err(|e| BuildError::Internal(format!("invoking `{cargo} build`: {e}")))?;
        if !status.success() {
            return Err(BuildError::User(format!(
                "cross-build of ecosystem module `{module}` for target `{triple}` failed \
                 (`{cargo} build -p {crate_name} --target {triple}` exited {status:?}). \
                 Did you run `rustup target add {triple}` first? \
                 See docs/agent/setup/cross-toolchain.md"
            )));
        }
        if candidate.exists() {
            return Ok(candidate);
        }
        return Err(BuildError::Internal(format!(
            "cross-built ecosystem `{module}` for `{triple}` but the archive is missing at {}",
            candidate.display()
        )));
    }

    // 0. Wheel-layout lookup (parity with libcobrust_stdlib.a).
    if let Some(p) = locate_wheel_lib_file(&archive_name) {
        if p.exists() {
            return Ok(p);
        }
    }

    // 1. Env override (CI / tests swap in a prebuilt archive).
    let env_key = format!("COBRUST_ECOSYSTEM_ARCHIVE_{}", module.to_uppercase());
    if let Ok(p) = std::env::var(&env_key) {
        let p = PathBuf::from(p);
        if p.exists() {
            return Ok(p);
        }
    }

    // 2. Workspace-relative fallback (dev builds).
    //
    // F44 (stale-green) — DO NOT short-circuit on a found
    // `target/<profile>/lib<mod>.a` here: a previously-built archive may be
    // STALE relative to the module's source (e.g. you edit `cobrust-pit`'s
    // C-ABI shim then rebuild only the bin, or `cargo test -p cobrust-cli`
    // rebuilds the module's rlib but NOT its staticlib). Linking the old
    // archive yields `undefined reference: __cobrust_<mod>_*` while a clean
    // full build is green — the classic stale-green that CI never catches.
    // Reaching this Phase-2 path ALREADY implies a dev workspace: the
    // installed/wheel case is served by Phase 0 (`locate_wheel_lib_file`)
    // and CI by Phase 1 (the `COBRUST_ECOSYSTEM_ARCHIVE_*` env override),
    // both of which return above before we get here. So it is safe — and
    // correct — to let cargo arbitrate staleness: run `cargo build -p
    // cobrust-<mod>` FIRST (a no-op when fresh; rebuilds the staticlib when
    // the source changed), THEN resolve the now-fresh archive.
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
    let candidate = target_dir.join(profile).join(&archive_name);

    // Dev convenience + F44 staleness arbitration: (re)build the staticlib
    // for the current profile. cargo is a no-op when the archive is already
    // fresh, so this does not regress incremental build times.
    let crate_name = format!("cobrust-{module}");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut build_cmd = Command::new(&cargo);
    build_cmd
        .current_dir(workspace)
        .arg("build")
        .arg("-p")
        .arg(&crate_name);
    if release {
        build_cmd.arg("--release");
    }
    let status = build_cmd
        .status()
        .map_err(|e| BuildError::Internal(format!("building {crate_name}: {e}")))?;
    if !status.success() {
        return Err(BuildError::Internal(format!(
            "failed to build ecosystem archive {archive_name} (`{cargo} build -p {crate_name}`)"
        )));
    }
    // macOS fs visibility race (CI #26580282088 2026-05-28 `libscale.a`):
    // when parallel `cobrust build` processes contend on cargo's package-cache
    // / target-dir lock, the winning cargo's archive write may not be visible
    // to the losing cargo's `exists()` check for tens of milliseconds. Brief
    // retry-with-backoff before erroring; same pattern as `locate_or_build_cross_stdlib`.
    for _ in 0..3 {
        if candidate.exists() {
            return Ok(candidate);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    // Last-resort defensive fallback: if the requested profile somehow did
    // not yield an archive but the OTHER profile has a prebuilt one, use it.
    // (Pre-F44 this was checked before building; kept here so a no-archive
    // edge case still degrades gracefully rather than hard-failing.)
    let other = if release { "debug" } else { "release" };
    let alt = target_dir.join(other).join(&archive_name);
    if alt.exists() {
        return Ok(alt);
    }
    Err(BuildError::Internal(format!(
        "cannot locate {archive_name} for ecosystem module `{module}` \
         (looked under {cand}); ensure `cobrust-{module}` declares the \
         `staticlib` crate-type (ADR-0072 Q5)",
        cand = candidate.display(),
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
    // F69 / §2.5 Direction-B — same error_ux routing as `build()` above so
    // this programmatic-use helper never leaks the raw MirError Debug repr.
    let mut mir = mir_lower(&typed)
        .map_err(|e| BuildError::Type(crate::error_ux::UserError::from(e).to_string()))?;
    intrinsics::rewrite_print(&mut mir).map_err(|e| BuildError::Type(format!("{e}")))?;
    Ok(mir)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// Serialize the env-var-touching tests below. The functions
    /// `resolve_wasi_sysroot` and `select_cc_resolved` read process-global
    /// env vars such as `COBRUST_WASI_SYSROOT`, `WASI_SDK_PATH`, and `CC`.
    /// Cargo runs tests in parallel by default, so a shared lock keeps
    /// concurrent set/remove from racing (F37 anti-flakiness discipline).
    static ENV_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Snapshot + clear the wasm-related env vars; restore on drop. Keeps
    /// each test hermetic regardless of the ambient CI environment (which
    /// DOES set these in the wasm32-cross-smoke job).
    struct WasmEnvScope {
        saved: Vec<(&'static str, Option<String>)>,
    }
    impl WasmEnvScope {
        fn clear() -> Self {
            let keys = [
                "COBRUST_WASI_SYSROOT",
                "WASI_SDK_PATH",
                "CC",
                "COBRUST_CC_WASM32_WASIP1",
            ];
            let saved = keys
                .iter()
                .map(|k| (*k, std::env::var(k).ok()))
                .collect::<Vec<_>>();
            for (k, _) in &saved {
                // SAFETY: test-only; serialized via ENV_GUARD. `remove_var`
                // is unsafe in Rust 2024 (no concurrent readers under lock).
                unsafe { std::env::remove_var(k) };
            }
            Self { saved }
        }
    }
    impl Drop for WasmEnvScope {
        fn drop(&mut self) {
            for (k, v) in &self.saved {
                // SAFETY: test-only restore under ENV_GUARD.
                match v {
                    Some(val) => unsafe { std::env::set_var(k, val) },
                    None => unsafe { std::env::remove_var(k) },
                }
            }
        }
    }

    #[test]
    fn emit_kind_default_executable() {
        // Smoke: kind enum is correctly compared.
        assert_ne!(EmitKind::Object, EmitKind::Executable);
    }

    #[test]
    fn mir_error_routes_through_error_ux_not_raw_debug() {
        // F69 / CLAUDE.md §2.5 Direction-B regression guard.
        //
        // `build()` lowers MIR via `mir_lower`, whose `borrow_check` pass
        // returns a structured `MirError` on an ownership violation. The
        // pre-F69 call site stringified it with `{e:?}`, leaking the raw
        // Debug repr (`UseAfterMove { local: 2, span: Span {..},
        // suggestion: Some(..) }`) to stderr — violating error_ux's "raw
        // internal representation never reaches the terminal" contract.
        //
        // This test reproduces the exact transformation the fixed call
        // site applies — `UserError::from(mir_err).to_string()` carried in
        // `BuildError::Type` — and asserts the rendered text is the
        // polished fix-suggestion, NOT the Debug field dump. It is immune
        // to the source-surface borrow-check gap that keeps the
        // `error_ux_snapshot.rs` snap_03 E2E case `#[ignore]`d (cross-
        // statement use-after-move is not yet flagged via `cobrust check`).
        use cobrust_frontend::span::{FileId, Span};
        use cobrust_mir::error::MirError;

        let mir_err = MirError::UseAfterMove {
            local: 2,
            span: Span::new(FileId::SYNTHETIC, 10, 12),
            suggestion: Some(
                "change to `&s` to borrow without consuming (ADR-0052a explicit shared borrow)",
            ),
        };

        // Exact path the fixed `build.rs` MIR call site takes.
        let build_err = BuildError::Type(crate::error_ux::UserError::from(mir_err).to_string());
        let rendered = format!("{build_err}");

        // The polished fix-suggestion (§2.5 Direction-B: print the FIX).
        assert!(
            rendered.contains("&s") && rendered.contains("borrow without consuming"),
            "MIR error must render the fix-suggestion via error_ux; got:\n{rendered}"
        );
        // The human-readable message, not internal field names.
        assert!(
            rendered.contains("use of moved value"),
            "MIR error must render the user-facing message; got:\n{rendered}"
        );
        // The raw Debug repr MUST NOT reach the terminal (error_ux contract).
        assert!(
            !rendered.contains("UseAfterMove {"),
            "raw MirError Debug repr leaked to user output (§2.5 / error_ux \
             contract violation); got:\n{rendered}"
        );
        // It must classify as a Type-tier error (exit code 2 per ADR-0024).
        assert_eq!(build_err.exit_code(), exit_codes::TYPE_ERROR);
    }

    #[test]
    fn host_target_never_gets_wasi_sysroot() {
        // ADR-0075 Phase 2 Sprint E — the wasi `--sysroot` must NEVER leak
        // onto a host build. `select_cc_resolved(None)` must return no
        // sysroot prefix arg even when WASI env vars are ambiently set.
        let _g = ENV_GUARD.lock().unwrap();
        let _scope = WasmEnvScope::clear();
        // SAFETY: serialized under ENV_GUARD; restored by `_scope` on drop.
        unsafe { std::env::set_var("WASI_SDK_PATH", "/nonexistent/wasi-sdk") };
        let (_cc, args) = select_cc_resolved(None).expect("host cc resolution");
        assert!(
            !args.iter().any(|a| a.starts_with("--sysroot")),
            "host build must not carry a wasi --sysroot; got {args:?}"
        );
    }

    #[test]
    fn resolve_wasi_sysroot_errors_when_unset() {
        // No env set → clear, fix-shaped error (CLAUDE.md §2.5-B), NOT a
        // silent fallback that would later fail deep in clang.
        let _g = ENV_GUARD.lock().unwrap();
        let _scope = WasmEnvScope::clear();
        let err = resolve_wasi_sysroot("wasm32-wasip1").expect_err("must error when unset");
        let msg = format!("{err}");
        assert!(msg.contains("wasi-libc sysroot"), "msg: {msg}");
        assert!(
            msg.contains("WASI_SDK_PATH"),
            "msg should name the env var: {msg}"
        );
    }

    #[test]
    fn resolve_wasi_sysroot_from_sdk_path_layout() {
        // $WASI_SDK_PATH → sysroot auto-derived at <SDK>/share/wasi-sysroot.
        let _g = ENV_GUARD.lock().unwrap();
        let _scope = WasmEnvScope::clear();
        let tmp = tempfile::tempdir().unwrap();
        let sysroot = tmp.path().join("share").join("wasi-sysroot");
        std::fs::create_dir_all(&sysroot).unwrap();
        // SAFETY: serialized under ENV_GUARD; restored on drop.
        unsafe { std::env::set_var("WASI_SDK_PATH", tmp.path()) };
        let resolved = resolve_wasi_sysroot("wasm32-wasip1").expect("derive from SDK path");
        assert_eq!(PathBuf::from(resolved), sysroot);
    }

    #[test]
    fn resolve_wasi_sysroot_direct_override_wins() {
        // $COBRUST_WASI_SYSROOT (direct) takes priority over $WASI_SDK_PATH.
        let _g = ENV_GUARD.lock().unwrap();
        let _scope = WasmEnvScope::clear();
        let tmp = tempfile::tempdir().unwrap();
        let direct = tmp.path().join("my-sysroot");
        std::fs::create_dir_all(&direct).unwrap();
        // SAFETY: serialized under ENV_GUARD; restored on drop.
        unsafe {
            std::env::set_var("COBRUST_WASI_SYSROOT", &direct);
            std::env::set_var("WASI_SDK_PATH", "/some/other/sdk");
        }
        let resolved = resolve_wasi_sysroot("wasm32-wasip1").expect("direct override");
        assert_eq!(PathBuf::from(resolved), direct);
    }
}
