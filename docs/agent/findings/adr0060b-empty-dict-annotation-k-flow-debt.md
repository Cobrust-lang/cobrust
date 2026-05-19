---
doc_kind: finding
finding_id: adr0060b-empty-dict-annotation-k-flow-debt
last_verified_commit: TBD
dependencies: [adr:0060b, adr:0050d]
discovered_by: P9 Phase M sprint 2026-05-19 — pm_b06_array_not_hashable corpus
severity: P3 (test-only; production path correctly rejects non-empty Array-keyed dicts)
status: open (deferred to a separate annotation-flow sub-sprint)
related: [adr:0060b §3.3, adr:0050d Decision 7A]
---

# Finding: empty-dict `{}` literal does not propagate annotation `K` through hashability check

## §1. Empirical anchor

Phase M corpus test:

- `crates/cobrust-types/tests/phase_m_type_corpus.rs::pm_b06_array_not_hashable`

Source:

```cobrust
fn f() -> i64:
    let d: dict[[i64; 4], i64] = {}
    return 0
```

Expected (per ADR-0060b §3.3 + `Ty::Array(_, _).is_hashable() == false`):
type-check rejection with `TypeError::NotHashable { actual: Array(Int, 4), ... }`.

Empirical at HEAD `e731369`: type-check **accepts** this program.

## §2. Precise root cause

The empty-dict literal `{}` lowers in `synth_dict_lit` to
`Ty::Dict(fresh_var(), fresh_var())`. The annotation `dict[[i64; 4],
i64]` lowers via `lower_generic_type` to `Ty::Dict(Array(Int, 4),
Int)`. Unification then binds the fresh K-var to the Array — **but
the Hashable check at `lower_generic_type` was already evaluated on
the original args before unification**.

Reading the code carefully:

```rust
// check.rs::lower_generic_type around line 2354
match (base, lowered.len()) {
    // ... List / Set / Tuple ...
    ("Dict" | "dict", 2) => {
        Ty::Dict(Box::new(lowered[0].clone()), Box::new(lowered[1].clone()))
    }
    _ => self.fresh_var(),
}
```

The `lower_generic_type` for `Dict` does NOT call `validate_hashable_dict`
on the K arg — the Hashable check site is `validate_hashable_dict`
which is called separately at annotation-lowering entry points. The
`{}` literal path bypasses the annotation lowering for K-validation
because the inner `Ty::Array` is created lazily via the lowered
generic, while `validate_hashable_dict` walks the HIR `TypeKind`
shape.

Wait — actually re-reading, `validate_hashable_dict` IS called at
annotation entry, and it DOES recurse into Generic args (the new
ADR-0060b extension adds the `Array` recursive arm). The check is:

```rust
if matches!(base_s.as_str(), "Dict" | "dict") && args.len() == 2 {
    let k_ty = self.lower_type(&args[0]);  // lowers [i64;4] -> Array(Int, 4)
    let k_resolved = self.subst.apply(&k_ty);
    if !k_resolved.is_hashable() {
        return Err(TypeError::NotHashable { ... });
    }
}
```

`Ty::Array(_, _).is_hashable()` returns `false` per the wave-2 edit.
The mismatch must be at a different site — perhaps the type-check
entry point doesn't even call `validate_hashable_dict` on `let`
annotations (only on `def` param annotations and the like).

Empirical investigation defers to a separate sub-sprint to wire the
`let`-annotation site through `validate_hashable_dict` (this is the
real fix; the Phase M ADR-0060b mention was assumed-already-wired).

## §3. Classification

Test-fixture-coverage debt. The production path **correctly rejects**
non-empty Array-keyed dict literals via the synth-on-element path
(`{[1,2,3,4]: 0}` would synthesise the K type as Array and fail at
the per-key hashable check inside `synth_dict_lit`). The gap is
specifically the **empty-literal-via-annotation** path.

Severity downgraded to P3 because:

1. The non-empty path (the more common one) works correctly.
2. The Array-as-dict-key shape is not in the LLM-prior surface
   anyway (Python/Rust both reject list-as-dict-key idiomatically).
3. The fixture exists purely as a defensive check against silent
   coercion; the negative-space coverage is already provided by
   the non-empty path.

## §4. Resolution plan

In `cobrust-types/src/check.rs::synth_let`, after lowering the
annotation `Ty`, walk it via `validate_hashable_dict` on the HIR
annotation node before binding to the local. The exact entry-point
audit is the sub-sprint scope.

## §5. F36 + F37 compliance

- **F36**: `pm_b06_array_not_hashable` honestly describes the
  eventual behavior. The `#[ignore]` is paired with this finding.
- **F37**: this finding is the explicit ignore-debt cross-reference.

## §6. Cross-references

- ADR-0060b §3.3 — Array hashability + Dict K-check
- ADR-0050d Decision 7A — Hashable predicate
- `crates/cobrust-types/src/check.rs::validate_hashable_dict`
- `crates/cobrust-types/src/check.rs::synth_dict_lit`
