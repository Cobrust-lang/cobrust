<div align="center">

**English** · [中文](README.zh.md)

# Cobrust

**AI-friendly Python successor in Rust, with LLM-driven translation pipeline and AI-native stdlib (in development).**

*Cobra 🐍 + Rust 🦀 — Python ergonomics, Rust safety, zero migration cost.*

[![CI](https://github.com/Cobrust-lang/cobrust/actions/workflows/ci.yml/badge.svg)](https://github.com/Cobrust-lang/cobrust/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0%20%2F%20MIT-blue.svg)](#license)
[![Stage](https://img.shields.io/badge/stage-0.5.0-brightgreen.svg)](https://github.com/Cobrust-lang/cobrust/releases/tag/v0.5.0)

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
# Option A — Via cargo (Rust toolchain required, 1.94+)
cargo install --git https://github.com/Cobrust-lang/cobrust cobrust-cli

# Option B — Download a prebuilt wheel (v0.5.0, 9 variants — pick your CPU tier)
# Linux x86_64 baseline (v1 — any x86_64)
curl -L https://github.com/Cobrust-lang/cobrust/releases/download/v0.5.0/cobrust-v0.5.0-x86_64-linux-gnu-v1.tar.gz | tar xz && sudo mv cobrust /usr/local/bin/
# Linux x86_64 AVX2 (v3 — Haswell+, most post-2013 desktops/servers)
curl -L https://github.com/Cobrust-lang/cobrust/releases/download/v0.5.0/cobrust-v0.5.0-x86_64-linux-gnu-v3.tar.gz | tar xz && sudo mv cobrust /usr/local/bin/
# Linux x86_64 AVX-512 (v4 — Skylake-X / Ice Lake server)
curl -L https://github.com/Cobrust-lang/cobrust/releases/download/v0.5.0/cobrust-v0.5.0-x86_64-linux-gnu-v4.tar.gz | tar xz && sudo mv cobrust /usr/local/bin/
# Linux x86_64 musl v1 — Alpine, distroless, minimal containers (no glibc required)
curl -L https://github.com/Cobrust-lang/cobrust/releases/download/v0.5.0/cobrust-v0.5.0-x86_64-linux-musl-v1.tar.gz | tar xz && sudo mv cobrust /usr/local/bin/
# Linux x86_64 musl v3 — Alpine + AVX2
curl -L https://github.com/Cobrust-lang/cobrust/releases/download/v0.5.0/cobrust-v0.5.0-x86_64-linux-musl-v3.tar.gz | tar xz && sudo mv cobrust /usr/local/bin/
# Linux aarch64 NEON (generic ARM64 — Graviton2, Ampere, Pi 4)
curl -L https://github.com/Cobrust-lang/cobrust/releases/download/v0.5.0/cobrust-v0.5.0-aarch64-linux-gnu-neon.tar.gz | tar xz && sudo mv cobrust /usr/local/bin/
# Linux aarch64 SVE (Neoverse V1/V2, Graviton3+)
curl -L https://github.com/Cobrust-lang/cobrust/releases/download/v0.5.0/cobrust-v0.5.0-aarch64-linux-gnu-sve.tar.gz | tar xz && sudo mv cobrust /usr/local/bin/
# macOS Apple Silicon M1 (baseline)
curl -L https://github.com/Cobrust-lang/cobrust/releases/download/v0.5.0/cobrust-v0.5.0-aarch64-apple-darwin-m1.tar.gz | tar xz && sudo mv cobrust /usr/local/bin/
# macOS Apple Silicon M2+ (AMX)
curl -L https://github.com/Cobrust-lang/cobrust/releases/download/v0.5.0/cobrust-v0.5.0-aarch64-apple-darwin-m2.tar.gz | tar xz && sudo mv cobrust /usr/local/bin/

# SHA256SUMS: https://github.com/Cobrust-lang/cobrust/releases/download/v0.5.0/SHA256SUMS

# Option C — cobrust install (Tier 3 wheel auto-select, end-to-end)
cobrust install <pkg>
# Detects CPU tier, fetches matching wheel, verifies SHA256, unpacks.
# Matches pip install UX. Requires cobrust-cli already installed.
```

> **Which wheel?** Use `musl` variants for Alpine / distroless / no-glibc containers.
> Use `gnu` variants on standard Linux distributions (Debian, Ubuntu, Fedora, RHEL, Arch).
> Use `v3` / `v4` / `neon` / `sve` / `m2` variants only if your CPU supports the instruction set —
> all 9 wheels are published per release with SHA256SUMS.

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

1. Install Cobrust v0.5.0+ (see [Install](#install) above)
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

**v0.5.0 PUBLIC RELEASE** — LSP v1.3 feature-complete (13 handlers + delta sync + resolve + cross-file); DAP v1.2 feature-complete (17 handlers + logpoints + data breakpoints + stepIn + result_err); ADR-0057f wave-4 + 0057g wave-5 ALL CLOSED; ADR-0059f wave-4 + 0059g wave-5 ALL CLOSED (incl. 0059f §3.4 RESOLVED); ADR-0023 §A3 production-scale resolved (0.293 O3/O0 ratio, empirical). Release notes: [docs/releases/v0.5.0.md](docs/releases/v0.5.0.md).

- ✅ **Compiler core** — lexer / parser / HIR / type checker / MIR / Cranelift codegen; zero clippy warnings under `-D warnings`.
- ✅ **Phase F.3 language completeness** (v0.2.0) — `break` / `continue`, `for` loops, `list[str]`, `f64` (full IEEE-754 + f-string `{:.Nf}`), `dict[K, V]` (insertion-ordered per [ADR-0050d](docs/agent/adr/0050d-dict-design.md)), string stdlib (split/join/replace/trim/find/contains/...), file IO (read/write/append, stdin/stdout/stderr).
- ✅ **Phase G LLM-first surface** (v0.3.0, all four directions closed):
  - **A — Explicit `&s` borrow** — eliminates `clone()` clutter; one-way call-site coercion per [ADR-0052a](docs/agent/adr/0052a-explicit-borrow-let-rebind.md) + [ADR-0052f](docs/agent/adr/0052f-borrow-of-call-relaxation.md) + [ADR-0052g](docs/agent/adr/0052g-borrow-of-call-result-type-check.md). `&s.method()` parse path unblocked.
  - **B — Errors print the FIX** — 41 variants total (24 `TypeError` + 11 `MirError` + 6 `LoweringError`) carry structured `suggestion: Option<&'static str>`; LSP `Diagnostic.relatedInformation` forward-compat per [ADR-0052b](docs/agent/adr/0052b-error-ux-fix-suggestions.md).
  - **C — `@py_compat` tier hard-bind to L2 verifier** — `Strict` / `Semantic` / `Numerical{rtol}` / `None` enum + `TierVerifier`; [ADR-0037](docs/agent/adr/0037-py-compat-hard-bind.md) activated per [ADR-0052c](docs/agent/adr/0052c-py-compat-tier-l2-bind.md).
  - **D — Method-call sugar infra** — 25 new method-form entries (Str×10 + List×5 + Float×5 + Int×5) per [ADR-0052d-prereq](docs/agent/adr/0052d-prereq-method-dispatch-infra.md); full LC-100 corpus migration deferred to v0.3.1 (ADR-0052d-final).
- ✅ **Phase H FULL CLOSED** (2026-05-18) — self-host type-checker scoping + 226 cobrust-types-cb parity tests PASS on DG; `.cb` files are READ-ONLY pseudocode policy ratified (ADR-0055/a/b/c/d/e; Wave-2 canonicalization surfaces).
- ✅ **Phase I FULL CLOSED** (2026-05-19) — Cranelift-JIT scaffold (`cobrust-cranelift-jit` crate, 12 unit tests) + TypeCheckCtx `Clone+Send` Arc-COW + Session + per-file invalidate (LSP unblocker) + REPL `fn` redefinition + per-symbol `invalidate_def` (ADR-0056a/b/c).
- ✅ **Phase J FULL CLOSED — v1.3 LSP server** (v0.5.0) — `cobrust-lsp` crate feature complete at 13 handlers. Wave-1: `textDocument/publishDiagnostics` over stdio, 16 tests, 42 `From` impls (ADR-0057a). Wave-2: `didChange` + snapshot reuse (ADR-0057b). Wave-3: `hover` + `completion` + `rename` + goto-def + codeAction + cross-file rename (ADR-0057c/d/e). Wave-4: inlay hints + semantic tokens + call hierarchy (ADR-0057f). Wave-5: delta sync + resolve + cross-file refactor (ADR-0057g) — ALL CLOSED. LLM agents in Cursor / Continue / Cody get the full 13-handler surface. Wave-6+: proposed.
- ✅ **Phase K FULL CLOSED** (2026-05-19) — 5 strands: 0058a LLVM IR emission + 0058b opt passes + multi-target + 0058c DWARF debug info + 0058d JIT/AOT lowering convergence + Strand #5 musl tier-1 static binary. **ADR-0023 §A3 PRODUCTION-SCALE RESOLVED** — empirical 0.293 O3/O0 ratio (O3 binary 70.7% smaller than O0) measured on production binary.
- ✅ **Phase L TRULY FULL CLOSED — v1.2 DAP server** (v0.5.0) — `cobrust-dap` crate feature complete at 17 handlers. Wave-1: lldb pretty-printers (ADR-0059a). Wave-2: cobrust-dap server 9-handler core + cobrust debug CLI (ADR-0059b/c). Wave-3: advanced debugger UX (ADR-0059d/e). Wave-4: evaluate + conditional bp + multi-thread + exception bp (ADR-0059f). Wave-5: logpoints + data breakpoints + stepIn + result_err; 0059f §3.4 RESOLVED (ADR-0059g) — ALL CLOSED. Wave-6+: proposed.
- ✅ **Phase M closure** (2026-05-19) — 6 language-surface gaps: i32/i8 narrow-int literals, `-> None` return annotation, `&T` reference annotation, `[T; N]` array literal syntax, anonymous-struct OOS. Follow-ups queued: BinOp-IntN widening, array-indexing dynamic index, empty-dict K-flow.
- ✅ **Phase N FULL CLOSED** — F44 + cargo-udeps + cargo-audit CI gates shipped.
- ✅ **Phase O W2-W4 CLOSED** — Tier-2 4-dim audit P0 fixed; all outstanding autonomous backlog closed.
- ✅ **LC-100 真 100/100** — `examples/leetcode-stress/`: leetcode_corpus_e2e 12/0 + stress 100/0 (was 16/87 pre-session). Production-validated Cobrust source corpus.
- ✅ **CLI tempdir RAII** — closes the Mac/DG `/tmp/cobrust-*` leak (235G temp-leak incident root cause); `tempfile::TempDir` RAII guarantees cleanup on panic / cancellation / signal.
- ✅ **Bilingual README** — `README.zh.md` ships with full Chinese translation parity to `README.md` per CLAUDE.md §3 dual-track documentation mandate.
- ✅ **Standard library** — io / collections / string / math / panic / env / fmt / iter + structured concurrency runtime (M13). AI-facing alpha: `cobrust.llm` / `.prompt` / `.tool` flat prelude fns (per [ADR-0049](docs/agent/adr/0049-alpha-honesty-and-onboarding-hardening.md) honesty hardening).
- ✅ **Package format** — `cobrust.toml`, content-addressed registry, deterministic lockfile.
- ✅ **AI translation pipeline** — production-validated on stateless + stateful tomli functions (real LLM, 12/12 + 14/14 strict deterministic over 5 runs). dateutil / msgpack: partial.
- ✅ **Hardware tiering Tier 1+2+3 FULL SHIPPED** — Tier 1 runtime-dispatch (ADR-0058b); Tier 2 `--target-cpu` (`5186c27` / `a4c2532`); Tier 3 `cobrust install <pkg>` end-to-end works: CPU detect + wheel select + SHA256 verify + unpack. 9 prebuilt wheel variants per release (linux-gnu v1/v3/v4 + linux-musl v1/v3 + linux-aarch64 neon/sve + darwin-arm64 m1/m2).
- 🚧 **Tooling** — REPL JIT scaffold landed (Phase I); full REPL interactive loop pending. LSP v1.3 feature complete: 13 handlers (publishDiagnostics + didChange + hover + completion + rename + goto-def + codeAction + inlay hints + semantic tokens + call hierarchy + delta sync + resolve + cross-file); wave-6+ proposed. DAP v1.2 feature complete: 17 handlers; wave-6+ proposed. No WASM target.
- 🚧 **LLVM backend** — Phase K closed (LLVM IR + DWARF + JIT/AOT conv + musl tier-1); 0058e AOT unification + 50MB+ production bench pending.
- 🚧 **Phase M follow-ups** — BinOp-IntN widening + dynamic-index Array (`#![forbid(unsafe_code)]` blocks GEP) + empty-dict K-flow.

**What this means**: Cobrust v0.5.0 — LSP v1.3 feature-complete (13 handlers) + DAP v1.2 feature-complete (17 handlers). LLM agents writing `.cb` get the full editor intelligence stack: diagnostics + hover + completion + rename + goto-def + codeAction + inlay hints + semantic tokens + call hierarchy + delta sync in any LSP-capable editor. Debugging is fully production-ready: logpoints + data breakpoints + multi-thread + conditional bp + stepIn all landed. O3 binary is **70.7% smaller** than O0 (empirical production measurement, ADR-0023 §A3 resolved).

**§2.5 constitutional pillar** ([CLAUDE.md §2.5](CLAUDE.md) + [ADR-0051](docs/agent/adr/0051-llm-first-design-principle.md)): "Cobrust is not the language most pleasant for humans to write — it is the language LLM agents write correctly on the first try." See [`docs/agent/skills/cobrust-first-try.md`](docs/agent/skills/cobrust-first-try.md) for the agent-facing onboarding skill.

**What's next**:
- Trademark check + Linguist PR submission (staged draft)
- Progopedia + Rosetta Code + 99-bottles outreach (staged)
- Phase J wave-6+ (beyond current 13 handlers) — proposed
- Phase L wave-6+ (beyond current 17 handlers) — proposed
- Production translation benchmarks (full L0-L3 pipeline on 3+ real libraries)
- 0058e AOT unification + 50MB+ production bench

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
| ~~**J**~~ ✅ FULL | `publishDiagnostics` + `didChange` + `hover` + `completion` + `rename` + goto-def + codeAction + inlay hints + semantic tokens + call hierarchy + delta + resolve + cross-file (ADR-0057a-g) — **v1.3 LSP server feature complete (13 handlers)** | closed v0.5.0 | **highest** |
| ~~**K**~~ ✅ | LLVM IR + DWARF + opt passes + multi-target + JIT/AOT conv + musl tier-1; **§A3 production-scale resolved (0.293 ratio)** | closed 2026-05-19 | neutral |
| ~~**L**~~ ✅ TRULY FULL | lldb pretty-printers + cobrust-dap server + cobrust debug CLI + evaluate + conditional bp + multi-thread + exception bp + logpoints + data bp + stepIn + result_err (ADR-0059a-g) — **v1.2 DAP server feature complete (17 handlers)** | closed v0.5.0 | low |
| ~~**M**~~ ✅ | 6 language-surface gaps (i32/i8, None-return, &T, [T;N], anon-struct OOS) + LC-100 100/100 | closed 2026-05-19 | **highest** |
| ~~**N**~~ ✅ | F44 + cargo-udeps + cargo-audit CI gates | closed v0.4.0 | medium |
| ~~**O**~~ ✅ W2-W4 | Tier-2 4-dim audit P0 fixed; autonomous backlog closed | closed v0.4.0 | medium |
| ~~**Tier 3**~~ ✅ FULL | `cobrust install <pkg>` end-to-end: CPU detect + wheel select + SHA verify + unpack; 9 wheel variants | shipped v0.4.0 | high |
| **J** wave-6+ | beyond current 13 handlers | proposed | **highest** |
| **L** wave-6+ | beyond current 17 handlers | proposed | low |
| **Production translation** | full L0-L3 pipeline on 3+ real libraries | proposed | high |
| **0058e** | AOT unification + 50MB+ production bench | pending | neutral |
| **M follow-ups** | BinOp-IntN widening + dynamic-index Array + empty-dict K-flow | pending | high |
| Trademark / Linguist | trademark check + Linguist PR + Progopedia / Rosetta outreach | staged | — |

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

**Cobrust v0.5.0** — built in public, by AI agents working with humans.
*If you tried it, tell us what broke.*

</div>
