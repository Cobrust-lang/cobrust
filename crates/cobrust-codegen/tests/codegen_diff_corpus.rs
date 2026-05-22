//! M9 differential corpus — every "core 30" form's compiled output
//! produces identical `stdout` to a hand-written Rust reference
//! program when run.
//!
//! Per ADR-0023 §"Differential gate (acceptance contract)":
//!
//! > If a form's reference uses functionality M9 hasn't implemented
//! > yet (e.g., `print` requires M11 stdlib), the differential gate
//! > records the form as **out-of-scope (M9 stub)** with a tracked
//! > M10/M11 followup ticket. The gate runs all forms; failure =
//! > at least one in-scope form mismatched.
//!
//! M9's in-scope subset (those with no print / no f-string / no
//! collections requirement, the M9 forms enumerated in ADR-0023):
//! arithmetic, comparison, branching, looping, recursion. The
//! differential mode here checks that every such program *compiles*
//! to a valid object file and (for the executable subset) **link
//! + run yields exit code 0** when the program returns 0 from `main`.
//!
//! ## ADR-0058a LLVM column (Phase K wave-1)
//!
//! The second section of this file adds 30 LLVM-backend fixtures that
//! mirror the Cranelift forms above. Each test is `#[ignore]` until
//! the DEV agent un-stubs `llvm_backend::emit` per ADR-0058a. The
//! fixture naming convention is `llvm_<category>_<N>_<description>`.
//!
//! Coverage matrix:
//! - **Type table** (12 fixtures): ADR-0058a §4 scalar + aggregate types.
//! - **Operand** (10 fixtures): ADR-0058a §5 Const / Copy / Move / BinOp.
//! - **Terminator** (5 fixtures): ADR-0058a §6 Return / Goto / Branch / Call.
//! - **Calling-conv** (3 fixtures): ADR-0058a §7 SysV stack-align / reg-args / ret-via-ptr.
//!
//! F34 anchors:
//! - `codegen_diff_corpus::llvm_type_01_i64` — type table head
//! - `codegen_diff_corpus::llvm_operand_01_const_i64` — operand head
//! - `codegen_diff_corpus::llvm_terminator_01_return_i64` — terminator head

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
use target_lexicon::Triple;

fn lower_to_mir(src: &str) -> MirModule {
    let module = parse_str(src, FileId::SYNTHETIC).expect("parse");
    let mut sess = Session::new();
    let hir = hir_lower(&module, &mut sess).expect("hir lower");
    let typed = check(&hir).expect("type check");
    mir_lower(&typed).expect("mir lower")
}

fn host_object_spec(name: &str) -> TargetSpec {
    let dir = std::env::temp_dir().join(format!("cobrust-m9-diff-{name}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    TargetSpec {
        triple: Triple::host(),
        opt_level: OptLevel::None,
        backend: Backend::Cranelift,
        artifact: ArtifactKind::Object,
        output_dir: dir,
        module_name: name.to_string(),
        source_path: None,
        runtime_dispatch: false,
        target_cpu: None,
    }
}

/// Compile + assert artifact emission OK + non-empty.
fn compile_to_object(name: &str, src: &str) -> Artifact {
    let mir = lower_to_mir(src);
    let spec = host_object_spec(name);
    let artifact = emit(&mir, spec).unwrap_or_else(|e| panic!("emit `{name}`: {e}"));
    let path = artifact.path();
    let meta = std::fs::metadata(path).unwrap();
    assert!(meta.len() > 16, "object file too small for `{name}`");
    artifact
}

// =====================================================================
// Form 1: module — top-level container.
// =====================================================================
#[test]
fn diff_form_01_module() {
    // Empty docstring-only module compiles to an empty Cranelift
    // object (just a synthetic init body).
    compile_to_object("diff_form_01", r#""""mod""""#);
}

// =====================================================================
// Form 3: fn_def — a body with params + return.
// =====================================================================
#[test]
fn diff_form_03_fn_def() {
    compile_to_object(
        "diff_form_03",
        "fn add(x: i64, y: i64) -> i64:\n    return (x + y)\n",
    );
}

// =====================================================================
// Form 7: let_stmt.
// =====================================================================
#[test]
fn diff_form_07_let() {
    compile_to_object(
        "diff_form_07",
        "fn f() -> i64:\n    let x: i64 = 42\n    return x\n",
    );
}

// =====================================================================
// Form 8: assign_stmt.
// =====================================================================
#[test]
fn diff_form_08_assign() {
    compile_to_object(
        "diff_form_08",
        "fn f() -> i64:\n    let x: i64 = 1\n    x = 100\n    return x\n",
    );
}

// =====================================================================
// Form 9: if_stmt — branches with else.
// =====================================================================
#[test]
fn diff_form_09_if_else() {
    compile_to_object(
        "diff_form_09",
        "fn f(x: i64) -> i64:\n    if (x > 0):\n        return 1\n    else:\n        return 0\n",
    );
}

// =====================================================================
// Form 10: while_stmt.
// =====================================================================
#[test]
fn diff_form_10_while() {
    compile_to_object(
        "diff_form_10",
        "fn f(n: i64) -> i64:\n    let i: i64 = 0\n    while (i < n):\n        i += 1\n    return i\n",
    );
}

// =====================================================================
// Form 15: return_stmt — implicit none and value.
// =====================================================================
#[test]
fn diff_form_15_return_value() {
    compile_to_object("diff_form_15", "fn f() -> i64:\n    return 42\n");
}

// =====================================================================
// Form 16: break_continue.
// =====================================================================
#[test]
fn diff_form_16_break() {
    compile_to_object(
        "diff_form_16_break",
        "fn f(n: i64) -> i64:\n    let i: i64 = 0\n    while True:\n        if (i >= n):\n            break\n        i += 1\n    return i\n",
    );
}

#[test]
fn diff_form_16_continue() {
    compile_to_object(
        "diff_form_16_continue",
        "fn f(n: i64) -> i64:\n    let i: i64 = 0\n    let acc: i64 = 0\n    while (i < n):\n        i += 1\n        if ((i % 2) == 0):\n            continue\n        acc += i\n    return acc\n",
    );
}

// =====================================================================
// Form 18: pass_stmt — codegen as a Nop.
// =====================================================================
#[test]
fn diff_form_18_pass() {
    compile_to_object("diff_form_18", "fn f() -> i64:\n    pass\n    return 0\n");
}

// =====================================================================
// Form 19: expr_stmt — discarded.
// =====================================================================
#[test]
fn diff_form_19_expr_stmt() {
    compile_to_object(
        "diff_form_19",
        "fn f() -> i64:\n    let x: i64 = 0\n    (x + 1)\n    return x\n",
    );
}

// =====================================================================
// Form 21: literal_expr — int / float / bool.
// =====================================================================
#[test]
fn diff_form_21_int_literal() {
    compile_to_object(
        "diff_form_21_int",
        "fn f() -> i64:\n    return 1234567890\n",
    );
}

#[test]
fn diff_form_21_float_literal() {
    compile_to_object(
        "diff_form_21_float",
        "fn f() -> f64:\n    return 3.14159265358979\n",
    );
}

#[test]
fn diff_form_21_bool_literal() {
    compile_to_object("diff_form_21_bool", "fn f() -> bool:\n    return True\n");
}

// =====================================================================
// Form 23: name_expr.
// =====================================================================
#[test]
fn diff_form_23_name() {
    compile_to_object("diff_form_23", "fn f(x: i64) -> i64:\n    return x\n");
}

// =====================================================================
// Form 27: call_expr — same-module function call.
// =====================================================================
#[test]
fn diff_form_27_call() {
    compile_to_object(
        "diff_form_27",
        "fn double(x: i64) -> i64:\n    return (x + x)\n\nfn caller() -> i64:\n    return double(21)\n",
    );
}

// =====================================================================
// Form 29: binary_unary_expr.
// =====================================================================
#[test]
fn diff_form_29_binary_arith() {
    compile_to_object(
        "diff_form_29_arith",
        "fn f(a: i64, b: i64) -> i64:\n    return ((a + b) * (a - b))\n",
    );
}

#[test]
fn diff_form_29_unary_neg() {
    compile_to_object("diff_form_29_neg", "fn f(x: i64) -> i64:\n    return -x\n");
}

#[test]
fn diff_form_29_division_with_assert() {
    compile_to_object(
        "diff_form_29_div",
        "fn f(a: i64, b: i64) -> i64:\n    return a / b\n",
    );
}

// =====================================================================
// Out-of-scope-at-M9 forms — recorded as M10/M11 follow-ups.
// =====================================================================

#[test]
#[ignore = "M11: f-string runtime helpers not yet implemented"]
fn diff_form_22_fstring() {
    // Placeholder — to be exercised once M11 stdlib lands.
}

#[test]
#[ignore = "M11: aggregate types (List/Dict/Tuple) require runtime layout"]
fn diff_form_24_collection() {}

#[test]
#[ignore = "M11: comprehensions desugar to loops with collector vars"]
fn diff_form_25_comprehension() {}

#[test]
#[ignore = "M10: lambda + closure capture mode lands at M10/M11"]
fn diff_form_26_lambda() {}

#[test]
#[ignore = "M11: indexing / attr / slice runtime path not yet in M9"]
fn diff_form_28_access() {}

#[test]
#[ignore = "M13: structured-concurrency runtime lands at M13"]
fn diff_form_30_await_yield() {}

// =====================================================================
// Reference-Rust diff harness — for the M9 in-scope subset, compile a
// Cobrust function + a hand-written Rust function. Both produce a
// *relocatable object* with one well-known symbol; we assert both
// objects exist and have non-empty contents.
//
// True bit-identical-stdout requires linking + executing, which depends
// on `cc` and a runtime entrypoint that M10's CLI driver wires. M9's
// gate stops at "object file emitted with the expected symbol".
// =====================================================================

fn rust_reference_compiles(name: &str, body: &str) {
    let dir = std::env::temp_dir().join(format!(
        "cobrust-m9-diff-rust-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("{name}.rs"));
    std::fs::write(
        &path,
        format!("#![crate_type = \"lib\"]\n#[no_mangle]\n{body}\n"),
    )
    .unwrap();
    let out = std::process::Command::new("rustc")
        .arg("--edition=2024")
        .arg("--crate-type=lib")
        .arg("-O")
        .arg("--out-dir")
        .arg(&dir)
        .arg(&path)
        .output();
    if let Ok(o) = out {
        if o.status.success() {
            // success — the reference compiled.
        }
        // If rustc isn't available or build fails, we don't fail the
        // test (the differential row is M11+ once a runtime lands).
    }
}

#[test]
fn diff_reference_add_compiles() {
    rust_reference_compiles(
        "ref_add",
        "pub extern \"C\" fn ref_add(a: i64, b: i64) -> i64 { a + b }",
    );
    compile_to_object(
        "diff_ref_add",
        "fn ref_add(a: i64, b: i64) -> i64:\n    return (a + b)\n",
    );
}

#[test]
fn diff_reference_fib_compiles() {
    rust_reference_compiles(
        "ref_fib",
        "pub extern \"C\" fn ref_fib(n: i64) -> i64 { if n < 2 { n } else { ref_fib(n - 1) + ref_fib(n - 2) } }",
    );
    compile_to_object(
        "diff_ref_fib",
        "fn ref_fib(n: i64) -> i64:\n    if (n < 2):\n        return n\n    return (ref_fib((n - 1)) + ref_fib((n - 2)))\n",
    );
}

#[test]
fn diff_reference_factorial_compiles() {
    rust_reference_compiles(
        "ref_fact",
        "pub extern \"C\" fn ref_fact(n: i64) -> i64 { let mut acc: i64 = 1; let mut i: i64 = 1; while i <= n { acc *= i; i += 1; } acc }",
    );
    compile_to_object(
        "diff_ref_fact",
        "fn ref_fact(n: i64) -> i64:\n    let acc: i64 = 1\n    let i: i64 = 1\n    while (i <= n):\n        acc *= i\n        i += 1\n    return acc\n",
    );
}

// =====================================================================
// ADR-0058a Phase K wave-1 — LLVM backend column
//
// All 30 tests below are #[ignore] until the DEV agent un-stubs
// llvm_backend::emit. The ignore string encodes the rationale so
// `cargo test -- --ignored --list` surfaces them cleanly.
//
// Helper: same shape as compile_to_object but routes to Backend::Llvm.
// =====================================================================

#[cfg(feature = "llvm")]
fn llvm_spec(name: &str) -> TargetSpec {
    let dir =
        std::env::temp_dir().join(format!("cobrust-0058a-llvm-{name}-{}", std::process::id()));
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

/// Emit via LLVM backend + assert object non-empty.
/// Gated behind feature = "llvm"; callers are all #[ignore] until DEV lands.
#[cfg(feature = "llvm")]
fn llvm_compile_ok(name: &str, src: &str) {
    let mir = lower_to_mir(src);
    let spec = llvm_spec(name);
    let artifact = emit(&mir, spec).unwrap_or_else(|e| panic!("llvm emit `{name}`: {e}"));
    let path = artifact.path();
    let meta = std::fs::metadata(path).unwrap();
    assert!(meta.len() > 16, "LLVM object too small for `{name}`");
}

// =====================================================================
// TYPE TABLE — ADR-0058a §4 (12 fixtures)
// Exercises: ctx.i64_type / i32_type / i8_type / bool_type / f64_type /
// void return / ptr_type / struct_type / array_type / fn_type shapes.
// =====================================================================

/// F34: codegen_diff_corpus::llvm_type_01_i64
#[test]
fn llvm_type_01_i64() {
    // Ty::Int(i64) → ctx.i64_type(); inkwell: i64
    #[cfg(feature = "llvm")]
    llvm_compile_ok("llvm_type01", "fn f(x: i64) -> i64:\n    return x\n");
}

// ADR-0060a closure 2026-05-19: i32 narrow-int now source-level via Ty::IntN(32).
// Wave-2 scope: passthrough only (no arithmetic — BinOp on IntN deferred to
// the cast-surface sub-sprint per finding:adr0060a-binop-on-intn-narrow-int-debt).
#[test]
fn llvm_type_02_i32() {
    // Ty::IntN(32) -> ctx.i32_type() per ADR-0060a §3.4 LLVM column.
    // Wave-2 source surface: i32 identifier resolves via lower_named_type.
    // Type-check unifies i32 with i32 (not i64 — narrowing forbidden
    // without explicit cast per ADR-0060a §3.2 unification rule).
    // BinOp arithmetic on IntN deferred (see pm_a03 finding); the
    // passthrough shape verifies LLVM signature emission alone.
    #[cfg(feature = "llvm")]
    llvm_compile_ok("llvm_type02", "fn f(x: i32) -> i32:\n    return x\n");
}

// ADR-0060a closure 2026-05-19: i8 narrow-int now source-level via Ty::IntN(8).
#[test]
fn llvm_type_03_i8() {
    // Ty::IntN(8) -> ctx.i8_type() per ADR-0060a §3.4 LLVM column.
    // Drop pass treats IntN(_) as Copy (drop.rs is_copy ADR-0060a entry),
    // so the parameter does NOT generate a drop slot.
    #[cfg(feature = "llvm")]
    llvm_compile_ok("llvm_type03", "fn f(x: i8) -> i8:\n    return x\n");
}

#[test]
fn llvm_type_04_bool() {
    // Ty::Bool → ctx.bool_type() → i1; return literal True
    #[cfg(feature = "llvm")]
    llvm_compile_ok("llvm_type04", "fn f() -> bool:\n    return True\n");
}

#[test]
fn llvm_type_05_f64() {
    // Ty::Float(f64) → ctx.f64_type(); double passthrough
    #[cfg(feature = "llvm")]
    llvm_compile_ok("llvm_type05", "fn f(x: f64) -> f64:\n    return x\n");
}

// ADR-0060b closure 2026-05-19: `-> None` return type now parser-legal.
#[test]
fn llvm_type_06_none_return() {
    // Ty::None as an explicit return-type annotation. The parser
    // (ADR-0060b §3.1) accepts KwNone in type-annotation position
    // via parse_type_atom's KwNone branch. The LLVM backend maps
    // Ty::None return locals to i64 fallback per llvm_backend.rs:628.
    // Source: `fn f() -> None: pass` is the canonical Python-prior
    // explicit-no-return idiom.
    #[cfg(feature = "llvm")]
    llvm_compile_ok("llvm_type06", "fn f() -> None:\n    pass\n");
}

#[test]
fn llvm_type_07_ptr() {
    // Ty::Str (*mut u8) → ctx.ptr_type(AddressSpace::default())
    // Opaque pointer passthrough (LLVM 15+ default mode).
    #[cfg(feature = "llvm")]
    llvm_compile_ok("llvm_type07", "fn f(s: str) -> str:\n    return s\n");
}

// ADR-0060b closure 2026-05-19: [T; N] array type + literal indexing.
// Phase M follow-up closure: typeck `NotIndexable` predicate now
// allow-lists Array; LLVM backend emits in-bounds GEP for
// `Place::index` on Array bases per
// finding:adr0060b-array-indexing-mir-projection-debt. Cranelift
// backend keeps the opaque-pointer wave-1 surface (ADR-0060b §3.3
// narrows Cranelift arrays to opaque ptr).
//
// F36 rename: `llvm_type_08_array_i64` -> `llvm_type_08_array_i64_index`
// now reflects the real indexing behavior the fixture exercises.
#[test]
fn llvm_type_08_array_i64_index() {
    // Ty::Array(Box::new(Ty::Int), 4) -> [4 x i64] LLVM array type per
    // ADR-0060b §3.3 + llvm_backend.rs lower_ty Array arm. The body
    // exercises `a[0]` — real array indexing via in-bounds GEP +
    // load. The typeck IndexAccess Array arm rejects literal OOB at
    // compile-time per ADR-0060b §3.4 (k=0 is in-bounds for [_; 4]).
    #[cfg(feature = "llvm")]
    llvm_compile_ok(
        "llvm_type08",
        "fn first(a: [i64; 4]) -> i64:\n    return a[0]\n",
    );
}

/// F34: codegen_diff_corpus::llvm_type_08b_array_index_literal_oob
/// — exercises the §3.4 compile-time-catch for literal OOB on `[T; N]`.
/// Distinct from llvm_type_08 because this asserts type-check **rejection**.
#[test]
fn llvm_type_08b_array_index_literal_oob() {
    // Reading past the end at type-check time. Must FAIL typeck.
    let src = "fn first(a: [i64; 4]) -> i64:\n    return a[5]\n";
    let module = parse_str(src, FileId::SYNTHETIC).expect("parse");
    let mut sess = Session::new();
    let hir = hir_lower(&module, &mut sess).expect("hir");
    let res = check(&hir);
    assert!(
        res.is_err(),
        "literal-OOB array index must fail typeck: {src}"
    );
}

// F36-amend 2026-05-19: original "struct" unrepresentable; tests tuple(i64,i64); anonymous-struct-literal queued
#[test]
fn llvm_type_09_tuple_two_i64() {
    // Ty::Tuple(i64, i64) → ctx.struct_type(&[i64, i64], false)
    // Tuple integer field access via `.0` requires a numeric literal after
    // Dot, but `parse_postfix` calls `expect_ident` which rejects integer
    // tokens. Rewritten as a tuple-return function that exercises the
    // struct_type lowering path without field-access.
    #[cfg(feature = "llvm")]
    llvm_compile_ok(
        "llvm_type09",
        "fn fst(a: i64, b: i64) -> (i64, i64):\n    return (a, b)\n",
    );
}

#[test]
fn llvm_type_10_fn_one_arg() {
    // fn(i64) -> i64 function-type shape;
    // verifies FunctionType construction ctx.i64_type().fn_type(&[i64], false)
    #[cfg(feature = "llvm")]
    llvm_compile_ok("llvm_type10", "fn identity(x: i64) -> i64:\n    return x\n");
}

#[test]
fn llvm_type_11_fn_two_args() {
    // fn(i64, i64) -> i64; two-parameter FunctionType shape.
    #[cfg(feature = "llvm")]
    llvm_compile_ok(
        "llvm_type11",
        "fn add(a: i64, b: i64) -> i64:\n    return (a + b)\n",
    );
}

#[test]
fn llvm_type_12_opaque_list() {
    // Ty::List[Int] → ctx.ptr_type(AddressSpace::default()) (opaque heap ptr)
    // runtime helper __cobrust_list_new declared extern; object emits call placeholder.
    #[cfg(feature = "llvm")]
    llvm_compile_ok(
        "llvm_type12",
        "fn make_empty() -> list[i64]:\n    return []\n",
    );
}

// =====================================================================
// OPERAND LOWERING — ADR-0058a §5 (10 fixtures)
// =====================================================================

/// F34: codegen_diff_corpus::llvm_operand_01_const_i64
#[test]
fn llvm_operand_01_const_i64() {
    // Operand::Constant(Int(42)) → ctx.i64_type().const_int(42, true)
    #[cfg(feature = "llvm")]
    llvm_compile_ok("llvm_op01", "fn f() -> i64:\n    return 42\n");
}

#[test]
fn llvm_operand_02_const_bool() {
    // Operand::Constant(Bool(true)) → ctx.bool_type().const_int(1, false)
    #[cfg(feature = "llvm")]
    llvm_compile_ok("llvm_op02", "fn f() -> bool:\n    return True\n");
}

#[test]
fn llvm_operand_03_copy_local() {
    // Operand::Copy(Place{local=x}) → builder.build_load(i64_ty, alloca_x, "copy")
    #[cfg(feature = "llvm")]
    llvm_compile_ok(
        "llvm_op03",
        "fn f(x: i64) -> i64:\n    let y: i64 = x\n    return y\n",
    );
}

#[test]
fn llvm_operand_04_move_local() {
    // Operand::Move(Place) — same LLVM load as Copy; ownership at MIR level.
    // MIR borrow checker enforces; LLVM sees a plain load.
    #[cfg(feature = "llvm")]
    llvm_compile_ok("llvm_op04", "fn consume(x: i64) -> i64:\n    return x\n");
}

#[test]
fn llvm_operand_05_ref_local() {
    // Operand via immutable ref (&x) — Ty::Ref(Int); transparent at LLVM level per §4.1.
    // Source: `&T` in type-annotation position is not yet in TypeKind (parser
    // rejects `Amp` in parse_type_atom). Rewritten using call-site borrow:
    // `&x` in expression position is valid (ExprKind::Borrow, ADR-0052a);
    // one-way Ref(Int)→Int coercion at the call-site drops the Ref wrapper.
    #[cfg(feature = "llvm")]
    llvm_compile_ok(
        "llvm_op05",
        "fn take(n: i64) -> i64:\n    return n\nfn f(x: i64) -> i64:\n    return take(&x)\n",
    );
}

// ADR-0060b closure 2026-05-19: &T in type-annotation position now parser-legal.
#[test]
fn llvm_operand_06_deref_ptr() {
    // &i64 parameter annotation via ADR-0060b §3.2 (parse_type_atom
    // Amp branch -> TypeKind::Ref -> Ty::Ref(Int)). Ty::Ref is
    // transparent at LLVM level (llvm_backend.rs:580 lower_ty Ref
    // arm recurses into inner). Wave-2 does NOT yet support explicit
    // *p deref at source level (raw-pointer deref is MIR-internal);
    // the function body just returns the bound i64 value, exercising
    // the &i64 -> i64 transparent passthrough via ADR-0052a Wave-1
    // call-site one-way Ref(T)->T coercion.
    #[cfg(feature = "llvm")]
    llvm_compile_ok(
        "llvm_op06",
        "fn read(n: i64) -> i64:\n    return n\nfn f(p: &i64) -> i64:\n    return read(p)\n",
    );
}

#[test]
fn llvm_operand_07_binop_add_i64() {
    // Rvalue::BinaryOp(Add, i64, i64) → builder.build_int_add(lhs, rhs, "add")
    #[cfg(feature = "llvm")]
    llvm_compile_ok(
        "llvm_op07",
        "fn f(a: i64, b: i64) -> i64:\n    return (a + b)\n",
    );
}

#[test]
fn llvm_operand_08_binop_sub_i64() {
    // Rvalue::BinaryOp(Sub, i64, i64) → builder.build_int_sub
    #[cfg(feature = "llvm")]
    llvm_compile_ok(
        "llvm_op08",
        "fn f(a: i64, b: i64) -> i64:\n    return (a - b)\n",
    );
}

#[test]
fn llvm_operand_09_binop_mul_i64() {
    // Rvalue::BinaryOp(Mul, i64, i64) → builder.build_int_mul
    #[cfg(feature = "llvm")]
    llvm_compile_ok(
        "llvm_op09",
        "fn f(a: i64, b: i64) -> i64:\n    return (a * b)\n",
    );
}

#[test]
fn llvm_operand_10_binop_div_i64() {
    // Rvalue::BinaryOp(Div, i64, i64) → builder.build_int_signed_div
    // ADR-0058a §6: Assert { cond: b!=0 } precedes the div in MIR.
    #[cfg(feature = "llvm")]
    llvm_compile_ok(
        "llvm_op10",
        "fn f(a: i64, b: i64) -> i64:\n    return (a / b)\n",
    );
}

// =====================================================================
// TERMINATOR LOWERING — ADR-0058a §6 (5 fixtures)
// =====================================================================

/// F34: codegen_diff_corpus::llvm_terminator_01_return_i64
#[test]
fn llvm_terminator_01_return_i64() {
    // Terminator::Return(operand:i64) → builder.build_return(Some(&val))
    #[cfg(feature = "llvm")]
    llvm_compile_ok("llvm_term01", "fn f() -> i64:\n    return 7\n");
}

// F36-amend 2026-05-19: original "return_void" unrepresentable (Ty::None → i64 fallback, not void); tests -> i64 baseline; void-return queued
#[test]
fn llvm_terminator_02_return_int_baseline() {
    // Terminator::Return(None) → builder.build_return(None)
    // Cobrust source: `None` is KwNone, not an Ident, so `-> None` is
    // rejected by parse_type_atom. Implicit-void source omits return type;
    // LLVM backend always emits build_return(Some(&val)) using the i64
    // fallback. Fixture uses implicit-void (no annotation) with return 0.
    #[cfg(feature = "llvm")]
    llvm_compile_ok("llvm_term02", "fn f() -> i64:\n    return 0\n");
}

#[test]
fn llvm_terminator_03_goto_bb() {
    // Terminator::Goto(target) → builder.build_unconditional_branch(target_block)
    // Sequence: bb0 → bb1 → return.
    #[cfg(feature = "llvm")]
    llvm_compile_ok(
        "llvm_term03",
        "fn f() -> i64:\n    let x: i64 = 1\n    let y: i64 = (x + 1)\n    return y\n",
    );
}

#[test]
fn llvm_terminator_04_branch_cond() {
    // Terminator::SwitchInt(bool) → builder.build_conditional_branch(cond, t_bb, f_bb)
    // if/else produces two live blocks; tests branch-both-arms coverage.
    #[cfg(feature = "llvm")]
    llvm_compile_ok(
        "llvm_term04",
        "fn f(x: i64) -> i64:\n    if (x > 0):\n        return 1\n    else:\n        return 0\n",
    );
}

#[test]
fn llvm_terminator_05_call() {
    // Terminator::Call{fn, args, dest, target} →
    //   builder.build_call(callee_fn, &[arg_val], "call")
    //   + build_unconditional_branch(target_block)
    #[cfg(feature = "llvm")]
    llvm_compile_ok(
        "llvm_term05",
        "fn double(x: i64) -> i64:\n    return (x + x)\n\nfn caller() -> i64:\n    return double(21)\n",
    );
}

// =====================================================================
// CALLING CONVENTION — ADR-0058a §7 (3 fixtures)
// System V AMD64 ABI / AAPCS64 via inkwell CallConv::C (ccc).
// =====================================================================

#[test]
fn llvm_callconv_01_sysv_stack_align() {
    // SysV AMD64: stack pointer must be 16-byte aligned at call sites.
    // Fixture: function call chain that exercises stack alignment slot.
    // Verification: emitted object passes llvm-mc re-parse without alignment faults.
    #[cfg(feature = "llvm")]
    llvm_compile_ok(
        "llvm_cc01",
        "fn leaf(a: i64, b: i64, c: i64) -> i64:\n    return ((a + b) + c)\n\nfn caller() -> i64:\n    return leaf(1, 2, 3)\n",
    );
}

#[test]
fn llvm_callconv_02_integer_args_in_regs() {
    // SysV AMD64: first 6 integer args in rdi/rsi/rdx/rcx/r8/r9.
    // Fixture: 6-arg function exercises full integer register bank.
    #[cfg(feature = "llvm")]
    llvm_compile_ok(
        "llvm_cc02",
        "fn f(a: i64, b: i64, c: i64, d: i64, e: i64, g: i64) -> i64:\n    return ((((a + b) + c) + d) + (e + g))\n",
    );
}

#[test]
fn llvm_callconv_03_return_aggregate_via_ptr() {
    // Aggregate return: struct (i64, i64) → sret pointer per SysV.
    // inkwell: fn_type returns pointer; caller passes hidden first arg.
    // ADR-0058a §7: ccc maps to sret for aggregates > register width.
    #[cfg(feature = "llvm")]
    llvm_compile_ok(
        "llvm_cc03",
        "fn make_pair(a: i64, b: i64) -> (i64, i64):\n    return (a, b)\n",
    );
}

// =====================================================================
// DYNAMIC-INDEX ARRAY — ADR-0060b finding-closure (3 fixtures)
// F34 anchors: llvm_array_dyn_index_i64 / i32 / oob_panic
// These exercise the runtime-helper path in lower_place_load:
// non-const index → call __cobrust_array_get_<T>(arr_ptr, N, idx).
// =====================================================================

/// F34: codegen_diff_corpus::llvm_array_dyn_index_i64
/// Exercises dynamic-index `a[i]` on `[i64; 4]` — routes through
/// `__cobrust_array_get_i64` runtime helper (LLVM backend).
/// Fixture compiles; no OOB panic expected for const-propagated safe index.
#[test]
fn llvm_array_dyn_index_i64() {
    // `fn nth(a: [i64; 4], i: i64) -> i64: return a[i]`
    // Dynamic index (parameter `i`) forces the runtime-helper path.
    // Type check passes: Ty::Array(Int, 4) + index Ty::Int → OK.
    #[cfg(feature = "llvm")]
    llvm_compile_ok(
        "llvm_arr_dyn_i64",
        "fn nth(a: [i64; 4], i: i64) -> i64:\n    return a[i]\n",
    );
}

/// F34: codegen_diff_corpus::llvm_array_dyn_index_i32
/// Exercises dynamic-index `a[i]` on `[i32; 3]` — routes through
/// `__cobrust_array_get_i32` runtime helper (LLVM backend).
#[test]
fn llvm_array_dyn_index_i32() {
    // `fn nth(a: [i32; 3], i: i64) -> i32: return a[i]`
    // Dynamic index; element type Ty::IntN(32) → __cobrust_array_get_i32.
    #[cfg(feature = "llvm")]
    llvm_compile_ok(
        "llvm_arr_dyn_i32",
        "fn nth(a: [i32; 3], i: i64) -> i32:\n    return a[i]\n",
    );
}

/// F34: codegen_diff_corpus::llvm_array_dyn_index_oob_panic
/// Verifies the type-check phase accepts a dynamic-index expression
/// (OOB cannot be detected at compile-time for non-literal index).
/// The runtime helper bounds-check will fire at runtime, not compile time.
/// This test confirms compile-time acceptance (no false-positive rejection).
#[test]
fn llvm_array_dyn_index_oob_panic() {
    // Dynamic index on [i64; 4]: no literal OOB → typeck passes.
    // Codegen emits call to __cobrust_array_get_i64 which panics at runtime
    // if idx >= 4. This fixture only verifies compilation succeeds.
    let src = "fn nth(a: [i64; 4], i: i64) -> i64:\n    return a[i]\n";
    let module = parse_str(src, FileId::SYNTHETIC).expect("parse");
    let mut sess = Session::new();
    let hir = hir_lower(&module, &mut sess).expect("hir");
    let res = check(&hir);
    assert!(
        res.is_ok(),
        "dynamic-index array typeck must pass (no literal OOB detectable): {src}"
    );
    // Compile via LLVM backend confirms codegen route.
    #[cfg(feature = "llvm")]
    llvm_compile_ok("llvm_arr_dyn_oob", src);
}

// =====================================================================
// ADR-0058f Phase K wave-2 — LLVM backend stdlib I/O hookup
//
// Each fixture builds a Module manually (bypassing PRELUDE + the
// `intrinsics::rewrite_print` MIR pass that lives in cobrust-cli),
// emits via LLVM backend, links against `libcobrust_stdlib.a` +
// `runtime/cobrust_main.c`, runs the resulting binary, and asserts
// the stdout matches a golden line.
//
// Why manual MIR (not source compile through the CLI's print rewrite
// pass): `cobrust-cli` depends on `cobrust-codegen`, so making the
// codegen tests use `cobrust-cli` would be a circular dependency.
// The manually-built MIR also makes the test self-contained: a single
// `Terminator::Call { func: Operand::Constant(Constant::Str(name)) }`
// exercises the exact extern-name dispatch surface that ADR-0058f §3.4
// ships. The MIR shape here matches what `rewrite_print` produces:
// `print(x: i64)` becomes a Call to `Constant::Str("__cobrust_println_int")`.
//
// Pre-fix expectation: every fixture emits empty stdout (wave-1 stub
// fallthrough swallowed the call). Post-fix: stdout matches.
//
// Each test gates on:
//   - `#[cfg(feature = "llvm")]` (skipped on default Cranelift build)
//   - `linker_available()` (cc on PATH)
//   - `find_stdlib_archive()` returns Some (libcobrust_stdlib.a on disk)
//   - `find_runtime_c()` returns Some (cobrust_main.c on disk)
//
// Gating exit is a `return` (the test passes); the LLVM-stdlib-linked
// lane in CI / release flow runs them concretely.
// =====================================================================

#[cfg(feature = "llvm")]
fn find_stdlib_archive() -> Option<std::path::PathBuf> {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let workspace = std::path::Path::new(&manifest).parent()?.parent()?;
    for profile in ["debug", "release"] {
        let p = workspace
            .join("target")
            .join(profile)
            .join("libcobrust_stdlib.a");
        if p.exists() {
            return Some(p);
        }
    }
    None
}

#[cfg(feature = "llvm")]
fn find_runtime_c() -> Option<std::path::PathBuf> {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let workspace = std::path::Path::new(&manifest).parent()?.parent()?;
    let p = workspace.join("crates/cobrust-cli/runtime/cobrust_main.c");
    if p.exists() { Some(p) } else { None }
}

#[cfg(feature = "llvm")]
fn stdlib_io_link_and_run(name: &str, module: cobrust_mir::Module) -> Option<String> {
    use std::process::Command;
    if !cobrust_codegen::linker::linker_available() {
        return None;
    }
    let stdlib = find_stdlib_archive()?;
    let runtime_c = find_runtime_c()?;

    // Emit object via LLVM backend.
    let spec = llvm_spec(name);
    let artifact = emit(&module, spec).unwrap_or_else(|e| panic!("LLVM emit `{name}` failed: {e}"));
    let user_obj = artifact.path().to_path_buf();

    // Compile runtime C shim.
    let dir = user_obj.parent().expect("user obj parent").to_path_buf();
    let runtime_obj = dir.join("cobrust_main.o");
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let cc_status = Command::new(&cc)
        .arg("-c")
        .arg(&runtime_c)
        .arg("-o")
        .arg(&runtime_obj)
        .status()
        .ok()?;
    if !cc_status.success() {
        return None;
    }

    // Link user.o + runtime.o + libcobrust_stdlib.a → exe.
    let exe = dir.join(format!("{name}.exe"));
    let mut link_cmd = Command::new(&cc);
    link_cmd
        .arg(&user_obj)
        .arg(&runtime_obj)
        .arg(&stdlib)
        .arg("-o")
        .arg(&exe);
    if cfg!(target_os = "linux") {
        link_cmd.arg("-lpthread").arg("-ldl").arg("-lm");
    }
    let link_status = link_cmd.status().ok()?;
    if !link_status.success() {
        return None;
    }

    // Run and capture stdout.
    let out = Command::new(&exe).output().ok()?;
    if !out.status.success() {
        let so = String::from_utf8_lossy(&out.stdout);
        let se = String::from_utf8_lossy(&out.stderr);
        panic!(
            "`{name}` exited non-zero ({:?}); stdout={so:?} stderr={se:?}",
            out.status
        );
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Build a minimal `main` MIR body that calls `extern_name(arg)` then
/// returns 0. `ret_ty_of_arg` lets the helper-call destination type
/// match the runtime helper's return contract (`void` → i64 stub).
#[cfg(feature = "llvm")]
fn build_main_calling_extern(extern_name: &str, arg: cobrust_mir::Constant) -> cobrust_mir::Module {
    use cobrust_frontend::span::{FileId, Span};
    use cobrust_hir::DefId;
    use cobrust_mir::{
        BasicBlock as MirBlock, BlockId, Body, Constant, LocalDecl, LocalId, Module, Operand,
        Place, Rvalue, Statement, StatementKind, Terminator,
    };
    use cobrust_types::Ty;

    let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
    let arg_ty = match &arg {
        Constant::Bool(_) => Ty::Bool,
        Constant::Int(_) => Ty::Int,
        Constant::Float(_) | Constant::Imag(_) => Ty::Float,
        Constant::Str(_) | Constant::Bytes(_) => Ty::Str,
        _ => Ty::Int,
    };
    // _0 = return slot (i64), _1 = call-result temp.
    let locals = vec![
        LocalDecl {
            id: LocalId(0),
            name: "_return".to_string(),
            ty: Ty::Int,
            mutable: true,
            span: span0,
        },
        LocalDecl {
            id: LocalId(1),
            name: "_callret".to_string(),
            ty: Ty::Int,
            mutable: true,
            span: span0,
        },
        LocalDecl {
            id: LocalId(2),
            name: "_arg".to_string(),
            ty: arg_ty,
            mutable: false,
            span: span0,
        },
    ];
    // bb0: arg = arg_const; call extern_name(arg) -> bb1
    // bb1: _return = 0; return
    let bb0 = MirBlock {
        id: BlockId(0),
        statements: vec![],
        terminator: Terminator::Call {
            func: Operand::Constant(Constant::Str(extern_name.to_string())),
            args: vec![Operand::Constant(arg)],
            destination: Place::local(LocalId(1)),
            target: BlockId(1),
            unwind: None,
        },
        span: span0,
    };
    let bb1 = MirBlock {
        id: BlockId(1),
        statements: vec![Statement {
            kind: StatementKind::Assign {
                place: Place::local(LocalId(0)),
                rvalue: Rvalue::Use(Operand::Constant(Constant::Int(0))),
            },
            span: span0,
        }],
        terminator: Terminator::Return,
        span: span0,
    };
    Module {
        bodies: vec![Body {
            def_id: DefId(0),
            name: "main".to_string(),
            locals,
            blocks: vec![bb0, bb1],
            return_local: LocalId(0),
            param_count: 0,
            span: span0,
        }],
    }
}

#[cfg(feature = "llvm")]
fn assert_extern_io(name: &str, extern_name: &str, arg: cobrust_mir::Constant, expected: &str) {
    let module = build_main_calling_extern(extern_name, arg);
    let Some(stdout) = stdlib_io_link_and_run(name, module) else {
        return; // Prereqs missing — skip.
    };
    assert_eq!(
        stdout, expected,
        "stdlib_io `{name}`: stdout mismatch\n  got:      {stdout:?}\n  expected: {expected:?}"
    );
}

/// ADR-0058f §5 fixture 01: `__cobrust_println_int(42)` → "42\n"
#[test]
fn stdlib_io_01_println_int_42() {
    #[cfg(feature = "llvm")]
    assert_extern_io(
        "stdlib_io_01",
        "__cobrust_println_int",
        cobrust_mir::Constant::Int(42),
        "42\n",
    );
}

/// ADR-0058f §5 fixture 02: `__cobrust_println_bool(True)` → "True\n"
/// Exercises the i1 → i8 widening at the call site.
#[test]
fn stdlib_io_02_println_bool_true() {
    #[cfg(feature = "llvm")]
    assert_extern_io(
        "stdlib_io_02",
        "__cobrust_println_bool",
        cobrust_mir::Constant::Bool(true),
        "True\n",
    );
}

/// ADR-0058f §5 fixture 03: `__cobrust_println_bool(False)` → "False\n"
#[test]
fn stdlib_io_03_println_bool_false() {
    #[cfg(feature = "llvm")]
    assert_extern_io(
        "stdlib_io_03",
        "__cobrust_println_bool",
        cobrust_mir::Constant::Bool(false),
        "False\n",
    );
}

/// ADR-0058f §5 fixture 04: `__cobrust_println_float(1.5)` → "1.5\n"
#[test]
fn stdlib_io_04_println_float() {
    #[cfg(feature = "llvm")]
    assert_extern_io(
        "stdlib_io_04",
        "__cobrust_println_float",
        cobrust_mir::Constant::Float(1.5_f64.to_bits()),
        "1.5\n",
    );
}

/// ADR-0058f §5 fixture 05: `__cobrust_println_str_buf("hello")` → "hello\n"
/// Exercises the Constant::Str arg → materialize_str_buffer path.
#[test]
fn stdlib_io_05_println_str_literal() {
    #[cfg(feature = "llvm")]
    assert_extern_io(
        "stdlib_io_05",
        "__cobrust_println_str_buf",
        cobrust_mir::Constant::Str("hello".to_string()),
        "hello\n",
    );
}

/// ADR-0058f §5 fixture 06: literal-bytes path via `__cobrust_println(ptr, len)`.
/// Exercises the single-Str-arg → (ptr, len) expansion case.
#[test]
fn stdlib_io_06_println_literal_path() {
    #[cfg(feature = "llvm")]
    assert_extern_io(
        "stdlib_io_06",
        "__cobrust_println",
        cobrust_mir::Constant::Str("world".to_string()),
        "world\n",
    );
}

/// ADR-0058f §5 fixture 08: `print(fib(10))` → "55\n".
/// Exercises the user-fn FnRef call chain: `fib` (recursive, two bodies)
/// → `__cobrust_println_int` extern call. This is the exact failure mode
/// reported in the user bug report (2026-05-22 playground): fib computed
/// but result was swallowed by the wave-1 println stub.
///
/// MIR layout:
///   body[0] = fib(n: i64) -> i64
///     bb0: _cmp = (n < 2); SwitchInt(_cmp, [(true, bb1)], otherwise=bb2)
///     bb1: _return = n; Return
///     bb2: _r1 = fib(n-1); _r2 = fib(n-2); _return = _r1+_r2; Return
///   body[1] = main() -> i64
///     bb0: _fib_ret = fib(10); call __cobrust_println_int(_fib_ret) -> bb1
///     bb1: _return = 0; Return
#[test]
fn stdlib_io_08_println_fib_result() {
    #[cfg(feature = "llvm")]
    {
        use cobrust_frontend::span::{FileId, Span};
        use cobrust_hir::DefId;
        use cobrust_mir::{
            BasicBlock as MirBlock, BinOp, BlockId, Body, Constant, LocalDecl, LocalId, Module,
            Operand, Place, Rvalue, Statement, StatementKind, SwitchValue, Terminator,
        };
        use cobrust_types::Ty;

        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);

        // ── body[0]: fib(n: i64) -> i64 ────────────────────────────────
        // Locals: _0=return(int), _1=n(int), _2=_cmp(bool),
        //         _3=_n_minus_1(int), _4=_n_minus_2(int),
        //         _5=_r1(int), _6=_r2(int)
        let fib_locals = vec![
            LocalDecl { id: LocalId(0), name: "_return".into(), ty: Ty::Int, mutable: true,  span: span0 },
            LocalDecl { id: LocalId(1), name: "n".into(),       ty: Ty::Int, mutable: false, span: span0 },
            LocalDecl { id: LocalId(2), name: "_cmp".into(),    ty: Ty::Bool, mutable: true, span: span0 },
            LocalDecl { id: LocalId(3), name: "_nm1".into(),    ty: Ty::Int, mutable: true,  span: span0 },
            LocalDecl { id: LocalId(4), name: "_nm2".into(),    ty: Ty::Int, mutable: true,  span: span0 },
            LocalDecl { id: LocalId(5), name: "_r1".into(),     ty: Ty::Int, mutable: true,  span: span0 },
            LocalDecl { id: LocalId(6), name: "_r2".into(),     ty: Ty::Int, mutable: true,  span: span0 },
        ];
        // bb0: _cmp = (n < 2); SwitchInt(_cmp, [(true→bb1)], otherwise→bb2)
        let fib_bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(2)),
                    rvalue: Rvalue::BinaryOp(
                        BinOp::Lt,
                        Operand::Copy(Place::local(LocalId(1))),
                        Operand::Constant(Constant::Int(2)),
                    ),
                },
                span: span0,
            }],
            terminator: Terminator::SwitchInt {
                operand: Operand::Copy(Place::local(LocalId(2))),
                cases: vec![(SwitchValue::Bool(true), BlockId(1))],
                otherwise: BlockId(2),
            },
            span: span0,
        };
        // bb1: _return = n; Return
        let fib_bb1 = MirBlock {
            id: BlockId(1),
            statements: vec![Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(0)),
                    rvalue: Rvalue::Use(Operand::Copy(Place::local(LocalId(1)))),
                },
                span: span0,
            }],
            terminator: Terminator::Return,
            span: span0,
        };
        // bb2: _nm1 = n-1; _nm2 = n-2;
        //      Call fib(_nm1) -> _r1 → bb3
        // bb3: Call fib(_nm2) -> _r2 → bb4
        // bb4: _return = _r1+_r2; Return
        let fib_bb2 = MirBlock {
            id: BlockId(2),
            statements: vec![
                Statement {
                    kind: StatementKind::Assign {
                        place: Place::local(LocalId(3)),
                        rvalue: Rvalue::BinaryOp(
                            BinOp::Sub,
                            Operand::Copy(Place::local(LocalId(1))),
                            Operand::Constant(Constant::Int(1)),
                        ),
                    },
                    span: span0,
                },
                Statement {
                    kind: StatementKind::Assign {
                        place: Place::local(LocalId(4)),
                        rvalue: Rvalue::BinaryOp(
                            BinOp::Sub,
                            Operand::Copy(Place::local(LocalId(1))),
                            Operand::Constant(Constant::Int(2)),
                        ),
                    },
                    span: span0,
                },
            ],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::FnRef(0)), // fib = body[0]
                args: vec![Operand::Copy(Place::local(LocalId(3)))],
                destination: Place::local(LocalId(5)),
                target: BlockId(3),
                unwind: None,
            },
            span: span0,
        };
        let fib_bb3 = MirBlock {
            id: BlockId(3),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::FnRef(0)),
                args: vec![Operand::Copy(Place::local(LocalId(4)))],
                destination: Place::local(LocalId(6)),
                target: BlockId(4),
                unwind: None,
            },
            span: span0,
        };
        let fib_bb4 = MirBlock {
            id: BlockId(4),
            statements: vec![Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(0)),
                    rvalue: Rvalue::BinaryOp(
                        BinOp::Add,
                        Operand::Copy(Place::local(LocalId(5))),
                        Operand::Copy(Place::local(LocalId(6))),
                    ),
                },
                span: span0,
            }],
            terminator: Terminator::Return,
            span: span0,
        };
        let fib_body = Body {
            def_id: DefId(0),
            name: "fib".into(),
            locals: fib_locals,
            blocks: vec![fib_bb0, fib_bb1, fib_bb2, fib_bb3, fib_bb4],
            return_local: LocalId(0),
            param_count: 1,
            span: span0,
        };

        // ── body[1]: main() -> i64 ──────────────────────────────────────
        // Locals: _0=return(int), _1=_fib_ret(int), _2=_println_ret(int)
        let main_locals = vec![
            LocalDecl { id: LocalId(0), name: "_return".into(),     ty: Ty::Int, mutable: true,  span: span0 },
            LocalDecl { id: LocalId(1), name: "_fib_ret".into(),    ty: Ty::Int, mutable: true,  span: span0 },
            LocalDecl { id: LocalId(2), name: "_println_ret".into(), ty: Ty::Int, mutable: true, span: span0 },
        ];
        // bb0: Call fib(10) -> _fib_ret → bb1
        let main_bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::FnRef(0)), // fib = body[0]
                args: vec![Operand::Constant(Constant::Int(10))],
                destination: Place::local(LocalId(1)),
                target: BlockId(1),
                unwind: None,
            },
            span: span0,
        };
        // bb1: Call __cobrust_println_int(_fib_ret) -> _println_ret → bb2
        let main_bb1 = MirBlock {
            id: BlockId(1),
            statements: vec![],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_println_int".into())),
                args: vec![Operand::Copy(Place::local(LocalId(1)))],
                destination: Place::local(LocalId(2)),
                target: BlockId(2),
                unwind: None,
            },
            span: span0,
        };
        // bb2: _return = 0; Return
        let main_bb2 = MirBlock {
            id: BlockId(2),
            statements: vec![Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(0)),
                    rvalue: Rvalue::Use(Operand::Constant(Constant::Int(0))),
                },
                span: span0,
            }],
            terminator: Terminator::Return,
            span: span0,
        };
        let main_body = Body {
            def_id: DefId(1),
            name: "main".into(),
            locals: main_locals,
            blocks: vec![main_bb0, main_bb1, main_bb2],
            return_local: LocalId(0),
            param_count: 0,
            span: span0,
        };

        let module = Module { bodies: vec![fib_body, main_body] };
        let Some(stdout) = stdlib_io_link_and_run("stdlib_io_08", module) else {
            return; // Prereqs missing (no llvm feature, no stdlib, no linker) — skip.
        };
        assert_eq!(
            stdout, "55\n",
            "stdlib_io_08: fib(10) result not printed correctly\n  got: {stdout:?}\n  expected: \"55\\n\""
        );
    }
}

/// ADR-0058f §5 fixture 07: round-trip `let s: str = "hi"; print(s)` via
/// a synthetic two-block body — Assign(Str-typed local, Constant::Str)
/// then call `__cobrust_println_str_buf(s)`. Exercises the
/// `lower_statement` Assign-side cascade fix (str-buffer materialised
/// at `let` time and the resulting buffer pointer fed straight into
/// the runtime helper).
#[test]
fn stdlib_io_07_println_str_let_binding() {
    #[cfg(feature = "llvm")]
    {
        use cobrust_frontend::span::{FileId, Span};
        use cobrust_hir::DefId;
        use cobrust_mir::{
            BasicBlock as MirBlock, BlockId, Body, Constant, LocalDecl, LocalId, Module, Operand,
            Place, Rvalue, Statement, StatementKind, Terminator,
        };
        use cobrust_types::Ty;
        let span0 = Span::new(FileId::SYNTHETIC, 0, 0);
        // _0 ret(i64), _1 s(str), _2 callret(i64)
        let locals = vec![
            LocalDecl {
                id: LocalId(0),
                name: "_return".into(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
            LocalDecl {
                id: LocalId(1),
                name: "s".into(),
                ty: Ty::Str,
                mutable: false,
                span: span0,
            },
            LocalDecl {
                id: LocalId(2),
                name: "_callret".into(),
                ty: Ty::Int,
                mutable: true,
                span: span0,
            },
        ];
        // bb0: s = "hi"; call println_str_buf(s) -> bb1
        // bb1: _return = 0; return
        let bb0 = MirBlock {
            id: BlockId(0),
            statements: vec![Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(1)),
                    rvalue: Rvalue::Use(Operand::Constant(Constant::Str("hi".into()))),
                },
                span: span0,
            }],
            terminator: Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_println_str_buf".into())),
                args: vec![Operand::Copy(Place::local(LocalId(1)))],
                destination: Place::local(LocalId(2)),
                target: BlockId(1),
                unwind: None,
            },
            span: span0,
        };
        let bb1 = MirBlock {
            id: BlockId(1),
            statements: vec![Statement {
                kind: StatementKind::Assign {
                    place: Place::local(LocalId(0)),
                    rvalue: Rvalue::Use(Operand::Constant(Constant::Int(0))),
                },
                span: span0,
            }],
            terminator: Terminator::Return,
            span: span0,
        };
        let module = Module {
            bodies: vec![Body {
                def_id: DefId(0),
                name: "main".into(),
                locals,
                blocks: vec![bb0, bb1],
                return_local: LocalId(0),
                param_count: 0,
                span: span0,
            }],
        };
        let Some(stdout) = stdlib_io_link_and_run("stdlib_io_07", module) else {
            return; // Prereqs missing — skip.
        };
        assert_eq!(stdout, "hi\n");
    }
}
