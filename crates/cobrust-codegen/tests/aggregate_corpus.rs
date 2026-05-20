//! M12.x Aggregate Rvalue corpus (per ADR-0027 §1).
//!
//! Each program lowers a Cobrust source containing tuple / list /
//! dict / set literals through the full pipeline and asserts the
//! Cranelift backend emits a non-empty object file. Together with
//! `runtime` round-trip tests in cobrust-stdlib, this gates the
//! Aggregate stub-removal commit.

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

use cobrust_codegen::{Artifact, ArtifactKind, Backend, OptLevel, TargetSpec, emit};
use cobrust_frontend::{parse_str, span::FileId};
use cobrust_hir::{Session, lower as hir_lower};
use cobrust_mir::{Module, lower as mir_lower};
use cobrust_types::check;
use target_lexicon::Triple;

fn lower_to_mir(src: &str) -> Module {
    let module = parse_str(src, FileId::SYNTHETIC).expect("parse");
    let mut sess = Session::new();
    let hir = hir_lower(&module, &mut sess).expect("hir lower");
    let typed = check(&hir).expect("type check");
    mir_lower(&typed).expect("mir lower")
}

fn host_object_spec(name: &str) -> TargetSpec {
    let dir = std::env::temp_dir().join(format!("cobrust-m12x-agg-{name}-{}", std::process::id()));
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

fn compile_ok(name: &str, src: &str) {
    let mir = lower_to_mir(src);
    let spec = host_object_spec(name);
    let artifact = emit(&mir, spec).unwrap_or_else(|e| panic!("emit `{name}`: {e}"));
    let path = artifact.path();
    let meta = std::fs::metadata(path).unwrap();
    assert!(meta.len() > 0, "object file empty for `{name}`");
    assert!(matches!(artifact, Artifact::Object(_)));
}

// =====================================================================
// Tuple aggregates — aggregates are valid as the function return value
// because the return type drives inference for empty / homogeneous
// literals. We exercise them by binding non-empty literals which the
// type checker can infer per-element.
// =====================================================================

#[test]
fn agg_tuple_pair_int() {
    compile_ok(
        "agg_tuple_pair_int",
        "fn f() -> i64:\n    let t: (i64, i64) = (1, 2)\n    return 0\n",
    );
}

#[test]
fn agg_tuple_triple_int() {
    compile_ok(
        "agg_tuple_triple_int",
        "fn f() -> i64:\n    let t: (i64, i64, i64) = (1, 2, 3)\n    return 0\n",
    );
}

#[test]
fn agg_tuple_with_neg() {
    compile_ok(
        "agg_tuple_with_neg",
        "fn f() -> i64:\n    let t: (i64, i64) = (-1, -2)\n    return 0\n",
    );
}

#[test]
fn agg_tuple_var_input() {
    compile_ok(
        "agg_tuple_var_input",
        "fn f(x: i64, y: i64) -> i64:\n    let t: (i64, i64) = (x, y)\n    return 0\n",
    );
}

#[test]
fn agg_tuple_quad() {
    compile_ok(
        "agg_tuple_quad",
        "fn f() -> i64:\n    let t: (i64, i64, i64, i64) = (1, 2, 3, 4)\n    return 0\n",
    );
}

#[test]
fn agg_tuple_after_branch() {
    compile_ok(
        "agg_tuple_after_branch",
        "fn f(b: bool) -> i64:\n    if b:\n        let t: (i64, i64) = (1, 2)\n    return 0\n",
    );
}

// =====================================================================
// List aggregates
// =====================================================================

#[test]
fn agg_list_single() {
    compile_ok(
        "agg_list_single",
        "fn f() -> i64:\n    let l: list[i64] = [42]\n    return 0\n",
    );
}

#[test]
fn agg_list_two() {
    compile_ok(
        "agg_list_two",
        "fn f() -> i64:\n    let l: list[i64] = [1, 2]\n    return 0\n",
    );
}

#[test]
fn agg_list_three() {
    compile_ok(
        "agg_list_three",
        "fn f() -> i64:\n    let l: list[i64] = [1, 2, 3]\n    return 0\n",
    );
}

#[test]
fn agg_list_long() {
    compile_ok(
        "agg_list_long",
        "fn f() -> i64:\n    let l: list[i64] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]\n    return 0\n",
    );
}

#[test]
fn agg_list_with_neg() {
    compile_ok(
        "agg_list_with_neg",
        "fn f() -> i64:\n    let l: list[i64] = [-1, 2, -3]\n    return 0\n",
    );
}

#[test]
fn agg_list_param_input() {
    compile_ok(
        "agg_list_param_input",
        "fn f(a: i64, b: i64) -> i64:\n    let l: list[i64] = [a, b]\n    return 0\n",
    );
}

#[test]
fn agg_list_in_loop() {
    compile_ok(
        "agg_list_in_loop",
        "fn f() -> i64:\n    let n = 0\n    while n < 3:\n        let l: list[i64] = [1, 2]\n        n = n + 1\n    return 0\n",
    );
}

// =====================================================================
// Dict aggregates
// =====================================================================

#[test]
fn agg_dict_single() {
    compile_ok(
        "agg_dict_single",
        "fn f() -> i64:\n    let d: dict[i64, i64] = {1: 10}\n    return 0\n",
    );
}

#[test]
fn agg_dict_two() {
    compile_ok(
        "agg_dict_two",
        "fn f() -> i64:\n    let d: dict[i64, i64] = {1: 10, 2: 20}\n    return 0\n",
    );
}

#[test]
fn agg_dict_three() {
    compile_ok(
        "agg_dict_three",
        "fn f() -> i64:\n    let d: dict[i64, i64] = {1: 10, 2: 20, 3: 30}\n    return 0\n",
    );
}

#[test]
fn agg_dict_neg_keys() {
    compile_ok(
        "agg_dict_neg_keys",
        "fn f() -> i64:\n    let d: dict[i64, i64] = {-1: 10, -2: 20}\n    return 0\n",
    );
}

#[test]
fn agg_dict_param_values() {
    compile_ok(
        "agg_dict_param_values",
        "fn f(a: i64, b: i64) -> i64:\n    let d: dict[i64, i64] = {1: a, 2: b}\n    return 0\n",
    );
}

// =====================================================================
// Set aggregates (Set literals are M2 form 24c)
// =====================================================================

#[test]
fn agg_set_single() {
    compile_ok(
        "agg_set_single",
        "fn f() -> i64:\n    let s: set[i64] = {1}\n    return 0\n",
    );
}

#[test]
fn agg_set_two() {
    compile_ok(
        "agg_set_two",
        "fn f() -> i64:\n    let s: set[i64] = {1, 2}\n    return 0\n",
    );
}

#[test]
fn agg_set_three() {
    compile_ok(
        "agg_set_three",
        "fn f() -> i64:\n    let s: set[i64] = {1, 2, 3}\n    return 0\n",
    );
}

#[test]
fn agg_set_with_neg() {
    compile_ok(
        "agg_set_with_neg",
        "fn f() -> i64:\n    let s: set[i64] = {-1, 0, 1}\n    return 0\n",
    );
}

#[test]
fn agg_set_param_input() {
    compile_ok(
        "agg_set_param_input",
        "fn f(a: i64, b: i64, c: i64) -> i64:\n    let s: set[i64] = {a, b, c}\n    return 0\n",
    );
}

// =====================================================================
// Aggregate compositions
// =====================================================================

#[test]
fn agg_list_then_tuple() {
    compile_ok(
        "agg_list_then_tuple",
        "fn f() -> i64:\n    let l: list[i64] = [1, 2]\n    let t: (i64, i64) = (3, 4)\n    return 0\n",
    );
}

#[test]
fn agg_tuple_then_dict() {
    compile_ok(
        "agg_tuple_then_dict",
        "fn f() -> i64:\n    let t: (i64, i64) = (1, 2)\n    let d: dict[i64, i64] = {1: 2}\n    return 0\n",
    );
}

#[test]
fn agg_all_four_kinds() {
    compile_ok(
        "agg_all_four_kinds",
        "fn f() -> i64:\n    let t: (i64, i64) = (1, 2)\n    let l: list[i64] = [3, 4]\n    let d: dict[i64, i64] = {5: 6}\n    let s: set[i64] = {7, 8}\n    return 0\n",
    );
}

#[test]
fn agg_in_branch() {
    compile_ok(
        "agg_in_branch",
        "fn f(b: bool) -> i64:\n    if b:\n        let l: list[i64] = [1, 2, 3]\n    else:\n        let l: list[i64] = [4, 5, 6]\n    return 0\n",
    );
}

#[test]
fn agg_two_lists_same_body() {
    compile_ok(
        "agg_two_lists_same_body",
        "fn f() -> i64:\n    let a: list[i64] = [1]\n    let b: list[i64] = [2]\n    return 0\n",
    );
}

#[test]
fn agg_dict_in_loop_body() {
    compile_ok(
        "agg_dict_in_loop_body",
        "fn f() -> i64:\n    let n = 0\n    while n < 5:\n        let d: dict[i64, i64] = {1: 2}\n        n = n + 1\n    return 0\n",
    );
}

#[test]
fn agg_nested_let_lists() {
    compile_ok(
        "agg_nested_let_lists",
        "fn f() -> i64:\n    let a: list[i64] = [1, 2]\n    let b: list[i64] = [3, 4]\n    let c: list[i64] = [5, 6]\n    return 0\n",
    );
}

#[test]
fn agg_after_arithmetic() {
    compile_ok(
        "agg_after_arithmetic",
        "fn f(x: i64) -> i64:\n    let y = x + 1\n    let l: list[i64] = [y, y]\n    return 0\n",
    );
}
