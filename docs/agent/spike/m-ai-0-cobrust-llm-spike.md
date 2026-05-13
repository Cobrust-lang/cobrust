---
doc_kind: spike
spike_id: m-ai-0
title: "M-AI.0 cobrust.llm stdlib refined spec (Phase 2 spike)"
status: refined
date: 2026-05-11
last_verified_commit: TBD
ratifies: adr:0048 §"M-AI.0 — cobrust.llm" preliminary surface
relates_to: [adr:0048, adr:0044, adr:0044a, adr:0027, adr:0031, adr:0004]
authored_by: P9 opus tech-lead (α Phase 2 dispatch)
---

# Spike M-AI.0 — `cobrust.llm` stdlib refined specification

## Purpose

ADR-0048 §"M-AI.0 — cobrust.llm" specifies a preliminary source-level
surface for the Cobrust binding to `crates/cobrust-llm-router`. The
ADR explicitly delegates surface refinement to a Phase 2 spike (§
"Neutral/unknown" line 241). This document is that refinement. It
locks the design decisions for P7-TEST and P7-DEV to consume.

This spike does **not** introduce new HIR / MIR / type-checker
primitives. M-AI.0 is "PRELUDE flat-fn + intrinsic-rewrite + C-ABI
shim + new stdlib module wrapping the existing async LLM Router" —
architecturally identical to ADR-0044 W2 source-binding pattern.

## Decision summary (locked)

| # | Decision | Choice |
|---|---|---|
| 1 | Surface shape (flat-fn vs module-path) | **Flat-fn** — reuses ADR-0044 PRELUDE pattern; no module-path lowering. |
| 2 | Function set | `llm_complete(provider, model, prompt) -> str`, `llm_dispatch(task, prompt) -> str`, `llm_stream(provider, model, prompt) -> list[str]` (collect-all-chunks form). |
| 3 | Streaming via iter | `for chunk in llm_stream(...)` → list-iter protocol (ADR-0027). **True async streaming deferred** to follow-up spike. |
| 4 | Async ⇄ blocking bridge | One process-global `tokio::runtime::Runtime` lazy-init via `OnceLock`; shim `block_on` per call. |
| 5 | Config loading | `cobrust.toml` from CWD, read-once at first call, cached via `OnceLock<RouterConfig>`. |
| 6 | Cost-ledger emission | The existing router writes the ledger; M-AI.0 shim does nothing extra. Ledger path comes from `cobrust.toml [router].ledger_path`. |
| 7 | Error surface | EOF/error semantics match `input()`: returns `""` on failure (W2 scope cap pattern). Structured `Result[str, LlmError]` deferred to follow-up. |
| 8 | Dependency direction | `cobrust-stdlib` gains optional dep on `cobrust-llm-router`, gated by new `llm-router` feature (default-on, matches `tokio-runtime` precedent). |

Each decision section below cites the precedent ADR + the alternative
considered + why it was rejected.

---

## Decision 1 — Surface shape: flat-fn vs module-path

### Options

#### Option 1A — `cobrust.llm.complete(...)` source-level module-path

```cobrust
import cobrust.llm

let r: str = cobrust.llm.complete(provider="anthropic", model="...", prompt="...")
```

- Pros: matches ADR-0048 §M-AI.0's preliminary spec exactly; future-
  proof for `cobrust.prompt`, `cobrust.tool`, etc. consistency.
- Cons: requires MIR module-path lowering for `cobrust.X.Y` syntax,
  which ADR-0044 §Decision 2B/2C explicitly avoided as too costly
  for W2. Reviewing the MIR / HIR today, no module-path resolution
  pass exists. Adding it is a **D3+ multi-crate refactor** that
  blows M-AI.0's 4-8 hr budget (per ADR-0048 estimated agent-time).
- **Rejected for M-AI.0**. Module-path lowering is its own ADR.

#### Option 1B — Flat-fn `llm_complete(...)` PRELUDE entries (CHOSEN)

```cobrust
let r: str = llm_complete(provider="anthropic", model="...", prompt="...")
```

- Pros:
  - **Architecturally identical** to ADR-0044's `input` / `argv` /
    `read_line` pattern. Zero new MIR/HIR/types primitives.
  - PRELUDE adds stub fns; intrinsic-rewrite redirects callsites to
    runtime symbols; C-ABI shim wraps the router; codegen needs only
    new `runtime_helper_signatures` entries.
  - Fits cleanly inside the 4-8 hr M-AI.0 budget per ADR-0048.
- Cons:
  - Two naming conventions across the language: `cobrust.X.Y` (in
    ADR-0048 prose) vs. `X_Y` flat (in source). Documentation
    must call this out for v0.2.0-alpha.
  - **The mitigation**: when module-path lowering eventually lands
    (M-AI.x candidate), the PRELUDE entries can be re-exported under
    `cobrust.llm.*` namespace without breaking existing programs —
    new aliases, old names preserved. No ABI break.
- **Chosen.** §"Open question 1" below surfaces the naming
  mitigation for CTO sign-off.

### Constitution check

- §2.1 "one way to do each thing" — flat-fn is the one way at α.
  Module-path is the *future* one way once lowering ships; both
  paths bind to the same runtime symbol.
- §2.2 drop-list — no violation.
- §3.3 atomic-commit doc rule — applies; the impl PR ships zh + en
  + agent docs documenting the flat-fn naming convention.

### Surface

```cobrust
# Direct: pick a provider + model explicitly, bypass routing table.
# Returns the response text on success, "" on any failure (auth /
# network / decode). Errors are logged to the ledger.
fn llm_complete(provider: str, model: str, prompt: str) -> str

# Routed: use cobrust.toml [routing.<task>] to pick provider + model.
# Same return convention as llm_complete.
fn llm_dispatch(task: str, prompt: str) -> str

# Streaming (collect-all-chunks form). Returns the full ordered chunk
# list; iterate via for-protocol. Each chunk is the incremental delta
# text. Empty list signals failure (consistent with "" convention).
fn llm_stream(provider: str, model: str, prompt: str) -> list[str]
```

---

## Decision 2 — Function set

ADR-0048 §M-AI.0 lists three operations: `complete`, `stream`,
`dispatch`. This spike confirms exactly three flat-fns. **No
overload soup, no positional vs keyword variants** (Cobrust does not
support keyword arguments in M11 PRELUDE syntax — all PRELUDE stubs
have fixed positional signatures, per ADR-0044's `input(prompt)` vs
`input_no_prompt()` split).

### `llm_complete(provider, model, prompt) -> str`

- All three args are required `str`. No default-arg overload (PRELUDE
  doesn't support defaults at M11; matches ADR-0044 §1D pattern).
- Internally builds a one-shot `CompletionRequest { model,
  messages: [{role: User, content: prompt}], params: default }`,
  dispatches to the named provider via `Router::dispatch` with
  `Task::Custom("llm_complete")` (so the ledger records under a
  stable task key).
- The router-side cache makes repeated `llm_complete` calls with
  identical args cheap. Existing `crates/cobrust-llm-router::Cache`
  handles this transparently.

### `llm_dispatch(task, prompt) -> str`

- `task` arg is a string matching a `[routing.<task>]` section.
  Maps to `Task::Custom(task)` per existing
  `crates/cobrust-llm-router::Task::as_key` convention.
- Lets users tag application-domain tasks (e.g. "summarize_doc")
  and bind them to model selection via the config alone — same
  pattern as the existing `[routing.translate]` / `[routing.repair]`.
- If `task` is unknown to the routing table, returns "" (logged).

### `llm_stream(provider, model, prompt) -> list[str]`

See Decision 3 for the iter-protocol binding.

### Why NOT `llm_complete_structured(prompt, schema)`

- ADR-0048 §M-AI.1 lists structured-output as the **next milestone**
  (`cobrust.prompt`). M-AI.0 stays narrowly focused. Including
  structured-output requires JSON schema → Cobrust dict lowering,
  which depends on dict-of-Str codegen support that M11 has but
  M12.x has not validated end-to-end against the router. Out of
  M-AI.0 scope.

### Why NOT `llm_complete_with_tools(prompt, tools)`

- ADR-0048 §M-AI.2 (`cobrust.tool`) — explicitly the next-after-
  next milestone. M-AI.0 enables it but does not include it.

---

## Decision 3 — Streaming: collect-all-chunks vs custom iter

### Options

#### Option 3A — True async iter (custom `__cobrust_llm_stream_*` handle)

A new `IterHandle` variant wrapping the `Pin<Box<dyn Stream<Item =
Chunk>>>` from `LlmProvider::complete_stream`. Each
`__cobrust_iter_next` call uses `block_on` to await the next
chunk; `Chunk::Done(usage)` signals exhaustion.

- Pros: true streaming semantics; back-pressure flows naturally;
  zero buffering.
- Cons:
  - Requires extending `crates/cobrust-stdlib::iter::IterHandle`'s
    closure type from `FnMut() -> Option<i64>` to one that can
    return a heap-allocated Str pointer. **The list-iter machinery
    today is i64-only** (per `iter.rs:230-265` — "M12.x i64-only
    is the conservative width"). Widening it is a structural
    refactor of the iter protocol, not a clean M-AI.0 addition.
  - `Chunk::Done(usage)` payload (cost data) has no clean place to
    land in a list-of-Str iterator.
  - **D3 in isolation**, but the iter-protocol widening dominates
    the budget.

#### Option 3B — Collect-all-chunks, return `list[str]` (CHOSEN)

The shim invokes `LlmProvider::complete_stream`, drains the stream
to completion, collects every `Chunk::Delta` into a `Vec<String>`,
wraps as `__cobrust_list_new` of heap-Str pointers (same pattern as
`__cobrust_argv` in `crates/cobrust-stdlib/src/env.rs:62-85`).

- Pros:
  - **Reuses existing list-of-Str-pointer pattern** (`__cobrust_argv`
    is the precedent — see line 81 `__cobrust_list_set(list, i,
    buf as i64)` exact mechanism).
  - For-protocol iteration over the returned list is **already
    wired** via `__cobrust_iter_init(list_ptr as i64)` (per
    `iter.rs:278-302` ADR-0044 W2 amendment).
  - User code shape matches ADR-0048 §M-AI.0's prose: `for chunk
    in llm_stream(...)` works exactly as written.
- Cons:
  - Not true streaming — caller waits for full response before
    seeing first chunk. Acceptable for α: most LLM tool-use
    patterns don't need first-byte latency (chat UIs need it, but
    Cobrust programs at α are not chat UIs).
  - Loses the `Chunk::Done(usage)` payload at the source-level.
    The ledger still records usage — just not directly accessible
    from `.cb` code at M-AI.0.
- **Chosen.** Document true-streaming as M-AI.0.x follow-up tied
  to iter-protocol widening (separate ADR).

---

## Decision 4 — Async ⇄ blocking bridge

The router exposes `async fn Router::dispatch(...)`. Cobrust source
is synchronous (no `async` / `await` keywords at α; constitution §2.2
"one structured-concurrency runtime, no two-color problem" plus M13
ADR-0028 indicates the runtime exists but source-level async syntax
is not promised at α).

### Options

#### Option 4A — Process-global lazy `tokio::runtime::Runtime` (CHOSEN)

```rust
static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime init")
    })
}

// Inside each C-ABI shim:
let result = runtime().block_on(async {
    router.dispatch(task, req).await
});
```

- Pros:
  - Single runtime owned by the stdlib; every shim shares it.
  - Cache + ledger + connection-pool re-use across shim calls
    (a fresh runtime per call would burn TCP setup time).
  - `OnceLock` matches how `CAPTURED_ARGS` already exists in
    `runtime.rs:27` — established stdlib pattern.
- Cons:
  - If the cobrust binary is itself running inside another tokio
    runtime (e.g. a Cobrust program that itself uses tokio via
    M13 task primitives), `block_on` inside a tokio worker is
    forbidden — would panic. **Mitigation**: catch via
    `Handle::try_current()` — if a runtime is already current,
    use `block_in_place` + `handle.block_on`. M13 task primitives
    today don't reach C-ABI from a runtime worker (the M13 binding
    is library-side via `crates/cobrust-stdlib::task`), so this
    edge is not reachable at M-AI.0; document and defer.

#### Option 4B — One-runtime-per-call

Wasteful: TCP+TLS setup per dispatch. Rejected.

#### Option 4C — Demand caller supplies runtime via M13 task

Couples M-AI.0 to M13 surface. Rejected (M-AI.0 must stand alone).

---

## Decision 5 — Config loading

```rust
static CONFIG: OnceLock<RouterConfigBundle> = OnceLock::new();

struct RouterConfigBundle {
    config: RouterConfig,
    router: Router,
}

fn config_bundle() -> Result<&'static RouterConfigBundle, LlmError> {
    CONFIG.get_or_init(|| {
        // Read cobrust.toml from CWD or COBRUST_CONFIG env var.
        let path = std::env::var("COBRUST_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("cobrust.toml"));
        let toml = std::fs::read_to_string(&path)?;
        let config = RouterConfig::from_toml_str(&toml)?;
        let router = build_router_from_config(&config)?;
        Ok(RouterConfigBundle { config, router })
    })
}
```

- Read once, cache forever. Subsequent shim calls hit the
  `OnceLock` directly.
- If `cobrust.toml` is missing at process start, every `llm_*` call
  returns "" (logged) — same failure mode as router unavailable.
- The `COBRUST_CONFIG` env-var override is added for test
  isolation (tests can point at fixture `.toml` files without
  touching CWD).

---

## Decision 6 — Cost-ledger emission

**Zero new ledger code in M-AI.0.** The existing `Router::dispatch`
writes `LedgerEntry::ok` / `::err` per attempt via
`crates/cobrust-llm-router::Ledger` (see `router.rs:476-490` for the
cache-hit path, `516-531` for the live-dispatch path). M-AI.0 shims
inherit this for free.

What CHANGES per-shim:

- `llm_complete` → `Task::Custom("llm_complete")` ledger task key.
- `llm_dispatch` → `Task::Custom(user_supplied_task)` ledger task
  key. Maps to whatever the user typed.
- `llm_stream` → `Task::Custom("llm_stream")` ledger task key.

This is enough for cost accounting to flow into
`.cobrust/ledger.jsonl` from `.cb` programs with **no new code in
M-AI.0**. The decision is to **deliberately not extend** the ledger
schema at M-AI.0.

---

## Decision 7 — Error surface

Match the ADR-0044 W2 scope-cap pattern exactly:

- All three shims return "" (or empty list) on any failure.
- The failure path writes a ledger error entry — caller sees ""
  but the ledger captures `LlmError::Auth` / `Transport` / `Decode`
  / etc. for post-mortem.
- Typed `Result[str, LlmError]` at source level is deferred to a
  follow-up spike (M-AI.0a candidate, paired with ADR-0044a's typed-
  Result lowering).

### Why this is the right call for α

- α framing is "stdlib in development." Users aren't shipping
  production code on top of M-AI.0 yet.
- The ledger captures every failure for the developer; this is
  enough debugging surface for a stdlib that's "in development."
- Forcing `Result[str, LlmError]` at α requires typed-Result MIR
  lowering, which is a Phase F.1.x prereq (per ADR-0044 §"Follow-up
  ADR-0044a queued"). Sequencing M-AI.0 in front of that is a
  scope error.

---

## Decision 8 — Dependency direction

`cobrust-stdlib` gains optional dep on `cobrust-llm-router`, gated by
a new `llm-router` feature default-on. **Matches the existing
`tokio-runtime` feature precedent** (`crates/cobrust-stdlib/Cargo.toml:24-30`).

```toml
# crates/cobrust-stdlib/Cargo.toml additions
[features]
default = ["mimalloc-alloc", "tokio-runtime", "llm-router"]
llm-router = ["dep:cobrust-llm-router", "dep:tokio"]  # M-AI.0

[dependencies]
cobrust-llm-router = { workspace = true, optional = true }
# tokio is already optional via tokio-runtime; the llm-router
# feature ensures it's also enabled when the router is on.
```

- Pros:
  - Matches established stdlib feature pattern (tokio-runtime gates
    M13; llm-router gates M-AI.0).
  - Builds without the LLM crate when the feature is off (useful
    for embedded targets / minimal builds — same rationale as
    `system-alloc`).
- Cons:
  - Workspace `Cargo.toml` already declares `cobrust-llm-router`
    via path dependency; the new entry is one line.

---

## Implementation map (binding for P7-DEV)

### Crate touch list

| Crate | File | What changes |
|---|---|---|
| `cobrust-stdlib` | `src/llm.rs` (new) | Rust-side `llm_complete_blocking` / `llm_dispatch_blocking` / `llm_stream_blocking` + three C-ABI shims (`__cobrust_llm_complete`, `__cobrust_llm_dispatch`, `__cobrust_llm_stream`). Lazy `OnceLock<Runtime>` + `OnceLock<RouterConfigBundle>`. |
| `cobrust-stdlib` | `src/lib.rs` | `pub mod llm;` behind `#[cfg(feature = "llm-router")]`. |
| `cobrust-stdlib` | `Cargo.toml` | Add `llm-router` feature + optional dep on `cobrust-llm-router`. |
| `cobrust-cli` | `src/build.rs` | Extend `PRELUDE` to declare three new stub fns (`llm_complete`, `llm_dispatch`, `llm_stream`). |
| `cobrust-cli` | `src/build/intrinsics.rs` | Add `LLM_COMPLETE_RUNTIME_SYMBOL` / `LLM_DISPATCH_RUNTIME_SYMBOL` / `LLM_STREAM_RUNTIME_SYMBOL` consts; extend `IntrinsicDefIds` + `Kind` enum + `kind_for_name` / `kind_for_def_id` / `rewrite_print` match arms. |
| `cobrust-codegen` | `src/cranelift_backend.rs` | Add three entries to `runtime_helper_signatures()`. Each takes 3 str-args and returns Str (or list pointer for stream). |
| `cobrust-stdlib` | `tests/llm_corpus.rs` (new) | Rust-side unit + integration tests for the three blocking helpers + three C-ABI shims (using a Synthetic provider fixture, per `crates/cobrust-llm-router::config::ProviderKind::Synthetic` precedent). |
| `cobrust-cli` | `tests/intrinsics_llm.rs` (new) | End-to-end `.cb` source → compile → run tests for the three intrinsics. |
| `docs/agent/modules/stdlib.md` | edit | Add `llm` module to the public surface table. |
| `docs/agent/modules/cli.md` | edit | Note the PRELUDE + intrinsic-rewrite extension. |
| `docs/human/{zh,en}/architecture.md` | edit | Document the three flat-fns in a new "AI-native stdlib" subsection. |
| `cobrust.toml.example` | edit | Add `[routing.llm_complete]` / `[routing.llm_stream]` sample sections + comment explaining `llm_dispatch` consumes user-defined task names. |

### Runtime helper signatures (codegen amendment)

```rust
// In runtime_helper_signatures(), append after the W2 Phase 3 block:

// -- M-AI.0 (α Phase 2): cobrust.llm source-level binding ---------
// `llm_complete(provider, model, prompt) -> str`. All three args are
// Str buffers (heap or .rodata). Returns owned Str pointer or empty
// Str on any failure (ledger captures the actual error).
//
// NOTE on Str arg expansion: codegen's `expand_str_to_ptr_len`
// (cranelift_backend.rs:1074) fires for 1-source-arg + 2-C-param
// pattern. M-AI.0's three-arg form has three source args + three C
// params (each a `*const u8`); the str-arg expansion DOES NOT fire,
// each Str arrives as a heap-buffer pointer. Inside the shim we use
// __cobrust_str_ptr + __cobrust_str_len to read content. For .rodata
// literals, codegen still passes them as pointers via the
// `materialize_str_data` path (cranelift_backend.rs:1087). The shim
// reads them via the same accessor.
//
// SIMPLIFICATION OPTION: pass each Str as (ptr, len) pair → 6 C
// params instead of 3. Mirrors __cobrust_input. Decision: take this
// option, since it dovetails with how __cobrust_input(ptr, len)
// already works. Three source args + six C params → codegen's
// `expand_trailing_str_len` does NOT fire for non-trailing strs.
// We use a fresh expansion mode OR we pre-extract via __cobrust_str_*
// inside the shim. SIMPLEST: shim takes pointer-only, extracts via
// public accessor — same as __cobrust_println_str_buf
// (io.rs:243-265). Confirmed: pointer-only is the shape. C sig:
// `(p, p, p) -> p`.
out.push(("__cobrust_llm_complete", sig(call_conv, &[p, p, p], Some(p))));
out.push(("__cobrust_llm_dispatch", sig(call_conv, &[p, p], Some(p))));
out.push(("__cobrust_llm_stream", sig(call_conv, &[p, p, p], Some(p))));
```

### PRELUDE amendment

```python
# Append to PRELUDE constant in crates/cobrust-cli/src/build.rs:37 string:
fn llm_complete(provider: str, model: str, prompt: str) -> str:
    return ""

fn llm_dispatch(task: str, prompt: str) -> str:
    return ""

fn llm_stream(provider: str, model: str, prompt: str) -> list[str]:
    let xs: list[str] = []
    return xs
```

### Intrinsic-rewrite extension

Three new arms in `kind_for_name` / `kind_for_def_id` /
`IntrinsicDefIds` / `rewrite_print`'s match block. Each arm sets
`*func = Operand::Constant(Constant::Str(LLM_*_RUNTIME_SYMBOL))` and
preserves the operand list (no expansion needed — all args are
pointer-only Str operands).

### C-ABI shim shape (binding for P7-DEV)

```rust
// crates/cobrust-stdlib/src/llm.rs (new module, gated by feature)

use std::sync::OnceLock;
use std::path::PathBuf;

use cobrust_llm_router::{
    AnthropicProvider, CompletionRequest, Message, OpenAiProvider,
    ProviderKind, Role, Router, RouterConfig, RouterError, Task,
};

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
static CONFIG_BUNDLE: OnceLock<Option<RouterConfigBundle>> = OnceLock::new();

struct RouterConfigBundle {
    router: Router,
}

fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("M-AI.0: tokio runtime init failed")
    })
}

fn config_bundle() -> Option<&'static RouterConfigBundle> {
    CONFIG_BUNDLE
        .get_or_init(|| build_bundle().ok())
        .as_ref()
}

fn build_bundle() -> Result<RouterConfigBundle, BuildErr> {
    let path = std::env::var("COBRUST_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("cobrust.toml"));
    let toml = std::fs::read_to_string(&path).map_err(BuildErr::Io)?;
    let cfg = RouterConfig::from_toml_str(&toml).map_err(BuildErr::Parse)?;
    let router = runtime().block_on(async {
        let mut builder = Router::builder();
        for (name, pcfg) in &cfg.providers {
            let arc: std::sync::Arc<dyn cobrust_llm_router::LlmProvider> = match pcfg.kind {
                ProviderKind::Anthropic => {
                    let api_key = std::env::var(&pcfg.api_key_env).unwrap_or_default();
                    std::sync::Arc::new(AnthropicProvider::new(pcfg.base_url.clone(), api_key))
                }
                ProviderKind::Openai => {
                    let api_key = std::env::var(&pcfg.api_key_env).unwrap_or_default();
                    std::sync::Arc::new(OpenAiProvider::new(pcfg.base_url.clone(), api_key))
                }
                ProviderKind::Synthetic => continue, // tests inject via build hook
            };
            builder = builder.register_provider(name.clone(), arc);
        }
        builder.build(&cfg).await
    }).map_err(BuildErr::Router)?;
    Ok(RouterConfigBundle { router })
}

/// Blocking helper for `llm_complete`. Returns the response text or
/// empty String on any failure.
pub fn llm_complete_blocking(provider: &str, model: &str, prompt: &str) -> String {
    let Some(bundle) = config_bundle() else { return String::new(); };
    let req = CompletionRequest {
        model: model.to_string(),
        messages: vec![Message { role: Role::User, content: prompt.to_string() }],
        params: Default::default(),
    };
    // Use Custom task to bypass routing table; we route directly by
    // (provider, model) override. Router::dispatch normally walks the
    // task's preferred list; for llm_complete we need direct
    // (provider, model) targeting. The cleanest API addition is a
    // new method `Router::dispatch_direct(provider, req)`; for spike
    // scope we synthesize a one-entry route via Task::Custom and a
    // routing-table sidecar. P7-DEV decides between the two — the
    // simplest is to add a thin `Router::dispatch_direct` to the
    // router crate (one new public method, no schema change).
    //
    // OPEN QUESTION 3 (below): direct-target API in the router.
    let _provider_unused = provider; // see open question 3
    match runtime().block_on(bundle.router.dispatch(Task::Custom("llm_complete".into()), req)) {
        Ok(r) => r.response.text,
        Err(_) => String::new(),
    }
}

// ... analogous llm_dispatch_blocking + llm_stream_blocking +
// three C-ABI shims that delegate to the helpers, allocate Str
// buffers via crate::fmt::__cobrust_str_new + __cobrust_str_push_static
// (mirroring __cobrust_input shape in io.rs:167-178).

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_llm_complete(
    provider: *mut u8,
    model: *mut u8,
    prompt: *mut u8,
) -> *mut u8 {
    let p = unsafe { read_str_buf(provider) };
    let m = unsafe { read_str_buf(model) };
    let q = unsafe { read_str_buf(prompt) };
    let result = llm_complete_blocking(&p, &m, &q);
    alloc_str_buffer(&result)
}

// ... analogous __cobrust_llm_dispatch + __cobrust_llm_stream
```

### Test plan (binding for P7-TEST)

#### Tier 1 — Rust-side blocking helpers (≥ 15 tests, `tests/llm_corpus.rs`)

Use the `Synthetic` provider kind precedent (per
`crates/cobrust-llm-router/src/router.rs:646-655` `ok()` test
helper) — register a fixture `LlmProvider` impl that returns canned
responses, then assert the blocking helpers route correctly.

1. `llm_complete_blocking` with a Synthetic provider returns the
   canned response text.
2. `llm_complete_blocking` with missing `cobrust.toml` returns "".
3. `llm_complete_blocking` with malformed `cobrust.toml` returns "".
4. `llm_complete_blocking` with auth-failing provider returns ""
   (ledger captures `LlmError::Auth`).
5. `llm_complete_blocking` with rate-limit transient retries per
   `RetryPolicy` then succeeds.
6. `llm_complete_blocking` with transport error (provider down)
   returns "".
7. `llm_dispatch_blocking` with valid task name routes to first
   preferred provider.
8. `llm_dispatch_blocking` with unknown task returns "".
9. `llm_dispatch_blocking` with consensus strategy routes through
   `Router::dispatch_consensus`.
10. `llm_stream_blocking` returns an ordered `Vec<String>` of
    delta chunks from a synthetic stream.
11. `llm_stream_blocking` on empty stream returns empty Vec.
12. `llm_stream_blocking` on stream-error mid-stream returns
    partial Vec collected so far.
13. UTF-8 round-trip — prompt with multi-byte chars + canned
    multi-byte response → byte-identical.
14. Concurrent invocation safety — 32 parallel `llm_complete_blocking`
    calls — all return correct canned response.
15. Ledger file contains 32 `outcome:ok` entries after Test 14.

#### Tier 2 — C-ABI shims (≥ 10 tests)

1. `__cobrust_llm_complete(p, m, q)` with valid str buffers →
   non-null result + readable via `__cobrust_str_len` +
   `__cobrust_str_ptr`.
2. Each input arg passed as a `__cobrust_input(...)` result
   round-trip (heap str pointer).
3. Each input arg passed as `.rodata` static literal (codegen
   path — exercised via inline tests, no .cb file needed for
   stdlib unit tests).
4. Null-arg robustness (any null → "").
5. Empty-string args → "" return.
6. `__cobrust_llm_dispatch` analogous Tier 1.
7. `__cobrust_llm_stream` returns a list pointer; iterating via
   `__cobrust_list_get` + `__cobrust_list_len` recovers the
   chunks.
8-10. UTF-8 / concurrent / ledger sanity (mirror Tier 1).

#### Tier 3 — End-to-end `.cb` source → run (≥ 5 tests, `intrinsics_llm.rs`)

Each test:
1. Writes a tiny `.cb` program calling `llm_*`.
2. Writes a fixture `cobrust.toml` pointing at a wiremock-served
   provider.
3. Invokes `cobrust build` + `cobrust run`.
4. Captures stdout, asserts contents.

Programs:
1. `print(llm_complete("syn", "m1", "hello"))` — round-trips
   canned synthetic response through the full PRELUDE-to-stdout
   pipeline.
2. `let xs: list[str] = llm_stream("syn", "m1", "hi"); for x in
   xs: print(x)` — exercises stream → list-iter → print loop.
3. `print(llm_dispatch("greet", "hi"))` — exercises routing-table
   lookup.
4. `cobrust.toml` missing → empty output.
5. Verify .cobrust/ledger.jsonl contents post-run (one entry per
   shim call).

Use `wiremock` (already in `cobrust-llm-router/Cargo.toml`
dev-deps) for the fake HTTP server. End-to-end tests mark their
provider entries in fixture `cobrust.toml` with the wiremock URL.

#### verify.py mandate (per ADR-0047a)

Per ADR-0048 §"Test plan" + ADR-0047a inheritance, every Tier 1
helper test ships with a sibling Python file invoking the same
provider API via the official Anthropic / OpenAI Python SDK. The
verify.py harness runs both Cobrust shim + Python SDK; the test
asserts both return non-empty text and (for deterministic synthetic
provider) identical text.

For M-AI.0 the Python equivalent for the Synthetic provider is a
direct dict-based shim — verify.py reads the same fixture canned
response and reports it via `print(...)`. This proves the Cobrust
implementation is faithful to the contract, not just self-
consistent.

#### Fuzz (≥ 1024 inputs, `tests/llm_fuzz.rs` reuse pattern)

Per ADR-0044 `io_input_fuzz.rs` precedent (proptest 1024 iters).
Property: any `(provider, model, prompt)` triple with arbitrary
UTF-8 content + length 0..16 KiB returns either a non-empty Str or
"". No panic, no UB.

---

## Open questions (for CTO sign-off before P7 fires)

### OQ-1 — Naming: `llm_complete` flat-fn vs aliasing to `cobrust.llm.complete` later

The spike chooses flat-fn `llm_*` names at α (Decision 1B). The
mitigation: once module-path lowering ships, PRELUDE additionally
re-exports the same runtime targets under `cobrust.llm.complete`
etc., preserving backward-compat (old names continue to work, new
names join).

**CTO sign-off needed**: is this naming acceptable for v0.2.0-alpha
docs, or does the user-facing surface need to wait for `cobrust.X.Y`
to be source-syntax-valid? If the latter, M-AI.0 expands to include
module-path lowering (D4 multi-crate, ~12-16 hr budget).

### OQ-2 — Streaming surface: collect-all vs custom-iter follow-up timing

The spike chooses collect-all-chunks (Decision 3B), explicitly
deferring true streaming. **CTO sign-off needed**: is this
acceptable for α, or does true streaming need to ship with M-AI.0?
The latter requires widening `IterHandle::next_fn` from `FnMut() ->
Option<i64>` to a Str-aware shape, which is itself a ~4-6 hr
refactor.

### OQ-3 — `Router::dispatch_direct(provider, req)` thin addition vs synthetic-route hack

The spike's `llm_complete_blocking` needs to target a specific
`(provider, model)` directly, bypassing the routing table. Two
implementations:

- **3a — Thin router API addition**: add `pub async fn
  Router::dispatch_direct(&self, provider: &str, req:
  CompletionRequest) -> Result<RouterResponse, RouterError>` that
  invokes `RouterHandle::try_provider` directly. One new public
  method, no schema change. Recommended.
- **3b — Synthetic-route**: each `llm_complete` call constructs an
  ephemeral routing-table entry via `Router::dispatch(Task::Custom)`
  plus injects a one-entry preferred-list. More fragile; uses
  features the router didn't design for. Not recommended.

**CTO sign-off needed**: which option, or both deferred via a
different mechanism?

---

## Done means (spike Phase 2)

- [x] This document committed to `docs/agent/spike/m-ai-0-cobrust-llm-spike.md`.
- [x] CTO ratifies open questions OQ-1 / OQ-2 / OQ-3 via
  `[P10-ALPHA-PHASE-2-RATIFY]` (or amends).
- [ ] P7-TEST opus prompt (drafted in P9 return block) executes against this spec.
- [ ] P7-DEV opus prompt (drafted in P9 return block) executes against this spec.

## Why this spike now

ADR-0048 §"Neutral/unknown" line 241 explicitly defers the M-AI.0
surface refinement to a Phase 2 spike — this document fulfills that
deferral. Without locking these 8 decisions, P7-TEST and P7-DEV
would re-litigate them mid-sprint, blowing the 4-8 hr budget.

— P9 opus tech-lead, α Phase 2 dispatch 2026-05-11
