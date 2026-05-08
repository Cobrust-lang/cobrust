# Corpus: click

Vendored representative subset of `click` 8.1.7 — the M-batch
ecosystem-translation deliverable per ADR-0022 §1.

## Scope window (M-batch)

- **In scope**:
  - `@click.command(name=..., help=...)` decorator.
  - `@click.option('--flag', type=int|str|bool, default=..., help=..., required=...)`.
  - `@click.argument('NAME', type=...)` with optional flag.
  - `command(argv)` runtime that returns parsed values keyed by name.
  - Single error type: `ClickError { kind, message }`.
- **Out of scope (M9+)**:
  - `@click.group` umbrella commands + sub-commands.
  - `Choice / Path / File / IntRange / DateTime / UUID` types.
  - `Context.invoke` / `Context.forward` / parent-context inheritance.
  - Autocompletion + shell completion scripts.
  - Prompts (`prompt=True`) and `confirmation_option`.

## L0 spec

`spec.toml` pins the public-surface signatures + decorator → builder
translation rules.

## Differential gate

The L3 differential test in `crates/cobrust-click/tests/
click_downstream.rs` exercises a matrix of argv shapes that mirror
the upstream click test bank's positive + negative cases. The L3 path
is pure-Rust subprocess-free because click parsing is deterministic.

## Why bind clap = "4"

Per ADR-0022 §3: clap's derive-mode (`#[derive(Parser)]`) is the
canonical Rust CLI parsing surface. Click's decorator-stack maps 1:1
onto clap's argument builder, with the difference that we surface a
fluent runtime API rather than derive macros (no proc-macro state
machine needed for the M-batch sprint; M9+ may add a derive-mode
companion crate).

## Translation provenance

Every emitted file at `crates/cobrust-click/src/` carries:

```text
// AUTO-GENERATED — DO NOT EDIT BY HAND.
// Translated by cobrust-translator (synthetic-LLM mode).
// source-library: click 8.1.7
// oracle: cpython 3.11 (module: click)
// functions translated: 16
// see PROVENANCE.toml for the full manifest.
```

Per-function provenance lines (one per translated function) follow
the M6 format:

```text
// fn:Command::option provider=synthetic model=click-canned-v1 cache_hit=false decision_id=blake3:committed-from-canned-v1 task=translate
```
