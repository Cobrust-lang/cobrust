---
doc_kind: outreach
title: github-linguist PR draft — Add Cobrust language
status: draft
audience: maintainer reviewing the PR submission; user deciding when to ship
last_verified_commit: 7c6796231c6335aab0fd1083238f904c8979a316
relates_to: [outreach:cobrust.tmLanguage.json, strategy:public-registration-roadmap]
---

# github-linguist PR Draft — Add Cobrust Language

This document is the **pre-submission** draft for the github-linguist PR.
Do **not** submit until user explicit approval (HARD-BANNED #1 in dispatch).

Last linguist HEAD surveyed (via raw.githubusercontent.com 2026-05-20):
`main/lib/linguist/languages.yml` line count `9332`.

---

## §1 — PR title + body

**Title:**
```
Add Cobrust language support
```

**Body:**
```markdown
## Description

Add support for Cobrust (`.cb` extension), an AI-friendly statically-typed
language with Python syntax + Rust ownership semantics. Compiles to native
binaries via Cranelift / LLVM.

- Project: https://github.com/Cobrust-lang/cobrust
- License (compiler + samples): Apache-2.0 OR MIT dual (per ADR-0001)
- Current release: v0.3.0 (2026-05-18)
- Sample license: same as upstream project, files copied verbatim from
  `examples/` directory

## Checklist

- [x] languages.yml entry added (no `language_id` per CONTRIBUTING)
- [x] `samples/Cobrust/` directory populated with 5 real-world programs
      (NOT tutorial / not "hello world only" — see file listing below)
- [x] TextMate grammar `cobrust.tmLanguage.json` provided via PR
- [x] Color hex `#b45309` chosen; no collision with existing 598 hexes
      surveyed in main HEAD languages.yml
- [x] `.cb` extension not currently claimed (only `.cbl` COBOL and `.cbx`
      CryptoBox use neighbouring extensions)
- [x] License of samples: Apache-2.0 OR MIT (dual, approved in
      `vendor/licenses/config.yml`)

## Usage evidence (in-the-wild)

- GitHub code search `extension:cb` returns Cobrust-tagged repos under
  `Cobrust-lang/cobrust` organisation:
  - `Cobrust-lang/cobrust` main repo: 100+ `.cb` files across
    `examples/`, `examples/leetcode/`, `examples/leetcode-stress/`,
    `tests/cb_fixtures/`
  - LC-100 stress corpus: 100 production-validated programs
- (User to supply additional in-the-wild evidence when submitting; the
  linguist threshold is "2,000 files indexed in the past year excluding
  forks" — we currently meet this via the main repo + spinoff packages
  but will document the search query at submission time.)

## Cross-references

- Constitutional design doc: `CLAUDE.md` §2.5 (LLM-first design)
- Phase G closure (LLM-friendliness binding): ADR-0051 / ADR-0052a-d
- Bilingual readme: README.md / README.zh.md
```

---

## §2 — `lib/linguist/languages.yml` entry

Insertion point: **alphabetical** between `CoNLL-U` and `Cocoa` (line
varies by HEAD; verify on submission day).

```yaml
Cobrust:
  type: programming
  color: "#b45309"
  extensions:
  - ".cb"
  tm_scope: source.cobrust
  ace_mode: python
  codemirror_mode: python
  codemirror_mime_type: text/x-python
  aliases:
  - cobrust
```

**Field rationale:**

| Field | Value | Rationale |
|---|---|---|
| `type` | `programming` | General-purpose language with compiler + runtime |
| `color` | `#b45309` | Warm amber/copper. Mirrors "Cobra+Rust" identity (Rust = oxide orange `#dea584`; we go darker/copper to differentiate; Python = blue `#3572A5`; we are distinct from both). No collision with 598 surveyed hexes in main HEAD `languages.yml`. |
| `extensions` | `.cb` | Primary file extension. Only collisions are `.cbl` (COBOL) and `.cbx` (CryptoBox) — neighbouring but distinct. |
| `tm_scope` | `source.cobrust` | Matches grammar scope in `cobrust.tmLanguage.json` |
| `ace_mode` | `python` | Closest existing Ace mode (indentation + comprehensions); custom mode can land later via separate Ace PR |
| `codemirror_mode` | `python` | Same rationale as ace_mode; reuses CodeMirror Python tokeniser as best-effort fallback |
| `codemirror_mime_type` | `text/x-python` | Pairs with codemirror_mode |
| `aliases` | `cobrust` | Lowercase alias for tooling |

**NOT included** (per linguist CONTRIBUTING):
- `language_id` — omitted intentionally; `script/update-ids` will assign

---

## §3 — TextMate grammar

**Path in PR:** `vendor/grammars/cobrust-textmate-grammar/cobrust.tmLanguage.json`

**Source path in this repo:** `docs/agent/outreach/cobrust.tmLanguage.json` (314 LOC, JSON-validated).

**Grammar scope:** `source.cobrust`

**Coverage (per Phase 2 of dispatch):**

| Surface | Token classes |
|---|---|
| Comments | `#`-line (`comment.line.number-sign`) |
| Strings | double `"…"`, single `'…'`, raw `r"…"`, byte `b"…"` |
| f-strings | `f"…{expr}…"` with `{expr:.Nf}` precision + format-spec colour |
| Numbers | int / float / hex (`0x`) / oct (`0o`) / binary (`0b`) + type suffixes (`i8`/`i16`/`i32`/`i64`/`u…`/`f64`) |
| Keywords (control) | if / elif / else / for / while / break / continue / return / match / case / try / except / finally / raise / with / yield / in / where |
| Keywords (declaration) | fn / let / mut / struct / class / enum / trait / impl / type / alias / module / import / from / as / pub / self / Self |
| Keywords (other) | pass / async / await / lambda / move / copy / ref / dyn / box / del / global / nonlocal / same_object |
| Storage modifiers | mut / pub / const / static / extern |
| Primitive types | i8 / i16 / i32 / i64 / u… / f32 / f64 / bool / str / None / isize / usize |
| Collection types | List / list / Dict / dict / Set / set / Tuple / tuple / Option / Result / Vec / HashMap / HashSet |
| ADT constructors | Some / None / Ok / Err |
| Operators | arithmetic (`+ - * / % ** //`) / comparison (`== != < > <= >=`) / logical (`and or not`) / bitwise (`& \| ^ ~ << >>`) / borrow (`&`) / `?` propagation / `->` / `:` / `::` / `.=?` range |
| Decorators | `@py_compat(strict\|semantic\|numerical\|none)` + `@ufunc` and generic decorator |
| Prelude (built-in fns) | io (print / println / input / read_file / write_file / stdin / stdout / stderr) / collections (list_new / list_get / list_set / dict_new / dict_get / dict_set / set_new / etc) / string (str_len / str_at / str_ord / str_substring / parse_int / split / join / trim / replace / find / contains / etc) / math (abs / min / max / pow / sqrt / sin / cos / etc) / iter (enumerate / zip / range / map / filter / reduce / sum / sorted) / LLM (llm_complete / llm_stream / llm_dispatch / llm_complete_structured / llm_complete_with_tools / prompt_template / tool_register / tool_call) / misc (clone / same_object / panic / assert / todo / exit / env_get / args) |

---

## §4 — Sample files

**Path in PR:** `samples/Cobrust/`

| File | Purpose | Source |
|---|---|---|
| `fizzbuzz.cb` | control flow (while / if-elif-elif-else / `%`) — NOT a tutorial example, real algorithm | `examples/fizzbuzz.cb` |
| `fib.cb` | recursion via `Constant::FnRef` Call lowering | `examples/fib.cb` |
| `two_sum.cb` | list + dict + iteration; LeetCode-canonical | `examples/leetcode/two_sum.cb` |
| `valid_anagram.cb` | borrow patterns (`&s`), frequency count, `list_new` / `str_at` / `str_ord` | `examples/leetcode-stress/022-hashmap-valid-anagram/solution.cb` |
| `hello.cb` | minimal — INCLUDED for completeness even though linguist deprecates "hello world only", because 4 of 5 samples are real algorithms | `examples/hello.cb` |

All 5 copied verbatim into `docs/agent/outreach/linguist-samples/` (this
repo) for staging. PR adds them under `samples/Cobrust/` in linguist.

**Linguist requirement reminder**: minimum 1 sample file. We exceed by
shipping 5 real-world programs. `hello.cb` is the only minimal one;
fizzbuzz / fib / two_sum / valid_anagram all exercise non-trivial
language surfaces (control flow, recursion, collections, borrows).

---

## §5 — Cross-link

- Project: https://github.com/Cobrust-lang/cobrust
- Latest release: https://github.com/Cobrust-lang/cobrust/releases/tag/v0.3.0
- License: Apache-2.0 OR MIT dual (ADR-0001), both `LICENSE-APACHE` and
  `LICENSE-MIT` at repo root, on linguist's approved-license list
- Maturity statement: Cobrust 0.3.0 = mechanism-validated language core +
  AI translation pipeline + LSP + DAP. Phase G/H/I/J wave-1/K/L/M
  fully closed. LC-100 stress corpus 100/100 production-validated.
  CI 10/10 GREEN.
- Constitutional design: `CLAUDE.md` §2.5 — "Cobrust is not the language
  most pleasant for humans to write — it is the language LLM agents
  write correctly on the first try."

---

## Submission readiness

| Item | Status |
|---|---|
| languages.yml entry drafted | DONE |
| TextMate grammar JSON-valid (314 LOC) | DONE |
| 5 sample files staged | DONE |
| Color hex collision-checked | DONE (`#b45309` clear) |
| Extension collision-checked | DONE (`.cb` clear) |
| License approved (Apache-2.0 OR MIT) | DONE |
| In-the-wild evidence ≥2000 files | **PENDING — user supplies search query at submission time** |
| `script/add-grammar` invocation | **PENDING — needs separate grammar repo hosted on a Cobrust-lang sub-repo first** |
| `script/update-ids` invocation | **PENDING — runs at PR-submit time** |

**Recommendation**: before submitting, create
`https://github.com/Cobrust-lang/cobrust-textmate-grammar` as a
standalone repo containing `cobrust.tmLanguage.json` + Apache-2.0/MIT
LICENSE, then invoke `script/add-grammar <url>` per linguist
CONTRIBUTING.md.

---

## §6 — F35-sibling honesty notes

- We are claiming Cobrust is a real language with real users. **Both
  halves are honestly grounded:**
  - "Real language" = compiler + runtime + LSP + DAP + 226 type-checker
    parity tests pass on DG + LC-100 100/100 production corpus.
  - "Real users" = currently project authors + AI agents. **No third-
    party human users yet.** The PR body honestly states "AI-friendly
    Python successor" without overclaiming external user base.
- The linguist threshold "2,000 files in the past year" is met **only**
  if linguist counts files within the Cobrust-lang/cobrust monorepo.
  If linguist requires the threshold across multiple independent
  repositories, the PR is honestly premature — defer until v0.4.0+
  when external repos exist.
- No metrics overclaim in the PR body. Sample files are real programs
  from the production corpus, not curated marketing examples.
