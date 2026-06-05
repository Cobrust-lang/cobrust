---
doc_kind: module
module_id: mod:re
crate: none
last_verified_commit: 351fa57
dependencies: [mod:types, mod:codegen, mod:stdlib]
---

# Module: re (regular-expression stdlib surface)

## Purpose

`import re` — the regular-expression Python stdlib module wired into
Cobrust (per ADR-0084). String/regex processing: `re.sub(p, r, s)`,
`re.findall(p, s)`, `re.match(p, s)`, `re.search(p, s)`.

NOT a crate. There is no `cobrust-re`; `re` is a compiler surface — a
manifest in `cobrust-types` + four `__cobrust_re_*` shims in
`cobrust-stdlib/src/re.rs` (backed by the `regex` crate) + the extern
decls in `cobrust-codegen`.

Assembled ENTIRELY from already-shipped ABIs (Str args, Str return,
list[str] return, bool return) — no new MIR arm, no new codegen fn-type.

## Status

- **ADR-0084 — delivered.** 4 stateless functions (sub / findall /
  match / search). 10 `cobrust-stdlib` shim unit tests + 144
  `cobrust-types` lib tests + 7 `.cb` e2e tests green.
- Match-object `.group()` form, `re.split` / `re.compile`, and a
  literal-pattern compile-check are documented follow-ups.

## Public surface — `lookup_module_fn("re", _)`

| `.cb` form | signature | runtime symbol | tier |
|---|---|---|---|
| `re.sub(pattern, repl, s)` | `[Str, Str, Str] -> Str` | `__cobrust_re_sub` | Semantic |
| `re.findall(pattern, s)` | `[Str, Str] -> List(Str)` | `__cobrust_re_findall` | Semantic |
| `re.match(pattern, s)` | `[Str, Str] -> Bool` | `__cobrust_re_match` | Semantic |
| `re.search(pattern, s)` | `[Str, Str] -> Bool` | `__cobrust_re_search` | Semantic |

`is_ecosystem_module("re") == true`.

## Semantics (oracle = /opt/homebrew/bin/python3.11)

- `re.sub` replaces **ALL** non-overlapping matches:
  `re.sub("a", "X", "banana") == "bXnXnX"` (three replacements). Returns
  a fresh `str`.
- `re.findall` returns **ALL** non-overlapping FULL matches as a
  `list[str]`: `re.findall("[0-9]+", "a1b22c333") == ["1", "22", "333"]`;
  `[]` on no match. Iterable in a `.cb` `for` loop.
- `re.match` is **START-anchored** (CPython `re.match`):
  `re.match("bc", "abc") == False`, `re.match("ab", "abc") == True`.
- `re.search` matches **ANYWHERE** (CPython `re.search`):
  `re.search("bc", "abc") == True`.
- **The anchor is load-bearing**: `re.match("bc", "abc")` is False but
  `re.search("bc", "abc")` is True. `re.match` is implemented as
  `find(s).start() == 0` (NOT by mutating the pattern with `\A`).

CPython's `re.match` / `re.search` return a Match object / `None`; this
first cut returns **`bool`**. The `.group()` form is deferred.

## @py_compat tier: Semantic

The Rust `regex` flavor matches Python `re` for common patterns (classes,
quantifiers, alternation, anchors, groups) but has **NO backreferences**
and **NO lookaround** (the linear-time guarantee). A `\1` / `(?=...)` /
`(?<=...)` pattern fails to compile → a clean trap (see below).

### findall group divergence

`re.findall` returns the **FULL** matches (`m.as_str()`). For a no-group
pattern this == CPython exactly. CPython's group-capture behavior (1 group
→ that group's text; >1 → tuples) is the documented deferral — a grouped
pattern returns the FULL match here.

## Invalid-pattern policy — clean runtime trap

A malformed **runtime** pattern (e.g. `"["`) makes `regex::Regex::new`
return `Err`; the shim's `compile_or_trap` turns that into a clean
process trap via `__cobrust_panic` (non-zero exit) — NEVER a silent
no-match, NEVER a Rust unwind across the C-ABI. CPython raises `re.error`.
A literal-pattern compile-time check is an ADR-0084 §Deferred follow-up.

## ABI (reuse map)

- Str args read via `__cobrust_str_ptr` / `__cobrust_str_len`
  (`str_buf_as_str_local`) — mirrors `string.rs`.
- Str return via `__cobrust_str_new` / `__cobrust_str_push_static`
  (`alloc_str_buffer_local`) — mirrors `string::__cobrust_str_replace`.
- list[str] return via `__cobrust_list_new` / `__cobrust_list_set` —
  mirrors `string::__cobrust_str_split` + `llm::__cobrust_llm_stream` +
  redis `smembers`.
- bool return via the Rust C-ABI `-> bool` (LLVM `i1`) — mirrors
  `math::__cobrust_math_isnan`.

## Lowering (no MIR / type-checker edit)

`re.sub(...)` is `Attr(Name(re-alias), "sub")` applied to args. The
generic `try_lower_ecosystem_call` Case-1 (free function) path lowers the
Str args via `lower_eco_arg` and the `EcoSig.ret` (Str / List(Str) /
Bool) via `emit_ecosystem_call` — the SAME path `math` / `redis` / `coil`
use. The type checker's `try_synth_ecosystem_call` consults the same
`lookup_module_fn` + `is_ecosystem_module`. Codegen only declares the
four `__cobrust_re_*` externs.

## Files

- `crates/cobrust-stdlib/src/re.rs` — the 4 shims + Rust helpers + 10 unit
  tests.
- `crates/cobrust-stdlib/src/lib.rs` — `mod re;`.
- `crates/cobrust-stdlib/Cargo.toml` — `regex = "1"`.
- `crates/cobrust-types/src/ecosystem.rs` — the 4 manifest rows +
  `is_ecosystem_module`.
- `crates/cobrust-codegen/src/llvm_backend.rs` — the 4 extern decls.
- `crates/cobrust-cli/tests/re_e2e.rs` — 7 compile→spawn e2e tests.

## Done means

- `import re` + the 4 functions compile, link, spawn, match python3.11.
- The `list[str]` return is iterated in a `.cb` for-loop (e2e).
- An invalid pattern traps with a non-zero exit (e2e).
- `cargo test -p cobrust-stdlib --lib re::` (10) +
  `cargo test -p cobrust-types --lib` (144) +
  `cargo test -p cobrust-cli --test re_e2e` (7) all green.
