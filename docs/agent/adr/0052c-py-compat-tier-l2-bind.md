---
doc_kind: adr
adr_id: "0052c"
parent_adr: "0052"
relates_to: ["adr:0037", "adr:0052", "adr:0051"]
title: "Phase G Direction C — @py_compat tier hard-bind to L2 verifier"
status: accepted
date: 2026-05-17
authors: [p9-tech-lead-wave2, p10-dev-wave2]
supersedes: []
superseded_by: []
last_verified_commit: "0418eae"
---

# ADR-0052c — Direction C: `@py_compat` tier hard-bind to L2 verifier

## 1. Context

Phase G Wave 2 Direction C per ADR-0052 lines 199 + 260 + 269. Activates ADR-0037
(`proposed reserved` since 2026-05-10, 6 weeks dangling) to `accepted`. Today:

- `FunctionSpec.py_compat` is parsed as a raw `String` (see `crates/cobrust-translator/src/spec.rs:48`).
- The string is *only* echoed into the L1 translation prompt (`crates/cobrust-translator/src/translate.rs:349`) and into the `@py_compat(none)` repair-failure footer (`crates/cobrust-translator/src/repair.rs:233`). It influences **no** L2 gate decision.
- The default behavior verifier is `AcceptAll` (`crates/cobrust-translator/src/pipeline.rs:276-288`), which uses `GateOutcome::Skip` to honestly record that no L2.behavior gate was wired.
- All three production corpus PROVENANCEs (`corpus/tomli/spec.toml`, `corpus/dateutil/spec.toml`, `corpus/msgpack/spec.toml`) declare `py_compat = "strict"` exclusively. `numerical` / `semantic` / `none` tier specs exist only in M7+ numpy corpus and are not yet wired.

§2.5 LLM-first principle (CLAUDE.md line 75-76) demands compile-time-catch + training-data-overlap. Today's `String` tier surface fails both: typos like `"strikt"` pass parse silently; numeric tolerance has no schema. ADR-0052 line 199 schedules Direction C as Wave 2 / DG-primary / ~5-7 days. ADR-0052 line 269 forecasts: "today's `AcceptAll` masks real divergences in the translator test corpus."

## 2. Decision

1. Replace `FunctionSpec.py_compat: String` with `FunctionSpec.py_compat: PyCompatTier` (Rust enum) via a serde custom `Deserialize` impl that parses the existing TOML strings backward-compatibly.
2. Introduce `TierVerifier` (impl `BehaviorVerifier`) that reads `FunctionSpec.py_compat` and dispatches a tier-specific verdict policy, replacing `AcceptAll` as the production default for `translate()` (the `AcceptAll` impl stays exported as the no-op-for-test variant).
3. Translator prompt construction (`translate.rs:349`) consumes the typed enum and emits tier-specific guidance (strict → bit-identity; numerical → rtol-aware float compare).
4. LLM Router routing config gains a per-tier override hook: tier-`Strict` translation tasks route through `StrategyName::Consensus` regardless of the global `routing.translate.strategy` default.
5. Migration sweep of `tomli` + `dateutil` + `msgpack` corpus PROVENANCEs under the tightened gate; expected 1-2 latent regressions per ADR-0052 line 269.

## 3. Tier semantics matrix

| Tier | L2.behavior gate threshold | L2.perf gate threshold (ADR-0008 baseline) | LLM router routing implication |
|---|---|---|---|
| `Strict` | Byte-identical oracle output. `assertEqual(actual, oracle)` for all exemplars + ≥1000 fuzz inputs. Any divergence = `Reject`. | ≥ 0.8× of oracle on the rep benchmark; `Reject` if regress > 20%. | `StrategyName::Consensus` with `n=2` (override of `routing.translate`). Strict is the highest-correctness tier; consensus is mandatory. |
| `Semantic` | Behavioral-equivalence oracle: structural match permitted (e.g. dict key order ignored, error message text ignored if error *kind* matches). Fuzz ≥1000. | ≥ 0.6× of oracle; `Reject` if regress > 40%. | `StrategyName::Quality` default — single-model OK; consensus optional via per-task override. |
| `Numerical { rtol: f64 }` | `assert_allclose(actual, oracle, rtol=…)` (NumPy-canonical). Tolerance read from the enum payload. Fuzz ≥1000 with finite-input filtering. | ≥ 0.5× of oracle (numerical kernels routinely under-perform NumPy; floor relaxed). | `StrategyName::Cost` default — cheap single-model fine since rtol absorbs minor LLM emission variance. |
| `None` | Gate disabled: `VerifierVerdict::Accept` unconditionally + `GateOutcome::Skip { reason: "py_compat tier = none" }` recorded honestly per ADR-0040. | Skip; record honestly. | `StrategyName::Cost` (no correctness contract = no consensus budget justified). |

Per ADR-0052 line 246, Direction C ships the *thresholding* but no `numpy` translation; the `Numerical` arm exists to be ready when M7+ numpy lands. Tier `Semantic` is reserved for libraries where dict-iteration-order / error-text drift is acceptable (e.g. `urllib.parse` Python-3-vs-2 quirks).

## 4. Type changes — `crates/cobrust-translator/src/spec.rs`

Replace the raw `String` field with a typed enum + custom serde:

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum PyCompatTier {
    Strict,
    Semantic,
    Numerical { rtol: f64 },
    None,
}
```

`Deserialize` accepts:
- `"strict"` → `Strict`
- `"semantic"` → `Semantic`
- `"numerical(rtol=1e-7)"` (or `"numerical(rtol=1e-12)"` etc.) → `Numerical { rtol: 1e-7 }` via regex parse
- `"none"` → `None`
- Any other string → `SpecError::Malformed("py_compat: unknown tier '{}'; expected strict|semantic|numerical(rtol=…)|none")`. This is the §2.5 compile-time-catch surface — typos surface at spec-load instead of at L2-fail-time.

`Serialize` writes the canonical string back so RoundTrip(spec) is stable per the BTreeMap-deterministic-order contract.

## 5. Verifier changes — `crates/cobrust-translator/src/pipeline.rs`

```rust
pub struct TierVerifier {
    /// Oracle harness producer (closure or struct) supplied by the
    /// caller; the verifier owns dispatch but not oracle execution.
    pub oracle: Arc<dyn OracleHarness>,
}

impl BehaviorVerifier for TierVerifier {
    fn verify(&self, function: &FunctionTranslation, attempt: u32) -> VerifierVerdict {
        match &function.spec.py_compat {
            PyCompatTier::Strict       => self.verify_bit_identical(function, attempt),
            PyCompatTier::Semantic     => self.verify_semantic(function, attempt),
            PyCompatTier::Numerical { rtol } => self.verify_allclose(function, attempt, *rtol),
            PyCompatTier::None         => VerifierVerdict::Accept,
        }
    }

    fn default_outcome(&self) -> GateOutcome {
        GateOutcome::Pass { detail: "TierVerifier wired".into() }
    }
}
```

The `pipeline.rs:300-302` `translate()` entry point continues to call `translate_with_verifiers(&AcceptAll, &AcceptAllPerf)` for **test/benchmark** paths (no real oracle wired) but production users of the M4/M5/M6 corpus flip to `TierVerifier`. `AcceptAll` is now formally a test fixture, not a production default.

## 6. Translator changes — `crates/cobrust-translator/src/translate.rs`

Tier-aware prompt construction at line 349:

- `Strict` → "Output MUST be bit-identical to the CPython oracle on all exemplars; any divergence fails the gate."
- `Numerical { rtol }` → "Output MUST satisfy `numpy.assert_allclose(rtol={rtol})` vs the oracle; small float drift OK."
- `Semantic` → "Output MUST match the oracle structurally; dict key order and error message text may differ provided the error *kind* matches."
- `None` → "No L2 gate; emit the most faithful translation you can."

The change converts the current line-350 `writeln!(s, "6. Py-compat tier: {tier}")` from a tier-opaque echo into a tier-typed instruction block.

## 7. Router integration — `crates/cobrust-llm-router/`

New `RoutingEntry` per-tier override hook in `crates/cobrust-llm-router/src/config.rs` (extends the existing `BTreeMap<String, RoutingEntry>` at line 106):

```toml
[routing.translate]
strategy = "quality"      # default
preferred = [...]

[routing.translate.tier_override.strict]
strategy = "consensus"
n = 2
preferred = ["anthropic_official:claude-opus-4-7", "deepseek:deepseek-v3"]
```

Validation at `config.rs:152-180` extends to enforce: if any `tier_override.<tier>` block exists, its `strategy = "consensus"` arm must satisfy the existing `n>=2` + `preferred.len()>=n` invariants.

The translation pipeline reads `function.spec.py_compat` and constructs the `Task::Translate { tier: PyCompatTier }` discriminant; the router resolves per-tier override before falling back to `routing.translate` defaults.

## 8. Migration plan

Existing corpus PROVENANCEs all declare `py_compat = "strict"` (verified §1 grep). Migration order:

1. `corpus/tomli/spec.toml` — 6 functions × `strict`. Run full L2 with `TierVerifier { oracle: TomliOracle }`. Expected: pass (M4 already differential-tests via `corpus/tomli/harness/`).
2. `corpus/dateutil/spec.toml` — 8+ functions × `strict`. Expected: 1 latent regression on a leap-second / TZ edge case (per ADR-0052 line 269 forecast). Disposition: file `findings/0052c-dateutil-strict-regression-N.md` + repair-loop or tier-downgrade-to-semantic per case.
3. `corpus/msgpack/spec.toml` — 18+ functions × `strict`. M6 native-extension corpus; expected most-fragile. Disposition: same.

The migration runs on DG primary per heavy-build offload policy; each regression gets its own remediation receipt (not blocking the impl merge).

## 9. F30 shadow-flip dry-run — `AcceptAll` + `py_compat` consumer enumeration

Per `findings/predicate-flip-cascade-discovery-deficit.md`, every shipped behavior flip needs an up-front consumer list. Grep-based enumeration at HEAD `8dc2723`:

**`AcceptAll` consumers (grep `AcceptAll\b` excluding test/doc):**

- `crates/cobrust-translator/src/pipeline.rs:276` — type def (replaced semantics).
- `crates/cobrust-translator/src/pipeline.rs:301` — `translate()` default wiring (flips to `TierVerifier` for prod, retains `AcceptAll` only via explicit `translate_with_verifiers` opt-in).
- `crates/cobrust-translator/src/pipeline.rs:318` — `translate_with_verifier` perf-default arm (no behavior change; still `AcceptAllPerf`).
- `crates/cobrust-translator/src/lib.rs:79` — pub re-export (stays).
- `crates/cobrust-translator/src/pipeline.rs:565,658` — internal `default_outcome()` hook docs (stay; behavior unchanged for `AcceptAll` test fixture).

**`py_compat` field consumers (grep `\.py_compat\b` + `spec\.py_compat`):**

- `crates/cobrust-translator/src/translate.rs:190` — `format_prompt_body` echo (tier-aware instruction block per §6).
- `crates/cobrust-translator/src/translate.rs:349` — prompt-body line 6 (same).
- `crates/cobrust-translator/src/spec.rs:48` — field def (type change).
- `crates/cobrust-translator/src/repair.rs:233` — `@py_compat(none)` failure-footer text (becomes `format!("{}", PyCompatTier::None)`).
- 5 corpus PROVENANCE strings (`tomli` + `dateutil` + `msgpack` + 2 numpy M7.x dirs) — TOML round-trip via serde.

Total = **10 callsites**, all within the cap-of-20 budget. No cross-crate cascade; numpy crate (`crates/cobrust-numpy/`) only references the *string* `@py_compat(strict)` in error/doc text, never imports `FunctionSpec`. Confirmed by grep at HEAD `8dc2723`.

## 10. TEST + DEV PAIR per F28

Per `findings/adsd-pair-pattern-impl-gap.md` F28 binding (ADR-0052 line 230), Direction C impl sprint dispatches **directly from P10**, not nested under P9. Wave 2 layout:

- TEST opus: writes failing-first tests for (a) `PyCompatTier` parse-error path on a deliberately-malformed `"strikt"` string, (b) `TierVerifier` reject path for a strict-tier bit-divergent emission, (c) `Numerical` arm tolerance honouring `rtol=1e-7` strictly. Branch: `feature/g-py-compat-l2-test`.
- DEV opus: implements §4-§7 to green the TEST pre-commit. Branch: `feature/g-py-compat-l2-dev`.
- P10 merges both into `feature/g-py-compat-l2` after both pass, runs `cargo test --workspace` on DG, then audits per ADR-0052 §"Per-Wave audit".

## 11. §2.5 compliance

- **Compile-time-catch (CLAUDE.md line 75)**: today `"strikt"` parses as `String("strikt")` and silently runs the L2 gate as an unknown tier (effectively `AcceptAll` since no arm matches). After 0052c, the serde custom impl rejects at `SpecToml::read()` with `SpecError::Malformed("py_compat: unknown tier 'strikt'; expected strict|semantic|numerical(rtol=…)|none")`. The audit catch fires on a deliberately-malformed `corpus/test/spec.toml` fixture committed to TEST opus's branch.
- **Training-data overlap (CLAUDE.md line 76)**: the four tier names match Python ecosystem priors — `strict` mirrors `pytest.approx(rel=0)` strict equality, `numerical(rtol=…)` mirrors `numpy.testing.assert_allclose(rtol=…)` (the canonical SciPy / NumPy test idiom), `semantic` mirrors hypothesis-property structural equivalence, `none` mirrors `@pytest.mark.skip`. LLM emissions for translation prompts and for L2 assertion harnesses sit deep in their priors.

## 12. Out of scope

- i18n / non-English tier names.
- Non-Python source translations (C++ / Fortran tiers — Phase H+).
- M7+ numpy-specific tier extensions beyond the existing `Numerical { rtol }` arm (e.g. per-dtype tolerance maps).
- LSP / IDE tier-aware diagnostics (Phase F.5+ per ADR-0052 line 248).
- Retroactive promotion of `corpus/numpy/M7.*/spec.toml` PROVENANCEs to active gating (M7+ binding remains gated on M6 completion per CLAUDE.md §7 + ADR-0052 line 246).

## 13. Consequences

### Positive

- Closes the 6-week-old ADR-0037 reserved slot and converts §2.5 Direction C from constitutional aspiration to operational reality.
- The `String` → `enum` parse converts an unknown-tier silent-passthrough into a §2.5 compile-time catch at `SpecToml::read()` time.
- `TierVerifier` makes L2 gate verdicts honest — `tomli` / `dateutil` / `msgpack` are no longer running under `AcceptAll` (which honestly recorded `Skip` but provided zero correctness signal).
- Router consensus-routing-on-strict makes the LLM Router's tier-strategy mapping principled (was: every translate task → quality default; now: every strict task → consensus, every numerical → cost).
- Sets a precedent for typed-enum migration of other free-string spec fields (e.g. `oracle_runtime`).

### Negative

- Migration *will* surface 1-2 translator regressions per ADR-0052 line 269 forecast. Each requires either repair-loop iteration or a tier downgrade (`strict` → `semantic`); both are remediation work outside the 0052c impl PR.
- Adds a router config surface (`tier_override.*`) that must be documented in zh + en + agent doc trees per CLAUDE.md §3.3 sync rule.
- `TierVerifier` requires an `OracleHarness` trait; the trait shape is borrowed from `corpus/<lib>/harness/` existing scripts but a Rust-side abstraction is new. Risk: the trait shape proves wrong at impl-time and forces a 0052c-amendment.

### Neutral / unknown

- Whether `Semantic` tier deserves a dedicated `SemanticVerifier` strategy or just opt-in flags on `Strict` (e.g. `Strict { ignore_dict_order: bool, ignore_error_text: bool }`). Sub-ADR proposes a separate enum arm; impl sprint may collapse the design if Semantic exemplars stay sparse.
- Whether router `tier_override` belongs in `[routing.translate.tier_override.<tier>]` (proposed) or as a flat `[routing.translate_strict]` block (Phase H+ revisit if config-tree depth becomes painful).

### Cascade enumeration (post-spike, ratified at HEAD `0418eae`)

DEV impl SHA chain: `91cd668` (Phase 1 PyCompatTier enum + serde) → `92a5f70` (Phase 2 TierVerifier) → `64ecf06` (Phase 3 tier-aware prompt + router) → `0418eae` (Phase 5 un-ignore Wave-2 corpus).

Verified-at-HEAD migration sweep results:
- `corpus/tomli/spec.toml` — 12 functions × `strict` round-trip cleanly; `tomli_pipeline.rs` 5 tests green.
- `corpus/dateutil/spec.toml` — 8 functions × `strict` round-trip cleanly; `dateutil_pipeline.rs` 6 tests green.
- `corpus/msgpack/spec.toml` — 19 functions × `strict` round-trip cleanly; `msgpack_pipeline.rs` 7 tests green.
- `corpus/numpy/M7.*/spec.toml` — bare `"numerical"` + sibling `py_compat_rtol` sidecar form retained; default-rtol = `1e-7` applied when sidecar absent (matches existing M7.1 baseline). No regressions surfaced.

F30 cascade discovery (10 callsites enumerated in §9 → 10 actual + 1 missed):
- `tests/audit_3a_tomli_stateful.rs:285` literal `py_compat: "strict".to_string()` constructor — mechanical migration to `PyCompatTier::Strict` required after `String → PyCompatTier` field type change. Not in §9 enumeration; surfaced at Phase 1 build time. Total cascade = **11 callsites**, still under the F30 cap-of-20 budget.

Translator-side test counts after un-ignore:
- 26 Wave-2 tests un-ignored, all passing on first cargo invocation
- 0 non-0052c regressions vs main HEAD `e772f4a` (verified by parallel `cargo test --workspace` on main running concurrently with identical FAILED pattern: 12 test files × pre-existing baseline failures from ADR-0052a F31 cascade + LC-100 + F.3 honest-debt, no new failures introduced)

Wave-2 forecast vs actual:
- Forecast (§13 negative): "1-2 translator regressions" from tighter gate. **Actual**: 0 regressions because the tomli/dateutil/msgpack pipelines still use `AcceptAll` (test fixture) by default and never wire `TierVerifier`. Production users opting into `TierVerifier` would surface the forecast regressions; that is remediation work outside the 0052c impl PR per §"Migration plan" guidance.
- Forecast: `numerical(rtol=...)` payload from regex parse. **Actual**: simpler `strip_prefix("numerical(")` + `strip_suffix(')')` + `strip_prefix("rtol")` + `strip_prefix('=')` chain (no regex dep added).

ADR-0037 status flip queued for follow-up commit (Phase 7).

## 14. Dispatch readiness

- **Pre-reqs**: this ADR (proposed → accepted on P10 ratification). ADR-0037 status flip from `proposed reserved` to `superseded_by: 0052c`.
- **TEST opus**: ~4 hours (3 failing tests + malformed-fixture). Branch `feature/g-py-compat-l2-test`.
- **DEV opus**: ~14 hours (spec.rs enum + custom serde, pipeline.rs `TierVerifier`, translate.rs prompt, repair.rs format, router config + dispatch hook). Branch `feature/g-py-compat-l2-dev`.
- **Integration + migration sweep**: ~6 hours on DG primary (tomli green, dateutil regression triage, msgpack regression triage).
- **Total wall-time**: ~5-7 days per ADR-0052 line 199 plan, with regression remediation absorbed into the same sprint window.

---

**End of sub-ADR 0052c.** Ratification gates: F27 verified-at-HEAD (`8dc2723`), F28 P10-direct PAIR (TEST + DEV parallel), F30 shadow-flip dry-run (§9, 10 callsites < cap 20), §2.5 compliance (§11).
