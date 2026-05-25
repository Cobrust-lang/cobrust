# Cobrust — VSCode / Cursor extension

Syntax highlighting + Language Server support for the
[Cobrust](https://github.com/Cobrust-lang/cobrust) language.

Works in:

- Visual Studio Code (1.80+)
- Cursor (any version with VSCode 1.80+ API compatibility)
- VSCodium
- code-server / GitHub Codespaces

## Features (v0.2.0)

- TextMate grammar for `.cb` files (comments, strings incl. f-strings with
  embedded expressions, numeric literals with type suffixes, decorators
  including `@py_compat` tier highlight, keywords, types, operators)
- LSP client wired to `cobrust-lsp` v1.3 (13 handlers: hover, completion,
  goto-def, references, rename, code-action, semantic tokens, inlay hints,
  call hierarchy, diagnostics, formatting, workspace symbols, signature
  help — see [ADR-0057a](../../docs/agent/adr/0057a-lsp-implementation.md))
- **DAP debugger** (new in 0.2.0): F5 / "Run and Debug" launches a
  `cobrust dap` session against the current `.cb` file. Launch-config
  template + snippet contributed via `contributes.debuggers`.
- Python-like indentation rules and bracket auto-close
- Snippets: `fn`, `if`, `for`, `while`, `class`, `struct`, `match`,
  `matchres`, `matchopt`, `@py`, `main`

## Prerequisites

You need a `cobrust` binary on your `$PATH`. Both LSP and DAP servers are
reached via subcommands of the single binary (per ADR-0068):

- **v0.6.0+ (recommended)**: extension spawns `cobrust lsp` and
  `cobrust dap` subcommands. This is the default and matches
  `cobrust.lsp.useSubcommand=true` + `cobrust.dap.useSubcommand=true`.
- **v0.5.2**: standalone `cobrust-lsp` binary bundled in the wheel;
  `cobrust-dap` v1.2 server also exists as a separate shim. Toggle
  `cobrust.lsp.useSubcommand=false` and/or `cobrust.dap.useSubcommand=false`
  to fall back to those shims.
- **v0.5.1 and earlier**: `cobrust-lsp` / `cobrust-dap` standalone binaries
  were NOT bundled in the wheel. Build from source via `cargo install --git
  https://github.com/Cobrust-lang/cobrust cobrust-cli` or symlink from a
  local cargo build.

Caveat about v0.5.x compile path: per F46
(`docs/agent/findings/f46-wheel-not-installable-runtime-stdlib-gap.md`),
v0.5.1 + v0.5.2 wheels were 100% broken for `cobrust run` because the
binary baked the GH Actions runner workspace path. LSP-only usage
(extension surface) was unaffected since the LSP server does not
invoke the compile pipeline. Upgrade compiler to v0.6.0 for working
`cobrust run` / `cobrust build`.

Install one of:

- **Cargo (Rust 1.94+)**
  ```bash
  cargo install --git https://github.com/Cobrust-lang/cobrust cobrust-cli
  ```
- **Prebuilt wheel v0.6.0+** (9 CPU-tier variants, ADR-0065 +
  ADR-0069 FHS layout)
  ```bash
  curl -L https://github.com/Cobrust-lang/cobrust/releases/download/v0.6.0/cobrust-v0.6.0-<triple>-<cpu>.tar.gz \
    | tar xz -C $HOME/.local/
  ln -sf $HOME/.local/cobrust-v0.6.0/bin/cobrust $HOME/.local/bin/cobrust
  ```

Verify:
```bash
cobrust --version           # → cobrust 0.6.0
cobrust lsp --help 2>&1 | head -1   # v0.6.0+ LSP subcommand path
cobrust dap --help 2>&1 | head -1   # v0.6.0+ DAP subcommand path
which cobrust-lsp || true   # v0.5.2 + v0.6.x shim binary path (fallback)
which cobrust-dap || true   # v0.5.2 + v0.6.x DAP shim path (fallback)
```

ADR + finding cross-refs:
- [ADR-0067](../../docs/agent/adr/0067-vscode-cursor-extension.md) — original extension scaffold (extension v0.1.0)
- [ADR-0068](../../docs/agent/adr/0068-single-binary-subcommand-collapse.md) — `cobrust lsp` / `cobrust dap` subcommand collapse (extension v0.2.0 activates this)
- [ADR-0069](../../docs/agent/adr/0069-wheel-layout-standardization.md) — FHS bin/lib/share wheel layout
- [F46](../../docs/agent/findings/f46-wheel-not-installable-runtime-stdlib-gap.md) — v0.5.x wheel runtime+stdlib bundle gap

## Installation

### VSCode (from a `.vsix` file)

```bash
code --install-extension cobrust-0.2.0.vsix
```

### Cursor (from a `.vsix` file)

```bash
cursor --install-extension ./cobrust-0.2.0.vsix
```

### VSCodium (from a `.vsix` file)

```bash
codium --install-extension ./cobrust-0.2.0.vsix
```

### From a marketplace

Once published (see [PUBLISHING.md](./PUBLISHING.md) — currently user-side):

- **VSCode Marketplace**: search "Cobrust", publisher `cobrust-lang`
- **Open VSX** (preferred by Cursor + VSCodium): same search

## Settings

| Setting | Default | Description |
|---|---|---|
| `cobrust.lspPath` | `"cobrust"` | Path to the `cobrust` binary used for LSP. With `lsp.useSubcommand=true` (default), the extension spawns `<lspPath> lsp`. Absolute paths recommended for pip installs in non-activated venvs. |
| `cobrust.lsp.useSubcommand` | `true` | When `true`, spawn `cobrust lsp` (v0.6.0+ canonical path per ADR-0068). When `false`, spawn the standalone `cobrust-lsp` shim (v0.5.x compat). |
| `cobrust.dapPath` | `"cobrust"` | Path to the `cobrust` binary used for DAP. With `dap.useSubcommand=true` (default), the extension spawns `<dapPath> dap`. |
| `cobrust.dap.useSubcommand` | `true` | When `true`, spawn `cobrust dap` (v0.6.0+ canonical path per ADR-0068). When `false`, spawn the standalone `cobrust-dap` shim (v0.5.x compat). |
| `cobrust.trace.server` | `"off"` | LSP wire trace level: `off` / `messages` / `verbose`. Output appears in the "Cobrust LSP Trace" output channel. |

## Debug

The extension contributes a `cobrust` debug type. Create
`.vscode/launch.json` with:

```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "cobrust",
      "request": "launch",
      "name": "Debug current Cobrust file",
      "program": "${file}",
      "stopOnEntry": false
    }
  ]
}
```

Then press F5 (or Run → Start Debugging) with a `.cb` file open. The
extension spawns `cobrust dap` over stdio. Add `"args": ["foo", "bar"]`
to pass CLI args to the program; set `"stopOnEntry": true` to break on
the first instruction. Requires `cobrust` v0.6.0+ on `$PATH` (for the
subcommand path) — see ADR-0068.

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
# yields cobrust-0.2.0.vsix
```

## Out of scope for 0.2.0

- Bundled binary (kept external per ADR-0067 §Options)
- REPL embed inside VSCode terminal
- Inline diagnostic decoration beyond what LSP / DAP publish by default

See [ADR-0067](../../docs/agent/adr/0067-vscode-cursor-extension.md) and
[ADR-0068](../../docs/agent/adr/0068-single-binary-subcommand-collapse.md)
for the full design rationale (scaffold + subcommand collapse).

## License

Apache-2.0 OR MIT (dual, per ADR-0001).
