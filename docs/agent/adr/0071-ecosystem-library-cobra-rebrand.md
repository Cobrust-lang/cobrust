---
doc_kind: adr
adr_id: 0071
title: Ecosystem library rebrand — distinctive cobra-themed names (drop Python names)
status: accepted
date: 2026-05-28
last_verified_commit: bef34ae
supersedes: []
superseded_by: []
relates_to: [adr:0022, "claude.md:§2.4", "claude.md:§2.5"]
---

# ADR-0071: Ecosystem library rebrand to cobra-themed names

## 1. Context

### 1.1 Owner directive (2026-05-28)

> "像 sqlite 和 numpy,咱们可以不叫这个名字,可以像 rust 那样起个自己的有特色的名字"

A successor language should not be "Python with the same library names." Rust did
not ship `python-requests` (it has `reqwest`) or `python-numpy` (it has `ndarray`).
Cobrust (Cobra 🐍 + Rust 🦀) should earn its own ecosystem identity.

The owner chose, via a structured prompt, **full rebrand** (drop the Python names)
with a **cobra/snake theme**, and delegated the specific names to the lead agent.

### 1.2 Scope

Applies ONLY to the **translated-Python-library crates** (`cobrust-<pylib>`). The
compiler / infrastructure crates (`cobrust-codegen` / `-frontend` / `-mir` /
`-hir` / `-types` / `-stdlib` / `-jit` / `-cli` / `-pkg` / `-registry` /
`-llm-router` / `-lsp` / `-dap` / `-translator`) keep their `cobrust-*` names —
they are Cobrust's own components, not borrowed library names. The `std.*` stdlib
namespace (`std.io` / `std.math` / `std.json` / …) is ALSO unchanged: `std.` is
already a Cobrust-native namespace, not a Python library name.

## 2. Decision — the naming table

| Python lib | Function | **Cobrust crate** | **module** | Rationale |
|---|---|---|---|---|
| numpy | N-dim numeric arrays | `cobrust-coil` | `coil` | a wound coil = a grid; snakes coil |
| sqlite3 | embedded SQL store | `cobrust-den` | `den` | a den = where things are kept |
| requests | HTTP client | `cobrust-strike` | `strike` | a snake's fast outbound strike = a request |
| msgpack | compact binary serde | `cobrust-scale` | `scale` | scales = compact armored packing |
| dateutil | date / time | `cobrust-molt` | `molt` | molting = cyclic time / seasons |
| tomli | TOML config parse | `cobrust-nest` | `nest` | a nest = structured config home |
| click | CLI framework | `cobrust-hood` | `hood` | the cobra hood = command / interaction surface |
| flask *(future)* | web server | `cobrust-pit` | `pit` | a snake pit handles many callers |

- The **Rust crate** keeps the workspace `cobrust-` prefix (`cobrust-coil`); the bare
  cobra word (`coil`) is the **user-facing module** (PyO3 Python module + `.cb`
  `import` name).
- The **API shape is unchanged**: `coil.eye(3)` is byte-for-byte the call shape of
  `np.eye(3)` — only the module token differs. Methods/signatures/semantics are
  identical to the Python originals.
- **No Python-name alias** (per the owner's "full rebrand" choice): `import numpy`
  does not resolve; the canonical (and only) name is `coil`.

## 3. §2.5 LLM-first reconciliation (conscious owner-ratified deviation)

CLAUDE.md §2.5 (the LLM-first north star) prizes **maximize-overlap-with-training-data**
so models write Cobrust correctly on the first try. Dropping Python library names is
in tension with that, so we record the reconciliation explicitly rather than violate
§2.5 silently:

- §2.5's "correct on the first try" rests primarily on **syntax, semantics, and
  types** — all of which Cobrust keeps Python-like (indentation blocks, comprehensions,
  f-strings, structural typing, no implicit truthiness, …). Those are unchanged.
- It also rests on **API shape**, which we preserve exactly: `coil.eye(3)` mirrors
  `np.eye(3)`; `den.connect(":memory:").cursor().execute(...)` mirrors `sqlite3`.
- The ONLY training-overlap cost is the **module-name token** (`coil` vs `numpy`),
  not the API. An LLM agent with Cobrust docs/context in scope writes `coil.eye(3)`
  as readily as `np.eye(3)` once it knows the one name — a bounded, one-token cost.
- The owner consciously chose ecosystem **identity** over that bounded cost. This is
  a legitimate "keep this document evolving" (CLAUDE.md §8) refinement: **§2.5 governs
  the language layer (syntax/semantics/types) and API shape; library naming is a brand
  layer where Cobrust identity wins.** The `feedback_cobrust_llm_first_design_principle`
  memory is updated to reflect this boundary.

## 4. Consequences

- **Provenance preserved**: each crate's `PROVENANCE.toml` + translation-header still
  records the *source* Python library (numpy/sqlite3/…) + version + oracle. The
  rebrand changes the Cobrust-facing name, not the documented translation source
  (ADR-0022 / CLAUDE.md §2.4 provenance mandate intact).
- **Registry / wheel names change** to the cobra names (`coil`, `den`, …).
- **Docs**: zh/en/agent entries + `scripts/doc-coverage.sh` surface checks repointed
  to the new names.
- Reversible at the rename level (internal until a public registry publish); no
  external consumers depend on the Python names yet.

## 5. Migration

One ecosystem-wide rename pass (single concern, paired-audited before push):
1. Per crate: rename dir `crates/cobrust-<pylib>` → `crates/cobrust-<cobra>`; update
   `Cargo.toml` `name`; update root `Cargo.toml` `members`; update any inter-crate
   path deps; update the PyO3 module name + `python/` wrapper; update `PROVENANCE.toml`
   (Cobrust name only; keep source-lib provenance); update the `.cb` import-resolution
   surface if wired.
2. Docs: `docs/human/{zh,en}/` + `docs/agent/` page renames + content; `doc-coverage.sh`
   surface-check repoint.
3. Update [[feedback_cobrust_llm_first_design_principle]] memory with the §3 boundary.
4. Verify `cargo build/test --workspace`, `clippy -D warnings`, `fmt --check`,
   `doc-coverage.sh` — all green; then push.

The in-flight `cobrust-numpy` (Stream W) + `cobrust-sqlite3` (Stream Z.7.c) work
(audited GREEN, on main, unpushed) is renamed AS PART OF this pass → lands as
`cobrust-coil` / `cobrust-den` rather than as Python-named crates that would be
immediately churned.
