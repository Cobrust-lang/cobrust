<div align="center">

**English** · [中文](README.zh.md)

# Cobrust

**AI-friendly Python successor in Rust, with LLM-driven translation pipeline and AI-native stdlib (in development).**

*Cobra 🐍 + Rust 🦀 — Python ergonomics, Rust safety, zero migration cost.*

[![CI](https://github.com/Cobrust-lang/cobrust/actions/workflows/ci.yml/badge.svg)](https://github.com/Cobrust-lang/cobrust/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0%20%2F%20MIT-blue.svg)](#license)
[![Stage](https://img.shields.io/badge/stage-0.6.2-brightgreen.svg)](https://github.com/Cobrust-lang/cobrust/releases/tag/v0.6.2)

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
# Option A (recommended on macOS/Linux) — Via Homebrew tap
brew tap cobrust-lang/cobrust
brew install cobrust
# Installs the single `cobrust` binary; the LSP/DAP servers are the
# `cobrust lsp` / `cobrust dap` subcommands (the standalone cobrust-lsp /
# cobrust-dap shim binaries were removed at v0.7.0 — ADR-0070 X.5).

# Option B — Via cargo (Rust toolchain 1.94+ AND LLVM 18 required)
#   Since v0.7.0 the codegen backend is LLVM by default (ADR-0070 X.3), so a
#   from-source build links system LLVM 18 via llvm-sys. Install LLVM 18 +
#   point llvm-sys at it FIRST:
#     Ubuntu/Debian: sudo apt-get install -y llvm-18 llvm-18-dev libpolly-18-dev
#                    export LLVM_SYS_181_PREFIX=/usr/lib/llvm-18
#     macOS:         brew install llvm@18
#                    export LLVM_SYS_181_PREFIX=$(brew --prefix llvm@18)
cargo install --git https://github.com/Cobrust-lang/cobrust cobrust-cli

# Option C — Download a prebuilt wheel (v0.7.0: x86_64-linux-gnu + aarch64-apple
# only; FHS bin/lib/share layout per ADR-0069). The Linux glibc wheel
# dynamically links system LLVM 18 — `sudo apt-get install -y libllvm18` first.
# Each tarball extracts to a self-contained cobrust-<version>/ directory.
# Symlink bin/cobrust into your $PATH; the runtime + stdlib stay siblings.
# Do NOT `cp cobrust /usr/local/bin/` — that breaks the wheel-layout lookup chain.
# (musl + aarch64-linux wheels are deferred at v0.7.0 — ADR-0070 X.6 / F77.)

# macOS Apple Silicon M1 (tier-1)
curl -L https://github.com/Cobrust-lang/cobrust/releases/download/v0.6.2/cobrust-v0.6.2-aarch64-apple-darwin-m1.tar.gz | tar xz -C $HOME/.local/ \
  && ln -sf $HOME/.local/cobrust-v0.6.2/bin/cobrust $HOME/.local/bin/cobrust

# Linux x86_64 baseline (v1 — any x86_64)
curl -L https://github.com/Cobrust-lang/cobrust/releases/download/v0.6.2/cobrust-v0.6.2-x86_64-unknown-linux-gnu-v1.tar.gz | tar xz -C $HOME/.local/ \
  && ln -sf $HOME/.local/cobrust-v0.6.2/bin/cobrust $HOME/.local/bin/cobrust

# Linux x86_64 musl static (Alpine / distroless / minimal containers)
curl -L https://github.com/Cobrust-lang/cobrust/releases/download/v0.6.2/cobrust-v0.6.2-x86_64-unknown-linux-musl-v1.tar.gz | tar xz -C $HOME/.local/ \
  && ln -sf $HOME/.local/cobrust-v0.6.2/bin/cobrust $HOME/.local/bin/cobrust

# Each tarball bundles:
#   bin/cobrust            — main driver (subcommands: build/run/check/fmt/translate/new/test/repl/lsp/dap/...)
#   bin/cobrust-lsp        — transitional shim binary (extension v0.1.x compat; ADR-0068 §4.2; deleted at v0.7.0)
#   bin/cobrust-dap        — transitional shim binary (extension v0.1.x compat; ADR-0068 §4.2; deleted at v0.7.0)
#   lib/cobrust/libcobrust_stdlib.a       — prebuilt static stdlib archive
#   share/cobrust/runtime/cobrust_main.c  — runtime C entrypoint
#   share/cobrust/runtime/cpu_features.c  — CPU feature detection helpers

# All 9 CPU-tier variants per release: v1/v3/v4 (x86_64 glibc), v1/v3 (x86_64 musl),
# neon/sve (aarch64 linux), m1/m2 (aarch64 darwin). SHA256SUMS published alongside.

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

1. Install Cobrust v0.6.0+ (see [Install](#install) above)
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

**v0.7.0-dev (in development, on top of the v0.6.2 release)** — building on the v0.6.2 baseline (LSP v1.3 feature-complete, 13 handlers; DAP v1.2 feature-complete, 17 handlers; ADR-0023 §A3 production-scale resolved at 0.293 O3/O0 ratio, empirical). Current focus: the `.cb` ecosystem surface — #156 FastAPI-real **type-driven request validation + OpenAPI** (ADR-0080 / ADR-0081, CI-verified; see below) and the per-import ecosystem static-link path (`pit` / `fang` / `coil` / …). Not yet tagged as a release. Last release notes: [docs/releases/v0.5.0.md](docs/releases/v0.5.0.md).

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
- 🚧 **LLVM backend** — **Default backend = Cranelift = full stdlib parity** (`cobrust build foo.cb` no flag; release wheels do NOT enable `--features llvm`). LLVM is `--features llvm` **experimental** opt-in only. Phase K closed (LLVM IR + DWARF + JIT/AOT conv + musl tier-1); stdlib I/O hookup wave-2 LANDED in v0.5.1 (ADR-0058f — `print` system + str-buffer subroutines; 8 `stdlib_io_*` fixtures PASS); wave-3 (input / argv / list / dict / set / tuple / panic / fmt / iter / math / parse_int / str methods / LLM router) tracked in [ADR-0058g](docs/agent/adr/0058g-llvm-backend-wave3-stdlib-hookup-roadmap.md) + [F45a](docs/agent/findings/f45a-llvm-backend-wave3-scope-systemic.md). **End-user `cobrust install` path uses Cranelift — not affected by wave-3 stubs.**
- 🚧 **Phase M follow-ups** — BinOp-IntN widening + dynamic-index Array (`#![forbid(unsafe_code)]` blocks GEP) + empty-dict K-flow.

**What this means**: Cobrust v0.5.0 — LSP v1.3 feature-complete (13 handlers) + DAP v1.2 feature-complete (17 handlers). LLM agents writing `.cb` get the full editor intelligence stack: diagnostics + hover + completion + rename + goto-def + codeAction + inlay hints + semantic tokens + call hierarchy + delta sync in any LSP-capable editor. Debugging is fully production-ready: logpoints + data breakpoints + multi-thread + conditional bp + stepIn all landed. O3 binary is **70.7% smaller** than O0 (empirical production measurement, ADR-0023 §A3 resolved).

**§2.5 constitutional pillar** ([CLAUDE.md §2.5](CLAUDE.md) + [ADR-0051](docs/agent/adr/0051-llm-first-design-principle.md)): "Cobrust is not the language most pleasant for humans to write — it is the language LLM agents write correctly on the first try." See [`docs/agent/skills/cobrust-first-try.md`](docs/agent/skills/cobrust-first-try.md) for the agent-facing onboarding skill.

---

## §2.5 in action — type-driven request validation + OpenAPI (FastAPI-real, no legacy debt)

A `.cb` web handler declares its request body as a **typed class**; the type *is* the contract. This is the §2.5 pillar made concrete on the ecosystem surface — and it is CI-verified end-to-end ([`examples/fastapi_real_demo/`](examples/fastapi_real_demo/) + the `pit_validated_body` / `pit_string_refinement` / `pit_openapi` E2Es; [ADR-0080](docs/agent/adr/0080-cb-native-type-driven-request-validation-and-openapi.md) + [ADR-0081](docs/agent/adr/0081-validated-body-field-read-serde-bridge.md)).

```python
import pit

# The validated body: a class whose fields carry a `where`-refinement.
class CreateUser:
    name:  str where 1 <= len(self) and len(self) <= 50   # string LENGTH
    age:   i64 where 0 <= self and self <= 150            # int RANGE
    email: str where pattern(self, ".+@.+")              # string PATTERN

fn create_user(req: pit.Request, body: CreateUser) -> pit.Response:
    let a: i64 = body.age              # typed field read — `body.aeg` is a COMPILE error
    if a >= 18:
        return pit.json_response(201, body)          # echo the validated body
    return pit.text_response(403, "must be 18 or older")  # business-rule branch

fn main() -> i64:
    let app = pit.App()
    let _ = app.route_validated("POST", "/users", create_user)
    let _ = app.serve_openapi("/openapi.json")   # schema derived from the SAME type
    let _exit = app.run("127.0.0.1", 8080)
    return 0
```

What this buys an LLM agent writing the code (each point ships and is exercised by a passing E2E):

- **Structure is caught at compile time.** Field presence + field type *are* the class field table — a typo'd `body.aeg` is a `TypeError`, not a runtime `KeyError`. You cannot ship a handler that reads a field that is not there (§2.5 compile-time-catch).
- **Value constraints are ONE boundary guard → a typed 422.** The `where`-refinements (int range, string length, string pattern) run once at the request boundary and render a `Result` → **422** — never a thrown exception, never an in-handler re-check (drops the pydantic exceptions-as-control-flow footgun).
- **`body.age` is a typed read**, statically `i64` — never a stringly-typed `body["age"]`.
- **The OpenAPI schema cannot drift.** `serve_openapi` derives `minimum`/`maximum`/`minLength`/`maxLength`/`pattern` from the *same* field table the validator reads — there is no second, hand-kept schema (unlike a utoipa / drf-spectacular annotation shell).

**Honest scope** (v0.7.0-dev Phase-1–3): refinements are a fixed grammar (int range / string length / string `pattern`), enforced at runtime as the 422 boundary guard; nested-object and list bodies, and compile-time-*checked* refinements, are deferred follow-ups ([ADR-0080](docs/agent/adr/0080-cb-native-type-driven-request-validation-and-openapi.md) §9), not yet shipped.

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

## Editor integration

VSCode / Cursor / VSCodium extension **v0.2.0** at
[`editors/vscode-cobrust/`](editors/vscode-cobrust/) (ADR-0067 + ADR-0068).
Wraps `cobrust-lsp` v1.3 (13 handlers) + `cobrust-dap` v1.2 (17 handlers)
+ bundles the TextMate grammar.

New in 0.2.0 (per ADR-0068):
- **DAP debugger** wired (F5 "Run and Debug" launches `cobrust dap` over
  stdio). Launch-config template + snippet contributed via
  `contributes.debuggers`.
- LSP path migrated to `cobrust lsp` subcommand (v0.6.0+ canonical entry).
  Both LSP and DAP have `*.useSubcommand` settings (default `true`) with
  fallback to the standalone `cobrust-lsp` / `cobrust-dap` shims for
  v0.5.x compatibility.

```bash
# Build from source (Node 20+ required):
cd editors/vscode-cobrust
npm install && npx vsce package
code   --install-extension ./cobrust-0.2.0.vsix   # VSCode
cursor --install-extension ./cobrust-0.2.0.vsix   # Cursor
codium --install-extension ./cobrust-0.2.0.vsix   # VSCodium
```

Marketplace + Open VSX publish steps documented in
[`editors/vscode-cobrust/PUBLISHING.md`](editors/vscode-cobrust/PUBLISHING.md)
(user-side action: publisher account + PAT required).

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
