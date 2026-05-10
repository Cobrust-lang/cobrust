<!-- Paste to: Cobrust/docs/post/why-cobrust.md
     Crosspost to: HN, r/rust, r/programming, lobste.rs, dev.to -->

# Why Cobrust — an AI-native compiler that translates Python libraries into verified Rust

> *Cobrust 0.1.0-beta · 2026-05-10 · Built in public, by AI agents working with humans*

---

## The premise

Python won the language popularity war. It also lost the performance war. Two decades of attempts to give Python more speed — Cython, PyPy, Pyston, Cinder, Mojo, PyO3 — have produced impressive engineering, but Python users are still stuck with the same trade-off: stay slow and easy, or rewrite for speed and pay an enormous transition cost.

Cobrust takes a different bet: **AI compilers can translate the existing Python ecosystem to verified Rust automatically.** Not just rewrite — translate, with closed-loop verification, with provenance manifests, with downstream-dep validation. Bit-identical output to the Python original where the spec demands; explicit divergence (per `@py_compat` tags) where Python's quirks aren't worth keeping.

You don't rewrite your Python code. You don't annotate. You point Cobrust at a library, and it translates with verification.

```bash
$ cobrust translate tomli
[L0] Spec extracted from tomli 2.0.1
[L1] Translating with codex gpt-5.5
[L2.build]    cargo build:  0 errors, 0 warnings
[L2.behavior] differential testing 1024 inputs:  99.71% strict PASS
[L2.perf]     1KB 13.8x / 100KB 10.8x / 10MB 9.05x faster than CPython tomllib (ADR-0039)
[L3] Downstream: pip-tools tomli usage compiles + tests pass

$ pip install ./cobrust-tomli
$ python -c "import tomli; print(tomli.loads('foo=1'))"
{'foo': 1}    # transparently backed by verified Rust
```

## What's actually new

Three things, in combination, that no prior project does:

**1. The compiler is a first-class LLM consumer.** Most compilers have a fixed translation function: source → IR → target. Cobrust's translator is closed-loop — it dispatches LLM calls through a router, gates output through build/behavior/perf checks, and feeds failures back to the LLM for repair. The router is provider-agnostic (Anthropic, OpenAI, DeepSeek, local vLLM all work the same). Verification is not optional.

**2. Translation is verified, not trusted.** Every translated function is differentially tested against the CPython oracle on 1000+ fuzzed inputs. Numerical tolerances are explicit (`@py_compat(numerical(rtol=1e-7))`). Behavioral divergences must be tagged or fixed. Failed gates trigger a repair loop with diagnostic feedback, not silent acceptance. Provenance manifests pin the source SHA, the model fingerprint, the exact prompts used.

**3. The output ecosystem replaces the input transparently.** A Cobrust-translated library is a Rust crate plus a PyO3 wrapper. `pip install ./cobrust-tomli` and Python imports work as before — but the parser is verified Rust, **9-14× faster on tomli (T1.1 measured vs CPython 3.11 tomllib, see ADR-0039)**, memory-safe. Other libraries pending Phase F.1 perf gates.

## Compared to the prior art

**Mojo** is brilliant — a new language with Python-like syntax and serious GPU/SIMD performance. But Mojo asks you to migrate. Cobrust meets you where you are: keep your Python, get verified Rust under the hood for libraries you depend on.

**Cython** is mature — annotate your Python, get C extensions. But it requires manual annotation, and the bug surface for `nogil` / `cdef` is human-managed. Cobrust does no annotation — the LLM writes the Rust, the verifier proves the equivalence (modulo `@py_compat`).

**PyO3** is the substrate Cobrust ships on top of — every translated library exposes itself as a PyO3 module. PyO3 is excellent infrastructure; Cobrust adds the layer that automatically *generates* the Rust to wrap.

**Pyston / Cinder** target the CPython hot path with custom JITs. Cobrust translates the libraries themselves, not the runtime — different layer.

We're complementary to all of these, not competing.

## Why now

Three trends converged:

- **LLMs got good enough.** Audit-1 and Audit-3a in our repository show real LLMs (Claude Opus 4.7) can produce strict-equivalent Rust translations of stateful Python functions, deterministically across multiple runs. This wasn't true two years ago.
- **Cranelift matured.** The Rust-pure codegen backend means Cobrust ships without a system LLVM dependency — `cargo install cobrust-cli` and you have a compiler.
- **PyPI is large enough that automation is worth more than perfection.** A Cobrust that translates 80% of top-100 PyPI with verified equivalence beats a hand-written successor language that translates 0%. Coverage × verification > craft × perfection, when the ecosystem is this big.

## What 0.1.0-beta means

We have:
- A working language: lexer, parser, HIR, type checker, MIR, Cranelift codegen, package format, REPL stub, structured-concurrency runtime
- A working translator: L0..L3 closed-loop with consensus mode and provenance manifests
- One library translated end-to-end with real LLMs: `tomli`. Full L0..L3 PASS, downstream-dep validation PASS, benchmark within 0.8× of CPython
- 768+ tests, 0 fail, on macOS arm64 + Linux x86_64

We do not have:
- LSP (yet — see [F.1.8 in the roadmap](docs/agent/adr/0038-phase-f-roadmap.md))
- Debugger (yet — F.2.3)
- More than one fully-translated library (yet — F.1.6)
- Self-hosting (yet — F.1.7 starts with AST printer)
- WASM (yet — F.2.4)

We are honest about what's mocked. The translation pipeline default is synthetic-mode (canned LLM responses, deterministic for tests). Production runs need real LLM credentials. The few `cobrust-<lib>` crates beyond tomli are partially synthetic — that's a debt we're paying down with the F.1.6 / F.2.1 push.

## How we got here in two days

This blog post is being published as part of a 2-day 0.1.0-beta release sprint. Here's the work-distribution:

- ~20% maintainer time on strategic decisions, signing off ADRs, recording the screencast, writing this post
- ~80% AI-agent time on coding, testing, translation, documentation, CI

Cobrust is itself an experiment in AI-velocity software engineering. We document our multi-agent topology in [`findings/multi-agent-cobrust-topology.md`](docs/agent/findings/multi-agent-cobrust-topology.md). The patterns generalize — if you're running parallel agents on a serious project, that finding may save you the failure modes we hit in the first hundred commits.

## Trying it

```bash
# install (Rust toolchain 1.94+ required)
cargo install cobrust-cli

# new project
cobrust new hello && cd hello
cobrust run src/main.cb
# → hello, world

# translate a library
cobrust translate tomli
pip install ./cobrust-tomli
python -c "import tomli; tomli.loads('foo=1')"
```

Or browse the [examples](examples/) directory — 10 progressive programs from hello world to multi-module packages.

## Help wanted

Things we'd love help with right now:

- **Translate more libraries.** `textwrap`, `base64`, `urllib.parse`, `tomllib` are good `good-first-issue` candidates. The dispatch prompts in [`docs/agent/adr/0038-phase-f-roadmap.md`](docs/agent/adr/0038-phase-f-roadmap.md) §F.1.6 detail what's needed.
- **LSP.** `cobrust-lsp` doesn't exist yet. F.1.8 is the entry point.
- **Cross-arch validation.** We test on macOS arm64 + Linux x86_64. Windows, Linux aarch64, FreeBSD all need volunteers.
- **AI router adapters.** More providers, more latency engineering, better consensus-aggregation algorithms.

GitHub: <https://github.com/Cobrust-lang/cobrust>
Discussions: <https://github.com/Cobrust-lang/cobrust/discussions>
First-time contributor: tag `good-first-issue` on Issues

## What I want from you, the reader

If you tried Cobrust on a Python library and it broke — file an issue with a minimum reproducer. Even a partial repro helps. Especially the codegen edge cases — we've already found and fixed 3 different `while` codegen bugs from "real users writing real algorithms" stress tests in the first 24 hours of intensive development. There will be more. Help us find them.

If you have feedback on the wedge ("AI Python accelerator" framing) — please tell us. We chose this position deliberately, but the opinions of Mojo / PyO3 / Cython users matter for whether the framing serves the community or just us.

If you want to write a translated library — start with the [`good-first-issue`](https://github.com/Cobrust-lang/cobrust/labels/good-first-issue) tag.

---

*Cobrust is open source under Apache-2.0 OR MIT. Built in public.*
*Want to discuss? [GitHub Discussions](https://github.com/Cobrust-lang/cobrust/discussions) or HN comments on this post.*
