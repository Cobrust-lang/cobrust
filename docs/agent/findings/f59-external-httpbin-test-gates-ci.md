---
finding_id: F59
title: External httpbin.org smoke test flaked CI — external-service dependency must not gate CI
status: RESOLVED (#[ignore]'d + skip-path widened to cover up-but-degraded)
date: 2026-05-27
severity: low
siblings: [F37, F44, F55, F56]
last_verified_commit: 8b810e7
---

# F59 — `l3_optional_httpbin_smoke` external-service flake gated CI

## §1 Context

Surfaced 2026-05-27 on the Z.5 (`std.json`) CI run (`26513150271`). `cargo test
(ubuntu-latest)` failed on `crates/cobrust-requests/tests/requests_downstream.rs::l3_optional_httpbin_smoke`
at `:230` (`assert_eq!(resp.status_code(), 200)`). The macOS test job + the local
dev run + all prior CI runs passed — classic flaky external-service divergence.
The Z.5 + F56 work was NOT the cause (the 4 `intrinsics_json.rs` E2E tests passed
on ubuntu).

## §2 Root cause

The test's "skip cleanly if offline" contract (ADR-0022 M-batch) only handled the
**probe-error** case: it probed `httpbin.org/get` with a 3s timeout and returned
early if the probe `.send()` errored. But httpbin.org is frequently *reachable yet
degraded* — it rate-limits / returns 503 under load. On this run the probe
succeeded (httpbin up) but the real `cobrust_get` returned a non-200, so the hard
`assert_eq!(.., 200)` failed. An external service's health thus gated CI —
violating the deterministic-CI discipline (F37 honest-debt / F44 stale-green:
CI green must mean "workspace correct", never "a third-party host happened to be
healthy").

## §3 Resolution

1. `#[ignore]` the test (external-service smoke is opt-in: `cargo test -- --ignored`
   / a dedicated network CI job). CI is now deterministic w.r.t. httpbin.
2. Widened the skip path so the test is robust when run opt-in: clean-skip on
   probe-unreachable, request error, non-200 status, AND malformed body — only a
   reachable+healthy+well-formed httpbin reaches the `url.contains("httpbin.org")`
   assertion.

## §4 Lineage

Sibling of F55/F56 (latent gaps surfaced as CI matured under the §X.3 LLVM-default
flip + the new always-run test matrix) and of F37/F44 (CI-determinism discipline).
General rule reinforced: **no test that depends on an external network service may
gate CI** — gate behind `#[ignore]` or an explicit opt-in env/feature.
