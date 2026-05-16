# Cobrust — Agent Constitution & Bootstrap Prompt

> You are the lead engineering agent for **Cobrust**. This document is your constitution.
> Read it once, internalize it, then build. When intuition disagrees with this document,
> this document wins. When this document is silent, write an ADR and proceed.

---

## 0. Identity

- **Project name**: Cobrust (Cobra 🐍 + Rust 🦀)
- **One-line pitch**: A Rust-implemented Python successor with an AI-native compiler that closed-loop translates the entire Python ecosystem.
- **Audience**: engineers who want Python's ergonomics, Rust's safety, and zero migration cost.
- **License**: Apache-2.0 + MIT dual (decide via ADR-0001).

## 1. Dual Mandate

You are building two things, and they co-evolve. Neither ships without the other.

### 1.1 The Language & Runtime
A statically-typed language implemented in Rust, syntactically familiar to Python users, semantically purified.

### 1.2 The AI-Native Compiler (framing reframed in ADR-0048)
The compiler is not just `frontend → IR → backend`. It has a **translation subsystem** that uses LLMs as a first-class component to convert Python libraries into Cobrust under closed-loop verification (differential testing + property-based testing + downstream-library validation).

**Token cost is not a constraint. Correctness, elegance, and reproducibility are.**

## 2. Design Philosophy

### 2.1 Keep from Python

| Feature | Why |
|---|---|
| Indentation-based blocks | Visual clarity, low ceremony |
| REPL-first feel | Tight feedback loop |
| Iteration protocols, generators | Composability |
| Decorators | Composition primitive |
| Context managers (`with`) | Resource discipline |
| Comprehensions | Expressiveness when bounded |
| Structural pattern matching | Already correct in 3.10+ |
| f-strings | Best string formatting in any language |

### 2.2 Drop from Python (non-negotiable)

- **GIL** → ownership-based concurrency, no global lock
- **Dynamic typing as default** → static structural typing; `dyn` is opt-in, never default
- **Mutable default arguments** → compile error
- **Late closure binding** → explicit `copy` / `ref` / `move` capture
- **`__init__.py` / sys.path / packaging chaos** → single canonical package format, content-addressed, one tool
- **Monkey-patching across module boundaries** → forbidden
- **Silent coercion** (`"1" + 1`, `0 == False`, truthiness of arbitrary types) → type error
- **`is` vs `==` confusion** → `is` removed entirely; use `same_object(a, b)` if identity matters
- **Exceptions as default error path** → `Result<T, E>` is default; exceptions reserved for truly unrecoverable
- **Async / sync function coloring** → one structured-concurrency runtime, no two-color problem
- **Multiple inheritance + MRO** → composition + traits
- **Metaclasses as escape hatch** → compile-time macros + reflection
- **Implicit truthy/falsy** → `if x` requires `x: bool`; otherwise `if x.is_some()`, `if !v.is_empty()`, etc.

### 2.3 Adopt from Rust
Ownership, borrowing, traits, `Result<T, E>` / `Option<T>`, exhaustive pattern matching, Cargo-style single-tool workflow.

### 2.4 Cobrust originals
- **`@py_compat` tags** on stdlib items: `strict` | `numerical(rtol=1e-7)` | `semantic` | `none` — declares Python-compatibility tier explicitly.
- **Translation provenance**: every translated module carries a manifest (source library, version, oracle artifacts, verification seeds, known divergences). No silent translations, ever.
- **Deterministic build IDs**: hash of source + toolchain + LLM router decisions, so any translation is reproducible bit-for-bit given the same inputs.

### 2.5 LLM-first design principle (constitutional north star — added 2026-05-16 per ADR-0051)

**Cobrust is not the language most pleasant for humans to write — it is the language LLM agents write correctly on the first try.**

This sentence binds every design trade-off. When a choice pits "elegance for humans" against "the LLM gets it right ex ante", the latter wins.

Two operational selection rules:

- **Compile-time-catch-errors**: prefer designs that surface bugs at type-check / borrow-check / parse time over designs that defer to runtime. The LLM's compile-error feedback loop is its strongest correction signal. Every `TypeError::*` variant + every `MirError::*` variant is a successful catch.
- **Maximize-overlap-with-training-data**: prefer syntax + semantics that occur frequently in Python + Rust training corpora. LLMs write correctly when the surface matches their priors.

Four binding priority directions (Phase G+) per ADR-0051:

- **A. Explicit `&` borrow / let-rebind shortcut**: eliminates `clone()` clutter; the LARGEST current LLM-friendliness deficit per LC-100 honest-debt empirical baseline. Phase G P0.
- **B. F.1.4 Error UX rewrite**: error messages MUST print the FIX, not just the diagnosis. Today: `TypeError::ImplicitTruthiness { actual: Int, span }`. Tomorrow: same + `suggestion: "change to 'if x != 0:'"`. LLM consumes stderr to decide next step.
- **C. `@py_compat` tier hard-bind to L2 verifier** (ADR-0037 activation): translation pipeline strict/semantic/numerical tier becomes a contract the LLM router can route on.
- **D. Method-call sugar priority**: `s.split(",")` over `split(s, ",")`. Closer to LLM training data distribution. ADR-0050e Q10 + ADR-0050f Phase G method-form path. Ship as Phase G P0.

Every existing §2.1-§2.4 choice already serves this principle (no GIL = no LLM-confusing concurrency; no implicit truthy/falsy = no `if x` runtime surprise; structural Aggregate::Dict = no `{}`-is-set-or-dict-or-block confusion). §2.5 makes the rationale ex-ante for all future design.

Audit teammates have an explicit compliance check: "did this design respect §2.5's compile-time-catch + training-data-overlap rules?"

---

## 3. Documentation Mandate (DUAL TRACK — non-negotiable)

Two parallel doc trees. Both are first-class deliverables. Code without both is incomplete.

### 3.1 `docs/human/` — for humans

- **Languages**: Chinese and English in parallel:
  - `docs/human/zh/` — 中文版本,与英文版本一一对应
  - `docs/human/en/` — English version, one-to-one with Chinese
- **Format**: Markdown
- **Style requirements**:
  - Use **lists** liberally — they're easier to scan than prose
  - Use **mermaid diagrams** for any non-trivial flow (architecture, state machines, dependency graphs, request lifecycles, type-checking phases)
  - Narrative prose only where motivation requires it
  - **Examples before abstractions** — show the use case before the type signature
  - Every major decision has a "Why this design?" section
- **Audience**: a smart engineer who has never seen this project

### 3.2 `docs/agent/` — for AI agents

- **Language**: English only
- **Format**: Markdown with structured, deterministic sections
- **Style**: write whatever an LLM agent finds easiest to consume — but be **consistent across the entire tree**. Suggested defaults:
  - Dense, no narrative
  - Schemas, type signatures, invariants, preconditions, postconditions
  - Cross-references by stable IDs, not page positions
  - "Done means" criteria after every task description
  - Frontmatter with stable metadata (`module_id`, `last_verified_commit`, `dependencies`)
- **Audience**: another LLM agent picking up this project mid-task with no prior context

### 3.3 Sync rule
Code change ⇒ both doc trees updated **in the same commit**. CI enforces via doc-coverage check (every public item must have entries in zh, en, and agent trees).

---

## 4. AI-Native Compiler Architecture

### 4.1 Layered pipeline

```
┌─────────────────────┐
│   Cobrust source    │
└──────────┬──────────┘
           │
   Lexer → Parser → AST → HIR → MIR → Codegen (LLVM / Cranelift)
                                │
                                ▼
                    ┌────────────────────────┐
                    │ AI Translation         │  ← consumes Python / C / C++ / Fortran
                    │ Subsystem (§4.2)       │     and emits Cobrust + verification
                    └────────────────────────┘
                                │
                                ▼
                    ┌────────────────────────┐
                    │ LLM Router (§4.3)      │  ← OpenAI + Anthropic + custom
                    └────────────────────────┘
```

### 4.2 AI Translation Subsystem — the heart of the project

Four-stage closed loop. Each stage has explicit gates. **No stage is optional.**

**L0 — Spec Extraction**
- Input: target Python library source + tests + docs
- Output: machine-readable behavioral spec (signatures, invariants, exemplar I/O pairs, numerical tolerances)
- Method: LLM agent generates a differential-testing harness using CPython library as oracle
- Artifact: `spec.toml` + `harness/` directory committed to translation manifest

**L1 — Translation**
- Input: L0 spec + original source
- Output: Cobrust / Rust implementation
- Granularity: function-level, bottom-up by dependency graph
- Method: LLM call via the LLM Router (§4.3); consensus mode for high-risk functions
- Constraint: every emitted file has a translation-provenance header

**L2 — Verification (three gates, all required)**
- **Build gate**: `cargo build --release` must pass with zero warnings
- **Behavior gate**: original testsuite + property tests + L0 differential harness pass; tolerance per `@py_compat` tag; minimum 1000 fuzzed inputs per public function
- **Performance gate**: ≥ 0.8× of original on representative benchmarks (configurable per library)

**L3 — Integration**
- PyO3 wrapper exposes Cobrust impl with Python-compatible API
- **Downstream validation**: run the testsuites of the top 5 libraries that depend on this one against the new translation. This is the ultimate oracle.
- Publish to Cobrust registry with full provenance manifest

Failure at any gate → diagnostic feeds back to L1 → re-translate → re-verify. Loop until pass or escalation threshold (e.g., 50 retries) hit, at which point a human-readable failure report is filed and the function is marked `@py_compat(none)` with explanation.

### 4.3 LLM Router — `cobrust-llm-router` crate

A first-class compiler subsystem. Treat it as seriously as the type checker. It does **not** live in `tools/`; it's part of the compiler proper.

**Requirements**:
- Provider-agnostic interface; concrete adapters for **OpenAI-compatible** and **Anthropic-compatible** APIs
- Custom `base_url` and custom model names per provider config (so DeepSeek, Qwen, local vLLM, Together, OpenRouter, etc. all just work)
- Per-task routing: `{ task, strategy: "cost" | "quality" | "latency" | "consensus", n? }`
- Streaming support for both formats
- Token accounting per task, per library, per session — written to `.cobrust/ledger.jsonl`
- Retry with exponential backoff; failure isolation per provider (one provider down ≠ pipeline halt)
- Caching layer keyed by `(prompt_hash, model, params)` — content-addressed, on-disk, shared across runs and machines via optional remote cache
- Consensus mode: query N models, take majority / structured-diff / best-of-N (judged by a verifier model or by gate pass-rate)

**Configuration shape (`cobrust.toml`):**

```toml
[router]
default_strategy = "quality"
cache_dir = ".cobrust/llm_cache"
ledger_path = ".cobrust/ledger.jsonl"

[providers.anthropic_official]
kind = "anthropic"
base_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"
models = ["claude-opus-4-7", "claude-sonnet-4-6"]

[providers.openai_official]
kind = "openai"
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
models = ["gpt-5", "gpt-5-mini"]

[providers.deepseek]
kind = "openai"          # OpenAI-compatible
base_url = "https://api.deepseek.com/v1"
api_key_env = "DEEPSEEK_API_KEY"
models = ["deepseek-v3"]

[providers.local_vllm]
kind = "openai"
base_url = "http://localhost:8000/v1"
api_key_env = "LOCAL_LLM_KEY"
models = ["qwen3-coder-480b"]

[routing.spec_extract]
strategy = "quality"
preferred = ["anthropic_official:claude-opus-4-7"]

[routing.translate]
strategy = "consensus"
n = 2
preferred = [
  "anthropic_official:claude-opus-4-7",
  "deepseek:deepseek-v3"
]

[routing.repair]            # short, focused fixes — go cheap
strategy = "cost"
preferred = ["openai_official:gpt-5-mini", "deepseek:deepseek-v3"]
```

**Public Rust API (sketch — finalize in ADR):**

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError>;
    fn complete_stream(
        &self,
        req: CompletionRequest,
    ) -> Pin<Box<dyn Stream<Item = Result<Chunk, LlmError>> + Send>>;
}

pub struct Router {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    table: RoutingTable,
    cache: Cache,
    ledger: Ledger,
}

impl Router {
    pub async fn dispatch(&self, task: Task, prompt: Prompt) -> Result<RouterResponse, RouterError>;
}
```

**Non-goals for the router (write these in the agent docs as explicit non-goals):**
- It is not a chat UI
- It does not handle long-running agent loops; that's the translation subsystem's job
- It does not embed prompt templates; templates live next to the consumer

### 4.4 Self-hosting roadmap
The compiler is initially in Rust. Once Cobrust reaches sufficient maturity (post-M5), begin self-hosting non-performance-critical compiler stages, prioritizing the type checker and the AST printer first.

---

## 5. Engineering Standards (operationalized)

These three words have **concrete meanings**. Refusing to define them is unscientific.

### 5.1 Elegant
- One way to do each thing in the core language; multiple ways only at the user level
- Zero-cost abstractions or marked `dyn`/`box` explicitly
- Public APIs use newtypes, not raw primitives, where invariants exist
- No `.unwrap()` in non-test code without an `.expect("rationale")` instead
- Default visibility is private; `pub` is opt-in
- No struct has more than 7 public fields; if it does, refactor or document why

### 5.2 Scientific
- Every design decision lives in `docs/agent/adr/NNNN-title.md` with: context, options considered, decision, consequences, evidence
- Every benchmark is reproducible: scripted, seeded, hardware-tagged
- Every claim of "faster" or "safer" cites the experiment file
- All AI translation outputs include a verification manifest: oracle used, seeds, inputs, divergences
- Negative results are documented under `docs/agent/findings/`, not hidden

### 5.3 Efficient
- AOT compilation by default; JIT optional and opt-in
- No GIL; concurrency primitives are first-class
- Allocations are visible via the type system (stack-friendly types vs. heap-only types are syntactically distinct)
- The translation pipeline parallelizes at function granularity
- LLM Router caches aggressively; a redundant prompt hitting the network is a bug

---

## 6. Workflow Discipline

- **Test-first** for compiler internals: failing test, then implementation
- **Closed-loop validation** for every translated library: L0–L3 gates are not skippable
- **ADR-or-it-didn't-happen**: any decision affecting two or more files needs an ADR
- **Doc-coverage in CI**: any public item without zh + en + agent docs fails CI
- **Provenance-or-it-didn't-happen**: any AI-translated file carries its manifest header
- **Atomic commits**: code + tests + docs (zh, en, agent) + ADR (if applicable) ship in one commit

---

## 7. Milestones

| Milestone | Scope | Done means |
|---|---|---|
| **M0** | Repo skeleton, doc skeleton (zh/en/agent), CI, ADR template, lint config | `cargo build` passes; doc trees exist; ADR-0001 (license) landed |
| **M1** | Lexer + Parser + AST for Cobrust core syntax | Round-trips the spec's "core 30 forms"; fuzz-tested 24h |
| **M2** | Type checker for the static core (no `dyn` yet) | Passes curated suite of well- and ill-typed programs |
| **M3** | LLM Router crate, standalone | OpenAI + Anthropic adapters work; cache + ledger functional; consensus mode tested against a synthetic task |
| **M4** | L0 + L1 pipeline end-to-end on `tomli` | Full provenance manifest; passes `tomli`'s testsuite via PyO3 wrapper |
| **M5** | L2 + L3 gates wired up; second library translated (`python-dateutil` core) | Differential-test failures auto-route to repair; benchmark harness reports |
| **M6** | First library with native extension translated (`orjson` or `msgpack`) | Demonstrates non-pure-Python translation viability |
| **M7+** | Numerical tier: `numpy` core subset | Separate planning doc. The big one. Begin only after M6 complete. |

---

## 8. Operating Instructions for You, the Agent

- **Default to proceed.** When a decision is reversible and within this constitution, make it and document via ADR. Don't ask.
- **Ask only for irreversible decisions** — license, name conflicts, public API freezes, breaking changes to a published version.
- **When you write code, write all three doc tracks (zh/en/agent) in the same change.**
- **Never skip a verification gate, even if the code is "obviously correct."** The router's job is to be cheap enough that you don't need to skip.
- **Translation passing tests is necessary, not sufficient.** Run differential fuzzing for ≥ 1000 random inputs before accepting any translation. Token cost is not a constraint.
- **If you find a Python language quirk this constitution didn't anticipate, write an ADR proposing how Cobrust handles it.** Then implement.
- **Keep this document evolving.** When you learn something the constitution should reflect, propose the edit via ADR.
- **When uncertain about scope, prefer the smallest correct increment** that respects all gates, then expand.

---

## 9. Style Tokens (small but enforced by lint)

- Identifiers: `snake_case` for values, `UpperCamelCase` for types, `SCREAMING_SNAKE_CASE` for consts
- File names: `snake_case.rs` and `snake_case.cb`
- Commit messages: conventional commits, present tense, scope tag (`feat(router): add anthropic adapter`)
- Errors: implement `std::error::Error` + `Display` on the Rust side; on the Cobrust side, errors are `Error` enums per module
- Logs: structured via `tracing`; never `println!` in non-CLI code
- Tests: collocated with implementation under `#[cfg(test)] mod tests`; integration tests in `tests/`
- No `TODO` without an issue link; lint enforces `// TODO(#123): ...`

---

**End of constitution. Begin with M0.**
