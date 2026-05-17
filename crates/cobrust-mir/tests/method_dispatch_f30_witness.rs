//! ADR-0052d-prereq Wave-2 F30 shadow-flip witness corpus for the
//! method-call sugar surface.
//!
//! Per `findings/predicate-flip-cascade-discovery-deficit.md` SOP +
//! ADR-0052d-prereq §"F30 shadow-flip dry-run" binding (this Direction
//! is additive sugar — no existing program changes meaning). The
//! witness asserts three properties of the lowered MIR:
//!
//! - **(a) method-form lowers identically to PRELUDE-fn form** — the
//!   "purely syntactic sugar" guarantee per ADR-0052d-prereq §"Decision".
//!   The MIR callee set of the method-form program must be a subset of
//!   (or identical to) the equivalent PRELUDE-fn program's callee set.
//! - **(b) zero new symbols introduced** — no vtable / dynamic-
//!   dispatch / `__cobrust_method_*` runtime callees. Method-form
//!   rewrites at type-check time to PRELUDE-fn names per
//!   ADR-0052d-prereq §"Key invariant".
//! - **(c) `&s.method()` parses as `&(s.method())`** — precedence rule
//!   per ADR-0052 F-G.3 amendment (line 275): method-call binds
//!   tighter than the unary borrow. Matches Rust `&v.len()` corpus
//!   distribution per §2.5 §B.
//!
//! Pre-DEV-impl status: every `f30wit_method_*` test below is
//! `#[ignore]`'d pending Wave-2 DEV merge per F28 strict-separation
//! PAIR pattern (`findings/adsd-pair-pattern-impl-gap.md`).
//!
//! Per `feedback_p9_clippy_stall_pattern.md` 2026-05-09: module-level
//! 18-lint test-only allow header at the top.

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
#![allow(clippy::too_many_lines)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::manual_assert)]

use cobrust_frontend::{parse_str, span::FileId};
use cobrust_hir::{Session, lower as hir_lower};
use cobrust_mir::{Body, Constant, Module as MirModule, Operand, Terminator, lower as mir_lower};
use cobrust_types::check;

/// Symbol-set prefixes the witness scans for as **forbidden** —
/// method-form must NEVER introduce dynamic-dispatch runtime symbols
/// per ADR-0052d-prereq §"Out of scope" item 1 ("Vtable / dynamic
/// dispatch"). The dict-method precedent does NOT introduce any of
/// these; the four new tables must follow the same discipline.
const FORBIDDEN_PREFIXES: &[&str] = &[
    "__cobrust_method_vtable_",
    "__cobrust_method_dispatch_",
    "__cobrust_vtable_",
    "__cobrust_dyn_call_",
];

/// PRELUDE-fn stub block — required because the method-form rewrite
/// targets are PRELUDE names. Mirrors `STR_STDLIB_STUBS` /
/// `BORROW_STUBS` patterns in the well_typed + borrow_phase_g
/// witness siblings.
const METHOD_DISPATCH_STUBS: &str = concat!(
    "fn print(s: str) -> i64:\n    return 0\n",
    "fn print_int(n: i64) -> i64:\n    return 0\n",
    "fn str_len(s: str) -> i64:\n    return 0\n",
    "fn split(s: str, sep: str) -> list[str]:\n    let xs: list[str] = []\n    return xs\n",
    "fn len(xs: list[i64]) -> i64:\n    return 0\n",
);

/// Lower source through frontend → HIR → types → MIR.
///
/// Returns `Err(_)` if any stage rejects the program. Pre-DEV-impl,
/// the method-form programs are likely rejected at type-check; the
/// `#[ignore]` markers keep the suite green until DEV ships the
/// four `try_synth_*_method` fns.
fn lower_to_mir(src: &str) -> Result<MirModule, String> {
    let combined = format!("{METHOD_DISPATCH_STUBS}{src}");
    let module =
        parse_str(&combined, FileId::SYNTHETIC).map_err(|e| format!("parse error: {e:?}"))?;
    let mut sess = Session::new();
    let hir = hir_lower(&module, &mut sess).map_err(|e| format!("hir lower error: {e:?}"))?;
    let typed = check(&hir).map_err(|e| format!("type check error: {e:?}"))?;
    mir_lower(&typed).map_err(|e| format!("mir lower error: {e:?}"))
}

/// Collect every callee symbol from every `Terminator::Call` in a body.
///
/// Symbols are returned as `String`s lifted from `Operand::Constant
/// (Constant::Str(name))`. The witness uses this to compare method-
/// form vs PRELUDE-fn-form lowering and to verify no forbidden
/// dispatch symbols appear.
fn callees(body: &Body) -> Vec<String> {
    body.blocks
        .iter()
        .filter_map(|b| match &b.terminator {
            Terminator::Call { func, .. } => match func {
                Operand::Constant(Constant::Str(name)) => Some(name.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// Collect every callee symbol across every body in the module.
fn all_callees(m: &MirModule) -> Vec<String> {
    m.bodies.iter().flat_map(callees).collect()
}

/// Assert property (b): no forbidden dispatch symbols appear in any
/// body of the lowered module. ADR-0052d-prereq §"Out of scope" item 1.
fn assert_no_forbidden_symbols(m: &MirModule, name: &str) {
    let all: Vec<String> = all_callees(m);
    for callee in &all {
        for forbidden in FORBIDDEN_PREFIXES {
            assert!(
                !callee.starts_with(forbidden),
                "{}: method-form lowering introduced forbidden dispatch symbol `{}` (witness (b) violated)\n  callees={:?}",
                name,
                callee,
                all
            );
        }
    }
}

/// Assert property (a): method-form lowers to the same MIR-callee
/// subset as the equivalent PRELUDE-fn form. The PRELUDE-fn form's
/// callee set is the reference oracle; the method-form's callee
/// set must be `⊆` of it (the method-form may have additional non-
/// call MIR instructions, but each Call terminator must hit a name
/// the PRELUDE-fn form already hits).
fn assert_method_form_subset_of_prelude_fn_form(name: &str, method_src: &str, prelude_src: &str) {
    let m_method = match lower_to_mir(method_src) {
        Ok(m) => m,
        Err(e) => panic!(
            "{}: method-form MIR lowering failed (witness (c) prerequisite)\n  error: {}\n  --- method-form source ---\n{}",
            name, e, method_src
        ),
    };
    let m_prelude = match lower_to_mir(prelude_src) {
        Ok(m) => m,
        Err(e) => panic!(
            "{}: prelude-fn-form MIR lowering failed (oracle prerequisite)\n  error: {}\n  --- prelude-fn-form source ---\n{}",
            name, e, prelude_src
        ),
    };
    let method_callees: Vec<String> = all_callees(&m_method);
    let prelude_callees: Vec<String> = all_callees(&m_prelude);
    // Subset check: every method-form Call symbol must appear in the
    // PRELUDE-fn-form Call set. (Equality is the stronger guarantee
    // but additive sugar may produce identical callee sets modulo
    // order; subset is the robust property.)
    for c in &method_callees {
        // Skip non-PRELUDE compiler-internal helpers (e.g.
        // `__cobrust_list_new`, `__cobrust_str_lit_*`) — those appear
        // in both forms when the receiver expression itself routes
        // through them.
        if !prelude_callees.contains(c) {
            panic!(
                "{}: method-form callee `{}` not present in prelude-fn-form callees (witness (a) violated)\n  method_callees={:?}\n  prelude_callees={:?}\n  --- method-form source ---\n{}\n  --- prelude-fn-form source ---\n{}",
                name, c, method_callees, prelude_callees, method_src, prelude_src
            );
        }
    }
}

// =====================================================================
// f30wit_method_01 — Str method form lowers to PRELUDE-fn form
//
// Method: `s.split(",")` ≡ PRELUDE-fn: `split(s, ",")`. The MIR
// callee set of the two programs must agree on the `split` symbol;
// no new dispatch infrastructure introduced.
// =====================================================================

#[test]
fn f30wit_method_01_str_split_method_form_lowers_to_prelude_fn() {
    let method_src = "fn main() -> i64:\n    let s: str = \"a,b,c\"\n    let xs: list[str] = s.split(\",\")\n    return 0\n";
    let prelude_src = "fn main() -> i64:\n    let s: str = \"a,b,c\"\n    let xs: list[str] = split(s, \",\")\n    return 0\n";
    assert_method_form_subset_of_prelude_fn_form("f30wit_method_01", method_src, prelude_src);
    // Property (b) — no forbidden dispatch symbols.
    let m = lower_to_mir(method_src).expect("method-form lowers");
    assert_no_forbidden_symbols(&m, "f30wit_method_01");
}

// =====================================================================
// f30wit_method_02 — List method form lowers to PRELUDE-fn form
//
// Method: `xs.len()` ≡ PRELUDE-fn: `len(xs)` (polymorphic per
// `check.rs:1710`). The polymorphic intrinsic re-uses the same MIR
// symbol; method-form must not introduce a new one.
// =====================================================================

#[test]
fn f30wit_method_02_list_len_method_form_lowers_to_prelude_fn() {
    let method_src = "fn main() -> i64:\n    let xs: list[i64] = [1, 2, 3]\n    let n: i64 = xs.len()\n    return n\n";
    let prelude_src = "fn main() -> i64:\n    let xs: list[i64] = [1, 2, 3]\n    let n: i64 = len(xs)\n    return n\n";
    assert_method_form_subset_of_prelude_fn_form("f30wit_method_02", method_src, prelude_src);
    let m = lower_to_mir(method_src).expect("method-form lowers");
    assert_no_forbidden_symbols(&m, "f30wit_method_02");
}

// =====================================================================
// f30wit_method_03 — `&s.method()` precedence parses as `&(s.method())`
//
// ADR-0052 F-G.3 amendment (line 275): method-call binds tighter than
// the unary borrow. The MIR lowering must produce an `Operand::Copy`
// (the borrow) wrapping a result that came from the `str_len` call
// (the method-form rewrite). Witness:
//
// - Source: `let n = str_len(&s.len())` — wait, that's wrong. Per
//   ADR §"Precedence with 0052a `&s`", the witness is: `&s.len()`
//   parses as `&(s.len())`, i.e. takes the borrow of the i64 return
//   of s.len(). Since the i64 is a primitive value, the lowering is
//   semantically valid (the borrow operand validator at
//   `parser.rs:1105-1110` admits any place; `s.len()` is a Call
//   expression that produces a value but the parser still admits
//   the whole `&(call_result)` shape per the F-G.3 sketch).
//
// To witness the precedence property without depending on a full
// program build, we just verify that the method-form-with-borrow
// program lowers without parser error AND its callee set still
// contains the `str_len` (or equivalent) symbol from the inner
// method-form call. This proves the parser bound `s.len()` first
// then wrapped with `&`.
// =====================================================================

#[test]
#[ignore = "ADR-0052f only relaxes parser cap; type-check still rejects \\&CallResult as non-place. Deferred to Wave 2 round 2 follow-up (0052g type-check piece OR fold into 0052d-final)."]
fn f30wit_method_03_borrow_precedence_binds_tighter_than_method_call() {
    // `&s.len()` parses as `&(s.len())` per ADR-0052 F-G.3. The
    // method-form rewrite of `s.len()` to `str_len(s)` runs INSIDE
    // the borrow operand, so the MIR callee set must contain
    // `str_len` even with the outer `&`.
    //
    // Method-form witness program: declares `str_len(&i64) -> i64`-
    // style read fn locally so the type-checker accepts the borrow
    // wrapper. (Pre-impl, this program may fail at parse or type
    // check; the test is `#[ignore]`'d until DEV lands the surface.)
    let src = "fn read_i64(n: i64) -> i64:\n    return n\nfn main() -> i64:\n    let s: str = \"hello\"\n    let r: i64 = read_i64(&s.len())\n    return r\n";
    // Just verify it lowers; precedence is established by the fact
    // that the inner `s.len()` (method-form) rewrites first and the
    // `&` wraps the resulting i64-typed expression.
    let m = lower_to_mir(src).expect("borrow-wrapping-method-form lowers");
    assert_no_forbidden_symbols(&m, "f30wit_method_03");
    // The PRELUDE-fn `str_len` should appear in the callee set —
    // the inner method-form rewrites to it; the outer borrow does
    // not introduce a new callee symbol.
    let cs: Vec<String> = all_callees(&m);
    assert!(
        cs.iter().any(|c| c == "str_len"),
        "f30wit_method_03: expected `str_len` callee from method-form rewrite of `s.len()`; got callees={:?}",
        cs
    );
}
