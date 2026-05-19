//! Regression test for the `list_polymorphic` AmbiguousType root-cause
//! fix (see `docs/agent/findings/list-polymorphic-instantiation-ambiguity-root-cause.md`).
//!
//! # Pre-fix bug
//!
//! The PRELUDE `list_new + list_set + list_get` triple, when called
//! with no element-type annotation on the binding (`let nums =
//! list_new(n)`), failed `check()` with `AmbiguousType` because every
//! call to `instantiate_list_polymorphic` allocated an independent
//! fresh `Ty::Var` per `Ty::List` slot — so the value-slot `i64` in
//! `list_set` and the return `i64` in `list_get` never propagated
//! back to the list-element type. The `nums` binding ended up with
//! `list[Var(α)]` where α was never anchored to a concrete type.
//!
//! # Fix
//!
//! `instantiate_intrinsic_signature` synthesises per-intrinsic
//! signatures that share one fresh element-type var across all
//! element-typed slots within a single call site (the `list[T]`
//! receiver + scalar value/return slots).
//!
//! # Affected corpus
//!
//! This pattern was the latent root cause for the 100-program LC-100
//! corpus mass-failure that ADR-0050c was previously blamed for. See
//! the finding for the empirical-falsification evidence (pure-i64
//! programs with no `&s` / no `str` also fail under the pre-fix
//! `instantiate_list_polymorphic`).
#![allow(clippy::missing_panics_doc)]

use cobrust_frontend::{parse_str, span::FileId};
use cobrust_hir::{Session, lower};
use cobrust_types::check;

const LIST_STUBS: &str = concat!(
    "fn list_new(capacity: i64) -> list[i64]:\n    let xs: list[i64] = []\n    return xs\n",
    "fn list_set(lst: list[i64], i: i64, v: i64) -> i64:\n    return 0\n",
    "fn list_get(lst: list[i64], i: i64) -> i64:\n    return 0\n",
);

fn must_accept(name: &str, src: &str) {
    let module = parse_str(src, FileId::SYNTHETIC)
        .unwrap_or_else(|e| panic!("{name}: parse error: {e:?}\nsource:\n{src}"));
    let mut sess = Session::new();
    let hir = lower(&module, &mut sess)
        .unwrap_or_else(|e| panic!("{name}: lowering error: {e:?}\nsource:\n{src}"));
    check(&hir)
        .unwrap_or_else(|e| panic!("{name}: should accept but rejected: {e:?}\nsource:\n{src}"));
}

#[test]
fn list_poly_pure_i64_triple() {
    // `let nums = list_new(n)` without annotation, then list_set + list_get.
    // No `&` borrow, no str, no f64 — pure i64. This is the LC-01 two_sum
    // shape stripped to its minimum.
    let src = format!(
        "{LIST_STUBS}fn main() -> i64:\n    let n: i64 = 5\n    let nums = list_new(n)\n    let i: i64 = 0\n    let _ = list_set(nums, i, 1)\n    let v = list_get(nums, i)\n    return v\n"
    );
    must_accept("list_poly_pure_i64_triple", &src);
}

#[test]
fn list_poly_two_sum_shape() {
    // Exact `examples/leetcode/two_sum.cb` minus IO PRELUDE imports.
    // Verifies the pattern that broke 100 LC programs.
    let src = format!(
        "{LIST_STUBS}fn main() -> i64:\n    let n: i64 = 5\n    let nums = list_new(n)\n    let i: i64 = 0\n    while i < n:\n        let _ = list_set(nums, i, i)\n        i = i + 1\n    let target: i64 = 7\n    let a: i64 = 0\n    while a < n:\n        let b: i64 = a + 1\n        while b < n:\n            if list_get(nums, a) + list_get(nums, b) == target:\n                return 0\n            b = b + 1\n        a = a + 1\n    return 0\n"
    );
    must_accept("list_poly_two_sum_shape", &src);
}

#[test]
fn list_poly_annotated_still_works() {
    // Sanity: explicit `: list[i64]` annotation should still type-check.
    // This is the pre-fix workaround that callers used to anchor α=i64.
    let src = format!(
        "{LIST_STUBS}fn main() -> i64:\n    let n: i64 = 5\n    let nums: list[i64] = list_new(n)\n    let _ = list_set(nums, 0, 1)\n    let v = list_get(nums, 0)\n    return v\n"
    );
    must_accept("list_poly_annotated_still_works", &src);
}
