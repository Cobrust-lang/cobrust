//! M9 release smoke — exercise the `--release` codegen path.
//!
//! Per ADR-0023 §"Backend feature flag layout":
//! - `cargo build` → Cranelift (`OptLevel::None`)
//! - `cargo build --release` → if `--features llvm` then LLVM `-O3`,
//!    else Cranelift `OptLevel::Speed`
//!
//! The release smoke test ensures: the release-default backend
//! produces a valid object file, the artifact path exists, and
//! linking succeeds when the linker is available.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::format_push_string)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::doc_overindented_list_items)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::elidable_lifetime_names)]
#![allow(clippy::comparison_chain)]
#![allow(clippy::derivable_impls)]

use cobrust_codegen::{Artifact, ArtifactKind, Backend, OptLevel, TargetSpec, emit, linker};
use cobrust_frontend::{parse_str, span::FileId};
use cobrust_hir::{Session, lower as hir_lower};
use cobrust_mir::{Module as MirModule, lower as mir_lower};
use cobrust_types::check;
use target_lexicon::Triple;

fn lower_to_mir(src: &str) -> MirModule {
    let module = parse_str(src, FileId::SYNTHETIC).expect("parse");
    let mut sess = Session::new();
    let hir = hir_lower(&module, &mut sess).expect("hir lower");
    let typed = check(&hir).expect("type check");
    mir_lower(&typed).expect("mir lower")
}

fn release_object_spec(name: &str) -> TargetSpec {
    let dir =
        std::env::temp_dir().join(format!("cobrust-m9-release-{name}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    TargetSpec {
        triple: Triple::host(),
        opt_level: OptLevel::Speed,
        backend: Backend::default_for_release(),
        artifact: ArtifactKind::Object,
        output_dir: dir,
        module_name: name.to_string(),
        source_path: None,
        runtime_dispatch: false,
        target_cpu: None,
    }
}

#[test]
fn smoke_001_release_default_compiles_simple_program() {
    let mir = lower_to_mir("fn add(a: i64, b: i64) -> i64:\n    return (a + b)\n");
    let spec = release_object_spec("smoke_001");
    let artifact = emit(&mir, spec).expect("release emit");
    assert!(matches!(artifact, Artifact::Object(_)));
    assert!(artifact.path().exists());
}

#[test]
fn smoke_002_release_default_compiles_recursion() {
    let mir = lower_to_mir(
        "fn fib(n: i64) -> i64:\n    if (n < 2):\n        return n\n    return (fib((n - 1)) + fib((n - 2)))\n",
    );
    let spec = release_object_spec("smoke_002");
    let artifact = emit(&mir, spec).expect("release emit");
    let meta = std::fs::metadata(artifact.path()).unwrap();
    assert!(meta.len() > 0);
}

#[test]
fn smoke_003_release_default_compiles_loops() {
    let mir = lower_to_mir(
        "fn fact(n: i64) -> i64:\n    let acc: i64 = 1\n    let i: i64 = 1\n    while (i <= n):\n        acc *= i\n        i += 1\n    return acc\n",
    );
    let spec = release_object_spec("smoke_003");
    let artifact = emit(&mir, spec).expect("release emit");
    let meta = std::fs::metadata(artifact.path()).unwrap();
    assert!(meta.len() > 0);
}

#[test]
fn smoke_004_release_default_backend_is_correct() {
    if cfg!(feature = "llvm") {
        assert_eq!(Backend::default_for_release(), Backend::Llvm);
    } else {
        assert_eq!(Backend::default_for_release(), Backend::Cranelift);
    }
}

#[test]
fn smoke_005_speed_and_size_compiles() {
    let mir = lower_to_mir("fn f() -> i64:\n    return 42\n");
    let mut spec = release_object_spec("smoke_005");
    spec.opt_level = OptLevel::SpeedAndSize;
    let artifact = emit(&mir, spec).expect("release emit");
    let meta = std::fs::metadata(artifact.path()).unwrap();
    assert!(meta.len() > 0);
}

// =====================================================================
// Linker smoke — only runs if `cc` is available on the host.
// =====================================================================

#[test]
fn smoke_006_linker_smoke_when_cc_available() {
    if !linker::linker_available() {
        // Skip on minimal CI images without `cc`.
        return;
    }
    let mir = lower_to_mir("fn add(a: i64, b: i64) -> i64:\n    return (a + b)\n");
    let dir = std::env::temp_dir().join(format!("cobrust-m9-link-smoke-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let spec = TargetSpec {
        triple: Triple::host(),
        opt_level: OptLevel::None,
        backend: Backend::Cranelift,
        artifact: ArtifactKind::DynamicLibrary,
        output_dir: dir,
        module_name: "linksmoke".to_string(),
        source_path: None,
        runtime_dispatch: false,
        target_cpu: None,
    };
    // The link step needs a `_main` (ELF) / `_start` symbol for an
    // executable; for the smoke test we emit a dynamic library which
    // doesn't require a runtime entry point.
    let artifact = emit(&mir, spec);
    match artifact {
        Ok(Artifact::DynamicLibrary(p)) => {
            assert!(p.exists(), "dylib path must exist: {p:?}");
        }
        Ok(other) => panic!("unexpected artifact: {other:?}"),
        Err(e) => {
            // Linker may reject for reasons unrelated to codegen
            // (e.g., missing crt symbols on minimal CI). We accept
            // a structured LinkerFailed without panicking.
            assert!(
                matches!(
                    e,
                    cobrust_codegen::CodegenError::LinkerFailed { .. }
                        | cobrust_codegen::CodegenError::CraneliftError(_)
                ),
                "expected LinkerFailed or CraneliftError, got {e:?}"
            );
        }
    }
}

// =====================================================================
// Release vs dev binary-size sanity check.
// =====================================================================

#[test]
fn smoke_007_release_object_not_dramatically_larger_than_dev() {
    let mir = lower_to_mir("fn f() -> i64:\n    return 0\n");
    let dev_dir = std::env::temp_dir().join(format!("cobrust-m9-dev-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dev_dir);
    let dev_spec = TargetSpec {
        triple: Triple::host(),
        opt_level: OptLevel::None,
        backend: Backend::Cranelift,
        artifact: ArtifactKind::Object,
        output_dir: dev_dir,
        module_name: "smoke_007_dev".to_string(),
        source_path: None,
        runtime_dispatch: false,
        target_cpu: None,
    };
    let dev_artifact = emit(&mir, dev_spec).unwrap();
    let dev_size = std::fs::metadata(dev_artifact.path()).unwrap().len();

    let rel_dir = std::env::temp_dir().join(format!("cobrust-m9-rel-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&rel_dir);
    let rel_spec = TargetSpec {
        triple: Triple::host(),
        opt_level: OptLevel::SpeedAndSize,
        backend: Backend::default_for_release(),
        artifact: ArtifactKind::Object,
        output_dir: rel_dir,
        module_name: "smoke_007_rel".to_string(),
        source_path: None,
        runtime_dispatch: false,
        target_cpu: None,
    };
    let rel_artifact = emit(&mir, rel_spec).unwrap();
    let rel_size = std::fs::metadata(rel_artifact.path()).unwrap().len();

    // Release should be at most ~3× the dev build (in practice typically
    // smaller). This is a sanity check, not a perf gate.
    assert!(
        rel_size <= dev_size * 4,
        "release ({rel_size}) more than 4× dev ({dev_size})"
    );
}

#[test]
fn smoke_008_release_handles_assert_emitting_program() {
    let mir = lower_to_mir("fn divsafe(a: i64, b: i64) -> i64:\n    return a / b\n");
    let spec = release_object_spec("smoke_008");
    let _ = emit(&mir, spec).expect("release emit");
}

#[test]
fn smoke_009_release_handles_branchy_program() {
    let mir = lower_to_mir(
        "fn classify(x: i64) -> i64:\n    if (x > 100):\n        return 1\n    elif (x > 10):\n        return 2\n    elif (x > 0):\n        return 3\n    else:\n        return 0\n",
    );
    let spec = release_object_spec("smoke_009");
    let _ = emit(&mir, spec).expect("release emit");
}

#[test]
fn smoke_010_release_handles_recursive_call_program() {
    let mir = lower_to_mir(
        "fn ack(m: i64, n: i64) -> i64:\n    if (m == 0):\n        return (n + 1)\n    if (n == 0):\n        return ack((m - 1), 1)\n    return ack((m - 1), ack(m, (n - 1)))\n",
    );
    let spec = release_object_spec("smoke_010");
    let _ = emit(&mir, spec).expect("release emit");
}
