---
doc_kind: adr
adr_id: 0084
title: re — regular-expression stdlib module (import re) via the regex crate + cobrust-stdlib shims
status: accepted
date: 2026-06-05
last_verified_commit: 351fa57
supersedes: []
superseded_by: []
---

# ADR-0084: `re` — the regular-expression stdlib module (`import re`)

## Context

String/regex processing is one of the most-used Python capabilities, and
the L0–L3 translation pipeline needs it. ADR-0083 (`math`) is the
precedent for a core scalar stdlib module wired through the
ecosystem-call path; this ADR adds `import re` the SAME way.

`re` is **distinct** from every existing module:

- It takes only **strings** and returns a `str` / `list[str]` / `bool` —
  no Buffer (unlike `coil`), no handle/state (unlike `redis`/`pit`/`den`).
- It is therefore assembled ENTIRELY from already-shipped ABIs (Str args,
  Str return, list[str] return, bool return) — no new MIR arm, no new
  codegen fn-type, no new runtime-link mechanism.

### Scope (the clean stateless subset — 4 functions)

This first cut ships the functions that need NO Match-object state (the
`.group()` form is a documented follow-up). Confirmed against
`/opt/homebrew/bin/python3.11`:

| `.cb` call | shape | semantics | oracle |
|---|---|---|---|
| `re.sub(pattern, repl, s)` | `[Str, Str, Str] -> Str` | replace ALL non-overlapping matches | `re.sub('a','X','banana') == 'bXnXnX'` |
| `re.findall(pattern, s)` | `[Str, Str] -> list[str]` | ALL non-overlapping FULL matches; `[]` on no match | `re.findall('[0-9]+','a1b22c333') == ['1','22','333']` |
| `re.match(pattern, s)` | `[Str, Str] -> Bool` | does `pattern` match at the START of `s` | `bool(re.match('bc','abc')) == False` |
| `re.search(pattern, s)` | `[Str, Str] -> Bool` | does `pattern` match ANYWHERE in `s` | `bool(re.search('bc','abc')) == True` |

CPython's `re.match` / `re.search` return a **Match object** (or `None`);
this first cut returns **`bool`**. The Match-object `.group()` /
`.start()` surface is a documented follow-up (see §Deferred).

## Decision

### Backing engine — the `regex` crate (1.x)

The shims (`cobrust-stdlib/src/re.rs`) wrap the `regex` crate.
`regex = "1"` is added to `crates/cobrust-stdlib/Cargo.toml` — it was
ALREADY in the workspace lock via `cobrust-types` / `cobrust-dap`, so
this is only a new dependency **edge** (no new crate download). Per the
**F64** lesson, the regenerated `Cargo.lock` (a single `+ "regex",`
line under `cobrust-stdlib`'s deps) is staged WITH `Cargo.toml` (CI
`--locked` rejects an unstaged lockfile).

`re.rs` is compiled into `cobrust-stdlib`'s staticlib, which is ALWAYS
linked into a `.cb` binary — so the `__cobrust_re_*` symbols resolve the
SAME way `__cobrust_str_split` / `__cobrust_llm_stream` (also in the
stdlib) do. No per-import `.a` (unlike `redis`'s `libredis.a`).

### `@py_compat` tier — Semantic (a documented divergence)

Tier `Semantic`. The Rust `regex` flavor matches Python `re` for the
common patterns (character classes, quantifiers, alternation, anchors,
groups) but has **NO backreferences** and **NO lookaround** — that is the
linear-time guarantee the `regex` crate trades for. A pattern using
`\1` / `(?=...)` / `(?<=...)` will fail to compile (→ a clean trap, see
below), where CPython would accept it. This is the declared Semantic-tier
divergence, recorded here and in the user docs.

### `re.sub` — Str args + Str return

Reuses the proven ABIs:

- **Str args** read via the f-string-buffer ABI (`__cobrust_str_ptr` /
  `__cobrust_str_len`) through `str_buf_as_str_local`, mirroring
  `string::str_buf_as_str_local` (which `coil.astype` also uses to read a
  `dtype` Str arg).
- **Str return** allocated via `__cobrust_str_new` +
  `__cobrust_str_push_static` through `alloc_str_buffer_local`, mirroring
  `string::__cobrust_str_replace` (the str-shim Str-return precedent).

`runtime_symbol = "__cobrust_re_sub"`, `(ptr, ptr, ptr) -> ptr`.

### `re.findall` — list[str] return (the `Ty::List(Str)` mint)

`EcoSig.ret = Ty::List(Box::new(Ty::Str))`. The shim mints a heap
`List<i64>` whose i64 slots store one owned `Str` pointer per FULL match,
via `__cobrust_list_new` + `__cobrust_list_set` — mirroring
`string::__cobrust_str_split` and `llm::__cobrust_llm_stream` (the
`-> list[str]` precedents) and the redis Phase-1d
`__cobrust_redis_client_smembers`. Codegen derives the extern (a
`Ty::List` return maps to an LLVM ptr return — NO new fn-type) and the
`.cb` for-loop / index / `Ty::List(Str)` drop schedule consume + free it
with NO new code.

**findall group semantics (the documented deferral)**: this returns the
**FULL** matches (`m.as_str()`). For a **no-group** pattern this equals
CPython exactly. CPython's group-capture behavior (1 group → the group's
text; >1 groups → tuples) is deferred — a **grouped** pattern returns the
FULL match here, which is the Semantic-tier divergence. (A faithful
1-group form would special-case `re.captures` on a single capture group;
the `list[str]` ABI cannot represent the tuple-of-groups case at all, so
that part stays deferred regardless.)

`runtime_symbol = "__cobrust_re_findall"`, `(ptr, ptr) -> ptr`.

### `re.match` vs `re.search` — bool return, the anchor is load-bearing

Both `EcoSig.ret = Ty::Bool` (the Rust C-ABI `-> bool` → LLVM `i1`,
mirroring `math.isnan`'s `__cobrust_math_isnan`). The i1 lands in the
`.cb` `_ecoret` Bool local, usable directly in `if re.search(...):`.

The **anchor** is the load-bearing distinction:

- `re.search` → `regex::Regex::is_match` (matches ANYWHERE).
- `re.match` → `regex::Regex::find(s).is_some_and(|m| m.start() == 0)`
  (START-anchored). The `find().start() == 0` form is chosen over
  prepending `\A` / `^` so the caller's `pattern` is NEVER mutated — a
  pattern that already begins with an anchor or a group would be
  corrupted by string-prepending.

So `re.match("bc", "abc") == False` but `re.search("bc", "abc") == True` —
the distinguishing test, locked in both the lib tests and the `.cb` e2e.

`runtime_symbol = "__cobrust_re_match"` / `"__cobrust_re_search"`,
`(ptr, ptr) -> bool`.

### Invalid-pattern policy — a clean runtime trap

A malformed **runtime** pattern (e.g. `"["`) makes
`regex::Regex::new` return `Err`. The shim's `compile_or_trap` turns that
into a clean process trap via `__cobrust_panic` (non-zero exit) —
**NEVER** a silent no-match and **NEVER** a Rust unwind across the C-ABI.
This mirrors `cobrust-coil`'s `coil_panic` discipline (a domain error is
a clean abort, not a wrong value). CPython raises `re.error`; Cobrust
traps. The trap message names the offending pattern
(`re: invalid pattern "[": regex parse error: ...`).

## Wiring (the 4 layers)

1. **Shims** — NEW `crates/cobrust-stdlib/src/re.rs` (`mod re;` added to
   `lib.rs`): `__cobrust_re_sub` / `_findall` / `_match` / `_search`,
   backed by the `regex` crate.
2. **Manifest** — `cobrust-types/src/ecosystem.rs`: four `("re", fn)`
   rows in `lookup_module_fn`; `is_ecosystem_module("re") = true`. All
   `PyCompatTier::Semantic`.
3. **MIR** — **no edit**. The generic `try_lower_ecosystem_call` Case-1
   (free-function) path already lowers Str args (via `lower_eco_arg`) +
   any `EcoSig.ret` (str/list/bool) through `emit_ecosystem_call`. Proven
   by `coil.astype` (Str arg), `redis.smembers` (list[str] ret),
   `math.isnan` (bool ret).
4. **Codegen** — `cobrust-codegen/src/llvm_backend.rs`: declare the four
   `__cobrust_re_*` externs (`re_sub` `(ptr,ptr,ptr)->ptr`, `re_findall`
   `(ptr,ptr)->ptr`, `re_match`/`re_search` `(ptr,ptr)->i1`). Codegen
   only declares the externs — the MIR retarget turns `re.sub(...)` into
   a `Terminator::Call`.

The type-checker side is also generic: `try_synth_ecosystem_call`
consults the SAME `lookup_module_fn` + `is_ecosystem_module`, so the four
rows type-check with no `check.rs` edit.

## Consequences

- `import re` + the four functions work end-to-end from `.cb`, confirmed
  by compile→link→spawn e2e tests (`crates/cobrust-cli/tests/re_e2e.rs`)
  whose output matches python3.11.
- The `list[str]` return is iterated in a `.cb` `for` loop in the e2e —
  proving it is a first-class, drop-scheduled, usable list.
- An invalid runtime pattern traps cleanly (non-zero exit), verified in
  the e2e.
- No `unsafe` unwind crosses the C-ABI; the str/list returns drop once
  (the `.cb` scope's existing `Ty::Str` / `Ty::List(Str)` drop schedule).

## Deferred (follow-ups)

- **Match-object `.group()` / `.start()` / `.span()`** — the stateful
  surface. `re.match` / `re.search` would return a Match handle (a
  den.Connection-shaped ADT) instead of `bool`; `re.findall` with capture
  groups would return tuples. This is the natural second cut.
- **`re.split` / `re.sub` with a count / `re.compile` (a reusable Pattern
  handle)** — more of the stateless/handle surface.
- **A compile-time check for a LITERAL pattern** (§2.5
  compile-time-catch). Today a malformed pattern is a RUNTIME trap because
  the pattern is a runtime `str`. When the pattern is a string LITERAL
  (the dominant case), the compiler COULD validate it at build time
  (`regex::Regex::new` at codegen) and emit a `TypeError`-class diagnostic
  — exactly as the `pit` Pattern refinement validates its regex at build
  time. That is a §2.5 win (move the error from run-time to compile-time)
  and a clean follow-up; this ADR ships the runtime-trap floor first.
- **Backreferences / lookaround** — out of scope of the `regex` crate
  (the Semantic-tier divergence); would require a different engine.

## Evidence

- Lib tests: `cargo test -p cobrust-stdlib --lib re::` — 10 pass.
- Manifest tests: `cargo test -p cobrust-types --lib` — 144 pass.
- E2E: `cargo test -p cobrust-cli --test re_e2e -- --test-threads=1` —
  7 pass (sub-all, sub-class, findall-iter, findall-empty,
  match-vs-search, match-true, invalid-pattern-trap).
- Oracle: `/opt/homebrew/bin/python3.11 -c "import re; ..."`.
