# Cobrust

> Cobra 🐍 + Rust 🦀

A Rust-implemented Python successor with an AI-native compiler that
closed-loop translates the entire Python ecosystem.

## Status

**Phase E complete @ d178a3f (M11.1 spirit-met).** As of 2026-05-09
(HEAD `b83ea80`): 31 ADRs accepted, 2,430+ tests passing on cold
integrated rebuild. The language + runtime fully compose (`.cb` → AST
→ HIR → typed-HIR → MIR → Cranelift → Mach-O on macOS arm64). M11.1
(ADR-0030) restored real algorithmic fizzbuzz via `while` + `if/elif`
+ modulo. M13 (ADR-0028) wired structured concurrency. M14 (ADR-0029)
shipped the interactive REPL with `:type/:ast/:hir/:mir` directives.
The AI translation subsystem delivers a synthetic-LLM mode pipeline
with five translated libraries and the `cobrust-numpy` numerical tier
(M7.0..M7.6). A 16-module stdlib and a content-addressed package
format ship at M11 + M12.

**Translation pipeline status** (per
[ADR-0019 §"Three-tier anchor"](docs/agent/adr/0019-phase-e-language-runtime-roadmap.md)):

- **Default mode**: synthetic-LLM (hand-authored response tables for
  determinism); real-LLM end-to-end audit queued per
  [`finding:translator-real-vs-synthetic-status`](docs/agent/findings/translator-real-vs-synthetic-status.md).
- **Verified**: LLM Router wire protocol (OpenAI-compatible adapter,
  cache, ledger, retry isolation) against live endpoint per
  [`finding:m5-m7-real-llm-validation`](docs/agent/findings/m5-m7-real-llm-validation.md).
- **Known limitation**: closed-loop L0→L1→L2→L3 verification has never
  executed on a real Python library. Audit #1 (real-LLM `tomli` E2E)
  in flight to validate the repair loop under real diagnostic
  feedback vs. synthetic canned responses.

See
[milestones (en)](docs/human/en/milestones.md) /
[里程碑 (zh)](docs/human/zh/milestones.md) for the full roadmap.

## Quick start (5 steps)

### 1. Clone

```bash
git clone https://github.com/cobrust/cobrust
cd cobrust
```

### 2. Build

```bash
cargo build --workspace
```

Produces `target/debug/cobrust` — the compiler CLI.

### 3. Hello world

Create `hello.cb`:

```cobrust
fn main() -> i64:
    print("hello, world")
    return 0
```

Compile and run:

```bash
./target/debug/cobrust build hello.cb
./hello
```

### 4. Real algorithm: FizzBuzz

Create `fizzbuzz.cb`:

```cobrust
fn main() -> i64:
    let n: i64 = 1
    while n <= 15:
        if n % 15 == 0:
            print("FizzBuzz")
        elif n % 3 == 0:
            print("Fizz")
        elif n % 5 == 0:
            print("Buzz")
        else:
            print_int(n)
        n = n + 1
    return 0
```

Compile and run:

```bash
./target/debug/cobrust build fizzbuzz.cb
./fizzbuzz
```

This demonstrates real Cobrust: `while` loops, `if/elif/else` branching,
modulo arithmetic, and mutable bindings (M11.1 enablement, ADR-0030).

### 5. Interactive REPL

```bash
./target/debug/cobrust repl
```

Try:

```
> let x: i64 = 42
> :type x
> let y: i64 = x + 1
> print_int(y)
> :hir let y
> :quit
```

Directives: `:type <var>`, `:ast`, `:hir <stmt>`, `:mir <stmt>`, `:clear`, `:help`.

For more, see [Getting started](docs/human/en/getting-started.md).

## Documentation

- 中文文档: [`docs/human/zh/`](docs/human/zh/README.md)
- English docs: [`docs/human/en/`](docs/human/en/README.md)
- Agent docs (LLM-facing): [`docs/agent/`](docs/agent/README.md)

## Constitution

The project's design constitution lives in [`CLAUDE.md`](CLAUDE.md). It
binds all engineering work and AI-agent contributions. When intuition
disagrees with the constitution, the constitution wins.

## License

Dual-licensed under either of:

- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT license ([`LICENSE-MIT`](LICENSE-MIT))

at your option. See
[ADR-0001](docs/agent/adr/0001-license.md) for rationale.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual-licensed as above, without any additional terms
or conditions.
