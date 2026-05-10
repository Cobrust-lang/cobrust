<!-- README-public.md — paste this to Cobrust/README.md, replacing current content. -->

<div align="center">

# Cobrust

**AI-native compiler that auto-translates Python libraries into verified Rust.**

*Cobra 🐍 + Rust 🦀 — Python ergonomics, Rust safety, zero migration cost.*

[![CI](https://github.com/Cobrust-lang/cobrust/actions/workflows/ci.yml/badge.svg)](https://github.com/Cobrust-lang/cobrust/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0%20%2F%20MIT-blue.svg)](#license)
[![Stage](https://img.shields.io/badge/stage-0.1.0--beta-orange.svg)](https://github.com/Cobrust-lang/cobrust/releases)

[**Why Cobrust?**](docs/post/why-cobrust.md) ·
[**Quick Start**](#quick-start) ·
[**Examples**](examples/) ·
[**Roadmap**](docs/agent/adr/0038-phase-f-roadmap.md) ·
[**Discussions**](https://github.com/Cobrust-lang/cobrust/discussions)

</div>

---

## ⚡ 30-second demo

```bash
# Install
$ cargo install cobrust-cli

# Translate a Python library to verified Rust
$ cobrust translate tomli
[L0] Spec extracted from tomli 2.0.1
[L1] Translating with claude-opus-4-7 (consensus n=2)
[L2.build]    cargo build:  0 errors, 0 warnings
[L2.behavior] differential testing 1000 inputs:  1000/1000 strict PASS
[L2.perf]     0.92x baseline (within 0.8x gate)
[L3] Downstream: pip-tools tomli usage compiles + tests pass

# Drop-in replace in Python
$ pip install ./cobrust-tomli
$ python -c "import tomli; print(tomli.loads('foo=1'))"
{'foo': 1}    # transparently backed by verified Rust now
```

That's it. Existing Python code unchanged, **9–14× faster on tomli
(T1.1 measured vs CPython 3.11 tomllib, see ADR-0039)**, memory-safe.
Other libraries pending Phase F.1 perf gates.

---

## What is Cobrust

Cobrust is **two halves co-designed**:

1. **A statically-typed language** — Python ergonomics (indentation, comprehensions, f-strings, decorators, structural pattern matching), Rust semantics (ownership, `Result<T, E>`, no GIL, no implicit truthiness, no mutable defaults). Compiles via Cranelift to native binaries.

2. **An AI-native compiler** — closed-loop translation pipeline: spec extraction → consensus translation → build/behavior/perf gates → downstream-dep validation. Uses LLMs as a first-class compiler component.

The wedge: **AI translates the existing Python ecosystem into Cobrust automatically.** No rewrite, no annotations, no manual port. Drop-in `pip install` of a verified Rust replacement.

> Like Mojo, but the AI translates the existing Python ecosystem **for you**.
> Like PyO3, but the **compiler** writes the Rust **for you, with verification**.
> Like Cython, but **no annotations**.

---

## Quick Start

### Install

```bash
# Via cargo (Rust toolchain required, 1.94+)
cargo install cobrust-cli

# Or download a prebuilt binary for macOS arm64 / Linux x86_64
curl -L https://github.com/Cobrust-lang/cobrust/releases/latest/download/cobrust-$(uname -sm | tr ' ' '-').tar.gz | tar xz
mv cobrust /usr/local/bin/
```

### Hello world

```bash
$ cobrust new hello && cd hello
$ cat src/main.cb
fn main() -> i64:
    print("hello, world")
    return 0

$ cobrust run src/main.cb
hello, world
```

### Real algorithm — recursive fib

```bash
$ cat src/main.cb
fn fib(n: i64) -> i64:
    if n < 2:
        return n
    return fib(n - 1) + fib(n - 2)

fn main() -> i64:
    print("fib(10) =")
    print_int(fib(10))
    return 0

$ cobrust run src/main.cb
fib(10) =
55
```

### Translate a Python library (the headline feature)

```bash
# Translate tomli to verified Rust + PyO3 wrapper
$ cobrust translate tomli

# Use the result transparently from Python
$ pip install ./cobrust-tomli
$ python -c "import tomli; tomli.loads('key = \"value\"')"
{'key': 'value'}
```

The translation pipeline gates each phase:
- **L0 spec extraction** — LLM reads source + tests, emits machine-readable spec
- **L1 translation** — function-level, bottom-up, consensus mode (multiple models, majority vote)
- **L2 verification** — build + behavior (1000 differential fuzz inputs vs CPython oracle) + perf (≥ 0.8× baseline)
- **L3 integration** — PyO3 wrapper + downstream-dep validation (libraries that use the translated lib must still pass their tests)

Every translation carries a **provenance manifest** — source SHA, model fingerprints, oracle artifacts, divergences. Reproducible bit-for-bit.

---

## Status

**0.1.0-beta** — first public release. Means:

- ✅ Compiler core (lexer / parser / HIR / type checker / MIR / Cranelift codegen) is solid; 2,500+ tests pass on `cargo test --workspace --locked`, zero clippy warnings under `-D warnings`
- ✅ Standard library: io / collections / string / math / panic / env / fmt / iter + structured concurrency runtime (M13)
- ✅ Package format: `cobrust.toml`, content-addressed registry, deterministic lockfile
- ✅ AI translation pipeline: production-validated on stateless + stateful tomli functions (real LLM, 12/12 + 14/14 strict deterministic over 5 runs)
- 🚧 Translated libraries: **tomli** is the canonical demo. dateutil / msgpack / numpy / requests / click are partial (synthetic-mode in places). See [translation status](docs/agent/findings/translator-real-vs-synthetic-status.md).
- 🚧 Tooling: REPL is stub-quality (M14), no LSP yet, no debugger, no WASM target. All on roadmap.
- 🚧 Self-hosting: 0%. Constitution §4.4 commits to start with type checker + AST printer; Phase F.

**What this means**: Cobrust is **mechanism-validated**. The translation pipeline works on real LLMs with real Python libraries. We are not yet **production-validated** for full PyPI ecosystem replacement. 0.1.0-beta is "we have something that demonstrably works on tomli; help us widen it."

See the [Phase F roadmap (ADR-0038)](docs/agent/adr/0038-phase-f-roadmap.md) for what's next.

---

## Examples

Progressive examples in [`examples/`](examples/):

| | |
|---|---|
| `examples/hello.cb` | minimal hello world |
| `examples/fizzbuzz.cb` | control flow (real `if/elif/else` + `%`) |
| `examples/fib.cb` | recursion via `Constant::FnRef` Call lowering |
| `examples/wc.cb` | file IO + iteration |
| `examples/cat.cb` | stream file to stdout |
| `examples/echo.cb` | argv echo |
| `examples/sort.cb` | sort lines from stdin |
| `examples/unique_lines.cb` | deduplicate lines |
| `examples/regex_grep.cb` | regex filter over stdin |
| `examples/csv_sum.cb` | aggregate a CSV column |
| `examples/json_pretty.cb` | pretty-print JSON |
| `examples/notebook/` | multi-module package |
| `examples/notebook-config/` | sibling package (path dependency) |

---

## Architecture (one paragraph)

Frontend (lexer → parser → AST → unparse round-trip) is recursive descent + Pratt parser, in pure Rust. AST → HIR (desugared, name-resolved) → typed-HIR (bidirectional type checker, no `dyn`, no implicit truthiness, exhaustive match) → MIR (control-flow-explicit, drop-schedule, borrow-check obligations discharged) → Codegen (Cranelift dev / LLVM stub for `--release`) → linker (system `cc` or `lld`).

The AI translation subsystem is **a first-class compiler component**, not a plugin. It consumes Python source + tests, dispatches to an LLM router (provider-agnostic — Anthropic, OpenAI-compatible, local vLLM all just work), and emits Cobrust source which re-enters the main pipeline. Every gate is mandatory; failure routes back to repair.

Full diagram: [docs/human/en/architecture.md](docs/human/en/architecture.md).

---

## Roadmap

**Phase E — DONE** (M0..M14): language core, codegen, package format, REPL stub.

**Phase F.1 — Now** (0.1.0-beta to 0.2.x):
- Translation ecosystem expansion (tomli → textwrap → base64 → urllib.parse → tomllib)
- Self-hosting kickoff (AST printer in Cobrust)
- LSP M0 (hover + go-to-definition)

**Phase F.2 — Next year**:
- Debugger (`cobrust debug`)
- WASM target
- LSP M1 (full diagnostics)
- Top-100 PyPI translation push

**Phase F.3 — 5 years**:
- 70%+ of compiler self-hosted in Cobrust
- Top-1000 PyPI auto-translated, in registries
- LSP / debugger / build tooling at parity with Cargo

Full timetable + criteria: [ADR-0038 Phase F roadmap](docs/agent/adr/0038-phase-f-roadmap.md).

---

## Contributing

We need:
- More translated libraries (see `good-first-issue` label for starter targets)
- LSP work (huge, foundational)
- Cross-arch validation (windows-x86_64, linux-aarch64)
- AI router adapters (more model backends)

See [CONTRIBUTING.md](CONTRIBUTING.md). Code of Conduct: [Contributor Covenant](CODE_OF_CONDUCT.md).

Joining: [GitHub Discussions](https://github.com/Cobrust-lang/cobrust/discussions) for design Qs, [Issues](https://github.com/Cobrust-lang/cobrust/issues) for bugs and feature requests.

---

## License

Dual-licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. See [ADR-0001](docs/agent/adr/0001-license.md) for rationale.

---

## Acknowledgements

Cobrust stands on the shoulders of:
- **Cranelift** — the codegen IR and backend, in pure Rust
- **Mojo / Pyston / Cinder / Cython** — earlier Python performance projects whose lessons we built on
- **PyO3** — the Rust↔Python FFI binding we ship in translation outputs
- **Anthropic / OpenAI / DeepSeek** — LLM providers powering the translation pipeline
- The **Rust** community — for the safety + performance that makes Cobrust possible

---

<div align="center">

**Cobrust 0.1.0-beta** — built in public, by AI agents working with humans.
*If you tried it, tell us what broke.*

</div>
