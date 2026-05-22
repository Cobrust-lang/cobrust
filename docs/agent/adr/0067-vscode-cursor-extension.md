---
doc_kind: adr
adr_id: 0067
title: VSCode / Cursor editor extension (TextMate grammar + LSP client)
status: accepted
date: 2026-05-22
last_verified_commit: <closure-sprint-final>
supersedes: []
superseded_by: []
---

# ADR-0067: VSCode / Cursor editor extension (TextMate grammar + LSP client)

## Context

- Cobrust v0.5.1 ships with `cobrust-lsp` v1.3 — feature-complete LSP server with 13
  handlers (hover, completion, goto-def, references, rename, code-action, semantic
  tokens, inlay hints, call hierarchy, diagnostic publish, formatting hook,
  workspace symbols, signature help). See ADR-0057a + skill §9c.
- TextMate grammar `cobrust.tmLanguage.json` already drafted at
  `docs/agent/outreach/cobrust.tmLanguage.json` (originally authored as a
  GitHub Linguist contribution; covers comments, f-strings, decorators with
  `@py_compat` tier highlight, keywords, primitive + ADT types, operators,
  PRELUDE call sites).
- v0.5.1 distribution (cargo + 9 prebuilt wheel variants per ADR-0065) installs
  the `cobrust-lsp` binary into a stable path. The binary is already on `$PATH`
  after any standard install.
- The remaining onboarding gap: a new user installs cobrust, opens a `.cb`
  file in VSCode or Cursor, and sees plain-text with no diagnostics. To close
  this gap we need an editor extension that (a) registers the grammar and (b)
  launches `cobrust-lsp` as a Language Client.
- Cursor (the AI editor) is API-compatible with VSCode extensions, as is
  VSCodium and Codespaces. One extension covers all three.

## Why now

- LSP v1.3 + DAP v1.2 both feature-complete (Phase J + Phase L closed at v0.5.0).
- Public release (v0.5.0) is LIVE on PyPI as 9 prebuilt wheels (ADR-0065).
- §2.5 LLM-first principle: an AI agent writing Cobrust will paste their `.cb`
  buffer into Cursor and expect inline diagnostics. Today they get nothing.
- The training-data-overlap principle (§2.5 D) also applies to editor UX: VSCode
  is the most common editor in LLM training corpora. Matching VSCode UX
  patterns reduces surprise.

## Options considered

1. **Standalone VSCode extension (TS) wrapping `cobrust-lsp` via stdio** — well-
   trodden path. `vscode-languageclient/node` handles the LSP wire protocol;
   the extension's job is reduced to: load grammar, spawn the binary, declare
   document selector. Same artefact (`.vsix`) installs into VSCode, Cursor,
   VSCodium, Codespaces, code-server.
2. **Bundle `cobrust-lsp` binary inside the .vsix** — avoids the user
   needing to `cargo install` first. But: (a) binary is platform-specific (we
   would need 9 .vsix variants matching the 9 wheel variants), (b) extension
   size balloons from ~50 KB to ~30 MB per variant, (c) updates require
   shipping a new .vsix for every LSP bug-fix even if grammar unchanged.
   **Rejected** for wave-1; revisit if user friction is high.
3. **Write the extension in Cobrust** — premature; the VSCode Extension API is
   pure JS/TS today, and Cobrust → JS codegen is M11+ (not on the roadmap).
   **Rejected** for wave-1.
4. **Skip VSCode extension; rely on generic LSP wiring (e.g., Neovim)** —
   misses the most common editor; misses Cursor entirely.
   **Rejected.**

## Decision

We author a TS VSCode extension at `editors/vscode-cobrust/` that:

- registers the `cobrust` language with `.cb` file extension,
- ships the TextMate grammar (file-copy from `docs/agent/outreach/` — the
  outreach copy remains canonical; the editor copy is allowed to diverge if
  Linguist tightens its grammar requirements; the divergence is recorded in
  `editors/vscode-cobrust/CHANGELOG.md`),
- ships a `language-configuration.json` with Python-like indent rules,
  bracket pairs, `#` line comments,
- ships snippets for `fn` / `if` / `for` / `class` / `match` skeletons,
- on activation, spawns `cobrust-lsp` from `$PATH` (configurable via
  `cobrust.lspPath` setting), wires stdio LSP transport.

The extension is published to:

- **VSCode Marketplace** under publisher `cobrust-lang`
  (`cobrust-lang.cobrust`),
- **Open VSX** (preferred by Cursor and VSCodium),
- **GitHub releases** as a `.vsix` attached to the release tag for offline
  install.

## Scope

### Wave-1 (this sprint)
- `editors/vscode-cobrust/` scaffold (package.json + tsconfig + src/extension.ts)
- TextMate grammar bundled (`syntaxes/cobrust.tmLanguage.json` — file-copy
  from outreach dir)
- `language-configuration.json` — brackets, auto-close, comments
- `snippets/cobrust.json` — fn / if / for / class / match
- LSP client wire-up: spawn `cobrust-lsp` from PATH, fallback to
  `cobrust.lspPath` setting
- README + PUBLISHING.md (publisher account + PAT documentation — user-side)
- `.vsix` build verified locally (if `node` available); otherwise build
  deferred to user-side with documented `npm install && npx vsce package`
  steps

### Wave-1 explicitly out-of-scope (OOS)
- **DAP integration** — Phase L wave-6 follow-up (separate ADR);
  `cobrust-dap` binary exists but no `launch.json` debugger configuration
  contribution here.
- **Debug launcher** (F5 → run current .cb file) — OOS until DAP wave-6.
- **Inline diagnostic decoration beyond LSP default** — the LSP server already
  publishes diagnostics; we don't add custom hover decorations on top.
- **REPL embed inside VSCode terminal** — OOS (separate ADR; integration with
  `cobrust repl` is non-trivial).
- **Bundled binary** — rejected per options-2 above.
- **Actual `vsce publish`** — user-side action. Publishing requires Azure
  DevOps PAT + Marketplace publisher account creation. ADR-0067 stages
  everything up to the point of `vsce login`; user runs the final publish.

### Cursor support
- Cursor is binary-compatible with VSCode extensions (uses VSCode Extension
  API verbatim).
- Install path: `cursor --install-extension ./cobrust-0.1.0.vsix`
- Open VSX is Cursor's default registry, so post-Open-VSX publish Cursor
  users get the extension via in-editor marketplace UI.
- No Cursor-specific code in `extension.ts`.

### Distribution channels
- **GitHub releases** — `.vsix` attached to each `v0.X.Y` release tag.
- **VSCode Marketplace** — `cobrust-lang.cobrust` (publisher account =
  user-side action, see PUBLISHING.md).
- **Open VSX** — same `.vsix`, published with `ovsx publish`. Required for
  Cursor + VSCodium auto-update.

## Consequences

- **Positive**
  - One install (`code --install-extension cobrust.vsix` or marketplace
    click-install) brings syntax highlighting + LSP + snippets.
  - Cursor users get the same experience for free.
  - The TextMate grammar canonical source remains in
    `docs/agent/outreach/` (Linguist PR target), but the editor copy is
    versioned independently — extension can ship grammar fixes faster than
    Linguist's monthly cycle.
- **Negative**
  - Extension version drift from compiler version: extension v0.1.0 ships
    with v0.5.1 LSP. Future LSP protocol breaks would require coordinated
    bumps. Mitigated by `cobrust.lspPath` setting + version-skew warning.
  - User must have `cobrust-lsp` on `$PATH`. If `cobrust` is installed via
    pip and the venv is not activated, LSP fails to launch. Mitigated by
    error message in `activate()` + README troubleshooting section.
  - Marketplace publish is not automated; user must hold the PAT.
- **Neutral / unknown**
  - Per-file LSP startup latency: cold-start is ~200 ms (Phase J benchmarks).
    Acceptable.
  - Snippet set is minimal (5 templates). May want to expand to match
    LeetCode patterns (see skill §1) in a follow-up.

## Evidence

- LSP feature inventory: `crates/cobrust-lsp/src/` (13 handlers)
- TextMate grammar source: `docs/agent/outreach/cobrust.tmLanguage.json`
- Skill §9c (LSP integration) and §9j (v0.5.0 install paths)
- ADR-0057a (LSP v1.3 closure)
- ADR-0059a-g (DAP v1.2 — Phase L wave-1 through wave-5)
- ADR-0065 (Tier-3 prebuilt wheel distribution → 9 variants on PyPI)
- VSCode extension API reference:
  https://code.visualstudio.com/api/references/extension-manifest
- vscode-languageclient/node: stable since 2019, used by rust-analyzer,
  pyright, tsserver, etc.
- Open VSX rationale (Cursor default registry):
  https://github.com/eclipse/openvsx
