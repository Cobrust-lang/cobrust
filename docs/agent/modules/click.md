---
doc_kind: module
module_id: mod:click
crate: cobrust-click
last_verified_commit: TBD
dependencies: [mod:translator]
---

# Module: click

## Purpose

Cobrust translation of `click` 8.1.7 — the M-batch ecosystem
deliverable per ADR-0022. Surface-translates Python's
decorator-heavy CLI parsing library onto Rust's `clap = "4"`. The
translation challenge (per ADR-0022 §3) is mapping decorator chains
(`@click.command / @click.option / @click.argument`) to Rust's
fluent builder API, and then lowering that to clap's `Arg / Command`
under the hood.

## Status

- **M-batch — delivered.** All 16 functions translated via the
  synthetic-LLM pipeline (`Command::new / about / option / argument /
  run / option_count / argument_count / about_text / name` +
  `OptionSpec::new / short / type_ / default / help / required / name` +
  `ArgumentSpec::new / type_ / optional / name` + `RunResult::option /
  argument / option_count / argument_count`). Backend bound to
  `clap = "4"` (no proc-macro / no derive — the M-batch sprint
  surfaces a fluent runtime API; M9+ may add a derive-mode companion).

## Public surface (M-batch)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamType { Str, Int, Bool, Float }

pub struct OptionSpec { /* private fields */ }

impl OptionSpec {
    pub fn new(long: impl Into<String>) -> Self;
    pub fn short(self, short: impl Into<String>) -> Self;
    pub fn type_(self, p: ParamType) -> Self;
    pub fn default(self, value: impl Into<String>) -> Self;
    pub fn help(self, help: impl Into<String>) -> Self;
    pub fn required(self) -> Self;
    pub fn name(&self) -> &str;
}

pub struct ArgumentSpec { /* private fields */ }

impl ArgumentSpec {
    pub fn new(name: impl Into<String>) -> Self;
    pub fn type_(self, p: ParamType) -> Self;
    pub fn optional(self) -> Self;
    pub fn name(&self) -> &str;
}

pub struct Command { /* private fields */ }

impl Command {
    pub fn new(name: impl Into<String>) -> Self;
    pub fn about(self, help: impl Into<String>) -> Self;
    pub fn option(self, opt: OptionSpec) -> Self;
    pub fn argument(self, arg: ArgumentSpec) -> Self;
    pub fn run<I, T>(&self, argv: I) -> Result<RunResult, ClickError>
        where I: IntoIterator<Item = T>, T: Into<String>;
    pub fn name(&self) -> &str;
    pub fn about_text(&self) -> Option<&str>;
    pub fn option_count(&self) -> usize;
    pub fn argument_count(&self) -> usize;
}

pub struct RunResult { /* private fields */ }

impl RunResult {
    pub fn option(&self, name: &str) -> Option<&str>;
    pub fn argument(&self, name: &str) -> Option<&str>;
    pub fn option_count(&self) -> usize;
    pub fn argument_count(&self) -> usize;
}

#[derive(Clone, Debug)]
pub struct ClickError { pub kind: ClickErrorKind, pub message: String }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickErrorKind { UsageError, MissingOption, MissingArgument, InvalidValue }
```

## Scope window (M-batch)

In scope:

- `@click.command(name=..., help=...)` decorator → `Command::new(name).about(help)`.
- `@click.option('--flag', type=int|str|bool, default=..., help=..., required=...)` →
  `OptionSpec::new('--flag').type_(...).default(...).help(...).required()`.
- `@click.argument('NAME', type=...)` with optional flag.
- `command.run(argv)` returns `RunResult` keyed by parameter name.
- Single error type with closed enum variant.

Out of scope (M9+):

- `@click.group` umbrella commands.
- `Choice / Path / File / IntRange / DateTime / UUID` parameter types.
- `Context.invoke` / `Context.forward` / parent-context inheritance.
- Autocompletion + shell completion scripts.
- Prompts (`prompt=True`) and `confirmation_option`.

## Decorator-translation table (per ADR-0022 §3)

| Python decorator | Rust translation |
|---|---|
| `@click.command(name='echo')` | `Command::new("echo")` |
| `@click.command(help='emit')` | `Command::new(...).about("emit")` |
| `@click.option('--name', default='world')` | `OptionSpec::new("name").default("world")` |
| `@click.option('-n', '--name')` | `OptionSpec::new("name").short("n")` |
| `@click.option('--count', type=int)` | `OptionSpec::new("count").type_(ParamType::Int)` |
| `@click.option('--loud', is_flag=True)` | `OptionSpec::new("loud").type_(ParamType::Bool)` |
| `@click.option('--api-key', required=True)` | `OptionSpec::new("api-key").required()` |
| `@click.argument('SRC')` | `ArgumentSpec::new("src")` |
| `@click.argument('DST', nargs=-1)` | (M9+ — variadic args) |
| `command(['echo', '--name', 'ada'])` | `cmd.run(vec!["echo", "--name", "ada"])` |

## Invariants

- **No silent translations.** Every emitted file carries a
  provenance header; every per-function emission carries a
  per-function provenance line.
- **Closed error taxonomy.** Every failure routes to one of four
  `ClickErrorKind` variants.
- **Decorator-chain order is irrelevant.** `cmd.option(a).option(b)`
  parses the same argv as `cmd.option(b).option(a)`.
- **Required options fail with `MissingOption`.** Missing required
  positional arguments fail with `MissingArgument`.

## Gates (M-batch — none optional)

| Stage | Gate | Pass criteria | Status |
|---|---|---|---|
| L0 | spec produced | `corpus/click/spec.toml` + harness committed | ✅ |
| L1 | code emitted | every file has provenance header + per-fn task tag | ✅ |
| L2.build | `cargo build --release` | zero warnings | ✅ |
| L2.behavior | argv matrix + decorator-chain fuzz | ≥ 1000 panic-free inputs across 3 seeds | ✅ |
| L2.perf | binding-overhead bench | surface-translate / Rust-binding tier 0.8× per ADR-0022 §6 | ✅ |
| L3.pyo3 | PyO3-shaped wrapper | `--features pyo3` compiles per ADR-0011 | ✅ |
| L3.dependents | (deferred to M9 per ADR-0022 §"Negative consequences") | typer/flask-cli/rich-click wait for runtime ADR | deferred 3/3 |

## Translation provenance

Written to `crates/cobrust-click/PROVENANCE.toml`. Schema per
ADR-0007 §3 + ADR-0022:

```toml
[source]
library = "click"
version = "8.1.7"

[gates]
l3_downstream_dependents = "deferred to M9 per ADR-0022 §"Negative consequences""

[gates.dependents]
covered = []
deferred = ["typer", "flask-cli", "rich-click"]
deferred_reason = "ADR-0022 §"Negative consequences""
```

## Done means (M-batch — DONE)

- [x] All 16 spec functions translated (Command + OptionSpec +
      ArgumentSpec + RunResult).
- [x] L0 spec + canned table + harness committed at `corpus/click/`.
- [x] L2.behavior argv matrix: 11 representative cases (defaults,
      explicit options, short forms, type validation, missing
      required, unknown options, optional positionals).
- [x] L2.behavior fuzz: ≥ 1000 inputs × 3 seeds; decorator-chain
      synthesis panic-free.
- [x] L2.perf gate: surface-translate / Rust-binding tier (0.8×)
      per ADR-0022 §6.
- [x] L3.pyo3 wrapper + `--features pyo3` build path wired per
      ADR-0011.

## Done means (M9+ — open)

- [ ] `@click.group` umbrella commands + sub-command dispatch.
- [ ] `Choice / Path / File / IntRange / DateTime / UUID` parameter types.
- [ ] Parent-context inheritance (`Context.invoke / Context.forward`).
- [ ] Derive-mode companion crate (`#[derive(CobrustCommand)]`).
- [ ] Downstream-dependent crates: typer + flask-cli + rich-click subsets.

## Non-goals

- **Not** a complete `click` implementation — see "Scope window".
- **Not** hand-written. Editing `src/decorators.rs` directly is
  forbidden; regenerate via the pipeline.
- **Not** a clap derive-macro replacement — the M-batch surface is
  fluent (runtime). M9+ may add a derive companion.

## Cross-references

- `mod:translator` — pipeline that emits this crate.
- `mod:requests` — sister M-batch crate (HTTP client).
- [adr:0022](../adr/0022-translation-ecosystem-batch.md) — M-batch methodology.
- [adr:0007](../adr/0007-translator-pipeline.md) — pipeline base.
- [adr:0011](../adr/0011-pyo3-build-path.md) — PyO3 build path.
- click upstream — https://github.com/pallets/click (BSD-3-Clause).
- clap crate — https://crates.io/crates/clap.
