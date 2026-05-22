# Cobrust — VSCode / Cursor extension

Syntax highlighting + Language Server support for the
[Cobrust](https://github.com/Cobrust-lang/cobrust) language.

Works in:

- Visual Studio Code (1.80+)
- Cursor (any version with VSCode 1.80+ API compatibility)
- VSCodium
- code-server / GitHub Codespaces

## Features (v0.1.0)

- TextMate grammar for `.cb` files (comments, strings incl. f-strings with
  embedded expressions, numeric literals with type suffixes, decorators
  including `@py_compat` tier highlight, keywords, types, operators)
- LSP client wired to `cobrust-lsp` v1.3 (13 handlers: hover, completion,
  goto-def, references, rename, code-action, semantic tokens, inlay hints,
  call hierarchy, diagnostics, formatting, workspace symbols, signature
  help — see [ADR-0057a](../../docs/agent/adr/0057a-lsp-implementation.md))
- Python-like indentation rules and bracket auto-close
- Snippets: `fn`, `if`, `for`, `while`, `class`, `struct`, `match`,
  `matchres`, `matchopt`, `@py`, `main`

## Prerequisites

You need the `cobrust-lsp` binary on your `$PATH`. It ships with the
`cobrust` compiler. Install one of:

- **Cargo (Rust 1.94+)**
  ```bash
  cargo install cobrust
  ```
- **Prebuilt wheel** (9 CPU-tier variants, see
  [ADR-0065](../../docs/agent/adr/0065-tier-3-prebuilt-multi-wheel-distribution.md))
  ```bash
  pip install cobrust
  ```
  When installed via pip, ensure your venv is activated when launching the
  editor (or pass `cobrust.lspPath` explicitly).

Verify:
```bash
which cobrust-lsp
cobrust-lsp --version
```

## Installation

### VSCode (from a `.vsix` file)

```bash
code --install-extension cobrust-0.1.0.vsix
```

### Cursor (from a `.vsix` file)

```bash
cursor --install-extension ./cobrust-0.1.0.vsix
```

### VSCodium (from a `.vsix` file)

```bash
codium --install-extension ./cobrust-0.1.0.vsix
```

### From a marketplace

Once published (see [PUBLISHING.md](./PUBLISHING.md) — currently user-side):

- **VSCode Marketplace**: search "Cobrust", publisher `cobrust-lang`
- **Open VSX** (preferred by Cursor + VSCodium): same search

## Settings

| Setting | Default | Description |
|---|---|---|
| `cobrust.lspPath` | `"cobrust-lsp"` | Path to the `cobrust-lsp` binary. Absolute paths recommended for pip installs in non-activated venvs. |
| `cobrust.trace.server` | `"off"` | LSP wire trace level: `off` / `messages` / `verbose`. Output appears in the "Cobrust LSP Trace" output channel. |

## Troubleshooting

### "Cobrust LSP failed to start"

1. Run `which cobrust-lsp` (Unix) or `where cobrust-lsp` (Windows) to verify
   the binary is on `$PATH`.
2. If you installed via `pip` in a venv, the editor may not inherit the
   venv. Either:
   - Launch the editor from the activated venv shell, or
   - Set `cobrust.lspPath` to the absolute path:
     `~/.venv/bin/cobrust-lsp` (Unix) or `C:\path\to\Scripts\cobrust-lsp.exe`
     (Windows).

### Diagnostics not appearing

1. Check the "Cobrust LSP" output channel for stderr.
2. Set `cobrust.trace.server` to `verbose` and inspect the
   "Cobrust LSP Trace" channel for JSON-RPC traffic.

## Build from source

```bash
cd editors/vscode-cobrust
npm install
npm run compile
npx vsce package --no-dependencies
# yields cobrust-0.1.0.vsix
```

## Out of scope for 0.1.0

- DAP debugger integration (deferred to Phase L wave-6 follow-up;
  `cobrust-dap` v1.2 server exists but extension-side `launch.json`
  contribution is pending a separate ADR)
- Bundled binary (kept external per ADR-0067 §Options)
- REPL embed

See [ADR-0067](../../docs/agent/adr/0067-vscode-cursor-extension.md) for the
full design rationale.

## License

Apache-2.0 OR MIT (dual, per ADR-0001).
