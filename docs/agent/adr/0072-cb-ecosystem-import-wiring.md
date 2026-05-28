---
doc_kind: adr
adr_id: 0072
title: .cb ecosystem-import wiring — privileged cobra-module namespaces over the intrinsic/C-ABI/static-link chain
status: accepted
date: 2026-05-28
last_verified_commit: aeb3f5a
relates_to: [adr:0019, adr:0028, adr:0050c, adr:0071, "claude.md:§2.5"]
---

# ADR-0072: `.cb` ecosystem-import wiring

## 1. Context

The rebranded translated-library crates (`coil`/`den`/`pit`/`strike`/`scale`/`molt`/
`nest`/`hood`, ADR-0071) are **Rust + PyO3 only**: a `.cb` program cannot `import coil`
and call `coil.eye(3)`. User-prioritized 2026-05-28 (task #150) as the foundational
unlock making the ecosystem real for Cobrust + the Z.8 REST-demo prerequisite.

**Current-mechanism map** (design investigation 2026-05-28):
- `.cb` reaches stdlib as **flat PRELUDE builtins** (`cobrust-frontend/src/prelude.rs`)
  → cli intrinsic-rewrite retargets the call's MIR `Call.func` to
  `Constant::Str("__cobrust_<fn>")` (`cobrust-cli/src/build/intrinsics.rs`)
  → codegen declares the extern (`llvm_backend.rs declare_runtime_helpers`)
  → runtime C-ABI shim `#[no_mangle] extern "C"` in `cobrust-stdlib`, linked as the
  single `libcobrust_stdlib.a` by `cc` in `cobrust-cli/src/build.rs`. There is NO
  `std.<mod>` dotted namespace at source level.
- `import` is parsed + HIR-bound (`DefKind::ImportAlias`) then **dropped**: MIR no-ops
  `ItemKind::Import`; the typechecker gives the alias a fresh `Var`; `coil.eye(3)`
  attribute access returns an unconstrained var → unresolved. So import is a no-op today.
- The ecosystem crates have **zero** C-ABI shims, **zero** intrinsic/extern entries, and
  **no static-link path** (`crate-type = ["rlib","cdylib"]`, cdylib for PyO3 only).

## 2. Decision — privileged built-in cobra-module namespaces over the proven chain

Reuse the proven flat-intrinsic chain, keyed on `(ecosystem-module-alias, attr)` instead
of a flat name — NOT a new `cobrust-pkg`-driven package-import/dynamic-link path.

- **Type side**: in `cobrust-types` `check.rs`, when an `ExprKind::Attr`/`synth_call` base
  is a `Name` bound to an `ImportAlias` resolving to a known ecosystem module, look up
  `(module, attr)` in an **ecosystem manifest** for the signature (twin of the existing
  `try_synth_method_call` table dispatch).
- **MIR side**: an ecosystem arm in the intrinsic-rewrite retargets `coil.eye(args)` →
  `Constant::Str("__cobrust_coil_eye")` (identical to the `STR_JOIN` retarget).
- **Codegen**: declare the ecosystem-shim externs over `{i64, f64, opaque_ptr}` in
  `declare_runtime_helpers`; emit handle-drop calls at scope exit.
- **Link**: each imported ecosystem crate gains `staticlib` crate-type (→ `lib<mod>.a`);
  `build.rs` links ONLY the archives for modules the program actually imports.

## 3. Open-question decisions (Q1–Q6)

- **Q1 (namespace mechanism)**: **built-in privileged namespaces** — hardcode the 8 cobra
  modules in the toolchain now (they are the canonical, workspace-vendored ecosystem; no
  Python alias per ADR-0071). A general third-party package-import→link path is deferred
  to post-v0.7.0; it can layer on without reworking this.
- **Q2 (manifest)**: a **Rust table in `cobrust-types`** (new `ecosystem` module):
  `module → { fn → (params:[CbTy], ret:CbTy, py_compat_tier) }` + handle-type defs + their
  drop symbols. Not PRELUDE-stub injection (handle types + tiers don't express as stub fns)
  and not yet crate-generated (generation deferred — risk: manifest drift, accepted for now).
- **Q3 (handle modeling)**: **nominal handle types** (a `Ty::Adt`-like nominal per handle,
  e.g. `den.Connection`/`den.Cursor`) bound to a drop symbol, reusing the non-`Copy`
  drop-scheduled path. Chosen over a generic `Ty::Opaque(drop_symbol)` for per-method
  compile-time type safety (CLAUDE.md §2.5 compile-time-catch).
- **Q4 (drop-obligation transfer)**: first proof keeps handles **scope-local** (no
  return/store/capture escape) → drop fires once at scope exit via the existing Str/List
  non-Copy drop schedule. Escape-transfer (move/borrow) semantics are a tracked follow-up.
- **Q5 (archive + crate-type)**: add `"staticlib"` to the ecosystem crate's `crate-type`
  (keep `cdylib` for PyO3 — cargo permits multiple); archive `lib<mod>.a`; per-import link
  in `build.rs` (drive off the resolved-import set). Ecosystem shims that need str-prims
  (`__cobrust_str_*`) declare them `extern "C"` and bind from the always-linked
  `libcobrust_stdlib.a` (no Rust-level dep on cobrust-stdlib).
- **Q6 (@py_compat propagation)**: each manifest entry records its tier (from the crate's
  `@py_compat`/PROVENANCE); binds to the L2 verifier per §2.5-C (verifier wiring deferred;
  record the tier now — den first proof = `strict`).

## 4. First proof — `den` end-to-end (recommended over `coil`)

`den`'s marshalling (opaque `Connection`/`Cursor` handles + scalar/text cells) is the
minimal interesting case; `coil.Array` (n-dim dtype/shape/strides ABI) is an order harder
and gets its own follow-up sub-ADR.

**Milestone program** (smallest slice proving every layer):
```
import den
let conn = den.connect(":memory:")
let cur = conn.execute("CREATE TABLE t(x INTEGER)")
let _ = conn.execute("INSERT INTO t VALUES (42)")
let rows = conn.execute("SELECT x FROM t").fetchall()
print(rows)
```
**Minimal shims**: `__cobrust_den_connect(*Str)->*Connection`,
`__cobrust_den_connection_execute(*Connection,*Str)->*Cursor`,
`__cobrust_den_cursor_fetchall(*Cursor)->*Str` (Str rendering for the first proof;
row→list[tuple] is the immediate follow-up), `__cobrust_den_connection_drop`,
`__cobrust_den_cursor_drop`.

**Done-means**: (1) type-checks against the manifest, no `AmbiguousType`; (2) MIR shows
`__cobrust_den_*` retargeted callees; (3) `cc` links `prog.o + cobrust_main.o +
libcobrust_stdlib.a + libden.a`, no unresolved symbols; (4) the binary opens `:memory:`,
CREATE/INSERT/SELECT, prints `42`, exits 0; (5) no leak/UAF — handle drops fire once at
scope exit (verify via a drop-count assertion in the shim or ASan).

## 5. Risks

1. **Cross-boundary handle drop scheduling (prime)** — schedule `__cobrust_den_*_drop`
   exactly once at the right scope exit. Str/List drop is the template, but a *foreign
   nominal opaque type with a foreign drop symbol* is new. First proof avoids handle
   escape to bound this.
2. **`!Send` handles** — den's `Rc<RefCell<>>` handles are single-threaded; must not cross
   into spawned tasks. First proof is single-threaded; mark the constraint.
3. **Link bloat** — link only imported modules' archives (thread the import set to build.rs).
4. **Manifest drift** — hand-maintained signatures can desync; generation deferred.
5. **`coil.Array` ABI** — deferred; needs its own marshalling sub-ADR.

## 6. Plan

Per-layer files: `cobrust-types/src/check.rs` + new `ecosystem` manifest; `cobrust-cli/src/
build/intrinsics.rs` (retarget arm); `cobrust-codegen/src/llvm_backend.rs` (externs + drop);
`cobrust-cli/src/build.rs` (`locate_ecosystem_archive` + per-import link); `cobrust-den`
(new `src/cabi.rs` shims + `staticlib` crate-type). Implement the `den` first proof
(§4) → paired audit → then row→list marshalling, handle-escape rules, `coil.Array` ABI,
and the remaining modules generalize off the proven chain.
