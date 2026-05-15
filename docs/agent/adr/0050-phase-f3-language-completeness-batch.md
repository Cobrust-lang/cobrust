---
doc_kind: adr
adr_id: 0050
title: "Phase F.3 — language completeness batch (dict, f64, list[str], break/continue, for) and v0.2.0 stable tag"
status: proposed
date: 2026-05-16
last_verified_commit: TBD
supersedes: []
superseded_by: []
relates_to: [adr:0003, adr:0006, adr:0019, adr:0023, adr:0024, adr:0025, adr:0027, adr:0030, adr:0034, adr:0038, adr:0044, adr:0044a, adr:0045, adr:0048, adr:0049]
discovered_by: P10 CTO user prioritization 2026-05-16 — "刷不了 leetcode" gap (W2 ADR-0044) compounded by "list[str] gap" (LC-100 Pattern B) + "no float" + "no dict" friction with first external users
ratification_path: in-session review (per user 2026-05-16 — independent audit via teammate, not separate session)
---

# ADR-0050: Phase F.3 — language completeness batch (dict, f64, list[str], break/continue, for) + v0.2.0 stable tag

## Context

### Strategic frame (2026-05-16)

ADR-0048 reframed Cobrust as **"AI-friendly Python successor with AI-native stdlib in development"** and merged the M-AI.0..M-AI.2 alpha flat-fn surfaces at `801eeb6`. ADR-0049 hardened those surfaces for external honesty at `85548f1`. The dual-mandate balance now leans heavy toward §1.2 AI-native compiler progress and underweights §1.1 language completeness.

Concretely, the project owner's 2026-05-16 prioritization names five language gaps that block "the language being a language":

```
P0:
├── dict             ← blocks everything, 2 weeks, depth task
├── f64              ← no floats = not a language, 1 week
├── list[str]       ← prerequisite for string processing, 1 week
├── break/continue  ← loops incomplete, 3 days
└── for loop         ← while is painful, 3 days

P1 (next):
├── 字符串全套 (split/join/replace/trim/find/contains)
├── 文件读写完整 (read/write/append/stdin/stdout)
├── JSON parser
└── error UX rewrite (F.1.4) → REPL (needs JIT) + LSP → LLVM backend
```

Each item maps to an existing constitution clause:

- **dict** — constitution §2.1 keeps Python comprehensions + iteration protocols; both presume `dict[K, V]` exists at the language tier.
- **f64** — constitution §1.1 "syntactically familiar to Python users" plus §2.1's "structural pattern matching"; first-class numeric tower presumes f64.
- **list[str]** — constitution §2.2 drop-list keeps "no late closure binding" + "no truthy/falsy"; the LC-100 Pattern B finding (`lc100-pattern-b-list-of-str-gap`) was deferred to ADR-0048 then to ADR-0049 then to Phase F.3. The TD-1 ownership debt (MIR `Ty::Str` / `Ty::List` are currently `Copy`) blocks honest `list[str]` semantics and must close inside this batch.
- **break/continue** — constitution §2.1 keeps indentation-based blocks + iteration protocols; loops without `break`/`continue` are incomplete by every Python-tradition standard.
- **for** — constitution §2.1 keeps iteration protocols + comprehensions; `while` as the only loop form is a permanent ergonomic tax.

### Existing W2 + Phase F.2 baseline

What is already shipped that this batch builds on:

| Surface | Anchor | Status |
|---|---|---|
| `print` / `println` / `print_int` / `print_no_nl` / `print_no_nl_lit` runtime intrinsic-rewrite + C-ABI shims | ADR-0025 + ADR-0030 + ADR-0044 + ADR-0047 Option H | ✅ shipped |
| `input(prompt)` / `input_no_prompt()` / `read_line()` / `argv()` PRELUDE+intrinsic-rewrite+C-ABI | ADR-0044 W2 Phase 2 + ADR-0049 input-str-buf fix | ✅ shipped |
| `parse_int` / `str_len` / `str_at` / `str_eq` / `str_eq_lit` / `str_ord` / `parse_int_tok` / `count_toks` | ADR-0044 W2 Phase 3 | ✅ shipped |
| `__cobrust_list_new` / `__cobrust_list_set` / `__cobrust_list_get` over `i64` slots | ADR-0044 W2 Phase 3 | ✅ shipped (i64 only) |
| `Constant::FnRef` Call lowering (recursive fn) | ADR-0034 M11.2 | ✅ shipped |
| `lower_condition` shared root primitive for `if` + `while` heads | ADR-0035 M11.3 | ✅ shipped |
| MIR `Aggregate` rvalue + `Ref` / `Cast` / for-protocol scaffolding | ADR-0027 M12.x | ✅ partially shipped; for-protocol intentionally placeholder |
| Cranelift backend object-structure assertions | ADR-0040 Wave 1A + Wave 2F M11 | ✅ shipped |

What this batch must add:

| Feature | Layers touched | Net new |
|---|---|---|
| **break/continue** | HIR Stmt + types (must be in loop) + MIR Terminator::Goto + codegen | small; reuses existing CFG plumbing |
| **for** | HIR ForStmt desugar (range-first, list-second) + types loop-var binding + reuse while/break/continue | medium; iter-protocol Phase 2 deferred |
| **f64** | lexer (literal forms) + parser FloatLit + HIR Ty::F64 + types (no implicit coercion per §2.2) + MIR F64 ops + codegen Cranelift fadd/fsub/fmul/fdiv/fcmp/fcvt + stdlib (floor/ceil/round/sqrt/sin/cos/pow/abs/min/max + format) + f-string {:.Nf} | large; touches every crate |
| **list[str]** | MIR `Ty::List<Ty::Str>` non-Copy + drop schedule + codegen list-of-Str + stdlib helpers + ADR-0050c Str ownership resolution | large; closes TD-1 |
| **dict** | HIR + types (`dict[K, V]` parametric) + MIR Aggregate::Dict + codegen via Rust HashMap C-ABI + Hash trait + iter-order + drop schedule + literal syntax + indexing + `in` operator | very large; ADR-0050d design first, impl in Wave 3 |

### Dependency graph

```
break/continue ─────┐
                    ├─→ for loop (uses break) ─────┐
ADR-0050c Str-own ──┴─→ list[str] ────────────────┤
                                                   ├─→ v0.2.0 stable
f64 ───────────────────────────────────────────────┤
ADR-0050d dict design ─→ dict impl ────────────────┘
```

- `break/continue` is a hard prerequisite for `for` (the desugar uses `break`).
- ADR-0050c **Str ownership** is a hard prerequisite for `list[str]` (a `list` of `Str` cannot be `Copy`).
- `f64` and `dict` are independent of every other batch item.
- All five are independent of P1 follow-ups, but P1 string stdlib depends on `list[str]` being honest.

### Constitution alignment

- §1.1 language-half mandate: this batch is the moment §1.1 catches up to §1.2's M-AI surfaces.
- §2.2 "drop from Python" — every new feature respects: no implicit coercion (`i64` ↔ `f64` requires explicit `as`), no truthy/falsy (`if d` requires `d: bool`; use `d.is_empty()` or `key in d`), no `is`-identity, `Result<T, E>` default error path.
- §3 doc-mandate: every public item in this batch ships zh + en + agent docs in the same commit.
- §5.1 elegant: dict / list[str] / f64 use newtypes where invariants exist; no `.unwrap()` in non-test code.
- §5.2 scientific: every design choice in this batch lives in ADR-0050a..d.
- §5.3 efficient: AOT default; dict backed by Rust `HashMap` C-ABI (no GC); list[str] drop schedule replaces leak-on-copy.

## Options considered

### Option A — sequential dispatch (one feature at a time)

- Pros: lowest peak memory pressure on Mac + DG; clear merge order; one ADR-PR pair at a time.
- Cons: ~6 weeks wall time minimum; loses the parallelizable structure of independent features; underutilizes self-hosted runner.
- **Rejected.** Per `feedback_autonomous_self_drive.md`, the default CTO mode is parallel-when-independent.

### Option B — depth-task-first (dict first, everything else queued)

- Pros: closes the "blocks everything" item earliest; smaller features pile up while dict design churns.
- Cons: 3-day quick wins (`break`/`continue` + `for`) stall behind a 2-week design task. The user-facing language stays awkward for the longest possible window.
- **Rejected.**

### Option C — three-wave parallel dispatch (CHOSEN)

- Pros: ships quick wins in week 1 while heavy design runs in parallel; matches the dependency graph above; mirrors ADR-0048 M-AI batch precedent.
- Cons: 3 concurrent P9 sprints + their P7 PAIRs hit DG simultaneously; per-runbook host routing the Mac is offload-only.
- **Chosen.**

### Option D — abandon Phase F.2 remaining M-AI.3..M-AI.6 work; pull language ahead of AI surfaces

- Pros: maximal P0-language focus.
- Cons: ADR-0048 v0.2.0-alpha tag binding becomes incoherent (M-AI.6 corpus + Phase 7.5 recursive types are still gating v0.2.0-alpha). Better to **resequence**, not abandon.
- **Rejected.** Phase F.3 supersedes the *priority* of M-AI.3..M-AI.6 but not their *existence*; they reframe as Phase F.4 post-v0.2.0.

## Decision

Adopt **Option C** — three-wave parallel dispatch on `feature/f3-*` branches off `main`, integrated wave-by-wave via `git merge --no-ff` after independent 5-gate verification on integrated `main`.

### Wave structure

| Wave | Branches dispatched in parallel | Duration | DG-load |
|---|---|---|---|
| **Wave 1** | `feature/f3-break-continue` (P9-A) · `feature/f3-for-loop` (P9-B) · `feature/f3-dict-design` (P9-C, ADR-only) | ~3-5 days | 2 P9 + 4 P7 PAIRs peak; dict-design is doc-only |
| **Wave 2** | `feature/f3-f64` (P9-D) · `feature/f3-str-ownership` (P9-E1) → `feature/f3-list-str` (P9-E2) | ~7-10 days | 2 P9 + 4 P7 PAIRs peak; Str-ownership ADR-0050c blocks list[str] impl |
| **Wave 3** | `feature/f3-dict-impl` (P9-F per ADR-0050d) | ~10-14 days | 1 P9 + multiple sequential P7 PAIRs across literal/index/iter/hash/drop sub-sprints |

### Sub-ADR slots

- **ADR-0050a — Loop control flow** (Wave 1, P9-A spike): break / continue semantics, label syntax (Cobrust drops Python `break label` per §2.2 minimalism; bare `break` / `continue` only; nested loops use innermost scope).
- **ADR-0050b — For-loop shape** (Wave 1, P9-B spike): range-first vs iter-protocol decision. Phase F.3 ships range-first + list-iter desugar; full iter protocol deferred to Phase G.
- **ADR-0050c — Str ownership** (Wave 2, P9-E1 spike, **TD-1 closure**): refcount vs proper Drop. Recommendation lock for `Drop`-by-default with explicit clone, mirroring Rust's `String`. Reasoning: ADR-0027 `Aggregate` rvalue + ADR-0044 `__cobrust_str_drop` schedule already presume a real Drop semantics; the Copy hack is a known shortcut.
- **ADR-0050d — Dict design** (Wave 1, P9-C spike): literal syntax `{k1: v1, k2: v2}`, indexing `d[k]`, `d[k] = v`, `key in d`, `for k, v in d.items()`, `.keys()` / `.values()` / `.items()` iter; backed by `std::collections::HashMap` via 8 C-ABI shims; iteration preserves Python 3.7+ insertion-order semantics.

### Implementation map

#### M-F.3.0 — break / continue (Wave 1, P9-A, D2, 3 days)

- ADR-0050a spike + impl.
- HIR: `Stmt::Break { label: None }` / `Stmt::Continue { label: None }`.
- Types: must be inside a loop scope; reject at well-typed gate.
- MIR: lowering emits `Terminator::Goto(loop_exit_bb)` / `Terminator::Goto(loop_header_bb)` with loop-scope stack maintained during HIR→MIR.
- Codegen: no change (Cranelift already supports cross-BB Goto).
- Corpus: ≥30 well-typed (single loop / nested / inside if / after early return) + ≥20 ill-typed (outside loop, in fn top-level, etc.).
- Done means: corpus 0-fail, 5-gate green, examples/early_exit.cb runs.

#### M-F.3.1 — for loop (Wave 1, P9-B, D2-D3, 3 days)

- ADR-0050b spike + impl.
- HIR: `Stmt::For { var, iter, body }` desugars to either:
  - `for i in range(a, b):` → `let mut i = a; while i < b: body; i += 1` (range-first, intrinsic-recognized).
  - `for x in xs:` where `xs: list[i64]` → index-based while desugar.
  - `for x in xs:` where `xs: list[str]` → blocked on Wave 2 list[str]; Wave 1 ships range + list[i64] only.
- Types: var binding to iter element type; iter expression must be `range(a, b)` or `list[T]`.
- Codegen: no change; desugars to existing while+break.
- Corpus: ≥30 well-typed + ≥20 ill-typed (iter not iterable, var-shadowing).
- Done means: corpus 0-fail, 5-gate green, examples/for_range.cb + for_list.cb run.

#### M-F.3.2 — list[str] (Wave 2, P9-E2, D4, 1 week, blocks on ADR-0050c)

- ADR-0050c Str ownership lands first.
- MIR: `Ty::Str` and `Ty::List<Ty::Str>` flip from `Copy` to non-Copy with explicit `drop_eligible` per ADR-0027 schedule.
- Codegen: `__cobrust_list_new(8, len)` slots store `*mut StringBuffer`; element drop emits `__cobrust_str_drop` per slot before `__cobrust_list_drop`.
- Stdlib: `__cobrust_list_str_get` / `__cobrust_list_str_set` / `__cobrust_list_str_push` / `__cobrust_list_str_len`.
- Corpus: ≥40 well-typed (push/pop/iterate/index) + ≥20 ill-typed (mixed types, out-of-bounds, drop-after-move).
- Done means: corpus 0-fail, 5-gate green, examples/list_str_split.cb runs, no Str leaks under valgrind on a representative program.

#### M-F.3.3 — f64 (Wave 2, P9-D, D4, 1 week)

- Lexer: `1.0`, `.5`, `1e10`, `1.5e-3`, `0.1`, `inf`, `nan` tokens.
- Parser: `Constant::F64(f64)` (use total-ordering wrapper or document IEEE 754 nan != nan compliance per constitution §2.2 no-silent-coercion).
- HIR: `Ty::F64`.
- Types: arithmetic ops `F64 × F64 → F64`; comparison ops obey IEEE 754 partial order; **no implicit `i64 ↔ f64` coercion** (per §2.2); explicit `as` cast required.
- MIR: F64 constants, F64Add/Sub/Mul/Div/Rem opcodes, F64Cmp(Lt/Le/Gt/Ge/Eq/Ne), F64ToI64 / I64ToF64 / F64ToF64 casts.
- Codegen: Cranelift `F64` ir type, `fadd`/`fsub`/`fmul`/`fdiv`/`fcmp`, `fcvt_to_sint`/`fcvt_from_sint`.
- Stdlib: `floor`/`ceil`/`round`/`sqrt`/`sin`/`cos`/`tan`/`pow`/`abs`/`min`/`max` + f-string `{:.Nf}` / `{:e}` / `{:g}`.
- Corpus: ≥60 well-typed (arithmetic / comparison / cast / format) + ≥30 ill-typed (implicit coerce, NaN equality assumption, divide-by-zero behavior) + IEEE 754 corner cases (0.1+0.2 ≠ 0.3 verified, NaN != NaN verified, ±∞ ordering verified).
- Done means: corpus 0-fail, 5-gate green, examples/{circle_area,mandel,float_format}.cb run.

#### M-F.3.4 — dict (Wave 3, P9-F per ADR-0050d, D5, 2 weeks)

- ADR-0050d lands first (Wave 1).
- Lexer + parser: `{k1: v1, k2: v2}` literal; empty `{}`; `d[k]` index; `d[k] = v` assign; `key in d` membership; `for k, v in d.items():` iter.
- HIR: `Aggregate::Dict { items: Vec<(Expr, Expr)> }` rvalue; type is `dict[K, V]` parametric.
- Types: `K` must implement `Hash` trait (`i64` and `str` ship in Phase F.3; `f64` rejected because NaN); `V` is any type.
- MIR: `Aggregate::Dict` + `DictGet` / `DictSet` / `DictContains` / `DictIter` rvalues + drop schedule.
- Codegen: 8 C-ABI shims backed by Rust `HashMap<KeyEnum, ValueEnum>` or specialized `HashMap<String, *mut V>`:
  - `__cobrust_dict_new() -> *mut Dict`
  - `__cobrust_dict_drop(*mut Dict)`
  - `__cobrust_dict_set_i64_i64(*mut Dict, i64 key, i64 val)`
  - `__cobrust_dict_set_str_i64(*mut Dict, *mut Str key, i64 val)`
  - (...similar for other K, V combinations, type-driven dispatch)
  - `__cobrust_dict_get_*` / `__cobrust_dict_contains_*` / `__cobrust_dict_len` / `__cobrust_dict_iter_*`
- Iter order: Python 3.7+ insertion order; backed by `LinkedHashMap` or `IndexMap` (`indexmap` crate already in workspace dep tree? check; if not, add).
- Corpus: ≥80 well-typed + ≥40 ill-typed + ≥20 differential vs Python 3.10 reference on insertion-order programs.
- Done means: corpus 0-fail, 5-gate green, examples/{word_count,lookup_table,json_obj}.cb run.

#### P1 follow-ups (post-P0, queued for Phase F.3 §"P1 wave")

- **M-F.3.5 string stdlib bundle** — `split` / `join` / `replace` / `trim` / `find` / `contains` / `starts_with` / `ends_with` / `lower` / `upper`. **Blocks on list[str]** for `split` return type. ~3-5 days, D2-D3.
- **M-F.3.6 file IO completion** — `read_file_lines() -> list[str]` / `append_file` / fully-binding `stdin().read_all()` / fully-binding `stdout().write` at source level. ~2-3 days, D2.
- **M-F.3.7 JSON parser** — `cobrust.json` stdlib: `parse(s: str) -> dict | list | str | f64 | i64 | bool | None` / `stringify`. Blocks on dict + list[str] + f64. ~5-7 days, D3.
- **M-F.3.8 error UX rewrite (F.1.4) + REPL + LSP** — multi-sprint:
  - F.1.4 error UX: ANSI-rendered errors with span carets and "did you mean" suggestions. ~3-5 days, D3.
  - REPL completion: needs JIT-aware Cranelift mode for instant feedback. ~7-10 days, D4. Connects to ADR-0029 M14 base.
  - LSP: separate `cobrust-lsp` crate; document-sync + hover + go-to-def + diagnostics. ~10-14 days, D4.
- **M-F.3.9 LLVM backend swap** — ADR-0023 §"M9.1 LLVM backend full lowering" is the formal track. Cranelift → LLVM swap targets near-Rust perf. **Out of Phase F.3 scope**; Phase F.5+.

### v0.2.0 stable tag binding

v0.2.0 stable tag (Cobrust-lang/cobrust) binds to:

1. M-F.3.0..M-F.3.4 all closed (5-gate green on integrated `main`).
2. ADR-0050a..d all `accepted` with `last_verified_commit` ≠ `TBD`.
3. Phase F.3 P1 §M-F.3.5 + M-F.3.6 closed (string + file-IO completeness; JSON parser optional for v0.2.0, target v0.2.1).
4. Release-readiness P7 sonnet GO per ADR-0045 (clean-shell install + first 3 examples).
5. AI alpha surfaces from ADR-0048 + ADR-0049 stay labeled `(alpha)` in README; v0.2.0 stability covers the language half only.

Resequence relative to ADR-0048's v0.2.0-alpha plan:

- ADR-0048 bound v0.2.0-alpha to M-AI.0..M-AI.6 + Phase 7.5 close.
- **Resequence**: M-AI.3..M-AI.6 + Phase 7.5 → reframed as **Phase F.4** post-Phase-F.3, no v0.2.0-alpha intermediate tag. v0.2.0 stable ships directly from `0.1.2` once Phase F.3 P0 + the two listed P1 items close. M-AI.0..M-AI.2 stay alpha-labeled inside v0.2.0.
- ADR-0048 status frontmatter remains `accepted`; the resequencing is noted in `relates_to` here and in a one-line marker added to ADR-0048's `relates_to` list.

### Dispatch routing per `feedback_heavy_build_offload_to_workstation.md`

| Wave | Workload | Host | Mode |
|---|---|---|---|
| Wave 1 break/continue | D2, single-crate touch | Mac local then DG verify | Mode C |
| Wave 1 for-loop | D2-D3, HIR + MIR | Mac local then DG verify | Mode C |
| Wave 1 dict-design | D5 doc-only | Mac local | direct |
| Wave 2 f64 | D4, full-vertical | DG primary | Mode C |
| Wave 2 Str-ownership ADR | D5 doc-only | Mac local | direct |
| Wave 2 list[str] | D4, structural | DG primary | Mode C |
| Wave 3 dict impl | D5, multi-sprint | DG primary | Mode C |

Every cargo-build-workspace + workspace-test invocation runs on DG (`ssh -p <runner-port> <runner-user>@<runner-ip>`).

### Audit model — teammate-in-session (user-mandated 2026-05-16)

ADR-0048 ratification used review-claude in a separate Claude Code session. Per user 2026-05-16, audits no longer require a separate session: P10 spawns a read-only external-review **teammate** agent at the wave-completion gate using `Agent(subagent_type=general-purpose, model=opus, …)` with:

- Working dir = current main HEAD (read-only)
- Mission = audit ADR-0050 + merged wave deltas vs ADSD methodology + constitution §2.2 + §5 + §6 + finding catalogue (`feedback_third_party_audit_2026_05_09.md` baseline).
- Constraint = "do NOT write to main; only read + draft + suggest" per ADSD §External-review.
- Output = `[EXTERNAL-REVIEW-VERDICT]` GO / BLOCK-WITH-FINDINGS / OBSERVATIONS.

The teammate runs in parallel with the next wave's dispatch.

## Consequences

- **Positive**
  - §1.1 language half catches up to §1.2 AI-native progress; Phase F.3 closes 5 P0 language gaps blocking external user trial.
  - TD-1 Str ownership debt (Phase F.2.x candidate) gets a load-bearing ADR (ADR-0050c) instead of further deferral.
  - LC-100 Pattern B (`lc100-pattern-b-list-of-str-gap`) finally closes after triple-deferral through ADR-0047→0048→0049.
  - v0.2.0 stable tag binds to *language-half* completeness, which is what external users measure first.
  - Phase F.3 P1 §M-F.3.5..M-F.3.7 unblocks practical Python-shape user programs (string processing + JSON parsing + file IO).
  - Mirrors ADR-0048 batch precedent without re-inventing the topology.

- **Negative**
  - Three concurrent P9 sprints in Wave 1 + two in Wave 2 burn Opus tokens at a higher rate than the ADR-0048 single-batch pattern.
  - M-AI.3..M-AI.6 + Phase 7.5 (ADR-0048 binding) defer by ~4-5 weeks. Phase F.4 must explicitly pick them back up.
  - dict design (ADR-0050d) is a 1-2 day P9 opus spike that can produce a `BLOCK-WITH-FINDINGS` outcome; if so, Wave 3 dispatch slips and the v0.2.0 timeline moves.
  - The Str-ownership flip (ADR-0050c) touches every MIR drop schedule and may surface latent leak bugs in existing W2 corpus; this is a *known unknown* requiring Wave 2 PAIR-test discipline.

- **Neutral / unknown**
  - ADR-0050c's exact ownership shape (refcount vs proper Drop) is open until the P9-E1 spike. The recommendation lock is `Drop`-by-default, but the spike may surface a refcount escape hatch for shared-state ergonomics.
  - dict literal syntax `{k: v, …}` collides with the existing block-syntax in some parser ambiguities; ADR-0050d must resolve this.
  - f64 NaN-equality semantics: constitution §2.2 forbids silent coercion but does NOT forbid IEEE 754 `NaN != NaN`. ADR-0050 ratifies "follow IEEE 754 strictly; document the surprise in zh/en getting-started".

## Evidence

- User prioritization 2026-05-16 — verbatim in §"Strategic frame" P0/P1 lists.
- LC-100 Pattern B finding — `docs/agent/findings/lc100-pattern-b-list-of-str-gap.md`.
- TD-1 Str ownership debt — surfaced by ADR-0027 §"Consequences" + P7 opus Phase 2 DEV honest disclosure during W2; carried forward to Phase F.2.x candidate status.
- ADR-0048 batch precedent — `docs/agent/adr/0048-ai-native-framing-reframe.md` §"Implementation map" + 9-surface atomic Phase 8.
- ADSD methodology — `https://github.com/Cobrust-lang/agent-driven-development` (P10 role boundaries + 5-gate verification + two-phase dispatch SOP + external-review constraint).
- Host routing memo — `feedback_heavy_build_offload_to_workstation.md`.
- Sub-agent model tier rule — `feedback_subagent_model_tier.md` (D-matrix + Opus/Sonnet/Haiku binding).
- Dev/Test PAIR pattern — `cto_operations_runbook.md` §"Dev/test pair pattern (2026-05-11+ MANDATORY for D1-D3 + D5)".

## Amendment 2026-05-16 — Audit verdict + scope correction (ADSD F2 addendum-not-rewrite)

Per ADSD §F2 (no retroactive ADR rewrite), the original §"Existing W2 + Phase F.2 baseline" + §"What this batch must add" + §"v0.2.0 stable tag binding" text above stays as the **historical record** of the spike-time understanding. This amendment captures the verified-at-HEAD scope correction surfaced by the pre-impl audit teammate run 2026-05-16 (agent `afe53e8f443c7ec32`, verdict `BLOCK-WITH-FINDINGS`, 5-lane review).

### A1 — Verified-at-HEAD scope correction (audit Findings 2.1 + 2.2 + 2.3)

Three of the five P0 features are **substantially already shipped** on `main@30cf2b2`. The audit cross-verified this against P9-A's spike (ADR-0050a on `feature/f3-break-continue`) and P9-B's spike (ADR-0050b on `feature/f3-for-loop`); both spikes independently arrived at the same conclusion before this amendment landed. The net-new deltas are smaller than the original "What this batch must add" table implied:

| Feature | Original "Net new" assumed | Verified-at-HEAD reality | Real Wave-1 work |
|---|---|---|---|
| **break/continue** | small; reuses existing CFG plumbing | **already shipped end-to-end** (lexer KwBreak/KwContinue + AST `StmtKind::BreakContinue(BreakKind)` + parser + HIR `StmtKind::Break/Continue` + types `loop_depth` reject-if-0 + MIR `loop_stack` Goto + Cranelift Goto). Codegen diff corpus `diff_form_16_break` + `diff_form_16_continue` already pass. | **Contract seal** (ADR-0050a) + ≥30-test corpus + ill-typed rejection corpus. No new lowering. |
| **for** | medium; iter-protocol Phase 2 deferred | **for-protocol operational** over list[i64] and list[str]-via-W2-reinterpret (`__cobrust_iter_init` / `_next` / `_drop` shipped per ADR-0044 W2 Phase 2 amendment; MIR `lower.rs:726-836` does full lowering). | **Plug `range(a, b)`** as a new iter source via PRELUDE+intrinsic-rewrite (mirrors `input` / `argv` precedent), document `for x in xs: list[str]` already works **read-only**, leak-safety on str element drop blocks on Wave 2 list[str]. |
| **f64** | large; touches every crate (lexer + parser + HIR Ty::F64 + types + MIR F64 ops + codegen fadd/fcvt + stdlib math + f-string {:.Nf}) | **80% shipped**: `Ty::Float` (types/ty.rs:43-45), `Constant::Float(u64)` (mir/tree.rs:313-314), full Cranelift F64 codegen (fadd/fsub/fmul/fdiv/fcmp + fcvt_to_sint/fcvt_from_sint at cranelift_backend.rs:305..2132), lexer Float token incl. `1.5e-3` (lexer.rs:564), Rust-side `crates/cobrust-stdlib/src/math.rs` ships sqrt/pow/sin/cos/abs/floor/ceil/round + PI/E, `__cobrust_fmt_float` exists (fmt.rs:115-121). | **Remaining gap (D2 sonnet scope, not D4 opus 1-week)**: (a) source-level `as` cast expression (parser does not have it; lexer has `KwAs` only in import-alias context), (b) PRELUDE+intrinsic-rewrite for `sqrt`/`floor`/`ceil`/`round`/`sin`/`cos`/`pow`/`abs`/`min`/`max` (math fns are not yet callable from `.cb`), (c) f-string `{:.Nf}` lowering, (d) `inf` / `nan` lexer literals, (e) NaN total-ordering newtype decision per Finding 1.4. |
| **list[str]** | large; closes TD-1 | **TD-1 debt is real** (`mir/lower.rs:1677-1686` + `mir/drop.rs:122-129` both treat `Ty::Str \| Ty::List(_)` as Copy with explicit ADR-0044 W2 Phase 3 comment). Iteration already works **read-only** via W2 reinterpret. | **No change to scope** — ADR-0050c Str-ownership flip remains the load-bearing Wave 2 work. List-of-str iter drop-correctness gates on it. |
| **dict** | very large; ADR-0050d design first, impl Wave 3 | **60% scaffolded** (parser literal `{k: v}` at parser.rs:1470; AST DictLit at ast.rs:390; type universe at ty.rs:65; type-check synth at check.rs:614; MIR `Aggregate` lowering at mir/lower.rs:1111-1137; M12.x stub `__cobrust_dict_{new,set,get,len,drop}` C-ABI for `Dict<i64,i64>` at stdlib/collections.rs:534-636). | **No change to scope** — Wave 3 swaps the M12.x `HashMap<i64,i64>` stub for `indexmap::IndexMap<KeyEnum, ValueEnum>` with type-dispatched shims + wires source-level indexing + iteration + methods. ADR-0050d Decision 6A pins `indexmap = "2"`. |

### A2 — Wave timing revision

Original §"Wave structure" estimated ~3-5 days Wave 1, ~7-10 days Wave 2, ~10-14 days Wave 3 = ~4-5 weeks total.

Revised given A1 verified-at-HEAD reality:

| Wave | Original | Revised | Reason |
|---|---|---|---|
| 1 (break/continue + for-loop + dict-design ADR) | 3-5 days | **1-2 days** | Wave 1 is contract-seal + ADR text + corpus; impl scaffolding already shipped |
| 2 (f64 + ADR-0050c Str-ownership + list[str]) | 7-10 days | **3-5 days** | f64 shrinks from D4 to D2; Str-ownership ADR-0050c is doc-only (P9 solo); only list[str] impl is heavy |
| 3 (dict impl per ADR-0050d) | 10-14 days | **10-14 days** | Unchanged — dict is the real opus-tier work this batch contains |

**Total batch ≈ 2-3 weeks**, not 4-5. Opus budget reallocates accordingly: P9-D f64 sprint downgrades from D4 opus to **D2 sonnet** + Mode-C-Mac+DG-verify (not DG-primary). P9-F dict impl is the only D5 sprint that truly needs opus + heavy DG.

### A3 — v0.2.0 stable tag binding clarification (audit Finding 5.3)

The original §"v0.2.0 stable tag binding" defers M-AI.3..M-AI.6 + TD-Recursive-Types Phase 7.5 to **Phase F.4** but does not explicitly state whether Phase 7.5 *blocks* v0.2.0 stable. ADR-0048 §"v0.2.0-alpha tag" L96 originally made Phase 7.5 a P0 blocker because it closes ADSD F24 (primitive-as-everything-simulation).

**Amendment**: Phase 7.5 (recursive struct types like `class Tree(val, left, right)`) **does NOT block v0.2.0 stable**. Reasoning:

1. Phase F.3's `dict[str, list[str]]` + `list[str]` together substantially close the F24 user-facing ergonomic gap: recursive-shaped data (trees, linked lists, graphs) can be modeled as `dict[i64, NodeRecord]` with i64 IDs as pointers.
2. The LC-100 Pattern B finding that drove F24's P0 framing is closed by `list[str]` shipping; the remaining LC-100 primitive-simulation cases all become idiomatic via dict-keyed indirection.
3. Native recursive struct syntax (Phase 7.5) is a Phase F.4 ergonomic improvement, not a stability blocker.
4. v0.2.0 stable continues to gate on §1.1 language-half completeness as defined in the original tag binding (M-F.3.0..M-F.3.4 + M-F.3.5 + M-F.3.6) without growing the scope.

### A4 — ADR-0050d follow-up (audit Finding 1.2)

ADR-0050d (`feature/f3-dict-design@8466433`) already addresses audit Findings 3.5 (indexmap pin at `"2"`) and 3.4 (f64-key NaN rejection cross-referenced to ADR-0050 §M-F.3.3). The surviving audit gap is **Finding 1.2**: `dict.is_empty() -> bool` is not yet pinned in the dict surface. Constitution §2.2 forbids implicit truthy/falsy, so without `is_empty()` users have no idiomatic path to `if d.is_empty(): …`. Addendum landed in a separate commit on `feature/f3-dict-design` before Wave 3 dispatches; pin recorded in ADR-0050d §"Decision 5 — Length / emptiness".

### A5 — ADSD F27 candidate (audit Lane 5 ADSD discipline check)

Audit surfaced an ADSD-upstream candidate: **F27 — ADR scope-reality divergence**. ADR-0050 §"Implementation map" cited work-needed across crates without source-code verification pre-acceptance; P9-A and P9-B independently re-discovered scope-already-shipped in their spikes. Recommended ADSD upstream addition: an "ADR pre-dispatch source-code verification gate" alongside the two-phase dispatch SOP. Cobrust mirrors this as a finding at `docs/agent/findings/adr-scope-reality-divergence.md`.

### A6 — Wave 2 dispatch hold

Wave 2 (f64 + Str-ownership + list[str]) **HOLDS** until this amendment lands on `main`. Wave 1 P9-A and P9-B continue uninterrupted — both have already self-corrected scope on their own branches and need no SendMessage redirection per audit recommendation. The audit `BLOCK-WITH-FINDINGS` verdict gated on this amendment + the dict-design `is_empty()` addendum; both ship now.

### A7 — PAIR pattern shift (user 2026-05-16 — ADSD F28 candidate)

User surfaced a structural ADSD-vs-Claude-Code-architecture gap during Wave 1 dispatch review: **Claude Code sub-agents are single-layer** — a P9 dispatched via `Agent(subagent_type=general-purpose)` does NOT have the `Agent` tool and therefore cannot literally spawn P7-TEST + P7-DEV sub-agents. The "P9 dispatches PAIR" ceremony copied verbatim from `cto_operations_runbook.md` §"Dev/test pair pattern" into Wave 1 P9-A and P9-B prompts was structurally void; both sprints fell back to single-Opus solo work doing TEST + DEV in one pass. Same-agent bias is retained — the very thing the PAIR pattern was designed to eliminate.

Wave 1 mitigation (limited damage):

- P9-A break/continue spike at `1998dbe` and P9-B for-loop spike at `909811f` are honest contract-seal + corpus work. The audit teammate verified the scope is narrow (≥30-test corpus + ill-typed corpus + ADR text). Single-Opus bias risk for contract-seal-narrow sprints is bounded — the impl is already shipped (per §A1) so the corpus only has to probe semantics, not validate independently-authored impl.
- The post-Wave-1 audit teammate spawned at merge time gains an explicit assignment: verify the test corpus exercises real semantics (not just type-check happy paths) + verify edge-case coverage looks like independent thinking. This is the retrofit mitigation pathway for Wave 1's single-Opus PAIR-ceremony.

Wave 2 + Wave 3 dispatch pattern lock (binding 2026-05-16+):

- **P10 directly dispatches TEST agent + DEV agent as two parallel `Agent(...)` calls.** No P9 intermediary for impl sprints.
- TEST prompt: forbidden to edit impl files; reports `[TEST-CORPUS-READY]` with paths + assertion counts + commit SHA.
- DEV prompt: forbidden to edit TEST corpus files (or fence with `DO NOT EDIT — TEST-AGENT-OWNED` comments); requires TEST commit SHA + paths as required reads.
- P10 acts as coordinator: reviews TEST corpus, SendMessage 补 if needed, then dispatches DEV.
- P9 layer is preserved for **ADR-authoring sprints + strategic decomposition only** (D4 / D5 design-only, like P9-C dict design ADR-0050d which correctly used P9 solo and was unaffected).

Per-Wave-2 sprint dispatch shape:

| Sprint | Pattern |
|---|---|
| ADR-0050c Str-ownership design | P9 solo opus (doc-only); no PAIR. |
| M-F.3.3 f64 (revised D2 sonnet) | **P10-direct PAIR**: TEST sonnet + DEV sonnet, parallel. |
| M-F.3.2 list[str] (D4 opus) | **P10-direct PAIR**: TEST opus + DEV opus, parallel. |
| M-F.3.4 dict impl per ADR-0050d (D5, multi-sub-sprint) | **P10-direct PAIR per sub-sprint** (parser / types / MIR / codegen / iter / drop / doc), staggered. |

ADSD upstream candidate: **F28 — PAIR pattern impl gap under single-layer sub-agent architecture**. Filed at `docs/agent/findings/adsd-pair-pattern-impl-gap.md` alongside F27. Proposed methodology fix: ADSD §"Dev/test pair pattern" should declare its implementation-layer responsibility explicitly — under multi-layer agent platforms P9 dispatches PAIR; under single-layer platforms P10 directly dispatches and coordinates.
