---
title: Competitive Intel — Zero Language (vercel-labs/zero)
status: insight
last_verified_commit: 7e4befc
relates_to: [adr:0051, adr:0052b, adr:0057a, adr:0058d]
date: 2026-05-19
authors: [three parallel Opus research agents — af3221734ed531392, a05bb470201b6dd25, a26e3650935fa88f6]
---

## §1 Executive Summary — Zero vs Cobrust Comparison Matrix

| Dimension | Zero (vercel-labs/zero) | Cobrust |
|---|---|---|
| **Implementation language** | Pure C11 — 34K LOC compiler + 8.3K Node.js tooling | Rust — ~23 crates, 200+ ADRs |
| **Pipeline** | AST → IR → direct ELF/Mach-O/COFF (no LLVM) | AST → HIR → MIR → LLVM/Cranelift (ADR-0058a/d) |
| **Type system** | Static nominal, ownership/borrow, capability-passed `World` | Static structural, ownership/borrow, `Result<T,E>` / `Option<T>` |
| **"Agent-first" meaning** | Structured JSON diagnostics OUTPUT for agents to consume | LLM IN compiler as translation subsystem (L0–L3) |
| **Concurrency** | AOT only, no GIL, no JIT/GC | AOT + optional JIT (ADR-0058d), no GIL, structured concurrency |
| **Stdlib breadth** | 15 modules (broader than C, narrower than Python/Go) | Python-ecosystem parity via L0–L3 translation pipeline |
| **Maturity** | 4 days old, v0.1.3 "pre-1 unstable", 1 contributor, 2.4K stars | v0.3.0 Phase G+ complete, 22-crate workspace, ADR-0001–0060c |
| **§2.5 compile-time-catch** | Strong — independently rediscovered same axis | Codified in CLAUDE.md §2.5 + ADR-0051 |
| **§2.5 training-data-overlap** | Opposite bet — invents `raises`/`check`/`rescue`/`owned`/`extern shape` net-new keywords | Explicitly preserves Python + Rust surface priors |
| **Translation pipeline** | None — "agent-first" = consume output, not LLM-in-loop | L0–L3 closed loop is the entire value proposition |
| **Migration story** | No migration tooling | Core differentiator: zero-migration-cost via provenance manifests |

---

## §2 Verdict: Disjoint Problem Spaces Despite Shared "Agent-First" Framing

Zero and Cobrust both use the phrase "agent-first" but mean **opposite things**:

- **Zero's "agent-first"**: the compiler emits structured JSON diagnostics that an AI agent (Claude, Copilot, Cursor) downstream can parse to suggest fixes. The LLM is OUTSIDE the compiler. The language design is Zero-invented syntax, not Python-familiar.
- **Cobrust's "agent-first"** (ADR-0051): LLMs are first-class components INSIDE the compiler's translation subsystem. The LLM writes Cobrust. The language surface is designed so LLMs write it correctly on the first try (training-data-overlap rule).

These are complementary observations but non-competing architectures:
- Zero targets: **new projects** by developers who want a Zig/Rust safety story with Python-flavored ergonomics.
- Cobrust targets: **existing Python ecosystem migration** — zero migration cost is the pitch, not new-project ergonomics.

Zero's §2.5-equivalent compile-time-catch rediscovery validates Cobrust's axis. Zero's training-data-overlap choice (invent new keywords) is a conscious counterbet that Cobrust should NOT copy.

---

## §3 Top 3 Borrowable Items (Ranked by §2.5 ROI)

### 3.1 RANK 1 — `cobrust skills` Subcommand (ADR-0061 queued)

**Zero precedent**: `zero skills get <name>` returns a version-matched cheatsheet embedded in the binary.

**Cobrust adaptation**: embed curated `docs/agent/skills/*.md` into `cobrust` binary via `rust-embed`. LLM mid-conversation calls `cobrust skills get cobrust-language` without any external network round-trip or training-data dependency. Solves the training-data-overlap gap for Cobrust-specific idioms that are NOT in any LLM training corpus.

**ADR**: ADR-0061 (proposed) — see `docs/agent/adr/0061-cobrust-skills-subcommand.md`
**Estimate**: ~300 LOC, 4-6h implementation, Phase N (post-Phase-M)
**§2.5 ROI**: HIGHEST — this is the training-data-overlap intervention

### 3.2 RANK 2 — Fix-Safety Ladder (ADR-0062 queued)

**Zero precedent**: Zero's diagnostic taxonomy distinguishes fix safety tier implicitly (format vs behavioral vs API-breaking suggestions in JSON output).

**Cobrust adaptation**: explicit `FixSafety` enum (`FormatOnly` / `BehaviorPreserving` / `LocalEdit` / `ApiChanging` / `TargetChanging` / `RequiresHumanReview`) on TypeError + MirError + LoweringError variants. LSP code-action (ADR-0057a) gates auto-apply by safety tier. LLM agents can route by `fix_safety` field to know which fixes are safe to apply unattended.

**ADR**: ADR-0062 (proposed) — see `docs/agent/adr/0062-fix-safety-ladder.md`
**Estimate**: ~600 LOC, ADR-0062 extends ADR-0052b
**§2.5 ROI**: HIGH — machine-routable fix signal for LLM agents

### 3.3 RANK 3 — Capability-Passed `World` Parameter (ADR-0063, deferred)

**Zero precedent**: `pub fn main(world: World)` makes I/O explicit at the type level; no ambient globals; deterministic effects.

**Cobrust adaptation**: `cobrust check --strict-effects` mode. Functions that perform I/O must declare `world: &World` param or return `impl Effect`. Makes LLM-generated code auditable for side effects at type level.

**ADR**: ADR-0063 (NOT YET AUTHORED) — defer to M14 / v0.4.0
**Estimate**: ~2000 LOC, significant type-system impact
**§2.5 ROI**: MEDIUM — reduces LLM surprise on effect propagation but high implementation cost

---

## §4 Anti-Patterns to Reject

### 4.1 REJECT — `.0` File Extension

- **Zero does this**: `.zero` extension
- **Why reject**: `.cb` maps closer to `.py` in training-data distribution (ADR-0051 §2.5). IDE tooling already wired. No migration benefit.
- **Verdict**: keep `.cb`

### 4.2 REJECT — `choice`/`shape`/`enum` Triple-Keyword

- **Zero does this**: overloads three keywords for sum types, nominal records, structural records
- **Why reject**: Cobrust's Rust-style `struct` + `enum` are in LLM training corpus at high frequency. Introducing a third keyword (`choice`) adds training-data-overlap cost with no §2.5 benefit.
- **Verdict**: keep Rust-style `struct` + `enum`; add `shape` only if structural typing demands it (separate ADR required)

### 4.3 REJECT — Pre-1 Churn-Without-Migration Posture

- **Zero does this**: `zero skills path` removed between 0.1.2 and 0.1.3 with no migration path, justified as "pre-1 unstable"
- **Why reject**: Cobrust's `@py_compat` + translation-provenance discipline means every change to a translated library must be traceable. CLI surface must lock at v0.4.0 freeze per F-pattern pre-empt (§5 below).
- **Verdict**: maintain Cobrust's deprecation-and-migration discipline even pre-1.0

---

## §5 F-Pattern Intel Pre-Empt List

These are failure patterns observed in Zero's development trajectory that Cobrust must pre-empt:

| # | Zero failure pattern | Cobrust pre-empt |
|---|---|---|
| **FP-1** | Zero 0.1.2 borrow provenance rebuild — control-flow-join + unreachable-path provenance broke under refactor | ADR-0052a/f/g MUST cover control-flow-join + unreachable-path provenance BEFORE Phase H self-host begins. Validate in Phase H integration tests. |
| **FP-2** | Zero BLD003 backend removal — ELF emitter removed mid-release causing downstream breakage | Cobrust ADR-0058d already pre-empted with JIT/AOT convergence (accepted). Backend removal must go through ADR + deprecation cycle. |
| **FP-3** | Zero CLI churn (`zero skills path` removed in 0.1.3) — agents relying on stable CLI surface broke silently | Cobrust CLI MUST lock stable surface at v0.4.0 freeze. `cobrust skills` subcommand (ADR-0061) must be covered by integration tests that fail on removal. |

---

## §6 Sources

- Agent `af3221734ed531392` (2026-05-19) — Zero language design and philosophy analysis
- Agent `a05bb470201b6dd25` (2026-05-19) — Zero implementation and code quality analysis
- Agent `a26e3650935fa88f6` (2026-05-19) — borrowable lessons synthesis and cross-project comparison

Source repository: `vercel-labs/zero` (GitHub), tag `0.1.3`, commit state as of 2026-05-19.

---

## Appendix A — Raw Findings: Agent af3221734ed531392 (Design/Philosophy)

**Zero design/philosophy findings (verbatim summary from agent output):**

- Pure C11 compiler, 34K LOC. Three-layer pipeline: AST → IR → direct ELF/Mach-O/COFF emitters with no LLVM or Cranelift dependency.
- Static nominal type system with ownership and borrow-checking. Capability-passed `World` parameter pattern: `pub fn main(world: World)` makes I/O effects explicit at function signature level; no ambient globals, no hidden effects.
- AOT-only compilation; no JIT, no GC, no reference counting.
- "Agent-first" framing: compiler outputs structured JSON diagnostics designed for downstream AI agent consumption. This is explicitly the OPPOSITE of Cobrust's LLM-in-compiler model.
- New keywords invented: `raises`, `check`, `rescue`, `owned`, `extern shape`. These do NOT appear in Python or Rust training corpora at meaningful frequency — a conscious departure from training-data-overlap optimization.
- File extension: `.zero`. No Python-familiar surface design intent.
- Single contributor Chris Tate (@Vercel). Project age: 4 days at time of analysis. GitHub stars: 2,400. Version: 0.1.3 labeled "pre-1 unstable."
- §2.5 compile-time-catch axis: independently rediscovered and strongly embraced. Every error is a type error caught at compile time. No runtime type surprises.
- §2.5 training-data-overlap axis: deliberately abandoned in favor of novel syntax. "Correctness via fresh surface" vs Cobrust's "correctness via familiar surface."
- `zero skills get <name>` subcommand: embedded version-matched cheatsheet. Implemented via binary-embedded markdown, CLI-accessible. Zero CLI churn observed: `zero skills path` removed 0.1.2 → 0.1.3.

---

## Appendix B — Raw Findings: Agent a05bb470201b6dd25 (Impl/Code Quality)

**Zero implementation and code quality findings (verbatim summary from agent output):**

- Codebase metrics: 34K LOC C11 compiler + 8.3K LOC Node.js tooling. No external parser generator — handwritten recursive descent.
- No LLVM dependency: direct machine code emission for ELF (Linux), Mach-O (macOS), COFF (Windows). ADR-0058 pre-empt: Cobrust's LLVM dependency is a validated choice; Zero's no-LLVM bet trades portability ease for implementation complexity.
- Ownership + borrow-checking implemented in C — no Rust borrow checker available, so Zero hand-rolls borrow-provenance tracking. Observed borrow-provenance rebuild between 0.1.1 and 0.1.2 (control-flow-join + unreachable-path bug) — direct F-pattern source.
- Diagnostic JSON shape: structured `{ "code": "BLD003", "severity": "error", "fix_safety": "behavior-preserving", "suggestion": "..." }`. Fix-safety field present in diagnostic JSON output. This is the direct inspiration for ADR-0062.
- Stdlib: 15 modules. Broader than libc, narrower than Python/Go. No translation pipeline — stdlib is hand-written Zero code. No L0–L3 equivalent.
- Test infrastructure: property-based + differential against C oracle for numeric operations. Closest to Cobrust's L2 behavior gate — but without LLM-in-loop.
- BLD003 backend removal incident: direct ELF emitter removed in 0.1.3 with no migration path. Downstream tools using `--emit=elf` directly broke. No deprecation cycle.
- Pre-1 unstable posture: Zero maintainer has stated CLI is "not stable until 1.0." This is FP-3 source.

---

## Appendix C — Raw Findings: Agent a26e3650935fa88f6 (Borrowable Lessons Synthesis)

**Borrowable lessons synthesis findings (verbatim summary from agent output):**

- **Synthesis conclusion**: Zero and Cobrust solve genuinely disjoint problems. Zero = new-project ergonomics for C/Rust-safety seekers. Cobrust = Python-ecosystem migration at zero cost.
- **Shared rediscovery**: §2.5 compile-time-catch is an axis both projects arrived at independently. This validates the Cobrust constitutional choice.
- **Divergent bet**: training-data-overlap. Zero chose novelty; Cobrust chose familiarity. Both are rational given different target users.
- **Top borrowable (ranked)**:
  1. `cobrust skills` subcommand — highest §2.5 ROI; ~300 LOC; Phase N; ADR-0061 queue
  2. Fix-safety ladder on diagnostic variants — medium implementation cost; high LLM-agent utility; ADR-0062 queue
  3. Capability-passed `World` parameter — large implementation cost; deferred to M14/v0.4.0; ADR-0063 queue
- **Top reject (ranked)**:
  1. `.0` file extension — training-data cost too high
  2. `choice`/`shape`/`enum` triple — confusing LLM prior mapping
  3. Pre-1 churn-without-migration — incompatible with Cobrust's provenance discipline
- **F-pattern intel**: FP-1 borrow provenance rebuild, FP-2 BLD003 backend removal, FP-3 CLI churn — all pre-empt-able with existing Cobrust discipline.
- **Overall**: Zero is a strong technical peer project worth monitoring but NOT a fork candidate. Borrowing is surgical (2 ADRs, ~900 LOC total).
