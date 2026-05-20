---
doc_kind: outreach
title: Programming-language directory registration plan
status: draft
audience: maintainer planning submissions; user deciding when to ship each
last_verified_commit: 7c6796231c6335aab0fd1083238f904c8979a316
relates_to: [outreach:linguist-pr-draft, strategy:public-registration-roadmap]
---

# PL Directory Registration Plan

Public language directories where Cobrust should be registered. github-
linguist is the **highest-UX-value** target (renders syntax-highlighted
`.cb` on the GitHub UI for every user, every repo); PL directories are
secondary but matter for discoverability + canonical-program coverage.

**DO NOT submit any directory entry without user explicit approval.** This
doc enumerates what each requires so user can sequence the rollout.

---

## §1 — Target directories (priority order)

| # | Directory | URL | Submission method | Effort | §2.5 ROI |
|---|---|---|---|---|---|
| 1 | **github-linguist** | https://github.com/github-linguist/linguist | PR (see `linguist-pr-draft.md`) | ~2-3h to land | **highest** (in-editor highlighting on GitHub) |
| 2 | **Progopedia** | https://progopedia.com/ | User-side submission (web form / email — verify) | ~1h authoring | medium (programmer encyclopedia, low traffic but canonical) |
| 3 | **Rosetta Code** | https://rosettacode.org/wiki/Category:Programming_Languages | Self-create page + port canonical tasks | ~3-5h authoring | medium-high (300+ task-portable programs) |
| 4 | **99 bottles of beer** | https://www.99-bottles-of-beer.net/ | Web form submission, single program | ~30min | low (one-off) |
| 5 | **Wikipedia entry** | https://en.wikipedia.org/wiki/List_of_programming_languages | NOT YET — needs 3+ independent secondary sources per WP:N | **DEFER to v0.5.0+** | n/a |

---

## §2 — Progopedia submission packet

Progopedia indexes ~200 languages with consistent metadata. Required
fields per observed entries (Rust, Mojo, Crystal, Nim, Elm, Go):

### §2.1 — Metadata table

| Field | Value |
|---|---|
| **Name** | Cobrust |
| **Designed by** | Cobrust contributors (Cobrust-lang organisation) |
| **Year** | 2026 |
| **Stable release** | v0.3.0 (2026-05-18) |
| **License** | Apache-2.0 OR MIT (dual) |
| **Implementation language** | Rust (1.94+) |
| **Typing discipline** | Static, structural, inferred |
| **Influences** | Python, Rust, TypeScript |
| **Paradigms** | imperative, functional (borrow-based), structured-concurrency, AI-translation-driven |
| **Filename extensions** | `.cb` |
| **Website** | https://github.com/Cobrust-lang/cobrust |
| **Wikipedia** | (none yet) |

### §2.2 — Brief description (≤500 chars)

```
Cobrust is a statically-typed Python successor implemented in Rust. It
combines Python's surface ergonomics (indentation, comprehensions,
f-strings, structural pattern matching) with Rust's ownership semantics
(no GIL, no implicit truthiness, Result<T,E> instead of exceptions). Its
defining feature is an LLM-driven translation subsystem that converts
existing Python libraries to verified Rust under closed-loop testing.
Designed so LLM agents write it correctly on the first try.
```

(Char count: 488. Under cap.)

### §2.3 — Canonical programs (Progopedia listing standard)

Progopedia hosts a "hello world" + a small set of canonical algorithms
per language. Cobrust submissions:

| Program | Source path | Approx LOC |
|---|---|---|
| Hello world | `examples/hello.cb` | 3 |
| Factorial | (write new — see below) | ~8 |
| Fibonacci (recursive) | `examples/fib.cb` | 9 |
| FizzBuzz | `examples/fizzbuzz.cb` | 14 |

**Factorial (to write before submission):**
```cobrust
fn fact(n: i64) -> i64:
    if n <= 1:
        return 1
    return n * fact(n - 1)

fn main() -> i64:
    print(fact(5))
    return 0
```

### §2.4 — Submission method

Progopedia does not publish a "submit a language" web form. Method:

- Option A: Email maintainer (Vasiliy Lavrov, listed on `progopedia.com/about/`) with the packet above + sample programs.
- Option B: Open issue on GitHub if repo exists (TBD — search at submission time).

**Pre-submission validation**: re-fetch `https://progopedia.com/about/`
at submission time to confirm contact method has not changed (the
2026-05-20 first-attempt fetch returned socket error; retry under clash
proxy `http://127.0.0.1:7897`).

---

## §3 — Rosetta Code rollout

Rosetta Code lists 300+ tasks; languages are added by user-creating a
language page + porting at least 5-10 tasks to demonstrate coverage.

### §3.1 — Language page

URL: `https://rosettacode.org/wiki/Category:Cobrust`

Content (Wiki markup):
```mediawiki
{{language|Cobrust
|exec=both
|gc=yes
|safety=safe
|strength=strong
|express=explicit
|checking=static
|parampass=value
|LCT=yes
}}

'''Cobrust''' is a statically-typed AI-friendly Python successor
implemented in Rust. It combines Python's surface ergonomics with Rust's
ownership semantics and ships an LLM-driven translation subsystem for
porting Python libraries.

Official: [https://github.com/Cobrust-lang/cobrust GitHub]
License: Apache-2.0 OR MIT (dual)
```

### §3.2 — Initial task ports (10 candidates)

Pick tasks already covered by `examples/leetcode/` + `examples/`:

| Rosetta task | Existing Cobrust file |
|---|---|
| 99 Bottles of Beer | (new — write ~20 LOC) |
| FizzBuzz | `examples/fizzbuzz.cb` |
| Fibonacci sequence | `examples/fib.cb` |
| Hello world | `examples/hello.cb` |
| Two Sum (LC-01) | `examples/leetcode/two_sum.cb` |
| Binary search | `examples/leetcode/binary_search.cb` |
| Reverse a string | `examples/leetcode/reverse_string.cb` |
| Merge sorted lists | `examples/leetcode/merge_two_sorted_lists.cb` |
| Maximum subarray | `examples/leetcode/maximum_subarray.cb` |
| Roman numerals | `examples/leetcode/roman_to_integer.cb` |

10 tasks = decent first-wave coverage. Add 10-20 more over v0.3.1 / v0.4.0.

### §3.3 — Submission method

Rosetta Code is a public MediaWiki. User creates account, edits pages
directly. No PR / approval gate (admin moderation post-hoc).

---

## §4 — 99 bottles of beer

Single program submission via web form at
`https://www.99-bottles-of-beer.net/submitnewlanguage.html`.

Required:
- Language name: Cobrust
- Author of program: Cobrust contributors
- Source code: (the program below)

**Program:**
```cobrust
fn main() -> i64:
    let n: i64 = 99
    while n > 0:
        if n == 1:
            print("1 bottle of beer on the wall, 1 bottle of beer.")
            print("Take one down and pass it around, no more bottles of beer on the wall.")
        else:
            print(f"{n} bottles of beer on the wall, {n} bottles of beer.")
            if n - 1 == 1:
                print("Take one down and pass it around, 1 bottle of beer on the wall.")
            else:
                print(f"Take one down and pass it around, {n - 1} bottles of beer on the wall.")
        n = n - 1
    print("No more bottles of beer on the wall, no more bottles of beer.")
    print("Go to the store and buy some more, 99 bottles of beer on the wall.")
    return 0
```

Verify on local Cobrust toolchain (`cobrust run`) before submission.

---

## §5 — Wikipedia (DEFERRED)

Wikipedia's WP:N (Notability) requires "significant coverage in
independent reliable secondary sources". As of 2026-05-20:

- No independent blog coverage
- No independent academic / industry analysis
- No external benchmark studies
- Project is < 1 year old in public form

**Decision**: defer Wikipedia entry to **v0.5.0+** (mid-2026 or later)
once at least 3 independent sources exist:

1. e.g. Hacker News front page coverage
2. e.g. an external blog (Phoronix, Real World OCaml-style writeup)
3. e.g. academic mention or industry benchmark report

Until then, an early Wikipedia draft will be deleted under WP:NPROD or
WP:AfD and would damage credibility. Better to let external coverage
accumulate first.

---

## §6 — Effort + sequence summary

Recommended user-side sequence (each gated on prior step's landing):

1. **github-linguist** — biggest UX win; **submit first** once
   `Cobrust-lang/cobrust-textmate-grammar` standalone repo created.
2. **Rosetta Code** — high-ROI canonical programs corpus; do in parallel
   with linguist (no dependency).
3. **99 bottles of beer** — single low-effort submission; do anytime.
4. **Progopedia** — verify maintainer email path, then submit the packet
   in §2.
5. **Wikipedia** — defer to v0.5.0+.

**Total effort estimate**: ~6-8 hours of user-side work across items
1-4. Cobrust agent contribution is **drafting all artifacts** (this
doc + `linguist-pr-draft.md` + grammar JSON + sample staging) — done.

---

## §7 — F35-sibling honesty cross-check

- Every claim in submissions mirrors `README.md` Status section.
- "Production-validated" applies to **LC-100 corpus**, not external user
  base.
- "AI-friendly" framing matches `CLAUDE.md §2.5` constitutional pillar,
  not marketing hyperbole.
- Influences listed (Python / Rust / TypeScript) are factually correct
  per ADR-0001 license + ADR-0051 design principle.
- No claim of being a "competitor" to Mojo / Cython / Pyston etc; the
  Acknowledgements section of README.md correctly positions them as
  prior art.

---

## §8 — Cross-references

- linguist PR draft: `docs/agent/outreach/linguist-pr-draft.md`
- TextMate grammar: `docs/agent/outreach/cobrust.tmLanguage.json`
- Sample files: `docs/agent/outreach/linguist-samples/`
- Strategy doc: `docs/agent/strategy/public-registration-roadmap.md`
- Project Constitution: `CLAUDE.md`
- ADR-0001 (license): `docs/agent/adr/0001-license.md`
- README.md Status section: `README.md` §Status
