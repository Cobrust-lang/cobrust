---
finding_id: cluster-a-letrebind-fieldborrow
last_verified_commit: 666ba8d
module_id: types/hir/frontend/mir
dependencies:
  - adr/0052a-explicit-borrow-let-rebind
  - adr/0051-llm-first-design-principle
status: open
---

# Cluster A let-rebind shortcut + `&p.<field>` field-projection borrow inventory

ADR-0052a §4.4 (let-rebind shortcut `let s = &s`) + §8 Wave-1 (`&p.<field>`
field-projection borrow) ship the **remaining gaps** in Wave-1's
parser+HIR+types+MIR scaffolding. Wave-1 v3 (commit `666ba8d`)
landed every other surface but deferred these two patterns; the CI
emergency triage marked the affected tests `#[ignore]` so the suite
stays green until DEV impl lands.

## Empirical `#[ignore]` cite inventory (HEAD `666ba8d`)

Total: **9 tests** carrying actual `#[ignore]` attrs citing ADR-0052a
§4.4 or §8 Wave-1. (The "16" figure from the dispatch brief includes
the `// Pre-DEV-impl status` cluster-marker comments + 6 ill-typed
i0052a_01..06 cases that are NOT actually `#[ignore]`'d — those run
against the Wave-1 surface today and assert `BorrowOfNonPlace` /
`UnknownName` / `ImplicitTruthiness` rejection paths. Spot-check
shows i0052a_01..06 pass under Wave-1 v3; they are the "30 well-typed
+ 6 ill-typed" Cluster A corpus baseline that already turned green.)

### (i) §4.4 let-rebind shortcut — 4 tests

| Test path | Surface |
|---|---|
| `crates/cobrust-cli/tests/borrow_phase_g_e2e.rs:260` `e0052a_e2e_08_synthetic_let_rebind_with_loop` | E2E: `let s = &s` + while-loop reads via rebind |
| `crates/cobrust-mir/tests/borrow_phase_g_f30_witness.rs:241` `f30wit_04_let_rebind_synthetic_no_clone_no_uaf` | MIR witness: rebind emits no `__cobrust_str_clone` + no `UseAfterMove` |
| `crates/cobrust-types/tests/well_typed.rs:2330` `w0052a_07_let_rebind_shortcut_basic` | Type check: `let s = input(""); let s = &s; let n = str_len(s)` |
| `crates/cobrust-types/tests/well_typed.rs:2341` `w0052a_08_let_rebind_then_multi_read` | Type check: rebind then 2 reads via the rebound binding |

**Root cause**: HIR `lower.rs:95` `DuplicateBinding` fires when the
pattern `Binding("s")` of the inner `let s = &s` calls `Scope::bind`
and finds the prior `s` in the same scope (`scope.rs:107..114`). Per
ADR-0052a §4.4 ("the new `s` shadows it inside the rebind's scope"),
let-statements MUST allow same-scope shadow. Constitution §2.5
"maximize-overlap-with-training-data": Rust permits `let` shadow at
the same scope unconditionally — Cobrust's blanket reject is the
deviation.

### (ii) §8 Wave-1 `&p.<field>` field-projection borrow — 5 tests

| Test path | Surface |
|---|---|
| `crates/cobrust-frontend/tests/borrow_phase_g_parse_corpus.rs:84` `bg0052a_p03_amp_field_access` | Parse: `&p.0` (parser pre-DEV emits `Expected RParen, found Float(.0)`) |
| `crates/cobrust-types/tests/well_typed.rs:2319` `w0052a_06_lc20_nested_str_eq_borrow` | Type check: nested-borrow path via `str_eq_lit(&c, "(")` (depends on `&c` of Str-typed local resolving cleanly through transparency rule) |
| `crates/cobrust-types/tests/well_typed.rs:2442` `w0052a_18_borrow_field_access` | Type check: `let n = str_len(&p.0)` where `p: (str, str)` |
| `crates/cobrust-types/tests/well_typed.rs:2458` `w0052a_19_borrow_field_access_then_arith` | Type check: `str_len(&p.0) + str_len(&p.1)` |
| `crates/cobrust-types/tests/well_typed.rs:2547` `w0052a_28_nested_borrow_in_tuple_constructor` | Type check: `let t = (str_len(&s), str_len(&s))` (nested in tuple ctor) |

**Root cause** (parser): Lexer pre-combines `.0` as `Float(".0")` when
the preceding byte is non-digit. The parser's `parse_postfix` Dot arm
calls `expect_ident()`, which never receives a Float token, so `p.0`
fails before the borrow validator runs. Per ADR-0052a §8 Wave-1, the
canonical tuple-field syntax is `.0/.1/...`; the parser must intercept
the `Float(.N)` post-Dot and treat as `Attribute { name: "N" }`.

**Root cause** (types): The `synth_expr → ExprKind::Attr` arm at
`check.rs:1219..1226` returns `fresh_var()` unconditionally — no
tuple-field resolution. Once the parser produces `Attr { base: p,
name: "0" }`, the type checker must resolve `p`'s type to `Ty::Tuple`
and return the element at index 0.

**Note on w0052a_06**: tagged as "nested-borrow path" but the source
is `str_eq_lit(&c, "(")` — actually a plain `Name`-shaped borrow that
hits a different blocker (likely `str_eq_lit` PRELUDE-stub being
applied non-transparently to `&Str`). Spot-check post-impl will
clarify whether this is a real §8 dep or a misclassification.

### (iii) Other — 0 tests

No Cluster A `#[ignore]`'d test is outside the (i)+(ii) split.

## Implementation strategy (chosen — see commits below)

1. **HIR let-shadow** — Add `bind_allow_shadow` variant or thread a
   `shadow_ok: bool` flag through `lower_pattern_with_bindings`.
   Restrict shadow to top-level `Binding` patterns in `Let` bodies
   (or-pattern + tuple-pattern same-name still rejects via
   non-shadow path).
2. **Parser `Float(.N)` interception** — In `parse_postfix`, after
   `Dot` `bump()`, peek for `Float(".N")` and accept as numeric
   `Attribute { name: "N" }`. Fall through to `expect_ident()` for
   non-numeric attrs.
3. **Type-check tuple-field via `Attr`** — In `synth_expr →
   ExprKind::Attr`, if `name` parses as `u32` and base resolves to
   `Ty::Tuple(items)`, return `items[idx].clone()` (with OOB
   `NotIndexable`-style error). Non-tuple bases retain `fresh_var()`
   fallback.
4. **MIR `&p.0` codegen** — already covered by `lower_borrow_inner`'s
   `_ => self.lower_expr(inner)` fall-through (the Attr lowering at
   `lower.rs:1412` emits `Operand::Copy(Place + Projection::Field(0))`
   placeholder). Whether the placeholder produces valgrind-clean
   binary for tuple-field reads is an open MIR debt; well_typed tests
   only invoke type-check.

## Expected post-impl results

- 8/9 unignores expected GREEN.
- w0052a_06 may need follow-up (str_eq_lit transparency interaction
  TBD); honest-defer if so per F37.
