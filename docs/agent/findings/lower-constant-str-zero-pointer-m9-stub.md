---
doc_kind: finding
finding_id: lower-constant-str-zero-pointer-m9-stub
last_verified_commit: 49009a8
dependencies: [adr:0050c]
discovered_by: post-Wave-2 audit teammate 2026-05-16 (a15e69b315007f341), F-W2-4 — surfaced by recovery DEV agent during ADR-0050c Phase 2 cascade investigation
severity: P1
status: closed_by_65a5335
related: [comp-lowering-zero-sentinel-collision, fstring-hole-mir-type-dispatch, adr-cross-surface-bug-fix-scope-creep]
---

# Finding: `lower_constant(Constant::Str(_))` returned zero pointer for non-callsite paths (M9-era stub)

## Hypothesis

Pre-existing M9-era codegen stub at `crates/cobrust-codegen/src/cranelift_backend.rs::lower_constant` returned the zero pointer (`0_i64`) for any `Constant::Str(_)` rvalue that wasn't materialized by a callsite-aware path (intrinsic-rewrite, FormatString Aggregate, etc.). User code patterns like `let v: str = "literal"` + `print(v)` or `echo("hello")` user-fn calls fell through to this stub, producing silent miscompile: null buffer at the binding, blank stdout when the user expected "hello".

## Method

Empirically discovered during list[str] DEV recovery (agent `a2056acb07469204f`) while diagnosing Phase 2 literal Str materialization. The TEST corpus's `let v: str = "x"; print(v)` cases produced empty stdout. Inspection of the codegen path traced back to `lower_constant` at the M9-era stub site.

## Result

Fixed at commit `65a5335` (Wave 2 list[str] DEV recovery cascade fix #3). Per the post-Wave-2 audit Lane 3:

> Pre-existing from M9-era; covered by callsite-aware materialize paths for the existing corpus, but `let v: str = "literal"` + `echo("literal")` user-fn calls fell through to the stub. Pre-fix: silent null-buffer printing nothing.

The bug was masked pre-Wave-2 because:
- `print("literal")` is intrinsic-rewritten at the call site (works pre-fix).
- f-string holes use the FormatString Aggregate materialization path (works pre-fix).
- The narrow surviving case was `let v: str = "literal"` (let-binding form) + subsequent `print(v)` / `echo(v)` (user-fn-call form). No corpus test exercised this combination until ADR-0050c list[str] DEV recovery surfaced it.

## Conclusion

**Severity P1** in retrospect: user-visible silent miscompile of a basic language idiom that pre-existed unnoticed for the entire M9..F.2 era. Closed by the recovery merge `aca5d87` (containing `65a5335`).

**Filed as finding** per post-Wave-2 audit F-W2-4 recommendation so future impl agents working in the lower_constant neighborhood see this trap. The fix shape (route Str literals through the heap-Str allocation path, not the zero-pointer stub) is reusable design guidance.

## Cross-references

- `crates/cobrust-codegen/src/cranelift_backend.rs::lower_constant` — fix site (per merge `aca5d87`, commit `65a5335`).
- `[[../adr/0050c-str-ownership.md]]` — ADR-0050c §"Consequences" did not enumerate this consumer; surfaced as cascade.
- `[[adr-cross-surface-bug-fix-scope-creep.md]]` — F29 candidate; this finding is a concrete instance of the cascade-bug surface.
- `[[predicate-flip-cascade-discovery-deficit.md]]` — F30 candidate (filed alongside this); names the structural pattern.
