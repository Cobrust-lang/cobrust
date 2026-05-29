---
doc_kind: finding
finding_id: F69
title: §2.5 borrow / error-surface debt — handle-method forced `&a` (anti-numpy training-data) + build.rs raw-Debug MIR-error bypass
status: partial
date: 2026-05-29
last_verified_commit: 608a43f
relates_to: [adr:0051, adr:0052a, adr:0052b, finding:F38]
resolution_commit: "(§B fix lands in the SAME commit that introduces this finding — fix(cli) route MIR errors through error_ux; SHA not self-citable pre-commit, per the f65 by-description convention. 608a43f was the doc-only base and does NOT contain the fix.)"
resolves: ["F69 §B — build.rs raw-Debug MIR-error bypass of error_ux renderer"]
---

> **PARTIAL (2026-05-29).** Two CLAUDE.md §2.5 deficits on the
> borrow / error surface. **§B (build.rs raw-Debug MIR-error bypass) is
> RESOLVED here** — `mir_lower`'s error now routes through the
> `error_ux::UserError` renderer instead of `format!("MIR error: {e:?}")`.
> **§A (handle methods force explicit `&a`) is DEFERRED** — it is a
> language-surface design change (HIR/type-layer auto-borrow for method
> receivers), not a CLI-layer rendering fix, and is tracked here as the
> open §2.5 Direction-A debt item.

# F69: §2.5 borrow / error-surface debt

## Summary

The retro-audit of the post-Phase-G CLI surface surfaced two distinct
violations of CLAUDE.md §2.5 ("Cobrust is the language LLM agents write
correctly on the first try"), both on the borrow / ownership error
surface:

- **§A — handle methods force an explicit `&a` borrow** at the call site
  (anti-numpy-training-data deficit; §2.5 Direction-A). DEFERRED.
- **§B — `build.rs:120` dumped the raw `{e:?}` Debug repr of `MirError`**
  to stderr, bypassing the `error_ux` renderer that prints the polished
  fix-suggestion (§2.5 Direction-B "print the FIX, not just the
  diagnosis"). RESOLVED in this commit.

The two share a root theme: the ownership system is Cobrust's biggest
LLM-friendliness lever (per the LC-100 honest-debt baseline cited in
ADR-0051), so both its *call-site ergonomics* (§A) and its *error
feedback loop* (§B) directly govern whether an LLM gets ownership right
ex-ante. §B is the cheaper, fully-reversible CLI-layer fix and is
closed now; §A is a language-surface change that needs its own ADR.

## §A — handle methods force explicit `&a` (DEFERRED)

### Deficit

ADR-0052a shipped the explicit `&s` shared-borrow surface so a value
read without consuming is written `str_len(&s)` rather than `str_len(s)`
(which moves). This is correct and was the LARGEST LC-100 friendliness
win at the time. But it leaves a residual asymmetry that cuts against
§2.5 Direction Maximize-overlap-with-training-data:

- In Python/numpy training data, a method-style read on a handle is
  `a.sum()` / `arr.reshape(...)` / `s.split(",")` — **no sigil**. The
  receiver is borrowed implicitly.
- In Cobrust today, the equivalent read-only use of an owned handle in
  a *function-call* position must be spelled `f(&a)` to avoid a move.
  An LLM trained on the numpy/Python corpus writes `f(a)` first, hits a
  `UseAfterMove`, and only then learns to insert `&`.

The §2.5-aligned target: for **method-call receivers** (ADR-0050f Phase
G method-form path; §2.5 Direction-D) and for read-only handle uses, the
borrow should be inferred — the LLM writes `a.len()` / `len(a)` and the
HIR/type layer auto-borrows when the use is non-consuming. This is the
`&`-elimination direction ADR-0051 names as Phase G P0 Direction-A
("eliminates `clone()` clutter; the LARGEST current LLM-friendliness
deficit").

### Why deferred (not fixed here)

- It is a **language-surface change** in the HIR / type / MIR layers
  (auto-borrow inference for non-consuming receiver/argument positions),
  NOT a CLI rendering fix. It belongs in a dedicated ADR (an ADR-0052a
  amendment or a new auto-borrow ADR), with the full
  compile-time-catch + differential-test discipline §2.5 + §6 demand.
- It is **not reversible at zero cost** the way §B is — it changes what
  source the compiler accepts, so it needs the well-typed / ill-typed
  curated-suite treatment (M2 discipline) before it can ship.
- Scoping it into this retro-audit fix would couple a one-line CLI
  rendering correction with a multi-crate language-semantics change —
  exactly the kind of scope-coupling F35-sibling warns against.

### Promotion criteria

§A promotes from this finding to its own ratified ADR when an
auto-borrow / receiver-borrow-inference design lands. Until then this
finding stays `status: partial` carrying §A as the open §2.5
Direction-A debt item. The `error_ux` `From<MirError>` `UseAfterMove`
suggestion (`change to \`&s\` to borrow without consuming`) is the
*interim mitigation*: when the LLM does hit the move, §B (below) now
guarantees it reads a clean, actionable fix rather than a Debug dump.

## §B — build.rs raw-Debug MIR-error bypass (RESOLVED)

### The bug

`crates/cobrust-cli/src/build.rs` line 120 (pre-fix):

```rust
let mut mir = mir_lower(&typed)
    .map_err(|e| BuildError::Type(format!("MIR error: {e:?}")))?;
```

`mir_lower` runs `borrow_check` internally
(`crates/cobrust-mir/src/lower.rs:56,64`), so a source-level ownership
violation returns a structured `MirError` — e.g.

```text
UseAfterMove { local: 2, span: Span { file: FileId(0), start: 10, end: 12 }, suggestion: Some("change to `&s` to borrow without consuming (ADR-0052a explicit shared borrow)") }
```

The `{e:?}` formatter dumped **that entire raw Debug struct** to stderr.
This violates two contracts at once:

1. **`error_ux.rs` lines 1-6 contract** — *"The raw internal
   representation (3000-line Cranelift IR, `{:#?}` debug dumps, etc.)
   never reaches the terminal."* The `MirError` Debug repr is exactly
   such an internal representation.
2. **CLAUDE.md §2.5 Direction-B** — error messages MUST print the FIX.
   The Debug dump buries the actionable `suggestion` string inside
   `Some(...)` alongside internal field names (`local`, `span`,
   `FileId`) the user cannot act on.

The irony: `error_ux.rs` lines 981-1096 ALREADY has a complete
`impl From<MirError> for UserError` that maps every `UseAfterMove` /
`ConflictingMutBorrow` / `EscapingBorrow` / … variant to a clean
`UserError::Type { msg, hint }`, lifting the construction-time
`suggestion` into the rendered `hint:` line and preserving the source
span via `span_to_line_col`. The `build.rs` call site simply never
called it — it stringified eagerly with `{e:?}` before the structured
error could reach the renderer. Every adjacent stage (parse / HIR / type
errors via `cobrust check`, and `UseAfterMove` via the snapshot corpus)
routed cleanly; only the `build`-path MIR error leaked.

### The fix

`build.rs` now converts the structured error through the renderer and
carries the rendered text in `BuildError::Type`:

```rust
let mut mir = mir_lower(&typed).map_err(|e| {
    BuildError::Type(crate::error_ux::UserError::from(e).to_string())
})?;
```

The same one-line routing is applied to the `lower_to_mir` programmatic
helper (build.rs, formerly `format!("MIR: {e:?}")`).

This is the **smallest correct** fix and mirrors how the adjacent errors
render:

- `From<MirError> for UserError` already exists — no new conversion
  logic, no `MirError` shape change, no span back-fill needed (the
  variants already carry `span`, and the From impl already maps it).
- The print sites (`build::run` `eprintln!("cobrust build: {e}")`,
  `run::run` `eprintln!("cobrust run: {e}")`, `pkg_build`) consume
  `BuildError`'s plain `Display` (= the inner string), so the rendered
  `UserError` text surfaces verbatim, exactly once (no double-render:
  `From<BuildError>` is not in any of those print paths).
- Compiler-internal `MirError` variants (`UnresolvedDefId`, `Internal`)
  route through `UserError::internal` inside the From impl, preserving
  the bug-report path — they are NOT user-source-fixable and must not
  masquerade as Type errors.

### Before vs after (empirical, `consume(s)` then `str_len(s)`)

Before (raw Debug):

```text
cobrust build: MIR error: UseAfterMove { local: 2, span: Span { .. }, suggestion: Some("change to `&s` ..") }
```

After (`NO_COLOR=1`):

```text
cobrust build: error[Type]: use of moved value `_2` after it was moved
  --> <source>:1:0
  hint: change to `&s` to borrow without consuming (ADR-0052a explicit shared borrow)
```

Exit code unchanged: `2` (TYPE_ERROR per ADR-0024).

### Regression coverage

Two guards land with the fix:

- **Unit test** `build::tests::mir_error_routes_through_error_ux_not_raw_debug`
  (in `build.rs`) — constructs a `MirError::UseAfterMove`, runs it through
  the exact `UserError::from(e).to_string()` path, asserts the rendered
  `BuildError` contains the fix-suggestion (`&s`, `borrow without
  consuming`) + the human message (`use of moved value`) and does NOT
  contain `UseAfterMove {`. Immune to the source-surface borrow-check gap.
- **E2E test** `e0069_use_after_move_renders_fix_suggestion_not_raw_debug`
  (in `tests/borrow_phase_g_e2e.rs`) — builds a real `.cb` that triggers
  `UseAfterMove` through `cobrust build`, asserts the same on the
  process's actual stderr (with `NO_COLOR=1` for deterministic match).

### Why the E2E goes through `build`, not `check`

The `error_ux_snapshot.rs` snap_03 case is `#[ignore]`d because
`cobrust check` stops at `type_check` and never runs MIR-lowering, so it
misses the cross-statement use-after-move (documented gap:
"intra-block borrow-checker does not cover `let zs = xs` then
`print(xs)` across statements"). The `build` path DOES run `mir_lower` →
`borrow_check`, so the F69 E2E (move into a by-value fn param, then read
again) fires reliably and needs no `#[ignore]`.

## Related findings

- **F38** (source-surface leakage of codegen internal primitive) —
  sibling §2.5 surface-leakage pattern; F38 fossilized a codegen
  primitive into the user surface, F69 §B fossilized a codegen-internal
  error repr into user output. Both are "internal detail reaching the
  LLM-facing surface" §2.5 violations.
- **F35-sibling** (commit-msg vs diff drift) — the resolving commit's
  subject scopes to the actual diff (a §2.5 error-rendering fix + 3
  finding docs), not the broader §A deferral.

## Evidence

- `crates/cobrust-cli/src/build.rs` (the `mir_lower` call site + the
  `lower_to_mir` helper, both now routed).
- `crates/cobrust-cli/src/error_ux.rs` lines 1-6 (contract), 981-1096
  (`From<MirError> for UserError`).
- `crates/cobrust-mir/src/lower.rs:56,64` (`borrow_check` invoked inside
  `lower`).
- `crates/cobrust-mir/src/borrow.rs:114,227` (the two `UseAfterMove`
  construction sites + their `suggestion`).
- `crates/cobrust-cli/tests/error_ux_snapshot.rs:155-186` (snap_03
  `#[ignore]` rationale — why the E2E uses `build` not `check`).
- `docs/agent/adr/0051-llm-first-design-principle.md` (§2.5 Directions
  A + B).
- `docs/agent/adr/0052a-explicit-shared-borrow.md`,
  `docs/agent/adr/0052b-error-ux-fix-suggestions.md`.
