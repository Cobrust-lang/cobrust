---
module_id: cluster-c-fstring-precision-bug
last_verified_commit: feature/cluster-c-fstring-precision
status: RESOLVED
---

# Cluster C — F-string `:.Nf` Precision Interning Bug

## §1 Reproduction

Smallest source that triggers the bug:

```
fn main() -> i64:
    let x: f64 = 3.14159
    print(f"{x:.2f}")
    return 0
```

Build fails at codegen with: `str payload ".2f" not interned; codegen-time bug`

## §2 Actual vs Expected

- Expected stdout: `3.14`
- Actual: `cobrust build` exits non-zero; no binary produced
- Error site: `materialize_str_data(".2f")` at `cranelift_backend.rs` — the bare spec
  `.2f` is absent from `str_data_globals` at codegen time

## §3 Root Cause

Two-pass interning in `lower_function_body`:

1. **First pass** (`collect_str_payloads_from_rvalue`) walks all `Rvalue::Aggregate`
   operands including `FormatString` ones. It interns `FMTSPEC:.2f` correctly.

2. **Second pass** (lines 830-865) was intended to also intern the bare spec `.2f`
   (needed by `materialize_str_data` when `lower_aggregate_format_string` calls
   `__cobrust_fmt_float_prec`). However, the bare-spec extraction was **inside** the
   `if !already_interned` guard:

   ```rust
   if str_data_ids.contains_key(payload) {
       continue;   // ← short-circuits before bare-spec extraction
   }
   // ... intern sentinel ...
   if let Some(spec) = payload.strip_prefix("FMTSPEC:") {
       fmtspec_extra.push(spec.to_string());  // ← never reached
   }
   ```

   Because the first pass already interned `FMTSPEC:.2f`, the `continue` fired and
   `.2f` was never queued.

## §4 Fix

`crates/cobrust-codegen/src/cranelift_backend.rs` — second pass restructured to
extract bare specs unconditionally for all FMTSPEC-prefixed operands, before the
already-interned guard:

```rust
if let Some(spec) = payload.strip_prefix("FMTSPEC:") {
    if !spec.is_empty() {
        fmtspec_extra.push(spec.to_string());
    }
    continue; // sentinel already interned by first pass
}
if str_data_ids.contains_key(payload) {
    continue;
}
// ... intern non-FMTSPEC payload ...
```

## §5 Tests Cleared

| Test | Precision | Input | Expected |
|------|-----------|-------|----------|
| `f64e13_fstring_fixed_2_decimals` | `.2f` | 3.14159 | `3.14` |
| `f64e14_fstring_fixed_4_decimals` | `.4f` | sqrt(2.0) | `1.4142` |
| `f64e15_fstring_zero_decimals` | `.0f` | 3.7 | `4` |
| `f64e33_circle_area_print_fixed_2` | `.2f` | π×5² | `78.54` |

All 4 tests un-ignored and pass. No regressions in f64_e2e (33 pass / 2 pre-existing ignore).
