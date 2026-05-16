---
doc_kind: finding
finding_id: fstring-hole-mir-type-dispatch
last_verified_commit: 49009a8
dependencies: [adr:0044, adr:0050c]
discovered_by: post-Wave-2 audit teammate 2026-05-16 (a15e69b315007f341), F-W2-5 — surfaced by recovery DEV agent during ADR-0050c Phase 2 cascade investigation
severity: P1
status: closed_by_2e9d456
related: [lower-constant-str-zero-pointer-m9-stub, adr-cross-surface-bug-fix-scope-creep]
---

# Finding: f-string Str holes misdispatched through `__cobrust_fmt_int` (W2 Phase 3 pre-existing bug)

## Hypothesis

Pre-existing W2 Phase 3 (ADR-0044) bug at `crates/cobrust-codegen/src/cranelift_backend.rs::lower_aggregate_format_string`. The f-string hole dispatch branched on the Cranelift IR value type (which is `i64` for all heap pointers, including Str pointers) rather than on the MIR-declared type. Result: a Str-typed value in an f-string hole would route to `__cobrust_fmt_int(buf, ptr_as_i64)`, which formatted the heap pointer as a decimal integer.

Symptom: `f"first={xs[0]}"` where `xs: list[str]` produced output like `"first=2199025418400"` (the pointer printed as decimal).

## Method

Empirically discovered during list[str] DEV recovery (agent `a2056acb07469204f`) while diagnosing why f3ls08-f3ls10 e2e tests (f-string over list[str] elements) produced large-integer stdout instead of the expected string content.

## Result

Fixed at commit `2e9d456` (Wave 2 list[str] DEV recovery cascade fix #5). The fix shape: consult MIR-declared type FIRST (`mir_ty` from `body.locals[place.local.0].ty` or the operand's tracked MIR type), then fall back to Cranelift value type. When MIR says Ty::Str, route through `__cobrust_fmt_str(buf, ptr, len)` after extracting (ptr, len) via the existing `__cobrust_str_ptr` + `__cobrust_str_len` accessors.

The bug was masked pre-Wave-2 because:
- Pre-list[str], the only Str values in f-string holes came from `input()` / `argv()` / literal forms — all of which were tested with f-strings producing readable output ("hi", "world", etc.). The pointer-as-decimal would have been technically wrong but visually plausible if no test pinned the exact expected output.
- ADR-0050c §"Consequences" tagged f-string lowering as "also-fixed transitively" but the actual fix required a dispatch-by-MIR-type rewrite, not just lifetime tracking. The transitive claim was over-optimistic.

## Conclusion

**Severity P1** in retrospect: user-visible miscompile of f-strings containing Str-typed holes (which is the most common f-string user pattern). Closed by the recovery merge `aca5d87` (containing `2e9d456`).

**Filed as finding** per post-Wave-2 audit F-W2-5 recommendation. The fix shape — "consult MIR type FIRST, then Cranelift value-type fallback" — is reusable design guidance for any future hole-aware dispatch. For dict impl (Wave 3), `f"{d[k]}"` with `d[k]: Ty::Str` must follow the same pattern; pre-flight should add a regression test.

## Cross-references

- `crates/cobrust-codegen/src/cranelift_backend.rs::lower_aggregate_format_string` — fix site (per merge `aca5d87`, commit `2e9d456`).
- `[[../adr/0050c-str-ownership.md]]` §"Consequences" — over-tagged this as "also-fixed transitively"; the actual fix was a dispatch rewrite.
- `[[../adr/0044-stdin-argv-source-binding.md]]` — W2 Phase 3 origin of the buggy dispatch shape.
- `[[lower-constant-str-zero-pointer-m9-stub.md]]` — sibling P1 cascade bug filed alongside.
- `[[predicate-flip-cascade-discovery-deficit.md]]` — F30 candidate names the structural pattern.
