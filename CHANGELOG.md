# Changelog

All notable changes to Cobrust are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[SemVer](https://semver.org/spec/v2.0.0.html). Each release cross-links its
governing ADR(s) under `docs/agent/adr/`.

## [0.7.0] — Unreleased

The **LLVM-default + dora-cb + network-libs** release. Master design:
[ADR-0070](docs/agent/adr/0070-v0.7.0-master-design.md).

### Changed

- **LLVM is the default (and only AOT) codegen backend.** `cobrust-codegen`
  ships `default = ["llvm"]` (ADR-0070 X.3); the Cranelift AOT backend was
  removed (X.4 — `cobrust-jit` keeps a dormant Cranelift JIT substrate,
  deferred to v0.8.x per §6 Q2). A from-source build and the prebuilt Linux
  wheel now require **system LLVM 18** (`LLVM_SYS_181_PREFIX`) — see the
  install notes in `README.md`.

### Removed

- **Transitional `cobrust-lsp` / `cobrust-dap` shim binaries** (ADR-0070
  X.5 / ADR-0068 §7.2). The LSP and DAP servers are now reached only via
  the `cobrust lsp` / `cobrust dap` subcommands of the single `cobrust`
  binary. **Breaking for IDE extensions** that invoked the standalone
  binaries — switch to the subcommands.
- The **`x86_64-unknown-linux-musl` and `aarch64-unknown-linux-gnu`
  prebuilt wheels are deferred** to v0.7.x (ADR-0070 X.6 / finding F77 —
  LLVM-default blocks a fully-static musl link and the `cross` aarch64
  image). v0.7.0 ships 5 wheels: `x86_64-unknown-linux-gnu` {v1,v3,v4} +
  `aarch64-apple-darwin` {m1,m2}.

### Added

- **Robotics readiness — `dora` ecosystem module** (Stream Y / ADR-0076,
  -0076c): a Cobrust-authored node participates in a live dora-rs dataflow
  (`examples/dora_hello/`); typed `event.data_buffer()` /
  `send_output_buffer()` Arrow↔`coil.Buffer` round-trip for 5 dtypes; a
  compile-time `DoraUnknownOutputId` check (ADR-0092) rejecting a mistyped
  `send_output` id at `cobrust check`.
- **Network backends + a REST demo** (Stream Z / ADR-0078): `pit`
  (HTTP) + `den` (SQLite) + `redis` etc.; a Cobrust REST service —
  HTTP server + DB-backed + JSON endpoints, demoable via `curl`
  (`examples/z8_rest_blog/`).
- **FastAPI-real type-driven request validation + OpenAPI** (#156 /
  ADR-0080, -0081): a declarative body `class` drives validation, OpenAPI
  schema emission, AND real validated-body field reads —
  `i64`/`str`/`f64`/`bool`/nested-class/`list[...]` — registration-gated
  (no UB on unregistered params).
- **`coil` numerical surface** (the NumPy rebrand): a broad `numpy`-shaped
  module — constructors, ufuncs, reductions, linalg, dtype-cast, and the
  `coil.array([list])` = `np.array` bridge.
- Core **stdlib modules** `math` / `re` / `random` / `time`, and §2.5
  LLM-first builtin fixes (`len`/`abs`/`range`/`min`/`max`/`sum` on the
  bare spellings an LLM reaches for).

[0.7.0]: https://github.com/Cobrust-lang/cobrust/compare/v0.6.2...HEAD
