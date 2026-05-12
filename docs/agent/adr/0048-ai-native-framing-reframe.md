---
doc_kind: adr
adr_id: 0048
title: "Cobrust framing: AI-friendly Python successor with AI-native stdlib direction (Phase F.2)"
status: proposed
date: 2026-05-12
last_verified_commit: TBD
supersedes: []
superseded_by: []
relates_to: [adr:0019, adr:0038, adr:0044, adr:0045, adr:0047]
discovered_by: review-claude strategic eval 2026-05-12 + user reframe dialogue
ratification_path: review-claude third-party review (mirrors ADR-0045 pattern)
---

# ADR-0048: Cobrust framing reframe — AI-friendly Python successor with AI-native stdlib direction

## Context

### The strategic question (2026-05-12)

After LC-100 Tier A closed at 99/100 (HEAD `459b820`) and triggered ADR-0047's "≥90% pass → SKIP back to W1" gate, the project owner asked a deeper strategic question: **what is Cobrust's actual differentiating frame**, beyond "Python successor in Rust"?

Three honest framings exist:

- **Just-another-Python-successor**: Cobrust ships `input()` + `argv()` + 100 LeetCode programs. So does Mojo, so will several others. The differentiation evaporates at the marketing surface.
- **First AI-native language**: tempting but over-claims. The training-data chicken-egg problem is unresolved (LLMs don't fluently emit Cobrust today). Marketing this would burn credibility on first contact with a code-emitting LLM.
- **AI-friendly Python successor + AI-native stdlib in progress**: calibrated. Cobrust is already 50% there (drop list per constitution §2.2 + L0-L3 translation pipeline §1.2 + ADR-0036 production-validated prompt builder); the visible delta is 5 stdlib modules + a corpus generation pipeline. The framing matches reality without burning credibility.

### Existing AI-friendly inventory (50% already shipped)

| Surface | Status | Anchor |
|---|---|---|
| Drop-from-Python list (no GIL / no late binding / no truthy-falsy / no metaclass / no exceptions-default) | accepted in compiler | constitution §2.2 + ADR-0041 H1-H8 |
| L0→L1→L2→L3 closed-loop translation | first leaf @ ADR-0032; production at ADR-0036; full library @ ADR-0039 | tomli E2E real-LLM verified |
| LLM Router (Anthropic + OpenAI-compatible) | shipped | ADR-0004 + crates/cobrust-llm-router |
| @py_compat tags | proposed scaffold | ADR-0037 |
| User-facing input/argv/algorithm corpus | shipped W2 + LC-100 | ADR-0044 + ADR-0047 |
| Release SOP + verify.py mandate | shipped | ADR-0045 / 0046 / 0047a |

What's missing (the 50% gap):

- Source-level `import cobrust.llm` (today users `extern crate cobrust_llm_router` Rust-side; no Cobrust-source binding)
- Source-level `import cobrust.prompt` (PromptTemplate composition primitives)
- Source-level `@tool` decorator + schema generator (LLM tool-calling at language level)
- Source-level `import cobrust.eval` (eval/benchmark stdlib primitives)
- Source-level `import cobrust.ast` (read own source for self-inspection / refactor)
- Synthetic Cobrust corpus for fine-tune partnership outreach

### What the reframe gates

Without this reframe + Phase F.2 stdlib work, Cobrust ships v0.2.0 as "Python successor #N with translation pipeline" — credible but not differentiated. With the reframe + Phase F.2:

- Cobrust ships v0.2.0-alpha as "AI-friendly Python successor with AI-native stdlib in development"
- Differentiates without over-claiming
- Acknowledges training-data risk and addresses it (M-AI.6 corpus generation, partnership outreach M-AI.7)
- Preserves the constitution's dual mandate (§1.1 language + §1.2 AI compiler)

## Options considered

### Option A — "First AI-native language" marketing

- Pros: maximum differentiation; bold claim.
- Cons: training-data chicken-egg unresolved (LLMs can't write Cobrust fluently today). First-contact failure ("the LLM emitted Rust when I asked for Cobrust") would burn credibility instantly. Over-claim.
- **Rejected.**

### Option B — "AI-friendly Python successor + AI-native stdlib in development" (CHOSEN)

- Pros: matches reality (50% already shipped); calibrated; acknowledges gap; gives a concrete roadmap (Phase F.2 milestones M-AI.0..M-AI.6).
- Cons: doesn't promise as much as A; requires the M-AI stdlib batch to land before v0.2.0-alpha tag.
- **Chosen.**

### Option C — no framing change, ship v0.2.0 as plain Python-successor

- Pros: zero framing risk.
- Cons: leaves Cobrust's actual differentiation unspoken; misses the strategic opening.
- **Rejected.**

## Decision

Adopt **Option B**. Concretely:

1. **README + `docs/post/why-cobrust.md`** updated with reframe language (1-2 paragraph diff per file, NOT a full rewrite). The headline becomes "AI-friendly Python successor with AI-native stdlib in development."
2. **Phase F.2 stdlib batch** (7 milestones, M-AI.0 through M-AI.6) per §Implementation map below.
3. **v0.2.0-alpha tag** binds to "M-AI.0..M-AI.6 closed + framing reframe shipped + release-readiness verify GO per ADR-0045". The `-alpha` suffix signals "AI-native stdlib direction, not yet stable surface."
4. **Async M-AI.7 partnership outreach** (training-data corpus contribution agreements with model providers) starts post-v0.2.0-alpha. Months-scale, NOT in this sprint.

## Implementation map

### M-AI.0 — `cobrust.llm` stdlib (D3 P9 opus + opus pair)

Source-level binding for the LLM Router crate. Surface (preliminary; P9 refines via spike commit):

```cobrust
import cobrust.llm

let response: str = cobrust.llm.complete(
    provider="anthropic",
    model="claude-sonnet-4-6",
    prompt="What is 2+2?",
)

for chunk in cobrust.llm.stream(provider="...", model="...", prompt="..."):
    print(chunk)

let response: str = cobrust.llm.dispatch(task="my_task", prompt="...")  # cobrust.toml [llm] routing
```

Reads `cobrust.toml` `[llm]` config. Wraps `crates/cobrust-llm-router` Rust API. Cost ledger emission per existing ADR-0031 provider_kind discipline. Estimated agent-time: 4-8 hr.

### M-AI.1 — `cobrust.prompt` stdlib (D2 P9 opus + sonnet pair)

PromptTemplate + few-shot composition + structured-output primitives:

```cobrust
import cobrust.prompt

let tmpl = cobrust.prompt.Template(
    system="You are a Cobrust expert.",
    user="Translate this Python to Cobrust: {code}",
    examples=[
        cobrust.prompt.Example(input="x = 1", output="let x: i64 = 1"),
    ],
)

let rendered: str = tmpl.render(code="def foo(): pass")
let result: dict = cobrust.llm.complete_structured(prompt=rendered, schema={"output": "str"})
```

Single-crate, well-scoped. Estimated: 2-4 hr.

### M-AI.2 — `cobrust.tool` stdlib (D3 P9 opus + opus pair, macro-heavy)

`@tool` decorator + LLM tool-calling + schema generator:

```cobrust
import cobrust.tool

@cobrust.tool.expose(description="Add two integers")
fn add(a: i64, b: i64) -> i64:
    return a + b

let schema: dict = add.schema()  # { name: "add", parameters: {...} }
let result: i64 = cobrust.tool.invoke(tool=add, args={"a": 1, "b": 2})  # → 3

let registry = cobrust.tool.Registry()
registry.register(add)
let response = cobrust.llm.complete_with_tools(prompt="What is 1 + 2?", tools=registry)
```

Macro work is delicate (procedural macro / reflection / multi-crate). Highest-risk milestone in α. Estimated: 6-10 hr. **CTO 守闸 mandatory** post-Phase-4 (per dispatch).

### M-AI.3 — `cobrust.eval` stdlib (D2 sonnet pair)

```cobrust
import cobrust.eval

let suite = cobrust.eval.Suite("translation_quality")
suite.add_case(input=..., expected=..., scorer=cobrust.eval.exact_match)
suite.add_case(input=..., expected=..., scorer=cobrust.eval.llm_judge(model="..."))

let report = suite.run()  # → pass_rate, per-case results
```

Estimated: 4-6 hr. Parallel with M-AI.4.

### M-AI.4 — `cobrust.ast` reader (D2 sonnet pair)

```cobrust
import cobrust.ast

let ast = cobrust.ast.parse_file("example.cb")
for fn in ast.functions:
    print(fn.name, fn.signature)
```

Estimated: 4-6 hr. Parallel with M-AI.3.

### M-AI.5 — AI-coding-eval benchmark suite (D2 sonnet pair)

~30-50 test programs exercising M-AI.0..M-AI.4 + measuring LLM-generation accuracy on Cobrust syntax vs Python baseline. This is **the 6th gate** (eval delta non-regression) per ADSD v1.2.0 evals-first reference. Estimated: 6-10 hr.

### M-AI.6 — Synthetic Cobrust corpus initial (D1 sonnet)

100-200 program samples covering core syntax + stdlib surface. Ongoing: monthly increments. Target: 1000+ programs by v0.3.0, ready for fine-tune partnership outreach (M-AI.7 async). Estimated: 4 hr initial.

## Backward compatibility

- All v0.1.x programs continue to compile. The new `cobrust.{llm,prompt,tool,eval,ast}` modules are additive — no existing identifier shadowed.
- `fn main() -> i64` signature unchanged.
- `cobrust.toml` schema extended with `[llm]` section (new); existing `[router]` / `[providers.*]` keys preserved.
- W2 wedge `input()` / `read_line()` / `argv()` source binding (ADR-0044) preserved.

## Test plan

- M-AI.0..M-AI.4: per ADR-0047a verify.py mandate, every test case ships with reference Python-equivalent + automatic oracle confirmation.
- M-AI.5: serves AS the 6th gate; defines acceptance criteria.
- Standard 5-gate green on each Wave merge.
- release-readiness verify before v0.2.0-alpha tag (per ADR-0045 + ADR-0046 tier-1 contract).

## Consequences

### Positive

- Cobrust differentiates without over-claiming
- Training-data risk acknowledged + addressed via M-AI.6 (synthetic corpus) + M-AI.7 (partnership outreach async)
- Phase F.2 milestones falsifiable (each M-AI.* has done-means)
- Post-v0.2.0-alpha customer dev gate fires per ADR-0045 (user-traction milestone)

### Negative

- 3-5 day W1 (AI translator) sideline while M-AI.* lands
- Partnership outreach (M-AI.7) is months-scale async; cannot accelerate via in-sprint work
- 7-milestone batch carries integration risk; mitigated by per-milestone PAIR pattern + CTO 守闸 at M-AI.2 + release-readiness verify

### Neutral / unknown

- Whether Cobrust-source `import cobrust.X` syntax requires module-path lowering (ADR-0044's deferred `std.env.args` case revisited) — Phase 2 spike commits clarify
- Whether macro work for `@cobrust.tool.expose(...)` needs new HIR forms — Phase 4 spike clarifies; CTO 守闸 catches scope creep
- Whether the 6th eval-delta gate (M-AI.5) lands meaningful regression detection at α scale (~50 programs) — empirically measured post-Phase-6

## Evidence

- review-claude strategic eval 2026-05-12 turn (post LC-100 Tier A close)
- LC-100 Tier A 99/100 stable @ HEAD `459b820` (ADR-0047 SKIP-back-to-W1 logic empirically fired)
- ADR-0036 production-validated prompt builder (M-AI.1 has existing precedent)
- ADR-0044 source-level binding pattern (M-AI.0..M-AI.4 reuse the same PRELUDE + intrinsic-rewrite mechanism)
- ADR-0047a verify.py mandate (Phase 2-7 sprints inherit)

## Cross-references

- ADR-0019 §"Phase F roadmap" — this ADR amends to add F.2 = α
- ADR-0038 §F.1 wedge "AI Python 加速器" — this ADR is the F.2 wedge "AI-native stdlib"
- ADR-0044 W2 source binding — Phase 2-7 pattern precedent
- ADR-0045 user-traction milestone gate — applies to v0.2.0-alpha tag
- ADR-0046 release.yml tier-1 contract — applies to v0.2.0-alpha asset upload
- ADR-0047 LC-100 coverage strategy — its SKIP-back-to-W1 fire is what enabled α
- ADR-0047a verify.py mandate — inherited by all Phase 2-7 P7-TEST sprints
- ADSD F23-B (synthetic distribution drift) — relevant to M-AI.5 benchmark design
- constitution §1.2 dual mandate — α delivers the language-half of the AI-native frame

## Why this ADR now

The β decision gate (ADR-0047) fired SKIP-back-to-W1 at 99/100 — meaning the language surface is sufficient for algorithmic Python. The next strategic opening is "what does Cobrust DO that other Python-successors don't?". Without this reframe + Phase F.2 stdlib, Cobrust ships v0.2.0 indistinguishable from Mojo / Codon / etc. With this reframe + Phase F.2, Cobrust ships v0.2.0-alpha as the calibrated AI-friendly Python successor with concrete stdlib differentiation — the position user explicitly reframed toward 2026-05-12.

ADR-0048 is the framing capture. Phase 2-7 are the execution. v0.2.0-alpha is the shipping. Total 3-5 days agent-time wall-clock per the dispatch's agent-velocity calibration.
