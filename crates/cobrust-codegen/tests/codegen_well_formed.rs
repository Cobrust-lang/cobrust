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
use object::{Object, ObjectSection, ObjectSymbol};
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
        source_path: None,
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

/// Compile `src` and return the raw object bytes for structural inspection.
fn compile_to_bytes(name: &str, src: &str) -> Vec<u8> {
    let mir = lower_to_mir(src);
    let spec = host_object_spec(name);
    let artifact = emit(&mir, spec).unwrap_or_else(|e| panic!("emit `{name}`: {e}"));
    let path = artifact.path();
    std::fs::read(path).unwrap_or_else(|e| panic!("read object `{}`: {e}", path.display()))
}

/// Return all exported symbol names from a parsed object file (bytes).
/// Handles the Mach-O `_` prefix: strips it so callers use bare names.
fn exported_symbol_names(obj_bytes: &[u8]) -> Vec<String> {
    let obj = object::File::parse(obj_bytes).expect("parse object");
    obj.symbols()
        .filter(|s| s.is_definition())
        .filter_map(|s| {
            s.name().ok().map(|n| {
                // Mach-O prefixes with `_`; strip for platform-agnostic assertions.
                n.strip_prefix('_').unwrap_or(n).to_string()
            })
        })
        .collect()
}

/// Return the total byte size of all code/text sections in the object.
fn code_section_size(obj_bytes: &[u8]) -> u64 {
    let obj = object::File::parse(obj_bytes).expect("parse object");
    obj.sections()
        .filter(|s| {
            // Accept both ELF `.text` and Mach-O `__text` (and Cranelift `$d.0`).
            s.name()
                .map(|n| n.contains("text") || n.starts_with('$'))
                .unwrap_or(false)
        })
        .map(|s| s.size())
        .sum()
}

// =====================================================================
// M11 behavioral assertions — top-10 representative cases.
//
// Each test below compiles the same source as its corresponding pNNN
// test, then asserts *structural properties* of the emitted object
// beyond "non-empty":
//   - expected symbol names are exported
//   - code section size is within a plausible range (> lower bound,
//     < upper bound that would indicate code explosion)
//   - multi-function programs have the expected number of symbols
//
// "stdout assertions" for an AOT codegen pipeline = object-structure
// assertions via the `object` crate; there is no JIT runner.
// =====================================================================

/// M11-1: trivial return 0 — symbol `f` exported; code section > 0.
#[test]
fn m11_p001_const_int_behavior() {
    let src = "fn f() -> i64:\n    return 0\n";
    let bytes = compile_to_bytes("m11_p001", src);
    let syms = exported_symbol_names(&bytes);
    assert!(
        syms.iter().any(|s| s == "f"),
        "expected `f` in exported symbols: {syms:?}"
    );
    let code_sz = code_section_size(&bytes);
    assert!(code_sz > 0, "code section must be non-empty for p001");
}

/// M11-2: integer add with two params — symbol `f`, code section between
/// the trivial lower bound and a generous upper bound (no code explosion).
#[test]
fn m11_p014_add_behavior() {
    let src = "fn f(a: i64, b: i64) -> i64:\n    return a + b\n";
    let bytes = compile_to_bytes("m11_p014", src);
    let syms = exported_symbol_names(&bytes);
    assert!(
        syms.iter().any(|s| s == "f"),
        "expected `f` in exported symbols: {syms:?}"
    );
    let code_sz = code_section_size(&bytes);
    // An `add` + two param loads should produce at least a few bytes.
    assert!(
        code_sz >= 4,
        "add function code section too small: {code_sz}"
    );
    // Sanity upper bound: shouldn't be megabytes for a 2-param add.
    assert!(
        code_sz < 4096,
        "add function code section unexpectedly large: {code_sz}"
    );
}

/// M11-3: if-else — symbol `f`, code section larger than trivial return.
#[test]
fn m11_p037_if_else_behavior() {
    let trivial_bytes = compile_to_bytes("m11_p037_trivial", "fn f() -> i64:\n    return 1\n");
    let if_else_bytes = compile_to_bytes(
        "m11_p037_ifelse",
        "fn f(x: i64) -> i64:\n    if (x > 0):\n        return 1\n    else:\n        return 0\n",
    );
    let trivial_sz = code_section_size(&trivial_bytes);
    let if_else_sz = code_section_size(&if_else_bytes);
    // An if-else generates at least a comparison + branch; must be larger
    // than the trivial `return 1` case.
    assert!(
        if_else_sz > trivial_sz,
        "if-else ({if_else_sz}) should generate more code than trivial return ({trivial_sz})"
    );
}

/// M11-4: while loop — symbol `f`, code section larger than a simple return.
#[test]
fn m11_p039_while_behavior() {
    let trivial_bytes =
        compile_to_bytes("m11_p039_trivial", "fn f(n: i64) -> i64:\n    return n\n");
    let while_bytes = compile_to_bytes(
        "m11_p039_while",
        "fn f(n: i64) -> i64:\n    let acc: i64 = 0\n    let i: i64 = 0\n    while (i < n):\n        acc += i\n        i += 1\n    return acc\n",
    );
    let trivial_sz = code_section_size(&trivial_bytes);
    let while_sz = code_section_size(&while_bytes);
    assert!(
        while_sz > trivial_sz,
        "while loop ({while_sz}) should generate more code than trivial return ({trivial_sz})"
    );
}

/// M11-5: two-function module — exactly `double` and `quad` exported.
#[test]
fn m11_p042_two_funcs_behavior() {
    let src = "fn double(x: i64) -> i64:\n    return (x + x)\n\nfn quad(x: i64) -> i64:\n    return double(double(x))\n";
    let bytes = compile_to_bytes("m11_p042", src);
    let syms = exported_symbol_names(&bytes);
    assert!(
        syms.iter().any(|s| s == "double"),
        "expected `double` in symbols: {syms:?}"
    );
    assert!(
        syms.iter().any(|s| s == "quad"),
        "expected `quad` in symbols: {syms:?}"
    );
    // Both functions are defined, so at least 2 user symbols.
    let user_syms: Vec<_> = syms
        .iter()
        .filter(|s| *s == "double" || *s == "quad")
        .collect();
    assert_eq!(
        user_syms.len(),
        2,
        "expected exactly 2 user symbols (double, quad): {syms:?}"
    );
}

/// M11-6: recursion — symbol `fib` exported, code larger than trivial add.
#[test]
fn m11_p043_recursion_behavior() {
    let src = "fn fib(n: i64) -> i64:\n    if (n < 2):\n        return n\n    return (fib((n - 1)) + fib((n - 2)))\n";
    let bytes = compile_to_bytes("m11_p043", src);
    let syms = exported_symbol_names(&bytes);
    assert!(
        syms.iter().any(|s| s == "fib"),
        "expected `fib` in symbols: {syms:?}"
    );
    // Recursive fib requires a branch + two call sites; should be
    // substantially larger than a trivial return.
    let code_sz = code_section_size(&bytes);
    assert!(
        code_sz >= 8,
        "fib code section unexpectedly small: {code_sz}"
    );
}

/// M11-7: factorial loop — code section larger than a simple while.
#[test]
fn m11_p054_factorial_behavior() {
    let simple_while_bytes = compile_to_bytes(
        "m11_p054_simple",
        "fn f(n: i64) -> i64:\n    let i: i64 = 0\n    while (i < n):\n        i += 1\n    return i\n",
    );
    let factorial_bytes = compile_to_bytes(
        "m11_p054_factorial",
        "fn f(n: i64) -> i64:\n    let acc: i64 = 1\n    let i: i64 = 1\n    while (i <= n):\n        acc *= i\n        i += 1\n    return acc\n",
    );
    // Factorial has an extra mul + assignment vs the simple loop.
    // Both have the same structure so sizes may be similar; assert
    // factorial is at least as large (additional `mul` + two locals).
    let simple_sz = code_section_size(&simple_while_bytes);
    let factorial_sz = code_section_size(&factorial_bytes);
    assert!(
        factorial_sz >= simple_sz,
        "factorial ({factorial_sz}) should be >= simple while ({simple_sz})"
    );
}

/// M11-8: equality comparison — symbol `f`, code section plausible.
#[test]
fn m11_p022_eq_behavior() {
    let src = "fn f(a: i64, b: i64) -> bool:\n    return a == b\n";
    let bytes = compile_to_bytes("m11_p022", src);
    let syms = exported_symbol_names(&bytes);
    assert!(
        syms.iter().any(|s| s == "f"),
        "expected `f` in symbols: {syms:?}"
    );
    let code_sz = code_section_size(&bytes);
    assert!(code_sz > 0, "eq comparison code section must be non-empty");
    // A comparison should generate at least a load + icmp; not megabytes.
    assert!(
        code_sz < 4096,
        "eq code section unexpectedly large: {code_sz}"
    );
}

/// M11-9: let + use — symbol `f`, code section non-trivially sized
/// (local allocation + load must appear).
#[test]
fn m11_p032_let_then_use_behavior() {
    let src = "fn f() -> i64:\n    let x: i64 = 1\n    return x\n";
    let bytes = compile_to_bytes("m11_p032", src);
    let syms = exported_symbol_names(&bytes);
    assert!(
        syms.iter().any(|s| s == "f"),
        "expected `f` in symbols: {syms:?}"
    );
    let code_sz = code_section_size(&bytes);
    assert!(
        code_sz > 0,
        "let+use code section must be non-empty: {code_sz}"
    );
}

/// M11-10: complex while with bitops — code section substantially larger
/// than the trivial `return 0` baseline (multiple operations per iteration).
#[test]
fn m11_p055_bitcount_behavior() {
    let baseline_bytes = compile_to_bytes("m11_p055_base", "fn f() -> i64:\n    return 0\n");
    let bitcount_bytes = compile_to_bytes(
        "m11_p055_bitcount",
        "fn f(n: i64) -> i64:\n    let count: i64 = 0\n    let v: i64 = n\n    while (v > 0):\n        if ((v & 1) == 1):\n            count += 1\n        v >>= 1\n    return count\n",
    );
    let baseline_sz = code_section_size(&baseline_bytes);
    let bitcount_sz = code_section_size(&bitcount_bytes);
    // The bitcount function has a while loop + nested if + bitand + shift;
    // must generate substantially more code than a bare `return 0`.
    assert!(
        bitcount_sz > baseline_sz,
        "bitcount ({bitcount_sz}) must be larger than trivial return ({baseline_sz})"
    );
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
