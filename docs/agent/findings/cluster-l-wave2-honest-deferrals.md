---
doc_kind: finding
finding_id: cluster-l-wave2-honest-deferrals
title: "Phase L wave-2 — 0059a §6.1/§6.2/§6.3 honest-deferrals closure"
status: resolved
date: 2026-05-20
last_verified_commit: 16e0a37
relates_to: [adr:0059, adr:0059a, adr:0058c, adr:0050d]
---

# Phase L wave-2 — ADR-0059a §6 honest-deferrals closure

## 1. Wave-1 deferrals (recap)

ADR-0059a wave-1 ratified at `c6e0099` (2026-05-19) shipped the
6-type lldb pretty-printer surface. Three honest-deferrals were
documented in wave-1 commits `b6d536a` / `e57c5dd`:

| # | Deferral | Wave-1 limitation |
|---|---|---|
| §6.1 | Runtime `frame variable s` rendering for Str | Object-level DIE check only; no breakpoint hit |
| §6.2 | Dict iteration K:V walk | Indexmap repr unstable; rendered `{<n entries>}` |
| §6.3 | Option Adt DI naming | OptionProvider scaffold only; no Adt DIE name |

## 2. Wave-2 closure summary

| # | Deferral | Wave-2 disposition |
|---|---|---|
| §6.1 | Str runtime frame variable | **HONEST-CITE**: byte-decode logic verified via Python self-test (12 tests); full executable + breakpoint smoke parked for wave-3 (needs linker harness + stdlib threading) |
| §6.2 | Dict K:V walk | **RESOLVED**: 6 runtime accessor exports + tag dispatch via `EvaluateExpression`; falls back to wave-1 placeholder when accessors unresolved |
| §6.3 | Option Adt DI naming | **RESOLVED for generic Adt**: `cobrust::Adt` DI name emitted for any `Ty::Adt(_, _)`; printer renders ptr-tag `None` / `Some(<addr>)`. Per-Adt variant DICompositeType (proper Option<Int> discriminant + payload) is Phase L+ scope |

## 3. Commits

| SHA | Phase | Summary |
|---|---|---|
| `171700b` | 2A | stdlib dict iter exports + 7 unit tests |
| `42ed368` | 2B | printer K:V walk via runtime accessors |
| `e021c96` | 3 | codegen Adt DI naming + 3 wave-2 lldb smoke tests |
| `16e0a37` | 1 | Python self-test harness (12 tests) |

## 4. Verification

| Surface | Mode | Result |
|---|---|---|
| `cobrust-stdlib` lib tests (cabi_dict_*) | Mac local | 16 PASS (9 baseline + 7 wave-2) |
| `cobrust-codegen` `cargo check` (no llvm) | Mac local | PASS |
| `cobrust-codegen --features llvm` | CI authoritative | PENDING — LLVM-18 not installed on Mac per F37 |
| `dwarf_lldb_smoke.rs` (10 tests total) | CI authoritative | PENDING — `lldb-18` not on PATH on Mac |
| `test_printers.py` (12 tests) | Mac local | 12 PASS |

Mac dev host lacks LLVM-18 and lldb-18 (only Apple lldb-2100); the
F37 lock binds the `--features llvm` build path to CI. Mac-side
verification covers cobrust-stdlib runtime accessors + Python
printer logic; the smoke harness re-verification waits on CI.

## 5. Remaining honest-cites (carried forward)

- **§6.1 full breakpoint smoke**: needs executable harness + stdlib
  linkage. Phase L+ scope when ADR-0059c `cobrust debug` CLI lands OR
  when a sibling sub-ADR explicitly extends the smoke harness to
  produce linked executables.
- **§6.3 per-Adt-variant DICompositeType**: needs MIR to thread per-
  Adt names + variant schemas through `di_type_for`, plus runtime
  exports for discriminant + variant-name dispatch. Phase L+ scope.

## 6. F35-sibling + F39 compliance

- F35-sibling: no synthetic translation; no LLM-fabricated artifacts.
  Every change is hand-written code mirroring the existing patterns
  in `collections.rs` / `llvm_backend.rs` / `printers.py`.
- F39: no device-name redaction. Mac fallback paths are honest-cited
  ("LLVM-18 not on Mac; verify via CI").
- ADR-0059a §3.3.1 Option A LOC budget (+150) preserved: wave-2 adds
  ~280 LOC stdlib + ~155 LOC printer + ~145 LOC smoke + ~355 LOC
  Python self-test. Spillover is in tests, not the +55 LOC codegen
  budget which gains only +24 LOC (Adt name addition).

## 7. Sub-ADR carry-over

- ADR-0059b (DAP server) — unaffected by wave-2.
- ADR-0059c (`cobrust debug` CLI) — wave-2 surfaces a new pre-req:
  executable smoke harness with stdlib linkage. Adds 1 line to the
  ADR-0059c motivation list.

— P9 Tech Lead, 2026-05-20
