---
doc_kind: adr
adr_id: 0055b
parent_adr: 0055
title: "Phase H Tier-1 — `crates/cobrust-types/src/error.rs` + `lib.rs` cb port"
status: proposed
date: 2026-05-18
last_verified_commit: f5d1f5a
supersedes: []
superseded_by: []
relates_to: [adr:0055, adr:0055e, adr:0055a, adr:0052b]
discovered_by: ADR-0055 §3.3 sub-ADR roster — Tier-1 wave-2 parallel batch
ratification_path: in-session review per ADR-0050 §"Audit model — teammate-in-session"; ratifies on impl merge under Phase H Wave-2 dispatch
---

# ADR-0055b: `error.rs` + `lib.rs` cb port

## 1. Context

Phase H Tier-1 stage per ADR-0055 §3.3 sub-ADR roster (`error.rs` + `lib.rs` cb port, Tier-1, week 1 days 3-5). ADR-0055 §3.5 places this ADR in **Wave 2** (parallel with 0055a) after Wave 1 (0055e parity-harness skeleton) confirms the Rust-vs-Rust diff-empty baseline.

`crates/cobrust-types/src/error.rs` at HEAD `f5d1f5a` is **239 LOC** containing:

- `TypeError` enum with **22 variants**, every one span-bearing + carrying a uniform `suggestion: Option<&'static str>` field per ADR-0052b §2 Direction B (LLM-first error UX).
- `#[error("...")]` thiserror-derived `Display` impl on every variant.
- One composite variant `TypeError::Multiple(Vec<TypeError>)` for multi-error aggregation.
- Variants carry payload of `Ty` (`TypeMismatch::expected`, `RowConflict::ty1`+`ty2`, `OccursCheck::ty`, `ImplicitTruthiness::actual`, `NotCallable::actual`, `NotIndexable::actual`, `NotIterable::actual`, `NotHashable::actual`), `VarId` (`OccursCheck::var`), or `String` (`UnknownName::name`, etc.).

`crates/cobrust-types/src/lib.rs` at HEAD `f5d1f5a` is **61 LOC** containing:

- `#![forbid(unsafe_code)]` + ~35 `#![allow(clippy::...)]` crate-level lints.
- 4 `pub mod` declarations (`check`, `error`, `infer`, `ty`).
- 4 `pub use` re-export lines exposing the canonical surface (`TypedModule`, `check`, `TypeError`, `Subst`, `finalize`, `unify`, plus 7 names from `ty`).

Per ADR-0055 §3.3, this sub-ADR ships **in parallel with 0055a** as Tier-1 wave-2 batch. Combined Tier-1 surface (this ADR's 239 + 61 = 300 LOC + 0055a's 407 LOC = 707 LOC) is ~21% of Phase H total scope (3368 LOC per ADR-0055 §4) and closes ~5-7 days into Wave 2 per ADR-0055 §3.5 budget. Per CLAUDE.md §2.5 §B training-data-overlap binding: error-message text + suggestion field must round-trip byte-equal between Rust impl + cb mirror so the LLM agentic stderr-consumption loop sees identical signal.

The §1 surface choice — porting `lib.rs` alongside `error.rs` rather than splitting into its own sub-ADR — follows ADR-0055 §3.3 sub-ADR roster: `lib.rs` is 61 LOC of pure module-exports + lint allows, too small to justify its own dispatch. Bundling with `error.rs` keeps the Tier-1 sub-ADR count at 2 and preserves the §3.5 Wave-2 parallelism contract.

## 2. Decision

**Port `error.rs` to `crates/cobrust-types-cb/src/error.cb` + port `lib.rs` to `crates/cobrust-types-cb/src/lib.cb`** under the same arena-form workaround as 0055a. The Rust impl at `crates/cobrust-types/src/error.rs` + `lib.rs` stays canonical per ADR-0055 §3.1; the cb mirror is a **proof artifact** verified diff-empty by the ADR-0055e parity harness on the M2 ill-typed corpus modulo arena-id renaming.

Concretely:

- `TypeError` enum mirrored 1:1 — same 22 variants, same variant names, same payload field names. `Ty` payload fields become `i64` arena handles (consuming 0055a's `TyArena` per §"Cross-ADR coordination"). `VarId` payload (`OccursCheck::var`) becomes `i64` per 0055a's VarId-as-i64 convention.
- `suggestion: Option<&'static str>` payload field — see §"Risk register" risk 1 for the `&'static str` representation decision. Cobrust has no `'static` lifetime annotation per ADR-0055 §4.1; field type becomes `Option[str]` (owned Cobrust string).
- `Display` impl — replaced by free function `display_error(arena: &TyArena, err: &TypeError) -> str` per ADR-0055 §4.1 ("User-defined traits NOT shipped"). Format strings match the Rust `#[error("...")]` arguments byte-for-byte.
- `lib.cb` — preserves module exports + re-exports. Cobrust has no `#![allow(...)]` lint attributes; the lint block omits cleanly (the Cobrust toolchain emits its own lint discipline per ADR-0048 §"Toolchain lints"). `pub mod` + `pub use` translate to Cobrust `pub mod` + `pub use` per ADR-0050a §"Module system" baseline. The `#![forbid(unsafe_code)]` attribute is **N/A** in cb — Cobrust has no `unsafe` keyword per CLAUDE.md §2.3 baseline; the cb mirror inherits forbid-unsafe semantics structurally.
- `TypeError::Multiple` flattening — the cb port preserves Rust's invariant that `Multiple` is non-recursive in practice (callers flatten before construction). No depth-limit guard added in cb; harness corpus exercises ≤2-level Multiple to match Rust observed surface.

## 3. Arena workaround (per ADR-0055 §"Option B" + 0055a §3)

Per ADR-0055 §5 + 0055a §3, every Rust `Ty` payload field in `TypeError` becomes an arena handle in the cb mirror:

| Rust impl (`error.rs::TypeError`) | cb mirror (`error.cb::TypeError`) |
|---|---|
| `TypeMismatch { expected: Ty, actual: Ty, span, suggestion }` | `TypeMismatch { expected: i64, actual: i64, span, suggestion }` |
| `RowConflict { field, ty1: Ty, ty2: Ty, span, suggestion }` | `RowConflict { field: str, ty1: i64, ty2: i64, span, suggestion }` |
| `ImplicitTruthiness { actual: Ty, span, suggestion }` | `ImplicitTruthiness { actual: i64, span, suggestion }` |
| `OccursCheck { var: VarId, ty: Ty, span, suggestion }` | `OccursCheck { var: i64, ty: i64, span, suggestion }` |
| `NotCallable { actual: Ty, ... }` (+ NotIndexable, NotIterable, NotHashable) | `NotCallable { actual: i64, ... }` (each follows the same pattern) |
| `Multiple(Vec<TypeError>)` | `Multiple(list[TypeError])` — list-of-TypeError, recursion at the enum level (not via arena), tree-shaped per ADR-0055 §5 |

`TypeError::Multiple` is **the only recursive variant** in `error.rs`. Per ADR-0055 §5, Phase H types are tree-shaped; the cb `list[TypeError]` form is portable (Cobrust `list[T]` is heap-backed per ADR-0050d §"Container internals" — analogous to Rust `Vec<TypeError>`). No arena workaround needed for `Multiple` because errors-of-errors do not cycle: the parser emits a flat error stream, and `Multiple` aggregates a non-recursive list.

The `Span` payload (`cobrust_frontend::span::Span`) is shared with the Rust frontend per ADR-0055 §3.1 ("frontend stays Rust"). Per ADR-0055e §3 closing paragraph, `Span` is **raw-equality** in the parity harness (not canonicalized). The cb mirror imports `Span` via the Cobrust-from-Rust FFI surface (TBD per ADR-0055 §6 pre-Phase-H `cobrust-cb compile-and-diff infrastructure spike`); Phase 1 of 0055e Phase 3 wire-in defines the FFI handle shape.

Phase 7.5 (recursive struct types) is **NOT a prerequisite** per ADR-0055 §3.2. Even `Multiple(list[TypeError])` works under M2 (no Phase 7.5 needed) because `list[T]` is heap-backed and `T` is the enum type itself — a list of values, not a recursive struct field.

## 4. Surface invariants

Per ADR-0055e §3 + §6 (BLOCK rules on per-input divergence), the cb port MUST satisfy:

- **Every `TypeError::*` variant** in Rust `error.rs` MUST appear in cb `error.cb` with **identical name** and **identical payload field names**. Variant ordering inside the enum is irrelevant (canonicalization is variant-name-keyed per ADR-0055e §3).
- **`suggestion` field** present on every variant per ADR-0052b §2 Direction B uniform shape. The Rust `Option<&'static str>` becomes cb `Option[str]` (owned); the parity harness compares string values byte-equal per ADR-0055e §6 BLOCK rule 4 ("`suggestion` field divergence → BLOCK").
- **`Span` payload** preserved on every variant; raw byte-offset equality enforced by the harness per ADR-0055e §6 BLOCK rule 3 ("`Span` raw byte-offset divergence → BLOCK").
- **`#[error("...")]` format strings** — every Rust `#[error("type mismatch: expected `{expected}`, found `{actual}` at {span}")]` literal MUST be reproduced byte-identically in cb `display_error` arm. Includes the backtick-quoted `{expected}` / `{actual}` glyph (consumes `display_ty(arena, expected_id)` per 0055a §4 Display-parity invariant) and the trailing `at {span}` clause.
- **`lib.cb` re-exports** — every `pub use` line in Rust `lib.rs` MUST be reproduced in cb `lib.cb` with identical name list. The 7 names from `ty` (`AdtId`, `AliasId`, `FnTy`, `GenericVar`, `Record`, `Ty`, `VarAllocator`, `VarId`) re-export under their arena-form types per 0055a §2. `Ty` re-export in cb form is `TyEntry` + `TyId` pair (consumers import both); naming alignment: the cb mirror keeps `Ty` as a re-export alias for `TyEntry` to preserve Rust-side import-shape per ADR-0055e parity-harness FFI surface.

### 4.1 Per-variant compliance matrix

The 22 `TypeError::*` variants split into four shape classes:

- **Name-only variants** (`BreakOutsideLoop`, `ContinueOutsideLoop`, `ReturnOutsideFn`, `YieldOutsideFn`, `MutableDefault`, `AmbiguousType`, `DictSpreadNotSupported`) — 7 variants with only `span` + `suggestion`. Trivial port: same shape under cb.
- **Name + String payload** (`UnknownName`, `KeywordArgMismatch`, `MissingArgument`, `DuplicateField`, `UseOfDroppedFeature`) — 5 variants. Cobrust `str` replaces Rust `String`; `&'static str` for `UseOfDroppedFeature::name` becomes owned `str` (§6 risk 1).
- **Name + Ty payload** (`TypeMismatch`, `RowConflict`, `ImplicitTruthiness`, `NotCallable`, `NotIndexable`, `NotIterable`, `NotHashable`, `OccursCheck`) — 8 variants. Each `Ty` field becomes `i64` arena handle per §3 table.
- **Composite + special** (`ArityMismatch`, `NonExhaustiveMatch`, `BorrowOfNonPlace`, `UnknownMethod`, `Multiple`) — 5 variants. `Multiple` flattens into `list[TypeError]`; `NonExhaustiveMatch::uncovered: Vec<String>` becomes `list[str]`; `BorrowOfNonPlace` has only span + suggestion (forward-compat per ADR-0052a Wave-1 §6).

## 5. Cobrust source coverage

Cb-port-required language features at HEAD `f5d1f5a` per ADR-0055 §4.1 feature-gap inventory:

- **`enum` with associated data** — shipped per ADR-0050d Dict + ADR-0006 ADT. Each `TypeError::*` variant carries struct-fields-style payload (matches Rust `Foo { field: T, ... }` per ADR-0050d §"Struct-shaped enum variants").
- **`Option[T]`** — shipped per ADR-0050a §"Option type" baseline. Used for `suggestion: Option[str]` on every variant.
- **`list[T]`** — shipped per ADR-0050d. Used for `Multiple(list[TypeError])`.
- **Owned `str`** — shipped per ADR-0050c §"Str ownership" + ADR-0052a Wave-1 (Str non-Copy uniformly). Used for `UnknownName::name`, `KeywordArgMismatch::name`, etc., and for the `suggestion: Option[str]` replacement of Rust's `&'static str` (see §6 risk 1).
- **Exhaustive `match`** — shipped (M2 baseline). Used in `display_error` dispatch over 22 variants.
- **`pub mod` + `pub use`** — shipped per ADR-0050a §"Module system" baseline. Used in `lib.cb` re-exports.
- **Method-call sugar** — shipped per ADR-0052d (Phase G method-form). Improves `display_error` ergonomics.

**Not required** (per ADR-0055 §4.1):

- `&'static str` lifetime — Cobrust has no `'static`; replaced by owned `Option[str]` (see §6 risk 1).
- `thiserror::Error` derive — Cobrust does not ship `thiserror`-equivalent macros at M2; the `#[error("...")]` Display formatting is hand-rolled in `display_error` (per ADR-0055 §4.1 "User-defined traits NOT shipped").
- `#![allow(clippy::...)]` lint attributes — Cobrust toolchain emits its own lint discipline per ADR-0048 §"Toolchain lints"; `lib.cb` omits the lint block cleanly.

All required features are ALREADY shipped per CLAUDE.md §2.1-2.4 baseline + ADR-0050a-f Phase F.3 + Phase G surface. No language-feature blocker between this ADR and impl dispatch.

## 6. Risk register

Top 3 risks ranked by impl-blast-radius:

1. **`suggestion: &'static str` representation in cb** — Rust impl uses `Option<&'static str>` because every suggestion is a compile-time literal per ADR-0052b §2 ("populated at construction time"). Cobrust has no `'static` lifetime; the natural port is `Option[str]` (owned). Two concerns: (a) cb-side construction allocates a fresh str (vs Rust zero-alloc literal reference); negligible cost at M2 since errors are not hot-path. (b) parity harness compares string **value** byte-equal per ADR-0055e §6 BLOCK rule 4 — both impls must emit the same characters regardless of lifetime; the lifetime difference is invisible to the harness. Mitigation: cb impl emits the same literal-text suggestions as Rust; the static-vs-owned distinction is purely an impl-internal storage choice with no observable surface impact.

2. **Pretty-printing parity** — every `#[error("...")]` format string in Rust must be reproduced byte-identically in cb `display_error`. Subtle drift risks: argument ordering (`{expected}` before `{actual}` per `TypeMismatch` arm); backtick-vs-quote glyphs (Rust `\`{name}\`` becomes cb-side `\` + name + `\``); `{span}` rendering invokes `Display for Span` from the shared Rust frontend (per ADR-0055 §3.1 frontend-stays-Rust) — the cb side calls back into the Rust `Span` Display through the FFI surface defined by ADR-0055 §6 pre-Phase-H spike. Mitigation: 0055e Phase 2 sanity stage extends to include "every `display_error` variant round-trips through canonicalization" property test; calibrates byte-equality on the 22 variant Display outputs before any cb impl wires in.

3. **`lib.rs` re-export equivalence** — the cb `lib.cb` re-export surface MUST preserve every `pub use` name from Rust `lib.rs` so downstream Tier-2 ports (0055c `infer.rs`, 0055d `check.rs`) can `use cobrust_types_cb::{TypeError, Ty, VarId, ...}` with the same import shape. Drift risks: arena-form `Ty` is `TyEntry`-and-arena-handle in cb; the re-export must preserve the **name** `Ty` (as alias to `TyEntry` per §4) for Tier-2 import-shape parity. `VarAllocator` re-export is name-identical; payload shape per 0055a §"Decision" instance-field-counter form. The 4 `pub mod` lines (`check`, `error`, `infer`, `ty`) translate 1:1; the 4 `pub use` lines preserve every name (`TypedModule`, `check`, `TypeError`, `Subst`, `finalize`, `unify`, `AdtId`, `AliasId`, `FnTy`, `GenericVar`, `Record`, `Ty`, `VarAllocator`, `VarId`). Mitigation: this ADR's §4 lists the re-export-name contract explicitly; the parity harness's per-input granularity (ADR-0055e §2) catches re-export drift at compile time on Tier-2 impl (cb file fails to compile = harness fails CI before parity is even tested).

## 7. Pre-dispatch gate

Required before this ADR's P9 design spike + P10-direct PAIR dispatches:

- [ ] **ADR-0055e accepted + Phase 1 + Phase 2 merged** — parity-harness skeleton + Rust-vs-Rust sanity baseline. Per ADR-0055 §3.5 Wave 1 → Wave 2 sequencing.
- [ ] **ADR-0055 frame ratified** — ratifies on first sub-ADR dispatch per its `ratification_path`. 0055e is the first; this ADR is Wave 2 (after 0055e closes).
- [ ] **F34 symbol-anchor convention** — adopted in this ADR per pre-read 6. All cross-references use `error.rs::TypeError::TypeMismatch` form, not `error.rs:55-61` numeric.

No dependency on Phase 7.5 (recursive struct types) per ADR-0055 §3.2.

## 8. Cross-ADR coordination

- **Feeds into 0055c (`infer.rs` cb port, Tier-2)** — `Subst::unify` emits `TypeError::TypeMismatch` / `OccursCheck` / `RowConflict` variants. Requires this ADR's `TypeError` enum + arena-form `Ty` payload to land first. Per ADR-0055 §3.5 Wave 2 → Wave 3 sequencing.
- **Feeds into 0055d (`check.rs` cb port, Tier-2)** — every checker rule emits one of the 22 `TypeError::*` variants. `lib.cb` re-exports must be stable before 0055d's `use cobrust_types_cb::{TypeError, Ty, ...}` lines compile.
- **Parallel with 0055a** — `ty.rs` cb port. Both Tier-1; both block on 0055e. Coordination point: this ADR's `TypeError` variants carry `Ty` payload as `i64` arena handles consuming 0055a's `TyArena` shape. Agree on arena-passing convention (`&TyArena` argument to `display_error`).
- **Coordinates with ADR-0055e** — parity harness BLOCK rules (§6: accept/reject, variant name, Span raw, suggestion, Ty canonical) all target `TypeError` shape. This ADR's §4 invariants are the per-variant compliance surface the harness enforces.
- **Inherits from ADR-0052b** — every variant's `suggestion: Option<&'static str>` thread originates in 0052b §2. The cb mirror preserves that thread in `Option[str]` form (see §6 risk 1).

## 9. Consequences / Dispatch readiness

### 9.1 Positive

- Tier-1 closure (with 0055a) hands Tier-2 ports (0055c + 0055d) a complete data-surface API: `TyArena` + `TyEntry` + `TypeError` + `lib.cb` re-exports. Tier-2 spikes can start without re-litigating Tier-1 surface choices.
- 22 `TypeError::*` variants in cb mirror become §2.5 §B training-data overlap surface — every future Cobrust error-type port (HIR errors, MIR errors per ADR-0054 §11) learns from this 22-variant cb-enum layout.
- `lib.cb` re-exports are mechanical; `lib.rs` ≈ 61 LOC ⇒ cb mirror ≈ 30 LOC after dropping the clippy lint block. Smallest sub-port in Phase H.

### 9.2 Negative

- `display_error` hand-roll (vs Rust `thiserror::Error` derive) duplicates the 22 format strings on the cb side. Risk of format-string drift between Rust and cb is real; ADR-0055e Phase 2 sanity test (per §6 risk 2 mitigation) catches drift but adds a new property-test surface.
- `Span` FFI surface (TBD per ADR-0055 §6 pre-Phase-H spike) is a hidden coupling; if the FFI handle shape changes during the pre-Phase-H spike, this ADR's §3 + §4 invariants need calibration. Phase 1 of 0055e Phase 3 wire-in is the calibration point.
- `suggestion` field's `Option[str]` (vs Rust `Option<&'static str>`) allocates fresh owned strings on every construction; M2 cost negligible (errors are cold-path) but the static-literal-reference advantage Rust has is lost. If post-M11 profiling surfaces an error-construction hotspot, the cb mirror may need to revisit (out-of-scope for Phase H per ADR-0054 §11).

### 9.3 Dispatch shape

- **TEST**: sonnet — well-scoped impl per this ADR's §4 invariants. Property tests on 22-variant Display round-trip + suggestion-field presence + Multiple-variant flattening.
- **DEV**: opus — variant proliferation (22 arms) is mechanical but Display byte-parity (risk 2) needs §2.5 compile-time-catch discipline.
- **Wall**: ~2-3 days per ADR-0055 §3.5 Wave 2 budget (smaller surface than 0055a; faster close possible).
- **Host**: DG primary per ADR-0055 §9.1 row 4. Mode C (P10-direct PAIR).

— P9 Tech Lead, 2026-05-18
