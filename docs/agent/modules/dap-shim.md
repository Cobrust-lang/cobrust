---
module_id: cobrust-dap-shim
last_verified_commit: HEAD
phase: v0.6.x transitional (ADR-0068 §4.2)
adr: 0068
dependencies: [cobrust-dap]
---

# cobrust-dap-shim — Transitional `cobrust-dap` binary wrapper

## Purpose

ADR-0068 collapsed `cobrust-dap` into a `cobrust dap` subcommand
(canonical v0.6.0+ entry); the standalone `cobrust-dap` binary name
is preserved by this shim crate so v0.5.x editor integrations and
existing `cobrust debug --dap` paths keep working unchanged. Deleted
at v0.7.0 per ADR-0068 §4.4.

## Public API surface

None. The shim is a `[[bin]]`-only crate that exposes no library
surface. Its `src/main.rs` is a 2-line `main()` that calls
[`cobrust_dap::run`](./dap.md) and translates the `Result` into a
`std::process::ExitCode`.

## Binary

| Binary name | Path | Notes |
|---|---|---|
| `cobrust-dap` | `crates/cobrust-dap-shim/src/main.rs` | Identical stdio DAP server protocol surface as `cobrust dap` subcommand — same `cobrust_dap::run()` lib entry |

## Tests

The crate hosts `tests/dap_e2e_smoke.rs` (moved from
`crates/cobrust-dap/tests/` in v0.6.0 because
`env!("CARGO_BIN_EXE_cobrust-dap")` only resolves inside the
bin-providing crate; the bin now lives here, not in `cobrust-dap`).
The smoke test is `#[ignore]` and runs only via `cargo test -p
cobrust-dap-shim -- --ignored`.

## Lifecycle

- **v0.6.0** — introduced as transitional shim alongside `cobrust dap`
  subcommand.
- **v0.6.x** — shipped in every wheel under `bin/cobrust-dap` for
  extension v0.1.x compat and `cobrust debug --dap` spawn path.
- **v0.7.0** — crate deleted; consumers must spawn `cobrust dap`
  directly.

## Done means

- [x] Crate exists with `[[bin]] name = "cobrust-dap"`.
- [x] `main()` is a 2-line wrapper around `cobrust_dap::run()`.
- [x] Workspace member set includes `crates/cobrust-dap-shim/`.
- [x] `release.yml` builds + packages this binary into `bin/cobrust-dap`
      of every v0.6.x wheel.
- [x] `dap_e2e_smoke.rs` test moved from `cobrust-dap` to this crate.

## Cross-references

- [ADR-0068](../adr/0068-single-binary-subcommand-collapse.md) — design
  rationale for the subcommand collapse + shim transition.
- [`cobrust-dap`](./dap.md) — the lib-only DAP crate this shim delegates
  to.
