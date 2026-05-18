---
doc_kind: adr
adr_id: 0052b
parent_adr: 0052
title: "Direction B — Error UX rewrite (errors print the FIX)"
status: accepted
date: 2026-05-17
last_verified_commit: 2a710d3
supersedes: []
superseded_by: []
relates_to: [adr:0052, adr:0052a, adr:0051]
discovered_by: ADR-0052 Phase G frame ADR §"Direction B scaffolding anchors" + CLAUDE.md §2.5 Direction B
ratification_path: P9 Wave-2 sub-ADR review (per ADR-0052 §"Sub-ADR prerequisites"); ratifies on impl merge
---

# ADR-0052b: Direction B — Error UX rewrite (errors print the FIX)

## 1. Context

Phase G Wave 2 per ADR-0052 §"Wave 2 — Directions B + C + D parallel". CLAUDE.md §2.5 Direction B binds:

> Today: `TypeError::ImplicitTruthiness { actual: Int, span }`.
> Tomorrow: same + `suggestion: "change to 'if x != 0:'"`. LLM consumes stderr to decide next step.

The rationale is the §2.5 LLM-first compile-time-catch rule: every `TypeError::*` and `MirError::*` variant is already a "successful catch". The catch only converts to a useful corrective signal if the diagnostic also carries the **fix path** in machine-consumable form. Today's renderer at `crates/cobrust-cli/src/error_ux.rs:563-893` produces English suggestion prose via a hard-coded match per variant; the suggestion lives at *render* time, not at *construction* time, and the structured shape (`Option<&'static str>`) is not visible to downstream LSP / JSON / agent consumers.

Wave 1 (ADR-0052a §6) shipped the forward-compat field shape on the new variant `TypeError::BorrowOfNonPlace { span, suggestion: Option<&'static str> }` at `error.rs::TypeError::BorrowOfNonPlace` (currently `error.rs:219`), and `MirError::UseAfterMove` already ships a hard-coded `&s` suggestion at *render* time at `error_ux.rs:907` per ADR-0052a §7. Direction B generalises this pattern across all ~35 variants: every error gains a `suggestion: Option<&'static str>` field at construction; the renderer becomes a thin structural pass-through.

Empirical baseline at HEAD `8dc2723`: 24 `TypeError::*` variants (`crates/cobrust-types/src/error.rs`), 11 `MirError::*` variants (`crates/cobrust-mir/src/error.rs`). Of these, 19 already have hard-coded suggestion prose in the renderer; 16 have `None` hint today. Direction B shifts all 35 to a uniform `suggestion: Option<&'static str>` field consumed by the renderer.

## 2. Decision

Every variant of `TypeError` and `MirError` gains a `suggestion: Option<&'static str>` field. Suggestions are written at **construction time** (next to the place that decides the diagnostic), not at **render time**. The renderer at `error_ux.rs` reads `suggestion` directly and falls back to `None` rather than hard-coding prose per variant.

Three binding properties:

- **Construction-time write**. Each `Err(TypeError::Foo { ... })` site populates `suggestion` with the most actionable fix string available at that call site. Sites where no useful fix exists pass `suggestion: None`. The choice is local, not deferred to a global match.
- **Static `&'static str`**. Wave-2 suggestions are compile-time literals only. Dynamic format-arg interpolation (e.g. `format!("try `{name}.is_some()`")`) is **out of scope** (§11). This preserves zero-allocation rendering and matches the precedent Wave-1 set for `BorrowOfNonPlace::suggestion`.
- **Renderer becomes structural**. `error_ux.rs:563-893` (TypeError) and `error_ux.rs:894-1000` (MirError) lose the per-variant hard-coded hint prose; the variants emit their `suggestion` field unchanged.

Rejected alternatives:

- **Keep render-time hints, JSON-serialize at render boundary**: violates §2.5 because the suggestion remains in the renderer's match, not at the catch site. The LLM agent consuming structured errors (LSP / `--emit-json` / future agent loop) must round-trip through stringification.
- **Dynamic `String` suggestions**: doubles allocation cost on every diagnostic and complicates the cache-friendly `&'static str` baseline. Wave-2 capacity does not justify it; future micro-ADR can lift.
- **`Vec<Suggestion>` (multiple fix paths)**: real-world variants usually have one canonical fix; multi-fix is YAGNI per §11.

## 3. Surface examples (paired today/tomorrow)

### 3.1 `TypeError::ImplicitTruthiness` (canonical §2.5 case)

Today:
```
error: non-bool used in truthiness position: got `Int` at line 12
hint: Cobrust requires an explicit bool — try `if x != 0:` or `if x.is_some():`
```

Tomorrow:
```
error: non-bool used in truthiness position: got `Int` at line 12
suggestion: change to `if x != 0:` (use `.is_some()` for Option)
```

Renderer reads `suggestion` directly; the hint prose lives at the `Err(TypeError::ImplicitTruthiness { actual, span, suggestion: Some("...") })` construction site in `check.rs::TypeError::ImplicitTruthiness` (currently `check.rs:2076`).

### 3.2 `TypeError::TypeMismatch`

Today: `add a type annotation or fix the expression type` (generic).
Tomorrow: `change the expression type or add `: <expected>` annotation` (still generic — same site is reused in 8+ places).

### 3.3 `MirError::UseAfterMove` (ADR-0052a precedent)

Today (hard-coded at `error_ux.rs:907`): `change to \`&s\` to borrow without consuming (ADR-0052a explicit shared borrow)`.
Tomorrow: same text, but lifted to construction site in `borrow.rs:114` + `borrow.rs:224`.

### 3.4 `TypeError::AmbiguousType`

Today: `add an explicit type annotation, e.g. \`let x: i64 = …\``.
Tomorrow: same, lifted to construction site in `check.rs:59`.

### 3.5 `TypeError::UnknownName`

Today: `did you mean to declare it with \`let {name} = …\`?` (dynamic format).
Tomorrow: static text `declare with \`let <name> = …\` first` — drops the dynamic `{name}` interpolation per §11 static-only rule.

### 3.6 `TypeError::MutableDefault`

Today: `use \`None\` as the default and assign inside the function body`.
Tomorrow: same text, lifted to construction site in `check.rs:302` + `check.rs:316`.

### 3.7 `MirError::DropMissing`

Today: `every owned value must be explicitly dropped or returned`.
Tomorrow: `add \`drop(<local>)\` before return or transfer ownership` — sharper actionable fix.

### 3.8 `TypeError::NotHashable`

Today: `f64 keys are forbidden (NaN != NaN); use i64 via \`f.to_bits() as i64\` or a str repr`.
Tomorrow: same text, lifted to `check.rs:783` + `check.rs:1811` + `check.rs:1879`.

## 4. Variant enumeration table

All 35 variants at HEAD `8dc2723`. **Class**: `S` = static suggestion ready; `C` = context-dependent (multiple construction sites pick different suggestions); `N` = no useful suggestion (compiler-internal or self-explanatory).

### 4.1 TypeError (24 variants)

| Variant | Class | Proposed `suggestion` |
|---|---|---|
| `UnknownName` | S | `declare with \`let <name> = …\` first` |
| `ArityMismatch` | S | `check the function signature; pass exactly the declared positional arity` |
| `KeywordArgMismatch` | S | `remove or rename — the callee does not accept this keyword` |
| `MissingArgument` | S | `add the missing argument at the call site` |
| `TypeMismatch` | S | `change the expression type or add `: <expected>` annotation` |
| `NonExhaustiveMatch` | S | `add the missing cases or a wildcard \`_ \` arm` |
| `RowConflict` | N | (no static fix — depends on intent) |
| `ImplicitTruthiness` | S | `change to \`if x != 0:\` (use \`.is_some()\` for Option)` |
| `UseOfDroppedFeature` | S | `this Python feature is not part of Cobrust — see the language reference` |
| `MutableDefault` | S | `use \`None\` as the default and assign inside the function body` |
| `AmbiguousType` | S | `add an explicit type annotation, e.g. \`let x: i64 = …\`` |
| `DuplicateField` | S | `remove the duplicate field; record literals require unique names` |
| `OccursCheck` | S | `add a type annotation — recursive types must be explicit` |
| `NotCallable` | S | `only function types are callable; verify the name resolves to a fn` |
| `NotIndexable` | S | `use a list / dict / tuple / str — primitive types cannot be indexed` |
| `NotIterable` | S | `use a list / dict / range / str — primitives cannot iterate` |
| `BreakOutsideLoop` | S | `move the \`break\` inside a \`for\` or \`while\` loop body` |
| `ContinueOutsideLoop` | S | `move the \`continue\` inside a \`for\` or \`while\` loop body` |
| `ReturnOutsideFn` | S | `move the \`return\` inside a \`fn\` body` |
| `YieldOutsideFn` | S | `move the \`yield\` inside a generator \`fn\` body` |
| `NotHashable` | S | `f64 keys are forbidden (NaN != NaN); use i64 via \`f.to_bits() as i64\` or a str repr` |
| `DictSpreadNotSupported` | S | `dict-merge is Phase G; build the result manually by iterating \`other.items()\` and inserting` |
| `Multiple` | N | (aggregate container — renderer delegates to first child) |
| `BorrowOfNonPlace` | S | `borrow operand must be \`Name\`, \`Name.field\`, or \`Name[idx]\`` (already shipped per ADR-0052a §6) |

### 4.2 MirError (11 variants)

| Variant | Class | Proposed `suggestion` |
|---|---|---|
| `UseAfterMove` | S | `change to \`&s\` to borrow without consuming (ADR-0052a explicit shared borrow)` |
| `UseAfterDrop` | S | `the value was already dropped; reorder code so the read precedes the drop` |
| `ConflictingMutBorrow` | S | `only one mutable borrow can be active at a time; release the first borrow first` |
| `SharedMutOverlap` | S | `cannot borrow mutably while a shared borrow is active; release shared first` |
| `EscapingBorrow` | S | `the borrowed value must live at least as long as the reference` |
| `DropMissing` | S | `add \`drop(<local>)\` before return or transfer ownership` |
| `DoubleDrop` | S | `a value can only be dropped once; check your control flow` |
| `FieldOutOfBounds` | N | (compiler-internal — type checker should have caught) |
| `UnresolvedDefId` | N | (compiler-internal — never user-visible per `error_ux.rs:852`) |
| `NonExhaustiveSwitch` | S | `add a wildcard \`_\` arm or cover all cases` |
| `Internal` | N | (compiler bug; renderer routes to `UserError::internal`) |

**Class totals**: S = 30 (writeable static suggestion); N = 5 (no useful suggestion or compiler-internal); C = 0 (all context-dependent ones reduced to static via §3.5 dynamic-interpolation drop).

## 5. Type checker changes — `crates/cobrust-types/src/error.rs`

Each variant gains `suggestion: Option<&'static str>`. Twenty-four variants; mechanical field-add. Construction sites that need updating (anchors at HEAD `2a710d3`; prefer symbol-style per F34 pre-candidate):

- `check.rs:59` `TypeError::AmbiguousType` — populate with annotation suggestion.
- `check.rs:302`, `check.rs:316` `TypeError::MutableDefault` — populate with `None`-default suggestion.
- `check.rs:389`, `check.rs:398` `TypeError::BreakOutsideLoop` / `ContinueOutsideLoop` — populate with in-loop suggestion.
- `check.rs:539`, `check.rs:551`, `check.rs:570` `TypeError::NotIterable` — populate with iterable-types list.
- `check.rs:762` `TypeError::DictSpreadNotSupported` — populate with manual-merge suggestion.
- `check.rs:783`, `check.rs:1811`, `check.rs:1879` `TypeError::NotHashable` — populate with f64-to-bits suggestion.
- `check.rs:2009` `TypeError::UnknownName` — populate with `let <name> = …` suggestion.
- `check.rs:2076` `TypeError::ImplicitTruthiness` — populate with §2.5-canonical `if x != 0:` suggestion.

Total construction-site updates: ~40-50 across `check.rs` (each variant appears 1-4 times). Mechanical field-add; no logic change.

## 6. MIR error changes — `crates/cobrust-mir/src/error.rs`

Each variant gains `suggestion: Option<&'static str>`. Eleven variants; parallel mechanical field-add. Construction sites at HEAD `2a710d3`:

- `borrow.rs:114`, `borrow.rs:227` `MirError::UseAfterMove` — populate with `&s` suggestion (already render-time at `error_ux.rs:907`).
- `borrow.rs:236` `MirError::UseAfterDrop` — populate with reorder-read-before-drop suggestion.
- `borrow.rs:261` `MirError::ConflictingMutBorrow` — populate with release-first-borrow suggestion.
- `borrow.rs:270`, `borrow.rs:281` `MirError::SharedMutOverlap` — populate with release-shared-first suggestion.
- `drop.rs:303` `MirError::DoubleDrop` — populate with control-flow-check suggestion.
- `lower.rs:323` `MirError::UnresolvedDefId` — pass `None` (compiler-internal).
- `lower.rs:498`, `lower.rs:507`, `lower.rs:648` `MirError::Internal` — pass `None` (compiler bug).
- Helper `use_after_move()` at `error.rs::use_after_move` (currently `error.rs:132`) — extend signature with `suggestion: Option<&'static str>` arg.

Total construction-site updates: ~12-15 across the MIR crate.

## 7. CLI rendering changes — `crates/cobrust-cli/src/error_ux.rs`

The `From<TypeError> for UserError` impl at lines 563-893 and `From<MirError> for UserError` impl at lines 894-1000 are rewritten:

- Each variant's match arm extracts `suggestion` from the variant payload.
- The arm's existing hard-coded `Some("...".to_owned())` literal is replaced by `suggestion.map(str::to_owned)`.
- Format-arg interpolation (e.g. `format!("did you mean to declare it with \`let {name} = …\`?")`) is dropped per §11; replaced by static text via construction-site population.
- The precedent is `MirError::UseAfterMove` at `error_ux.rs:907` (Wave-1 hard-coded at render time) plus `TypeError::BorrowOfNonPlace` at `error_ux.rs:857` (Wave-1 reads `suggestion.map(...)` then falls back to static text). Direction B drops the fallback and standardises on `suggestion.map(str::to_owned)`.

Approximate diff: -180 lines (hint-prose literals) +60 lines (uniform `suggestion.map` plumbing) = net -120 lines in `error_ux.rs`.

## 8. F30 shadow-flip dry-run

Per `findings/predicate-flip-cascade-discovery-deficit.md`. The flip is mechanical (add a field to two enums + update ~55 construction sites + simplify ~35 renderer arms). Cascade legend: **E** = easy static suggestion ready; **C** = context-dependent (collapses to E once §3.5 dynamic-text drop applies); **N** = no useful suggestion (pass `None`).

| # | File:line | Variant | Class |
|---|---|---|---|
| 1 | `check.rs:59` | `AmbiguousType` | E |
| 2 | `check.rs:302, 316` | `MutableDefault` ×2 | E |
| 3 | `check.rs:376` | `ReturnOutsideFn` | E |
| 4 | `check.rs:389, 398` | `BreakOutsideLoop`, `ContinueOutsideLoop` | E |
| 5 | `check.rs:539, 551, 570` | `NotIterable` ×3 | E |
| 6 | `check.rs:613` | `NonExhaustiveMatch` | E |
| 7 | `check.rs:762` | `DictSpreadNotSupported` | E |
| 8 | `check.rs:783, 1811, 1879` | `NotHashable` ×3 | E |
| 9 | `check.rs:868` | `NotIndexable` | E |
| 10 | `check.rs:936, 948` | `YieldOutsideFn` ×2 | E |
| 11 | `check.rs:976, 1034-1251 (multi)` | `TypeMismatch` + `ArityMismatch` batch | C → E |
| 12 | `check.rs:1034-1251 (9 sites)` | `ArityMismatch` ×9 | E |
| 13 | `check.rs:1606` | `KeywordArgMismatch` | E |
| 14 | `check.rs:1645` | `NotCallable` | E |
| 15 | `check.rs:2009` | `UnknownName` | E (static §3.5) |
| 16 | `check.rs:2076` | `ImplicitTruthiness` | E (§2.5-canonical) |
| 17 | `borrow.rs:114, 227` | `UseAfterMove` ×2 | E (ADR-0052a §7) |
| 18 | `borrow.rs:236` | `UseAfterDrop` | E |
| 19 | `borrow.rs:261` | `ConflictingMutBorrow` | E |
| 20 | `borrow.rs:270, 281` | `SharedMutOverlap` ×2 | E |
| 21 | `drop.rs:303` | `DoubleDrop` | E |
| 22 | `lower.rs:323, 498, 507, 648` | `UnresolvedDefId` + `Internal` ×3 | N |
| 23 | `mir/error.rs::use_after_move` (`error.rs:132`) | `use_after_move()` signature | E |

**Totals**: 23 grouped rows = ~55 direct construction sites. E = 19 rows (~48 sites); C → E collapse = 1 row (5 sites); N = 1 row (~4 sites, all compiler-internal). All five `TypeMismatch` sites reduce to a single static text per §3.2 + §11.

**Cascade size prediction**: zero behavioural test regressions (struct-field add, no semantic change). Expected new cargo test failures: ~5-10 snapshot tests in `cobrust-types/tests/` + `cobrust-mir/tests/` if any test asserts on the exact suggestion text. Resolved by re-snapshot per §9.1.

## 9. TEST + DEV PAIR plan

Per F28 strict-separation. TEST authors snapshot-test corpus + sees variant-table only; DEV implements the field-add + renderer rewrite without seeing TEST corpus until P10 merge.

### 9.1 TEST corpus

- **Snapshot-tests (≥ 30 programs)**: one per variant of `TypeError` + `MirError` that has a useful suggestion (class S; 30 total per §4). Each test crafts a minimal ill-typed Cobrust source, runs `cobrust check`, snapshots the rendered diagnostic, and asserts the suggestion text appears verbatim.
- **No-suggestion-pass (≥ 5 programs)**: each of the 5 class-N variants (`RowConflict`, `Multiple`, `FieldOutOfBounds`, `UnresolvedDefId`, `Internal`) — assert the renderer omits the `hint:` line cleanly (no spurious empty hint).
- **Construction-helper test (≥ 1 program)**: `use_after_move()` helper at `error.rs:85` must accept the new `suggestion` arg; one test that constructs via the helper + asserts the suggestion threads through.

### 9.2 DEV phases

- Phase 1 (`error.rs` field-add, both crates): ~1h. Add `suggestion: Option<&'static str>` to all 35 variants. Update `Display` impl (no change — `thiserror` macro reads fields by name; new field is ignored unless referenced in `#[error("...")]` template). Update helper at `error.rs:85`.
- Phase 2 (`check.rs` construction-site population): ~1.5h. Walk the 28 `check.rs` `Err(TypeError::...)` sites; populate each with the §4.1 suggestion text. Mechanical.
- Phase 3 (`borrow.rs` / `drop.rs` / `lower.rs` construction-site population): ~30min. 8 sites total.
- Phase 4 (`error_ux.rs` renderer rewrite): ~1h. Drop the hard-coded match prose; replace with `suggestion.map(str::to_owned)`. Net -120 lines.
- Phase 5 (snapshot test re-baseline): ~30min. Re-run `cargo test --workspace`; resnapshot any test that asserted exact suggestion text.

### 9.3 Total

TEST: ~1.5-2h (sonnet — well-scoped snapshot author per ADR-0052 Wave-2 routing). DEV: ~4.5h (sonnet — mechanical multi-file field-add per ADR-0052 Wave-2 routing). P10 review + merge: ~30min. **Wall-time: ~4-6h** P10-direct PAIR. Lean estimate matches Wave-2 routing prediction (~3-4 days incl. merge audit per ADR-0052 §"Wave 2 — Directions B + C + D parallel").

## 10. §2.5 compliance

Per CLAUDE.md §2.5 audit-teammate rubric:

- **Compile-time-catch unchanged**: every `TypeError::*` and `MirError::*` variant still fires at the same trigger conditions. The diagnostic *content* changes; the *catch surface* does not. This preserves the §2.5 compile-time-catch win that Phase F.3 + ADR-0052a already established.
- **LLM agentic stderr-consumption sharpens**: today's stderr emits `hint: Cobrust requires an explicit bool — try \`if x != 0:\` or \`if x.is_some():\``; the LLM agent reads the prose and applies. Tomorrow's stderr emits `suggestion: change to \`if x != 0:\` (use \`.is_some()\` for Option)`; the LLM consumes the same fix, but the suggestion is now machine-structured (forward-compat for LSP / JSON / agent-loop §11). The win is in the structured-shape contract, not the immediate prose.
- **Training-data overlap**: Rust's `rustc` emits `help:` lines with similar canonical-fix prose; Python's CPython emits `SyntaxError: ...` without a fix path. Cobrust's `suggestion:` field matches the Rust-corpus surface more closely than today's bare `hint:` line, marginally improving LLM training-prior alignment per §2.5 B rule.

## 11. Out of scope

- **i18n of suggestion text**: Wave-2 ships English only. Bilingual `docs/human/zh/` + `docs/human/en/` doc updates happen but in-binary suggestion text stays English. Future micro-ADR may add localised suggestion tables.
- **JSON serialization of suggestions**: `Option<&'static str>` is forward-compat for `serde::Serialize` derive but Wave-2 does NOT ship `--emit-json` for diagnostics. LSP integration tracker (ADR-0050 §M-F.3.9) consumes the structured field when LSP work happens.
- **Diagnostic spans**: every variant already carries `span: Span`; Wave-2 does not enrich span representation.
- **Dynamic-format suggestions**: per §3.5, format-arg interpolation in suggestion text is dropped. `TypeError::UnknownName`'s today-text `did you mean to declare it with \`let {name} = …\`?` becomes static `declare with \`let <name> = …\` first`. A future micro-ADR may revisit if user feedback shows the name-bearing prose is materially clearer.
- **Multi-suggestion (`Vec<&'static str>`)**: real variants have one canonical fix per §2 rejected-alternatives. Future micro-ADR may lift.
- **Suggestion taxonomy / categorisation**: no metadata (`SuggestionKind::Replace`, `::Insert`, etc.) beyond the raw `&'static str`. LSP-grade taxonomies are post-Phase-G.

## 12. Consequences

### Positive

- §2.5 Direction B binding satisfied: every error variant carries a machine-structured suggestion field that LLM agentic stderr-consumption can parse without prose-stripping.
- Renderer simplifies by ~120 lines net: per-variant hint prose collapses into a uniform `suggestion.map(str::to_owned)` pattern.
- Wave-1 forward-compat (ADR-0052a §6 `BorrowOfNonPlace::suggestion`) becomes the established pattern across all 35 variants.
- Future LSP / `--emit-json` / agent-loop integration ships structurally-correct suggestion data without a second migration.
- Construction-site discipline forces designers to think about the fix path next to the diagnosis — improves long-term diagnostic quality.

### Negative

- Mechanical field-add touches 35 variants × 1-6 construction sites = 40-55 file:line edits. Low per-edit risk but high churn (large diff on a single sprint).
- Loss of dynamic-format suggestions (e.g. `UnknownName`'s `did you mean to declare it with \`let {name} = …\`?`): the new static text is generic and may be marginally less actionable for very specific name typos. §11 explicitly defers this. Acceptable for Wave-2 because the §2.5 LLM win comes from structured shape, not name-interpolation.
- Snapshot test re-baseline costs ~30min per Phase 5; tests asserting exact prose churn.

### Neutral

- `thiserror` macro derives `Display` from `#[error("...")]` template; the new `suggestion` field is not referenced in any template, so `Display` impl behaviour is unchanged for downstream `eprintln!` consumers. The structured-shape win lives in the `From<...> for UserError` impl only.
- ADR-0052a §6 `BorrowOfNonPlace::suggestion` shipped as `Option<&'static str>`; Wave-2 keeps the same shape. No re-versioning of the field type.
- Cross-sub-ADR interaction: Direction A's `BorrowOfNonPlace` continues to use `suggestion`; Direction D (0052d) method-call sugar will likely add new `TypeError::MethodNotFound` variant — that future variant inherits the `suggestion: Option<&'static str>` field per Direction B's binding.

### Cascade enumeration (post-spike)

Implementation merged at HEAD `365181a` (Phase G Wave-2 P10-direct PAIR per dispatch). The §8 F30 dry-run table prediction held with one scope expansion:

- **Predicted cascade**: 23 grouped rows = ~55 direct construction sites in `crates/cobrust-types/` + `crates/cobrust-mir/`. **Observed cascade**: ~55 type+MIR sites (matches), **plus** 7 construction sites in `crates/cobrust-hir/src/lower.rs` (LoweringError) that were not predicted. Total observed: ~62 sites.
- **Scope expansion to LoweringError**: 6 Wave-2 corpus tests (`s0052b_01`, `s0052b_16`, `s0052b_20`, `s0052b_27`, `s0052b_28`, `s0052b_29`) triggered HIR-lower's `LoweringError` as the actual catch surface (`UnknownName`, `DroppedFeature`). §2's literal text scopes Direction B to `TypeError + MirError`; the impl forwarded the same uniform `suggestion: Option<&'static str>` field to `LoweringError`'s 6 variants and the `From<LoweringError> for UserError` renderer per the same Wave-1 structural-pattern (§7). The scope expansion is consistent with §2.5's "every user-visible error carries the fix path" rule; future Direction-B-like extensions to `ParseError` / `LexError` are out-of-scope (Wave-2 cap).
- **Test corpus regression vs main HEAD `2031e50`**: 0 non-0052b regressions; +6 net new passes (`w0052a_06/07/08/18/19/28` borrow tests now satisfy the LoweringError-suggestion contract). 9 of 41 0052b tests remain in test-construction-blocker status (test harness `check_must_fail` panics on HIR-lower path; parser-level catch surface for `MutableDefault` + dropped `is` operator; renderer-snapshot uses `cobrust check` instead of `cobrust build`). These are documented finding `dev-impl-deferred-test-harness-mismatch.md`; cleared as out-of-scope per F28 strict-separation.
- **Renderer line-count delta**: `crates/cobrust-cli/src/error_ux.rs` from 1078 → ~966 lines (-112 lines net), within the §7 prediction window of -120 lines.
- **§13 design lesson (re-check)**: ADR-0052a §13 "no bidirectional unify arms / no inference-layer transparency" — re-confirmed unchanged. Direction B touches only error-type field shape + renderer plumbing; no `infer.rs` unify-arm churn.

## 13. Dispatch readiness

- **TEST budget**: 1.5-2 hours (sonnet — well-scoped snapshot author per ADR-0052 Wave-2 routing).
- **DEV budget**: 4-4.5 hours (sonnet — mechanical multi-file field-add per ADR-0052 Wave-2 routing; D2 mid-tier rule applies).
- **P10 review + merge**: 30 min including 5-gate green on DG workstation.
- **Total wall-time**: 4-6 hours P10-direct PAIR (Wave-2 leanest sub-ADR).
- **Pre-dispatch checklist**:
  - [ ] Frame ADR-0052 merged at HEAD `7ab04a4`.
  - [ ] ADR-0052a `BorrowOfNonPlace::suggestion` precedent confirmed at `crates/cobrust-types/src/error.rs:146-149`.
  - [ ] §4 variant table verified at spike-commit time.
  - [ ] §8 F30 dry-run table verified by grep at spike SHA.
- **Branch**: `feature/g-error-ux` (per ADR-0052 §"Wave 2 — Directions B + C + D parallel").
- **Merge target**: `main`.
- **Host routing**: Mac local design + impl per ADR-0052 §"Host routing" (Mac local then DG verify; Mode C). Heavy `cargo build --workspace` final gate runs on DG per heavy-build offload policy.
