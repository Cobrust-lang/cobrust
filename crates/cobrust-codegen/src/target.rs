//! [`TargetSpec`] + supporting selectors (per ADR-0023 §"Public surface").

use std::path::PathBuf;

use target_lexicon::Triple;

use crate::artifact::ArtifactKind;

/// Target specification — drives backend selection, optimization
/// level, output kind, and module identity.
///
/// The `triple` is parsed via `target-lexicon`; supported triples
/// at M9 are enumerated in ADR-0023 §"Target triple matrix".
#[derive(Clone, Debug)]
pub struct TargetSpec {
    /// Target triple (e.g. `x86_64-unknown-linux-gnu`).
    pub triple: Triple,
    /// Optimization level. Cranelift maps this to its own opt
    /// settings; LLVM maps to `-O0` / `-O2` / `-Oz`.
    pub opt_level: OptLevel,
    /// Backend selection. Post ADR-0070 §X.3 (RATIFIED 2026-05-26),
    /// [`Backend::Llvm`] is the default when the crate is built with
    /// the `llvm` feature (now in `default = ["llvm"]`); [`Backend::Cranelift`]
    /// remains the fallback when the feature is disabled.
    pub backend: Backend,
    /// Artifact kind (`Object` / `Executable` / `DynamicLibrary`).
    pub artifact: ArtifactKind,
    /// Output directory. The emitted file's name is derived from
    /// `module_name` + the platform extension.
    pub output_dir: PathBuf,
    /// Module name — used for the artifact filename and as the
    /// linker symbol prefix.
    pub module_name: String,
    /// Optional source-file path for DWARF emission (ADR-0058c §3.3).
    ///
    /// When `Some`, the LLVM backend builds a per-Span `LineMap` from
    /// the file's contents at emit time + emits per-statement
    /// `DILocation`s keyed against real (line, column) pairs. When
    /// `None` (default — most tests + synthetic modules), the DWARF
    /// emission falls back to `module_name` as the filename + `.` as
    /// the directory; line table collapses to 0/0 for every statement
    /// (DI structure still validates per `llvm-dwarfdump`).
    ///
    /// Cranelift backend ignores this field.
    pub source_path: Option<PathBuf>,
    /// Tier 1 runtime-dispatch multi-versioning
    /// (numerical-compute-hardware-tiering.md §Tier1).
    ///
    /// When `true`, the LLVM backend emits three specialisations of
    /// every top-level function:
    ///
    /// - `<fn>_v1_sse2`   — compiled with `+sse2` (x86_64 baseline)
    /// - `<fn>_v2_avx2`   — compiled with `+avx2,+fma`
    /// - `<fn>_v3_avx512` — compiled with `+avx512f,+avx512dq`
    ///
    /// A thin dispatcher `<fn>` is synthesised that calls the fastest
    /// available version detected at **startup** via Rust's safe macro
    /// `is_x86_feature_detected!` (no `unsafe`, no `#![forbid]`
    /// relaxation). On `aarch64` the flag is silently treated as
    /// single-version NEON-always-on (SVE multi-versioning is deferred
    /// per strategy doc §NEON/SVE).
    ///
    /// **Default**: `true` when `opt_level != OptLevel::None`
    /// (i.e. `cobrust build --release`). False on debug builds.
    ///
    /// Cranelift backend ignores this field.
    pub runtime_dispatch: bool,
    /// Tier 2 host-specific CPU tuning
    /// (numerical-compute-hardware-tiering.md §Tier 2).
    ///
    /// When `Some("native")`, LLVM auto-detects the host CPU and enables
    /// all available instruction-set extensions (no dispatch overhead;
    /// binary is host-only). When `Some(<name>)`, the named CPU string
    /// (e.g. `"skylake"`, `"apple-m1"`, `"neoverse-v1"`) is passed
    /// directly to `TargetMachine::create_target_machine`. When `None`
    /// (default), LLVM targets the `"generic"` baseline — same as
    /// current behaviour prior to Tier 2.
    ///
    /// Compatible with Tier 1 `runtime_dispatch`:
    /// - `None` + `runtime_dispatch=true`  → Tier 1 only (default `--release`).
    /// - `Some("native")` + `runtime_dispatch=false` → Tier 2 only.
    /// - `Some("native")` + `runtime_dispatch=true`  → both layers active.
    ///
    /// Cranelift backend ignores this field.
    pub target_cpu: Option<String>,
}

impl TargetSpec {
    /// A "host development build" target — uses the host triple,
    /// no opt, Cranelift, executable artifact, output to a temp dir.
    ///
    /// Useful for tests + CLI smoke checks.
    #[must_use]
    pub fn host_dev(output_dir: PathBuf, module_name: impl Into<String>) -> Self {
        Self {
            triple: Triple::host(),
            opt_level: OptLevel::None,
            backend: Backend::default_for_dev(),
            artifact: ArtifactKind::Executable,
            output_dir,
            module_name: module_name.into(),
            source_path: None,
            runtime_dispatch: false,
            target_cpu: None,
        }
    }

    /// A "host release build" target — host triple, full opt,
    /// LLVM if `--features llvm` else Cranelift, executable.
    #[must_use]
    pub fn host_release(output_dir: PathBuf, module_name: impl Into<String>) -> Self {
        Self {
            triple: Triple::host(),
            opt_level: OptLevel::Speed,
            backend: Backend::default_for_release(),
            artifact: ArtifactKind::Executable,
            output_dir,
            module_name: module_name.into(),
            source_path: None,
            runtime_dispatch: true,
            target_cpu: None,
        }
    }

    /// A "host object" target — emit a relocatable `.o` only.
    #[must_use]
    pub fn host_object(output_dir: PathBuf, module_name: impl Into<String>) -> Self {
        Self {
            triple: Triple::host(),
            opt_level: OptLevel::None,
            backend: Backend::default_for_dev(),
            artifact: ArtifactKind::Object,
            output_dir,
            module_name: module_name.into(),
            source_path: None,
            runtime_dispatch: false,
            target_cpu: None,
        }
    }
}

/// Optimization level passed to the backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum OptLevel {
    /// `-O0` — no opt; fastest compile, slowest run.
    #[default]
    None,
    /// Speed-focused opt (Cranelift `speed`, LLVM `-O2`).
    Speed,
    /// Speed + size opt (Cranelift `speed_and_size`, LLVM `-Oz`).
    SpeedAndSize,
}

/// Backend selector — Cranelift is the default; LLVM is opt-in
/// via `--features llvm`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Backend {
    /// Pure-Rust Cranelift backend (default for `cargo build`).
    Cranelift,
    /// LLVM via inkwell (requires `--features llvm`).
    Llvm,
}

impl Backend {
    /// Recommended default for development / debug builds.
    /// Post ADR-0070 §X.3 RATIFIED 2026-05-26: LLVM when available
    /// (now default per `default = ["llvm"]`), else Cranelift.
    #[must_use]
    pub fn default_for_dev() -> Self {
        if cfg!(feature = "llvm") {
            Backend::Llvm
        } else {
            Backend::Cranelift
        }
    }

    /// Recommended default for release builds: LLVM if available,
    /// else Cranelift.
    #[must_use]
    pub fn default_for_release() -> Self {
        if cfg!(feature = "llvm") {
            Backend::Llvm
        } else {
            Backend::Cranelift
        }
    }
}

impl Default for Backend {
    fn default() -> Self {
        // ADR-0070 §X.3 RATIFIED — LLVM default when feature available.
        if cfg!(feature = "llvm") {
            Backend::Llvm
        } else {
            Backend::Cranelift
        }
    }
}
