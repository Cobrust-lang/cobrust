# Cobrust v0.6.1 — patch release (2026-05-25)

Patch release on top of v0.6.0. Bug fixes + ecosystem polish.

## Highlights

### Bug fixes
- **F47** — f-string interpolation on user-fn-returned `str` previously rendered empty / decimal-format-of-pointer; now renders the actual string. Repro `print(f"{user_fn_returning_str()}")` works correctly. (commits `cf0864c` + `dcb1714`; finding `f47-fstring-user-fn-str-interp-empty.md` status RESOLVED)

### Ecosystem
- **Homebrew tap** — `brew tap cobrust-lang/cobrust && brew install cobrust` is now the recommended install path on macOS / Linux. Tap repo: <https://github.com/Cobrust-lang/homebrew-cobrust>. (commit `9853906`)
- **VSCode/Cursor extension v0.2.0** — DAP debugger integration (launch.json `"type": "cobrust"` + F5 → `cobrust dap` stdio). Subcommand-first LSP path with shim fallback (v0.6.0 uses `cobrust lsp`; v0.5.x users still work via `cobrust-lsp` shim). (commits `9a83d5c` + `10f8e3a`)

## What is NOT in v0.6.1

- Open VSX extension v0.2.0 publish — pending user-side `OVSX_PAT` provision
- LLVM backend wave-3 stubs (input / list / dict / etc; tracked in ADR-0058g, F45a)
- Homebrew formula auto-update workflow on tap repo (deferred to v0.6.x polish)

## Install paths (recommended order)

1. `brew tap cobrust-lang/cobrust && brew install cobrust`
2. `cargo install cobrust` (Rust 1.94+)
3. Manual wheel download from this release

## Cross-references

- v0.6.0 release notes: <https://github.com/Cobrust-lang/cobrust/blob/main/RELEASE_NOTES_v0.6.0.md>
- F47 finding: `docs/agent/findings/f47-fstring-user-fn-str-interp-empty.md`
- ADR-0068 subcommand collapse: `docs/agent/adr/0068-single-binary-subcommand-collapse.md`
