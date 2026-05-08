//! M9 well-formed codegen tests — every program here lowers from
//! Cobrust source → AST → HIR → typed-HIR → MIR → object file
//! cleanly via the Cranelift backend.
//!
//! ADR-0023 §"Differential gate (acceptance contract)" pins the
//! M9 acceptance bar at ≥ 50 well-formed programs.

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
use cobrust_mir::{Module, lower as mir_lower};
use cobrust_types::check;
use target_lexicon::Triple;

/// Compile `src` end-to-end through the pipeline up to MIR.
fn lower_to_mir(src: &str) -> Module {
    let module = parse_str(src, FileId::SYNTHETIC).expect("parse");
    let mut sess = Session::new();
    let hir = hir_lower(&module, &mut sess).expect("hir lower");
    let typed = check(&hir).expect("type check");
    mir_lower(&typed).expect("mir lower")
}

/// Spec for a host-targeted relocatable object file.
fn host_object_spec(name: &str) -> TargetSpec {
    let dir = std::env::temp_dir().join(format!("cobrust-m9-{name}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    TargetSpec {
        triple: Triple::host(),
        opt_level: OptLevel::None,
        backend: Backend::Cranelift,
        artifact: ArtifactKind::Object,
        output_dir: dir,
        module_name: name.to_string(),
    }
}

/// Compile `src` to an object file via Cranelift; assert the artifact
/// path exists + is non-empty.
fn compile_ok(name: &str, src: &str) {
    let mir = lower_to_mir(src);
    let spec = host_object_spec(name);
    let artifact = emit(&mir, spec).unwrap_or_else(|e| panic!("emit `{name}`: {e}"));
    let path = artifact.path();
    let meta = std::fs::metadata(path)
        .unwrap_or_else(|e| panic!("metadata for `{}`: {}", path.display(), e));
    assert!(meta.len() > 0, "object file empty for `{name}`");
    assert!(matches!(artifact, Artifact::Object(_)));
}

// =====================================================================
// 50+ well-formed programs covering the M9 in-scope MIR forms.
// =====================================================================

#[test]
fn p001_const_int() {
    compile_ok("p001", "fn f() -> i64:\n    return 0\n");
}
#[test]
fn p002_const_int_pos() {
    compile_ok("p002", "fn f() -> i64:\n    return 42\n");
}
#[test]
fn p003_const_int_neg() {
    compile_ok("p003", "fn f() -> i64:\n    return -7\n");
}
#[test]
fn p004_const_int_max() {
    compile_ok("p004", "fn f() -> i64:\n    return 9223372036854775807\n");
}
#[test]
fn p005_const_int_min() {
    compile_ok("p005", "fn f() -> i64:\n    return -9223372036854775808\n");
}

#[test]
fn p006_const_float() {
    compile_ok("p006", "fn f() -> f64:\n    return 0.0\n");
}
#[test]
fn p007_const_float_pos() {
    compile_ok("p007", "fn f() -> f64:\n    return 3.14\n");
}
#[test]
fn p008_const_float_neg() {
    compile_ok("p008", "fn f() -> f64:\n    return -2.71\n");
}

#[test]
fn p009_const_bool_true() {
    compile_ok("p009", "fn f() -> bool:\n    return True\n");
}
#[test]
fn p010_const_bool_false() {
    compile_ok("p010", "fn f() -> bool:\n    return False\n");
}

#[test]
fn p011_param_passthrough_int() {
    compile_ok("p011", "fn f(x: i64) -> i64:\n    return x\n");
}
#[test]
fn p012_param_passthrough_float() {
    compile_ok("p012", "fn f(x: f64) -> f64:\n    return x\n");
}
#[test]
fn p013_param_passthrough_bool() {
    compile_ok("p013", "fn f(x: bool) -> bool:\n    return x\n");
}

#[test]
fn p014_add() {
    compile_ok("p014", "fn f(a: i64, b: i64) -> i64:\n    return a + b\n");
}
#[test]
fn p015_sub() {
    compile_ok("p015", "fn f(a: i64, b: i64) -> i64:\n    return a - b\n");
}
#[test]
fn p016_mul() {
    compile_ok("p016", "fn f(a: i64, b: i64) -> i64:\n    return a * b\n");
}

#[test]
fn p017_fadd() {
    compile_ok("p017", "fn f(a: f64, b: f64) -> f64:\n    return a + b\n");
}
#[test]
fn p018_fsub() {
    compile_ok("p018", "fn f(a: f64, b: f64) -> f64:\n    return a - b\n");
}
#[test]
fn p019_fmul() {
    compile_ok("p019", "fn f(a: f64, b: f64) -> f64:\n    return a * b\n");
}

#[test]
fn p020_div_with_assert() {
    compile_ok("p020", "fn f(a: i64, b: i64) -> i64:\n    return a / b\n");
}
#[test]
fn p021_mod_with_assert() {
    compile_ok("p021", "fn f(a: i64, b: i64) -> i64:\n    return a % b\n");
}

#[test]
fn p022_eq() {
    compile_ok("p022", "fn f(a: i64, b: i64) -> bool:\n    return a == b\n");
}
#[test]
fn p023_neq() {
    compile_ok("p023", "fn f(a: i64, b: i64) -> bool:\n    return a != b\n");
}
#[test]
fn p024_lt() {
    compile_ok("p024", "fn f(a: i64, b: i64) -> bool:\n    return a < b\n");
}
#[test]
fn p025_lte() {
    compile_ok("p025", "fn f(a: i64, b: i64) -> bool:\n    return a <= b\n");
}
#[test]
fn p026_gt() {
    compile_ok("p026", "fn f(a: i64, b: i64) -> bool:\n    return a > b\n");
}
#[test]
fn p027_gte() {
    compile_ok("p027", "fn f(a: i64, b: i64) -> bool:\n    return a >= b\n");
}

#[test]
fn p028_neg() {
    compile_ok("p028", "fn f(x: i64) -> i64:\n    return -x\n");
}
#[test]
fn p029_plus() {
    compile_ok("p029", "fn f(x: i64) -> i64:\n    return +x\n");
}
#[test]
fn p030_bitnot() {
    compile_ok("p030", "fn f(x: i64) -> i64:\n    return ~x\n");
}
#[test]
fn p031_not() {
    compile_ok("p031", "fn f(x: bool) -> bool:\n    return not x\n");
}

#[test]
fn p032_let_then_use() {
    compile_ok("p032", "fn f() -> i64:\n    let x: i64 = 1\n    return x\n");
}
#[test]
fn p033_let_assign_use() {
    compile_ok(
        "p033",
        "fn f() -> i64:\n    let x: i64 = 1\n    x = 2\n    return x\n",
    );
}
#[test]
fn p034_let_aug_assign() {
    compile_ok(
        "p034",
        "fn f() -> i64:\n    let x: i64 = 1\n    x += 5\n    return x\n",
    );
}
#[test]
fn p035_let_two_locals() {
    compile_ok(
        "p035",
        "fn f() -> i64:\n    let x: i64 = 1\n    let y: i64 = 2\n    return (x + y)\n",
    );
}

#[test]
fn p036_if_then() {
    compile_ok(
        "p036",
        "fn f(x: i64) -> i64:\n    if (x > 0):\n        return x\n    return 0\n",
    );
}
#[test]
fn p037_if_else() {
    compile_ok(
        "p037",
        "fn f(x: i64) -> i64:\n    if (x > 0):\n        return 1\n    else:\n        return 0\n",
    );
}
#[test]
fn p038_if_elif() {
    compile_ok(
        "p038",
        "fn f(x: i64) -> i64:\n    if (x > 0):\n        return 1\n    elif (x < 0):\n        return -1\n    return 0\n",
    );
}

#[test]
fn p039_while_simple() {
    compile_ok(
        "p039",
        "fn f(n: i64) -> i64:\n    let acc: i64 = 0\n    let i: i64 = 0\n    while (i < n):\n        acc += i\n        i += 1\n    return acc\n",
    );
}
#[test]
fn p040_while_break() {
    compile_ok(
        "p040",
        "fn f(n: i64) -> i64:\n    let i: i64 = 0\n    while True:\n        if (i >= n):\n            break\n        i += 1\n    return i\n",
    );
}
#[test]
fn p041_while_continue() {
    compile_ok(
        "p041",
        "fn f(n: i64) -> i64:\n    let i: i64 = 0\n    let acc: i64 = 0\n    while (i < n):\n        i += 1\n        if ((i % 2) == 0):\n            continue\n        acc += i\n    return acc\n",
    );
}

#[test]
fn p042_two_funcs() {
    compile_ok(
        "p042",
        "fn double(x: i64) -> i64:\n    return (x + x)\n\nfn quad(x: i64) -> i64:\n    return double(double(x))\n",
    );
}
#[test]
fn p043_recursion() {
    compile_ok(
        "p043",
        "fn fib(n: i64) -> i64:\n    if (n < 2):\n        return n\n    return (fib((n - 1)) + fib((n - 2)))\n",
    );
}

#[test]
fn p044_bitand() {
    compile_ok("p044", "fn f(a: i64, b: i64) -> i64:\n    return (a & b)\n");
}
#[test]
fn p045_bitor() {
    compile_ok("p045", "fn f(a: i64, b: i64) -> i64:\n    return (a | b)\n");
}
#[test]
fn p046_bitxor() {
    compile_ok("p046", "fn f(a: i64, b: i64) -> i64:\n    return (a ^ b)\n");
}
#[test]
fn p047_shl() {
    compile_ok(
        "p047",
        "fn f(a: i64, b: i64) -> i64:\n    return (a << b)\n",
    );
}
#[test]
fn p048_shr() {
    compile_ok(
        "p048",
        "fn f(a: i64, b: i64) -> i64:\n    return (a >> b)\n",
    );
}

#[test]
fn p049_logical_and() {
    compile_ok(
        "p049",
        "fn f(a: bool, b: bool) -> bool:\n    return (a and b)\n",
    );
}
#[test]
fn p050_logical_or() {
    compile_ok(
        "p050",
        "fn f(a: bool, b: bool) -> bool:\n    return (a or b)\n",
    );
}

#[test]
fn p051_pass_stmt() {
    compile_ok("p051", "fn f() -> i64:\n    pass\n    return 0\n");
}

#[test]
fn p052_chain_arith() {
    compile_ok(
        "p052",
        "fn f(x: i64) -> i64:\n    let a: i64 = (x + 1)\n    let b: i64 = (a * 2)\n    let c: i64 = (b - 3)\n    return c\n",
    );
}

#[test]
fn p053_nested_if() {
    compile_ok(
        "p053",
        "fn f(x: i64, y: i64) -> i64:\n    if (x > 0):\n        if (y > 0):\n            return 1\n        return 2\n    return 0\n",
    );
}

#[test]
fn p054_factorial_loop() {
    compile_ok(
        "p054",
        "fn f(n: i64) -> i64:\n    let acc: i64 = 1\n    let i: i64 = 1\n    while (i <= n):\n        acc *= i\n        i += 1\n    return acc\n",
    );
}

#[test]
fn p055_pow_of_two_count() {
    compile_ok(
        "p055",
        "fn f(n: i64) -> i64:\n    let count: i64 = 0\n    let v: i64 = n\n    while (v > 0):\n        if ((v & 1) == 1):\n            count += 1\n        v >>= 1\n    return count\n",
    );
}

// --- Backend selection cases ---------------------------------------------

#[test]
fn p056_default_backend_is_cranelift() {
    let mir = lower_to_mir("fn f() -> i64:\n    return 0\n");
    let mut spec = host_object_spec("p056");
    spec.backend = Backend::default_for_dev();
    assert_eq!(spec.backend, Backend::Cranelift);
    let _ = emit(&mir, spec).expect("emit");
}

#[test]
fn p057_release_backend_falls_back_when_no_llvm() {
    let mir = lower_to_mir("fn f() -> i64:\n    return 0\n");
    let mut spec = host_object_spec("p057");
    spec.backend = Backend::default_for_release();
    // Without `--features llvm` the release default is still Cranelift.
    if !cfg!(feature = "llvm") {
        assert_eq!(spec.backend, Backend::Cranelift);
        let _ = emit(&mir, spec).expect("emit");
    }
}

#[test]
fn p058_optlevel_speed_cranelift() {
    let mir = lower_to_mir("fn f() -> i64:\n    return 0\n");
    let mut spec = host_object_spec("p058");
    spec.opt_level = OptLevel::Speed;
    let _ = emit(&mir, spec).expect("emit");
}

#[test]
fn p059_optlevel_speed_and_size_cranelift() {
    let mir = lower_to_mir("fn f() -> i64:\n    return 0\n");
    let mut spec = host_object_spec("p059");
    spec.opt_level = OptLevel::SpeedAndSize;
    let _ = emit(&mir, spec).expect("emit");
}

#[test]
fn p060_param_threads_through_locals() {
    compile_ok(
        "p060",
        "fn f(a: i64) -> i64:\n    let b: i64 = a\n    let c: i64 = b\n    return c\n",
    );
}
