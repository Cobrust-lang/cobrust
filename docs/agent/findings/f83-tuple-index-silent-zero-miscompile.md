---
finding_id: F83
title: tuple `t[i]` constant-index silently miscompiles to 0 (lowering stub) — RESOLVED by ADR-0106 (REDO; attempt-1 reverted for breaking the __cobrust_tuple_* fixture)
date: 2026-06-13
resolved_date: 2026-06-15
status: resolved
severity: major
discovered_by: verify-the-gap idiom probe (2026-06-13), the str/bytes/list indexing-arc continuation
resolved_by: adr:0106
relates_to: ["finding:f81", "finding:f78", adr:0094, adr:0096, adr:0106, "claude.md:§2.2", "claude.md:§5.2"]
---

## RESOLVED (2026-06-15, ADR-0106 — the REDO)

The struct lowering is RE-APPLIED on current main AND the revert cause is
RECONCILED. `Ty::Tuple` lowers to a real by-value LLVM struct
(`lower_ty`); construction via `build_insert_value`
(`lower_aggregate_tuple`); `t[i]` constant-index (Python-negative
normalised) reads `Projection::Field(off)` via `build_extract_value`
(`lower_place_load`). The checker rejects a non-literal index AND a
constant-OOB index (`TypeError::NotIndexable`, §2.5-A). The inference fixes
(`llvm_scalar_ty(Tuple)` = struct, `llvm_operand_ty` resolving a `Field(i)`
projection to the i-th FIELD type) keep a str field's dest correctly typed.

**Reconciliation (the load-bearing REDO step):** the obsolete pointer-ABI
fixture `llvm_emits_tuple_end_to_end`
(`crates/cobrust-codegen/tests/llvm_wave3_dict_set_tuple.rs`) — which
hand-built a `Ty::Tuple` local and passed it BY VALUE to ptr-typed
`__cobrust_tuple_new/set/get/drop` externs, the exact source of the
attempt-1 LLVM-verify failure — is RETIRED (the `__cobrust_tuple_*` model
is superseded by the struct model; no real path emits it; the real surface
is covered by `crates/cobrust-cli/tests/tuple_e2e.rs`). The
`__cobrust_tuple_*` STDLIB functions + their `collections.rs` unit test
REMAIN (no longer wired to `Ty::Tuple` codegen). Verified with
`cargo test -p cobrust-codegen --features llvm` GREEN (the crate that
reverted attempt-1) AND `cargo test --workspace --locked`.

**Honest-debt carry-over:** a tuple-owned `str`/`list` field still LEAKS on
tuple drop (codegen no-op; F82-class) — memory-SAFE leak-or-free-once,
never double-free. The `tuple_e2e_07` test proves NO double-free + value
correctness only; it does NOT claim drop-balance (documented in ADR-0106
§Ownership and the test comment).

# F83 — tuple `t[i]` silent-0 miscompile (attempt reverted)

## What

`(7, "x")[0]` builds OK + runs returning `0` (CPython `7`). The type
checker (`check.rs:~2233`, `(Ty::Tuple, IndexKind::Expr)` +
`resolve_tuple_index`) types `t[i]` correctly (per-position element type
for a literal index), but the MIR lowering returns a STUB
`Constant::Int(0)` (`lower.rs:~845` `IndexKind::Tuple(_) => Int(0)`; the
`ExprKind::Index` rvalue path has no `Ty::Tuple` branch). A §2.2 silent
miscompile, untested (no tuple_e2e existed), the tuple analogue of the
str/bytes/list indexing arc (F78/F79/F81).

## Attempt 1 — REVERTED (2026-06-13, workflow wsbsj4vfx, commits 9120c1b+86b5369 reverted by 6bb3bce+0b4d155)

The fix made `Ty::Tuple` a REAL by-value LLVM struct across 3 layers
(check.rs per-position fold + reject const-OOB/non-literal; lower.rs
`Projection::Field`; codegen `lower_ty` struct + insert/extract_value).
BOTH 2-lens audits SHIP'd and a local `cargo test -p cobrust-cli` was
green — BUT that scope was WRONG: CI runs **`cargo test --workspace
--locked`**, and the broad `Ty::Tuple → struct` change BROKE an existing
**cobrust-codegen** test `llvm_emits_tuple_end_to_end`
(`llvm_wave3_dict_set_tuple.rs:146`):

```
LLVM module verify failed: Call parameter type does not match function
signature!  call void @__cobrust_tuple_set({i64,i64,i64} %load, ...)
```

There is a SEPARATE, pre-existing tuple codegen path that calls the
runtime ABI `__cobrust_tuple_set` / `_get` / `_drop` taking a tuple
**POINTER**; once `Ty::Tuple` became a by-value struct, codegen loaded the
struct and passed it BY VALUE to those `ptr`-typed externs → LLVM verify
failure (6 call sites). The two tuple models (new by-value struct vs old
pointer-ABI) are inconsistent. (CI also showed a macOS test-job failure +
a 4-hour ubuntu test-job HANG on the F83 commit — the hard codegen
failure + infra flake compounded.)

REVERTED to keep main green for the v0.7.0-rc1 tag; F83 work preserved on
branch `fix/f83-redo-tuple-struct` (@ 86b5369). The tuple silent-0
reverts to its pre-existing (now-documented) state.

## LESSON (F80 extended)

A change that touches a CORE TYPE's codegen representation (`Ty::Tuple` in
`lower_ty`/`llvm_scalar_ty`) has WORKSPACE-WIDE blast radius. CTO-verify +
the workflow Build's local verify MUST run **`cargo test --workspace
--locked`** (the exact CI command), NOT `-p cobrust-cli` — the broken test
lived in `cobrust-codegen`, invisible to a cli-only sweep. Sibling of the
F80 "global-render change needs the full sweep" lesson, widened to
"core-type-repr change needs the full WORKSPACE sweep".

## Redo direction (next sprint)

Reconcile the tuple model: EITHER (a) update the `__cobrust_tuple_*`
call-site codegen to pass the struct by POINTER (the alloca), consistent
with the new by-value struct + extract_value reads — and confirm the
`llvm_emits_tuple_end_to_end` fixture + any real path; OR (b) if the
`__cobrust_tuple_*` runtime ABI is fully obsoleted by the struct model,
remove that dead codegen path + retire/update the synthetic fixture. Then
run `cargo test --workspace --locked` (not just -p cobrust-cli) before
merge. Branch `fix/f83-redo-tuple-struct` has the working struct lowering
to build on.
