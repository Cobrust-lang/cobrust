# Corpus: requests

Vendored representative subset of `requests` 2.31.0 — the M-batch
ecosystem-translation deliverable per ADR-0022 §1.

## Scope window (M-batch)

- **In scope**:
  - Free verb functions: `get / post / put / patch / delete / head`.
  - `Session()` class with the same six verb methods.
  - `Response` with `.status_code / .text / .json / .headers / .ok`.
  - Single error type: `HttpError { kind, message }` covering
    `requests.exceptions.{ConnectionError, Timeout, HTTPError,
    JSONDecodeError}`.
- **Out of scope (M9+)**:
  - Cookie jar / `Session.cookies` API.
  - Auth shims (`auth=HTTPBasicAuth(...)`, OAuth integrations).
  - Streaming responses (`stream=True` + iterator API).
  - `Response.raise_for_status()` exception escalation.
  - Mounted adapters / custom transport adapters.

## L0 spec

`spec.toml` pins the public-surface signatures + semantic
invariants (URL parsing routes to `InvalidUrl`, body decoding routes
to `DecodeBody`, etc.).

## Differential gate

The L3 differential test in `crates/cobrust-requests/tests/
requests_downstream.rs` spins an in-process HTTP wiremock on a
random localhost port + dispatches the cobrust-requests verbs at
it. Optional smoke against `https://httpbin.org/get` runs when
network is reachable; logs a clean skip otherwise.

## Why bind reqwest::blocking

Per ADR-0022 §2: matches `requests.get(...)` sync semantics 1:1;
honours constitution §2.2 ("no async / sync coloring"). The
Cobrust structured-concurrency runtime (per ADR-0019 Phase E lands
M8+) will replace the blocking backend without a public-API break.

## Translation provenance

Every emitted file at `crates/cobrust-requests/src/` carries:

```text
// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: requests 2.31.0
// oracle: cpython 3.11 (module: requests)
// functions translated: 13
// see PROVENANCE.toml for the full manifest.
```

Per-function provenance lines (one per translated function) follow
the M6 format:

```text
// fn:Session::get provider=synthetic model=requests-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
```
