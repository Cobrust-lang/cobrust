# Contributing to Cobrust

Welcome — Cobrust is built in public, by AI agents working with humans.
Contributions of any size are welcome.

## Quick map

- **Bug reports** → [GitHub Issues](https://github.com/Cobrust-lang/cobrust/issues/new?template=bug.yml)
- **Feature requests / RFCs** → [Discussions](https://github.com/Cobrust-lang/cobrust/discussions/categories/ideas)
- **Question?** → [Discussions Q&A](https://github.com/Cobrust-lang/cobrust/discussions/categories/q-a)
- **Translated library proposals** → see "Translation contributions" below
- **Code contributions** → fork, branch, PR; see "Workflow" below

## What we need help with

We tag entry points with these labels:

- `good-first-issue` — small, mostly-doc, learnable in an afternoon
- `help-wanted` — meaningful but tractable; maintainers will mentor
- `translate-target` — a Python library we want to translate; see ADR-0022
- `lsp` — anything LSP / IDE; F.1.8 + F.2.2
- `self-hosting` — anything in the F.1.7 / F.2.5 self-hosting track

## Code workflow

1. **Fork** `github.com/Cobrust-lang/cobrust` (or your namespace)
2. **Branch**: `feature/<short-description>` or `fix/<issue-number>`
3. **Local setup**:
   ```bash
   git clone https://github.com/<you>/cobrust && cd cobrust
   cargo build --workspace --locked     # ~50-60s on Apple Silicon
   cargo test --workspace --locked      # 2,545+ tests, ~30s
   ```
4. **Make your change** — see "What touches what" below
5. **5-gate locally** before pushing:
   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets --locked -- -D warnings
   cargo build --workspace --locked
   cargo test --workspace --locked
   bash scripts/doc-coverage.sh
   ```
5a. **Install the pre-commit snapshot-lint hook** (one-time, optional but recommended):
   ```bash
   git config core.hooksPath .githooks
   chmod +x .githooks/pre-commit-snapshot-lint
   ```
   This hook checks project state invariants before each commit.
6. **Commit** with conventional-commits: `feat(scope): subject`,
   `fix(scope): subject`, `docs(scope): subject`. Co-author lines welcome.
7. **PR** against `main`. CI runs the 5 gates on macOS arm64 + Linux x86_64.
8. A maintainer will review within 5 working days. If you hear nothing
   in 7, ping in [Discussions](https://github.com/Cobrust-lang/cobrust/discussions).

## Architecture in one minute

Cobrust is a monorepo of crates:

```
cobrust-frontend  →  cobrust-hir   →  cobrust-types  →  cobrust-mir   →  cobrust-codegen  →  binary
                                                          ↑
                                              cobrust-stdlib (linked at codegen time)

cobrust-translator  ← AI translation subsystem; consumes Python source, emits Cobrust
   ↓
cobrust-llm-router  ← provider-agnostic LLM dispatch + cache + ledger
```

Plus translated outputs: `cobrust-tomli`, `cobrust-dateutil`, `cobrust-msgpack`, `cobrust-numpy`, `cobrust-requests`, `cobrust-click`. These are *products* of the translator, not hand-written Rust (the synthetic-mode entries are being phased out — see [`docs/agent/findings/translator-real-vs-synthetic-status.md`](docs/agent/findings/translator-real-vs-synthetic-status.md)).

Detailed architecture: [`docs/human/en/architecture.md`](docs/human/en/architecture.md).

## What touches what

**Adding a syntactic form** (e.g. a new operator):
- `crates/cobrust-frontend/src/{lexer.rs, parser.rs, ast.rs, unparse.rs}`
- ADR document in `docs/agent/adr/` if changing semantics
- 30-form round-trip test in `tests/round_trip.rs`

**Adding a stdlib function**:
- `crates/cobrust-stdlib/src/{io,collections,string,math,...}.rs`
- C-ABI export if codegen-emitted code needs to call it
- Triple-tree doc sync (zh / en / agent) in `docs/`

**Translating a new library**:
- Vendor source under `corpus/<library>/UPSTREAM_VERSION` + `corpus/<library>/spec.toml`
- Add a `crates/cobrust-<library>/` crate
- See ADR-0022 for the established translation-batch pattern

**Adding an LLM provider**:
- `crates/cobrust-llm-router/src/{provider.rs, <new>.rs}`
- New `LlmProvider` trait impl
- Wire-test with `tests/real_llm_smoke.rs`

## Doc tracks

Cobrust ships **dual-track docs**:
- `docs/human/{zh,en}/` — for humans (Markdown, mermaid diagrams, narrative as needed)
- `docs/agent/` — for AI agents (dense, schemas, no narrative)

Any code change that affects user-visible behavior or public API must update **both tracks in the same commit**. CI's `scripts/doc-coverage.sh` enforces.

## Translation contributions

If you want to propose a Python library for translation:

1. Open a discussion in [Translation Targets](https://github.com/Cobrust-lang/cobrust/discussions/categories/translation-targets)
2. Include: PyPI url, license (must be permissive), LOC count, downstream-dep count (`pipdeptree --reverse`)
3. Maintainers tag with priority + score against the [F.1 / F.2 backlog](docs/agent/adr/0038-phase-f-roadmap.md)
4. Approved libraries get a `translate-target` issue with a dispatch prompt for the next P9 sprint

## ADR (Architecture Decision Record)

We document decisions affecting more than one file as ADRs. Template at `docs/agent/adr/_template.md`.

Submit ADR draft as a PR; comments and counter-proposals go in the PR review.

## Code style

- `snake_case` for values, `UpperCamelCase` for types, `SCREAMING_SNAKE_CASE` for consts
- File names: `snake_case.rs` and `snake_case.cb`
- Commit messages: conventional commits, present tense
- No `TODO` without a linked issue: `// TODO(#123): ...`
- No `unwrap()` in non-test code; use `.expect("rationale")`
- Default visibility is `private`; `pub` is opt-in

## Reviews + maintainership

Maintainers (as of 2026-05-10):
- @wbj010101 (lead)
- review-claude (third-party audit window — non-merging reviewer)

Becoming a maintainer: ship 5+ merged non-trivial PRs, no controversies in discussions, demonstrated good judgment in reviews. Ping the lead.

## Code of Conduct

We follow the [Contributor Covenant 2.1](https://www.contributor-covenant.org/version/2/1/code_of_conduct/) — see [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

In short: be kind. AI-generated content is welcome but should be marked. Disagree on technical merit, not personality. We don't tolerate harassment or bad-faith arguments.

## License

By contributing, you agree your contribution is dual-licensed under Apache-2.0 OR MIT (the project license, see [LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT)).

If you don't agree, don't contribute. (We won't accept PRs that try to relicense parts.)

---

Thanks for considering Cobrust. It's an experiment in AI-human collaboration on a serious systems project. Every contribution makes the experiment more credible.
