---
doc_kind: module
module_id: mod:cli
crate: cobrust-cli
last_verified_commit: TBD
dependencies: []
---

# Module: cli

## Purpose

`cobrust` binary entrypoint. Subcommand dispatch + global flags.

## Status

M0 — empty stub (`fn main() {}`). Subcommands wire up starting at M1
(`cobrust lex`, `cobrust parse`, `cobrust build`, etc.).

## Public surface (target — M1+)

TBD; subcommand registry to be defined in an ADR before M1 ships.

Anticipated subcommands (non-binding):

| Subcommand | Lands at | Purpose |
|---|---|---|
| `cobrust lex <file>` | M1 | Run lexer, dump tokens |
| `cobrust parse <file>` | M1 | Run parser, dump AST |
| `cobrust check <file>` | M2 | Run type checker |
| `cobrust build` | M3+ | Compile workspace |
| `cobrust translate <pylib>` | M4 | Run translation pipeline |

## Invariants

- Exits with stable, documented status codes (`0` success, non-zero per
  failure category — final scheme in pending ADR).
- All non-zero exits write a structured diagnostic via `tracing` to
  stderr.
- Never panics on invalid CLI input — invalid input → diagnostic + exit
  code, not panic.

## Done means (M0)

- [x] `cargo build -p cobrust-cli` produces `target/debug/cobrust`.
- [x] Binary exits 0 with no output.

## Done means (M1)

- [ ] `cobrust lex` and `cobrust parse` round-trip the "core 30 forms"
      from `mod:frontend`.
- [ ] `cobrust --help` lists all subcommands.

## Non-goals

- No interactive REPL in M1 (separate milestone).
- No daemon mode / persistent server — every invocation is independent.

## Cross-references

- `mod:frontend` — first subcommands operate on it.
- Future ADR — subcommand registry and exit-code scheme.
