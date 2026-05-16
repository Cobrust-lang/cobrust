---
doc_kind: adr
adr_id: 0050e
title: "M-F.3.5 string stdlib design (split / join / replace / trim / find / contains / starts_with / ends_with / lower / upper + clone)"
status: accepted
date: 2026-05-16
last_verified_commit: 8c104cc
supersedes: []
superseded_by: []
relates_to: [adr:0019, adr:0024, adr:0025, adr:0027, adr:0034, adr:0044, adr:0044a, adr:0049, adr:0050, adr:0050b, adr:0050c, adr:0050d]
discovered_by: ADR-0050 §"P1 follow-ups" §M-F.3.5 prereq for v0.2.0 stable tag; P9-G design-only sprint on `feature/f3-string-stdlib`
ratification_path: in-session review per ADR-0050 §"Audit model — teammate-in-session"
---

# ADR-0050e: M-F.3.5 string stdlib design (split / join / replace / trim / find / contains / starts_with / ends_with / lower / upper + clone)

## Context

### Wedge framing — what M-F.3.5 unlocks

Phase F.3 P0 (M-F.3.0..M-F.3.4) closes the language-half completeness baseline
that the project owner's 2026-05-16 prioritization called out as gating
"the language being a language". ADR-0050 §"P1 follow-ups" names §M-F.3.5 as
the next user-traction surface after P0: a string-processing surface
exposes the daily-program shape (Python-class user code: log parsing,
CSV slicing, simple text transforms) that today's W2 Phase 3 helpers
(`str_at` / `str_len` / `str_eq` / `parse_int_tok` / `count_toks`) only
hint at.

The ten-fn surface ADR-0050 §"P1 follow-ups" names is:

```
split / join / replace / trim / find / contains
  / starts_with / ends_with / lower / upper
```

This ADR locks the design — option pick, signatures, ownership semantics,
sub-sprint decomposition — before the M-F.3.5 P10-direct PAIR dispatches.

### Inheritance from ADR-0050c Option A

ADR-0050c §"Decision" picked **Option A — Full-Drop schedule + explicit
`__cobrust_str_clone`; Str non-Copy uniformly across operand-level and
drop-level.** The list[str] DEV recovery introduced a Phase 2a walk-back
for `Ty::List(_)` (Copy-at-operand but non-Copy-at-drop, see
`crates/cobrust-mir/src/lower.rs:1936-1962`); the **Str walk-back was
NOT applied** per `findings/lc100-str-use-after-move-regression-from-adr0050c.md`
Path D honest-debt disposition.

The disposition has a binding consequence for every M-F.3.5 surface fn:

- **Every Str parameter is Move-only at the call site.** A `fn f(s: str)`
  call written as `f(s)` consumes `s`; `s` is unreachable after the call
  without an explicit copy / re-binding.
- The existing W2 Phase 3 PRELUDE shape (`fn str_len(s: str) -> i64`)
  already has this hazard. LC-100 reverse_string's
  `let n = str_len(s); let c = str_at(s, i)` pattern is the documented
  honest-debt baseline.
- M-F.3.5 surface MUST NOT widen this hazard. It MUST surface a
  source-level mitigation in the same sprint (this ADR proposes
  `clone(s: str) -> str`).

### Constitution alignment

| Clause | This ADR's adherence |
|---|---|
| §2.1 "f-strings the best string format in any language" | M-F.3.5 surface complements f-strings; f-strings build new Str from holes, M-F.3.5 transforms existing Str. `join` returns `str` and slots into f-string `{}` holes via the existing `__cobrust_fmt_str` dispatch (`fmt.rs:212`). |
| §2.1 "iteration protocols" | `split` returns `list[str]` which slots into `for x in xs:` length-bound iter (ADR-0050b) and `for x in xs:` element-drop-on-Str (ADR-0050c Phase 2). |
| §2.2 "no silent coercion" | Every surface fn's args are typed `str` (or `list[str]` for `join`); no implicit i64→str promotion. |
| §2.2 "no implicit truthy/falsy" | `contains` / `starts_with` / `ends_with` return `bool` explicitly; `find` returns `i64` with `-1` sentinel (decision below) so users write `if find(s, x) != -1:` not `if find(s, x):`. |
| §2.3 "Adopt from Rust — ownership" | The Str=non-Copy semantics inherited from ADR-0050c remain in force; every M-F.3.5 fn that takes Str moves it. Mirrors Rust `String::split(self, …)` shape (where applicable) and `&str` borrow (where Cobrust can't yet express that). |
| §5.1 "one way to do each thing" | Single PRELUDE-fn form chosen (Option B below). No method-call sugar in Phase F.3. |
| §5.3 "efficient — allocations visible via the type system" | Every Str-returning surface fn emits a new heap StringBuffer; every i64/bool-returning surface fn allocates nothing. The MIR-dump count of Str allocations per program is auditable. |

### Existing W2 Phase 3 + AI alpha precedent

The PRELUDE-fn + intrinsic-rewrite + C-ABI shim trio is the W2 Phase 3
shape (ADR-0044 §"W2 Phase 3"). The M-F.3.5 surface follows it
verbatim. Per `crates/cobrust-cli/src/build/intrinsics.rs:686-727`
the `Kind` enum already enumerates the eight W2 Phase 3 str helpers
(`StrLen`, `StrAt`, `StrEq`, `StrEqLit`, `StrOrd`, `ParseIntTok`,
`CountToks`, `ParseInt`) plus the ADR-0049 input helpers; M-F.3.5
adds eleven new variants (the ten surface fns + `clone`).

The math intrinsics added in M-F.3.3 (`sqrt`/`floor`/`ceil`/`round`/
`abs`/`pow`/`sin`/`cos`/`tan`/`log`/`exp` — see
`intrinsics.rs:715-727`) are the most-recent precedent for the
"add N PRELUDE stubs + N intrinsic-rewrite arms + N C-ABI signatures"
pattern; M-F.3.5 mirrors it surface-for-surface.

## Verified-at-HEAD (F27 SOP)

Per `findings/adr-scope-reality-divergence.md` F27 SOP, every claim
below was cross-checked at `HEAD=0ddcd27` via the cited
`grep`/`sed`/Read call.

| Surface fn | Rust-side `pub fn` | Rust-side `pub extern "C" fn __cobrust_str_*` | PRELUDE stub | intrinsic-rewrite arm | C-ABI signature in `runtime_helper_signatures` | Classification |
|---|---|---|---|---|---|---|
| `split` | `crates/cobrust-stdlib/src/string.rs:35` (`split(s, sep) -> Vec<String>`) | **none** (no `__cobrust_str_split`) | **none** | **none** | **none** | **partially-shipped** — Rust impl exists; C-ABI + PRELUDE + intrinsic missing |
| `join` | **none** (Rust-side `string.rs` does not expose `join`; only `format`) | **none** | **none** | **none** | **none** | **not-yet** — needs Rust impl + C-ABI + PRELUDE + intrinsic |
| `replace` | `string.rs:27` | **none** | **none** | **none** | **none** | **partially-shipped** |
| `trim` | `string.rs:43` (named `strip`, NOT `trim` — naming collision with Python `str.strip()` vs Rust `str::trim()`; resolved at Decision §"Naming") | **none** | **none** | **none** | **none** | **partially-shipped (renaming required)** |
| `find` | `string.rs:22` (returns `Option<usize>`) | **none** | **none** | **none** | **none** | **partially-shipped (signature shift required: Option → i64 sentinel)** |
| `contains` | **none** (Rust `str::contains` is intrinsic; not wrapped) | **none** | **none** | **none** | **none** | **not-yet** |
| `starts_with` | **none** | **none** | **none** | **none** | **none** | **not-yet** |
| `ends_with` | **none** | **none** | **none** | **none** | **none** | **not-yet** |
| `lower` | `string.rs:50` | **none** | **none** | **none** | **none** | **partially-shipped** |
| `upper` | `string.rs:55` | **none** | **none** | **none** | **none** | **partially-shipped** |
| **`clone`** (proposed mitigation, see §Decision) | **none** at source-level; **`__cobrust_str_clone`** at `crates/cobrust-stdlib/src/fmt.rs:306` exists end-to-end | **`__cobrust_str_clone`** at `fmt.rs:306` | **none** (PRELUDE absent) | **none** (intrinsic-rewrite arm absent) | **none** in `runtime_helper_signatures` | **partially-shipped (most advanced)** — the underlying shim is the most complete of the eleven |

Greppable verification (each row above):

```
$ grep -rn "^pub fn " crates/cobrust-stdlib/src/string.rs
crates/cobrust-stdlib/src/string.rs:17:pub fn len(s: &str) -> usize {
crates/cobrust-stdlib/src/string.rs:22:pub fn find(s: &str, pat: &str) -> Option<usize> {
crates/cobrust-stdlib/src/string.rs:27:pub fn replace(s: &str, from: &str, to: &str) -> String {
crates/cobrust-stdlib/src/string.rs:35:pub fn split(s: &str, sep: &str) -> Vec<String> {
crates/cobrust-stdlib/src/string.rs:43:pub fn strip(s: &str) -> &str {
crates/cobrust-stdlib/src/string.rs:50:pub fn lower(s: &str) -> String {
crates/cobrust-stdlib/src/string.rs:55:pub fn upper(s: &str) -> String {

$ grep -rn "__cobrust_str_split\|__cobrust_str_join\|__cobrust_str_replace\
    \|__cobrust_str_trim\|__cobrust_str_find\|__cobrust_str_contains\
    \|__cobrust_str_starts_with\|__cobrust_str_ends_with\
    \|__cobrust_str_lower\|__cobrust_str_upper" crates/
(no matches)

$ grep -n "extern \"C\"" crates/cobrust-stdlib/src/fmt.rs | grep -i "clone\|drop\|new"
crates/cobrust-stdlib/src/fmt.rs:75:pub unsafe extern "C" fn __cobrust_str_new() -> *mut u8 {
crates/cobrust-stdlib/src/fmt.rs:284:pub unsafe extern "C" fn __cobrust_str_drop(buf: *mut u8) {
crates/cobrust-stdlib/src/fmt.rs:306:pub unsafe extern "C" fn __cobrust_str_clone(buf: *mut u8) -> *mut u8 {

$ grep -n "Kind::\|kind_for_name" crates/cobrust-cli/src/build/intrinsics.rs | head -5
crates/cobrust-cli/src/build/intrinsics.rs:680:#[derive(Copy, Clone, Eq, PartialEq)]
crates/cobrust-cli/src/build/intrinsics.rs:729:fn kind_for_name(name: &str) -> Option<Kind> {
```

**Net assessment**: of the eleven targeted surface fns,
- 6 are partially-shipped (Rust-side helper exists but no C-ABI/PRELUDE/intrinsic plumbing): `split` / `replace` / `trim` (renaming required) / `find` (signature shift required) / `lower` / `upper`
- 4 are not-yet (no Rust-side helper, no plumbing): `join` / `contains` / `starts_with` / `ends_with`
- 1 (`clone`) is most-advanced: full C-ABI shim ships at `fmt.rs:306`; only PRELUDE+intrinsic-rewrite missing

This breakdown drives sub-sprint sizing (§Implementation map below).

### Dependency framing for v0.2.0

ADR-0050 §"v0.2.0 stable tag binding" §3 names M-F.3.5 + M-F.3.6 as
required for v0.2.0 stable. The dependency edges:

```
M-F.3.2 list[str] ──────┐
ADR-0050c Str=non-Copy ─┤ (drop schedule for split's return + Str arg moves)
ADR-0050b for-loop ─────┼─→ M-F.3.5 string stdlib ─→ M-F.3.7 JSON parser ─→ v0.2.1+
                        │                       └─→ better corpus / examples ─→ v0.2.0 stable
ADR-0044 W2 PRELUDE ────┘
```

M-F.3.5 inherits the LC-100 honest-debt: source programs using
multiple M-F.3.5 surface fns over the same Str must `clone(s)` explicitly
between calls or face `UseAfterMove`. The Phase G closure (explicit borrow
forms or compiler-inserted clone) re-greens both LC-100 corpus and any
M-F.3.5 corpus tests that lock the same hazard.

## Options considered

### Option A — Method-call form `s.split(",")`

**Mechanism**: parse `s.method(...)` as a method-call expression; the
type checker resolves `method` against a per-type method table; the
HIR/MIR lowering emits a `Terminator::Call` to a name-mangled function
(e.g. `__cobrust_str_split` or `str.split`).

**Pros**:
- Python-familiar one-to-one: `s.split(",")` is verbatim Python.
- Composable on chains: `s.trim().lower().split(",")` reads left-to-right
  in evaluation order.

**Cons**:
- **Needs method-dispatch infrastructure that does not yet exist.**
  `crates/cobrust-mir/src/lower.rs` has zero method-call support
  outside of for-protocol's opaque iter handle (ADR-0027). Method
  resolution against per-type tables is undisclosed scope; user-defined
  types' method dispatch is Phase G post-recursive-types per ADR-0050
  §A3.
- **Phase F.3 freezes scope on existing PAIR-pattern shapes.** Method
  dispatch is a multi-day spike that the F.3 timeline (3-5 days for
  M-F.3.5 P10-direct PAIR) does not absorb.
- **Inconsistency risk.** M-F.3.5 ships method-call sugar while
  M-F.3.2 list[str] uses `list_set(xs, i, v)` PRELUDE-fn form; Phase
  G must then retrofit method-call sugar on lists. Two-step migration
  is worse than one-step.

### Option B — PRELUDE-fn form `split(s, ",")` (CHOSEN)

**Mechanism**: every surface fn is declared as a PRELUDE stub
(`crates/cobrust-cli/src/build.rs` PRELUDE constant). Every call site
in `.cb` source resolves to the PRELUDE stub at type-check time; the
intrinsic-rewrite pass at `crates/cobrust-cli/src/build/intrinsics.rs`
recognizes the call by `Kind` enum + `kind_for_name` and rewrites the
MIR `Terminator::Call`'s `func` operand to point at the C-ABI runtime
symbol (`__cobrust_str_split`, …). The C-ABI shim lives in
`crates/cobrust-stdlib/src/string.rs` (new C-ABI surface bolted onto
the existing Rust-side helpers).

**Pros**:
- **Zero new dispatch infrastructure.** Mirrors W2 Phase 3 (`str_at` /
  `str_eq` / `str_ord` / `parse_int_tok` / `count_toks`) and M-F.3.3
  math intrinsics (`sqrt` / `floor` / `pow` / `sin` / …) verbatim.
- **One sprint to ship eleven surfaces.** Each surface is 1 PRELUDE
  line + 1 `Kind` variant + 1 `kind_for_name` arm + 1
  `runtime_helper_signatures` line + 1 C-ABI shim wrapping the
  existing Rust-side `string.rs` helper. The decomposition is
  embarrassingly parallel within the DEV sub-sprint.
- **Constitution §5.1 "one way" preserved.** All Phase F.3 stdlib
  surfaces use the same shape; users learn one calling convention.
- **PAIR-pattern friendly.** ADR-0050 §A7 P10-direct PAIR shape
  (TEST + DEV parallel) maps cleanly: TEST authors the well-typed
  / ill-typed corpus exercising eleven surface fns at source level;
  DEV adds the eleven PRELUDE stubs + eleven intrinsic-rewrite arms
  + eleven C-ABI shims in a single crate touch (`cobrust-stdlib`,
  `cobrust-cli`, `cobrust-codegen`).

**Cons**:
- **No method-chain syntax.** `s.trim().lower().split(",")` instead
  reads `split(lower(trim(s)), ",")`. This is the same inversion W2
  imposed on `str_at(str_at(s, 0), 1)` — users adapt; Phase G adds
  method-call sugar.
- **Phase G migration is non-zero.** When method-call sugar lands,
  every PRELUDE-fn form `split(s, ",")` and method-call form
  `s.split(",")` will coexist (consistent with §C below). Phase G
  must decide whether the PRELUDE form deprecates or stays as
  alias-of-method. Recommendation: stay-as-alias, no deprecation;
  matches Rust's `&str::split` (method) + `str::split(s, …)`
  (intrinsic-call) duality.

### Option C — Both, sequenced (PRELUDE in F.3 + method in Phase G)

**Mechanism**: ship Option B for Phase F.3; Phase G adds method-call
infrastructure that desugars `s.method(args...)` → `method(s, args...)`
sugar via a parser/HIR pass; the underlying PRELUDE fn + intrinsic-rewrite
machinery survives unchanged.

**Pros**: Phase F.3 ships in the 3-5 day envelope; Phase G adds
ergonomics on top of correctness. No retrofit work — Phase G's
method-call sugar lowers to Option B's PRELUDE-fn call. Both forms
coexist (mirrors Python `len(s)` vs `s.__len__()`, where the function
form is the canonical and the method form is sugar).

**Cons**: defers method-call ergonomics ~4-8 weeks (Phase G is post-v0.2.0).
Users typing `s.split(",")` get a parse error meanwhile; documentation
must warn.

### Option D — Defer M-F.3.5 entirely to Phase G

**Mechanism**: skip the string stdlib; tell users to write
`split` themselves over W2 `str_len` + `str_at` + index loops.

**Pros**: zero work; M-F.3.5 sprint cost is zero.

**Cons**: **violates ADR-0050 §"v0.2.0 stable tag binding" §3** which
makes M-F.3.5 a P1 gate. The user-traction wedge stays open; daily
programs still need split/join/lower/upper to be usable. Defer-the-
deliverable is `feedback_third_party_audit_2026_05_09.md`'s dominant
honesty risk pattern — rejected on the same grounds ADR-0050c §"Option D"
was rejected.

### Recommendation

**Adopt Option C — ship Option B (PRELUDE-fn form) in Phase F.3
M-F.3.5; defer method-call sugar to Phase G.**

Option C is Option B with an explicit forward-pointer. The Phase G
extension is opportunistic (post-recursive-types per ADR-0050 §A3)
and does not block v0.2.0.

## Decision

### Decision 1 — Calling convention: PRELUDE-fn form (Option B)

All ten surface fns + `clone` are PRELUDE stubs. Call sites in `.cb`
parse as ordinary `Call` expressions; intrinsic-rewrite reroutes to
runtime C-ABI symbols. No method-call syntax in Phase F.3.

### Decision 2 — clone() is in scope for M-F.3.5

**Q1 from the mission resolves: add `clone` to M-F.3.5 scope.**

Rationale: the LC-100 honest-debt is structurally identical to every
multi-use Str pattern users will write with M-F.3.5
(`let parts = split(s, ","); let n = str_len(s)` — second `s` use
faults). Without an explicit `clone()`, users must restructure programs
to single-use Str patterns or accept compile errors. With
`clone()`, users mitigate locally: `let parts = split(clone(s), ",")`.

Cost: 1 PRELUDE stub + 1 `Kind` variant + 1 intrinsic-rewrite arm + 1
C-ABI signature in `runtime_helper_signatures`. **The C-ABI shim
`__cobrust_str_clone` already ships at `crates/cobrust-stdlib/src/fmt.rs:306`
end-to-end (ADR-0050c Phase 3 deliverable).** Total marginal cost
in M-F.3.5: ~10 LoC of plumbing.

Benefit:
1. Retroactively unblocks LC-100 corpus (users can `clone(s)` before
   each PRELUDE call). The honest-debt receipt in
   `findings/lc100-str-use-after-move-regression-from-adr0050c.md`
   §"Phase G closure scope" L94 names this exact mitigation
   ("Source-level `clone(s)` builtin OR a `&` borrow form …").
2. Closes Q1 in `findings/lc100-...` without needing the explicit-borrow
   form (a more invasive parser change).
3. Unlocks idiomatic M-F.3.5 programs (`let trimmed = trim(clone(s));
   let parts = split(s, ",")`).

The Phase G `&` borrow form remains the long-term solution; `clone()`
in M-F.3.5 is the explicit-allocate-now interim. Both can coexist —
`clone()` becomes the "I want a deep copy" form, `&` becomes the
"I want a shared read-only view" form. Mirrors Rust's `s.clone()`
vs `&s`.

### Decision 3 — Surface signatures + ownership semantics

The ten surface fns + `clone`:

| # | Fn signature | Ownership (Str args) | Return | Notes |
|---|---|---|---|---|
| 1 | `fn split(s: str, sep: str) -> list[str]` | both Move | new heap `list[str]` (each element fresh Str) | Most common pattern. Inherits list[str] drop schedule (ADR-0050c Phase 2 `__cobrust_list_drop_elems`). Empty sep returns `[s]` per existing Rust-side `string::split` semantics. |
| 2 | `fn join(parts: list[str], sep: str) -> str` | `parts` Move-at-drop (List walk-back Copy@operand per `lower.rs:1958-1962`); `sep` Move | new heap Str | List walk-back means `parts` survives this call at operand-level; at scope-exit it still drops per Phase 2 schedule. `sep` follows Str semantics. |
| 3 | `fn replace(s: str, old: str, new: str) -> str` | all three Move | new heap Str | Three Str args, all consumed. Mitigation: `replace(s, clone(o), clone(n))` if old/new reused. |
| 4 | `fn trim(s: str) -> str` | Move | new heap Str | Named `trim` (Rust + LeetCode familiar), NOT `strip` (existing Rust-side `string::strip` renamed at sub-sprint 3). See Decision 4. |
| 5 | `fn find(s: str, needle: str) -> i64` | both Move | `-1` if absent, else byte index `≥ 0` | i64 sentinel chosen over `Option[i64]` per Decision 5 / Q2. |
| 6 | `fn contains(s: str, needle: str) -> bool` | both Move | `true`/`false` | Direct membership; no allocation. |
| 7 | `fn starts_with(s: str, prefix: str) -> bool` | both Move | `true`/`false` | |
| 8 | `fn ends_with(s: str, suffix: str) -> bool` | both Move | `true`/`false` | |
| 9 | `fn lower(s: str) -> str` | Move | new heap Str | ASCII-fast-path matches Rust `str::to_lowercase`. Full Unicode case-folding deferred to Phase G (matches `string.rs:46-52` existing caveat). |
| 10 | `fn upper(s: str) -> str` | Move | new heap Str | Same caveat. |
| 11 | `fn clone(s: str) -> str` | Move (consumes input) | new heap Str (deep copy) | The s arg IS moved; users typically write `clone(s)` as an rvalue, not `let s2 = clone(s)`. The deep-copy reallocates the StringBuffer. Mirrors `__cobrust_str_clone` at `fmt.rs:306-316` exactly. |

**Critical observation**: under ADR-0050c Option A + Path D, every
Str argument is consumed by the call. Users wanting to reuse a Str
must `clone()` it explicitly per call. This is the LC-100 hazard,
preserved verbatim. **M-F.3.5 does not widen this hazard; it surfaces
the mitigation.**

Example ergonomic shape (M-F.3.5 idiomatic):

```cobrust
fn main() -> i64:
    let s = input("> ")
    if contains(clone(s), "ERROR"):
        let parts = split(clone(s), ":")
        let level = trim(clone(parts[0]))  # if list[str] indexing works honestly
        print(upper(level))
    print(lower(s))  # final use; no clone needed
    return 0
```

The `clone(s)` calls before each non-final use are explicit allocations
that show up in the MIR dump as `__cobrust_str_clone` callsites. Per
constitution §5.3 "allocations visible via the type system", this is
correct.

### Decision 4 — Naming: `trim` not `strip`; `find` not `index`/`indexOf`

- **`trim`** chosen over `strip`. Rationale: existing Rust-side
  `string.rs:43` is `pub fn strip(s: &str) -> &str`. **Sub-sprint 3
  renames** this to `pub fn trim(...)` because:
  - Python's `str.strip()` semantics is "trim whitespace both sides";
    Rust's `str::trim()` is identical.
  - LeetCode users say "trim" colloquially.
  - Python `str.strip(chars)` takes an optional chars argument; Cobrust
    Phase F.3 only ships no-argument trim (whitespace-only); the optional
    argument is Phase G.
  - The Rust-side `strip` was a poorly-considered name (it shadows
    Rust's `str::strip_prefix` / `strip_suffix`). Renaming closes the
    confusion.
- **`find`** chosen over `index_of` / `pos`. Rationale: Python `str.find()`
  is the closest one-to-one (returns -1 if absent). Cobrust's Phase F.3
  uses the i64 sentinel (Decision 5).
- **`contains`** chosen over `has` / `in_str`. Rust + Python + LeetCode
  all converge on `contains`; no ambiguity.
- **`starts_with`** / **`ends_with`** chosen over `prefix` / `suffix`
  predicates. snake_case matches §9 style tokens.

### Decision 5 — `find` returns i64 with -1 sentinel (Q2 resolved)

**Q2 from the mission resolves: i64 with -1 sentinel.**

Trade-off:

| Approach | Pros | Cons |
|---|---|---|
| **`-1` sentinel (CHOSEN)** | Single primitive return; consistent with C `strstr` / Python `str.find()`. No Option newtype required. SwitchInt-friendly. | Constitution §2.2 forbids implicit truthy/falsy; `if find(s, x):` is false for `find -> 0` (match at position 0) and true for both `find -> -1` (absent) AND `find -> 5` (match at 5). Users MUST write `if find(s, x) != -1:`. |
| **`Option[i64]`** | Forces explicit `match` / `unwrap_or(-1)`; safer. | Option type not yet wired through every MIR pass for `i64` payload; requires Aggregate + sum-type lowering (`Option<i64>` is partial today). Adds non-trivial scope to M-F.3.5. |
| **`Result[i64, NotFound]`** | Symmetric with `Result<str, IoError>` from ADR-0044a. | Same scope penalty as Option; semantically over-engineered. |

The i64 sentinel resolves cleanly given existing `__cobrust_str_at` and
`__cobrust_parse_int_tok` precedents (also i64-returning with sentinels
at `io.rs:480-491` and `io.rs:594-608`). Phase G can widen to
`Option[i64]` once Option-of-primitive lowering is robust; the wire
format (i64) stays the same as a `repr(i64)` enum.

**Sentinel doc requirement** (binding for sub-sprint 4): every
M-F.3.5 zh/en/agent doc page for `find` MUST include a "use idiom":

```cobrust
let pos = find(clone(s), "needle")
if pos != -1:
    print("found at pos " + str(pos))
else:
    print("not found")
```

NOT `if find(...):`. This blocks the §2.2 implicit-truthy footgun.

### Decision 6 — Unicode policy: byte-level (Q3 resolved)

**Q3 from the mission resolves: byte-level for Phase F.3, defer grapheme
to Phase G.**

Justification:
- W2 Phase 3 `str_at(s, i)` returns the byte at position `i`, NOT the
  i-th code-point (see `crates/cobrust-stdlib/src/io.rs:480-491`). Every
  M-F.3.5 surface fn must compose with `str_at` consistently; switching
  to grapheme indexing in M-F.3.5 while `str_at` is byte-indexed creates
  a confusing split-personality stdlib.
- `find` returns a **byte** offset (matching Python's
  `str.find()` which also returns a byte offset in `bytes` mode + a
  code-point offset in `str` mode; we pick byte offset to compose with
  `str_at`).
- `contains` / `starts_with` / `ends_with` are byte-content predicates;
  no Unicode tailoring needed.
- `split(s, sep)` matches the **byte sequence** of `sep` in `s`; same
  semantics as Rust `str::split(self, sep: &str)`.
- `lower` / `upper` use Rust `str::to_lowercase` / `to_uppercase` which
  is **Unicode-aware by default** in Rust stdlib (uses
  `core::unicode::conversions`). This is a contradiction with
  byte-level-elsewhere but matches Rust precedent. Users get
  "ASCII fast path is correct + most Unicode case-folding works"
  with the same ~5 KiB extra binary size Rust's stdlib pays.

Phase G consolidation will add `chars()` iterator + grapheme-indexed
variants (`char_at` / `find_char` / `split_char`) once a `char` type
or a `Iterator[str]` newtype lands. Out of M-F.3.5 scope.

### Decision 7 — Case-insensitive variants: deferred (Q4 resolved)

**Q4 from the mission resolves: defer `contains_ignore_case` etc. to
Phase G.**

Justification:
- Phase F.3 surface is already 11 fns (10 + `clone`). Adding 4 case-
  insensitive variants (`contains_ignore_case` /
  `starts_with_ignore_case` / `ends_with_ignore_case` /
  `find_ignore_case`) would bloat to 15 with no user pull yet.
- Workaround until Phase G: `contains(lower(clone(s)), lower(clone(needle)))`.
  Costly (extra clones + extra allocations) but functionally equivalent
  for ASCII-only inputs.
- The Phase G plan ships a single combinator `s.lower().contains(n.lower())`
  when method-call sugar lands; the case-insensitive variants become
  one-line conveniences rather than primitive surface.

### Decision 8 — Empty-input / edge-case semantics

| Case | Behavior | Source / rationale |
|---|---|---|
| `split("", ",")` | Returns `[""]` (singleton list with one empty string) | Mirrors Rust `"".split(",").collect::<Vec<_>>()` → `[""]`. Matches existing `string::split` Rust impl at `string.rs:35-40`. |
| `split(s, "")` | Returns `[s]` (singleton) | Per existing `string::split` impl at `string.rs:36-38` — "Empty separator yields a singleton vector containing the original string" — Python `str.split('')` raises; Cobrust takes the safe path. |
| `join([], ",")` | Returns `""` | Empty list joins to empty Str. |
| `join(["a"], ",")` | Returns `"a"` (no separator emitted) | One-element list omits separator. |
| `replace(s, "", new)` | Inserts `new` at every byte position | Per existing `string::replace` Rust impl at `string.rs:27-29` — "Rust's `str::replace` on empty `from` inserts `to` at every position; we follow that semantic." |
| `trim("")` | Returns `""` | Existing `string::strip` semantics at `string.rs:43-45`. |
| `find(s, "")` | Returns `0` | Empty pattern matches at position 0; mirrors `string::find` existing impl + Python's `str.find('')`. |
| `contains(s, "")` | Returns `true` | Symmetric with `find(s, "") == 0`. |
| `starts_with(s, "")` | Returns `true` | Empty prefix is universal prefix. |
| `ends_with(s, "")` | Returns `true` | Empty suffix is universal suffix. |
| `lower("")` / `upper("")` / `trim("")` / `clone("")` | All return `""` | No-op on empty input; one allocation each (empty StringBuffer). |
| `find` byte-overlap inside multi-byte UTF-8 codepoint | Returns the byte index even if it falls inside a UTF-8 sequence | Byte-level policy (Decision 6). Phase G grapheme-aware variant fixes; for now, the user is responsible for valid UTF-8 needles. |

These edge cases are **binding** for the M-F.3.5 test corpus (sub-sprint
4): every well-typed corpus test exercises at least one edge case from
the table.

### Decision 9 — Comp-lowering 0-sentinel collision (F30 mandatory grep)

Per the F30 SOP, this ADR's §"Consequences" enumerates whether any
M-F.3.5 surface produces a list whose first element could be 0 / null
(triggering the open `comp-lowering-zero-sentinel-collision.md` finding
collision).

Analysis:
- `split` is the only `list[str]`-returning surface fn.
- `split("", ",")` returns `[""]`. The element is an empty Str (pointer
  to a zero-byte StringBuffer), NOT a null pointer.
  `__cobrust_str_new` returns a non-null pointer to an empty buffer
  per `fmt.rs:75-85`.
- `split(",", ",")` returns `["", ""]`. Same — two empty buffers, both
  non-null pointers.
- Therefore: **no element of any split-returned list is a null pointer.**
  The 0-sentinel collision does NOT engage.

However, this analysis depends on `__cobrust_str_new` continuing to
return non-null on empty input. The M-F.3.5 corpus must lock this
contract with a regression test:

```rust
#[test]
fn split_empty_str_yields_non_null_elements() {
    unsafe {
        let s = alloc_str_buffer("");
        let result = __cobrust_str_split(s, alloc_str_buffer(","));
        // result is *mut u8 pointing at a list of *mut u8;
        // result[0] is the i64 reinterpretation of a *mut u8 to an
        // empty StringBuffer. Verify non-null.
        let elem0 = __cobrust_list_get(result, 0);
        assert_ne!(elem0, 0, "split('', ',')[0] must be a valid empty Str, not null");
    }
}
```

This regression test is binding for sub-sprint 3.

### Decision 10 — Sub-sprint decomposition (4 sub-sprints)

Sub-sprint count: **4**, per the mission §"Implementation map". Detailed
in §"Implementation map" below.

### Quick-reference decision table

| Decision | Choice | One-line rationale |
|---|---|---|
| Calling convention | PRELUDE-fn form | Zero new dispatch infra; matches W2 Phase 3 + M-F.3.3 math precedents |
| `clone()` in scope? | **YES, in M-F.3.5** | `__cobrust_str_clone` already ships at `fmt.rs:306`; ~10 LoC marginal cost; closes LC-100 honest-debt mitigation gap |
| `find` return type | `i64` with `-1` sentinel | Composes with SwitchInt + matches Python/C; `if find(...) != -1:` idiom doc-required |
| `trim` vs `strip` name | **`trim`** | Rust + Python + LeetCode converge; existing Rust-side `strip` renames to `trim` |
| Unicode policy | Byte-level | Composes with W2 `str_at` byte-level; grapheme deferred to Phase G |
| Case-insensitive variants | Deferred to Phase G | 4 extra surfaces; no user pull yet |
| Method-call sugar (`s.split(",")`) | Deferred to Phase G | Needs method-dispatch infra; out of F.3 scope |
| Empty-input semantics | Per Decision 8 table | All explicit; corpus locks each |
| 0-sentinel collision | Does not engage (empty Str ≠ null Str) | Regression test locks `__cobrust_str_new` non-null contract |
| Sub-sprint count | 4 | parser+types / MIR+intrinsic / stdlib / docs+corpus |

## Implementation map

Per ADR-0050 §A7 P10-direct PAIR shape: M-F.3.5 dispatches as a
single P10-direct PAIR (TEST opus + DEV opus parallel) with the four
sub-sprints serialized on the DEV side. TEST authors the full
corpus up-front (no sub-sprint partition on TEST side).

### Sub-sprint 1 — Parser/AST/HIR/types stubs (3-4 hours)

**Goal**: type checker accepts all eleven new PRELUDE fn signatures;
ill-typed call sites (wrong arg types / arities) are rejected at
type-check.

**Files touched**:
- `crates/cobrust-cli/src/build.rs` (PRELUDE constant at line 51) —
  add eleven new `fn` declarations:

```cobrust
fn split(s: str, sep: str) -> list[str]:
    let xs: list[str] = []
    return xs

fn join(parts: list[str], sep: str) -> str:
    return ""

fn replace(s: str, old: str, new: str) -> str:
    return ""

fn trim(s: str) -> str:
    return ""

fn find(s: str, needle: str) -> i64:
    return -1

fn contains(s: str, needle: str) -> bool:
    return False

fn starts_with(s: str, prefix: str) -> bool:
    return False

fn ends_with(s: str, suffix: str) -> bool:
    return False

fn lower(s: str) -> str:
    return ""

fn upper(s: str) -> str:
    return ""

fn clone(s: str) -> str:
    return s
```

**Note on `clone()`'s stub body**: it can be either `return ""` or
`return s` — both work because the intrinsic-rewrite pass replaces the
body's callsite. We prefer `return s` because (a) it documents the
move-then-return intent, (b) it gives the type checker a sanity check
that Str-move-then-Str-return is well-typed (regression coverage for
ADR-0050c Phase 5 doc fix).

**No HIR/types changes needed** — the PRELUDE stubs type-check exactly
like existing W2 helpers. `kind_for_name` (next sub-sprint) bypasses
the stub at MIR time, so the stub body is never compiled.

**TEST corpus inputs for sub-sprint 1** (well-typed + ill-typed):
- Well-typed: ≥22 tests (2 per surface fn × 11 fns) exercising basic
  argument types + return-binding via `let x: i64 = find(s, "a")` etc.
- Ill-typed: ≥22 tests covering:
  - `split(123, ",")` → ArgTypeError
  - `split(s)` (1 arg, 2 required) → ArityError
  - `contains(s, x)` where `x: i64` → ArgTypeError
  - `let n: i64 = trim(s)` (return type mismatch) → AssignTypeError
  - `find(s, n)` → ArgTypeError
  - `clone()` (zero args, 1 required) → ArityError
  - etc. for each fn

**Dependencies**: none new; relies on existing PRELUDE plumbing.

**Estimated PAIR shape**: TEST sonnet + DEV sonnet, parallel,
~3-4 hour total wall time (TEST corpus is the long pole).

### Sub-sprint 2 — MIR + intrinsic-rewrite (2-3 hours)

**Goal**: every call site to the eleven new fns is rewritten at MIR
to a C-ABI runtime symbol call.

**Files touched**:
- `crates/cobrust-cli/src/build/intrinsics.rs`:
  - Add eleven new `Kind` enum variants after `MathExp` at line ~727:
    ```rust
    // ---- M-F.3.5 string stdlib ----
    StrSplit,
    StrJoin,
    StrReplace,
    StrTrim,
    StrFind,
    StrContains,
    StrStartsWith,
    StrEndsWith,
    StrLower,
    StrUpper,
    StrClone,
    ```
  - Add eleven new arms to `kind_for_name` at line ~775 (before the
    `_ => None,` catch-all):
    ```rust
    "split" => Some(Kind::StrSplit),
    "join" => Some(Kind::StrJoin),
    "replace" => Some(Kind::StrReplace),
    "trim" => Some(Kind::StrTrim),
    "find" => Some(Kind::StrFind),
    "contains" => Some(Kind::StrContains),
    "starts_with" => Some(Kind::StrStartsWith),
    "ends_with" => Some(Kind::StrEndsWith),
    "lower" => Some(Kind::StrLower),
    "upper" => Some(Kind::StrUpper),
    "clone" => Some(Kind::StrClone),
    ```
  - Add eleven new `pub const` symbols above the `Kind` enum
    (after `MATH_EXP_RUNTIME_SYMBOL` at line ~218):
    ```rust
    pub const STR_SPLIT_RUNTIME_SYMBOL: &str = "__cobrust_str_split";
    pub const STR_JOIN_RUNTIME_SYMBOL: &str = "__cobrust_str_join";
    pub const STR_REPLACE_RUNTIME_SYMBOL: &str = "__cobrust_str_replace";
    pub const STR_TRIM_RUNTIME_SYMBOL: &str = "__cobrust_str_trim";
    pub const STR_FIND_RUNTIME_SYMBOL: &str = "__cobrust_str_find";
    pub const STR_CONTAINS_RUNTIME_SYMBOL: &str = "__cobrust_str_contains";
    pub const STR_STARTS_WITH_RUNTIME_SYMBOL: &str = "__cobrust_str_starts_with";
    pub const STR_ENDS_WITH_RUNTIME_SYMBOL: &str = "__cobrust_str_ends_with";
    pub const STR_LOWER_RUNTIME_SYMBOL: &str = "__cobrust_str_lower";
    pub const STR_UPPER_RUNTIME_SYMBOL: &str = "__cobrust_str_upper";
    pub const STR_CLONE_RUNTIME_SYMBOL: &str = "__cobrust_str_clone";
    ```
  - Add eleven new arms to the `rewrite_print` dispatch around line ~1495
    (the existing `Kind::MathSqrt => MATH_SQRT_RUNTIME_SYMBOL` etc.).

- `crates/cobrust-codegen/src/cranelift_backend.rs`:
  - Add eleven new entries to `runtime_helper_signatures()` near
    line ~2425 (where `__cobrust_math_pow` and friends already live):
    ```rust
    out.push(("__cobrust_str_split", sig(call_conv, &[p, p], Some(p))));
    out.push(("__cobrust_str_join", sig(call_conv, &[p, p], Some(p))));
    out.push(("__cobrust_str_replace", sig(call_conv, &[p, p, p], Some(p))));
    out.push(("__cobrust_str_trim", sig(call_conv, &[p], Some(p))));
    out.push(("__cobrust_str_find", sig(call_conv, &[p, p], Some(i64))));
    out.push(("__cobrust_str_contains", sig(call_conv, &[p, p], Some(i64))));
    out.push(("__cobrust_str_starts_with", sig(call_conv, &[p, p], Some(i64))));
    out.push(("__cobrust_str_ends_with", sig(call_conv, &[p, p], Some(i64))));
    out.push(("__cobrust_str_lower", sig(call_conv, &[p], Some(p))));
    out.push(("__cobrust_str_upper", sig(call_conv, &[p], Some(p))));
    // __cobrust_str_clone signature: already exists if grep shows it
    // (it's at fmt.rs:306 — but the SIGNATURE may or may not yet be in
    // runtime_helper_signatures; verify with grep + add if absent):
    out.push(("__cobrust_str_clone", sig(call_conv, &[p], Some(p))));
    ```
  Note: `find`/`contains`/`starts_with`/`ends_with` return `i64`
  even though the surface signature is `bool` for the predicates.
  Reason: SwitchInt codegen consumes i64 for bool-shaped branches
  (see `__cobrust_str_eq` at `io.rs:504` — same convention). The
  type checker accepts the surface as `bool`; codegen materializes
  i64 0/1 from the runtime call.

- `crates/cobrust-mir/src/lower.rs`: **no changes.** PRELUDE stubs
  type-check; intrinsic-rewrite operates post-MIR on the
  `Terminator::Call` operand. No MIR opcode changes needed.

**TEST corpus inputs for sub-sprint 2**:
- ≥10 MIR-dump tests verifying each fn lowers to a `Terminator::Call`
  with the right runtime symbol.
- ≥5 codegen tests verifying the C-ABI signature matches (linker-time
  check via `cargo test --test stdlib_e2e`).

**Dependencies**: sub-sprint 1 (PRELUDE stubs must parse correctly first).

**Estimated PAIR shape**: TEST sonnet + DEV sonnet, parallel,
~2-3 hour total wall time.

### Sub-sprint 3 — Stdlib C-ABI shims (3-4 hours)

**Goal**: eleven new C-ABI shims in `crates/cobrust-stdlib/src/string.rs`
that wrap the existing Rust-side helpers (or implement new helpers
for the four `not-yet` cases: `join` / `contains` / `starts_with` /
`ends_with`).

**Files touched**:
- `crates/cobrust-stdlib/src/string.rs`:
  - Rename `pub fn strip` → `pub fn trim` (Decision 4); update all
    internal callsites + the `#[cfg(test)] mod tests` block's
    `strip_whitespace` / `strip_no_whitespace` / `strip_only_whitespace`
    tests to use the new name.
  - Add new Rust-side `pub fn`s for the four `not-yet` cases:
    ```rust
    pub fn join(parts: &[&str], sep: &str) -> String { /* iter + push */ }
    pub fn contains(s: &str, needle: &str) -> bool { s.contains(needle) }
    pub fn starts_with(s: &str, prefix: &str) -> bool { s.starts_with(prefix) }
    pub fn ends_with(s: &str, suffix: &str) -> bool { s.ends_with(suffix) }
    ```
  - Add eleven new `pub unsafe extern "C" fn` C-ABI shims, all
    receiving Str-pointer (`*mut u8`) arguments and returning either
    `*mut u8` (for Str/List[Str] returns) or `i64` (for i64/bool):

```rust
/// C-ABI shim for source-level `split(s: str, sep: str) -> list[str]`.
///
/// # Safety
///
/// `s` and `sep` must be Str pointers per `__cobrust_str_new` / `_push_static`
/// (or `__cobrust_str_clone`). Returns a fresh `list[str]` whose elements
/// must be dropped via `__cobrust_list_drop_elems(list, __cobrust_str_drop)`
/// at scope exit (the codegen drop schedule per ADR-0050c Phase 2 does
/// this automatically).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_split(s: *mut u8, sep: *mut u8) -> *mut u8 {
    // 1. Read s/sep as &str via str_buf_as_str_phase3.
    // 2. Call split(s_str, sep_str) -> Vec<String>.
    // 3. Materialize a list[str] via __cobrust_list_new(n) + per-element
    //    __cobrust_str_new + __cobrust_str_push_static (mirror env.rs:64-85
    //    argv materialization).
    // 4. ADR-0050c: drop s + sep here per Move semantics on Str args.
    //    Actually NO — the codegen will not have dropped them yet because
    //    they're consumed by this call. The C-ABI shim is responsible for
    //    dropping its Str args because the caller's `Terminator::Call`
    //    materialized them as Move operands and the codegen does not
    //    insert a Drop after the Call. Audit this with the M-F.3.2 + M-F.3.5
    //    DEV: which side owns the drop?
    // — RESOLVED at design time: see §"Open question Q-shim-drop-owner" below.
    todo!("sub-sprint 3 DEV implements")
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_join(parts: *mut u8, sep: *mut u8) -> *mut u8 {
    // Iterate the list[str] via __cobrust_list_len + __cobrust_list_get
    // (each get returns an i64 reinterpretation of a *mut u8 to a
    // StringBuffer). Concatenate with sep between.
    todo!()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_replace(
    s: *mut u8, old: *mut u8, new_: *mut u8
) -> *mut u8 {
    // Delegate to Rust-side replace(s, old, new).
    todo!()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_trim(s: *mut u8) -> *mut u8 { todo!() }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_find(s: *mut u8, needle: *mut u8) -> i64 {
    // Delegate to Rust-side find(s, needle). Return -1 if None.
    todo!()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_contains(s: *mut u8, needle: *mut u8) -> i64 {
    // Return 1 if contains, 0 otherwise. i64 not bool for SwitchInt.
    todo!()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_starts_with(s: *mut u8, prefix: *mut u8) -> i64 {
    todo!()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_ends_with(s: *mut u8, suffix: *mut u8) -> i64 {
    todo!()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_lower(s: *mut u8) -> *mut u8 { todo!() }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_upper(s: *mut u8) -> *mut u8 { todo!() }

// __cobrust_str_clone already ships at fmt.rs:306. No new shim;
// only the intrinsic-rewrite/PRELUDE plumbing is new in M-F.3.5.
```

**Open shim-drop-owner question** (resolved here, binding for DEV):
the C-ABI shim is responsible for **NOT** dropping its Str args.
Reason: the caller's MIR has the Str-typed args as `Operand::Move(s)`
operands; under ADR-0050c Phase 1 + 2, the drop pass enumerates the
Str-typed locals as drop-eligible at their scope exit. The codegen's
`Terminator::Drop` arm (currently TODO per `findings/lc100-...`)
emits `__cobrust_str_drop` at scope exit. The shim simply READS its
args and returns; the drop happens later, at the caller's scope.

This means the shim implementation is a thin wrapper:

```rust
pub unsafe extern "C" fn __cobrust_str_trim(s: *mut u8) -> *mut u8 {
    if s.is_null() { return alloc_str_buffer(""); }
    let s_str = unsafe { str_buf_as_str_phase3(s) };
    alloc_str_buffer(s_str.trim())
    // s NOT dropped here — caller's MIR drop pass owns the lifetime.
}
```

The mirror of W2 `__cobrust_str_at` at `io.rs:480-491` confirms this
pattern: the shim allocates the return Str via `alloc_str_buffer`
and lets the caller manage the input lifetime.

**TEST corpus inputs for sub-sprint 3**:
- ≥40 well-typed end-to-end tests (4 per surface fn × 11 = 44) covering:
  - Basic happy path (`split("a,b,c", ",")` → `["a", "b", "c"]`)
  - Empty input (`split("", ",")` → `[""]`)
  - Empty needle/sep
  - Whitespace edge cases (`trim`)
  - Edge of byte-vs-char (`find` with UTF-8 multibyte input)
  - Round-trip (`join(split(s, ","), ",") == s` for non-pathological s)
  - Sentinel correctness (`find` returns -1 on absence)
- ≥10 negative tests covering use-after-move (`let v = split(s, ","); let n = str_len(s)` → UseAfterMove; this LOCKS the LC-100 honest-debt baseline).
- ≥10 valgrind-clean exit tests verifying no Str leaks on representative programs.

**Dependencies**: sub-sprints 1 + 2 (PRELUDE + intrinsic-rewrite must
land before the C-ABI shims are callable end-to-end).

**Estimated PAIR shape**: TEST opus + DEV opus, parallel, ~3-4 hour
total wall time. Bumped to opus tier because the empty-edge-case +
valgrind-clean discipline + cross-shim semantics consistency
(`split` + `join` round-trip, `contains` ↔ `find != -1` symmetry) is
multi-surface coordination work that benefits from opus reasoning.

### Sub-sprint 4 — Docs + corpus (3-4 hours)

**Goal**: zh + en + agent docs ship for every surface fn; the running
M-F.3.5 corpus is committed; getting-started doc gains a string-processing
section.

**Files touched**:
- `docs/human/zh/architecture.md` — add eleven rows to the std.string
  table (line ~1670 area; mirror M-F.3.3's math fn precedent).
- `docs/human/en/architecture.md` — mirror.
- `docs/human/zh/getting-started-leetcode.md` — add §"字符串处理"
  showing 3-5 idiomatic usage patterns including the `clone()` mitigation
  for multi-use Str.
- `docs/human/en/getting-started-leetcode.md` — mirror.
- `docs/agent/modules/stdlib.md` — add §"M-F.3.5 string stdlib surface"
  with the eleven-fn signature + ownership + return-type table; cite
  this ADR. Anchor: `[ADR-0050e]`.
- `docs/agent/modules/cli.md` — note the PRELUDE extension (eleven new
  stubs) + intrinsic-rewrite extension (eleven new arms).
- `scripts/doc-coverage.sh` — add to the M11 stdlib check list:
  `split`, `join`, `replace`, `trim`, `find`, `contains`,
  `starts_with`, `ends_with`, `lower`, `upper`, `clone`,
  `__cobrust_str_split`, `__cobrust_str_join`, `__cobrust_str_replace`,
  `__cobrust_str_trim`, `__cobrust_str_find`, `__cobrust_str_contains`,
  `__cobrust_str_starts_with`, `__cobrust_str_ends_with`,
  `__cobrust_str_lower`, `__cobrust_str_upper`, `__cobrust_str_clone`,
  `ADR-0050e`.
- `docs/agent/adr/README.md` — append ADR-0050e row to the roster.
- `examples/leetcode/string_processing.cb` (new) — small demo program
  using ≥5 of the eleven surface fns; runs against `corpus/leetcode/`
  oracle inputs.
- (optional) `examples/log_filter.cb` (new) — split + filter + join
  demo showing the daily-program shape M-F.3.5 unlocks.

**TEST corpus inputs for sub-sprint 4**:
- Doc-coverage script must pass (every M-F.3.5 public surface item
  has zh + en + agent entries).
- Examples must run clean (`cobrust run examples/leetcode/string_processing.cb`
  exits 0 with expected stdout).
- 5-gate baseline (fmt + clippy + build + workspace test + doc-coverage)
  green.

**Dependencies**: sub-sprints 1-3 (every fn must work end-to-end before
docs can claim it does).

**Estimated PAIR shape**: TEST sonnet + DEV sonnet, parallel,
~3-4 hour total wall time. Doc-heavy, low risk.

### Sub-sprint estimated wall-time roll-up

| Sub-sprint | TEST agent | DEV agent | Total |
|---|---|---|---|
| 1 — Parser/types/PRELUDE | sonnet, ~3-4h | sonnet, ~2h | **~3-4h** wall (parallel) |
| 2 — MIR + intrinsic-rewrite | sonnet, ~1.5h | sonnet, ~2h | **~2-3h** wall |
| 3 — Stdlib C-ABI shims | opus, ~3-4h | opus, ~3-4h | **~3-4h** wall |
| 4 — Docs + corpus | sonnet, ~3h | sonnet, ~3h | **~3-4h** wall |
| **Total M-F.3.5 PAIR** | | | **~11-15h** wall, ~7-10h CPU |

This compares favorably to ADR-0050 §"P1 follow-ups" original estimate
"~3-5 days, D2-D3" for M-F.3.5. The verified-at-HEAD existing-Rust-side
helpers (5 of 11 fns) compress the impl significantly.

## F30 §"Consequences" — predicate-flip cascade enumeration

This ADR does NOT flip any shared predicate (per F30 SOP signal §3).
M-F.3.5 only ADDS surface fns; it does not change `is_copy_type` /
`is_copy` / drop-eligibility / borrow-check / type-universe predicates.
The F30 shadow-flip dry-run is therefore NOT applicable.

However, the F29 enumeration (per `findings/adr-cross-surface-bug-fix-scope-creep.md`)
IS required because eleven new C-ABI shims widen the
`__cobrust_str_*` consumer surface that ADR-0050c §"Consequences"
enumerated. Below is the F29-style enumeration of every shared
infrastructure consumer M-F.3.5 touches:

### Consumer enumeration (F29 SOP)

#### Every `__cobrust_str_*` shim that M-F.3.5's new shims call into

| Existing shim | M-F.3.5 new shims using it | Status |
|---|---|---|
| `__cobrust_str_new` (`fmt.rs:75`) | all eleven new shims (allocate return buffer via `alloc_str_buffer` helper, which wraps `_str_new` + `_push_static`) | **also-fixed transitively**; the existing Drop-schedule per ADR-0050c picks up the returned buffer at the caller's scope. |
| `__cobrust_str_push_static` (`fmt.rs:88`) | all Str-returning new shims (split's per-element + join's accumulator + replace/trim/lower/upper/clone returns) | **also-fixed transitively**. |
| `__cobrust_str_drop` (`fmt.rs:284`) | NONE called directly by M-F.3.5 shims; the codegen drop schedule (per ADR-0050c Phase 2, status: open per `findings/lc100-...`) is the sole emitter | **fixed-later-with-anchor** — the Phase 2 codegen drop arm is still TODO per the LC-100 honest-debt; M-F.3.5 doesn't widen this gap but doesn't close it either. |
| `__cobrust_str_clone` (`fmt.rs:306`) | NEW source-level surface in M-F.3.5 (via PRELUDE+intrinsic-rewrite) | **also-fixed** — the shim ships end-to-end; M-F.3.5 surfaces it to user-level source. |
| `__cobrust_str_len` (`fmt.rs:247`) | NONE — M-F.3.5 shims work in `&str` slice land via `str_buf_as_str_phase3`, not via `_str_len` | **no impact**. |
| `__cobrust_str_ptr` (`fmt.rs:264`) | NONE directly; `str_buf_as_str_phase3` reads the bytes field directly | **no impact**. |
| `__cobrust_str_len_src` (`io.rs:465`) | NONE | **no impact** — distinct path used by W2 `str_len(s)` PRELUDE. |
| `__cobrust_str_at` (`io.rs:482`) | NONE | **no impact** — distinct W2 path. |
| `__cobrust_str_eq` (`io.rs:504`) | NONE | **no impact**. |
| `__cobrust_str_eq_lit` (`io.rs:531`) | NONE | **no impact**. |
| `__cobrust_str_ord` (`io.rs:555`) | NONE | **no impact**. |

**Direct Str shim consumer count**: 4 existing shims `also-fixed` /
`also-fixed transitively` (str_new, str_push_static, str_clone +
the str_buf_as_str_phase3 helper); 1 existing shim `fixed-later-with-anchor`
(str_drop — LC-100 honest-debt remains the Phase G closure target);
6 existing shims `no impact`.

#### Every `__cobrust_list_*` shim that M-F.3.5's `split` / `join` shims call into

| Existing shim | M-F.3.5 new shims using it | Status |
|---|---|---|
| `__cobrust_list_new` (`collections.rs:534-540`) | `__cobrust_str_split` (materializes the return `list[str]`) | **also-fixed transitively** — the shim's i64-slot layout works for Str-pointer slots (the W2 + ADR-0050c argv precedent). |
| `__cobrust_list_set` (`collections.rs:557-570`) | `__cobrust_str_split` (populates each element with a Str pointer cast to i64) | **also-fixed transitively**. |
| `__cobrust_list_get` (`collections.rs:583-595`) | `__cobrust_str_join` (reads each element pointer back) | **also-fixed transitively**. |
| `__cobrust_list_len` (`collections.rs:459-470`) | `__cobrust_str_join` (iterates parts) | **also-fixed transitively**. |
| `__cobrust_list_is_empty` (`collections.rs:472+`, per ADR-0050c Phase 6) | NONE directly by M-F.3.5; consumers can call it on `split` returns | **no impact**. |
| `__cobrust_list_drop` (`collections.rs:520+`) | NONE — caller's drop schedule owns; M-F.3.5 shims don't drop their inputs | **no impact**. |
| `__cobrust_list_drop_elems` (ADR-0050c Phase 3) | NONE — caller's drop schedule owns | **no impact**. |

**Direct List shim consumer count**: 4 `also-fixed transitively`; 3 `no impact`.

#### Every Cobrust source-level path that's UNTOUCHED by M-F.3.5 but ergonomically affected

Per F30 §"Pattern signal" item 3: the codebase has multiple eras
(W2 / M-F.3.3 / M-F.3.5). Each era's PRELUDE-fn pattern composes with
M-F.3.5; we enumerate to verify no era-collision:

| Era surface | M-F.3.5 interaction | Status |
|---|---|---|
| W2 Phase 3 `str_at(s, i)` returns `*mut u8` (owned Str) | User can chain `contains(str_at(clone(s), 0), "a")` — the inner `str_at` return is a temp Str that drops at expression end | **also-fixed transitively** — Str-non-Copy makes the temp lifecycle clean. |
| W2 Phase 3 `str_len(s)` returns `i64` | `let n = str_len(clone(s)); let parts = split(s, ",")` works; the `clone(s)` mitigation is the user's responsibility | **also-fixed** — the LC-100 honest-debt mitigation applies symmetrically. |
| W2 Phase 3 `str_eq(a, b)` returns `i64` | M-F.3.5 `contains(s, n)` is a different surface; `str_eq` is positional equality, `contains` is substring | **no impact**. |
| W2 Phase 2 `input(prompt)` returns `*mut Str` | `let s = input(""); contains(clone(s), "ERROR")` is the canonical pattern | **also-fixed transitively**. |
| W2 Phase 2 `argv()` returns `list[str]` | `let args = argv(); contains(args[0], "v0.2")` (when list[str] indexing lands) | **fixed-later-with-anchor** — list[str] indexing semantics in M-F.3.2 must be honest; argv → contains chain works if M-F.3.2 ships index-by-i64 cleanly. |
| f-string `{s}` hole | `f"got {split(clone(s), ',')[0]}"` — f-string composition with M-F.3.5 returns works if list[str] indexing yields a `str` that the f-string hole accepts | **fixed-later-with-anchor** — same M-F.3.2 dependency. |
| ADR-0050c codegen `Terminator::Drop` (currently TODO per LC-100 honest-debt) | M-F.3.5 returns drop at scope exit IF the codegen drop arm ships; today they leak (same LC-100 baseline) | **fixed-later-with-anchor** — LC-100 Phase G closure target. M-F.3.5 inherits the existing leak; does not widen. |
| M-F.3.3 math fns `sqrt(x)` etc. | Orthogonal — math fns take f64, M-F.3.5 takes str | **no impact**. |
| M-F.3.4 dict (post-Wave-3) | `dict[str, str]` keys/values follow Str=non-Copy; M-F.3.5 fns on dict values work after a clone | **fixed-later-with-anchor** — Wave 3 dict impl inherits the ownership shape; M-F.3.5 doesn't widen. |
| Comprehension lowering | `[upper(clone(x)) for x in xs]` — the comprehension's iter-protocol + M-F.3.5 surface compose if the open finding `comp-lowering-zero-sentinel-collision.md` closes | **fixed-later-with-anchor** — open finding. |

#### Consumer enumeration totals

| Bucket | Count |
|---|---|
| `also-fixed` (direct or transitive) | **8** |
| `fixed-later-with-anchor` | **5** |
| `no impact` | **9** |
| **Total enumerated consumers** | **22** |

Per F29 SOP, this enumeration is the audit gate: a post-merge audit
teammate verifies the enumeration by grepping `__cobrust_str_` +
`__cobrust_list_` + cross-referencing every M-F.3.5 surface fn against
the table. The shadow-flip dry-run is NOT required (no predicate
flips), but the audit MUST verify (a) the eleven new C-ABI symbols
appear in `runtime_helper_signatures`, (b) the eleven new
intrinsic-rewrite arms appear in `kind_for_name` + `rewrite_print`,
(c) the eleven new PRELUDE stubs appear in `build.rs`, (d) the
eleven new C-ABI shims appear in `stdlib/src/string.rs`.

## Open questions

### Q1 — `clone()` scope: ADD-TO-M-F.3.5 (RESOLVED)

Per §Decision 2 above. Already incorporated into all sub-sprint plans.

### Q2 — `find` return type: i64 with -1 sentinel (RESOLVED)

Per §Decision 5 above. The `if find(...) != -1:` doc-required idiom is
binding for sub-sprint 4 docs.

### Q3 — Unicode handling: byte-level (RESOLVED)

Per §Decision 6 above. Phase G adds grapheme variants.

### Q4 — Case-insensitive variants: defer to Phase G (RESOLVED)

Per §Decision 7 above.

### Q5 — `trim` chars argument (Phase G)

Python's `str.strip(chars)` takes an optional argument. Phase F.3
ships only no-arg `trim(s)` (whitespace). Phase G adds
`trim_chars(s, chars)` if user pull surfaces; for now the workaround
is `replace(s, "x", "")` for each unwanted char.

### Q6 — `replace` count limit (Phase G)

Python's `str.replace(old, new, count)` takes an optional `count`.
Phase F.3 ships only the all-occurrences form. Phase G adds
`replace_n(s, old, new, n)` if user pull surfaces.

### Q7 — `split` maxsplit argument (Phase G)

Python's `str.split(sep, maxsplit)` takes an optional limit. Phase
F.3 ships only the all-positions form. Phase G adds
`split_n(s, sep, n)` if user pull surfaces.

### Q8 — `rsplit` / `rfind` / `rstrip` / `lstrip` (Phase G)

Direction-asymmetric variants. Defer; the symmetric forms cover ~95%
of user patterns. The forward-only set is the Phase F.3 scope.

### Q9 — `format()` integration (Phase G)

The existing Rust-side `string::format(template, args)` at
`string.rs:100-141` is currently uncalled from `.cb` source — there's
no PRELUDE binding. Phase F.3 leaves this alone because Cobrust
f-strings (`f"hi {x}"`) cover the use case at compile time. Phase G
may add a runtime `format(template, args: list[FormatArg])` PRELUDE
fn if dynamic-format patterns surface.

### Q10 — `clone()` source-name collision with type-name `Clone` trait (Phase G + Phase 7.5)

When Phase 7.5 lands recursive types + Phase G lands traits/method-call,
`clone(s)` (free fn) and `s.clone()` (Clone trait method) may collide.
Phase G resolution: free-fn `clone` becomes an alias for the trait
method; both forms coexist. Today (M-F.3.5) there is no `Clone` trait,
so no collision. The PRELUDE stub `fn clone(s: str) -> str` is the
canonical surface for now.

## Cross-references + dependencies

### Depends on (must close before M-F.3.5 PAIR dispatches)

- **ADR-0050c** (Str ownership) — accepted at `aca5d87`. Sets the
  Str=non-Copy semantics M-F.3.5 inherits.
- **ADR-0050b** (For-loop shape) — accepted. Enables `for x in
  split(clone(s), ",")` over the `split` return value (the for-loop
  length-bound path at `mir/lower.rs:726-836` consumes `list[str]`).
- **ADR-0044 W2 Phase 3** (PRELUDE+intrinsic-rewrite pattern) —
  accepted at `9caef99`. The shape M-F.3.5 replicates.
- **M-F.3.2 list[str]** — landed via ADR-0050c Phase 2 + 3 + the
  list[str] DEV recovery merge at `aca5d87`. Enables `split` and
  `join`.
- **ADR-0050 §"P1 follow-ups"** — accepted at `f566026`. Names
  M-F.3.5 in scope.

### Blocks (cannot ship without M-F.3.5 first)

- **M-F.3.7 JSON parser** (ADR-0050 §"P1 wave") — JSON tokenizer
  uses `split` for delimiter splitting + `contains` / `starts_with`
  for keyword detection. ADR-0050 estimates ~5-7 days; assumes
  M-F.3.5 is shipped.
- **`docs/human/{zh,en}/getting-started-leetcode.md`** §"字符串处理"
  / "string processing" example sections — require M-F.3.5 to ship.
- **v0.2.0 stable tag readiness** (ADR-0050 §"v0.2.0 stable tag
  binding" §3) — M-F.3.5 is named as a gate.

### Relates to

- **ADR-0050d** (dict design) — once dict[str, str] lands in Wave 3,
  M-F.3.5 surface composes for `for k in dict_keys(d): replace(k, " ", "_")`.
- **ADR-0049** (alpha honesty lanes) — M-F.3.5 surface shipping
  in v0.2.0 stable continues the alpha-vs-stable label discipline:
  M-F.3.5 is stable (no `(alpha)` marker), distinct from the M-AI.0..M-AI.2
  surfaces.
- **ADR-0029** (M14 REPL) — Phase F.3 §M-F.3.8 REPL will surface
  M-F.3.5 as the primary string-processing surface for interactive
  use.
- **`findings/lc100-str-use-after-move-regression-from-adr0050c.md`** —
  M-F.3.5 inherits the honest-debt baseline; the `clone()` PRELUDE
  fn surfaces the explicit mitigation.
- **`findings/predicate-flip-cascade-discovery-deficit.md`** (F30
  candidate) — M-F.3.5 is NOT a predicate-flip ADR; shadow-flip
  dry-run does NOT apply.
- **`findings/comp-lowering-zero-sentinel-collision.md`** — M-F.3.5
  surface fns do not trigger the 0-sentinel collision (Decision 9
  analysis); a regression test locks `__cobrust_str_new` non-null
  contract.

### Phase G followups (out of M-F.3.5 scope)

| Surface | Defer rationale |
|---|---|
| Method-call sugar `s.split(",")` | Needs method-dispatch infra (Option C deferral) |
| `trim(s, chars)` / `trim_left` / `trim_right` | No user pull yet |
| `replace(s, old, new, count)` | No user pull yet |
| `split(s, sep, maxsplit)` | No user pull yet |
| `rsplit` / `rfind` / `rstrip` / `lstrip` | Symmetric forms cover ~95% |
| Grapheme-indexed variants | Phase G char-iterator dep |
| Case-insensitive variants | Phase G composability via method-chain |
| `format(template, args)` runtime fn | f-strings cover compile-time case |
| `Clone` trait + `s.clone()` method | Phase G + Phase 7.5 traits |
| `&str` borrow form | Phase G ownership consolidation |

## Why this ADR now

The user prioritization 2026-05-16 P1 list names string stdlib as the
next user-traction surface after Phase F.3 P0 closes. v0.2.0 stable
tag binding (ADR-0050 §"v0.2.0 stable tag binding" §3) makes M-F.3.5
a gate. The string stdlib is the wedge surface that turns Cobrust
into a usable language for daily programs — without it, users who
finish Phase F.3 P0 still cannot write log parsing, CSV slicing, or
even a `grep`-equivalent.

The design-only sprint runs PARALLEL to Wave 3 tranche 1 (dict parser
TEST + file IO design) so impl-side dispatch is unblocked the moment
Wave 3 capacity opens. Per ADR-0050 §"Dispatch routing", M-F.3.5
P10-direct PAIR is a single Mac-local-then-DG-verify sprint (no
DG-primary needed — the surface is small).

The `clone()` add-to-scope decision (§Decision 2) retroactively
unblocks LC-100 corpus mitigation: every honest-debt program from
`findings/lc100-...` can `clone(s)` between PRELUDE calls to bypass
the Str=non-Copy hazard. This is the single highest-leverage decision
in this ADR.

— P9-G design-only sprint, 2026-05-16

## Amendment 2026-05-16 — clone() mitigation idiom is INLINE-CLONE-AT-CALLSITE (F-W3-5)

Per ADSD F2 addendum-not-rewrite. Post-Wave-3 audit (a19ec12e17f7212b3)
F-W3-5 surfaced that this ADR's Lane 4 framing was slightly too
optimistic about what `clone()` mitigates.

**The mitigation idiom that WORKS** (per ADR-0050c Option A + ADR-0050d
symmetric walk-back):

```cobrust
let s: str = input("")
let n: i64 = str_len(clone(s))      # inline clone — fresh buffer for str_len
let i: i64 = 0
while i < n:
    let c: str = str_at(clone(s), i)  # inline clone — fresh buffer for str_at
    let _ = print(c)
    i = i + 1
let final: str = upper(s)            # bare `s` — final consumer, no clone
```

Each PRELUDE call gets its own freshly-cloned Str buffer; the bare
`s` is the LAST consumer.

**The pattern that DOES NOT WORK** (caught at Wave 3 string-stdlib TEST
corpus authoring, per `f3str16` / `f3str17` / `f3str22`):

```cobrust
let s: str = input("")
let s2: str = clone(s)               # `s` is MOVED into the clone() call.
let n: i64 = str_len(s)              # UseAfterMove — `s` already moved.
```

The naive consume-then-reuse pattern is itself UseAfterMove because
`clone(s)` consumes `s` under ADR-0050c Option A. The TEST corpus
author wrote this naive pattern thinking it would work; it does not.
The 3 failing f3str{16,17,22} tests are documented at
`findings/lc100-str-use-after-move-regression-from-adr0050c.md` Path
D long-term-deferral disposition.

**LC-100 corpus mitigation pathway** (when revisited post-Phase G):
re-author each LC-100 program to use the inline-clone-at-callsite
idiom. ~100 programs × small mechanical edit. Could batch as a
post-Phase G corpus-cleanup sprint when LC-100 closure becomes
worth the effort (currently long-term deferral per user 2026-05-16
disposition).

— Post-Wave-3 audit F-W3-5 closure, 2026-05-16 night
