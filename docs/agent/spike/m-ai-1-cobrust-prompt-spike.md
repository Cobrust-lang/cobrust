---
doc_kind: spike
spike_id: m-ai-1
title: "M-AI.1 cobrust.prompt stdlib refined spec (α Phase 3)"
status: refined
date: 2026-05-12
last_verified_commit: TBD
ratifies: adr:0048 §"M-AI.1 — cobrust.prompt" preliminary surface
relates_to: [adr:0048, adr:0036, adr:0044, adr:0027, m-ai-0]
authored_by: P9 opus tech-lead (α Phase 3 dispatch)
---

# Spike M-AI.1 — `cobrust.prompt` stdlib refined specification

## Purpose

ADR-0048 §"M-AI.1 — cobrust.prompt" specifies a preliminary surface
sketch built around `Template(system=..., user=..., examples=[...])` +
`Example(input=..., output=...)` + `complete_structured(prompt=...,
schema={...})`. That sketch uses **kwargs**, **decorator-flavored
class constructors**, and **dict-typed schemas** — none of which are
source-syntax valid in α Cobrust today (PRELUDE-fn signatures are
fixed-positional per ADR-0044 §1D; no struct surface lands until
ADR-0049 Phase 7.5 recursive-types; `dict[str, str]` codegen is gated
at M12.x level and not yet wired through the type-checker as a function
parameter at α).

This spike refines the surface to **flat-fn α primitives** that work
inside the existing M-AI.0 PRELUDE pattern, while preserving forward-
compatibility with the ADR-0048 prose surface (post-recursive-types,
post-dict-of-str). It locks the design decisions for P7-TEST and
P7-DEV to consume.

This spike does **not** introduce new HIR / MIR / type-checker
primitives. M-AI.1 is "PRELUDE flat-fn + intrinsic-rewrite + C-ABI
shim + new pure-Rust stdlib module" — architecturally identical to
M-AI.0 (spike `m-ai-0-cobrust-llm-spike.md`) and ADR-0044 W2 source-
binding pattern.

Compared to M-AI.0 the work is **strictly simpler**:

- No new workspace crate dependency (pure Rust string-formatting).
- No tokio runtime (synchronous, no async ⇄ blocking bridge).
- No `cobrust.toml` config loading (stateless functions).
- No network I/O (no ledger emission).
- No new Cargo feature (lives unconditionally in stdlib root).

D-rating reaffirmed: **D2 sonnet pair** per ADR-0048 §M-AI.1.

## Decision summary (locked)

| # | Decision | Choice |
|---|---|---|
| 1 | Surface shape (flat-fn vs module-path / vs struct) | **Flat-fn** `prompt_*` — reuses ADR-0044 PRELUDE + M-AI.0 OQ-1A precedent. No module-path lowering. No struct surface. |
| 2 | Function set | **Five primitives**: `prompt_render(system, user, vars) -> str`, `prompt_format_few_shot(examples_in, examples_out, current_input) -> str`, `prompt_format_system_user(system, user) -> str`, `prompt_escape_braces(text) -> str`, `llm_complete_structured(prompt, schema_json) -> str`. |
| 3 | `vars` shape (dict[str,str] vs list[str] vs separate ABI) | **`list[str]` of even-indexed `[key1, val1, key2, val2, ...]`** — Cobrust source has no `dict[str,str]` surface at α; list-of-str works under the same `argv()`/`llm_stream()` ABI. Odd-length input → unmatched trailing key is silently dropped. |
| 4 | Variable interpolation syntax in templates | **`{key}` curly-brace placeholder** — matches f-string user mental model + Python `str.format()`. Unknown keys remain literal (not error — Decision 7 mirror of M-AI.0 error surface). Escape via `{{` / `}}` literals (per `prompt_escape_braces` helper). |
| 5 | Few-shot rendering format | **Canonical "Input: <in>\nOutput: <out>\n\n" loop + "Input: <current_input>\nOutput:" trailer** — the most LLM-prompt-tested form (per ADR-0036 production prompt builder precedent). Exact wording locked here; downstream wrappers re-format if needed. |
| 6 | Structured-output (M-AI.1 cap) | **`llm_complete_structured(prompt, schema_json) -> str`** — appends a "Respond with valid JSON matching the schema below: <schema_json>" instruction onto the prompt + calls `llm_dispatch(task="structured", prompt=augmented)` under the hood. **Caller parses the returned JSON string themselves.** No `cobrust.json` stdlib at α; defer JSON-to-struct lowering to follow-up (M-AI.x post-ADR-0049). |
| 7 | Error surface | **Empty string `""` on any failure** — exact mirror of M-AI.0 OQ-2 Decision 7. No `Result[str, E]` at α. Failure paths: unmatched vars list parity, empty examples list with non-empty current input (still produces a useful trailer), `llm_complete_structured` underlying `llm_dispatch` failure. |
| 8 | Dependency direction | **Zero new workspace deps.** Pure-Rust string manipulation. The `llm_complete_structured` shim invokes `crate::llm::llm_dispatch_blocking` directly — same workspace, same `llm-router` feature gate (`#[cfg(feature = "llm-router")]` on the structured fn only; the other four prompt fns are unconditional). |

Each decision section below cites the precedent ADR + the alternative
considered + why it was rejected.

---

## Decision 1 — Surface shape: flat-fn vs module-path vs struct

### Options

#### Option 1A — `cobrust.prompt.Template(system, user, examples).render(vars)` (ADR-0048 prose surface)

- Pros: matches ADR-0048 §M-AI.1 preliminary spec exactly; future-proof for v0.3.0 once recursive-types + dict-of-str land.
- Cons:
  - Requires struct codegen (`Template` is a record with `system: str`, `user: str`, `examples: list[Example]`). **ADR-0049 (TD-Recursive-Types Phase 7.5) is a v0.2.0-alpha P0 blocker** but lands AFTER M-AI.1..M-AI.6 per ADR-0048 dispatch order — M-AI.1 cannot wait for it.
  - Method dispatch on `Template` requires vtable / Aggregate field machinery that codegen does not yet emit (same constraint that drove ADR-0044 §1B "no method-chain at α").
  - Requires module-path lowering for `cobrust.prompt.*` syntax — explicitly rejected for M-AI.0 (OQ-1A) and the same reasoning applies.
- **Rejected for M-AI.1.** Struct surface is its own ADR.

#### Option 1B — Flat-fn `prompt_*` PRELUDE entries (CHOSEN)

```cobrust
let rendered: str = prompt_render(
    "You are a Cobrust expert.",
    "Translate this Python to Cobrust: {code}",
    ["code", "def foo(): pass"],
)

let few_shot: str = prompt_format_few_shot(
    ["x = 1", "y = 2"],
    ["let x: i64 = 1", "let y: i64 = 2"],
    "z = 3",
)

let result: str = llm_complete_structured(
    rendered,
    "{\"type\":\"object\",\"properties\":{\"code\":{\"type\":\"string\"}}}",
)
```

- Pros:
  - **Architecturally identical** to M-AI.0 + ADR-0044 W2 input/argv pattern. Zero new MIR/HIR/types primitives.
  - PRELUDE adds 5 stub fns; intrinsic-rewrite redirects callsites to runtime symbols; C-ABI shims wrap pure-Rust helpers; codegen needs only new `runtime_helper_signatures` entries.
  - Fits cleanly inside the 2-4 hr M-AI.1 budget per ADR-0048.
  - `list[str]` for `vars` / few-shot examples reuses the same heap-list-of-Str ABI as `argv()` / `llm_stream()` — already exercised by M-AI.0 Tier 2 #7.
- Cons:
  - Same flat-fn α naming convention question as M-AI.0 OQ-1A — already resolved (P10 ratified flat-fn for M-AI.0; α inherits the precedent).
  - The mitigation: once module-path lowering + struct codegen ship (M-AI.x candidate post-ADR-0049), PRELUDE additionally re-exports the same runtime targets under `cobrust.prompt.Template.render` etc., preserving the flat-fn α names as backward-compat aliases.
- **Chosen.** No new open question — α flat-fn convention is now precedent, not novelty.

### Constitution check

- §2.1 "one way to do each thing" — flat-fn is the one way at α (matching M-AI.0).
- §2.2 drop-list — no violation.
- §3.3 atomic-commit doc rule — applies; the impl PR ships zh + en + agent docs documenting the five prompt fns.

### Surface (refined)

```cobrust
# Variable interpolation. Reads `vars` as even-indexed [k1, v1, k2, v2, ...]
# pairs and substitutes `{k}` placeholders in both `system` and `user`
# templates. Unknown keys remain literal. Returns the system template +
# "\n" + user template (joined; downstream caller decides whether to
# send as system / user / one prompt). On empty `vars` returns system +
# "\n" + user verbatim. Decision 4 + Decision 7 — failures return "".
fn prompt_render(system: str, user: str, vars: list[str]) -> str

# Few-shot format. Renders the canonical "Input: <in_i>\nOutput: <out_i>"
# pair loop over `examples_in[i]` / `examples_out[i]` (truncates at the
# shorter list), then appends "Input: <current_input>\nOutput:" trailer.
# Decision 5. Empty examples list → just "Input: <current>\nOutput:".
# Mismatched length → truncate to min(in, out). Decision 7 — failures "".
fn prompt_format_few_shot(
    examples_in: list[str],
    examples_out: list[str],
    current_input: str,
) -> str

# Simple system+user concatenator. Returns "<system>\n\n<user>" without
# variable interpolation. Useful as a primitive for hand-built prompts.
# Decision 7 — empty inputs return literal "\n\n".
fn prompt_format_system_user(system: str, user: str) -> str

# Escape `{` and `}` literals so they survive `prompt_render`'s
# interpolation pass. Each `{` becomes `{{`, each `}` becomes `}}`.
# Symmetric to Python's `str.replace`. Decision 4 — escape mechanism.
fn prompt_escape_braces(text: str) -> str

# Structured-output convenience. Appends a "Respond with JSON matching
# the schema below: <schema_json>" instruction onto `prompt`, then
# routes through `llm_dispatch(task="structured", prompt=augmented)`.
# Returns the raw response text (caller parses the JSON themselves).
# Decision 6. Failure (config missing, dispatch error, etc.) → "".
fn llm_complete_structured(prompt: str, schema_json: str) -> str
```

---

## Decision 2 — Function set

ADR-0048 §M-AI.1's preliminary spec implies four primitive operations:
template render, few-shot render, structured-output complete, and
example construction. This spike flattens "example construction" into
the few-shot fn's argument list (no separate `Example` constructor)
and adds two narrowly-scoped helpers (`prompt_format_system_user` and
`prompt_escape_braces`) for ergonomics.

**Why exactly five primitives:**

1. `prompt_render(system, user, vars)` — the headline workhorse.
2. `prompt_format_few_shot(examples_in, examples_out, current_input)` — explicit few-shot composition.
3. `prompt_format_system_user(system, user)` — when no interpolation is needed (skip the var-list parsing overhead + clearer source-level intent).
4. `prompt_escape_braces(text)` — needed when interpolation values themselves contain `{` `}` (e.g. JSON examples in few-shot). Tiny but mandatory for correctness.
5. `llm_complete_structured(prompt, schema_json)` — wraps M-AI.0 `llm_dispatch` with a structured-output instruction.

### Why NOT a `prompt_template_from_file(path)` reader

- Reading prompts from disk introduces I/O failure modes + path-handling decisions (CWD vs config dir vs ADR-0044 stdin-style scope). Out of M-AI.1 scope. Defer to M-AI.4 (`cobrust.ast` reads .cb files) or a separate M-AI.x file-reading primitive.

### Why NOT `prompt_validate(template, vars) -> i64` (returns 1 if all `{k}` are matched)

- Useful but not yet motivated by α use-cases. Decision 7 collapses "unknown key remains literal" to silent forwarding — if a user wants strict validation, they can iterate the template themselves with `str_at` / `str_len`. Defer.

### Why NOT `prompt_render_to_chat_messages(...) -> list[str]` that returns role-tagged messages

- Cobrust source has no `(str, str)` tuple type at α (per `lc100-pattern-b-list-of-str-gap.md`). The `[role1, content1, role2, content2, ...]` list-of-str pattern works but is fragile; would need a separate ADR to commit to it. Defer to M-AI.x once tuple/struct lands.

---

## Decision 3 — `vars` shape: `list[str]` vs new dict-of-str ABI

### Options

#### Option 3A — `dict[str, str]` first-class parameter

```cobrust
let rendered: str = prompt_render(system, user, {"code": "def foo()"})
```

- Pros: maps cleanly to Python `str.format(**kwargs)` mental model.
- Cons: **dict-of-str source-level lowering is not yet exposed at α**. The stdlib has `__cobrust_dict_*` C-ABI for i64→i64 maps (`crates/cobrust-stdlib/src/collections.rs`), but Cobrust source can only construct `dict[i64, i64]` at this milestone (the parser parses `{...}` only for empty dicts and i64-keyed literals). Adding `dict[str, str]` source-level construction is a multi-crate refactor (parser literal-lowering + type-checker + codegen + stdlib accessor). D3+ in isolation — blows the 2-4 hr M-AI.1 budget.
- **Rejected for M-AI.1.**

#### Option 3B — Two parallel `list[str]` parameters: `keys` and `values` (CHOSEN: rejected sub-form)

```cobrust
let rendered: str = prompt_render(system, user, ["code"], ["def foo()"])
```

- Pros: cleaner type signature; harder to misuse (each key has exactly one value slot).
- Cons: PRELUDE signatures pin arg counts; this would be a 4-arg fn vs Option 3C's 3-arg fn. Slightly more verbose source-level call sites.
- Considered but **dropped in favor of 3C** since both have the same complexity and 3C is one arg fewer.

#### Option 3C — Single `list[str]` even-indexed `[k1, v1, k2, v2, ...]` (CHOSEN)

```cobrust
let rendered: str = prompt_render(system, user, ["code", "def foo()", "lang", "python"])
```

- Pros:
  - Single `list[str]` parameter — same shape `argv()` returns; users already understand it.
  - Lower arity (3 args vs 4 args) → simpler source-level callsite when there are few variables.
  - At α with no struct/tuple stdlib, this is the most natural source-level shape (developer reads it as "key/value pair list").
- Cons:
  - Slightly looser type safety: odd-length list → trailing key has no value. **Decision 7 says: silently drop the unmatched trailing key (no error)**. Documented in Decision 4 and in the stdlib module-level doc.
  - Compared to dict, key order matters less to the user but matters to the runtime (we iterate sequentially). Documented as "later same-key bindings override earlier ones" so deduplication semantics are predictable.
- **Chosen.** Future migration: once `dict[str, str]` source-level construction ships, `prompt_render` adds an overload (PRELUDE adds `prompt_render_with_dict(system, user, vars: dict[str, str]) -> str`); the list-of-str variant stays as α-back-compat — no ABI break.

---

## Decision 4 — Variable interpolation syntax

Single decision: `{key}` curly-brace placeholder, matching Python
`str.format()` and Cobrust's own f-string syntax mental model.

- Single-brace literal `{` or `}` escapes to `{{` / `}}` (Python-compat).
- Unknown `{k}` in the template (where `k` is not in `vars`) remains literal — silent forwarding. The user can validate via `prompt_format_few_shot` post-render if needed, or via their own `str_eq` walk.
- The interpolation pass is **single-pass** — substituted text is NOT re-scanned for placeholders. Prevents recursive-substitution surprises.

Algorithm (lockable for P7-DEV):

```
fn prompt_render(system: &str, user: &str, vars: &[String]) -> String:
    1. Combine: combined = format!("{system}\n{user}")
    2. Build BTreeMap<&str, &str> from even-indexed vars (silently drop trailing).
    3. Single-pass walk:
       - Find next `{`. If preceded/followed by another `{`, emit literal `{` and skip.
       - Else read until matching `}`. The `{...}` becomes the lookup key.
       - If key in map → emit map[key]. Else → emit the literal `{key}`.
    4. Return combined.
```

### `prompt_escape_braces` is the symmetric pre-pass

If a user has a literal `{x}` they want to ship into the final
rendered prompt (e.g. a JSON example in a few-shot), they call
`prompt_escape_braces(my_text)` first, which produces `{{x}}`,
which the interpolation pass collapses back to `{x}`.

---

## Decision 5 — Few-shot rendering format

The canonical few-shot format (per ADR-0036 production prompt builder
precedent + survey of LLM prompt-design literature) is:

```
Input: <example_in_0>
Output: <example_out_0>

Input: <example_in_1>
Output: <example_out_1>

...

Input: <current_input>
Output:
```

- Trailing newline after each `Output: <out>` and one blank line between pairs.
- Final `Output:` has NO trailing newline (LLM continuation expected).
- If `examples_in.len() != examples_out.len()`, truncate to the shorter (Decision 7 silent-failure parity).
- Empty examples lists → just `Input: <current>\nOutput:` trailer.

This format is **locked** at the stdlib level so downstream users get
a consistent shape. Users who need a different format (e.g. role-
tagged chat messages) can compose `prompt_format_system_user` +
manual concatenation. M-AI.1 ships one well-tested format, not a
configuration matrix.

---

## Decision 6 — Structured-output (M-AI.1 cap)

ADR-0048 §M-AI.1's preliminary surface uses `dict` for the schema
arg. Same constraint as Decision 3: dict-of-str source-level
construction is not exposed at α.

`llm_complete_structured` takes the schema as a **`str` parameter
containing pre-serialized JSON** that the user authored (or that they
loaded from a literal string at source). The shim:

1. Augments `prompt` with the structured-output instruction:
   ```
   <prompt>

   Respond with valid JSON matching this schema:
   <schema_json>
   ```
2. Routes through `llm_dispatch(task="structured", prompt=augmented)`.
3. Returns the raw response text.

**The caller is responsible for parsing the returned JSON.** Cobrust
has no `cobrust.json` stdlib at α (parser + serializer are M-AI.x
candidate post-ADR-0049 once struct surface lands). Source-level
users will manipulate the returned string with `str_at` / `str_len` /
`str_eq_lit` until JSON helpers ship.

### Why route through `llm_dispatch` instead of `llm_complete` directly

- `llm_dispatch` consults the user's `cobrust.toml`
  `[routing.structured]` section, letting users pick their preferred
  structured-output-friendly model per project (e.g. gpt-5 with
  response-format=json, vs Claude with prompt-only JSON shaping).
- If `[routing.structured]` is undeclared, M-AI.0's Decision 7
  collapses the dispatch to "" — caller sees empty string + ledger
  captures the route-not-found error.
- Future enhancement: when the LLM router crate exposes
  `Provider::response_format` (currently a router-internal field,
  not part of `Task::Custom`), `llm_complete_structured` will route
  through that explicitly. M-AI.1 deliberately stays on the simpler
  prompt-augmentation path.

### Module gating

`llm_complete_structured` requires `cobrust-llm-router` (it calls
into `crate::llm::llm_dispatch_blocking`). It lives in
`crates/cobrust-stdlib/src/prompt.rs` behind
`#[cfg(feature = "llm-router")]`. The other four `prompt_*` fns are
pure Rust and live unconditionally in the same module (no Cargo
feature gate). The `pub mod prompt` declaration in `lib.rs` is
unconditional.

---

## Decision 7 — Error surface

Exact mirror of M-AI.0 OQ-2 Decision 7:

- All five fns return `""` on any failure.
- No panic, no UB, no observable crash.
- Failure modes per fn:
  - `prompt_render`: odd-length vars list → drop trailing key silently. Empty system+user → returns `"\n"`. Never errors.
  - `prompt_format_few_shot`: mismatched list lengths → truncate to min. Empty current_input → still emits `Input: \nOutput:` trailer.
  - `prompt_format_system_user`: always succeeds (string concatenation cannot fail under stdlib's mimalloc allocator at α scale).
  - `prompt_escape_braces`: always succeeds.
  - `llm_complete_structured`: underlying `llm_dispatch_blocking` returns `""` on dispatch failure → propagates `""` upward.

### Why no Result[str, E] at α

Same rationale as M-AI.0: typed-Result MIR lowering is a Phase F.1.x
prereq (ADR-0044a). Sequencing M-AI.1 in front of it is a scope
error. Ledger-capture of underlying LLM errors continues to work
through M-AI.0's existing router emission — no new ledger work in
M-AI.1.

---

## Decision 8 — Dependency direction

`crates/cobrust-stdlib/src/prompt.rs` (NEW module):

- Pure-Rust string-manipulation for `prompt_render` / `prompt_format_few_shot` / `prompt_format_system_user` / `prompt_escape_braces`.
- **Zero new workspace dependencies.** The four pure fns use only `std::String` / `std::format!` / `std::collections::BTreeMap`.
- `llm_complete_structured` invokes `crate::llm::llm_dispatch_blocking` — same crate, no new dep direction.

### `Cargo.toml` impact

```toml
# crates/cobrust-stdlib/Cargo.toml — NO CHANGES.
# The prompt module is part of the unconditional public surface.
# The structured-output fn is gated by the existing `llm-router`
# feature via a `#[cfg(feature = "llm-router")]` on the fn alone.
```

### Why not a `prompt-runtime` Cargo feature

- Four out of five prompt fns are pure-Rust + tiny. Gating them
  behind a feature would force every consumer to opt-in for
  string-formatting helpers — out of proportion to the cost
  (~150 LOC).
- The fifth fn (`llm_complete_structured`) is naturally gated
  by the existing `llm-router` feature it transitively depends
  on — no new feature needed.

---

## Implementation map (binding for P7-DEV)

### Crate touch list

| Crate | File | What changes |
|---|---|---|
| `cobrust-stdlib` | `src/prompt.rs` (new) | Rust-side `prompt_render_helper` / `prompt_format_few_shot_helper` / `prompt_format_system_user_helper` / `prompt_escape_braces_helper` / `llm_complete_structured_helper` + five C-ABI shims (`__cobrust_prompt_render`, `__cobrust_prompt_format_few_shot`, `__cobrust_prompt_format_system_user`, `__cobrust_prompt_escape_braces`, `__cobrust_llm_complete_structured`). |
| `cobrust-stdlib` | `src/lib.rs` | `pub mod prompt;` (unconditional). |
| `cobrust-stdlib` | `Cargo.toml` | **No changes** (pure-Rust + reuses `llm-router` feature). |
| `cobrust-cli` | `src/build.rs` | Extend `PRELUDE` to declare five new stub fns. |
| `cobrust-cli` | `src/build/intrinsics.rs` | Add five `PROMPT_*_RUNTIME_SYMBOL` / `LLM_COMPLETE_STRUCTURED_RUNTIME_SYMBOL` consts; extend `IntrinsicDefIds` + `Kind` enum + `kind_for_name` / `kind_for_def_id` / `rewrite_print` match arms. |
| `cobrust-codegen` | `src/cranelift_backend.rs` | Add five entries to `runtime_helper_signatures()`. |
| `cobrust-stdlib` | `tests/prompt_corpus.rs` (new) | Rust-side unit + integration tests for the five blocking helpers (Tier 1, ≥ 15 tests). |
| `cobrust-stdlib` | `tests/prompt_cabi_corpus.rs` (new) | C-ABI shim tests (Tier 2, ≥ 10 tests). |
| `cobrust-stdlib` | `tests/prompt_fuzz.rs` (new) | proptest 1024-iter fuzz (≥ 3 properties). |
| `cobrust-cli` | `tests/intrinsics_prompt.rs` (new) | End-to-end `.cb` source → compile → run tests (Tier 3, ≥ 5 tests). |
| `docs/agent/modules/stdlib.md` | edit | Add `prompt` module to the public surface table. |
| `docs/agent/modules/cli.md` | edit | Note the PRELUDE + intrinsic-rewrite extension (mirror M-AI.0 entry). |
| `docs/human/{zh,en}/architecture.md` | edit | Document the five flat-fns in the existing "AI-native stdlib" subsection (the section M-AI.0 created). |
| `cobrust.toml.example` | edit | Add a `[routing.structured]` sample section + comment. |

### Runtime helper signatures (codegen amendment)

```rust
// In runtime_helper_signatures(), append after the M-AI.0 block:

// -- M-AI.1 (α Phase 3): cobrust.prompt source-level binding ------
// `prompt_render(system: str, user: str, vars: list[str]) -> str`.
// system + user are Str pointers; vars is a list pointer (heap List
// whose i64 slots store heap-Str pointers — same shape `__cobrust_argv`
// returns + `llm_stream` returns).
out.push(("__cobrust_prompt_render", sig(call_conv, &[p, p, p], Some(p))));

// `prompt_format_few_shot(examples_in: list[str], examples_out: list[str],
//                         current_input: str) -> str`.
// All three are pointers.
out.push((
    "__cobrust_prompt_format_few_shot",
    sig(call_conv, &[p, p, p], Some(p)),
));

// `prompt_format_system_user(system: str, user: str) -> str`.
out.push((
    "__cobrust_prompt_format_system_user",
    sig(call_conv, &[p, p], Some(p)),
));

// `prompt_escape_braces(text: str) -> str`.
out.push(("__cobrust_prompt_escape_braces", sig(call_conv, &[p], Some(p))));

// `llm_complete_structured(prompt: str, schema_json: str) -> str`.
out.push((
    "__cobrust_llm_complete_structured",
    sig(call_conv, &[p, p], Some(p)),
));
```

### PRELUDE amendment

```python
# Append to PRELUDE constant in crates/cobrust-cli/src/build.rs:37 string:
fn prompt_render(system: str, user: str, vars: list[str]) -> str:
    return ""

fn prompt_format_few_shot(examples_in: list[str], examples_out: list[str], current_input: str) -> str:
    return ""

fn prompt_format_system_user(system: str, user: str) -> str:
    return ""

fn prompt_escape_braces(text: str) -> str:
    return ""

fn llm_complete_structured(prompt: str, schema_json: str) -> str:
    return ""
```

### Intrinsic-rewrite extension

Five new arms in `kind_for_name` / `kind_for_def_id` /
`IntrinsicDefIds` / `rewrite_print`'s match block. Each arm sets
`*func = Operand::Constant(Constant::Str(PROMPT_*_RUNTIME_SYMBOL))`
and preserves the operand list (no expansion needed — all args are
pointer-only Str / list-pointer operands).

### C-ABI shim shape (binding for P7-DEV)

```rust
// crates/cobrust-stdlib/src/prompt.rs (new module)

//! `cobrust.prompt` — source-level binding to prompt-composition
//! primitives.
//!
//! ADR-0048 §"M-AI.1 — cobrust.prompt" pins this module; refined by
//! `docs/agent/spike/m-ai-1-cobrust-prompt-spike.md` (SHA TBD) and
//! ratified by `[P10-ALPHA-PHASE-3-RATIFY]`.
//!
//! Five source-level intrinsics live in `PRELUDE`:
//!
//! - `prompt_render(system, user, vars) -> str`
//! - `prompt_format_few_shot(examples_in, examples_out, current_input) -> str`
//! - `prompt_format_system_user(system, user) -> str`
//! - `prompt_escape_braces(text) -> str`
//! - `llm_complete_structured(prompt, schema_json) -> str`
//!   (gated by `#[cfg(feature = "llm-router")]` via the M-AI.0 path)
//!
//! Decision references: see spike §Decision 1 (flat-fn), §Decision 3
//! (list[str] even-indexed vars), §Decision 4 (`{k}` interpolation +
//! `{{` `}}` escapes), §Decision 5 (canonical few-shot format),
//! §Decision 6 (structured wraps `llm_dispatch`), §Decision 7
//! (empty-on-failure error surface).

use std::collections::BTreeMap;

// =====================================================================
// Rust-side blocking helpers — unit-testable counterparts of the C-ABI
// shims. Decision 7: failures collapse to empty String.
// =====================================================================

/// `prompt_render` — variable interpolation pass per Decision 4.
#[must_use]
pub fn prompt_render_helper(system: &str, user: &str, vars: &[String]) -> String {
    // Build BTreeMap from even-indexed pairs. Drop trailing odd key
    // silently (Decision 3 + 7). Later same-key bindings override.
    let mut map: BTreeMap<&str, &str> = BTreeMap::new();
    let mut i = 0;
    while i + 1 < vars.len() {
        map.insert(vars[i].as_str(), vars[i + 1].as_str());
        i += 2;
    }

    let combined = format!("{system}\n{user}");
    let mut out = String::with_capacity(combined.len());
    let bytes = combined.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() {
        match bytes[idx] {
            b'{' if idx + 1 < bytes.len() && bytes[idx + 1] == b'{' => {
                out.push('{');
                idx += 2;
            }
            b'}' if idx + 1 < bytes.len() && bytes[idx + 1] == b'}' => {
                out.push('}');
                idx += 2;
            }
            b'{' => {
                // Find matching `}`.
                let start = idx + 1;
                let mut end = start;
                while end < bytes.len() && bytes[end] != b'}' {
                    end += 1;
                }
                if end >= bytes.len() {
                    // Unterminated `{` — emit literal rest.
                    out.push_str(&combined[idx..]);
                    break;
                }
                let key = &combined[start..end];
                if let Some(v) = map.get(key) {
                    out.push_str(v);
                } else {
                    // Unknown key — keep literal (Decision 4).
                    out.push_str(&combined[idx..=end]);
                }
                idx = end + 1;
            }
            _ => {
                // Push char (handle multi-byte via str slicing).
                let c_start = idx;
                let ch = combined[c_start..].chars().next().unwrap();
                out.push(ch);
                idx += ch.len_utf8();
            }
        }
    }
    out
}

/// `prompt_format_few_shot` — canonical format per Decision 5.
#[must_use]
pub fn prompt_format_few_shot_helper(
    examples_in: &[String],
    examples_out: &[String],
    current_input: &str,
) -> String {
    let n = examples_in.len().min(examples_out.len());
    let mut out = String::new();
    for i in 0..n {
        out.push_str("Input: ");
        out.push_str(&examples_in[i]);
        out.push('\n');
        out.push_str("Output: ");
        out.push_str(&examples_out[i]);
        out.push_str("\n\n");
    }
    out.push_str("Input: ");
    out.push_str(current_input);
    out.push_str("\nOutput:");
    out
}

/// `prompt_format_system_user`.
#[must_use]
pub fn prompt_format_system_user_helper(system: &str, user: &str) -> String {
    format!("{system}\n\n{user}")
}

/// `prompt_escape_braces`.
#[must_use]
pub fn prompt_escape_braces_helper(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '{' => out.push_str("{{"),
            '}' => out.push_str("}}"),
            _ => out.push(ch),
        }
    }
    out
}

/// `llm_complete_structured` — only available with `llm-router` feature.
#[cfg(feature = "llm-router")]
#[must_use]
pub fn llm_complete_structured_helper(prompt: &str, schema_json: &str) -> String {
    let augmented = format!(
        "{prompt}\n\nRespond with valid JSON matching this schema:\n{schema_json}"
    );
    crate::llm::llm_dispatch_blocking("structured", &augmented)
}

// =====================================================================
// C-ABI shims (codegen targets these via the intrinsic-rewrite pass)
// =====================================================================

/// Read a heap `Str` pointer as a `String`. Tolerates null + empty.
/// Mirrors the M-AI.0 `read_str_buf` helper. P7-DEV may inline or
/// share via a `pub(crate)` re-export from `crate::llm` — DEV decides.
unsafe fn read_str_buf(buf: *mut u8) -> String {
    if buf.is_null() {
        return String::new();
    }
    unsafe {
        let ptr = crate::fmt::__cobrust_str_ptr(buf);
        let len = crate::fmt::__cobrust_str_len(buf);
        if ptr.is_null() || len <= 0 {
            return String::new();
        }
        let bytes = std::slice::from_raw_parts(ptr, len as usize);
        std::str::from_utf8(bytes).unwrap_or("").to_string()
    }
}

/// Allocate a heap `Str` buffer carrying `s`. Mirrors M-AI.0
/// `alloc_str_buffer`. Same caveat about sharing.
fn alloc_str_buffer(s: &str) -> *mut u8 {
    unsafe {
        let buf = crate::fmt::__cobrust_str_new();
        if !s.is_empty() {
            crate::fmt::__cobrust_str_push_static(buf, s.as_ptr(), s.len() as i64);
        }
        buf
    }
}

/// Read a `list[str]` heap pointer into a `Vec<String>`. Each slot is
/// an i64 storing a heap-Str pointer per the `__cobrust_argv` /
/// `__cobrust_llm_stream` precedent.
unsafe fn read_list_of_str(list_ptr: *mut u8) -> Vec<String> {
    if list_ptr.is_null() {
        return Vec::new();
    }
    unsafe {
        let len = crate::collections::__cobrust_list_len(list_ptr);
        let mut out = Vec::with_capacity(len as usize);
        for i in 0..len {
            let elem = crate::collections::__cobrust_list_get(list_ptr, i) as *mut u8;
            out.push(read_str_buf(elem));
        }
        out
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_prompt_render(
    system: *mut u8,
    user: *mut u8,
    vars: *mut u8,
) -> *mut u8 {
    let s = unsafe { read_str_buf(system) };
    let u = unsafe { read_str_buf(user) };
    let vs = unsafe { read_list_of_str(vars) };
    let result = prompt_render_helper(&s, &u, &vs);
    alloc_str_buffer(&result)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_prompt_format_few_shot(
    examples_in: *mut u8,
    examples_out: *mut u8,
    current_input: *mut u8,
) -> *mut u8 {
    let xin = unsafe { read_list_of_str(examples_in) };
    let xout = unsafe { read_list_of_str(examples_out) };
    let cur = unsafe { read_str_buf(current_input) };
    let result = prompt_format_few_shot_helper(&xin, &xout, &cur);
    alloc_str_buffer(&result)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_prompt_format_system_user(
    system: *mut u8,
    user: *mut u8,
) -> *mut u8 {
    let s = unsafe { read_str_buf(system) };
    let u = unsafe { read_str_buf(user) };
    alloc_str_buffer(&prompt_format_system_user_helper(&s, &u))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_prompt_escape_braces(text: *mut u8) -> *mut u8 {
    let t = unsafe { read_str_buf(text) };
    alloc_str_buffer(&prompt_escape_braces_helper(&t))
}

/// Gated by `llm-router` feature; if the feature is off, the shim
/// still compiles but returns empty (preserves PRELUDE callsites at
/// no-feature builds).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_llm_complete_structured(
    prompt: *mut u8,
    schema_json: *mut u8,
) -> *mut u8 {
    let p = unsafe { read_str_buf(prompt) };
    let s = unsafe { read_str_buf(schema_json) };
    #[cfg(feature = "llm-router")]
    {
        return alloc_str_buffer(&llm_complete_structured_helper(&p, &s));
    }
    #[cfg(not(feature = "llm-router"))]
    {
        let _ = (p, s);
        return alloc_str_buffer("");
    }
}
```

### Test plan (binding for P7-TEST)

#### Tier 1 — Rust-side blocking helpers (≥ 15 tests, `tests/prompt_corpus.rs`)

Pure functions — no synthetic-provider needed for the four pure fns.
`llm_complete_structured_helper` reuses the M-AI.0 synthetic seam
(`cobrust_stdlib::llm::__test_register_synthetic_provider`).

1. `prompt_render_helper` with empty vars returns `format!("{system}\n{user}")`.
2. `prompt_render_helper` with single key/value pair substitutes correctly.
3. `prompt_render_helper` with multiple pairs substitutes all.
4. `prompt_render_helper` with unknown placeholder keeps literal `{unknown}`.
5. `prompt_render_helper` with `{{` and `}}` escapes correctly to `{` `}`.
6. `prompt_render_helper` with odd-length vars list drops trailing key silently.
7. `prompt_render_helper` with empty system + empty user + non-empty vars returns `"\n"` (no substitution opportunities).
8. `prompt_render_helper` with UTF-8 multi-byte template + UTF-8 multi-byte value substitutes byte-correctly.
9. `prompt_render_helper` with later same-key override returns latest binding.
10. `prompt_format_few_shot_helper` with one example pair + current input produces canonical format.
11. `prompt_format_few_shot_helper` with multiple example pairs + current input emits N blocks + trailer.
12. `prompt_format_few_shot_helper` with empty examples lists + non-empty current input emits just trailer.
13. `prompt_format_few_shot_helper` with mismatched lengths truncates to min.
14. `prompt_format_few_shot_helper` with UTF-8 multi-byte content preserves bytes exactly.
15. `prompt_format_system_user_helper` produces `"<system>\n\n<user>"`.
16. `prompt_escape_braces_helper` escapes `{` to `{{` and `}` to `}}`.
17. `prompt_escape_braces_helper` round-trips through `prompt_render_helper` (escape → render → original literal).
18. `llm_complete_structured_helper` (gated): with synthetic provider routes "structured" task → returns canned response.
19. `llm_complete_structured_helper` (gated): with missing cobrust.toml returns "".
20. **verify.py oracle**: pick 5 deterministic Tier 1 cases (one per fn, plus the round-trip); each must match `tests/prompt_corpus_verify.py <case>` output (ADR-0047a mandate). Test #20 is the gate-binding assertion.

#### Tier 2 — C-ABI shims (≥ 10 tests, `tests/prompt_cabi_corpus.rs`)

1. `__cobrust_prompt_render` with three valid Str buffers + list-of-str vars produces correct interpolated text.
2. `__cobrust_prompt_render` with null vars list (`std::ptr::null_mut()`) returns valid result (vars empty).
3. `__cobrust_prompt_render` with null system/user pointer → empty string treatment + result is the non-null arg.
4. `__cobrust_prompt_format_few_shot` with three lists builds correct format.
5. `__cobrust_prompt_format_few_shot` with empty lists builds correct trailer-only output.
6. `__cobrust_prompt_format_system_user` simple concat works.
7. `__cobrust_prompt_escape_braces` escapes literal braces.
8. `__cobrust_llm_complete_structured` with synthetic provider + `[routing.structured]` returns canned text.
9. `__cobrust_llm_complete_structured` with `llm-router` feature off (compiled into the test crate via `--no-default-features`)  returns "" (DEV exercises this via a separate `cfg`-gated test if practical, or documents that the test binary always has the feature on and skips this case).
10. UTF-8 round-trip: each shim with multi-byte text in/out is byte-identical.

#### Tier 3 — End-to-end `.cb` source → run (≥ 5 tests, `intrinsics_prompt.rs`)

Each test:
1. Writes a tiny `.cb` program using one of the five prompt fns.
2. Compiles via `cobrust build`.
3. Runs the executable, captures stdout, asserts.

Programs:

1. `prompt_render` with hardcoded system + user + 2-pair vars list → asserts substituted output in stdout.
2. `prompt_format_few_shot` with two examples + current input → asserts canonical format in stdout.
3. `prompt_format_system_user` simple concat → asserts joined string.
4. `prompt_escape_braces` of `"hello {world}"` → asserts `"hello {{world}}"`.
5. `llm_complete_structured` with wiremock-backed `[routing.structured]` (mirror M-AI.0 Tier 3 #1's wiremock setup) → asserts canned JSON response in stdout.

#### verify.py mandate (per ADR-0047a)

`tests/prompt_corpus_verify.py` — Python 3.11+, no third-party deps.
Mirror `tests/llm_corpus_verify.py` shape. Maps each Tier 1
deterministic case name to its expected text. P7-TEST authors this
file as part of the corpus.

For M-AI.1, **most** functions are deterministic pure-Rust string
manipulation, so verify.py is straightforward: reimplement the same
algorithm in Python and assert byte-identical output. This is the
ADR-0047a F23-A oracle-independence check.

For `llm_complete_structured_helper`, verify.py for the gated tests
prints the synthetic canned response (just like M-AI.0's
`llm_corpus_verify.py`).

#### Fuzz (≥ 1024 inputs, `tests/prompt_fuzz.rs`)

Per ADR-0044 `io_input_fuzz.rs` precedent (proptest 1024 iters).
Three properties:

1. **`prompt_render` never panics** on arbitrary UTF-8 lossy
   `(system, user)` + arbitrary `Vec<String>` vars (0..=16 elements
   each ≤ 4 KiB).
2. **`prompt_format_few_shot` never panics** on arbitrary UTF-8 lossy
   `Vec<String>` examples_in + examples_out + current_input.
3. **`prompt_escape_braces` round-trip**: for any text `t`,
   `prompt_render_helper("", &prompt_escape_braces_helper(&t), &[])`
   ends with `t` after the leading `"\n"` (verify the escape is
   defeating the interpolation pass exactly as Decision 4 specifies).

---

## Open questions (for CTO sign-off before P7 fires)

### OQ-1 — `prompt_render` signature: 3-arg fn or 4-arg (separate keys + values lists)

The spike chose Option 3C (single even-indexed `list[str]`). The
alternative was Option 3B (separate `keys: list[str]` + `values:
list[str]`).

- 3C pro: lower arity, source-level cleaner.
- 3B pro: harder-to-misuse type signature (length-mismatch is a
  list-length comparison, not a parity check).
- The spike chose 3C; this OQ surfaces it for CTO confirmation.

**Recommended**: keep 3C. CTO sign-off needed only to lock the
choice.

### OQ-2 — `llm_complete_structured` routing task name: hardcoded "structured" vs configurable

The spike chose hardcoded `Task::Custom("structured")`. The
alternative was to add a fourth arg `task_name: str` to let users
pick the routing-table entry.

- Hardcoded pro: simpler source-level call (2 args vs 3).
- Configurable pro: lets users route different structured-output
  workflows through different models.
- The spike chose hardcoded; users wanting per-call control can
  always call `llm_dispatch(custom_task, prompt + schema_instruction)`
  directly — `llm_complete_structured` is a convenience layer.

**Recommended**: keep hardcoded. CTO sign-off requested.

### OQ-3 — Few-shot trailer: `"Output:"` (no newline) vs `"Output: "` (trailing space) vs `"Output:\n"`

The spike chose `"Output:"` (no trailing whitespace) per Decision 5.

- `"Output:"` pro: cleanest LLM continuation surface (model can emit either ` foo` or `\nfoo`).
- `"Output: "` pro: forces model to emit content on same line; useful for short structured outputs.
- `"Output:\n"` pro: signals "begin a new line" to the model; reads natural for multi-line outputs.

Different LLM providers respond best to different choices; the M-AI.5 eval benchmark (per ADR-0048 §M-AI.5) will measure this.

**Recommended**: pick `"Output:"` (no whitespace) for α-stability; document that wrapper code can append `" "` or `"\n"` if needed. Reopen in M-AI.5 if eval data shows a clear winner.

CTO sign-off: confirm choice or amend.

---

## Done means (spike Phase 3)

- [x] This document committed to `docs/agent/spike/m-ai-1-cobrust-prompt-spike.md`.
- [ ] CTO ratifies open questions OQ-1 / OQ-2 / OQ-3 via `[P10-ALPHA-PHASE-3-RATIFY]` (or amends).
- [ ] P7-TEST sonnet prompt (drafted in P9 return block) executes against this spec.
- [ ] P7-DEV sonnet prompt (drafted in P9 return block) executes against this spec.

## Why this spike now

ADR-0048 §"Neutral/unknown" line 241-242 explicitly defers M-AI.0..M-AI.4 surface refinement to Phase 2-N spikes. M-AI.0 closed at `2ed8092` post-spike `705f592`; M-AI.1 inherits the same dispatch shape. Without locking these 8 decisions, P7-TEST and P7-DEV would re-litigate them mid-sprint, blowing the 2-4 hr budget.

— P9 opus tech-lead, α Phase 3 dispatch 2026-05-12
