---
doc_kind: adr
adr_id: 0050d
title: "Dict design — literal syntax, indexing, iteration order, Hashable keys, and Wave-3 implementation map"
status: accepted
date: 2026-05-16
last_verified_commit: d15bde7
supersedes: []
superseded_by: []
relates_to: [adr:0003, adr:0006, adr:0019, adr:0023, adr:0025, adr:0027, adr:0030, adr:0034, adr:0035, adr:0038, adr:0044, adr:0048, adr:0049, adr:0050, adr:0050a, adr:0050b, adr:0050c]
parent_adr: adr:0050
sub_adr_of: 0050 (Phase F.3 batch §"Sub-ADR slots / ADR-0050d")
ratification_path: in-session review per ADR-0050 audit-teammate pattern (read-only opus general-purpose audit at Wave-1 close)
---

# ADR-0050d: Dict design — literal syntax, indexing, iteration order, Hashable keys, and Wave-3 implementation map

## Context

### Why dict now

The project owner's 2026-05-16 prioritization (verbatim in ADR-0050 §"Strategic
frame") names `dict` as the first P0 item: **"dict ← blocks everything, 2 weeks,
depth task"**. The constitution's keep-list (§2.1) presumes dict at the language
tier (comprehensions, iteration protocols, structural pattern matching all
imply hashmap support); the type-checker already synthesizes `Ty::Dict(K, V)`
(`crates/cobrust-types/src/ty.rs:65`), the AST already carries `DictEntry::Pair`
/ `DictEntry::Spread` (`crates/cobrust-frontend/src/ast.rs:390`), and the MIR
already emits `Rvalue::Aggregate(AggregateKind::Dict, ops)`
(`crates/cobrust-mir/src/lower.rs:1133`). The stdlib even ships an M12.x stub
`__cobrust_dict_new` / `__cobrust_dict_set` / `__cobrust_dict_get` /
`__cobrust_dict_len` / `__cobrust_dict_drop` C-ABI for `Dict<i64, i64>`
(`crates/cobrust-stdlib/src/collections.rs:534-636`).

**What is missing** is the Wave-3 honest implementation: end-to-end source-level
dict construction, indexing, mutation, membership (`in`), iteration with
Python 3.7+ insertion-order semantics, and a typed Hashable surface that admits
`i64` + `str` keys while rejecting `f64`. This ADR's job is to lock those
surface decisions before Wave-3 dispatch so the P9-F sprint has a complete
blueprint to dispatch P7 sub-sprints against.

### Constitution alignment

- **§1.1** "syntactically familiar to Python users" — dict literal is
  `{k: v}`; `d[k]` indexing reads/writes; `key in d` membership; `len(d)`
  size; `for k in d:` / `for k, v in d.items():` iteration.
- **§2.1 keep** — comprehensions, iteration protocols, structural pattern
  matching, decorators all presume dict; this ADR lands the substrate.
- **§2.2 drop** — `if d` is rejected (`TypeError::ImplicitTruthiness`); use
  `len(d) > 0` or `key in d` instead. No silent coercion: `d[k] = v` with
  `v: str` after `d: dict[str, i64]` is a `TypeError::TypeMismatch`. `is`-vs-`==`
  confusion already removed by parser. `Result<T, E>` is the default error
  path: see §"Indexing" decision for the missing-key surface.
- **§2.3 adopt** — Hash is a trait surface; `K` must implement it.
  Iter order is a documented invariant.
- **§5.1 elegant** — one literal form, one indexing form, one membership
  operator; explicit `.get(k) -> Option[V]` for the missing-key escape hatch.
- **§5.2 scientific** — every surface decision below cites an alternative
  considered and an evidence anchor.
- **§5.3 efficient** — backed by `indexmap::IndexMap` (insertion-order
  hashmap) via `__cobrust_dict_*` C-ABI; no GC, no global lock, allocations
  visible at codegen.

### Existing scaffolding

Net new in Wave 3 (per ADR-0050 §M-F.3.4) builds on:

| Surface | Status | Anchor |
|---|---|---|
| `Ty::Dict(K, V)` parametric | ✅ exists | `cobrust-types/src/ty.rs:65` |
| `ExprKind::Dict(Vec<DictEntry>)` AST | ✅ exists | `cobrust-frontend/src/ast.rs:390` |
| `parse_brace_expr` `{}` / `{k:v}` / `{x}` / `{**rest}` disambiguation | ✅ exists | `cobrust-frontend/src/parser.rs:1470` |
| `AggregateKind::Dict` MIR rvalue | ✅ exists | `cobrust-mir/src/tree.rs:387` |
| `lower.rs ExprKind::Dict` → `Rvalue::Aggregate(Dict, ops)` | ✅ exists | `cobrust-mir/src/lower.rs:1111` |
| Type-checker `synth Dict` over entries | ✅ exists | `cobrust-types/src/check.rs:614` |
| `BinOp::In / NotIn` over iterable RHS, returns Bool | ✅ exists | `cobrust-types/src/check.rs:881` |
| `iter_element(Dict(K,V)) -> K` (so `for k in d:` already keys) | ✅ exists | `cobrust-types/src/check.rs:451` |
| `TypeError::MutableDefault` blocks `def f(x: dict = {})` already | ✅ exists | `cobrust-types/src/check.rs:245` |
| `__cobrust_dict_new` / `_set` / `_get` / `_len` / `_drop` C-ABI (i64,i64 only, stub) | ⚠ partial | `cobrust-stdlib/src/collections.rs:534-636` |
| Codegen: `Aggregate::Dict` Cranelift lowering to runtime helpers | ❌ stub returns null | `cobrust-codegen/src/cranelift_backend.rs` (M9 stub per ADR-0027 §1) |
| `d[k] = v` source assign — HIR `Stmt::IndexAssign` | ❌ not lowered to dict_set | Wave 3 sub-sprint c |
| `d[k]` read — currently `iter_element` path lowers to list-of-i64 dispatch | ❌ no dict dispatch | Wave 3 sub-sprint c |
| `key in d` for dict — `BinOp::In` lowers to `__cobrust_list_contains` placeholder | ❌ no dict dispatch | Wave 3 sub-sprint c |
| `d.items()` / `d.keys()` / `d.values()` method dispatch | ❌ method-on-dict not lowered | Wave 3 sub-sprint e |
| Insertion-order iteration (Python 3.7+) backed by `indexmap::IndexMap` | ❌ uses `HashMap` | Wave 3 sub-sprint d |
| Str-keyed dict (`__cobrust_dict_set_str_i64`, etc.) | ❌ not implemented | Wave 3 sub-sprint d (blocks on ADR-0050c Str ownership) |
| Hashable trait surface (codegen-side dispatch) | ❌ not implemented | Wave 3 sub-sprint b |
| Drop schedule for dict-of-Str-keyed | ❌ blocks on ADR-0050c | Wave 3 sub-sprint f |

The Wave-3 sprint is therefore **finish what M12.x stubbed**, with two
genuinely new surfaces: insertion-order iteration (`indexmap`) and Str-keyed
dict (which requires ADR-0050c Str ownership to land first).

### Dependency graph for Wave 3

```
ADR-0050a (break/continue)  ─┐
ADR-0050b (for loop)        ─┼─→ sub-sprint e (iter desugar)
ADR-0050c (Str ownership)   ─┴─→ sub-sprint d (str-keyed C-ABI), sub-sprint f (drop)

ADR-0050d (this ADR)        ─→ sub-sprints a..g (Wave 3 dispatch blueprint)
```

`break/continue` and `for-loop` (Wave 1) close before Wave 3 starts; Str
ownership (Wave 2) closes before Wave 3 sub-sprint d/f start. Sub-sprints a..c
are independent of Waves 1/2 and could in theory begin earlier — but the Wave
ordering in ADR-0050 dictates dict-impl is Wave 3.

## Options considered (10 surface decisions)

Each decision below: option, pros/cons, at least one alternative, the
chosen path. Single comprehensive table after the rationales for quick
reference.

### Decision 1 — Literal syntax: `{}` is dict, not set

#### Option 1A — `{}` empty dict + `set()` ctor for empty set (CHOSEN, matches Python)

- Pros: matches Python exactly (`type({})` is `dict`, `type(set())` is `set`);
  the parser already implements this (`parser.rs:1473-1481` — `{}` becomes
  `CollectionLit::Dict(vec![])`); zero parser churn.
- Cons: `set()` ctor requires intrinsic-rewrite or stdlib free fn (deferred to
  Phase G; for Wave 3, source-level set literals are a Phase G item anyway —
  ADR-0050 only commits dict for Phase F.3).

#### Option 1B — `{}` ambiguous; force `dict()` ctor for empty dict + `{}` for empty set

- Pros: removes ambiguity at the type level.
- Cons: **diverges from Python**; user mental model "{} is dict" baked in
  by 30 years of Python tradition + LeetCode wedge audience. Parser already
  resolved the ambiguity in 1A's favor. **Rejected.**

#### Option 1C — `{}` only allowed in annotated context

- Pros: forces the user to annotate `d: dict[str, i64] = {}`.
- Cons: hostile to script-style code; the type checker can already
  synthesize fresh `K`/`V` vars at `{}` site (`check.rs:614-620`) and let
  later use sites pin them. **Rejected.**

**Chosen: 1A**. Match Python. Empty `{}` is dict; empty set is `set()` (Phase G).
Note that ADR-0050 doesn't commit set for Phase F.3 — the parser
`CollectionLit::Set` path stays operational for `{x, y, z}` but stdlib backing
remains the M12.x stub.

### Decision 2 — Indexing `d[k]` semantics: panic-on-missing + `.get(k) -> Option[V]` safe escape

#### Option 2A — `d[k] -> V`, **panic** on missing key (CHOSEN, matches Python's `KeyError`)

- Pros: matches Python; ergonomic for the 95% case where the user has
  verified `k in d`; cheap codegen (no Option wrap); zero allocation in the
  hot path; **explicit safe escape via `d.get(k) -> Option[V]`** for the
  conservative path.
- Cons: panics are not the constitution's `Result`-default error path.
  Mitigation: the runtime helper returns a sentinel + abort signal, not a
  Rust panic; the codegen lowers to "branch on sentinel → call
  `__cobrust_dict_keyerror_abort(k_repr) -> !`" which prints
  `"KeyError: <k>"` and exits 134 (SIGABRT-style), matching Rust's
  `slice[oob]` policy. The contract is documented in zh + en
  getting-started.

#### Option 2B — `d[k] -> Option[V]`, no panic ever

- Pros: 100% `Result`-default-aligned; no abort path.
- Cons: every dict read forces `.unwrap()` or `match`; **breaks Python
  mental model** (in Python `d[k]` returns the value directly); the
  ergonomics tax is heavy enough to discourage dict use — exactly the
  opposite of what Phase F.3's "blocks everything" mandate wants.
  Mitigation `d[k] = d.get(k).unwrap()` is also worse than 2A.

#### Option 2C — `d[k] -> Result[V, KeyError]`

- Pros: most rigorous; threads `?`-operator-style propagation.
- Cons: `?`-operator desugaring for `Result[V, E]` is not yet wired in
  the type checker / MIR (ADR-0044a deferred `Result[Str, IoError]`
  to Phase F.1.x candidate; **same blocker applies here**). Would
  force this batch to ship a typed-Result lowering ADR first.
  **Out of Wave 3 scope.**

**Chosen: 2A**. Python-faithful + an explicit safe escape via `.get(k)`. The
constitution §2.2 `Result`-default reading is preserved at the **safe-escape
tier** (`.get(k) -> Option[V]`); the panic-on-missing path is documented as
the **fast tier** with the abort contract — same precedent as Rust's
`Vec<T>` indexing (`v[i]` panics, `v.get(i)` returns `Option<&T>`).

### Decision 3 — Insertion `d[k] = v` is rebind, not append

#### Option 3A — `d[k] = v` rebinds existing or inserts new (CHOSEN)

- Pros: matches Python; matches `HashMap::insert` semantics; trivial.
- Cons: none.

#### Option 3B — Separate `d.insert(k, v)` and `d.update(k, v)` methods

- Pros: explicit about replace-vs-insert.
- Cons: heavyweight; not Python-like; would require user to do
  `if k in d: d.update(k, v) else: d.insert(k, v)` for the common rebind
  case. **Rejected.**

**Chosen: 3A**. Standard hashmap semantics.

### Decision 4 — Membership `key in d` returns `bool`, not `Option[V]`

#### Option 4A — `key in d -> bool` (CHOSEN)

- Pros: matches Python (`__contains__`); already wired via
  `BinOp::In` in `check.rs:881` which returns `Ty::Bool`; codegen
  lowers to `__cobrust_dict_contains_<K> -> i64` (0/1), then
  `iconst 0 → bool`.
- Cons: forces a second lookup `d[k]` after `if k in d:`. Mitigation:
  the user who needs both can write `match d.get(k) { Some(v) => …,
  None => … }`.

#### Option 4B — `key in d -> Option[V]` (Rust `HashMap::get`-style)

- Pros: combines containment check + read.
- Cons: forces every membership test into `Option`-unwrap territory;
  breaks Python mental model. `key in d` returning a non-bool is
  surprising. **Rejected.**

**Chosen: 4A**. `bool` return; `.get(k) -> Option[V]` covers the combined case.

### Decision 5 — `len(d)` returns `i64`, not `usize`

#### Option 5A — `len(d) -> i64` (CHOSEN)

- Pros: matches the existing `len(list)` signature
  (`__cobrust_list_len -> i64`) and `len(str)` signature; uniform
  numeric type at the source surface.
- Cons: `i64::MIN..0` is technically meaningless for dict size; the
  runtime helper clamps. Same caveat as list/str.

#### Option 5B — `len(d) -> u64`

- Pros: closer to the actual nonnegative semantics.
- Cons: u64 is not yet a first-class type in Cobrust (M2 single-width
  i64; `u32`/`u64` reserved for Phase G alongside integer-width
  expansion). **Out of scope.**

**Chosen: 5A**. Match list/str.

#### Surface addendum 2026-05-16 — `dict.is_empty() -> bool` (audit Finding 1.2)

Pin `dict.is_empty() -> bool` in the surface alongside `len(d)`. Reasoning: constitution §2.2 forbids implicit truthy/falsy. Without `is_empty()` users have no idiomatic path to "is this dict empty?" — they would either write `if d:` (forbidden by §2.2) or `if len(d) == 0:` (allowed but less readable). `dict.is_empty()` mirrors `list.is_empty()` already-shipping in `__cobrust_list_len == 0` patterns and the constitution's preference for explicit predicate methods over coercion. Sub-sprint d ships `__cobrust_dict_is_empty(*mut Dict) -> i64` as the C-ABI shim (i64 0/1 per the SwitchInt codegen convention; see ADR-0044 W2 Phase 3 `str_eq` precedent at `stdlib/io.rs:502-515`).

### Decision 6 — Iteration order: Python 3.7+ insertion-order (backed by `indexmap::IndexMap`)

#### Option 6A — `indexmap::IndexMap` for insertion order (CHOSEN)

- Pros: matches Python 3.7+ formal guarantee; `indexmap` is a mature
  upstream crate (4.5k stars, well-maintained, dep-light); preserves
  order for `for k in d:`, `for k, v in d.items():`, `d.keys()`,
  `d.values()`, repr, equality (well — see Decision 8 for equality);
  drop-in for `HashMap` at the API surface; lookup is O(1) average.
- Cons: ~2x memory overhead vs raw `HashMap` (a hash table + an
  ordered Vec of entries); ~15-25% slower insertion than `HashMap` in
  benchmarks. Acceptable per §5.3 (efficient is "no GC, no GIL,
  allocations visible" — not "fastest possible map").

#### Option 6B — Hand-rolled doubly-linked `LinkedHashMap`

- Pros: zero new dependency.
- Cons: ~500 LoC of unsafe Rust we maintain in `cobrust-stdlib`;
  reinvents `indexmap`; bug surface is large. **Rejected.**

#### Option 6C — `std::collections::HashMap` + abandon insertion-order guarantee

- Pros: cheapest; matches Python pre-3.7 semantics.
- Cons: **diverges from Python ≥ 3.7**; LeetCode wedge audience expects
  insertion order; would cause silent test failures when users port
  Python code that depends on order. Worth rejecting even though
  Python ≤ 3.6 also lacked the guarantee, because the **mental model
  reset cost** is higher than the dependency cost. **Rejected.**

**Chosen: 6A**. `indexmap::IndexMap` keyed by a `KeyEnum` (see Decision 7).

#### Cargo manifest implication

`indexmap = "2"` is **proposed** for addition to `[workspace.dependencies]`
in `Cargo.toml` as a Wave-3 sub-sprint-d landing item (not added by this
ADR; design-only). The proposed line:

```toml
indexmap = "2"     # ADR-0050d Decision 6A — Wave 3 sub-sprint d Dict storage backing
```

`cobrust-stdlib` adds `indexmap = { workspace = true }` to its
`Cargo.toml`. Dep-tree audit: `indexmap` depends only on `equivalent`
(MIT/Apache-2.0) and `hashbrown` (transitively, which is also already a
common dep). Confirmed light per ADR-0050 §"Negative" consequence
budget.

### Decision 7 — Type parameters: `K ∈ {i64, str}` for Phase F.3; reject `f64`; defer custom types

#### Option 7A — Phase F.3 ships `K ∈ {i64, str}` + reject f64 at type-check (CHOSEN)

- Pros: covers ≥ 95% of real-world dict use (LeetCode + AI prompt
  caches + word-count examples + JSON parsing scaffold); enforces the
  Hash trait invariant via **codegen-side dispatch** (no runtime trait
  vtable); rejects `f64` keys at type-check time (`TypeError::NotHashable
  { actual: Ty::Float }`) because NaN != NaN breaks Hash invariants
  per IEEE 754 (constitution §2.2 "no silent coercion" + §5.2
  "scientific — every choice with evidence").
- Cons: custom-type `K` (e.g., user-defined `struct Point`) is not
  supported until Phase G traits ship; users wanting that today must
  serialize to `str` and use that as the key.

**The Hash dispatch model**: codegen branches on the static `K` type
(derived from the `Ty::Dict(K, V)` type-checker output) at every
`d[k]` / `d[k] = v` / `key in d` callsite. There are exactly **2 ×
2 = 4** (K, V) shapes shipped in Phase F.3:

| K | V | C-ABI shim suffix |
|---|---|---|
| i64 | i64 | `_i64_i64` (already exists, M12.x stub) |
| i64 | str | `_i64_str` (new) |
| str | i64 | `_str_i64` (new; blocks on ADR-0050c Str-keyed hashing) |
| str | str | `_str_str` (new; blocks on ADR-0050c) |

The `__cobrust_dict_new` shim takes `(k_size, v_size, len)` already
(per `collections.rs:549`); a Phase G extension can add a per-type
`type_id` enum + per-K Hash vtable behind a `dyn` opt-in keyword
without breaking this surface (the M12.x signature stays compatible
because the size argument is already an opaque discriminator).

#### Option 7B — Phase F.3 ships f64 keys with IEEE 754 contract documented

- Pros: maximum surface coverage.
- Cons: NaN != NaN means `d[nan] = 1; d[nan]` will panic (key not
  found) — a **silent footgun** that the constitution explicitly
  rejects via §2.2 "no silent coercion". The fix is to either (a)
  rewrite NaN to a canonical bit pattern (loses IEEE 754 strict
  compliance per ADR-0050 §"f64") or (b) reject NaN at insert time
  (defeats the parametric Hash contract). Cleaner to reject f64
  entirely at type-check. **Rejected.**

#### Option 7C — Phase F.3 ships only i64 keys; str keys deferred to Phase F.3 P1

- Pros: smallest sprint.
- Cons: str-keyed dict is **the** Python-shape use case (word count,
  JSON object, env var lookup, AI prompt cache, …). Shipping i64-only
  is not credible as "dict shipped". **Rejected.**

#### Option 7D — Ship a full Hashable trait at language tier

- Pros: most general.
- Cons: requires Phase G trait surface; out of Phase F.3 scope per
  ADR-0050. **Out of scope.**

**Chosen: 7A**. Two K types (`i64`, `str`), any V type that fits the
codegen lowering (Phase F.3 ships V ∈ {i64, str} for completeness;
extension to V ∈ {f64, list[T], dict[K, V], record, ADT} is Phase G).

### Decision 8 — Equality `d1 == d2` is structural (key-set equal + per-key value equal), order-independent

#### Option 8A — Structural equality, order-independent (CHOSEN, matches Python)

- Pros: matches Python (`{1:2, 3:4} == {3:4, 1:2}` is True);
  matches `HashMap` mental model; order-sensitive equality would
  punish `indexmap` users by **leaking** a backing-store choice
  into the language semantic.
- Cons: O(min(|d1|, |d2|)) comparison vs O(1) pointer compare. Acceptable.

#### Option 8B — Order-sensitive equality (insertion order matters for `==`)

- Pros: emphasizes the iteration order guarantee.
- Cons: **diverges from Python**; the Python guarantee is about
  iteration, not equality. **Rejected.**

**Chosen: 8A**. Structural, order-independent.

### Decision 9 — Comprehensions `{k: v for ...}` already wired; ship in Phase F.3

#### Option 9A — Wire dict comprehensions in Phase F.3 (CHOSEN, low marginal cost)

- Pros: the parser already produces `Comprehension { kind: Dict,
  element: ComprehensionElem::KeyValue(...) }` (per
  `parser.rs:1491-1503`); the type checker already synthesizes
  `Ty::Dict(kt, vt)` for dict comp (per `check.rs:935-939`); the MIR
  lower path is a small extension of the existing list-comp lowering
  (`lower.rs:1138-...`) — `__cobrust_dict_set` per iteration instead
  of `__cobrust_list_append`; total ~50 LoC marginal cost.
- Cons: comprehensions in general are a Phase G scope item per ADR-0050
  §M-F.3.4 nominal scope. But the marginal cost here is small enough
  that **deferring it would force a second sprint** to wire something
  already partially in place.

#### Option 9B — Defer dict comprehensions to Phase G (per ADR-0050 conservative reading)

- Pros: keeps Wave 3 scope tight.
- Cons: leaves users writing 5-line desugars by hand for the most
  common dict construction pattern; comprehensions are constitution
  §2.1 keep-list. **Rejected.**

**Chosen: 9A**. Wire dict comprehensions in Wave 3 sub-sprint c (MIR lowering
extension) alongside the indexing/insertion work; it's the same code path
extension. The acceptance corpus includes ≥ 10 dict-comp programs in addition
to ≥ 80 base dict programs (per ADR-0050 §M-F.3.4 test budget).

### Decision 10 — `copy()` / `clone()`: shallow only; deep is Phase G

#### Option 10A — Shallow clone via `__cobrust_dict_clone(d)` (CHOSEN)

- Pros: matches Python `dict.copy()`; clear semantics (new dict, same
  references for nested mutables); cheap.
- Cons: shared mutable substructure means `d2 = d1.copy(); d2[k][0] = 99`
  mutates `d1[k][0]` too. Document.

#### Option 10B — Deep clone

- Pros: full isolation.
- Cons: requires full deep-copy traversal which depends on `V` being
  Cloneable + a clone vtable. Phase G material.

**Chosen: 10A**. `d.copy() -> dict[K, V]` shallow clone, source-level free fn
or method (Wave 3 sub-sprint e wires the method-dispatch alongside `.keys()`).

### Quick-reference decision table

| Decision | Surface | Choice | Constitution anchor | Wave 3 sub-sprint |
|---|---|---|---|---|
| 1 | Empty `{}` | Dict (Python-compat) | §1.1, §2.1 | sub-sprint a (parser, ✅ already exists) |
| 2 | `d[k]` missing key | Panic + abort + `.get(k)` safe escape | §2.2 (safe tier preserved) | sub-sprint c (codegen abort path) |
| 3 | `d[k] = v` | Rebind/insert | §1.1 | sub-sprint c |
| 4 | `key in d` | `bool` | §1.1, §5.1 | sub-sprint c |
| 5 | `len(d)` | `i64` | §5.1 uniform | sub-sprint d |
| 6 | Iter order | Insertion order via `indexmap` | §5.3 efficient, §1.1 Python 3.7+ | sub-sprint d |
| 7 | K type | `i64` or `str`; reject `f64` | §2.2 no silent coercion (NaN) | sub-sprint b (type check) |
| 8 | `d1 == d2` | Structural, order-independent | §1.1 Python-compat | sub-sprint c |
| 9 | Dict comprehension | Wire in Phase F.3 | §2.1 keep-list | sub-sprint c |
| 10 | `d.copy()` | Shallow clone | §1.1 Python-compat | sub-sprint e (method dispatch) |

## Decision

Adopt all 10 choices in §"Quick-reference decision table". The dict surface
ships in Wave 3 (per ADR-0050 §"Wave structure") as a single P9-F sprint
dispatching seven sequential P7 sub-sprints below.

### Cobrust source-level surface (binding)

```python
# Construction
let d: dict[str, i64] = {}                  # empty dict, parametric K/V inferred from later use
let d2: dict[str, i64] = {"a": 1, "b": 2}   # literal with entries
let d3 = {"a": 1, "b": 2}                   # type inferred as dict[str, i64]

# Indexing — read
let v = d2["a"]                             # v: i64; panics + aborts if "a" not in d2
let opt = d2.get("a")                       # opt: Option[i64]; safe escape

# Indexing — write (rebind-or-insert)
d2["c"] = 3
d2["a"] = 99                                # rebind

# Membership
if "a" in d2: ...                           # bool
if "z" not in d2: ...                       # bool

# Size
let n = len(d2)                             # i64

# Iteration
for k in d2: print(k)                       # iterates keys, insertion order
for k in d2.keys(): print(k)                # explicit; same as above
for v in d2.values(): print_int(v)          # iterates values, insertion order
for (k, v) in d2.items(): ...               # iterates (key, value) tuples, insertion order

# Comprehension
let evens = {x: x*2 for x in range(10) if x % 2 == 0}    # dict[i64, i64]

# Equality
let eq = (d2 == {"a": 99, "b": 2, "c": 3})  # bool; order-independent

# Shallow clone
let d4 = d2.copy()                          # dict[str, i64]; same V references for nested

# Type-check rejections (ill-typed corpus targets)
let bad: dict[f64, i64] = {1.0: 1}          # TypeError::NotHashable { actual: Ty::Float }
let bad2: dict[str, i64] = {"a": "x"}       # TypeError::TypeMismatch { expected: i64, actual: str }
if d2: ...                                  # TypeError::ImplicitTruthiness — use len(d2) > 0
```

### MIR shape (binding)

`Rvalue::Aggregate(AggregateKind::Dict, ops)` is already in place; **ops are
interleaved K, V pairs** (matching `lower.rs:1111`'s existing convention:
`ops.push(k); ops.push(v);` per entry, with spread entries pushing the spread
operand alone — for Wave 3 we **reject spread at non-empty dict literals**
since dict-merge semantics are Phase G; spread-in-comprehension stays a
comprehension-clause concern).

**Net new MIR Rvalues**:

```rust
pub enum Rvalue {
    // ... existing variants ...
    /// Read d[k]; panics on missing key (lowering inserts an abort BB).
    /// Codegen dispatches by K's concrete type from the surrounding `Ty::Dict(K, _)`.
    DictGet(Place /* d */, Operand /* k */),
    /// Write d[k] = v; rebinds existing or inserts new.
    DictSet(Place /* d */, Operand /* k */, Operand /* v */),
    /// `key in d` → bool.
    DictContains(Place /* d */, Operand /* k */),
    /// Length.
    DictLen(Place /* d */),
}
```

OR (alternative, lighter): **reuse `Terminator::Call` with C-ABI symbol
names** to avoid expanding the `Rvalue` enum. The intrinsic-rewrite pass
(see ADR-0044 §"intrinsic-rewrite") already converts `d[k]` HIR Index +
type info into a Call to `__cobrust_dict_get_<K_V>`. **Choice deferred to
sub-sprint c**; the type-checker output (the resolved `Ty::Dict(K, V)`) is
authoritative for dispatch in either lowering.

Recommendation: **lighter path — intrinsic-rewrite to C-ABI calls**, mirroring
how `print` / `input` / `argv` / `parse_int` / `str_len` were wired in W2.
Reasons: (a) keeps the `Rvalue` enum cardinality stable; (b) reuses the
proven `runtime_helper_signatures` path; (c) the codegen branches on the
**call symbol name** which already encodes the (K, V) shape; (d) Phase G
custom-K extension just adds new call symbols, no MIR churn.

### Codegen + C-ABI shim surface (binding)

Sub-sprint d emits **12 new C-ABI shims** in `crates/cobrust-stdlib/src/collections.rs`,
mirroring the M12.x dict shape but with `indexmap::IndexMap` backing and the
(K, V) shape encoded in the symbol name:

```rust
// New / amended:
pub unsafe extern "C" fn __cobrust_dict_new(k_size: i64, v_size: i64, len: i64) -> *mut u8;
pub unsafe extern "C" fn __cobrust_dict_drop(d: *mut u8);
pub unsafe extern "C" fn __cobrust_dict_len(d: *mut u8) -> i64;
pub unsafe extern "C" fn __cobrust_dict_clone(d: *mut u8) -> *mut u8;          // shallow

// Per-(K, V) — i64 keys
pub unsafe extern "C" fn __cobrust_dict_set_i64_i64(d: *mut u8, k: i64, v: i64);
pub unsafe extern "C" fn __cobrust_dict_get_i64_i64(d: *mut u8, k: i64) -> i64;
pub unsafe extern "C" fn __cobrust_dict_contains_i64_i64(d: *mut u8, k: i64) -> i64;   // 0/1
pub unsafe extern "C" fn __cobrust_dict_set_i64_str(d: *mut u8, k: i64, v: *mut u8);
pub unsafe extern "C" fn __cobrust_dict_get_i64_str(d: *mut u8, k: i64) -> *mut u8;

// Per-(K, V) — str keys (block on ADR-0050c)
pub unsafe extern "C" fn __cobrust_dict_set_str_i64(d: *mut u8, k: *mut u8, v: i64);
pub unsafe extern "C" fn __cobrust_dict_get_str_i64(d: *mut u8, k: *mut u8) -> i64;
pub unsafe extern "C" fn __cobrust_dict_contains_str_i64(d: *mut u8, k: *mut u8) -> i64;
pub unsafe extern "C" fn __cobrust_dict_set_str_str(d: *mut u8, k: *mut u8, v: *mut u8);
pub unsafe extern "C" fn __cobrust_dict_get_str_str(d: *mut u8, k: *mut u8) -> *mut u8;

// Iteration — returns an opaque iterator handle (insertion order)
pub unsafe extern "C" fn __cobrust_dict_iter_init(d: *mut u8, mode: i64) -> *mut u8;
// mode encoding: 0 = keys, 1 = values, 2 = items (key+value pairs, packed as 2*i64 / 2*ptr)
pub unsafe extern "C" fn __cobrust_dict_iter_next(it: *mut u8) -> i64; // 0/1 has-next; loads via accessor
pub unsafe extern "C" fn __cobrust_dict_iter_key_i64(it: *mut u8) -> i64;
pub unsafe extern "C" fn __cobrust_dict_iter_key_str(it: *mut u8) -> *mut u8;
pub unsafe extern "C" fn __cobrust_dict_iter_val_i64(it: *mut u8) -> i64;
pub unsafe extern "C" fn __cobrust_dict_iter_val_str(it: *mut u8) -> *mut u8;
pub unsafe extern "C" fn __cobrust_dict_iter_drop(it: *mut u8);

// Equality
pub unsafe extern "C" fn __cobrust_dict_eq_i64_i64(a: *mut u8, b: *mut u8) -> i64;  // 0/1
pub unsafe extern "C" fn __cobrust_dict_eq_str_str(a: *mut u8, b: *mut u8) -> i64;
// (other (K,V) shapes generated by sub-sprint d as needed)

// Missing-key panic helper (replaces a Rust panic with a Cobrust-style abort)
pub unsafe extern "C" fn __cobrust_dict_keyerror_abort_i64(k: i64) -> !;
pub unsafe extern "C" fn __cobrust_dict_keyerror_abort_str(k: *mut u8) -> !;
```

Total **~24 new symbols** (the table above is illustrative; the exact count
lands in sub-sprint d). The existing M12.x stub `__cobrust_dict_set` /
`__cobrust_dict_get` (untyped) is **replaced** by the typed `_i64_i64`
variant; the old generic name is removed (callers are M12.x test-only,
all updated in the same commit).

`indexmap::IndexMap<KeyEnum, ValueEnum>` is the storage backing where `KeyEnum`
is `enum { I64(i64), Str(*mut StringBuffer) }` (Phase F.3 closed enum;
Phase G extends to custom K via a trait vtable). `ValueEnum` is symmetrical.
The codegen passes the `(K, V)` shape through the symbol-name dispatch so
the runtime helper knows exactly which enum variant to expect; the runtime
can `debug_assert!` the variant on every call for honest crash-on-misuse.

### Iteration desugar (binding)

`for k in d:` and `for k, v in d.items():` desugar (in HIR or MIR; sub-sprint
e decides) to a while loop with the iterator handle:

```python
# Source:
for (k, v) in d.items():
    body

# Desugar (HIR-tier; mirrors ADR-0027 §4 for-protocol shape):
let __it = __cobrust_dict_iter_init(d, 2)            # mode=2: items
while __cobrust_dict_iter_next(__it) == 1:
    let k = __cobrust_dict_iter_key_<K>(__it)
    let v = __cobrust_dict_iter_val_<V>(__it)
    body
__cobrust_dict_iter_drop(__it)
```

The `<K>` / `<V>` placeholders are resolved at HIR lowering from the
type-checker output. `__cobrust_dict_iter_drop` is called at the **post-loop
basic block** (per ADR-0027 §"Drop schedule"); the iterator handle does
not borrow the dict — `indexmap` iterators are by-index so dict mutation
inside the loop is **undefined** and rejected at type check (Phase F.3
ships **no mutation-inside-iteration** detection; documented as a
known UB; Phase G adds aliasing-style enforcement). For the wedge audience
this matches Python (where mutation-during-iteration is also "don't" but
not enforced at compile time).

### Type-checker amendments (binding)

Net new in `cobrust-types/src/check.rs`:

1. `TypeError::NotHashable { actual: Ty, span: Span }` — emitted when
   `Ty::Dict(K, V)` is constructed with `K ∉ {i64, str}`. The check is
   added in `synth_dict_lit` (`check.rs:614`) **after** the existing
   `unify` over entries; if the resolved K's `is_hashable()` returns
   `false`, raise `NotHashable`. Same check at every `dict[K, V]`
   annotation site (`lower_type` -> `Ty::Dict` -> validate).
2. `Ty::is_hashable(&self) -> bool` returns true for `Bool`, `Int`,
   `Str`, `Bytes`, `Tuple(items)` if every item is hashable,
   `None`, `Never`; returns false for `Float`, `Imag`, `List`, `Set`,
   `Dict`, `Record`, `Fn`, `Adt`, `Alias`, `Generic`, `Var`. (Phase G
   extends ADT to hashable-if-trait-impl.)
3. `iter_element(Dict(K, _))` already returns K
   (`check.rs:451`); **no change**.
4. `synth_method_call` (or equivalent) recognizes `d.keys()` /
   `d.values()` / `d.items()` / `d.get(k)` / `d.copy()` as dict
   methods returning the corresponding types. Phase F.3 scope cap:
   these are recognized as **intrinsic methods** (similar to W2's
   `argv()` pattern), not via a full method-resolution machinery.

The 9 type-soundness obligations of ADR-0006 §"Soundness proof
obligation list" are respected:

| Obligation | Dict respects via |
|---|---|
| 1 Progress | well-typed dict access either yields a value or aborts (panic counts as well-defined termination) |
| 2 Preservation | `d[k] = v` preserves `dict[K, V]` type after the assign |
| 3 Lowering preservation | HIR -> MIR shape (Aggregate + intrinsic-call) is deterministic |
| 4 Decidability of inference | K/V are unified from entries or annotation; no implicit subtyping introduced |
| 5 Pattern exhaustiveness | dict isn't a scrutinee at Phase F.3 (Phase G adds dict patterns); current `match` over dict is non-exhaustive **TypeError** at use |
| 6 Implicit-truthiness rejection | `if d:` raises ImplicitTruthiness; no exception |
| 7 `is` non-occurrence | unchanged; dict identity is `==` (structural per Decision 8) |
| 8 Mutable-default rejection | `def f(x = {})` already rejected (`check.rs:245`); confirmed for dict |
| 9 No silent coercion | `d[k] = v` requires v's type unify with V (no implicit Int → Float etc.) |

### Parser amendments (binding)

The parser already handles `{}`, `{k:v}`, `{x}`, `{**rest}`, and dict
comprehensions per `parser.rs:1470-1574`. **Net new in Wave 3 sub-sprint a**:

1. **Reject `**spread` in non-comprehension dict literals** (per Decision 1
   commentary; dict-merge semantics are Phase G). Existing
   `DictEntry::Spread(s)` AST variant stays for forward compat; type-checker
   raises `TypeError::DictSpreadNotSupported` (new variant) at any
   `Spread` entry inside a `CollectionLit::Dict` in Phase F.3.
2. **Confirm `{}` resolves to `CollectionLit::Dict(vec![])`** at the parser
   level (already done at `parser.rs:1473-1481`); add a regression test.
3. **`d[k]`** as a left-of-assign place — already handled by the generic
   `Subscript` parser path (lists use the same path); no change.

### Implementation map — Wave 3 P9-F dispatch blueprint

Seven sub-sprints, sequential where dependencies exist, parallel where they
don't. Total Wave 3 estimate: **~15-18 days wall time** at the P9-F level
(matches ADR-0050 §"Wave structure" Wave 3 estimate of 10-14 days; the
upper end accommodates Str-keyed dependency on ADR-0050c closing).

Per `feedback_subagent_model_tier.md`: D1-D3 = sonnet OK with PAIR; D4 =
opus PAIR mandatory; D5 = opus solo or opus PAIR depending on whether
spike-style design or impl-style execution dominates.

#### Sub-sprint a — Parser literal `{k: v}` + AST/HIR + spread rejection

- **D-rating**: D3
- **Wall**: ~2 days
- **Model**: sonnet PAIR per `feedback_subagent_model_tier.md`
- **Touch**: `crates/cobrust-frontend/src/parser.rs`,
  `crates/cobrust-frontend/src/ast.rs` (no change), `crates/cobrust-hir/`
  (lowering Dict entries from AST `CollectionLit::Dict` to HIR
  `ExprKind::Dict`; mostly already wired but add Spread-rejection check).
- **Acceptance gates**: 5-gate green (fmt + clippy + build + workspace
  test + doc-coverage). Corpus: ≥ 20 well-typed dict-literal programs
  (incl. empty, single, multi, nested) + ≥ 10 ill-typed
  (spread-in-non-comp, missing colon, missing value, trailing-comma-only).
- **Blocks**: none (Wave 3 entry).

#### Sub-sprint b — Type checker `dict[K, V]` parametric + NotHashable check

- **D-rating**: D4
- **Wall**: ~3 days
- **Model**: opus PAIR
- **Touch**: `crates/cobrust-types/src/ty.rs` (`Ty::is_hashable`),
  `crates/cobrust-types/src/error.rs` (`TypeError::NotHashable`,
  `TypeError::DictSpreadNotSupported`), `crates/cobrust-types/src/check.rs`
  (synth Dict + dispatch methods + annotation validation),
  `crates/cobrust-types/src/infer.rs` (no change; `Dict(K,V)` already
  unifies; verify).
- **Acceptance gates**: 5-gate green + ≥ 40 well-typed dict-type programs
  + ≥ 30 ill-typed (f64 key rejected, mixed K/V rejected, implicit
  truthiness rejected, mutable default rejected, dict-method on non-dict
  rejected). Type-soundness obligations 1-9 audited (see table above).
- **Blocks on**: sub-sprint a (parser must emit `ExprKind::Dict` first).

#### Sub-sprint c — MIR `Rvalue::Aggregate::Dict` + intrinsic-rewrite for indexing + dict comprehension

- **D-rating**: D4
- **Wall**: ~2 days
- **Model**: sonnet PAIR (impl-shape, well-scoped; opus PAIR if D4 lifts
  to D5 due to dispatch-table size)
- **Touch**: `crates/cobrust-mir/src/lower.rs` (`ExprKind::Dict` already
  wired at L1111; **extend** to handle Spread-rejection echo from
  type-checker), `crates/cobrust-mir/src/lower.rs::ExprKind::Subscript`
  (read path → intrinsic-call to `__cobrust_dict_get_<K_V>` with abort
  branch), assignment statement (write path → intrinsic-call to
  `__cobrust_dict_set_<K_V>`), `BinOp::In` (membership →
  `__cobrust_dict_contains_<K_V>`), dict comprehension (extend
  `lower.rs:1600` from `__cobrust_list_append` to dispatch on
  comp kind: List → list_append, Dict → dict_set).
- **Acceptance gates**: 5-gate green + ≥ 30 well-typed MIR golden tests
  + integration with sub-sprint d for end-to-end runtime tests.
- **Blocks on**: sub-sprint b (type info drives K/V dispatch).

#### Sub-sprint d — Codegen + C-ABI shims (`indexmap` backing)

- **D-rating**: D4
- **Wall**: ~3 days
- **Model**: opus PAIR
- **Touch**: `crates/cobrust-stdlib/src/collections.rs` (replace M12.x
  `Dict<i64,i64>` stub with `indexmap::IndexMap<KeyEnum, ValueEnum>`
  backed implementation; ~24 new shims listed above),
  `crates/cobrust-codegen/src/cranelift_backend.rs::runtime_helper_signatures`
  (add ~24 signatures), `Cargo.toml` (workspace `indexmap = "2"`),
  `crates/cobrust-stdlib/Cargo.toml` (add `indexmap = { workspace = true }`).
- **Acceptance gates**: 5-gate green + ≥ 60 stdlib unit tests (per K/V
  shape × per operation matrix) + valgrind-clean for a representative
  program (drop schedule).
- **Blocks on**: sub-sprint b (signatures need K/V shape) + ADR-0050c
  Str-ownership ratified (str-keyed shims need real Drop semantics).
- **Parallel-eligible with sub-sprint c** if sub-sprint b ratifies the
  dispatch table early; otherwise sequential.

#### Sub-sprint e — Iteration `for k, v in d.items()` desugar + stdlib methods (keys/values/items/get/copy)

- **D-rating**: D3
- **Wall**: ~2 days
- **Model**: sonnet PAIR
- **Touch**: `crates/cobrust-hir/src/lower.rs` (or `cobrust-mir` —
  TBD by sub-sprint c outcome) `for` desugar extension for dict iter;
  `crates/cobrust-cli/src/build/intrinsics.rs` (recognize `d.keys()`
  / `d.values()` / `d.items()` / `d.get(k)` / `d.copy()` as
  method-intrinsic calls and rewrite to runtime symbols, mirroring
  W2 `argv()` pattern).
- **Acceptance gates**: 5-gate green + ≥ 20 iteration-pattern programs
  + ≥ 10 method-dispatch programs.
- **Blocks on**: ADR-0050a (break/continue inside dict-loop), ADR-0050b
  (for-loop shape), sub-sprint d (iter shims).

#### Sub-sprint f — Drop schedule + stress testing

- **D-rating**: D3
- **Wall**: ~2 days
- **Model**: sonnet PAIR
- **Touch**: `crates/cobrust-mir/src/drop.rs` (extend drop schedule for
  `Ty::Dict(K, V)` and nested-dict cases); `cobrust-codegen` drop
  terminator routing for dict; stress-test corpus
  (`crates/cobrust-stdlib/tests/dict_stress.rs`): ≥ 1000-element dicts,
  insert/delete/iterate loops, valgrind / memlog clean.
- **Acceptance gates**: 5-gate green + valgrind-clean on a 10k-entry
  word-count program + dropped-dict-after-move rejected at borrow-check.
- **Blocks on**: ADR-0050c Str-ownership (Str-keyed drop), sub-sprint d.

#### Sub-sprint g — Doc + examples + getting-started

- **D-rating**: D1
- **Wall**: ~1 day
- **Model**: sonnet solo (low-risk doc-update sprint; per
  `feedback_quantitative_claims_verify.md` CTO greps the final numeric
  claims before merge)
- **Touch**: `docs/human/zh/getting-started.md` (+ a §"Dict 入门" block),
  `docs/human/en/getting-started.md` mirror, `docs/agent/modules/stdlib.md`
  (add dict surface), `docs/agent/modules/types.md` (NotHashable
  taxonomy), `examples/word_count.cb`, `examples/lookup_table.cb`,
  `examples/json_obj.cb`, README "Quick Start" §dict.
- **Acceptance gates**: 5-gate green + doc-coverage exit 0 + each example
  compiles + runs against expected stdout.
- **Blocks on**: a-f closure (this is the wrap).

### Test corpus targets (Wave 3 binding, per ADR-0050 §M-F.3.4)

- ≥ **80 well-typed** dict programs: literal construction, indexing read,
  indexing write, comprehension, iteration (all 4 modes: `for k in d`,
  `keys`, `values`, `items`), `len`, `in`, `.get`, `.copy`, equality,
  nested dicts, dict-as-fn-param, dict-as-fn-return, dict in if/while.
- ≥ **40 ill-typed**: f64-key rejected, mixed K rejected, mixed V
  rejected, implicit-truthiness rejected, mutable-default rejected,
  spread-in-non-comp rejected, dict.method-on-non-dict rejected, etc.
- ≥ **20 differential vs Python 3.10** on insertion-order programs:
  word_count over a fixed string outputs identical sorted-by-insertion
  keys; JSON-object iteration preserves the document's field order.
- ≥ **1024 proptest fuzz** inputs: random (K, V) shapes × random op
  sequences (insert, lookup, delete, iterate, clone, equality);
  property: no panic outside the documented missing-key abort case.

### Examples shipped (Wave 3 sub-sprint g)

Per ADR-0050 §M-F.3.4 examples list:

- `examples/word_count.cb` — read stdin lines, increment dict entries,
  print sorted counts.
- `examples/lookup_table.cb` — `dict[i64, str]` from a fixed table,
  user-input lookup.
- `examples/json_obj.cb` — toy JSON-object builder + pretty-print
  (substring-level; full JSON parser is M-F.3.7).

## Open questions / spikes worth carving out

These four items are explicitly **not** resolved by this ADR; each is a
candidate for a Wave-3 spike commit before the relevant sub-sprint
dispatches. Surface them now so they don't ambush mid-sprint.

### Q1 — Hash trait surface: user-visible or codegen-internal?

**Question**: Do we expose `impl Hash for str` / `impl Hash for i64` as
user-visible trait machinery, or is the hashability check **purely
internal** to the codegen (codegen branches on the resolved K type and
emits the appropriate C-ABI call)?

**Recommendation**: codegen-internal for Phase F.3. The user sees
`TypeError::NotHashable` at well-typed/ill-typed boundary; Hash itself
is not a Cobrust-source-level trait until Phase G traits ship.

**Risk if wrong**: forces a breaking change at Phase G when traits are
introduced. **Mitigation**: ADR-0006 enum `Ty` already lacks a Trait
variant; adding `impl Hash for T` at Phase G is purely additive at the
type universe; the codegen's existing K-dispatch can transparently
extend to "call the user's `__cobrust_hash_<TypeId>` helper" without
breaking existing source.

### Q2 — `d[k]` panic vs `Option[V]`: did we choose right?

**Question**: ADR-0050d Decision 2 chose panic + `.get(k)` safe escape.
Should we have chosen `d[k] -> Option[V]` instead, forcing Result-default
ergonomics?

**Recommendation**: stay with panic. The wedge audience (LeetCode user,
external first user) writes `d[k]` 50× per program; forcing `.unwrap()`
or pattern-match at every site is a productivity tax that outweighs
the §2.2 Result-default reading. The `.get(k) -> Option[V]` escape
hatch is the constitution-compliant path for code that needs it.
Document the abort contract explicitly in zh + en getting-started.

**Risk if wrong**: external audit raises "panic-default contradicts
§2.2". **Mitigation**: §2.2 says "*exceptions* as default error path"
should be replaced by `Result`; a **panic** is not an exception in
the unrecoverable sense — it's an abort-on-bug, matching Rust's
`slice[oob]` and `unwrap()`. The Cobrust contract is: indexing a
missing key is a programmer bug, not a recoverable error. If the
audit pushes back, ADR-0050d-v2 can flip to Option without a Phase
F.3 ABI break (just a source-level surface change; the C-ABI shim
already returns 0/sentinel today).

### Q3 — f64 keys: rejected at type-check or at runtime?

**Question**: We chose to reject `f64` keys at the type-check (ADR-0050d
Decision 7A: `NotHashable { actual: Ty::Float }`). Should we instead
let `f64` keys through and document NaN's surprise at runtime?

**Recommendation**: stay with type-check rejection. The constitution
§2.2 forbids "silent coercion"; NaN != NaN means a `f64`-keyed dict's
insert + lookup of NaN silently disagrees, which is exactly the kind
of silent surprise §2.2 forbids. Type-check rejection is the safest
+ honest path; users wanting f64-like keys can convert to `i64` via
`f.to_bits() as i64` or use a string repr.

**Risk if wrong**: numerical-tier users (NumPy translation) need
`dict[f64, V]`. **Mitigation**: numerical tier is M7+ per ADR-0012;
when it lands, the Hash trait surface (Q1) can ship a user-defined
`impl Hash for f64` that canonicalizes NaN to a single bit pattern,
preserving determinism.

### Q4 — Unified vs per-type codegen for List + Dict?

**Question**: List and Dict are both opaque-pointer-backed aggregates
with parametric element types. Should we refactor the codegen to a
single **generic-aggregate** path (Phase G material) or keep them as
separate codegen arms with duplicate skeleton code?

**Recommendation**: keep separate for Phase F.3. The refactor is a
3-5 day Phase G item that adds risk to Wave 3 (per `feedback_p9_clippy_stall_pattern.md`,
broad refactors over test-heavy code paths stall on clippy-pedantic).
Phase F.3's goal is "ship dict honestly"; a refactor is a Phase G
follow-up after both list[str] (ADR-0050c) and dict are honest.

**Risk if wrong**: ~200 LoC of duplicate codegen between list and dict.
**Mitigation**: acceptable Phase F.3 tax; Phase G's `Aggregate<T>`
generic codegen ADR (not yet drafted; queued for post-v0.2.0) lifts
the duplication.

## Cross-references + dependencies

### Wave 3 dependency edges

- **Blocks on (must close first)**:
  - ADR-0050a (break/continue) — Wave 1; needed by sub-sprint e iter loop.
  - ADR-0050b (for-loop shape) — Wave 1; needed by sub-sprint e iter desugar.
  - ADR-0050c (Str ownership) — Wave 2; needed by sub-sprint d (str-keyed
    shims) and sub-sprint f (str-keyed drop).
- **Relates to (cited but not blocking)**:
  - ADR-0003 (core 30 forms) — `Dict` literal is form 23; this ADR
    materializes its semantics.
  - ADR-0006 (type system) — soundness obligations 1-9; the dict surface
    respects all 9 (see audit table in §"Type-checker amendments").
  - ADR-0019 (Phase E roadmap) — M11 stdlib + runtime; dict shim
    surface extends this.
  - ADR-0023 (M9 codegen) — Aggregate Rvalue stub deferral.
  - ADR-0025 (M11 stdlib + runtime) — `__cobrust_dict_*` original
    M12.x stub origin.
  - ADR-0027 (M12.x codegen amendments) — Aggregate / Ref / Cast / for /
    f-string deferral; this ADR closes the dict half of Aggregate.
  - ADR-0030 (M11.1 print_int) — intrinsic-rewrite precedent.
  - ADR-0034 (M11.2 FnRef Call lowering) — call-symbol dispatch model.
  - ADR-0035 (M11.3 while/if condition primitive) — lower_condition shared
    primitive, reused by dict-iter while-shape.
  - ADR-0038 (Phase F roadmap) — long-range frame.
  - ADR-0044 (stdin/argv source binding) — PRELUDE + intrinsic-rewrite
    precedent reused by sub-sprint e for `.keys()` / `.values()` /
    `.items()` / `.get()` / `.copy()`.
  - ADR-0048 (AI-native framing) — context for v0.2.0 binding.
  - ADR-0049 (alpha honesty + onboarding) — context for v0.2.0 binding.
- **Future / superseded by**:
  - Phase G iter-protocol generalization (`for x in d:` shorthand for
    `.keys()`; today already handled by `iter_element(Dict) = K` in
    `check.rs:451`, so no future ADR strictly needed unless the
    Phase G iter trait surface lands; this ADR is forward-compat).
  - Phase G traits (`impl Hash for UserType`) — extends K beyond
    `{i64, str}` to any user-defined hashable type without breaking
    this ADR's surface.
  - Phase G dict patterns in `match` — exhaustiveness obligation 5 is
    intentionally non-applicable to Phase F.3 dict.
  - Phase G deep-copy / `dict.deepcopy()` — extends Decision 10.
  - Phase G dict-merge `{**a, **b}` — extends Decision 1 spread
    rejection.

### Evidence anchors

- User prioritization 2026-05-16 — verbatim in ADR-0050 §"Strategic
  frame" P0 list "dict ← blocks everything, 2 weeks, depth task".
- LC-100 Pattern B finding — `docs/agent/findings/lc100-pattern-b-list-of-str-gap.md`
  (orthogonal but adjacent gap; both close in Phase F.3).
- ADR-0048 §"Implementation map" — 9-surface atomic Phase 8 batch
  precedent for multi-sprint dispatch over a single feature surface.
- ADSD methodology — `https://github.com/Cobrust-lang/agent-driven-development`
  (P10 role + 5-gate verification + two-phase dispatch SOP +
  external-review constraint).
- Host routing memo — `feedback_heavy_build_offload_to_workstation.md`
  (DG primary for D4-D5 cargo builds).
- Sub-agent model tier — `feedback_subagent_model_tier.md` (D-matrix
  + Opus/Sonnet/Haiku binding; D4 → opus PAIR, D3 → sonnet PAIR).
- Dev/Test PAIR pattern — `cto_operations_runbook.md` §"Dev/test pair
  pattern" (D1-D3 + D5 mandatory PAIR; D5-design ADR-authoring solo).
- M12.x stub source — `crates/cobrust-stdlib/src/collections.rs:534-636`
  (existing `Dict<i64,i64>` C-ABI shape; replaced by sub-sprint d).
- AST + parser + types + MIR existing dict scaffolding — see file:line
  anchors in §"Existing scaffolding" table above.

## Consequences

### Positive

- **Locks the dict surface for Wave 3** — P9-F dispatch has a complete
  blueprint. Sub-sprints a-g are pre-scoped with D-rating, model tier,
  wall estimate, dependency edges, and acceptance gates. No ambiguity
  to litigate mid-sprint.
- **Closes ADR-0027 §1 dict-half deferral** — the M12.x stub
  `__cobrust_dict_*` C-ABI is honestly upgraded from `HashMap<i64,i64>`
  toy to `indexmap::IndexMap<KeyEnum, ValueEnum>` production-shape with
  full source-level surface.
- **Constitution-faithful** — §2.1 keep-list dict semantics ship in
  full; §2.2 drop-list `if d` / silent-coercion / `is`-identity all
  rejected at type-check; §5.1 elegant "one way per thing" for
  literal/index/membership/length/iteration; §5.2 scientific evidence
  trail on every decision; §5.3 efficient AOT + indexmap (no GC).
- **Python-mental-model preserved** — `{}` is dict, `d[k]` reads,
  `key in d` is bool, `len(d)`, `for k in d:` iterates keys in
  insertion order. LeetCode wedge audience reads `examples/word_count.cb`
  and feels at home in 30 seconds.
- **Forward-compat with Phase G** — Q1 (Hash trait), Q3 (f64 keys via
  user-defined Hash), Q4 (generic Aggregate codegen) all extend this
  surface additively. No Phase G change requires breaking this ADR.

### Negative

- **24+ new C-ABI shims** add ~600 LoC to `cobrust-stdlib/src/collections.rs`.
  Per `feedback_p9_clippy_stall_pattern.md`, large stdlib expansions can
  trigger clippy-pedantic stalls; sub-sprint d must apply the
  module-level `#![allow(...)]` test-pedantic discipline preemptively.
- **`indexmap = "2"` workspace dep** adds a new transitive dependency
  tree (equivalent + hashbrown). Acceptable per dep-tree audit but
  worth tracking; the M9 LLVM backend swap (Phase F.5) may want to
  audit `indexmap`'s C-FFI surface separately.
- **Panic-on-missing-key contract** is a documented divergence from
  the §2.2 Result-default reading. Mitigation in Q2: it's an abort,
  not an exception; mirrors Rust `slice[oob]`. External audit may
  push back; ADR-0050d-v2 escape hatch is documented.
- **Method-dispatch in sub-sprint e** (`.keys()` / `.values()` / `.items()`
  / `.get()` / `.copy()`) is implemented as intrinsic-rewrite, not
  via a real method-resolution machinery. Same surface tension as
  ADR-0044 `argv()` (Cobrust-source vs Rust-side `std::env::args()`);
  Phase G method-resolution ADR (not yet drafted) cleans this up.
- **No mutation-during-iteration enforcement** at compile time —
  matches Python but is a known UB. Phase G aliasing-style enforcement
  closes the gap.
- **Str-keyed dict blocks on ADR-0050c** — Wave 3 sub-sprint d cannot
  start str-keyed shims until Str-ownership lands (Wave 2). The
  dependency ordering already accounted for in ADR-0050's Wave plan.

### Neutral / unknown

- **Whether `d[k]` panic-handler should print K's repr in the abort
  message** — open. Recommended yes for debuggability; sub-sprint d
  decides. `__cobrust_dict_keyerror_abort_i64(k: i64)` already has
  the value; the str variant requires copying the Str's bytes into
  the panic message.
- **Whether to support `del d[k]` source-level** — open. The runtime
  helper `__cobrust_dict_remove_<K_V>` is trivial to add; the parser
  doesn't yet emit a `Stmt::Del { target }` AST node for index
  targets. Phase G adds the parser path; for Phase F.3 ship without
  `del`. Users can write `d2 = {k:v for k,v in d.items() if k != bad}`
  as a workaround in dict-comp once sub-sprint c closes.
- **Whether dict comprehension `{k: v for ...}` should also accept the
  `**spread` syntax inside the element** — open. Phase F.3 ships
  without; Phase G dict-merge ADR adds.
- **Whether the 8th obligation of ADR-0006 (mutable-default rejection)
  needs a Phase F.3-specific test asserting `def f(d: dict = {})`
  fails** — yes, sub-sprint b corpus includes this exact program.

## Evidence

### Existing surface — grep evidence (HEAD `30cf2b2`)

```
cobrust-frontend/src/ast.rs:390         DictEntry::Pair / Spread
cobrust-frontend/src/parser.rs:1470     parse_brace_expr (already handles {} / {k:v} / {x} / {**rest})
cobrust-hir/src/lower.rs                ExprKind::Dict (HIR side wired)
cobrust-types/src/ty.rs:65              Ty::Dict(Box<Ty>, Box<Ty>) parametric exists
cobrust-types/src/check.rs:614          synth_expr Dict already unifies K/V over entries
cobrust-types/src/check.rs:451          iter_element(Dict(K,_)) -> K (so for x in d: keys-mode works today)
cobrust-types/src/check.rs:881          BinOp::In/NotIn returns Bool, uses iter_element
cobrust-types/src/check.rs:245          MutableDefault rejection (covers def f(d: dict = {}))
cobrust-mir/src/tree.rs:387             AggregateKind::Dict enum variant
cobrust-mir/src/lower.rs:1111-1137      lower ExprKind::Dict to Rvalue::Aggregate(Dict, interleaved-k-v ops)
cobrust-stdlib/src/collections.rs:534-636  M12.x __cobrust_dict_{new,set,get,len,drop} stub (i64,i64 only)
cobrust-codegen/src/cranelift_backend.rs   runtime_helper_signatures + Aggregate Cranelift lowering (M9 stub for dict path)
Cargo.toml [workspace.dependencies]     no indexmap; sub-sprint d adds
```

### Surface coverage matrix (Phase F.3 binding)

| User-facing surface | K∈ | V∈ | Wave 3 sub-sprint | Status pre-Wave-3 |
|---|---|---|---|---|
| `{k: v, ...}` literal | i64, str | i64, str | a (parser ✅), c (MIR) | parser done, MIR partial |
| `d[k]` read | i64, str | i64, str | c | M12.x stub, untyped |
| `d[k] = v` write | i64, str | i64, str | c | M12.x stub, untyped |
| `key in d` | i64, str | any | c | wired but lowers to list-contains placeholder |
| `len(d)` | any | any | d | M12.x stub via __cobrust_dict_len |
| `for k in d:` | i64, str | any | e (depends on Wave 1) | type-check works, MIR desugar TBD |
| `for k, v in d.items():` | i64, str | i64, str | e | not wired |
| `d.keys()` / `.values()` / `.items()` | i64, str | i64, str | e (intrinsic-rewrite) | not wired |
| `d.get(k)` returns `Option[V]` | i64, str | i64, str | e (depends on typed Option) | not wired; may scope-cap to Phase G if Option lowering not in place |
| `d.copy()` shallow | any | any | e | not wired |
| `{k: v for ...}` dict comp | i64, str | i64, str | c | parser done, MIR extension |
| `d1 == d2` structural | any | any | c (relops) | wired but lowers to ptr-eq placeholder |
| `dict[f64, V]` | f64 | — | b (rejected) | not rejected today |
| `if d:` | — | — | b (rejected via existing ImplicitTruthiness path) | rejected today already |
| `def f(d: dict = {})` | — | — | b (rejected via existing MutableDefault path) | rejected today already |

**Caveat on `d.get(k) -> Option[V]`** — Phase F.3 ships this **iff** typed
`Option[T]` lowering is in scope; if it isn't, sub-sprint e scope-caps
`.get()` to a sentinel-pair `(present: bool, value: V)` return tuple,
matching ADR-0044 W2 Phase 2 scope cap precedent. The full Option-typed
return then lands alongside the Result-typed `read_line()` follow-on
(ADR-0044a Phase F.1.x candidate). **Recommendation**: defer `.get(k)`
to a Phase F.3-late sprint or accept the sentinel-pair scope cap; either
way, the user-facing `d[k]` panic path is unaffected. Sub-sprint b
captures the decision.

## Why this ADR now

ADR-0050 §M-F.3.4 named dict as Wave 3 and slotted ADR-0050d as the
Wave-1 design spike. P10 dispatch SOP per `cto_operations_runbook.md`:
D5 design ADRs are P9 opus solo, no PAIR overhead, 4-12 hour wall
budget. This ADR lands at the Wave-1 close window so the audit
teammate (per ADR-0050 §"Audit model") can review it in parallel with
Wave-2 dispatch, and Wave 3 P9-F can dispatch immediately on Wave-2
close without ADR-drafting overhead in the critical path.

Phase F.3 v0.2.0 stable binding depends on M-F.3.4 closing; M-F.3.4
closing depends on this ADR + sub-sprints a-g. The Wave plan is
tight; this ADR is the design-side prerequisite to keep the wall
clock honest.

— P9 opus tech-lead, 2026-05-16
