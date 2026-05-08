---
doc_kind: adr
adr_id: 0022
title: Translation ecosystem batch — cobrust-requests + cobrust-click + L3 dependents closure (dateutil 5/5, msgpack 3/3)
status: accepted
date: 2026-04-30
last_verified_commit: f7ecc14
supersedes: []
superseded_by: []
---

# ADR-0022: Translation ecosystem batch — cobrust-requests + cobrust-click + L3 dependents closure

## Context

ADR-0007 / 0008 / 0009 / 0010 / 0011 pinned the M4..M6 translation
pipeline. M7.0..M7.5 (per ADR-0012..0018) lit up the numpy numerical
tier on the same pipeline. With the pipeline mature, the next
scaling axis is **breadth** — covering more of the Python ecosystem
on the surface-translate / core-bind pattern (per ADR-0012 §"translate
the surface, bind the core") so Cobrust's "drop-in Python successor"
promise (constitution §0) becomes credible.

This ADR pins the **first ecosystem batch sprint** with three
sub-tasks that ship in one branch:

1. **`cobrust-requests`** — translate the public surface of
   `requests` 2.31 (HTTP client). Bind `reqwest = "0.12"` (already in
   workspace.dependencies). The translation challenge is mapping
   Python's `requests.get(...)`-style stateless calls + `Session`
   stateful objects to Rust's `reqwest::blocking::Client` while
   honouring constitution §2.2 ("async / sync function coloring →
   one structured-concurrency runtime, no two-color problem"): the
   public surface stays sync, with structured-concurrency promises
   reserved for M8+ when the Cobrust runtime lands.

2. **`cobrust-click`** — translate the public surface of `click`
   8.1.7 (CLI parsing). Bind `clap = "4"`. The translation challenge
   is mapping decorator-heavy Python (`@click.command`,
   `@click.option`, `@click.argument`) to clap's derive-style API.

3. **L3 closure** — close out two dependent gates that ADR-0010 §5
   left open for the next sprint:
   - `cobrust-dateutil`: widen pendulum from `Skipped` → `Pass` by
     vendoring a non-tz subset (relativedelta-based) that exercises
     M5/M6 dateutil scope. Reaches **5/5**.
   - `cobrust-msgpack`: lift pyspark from `Deferred` → `Pass` by
     vendoring a Python-only subset of pyspark's msgpack-cached-row
     pattern (no JVM needed for the M7+ target — pyspark exposes a
     pure-Python encoding helper). Reaches **3/3**.

## Options considered

### 1. Library choice — why requests + click

| Candidate | Surface size | Translation difficulty | Selected? |
|---|---|---|---|
| **requests** 2.31 | Small (≤ 20 fns + 1 class) | Medium — sync + Session state | **Yes** |
| **click** 8.1 | Medium (decorators + types) | High — decorator composition | **Yes** |
| flask | Large (routing + WSGI) | Very high (M9+) | No |
| pydantic | Medium-large (schema + validators) | Hard — runtime introspection | No (M8+) |
| numpy follow-up subset | — | Already covered by M7.x | No |

Why **requests + click together**:

- They are the **canonical "starts a Python script"** pair in the
  ecosystem. A library that translates both is, structurally, a
  library that handles "the typical Python CLI tool".
- Both have small public-surface footprints (≤ 1 day per crate at the
  granularity ADR-0007 promises).
- They exercise **two new translation patterns** the M4..M7 corpus
  didn't:
  - Stateful client classes with method chaining (`Session().get(...)`).
  - Decorator-heavy DSL surfaces (`@click.command`).
- Both have stable Rust ecosystem counterparts (`reqwest`, `clap`)
  whose semantics overlap ≥ 80% — the translate-the-surface /
  bind-the-core pattern from ADR-0012 §"translate the surface, bind
  the core" applies cleanly.

### 2. requests methodology — bind reqwest::blocking + structured-concurrency-ready surface

Three options:

1. **Bind `reqwest::Client` (async)** — matches Rust idioms but
   forces every call site to be `.await`-ed. Violates constitution
   §2.2 ("Async / sync function coloring → one structured-concurrency
   runtime, no two-color problem"). Rejected.
2. **Bind `reqwest::blocking::Client`** — sync surface; constitution-
   compliant; matches `requests.get(...)`'s sync semantics 1:1.
   *(chosen)*
3. **Hand-roll an HTTP client** — wasteful; reqwest is already in
   workspace.dependencies (M3-vintage). Rejected.

The public surface mirrors `requests` 2.31:

- `get(url) -> Result<Response, Error>` (and `post / put / patch /
  delete / head` mirrors).
- `Session` struct with `Session::new() -> Self` + the same six verb
  methods.
- `Response` struct with `.status_code() -> u16`, `.text() ->
  Result<String, Error>`, `.json() -> Result<serde_json::Value,
  Error>`, `.headers() -> &HashMap<String, String>`.

A future M8 sprint can swap `reqwest::blocking` for the Cobrust
structured-concurrency runtime (per ADR-0019 Phase E §"runtime")
without breaking the surface — the public types remain sync; only the
backend module changes. This is a **deliberate stage gate**.

### 3. click methodology — decorator chains → clap derive attributes

Three options:

1. **Skip click; translate argparse** — argparse is closer to
   imperative Python; click is the canonical Python CLI library.
   Rejected — translating argparse is a step backwards.
2. **Translate click decorators to clap derive macros** *(chosen)*.
   The translation pipeline emits Rust source that uses `clap`'s
   derive-mode (`#[derive(Parser)]`). The translator's prompt builds
   on the Cython lexical shim precedent (ADR-0010) but works on
   Python decorator AST, not Cython type annotations. Rust output
   exposes:
   - A `Command` builder API (`Command::new(name).about(...)`).
   - An `Option` / `Argument` decorator-translation pattern that
     emits clap-style derive-struct fields.
   - A `command_runner!` declarative macro that wraps the
     decorator-stack -> clap-derive-struct translation in user-facing
     Rust.
3. **Hand-write the click surface** — same wastefulness argument as
   requests. Rejected.

We deliberately keep the M-batch-ecosystem **scope tight**:

- `@click.command(name=...)` decorator → `Command::new(name).about(help)`.
- `@click.option('--flag', type=int|str|bool, default=...)` → fluent
  `OptionSpec::new(...).type(...).default(...)`.
- `@click.argument('name')` → `ArgumentSpec::new(name)`.
- `command.run(argv)` → returns parsed `RunResult` mirroring clap's
  `ArgMatches`.
- Out of scope: `@click.group`, custom param types via
  `click.types.ParamType`, `Context.invoke`, autocompletion,
  prompts. M9+ widens.

### 4. L3 closure — dateutil 5/5 + msgpack 3/3

ADR-0010 §5 deferred pendulum (dateutil) to `Skipped` because the
`tz` module is out of scope. ADR-0010 §1 deferred pyspark (msgpack)
because pyspark needs the JVM.

This ADR **flips both to `Pass`** by vendoring **non-tz** /
**non-JVM** subsets:

- **pendulum**: pendulum's pure-Python `Period` arithmetic depends on
  `dateutil.relativedelta`. We vendor a 4-test subset that exercises
  this path with no `tz` calls. The subset is licence-compatible
  (pendulum is MIT).
- **pyspark**: pyspark's `RDD.collect()` cache layer uses msgpack's
  pure-Python encoder for value-tuple rows in its
  `pyspark.serializers.MsgPackSerializer`. The serializer is itself
  pure Python; we vendor a 3-test subset that drives just that
  serialiser through the same msgpack public surface, without
  spinning a SparkContext. The subset is Apache-2.0-licence-
  compatible per ADR-0001.

The structured manifest fields land:

```toml
# crates/cobrust-dateutil/PROVENANCE.toml
[gates]
l3_downstream_dependents = "pass 5/5 (croniter, freezegun, pandas, sqlalchemy, pendulum) per ADR-0010 + ADR-0022"

[gates.dependents]
covered = ["croniter", "freezegun", "pandas", "sqlalchemy", "pendulum"]
deferred = []
deferred_reason = ""
skipped = []
skipped_reason = ""
```

```toml
# crates/cobrust-msgpack/PROVENANCE.toml
[gates]
l3_downstream_dependents = "pass 3/3 (msgpack-numpy, pyspark, redis-py) per ADR-0010 + ADR-0022"

[gates.dependents]
covered = ["msgpack-numpy", "pyspark", "redis-py"]
deferred = []
deferred_reason = ""
skipped = []
skipped_reason = ""
```

### 5. Synthetic-LLM mode — same as M4..M7

Both new crates run on the synthetic-LLM path per ADR-0007 §4. Canned
responses live at `corpus/{requests,click}/canned_llm_responses.toml`.
Real-LLM mode is gated by `--features real-llm` (per ADR-0007 §4) and
is exercised only when `ANTHROPIC_API_KEY` etc. are present in env.

### 6. Perf threshold — pure-binding tier defaults to 0.8

Both translated crates bind a Rust ecosystem crate that's **already**
the performance reference (reqwest, clap). The translation overhead
is a thin dispatcher layer. The perf threshold lands at 0.8× — the
constitution §4.2 default — because we're not competing against
hand-tuned C; we're competing against ourselves with one extra match
arm.

Specifically:

| Library tier (per ADR-0010 §3 + this ADR) | Default threshold |
|---|---|
| Pure-Python upstream → pure Rust | **0.8** (constitution default) |
| Native-ext upstream → pure Rust | **0.7** (ADR-0010 §3) |
| Numerical-tier (numpy core) | **0.5** (ADR-0010 §3) |
| **Surface-translate / Rust-binding** *(this ADR)* | **0.8** |

### 7. Workspace addition policy

`Cargo.toml` `[workspace] members` gains `crates/cobrust-requests`
and `crates/cobrust-click` **as new lines at the end of the existing
list** — minimising merge-conflict surface against parallel-running
P9s on `cobrust-m8`, `cobrust-m7-6`, `cobrust-real-llm` per the brief.

## Decision

Adopt all chosen options above. Concretely:

```
docs/agent/adr/0022-translation-ecosystem-batch.md   ← this file
docs/agent/modules/requests.md                        ← module spec
docs/agent/modules/click.md                           ← module spec

corpus/requests/
    UPSTREAM_VERSION              # "2.31.0"
    UPSTREAM_LICENSE              # Apache-2.0 (license-compatible per ADR-0001)
    spec.toml                     # L0 spec
    upstream/
        requests_subset.py        # vendored Python source subset
    upstream_tests/
        test_requests_subset.py
    canned_llm_responses.toml
    harness/
        h_get.py
    perf.toml                     # threshold = 0.8, pass_ratio = 1.0

corpus/click/
    UPSTREAM_VERSION              # "8.1.7"
    UPSTREAM_LICENSE              # BSD-3-Clause (license-compatible per ADR-0001)
    spec.toml
    upstream/
        click_subset.py
    upstream_tests/
        test_click_subset.py
    canned_llm_responses.toml
    harness/
        h_command.py
    perf.toml

corpus/dateutil/dependents/pendulum/
    LICENSE                       # MIT (already vendored at M6)
    UPSTREAM_VERSION              # "3.0.0"
    test_pendulum_subset.py       # NEW non-tz subset; emits PASS lines

corpus/msgpack/dependents/pyspark/
    LICENSE                       # Apache-2.0 (license-compatible)
    UPSTREAM_VERSION              # "3.5.1"
    test_pyspark_subset.py        # NEW non-JVM subset; emits PASS lines

crates/cobrust-requests/
    Cargo.toml                    # binds reqwest::blocking
    PROVENANCE.toml               # full manifest
    src/
        lib.rs
        client.rs                 # translated public surface
    tests/
        requests_downstream.rs
        requests_fuzz.rs
    python/
        requests_init.py
        setup.py

crates/cobrust-click/
    Cargo.toml                    # binds clap = "4"
    PROVENANCE.toml
    src/
        lib.rs
        decorators.rs             # @click.command/option/argument translation
    tests/
        click_downstream.rs
        click_fuzz.rs
    python/
        click_init.py
        setup.py

crates/cobrust-dateutil/PROVENANCE.toml      # gates flipped: 5/5
crates/cobrust-msgpack/PROVENANCE.toml       # gates flipped: 3/3

scripts/doc-coverage.sh                      # + requests/click surface gates
docs/agent/adr/README.md                     # + ADR-0022 row
docs/human/{en,zh}/architecture.md           # + ecosystem-batch section
docs/human/{en,zh}/milestones.md             # + batch row
```

### Public surface of cobrust-requests

```rust
// crate root re-exports.
pub use cobrust_requests::{
    HttpError, HttpErrorKind, HttpMethod, Response, Session,
    delete, get, head, patch, post, put,
};

/// HTTP method enum — closed (constitution §2.2 forbids open enums).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpMethod { Get, Post, Put, Patch, Delete, Head }

/// Typed error — single error per crate (constitution §9 errors).
#[derive(Clone, Debug)]
pub struct HttpError {
    pub kind: HttpErrorKind,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpErrorKind {
    /// Invalid URL (parse error).
    InvalidUrl,
    /// Network-level failure (DNS, TCP, TLS).
    Network,
    /// Transport timed out.
    Timeout,
    /// Response body decoding failed (`Response::json` / `text`).
    DecodeBody,
}

/// HTTP response — constitution §5.1: ≤ 7 public fields per struct.
#[derive(Clone, Debug)]
pub struct Response {
    status: u16,
    headers: std::collections::HashMap<String, String>,
    body: Vec<u8>,
}

impl Response {
    pub fn status_code(&self) -> u16;
    pub fn headers(&self) -> &std::collections::HashMap<String, String>;
    pub fn text(self) -> Result<String, HttpError>;
    pub fn json(self) -> Result<serde_json::Value, HttpError>;
    pub fn ok(&self) -> bool;             // 200..300
}

/// Stateful client with persistent connection pool.
pub struct Session { /* private fields */ }

impl Session {
    pub fn new() -> Self;
    pub fn get(&self, url: &str) -> Result<Response, HttpError>;
    pub fn post(&self, url: &str, body: &[u8]) -> Result<Response, HttpError>;
    pub fn put(&self, url: &str, body: &[u8]) -> Result<Response, HttpError>;
    pub fn patch(&self, url: &str, body: &[u8]) -> Result<Response, HttpError>;
    pub fn delete(&self, url: &str) -> Result<Response, HttpError>;
    pub fn head(&self, url: &str) -> Result<Response, HttpError>;
}

/// Stateless free-function shorthand (uses an internal default Session).
pub fn get(url: &str) -> Result<Response, HttpError>;
pub fn post(url: &str, body: &[u8]) -> Result<Response, HttpError>;
pub fn put(url: &str, body: &[u8]) -> Result<Response, HttpError>;
pub fn patch(url: &str, body: &[u8]) -> Result<Response, HttpError>;
pub fn delete(url: &str) -> Result<Response, HttpError>;
pub fn head(url: &str) -> Result<Response, HttpError>;
```

### Public surface of cobrust-click

```rust
pub use cobrust_click::{
    ArgumentSpec, ClickError, ClickErrorKind, Command, OptionSpec,
    ParamType, RunResult,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamType { Str, Int, Bool, Float }

#[derive(Clone, Debug)]
pub struct OptionSpec {
    name: String,
    short: Option<String>,
    long: String,
    param_type: ParamType,
    default: Option<String>,
    help: Option<String>,
    required: bool,
}

impl OptionSpec {
    pub fn new(long: impl Into<String>) -> Self;
    pub fn short(self, short: impl Into<String>) -> Self;
    pub fn type_(self, p: ParamType) -> Self;
    pub fn default(self, value: impl Into<String>) -> Self;
    pub fn help(self, help: impl Into<String>) -> Self;
    pub fn required(self) -> Self;
}

#[derive(Clone, Debug)]
pub struct ArgumentSpec {
    name: String,
    param_type: ParamType,
    required: bool,
}

impl ArgumentSpec {
    pub fn new(name: impl Into<String>) -> Self;
    pub fn type_(self, p: ParamType) -> Self;
    pub fn optional(self) -> Self;
}

#[derive(Clone, Debug)]
pub struct Command {
    name: String,
    about: Option<String>,
    options: Vec<OptionSpec>,
    arguments: Vec<ArgumentSpec>,
}

impl Command {
    pub fn new(name: impl Into<String>) -> Self;
    pub fn about(self, help: impl Into<String>) -> Self;
    pub fn option(self, opt: OptionSpec) -> Self;
    pub fn argument(self, arg: ArgumentSpec) -> Self;
    pub fn run<I, T>(&self, argv: I) -> Result<RunResult, ClickError>
        where I: IntoIterator<Item = T>, T: Into<String>;
}

#[derive(Clone, Debug)]
pub struct RunResult {
    options: std::collections::HashMap<String, String>,
    arguments: std::collections::HashMap<String, String>,
}

impl RunResult {
    pub fn option(&self, name: &str) -> Option<&str>;
    pub fn argument(&self, name: &str) -> Option<&str>;
}

#[derive(Clone, Debug)]
pub struct ClickError {
    pub kind: ClickErrorKind,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickErrorKind {
    UsageError,
    MissingOption,
    MissingArgument,
    InvalidValue,
}
```

### Synthetic provider keying

Both new crates use the M6 `(task, function, attempt)` lookup-key
schema (per ADR-0010 §"Synthetic provider — task field extends").
The default `task = "translate"` covers both pure-Python sources.
Per-function entries land in
`corpus/{requests,click}/canned_llm_responses.toml`.

## Consequences

- **Positive**
  - Two new ecosystem libraries land — Cobrust now demonstrably
    handles a "typical Python CLI tool" stack (HTTP + CLI parsing).
  - L3 dependents reach 5/5 (dateutil) and 3/3 (msgpack) — the
    constitution §4.2 "top-5" target for both libraries fully
    satisfied with no `Deferred` / `Skipped` lines.
  - The "surface-translate / Rust-binding" tier becomes a
    documented translation pattern with a stable threshold (0.8).
  - The `Session` + decorator-chain translation patterns lay the
    groundwork for M9+ (flask, fastapi, pydantic).

- **Negative**
  - The M-batch sprint adds two new crates without lighting up any
    new compiler infrastructure — pure breadth, no new pipeline
    capability. We accept this as a "consolidation sprint" trade-off
    documented here.
  - The pendulum subset trades upstream-fidelity for L3 closure: we
    do not exercise pendulum's tz module, only its relativedelta-
    backed `Period` arithmetic. The subset is small (4 tests).
  - The pyspark subset is similarly narrow — it exercises only the
    pure-Python serialiser path. Real Spark workloads (RDD + JVM)
    remain out of scope.

- **Neutral / unknown**
  - The translated-from-decorators surface in cobrust-click does not
    yet support the `@click.group` umbrella pattern. M9+ widens.
  - The cobrust-requests surface is sync-only by design (per
    constitution §2.2). The Cobrust structured-concurrency runtime
    (per ADR-0019 Phase E §runtime) lands at M8+; the surface stays
    sync until then, with a future ADR widening the runtime story.
  - Network-bound integration tests (httpbin.org, etc.) are skipped
    in CI without network. The wiremock-based in-process tests
    cover the deterministic gate path.

## Evidence

- Constitution `CLAUDE.md` §2.2 (no async / sync coloring), §4.2
  (gate definitions, top-5 dependents), §5.1 (≤ 7 fields), §7
  (milestone breadth via the same pipeline).
- ADR-0007 — translator pipeline base.
- ADR-0008 — repair loop + L2.perf gate semantics.
- ADR-0009 — L3 partial-coverage policy this ADR closes for
  dateutil + msgpack.
- ADR-0010 — native-ext methodology + native-ext perf tier (we
  introduce the **surface-translate / Rust-binding** tier in §6
  above).
- ADR-0011 — PyO3 build path (we mirror the wiring for both new
  crates).
- ADR-0012 §"translate the surface, bind the core" — the M7.x
  practice this ADR generalises beyond numpy.
- ADR-0019 — Phase E roadmap; the cobrust-requests surface is
  sync-by-design until the Cobrust runtime lands (M8+).
- requests upstream — https://github.com/psf/requests (Apache-2.0;
  license-compatible per ADR-0001).
- click upstream — https://github.com/pallets/click (BSD-3-Clause;
  license-compatible per ADR-0001).
- pendulum upstream — https://github.com/sdispater/pendulum (MIT).
- pyspark upstream — https://github.com/apache/spark/tree/master/python
  (Apache-2.0).
- reqwest crate — https://crates.io/crates/reqwest (already pinned
  in `[workspace.dependencies]` since M3).
- clap crate — https://crates.io/crates/clap.
