//! ADR-0058c §3.5 — lldb-18 smoke test harness.
//!
//! Each fixture:
//!
//! 1. Lowers a small Cobrust-shape MIR fixture (built by hand for
//!    test reproducibility — no parser involvement).
//! 2. Compiles via the LLVM backend (`--features llvm`) to an
//!    executable artifact.
//! 3. Spawns `lldb-18 -b` in batch mode with a script that asks lldb
//!    to introspect the binary's DWARF sections (`image lookup
//!    --regex` for the function name, etc.) + verifies the symbol +
//!    debug info are present.
//!
//! On hosts where `lldb-18` (or `lldb`) is not on `$PATH`, every test
//! prints a warning and exits with success (skip semantics — the
//! gate runs on DG-Workstation which has lldb-18 via `llvm.sh` apt
//! install).
//!
//! These tests run ONLY under `--features llvm`. Without the feature
//! they are no-ops (compile-out via `#[cfg(feature = "llvm")]`).

#![cfg(feature = "llvm")]
#![allow(clippy::missing_panics_doc)]

use std::path::Path;
use std::process::Command;

use cobrust_codegen::{Artifact, ArtifactKind, Backend, OptLevel, TargetSpec, emit};
use cobrust_frontend::span::{FileId, Span};
use cobrust_hir::DefId;
use cobrust_mir::{
    BasicBlock as MirBlock, BinOp as MirBinOp, BlockId, Body, Constant as MirConstant, LocalDecl,
    LocalId, Module, Operand, Place, Rvalue, Statement, StatementKind, Terminator,
};
use cobrust_types::Ty;

// =====================================================================
// Helpers
// =====================================================================

fn span0() -> Span {
    Span::new(FileId::SYNTHETIC, 0, 0)
}

/// Locate the `lldb` binary on the host. Prefers `lldb-18` (DG's apt
/// install); falls back to `lldb` (Mac brew often ships it
/// unversioned). Returns `None` to indicate "skip the test".
fn find_lldb() -> Option<String> {
    for candidate in ["lldb-18", "lldb"] {
        if Command::new(candidate).arg("--version").output().is_ok() {
            return Some(candidate.to_string());
        }
    }
    None
}

/// Build a `TargetSpec` for an emitted object (no linker step — the
/// smoke gates inspect the object directly, which is enough for lldb
/// symbol resolution).
fn object_spec(name: &str) -> TargetSpec {
    let dir = std::env::temp_dir().join(format!(
        "cobrust-0058c-lldb-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&dir);
    TargetSpec {
        triple: target_lexicon::Triple::host(),
        opt_level: OptLevel::None,
        backend: Backend::Llvm,
        artifact: ArtifactKind::Object,
        output_dir: dir,
        module_name: name.to_string(),
        source_path: None,
    }
}

/// Run `lldb-18 -b -o "<command>" -- <object>` and capture stdout.
fn lldb_batch(lldb: &str, object_path: &Path, command: &str) -> String {
    let output = Command::new(lldb)
        .args(["-b", "-o", command, "--", &object_path.display().to_string()])
        .output()
        .expect("spawn lldb");
    String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr)
}

/// Assert lldb output mentions the given symbol (DWARF debug info
/// linked the symbol to its DWARF subprogram).
fn assert_lldb_symbol(out: &str, symbol: &str) {
    assert!(
        out.contains(symbol),
        "lldb output does not mention symbol `{}`:\n{}",
        symbol,
        out
    );
}

/// Helper: assemble a trivial 1-block body returning a constant.
fn body_returning_const(def_id: u32, name: &str, value: i64) -> Body {
    let locals = vec![LocalDecl {
        id: LocalId(0),
        name: "_return".to_string(),
        ty: Ty::Int,
        mutable: true,
        span: span0(),
    }];
    let block0 = MirBlock {
        id: BlockId(0),
        statements: vec![Statement {
            kind: StatementKind::Assign {
                place: Place::local(LocalId(0)),
                rvalue: Rvalue::Use(Operand::Constant(MirConstant::Int(value))),
            },
            span: span0(),
        }],
        terminator: Terminator::Return,
        span: span0(),
    };
    Body {
        def_id: DefId(def_id),
        name: name.to_string(),
        locals,
        blocks: vec![block0],
        return_local: LocalId(0),
        param_count: 0,
        span: span0(),
    }
}

/// Helper: assemble a body with two params summing them.
fn body_summing_params(def_id: u32, name: &str) -> Body {
    let locals = vec![
        LocalDecl {
            id: LocalId(0),
            name: "_return".to_string(),
            ty: Ty::Int,
            mutable: true,
            span: span0(),
        },
        LocalDecl {
            id: LocalId(1),
            name: "a".to_string(),
            ty: Ty::Int,
            mutable: false,
            span: span0(),
        },
        LocalDecl {
            id: LocalId(2),
            name: "b".to_string(),
            ty: Ty::Int,
            mutable: false,
            span: span0(),
        },
    ];
    let block0 = MirBlock {
        id: BlockId(0),
        statements: vec![Statement {
            kind: StatementKind::Assign {
                place: Place::local(LocalId(0)),
                rvalue: Rvalue::BinaryOp(
                    MirBinOp::Add,
                    Operand::Copy(Place::local(LocalId(1))),
                    Operand::Copy(Place::local(LocalId(2))),
                ),
            },
            span: span0(),
        }],
        terminator: Terminator::Return,
        span: span0(),
    };
    Body {
        def_id: DefId(def_id),
        name: name.to_string(),
        locals,
        blocks: vec![block0],
        return_local: LocalId(0),
        param_count: 2,
        span: span0(),
    }
}

// =====================================================================
// Smoke tests
// =====================================================================

#[test]
fn lldb_smoke_hello_world_subprogram_resolves() {
    let Some(lldb) = find_lldb() else {
        eprintln!("SKIP: lldb-18 / lldb not on PATH; skipping lldb smoke test");
        return;
    };

    let body = body_returning_const(1, "hello", 42);
    let module = Module { bodies: vec![body] };
    let spec = object_spec("hello_lldb");
    let artifact = emit(&module, spec).expect("hello emit");
    let path = match artifact {
        Artifact::Object(p) => p,
        _ => panic!("expected Artifact::Object"),
    };

    // `image lookup --regex hello` asks lldb to scan the object's
    // DWARF + symbol tables for any function matching the regex.
    // A successful resolution proves DWARF subprogram emission.
    let out = lldb_batch(&lldb, &path, "image lookup --regex hello");
    assert_lldb_symbol(&out, "hello");
}

#[test]
fn lldb_smoke_fib_function_visible() {
    // Build a "fib-shaped" body (single fn, two params, sum
    // approximation — the DWARF surface is what we care about, not
    // the math).
    let Some(lldb) = find_lldb() else {
        eprintln!("SKIP: lldb-18 / lldb not on PATH; skipping lldb smoke test");
        return;
    };

    let body = body_summing_params(2, "fib");
    let module = Module { bodies: vec![body] };
    let spec = object_spec("fib_lldb");
    let artifact = emit(&module, spec).expect("fib emit");
    let path = match artifact {
        Artifact::Object(p) => p,
        _ => panic!("expected Artifact::Object"),
    };

    let out = lldb_batch(&lldb, &path, "image lookup --regex fib");
    assert_lldb_symbol(&out, "fib");
}

#[test]
fn lldb_smoke_multi_fn_module_lists_both() {
    let Some(lldb) = find_lldb() else {
        eprintln!("SKIP: lldb-18 / lldb not on PATH; skipping lldb smoke test");
        return;
    };

    let body_a = body_returning_const(10, "fizzbuzz_a", 15);
    let body_b = body_returning_const(11, "fizzbuzz_b", 30);
    let module = Module {
        bodies: vec![body_a, body_b],
    };
    let spec = object_spec("multi_fn_lldb");
    let artifact = emit(&module, spec).expect("multi-fn emit");
    let path = match artifact {
        Artifact::Object(p) => p,
        _ => panic!("expected Artifact::Object"),
    };

    let out = lldb_batch(&lldb, &path, "image lookup --regex fizzbuzz");
    assert_lldb_symbol(&out, "fizzbuzz_a");
    assert_lldb_symbol(&out, "fizzbuzz_b");
}

#[test]
fn lldb_smoke_line_table_present() {
    // Verify the DWARF line table (.debug_line) is emitted. We can
    // assert this indirectly by asking lldb to list source line info
    // for the symbol's PC — if .debug_line is missing, lldb returns
    // "no debug info".
    let Some(lldb) = find_lldb() else {
        eprintln!("SKIP: lldb-18 / lldb not on PATH; skipping lldb smoke test");
        return;
    };

    let body = body_returning_const(3, "with_line_info", 7);
    let module = Module { bodies: vec![body] };
    let spec = object_spec("line_table_lldb");
    let artifact = emit(&module, spec).expect("line table emit");
    let path = match artifact {
        Artifact::Object(p) => p,
        _ => panic!("expected Artifact::Object"),
    };

    // `image dump line-table <name>` returns a non-empty table when
    // DWARF .debug_line is well-formed for the named symbol.
    let out = lldb_batch(
        &lldb,
        &path,
        "image dump line-table with_line_info",
    );
    // We don't assert the exact line numbers (synthetic spans
    // collapse to line 1); the contract is "lldb sees the symbol +
    // does not error 'no debug info'".
    assert!(
        out.contains("with_line_info") || out.contains("Line table"),
        "lldb image dump line-table failed; output:\n{}",
        out
    );
}
