---
name: "0061"
title: cobrust skills subcommand
status: proposed
phase: Phase N (post-Phase-M)
relates_to: [adr:0024, adr:0051, adr:0057a]
date: 2026-05-19
author: CTO (P10)
competitive_source: docs/agent/strategy/competitive-intel-zero-language.md §3.1
---

## §1 Motivation

### 1.1 The Training-Data-Overlap Problem

ADR-0051 / CLAUDE.md §2.5 states: "prefer syntax + semantics that occur frequently in Python + Rust training corpora." This rule covers language surface design well, but it has a blind spot: **Cobrust-specific idioms that postdate any LLM's training cutoff will never be in training data**.

Examples of never-in-training-data Cobrust patterns:
- `@py_compat(strict)` / `@py_compat(numerical(rtol=1e-7))` decorator semantics
- `&borrow` explicit borrow syntax (ADR-0052a)
- `Result<T, E>` idiomatic Cobrust vs Python's exception-based control flow
- Cobrust error code taxonomy (TypeError::ImplicitTruthiness, etc.)
- L0–L3 translation manifest header format

An LLM agent mid-conversation has no way to retrieve this information without:
a) being given it in the system prompt (expensive, always included even when not needed), or
b) having a tool call that fetches it on demand.

### 1.2 Zero Language Empirical Inspiration

The Zero language (vercel-labs/zero, 0.1.3) ships `zero skills get <name>` — a binary-embedded version-matched cheatsheet CLI. The LLM (Cursor, Copilot, Claude) calls it mid-conversation to retrieve Zero-specific syntax it was never trained on. This is a low-complexity (~300 LOC) implementation that delivers measurable §2.5 ROI.

Cobrust SHOULD replicate this pattern. See `docs/agent/strategy/competitive-intel-zero-language.md §3.1` for full competitive context.

### 1.3 Version-Matched Guarantee

The key invariant: the cheatsheet returned by `cobrust skills get <name>` must match the **installed cobrust version**. A skill file for 0.3.0 borrow syntax must NOT be served to an agent running 0.4.0. Binary-embedding via `rust-embed` guarantees this — the skills are frozen at compile time.

---

## §2 §2.5 LLM-First Audit

| §2.5 rule | Assessment |
|---|---|
| Compile-time-catch-errors | N/A (tooling feature, not language design) |
| Maximize-overlap-with-training-data | **This IS the training-data-overlap intervention.** Skills docs are fetched at runtime by the LLM; they substitute for absent training coverage. HIGHEST ROI of any pending §2.5 work item. |

ADR-0051 direction B (F.1.4 Error UX) tells the LLM which fix to apply. ADR-0061 tells the LLM **how to write Cobrust in the first place**. Both are required; this one is lower risk.

---

## §3 Scope

### 3.1 CLI Surface

Three subcommands:

```
cobrust skills list
cobrust skills get <name>
cobrust skills get <name> --json
```

**`cobrust skills list`**:
- Output: newline-delimited list of available skill names, one per line
- Example output:
  ```
  cobrust-language
  cobrust-stdlib
  cobrust-error-codes
  cobrust-debugger
  ```

**`cobrust skills get <name>`**:
- Output: raw markdown text of the skill file to stdout
- Exit 0 on success, exit 1 if skill name not found (with error message to stderr)
- LLM calls this, reads the markdown, incorporates into working context

**`cobrust skills get <name> --json`**:
- Output: JSON object `{"name": "...", "version": "...", "content": "..."}` where `version` is the cobrust binary version string
- Useful for programmatic consumption / MCP tool integration

### 3.2 Initial Skill Catalog (Phase N)

Four curated skill files embedded at launch:

| Skill name | Source file | Contents |
|---|---|---|
| `cobrust-language` | `docs/agent/skills/cobrust-language.md` | Core syntax reference: types, let/fn/struct/enum/match, borrow syntax, @py_compat, f-strings, comprehensions |
| `cobrust-stdlib` | `docs/agent/skills/cobrust-stdlib.md` | Key stdlib modules and function signatures: string, list, dict, file I/O, Result/Option helpers |
| `cobrust-error-codes` | `docs/agent/skills/cobrust-error-codes.md` | All TypeError + MirError + LoweringError variants with fix hints (extends ADR-0052b UX work) |
| `cobrust-debugger` | `docs/agent/skills/cobrust-debugger.md` | `cobrust debug` subcommand (ADR-0059c) + DAP protocol cheatsheet for AI-driven debugging |

**Note**: skill markdown files MUST be written as part of ADR-0061 implementation. The files do not exist yet. They are authored alongside the binary-embed wiring.

---

## §4 Implementation

### 4.1 Crate Location

`crates/cobrust-cli/src/main.rs` — add `skills` subcommand to existing CLI parser (clap).

### 4.2 Binary Embedding

Add `rust-embed` to `crates/cobrust-cli/Cargo.toml`:

```toml
[dependencies]
rust-embed = { version = "8", features = ["compression"] }
```

Define embedded assets:

```rust
#[derive(rust_embed::Embed)]
#[folder = "../../docs/agent/skills/"]
#[include = "*.md"]
struct SkillAssets;
```

### 4.3 Subcommand Implementation Sketch

```rust
pub fn cmd_skills(sub: &SkillsArgs) -> Result<(), CliError> {
    match sub {
        SkillsArgs::List => {
            for file in SkillAssets::iter() {
                let name = file.trim_end_matches(".md");
                println!("{name}");
            }
            Ok(())
        }
        SkillsArgs::Get { name, json } => {
            let key = format!("{name}.md");
            match SkillAssets::get(&key) {
                Some(asset) => {
                    let content = std::str::from_utf8(asset.data.as_ref())
                        .map_err(|_| CliError::SkillCorrupt(name.clone()))?;
                    if *json {
                        println!(
                            r#"{{"name":"{name}","version":"{}","content":{}}}"#,
                            env!("CARGO_PKG_VERSION"),
                            serde_json::to_string(content)?
                        );
                    } else {
                        print!("{content}");
                    }
                    Ok(())
                }
                None => {
                    eprintln!("error: skill '{name}' not found");
                    eprintln!("  run 'cobrust skills list' to see available skills");
                    std::process::exit(1);
                }
            }
        }
    }
}
```

### 4.4 Files to Create/Modify

- `crates/cobrust-cli/Cargo.toml` — add `rust-embed`, `serde_json` deps
- `crates/cobrust-cli/src/main.rs` — add `skills` subcommand to clap + dispatch to `cmd_skills`
- `crates/cobrust-cli/src/skills.rs` — implementation of `cmd_skills`
- `docs/agent/skills/cobrust-language.md` — NEW (authored during impl)
- `docs/agent/skills/cobrust-stdlib.md` — NEW (authored during impl)
- `docs/agent/skills/cobrust-error-codes.md` — NEW (authored during impl)
- `docs/agent/skills/cobrust-debugger.md` — NEW (authored during impl)
- `crates/cobrust-cli/tests/test_skills.rs` — integration tests (§6)

---

## §5 Non-Goals

- **NO live doc-build at run-time**: skills are frozen at compile time via `rust-embed`. No file-system reads at runtime.
- **NO external skill-pack download**: no network calls, no plugin system, no `cobrust skills install`. Skills are Cobrust's own docs, not third-party extensions.
- **NO skill versioning beyond binary version**: skills are tied to the cobrust binary version. No independent skill-pack versioning.
- **NO web UI**: this is a CLI tool only. MCP integration is a future concern (Phase P+).
- **NO skills for third-party libraries**: only Cobrust's own language surface. Translated library skills (e.g., `cobrust-tomli`) are a Phase N+ extension.

---

## §6 Acceptance Gates

Three integration tests in `crates/cobrust-cli/tests/test_skills.rs`:

| Test | Assertion |
|---|---|
| `test_skills_list_nonempty` | `cobrust skills list` exits 0 AND stdout contains at least one line matching `cobrust-language` |
| `test_skills_get_language_returns_content` | `cobrust skills get cobrust-language` exits 0 AND stdout length > 100 bytes AND contains the string `@py_compat` |
| `test_skills_get_json_valid` | `cobrust skills get cobrust-language --json` exits 0 AND stdout is valid JSON AND JSON object has keys `name`, `version`, `content` |

All three tests MUST pass on CI before ADR-0061 is marked `accepted`.

---

## §7 Risk Register

| Risk | Mitigation |
|---|---|
| **Skills drift from docs** | CI gate: if `docs/agent/skills/*.md` content hash differs from embedded hash at build time, emit build error. Implement as a `build.rs` hash-compare check in cobrust-cli. |
| **Skills files become large** | `rust-embed` with `compression` feature. Compressed at build time. Current estimate: 4 files × ~5KB = ~20KB uncompressed, ~6KB compressed. Acceptable binary size delta. |
| **New error variant added without skills update** | Link skills-update to `CHANGELOG.md` entry requirement. Doc-coverage CI check (§3.3 CLAUDE.md) covers public items — add a check that `cobrust-error-codes.md` mentions each TypeError/MirError variant name. |
| **Zero precedent: skill removed without notice (FP-3)** | Integration tests (§6) are the contract. Removing a skill name from `list` output MUST fail `test_skills_list_nonempty`. Any removal requires a deprecation cycle. |

---

## §8 Implementation Plan

| Task | Estimate | Notes |
|---|---|---|
| Cargo.toml deps + rust-embed wiring | 30 min | Mechanical |
| Clap subcommand + dispatch | 1h | Pattern exists for other subcommands |
| `skills.rs` implementation | 1h | ~150 LOC |
| Author 4 skill markdown files | 2h | Highest effort; requires subject-matter accuracy |
| Integration tests | 30 min | ~60 LOC |
| CI build.rs drift check | 30 min | ~50 LOC build script |
| **Total** | **~5.5h** | Within 4-6h estimate |

**Total LOC estimate**: ~300 LOC Rust + ~4×300 LOC markdown skills = ~1500 LOC total change.

**Dispatch readiness**: dispatchable as Phase N P0 immediately after Phase M closure. No blocking dependency. Recommend Sonnet-tier agent (well-scoped, existing ADR, clear acceptance gates).
