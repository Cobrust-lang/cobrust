---
doc_kind: module
module_id: mod:strike
crate: cobrust-strike
last_verified_commit: f7ecc14
dependencies: [mod:translator]
---

# Module: strike

## Purpose

Cobrust translation of `requests` 2.31.0 — the M-batch ecosystem
deliverable per ADR-0022. Surface-translates Python's `requests`
HTTP client onto Rust's `reqwest::blocking::Client`, keeping the
public API sync (constitution §2.2: "no async / sync coloring").
Demonstrates that Cobrust's translation pipeline handles **stateful
client classes** (`Session`) end-to-end on top of the M4..M7
translator infrastructure.

## Status

- **M-batch — delivered.** All 13 functions translated via the
  synthetic-LLM pipeline (6 free verbs + `Session::new` + 6 Session
  methods). Backend bound to `reqwest = "0.12"` (already pinned in
  `[workspace.dependencies]` since M3); we add the `blocking` +
  `json` features at the crate level. The L3 differential gate runs
  against an in-process HTTP wiremock spun on a random localhost
  port; an optional smoke against `https://httpbin.org/get` runs
  when network is reachable, skipping cleanly otherwise.

## Public surface (M-batch)

```rust
pub fn get(url: &str) -> Result<Response, HttpError>;
pub fn post(url: &str, body: &[u8]) -> Result<Response, HttpError>;
pub fn put(url: &str, body: &[u8]) -> Result<Response, HttpError>;
pub fn patch(url: &str, body: &[u8]) -> Result<Response, HttpError>;
pub fn delete(url: &str) -> Result<Response, HttpError>;
pub fn head(url: &str) -> Result<Response, HttpError>;

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

pub struct Response { /* status / headers / body — all private */ }

impl Response {
    pub fn status_code(&self) -> u16;
    pub fn ok(&self) -> bool;
    pub fn headers(&self) -> &HashMap<String, String>;
    pub fn text(self) -> Result<String, HttpError>;
    pub fn json(self) -> Result<serde_json::Value, HttpError>;
    pub fn bytes(self) -> Vec<u8>;
    pub fn from_parts(status: u16, headers: HashMap<String, String>, body: Vec<u8>) -> Self;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpMethod { Get, Post, Put, Patch, Delete, Head }

#[derive(Clone, Debug)]
pub struct HttpError { pub kind: HttpErrorKind, pub message: String }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpErrorKind { InvalidUrl, Network, Timeout, DecodeBody }
```

## Scope window (M-batch)

In scope:

- The six standard HTTP verbs (`GET / POST / PUT / PATCH / DELETE / HEAD`).
- Persistent connection pool via `Session` (reqwest's
  `blocking::Client` is `Send + Sync` and pools by default).
- Body decoding: `Response::text` (utf-8) and `Response::json`
  (serde_json::Value).
- Sync surface (no async / no `.await`) per constitution §2.2.

Out of scope (M9+):

- Cookie jar / `Session.cookies` API.
- Auth shims (`HTTPBasicAuth`, OAuth integrations).
- Streaming bodies (`stream=True` + iterator API).
- Custom transport adapters.
- `Response.raise_for_status()` exception escalation.

## Invariants

- **No silent translations.** Every emitted file carries a
  provenance header; every translated function carries a
  per-function provenance line.
- **Sync surface.** Public API never exposes `Future` / `async fn`
  per constitution §2.2.
- **Closed error taxonomy.** Every failure routes to one of four
  `HttpErrorKind` variants; opaque `Box<dyn Error>` is forbidden.
- **OK is 2xx.** `Response::ok()` returns `(200..300).contains(status)`.

## Gates (M-batch — none optional)

| Stage | Gate | Pass criteria | Status |
|---|---|---|---|
| L0 | spec produced | `corpus/requests/spec.toml` + harness committed | ✅ |
| L1 | code emitted | every file has provenance header + per-fn task tag | ✅ |
| L2.build | `cargo build --release` | zero warnings | ✅ |
| L2.behavior | wiremock + URL-parser fuzz | ≥ 1000 panic-free inputs across 3 seeds | ✅ |
| L2.perf | binding-overhead bench | surface-translate / Rust-binding tier 0.8× per ADR-0022 §6 | ✅ |
| L3.pyo3 | PyO3-shaped wrapper | `--features pyo3` compiles per ADR-0011 | ✅ |
| L3.dependents | (deferred to M9 per ADR-0022 §"Negative consequences") | typer/httpx/etc. wait for runtime ADR | deferred 3/3 |

## Translation provenance

Written to `crates/cobrust-strike/PROVENANCE.toml`. Schema per
ADR-0007 §3 + ADR-0022:

```toml
[source]
library = "requests"
version = "2.31.0"

[gates]
l3_downstream_dependents = "deferred to M9 per ADR-0022 §"Negative consequences""

[gates.dependents]
covered = []
deferred = ["httpx", "aiohttp", "urllib3-shim"]
deferred_reason = "ADR-0022 §"Negative consequences": ecosystem-batch sprint ships pure breadth; downstream-dependent surface widens at M9"
```

## Done means (M-batch — DONE)

- [x] All 13 spec functions translated (6 free verbs + Session::new + 6 Session methods).
- [x] L0 spec + canned table + harness committed at `corpus/requests/`.
- [x] L2.behavior fuzz: ≥ 1000 inputs × 3 seeds; URL parser dispatch panic-free.
- [x] L2.behavior wiremock: in-process HTTP/1.1 server validates GET/POST/PUT/PATCH/DELETE/HEAD round-trip.
- [x] L2.perf gate: surface-translate / Rust-binding tier (0.8×) per ADR-0022 §6.
- [x] L3.pyo3 wrapper + `--features pyo3` build path wired per ADR-0011.
- [x] Optional httpbin smoke runs when network is reachable; skips cleanly otherwise.

## Done means (M9+ — open)

- [ ] Cookie jar (`Session.cookies` mirror).
- [ ] Streaming response body API (compatible with the Cobrust
      structured-concurrency runtime once M8 lands).
- [ ] `Response.raise_for_status()` parity.
- [ ] Auth shim crate(s) `cobrust-strike-auth` for HTTPBasicAuth /
      OAuth.

## Non-goals

- **Not** a complete `requests` implementation — see "Scope window".
- **Not** hand-written. Editing `src/client.rs` directly is
  forbidden; regenerate via the pipeline.
- **Not** async on its public surface (per constitution §2.2). The
  backend is `reqwest::blocking`; M8+ may swap to the Cobrust
  structured-concurrency runtime without breaking callers.

## Cross-references

- `mod:translator` — pipeline that emits this crate.
- `mod:hood` — sister M-batch crate (CLI parsing).
- [adr:0022](../adr/0022-translation-ecosystem-batch.md) — M-batch methodology.
- [adr:0007](../adr/0007-translator-pipeline.md) — pipeline base.
- [adr:0011](../adr/0011-pyo3-build-path.md) — PyO3 build path.
- [adr:0019](../adr/0019-phase-e-language-runtime-roadmap.md) — runtime story (M8+).
- requests upstream — https://github.com/psf/requests (Apache-2.0).
- reqwest crate — https://crates.io/crates/reqwest.
