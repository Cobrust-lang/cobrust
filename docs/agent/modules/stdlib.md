---
doc_kind: module
module_id: mod:stdlib
crate: cobrust-stdlib
last_verified_commit: b42391f
dependencies: [mod:codegen, mod:mir, mod:hir, adr:0027]
---

# Module: stdlib

## Purpose

Cobrust's standard library — the seven binding modules from
ADR-0019 §"M11" + the runtime shim that codegen-emitted programs
link against. Constitution §1.1 dual mandate: the runtime half of
"a statically-typed language implemented in Rust".

## Status

- **M11 — delivered.** ADR-0025 binds the seven module surfaces
  (io / collections / string / math / panic / env / fmt), the
  runtime ABI (mimalloc allocator, panic handler, main shim),
  the print-intrinsic lift superseding ADR-0024 §"Hello-world
  contract", and codegen amendments materializing `Constant::Str`
  via `.rodata`.
- **M13 — delivered.** ADR-0028 adds two modules behind the
  default-on `tokio-runtime` Cargo feature: `task` (`spawn /
  JoinHandle / scope / cancel`) and `sync` (bounded MPSC
  `channel`). Constitution §2.2 "no async/sync coloring" is
  honored at the user surface — every public function in the
  M13 modules is `fn`, not `async fn`. Backed by `tokio = "1"`
  with `Sender::blocking_send` / `Receiver::blocking_recv`
  bridging the sync surface onto the async runtime singleton.
- **M-AI.0 — delivered.** ADR-0048 + spike
  `docs/agent/spike/m-ai-0-cobrust-llm-spike.md` (SHA 705f592)
  add `llm` module behind a default-on `llm-router` Cargo
  feature. Three flat-fn source-level intrinsics
  (`llm_complete` / `llm_dispatch` / `llm_stream`) lower to
  C-ABI shims wrapping `crates/cobrust-llm-router` via a lazy
  process-global tokio runtime. Synthesizes routing-table
  entries per declared `(provider, model)` per OQ-3 WRAP
  ratification (router crate frozen).
- **M-AI.1 — delivered.** ADR-0048 §M-AI.1 + spike
  `docs/agent/spike/m-ai-1-cobrust-prompt-spike.md` (α Phase 3)
  add `prompt` module unconditionally (no Cargo feature). Five
  flat-fn source-level intrinsics (`prompt_render` /
  `prompt_format_few_shot` / `prompt_format_system_user` /
  `prompt_escape_braces` / `llm_complete_structured`) lower to
  C-ABI shims wrapping pure-Rust string-formatting helpers.
  `llm_complete_structured` gated by the existing `llm-router`
  feature; the other four are always present. D2 sonnet pair per
  ADR-0048 §M-AI.1.
- **M-AI.2 — delivered.** ADR-0048 §M-AI.2 + spike
  `docs/agent/spike/m-ai-2-cobrust-tool-spike.md` (α Phase 4)
  add `tool` module unconditionally. Five flat-fn source-level
  intrinsics (`tool_schema` / `tool_registry_new` /
  `tool_registry_register` / `tool_invoke` /
  `llm_complete_with_tools`) lower to C-ABI shims wrapping
  deterministic JSON schema/registry helpers. `tool_invoke` is a
  closed-world α dispatcher with only the `add_i64` exemplar;
  arbitrary user-function invocation, decorators, `.schema()`,
  `Registry`, and native provider tool-calling APIs are deferred.

## Public surface (M11)

```rust
// crates/cobrust-stdlib/src/lib.rs

pub mod io;
pub mod collections;
pub mod string;
pub mod math;
pub mod panic;
pub mod env;
pub mod fmt;
pub mod iter;        // M12.x ADR-0027 §4
pub mod runtime;
#[cfg(feature = "llm-router")]
pub mod llm;         // M-AI.0 (α Phase 2 ADR-0048 + spike 705f592)
pub mod prompt;      // M-AI.1 (α Phase 3 ADR-0048 + spike m-ai-1) — unconditional
pub mod tool;        // M-AI.2 (α Phase 4 ADR-0048 + spike m-ai-2) — unconditional

pub use runtime::{Error, ErrorKind};
pub use collections::{Dict, List, Set};
pub use iter::{DictIter, Iterator, ListIter, RangeIter, SetIter};  // M12.x
```

### `std.iter` (M12.x — ADR-0027 §4)

The for-protocol surface. HIR `Stmt::For` lowers to MIR Calls into
`__cobrust_iter_init` / `__cobrust_iter_next` / `__cobrust_iter_drop`
which bind to one of the four closed-world types here.

```rust
pub trait Iterator {
    type Item;
    fn next(&mut self) -> Option<Self::Item>;
}

pub struct ListIter<T> { /* Vec<T>-backed */ }
pub struct DictIter<K: Eq+Hash, V> { /* HashMap<K,V>-backed */ }
pub struct SetIter<T: Eq+Hash> { /* HashSet<T>-backed */ }
pub struct RangeIter { /* arithmetic range; saturating step */ }

// C ABI (codegen targets these — see codegen module's runtime
// helper signature table):
pub unsafe extern "C" fn __cobrust_iter_init(iter_val: i64) -> *mut u8;
pub unsafe extern "C" fn __cobrust_iter_next(handle: *mut u8) -> i64;  // 0 = exhausted; non-zero = Some(v-1)
pub unsafe extern "C" fn __cobrust_iter_drop(handle: *mut u8);
```

### Heap allocator + Aggregate runtime (M12.x — ADR-0027 §1)

The codegen Aggregate / Drop lowering routes through these C-ABI
symbols. M12.x is i64-only at the storage layer; Phase F widens
to per-type element_size dispatch.

```rust
// crates/cobrust-stdlib/src/runtime.rs
pub unsafe extern "C" fn __cobrust_alloc(size: i64) -> *mut u8;
pub unsafe extern "C" fn __cobrust_dealloc(ptr: *mut u8, size: i64);

// crates/cobrust-stdlib/src/collections.rs
pub unsafe extern "C" fn __cobrust_list_new(elem_size: i64, len: i64) -> *mut u8;
pub unsafe extern "C" fn __cobrust_list_set(list: *mut u8, i: i64, v: i64);
pub unsafe extern "C" fn __cobrust_list_get(list: *mut u8, i: i64) -> i64;
pub unsafe extern "C" fn __cobrust_list_len(list: *mut u8) -> i64;
pub unsafe extern "C" fn __cobrust_list_drop(list: *mut u8);

pub unsafe extern "C" fn __cobrust_dict_new(k_size: i64, v_size: i64, len: i64) -> *mut u8;
pub unsafe extern "C" fn __cobrust_dict_set(dict: *mut u8, k: i64, v: i64);
pub unsafe extern "C" fn __cobrust_dict_get(dict: *mut u8, k: i64) -> i64;
pub unsafe extern "C" fn __cobrust_dict_len(dict: *mut u8) -> i64;
pub unsafe extern "C" fn __cobrust_dict_drop(dict: *mut u8);
// M-F.3.4 / ADR-0050d Decision 5 addendum.
pub unsafe extern "C" fn __cobrust_dict_is_empty(dict: *mut u8) -> i64;

pub unsafe extern "C" fn __cobrust_set_new(elem_size: i64, len: i64) -> *mut u8;
pub unsafe extern "C" fn __cobrust_set_insert(set: *mut u8, v: i64);
pub unsafe extern "C" fn __cobrust_set_contains(set: *mut u8, v: i64) -> i64;
pub unsafe extern "C" fn __cobrust_set_len(set: *mut u8) -> i64;
pub unsafe extern "C" fn __cobrust_set_drop(set: *mut u8);

pub unsafe extern "C" fn __cobrust_tuple_new(n: i64) -> *mut u8;
pub unsafe extern "C" fn __cobrust_tuple_set(tup: *mut u8, i: i64, v: i64);
pub unsafe extern "C" fn __cobrust_tuple_get(tup: *mut u8, i: i64) -> i64;
pub unsafe extern "C" fn __cobrust_tuple_drop(tup: *mut u8, n: i64);
```

### F-string runtime (M12.x — ADR-0027 §5)

HIR `Expr::Format` lowers to MIR `Aggregate(FormatString, ops)`,
which the codegen materializes via:

```rust
// crates/cobrust-stdlib/src/fmt.rs
pub unsafe extern "C" fn __cobrust_str_new() -> *mut u8;
pub unsafe extern "C" fn __cobrust_str_push_static(buf: *mut u8, ptr: *const u8, len: i64);
pub unsafe extern "C" fn __cobrust_fmt_int(buf: *mut u8, v: i64);
pub unsafe extern "C" fn __cobrust_fmt_float(buf: *mut u8, v: f64);
pub unsafe extern "C" fn __cobrust_fmt_bool(buf: *mut u8, v: i64);
pub unsafe extern "C" fn __cobrust_fmt_str(buf: *mut u8, ptr: *const u8, len: i64);
pub unsafe extern "C" fn __cobrust_fmt_repr(buf: *mut u8, ptr: *mut u8, type_id: i64);
pub unsafe extern "C" fn __cobrust_str_len(buf: *mut u8) -> i64;
pub unsafe extern "C" fn __cobrust_str_ptr(buf: *mut u8) -> *const u8;
pub unsafe extern "C" fn __cobrust_str_drop(buf: *mut u8);
```

### `std.io`

```rust
pub fn print(s: &str);
pub fn println(s: &str);
pub fn read_line() -> Result<String, Error>;
pub fn read_file(path: &str) -> Result<String, Error>;
pub fn write_file(path: &str, contents: &str) -> Result<(), Error>;
pub fn stdin() -> Stdin;
pub fn stdout() -> Stdout;
pub fn stderr() -> Stderr;

// ADR-0044 W2 Phase 2 — source-level stdin binding (Cobrust-source
// callers: `input(prompt)`, `input_no_prompt()`, `read_line()`).
// Returns plain `String` under the W2 scope cap (Decision 1D);
// typed `Result[str, IoError]` deferred to ADR-0044a.
pub fn input_from<R: BufRead>(prompt: &str, reader: &mut R) -> String;
pub fn read_line_from<R: BufRead>(reader: &mut R) -> String;

// C ABI (codegen targets these):
pub unsafe extern "C" fn __cobrust_print(ptr: *const u8, len: usize);
pub unsafe extern "C" fn __cobrust_println(ptr: *const u8, len: usize);
pub extern "C" fn __cobrust_println_int(v: i64);  // ADR-0030 §Decision step 5
// ADR-0044 W2 Phase 2 — heap-buffer print + stdin/argv runtime shims.
pub unsafe extern "C" fn __cobrust_println_str_buf(buf: *mut u8);
pub unsafe extern "C" fn __cobrust_input(ptr: *const u8, len: usize) -> *mut u8;
pub unsafe extern "C" fn __cobrust_input_no_prompt() -> *mut u8;
pub unsafe extern "C" fn __cobrust_read_line() -> *mut u8;
```

### `std.env` — ADR-0044 W2 Phase 2 amendment

```rust
pub fn args() -> Vec<String>;
pub fn argv_list() -> Vec<String>;          // ADR-0044 alias for source-level `argv()`
pub fn var(name: &str) -> Option<String>;

// C ABI (codegen targets this):
pub unsafe extern "C" fn __cobrust_capture_argv(argc: i32, argv: *const *const u8);
pub unsafe extern "C" fn __cobrust_argv() -> *mut u8;   // returns *mut List_Str
```

### `std.llm` (M-AI.0 — ADR-0048 + spike 705f592)

```rust
// Rust-side blocking helpers (unit-testable counterparts to the C-ABI shims):
pub fn llm_complete_blocking(provider: &str, model: &str, prompt: &str) -> String;
pub fn llm_dispatch_blocking(task: &str, prompt: &str) -> String;
pub fn llm_stream_blocking(provider: &str, model: &str, prompt: &str) -> Vec<String>;

// C ABI (codegen targets these via the cobrust-cli intrinsic-rewrite pass):
pub unsafe extern "C" fn __cobrust_llm_complete(provider: *mut u8, model: *mut u8, prompt: *mut u8) -> *mut u8;
pub unsafe extern "C" fn __cobrust_llm_dispatch(task: *mut u8, prompt: *mut u8) -> *mut u8;
pub unsafe extern "C" fn __cobrust_llm_stream(provider: *mut u8, model: *mut u8, prompt: *mut u8) -> *mut u8;  // returns *mut List_Str
```

Decision references:

- **OQ-1A flat-fn**: source-level names are `llm_complete` / `llm_dispatch` / `llm_stream`; no `cobrust.llm.*` module-path syntax at α (deferred to a follow-up spike when module-path lowering ships).
- **OQ-2B collect-all-chunks**: `llm_stream` returns `list[str]`; for-protocol iteration walks the collected chunks. True async streaming requires iter-protocol widening — out of M-AI.0 scope.
- **OQ-3 WRAP**: target-by-(provider, model) is implemented via routing-table entry synthesis at `RouterConfigBundle` init — the router crate is frozen for M-AI.0. For each declared `[providers.<p>]` × `models = [m_i, ...]`, the bundle adds two synthesized entries: `[routing.llm_complete_<p>_<m_i>]` + `[routing.llm_stream_<p>_<m_i>]`, each with `preferred = ["<p>:<m_i>"]`. The router's `try_provider` enforces `request.model = pm.model.clone()` (router.rs:462-465), so the synthesized `(provider, model)` is honored bit-for-bit.
- **Decision 7 error surface**: all three helpers return `""` / empty `Vec` on any failure (missing `cobrust.toml`, malformed config, unknown provider/model, auth failure, transport error, etc.). The ledger captures the actual `LlmError` for post-mortem.

### `std.prompt` (M-AI.1 — ADR-0048 §M-AI.1 + spike m-ai-1)

Five source-level intrinsics for prompt composition. Pure-Rust
string manipulation; no Cargo feature gate (unconditional). The
fifth function (`llm_complete_structured`) gated by `llm-router`.

```rust
// Rust-side blocking helpers (unit-testable; spike Decision 7: failures → ""):

// Variable interpolation: builds BTreeMap from even-indexed `vars`
// [k1, v1, k2, v2, ...] pairs; substitutes `{k}` in combined
// system + "\n" + user template. `{{`/`}}` → `{`/`}` escapes.
// Unknown keys remain literal. Returns `"<system>\n<user>"` on
// empty vars.
#[must_use]
pub fn prompt_render_helper(system: &str, user: &str, vars: &[String]) -> String;

// Canonical few-shot format: emits "Input: <in_i>\nOutput: <out_i>\n\n"
// for min(len(examples_in), len(examples_out)) pairs, then appends
// "Input: <current_input>\nOutput:" (no trailing newline).
// Empty examples → just the trailer. Mismatched lengths → truncate to min.
#[must_use]
pub fn prompt_format_few_shot_helper(
    examples_in: &[String],
    examples_out: &[String],
    current_input: &str,
) -> String;

// Simple system+user concatenator. Returns `"<system>\n\n<user>"`.
// Always succeeds; no interpolation.
#[must_use]
pub fn prompt_format_system_user_helper(system: &str, user: &str) -> String;

// Escape `{` → `{{` and `}` → `}}`. Symmetric pre-pass for
// prompt_render_helper when values contain literal braces.
#[must_use]
pub fn prompt_escape_braces_helper(text: &str) -> String;

// Structured-output convenience (gated by `llm-router`). Augments
// `prompt` with "Respond with valid JSON matching this schema:\n<schema_json>",
// then routes through `llm_dispatch_blocking("structured", augmented)`.
// Returns raw response text; caller parses JSON. Failure → "".
#[cfg(feature = "llm-router")]
#[must_use]
pub fn llm_complete_structured_helper(prompt: &str, schema_json: &str) -> String;

// C ABI (codegen targets these via the cobrust-cli intrinsic-rewrite pass):
pub unsafe extern "C" fn __cobrust_prompt_render(system: *mut u8, user: *mut u8, vars: *mut u8) -> *mut u8;
pub unsafe extern "C" fn __cobrust_prompt_format_few_shot(examples_in: *mut u8, examples_out: *mut u8, current_input: *mut u8) -> *mut u8;
pub unsafe extern "C" fn __cobrust_prompt_format_system_user(system: *mut u8, user: *mut u8) -> *mut u8;
pub unsafe extern "C" fn __cobrust_prompt_escape_braces(text: *mut u8) -> *mut u8;
pub unsafe extern "C" fn __cobrust_llm_complete_structured(prompt: *mut u8, schema_json: *mut u8) -> *mut u8;
```

Decision references (spike `m-ai-1-cobrust-prompt-spike.md` + ADR-0048 §M-AI.1):

- **Decision 1B flat-fn**: same α naming convention as M-AI.0 — no module-path lowering. Five PRELUDE stubs; intrinsic-rewrite redirects callsites.
- **Decision 3C even-indexed list[str]**: `vars` for `prompt_render` is a `list[str]` of `[k1, v1, k2, v2, ...]` pairs — reuses `argv()` / `llm_stream()` ABI; odd-length input drops trailing key silently.
- **Decision 4 `{key}` interpolation**: `{k}` placeholders substituted from vars map; `{{`/`}}` → `{`/`}` escape; unknown keys remain literal; single-pass (no recursive substitution).
- **Decision 5 canonical few-shot**: "Input: <in>\nOutput: <out>\n\n" loop + "Input: <current>\nOutput:" trailer; locked format for α stability.
- **Decision 6 structured-output**: `llm_complete_structured` appends a JSON-schema instruction then routes via `llm_dispatch(task="structured", ...)`. Caller parses the returned JSON string.
- **Decision 7 error surface**: all five helpers return `""` on any failure — exact mirror of M-AI.0 OQ-2 Decision 7.
- **Decision 8 zero new deps**: pure-Rust string manipulation for four fns; fifth reuses `crate::llm::llm_dispatch_blocking` — no new workspace deps.

### `std.tool` (M-AI.2 — ADR-0048 §M-AI.2 + spike m-ai-2)

Five source-level intrinsics for tool schema/registry construction and the α
closed-world invocation exemplar. JSON is canonical compact serde_json output.

```rust
// Rust-side helpers:
pub fn tool_schema_helper(
    name: &str,
    description: &str,
    parameters_json: &str,
    return_type: &str,
) -> String;
pub fn tool_registry_new_helper() -> String;
pub fn tool_registry_register_helper(registry_json: &str, schema_json: &str) -> String;
pub fn tool_invoke_helper(tool_name: &str, args_json: &str) -> String;
pub fn augment_prompt_with_tools_helper(prompt: &str, registry_json: &str) -> String;
pub fn llm_complete_with_tools_helper(prompt: &str, registry_json: &str) -> String;

// C ABI (codegen targets these via the cobrust-cli intrinsic-rewrite pass):
pub unsafe extern "C" fn __cobrust_tool_schema(name: *mut u8, description: *mut u8, parameters_json: *mut u8, return_type: *mut u8) -> *mut u8;
pub unsafe extern "C" fn __cobrust_tool_registry_new() -> *mut u8;
pub unsafe extern "C" fn __cobrust_tool_registry_register(registry_json: *mut u8, schema_json: *mut u8) -> *mut u8;
pub unsafe extern "C" fn __cobrust_tool_invoke(tool_name: *mut u8, args_json: *mut u8) -> *mut u8;
pub unsafe extern "C" fn __cobrust_llm_complete_with_tools(prompt: *mut u8, registry_json: *mut u8) -> *mut u8;
```

Decision references (spike `m-ai-2-cobrust-tool-spike.md` + ADR-0048 §M-AI.2):

- **Flat-fn naming**: `tool_schema`, `tool_registry_new`, `tool_registry_register`, `tool_invoke`, `llm_complete_with_tools`; no `cobrust.tool.*` module-path lowering at α.
- **Schema JSON**: `tool_schema` validates `name`, `return_type`, and an array of `{name,type}` parameter objects, then returns compact JSON with field order `name`, `description`, `parameters`, `returns`.
- **Registry JSON**: `tool_registry_new()` returns `{"tools":[]}`; `tool_registry_register` validates inputs, removes any existing same-name schema, appends the new schema, and serializes compact JSON. Duplicate names are last-schema-wins.
- **Closed-world invoke**: `tool_invoke` supports only `add_i64` with args `{"a":1,"b":2}` style JSON and returns the numeric result as `str`; unknown tools, malformed args, missing fields, non-integers, and overflow return `""`.
- **LLM tool calling**: `llm_complete_with_tools` prompt-augments with the registry and routes via `llm_dispatch_blocking("tools", augmented)` when `llm-router` is enabled. Projects using this flow must declare `[routing.tools]` in `cobrust.toml`; if the route or feature is absent, the helper returns `""`. Native provider tool-call API fields are deferred.
- **Deferred future surface**: `@cobrust.tool.expose`, function `.schema()`, `cobrust.tool.Registry`, `registry.register(...)`, arbitrary user-function reflection/invocation, dict-literal args, and JSON-to-typed-Cobrust decoding are not implemented in M-AI.2 α.
- **Error surface**: all five helpers return `""` on malformed JSON, invalid schema, unknown tool, router failure, or unavailable feature, matching M-AI.0/M-AI.1 α convention.

### `std.collections`

```rust
pub struct List<T> { /* Vec<T>-backed */ }
pub struct Dict<K, V> { /* HashMap<K, V>-backed; K: Eq + Hash */ }
pub struct Set<T> { /* HashSet<T>-backed; T: Eq + Hash */ }

impl<T> List<T> {
    pub fn new() -> Self;
    pub fn with_capacity(n: usize) -> Self;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;     // Constitution §2.2: no implicit truthiness.
    pub fn push(&mut self, value: T);
    pub fn pop(&mut self) -> Option<T>;
    pub fn get(&self, idx: usize) -> Result<&T, Error>;
    pub fn iter(&self) -> std::slice::Iter<'_, T>;
}
impl<T: Ord> List<T> { pub fn sort(&mut self); }
impl<T: PartialEq> List<T> { pub fn contains(&self, target: &T) -> bool; }
```

`Dict<K, V>` and `Set<T>` follow the same shape with the obvious
method differences (`insert`/`get`/`contains_key`/`remove`).

### `std.string`

```rust
pub fn len(s: &str) -> usize;
pub fn find(s: &str, pat: &str) -> Option<usize>;
pub fn replace(s: &str, from: &str, to: &str) -> String;
pub fn split(s: &str, sep: &str) -> Vec<String>;
pub fn trim(s: &str) -> &str;                            // ADR-0050e Decision 4 — renamed from `strip`
pub fn lower(s: &str) -> String;
pub fn upper(s: &str) -> String;
pub fn join(parts: &[&str], sep: &str) -> String;        // ADR-0050e M-F.3.5
pub fn contains(s: &str, needle: &str) -> bool;          // ADR-0050e M-F.3.5
pub fn starts_with(s: &str, prefix: &str) -> bool;       // ADR-0050e M-F.3.5
pub fn ends_with(s: &str, suffix: &str) -> bool;         // ADR-0050e M-F.3.5
pub fn format(template: &str, args: &[FormatArg<'_>]) -> String;

pub enum FormatArg<'a> { Str(&'a str), Int(i64), Float(f64), Bool(bool) }

// -- M-F.3.5 string stdlib C-ABI surface (ADR-0050e) ------------------
// Each shim takes Str buffer pointers (`*mut u8`) per ADR-0044 W2 Phase 3
// + ADR-0050c convention. Returns `*mut u8` (new heap StringBuffer) or
// i64 (find sentinel / bool predicates as 0/1).
//
// Ownership: shims do NOT drop their inputs — the caller's MIR drop
// schedule (ADR-0050c Phase 2) owns the input lifetime. Outputs are
// drop-eligible per the caller's binding scope.

pub unsafe extern "C" fn __cobrust_str_split(s: *mut u8, sep: *mut u8) -> *mut u8;     // -> list[str]
pub unsafe extern "C" fn __cobrust_str_join(parts: *mut u8, sep: *mut u8) -> *mut u8;  // -> str
pub unsafe extern "C" fn __cobrust_str_replace(s: *mut u8, old: *mut u8, new_: *mut u8) -> *mut u8;
pub unsafe extern "C" fn __cobrust_str_trim(s: *mut u8) -> *mut u8;
pub unsafe extern "C" fn __cobrust_str_find(s: *mut u8, needle: *mut u8) -> i64;       // -1 sentinel
pub unsafe extern "C" fn __cobrust_str_contains(s: *mut u8, needle: *mut u8) -> i64;   // 0/1
pub unsafe extern "C" fn __cobrust_str_starts_with(s: *mut u8, prefix: *mut u8) -> i64;
pub unsafe extern "C" fn __cobrust_str_ends_with(s: *mut u8, suffix: *mut u8) -> i64;
pub unsafe extern "C" fn __cobrust_str_lower(s: *mut u8) -> *mut u8;
pub unsafe extern "C" fn __cobrust_str_upper(s: *mut u8) -> *mut u8;
// __cobrust_str_clone already shipped at fmt.rs:306 (ADR-0050c Phase 3).
```

### `std.math`

```rust
pub const PI: f64 = std::f64::consts::PI;
pub const E: f64 = std::f64::consts::E;
pub fn sqrt(x: f64) -> f64;
pub fn pow(x: f64, y: f64) -> f64;
pub fn sin(x: f64) -> f64;
pub fn cos(x: f64) -> f64;
pub fn abs_f64(x: f64) -> f64;
pub fn abs_i64(x: i64) -> i64;
pub fn floor(x: f64) -> f64;
pub fn ceil(x: f64) -> f64;
pub fn round(x: f64) -> f64;
```

### `std.panic`

```rust
pub fn panic(msg: &str) -> !;
pub fn assert(cond: bool, msg: &str);

pub unsafe extern "C" fn __cobrust_panic(ptr: *const u8, len: usize) -> !;
pub unsafe extern "C" fn __cobrust_assert(cond: bool, ptr: *const u8, len: usize);
```

ADR-0024 §"Exit-code scheme" — `panic` exits with code 3
(`INTERNAL_PANIC`).

### `std.env`

```rust
pub fn args() -> Vec<String>;
pub fn var(name: &str) -> Option<String>;
```

### `std.fmt`

```rust
pub fn format_int(i: i64) -> String;
pub fn format_float(x: f64) -> String;
pub fn format_bool(b: bool) -> String;
pub fn format_str(s: &str) -> String;
```

### `std.task` (M13 — ADR-0028)

```rust
pub fn spawn<F, T>(work: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static;

pub struct JoinHandle<T> { /* tokio handle + cancel flag */ }

impl<T: Send + 'static> JoinHandle<T> {
    pub fn wait(self) -> Result<T, JoinError>;
    pub fn cancel(&self);
    pub fn is_cancelled(&self) -> bool;
}

pub fn cancel<T: Send + 'static>(handle: &JoinHandle<T>);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JoinError { Cancelled, Panicked }

pub fn scope<F, T>(body: F) -> T
where
    F: FnOnce(&Scope) -> T;

pub struct Scope { /* tracks children */ }

impl Scope {
    pub fn spawn<F, T>(&self, work: F) -> JoinHandle<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static;
}
```

Constitution §2.2 — every signature is `fn`, never `async fn`. The
M13 surface is sync at the user layer; tokio drives the async
runtime under the hood (per ADR-0028 §B.2 explicit-await; §B.1
implicit-await is the future shape).

### `std.sync` (M13 — ADR-0028)

```rust
pub fn channel<T: Send + 'static>(capacity: usize) -> (Sender<T>, Receiver<T>);

pub struct Sender<T> { /* tokio mpsc::Sender */ }
pub struct Receiver<T> { /* tokio mpsc::Receiver */ }

impl<T: Send + 'static> Sender<T> {
    pub fn send(&self, value: T) -> Result<(), SendError<T>>;
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>>;
}
impl<T> Clone for Sender<T> { /* multi-producer */ }

impl<T: Send + 'static> Receiver<T> {
    pub fn recv(&mut self) -> Option<T>;          // None = all senders dropped
    pub fn try_recv(&mut self) -> Result<T, TryRecvError>;
}

#[derive(Debug, Eq, PartialEq)]
pub struct SendError<T>(pub T);

#[derive(Debug, Eq, PartialEq)]
pub enum TrySendError<T> { Full(T), Closed(T) }

#[derive(Debug, Eq, PartialEq)]
pub enum TryRecvError { Empty, Disconnected }
```

Bounded MPSC, capacity 0 is approximated as 1 at M13 (capacity-zero
rendezvous is a Phase F follow-up; the M13 `send` blocks the OS
thread when the buffer is full anyway, so the observable contract
already matches rendezvous semantics for capacity 1).


### Cobrust source-level surface

The seven binding modules project onto Cobrust source-level imports
(M11 ships the runtime + Rust shim; the source-level Cobrust import
machinery is M12 scope per ADR-0019 §"M12 — Package format"). The
canonical paths a user will write at M12+:

- `std.io.println(s)` / `std.io.print(s)` / `std.io.read_line()` / `std.io.read_file(path)` / `std.io.write_file(path, contents)`
- `std.collections.List<T>` / `std.collections.Dict<K, V>` / `std.collections.Set<T>`
- `std.string.format(template, args)` / `std.string.split(s, sep)` / `std.string.find(s, pat)` / `std.string.replace(s, from, to)`
- `std.math.sqrt(x)` / `std.math.PI` / `std.math.E` / `std.math.sin(x)` / `std.math.pow(x, y)`
- `std.panic.panic(msg)` / `std.panic.assert(cond, msg)`
- `std.env.args()` / `std.env.var(name)`
- `std.fmt.format_int(i)` / `std.fmt.format_float(x)` / `std.fmt.format_bool(b)`
- `std.task.spawn(fn)` / `std.task.scope(closure)` / `std.task.cancel(handle)` (M13 — ADR-0028)
- `std.task.JoinHandle::wait()` / `std.task.JoinHandle::cancel()` / `std.task.JoinHandle::is_cancelled()` (M13)
- `std.sync.channel(capacity)` / `Sender::send(value)` / `Sender::try_send(value)` / `Sender::clone()` / `Receiver::recv()` / `Receiver::try_recv()` (M13)

At M11 these resolve through the `cobrust-stdlib` Rust crate; M12 will
bind the source-level `import std.X` machinery to the same Rust shim.

### `runtime`

```rust
pub enum ErrorKind { Io, Parse, Custom, OutOfBounds, KeyNotFound, Runtime }
pub struct Error { /* kind + message */ }

pub mod exit_codes {
    pub const SUCCESS: u8 = 0;
    pub const USER_ERROR: u8 = 1;
    pub const TYPE_ERROR: u8 = 2;
    pub const INTERNAL_PANIC: u8 = 3;
    pub const RUNTIME_PANIC: u8 = 4;
}

// Heap allocator (gated by feature `mimalloc-alloc`, default on).
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

// C-ABI argv capture (called by the C entry shim cobrust_main.c).
pub unsafe extern "C" fn __cobrust_capture_argv(argc: i32, argv: *const *const u8);
pub unsafe extern "C" fn _cobrust_drop_str(_place: *mut u8);

// User entry-point symbol — codegen exports the user's `fn main` Body
// as `_cobrust_user_main`. The C entry shim (cobrust_main.c) provides
// the platform `int main(int, char**)` and dispatches here.
extern "C" { pub fn _cobrust_user_main() -> i64; }
```

## Invariants

- **No implicit truthiness** — every collection has `is_empty()`;
  there is no `bool` coercion path through `List` / `Dict` / `Set`.
- **Result<T, E> is the default error path** for all fallible
  operations (constitution §2.2). Panic is reserved for "truly
  unrecoverable" via `std.panic.panic`.
- **No `dyn` in the public surface** (constitution §5.1) — every
  trait bound is a generic parameter.
- **C ABI symbols are stable** — the runtime ABI between codegen
  and `cobrust-stdlib` is closed-set + documented (this file +
  ADR-0025 §"Runtime ABI").
- **String literals are `.rodata` interned** at codegen time;
  `_cobrust_drop_str` is a no-op for `.rodata` strings (they don't
  own heap state at M11). Heap-allocated strings are M12+.
- **No `async fn` keyword in the M13 public surface** — every
  function in `task` and `sync` is `fn`. Constitution §2.2 "no
  async/sync coloring" is honored at the user-visible API.
- **The runtime singleton is process-wide** — ADR-0028 §A.
  Calling M13 task/sync APIs from inside a user-owned tokio
  runtime is forbidden (would deadlock); documented as a known
  limitation per ADR-0028 §"Consequences".

## Done means (M11)

- [x] Seven binding modules ship: io, collections, string, math,
      panic, env, fmt.
- [x] Runtime shim (mimalloc allocator + main entry +
      __cobrust_capture_argv) ships.
- [x] C-ABI symbols (__cobrust_print, __cobrust_println,
      __cobrust_println_int, __cobrust_panic, __cobrust_assert,
      _cobrust_drop_str) exported from libcobrust_stdlib.a.
      (__cobrust_println_int added by ADR-0030 M11.1 sprint.)
- [x] hello.cb regression: PASS through the M11 lift.
- [x] 10 representative example programs build + run + match
      expected stdout + exit 0 (per ADR-0025 §"Examples (binding)").
- [x] ≥ 200 stdlib unit tests + integration tests:
      262 passing (133 unit + 11 example gate +118 integration).
- [x] ADR-0025 accepted.

### Done means (M13)

- [x] `std.task.spawn / JoinHandle::wait / cancel / is_cancelled`
      shipped (ADR-0028 §C).
- [x] `std.task.scope` with drop-on-exit cancellation (ADR-0028 §D).
- [x] `std.sync.channel` bounded MPSC + Sender::send / try_send /
      clone + Receiver::recv / try_recv (ADR-0028 §C).
- [x] No `async fn` in the M13 public surface.
- [x] Differential perf gate: `task_perf_concurrency_producer_consumer_within_budget`
      passes at the amended 0.3× budget (ADR-0028 §F + finding-m13-sync-bridge-cost.md).
- [x] mimalloc + tokio TLS interaction smoke test
      (`task_perf_mimalloc_tokio_tls_interaction_smoke`) green —
      closes ADR-0025 §"Consequences" §"Neutral / unknown".
- [x] ≥ 30 well-typed + ≥ 30 ill-typed M13 tests + corpus +
      perf — 79 new tests total (35 well-typed + 32 ill-typed +
      10 corpus + 2 perf).
- [x] ADR-0028 accepted; finding-m13-sync-bridge-cost.md filed.

## Non-goals

- **Full closure / iteration-protocol lowering through MIR** —
  for-loops over `List<T>` and friends are M12 scope. M11 ships
  the stdlib API + runtime ABI; the codegen end-to-end iteration
  arrives later.
- **Heap-allocated `Str`** — M11 strings live in `.rodata`. M12+
  add the heap-`String` path with `_cobrust_drop_str` materializing.
- **Async / sync coloring** — constitution §2.2 forbids it; the
  structured-concurrency runtime is delivered at M13 (ADR-0028).
  Implicit-await (option B.1) is a future milestone post-MIR
  continuation modeling — explicit `JoinHandle::wait()` is the
  M13 surface.
- **REPL** — M14.
- **Full Unicode case-folding** in `string::lower`/`upper` —
  ASCII fast-path at M11; full case-folding is M11.x.

## M-F.3.3 — f64 math C-ABI shims (ADR-0050 §A1)

| Symbol | C ABI | Notes |
|---|---|---|
| `__cobrust_math_sqrt(f64) -> f64` | `math.rs` | `x.sqrt()` |
| `__cobrust_math_floor(f64) -> f64` | `math.rs` | `x.floor()` |
| `__cobrust_math_ceil(f64) -> f64` | `math.rs` | `x.ceil()` |
| `__cobrust_math_round(f64) -> f64` | `math.rs` | `x.round()` (half-away-from-zero) |
| `__cobrust_math_abs(f64) -> f64` | `math.rs` | `x.abs()` |
| `__cobrust_math_pow(f64, f64) -> f64` | `math.rs` | `base.powf(exp)` |
| `__cobrust_math_sin(f64) -> f64` | `math.rs` | `x.sin()` |
| `__cobrust_math_cos(f64) -> f64` | `math.rs` | `x.cos()` |
| `__cobrust_math_tan(f64) -> f64` | `math.rs` | `x.tan()` |
| `__cobrust_math_log(f64) -> f64` | `math.rs` | `x.ln()` (natural log) |
| `__cobrust_math_exp(f64) -> f64` | `math.rs` | `x.exp()` |
| `__cobrust_fmt_float_prec(buf: *mut u8, val: f64, spec_ptr: *const u8, spec_len: i64)` | `fmt.rs` | fixed / scientific / general via Python-style format spec |

`__cobrust_fmt_float_prec` spec rules:
- `.Nf` — fixed N decimal places
- `e` — scientific notation
- `g` — shortest repr (default float display)
- empty / unknown — falls back to `format_float(v)`

## Cross-references

- `mod:codegen` — emits calls into the C ABI symbols this module
  provides; ADR-0025 §"Codegen amendments" pins the contract.
- `mod:cli` — links against `libcobrust_stdlib.a` at every
  `cobrust build` invocation per ADR-0025 §"Runtime ABI".
- `mod:hir` — the print-intrinsic lift superseding ADR-0024.
- ADR-0019 §"M11" — milestone scope.
- ADR-0023 §"Drop-handler ABI" — Drop terminator materialization
  delegated to M11.
- ADR-0024 §"Hello-world contract" — M10 supersedes pinned here.
- ADR-0025 — M11 design (this milestone).
- `adr:0050` §A1 — M-F.3.3 f64 gap table.
- ADR-0028 — M13 structured-concurrency runtime.
- `adr:0050c` — M-F.3.2 Str ownership + list[str] drop schedule
  (new shims `__cobrust_str_clone` / `__cobrust_list_drop_elems` /
  `__cobrust_list_is_empty`).
- `finding:m13-sync-bridge-cost` — empirical perf finding +
  budget amendment justification (0.7× → 0.3×).

## ADR-0050c M-F.3.2 — new C-ABI shims

| Symbol | Definition | Purpose |
|---|---|---|
| `__cobrust_str_clone(buf: *mut u8) -> *mut u8` | `stdlib/fmt.rs` — allocates a fresh `StringBuffer`, copies the bytes from the source buffer, returns the new pointer. NULL input returns NULL. | Explicit Str clone path for ADR-0050c Phase 4. Used by for-loop body, index-expression, and Aggregate(List) lowering when a Str-typed operand needs a fresh owning copy. |
| `__cobrust_list_drop_elems(list: *mut u8, elem_drop_fn: extern "C" fn(*mut u8))` | `stdlib/collections.rs:548-589` — walks the list's i64 slots, casts each non-zero slot to `*mut u8`, calls `elem_drop_fn(slot)`, then `__cobrust_list_drop(list)`. NULL list is a no-op. | Per-element drop dispatch for `Ty::List(Ty::Str)`. Codegen passes `__cobrust_str_drop` as the elem_drop_fn (materialised via `func_addr`). |
| `__cobrust_list_is_empty(list: *mut u8) -> i64` | `stdlib/collections.rs:594-605` — returns 1 if `len == 0`, 0 otherwise. NULL is treated as empty. | §2.2-mandated emptiness predicate; the `if xs:` implicit-truthiness ban requires this for the `if list_is_empty(xs):` canonical pattern. |

## M-F.3.5 string stdlib (ADR-0050e)

Eleven new PRELUDE fns + 10 C-ABI shims (`__cobrust_str_clone`
already shipped with ADR-0050c). Surface listed in §"std.string"
above. Ownership: every shim takes Move-consumed Str args at the
source level; the codegen drop schedule (ADR-0050c Phase 2) owns the
input lifetime — shims do NOT drop their inputs.

| PRELUDE fn | C-ABI shim | Return | Notes |
|---|---|---|---|
| `split(s, sep)` | `__cobrust_str_split` | `*mut u8` (list[str]) | Materializes a fresh List<i64> with Str-pointer slots (mirrors `__cobrust_argv`). |
| `join(parts, sep)` | `__cobrust_str_join` | `*mut u8` (str) | Reads each slot of `parts` back via `__cobrust_list_get`. |
| `replace(s, old, new)` | `__cobrust_str_replace` | `*mut u8` (str) | Delegates to Rust `string::replace`. |
| `trim(s)` | `__cobrust_str_trim` | `*mut u8` (str) | Renamed from `strip` (Decision 4). |
| `find(s, needle)` | `__cobrust_str_find` | `i64` | `-1` sentinel per Decision 5. |
| `contains(s, needle)` | `__cobrust_str_contains` | `i64` (0/1) | bool source-side; i64 ABI. |
| `starts_with(s, prefix)` | `__cobrust_str_starts_with` | `i64` (0/1) | bool source-side. |
| `ends_with(s, suffix)` | `__cobrust_str_ends_with` | `i64` (0/1) | bool source-side. |
| `lower(s)` | `__cobrust_str_lower` | `*mut u8` (str) | Unicode-aware via Rust stdlib. |
| `upper(s)` | `__cobrust_str_upper` | `*mut u8` (str) | Unicode-aware via Rust stdlib. |
| `clone(s)` | `__cobrust_str_clone` (existing) | `*mut u8` (str) | LC-100 honest-debt mitigation; the shim ships at `fmt.rs:306`. |

E2E corpus: `crates/cobrust-cli/tests/string_stdlib_e2e.rs` (25
tests; 22 pass under `--include-ignored`. The 3 remaining
`f3str{16,17,22}` are LC-100 honest-debt mitigation tests whose
source pattern `let s2 = clone(s); let n = str_len(s); ...` does
NOT mitigate use-after-move under strict ADR-0050c Path D Option A
— Phase G closure target).

## ADR-0050d M-F.3.4 — dict surface C-ABI shims

Sub-sprint a+b lands the source-level + type-checker surface plus the
`dict_is_empty` C-ABI shim. The remaining shims (iter init/next/drop,
typed get/set per (K, V) shape, equality, keyerror abort) ship in
sub-sprint d alongside the `indexmap::IndexMap` backing swap.

| Symbol | Definition | Purpose |
|---|---|---|
| `__cobrust_dict_is_empty(dict: *mut u8) -> i64` | `stdlib/collections.rs` — returns 1 if `map.is_empty()`, 0 otherwise. NULL dict is treated as empty. M12.x backing is `HashMap<i64, i64>`; sub-sprint d swaps to indexmap without breaking the signature. | §2.2-mandated emptiness predicate; the `if d:` implicit-truthiness ban requires this for the `if dict_is_empty(d):` canonical pattern. Source binding at `intrinsics.rs::DICT_IS_EMPTY_RUNTIME_SYMBOL`. Row-polymorphic at the type-checker via `is_list_polymorphic_intrinsic_name` Dict widening. |
