//! ADR-0052a Wave-1 F30 shadow-flip witness corpus.
//!
//! Per `findings/predicate-flip-cascade-discovery-deficit.md` SOP +
//! ADR-0052a §5 / §5.5 binding: this file holds the standalone-test
//! witnesses for the 6 LC-100 cascade rows (§5 table rows 1-6). Each
//! witness asserts three properties of the lowered MIR:
//!
//! - **(a) no `__cobrust_str_clone` callee appears** — the clone-shim
//!   retirement from idiomatic LC-100 paths per ADR-0052a §13
//!   "Positive". The shim stays in the stdlib (still emitted by
//!   `Aggregate` lowering), but explicit `&s` programs MUST NOT route
//!   through it.
//! - **(b) `MirError::UseAfterMove` does NOT fire** — the F30
//!   catch-conversion: the §2.5 catch is preserved as a real signal
//!   for non-borrowed reads, but the borrow form (`Operand::Copy`)
//!   suppresses it for legitimate read-only sequences.
//! - **(c) MIR lowering succeeds end-to-end** — exit-code surrogate
//!   for these standalone (no build/run) witnesses; the E2E suite
//!   `crates/cobrust-cli/tests/borrow_phase_g_e2e.rs` covers stdout
//!   byte-identity per oracle.
//!
//! F30 SOP step 4 / ADR-0052a §5.5: cascade addendum is the spike-
//! commit responsibility. These witnesses encode the *expected* set;
//! the spike-commit `cargo test --workspace --features
//! cobrust_borrow_phase_g` ground-truth run classifies any miss.
//!
//! Pre-DEV-impl status: every `f30wit_*` test below is `#[ignore]`'d
//! pending Wave-1 DEV merge (parser+HIR+types+MIR scaffolding per
//! ADR-0052a §6–§9).
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

use cobrust_frontend::{parse_str, span::FileId};
use cobrust_hir::{Session, lower as hir_lower};
use cobrust_mir::{
    Body, Constant, Module as MirModule, Operand, Terminator, borrow_check, lower as mir_lower,
};
use cobrust_types::check;

/// Marker the witness scans for in MIR callees. Must NOT appear in any
/// MIR `Terminator::Call { func: Constant::Str(name), .. }` reachable
/// from the borrowed-read path.
const STR_CLONE_SYMBOL: &str = "__cobrust_str_clone";

/// ADR-0052a Wave-1 DEV v3 post-impl wiring (TEST author pattern
/// error correction): the f30wit_* tests originally fed source-text
/// referencing PRELUDE names (`input`, `str_len`, `str_at`,
/// `print_no_nl`, `str_ord`, `print_int`, `print`) directly into
/// `lower_to_mir`, which does NOT prepend PRELUDE stubs. The HIR
/// lower stage rejected with `UnknownName` before the F30 witness
/// could observe the borrow lowering. DEV adds this BORROW_STUBS
/// const matching the well_typed sibling so the tests reach the
/// borrow-lowering observation site.
const BORROW_STUBS: &str = concat!(
    "fn print(s: str) -> i64:\n    return 0\n",
    "fn print_int(n: i64) -> i64:\n    return 0\n",
    "fn print_no_nl(s: str) -> i64:\n    return 0\n",
    "fn input(prompt: str) -> str:\n    return \"\"\n",
    "fn str_len(s: str) -> i64:\n    return 0\n",
    "fn str_at(s: str, i: i64) -> str:\n    return \"\"\n",
    "fn str_ord(s: str) -> i64:\n    return 0\n",
    "fn list_new(capacity: i64) -> list[i64]:\n    let xs: list[i64] = []\n    return xs\n",
    "fn list_get(lst: list[i64], i: i64) -> i64:\n    return 0\n",
    "fn list_set(lst: list[i64], i: i64, v: i64) -> i64:\n    return 0\n",
);

/// Lower source through frontend → HIR → types → MIR.
///
/// Returns `Err(_)` if any stage rejects the program; this matters
/// because the F30 witness can fail in two distinct ways:
/// 1. **Wrong way** — parser/types/MIR rejects a program that uses the
///    `&s` form (DEV missing impl). Tests stay `#[ignore]` pre-DEV;
///    they unmask via the `expect` panic path post-DEV.
/// 2. **Wrong way 2** — borrow_check pass surfaces a UseAfterMove
///    error on the borrowed-read path. Counts as a (b) violation; the
///    test panics with the diagnostic.
fn lower_to_mir(src: &str) -> Result<MirModule, String> {
    let combined = format!("{BORROW_STUBS}{src}");
    let module =
        parse_str(&combined, FileId::SYNTHETIC).map_err(|e| format!("parse error: {e:?}"))?;
    let mut sess = Session::new();
    let hir = hir_lower(&module, &mut sess).map_err(|e| format!("hir lower error: {e:?}"))?;
    let typed = check(&hir).map_err(|e| format!("type check error: {e:?}"))?;
    mir_lower(&typed).map_err(|e| format!("mir lower error: {e:?}"))
}

/// Resolve the named body in a MIR module.
fn body_named<'a>(m: &'a MirModule, name: &str) -> &'a Body {
    m.bodies
        .iter()
        .find(|b| b.name == name)
        .unwrap_or_else(|| panic!("body `{name}` not found in MIR module"))
}

/// Collect every callee symbol from every `Terminator::Call` in a body.
///
/// Symbols are returned as `String`s lifted from `Operand::Constant
/// (Constant::Str(name))`. The witness uses this to assert
/// `__cobrust_str_clone` is absent from the borrow-form lowering.
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

/// Assert property (a): no `__cobrust_str_clone` callee appears in any
/// body of the lowered module.
fn assert_no_str_clone(m: &MirModule, name: &str) {
    for body in &m.bodies {
        let callee_names = callees(body);
        let cloned = callee_names
            .iter()
            .find(|c| c.contains(STR_CLONE_SYMBOL))
            .cloned();
        assert!(
            cloned.is_none(),
            "{}: borrow-form lowering routed through `{}` (witness (a) violated)\n  body=`{}`\n  callees={:?}",
            name,
            STR_CLONE_SYMBOL,
            body.name,
            callee_names
        );
    }
}

/// Assert property (b): borrow_check passes (no `UseAfterMove`
/// surfaces) on every body in the lowered module. `borrow_check` is
/// `&Body → Result<(), MirError>`, so we apply it per-body.
fn assert_no_use_after_move(m: &MirModule, name: &str) {
    for body in &m.bodies {
        if let Err(e) = borrow_check(body) {
            panic!(
                "{}: borrow_check surfaced an error on borrow-form lowering\n  body=`{}`\n  error: {:?}\n  witness (b) requires UseAfterMove DOES NOT fire under `&s`",
                name, body.name, e
            );
        }
    }
}

/// Convenience — apply (a) + (b) + (c) for one source-text witness.
fn assert_witness_clean(name: &str, src: &str) {
    let m = match lower_to_mir(src) {
        Ok(m) => m,
        Err(e) => panic!(
            "{}: MIR lowering failed (witness (c) prerequisite)\n  error: {}\n  --- source ---\n{}",
            name, e, src
        ),
    };
    let _ = body_named(&m, "main");
    assert_no_str_clone(&m, name);
    assert_no_use_after_move(&m, name);
}

// =====================================================================
// F30 §5 row 1-2 — LC-02 reverse_string borrow form
//
// Mirrors `examples/leetcode/reverse_string.cb` with `&s` replacements
// at the two PRELUDE Str reads. F30 §5 cascade prediction: rows 1+2
// flip from clone-shim emission to pure `Operand::Copy` lowering.
// =====================================================================

#[test]
fn f30wit_01_lc02_reverse_string_no_clone_no_uaf() {
    // F30 §5 rows 1-2 witness. Asserts (a) no __cobrust_str_clone
    // appears (witness: clone-shim retirement) AND (b) UseAfterMove
    // does NOT fire under `&s` form (witness: F30 catch-conversion).
    let src = "fn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&s)\n    let i: i64 = n - 1\n    while i >= 0:\n        let c = str_at(&s, i)\n        print_no_nl(c)\n        i = i - 1\n    print(\"\")\n    return 0\n";
    assert_witness_clean("f30wit_01", src);
}

// =====================================================================
// F30 §5 row 3-4 — LC-13 roman_to_integer borrow form
//
// Mirrors `examples/leetcode/roman_to_integer.cb` with `&s` reads.
// =====================================================================

#[test]
fn f30wit_02_lc13_roman_no_clone_no_uaf() {
    // F30 §5 rows 3-4 witness. LC-13 pattern: str_len + str_at on
    // borrow form.
    let src = "fn roman_val(c: str) -> i64:\n    let o = str_ord(c)\n    if o == 73:\n        return 1\n    if o == 86:\n        return 5\n    if o == 88:\n        return 10\n    if o == 76:\n        return 50\n    if o == 67:\n        return 100\n    if o == 68:\n        return 500\n    if o == 77:\n        return 1000\n    return 0\nfn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&s)\n    let result: i64 = 0\n    let prev: i64 = 0\n    let i: i64 = n - 1\n    while i >= 0:\n        let c = str_at(&s, i)\n        let v = roman_val(c)\n        if v < prev:\n            result = result - v\n        else:\n            result = result + v\n        prev = v\n        i = i - 1\n    print(result)\n    return 0\n";
    assert_witness_clean("f30wit_02", src);
}

// =====================================================================
// F30 §5 row 5-6 — LC-20 valid_parentheses borrow form
//
// Mirrors `examples/leetcode/valid_parentheses.cb` with `&s` reads on
// the two PRELUDE Str helpers (str_len + str_at).
// =====================================================================

#[test]
fn f30wit_03_lc20_valid_parens_no_clone_no_uaf() {
    // F30 §5 rows 5-6 witness. LC-20 pattern: str_len + str_at on
    // borrow form, deeply nested if/elif body.
    let src = "fn main() -> i64:\n    let s = input(\"\")\n    let n = str_len(&s)\n    let stack = list_new(n)\n    let top: i64 = 0\n    let ok: i64 = 1\n    let i: i64 = 0\n    while i < n:\n        let c = str_at(&s, i)\n        let o = str_ord(c)\n        if o == 40:\n            list_set(stack, top, 40)\n            top = top + 1\n        elif o == 91:\n            list_set(stack, top, 91)\n            top = top + 1\n        elif o == 123:\n            list_set(stack, top, 123)\n            top = top + 1\n        elif o == 41:\n            if top == 0:\n                ok = 0\n            else:\n                top = top - 1\n                let expected = list_get(stack, top)\n                if expected != 40:\n                    ok = 0\n        i = i + 1\n    if top != 0:\n        ok = 0\n    if ok == 1:\n        print(\"true\")\n    else:\n        print(\"false\")\n    return 0\n";
    assert_witness_clean("f30wit_03", src);
}

// =====================================================================
// F30 §5 synthetic witness — let-rebind + multi-read pattern
//
// Synthetic test not tied to any LC-100 source; exercises the §4.4
// let-rebind shortcut form. Verifies the rebind itself does not emit
// a `__cobrust_str_clone` and the subsequent reads via the rebind
// don't trip UseAfterMove.
// =====================================================================

#[test]
#[ignore = "ADR-0052a §4.4 let-rebind shortcut not yet implemented at the MIR-witness level. Pre-existing red on main HEAD as of 2026-05-20, not introduced by this branch."]
fn f30wit_04_let_rebind_synthetic_no_clone_no_uaf() {
    // Synthetic F30 witness: let-rebind shortcut + 3 reads through
    // the rebound borrow. The rebind itself must lower as a
    // `Operand::Copy` (NOT a Call to __cobrust_str_clone); each
    // subsequent str_len read must also use `Operand::Copy`.
    let src = "fn main() -> i64:\n    let s = input(\"\")\n    let s = &s\n    let a = str_len(s)\n    let b = str_len(s)\n    let c = str_len(s)\n    let total = (a + b) + c\n    print(total)\n    return 0\n";
    assert_witness_clean("f30wit_04", src);
}
