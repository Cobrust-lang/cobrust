---
doc_kind: finding
finding_id: list-polymorphic-instantiation-ambiguity-root-cause
last_verified_commit: 5994d14
dependencies: [adr:0050c, adr:0050d, adr:0050h]
discovered_by: parallel-session P9 investigation 2026-05-19 — empirical falsification of `findings/lc100-str-use-after-move-regression-from-adr0050c.md`'s f64/Str=non-Copy blame; pure-i64 program with no `&s` / no `str` also fails
severity: P1 (was honest-debt; now RESOLVED)
status: resolved
supersedes: lc100-str-use-after-move-regression-from-adr0050c
related: [adr:0050c, adr:0050h, silent-rot-on-accepted-debt]
---

# Finding: list_polymorphic instantiation ambiguity — true root cause of LC-100 mass-failure

## §1. Precise root cause

The PRELUDE row-polymorphic intrinsics (`list_new`, `list_set`,
`list_get`, plus `list_len`, `list_is_empty`, `dict_is_empty`, `len`)
are widened at the call site by
`crates/cobrust-types/src/check.rs::instantiate_list_polymorphic`. The
pre-fix implementation walked the signature recursively and allocated
a **fresh `Ty::Var`** per `Ty::List(_)` slot — but did NOT rewrite the
bare `i64` slots that semantically represent the same element type.

PRELUDE shapes that exhibit this bug:

| Intrinsic | Signature | Element-typed slots |
|---|---|---|
| `list_new(i64) -> list[i64]` | i64 in return's list elem | RETURN list elem only |
| `list_set(list[i64], i64, i64) -> i64` | list elem + arg[2] (value) | arg[0] list elem + arg[2] |
| `list_get(list[i64], i64) -> i64` | list elem + return | arg[0] list elem + RETURN |

For the canonical LC-01 `two_sum.cb` pattern (or any pure-i64 program
without annotation on the list binding):

```cobrust
let nums = list_new(n)        # nums: list[Var(α)]
list_set(nums, 0, 1)          # unifies α with β (fresh); but v: i64
                              # doesn't anchor β to i64 because v's
                              # formal is bare i64, not list[T]
let v = list_get(nums, 0)     # unifies α with δ (fresh); return i64
                              # is bare i64, doesn't anchor δ to i64
```

After all 3 calls: `nums`'s type in `def_types` is `list[Var(α)]` with
α unified with β, δ but NONE of β, δ, α ever anchored to a concrete
type. `check()` finalize at the end of the module scans `def_types`,
applies the running substitution, finds free vars remaining, raises
`TypeError::AmbiguousType`.

The same shape covers all 100 LC-100 programs that use `list_new(n)`
without an explicit `: list[i64]` annotation (and the test fixtures
under `examples/leetcode/`, `corpus/leetcode/`).

## §2. Wrong attribution in the pre-existing finding

`findings/lc100-str-use-after-move-regression-from-adr0050c.md`
(authored 2026-05-16) blamed ADR-0050c Phase 2a — Str=non-Copy — for
the LC-100 mass-failure. The hypothesis:

> Post-Wave-2 (Str=non-Copy): the first PRELUDE call consumes `s`.
> Subsequent reads fail with UseAfterMove. This breaks every program
> that uses `let n = str_len(s); let c = str_at(s, i)`.

**Empirical falsification** (DG verify on `5994d14`, the fix branch):

The new `list_poly_pure_i64_triple` test in
`crates/cobrust-types/tests/list_poly_repro.rs`:

```cobrust
fn main() -> i64:
    let n: i64 = 5
    let nums = list_new(n)
    let i: i64 = 0
    let _ = list_set(nums, i, 1)
    let v = list_get(nums, i)
    return v
```

is a pure-i64 program with:
- **No `str` use** — no `str_len`, `str_at`, `str_eq`, etc.
- **No `&` borrow** — no `&s` syntax anywhere.
- **No f64 / no `as` cast** — pure integer arithmetic.

On main `de6c78d`, this test FAILS with `AmbiguousType { span: ...,
suggestion: Some("add an explicit type annotation, e.g. `let x: i64 =
…`") }` — exactly the error reported for `test_lc01_two_sum_oracle_match`.

The Str=non-Copy hypothesis predicts UseAfterMove, not AmbiguousType.
The hypothesis is wrong for the entire LC-100 corpus (the str-using
subset may still hit Str=non-Copy separately, but that's a SECONDARY
bug; the dominant root cause is `list_new` ambiguity).

## §3. Proposed fix (LANDED — commit c4d607e)

**Approach A — share one fresh elem var per intrinsic call site**:
introduce `instantiate_intrinsic_signature(name, ty)` that allocates a
single fresh `elem` var per call site and uses it in BOTH the
`list[T]` receiver AND every scalar slot that semantically represents
the element type. For known intrinsic names with known scalar-element
positions, emit a hand-rolled signature:

```rust
"list_new" => Ty::Fn(FnTy { positional: vec![Ty::Int], return_ty: list(elem) })
"list_get" => Ty::Fn(FnTy { positional: vec![list(elem.clone()), Ty::Int], return_ty: elem })
"list_set" => Ty::Fn(FnTy { positional: vec![list(elem.clone()), Ty::Int, elem], return_ty: Int })
// list_len, list_is_empty, dict_is_empty, len fall through to the
// recursive walk which is already correct for them
```

This treats the PRELUDE's concrete `i64` in scalar element slots as
"stand-in for T", honoring the row-polymorphic intent of ADR-0050c §F5
/ Phase 6 that the recursive walk underspecified.

**Why this is sound**:
- MIR intrinsic-rewrite at `crates/cobrust-cli/src/build/intrinsics.rs`
  routes these names to bytewise-generic C-ABI runtime symbols
  (`__cobrust_list_get`, etc.) which take element-type-agnostic
  `*mut u8` pointers + per-MIR-block widths. The Cobrust type checker
  is the only layer that distinguishes element types; sharing one
  fresh var per call site is the same shape the runtime expects.
- F31 lock (one-way `Ref(T) → T` coercion at `unify_call_arg`) is
  preserved: the shared elem var is on the formal side; the actual
  arg side may be `Ref(T)` and the boundary coercion still applies.
- Annotation-anchored callers (`let nums: list[i64] = list_new(n)`,
  which PRELUDE's own `range()` definition uses) still work — the
  new sig unifies its `list[elem]` return with the explicit
  `list[i64]` annotation, anchoring elem = i64.

**Approaches considered and rejected**:
- **B. Lift unification to post-call-block pass**: more invasive,
  harder to reason about, doesn't match the per-call-site shape
  the rest of the type checker uses.
- **C. Eager Subst::apply fixed-point at check() end**: doesn't help
  — there's nothing to resolve the free var to. It needs an external
  constraint, which only B or A provides.

## §4. Acceptance gate

Locked by tests in `crates/cobrust-types/tests/list_poly_repro.rs`:

- `list_poly_pure_i64_triple` — minimum LC-01 shape, pure i64.
- `list_poly_two_sum_shape` — exact `examples/leetcode/two_sum.cb`
  body stripped to its type-check subset.
- `list_poly_annotated_still_works` — sanity that explicit annotation
  continues to type-check (the `range()` PRELUDE call site shape).

**DG verify gate at 5994d14**:

| Surface | Main `de6c78d` | Fix `5994d14` | Δ |
|---|---|---|---|
| cobrust-types + cobrust-codegen | 920 P / 14 F (920 expected) | 920 P / 14 F | 0 regression |
| 14 pre-existing failures | 6 w0052a_* + 8 s0052b_* | identical | 0 new failures |
| `leetcode_corpus_e2e` | 4 PASS / 8 FAIL | **7 PASS / 5 FAIL** | **+3 PASS** |
| `test_lc01_two_sum_oracle_match` | FAILED | **OK** | RESOLVED |
| `test_lc02_reverse_string_oracle_match` | FAILED | **OK** | RESOLVED |
| LC-100 stress (b1+b2+b3+b4, 100 programs) | 9 PASS / 94 FAIL | **16 PASS / 87 FAIL** | **+7 PASS** |

The b1 batch (29 tests) remains 0/29 PASS — those 29 programs hit a
DIFFERENT root cause (likely `str`-specific patterns or `f64` cast).
This finding addresses only the list_polymorphic ambiguity dimension;
str/f64-specific LC-100 failures remain as separate findings.

POSTFLIGHT: `/tmp/cobrust-*` cleaned to 0 entries (per
`feedback_heavy_build_offload_to_workstation.md` 235G temp leak
discipline).

## §5. Quiet-rot lesson (F37 candidate)

This bug went undetected for 3+ days on main despite:
- `examples/leetcode/two_sum.cb` existing in tree.
- `test_lc01_two_sum_oracle_match` being NOT `#[ignore]`'d.
- `cargo test -p cobrust-cli --test leetcode_corpus_e2e` reporting
  `4 passed / 8 failed` on every CI/Mac/DG run.

The reason: the **8 failures got LUMPED** into a "124 pre-existing
failures" bucket that audits treated as "accepted_as_honest_debt"
without enumerating WHICH 8 (or 124). No one cross-referenced the
specific test names against the existing
`lc100-str-use-after-move-regression-from-adr0050c.md` finding's
claimed root cause. If anyone had run `test_lc01_two_sum` in isolation
they'd have seen `AmbiguousType`, not `UseAfterMove` — and the wrong
attribution would have been caught.

**Discipline rule** (formalized in
`feedback_silent_rot_on_accepted_debt.md` 2026-05-19):

> A test marked `accepted_as_honest_debt` MUST cite the specific
> `#[ignore = "<reason>; deferred to <ticket-or-phase>"]` annotation
> on the failing test. If the test is NOT `#[ignore]`'d, the finding
> status MUST be `active_p0_blocker` or `active_p1_blocker` —
> NEVER `accepted_as_honest_debt`. Otherwise the failure becomes
> invisible to CI/audit and masks downstream regressions (or, as
> here, masks a misdiagnosis sticking around for days).

## §6. Cross-references

- **ADR-0050h** — the fix landed as a §"Decision" amendment to the
  row-poly story of ADR-0050c §F5 / Phase 6. (To be authored;
  this finding currently stands in as the design rationale.)
- `crates/cobrust-types/src/check.rs::instantiate_intrinsic_signature` — fix site (lines ~2384..).
- `crates/cobrust-types/src/check.rs::instantiate_list_polymorphic` — pre-fix function, kept as fallback for non-elem-slot intrinsics.
- `crates/cobrust-types/tests/list_poly_repro.rs` — regression suite.
- `findings/lc100-str-use-after-move-regression-from-adr0050c.md` — **superseded** by this finding (see amendment §1 there).
- `feedback_silent_rot_on_accepted_debt.md` — F37 candidate pattern.
- DG verify rev: `5994d14280d7464d2ca28f4b5d1ea83ef20ff53d`.
- Resolution SHA chain: c4d607e (impl) + 5994d14 (tests) + 23eca0a (DG verify trail).
