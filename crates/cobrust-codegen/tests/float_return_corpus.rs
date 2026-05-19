//! ADR-0033 float-return codegen corpus.
//!
//! Targets the `Ty::None` synthetic-temp type-inference gap in
//! `cranelift_backend.rs::operand_ty` first surfaced by the M9
//! cross-arch validation finding
//! (`docs/agent/findings/m9-cross-arch-linux-x86_64-validation.md`).
//!
//! Bug shape: MIR lowering introduces `_un` / `_bin` / `_callret`
//! synthetic locals declared as `Ty::None`. When the body's return
//! chain ends with `_0 = Use(Copy(_un))`, the codegen-time return-type
//! inference path (`infer_return_type` → `rvalue_ty` → `operand_ty`)
//! looks up `body.locals[_un].ty` (= `Ty::None`) and resolves it to
//! `cranelift_scalar_ty(None) = I8`, so `_0` gets declared `I8`. The
//! actual stored value is `F64`. The implicit F64 → I8 narrowing
//! lowers via Cranelift's `CvtFloatToSintSeq`, which on x86_64
//! `unreachable!()`s for `Size8` destinations and on aarch64
//! silently produces wrong values. The fix unifies the inference so
//! `operand_ty` consults the inferred-locals map fixed-point.
//!
//! Categories (≥ 12 cases, mixing compile-only + value-correctness):
//!
//! 1. Direct float return (4 cases: const + arith + neg)
//! 2. Float arithmetic returns (`+ - * /`)
//! 3. Float negation
//! 4. Float chain through temp (the bug-trigger pattern)
//! 5. Mixed cast through temp
//! 6. Float compare in if (returns int from float predicate)
//!
//! Each case shells out via the `cobrust` binary, runs the produced
//! executable, captures stdout (via `print(f"{x}")`-style probes), and
//! asserts. Compile-only categories run via direct `compile_ok`-style
//! `emit()` calls so x86_64 panics surface even where stdout is hard
//! to materialize.

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

use std::path::{Path, PathBuf};
use std::process::Command;

use cobrust_codegen::{Artifact, ArtifactKind, Backend, OptLevel, TargetSpec, emit};
use cobrust_frontend::{parse_str, span::FileId};
use cobrust_hir::{Session, lower as hir_lower};
use cobrust_mir::{Module as MirModule, lower as mir_lower};
use cobrust_types::check;
use target_lexicon::Triple;

// =====================================================================
// In-process compile-only helpers (compile_ok shape — same as
// `codegen_well_formed.rs`'s p008/p017/p018/p019). On x86_64 these
// trigger `CvtFloatToSintSeq` panic when the bug is present; on arm64
// they pass even with the bug, but the value-correctness shell-out
// tests below catch the latent miscompile.
// =====================================================================

fn lower_to_mir(src: &str) -> MirModule {
    let module = parse_str(src, FileId::SYNTHETIC).expect("parse");
    let mut sess = Session::new();
    let hir = hir_lower(&module, &mut sess).expect("hir lower");
    let typed = check(&hir).expect("type check");
    mir_lower(&typed).expect("mir lower")
}

fn host_object_spec(name: &str) -> TargetSpec {
    let dir = std::env::temp_dir().join(format!("cobrust-adr0033-{name}-{}", std::process::id()));
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
    }
}

fn compile_ok(name: &str, src: &str) {
    let mir = lower_to_mir(src);
    let spec = host_object_spec(name);
    let artifact = emit(&mir, spec).unwrap_or_else(|e| panic!("emit `{name}`: {e}"));
    let path = artifact.path();
    let meta =
        std::fs::metadata(path).unwrap_or_else(|e| panic!("metadata `{}`: {e}", path.display()));
    assert!(meta.len() > 0, "object empty for `{name}`");
    assert!(matches!(artifact, Artifact::Object(_)));
}

// =====================================================================
// Shell-out helpers (value-correctness probes via `print(f"{x}")`).
// Pattern transcribed from
// `crates/cobrust-codegen/tests/while_if_corpus.rs` (M11.1 fix).
// =====================================================================

fn cobrust_binary() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest
        .parent()
        .and_then(Path::parent)
        .expect("workspace root from CARGO_MANIFEST_DIR");
    let debug_bin = workspace.join("target/debug/cobrust");
    if debug_bin.exists() {
        return debug_bin;
    }
    let release_bin = workspace.join("target/release/cobrust");
    if release_bin.exists() {
        return release_bin;
    }
    PathBuf::from("cobrust")
}

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("workspace root")
}

fn write_temp(name: &str, contents: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cobrust-adr0033-corpus-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&dir);
    let p = dir.join(format!("{name}.cb"));
    std::fs::write(&p, contents).expect("write temp .cb");
    p
}

fn build(name: &str, src_path: &Path) -> PathBuf {
    let bin = cobrust_binary();
    let workspace = workspace_root();
    let exe_dir = std::env::temp_dir().join(format!(
        "cobrust-adr0033-exe-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&exe_dir);
    let exe_path = exe_dir.join(name);
    let out = Command::new(&bin)
        .arg("build")
        .arg(src_path)
        .arg("-o")
        .arg(&exe_path)
        .arg("--quiet")
        .current_dir(&workspace)
        .output()
        .expect("invoke cobrust build");
    assert!(
        out.status.success(),
        "cobrust build failed for {name}:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    exe_path
}

fn run(exe_path: &Path) -> String {
    let out = Command::new(exe_path)
        .output()
        .expect("invoke produced executable");
    assert!(
        out.status.success(),
        "binary {} exited non-zero ({:?})\nstderr={}",
        exe_path.display(),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

// =====================================================================
// Category 1 — direct float return (no arithmetic temp)
// =====================================================================

#[test]
fn fr01_const_float_pos() {
    // `_0 = Constant::Float(...)` direct — the simplest case.
    compile_ok("fr01", "fn f() -> f64:\n    return 1.5\n");
}

#[test]
fn fr02_param_passthrough() {
    // `_0 = Use(Copy(_x))` where `_x: Float` (declared, not Ty::None).
    // Tests the easy path: declared float local, return chain is
    // a single Copy.
    compile_ok("fr02", "fn f(x: f64) -> f64:\n    return x\n");
}

// =====================================================================
// Category 2 — bug-trigger: float through Ty::None temp
// =====================================================================

#[test]
fn fr03_const_float_neg_via_un() {
    // The exact pattern in the cross-arch finding: `return -2.71`
    // lowers to:
    //   _un: Ty::None = UnaryOp(Neg, Constant::Float(2.71))   — _un should be F64
    //   _0  = Use(Copy(_un))                                  — bug: _0 typed I8
    //
    // Equivalent to existing test `p008_const_float_neg`; here we keep
    // it inside the ADR-0033 corpus so the fix verification is
    // self-contained.
    compile_ok("fr03", "fn f() -> f64:\n    return -2.71\n");
}

#[test]
fn fr04_float_add_via_bin() {
    // `_bin = BinaryOp(Add, a, b)` then `_0 = Use(Copy(_bin))`.
    // Same pattern as p017_fadd.
    compile_ok("fr04", "fn f(a: f64, b: f64) -> f64:\n    return a + b\n");
}

#[test]
fn fr05_float_sub_via_bin() {
    // p018_fsub equivalent.
    compile_ok("fr05", "fn f(a: f64, b: f64) -> f64:\n    return a - b\n");
}

#[test]
fn fr06_float_mul_via_bin() {
    // p019_fmul equivalent.
    compile_ok("fr06", "fn f(a: f64, b: f64) -> f64:\n    return a * b\n");
}

#[test]
fn fr07_float_neg_param() {
    // `return -x` where `x: f64` is a parameter. Lowers to
    // `_un: Ty::None = UnaryOp(Neg, Copy(_x))`, `_0 = Copy(_un)`.
    // Bug-trigger: `_un` is `Ty::None`, return chain reads it. Same
    // mechanism as fr03 but with a parameter operand instead of a
    // constant — verifies the inference handles `Operand::Copy(_x)`
    // (declared `Ty::Float`) correctly inside the unary-op rvalue.
    compile_ok("fr07", "fn f(x: f64) -> f64:\n    return -x\n");
}

// ADR-0033 scope note: float division (`fn f(a: f64, b: f64) -> f64:
// return a / b`) is intentionally **not** in this corpus.
//
// The float-return type-inference fix in this ADR exposes a separate,
// orthogonal bug in the MIR `lower_bin` division-by-zero assert path
// (`crates/cobrust-mir/src/lower.rs:1218-1227`): the assert lowers as
// `_divcond = NotEq(rhs, Constant::Int(0))` regardless of the
// operand type, producing `fcmp ne <f64>, <i64>` IR that the
// Cranelift verifier rejects as a type mismatch. Pre-ADR-0033 the
// verifier silently accepted the IR (because the return-chain bug
// produced an I8 return type that was rejected at emit time first),
// masking this latent issue.
//
// Tracking the float-div assert bug as a follow-up finding is the
// surgical move; ADR-0033 stays scoped to the cross-arch
// type-inference gap. The 4 named failing tests
// (`p008_const_float_neg`, `p017_fadd`, `p018_fsub`, `p019_fmul`)
// are all non-div paths and DO get fixed by this ADR.

// =====================================================================
// Category 3 — chain through multiple Ty::None temps (depth ≥ 2)
// =====================================================================

#[test]
fn fr08_float_chain_through_let() {
    // `let y: f64 = x * 2.0; return y` — lowers to
    //   _bin: Ty::None  = BinaryOp(Mul, x, 2.0)
    //   y: Float        = Use(Copy(_bin))     (declared f64, ok)
    //   _0              = Use(Copy(y))        (declared f64, fine)
    //
    // y has a real declared type so this is not the worst trigger,
    // but it exercises the bin-temp inference depth-1 chain.
    compile_ok(
        "fr08",
        "fn f(x: f64) -> f64:\n    let y: f64 = x * 2.0\n    return y\n",
    );
}

#[test]
fn fr09_float_double_neg() {
    // Two unary ops: `--x` lowers to two `_un: Ty::None` temps in
    // series; the *outer* temp's rvalue is `UnaryOp(Neg, Copy(inner))`
    // where `inner: Ty::None`. The fix's fixed-point inference must
    // resolve `inner → F64` BEFORE typing the outer temp.
    compile_ok("fr09", "fn f(x: f64) -> f64:\n    return -(-x)\n");
}

#[test]
fn fr10_float_compound_arith() {
    // `(a + b) * c` — TWO bin temps in series:
    //   _bin1: Ty::None = BinaryOp(Add, a, b)
    //   _bin2: Ty::None = BinaryOp(Mul, Copy(_bin1), c)
    //   _0             = Use(Copy(_bin2))
    //
    // Without fixed-point, the inner _bin1 inference is correct
    // (operand_ty(a)=F64), but the outer _bin2 calls
    // operand_ty(Copy(_bin1)) → declared(_bin1)=Ty::None → I8 (bug).
    // Validates the chain-traversal is fixed.
    compile_ok(
        "fr10",
        "fn f(a: f64, b: f64, c: f64) -> f64:\n    return (a + b) * c\n",
    );
}

// =====================================================================
// Category 4 — float compare returning int (different return type)
// =====================================================================

#[test]
fn fr11_float_compare_returns_bool() {
    // `return a > b` — comparisons emit `BinaryOp(Gt, ...)` whose
    // rvalue_ty is hard-coded I8 (bool). The return-chain bug is
    // float-specific so this should ALWAYS pass. Acts as a control
    // (covers the no-bug shape so a regression in the int path is
    // also caught).
    compile_ok("fr11", "fn f(a: f64, b: f64) -> bool:\n    return a > b\n");
}

#[test]
fn fr12_float_compare_in_if_returns_int() {
    // `if x > 0.0: return 1 else: return 0` — switches on float-compare
    // (no bug), returns an int (no bug). Sanity check that the
    // post-fix codegen does not regress the integer return path.
    compile_ok(
        "fr12",
        "fn f(x: f64) -> i64:\n    if x > 0.0:\n        return 1\n    else:\n        return 0\n",
    );
}

#[test]
fn fr13_float_neg_compound() {
    // `return -a + b` — combines unary + binary temps.
    //   _un:  Ty::None = UnaryOp(Neg, a)
    //   _bin: Ty::None = BinaryOp(Add, Copy(_un), b)
    //   _0             = Use(Copy(_bin))
    compile_ok("fr13", "fn f(a: f64, b: f64) -> f64:\n    return -a + b\n");
}

// =====================================================================
// Category 5 — value-correctness probes via shell-out + bracket
// comparisons + `print_int`.
//
// Constraints (M11 / M11.2 deferred-feature inventory):
//
// - `print` only accepts string literals
//   (`cobrust-cli/src/build/intrinsics.rs:48`). Value-shaped probes
//   use `print_int` (ADR-0030 §Decision step 5).
// - User-defined function calls from `main` are M11.2-deferred: the
//   `Terminator::Call` whose `func` is `Operand::Move(FnRef-local)`
//   falls through to the M9 stub at
//   `cranelift_backend.rs:903-904` which writes `iconst.i64 0` to
//   the destination. So a probe calling `under_test()` from `main`
//   reads garbage *regardless of ADR-0033's fix* — its failure
//   would not attribute to the float-return bug.
//
// Therefore the probe shape exercises **chain-depth ≥ 2** synthetic
// temps inside `main` itself. The depth-2 case is the strong
// bug-trigger because `infer_local_types` (pre-fix) would type the
// outer temp via `operand_ty(Copy(_inner))` → declared(_inner) =
// `Ty::None` → `I8`, even when the inner temp's inferred type is
// `F64`. That is exactly what the fixed-point iteration in this ADR
// closes.
//
// Depth-1 cases (`let x: f64 = -2.71`, `let x: f64 = a + b`) do NOT
// reproduce the bug because the rvalue's operand is a `Constant` or
// a parameter `Copy(_x)` whose declared type is fully resolved —
// the original `infer_local_types` already handled them correctly.
// Those paths are covered by the compile-only tests above
// (`fr03..fr07`).
// =====================================================================

#[test]
fn fr14_value_correctness_double_neg_const() {
    // `let y: f64 = -(-3.25)` — depth-2 chain through two `_un:
    // Ty::None` temps. Inner: `_inner = UnaryOp(Neg, Float(3.25))`.
    // Outer: `_outer = UnaryOp(Neg, Copy(_inner))`. Without the
    // fixed-point loop, `_outer`'s rvalue_ty consults
    // operand_ty(Copy(_inner)) → declared(_inner)=None → I8, so
    // `_outer` Variable is I8. The fneg.f64 → write_place(I8) goes
    // through `fcvt_to_sint_sat(I8, ...)` which on aarch64 silently
    // truncates to {-128..127}; -(-3.25) → 3.25 → fcvt_to_sint_sat
    // → 3 → fcvt_from_sint(F64, 3) → 3.0 (close to 3.25 but
    // outside the ±0.01 bracket).
    let src = write_temp(
        "fr14",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let y: f64 = -(-3.25)\n\
         \x20\x20\x20\x20if y > 3.24:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if y < 3.26:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20print_int(1)\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20return 0\n\
         \x20\x20\x20\x20print_int(0)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("fr14", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "1\n", "fr14 stdout mismatch: {stdout:?}");
}

#[test]
fn fr15_value_correctness_compound_arith() {
    // `let z: f64 = (a + b) * c` — depth-2 binary chain.
    //   _bin1 = BinaryOp(Add, a, b)
    //   _bin2 = BinaryOp(Mul, Copy(_bin1), c)
    //   z     = Use(Copy(_bin2))
    //
    // `_bin2`'s rvalue_ty consults operand_ty(Copy(_bin1)) →
    // declared(_bin1)=None → I8 pre-fix. Post-fix, fixed-point
    // iteration ensures _bin1 is resolved to F64 before _bin2 is
    // evaluated. Pre-fix sequence: fmul.f64 gives 1.8, then
    // write_place to var_ty=I8 emits fcvt_to_sint_sat(I8, 1.8) = 1,
    // then z reads as fcvt_from_sint(F64, 1) = 1.0 — outside
    // [1.79, 1.81]. Post-fix z = 1.8 inside the bracket.
    //
    // Note: arithmetic intentionally uses non-integer-valued result
    // so the i8 saturation truncates visibly. (1.5 + 2.5) * 2.0 = 8.0
    // would round-trip cleanly through i8 and mask the bug.
    let src = write_temp(
        "fr15",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let a: f64 = 0.5\n\
         \x20\x20\x20\x20let b: f64 = 0.7\n\
         \x20\x20\x20\x20let c: f64 = 1.5\n\
         \x20\x20\x20\x20let z: f64 = (a + b) * c\n\
         \x20\x20\x20\x20if z > 1.79:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if z < 1.81:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20print_int(1)\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20return 0\n\
         \x20\x20\x20\x20print_int(0)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("fr15", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "1\n", "fr15 stdout mismatch: {stdout:?}");
}

#[test]
fn fr16_value_correctness_neg_plus() {
    // `let r: f64 = -a + b` — mixed unary+binary depth-2 chain.
    //   _un  = UnaryOp(Neg, a)        — _un: Ty::None
    //   _bin = BinaryOp(Add, Copy(_un), b)  — _bin: Ty::None
    //   r    = Use(Copy(_bin))
    //
    // `_bin`'s rvalue_ty pre-fix consults operand_ty(Copy(_un)) →
    // declared(_un)=None → I8. Post-fix, fixed-point converges to
    // F64. -1.0 + 4.5 = 3.5; bracket around 3.5.
    let src = write_temp(
        "fr16",
        "fn main() -> i64:\n\
         \x20\x20\x20\x20let a: f64 = 1.0\n\
         \x20\x20\x20\x20let b: f64 = 4.5\n\
         \x20\x20\x20\x20let r: f64 = -a + b\n\
         \x20\x20\x20\x20if r > 3.49:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20if r < 3.51:\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20print_int(1)\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20return 0\n\
         \x20\x20\x20\x20print_int(0)\n\
         \x20\x20\x20\x20return 0\n",
    );
    let exe = build("fr16", &src);
    let stdout = run(&exe);
    assert_eq!(stdout, "1\n", "fr16 stdout mismatch: {stdout:?}");
}
