---
name: f54
status: RESOLVED
family: F53-sibling
date: 2026-05-26
last_verified_commit: 9aec0fc
---

# F54 — FMTSPEC precision-spec stripped payload not interned (X.3 flip surface)

## §1 Context

Surfaced 2026-05-26 during ADR-0070 §X.3 LLVM-default flip (P10 takeover after opus 529). With LLVM as default backend, 4 `f64_e2e` fixed-precision f-string tests failed:
`f64e13_fstring_fixed_2_decimals` / `f64e14_fstring_fixed_4_decimals` / `f64e15_fstring_zero_decimals` / `f64e33_circle_area_print_fixed_2`.

Error: `internal codegen error: str payload ".2f" not interned; intern_str_payloads pre-pass bug`.

## §2 Root cause

`lower_aggregate_format_string` (F53 impl) correctly routes float holes with a trailing `FMTSPEC:.2f` sentinel to `__cobrust_fmt_float_prec(buf, v, spec_ptr, spec_len)`, materializing the **stripped** spec (`.2f`). But `intern_str_payloads` pre-pass only interned the **full** operand payload (`FMTSPEC:.2f`), never the stripped form. `str_data_ptr_for(".2f")` then panicked: payload not in the interned global table.

F53 closed the aggregate-lowering caller gap but its companion intern pre-pass missed the stripped-spec sub-payload. F53-sibling.

## §3 Resolution

`intern_str_payloads` `push_unique` closure extended: when a payload `starts_with("FMTSPEC:")`, also intern the stripped suffix. One-site fix in `llvm_backend.rs`. Post-fix `f64_e2e` 33/33 PASS.

## §4 Detection rule

The X.2 sweep + F53 verification both ran on Cranelift-default (LLVM opt-in) state where this path wasn't the default-exercised one. **X.3 flip is itself the detection gate** — flipping default surfaces all latent LLVM-only paths. F35-sibling lesson: feature-default-flip must run FULL workspace test (not just per-helper fixtures) to catch intern/codegen interaction gaps.

## §5 Cross-refs

- F53 (parent — lower_aggregate List+FormatString gap)
- F45a (sub-wave-5 over-claim ancestor)
- ADR-0070 §X.3 (LLVM-default flip)

## §6 Status

RESOLVED 2026-05-26 via X.3 flip sprint (P10).
