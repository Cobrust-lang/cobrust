---
module_id: cobrust-lsp-shim
last_verified_commit: HEAD
phase: v0.6.x transitional (ADR-0068 §4.2)
adr: 0068
dependencies: [cobrust-lsp]
---

# cobrust-lsp-shim — Transitional `cobrust-lsp` binary wrapper

## Purpose

ADR-0068 collapsed `cobrust-lsp` into a `cobrust lsp` subcommand
(canonical v0.6.0+ entry); the standalone `cobrust-lsp` binary name
is preserved by this shim crate so v0.1.x editor extensions (which
spawn `cobrust-lsp` from `$PATH`) keep working unchanged. Deleted at
v0.7.0 per ADR-0068 §4.4.

## Public API surface

None. The shim is a `[[bin]]`-only crate that exposes no library
surface. Its `src/main.rs` is a 2-line `main()` that calls
[`cobrust_lsp::run`](./lsp.md) and translates the `Result` into a
`std::process::ExitCode`.

## Binary

| Binary name | Path | Notes |
|---|---|---|
| `cobrust-lsp` | `crates/cobrust-lsp-shim/src/main.rs` | Identical stdio LSP server protocol surface as `cobrust lsp` subcommand — same `cobrust_lsp::run()` lib entry |

## Lifecycle

- **v0.6.0** — introduced as transitional shim alongside `cobrust lsp`
  subcommand.
- **v0.6.x** — shipped in every wheel under `bin/cobrust-lsp` for
  extension v0.1.x compat.
- **v0.7.0** — crate deleted; extension v0.2.0+ must spawn `cobrust
  lsp` directly (or the install will break on the standalone-binary
  PATH lookup).

## Done means

- [x] Crate exists with `[[bin]] name = "cobrust-lsp"`.
- [x] `main()` is a 2-line wrapper around `cobrust_lsp::run()`.
- [x] Workspace member set includes `crates/cobrust-lsp-shim/`.
- [x] `release.yml` builds + packages this binary into `bin/cobrust-lsp`
      of every v0.6.x wheel.

## Cross-references

- [ADR-0068](../adr/0068-single-binary-subcommand-collapse.md) — design
  rationale for the subcommand collapse + shim transition.
- [`cobrust-lsp`](./lsp.md) — the lib-only LSP crate this shim delegates
  to.
