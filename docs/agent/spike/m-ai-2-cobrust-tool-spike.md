---
doc_kind: spike
spike_id: m-ai-2
title: "M-AI.2 cobrust.tool stdlib refined spec (α Phase 4)"
status: refined
date: 2026-05-12
last_verified_commit: TBD
ratifies: adr:0048 §"M-AI.2 — cobrust.tool" preliminary surface
relates_to: [adr:0048, adr:0044, adr:0027, m-ai-0, m-ai-1]
authored_by: P10 CTO session 96e2d0dc (ADSD Phase 1 spike)
---

# Spike M-AI.2 — `cobrust.tool` stdlib refined specification

## Purpose

ADR-0048 §"M-AI.2 — cobrust.tool" sketches an ergonomic future surface:

```cobrust
import cobrust.tool

@cobrust.tool.expose(description="Add two integers")
fn add(a: i64, b: i64) -> i64:
    return a + b

let schema: dict = add.schema()
let result: i64 = cobrust.tool.invoke(tool=add, args={"a": 1, "b": 2})

let registry = cobrust.tool.Registry()
registry.register(add)
let response = cobrust.llm.complete_with_tools(prompt="What is 1 + 2?", tools=registry)
```

That sketch is strategically correct but not α-runnable as written. Current
Cobrust can parse decorators, but decorators do not execute macro/reflection
semantics: the type checker passes `ItemKind::Decorated` through to the inner
item (`crates/cobrust-types/src/check.rs:148`, `:210`) and MIR lowering does
the same (`crates/cobrust-mir/src/lower.rs:129-130`). Attribute access is also
not a method-dispatch surface: type checking returns a fresh inference variable
(`crates/cobrust-types/src/check.rs:679-685`) and MIR lowering projects a
placeholder `.Field(0)` (`crates/cobrust-mir/src/lower.rs:524-526`,
`:1164-1175`).

This spike therefore locks an **α-runnable flat-function tool surface** that
reuses the same architecture as M-AI.0 and M-AI.1:

- `PRELUDE` flat function stubs (`crates/cobrust-cli/src/build.rs:36`)
- intrinsic rewrite to runtime symbols (`crates/cobrust-cli/src/build/intrinsics.rs`)
- C-ABI helper signatures in codegen (`crates/cobrust-codegen/src/cranelift_backend.rs:1886-2055`)
- Rust-side helper functions + C-ABI shims in `cobrust-stdlib`

It also explicitly defers the future decorator/method/reflection surface so
M-AI.2 does not become a F24-style simulation that passes tests while claiming
coverage of a feature the compiler does not actually implement.

D-rating reaffirmed: **D3**. This touches CLI PRELUDE, MIR intrinsic rewrite,
codegen runtime helper signatures, stdlib runtime shims, tests, and triple-tree
docs. Per ADSD, Phase 2 uses an **Opus P9 + Opus TEST/DEV pair**.

## Decision summary (locked)

| # | Decision | Choice |
|---|---|---|
| 1 | Surface shape | **Flat functions**. No `@cobrust.tool.expose`, no `.schema()`, no `Registry` class at α. Future ergonomic surface deferred. |
| 2 | Function set | Five α primitives: `tool_schema`, `tool_registry_new`, `tool_registry_register`, `tool_invoke`, `llm_complete_with_tools`. |
| 3 | Schema format | Canonical compact JSON string. Caller supplies `parameters_json`; stdlib validates and canonicalizes it. |
| 4 | Registry format | Canonical compact JSON string: `{"tools":[<schema>...]}`. Duplicate tool names use last-schema-wins. |
| 5 | Invocation semantics | **Closed-world α dispatcher**, not user-defined function reflection. M-AI.2 ships a deterministic `add_i64` exemplar only. Unknown tools return `""`. |
| 6 | LLM tool-calling | Prompt-augmentation wrapper over M-AI.0 `llm_dispatch("tools", augmented_prompt)`. Native provider tool API deferred. |
| 7 | Error surface | Empty string `""` on malformed JSON / invalid schema / unknown tool / router failure, matching M-AI.0 and M-AI.1. |
| 8 | Dependencies | `cobrust-stdlib` may add `serde` + `serde_json` for deterministic JSON validation/canonicalization. |

## Decision 1 — Surface shape: flat functions, not decorator/method syntax

### Option 1A — Implement ADR-0048 literally with decorators + methods

- Pros: matches the future user story exactly.
- Cons:
  - Decorators are currently syntactic wrappers only; they do not generate
    schema metadata or rewrite function definitions.
  - `.schema()` depends on real attribute/method dispatch, but attribute access
    is currently a fresh type variable plus MIR `.Field(0)` placeholder.
  - `Registry()` requires source-visible structs/classes with method calls;
    current α stdlib public surfaces have avoided this through PRELUDE flat
    functions.
  - Dynamic `tool=add` invocation requires first-class function values plus
    runtime metadata tying function signatures to JSON schemas.
- **Rejected for M-AI.2 α.** This is a future M-AI.2.x / post-module-path /
  post-reflection milestone.

### Option 1B — Flat function JSON manifest surface (CHOSEN)

```cobrust
let schema: str = tool_schema(
    "add_i64",
    "Add two integers",
    "[{\"name\":\"a\",\"type\":\"i64\"},{\"name\":\"b\",\"type\":\"i64\"}]",
    "i64",
)
let registry: str = tool_registry_register(tool_registry_new(), schema)
let response: str = llm_complete_with_tools("What is 1 + 2?", registry)
let result: str = tool_invoke("add_i64", "{\"a\":1,\"b\":2}")
```

- Pros:
  - Same proven architecture as M-AI.0 and M-AI.1.
  - No new HIR / MIR / type-checker primitives.
  - Gives α users a real, deterministic tool manifest + registry + prompt
    augmentation path.
  - Keeps the future decorator surface honest by documenting it as deferred.
- Cons:
  - Users manually write schema JSON instead of deriving it from typed function
    signatures.
  - `tool_invoke` is closed-world, not arbitrary user function invocation.
- **Chosen.** The limitation is load-bearing and must be documented everywhere
  the public surface is mentioned.

### Option 1C — Prompt-only tools with no invoke primitive

- Pros: avoids any risk of pretending dynamic invocation exists.
- Cons: fails ADR-0048's `invoke` use-case entirely and cannot exercise JSON
  argument parsing through an executable source-level path.
- **Rejected.** M-AI.2 needs an invocation exemplar, but it must be explicitly
  closed-world.

## Decision 2 — Function set

M-AI.2 α exposes exactly five source-level flat functions:

```cobrust
# Build a canonical tool schema JSON string.
fn tool_schema(name: str, description: str, parameters_json: str, return_type: str) -> str

# Build an empty canonical registry JSON string: {"tools":[]}.
fn tool_registry_new() -> str

# Register one schema into a registry, replacing an existing same-name schema.
fn tool_registry_register(registry_json: str, schema_json: str) -> str

# Closed-world α invocation. Supports documented builtin exemplar tools only.
fn tool_invoke(tool_name: str, args_json: str) -> str

# Prompt-augment + dispatch through the existing M-AI.0 router under task "tools".
fn llm_complete_with_tools(prompt: str, registry_json: str) -> str
```

### Why exactly these five

1. `tool_schema` — substitutes for future `@tool` schema generation by making
   schema construction explicit and testable.
2. `tool_registry_new` — avoids relying on a `Registry` constructor or class
   surface.
3. `tool_registry_register` — validates schema JSON and creates a deterministic
   manifest LLMs can consume.
4. `tool_invoke` — tests the result side of a tool call without claiming
   arbitrary function reflection.
5. `llm_complete_with_tools` — ties tool manifests into the M-AI.0 LLM path.

### Why not `tool_registry_names`, `tool_call_name`, or JSON extraction helpers

They are useful, but they would widen M-AI.2 into a JSON stdlib. Until a
`cobrust.json` surface exists, M-AI.2 remains narrowly scoped to schema,
registry, exemplar invoke, and prompt augmentation.

## Decision 3 — Schema format and validation

`tool_schema(name, description, parameters_json, return_type)` returns compact
canonical JSON with this exact field order:

```json
{"name":"add_i64","description":"Add two integers","parameters":[{"name":"a","type":"i64"},{"name":"b","type":"i64"}],"returns":"i64"}
```

Validation rules:

- `name` must match `[A-Za-z_][A-Za-z0-9_]*`; otherwise return `""`.
- `description` can be any string; it is JSON-escaped by `serde_json`.
- `parameters_json` must parse as a JSON array.
- Each parameter must be an object containing non-empty string fields `name`
  and `type`; extra fields are preserved only if the implementation can keep
  deterministic output. If preserving extras complicates canonicalization,
  reject extras and document the rejection.
- `return_type` must be a non-empty string.

The returned string is the ABI: source-level Cobrust does not inspect it except
by passing it to registry helpers. Rust tests may parse it as `serde_json::Value`
for structural assertions.

## Decision 4 — Registry format and duplicate policy

`tool_registry_new()` returns:

```json
{"tools":[]}
```

`tool_registry_register(registry_json, schema_json)`:

1. Parses `registry_json` as an object with `tools: array`.
2. Parses `schema_json` as a valid M-AI.2 schema object.
3. Removes any existing schema whose `name` equals the new schema name.
4. Appends the new schema at the end.
5. Serializes compact canonical JSON.

Duplicate policy: **last schema wins**. This mirrors config override behavior
and keeps incremental registry construction deterministic.

Malformed registry or schema returns `""`. There is no partial recovery.

## Decision 5 — Invocation semantics: closed-world α dispatcher

`tool_invoke(tool_name, args_json) -> str` is **not** arbitrary user-defined
function invocation. It is a closed-world α dispatcher implemented inside
`cobrust-stdlib::tool`.

M-AI.2 ships exactly one required exemplar:

```text
tool_name = "add_i64"
args_json = {"a": 1, "b": 2}
return    = "3"
```

Rules:

- `tool_invoke("add_i64", args_json)` parses `args_json` as an object and reads
  integer fields `a` and `b`.
- Missing, non-integer, malformed, or overflowed args return `""`.
- Unknown `tool_name` returns `""`.
- The return value is a string because α Cobrust has no JSON/union return
  surface for tool results. Future typed returns require a `cobrust.json` or
  typed reflection milestone.

### Why this is acceptable and not F24

This spike does **not** claim `tool_invoke` can call decorated user functions.
Docs must say: "M-AI.2 α invocation is closed-world and exemplar-only; arbitrary
user-function tool invocation is deferred." The closed-world exemplar exists to
exercise JSON argument parsing and end-to-end `.cb` compile/run plumbing, not to
pretend reflection has shipped.

P9 STOP condition: if implementation or docs describe `tool_invoke` as invoking
arbitrary user-defined Cobrust functions, stop and correct the docs before
proceeding.

## Decision 6 — `llm_complete_with_tools` as prompt augmentation

`llm_complete_with_tools(prompt, registry_json) -> str` builds an augmented
prompt and routes through M-AI.0:

```text
<prompt>

Available tools:
<registry_json>

If a tool is needed, respond with JSON: {"tool":"<name>","args":{...}}
Otherwise answer directly.
```

Then it calls:

```rust
crate::llm::llm_dispatch_blocking("tools", &augmented)
```

This means users configure `[routing.tools]` in `cobrust.toml`, just as
M-AI.1's `llm_complete_structured` uses the fixed `structured` route.

Native provider tool-calling APIs are deferred because the router's
`CompletionRequest` does not yet carry a provider-neutral tool schema field.
Widening the router API is a separate D3/D5 milestone and would contaminate
M-AI.2 with provider semantics.

## Decision 7 — Error surface

All five source-level functions return `""` on invalid input or runtime
failure, matching M-AI.0 and M-AI.1 α convention.

This is intentionally not the final Cobrust error model. Future work should
move these to `Result<T, E>` after the stdlib exposes stable typed error
surfaces. α keeps the flat-function ABI simple and consistent.

## Decision 8 — Dependency direction

`cobrust-stdlib` may add direct dependencies on:

- `serde`
- `serde_json`

Rationale: tool schemas and registries are JSON by definition, and hand-rolled
escaping would recreate known bugs. Deterministic compact serialization is part
of the public contract, so using a real JSON library is the safer option.

No new workspace crate is introduced. `llm_complete_with_tools` reuses the
existing `llm-router` feature gate because it calls `crate::llm`.

## Explicit non-goals / deferred surface

M-AI.2 α must not implement or claim any of the following:

- `@cobrust.tool.expose(...)` executing as a schema-generation macro.
- `.schema()` on function values.
- `cobrust.tool.Registry()` class or `registry.register(...)` method syntax.
- `cobrust.tool.invoke(tool=add, args={...})` invoking arbitrary user functions.
- Dict-literal `args={"a": 1, "b": 2}` at the source surface.
- Native provider tool-call API requests in `cobrust-llm-router`.
- JSON-to-typed-Cobrust decoding beyond the closed-world `add_i64` exemplar.

These are future surfaces. The implementation may leave syntax parsing support
untouched, but it must not rely on placeholder attribute lowering as if real
method dispatch existed.

## Implementation map

### `crates/cobrust-cli/src/build.rs`

Add five PRELUDE stubs:

```cobrust
fn tool_schema(name: str, description: str, parameters_json: str, return_type: str) -> str:
    return ""

fn tool_registry_new() -> str:
    return ""

fn tool_registry_register(registry_json: str, schema_json: str) -> str:
    return ""

fn tool_invoke(tool_name: str, args_json: str) -> str:
    return ""

fn llm_complete_with_tools(prompt: str, registry_json: str) -> str:
    return ""
```

### `crates/cobrust-cli/src/build/intrinsics.rs`

Add runtime symbols:

```rust
pub const TOOL_SCHEMA_RUNTIME_SYMBOL: &str = "__cobrust_tool_schema";
pub const TOOL_REGISTRY_NEW_RUNTIME_SYMBOL: &str = "__cobrust_tool_registry_new";
pub const TOOL_REGISTRY_REGISTER_RUNTIME_SYMBOL: &str = "__cobrust_tool_registry_register";
pub const TOOL_INVOKE_RUNTIME_SYMBOL: &str = "__cobrust_tool_invoke";
pub const LLM_COMPLETE_WITH_TOOLS_RUNTIME_SYMBOL: &str = "__cobrust_llm_complete_with_tools";
```

Wire these through:

- `IntrinsicDefIds` fields
- `IntrinsicDefIds::all`
- `IntrinsicDefIds::is_empty`
- `collect_print_def_ids`
- `Kind`
- `kind_for_name`
- `kind_for_def_id`
- `rewrite_print` callsite rewriting

Use the same argument-preserving rewrite pattern as M-AI.1 prompt functions.

### `crates/cobrust-codegen/src/cranelift_backend.rs`

Add C-ABI signatures to `runtime_helper_signatures`:

```rust
out.push(("__cobrust_tool_schema", sig(call_conv, &[p, p, p, p], Some(p))));
out.push(("__cobrust_tool_registry_new", sig(call_conv, &[], Some(p))));
out.push(("__cobrust_tool_registry_register", sig(call_conv, &[p, p], Some(p))));
out.push(("__cobrust_tool_invoke", sig(call_conv, &[p, p], Some(p))));
out.push(("__cobrust_llm_complete_with_tools", sig(call_conv, &[p, p], Some(p))));
```

### `crates/cobrust-stdlib/src/lib.rs`

Add:

```rust
pub mod tool;
```

`llm_complete_with_tools` inside `tool` should be gated with the existing
`llm-router` feature if it calls `crate::llm`; pure schema/registry/invoke
helpers remain unconditional.

### `crates/cobrust-stdlib/src/tool.rs`

Implement Rust-side helpers:

```rust
pub fn tool_schema_helper(name: &str, description: &str, parameters_json: &str, return_type: &str) -> String;
pub fn tool_registry_new_helper() -> String;
pub fn tool_registry_register_helper(registry_json: &str, schema_json: &str) -> String;
pub fn tool_invoke_helper(tool_name: &str, args_json: &str) -> String;
pub fn augment_prompt_with_tools_helper(prompt: &str, registry_json: &str) -> String;

#[cfg(feature = "llm-router")]
pub fn llm_complete_with_tools_helper(prompt: &str, registry_json: &str) -> String;
```

Export C-ABI shims:

```rust
pub unsafe extern "C" fn __cobrust_tool_schema(name: *mut u8, description: *mut u8, parameters_json: *mut u8, return_type: *mut u8) -> *mut u8;
pub unsafe extern "C" fn __cobrust_tool_registry_new() -> *mut u8;
pub unsafe extern "C" fn __cobrust_tool_registry_register(registry_json: *mut u8, schema_json: *mut u8) -> *mut u8;
pub unsafe extern "C" fn __cobrust_tool_invoke(tool_name: *mut u8, args_json: *mut u8) -> *mut u8;
pub unsafe extern "C" fn __cobrust_llm_complete_with_tools(prompt: *mut u8, registry_json: *mut u8) -> *mut u8;
```

Reuse the M-AI.0/M-AI.1 `read_str_buf` and `alloc_str_buffer` pattern.

### Tests

P7-TEST writes failing tests first.

Required stdlib helper tests in `crates/cobrust-stdlib/tests/tool_corpus.rs`:

- deterministic `tool_schema("add_i64", ...)` JSON
- malformed `parameters_json` returns `""`
- schema escaping for quotes/newlines/braces in descriptions
- `tool_registry_new()` returns `{"tools":[]}`
- valid register returns `{"tools":[schema]}`
- duplicate registration uses last-schema-wins
- invalid registry returns `""`
- `tool_invoke("add_i64", "{\"a\":1,\"b\":2}") == "3"`
- unknown tool returns `""`
- malformed args return `""`
- `augment_prompt_with_tools_helper` exact prompt text

Required CLI E2E test in `crates/cobrust-cli/tests/intrinsics_tool.rs`:

- compile and run a `.cb` program that calls `tool_schema`,
  `tool_registry_new`, `tool_registry_register`, `tool_invoke`, and `print`
- expected stdout: `3\n`

Do not add tests that assert decorator, `.schema()`, or `Registry` works. If a
regression test is added for those future surfaces, it must assert they are
**not yet supported** or document why a new ADR has intentionally widened scope.

### Documentation

Update:

- `docs/agent/modules/stdlib.md` — add `std.tool` section with the five flat
  functions and the closed-world invocation caveat.
- `docs/human/en/architecture.md` — add M-AI.2 α surface, explicitly separating
  future decorator syntax from current flat functions.
- `docs/human/zh/architecture.md` — parallel Chinese update.
- `scripts/doc-coverage.sh` — extend public-surface checks for the five M-AI.2
  flat functions if this script already checks M-AI.0/M-AI.1 strings.

## ADSD Phase 2 dispatch requirements

- Difficulty: **D3**.
- Model: **Opus P9**, with **Opus P7-TEST + Opus P7-DEV pair**.
- P7-TEST writes the failing corpus before P7-DEV implements.
- P9 must inspect the test corpus before spawning DEV.
- Any implementation that uses hardcoded `add_i64` must document it as the
  closed-world exemplar, not as general reflection.
- Any attempt to implement decorator/method/reflection semantics without a
  new ADR must STOP and escalate.

## Gates

All heavy gates run on <self-hosted-runner> via Mode C. The Mac remains edit/dispatch
brain only.

Minimum completion evidence:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --all-targets --locked
cargo test --workspace --locked --no-fail-fast
bash scripts/doc-coverage.sh
```

Because ADSD Studio showed raw `cargo test | grep` can lie, P9 completion must
also report both:

- the `cargo test` process exit code
- `grep -c '^test result: FAILED' <test-log>`

A completion report that paraphrases "tests passed" without raw exit-code and
FAILED-grep evidence is not mergeable.

## Done means

- [ ] P7-TEST corpus lands before implementation work begins.
- [ ] The five PRELUDE functions compile from Cobrust source.
- [ ] CLI E2E `.cb` program prints `3\n` through `tool_invoke("add_i64", ...)`.
- [ ] Rust helper tests cover deterministic JSON, invalid JSON, escaping,
      duplicate registry policy, closed-world invoke, and prompt augmentation.
- [ ] `llm_complete_with_tools` augments prompts deterministically and dispatches
      through fixed route `tools` when `llm-router` is enabled.
- [ ] Docs in agent + zh + en distinguish α flat functions from future
      decorator/method syntax.
- [ ] Workstation gates are green with raw exit-code and FAILED-grep evidence.

## Future work

- M-AI.2.x decorator macro semantics once HIR/type/MIR can preserve and execute
  decorator metadata.
- M-AI.2.x `.schema()` method surface after real attribute/method dispatch.
- M-AI.2.x user-defined function invocation after first-class function metadata
  and typed JSON decoding exist.
- Native provider tool-call request fields in `cobrust-llm-router`.
- `cobrust.json` stdlib for typed result parsing.
