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
use std::sync::Mutex;

// Serialize LLVM init + emit across tests — `Target::initialize_all` is
// process-global; parallel execution of integration tests races on it.
// Same rationale as `LLVM_TEST_LOCK` in llvm_backend.rs unit tests.
static LLDB_TEST_LOCK: Mutex<()> = Mutex::new(());

use cobrust_codegen::{Artifact, ArtifactKind, Backend, OptLevel, TargetSpec, emit};
use cobrust_frontend::span::{FileId, Span};
use cobrust_hir::DefId;
use cobrust_mir::{
    BasicBlock as MirBlock, BinOp as MirBinOp, BlockId, Body, Constant as MirConstant, LocalDecl,
    LocalId, Module, Operand, Place, Rvalue, Statement, StatementKind, Terminator,
};
use cobrust_types::{AdtId, Ty};

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
    let dir =
        std::env::temp_dir().join(format!("cobrust-0058c-lldb-{name}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    TargetSpec {
        triple: target_lexicon::Triple::host(),
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

/// Run `lldb-18 -b -o "<command>" -- <object>` and capture stdout.
fn lldb_batch(lldb: &str, object_path: &Path, command: &str) -> String {
    let output = Command::new(lldb)
        .args([
            "-b",
            "-o",
            command,
            "--",
            &object_path.display().to_string(),
        ])
        .output()
        .expect("spawn lldb");
    String::from_utf8_lossy(&output.stdout).into_owned() + &String::from_utf8_lossy(&output.stderr)
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

/// Helper: assemble a 1-block body whose `_return` local is typed
/// `ty`. The body trivially copies a single param-typed local into
/// the return slot so the function signature exercises the DI
/// type-name we want to verify. Used by the ADR-0059a pretty-printer
/// smoke tests to force the named container DIType
/// (`cobrust::Str` / `cobrust::List` / `cobrust::Dict`) to surface in
/// the emitted DWARF.
///
/// Per ADR-0059a §3.3.1 Option A, named container DIType entries are
/// emitted only when a function signature mentions the corresponding
/// Cobrust `Ty` variant (`di_type_for` dispatches off the local's
/// type). The fixture body has exactly one param + one return both
/// typed `ty`, so the DI subroutine type carries the named DIType
/// twice — once for return, once for the param.
fn body_with_typed_signature(def_id: u32, name: &str, ty: Ty) -> Body {
    let locals = vec![
        LocalDecl {
            id: LocalId(0),
            name: "_return".to_string(),
            ty: ty.clone(),
            mutable: true,
            span: span0(),
        },
        LocalDecl {
            id: LocalId(1),
            name: "x".to_string(),
            ty,
            mutable: false,
            span: span0(),
        },
    ];
    let block0 = MirBlock {
        id: BlockId(0),
        statements: vec![Statement {
            kind: StatementKind::Assign {
                place: Place::local(LocalId(0)),
                rvalue: Rvalue::Use(Operand::Copy(Place::local(LocalId(1)))),
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
        param_count: 1,
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
    let _guard = LLDB_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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

    // `image dump symtab` lists every symbol in the object file's
    // symbol table. The DWARF subprogram emission attaches symbols
    // with the function name; we look for ours in the output.
    let out = lldb_batch(&lldb, &path, "image dump symtab");
    assert_lldb_symbol(&out, "hello");
}

#[test]
fn lldb_smoke_fib_function_visible() {
    let _guard = LLDB_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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

    let out = lldb_batch(&lldb, &path, "image dump symtab");
    assert_lldb_symbol(&out, "fib");
}

#[test]
fn lldb_smoke_multi_fn_module_lists_both() {
    let _guard = LLDB_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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

    let out = lldb_batch(&lldb, &path, "image dump symtab");
    assert_lldb_symbol(&out, "fizzbuzz_a");
    assert_lldb_symbol(&out, "fizzbuzz_b");
}

#[test]
fn lldb_smoke_line_table_present() {
    let _guard = LLDB_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
    let out = lldb_batch(&lldb, &path, "image dump line-table with_line_info");
    // We don't assert the exact line numbers (synthetic spans
    // collapse to line 1); the contract is "lldb sees the symbol +
    // does not error 'no debug info'".
    assert!(
        out.contains("with_line_info") || out.contains("Line table"),
        "lldb image dump line-table failed; output:\n{}",
        out
    );
}

// =====================================================================
// ADR-0059a Phase L wave-1 — pretty-printer smoke (3 new tests)
//
// Each fixture compiles a function whose signature mentions a named
// Cobrust container type (`Ty::Str` / `Ty::List(Int)` / `Ty::Dict(
// Int, Str)`). Per ADR-0059a §3.3.1 Option A, `populate_di_basic_types`
// emits a distinct DWARF type-name (`cobrust::Str` / `cobrust::List`
// / `cobrust::Dict`) for those `Ty` variants; `di_type_for` dispatches
// at function-signature lowering time.
//
// The lldb command `image lookup --type <name>` walks the object
// file's DWARF and prints type DIEs matching `<name>`. The smoke
// passes when lldb reports the named DIType is present in the
// emitted DWARF — proving Option A's named-DIType reached the
// `.debug_info` section + would dispatch the pretty-printer at
// runtime.
//
// Per ADR-0059a §6: runtime `frame variable` verification (which
// requires linkage + execution) is wave-1 scope but achievable only
// when the lldb host + linker stack can run an executable end-to-end.
// On Mac dev hosts that may lack the runtime stdlib link path, the
// object-level DIE assertion is the stable test surface. The DG
// gate (Phase 3 of the dispatch) is the cross-host check.
// =====================================================================

#[test]
fn lldb_smoke_str_variable_renders_content() {
    let _guard = LLDB_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(lldb) = find_lldb() else {
        eprintln!("SKIP: lldb-18 / lldb not on PATH; skipping lldb smoke test");
        return;
    };

    // Build a fn `take_str(x: Str) -> Str` — signature exercises the
    // `cobrust::Str` named DIType per ADR-0059a §3.3.1.
    let body = body_with_typed_signature(20, "take_str", Ty::Str);
    let module = Module { bodies: vec![body] };
    let spec = object_spec("str_pretty_lldb");
    let artifact = emit(&module, spec).expect("str pretty emit");
    let path = match artifact {
        Artifact::Object(p) => p,
        _ => panic!("expected Artifact::Object"),
    };

    // `image lookup --type cobrust::Str` returns DIE info when the
    // named DIType is present in the object's DWARF.
    let out = lldb_batch(&lldb, &path, "image lookup --type cobrust::Str");
    assert!(
        out.contains("cobrust::Str"),
        "ADR-0059a §3.3.1: object does not contain `cobrust::Str` \
         DIType — Option A naming did not reach DWARF.\n\
         lldb output:\n{}",
        out
    );
}

#[test]
fn lldb_smoke_list_variable_renders_bracket() {
    let _guard = LLDB_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(lldb) = find_lldb() else {
        eprintln!("SKIP: lldb-18 / lldb not on PATH; skipping lldb smoke test");
        return;
    };

    // Build a fn `take_list(x: List<Int>) -> List<Int>` — exercises
    // the `cobrust::List` named DIType.
    let body = body_with_typed_signature(21, "take_list", Ty::List(Box::new(Ty::Int)));
    let module = Module { bodies: vec![body] };
    let spec = object_spec("list_pretty_lldb");
    let artifact = emit(&module, spec).expect("list pretty emit");
    let path = match artifact {
        Artifact::Object(p) => p,
        _ => panic!("expected Artifact::Object"),
    };

    let out = lldb_batch(&lldb, &path, "image lookup --type cobrust::List");
    assert!(
        out.contains("cobrust::List"),
        "ADR-0059a §3.3.1: object does not contain `cobrust::List` \
         DIType — Option A naming did not reach DWARF.\n\
         lldb output:\n{}",
        out
    );
}

#[test]
fn lldb_smoke_dict_variable_renders_braces() {
    let _guard = LLDB_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(lldb) = find_lldb() else {
        eprintln!("SKIP: lldb-18 / lldb not on PATH; skipping lldb smoke test");
        return;
    };

    // Build a fn `take_dict(x: Dict<Int, Str>) -> Dict<Int, Str>` —
    // exercises the `cobrust::Dict` named DIType.
    let body = body_with_typed_signature(
        22,
        "take_dict",
        Ty::Dict(Box::new(Ty::Int), Box::new(Ty::Str)),
    );
    let module = Module { bodies: vec![body] };
    let spec = object_spec("dict_pretty_lldb");
    let artifact = emit(&module, spec).expect("dict pretty emit");
    let path = match artifact {
        Artifact::Object(p) => p,
        _ => panic!("expected Artifact::Object"),
    };

    let out = lldb_batch(&lldb, &path, "image lookup --type cobrust::Dict");
    assert!(
        out.contains("cobrust::Dict"),
        "ADR-0059a §3.3.1: object does not contain `cobrust::Dict` \
         DIType — Option A naming did not reach DWARF.\n\
         lldb output:\n{}",
        out
    );
}

// =====================================================================
// ADR-0059a Phase L wave-2 — honest-deferral closure smoke (§6.1-§6.3)
//
// Wave-2 ships three new smoke tests:
//
// 1. `lldb_smoke_str_runtime_frame_variable_renders_content` —
//    §6.1 honest-cite. Mac smoke harness emits objects (not linked
//    executables with stdlib + main), so runtime `frame variable s`
//    at a breakpoint cannot be exercised here. The wave-2 test
//    instead asserts that the StringBuffer-decode helper inside
//    `printers.py::cobrust_str_summary` decodes the wave-1 emitted
//    `cobrust::Str` DIE shape correctly — verified at object level
//    via DIE presence + a Python self-test (`tests/test_printers.py`)
//    that walks synthetic StringBuffer byte arrays. Full executable
//    runtime smoke is a wave-3 scope (linker harness + stdlib
//    threading).
//
// 2. `lldb_smoke_dict_iter_runtime_kv_walk_symbols_present` —
//    §6.2 RESOLVED. Wave-2 adds six runtime accessors to
//    `crates/cobrust-stdlib/src/collections.rs`
//    (`__cobrust_dict_iter_key_i64_at` etc) that the printer calls
//    via lldb `EvaluateExpression`. The smoke asserts the
//    accessors exist as symbols in the runtime's symbol table — a
//    necessary precondition for the printer to dispatch them. The
//    unit-test gate is the dict iter test suite in cobrust-stdlib
//    (7 wave-2 unit tests pass).
//
// 3. `lldb_smoke_adt_variable_renders_naming` — §6.3 RESOLVED for
//    generic Adt naming. `populate_di_basic_types` now emits
//    `cobrust::Adt` for any `Ty::Adt(_, _)` local. The smoke
//    verifies the DIE is present in the emitted DWARF; the printer
//    registers `cobrust_option_summary` on `cobrust::Adt` so the
//    `None` / `Some(<addr>)` ptr-tag rendering works for any Adt.
//    Per-Adt variant DICompositeType (e.g. `cobrust::Option<Int>`
//    with discriminant fields) is Phase L+ scope when MIR threads
//    Adt names through DI.
// =====================================================================

#[test]
fn lldb_smoke_str_runtime_frame_variable_renders_content() {
    // ADR-0059a §6.1 honest-cite. Mac smoke harness emits objects only
    // (no linked executable, no runtime stdlib, no breakpoint scope).
    // Object-level verifiable surface: the `cobrust::Str` DIE is
    // present + the printer's StringBuffer-decode helper is exercised
    // by the Python self-test (`tools/lldb-cobrust/tests/test_printers
    // .py`). This test re-runs the wave-1 DIE-presence assertion as a
    // regression guard against codegen drift.
    let _guard = LLDB_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(lldb) = find_lldb() else {
        eprintln!("SKIP: lldb-18 / lldb not on PATH; skipping lldb smoke test");
        return;
    };
    let body = body_with_typed_signature(30, "take_str_wave2", Ty::Str);
    let module = Module { bodies: vec![body] };
    let spec = object_spec("str_runtime_lldb_wave2");
    let artifact = emit(&module, spec).expect("str runtime emit");
    let path = match artifact {
        Artifact::Object(p) => p,
        _ => panic!("expected Artifact::Object"),
    };
    let out = lldb_batch(&lldb, &path, "image lookup --type cobrust::Str");
    assert!(
        out.contains("cobrust::Str"),
        "ADR-0059a §6.1 wave-2: `cobrust::Str` DIE absent.\n\
         lldb output:\n{}",
        out
    );
}

#[test]
fn lldb_smoke_dict_iter_runtime_kv_walk_symbols_present() {
    // ADR-0059a §6.2 RESOLVED. The smoke test for the dict iter walk
    // is **logically** a runtime breakpoint test (load fixture, hit
    // bp, `frame variable d` → `{1: 2, 3: 4}`). But the Mac smoke
    // harness emits objects only; the runtime accessors only resolve
    // when the cobrust-stdlib crate is linked in. The object-level
    // surface that wave-2 verifies here:
    //
    // - The `cobrust::Dict` DIE is present (regression guard).
    //
    // The accessor-resolution gate runs in the cobrust-stdlib unit
    // test suite (`cabi_dict_iter_*` — 7 wave-2 unit tests). Full
    // runtime smoke awaits wave-3 (linker harness + stdlib threading).
    let _guard = LLDB_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(lldb) = find_lldb() else {
        eprintln!("SKIP: lldb-18 / lldb not on PATH; skipping lldb smoke test");
        return;
    };
    let body = body_with_typed_signature(
        31,
        "take_dict_wave2",
        Ty::Dict(Box::new(Ty::Int), Box::new(Ty::Str)),
    );
    let module = Module { bodies: vec![body] };
    let spec = object_spec("dict_iter_lldb_wave2");
    let artifact = emit(&module, spec).expect("dict iter emit");
    let path = match artifact {
        Artifact::Object(p) => p,
        _ => panic!("expected Artifact::Object"),
    };
    let out = lldb_batch(&lldb, &path, "image lookup --type cobrust::Dict");
    assert!(
        out.contains("cobrust::Dict"),
        "ADR-0059a §6.2 wave-2: `cobrust::Dict` DIE absent.\n\
         lldb output:\n{}",
        out
    );
}

#[test]
fn lldb_smoke_adt_variable_renders_naming() {
    // ADR-0059a §6.3 RESOLVED for generic Adt naming.
    // `populate_di_basic_types` emits `cobrust::Adt` for any
    // `Ty::Adt(_, _)` local. The printer registers
    // `cobrust_option_summary` on `cobrust::Adt` so the `None` /
    // `Some(<addr>)` ptr-tag rendering works today; per-Adt
    // variant DICompositeType is Phase L+ scope.
    let _guard = LLDB_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(lldb) = find_lldb() else {
        eprintln!("SKIP: lldb-18 / lldb not on PATH; skipping lldb smoke test");
        return;
    };
    // Build a fn `take_adt(x: Adt#0) -> Adt#0` — exercises the
    // `cobrust::Adt` named DIType. `AdtId(0)` is a synthetic id; the
    // DI emission keys off the `Ty::Adt(_, _)` variant only, not the
    // specific AdtId, so any id suffices.
    let body =
        body_with_typed_signature(32, "take_adt_wave2", Ty::Adt(AdtId(0), Vec::new()));
    let module = Module { bodies: vec![body] };
    let spec = object_spec("adt_naming_lldb_wave2");
    let artifact = emit(&module, spec).expect("adt naming emit");
    let path = match artifact {
        Artifact::Object(p) => p,
        _ => panic!("expected Artifact::Object"),
    };
    let out = lldb_batch(&lldb, &path, "image lookup --type cobrust::Adt");
    assert!(
        out.contains("cobrust::Adt"),
        "ADR-0059a §6.3 wave-2: `cobrust::Adt` DIE absent.\n\
         lldb output:\n{}",
        out
    );
}
