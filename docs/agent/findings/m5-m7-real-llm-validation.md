---
doc_kind: finding
finding_id: m5-m7-real-llm-validation
last_verified_commit: 6103b91
dependencies: [adr:0004, mod:llm-router]
---

# Finding: M3 LLM Router round-trips against a real OpenAI-compatible endpoint

## Hypothesis

The M3 router (`mod:llm-router`) was delivered with 56 in-process /
`wiremock`-mocked tests. None of those tests exercised a real LLM
provider over the public network. Three load-bearing claims of
`adr:0004` could therefore only be verified against the synthetic
contract, not the wire:

1. The `OpenAiProvider` adapter is correct against an *arbitrary*
   OpenAI-compatible base URL — not just the `wiremock` server's
   handcrafted JSON.
2. The content-addressed cache replays a live response bit-for-bit
   from disk on the second identical dispatch, and the ledger records
   `cache_hit=true` with `latency_ms=0`.
3. Transport-level failure (TCP connect refused) surfaces as
   `LlmError::Transport` through `RouterError::AllFailed` without
   panicking and with bounded wall-clock.

The hypothesis was that **all three contracts hold against a private
OpenAI-compatible deployment** ("user_codex", `http://104.244.92.250:8317/v1`,
model `gpt-5.5`) operated by the user — the same kind of endpoint the
production translation pipeline (M4–M7) will dispatch against.

## Method

Implementation: `crates/cobrust-llm-router/tests/real_llm_smoke.rs`
(integration test, gated on `USER_CODEX_API_KEY`).

### Why an integration test, not an ADR

This is a **contract validation**, not a design change. The router's
public surface, error taxonomy, retry policy, cache key
canonicalisation, and ledger schema are all binding under
`adr:0004` and were not modified. The test verifies that those
already-pinned contracts hold over the wire. No new ADR is required.

### Endpoint

- Base URL: `http://104.244.92.250:8317/v1`
- Model: `gpt-5.5`
- Wire format: OpenAI-compatible (`POST /v1/chat/completions`,
  `Authorization: Bearer …`, `usage` object on response)
- Auth: API key passed via `USER_CODEX_API_KEY` env var only;
  literal key value is **never** committed to the repo

The endpoint advertises
`{"endpoints": ["POST /v1/chat/completions","POST /v1/completions","GET /v1/models"]}`
on `GET /`. A `GET /v1/models` enumerates ~30 model ids spanning
Anthropic and OpenAI families — a multi-provider proxy.

### Skip discipline

- `USER_CODEX_API_KEY` unset → both subtests `eprintln!` a skip
  message and `return;`. Default `cargo test --workspace --locked`
  invocation makes **zero** network calls. This mirrors
  `crates/cobrust-translator/tests/msgpack_pipeline.rs`'s
  `msgpack_real_llm_smoke_runs_when_key_in_env` pattern.

### Three-phase smoke

1. **Live round-trip** (`real_llm_round_trip_then_cache_replay_is_bit_identical`)
   - Configure a single-provider router (`user_codex` → `gpt-5.5`,
     strategy `quality`).
   - Dispatch a 16-token completion with prompt
     `"Reply with the single word: ok"` under a 30 s `tokio::timeout`.
   - Assert: response received, `text` non-empty, `provider="user_codex"`,
     `cache_hit=false`. Ledger has exactly one entry with
     `outcome="ok"`, `cache_hit=false`, BLAKE3 cache key.

2. **Cache replay** (same test, second dispatch)
   - Re-dispatch the *identical* `CompletionRequest` under a 2 s
     timeout.
   - Assert: `cache_hit=true`, `replay.response == live.response`
     (bit-for-bit `Eq`), ledger now has two entries, second has
     `cache_hit=true` and `latency_ms=0` (per `adr:0004` ledger
     schema; the router records cache hits with zero latency).

3. **Transport-failure isolation** (`real_llm_transport_failure_is_isolated_not_panic`)
   - Re-build a router pointing the adapter at `http://127.0.0.1:1`
     (closed port).
   - Override `RetryPolicy` to `max_attempts=1, max_total_ms=1500` so
     the failure surfaces fast.
   - Wrap the dispatch in a 15 s `tokio::timeout` (darwin loopback
     ECONNREFUSED via reqwest can take 1–4 s; 15 s is the safety net,
     not the operational cap).
   - Assert: `RouterError::AllFailed` containing one
     `(provider, LlmError::Transport | LlmError::Server)` pair,
     ledger has one entry with `outcome != Ok` and
     `error_code in {"transport","server"}`.

### Reproduction

```bash
# Skip path (CI default — never makes a network call).
unset USER_CODEX_API_KEY
cargo test --locked -p cobrust-llm-router --test real_llm_smoke -- --nocapture
# → real-LLM smoke: USER_CODEX_API_KEY unset — skipping
# → real-LLM smoke: USER_CODEX_API_KEY unset — skipping transport-failure case
# → test result: ok. 2 passed

# Validation path (operator-only).
USER_CODEX_API_KEY='<your-key>' cargo test --locked \
    -p cobrust-llm-router --test real_llm_smoke -- --nocapture
```

Hardware: macOS arm64. `[providers.user_codex]` configuration
mirrored into `cobrust.toml.example` for documentation parity.

## Result

**Hypothesis confirmed** on all three contracts.

### Round-trip evidence

```text
real-LLM smoke: live response = "ok" (model=gpt-5.5,
                                       prompt_tokens=355,
                                       completion_tokens=5)
```

The 355 prompt tokens is striking — the user codex proxy injects
its own system prefix on top of every request, yielding a token
count that is **not** equal to the literal prompt's tokenisation.
This is observable but not actionable: the proxy is the source of
truth for billing, and the router faithfully records what the
provider reports. Documented here so future readers don't waste
time hunting a phantom token-accounting bug.

### One actual ledger entry (`live`)

Captured verbatim from a 2026-05-08 run:

```json
{
  "ts": "2026-05-08T02:11:06.079908Z",
  "task": "translate",
  "provider": "user_codex",
  "model": "gpt-5.5",
  "cache_key": "blake3:b406dcb795c3bedaf646c7bd2753a8589c80a29419f8757152729e605cb241c5",
  "cache_hit": false,
  "prompt_tokens": 355,
  "completion_tokens": 5,
  "total_tokens": 360,
  "latency_ms": 2782,
  "attempt": 1,
  "outcome": "ok",
  "error_code": null,
  "consensus_group": null
}
```

### One actual ledger entry (`cache_hit replay`, same run)

```json
{
  "ts": "2026-05-08T02:11:06.080375Z",
  "task": "translate",
  "provider": "user_codex",
  "model": "gpt-5.5",
  "cache_key": "blake3:b406dcb795c3bedaf646c7bd2753a8589c80a29419f8757152729e605cb241c5",
  "cache_hit": true,
  "prompt_tokens": 355,
  "completion_tokens": 5,
  "total_tokens": 360,
  "latency_ms": 0,
  "attempt": 1,
  "outcome": "ok",
  "error_code": null,
  "consensus_group": null
}
```

Notes:
- The two entries share the **same** `cache_key`, proving the
  BLAKE3 canonical-bytes hash is deterministic across in-process
  back-to-back dispatches.
- `latency_ms` is `2782` (live) vs `0` (cache hit), exactly per
  `adr:0004`'s schema — cache hits explicitly do NOT bill latency.
- Tokens reported on the cache hit are the **cached** values, not a
  fresh provider call. Operators reading the ledger to compute spend
  must filter on `cache_hit=false`.

### Latency profile

Three cold round-trips sampled out-of-band (curl direct to
`/v1/chat/completions`):

| Call | latency_ms |
|---|---|
| 1 | 1910 |
| 2 | 1592 |
| 3 | 1578 |
| Router dispatch | 2782 |

The router-recorded 2782 ms is roughly the curl mean (~1.7 s) plus
overhead for the test build cache check, JSON serialisation, and
cache `put`. The 5 s timeout cap on the smoke is therefore ~2× the
operational latency — comfortable margin for slow CI without
inviting flaky long-tail timeouts.

### Transport-failure isolation

Test runtime (full subtest) ≈ 1.30 s when the binary is invoked
directly. Bounded by `RetryPolicy.max_total_ms = 1500` and the
single-attempt budget. The 15 s `tokio::timeout` was never
triggered across six back-to-back runs (three with key, three without).

Surface: `RouterError::AllFailed([("user_codex", LlmError::Transport(_))])`
in three runs out of three. Ledger row was
`{"cache_hit": false, "outcome": "error_transient", "error_code": "transport"}`
with `prompt_tokens=0, completion_tokens=0` (no provider call
was billed). Exactly the contract `adr:0004` pins.

### Token-spend ledger for the validation pass

Per smoke run (the only command an operator types):

| Phase | Live calls | Tokens billed |
|---|---|---|
| Round-trip | 1 | 360 |
| Cache replay | 0 (served from `<tempdir>/cache/...`) | 0 |
| Transport-failure | 1 attempted (TCP refused, no HTTP body) | 0 |
| **Total** | **2 wire-touched, 1 billed** | **360** |

Five operator-side runs during this validation effort (test +
debug + final) totalled ~1.8 k tokens — well below the brief's
"5–10 calls" ceiling.

### Divergences from synthetic-mode

None observed in the contract surface. Two operational
observations worth recording for future translators:

- **Multi-provider proxies inject system prefixes.** Token counts
  on the user codex are 70× the literal-prompt tokenisation. M4+
  translators that want to rate-limit on prompt tokens must read
  the ledger's reported `prompt_tokens`, not estimate from the
  request bytes.
- **Loopback ECONNREFUSED on darwin is not instantaneous via
  `reqwest`.** The test binary spends ~1.3 s on the failure path
  even though raw `socket.connect` returns in <1 ms. The 5 s cap
  is reasonable; tightening below 2 s would risk false flakes on
  shared CI.

## Conclusion

- **Operational decision.** The M3 router is now empirically
  validated against a real OpenAI-compatible endpoint. All
  invariants pinned by `adr:0004` (cache determinism, ledger
  schema, error-taxonomy mapping, transport-failure isolation)
  hold over the wire.
- **Reusable rule.** Every additional provider deployment a
  Cobrust operator wants to support can be smoke-tested by
  copy-pasting `cobrust.toml.example`'s `[providers.user_codex]`
  block, swapping `base_url` / `api_key_env` / `models`, and
  re-running `cargo test --locked -p cobrust-llm-router --test
  real_llm_smoke -- --nocapture` with the appropriate env var.
  The skip discipline keeps default CI hermetic.
- **Followup (optional, not in this commit).**
  `crates/cobrust-translator/tests/msgpack_pipeline.rs::msgpack_real_llm_smoke_runs_when_key_in_env`
  currently no-ops even when a key is present — it predates the
  `OpenAiProvider` HTTP adapter being wire-validated. With this
  finding's evidence, that stub can be safely upgraded to actually
  dispatch (gated on `USER_CODEX_API_KEY` for compatibility with
  the user's deployment, or `OPENAI_API_KEY` / `ANTHROPIC_API_KEY`
  for the official providers). Out of scope here: that file is in
  a peer crate's domain.

## Cross-references

- `adr:0004` — load-bearing decisions this finding validates over
  the wire.
- `mod:llm-router` — `OpenAiProvider`, `Router`, `Cache`, `Ledger`
  surface that the smoke exercises.
- `crates/cobrust-llm-router/tests/real_llm_smoke.rs` —
  implementation.
- `cobrust.toml.example` — `[providers.user_codex]` +
  `[routing.real_llm_smoke]` blocks documenting the validated
  endpoint shape.
- `crates/cobrust-translator/tests/msgpack_pipeline.rs::msgpack_real_llm_smoke_runs_when_key_in_env`
  — the M5/M6 stub flagged in "Followup" above.
