---
name: "0062"
title: fix-safety ladder on diagnostic variants
status: proposed
phase: Phase G+ (extends ADR-0052b)
relates_to: [adr:0052b, adr:0057a]
date: 2026-05-19
author: CTO (P10)
competitive_source: docs/agent/strategy/competitive-intel-zero-language.md §3.2
---

## §1 Motivation

### 1.1 The Problem: LLM Agents Don't Know Which Fix is Safe to Apply

ADR-0052b added a `suggestion` field to TypeError variants — a human-readable fix hint. ADR-0057a wires TypeError diagnostics into the LSP `publishDiagnostics` response with a `code_action` array.

The gap: **the LLM agent cannot determine unattended whether it is safe to auto-apply a suggested fix.**

Consider three TypeError variants with existing `suggestion` fields:
- `TypeError::ImplicitTruthiness { actual: Int }` → suggestion: `"change to 'if x != 0:'"` — safe to apply automatically (no behavior change except honoring the language rule)
- `TypeError::ReturnTypeMismatch { expected, actual }` → suggestion: `"add explicit conversion or change return type"` — MAY change public API (safe only if function is private)
- `TypeError::PublicApiBreakingChange { symbol }` → suggestion: `"add migration shim"` — requires human review; downstream code may break

An LLM agent today must infer safety tier from the error message text — brittle, model-dependent, and wrong on edge cases.

### 1.2 Zero Language Empirical Inspiration

Zero (vercel-labs/zero 0.1.3) ships structured diagnostic JSON with an explicit `fix_safety` field:

```json
{
  "code": "TYP041",
  "severity": "error",
  "fix_safety": "behavior-preserving",
  "suggestion": "change 'if x:' to 'if x != 0:'"
}
```

Zero's taxonomy: `format-only` / `behavior-preserving` / `local-edit` / `api-changing` / `target-changing` / `requires-human-review`.

This field is consumed by Zero's VS Code extension to gate code-action auto-apply. Cursor and Copilot consume it via the LSP code-action `kind` field.

Cobrust SHOULD adopt this pattern, adapted to Cobrust's six-tier taxonomy. See `docs/agent/strategy/competitive-intel-zero-language.md §3.2` for full competitive context.

### 1.3 ADR-0052b Extension

ADR-0052b introduced the `suggestion` string on TypeError variants and the `Suggestion { message, span }` struct. This ADR extends that struct with a `fix_safety: FixSafety` field. No breaking change to existing code — `fix_safety` defaults to `FixSafety::RequiresHumanReview` on any variant not yet tagged, which is conservatively safe.

---

## §2 §2.5 LLM-First Audit

| §2.5 rule | Assessment |
|---|---|
| Compile-time-catch-errors | Indirect: fix_safety enables auto-apply of compile-error fixes at the correct tier. Reduces LLM "is this safe?" inference overhead. |
| Maximize-overlap-with-training-data | Fix-safety terminology (`behavior-preserving`, `api-changing`) appears in Python refactoring tooling literature at reasonable frequency. No novel syntax introduced. |

Key §2.5 alignment: **LLM agents need a machine-routable signal to decide which fix is safe to apply unattended.** Today that signal is absent. Every auto-apply decision is a guess. `FixSafety` makes the compiler's intent explicit.

---

## §3 Scope

### 3.1 The `FixSafety` Enum

New type in `crates/cobrust-types/src/error.rs` (or `crates/cobrust-errors/src/lib.rs` — finalize in impl):

```rust
/// Safety tier for a suggested fix, from the compiler's perspective.
/// Consumers (LSP code-actions, LLM agents) gate auto-apply on this field.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FixSafety {
    /// Whitespace / formatting only. Never changes semantics.
    /// Auto-apply unconditionally.
    FormatOnly,
    /// Semantically equivalent rewrite within the function body.
    /// Auto-apply if no downstream tests fail.
    BehaviorPreserving,
    /// Changes confined to a single call-site or binding.
    /// Auto-apply with caution; may require adjacent test update.
    LocalEdit,
    /// Changes the public API of a function or type.
    /// Auto-apply only in agent-mode with explicit user confirmation or
    /// if all call sites are in-crate.
    ApiChanging,
    /// Changes target platform, ABI, or linking contract.
    /// Never auto-apply.
    TargetChanging,
    /// Semantic ambiguity or migration risk beyond compiler's ability to assess.
    /// Always requires human review before apply.
    RequiresHumanReview,
}

impl Default for FixSafety {
    fn default() -> Self {
        FixSafety::RequiresHumanReview
    }
}
```

### 3.2 Extend `Suggestion` Struct (ADR-0052b)

Current shape (ADR-0052b):
```rust
pub struct Suggestion {
    pub message: String,
    pub span: Option<Span>,
}
```

New shape:
```rust
pub struct Suggestion {
    pub message: String,
    pub span: Option<Span>,
    pub fix_safety: FixSafety,     // NEW — defaults to RequiresHumanReview
    pub replacement: Option<String>, // NEW — machine-applicable text replacement
}
```

`replacement` is the literal text to substitute at `span`. When `Some`, the LSP code-action can apply it directly. When `None`, the fix requires agent reasoning.

### 3.3 Variant Tagging — TypeError

All TypeError variants with existing `suggestion` fields receive explicit `fix_safety` tagging. Non-exhaustive representative tagging:

| Variant | FixSafety | Rationale |
|---|---|---|
| `ImplicitTruthiness { actual }` | `BehaviorPreserving` | `if x != 0:` is semantically equivalent for all defined `actual` types |
| `MissingReturnType` | `LocalEdit` | Add return annotation; no caller impact |
| `UnusedVariable` | `FormatOnly` | Prefix with `_`; no semantic change |
| `ReturnTypeMismatch { expected, actual }` | `LocalEdit` | Return-type change confined to function; API impact only if `pub` |
| `PublicApiBreakingChange` | `RequiresHumanReview` | Downstream breakage risk |
| `CloneHint` | `LocalEdit` | Add `.clone()` call; localized |
| `BorrowConflict` | `LocalEdit` | Restructure borrow scope; localized |
| `MutableDefaultArgument` | `BehaviorPreserving` | Compiler-mandated fix; Python semantics never matched intent |

### 3.4 Variant Tagging — MirError + LoweringError

All MirError and LoweringError variants with `suggestion` fields receive the same treatment. Tagging follows the same principle: if the fix changes observable behavior across a module boundary → `ApiChanging` or `RequiresHumanReview`; if fix is local → `BehaviorPreserving` or `LocalEdit`.

### 3.5 LSP Wire Shape (ADR-0057a integration)

In `cobrust-lsp`, when emitting a `CodeAction` for a diagnostic with `Suggestion`:

```rust
let kind = match suggestion.fix_safety {
    FixSafety::FormatOnly => CodeActionKind::SOURCE_FIX_ALL,
    FixSafety::BehaviorPreserving => CodeActionKind::QUICK_FIX,
    FixSafety::LocalEdit => CodeActionKind::QUICK_FIX,
    FixSafety::ApiChanging => CodeActionKind::REFACTOR,
    FixSafety::TargetChanging | FixSafety::RequiresHumanReview => {
        // Surface as diagnostic info only, not auto-apply code action
        return None;
    }
};
```

---

## §4 Implementation Plan

### 4.1 Step Sequence

1. Define `FixSafety` enum + `Default` impl in `crates/cobrust-types/src/error.rs`
2. Extend `Suggestion` struct with `fix_safety` + `replacement` fields
3. Update all `Suggestion { message, span }` constructors to supply `fix_safety` — compiler will error on missing fields (structural guarantee)
4. Tag ~35 TypeError variants with correct `FixSafety` tier (bulk of the work)
5. Tag MirError variants
6. Tag LoweringError variants
7. Extend `cobrust-lsp` CodeAction emission to gate by `fix_safety`
8. Extend JSON diagnostic output (`--emit=json`) to include `fix_safety` in `"suggestion"` object
9. Unit tests (§6)
10. CI lint gate (§7)

### 4.2 LOC Estimate by Component

| Component | LOC |
|---|---|
| `FixSafety` enum + `Default` + `serde` derives | ~30 |
| `Suggestion` struct extension | ~15 |
| TypeError variant tagging (~35 variants) | ~210 |
| MirError variant tagging (~10 variants) | ~60 |
| LoweringError variant tagging (~8 variants) | ~48 |
| LSP CodeAction gating | ~50 |
| JSON emit extension | ~30 |
| Unit tests | ~120 |
| CI lint gate (build.rs or proc-macro) | ~40 |
| **Total** | **~600 LOC** |

---

## §5 Non-Goals

- **NO LSP code-action auto-apply UI changes**: consumer-side auto-apply behavior (e.g., VS Code "apply all safe fixes on save") is out of scope. This ADR only defines the wire signal. The client decides what to do with it.
- **NO semantic analysis to determine FixSafety automatically**: tagging is hand-annotated by the compiler engineer who adds each variant. No inference — inference would be unreliable.
- **NO external tools integration**: language server protocol only. No integration with rustfix, cargo fix, or other external fix-application tools in this ADR.
- **NO `fix_safety` field on warnings without `suggestion`**: only variants that already have or newly receive a `suggestion` field are tagged. Warnings with no fix suggestion are unaffected.

---

## §6 Acceptance Gates

Five unit tests in `crates/cobrust-types/tests/test_fix_safety.rs` (or collocated under `#[cfg(test)]`):

| Test | Assertion |
|---|---|
| `test_fix_safety_default_is_requires_human_review` | `FixSafety::default() == FixSafety::RequiresHumanReview` |
| `test_fix_safety_format_only_is_lowest_tier` | `FixSafety::FormatOnly < FixSafety::BehaviorPreserving` (Ord impl) |
| `test_fix_safety_requires_human_review_is_highest_tier` | `FixSafety::RequiresHumanReview > FixSafety::TargetChanging` |
| `test_fix_safety_serde_roundtrip` | `serde_json::from_str::<FixSafety>(r#""behavior-preserving""#) == Ok(FixSafety::BehaviorPreserving)` |
| `test_suggestion_has_fix_safety` | Construct a `Suggestion` with `fix_safety: FixSafety::LocalEdit`; assert `suggestion.fix_safety == FixSafety::LocalEdit` |

All five tests MUST pass. Additionally, `cargo build` MUST produce zero warnings after tagging (the missing-field compiler error is the primary enforcement mechanism for the tagging requirement).

---

## §7 Risk Register

| Risk | Mitigation |
|---|---|
| **Tagging drift — new TypeError variant added without fix_safety** | Struct field is non-optional — `Suggestion { message, span }` must become `Suggestion { message, span, fix_safety, replacement }`. Any code constructing `Suggestion` without all fields is a compile error. No proc-macro needed. |
| **Wrong tier assigned to a variant** | Unit test coverage for the most safety-critical variants (format-only and requires-human-review ends of the spectrum). Code review gate: any `ApiChanging` or `TargetChanging` tagging requires explicit reviewer confirmation in PR. |
| **LSP code-action consumer ignores fix_safety** | Out of scope for this ADR. The wire signal is correct; downstream consumers are their own ADRs (ADR-0057a series). |
| **`replacement: Option<String>` not filled for many variants initially** | Acceptable. `None` means the fix is descriptive only. Fill incrementally; do NOT block ADR-0062 acceptance on 100% `replacement` coverage. |
| **`FixSafety` Ord usage for tier comparison** | Derive `PartialOrd + Ord` — variant declaration order IS the tier order (FormatOnly = lowest, RequiresHumanReview = highest). Enforce this with `test_fix_safety_format_only_is_lowest_tier`. |

---

## §8 Consequences

### Positive
- LLM agents can route fix application by `fix_safety` field with zero inference — machine-routable signal
- LSP code-actions correctly gate auto-apply (ADR-0057a integration)
- JSON diagnostic output becomes richer for CI integration (e.g., auto-apply `FormatOnly` fixes in CI)
- Compiler team has explicit discipline for classifying new error variants

### Negative
- ~35 existing `Suggestion` constructors must be updated — mechanical but non-trivial
- Possibility of over-conservative tagging (`RequiresHumanReview` used as a default escape hatch reduces signal quality over time)

### Neutral
- No change to existing Cobrust language semantics
- No change to public-facing error message text (only the structured field changes)

---

## §9 Open Questions (resolve in impl)

1. Does `FixSafety` live in `cobrust-types` or a new `cobrust-diagnostics` crate? Prefer `cobrust-types` to avoid a new crate dependency.
2. Should `replacement: Option<String>` be a newtype `Replacement(String)` to distinguish from `message: String`? Prefer newtype for public API clarity.
3. Should `FixSafety` implement `Display` for the JSON field name (`"behavior-preserving"`)? Yes — the `serde(rename_all = "kebab-case")` handles serialization; add `Display` for CLI `--emit=text` output.
