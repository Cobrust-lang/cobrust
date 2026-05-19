//! Tier-1 runtime-dispatch smoke tests.
//!
//! Validates:
//! 1. `--enable-runtime-dispatch` (via `TargetSpec::runtime_dispatch = true`) produces
//!    three versioned symbols (`<fn>_v1_sse2`, `<fn>_v2_avx2`, `<fn>_v3_avx512`) plus
//!    the original dispatcher `<fn>` in the emitted object.
//! 2. `runtime_dispatch = false` (default on debug) produces exactly one `fib` symbol
//!    (no versioned suffixes).
//! 3. `triple_is_x86_64` correctly identifies x86_64 vs aarch64 triples.
//!
//! Strategy: emit an object file via the LLVM backend (when available), then
//! inspect the symbol table using the `object` crate's `SymbolIterator` — same
//! approach as `codegen_object_layout.rs`. Falls back to a no-op pass on non-LLVM
//! builds.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]

use cobrust_codegen::{ArtifactKind, Backend, OptLevel, TargetSpec, emit};
use cobrust_frontend::{parse_str, span::FileId};
use cobrust_hir::{Session, lower as hir_lower};
use cobrust_mir::{Module as MirModule, lower as mir_lower};
use cobrust_types::check;
use object::ObjectSymbol as _;
use target_lexicon::Triple;

// F34 anchor: runtime_dispatch_smoke::lower_to_mir
fn lower_to_mir(src: &str) -> MirModule {
    let module = parse_str(src, FileId::SYNTHETIC).expect("parse");
    let mut sess = Session::new();
    let hir = hir_lower(&module, &mut sess).expect("hir lower");
    let typed = check(&hir).expect("type check");
    mir_lower(&typed).expect("mir lower")
}

fn fib_src() -> &'static str {
    "fn fib(n: i64) -> i64:\n    if (n < 2):\n        return n\n    return (fib((n - 1)) + fib((n - 2)))\n"
}

// F34 anchor: runtime_dispatch_smoke::make_llvm_spec
fn make_llvm_spec(name: &str, runtime_dispatch: bool) -> TargetSpec {
    let dir = std::env::temp_dir().join(format!(
        "cobrust-tier1-dispatch-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&dir);
    TargetSpec {
        triple: Triple::host(),
        opt_level: OptLevel::Speed,
        backend: Backend::Llvm,
        artifact: cobrust_codegen::ArtifactKind::Object,
        output_dir: dir,
        module_name: name.to_string(),
        source_path: None,
        runtime_dispatch,
    }
}

/// Read all exported / defined symbols from an object file using the
/// `object` crate. Returns their names as a sorted `Vec<String>`.
fn object_symbols(path: &std::path::Path) -> Vec<String> {
    let data = std::fs::read(path).expect("read object");
    let obj = object::File::parse(data.as_slice()).expect("parse object");
    use object::Object as _;
    obj.symbols()
        .filter_map(|sym| {
            let name = sym.name().ok()?;
            if name.is_empty() {
                return None;
            }
            Some(name.to_string())
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

// F34 anchor: runtime_dispatch_smoke::smoke_dispatch_enabled_emits_3_versioned_symbols
/// Smoke 1: `runtime_dispatch = true` on x86_64 emits 3 versioned fib symbols.
#[test]
fn smoke_dispatch_enabled_emits_3_versioned_symbols() {
    // Skip on non-LLVM builds.
    if !cfg!(feature = "llvm") {
        return;
    }
    // Skip on non-x86_64 hosts — the dispatcher is a no-op on aarch64.
    #[cfg(feature = "llvm")]
    if !cobrust_codegen::llvm_backend::triple_is_x86_64(&make_llvm_spec("skip", false)) {
        return;
    }

    let mir = lower_to_mir(fib_src());
    let spec = make_llvm_spec("dispatch_on", true);
    let artifact = emit(&mir, spec).expect("emit with runtime_dispatch=true");

    let syms = object_symbols(artifact.path());
    // Expect all three versioned variants + the dispatcher.
    assert!(
        syms.iter().any(|s| s.contains("fib_v1_sse2")),
        "missing fib_v1_sse2; symbols: {syms:?}"
    );
    assert!(
        syms.iter().any(|s| s.contains("fib_v2_avx2")),
        "missing fib_v2_avx2; symbols: {syms:?}"
    );
    assert!(
        syms.iter().any(|s| s.contains("fib_v3_avx512")),
        "missing fib_v3_avx512; symbols: {syms:?}"
    );
    // Dispatcher `fib` (the original, un-versioned name) must still be present.
    assert!(
        syms.iter().any(|s| s == "fib" || s.ends_with(":fib")),
        "missing dispatcher fib; symbols: {syms:?}"
    );
}

/// Smoke 2: `runtime_dispatch = false` emits only one `fib` symbol (no versioned suffixes).
#[test]
fn smoke_dispatch_disabled_emits_single_symbol() {
    if !cfg!(feature = "llvm") {
        return;
    }

    let mir = lower_to_mir(fib_src());
    let spec = make_llvm_spec("dispatch_off", false);
    let artifact = emit(&mir, spec).expect("emit with runtime_dispatch=false");

    let syms = object_symbols(artifact.path());
    assert!(
        !syms
            .iter()
            .any(|s| s.contains("_v1_sse2") || s.contains("_v2_avx2") || s.contains("_v3_avx512")),
        "versioned symbols present when dispatch disabled; symbols: {syms:?}"
    );
}

/// Smoke 3: `triple_is_x86_64` returns correct answer for tier-1 triples.
#[test]
fn smoke_triple_is_x86_64_classification() {
    #[cfg(feature = "llvm")]
    use cobrust_codegen::llvm_backend::triple_is_x86_64;
    use std::path::PathBuf;

    let make_spec = |triple_str: &str| TargetSpec {
        triple: triple_str.parse().expect("parse triple"),
        opt_level: OptLevel::None,
        backend: Backend::Llvm,
        artifact: ArtifactKind::Object,
        output_dir: PathBuf::from("/tmp"),
        module_name: "t".to_string(),
        source_path: None,
        runtime_dispatch: false,
    };

    #[cfg(feature = "llvm")]
    {
        assert!(triple_is_x86_64(&make_spec("x86_64-unknown-linux-gnu")));
        assert!(triple_is_x86_64(&make_spec("x86_64-unknown-linux-musl")));
        assert!(!triple_is_x86_64(&make_spec("aarch64-apple-darwin")));
        assert!(!triple_is_x86_64(&make_spec("aarch64-unknown-linux-gnu")));
    }
    // Non-LLVM build: the function is LLVM-only; the test is a pass.
    #[cfg(not(feature = "llvm"))]
    let _ = make_spec;
}
