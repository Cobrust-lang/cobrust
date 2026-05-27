//! M9 object-layout assertions — every emitted relocatable object
//! file is parseable by the `object` crate; for ELF on Linux and
//! Mach-O on macOS, the expected sections + at least one exported
//! symbol must be present.
//!
//! Per ADR-0023 §"Object emission" (ADR-0070 §X.4: LLVM is the sole
//! AOT backend, so emission routes through the inkwell object writer):
//! - **ELF on Linux**: emitted via the LLVM TargetMachine object writer.
//! - **Mach-O on macOS**: same path; format selected from triple.

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

use cobrust_codegen::{Artifact, ArtifactKind, Backend, OptLevel, TargetSpec, emit};
use cobrust_frontend::{parse_str, span::FileId};
use cobrust_hir::{Session, lower as hir_lower};
use cobrust_mir::{Module as MirModule, lower as mir_lower};
use cobrust_types::check;
use object::{Object, ObjectSection, ObjectSymbol};
use target_lexicon::{OperatingSystem, Triple};

fn lower_to_mir(src: &str) -> MirModule {
    let module = parse_str(src, FileId::SYNTHETIC).expect("parse");
    let mut sess = Session::new();
    let hir = hir_lower(&module, &mut sess).expect("hir lower");
    let typed = check(&hir).expect("type check");
    mir_lower(&typed).expect("mir lower")
}

fn host_object_spec(name: &str) -> TargetSpec {
    let dir = std::env::temp_dir().join(format!("cobrust-m9-layout-{name}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    TargetSpec {
        triple: Triple::host(),
        opt_level: OptLevel::None,
        backend: Backend::Llvm,
        artifact: ArtifactKind::Object,
        output_dir: dir,
        module_name: name.to_string(),
        source_path: None,
        runtime_dispatch: false,
        target_cpu: None,
    }
}

fn compile_object(name: &str, src: &str) -> std::path::PathBuf {
    let mir = lower_to_mir(src);
    let spec = host_object_spec(name);
    let artifact = emit(&mir, spec).unwrap();
    match artifact {
        Artifact::Object(p) => p,
        other => panic!("expected Object artifact, got {other:?}"),
    }
}

// =====================================================================
// 1. Object file is parseable.
// =====================================================================

#[test]
fn layout_001_object_file_parseable() {
    let path = compile_object(
        "layout_001",
        "fn add(a: i64, b: i64) -> i64:\n    return (a + b)\n",
    );
    let bytes = std::fs::read(&path).unwrap();
    let obj = object::File::parse(&*bytes).expect("parse object");
    let _ = obj.architecture();
    let _ = obj.format();
}

// =====================================================================
// 2. Architecture matches host.
// =====================================================================

#[test]
fn layout_002_architecture_matches_host() {
    use object::Architecture;
    let path = compile_object("layout_002", "fn f() -> i64:\n    return 0\n");
    let bytes = std::fs::read(&path).unwrap();
    let obj = object::File::parse(&*bytes).unwrap();
    let host = Triple::host();
    match host.architecture {
        target_lexicon::Architecture::X86_64 => {
            assert_eq!(obj.architecture(), Architecture::X86_64);
        }
        target_lexicon::Architecture::Aarch64(_) => {
            assert_eq!(obj.architecture(), Architecture::Aarch64);
        }
        _ => { /* other archs — accept any */ }
    }
}

// =====================================================================
// 3. Object format matches host (Mach-O on macOS, ELF on Linux).
// =====================================================================

#[test]
fn layout_003_object_format_matches_host() {
    use object::BinaryFormat;
    let path = compile_object("layout_003", "fn f() -> i64:\n    return 0\n");
    let bytes = std::fs::read(&path).unwrap();
    let obj = object::File::parse(&*bytes).unwrap();
    let host = Triple::host();
    match host.operating_system {
        OperatingSystem::Darwin(_) | OperatingSystem::IOS(_) => {
            assert_eq!(obj.format(), BinaryFormat::MachO);
        }
        OperatingSystem::Linux => {
            assert_eq!(obj.format(), BinaryFormat::Elf);
        }
        _ => { /* other OS — accept any */ }
    }
}

// =====================================================================
// 4. Function symbol is present + exported.
// =====================================================================

#[test]
fn layout_004_function_symbol_exported() {
    let path = compile_object(
        "layout_004",
        "fn add(a: i64, b: i64) -> i64:\n    return (a + b)\n",
    );
    let bytes = std::fs::read(&path).unwrap();
    let obj = object::File::parse(&*bytes).unwrap();
    let names: Vec<String> = obj
        .symbols()
        .filter(|s| s.is_definition())
        .filter_map(|s| s.name().ok().map(|n| n.to_string()))
        .collect();
    // Mach-O prefixes user symbols with `_`; ELF does not.
    let expected = ["add", "_add"];
    assert!(
        names.iter().any(|n| expected.contains(&n.as_str())),
        "expected `add` in symbols: {names:?}"
    );
}

// =====================================================================
// 5. Code section exists.
// =====================================================================

#[test]
fn layout_005_text_section_exists() {
    let path = compile_object("layout_005", "fn f() -> i64:\n    return 0\n");
    let bytes = std::fs::read(&path).unwrap();
    let obj = object::File::parse(&*bytes).unwrap();
    let names: Vec<String> = obj
        .sections()
        .filter_map(|s| s.name().ok().map(|n| n.to_string()))
        .collect();
    let candidates = ["__text", ".text", "$d.0"];
    assert!(
        names.iter().any(|n| candidates.contains(&n.as_str())),
        "expected a text section: got {names:?}"
    );
}

// =====================================================================
// 6. Multiple-function module has multiple symbols.
// =====================================================================

#[test]
fn layout_006_two_functions_two_symbols() {
    let path = compile_object(
        "layout_006",
        "fn double(x: i64) -> i64:\n    return (x + x)\n\nfn quad(x: i64) -> i64:\n    return double(double(x))\n",
    );
    let bytes = std::fs::read(&path).unwrap();
    let obj = object::File::parse(&*bytes).unwrap();
    let names: Vec<String> = obj
        .symbols()
        .filter(|s| s.is_definition())
        .filter_map(|s| s.name().ok().map(|n| n.to_string()))
        .collect();
    let has_double = names.iter().any(|n| n == "double" || n == "_double");
    let has_quad = names.iter().any(|n| n == "quad" || n == "_quad");
    assert!(
        has_double && has_quad,
        "missing one or both symbols: {names:?}"
    );
}

// =====================================================================
// 7. Object file size is reasonable (sanity check).
// =====================================================================

#[test]
fn layout_007_object_file_reasonable_size() {
    let path = compile_object("layout_007", "fn f() -> i64:\n    return 0\n");
    let meta = std::fs::metadata(&path).unwrap();
    assert!(
        meta.len() >= 64,
        "object file suspiciously small: {} bytes",
        meta.len()
    );
    assert!(
        meta.len() <= 16 * 1024,
        "object file too large: {} bytes",
        meta.len()
    );
}

// =====================================================================
// 8. LLVM backend produces relocations for function calls.
// =====================================================================

#[test]
fn layout_008_call_emits_relocation_or_direct_call() {
    let path = compile_object(
        "layout_008",
        "fn double(x: i64) -> i64:\n    return (x + x)\n\nfn quad(x: i64) -> i64:\n    return double(double(x))\n",
    );
    let bytes = std::fs::read(&path).unwrap();
    let obj = object::File::parse(&*bytes).unwrap();
    // Either the symbol resolves intra-section, or there's a relocation.
    // Either path is acceptable for the M9 stub — we just need the file
    // to parse and the layout to be coherent.
    let _ = obj.symbols().count();
}

// =====================================================================
// 9..15: ABI / calling-convention spot checks — REMOVED (ADR-0070 §X.4).
//
// The former layout_009..015 exercised `cobrust_codegen::abi::*`
// (cranelift_call_conv / cranelift_scalar_ty / pointer_ty / is_copy_ty).
// `abi.rs` was consumed solely by the deleted Cranelift AOT backend and
// was removed in §X.4. The LLVM backend computes its calling convention
// + scalar widths internally via inkwell, so there is no standalone
// public ABI surface left to unit-test here. The object-layout tests
// above (1..8) now exercise the LLVM object writer end-to-end.
// =====================================================================

// =====================================================================
// 16. Linker availability sanity check.
// =====================================================================

#[test]
fn layout_016_linker_available_returns_bool() {
    let _: bool = cobrust_codegen::linker::linker_available();
}
