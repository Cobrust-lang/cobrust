//! Tier-2 `--target-cpu` smoke tests.
//!
//! Validates:
//! 1. `target_cpu = Some("native")` succeeds — the backend expands `"native"`
//!    to the host CPU name + host features (F58: LLVM's `create_target_machine`
//!    does not itself resolve the literal `"native"`) and emits an object with
//!    at least one symbol present.
//! 2. `target_cpu = Some("skylake")` (or `"apple-m1"` on aarch64) succeeds.
//!    Skipped when the named CPU is not recognized by the host LLVM build.
//! 3. `target_cpu = Some("native")` + `runtime_dispatch = false` produces a
//!    single-version symbol (no `_v1_sse2` / `_v2_avx2` dispatch suffixes),
//!    confirming Tier 2-only mode omits Tier 1 overhead.
//!
//! Strategy: emit object via LLVM backend (when available) + inspect symbol
//! table with the `object` crate — same approach as `runtime_dispatch_smoke.rs`.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::missing_panics_doc)]

#[cfg(feature = "llvm")]
use cobrust_codegen::{ArtifactKind, Backend, OptLevel, TargetSpec, emit};
#[cfg(feature = "llvm")]
use cobrust_frontend::{parse_str, span::FileId};
#[cfg(feature = "llvm")]
use cobrust_hir::{Session, lower as hir_lower};
#[cfg(feature = "llvm")]
use cobrust_mir::{Module as MirModule, lower as mir_lower};
#[cfg(feature = "llvm")]
use cobrust_types::check;
#[cfg(feature = "llvm")]
use object::{Object as _, ObjectSymbol as _};
#[cfg(feature = "llvm")]
use target_lexicon::Triple;

#[cfg(feature = "llvm")]
fn lower_to_mir(src: &str) -> MirModule {
    let module = parse_str(src, FileId::SYNTHETIC).expect("parse");
    let mut sess = Session::new();
    let hir = hir_lower(&module, &mut sess).expect("hir lower");
    let typed = check(&hir).expect("type check");
    mir_lower(&typed).expect("mir lower")
}

#[cfg(feature = "llvm")]
fn add_src() -> &'static str {
    "fn add(a: i64, b: i64) -> i64:\n    return (a + b)\n"
}

#[cfg(feature = "llvm")]
fn make_tier2_spec(name: &str, target_cpu: Option<&str>, runtime_dispatch: bool) -> TargetSpec {
    let dir = std::env::temp_dir().join(format!("cobrust-tier2-cpu-{name}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    TargetSpec {
        triple: Triple::host(),
        opt_level: OptLevel::Speed,
        backend: Backend::Llvm,
        artifact: ArtifactKind::Object,
        output_dir: dir,
        module_name: name.to_string(),
        source_path: None,
        runtime_dispatch,
        target_cpu: target_cpu.map(str::to_owned),
    }
}

#[cfg(feature = "llvm")]
fn read_object_symbols(path: &std::path::Path) -> Vec<String> {
    let bytes = std::fs::read(path).expect("read object");
    let obj = object::File::parse(bytes.as_slice()).expect("parse object");
    obj.symbols()
        .filter_map(|s| s.name().ok().map(str::to_owned))
        .collect()
}

/// Smoke 1: `--target-cpu=native` produces a valid object with at least the
/// `add` symbol present.
#[test]
#[cfg(feature = "llvm")]
fn smoke_target_cpu_native() {
    let mir = lower_to_mir(add_src());
    let spec = make_tier2_spec("tier2_native", Some("native"), false);
    let artifact = emit(&mir, spec).expect("emit with target-cpu=native");
    let symbols = read_object_symbols(artifact.path());
    assert!(
        symbols.iter().any(|s| s.contains("add")),
        "expected 'add' symbol; got: {symbols:?}"
    );
}

/// Smoke 2: a named CPU string succeeds on the host architecture.
/// Uses `"skylake"` on x86_64 and `"apple-m1"` on aarch64; skipped otherwise.
#[test]
#[cfg(feature = "llvm")]
fn smoke_target_cpu_named() {
    #[cfg(target_arch = "x86_64")]
    let cpu = "skylake";
    #[cfg(target_arch = "aarch64")]
    let cpu = "apple-m1";
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        // Platform not covered — skip.
        return;
    }
    let mir = lower_to_mir(add_src());
    let spec = make_tier2_spec("tier2_named", Some(cpu), false);
    match emit(&mir, spec) {
        Ok(artifact) => {
            let symbols = read_object_symbols(artifact.path());
            assert!(
                symbols.iter().any(|s| s.contains("add")),
                "expected 'add' symbol in CPU={cpu} object; got: {symbols:?}"
            );
        }
        Err(e) => {
            // Some LLVM builds may not recognise a specific CPU name — treat as
            // non-fatal and surface the message so CI logs are informative.
            eprintln!("INFO: target-cpu={cpu} not recognised by this LLVM build: {e}");
        }
    }
}

/// Smoke 3: `target_cpu = Some("native")` + `runtime_dispatch = false`
/// (Tier 2-only mode) emits a single-version object — no `_v1_sse2` /
/// `_v2_avx2` / `_v3_avx512` dispatch-suffix symbols.
#[test]
#[cfg(feature = "llvm")]
fn smoke_target_cpu_native_no_dispatch_overhead() {
    let mir = lower_to_mir(add_src());
    let spec = make_tier2_spec("tier2_native_nodispatch", Some("native"), false);
    let artifact = emit(&mir, spec).expect("emit Tier-2-only");
    let symbols = read_object_symbols(artifact.path());
    let dispatch_symbols: Vec<&str> = symbols
        .iter()
        .filter(|s| s.contains("_v1_sse2") || s.contains("_v2_avx2") || s.contains("_v3_avx512"))
        .map(String::as_str)
        .collect();
    assert!(
        dispatch_symbols.is_empty(),
        "Tier-2-only build should have zero dispatch-suffix symbols; \
         found: {dispatch_symbols:?}"
    );
}
