---
doc_kind: adr
adr_id: 0077
title: coil Buffer operator + index + attribute dispatch — the first ecosystem-handle operator design (a + b, a[i], a.shape, a.dot(b))
status: draft
date: 2026-05-29
decision_owner: cto
last_verified_commit: 936f13c
relates_to: [adr:0013, adr:0016, adr:0050c, adr:0050d, adr:0060a, adr:0060b, adr:0072, adr:0073, "claude.md:§2.2", "claude.md:§2.5"]
---

# ADR-0077: coil `Buffer` operator + index + attribute dispatch

## 1. Context

ADR-0072 wired the `.cb` ecosystem-import chain (L1 typecheck → L5 link) and made
`import coil` real for **value-handle calls**: `coil.zeros(3)`, `coil.eye(2)`,
`coil.mean(a)`, `coil.print_buffer(a)` (16 manifest rows at commit `936f13c`). ADR-0073
extended the chain to cross-boundary **callbacks** (the pit/hood/dora trampoline).

What the chain does **not** yet handle is the part of numpy that LLMs actually write:

```python
import coil
a = coil.ones(3)
b = coil.ones(3)
c = a + b          # operator dispatch — Q1
x = a[0]           # index read — Q2
a[0] = 9.0         # index write — Q2
s = a.shape        # attribute access (no parens) — Q3
d = a.dot(b)       # method-form op — Q5
```

Every coil sprint deferred this surface explicitly. The deferral note is recorded in
**three** places at HEAD, all pointing here:

- `crates/cobrust-coil/src/cabi.rs:43-51` — "Operator dispatch (`a + b`) and index
  dispatch (`a[i]`) are EXPLICITLY DEFERRED to a sub-ADR per ADR-0072 — those want their
  own design pass (the `EcoParam` manifest shape doesn't yet model binary operators, and
  the .cb-side BinOp dispatch needs a method-form lowering)."
- `crates/cobrust-types/src/ecosystem.rs:133-135` + `516-518` — same deferral on the
  `COIL_BUFFER_ADT` block + the manifest rows.
- `crates/cobrust-codegen/src/llvm_backend.rs:2844-2847` — same deferral on the extern
  block.

ADR-0072 §5 risk 5 ("`coil.Array` ABI — deferred; needs its own marshalling sub-ADR")
and the §"coil deep operator/index" boundary note are this ADR's scope. **This is the
biggest remaining numpy-gap surface** and — per CLAUDE.md §2.5 — the highest-value one,
because `a + b` and `a[i]` are the exact shapes LLMs emit from numpy training data.

**This ADR is DESIGN ONLY (doc, zero src).** It unblocks a future fill-in-the-blanks
impl sprint. It is also the **first ecosystem-handle operator** ADR — it sets the
precedent (Q1 mechanism, §6 Precedent) for any future handle that wants `+`/`[]`
(`decimal`, `fraction`, a matrix type).

### 1.1 Current-mechanism map (verified at `936f13c`)

The load-bearing finding for the Implementation map below: **operators and indexing are
NOT deeply baked into LLVM `iadd` with no retarget seam.** Both have a clean,
precedented insertion point at the MIR-lowering tail, and the existing Dict/List index
arms are a verbatim template. Three layers carry today's behavior:

- **Typecheck** — `cobrust-types/src/check.rs`:
  - `synth_bin` (check.rs:2426) — arithmetic arm (`Add|Sub|Mul|Div|...|MatMul`,
    check.rs:2437) `unify`s LHS/RHS then matches the resolved type. The accept set is
    `{Int, Float, Str, IntN(_), Var(_)}` (check.rs:2455); **everything else, including
    `Ty::Adt(COIL_BUFFER_ADT)`, falls into the `other =>` arm and is rejected with
    `TypeError::TypeMismatch`** (check.rs:2456). So `a + b` on two Buffers is a
    type-error *today* — Phase 1 MUST add a typecheck arm; it is not MIR-only.
  - `synth_expr` Index arm (check.rs:1280) and Attr arm (check.rs:1250) — the
    receiver-type-driven dispatch sites for `a[i]` and `a.shape`.
- **MIR** — `cobrust-mir/src/lower.rs`:
  - `lower_bin` (lower.rs:1968) — `and`/`or` short-circuit at 1979; `in`/`not in` Dict
    retarget at 1992-2035 (the **precedent guard**: `synth_expr_ty(self, rhs)` →
    `Ty::Dict` → emit `Terminator::Call` to `__cobrust_dict_contains_*`); the generic
    arithmetic path computes `lhs_op`/`rhs_op`/`bin_to_mir(op)` then emits
    `Rvalue::BinaryOp(mir_op, lhs_op, rhs_op)` **only at the tail** (lower.rs:2037-2073).
    A Buffer guard inserted *before* line 2037 (sibling of the `in`/`not in` guard)
    retargets cleanly.
  - `lower_expr` Index arm (lower.rs:1425) — **already** dispatches on `base_ty`: a Dict
    arm (1451) emits `Terminator::Call` to `__cobrust_dict_get_*`; a List arm (1480)
    emits `Terminator::Call` to `__cobrust_list_get` (+ `__cobrust_str_clone` for
    `list[str]`). A `Ty::Adt(COIL_BUFFER_ADT)` arm slots in beside them with the same
    shape. The fall-through at 1552 (`Projection::Index`) is the no-op legacy path we do
    NOT use for Buffers.
  - `try_lower_ecosystem_call` (lower.rs:1882) — the method-call retarget for
    `a.dot(b)` / `a.reshape(...)`; Q5 confirms this needs **no new mechanism**, only
    manifest rows (plus tuple-arg marshalling for `reshape`).
  - `emit_ecosystem_call` (lower.rs:1946) — the shared "declare `_ecoret` of the
    manifest return type, emit `Terminator::Call` with a `Constant::Str` symbol" helper;
    the Buffer-returning operator path reuses it verbatim.
- **Codegen** — `cobrust-codegen/src/llvm_backend.rs`:
  - `declare_runtime_helpers` (llvm_backend.rs:1190) — the coil extern block
    (2854-2895) declares `__cobrust_coil_*` over `{i64, f64, opaque_ptr}` fn types. A new
    operator/index extern is one more tuple in that loop.
  - Because the MIR retarget turns `a + b` into a `Terminator::Call`, codegen sees a
    normal runtime-helper call — **`lower_rvalue`'s `Rvalue::BinaryOp` arm
    (llvm_backend.rs:4126 → `lower_binop`) is never reached for Buffers.** No
    codegen-side type-switch on operands is needed. This is the whole reason Q1 picks the
    MIR-level retarget.
- **Runtime** — `cobrust-coil/src/cabi.rs` — new `__cobrust_coil_buffer_add` etc.
  trampolines, mirroring `broadcast_to`'s borrow-two-handles-return-fresh-handle shape
  (cabi.rs:262).

## 2. Decision (summary)

| # | Question | Decision (Yes pick) |
|---|---|---|
| Q1 | BinOp dispatch mechanism | **MIR-level retarget** — guard in `lower_bin` on `Ty::Adt(COIL_BUFFER_ADT)` → `Terminator::Call` to `__cobrust_coil_buffer_add`/`sub`/`mul` (sibling of the existing `in`/`not in` Dict guard). Typecheck adds a Buffer arm to `synth_bin`. NOT HIR `__add__` desugar; NOT a typeck operator-trait system. |
| Q2 | Index dispatch | **MIR-level retarget** in the `lower_expr` Index arm, beside the Dict/List arms. Phase 1: scalar read `a[i]` → `__cobrust_coil_buffer_getitem(a, i) -> f64`. `a[i] = v` write + slice deferred to Phase 2. `a[i]` returns **`f64` scalar** (Cobrust-honest; numpy's 0-d scalar is not a Cobrust type). |
| Q3 | Attribute access | **manifest handle-attribute table** (new `lookup_handle_attr`, twin of `lookup_handle_method`) keyed `(AdtId, attr)`. `a.shape` → `__cobrust_coil_buffer_shape(a) -> list[i64]`; `a.ndim`/`a.size` → `i64`. Returns an **owned `list[i64]`** (existing List drop-schedule), NOT a tuple/new handle. |
| Q4 | Broadcasting + dtype-mismatch | **Runtime `Result`-on-the-wire, panic-on-shape-error for Phase 1.** Cobrust static types carry no shape, so shape errors are runtime. Operator returns a plain `Buffer` (NOT `Result<Buffer, ShapeError>`) — the §2.5 ergonomic tension is resolved toward "looks like numpy"; a fallible `a.checked_add(b) -> Result` escape is a Phase 2 follow-up. dtype is the only compile-time-catchable axis and is deferred (Phase 1 is f64-only). |
| Q5 | Method-form ops (`dot`/`transpose`/`reshape`/`sum`) | **Reuse the ADR-0073 handle-method chain — no new mechanism.** `a.dot(b)` and `a.transpose()` are pure manifest rows. `a.reshape(tuple)` + `a.sum(axis=...)` DO need new arg marshalling (tuple-arg + keyword-arg over the C-ABI) — flagged as Phase 2/3 sub-work, not a Phase 1 blocker. |
| Q6 | Phased rollout | **Phase 1** (≤1-2 day): `a + b`/`a - b`/`a * b` same-shape elementwise (runtime shape-check) + scalar `a[i]` read + `a.shape`/`a.ndim`/`a.size`. **Phase 2**: slice `a[1:3]`, broadcasting, `a[i] = v` write, `a.dot`. **Phase 3**: axis-reductions (`a.sum(axis=)`), `a.reshape`/`a.transpose`. |

## 3. Q1 — BinOp dispatch mechanism

`a + b` where `a, b: coil.Buffer` must route to a runtime `__cobrust_coil_buffer_add`
instead of an LLVM `iadd` (and instead of today's `TypeError::TypeMismatch` reject).

### Options

- **(a) MIR-level retarget** — in `lower_bin`, before the generic
  `Rvalue::BinaryOp` tail (lower.rs:2037), add a guard:
  `if let Ty::Adt(id, _) = synth_expr_ty(self, lhs) and is COIL_BUFFER_ADT → emit
  Terminator::Call to the per-op coil symbol`. This is the **exact sibling** of the
  `in`/`not in` Dict guard already living at lower.rs:1992-2035, and reuses
  `emit_ecosystem_call`. Typecheck side: `synth_bin` gains a Buffer arm returning
  `coil_buffer_ty()` for `Add|Sub|Mul` (and rejecting `Div|Mod|Pow|...` in Phase 1 with a
  clear "operator not yet supported on coil.Buffer" suggestion). **(Yes)**
- **(b) HIR-level desugar** `a + b` → `a.__add__(b)` then route through the existing
  handle-method path. Rejected: (1) Cobrust has **no dunder-method protocol**
  (CLAUDE.md §2.2 drops monkey-patching + metaclasses; there is no `__add__` surface and
  inventing one is a language-level change far exceeding this ADR); (2) it would make the
  operator's runtime symbol depend on a synthesized method name, muddying the manifest;
  (3) it pulls a Python-runtime concept (operator overloading via dunders) into a
  statically-typed core that deliberately omits it.
- **(c) Typeck-level operator-trait resolution** — model a Rust-style `Add` trait,
  resolve `a + b` to a trait impl, lower the impl call. Rejected for Phase 1: no trait
  system exists for ecosystem handles, and building one is a multi-ADR effort. It is the
  *eventual* general answer (see §6 Precedent / §8 open questions), but the manifest +
  MIR-retarget mechanism is the correct **first** increment and a trait layer can later
  generate manifest rows without reworking MIR/codegen.

### Rationale

Option (a) fits the chain (it is the 7th use of the "retarget a source operation to a
`Constant::Str` runtime call" pattern: Dict get/contains, List get, ecosystem free-fn,
ecosystem method, str-clone, and now Buffer-op), keeps codegen **untouched**
(`lower_binop` is never reached for Buffers), and maximizes §2.5 training-data overlap
(`a + b` is verbatim numpy). It does require a typecheck arm — see §1.1; this is a small,
well-bounded edit, not a redesign.

**Manifest shape extension (Q1 precedent-setting):** the operator needs a manifest entry
so the MIR guard reads the runtime symbol + return type from one source of truth. Add a
`lookup_buffer_binop(op: BinOp) -> Option<EcoSig>` (or a `binop_capability` flag region
on the `COIL_BUFFER_ADT` block) returning `__cobrust_coil_buffer_add` etc. This is the
**first operator entry** in `ecosystem.rs`; §6 records how it generalizes.

## 4. Q2 — Index dispatch

`a[i]` (read) and `a[i] = v` (write) where `a: coil.Buffer`.

### Options for the read path

- **(a) MIR-level retarget in the Index arm** (lower.rs:1425), beside Dict/List → emit
  `Terminator::Call` to `__cobrust_coil_buffer_getitem(a, i) -> f64`. Typecheck: the
  Index arm (check.rs:1280) gains a Buffer-receiver case returning `Ty::Float`.
  **(Yes)**
- **(b) `Projection::Index` codegen path** (the fall-through at lower.rs:1552 +
  llvm_backend.rs:4239). Rejected: this path is a documented Wave-1 stub/no-op for
  dynamic indices (the same hazard ADR-0050c fixed for `list` — see lower.rs:1426-1432:
  it "surfac[es] as a segfault when the user actually consumes the result"). Buffers must
  not regress into it.

### Scope (scalar first, slice deferred)

- **Phase 1**: scalar index `a[0]` / `a[i]` only. First-proof shape mirrors coil's
  constructor first-proof discipline (cabi.rs §"First-proof scope").
- **Phase 2**: slice `a[1:3]` (returns a fresh `Buffer` view/copy) — needs a slice ABI
  (`start, stop, step` over the C-ABI) deferred to keep Phase 1 ≤1-2 day. The HIR already
  models `IndexKind` (hir/lower.rs:1402 `lower_index`), so the slice surface parses; only
  the lowering + runtime are deferred.

### Return type of `a[i]` (the Cobrust-honest answer)

numpy returns a **0-d numpy scalar** (`numpy.float64`) — an object that is both a scalar
and a 0-d array. Cobrust has **no such type** and inventing one violates §2.2 (one way to
do each thing) and §5.1 (elegant). Decision: **`a[i]` returns a plain `f64`** for the
f64-only Phase 1. This diverges from numpy (`type(a[0])` is `numpy.float64` there, `f64`
here) but is the honest, ergonomic, §2.5-aligned choice — LLMs write `x = a[0]` expecting
a usable number, and `f64` *is* a usable number that flows into arithmetic + `print`.
The divergence is recorded as a known divergence in the coil PROVENANCE manifest
(ADR-0072 Q6 tier discipline). A future dtype-generic coil would return the element type
(`i64` for an int buffer); the manifest-driven dispatch makes that a fill-in, not a
rework.

### Write path `a[i] = v` (Phase 2)

Assignment targets already have a lowering site: `lower.rs:594` handles
`ExprKind::Index { base, index }` as an assignment target (the `a[0] = v` LHS form). The
Buffer write retargets there to `__cobrust_coil_buffer_setitem(a, i, v) -> ()` (borrows
`a` mutably — the in-place mutation is sound because the `.cb` source owns the only handle
to that box; ADR-0072 Q4 scope-local discipline). Deferred to Phase 2 to keep Phase 1
read-only and small.

## 5. Q3 — Attribute access (`a.shape`, `a.dtype`, `a.ndim`, `a.size`)

These are `Attr`-on-handle with **no method-call parens** — `a.shape`, not `a.shape()`.

### Options for modeling handle-attributes

- **(a) New `lookup_handle_attr(receiver: &Ty, attr) -> Option<EcoSig>` table** in
  `ecosystem.rs`, the structural twin of `lookup_handle_method` (ecosystem.rs:652), keyed
  `(AdtId, attr)`. The MIR Attr arm (lower.rs:1412 — currently a `Projection::Field(0)`
  placeholder) gains a Buffer-receiver branch that retargets to
  `__cobrust_coil_buffer_shape(a)` etc. via `emit_ecosystem_call`. Typecheck Attr arm
  (check.rs:1250) consults the new table. **(Yes)**
- **(b) Treat `a.shape` as sugar for a zero-arg method `a.shape()`** and reuse
  `lookup_handle_method`. Rejected: it conflates two distinct source surfaces (numpy's
  `a.shape` is an attribute, `a.dot(b)` is a method); §2.5 training-data overlap is
  *higher* when `a.shape` (no parens) type-checks, because that is exactly what LLMs
  write. Collapsing them would force `a.shape()` and reject the idiomatic form.
- **(c) Codegen-side magic on `Projection::Field`** — special-case field 0 of a Buffer.
  Rejected: opaque, no compile-time type for the result, and duplicates the
  manifest-as-source-of-truth principle ADR-0072 Q2 established.

### Return types

- `a.shape` → **owned `list[i64]`** (`Ty::List(Box::new(Ty::Int))`). Reuses the existing
  List drop schedule (ADR-0050c) — the runtime allocates the list, the `.cb` scope drops
  it once. Chosen over (i) a **tuple** — Cobrust tuples are fixed-arity and shape rank is
  runtime-variable, so a tuple type cannot be assigned statically; and (ii) a **new
  `coil.Shape` handle** — over-engineered for Phase 1 (a list is directly indexable
  `a.shape[0]` + printable, which is what users want). numpy returns a tuple; the
  `list[i64]` divergence is recorded as a known divergence (same tier discipline as Q2).
- `a.ndim` → `i64`; `a.size` → `i64` — both by-value scalars, the simplest case.
- `a.dtype` → **deferred to Phase 2+** (returning a dtype needs either a `str` rendering
  `"float64"` or a `coil.Dtype` handle; Phase 1 is f64-only so `a.dtype` is uninteresting
  and would ship a constant). Flagged so the impl sprint does not silently widen scope.

## 6. Q4 — Broadcasting + dtype-mismatch semantics

When `a + b` have mismatched shapes, numpy **broadcasts** (e.g. `(3,) + (1,)`);
mismatched-beyond-broadcast (e.g. `(3,) + (4,)`) errors **at runtime** with a
`ValueError`.

### What can be caught at compile time (the §2.5 compile-time-catch lens)?

Be honest: **Cobrust's static types carry no shape**. `coil.Buffer` is a single
`Ty::Adt(COIL_BUFFER_ADT)` regardless of `(3,)` vs `(4,)` vs `(2,3)`. Therefore **all
shape errors are runtime errors** — there is no type-level rank/shape to check. This is a
genuine §2.5 limitation, recorded plainly. The only axis that *could* be compile-time is
**dtype** (an `i64` buffer + an `f64` buffer is a dtype mismatch), but Phase 1 is
**f64-only** so dtype-mismatch cannot arise yet; a future dtype-parameterized
`Ty::Adt(COIL_BUFFER_ADT, [dtype])` could make dtype-mismatch a compile error (§8 open
question).

### Error surface — `Buffer` or `Result<Buffer, ShapeError>`? (the §2.2 tension)

CLAUDE.md §2.2 makes `Result<T, E>` the default error path. But an operator that returns
`Result` is **ergonomically heavy** — `let c = (a + b)?` or worse `match a + b { ... }`
on every arithmetic line is exactly what LLMs do *not* write from numpy priors. This is a
real tension between §2.2 (Result-default) and §2.5 (write-it-like-numpy).

**Decision (Phase 1): the operator returns a plain `Buffer`; a shape mismatch
panics-and-aborts at runtime** via the existing `__cobrust_panic` shim (the same Q5
abort-on-error discipline ADR-0073 chose for the callback boundary;
llvm_backend.rs:1123). Rationale:

- §2.5 wins the tie for the **operator form** specifically: `c = a + b` must look like
  numpy or the entire surface fails its reason-to-exist. numpy itself raises (a Python
  exception, the moral equivalent of an abort) on shape mismatch — so panic-on-mismatch
  is *behaviorally* the closest honest match, not a §2.2 violation in spirit.
- §2.2 is honored by offering a **fallible escape hatch** in Phase 2: a method-form
  `a.checked_add(b) -> Result<Buffer, ShapeError>` (or `coil.add(a, b)` free-fn variant)
  for code that wants to handle the mismatch. The ergonomic default (`+`) aborts; the
  explicit method opts into `Result`. This mirrors how the Dict `d[k]` (panic) vs
  `d.get(k)` (safe) split was resolved in ADR-0050d Decision 2A — direct precedent.

This is the one place the surface knowingly trades §2.2's letter for §2.5's intent;
documented as such so a future audit does not flag it as drift.

## 7. Q5 — Method-form ops (`a.dot(b)`, `a.transpose()`, `a.reshape(...)`, `a.sum(axis=...)`)

These already have a home: the ADR-0073 handle-method chain (`lookup_handle_method` +
`try_lower_ecosystem_call` Case 2 + `emit_ecosystem_call`).

- **`a.dot(b)`** — receiver `Buffer`, one `Value(coil_buffer_ty())` arg, returns
  `coil_buffer_ty()` (or `Ty::Float` for the 1-D dot-product producing a scalar — Phase 2
  picks per-rank; the manifest can carry only one return type, so the first proof targets
  matrix `dot` returning a `Buffer` and notes the scalar 1-D case as a divergence /
  follow-up). **No new mechanism** beyond a manifest row + a `cabi.rs` trampoline +
  an extern declaration. Confirmed against the `broadcast_to(a, n)` precedent
  (cabi.rs:262) which already borrows-two-and-returns-fresh.
- **`a.transpose()`** — zero-arg, returns `Buffer`. Pure manifest row. No new mechanism.
- **`a.reshape(...)`** — **needs new arg marshalling.** `reshape((2, 3))` takes a *shape
  tuple/list*; the C-ABI today passes scalars + opaque handle pointers, not tuples. The
  honest first-proof shape is `a.reshape(rows, cols) -> Buffer` (two `Ty::Int` args, the
  proven scalar ABI), deferring variadic-rank `reshape(tuple)` until a tuple/list-arg
  marshalling sub-design lands (sibling of ADR-0072 Q5's deferred bytes-ABI). Flagged
  Phase 3.
- **`a.sum(axis=...)`** — **needs keyword-arg marshalling.** `axis=0` is a keyword
  argument; the ecosystem-call arg lowering (`collect_positional_args`, lower.rs:1904)
  takes *positional* args only today. First proof: `a.sum() -> f64` (full reduction, zero
  args, reuses the existing `coil.mean`/`std` scalar-return ABI verbatim); axis-reductions
  returning a `Buffer` are Phase 3, gated on keyword-arg marshalling.

**Conclusion:** `dot` + `transpose` + `sum()`-full need only manifest rows (Phase 2/3
fill-in). `reshape(tuple)` + `sum(axis=)` need a small arg-marshalling extension that is
its own bounded sub-work, NOT a Phase 1 blocker.

## 8. Q6 — Phased rollout (matched to "周→天")

Each phase: scope, done-means, layers touched, chain-generality expectation.

### Phase 1 (≤1-2 day) — the proof

**Scope:** `a + b` / `a - b` / `a * b` elementwise on **same-shape f64** buffers (runtime
shape-check, panic-on-mismatch per Q4) + scalar `a[i]` **read** → `f64` + `a.shape`
(`list[i64]`) / `a.ndim` (`i64`) / `a.size` (`i64`).

**Layers touched:** all five, but each by a small precedented edit —

1. **types/ecosystem.rs** — `lookup_buffer_binop(BinOp) -> Option<EcoSig>`;
   `lookup_handle_attr(&Ty, &str) -> Option<EcoSig>`; rows for `add`/`sub`/`mul`,
   `getitem`, `shape`/`ndim`/`size`.
2. **types/check.rs** — `synth_bin` Buffer arm (check.rs:2455 accept-set extension);
   Index arm Buffer case (check.rs:1280); Attr arm Buffer case (check.rs:1250) consulting
   `lookup_handle_attr`.
3. **mir/lower.rs** — `lower_bin` Buffer guard before line 2037 (new
   `try_lower_buffer_binop`); Index arm Buffer branch beside lower.rs:1451/1480; Attr arm
   Buffer branch at lower.rs:1412.
4. **codegen/llvm_backend.rs** — extern declarations in the coil block (after
   llvm_backend.rs:2895): `__cobrust_coil_buffer_add`/`sub`/`mul` (`ptr,ptr -> ptr`),
   `__cobrust_coil_buffer_getitem` (`ptr,i64 -> f64`), `__cobrust_coil_buffer_shape`
   (`ptr -> ptr` returning a list handle), `_ndim`/`_size` (`ptr -> i64`).
5. **coil/cabi.rs** — trampolines mirroring `broadcast_to` (borrow inputs, return fresh
   handle or scalar); `shape` allocates a `list[i64]` via the stdlib `__cobrust_list_*`
   externs (ADR-0072 Q5 cross-crate str/list-prim binding pattern).

**Done-means:** a `.cb` program `let a = coil.ones(3); let b = coil.ones(3); let c = a + b;
coil.print_buffer(c); let x = a[0]; print(x); let s = a.shape; print(s[0])` type-checks,
MIR shows `__cobrust_coil_buffer_add` + `__cobrust_coil_buffer_getitem` +
`__cobrust_coil_buffer_shape` retargeted callees, links, runs, prints `array([2, 2, 2]...)`
+ `1.0` + `3`, exits 0, and `coil::cabi::DROP_COUNT` shows every Buffer (a, b, c) + the
shape list dropped exactly once. ≥3 negative cases: `a + 1` (Buffer + Int — Phase 1
rejects with a clear suggestion), `a / b` (unsupported op — clear suggestion), `a[0] = 9`
(write — Phase 1 "deferred to Phase 2" diagnostic).

**Chain-generality expectation:** Phase 1 establishes the operator + index + attr
mechanism *generically* (the manifest tables are AdtId-keyed). A second handle wanting
`+`/`[]`/`.attr` (§6 Precedent) reuses all three tables with new rows + trampolines, no
mir/codegen rework.

### Phase 2 — broadcasting + slice + write + dot

**Scope:** broadcasting in `a + b` (shape `(3,) + (1,)`); slice read `a[1:3] -> Buffer`;
index write `a[i] = v`; `a.dot(b)`; the `a.checked_add(b) -> Result` fallible escape (Q4).
**Layers:** runtime (broadcast logic already in `cobrust-coil/src/broadcast.rs`); MIR
slice-ABI + assignment-target Buffer branch (lower.rs:594); manifest rows.
**Done-means:** broadcast + slice + write + dot E2E corpus passes; fallible escape returns
a real `Result`. **Chain-generality:** slice-ABI + assignment-target retarget are
reusable for any future indexable handle.

### Phase 3 — axis-reductions + reshape/transpose

**Scope:** `a.sum(axis=...)` / `a.mean(axis=...)` returning a `Buffer`;
`a.reshape(tuple)`; `a.transpose()`. **Layers:** keyword-arg + tuple-arg ecosystem-call
marshalling (the bounded sub-work from Q5); manifest rows; runtime (coil's
`reduce.rs`/`view.rs` already implement these). **Done-means:** axis-reduction + reshape
corpus passes. **Chain-generality:** keyword/tuple-arg marshalling unblocks every
ecosystem method with non-scalar args (pit/strike future surfaces benefit).

## 9. Implementation map (Phase 1 — fill-in-the-blanks for the impl sprint)

Exact files + functions a future Phase 1 impl sprint will touch. Line numbers are
anchors at `936f13c`; the impl sprint re-greps the named functions.

| Layer | File | Function / site | Edit |
|---|---|---|---|
| Manifest | `crates/cobrust-types/src/ecosystem.rs` | new `lookup_buffer_binop(op) -> Option<EcoSig>` (~after line 644) | rows `add`/`sub`/`mul` → `__cobrust_coil_buffer_{add,sub,mul}`, ret `coil_buffer_ty()`, tier `Semantic` |
| Manifest | `crates/cobrust-types/src/ecosystem.rs` | new `lookup_handle_attr(recv, attr) -> Option<EcoSig>` (twin of `lookup_handle_method` @652) | `shape` → `__cobrust_coil_buffer_shape`, ret `Ty::List(Int)`; `ndim`/`size` → `Ty::Int` |
| Manifest | `crates/cobrust-types/src/ecosystem.rs` | extend `lookup_handle_method` @652 | `getitem`-style row OR handle in MIR Index arm directly — pick: Index arm reads a dedicated `coil_buffer_getitem_symbol()` const (cleaner than a fake method name) |
| Typecheck | `crates/cobrust-types/src/check.rs` | `synth_bin` @2426, arith accept-set @2455 | add `Ty::Adt(COIL_BUFFER_ADT)` arm: `Add`/`Sub`/`Mul` → `Ok(coil_buffer_ty())`; other ops → `TypeError` w/ "operator not yet supported on coil.Buffer" suggestion |
| Typecheck | `crates/cobrust-types/src/check.rs` | `synth_expr` Index arm @1280 | Buffer-receiver → `Ty::Float` (scalar read) |
| Typecheck | `crates/cobrust-types/src/check.rs` | `synth_expr` Attr arm @1250 | Buffer-receiver → consult `lookup_handle_attr` |
| MIR | `crates/cobrust-mir/src/lower.rs` | `lower_bin` @1968, before generic tail @2037 | new `try_lower_buffer_binop(op, lhs, rhs)` guard on `synth_expr_ty(self, lhs) == Ty::Adt(COIL_BUFFER_ADT)` → `emit_ecosystem_call(sym, coil_buffer_ty(), [lhs, rhs])` (mirror `in`/`not in` guard @1992) |
| MIR | `crates/cobrust-mir/src/lower.rs` | `lower_expr` Index arm @1425, beside Dict @1451 / List @1480 | Buffer branch → `Terminator::Call __cobrust_coil_buffer_getitem(base, idx) -> f64` (borrow base via `upgrade_move_to_copy_handle`) |
| MIR | `crates/cobrust-mir/src/lower.rs` | `lower_expr` Attr arm @1412 | Buffer branch → `emit_ecosystem_call(shape_sym, list_i64, [recv])` |
| Codegen | `crates/cobrust-codegen/src/llvm_backend.rs` | `declare_runtime_helpers` coil block, after @2895 | extern decls: `_add`/`_sub`/`_mul` (`ptr,ptr->ptr`), `_getitem` (`ptr,i64->f64`), `_shape` (`ptr->ptr`), `_ndim`/`_size` (`ptr->i64`) |
| Runtime | `crates/cobrust-coil/src/cabi.rs` | new shims, mirror `broadcast_to` @262 | `__cobrust_coil_buffer_add`/`sub`/`mul` (borrow 2, shape-check or `__cobrust_panic`, return fresh box); `_getitem` (borrow, bounds-check, return `f64`); `_shape` (borrow, build `list[i64]` via stdlib `__cobrust_list_*`); `_ndim`/`_size` (borrow, return `i64`) |
| Runtime elementwise | `crates/cobrust-coil/src/` | reuse `broadcast.rs` / `ufunc.rs` / `array.rs` | elementwise add/sub/mul already exist on `Array`; the shim wires them |
| Build | `crates/cobrust-cli/src/build/intrinsics.rs` | the `__cobrust_coil_*` recognizer arm | confirm the new symbols match the existing coil prefix recognizer (likely already prefix-matched; verify) |
| Tests | `crates/cobrust-coil/src/cabi.rs` `#[cfg(test)]` + a new CLI E2E | mirror `broadcast_to_round_trip` + a `coil_ops_e2e.rs` | drop-once assertions + the §8 Phase-1 done-means program |
| Docs | `docs/{agent,human/zh,human/en}` coil module specs | add operator/index/attr surface rows | per CLAUDE.md §3.3 sync rule, in the impl commit |

**Honest difficulty read for the estimate:** the MIR/codegen dispatch is **NOT** harder
than expected — operators are not baked into `iadd` with no seam (§1.1). The retarget
slots into a precedented guard (`in`/`not in` @1992) and the Index arm already
multi-dispatches on base type. The *one* surprise vs a naive "MIR-only" estimate is that
**`synth_bin` rejects `Adt + Adt` today** (check.rs:2456), so the typecheck layer is a
mandatory Phase-1 touch (3 small arms). This keeps Phase 1 at ≤1-2 day, not sub-day. The
elementwise math itself is free (coil's `Array` already implements it). The only genuine
*new* runtime work is building a `list[i64]` from Rust for `a.shape` via the cross-crate
stdlib list externs (the ADR-0072 Q5 pattern, proven but not yet used by coil).

## 10. Precedent — the first ecosystem-handle operator

This is the **first ADR to give an ecosystem handle an operator/index/attribute surface.**
Until now every handle (den.Connection, strike.Response, pit.App, dora.Node, coil.Buffer)
was call/method-only. The mechanism this ADR establishes generalizes:

- **Operator generality:** the `lookup_buffer_binop` table + the `lower_bin` Buffer guard
  are AdtId-specific in Phase 1, but the *pattern* (typecheck arm accepting the handle Ty
  for an op + MIR guard retargeting to a per-(handle, op) runtime symbol) is reusable. A
  future `decimal.Decimal` or `fraction.Fraction` or a matrix type wanting `a + b` adds
  (i) a typecheck accept-arm, (ii) a `lookup_<handle>_binop` table, (iii) a MIR guard,
  (iv) trampolines. To make this clean, the impl sprint SHOULD factor the Buffer guard as
  a generic `try_lower_handle_binop(adt_id, op, ...)` dispatching on a manifest
  `binop_capability(adt_id, op) -> Option<symbol>` rather than hardcoding
  `COIL_BUFFER_ADT` — a one-line generalization that turns the next handle-operator into
  pure manifest rows. (pit/den/strike/dora will NOT need `+` — but `decimal`/`fraction`/
  a future matrix or complex type will, and this is the seam they plug into.)
- **Index generality:** the Index-arm Buffer branch sits beside Dict/List; a future
  indexable handle (`pandas`-like frame, a tensor type) reuses the same arm shape.
- **Attribute generality:** `lookup_handle_attr` is the reusable table for *any* handle
  with parens-free attributes (`response.status`-as-attr, a future `.columns`).

The §2.5 payoff: once this mechanism exists, every ecosystem handle that mirrors a
Python type with operators/indexing/attributes gets the LLM-correct surface for free
(manifest rows), and the compiler internals never change again.

## 11. §2.5 analysis — LLM-first scoring

Explicit scoring of how close each surface lands to numpy training-data shape, and where
Cobrust's Result-default / static-typing forces divergence.

| Surface | numpy shape | Cobrust Phase-1 shape | §2.5 overlap | Forced divergence |
|---|---|---|---|---|
| `a + b` | `a + b` | `a + b` | **1.0 — identical** | none at the surface (runtime panic-on-mismatch matches numpy's raise) |
| `a - b`, `a * b` | same | same | **1.0** | none |
| `a[i]` read | `a[i]` → `np.float64` | `a[i]` → `f64` | **~0.95** | result type is `f64` not a 0-d numpy scalar (invisible to most code; surfaces only on `type(a[0])`) |
| `a.shape` | `a.shape` → `tuple` | `a.shape` → `list[i64]` | **~0.9** | tuple → list (both index + print identically; `len`/unpack differ) |
| `a.ndim`, `a.size` | `a.ndim` → `int` | `a.ndim` → `i64` | **1.0** | none |
| `a.dot(b)` (Ph2) | `a.dot(b)` | `a.dot(b)` | **1.0** | none |
| `a[i] = v` (Ph2) | `a[i] = v` | `a[i] = v` | **1.0** | none |
| `a / b` (Ph1) | `a / b` | rejected w/ suggestion | **0.0 Ph1** | not-yet-supported (Phase ≥2); §2.5-B error UX prints the FIX |
| `a + 1` scalar bcast (Ph1) | `a + 1` | rejected w/ suggestion | **0.0 Ph1** | scalar-broadcast deferred; clear diagnostic |
| `a.checked_add(b)` | (no numpy analog) | `-> Result` (Ph2) | n/a | Cobrust-original §2.2 escape hatch |

**Aggregate:** the Phase-1 *shipped* surface (`+`/`-`/`*`, `a[i]`, `.shape`/`.ndim`/
`.size`) scores **~0.97 average training-data overlap** — these are verbatim or
near-verbatim numpy. The two forced divergences (`a[i]: f64` not 0-d scalar;
`a.shape: list` not tuple) are **semantically benign** (the values are usable identically
in the common case) and are recorded as known divergences in the coil PROVENANCE manifest
per ADR-0072 Q6.

**Where static-typing forces divergence (honest §2.5 deficit):** shape-correctness is
**uncheckable at compile time** (Q4) — the strongest LLM correction signal (the
type-error feedback loop, §2.5 compile-time-catch) is unavailable for `(3,) + (4,)`; the
LLM only learns at runtime. This is intrinsic to "Cobrust handles carry no shape in the
type" and is the principal §2.5 limitation of this design. A shape-/rank-parameterized
`Ty::Adt(COIL_BUFFER_ADT, [rank, dtype])` would recover some compile-time catch (§8) but
is a major undertaking deferred to its own ADR.

**Where Result-default (§2.2) bends to §2.5:** Q4 — the operator returns a bare `Buffer`
(panic-on-mismatch) rather than `Result<Buffer, ShapeError>`, because `let c = (a + b)?`
on every line is anti-numpy. The §2.2 letter is preserved via the explicit
`a.checked_add(b) -> Result` escape (Phase 2). This is the single deliberate §2.2↔§2.5
trade in the design, documented for audit.

## 12. Open questions for sub-ADRs

Deferred surfaces, each warranting its own design pass when reached:

- **Multi-D indexing tuples** — `a[i, j]` (numpy comma-index). Cobrust parses `a[i, j]`
  as indexing by a tuple `(i, j)`; needs a tuple-index ABI distinct from scalar `a[i]`.
  Sibling of Q5's `reshape(tuple)` marshalling.
- **Slice with step / negative indices** — `a[::2]`, `a[-1]`, `a[1:3, ::2]`. The
  start/stop/step + negative-normalization ABI; Phase 2 does the simple contiguous slice
  first.
- **Scalar broadcast** — `a + 1`, `2 * a` (Buffer ⊕ scalar). Needs a mixed-operand
  manifest entry + the typecheck arm to admit `Buffer + Int`/`Buffer + Float`. Common
  enough in numpy that it is a strong Phase-2 candidate.
- **ufunc broadcasting rules** — full numpy broadcasting semantics (trailing-dim
  alignment, dim-1 stretch); Phase 2 does the common cases, the full ruleset is its own
  spec.
- **`out=` parameter** — `coil.add(a, b, out=c)` in-place. Needs keyword-arg marshalling
  (Q5) + mutable-borrow semantics for the `out` handle.
- **einsum** — `coil.einsum("ij,jk->ik", a, b)`. A string-spec'd contraction; large
  surface, far-future.
- **dtype-parameterized `Ty::Adt`** — `Ty::Adt(COIL_BUFFER_ADT, [dtype])` to recover
  compile-time dtype-mismatch catch (the §11 §2.5-deficit mitigation). Touches the type
  system broadly; major sub-ADR.
- **Comparison operators returning a bool `Buffer`** — `a < b` → element-wise bool mask.
  numpy returns a bool array; Cobrust would need a bool-dtype Buffer. Deferred.
- **`@` matmul operator** — `a @ b`. `HirBinOp::MatMul` / `BinOp::MatMul` already exist
  (lower.rs:2435) and currently reject; a Buffer arm could route `@` to
  `__cobrust_coil_buffer_matmul` exactly like `+`. Natural Phase-2/3 extension once
  `dot` lands (they share the runtime).

## 13. Consequences

- **Positive:** unblocks the highest-§2.5-value numpy surface with a precedented,
  codegen-untouched mechanism; establishes the reusable ecosystem-handle-operator pattern;
  Phase 1 is a genuine ≤1-2 day fill-in-the-blanks given §9.
- **Negative / accepted:** shape-correctness stays runtime-only (intrinsic §2.5 deficit,
  §11); two benign known divergences (`a[i]: f64`, `a.shape: list`); the §2.2↔§2.5 trade
  on operator error-surface (Q4) is a deliberate, documented exception.
- **Risk — manifest drift:** new operator/attr tables join the hand-maintained manifest
  (ADR-0072 §5 R4 accepted debt); generation still deferred.
- **Risk — `a.shape` list-build:** first use of cross-crate stdlib `__cobrust_list_*`
  externs from coil; proven pattern (ADR-0072 Q5) but new to this crate — the impl sprint
  verifies the link wiring (build.rs already always-links `libcobrust_stdlib.a`).
- **Follow-up:** ratify draft→accepted when the Phase 1 impl sprint lands + passes the
  §8 done-means + paired audit.
