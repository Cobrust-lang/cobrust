# Changelog

All notable changes to the Cobrust VSCode/Cursor extension are documented in
this file. Follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.0] — 2026-05-25

### Added
- **DAP debugger integration** via `contributes.debuggers` (debug type
  `cobrust`). Provides initial-configuration template, snippet, and an
  `onDebug` activation event. F5 / "Run and Debug" launches a session
  against `program: ${file}` with optional `stopOnEntry` and `args`.
- `CobrustDebugAdapterFactory` (extension.ts) implements
  `DebugAdapterDescriptorFactory`: spawns `cobrust dap` (subcommand path,
  v0.6.0+) by default; falls back to standalone `cobrust-dap` shim when
  `cobrust.dap.useSubcommand=false`.
- New settings:
  - `cobrust.dapPath` (default `"cobrust"`) — binary for DAP server.
  - `cobrust.dap.useSubcommand` (default `true`) — toggle subcommand vs shim.
  - `cobrust.lsp.useSubcommand` (default `true`) — symmetric for LSP.
- Categories now include `Debuggers`; keywords gain `dap` + `debugger`.

### Changed
- **LSP launch path migrated to `cobrust lsp` subcommand** (ADR-0068 single-
  binary collapse). `cobrust.lspPath` default flipped `"cobrust-lsp"` →
  `"cobrust"`. The `cobrust-lsp` standalone shim remains a documented
  fallback via `cobrust.lsp.useSubcommand=false` for v0.5.x compatibility.
- Error message on LSP-start failure now reports whether the subcommand
  or shim path was attempted, and how to switch.

### Cross-refs
- [ADR-0067](../../docs/agent/adr/0067-vscode-cursor-extension.md) — original scaffold (v0.1.0).
- [ADR-0068](../../docs/agent/adr/0068-single-binary-subcommand-collapse.md) — `cobrust lsp` / `cobrust dap` subcommand collapse (this release activates the extension-side wire-up).

### Out-of-scope for 0.2.0
- Bundled `cobrust` binary (still external; user must `cargo install`
  or use a prebuilt wheel per ADR-0065 + ADR-0069).
- REPL embed inside VSCode terminal.
- Inline diagnostic decoration beyond what LSP / DAP publish by default.

## [0.1.0] — 2026-05-22

### Added
- Initial release. Scaffolded per ADR-0067.
- TextMate grammar for `.cb` files (`source.cobrust` scope) covering
  comments, strings (regular, raw, byte, f-string with embedded
  expressions), numeric literals (decimal, hex, octal, binary,
  type-suffixed), decorators (with `@py_compat(strict|semantic|numerical|none)`
  tier highlight), keywords, primitive types, ADT constructors
  (`Some` / `None` / `Ok` / `Err`), operators including borrow `&` and
  error-propagation `?`, and PRELUDE call sites.
- Language configuration: `#` line comments, bracket pairs, auto-close
  for `"` / `'` / `f"` / `r"` / `b"`, Python-like indentation rules.
- LSP client wired to `cobrust-lsp` v1.3 (13 handlers, see ADR-0057a and
  cobrust skill §9c). Configurable via `cobrust.lspPath` setting; defaults
  to `cobrust-lsp` resolved on `$PATH`.
- Snippets: `fn`, `if`, `for`, `while`, `class`, `struct`, `match`,
  `matchres` (Result destructure), `matchopt` (Option destructure),
  `@py` (py_compat-decorated fn), `main`.
- Trace plumbing: `cobrust.trace.server` setting (`off` / `messages` /
  `verbose`) for LSP wire diagnostics.

### Source-of-truth note
- The TextMate grammar in `syntaxes/` is a file-copy of
  `docs/agent/outreach/cobrust.tmLanguage.json` (the canonical Linguist
  contribution source). Future divergence (extension ships grammar fixes
  faster than Linguist cycle) is allowed and will be recorded here.

### Out-of-scope for 0.1.0 (see ADR-0067)
- DAP debugger integration (Phase L wave-6 follow-up; separate ADR)
- Bundled `cobrust-lsp` binary (user must `cargo install cobrust` or
  install via prebuilt wheel per ADR-0065)
- REPL embed inside VSCode terminal
- Inline diagnostic decoration beyond what LSP publishes by default
