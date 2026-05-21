# Cobrust v0.4.0 — v1 LSP server + Tier 1/2/3 W1 hardware tiering + Phase L FULL CLOSED

**Released:** 2026-05-21  
**Commits since v0.3.0:** 338  
**Lines of code:** ~49K LOC (estimated)  
**LC-100 score:** 100/100 (maintained)

---

## External-user-scenario binding (ADR-0045 mandate)

- **Cursor / Continue / Cody web-UI users** editing `.cb` files now get hover, completion, rename, and auto-diagnostics on every keystroke via `cobrust-lsp` (v1 LSP server — Phase J wave-2 FULL CLOSED).
- **Cobrust agents writing code** can consume `cobrust skills get cobrust-language` mid-conversation per ADR-0061, enabling LLM agents to self-describe the language surface without external lookups.

---

## Shipped phases since v0.3.0

### Phase H — Tier-2 closure
- 227+ parity tests passing
- Tier-2 security hardening: `MAX_PARSER_DEPTH = 50` + `ExpressionTooDeep` variant (P0-1), lldb_quote safe-arg-escape helper (P0-2), LLM API key scrub before lldb spawn (P1-1), cargo-audit CI job (P0-3 non-blocking advisory)

### Phase I — FULL CLOSED
- ADR-0056a: Cranelift-backed JIT engine for REPL incremental evaluation
- ADR-0056b: Session state management in REPL
- ADR-0056c: REPL function redefinition support

### Phase J wave-1 + wave-2 — FULL CLOSED (v1 LSP server)
- ADR-0057a: `publishDiagnostics` on `textDocument/didOpen` (wave-1)
- ADR-0057b: `textDocument/didChange` + Session reuse with bounded debounce (wave-2.1)
- ADR-0057c: `textDocument/hover` + `textDocument/completion` (wave-2.2) — live editor productivity
- ADR-0057d: `textDocument/rename` + `workspace/applyEdit` (wave-2.3) — WorkspaceEdit with TextEdit[]

### Phase K — FULL CLOSED
- LLVM IR core generation
- Optimization passes + multi-target support
- DWARF debug information emission
- JIT/AOT convergence
- musl Tier-1 static binary support

### Phase L — FULL CLOSED
- ADR-0059b: DAP server wave-2 (Debug Adapter Protocol)
- ADR-0059d: Linker harness + per-variant Option DI + lldb pretty-printers (wave-3)
- `cobrust debug` CLI subcommand
- lldb pretty-printers with `cobrust_option_summary` tag-dispatch
- Executable spec + linker-harness helpers in `tests/dwarf-lldb/`

### Phase M — Language-surface closure (6 gaps + 3 follow-ups)
- Dynamic-index Array support
- Language surface gap closure per `docs/human/{zh,en}/phase-m-language-surface-closure.md`

### Phase O — Tier 3 W1 SHIPPED
- ADR-0065: 9-wheel CPU-level matrix in `release.yml` (linux-x86_64-generic, linux-x86_64-v2, linux-x86_64-v3, linux-x86_64-v4, linux-aarch64, linux-x86_64-musl, linux-aarch64-musl, darwin-arm64, darwin-x86_64)
- W2-W4 (`cobrust install` subcommand, auto-detect, checksum verify) queued for v0.5.0

---

## New ADRs (8 since v0.3.0)

| ADR | Title |
|-----|-------|
| ADR-0057a | Phase J wave-1 publishDiagnostics |
| ADR-0057b | Phase J wave-2.1 didChange + Session reuse |
| ADR-0057c | Phase J wave-2.2 hover + completion |
| ADR-0057d | Phase J wave-2.3 rename |
| ADR-0058e | AOT cranelift_backend substrate delegation (closes 0058d §2.3 deferral) |
| ADR-0061 | `cobrust skills` subcommand |
| ADR-0062 | FixSafety ladder |
| ADR-0064 | Print monomorphization (resolves F38 source-surface leakage) |
| ADR-0065 | Tier 3 prebuilt multi-wheel distribution spec |

---

## F-pattern intel (findings registry)

- **F35-sibling**: no sibling commits on any commit in this release (compliant)
- **F36**: tracked
- **F37**: `check-exit-code-borrow-gap` — 1 `#[ignore]` retained with finding-id cite
- **F38**: print monomorphization source-surface leakage — RESOLVED via ADR-0064
- **F39**: device-name redaction — second-pass applied (3 files); Privacy emergency rewrite Option A force-push scheduled 2026-05-19 (post-tag)
- **F40**: DG abandonment (heavy build policy; Mac per-crate verify only)
- **F41–F43**: ADSD PR #1 + #2 staged

---

## Tier-1 binary install paths (4 platforms)

Pre-built release binaries published per tagged release:

| Platform | Binary |
|----------|--------|
| `linux-x86_64-gnu` | `cobrust-linux-x86_64` |
| `linux-x86_64-musl` | `cobrust-linux-x86_64-musl` |
| `linux-aarch64` | `cobrust-linux-aarch64` |
| `darwin-arm64` | `cobrust-darwin-arm64` |

## Tier-3 wheel distribution (9 variants, W1 SHIPPED)

9 wheel files built per tagged release via the CPU-level matrix in `release.yml`. Install via:

```bash
pip install cobrust  # selects best wheel for your CPU
```

---

## Known issues / honest debt

- **Phase J wave-3** (go-to-def + codeAction + cross-file rename) — proposed, not shipped; queued for v0.5.0
- **Phase L §6.1** Str runtime full closure — deferred to ADR-0059e
- **Tier 3 W2-W4** (`cobrust install` subcommand, auto-detect, checksum verify) — queued for v0.5.0
- **Cluster A**: 1 honest-deferred `#[ignore]` test retained with finding-id cite
- **Cluster B**: 2 honest-deferred `#[ignore]` tests retained with finding-id cites
- **f3ls list[str] gap**: 2 items deferred
- **Code Quality P0**:
  - `doc_markdown` Clippy lint: 785 warnings; workspace-allow applied (lint set to `allow` in `[workspace.lints.clippy]`); bulk fix queued for v0.5.0
  - `cobrust-dap` test density: 0.15 (below 0.3 target); additional test coverage queued
  - `cobrust-frontend/types/mir`: §B gap (ADR-0051 §2.5 error-UX fix suggestions not yet printed)
- **5 honest-deferred `#[ignore]` tests** retained with finding-id cites per audit SOP

---

## Upgrade path from v0.3.0

No breaking changes to the Cobrust language surface. The LSP server (`cobrust-lsp`) is a new binary; add it to your editor config:

```json
{
  "languageserver": {
    "cobrust": {
      "command": "cobrust-lsp",
      "filetypes": ["cobrust"]
    }
  }
}
```

For DAP (debugging), add `cobrust-dap` as your debug adapter in your editor's DAP config.

---

## Full changelog

```
git log v0.3.0..v0.4.0 --oneline
```
