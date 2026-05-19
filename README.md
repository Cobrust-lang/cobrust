<div align="center">

**English** · [中文](README.zh.md)

# Cobrust

**AI-friendly Python successor in Rust, with LLM-driven translation pipeline and AI-native stdlib (in development).**

*Cobra 🐍 + Rust 🦀 — Python ergonomics, Rust safety, zero migration cost.*

[![CI](https://github.com/Cobrust-lang/cobrust/actions/workflows/ci.yml/badge.svg)](https://github.com/Cobrust-lang/cobrust/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0%20%2F%20MIT-blue.svg)](#license)
[![Stage](https://img.shields.io/badge/stage-0.3.0-orange.svg)](https://github.com/Cobrust-lang/cobrust/releases)

[**Why Cobrust?**](docs/post/why-cobrust.md) ·
[**Quick Start**](#quick-start) ·
[**Examples**](examples/) ·
[**Roadmap**](docs/agent/adr/0054-post-phase-g-roadmap.md) ·
[**Discussions**](https://github.com/Cobrust-lang/cobrust/discussions)

</div>

---

## ⚡ 30-second demo

```bash
# Install (build from source)
$ cargo install --git https://github.com/Cobrust-lang/cobrust cobrust-cli

# Translate a Python library to verified Rust
$ cobrust translate tomli
[L0] Spec extracted from tomli 2.0.1
[L1] Translating with codex gpt-5.5
[L2.build]    cargo build:  0 errors, 0 warnings
[L2.behavior] differential testing 1024 inputs:  99.71% strict PASS
[L2.perf]     1KB 13.8x / 100KB 10.8x / 10MB 9.05x faster than CPython tomllib (ADR-0039)
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

2. **An LLM-driven translation subsystem** — closed-loop pipeline: spec extraction → consensus translation → build/behavior/perf gates → downstream-dep validation. Uses LLMs as a first-class compiler component.

The wedge: **AI translates the existing Python ecosystem into Cobrust automatically.** No rewrite, no annotations, no manual port. Drop-in `pip install` of a verified Rust replacement.

> Like Mojo, but the AI translates the existing Python ecosystem **for you**.
> Like PyO3, but the **compiler** writes the Rust **for you, with verification**.
> Like Cython, but **no annotations**.

---

## Quick Start

### Install

```bash
# Via cargo (Rust toolchain required, 1.94+)
cargo install --git https://github.com/Cobrust-lang/cobrust cobrust-cli
# (crates.io publish queued for v0.3.0)

# Or download a prebuilt binary (tier-1 targets per ADR-0046)
# macOS arm64
curl -L https://github.com/Cobrust-lang/cobrust/releases/latest/download/cobrust-v0.3.0-aarch64-apple-darwin.tar.gz | tar xz
sudo mv cobrust /usr/local/bin/
# Linux arm64
curl -L https://github.com/Cobrust-lang/cobrust/releases/latest/download/cobrust-v0.3.0-aarch64-unknown-linux-gnu.tar.gz | tar xz
sudo mv cobrust /usr/local/bin/
# Linux x86_64 (glibc — standard distros: Debian, Ubuntu, Fedora, RHEL)
curl -L https://github.com/Cobrust-lang/cobrust/releases/latest/download/cobrust-v0.3.0-x86_64-unknown-linux-gnu.tar.gz | tar xz
sudo mv cobrust /usr/local/bin/
# Linux x86_64 static musl — Alpine, distroless, minimal containers (no glibc required)
curl -L https://github.com/Cobrust-lang/cobrust/releases/latest/download/cobrust-v0.3.0-x86_64-unknown-linux-musl.tar.gz | tar xz
sudo mv cobrust /usr/local/bin/
```

> **Which Linux binary?** Use the `musl` variant for Alpine containers, scratch-based images,
> or any environment without a glibc installation. Use the `gnu` variant on standard Linux
> distributions (Debian, Ubuntu, Fedora, RHEL, Arch) that ship glibc.
> Both are tier-1 targets; both are built and published on every release.

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
    print(f"fib(10) = {fib(10)}")
    return 0

$ cobrust run src/main.cb
fib(10) = 55
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

### Try the AI alpha surfaces

If you want to try the merged AI-facing stdlib alpha without reading the full architecture doc first:

- Configure at least one provider in `cobrust.toml` using [`cobrust.toml.example`](cobrust.toml.example).
- Declare the route you need:
  - `[routing.structured]` for `llm_complete_structured(...)`
  - `[routing.tools]` for `llm_complete_with_tools(...)`
  - any custom `[routing.<task>]` for `llm_dispatch(...)`
- Call the current surfaces as **flat prelude functions** such as `llm_complete(...)`, `llm_dispatch(...)`, `llm_stream(...)`, `llm_complete_structured(...)`, and `llm_complete_with_tools(...)`.
- Do not write `cobrust.llm.*`, `cobrust.prompt.*`, or `cobrust.tool.*` module-path syntax yet; that naming is architectural framing, not current source syntax.
- Current alpha caveat: when routing or provider setup is missing or fails, these helpers currently return `""` (or `[]` for `llm_stream(...)`) instead of rich runtime errors.

See [docs/human/en/getting-started.md](docs/human/en/getting-started.md) for the short setup path and [docs/human/en/architecture.md](docs/human/en/architecture.md) for the full design notes.

---

## Quick Start for LeetCode

Want to solve LeetCode problems in Cobrust? Two steps:

1. Install Cobrust v0.3.0+ (see [Install](#install) above)
2. Read the guide:
   - English: [LeetCode with Cobrust](docs/human/en/getting-started-leetcode.md)
   - 中文: [用 Cobrust 刷 LeetCode](docs/human/zh/getting-started-leetcode.md)

10 ready-to-run example programs in [`examples/leetcode/`](examples/leetcode/), covering:
hash-map simulation, string reversal, recursion/DP, stack-based parsing, merge sort,
Kadane's algorithm, binary search, climbing stairs, greedy stock, and Roman numerals.

```bash
# Try Two Sum right now:
printf "4\n2\n7\n11\n15\n9\n" | cargo run -p cobrust-cli -- run examples/leetcode/two_sum.cb
# Expected output:
# 0
# 1
```

Full problem catalog and input formats: [`examples/leetcode/README.md`](examples/leetcode/README.md)

---

## Status

**0.3.0** — Phase G closure (all four §2.5 directions A/B/C/D shipped). Full release notes in [docs/releases/v0.3.0.md](docs/releases/v0.3.0.md).

- ✅ **Compiler core** — lexer / parser / HIR / type checker / MIR / Cranelift codegen; zero clippy warnings under `-D warnings`.
- ✅ **Phase F.3 language completeness** (v0.2.0) — `break` / `continue`, `for` loops, `list[str]`, `f64` (full IEEE-754 + f-string `{:.Nf}`), `dict[K, V]` (insertion-ordered per [ADR-0050d](docs/agent/adr/0050d-dict-design.md)), string stdlib (split/join/replace/trim/find/contains/...), file IO (read/write/append, stdin/stdout/stderr).
- ✅ **Phase G LLM-first surface** (v0.3.0, all four directions closed):
  - **A — Explicit `&s` borrow** — eliminates `clone()` clutter; one-way call-site coercion per [ADR-0052a](docs/agent/adr/0052a-explicit-borrow-let-rebind.md) + [ADR-0052f](docs/agent/adr/0052f-borrow-of-call-relaxation.md) + [ADR-0052g](docs/agent/adr/0052g-borrow-of-call-result-type-check.md). `&s.method()` parse path unblocked.
  - **B — Errors print the FIX** — 41 variants total (24 `TypeError` + 11 `MirError` + 6 `LoweringError`) carry structured `suggestion: Option<&'static str>`; LSP `Diagnostic.relatedInformation` forward-compat per [ADR-0052b](docs/agent/adr/0052b-error-ux-fix-suggestions.md).
  - **C — `@py_compat` tier hard-bind to L2 verifier** — `Strict` / `Semantic` / `Numerical{rtol}` / `None` enum + `TierVerifier`; [ADR-0037](docs/agent/adr/0037-py-compat-hard-bind.md) activated per [ADR-0052c](docs/agent/adr/0052c-py-compat-tier-l2-bind.md).
  - **D — Method-call sugar infra** — 25 new method-form entries (Str×10 + List×5 + Float×5 + Int×5) per [ADR-0052d-prereq](docs/agent/adr/0052d-prereq-method-dispatch-infra.md); full LC-100 corpus migration deferred to v0.3.1 (ADR-0052d-final).
- ✅ **Phase H FULL CLOSED** (2026-05-18) — self-host type-checker scoping + 226 cobrust-types-cb parity tests PASS on DG; `.cb` files are READ-ONLY pseudocode policy ratified (ADR-0055/a/b/c/d/e; Wave-2 canonicalization surfaces).
- ✅ **Phase I FULL CLOSED** (2026-05-19) — Cranelift-JIT scaffold (`cobrust-cranelift-jit` crate, 12 unit tests) + TypeCheckCtx `Clone+Send` Arc-COW + Session + per-file invalidate (LSP unblocker) + REPL `fn` redefinition + per-symbol `invalidate_def` (ADR-0056a/b/c).
- ✅ **Phase J wave-1 closed** (2026-05-19) — `cobrust-lsp` crate: `textDocument/publishDiagnostics` over stdio, 16 tests (incl. 5 insta snapshots), 42 `From` impls, dual-track docs (ADR-0057a). Wave-2 `didChange` + CodeAction (0057d) pending.
- ✅ **CLI tempdir RAII** — closes the Mac/DG `/tmp/cobrust-*` leak (235G temp-leak incident root cause); `tempfile::TempDir` RAII guarantees cleanup on panic / cancellation / signal.
- ✅ **Bilingual README** — `README.zh.md` ships with full Chinese translation parity to `README.md` per CLAUDE.md §3 dual-track documentation mandate.
- ✅ **Standard library** — io / collections / string / math / panic / env / fmt / iter + structured concurrency runtime (M13). AI-facing alpha: `cobrust.llm` / `.prompt` / `.tool` flat prelude fns (per [ADR-0049](docs/agent/adr/0049-alpha-honesty-and-onboarding-hardening.md) honesty hardening).
- ✅ **Package format** — `cobrust.toml`, content-addressed registry, deterministic lockfile.
- ✅ **AI translation pipeline** — production-validated on stateless + stateful tomli functions (real LLM, 12/12 + 14/14 strict deterministic over 5 runs). dateutil / msgpack: partial.
- 🚧 **Tooling** — REPL JIT scaffold landed (Phase I); full REPL interactive loop pending. LSP `publishDiagnostics` live (Phase J wave-1); `didChange` + CodeAction pending (wave-2). No debugger (Phase L). No WASM target.
- 🚧 **LLVM backend** — Phase K (queued; 3-4 weeks); current release builds use Cranelift.
- 🚧 **Phase J wave-2+** — `didChange` snapshot reuse + CodeAction (ADR-0057d); ~1-2 weeks.

**What this means**: Cobrust is **mechanism-validated** for the language core + AI translation pipeline. **Phase G LLM-friendliness is fully closed in v0.3.0**. **Phases H and I are fully closed**; Phase J wave-1 (LSP diagnostics) is closed. Phase J wave-2 + Phase K (LLVM + Drop + IR opt + JIT/AOT conv + xarch) are next.

**§2.5 constitutional pillar** ([CLAUDE.md §2.5](CLAUDE.md) + [ADR-0051](docs/agent/adr/0051-llm-first-design-principle.md)): "Cobrust is not the language most pleasant for humans to write — it is the language LLM agents write correctly on the first try." See [`docs/agent/skills/cobrust-first-try.md`](docs/agent/skills/cobrust-first-try.md) for the agent-facing onboarding skill.

**What's next** (queue order): Phase J wave-2 (`didChange` + CodeAction) → Phase K codegen hardening (LLVM backend, Drop schedule, MIR-level IR opt, JIT/AOT lowering convergence, cross-compile matrix expansion) → outstanding tasks #26 / #30 / #52.

See the [post-Phase-G roadmap (ADR-0054)](docs/agent/adr/0054-post-phase-g-roadmap.md) for full detail.

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

**Phase F — DONE** (v0.1.x → v0.2.0): translation pipeline production-validated (tomli 5/5 + dateutil 5/5 real-LLM); AI-native stdlib alpha (`cobrust.llm` / `.prompt` / `.tool`); Phase F.3 language completeness (break/continue, for, list[str], f64, dict, string stdlib, file IO).

**Phase G — DONE** (v0.2.0 → v0.3.0): the four §2.5 LLM-first binding directions — all shipped
- ✅ A — Explicit `&s` borrow (ADR-0052a/f/g; LARGEST LLM-friendliness deficit per LC-100 honest-debt empirical baseline)
- ✅ B — Errors print the FIX (41 variants; structured `suggestion` field; LSP forward-compat)
- ✅ C — `@py_compat` tier hard-bind to L2 verifier (ADR-0037 reserved → activated via ADR-0052c)
- ✅ D — Method-call sugar infra (25 new entries across 4 types; full LC-100 corpus migration deferred to v0.3.1)

**Post-Phase-G roadmap** ([ADR-0054](docs/agent/adr/0054-post-phase-g-roadmap.md), ~10-12 weeks total at agent-velocity):

| Phase | Surface | Wall | §2.5 ROI |
|---|---|---|---|
| ~~**H**~~ ✅ | Self-host type checker scoping; 226 parity tests; `.cb` READ-ONLY policy | closed 2026-05-18 | medium |
| ~~**I**~~ ✅ | Cranelift-JIT scaffold + Session Clone+Send + REPL fn-redef | closed 2026-05-19 | medium |
| **J** wave-1 ✅ | `publishDiagnostics` LSP server (cobrust-lsp, 16 tests, 42 From impls) | closed 2026-05-19 | **highest** |
| **J** wave-2+ | `didChange` snapshot reuse + CodeAction (ADR-0057d) | ~1-2 weeks | **highest** |
| **K** | LLVM Backend + Drop schedule + MIR IR opt + JIT/AOT conv + xarch | ~3-4 weeks | neutral |
| **L** | Debugger (DWARF from K + breakpoint runtime + REPL integration) | ~1 week | low |

§2.5 ROI rerank explanation: J is highest because in-editor LLM agents (Cursor / Continue / Cody) read LSP diagnostics + suggestions directly — ADR-0052b's structured `suggestion` field is the precise payload Phase J wires into `Diagnostic.relatedInformation` + `CodeAction.title`.

Full Phase-by-Phase sub-ADR roster + compression-ratio empirical grounding: [ADR-0054](docs/agent/adr/0054-post-phase-g-roadmap.md).

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

**Cobrust 0.3.0** — built in public, by AI agents working with humans.
*If you tried it, tell us what broke.*

</div>
