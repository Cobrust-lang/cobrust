---
doc_kind: adr
adr_id: 0026
title: M12 package format — user-crate cobrust.toml schema, lockfile determinism, content-addressed registry, semver resolver, namespace collision (Option C)
status: accepted
date: 2026-04-30
last_verified_commit: TBD
supersedes: []
superseded_by: []
dependencies: [adr:0004, adr:0007, adr:0019, adr:0024, adr:0025]
---

# ADR-0026: M12 package format — user-crate `cobrust.toml` schema, lockfile determinism, content-addressed registry, semver resolver, namespace collision (Option C)

## Context

ADR-0019 §"M12 — Package format + dependency resolution" pinned the
milestone scope:

> A user crate has `cobrust.toml` declaring `name`, `version`,
> `dependencies`, `[bin] / [lib]`, `[test]`. `cobrust build` resolves
> `dependencies` to a content-addressed cache under
> `~/.cobrust/registry/blake3/<hash>/`. Determinism: same inputs →
> same lockfile (`cobrust.lock`) bit-for-bit. Constitution §2.2
> "single canonical package format, content-addressed, one tool" —
> this milestone delivers it.

Constitution `CLAUDE.md` §2.2 binds the non-negotiable:

> `__init__.py` / sys.path / packaging chaos → **single canonical
> package format, content-addressed, one tool**

§2.4 binds the deterministic-build promise:

> **Deterministic build IDs**: hash of source + toolchain + LLM router
> decisions, so any translation is reproducible bit-for-bit given
> the same inputs.

ADR-0024 §"Package config namespacing" already adopted a **disjoint
top-level table convention** at M10:

> The router config uses `[router]`, `[providers.*]`, `[routing.*]`
> top-level tables. The M10 user-crate scaffold uses `[package]`.
> The two namespaces are disjoint; consumers detect which
> `cobrust.toml` they're reading by the presence of `[package]` vs.
> `[router]`. M12 (ADR-0025) will own the full user-crate schema.

ADR-0025 §H closed the M11 question on package format:

> M11 adds **no** new schema keys to `cobrust.toml`. The M10
> `[package]` placeholder stays exactly as-is. M11 produces
> standalone executables that don't yet need dependency resolution —
> that's the M12 cut.

ADR-0007 §"PROVENANCE manifest" + each translated crate's
`PROVENANCE.toml` (tomli, dateutil, msgpack, numpy, requests, click)
already commit to a `deterministic_id = "blake3:<hex>"` per-crate
identity. M12's lockfile must integrate this provenance hash so
translated crates round-trip into user-crate dep graphs without
losing reproducibility.

The M11 stdlib delivered the C-ABI runtime (mimalloc, drop handlers,
`__cobrust_print/println/panic`, entry-shim) but every `.cb` build
links a single user-source binary against `libcobrust_stdlib.a`.
**No multi-crate user program exists today.** M12's pkg crate is
the missing piece — it lets `cobrust build` produce executables
that link multiple user-authored modules + translated libraries.

ADR-0019 §"Definition of usable for most projects" line 2 binds:

> 2. M12 done means is met (a user crate with non-trivial deps
>    resolves + builds + tests pass).

and line 3:

> 3. At least one moderately-sized program (≥ 1000 LOC, ≥ 3 modules,
>    uses stdlib + at least one translated library — e.g.
>    `cobrust-tomli` for config) builds + runs end-to-end.

This ADR pins the schemas + algorithms to deliver line 2; line 3 is
the `examples/notebook/` deliverable + lives outside this ADR but
exercises every public surface this ADR creates.

## Options considered

### A. Manifest schema shape

1. **Cargo-style top-level (`[package]` + `[dependencies]` +
   `[[test]]`)** *(adopted)*
   - Pros: zero relearning cost for engineers familiar with `Cargo.toml`;
     fits the constitution's "Cargo-style single-tool workflow"
     (constitution §2.3).
   - Cons: `[bin]` vs `[[bin]]` ambiguity (Cargo permits both — single
     vs multi); we resolve by binding M12 to **single `[bin]` table
     + single `[lib]` table, multi `[[test]]` table-array** for clarity.
     Future Phase F can lift `[bin]` to `[[bin]]` if multi-bin crates
     materialize.

2. **Python-style (`pyproject.toml` w/ `[project]`)** — Cons: Cobrust
   §2.2 explicitly drops Python's packaging chaos. Rejected.

3. **Bespoke** — Cons: pointless cognitive cost. Rejected.

### B. Namespace collision with the M3 LLM-router `cobrust.toml`

ADR-0019 §"Out of scope (Phase E)" + ADR-0024 §"Package config
namespacing" both flagged this. The router config (ADR-0004) uses
`[router]` + `[providers.*]` + `[routing.*]` tables; the user-crate
config uses `[package]` + `[dependencies]` + `[bin]`/`[lib]`/`[[test]]`.

Three options were on the table:

1. **Option A — User-crate config moves to `Cobrust.toml` (capital C)**;
   router stays at `cobrust.toml`.
   - Pros: superficially Cargo-like (Cargo uses `Cargo.toml`).
   - Cons: case-insensitive filesystems on macOS + Windows. **Rejected.**

2. **Option B — User-crate stays at `cobrust.toml`; router moves to
   `cobrust-router.toml`**.
   - Pros: clean split.
   - Cons: ADR-0004 is `accepted` and the `cobrust.toml.example`
     template is shipped + documented + indexed by every
     M3..M11 doc tree. Renaming forces a breaking change to the
     M3 LLM router consumers (every M4..M11 translator run, every
     dev environment). **Rejected for M12.**

3. **Option C — Both share `cobrust.toml`; user-crate has `[package]`
   table at top, router has `[router]` + `[providers.*]` + `[routing.*]`
   tables; each tool detects the schema it's reading by presence of
   the corresponding root key.** *(adopted — extends M10 ADR-0024)*
   - Pros: zero migration; M10 already adopted this; 99 % of users
     in 99 % of repos see only one of the two schemas (translated
     crates ship with the router config in `corpus/<lib>/router.toml`,
     while user crates ship `cobrust.toml` at their root).
   - Cons: a single `cobrust.toml` could in principle contain both
     tables and confuse tooling. We resolve by **rejecting on
     load**: `Manifest::parse_str` returns `PkgError::IsRouterConfig`
     if `[router]` is present and `[package]` is absent;
     `Router::Config::from_str` (consumer of ADR-0004's schema)
     does the converse check. The two schemas are disjoint at the
     consumer layer.

We adopt **Option C**. Rationale: M10 already shipped half of it;
rejecting now requires reverting M10's `cobrust new` scaffolding,
which costs more than the disjoint-tables policy. The ADR-0024
deferral is closed by this ADR.

### C. Lockfile determinism strategy

The constitution §2.4 "Deterministic build IDs" binds reproducibility
bit-for-bit given the same inputs. The lockfile is the canonical
"frozen dep graph" artifact; same `(manifest, registry-state)` must
emit byte-identical lockfile.

1. **Sort everything** — `[[package]]` entries by `(name, version)`;
   `dependencies = []` lists by name; field order canonical;
   `\n`-only line endings; trailing newline. *(adopted)*

2. **Hash-sort** (sort by content hash) — Cons: surprises users
   reading the lockfile. **Rejected.**

3. **Insertion-order preservation** — Cons: violates determinism.
   **Rejected.**

We use BTreeMap for every key set, write the lockfile through a
**deterministic-toml** serializer (canonical key order;
sorted-array-of-tables; integer-formatted versions; `lf`-only line
endings; trailing newline). Same input → same bytes. Test
`lockfile_determinism.rs` enforces this with a permutation harness.

### D. Dependency resolution algorithm

ADR-0019 said "max compatible" without binding the exact strategy.
Three options:

1. **Max-compatible greedy (pick highest version satisfying caller's
   `VersionReq` that also satisfies every transitive sibling)**.
   *(adopted for M12)*
   - Pros: deterministic; handles >95 % of real-world dep graphs;
     fits the M12 stdlib-only + a-handful-of-translated-libraries
     scope.
   - Cons: NP-hard in pathological cases (siblings with conflicting
     reqs); we surface via `ResolutionError::Conflict { package,
     reqs }`. Phase F can replace with a SAT-based solver
     (PubGrub-style) without changing the public surface — `Resolver`
     is trait-shaped.

2. **PubGrub** (Dart-style conflict-driven backtracking) — Cons:
   significant engineering investment; M12 budget says no.

3. **Backtracking depth-first** — Cons: worst-case exponential;
   no clear win over option 1 at M12 scope.

### E. Source resolvers

Three source kinds in M12 scope: path / git / registry.

1. **Path source** — `dependencies.foo = { path = "../foo" }`. Bidirectional:
   we accept `path = "../foo"` as the inline-shorthand and the explicit
   `{ path = "..." }` table. Adopted.

2. **Git source** — `dependencies.foo = { git = "https://...", rev = "abc123" }`.
   Implementation: invoke the system `git` CLI (no `git2`/`libgit2`
   dep — keeps the binary footprint small; matches the C-side `cc`
   invocation already in the linker step). Document the constraint
   (`git` must be on PATH) in pkg.md + the human docs. Adopted.

3. **Registry source** — `dependencies.foo = "1.2"` or
   `dependencies.foo = { version = "1.2", registry = "default" }`.
   The default registry at M12 returns `RegistryError::Offline`
   when consulted. Adopted as a stub; full HTTP-fetch lands in
   Phase F under a separate ADR. Rationale: at M12 the only
   registry-shaped consumers are the bundled translated libraries
   (cobrust-tomli, etc.) which live in-tree as path deps; nobody
   has stood up a registry server yet.

### F. Content-addressed registry layout

`~/.cobrust/registry/blake3/<64-hex>/<source-tree>` — entries are
keyed by `blake3(deterministic-tarball-of-source)`.

1. **One hex-hash directory per source tree** *(adopted)*
   - Pros: trivial to GC; trivial to share across machines (copy the
     directory); one hash → one directory, no collisions by
     construction (blake3 is collision-resistant).
   - Cons: ~1 KB per blake3 directory overhead on directory inodes;
     negligible.

2. **One hex-hash file per source tree** (binary blob) — Cons: forces
   re-extraction on each `cobrust build`. Rejected.

3. **Sled / sqlite-backed registry** — Cons: extra dep; deterministic
   tar + flat dir is sufficient at M12. Rejected.

### G. Tarball determinism

When `Registry::insert` accepts a source tree, it must produce a
deterministic tarball before hashing. Options:

1. **Sort entries; clear `mtime`/`uid`/`gid`/`uname`/`gname`;
   canonical permission bits (0o644 for files, 0o755 for dirs)**.
   *(adopted)*

2. **Hash the source tree directly** (skip the tarball).
   - Cons: the on-disk extracted form is what we cache; tarball is
     required to ship sources between machines. We compute both
     (`blake3(tarball)` + extracted-tree).

### H. Provenance integration

Translated crates already carry `PROVENANCE.toml` with
`deterministic_id = "blake3:<hex>"` (ADR-0007). M12 lockfile must
preserve this so the dep graph reproduces.

Decision: every `[[package]]` entry carries `hash = "blake3:<hex>"`.
- Path-source: hash = `blake3(deterministic-tarball-of-source)` at
  load time.
- Git-source: hash = `blake3(deterministic-tarball-of-source)` after
  checkout.
- Registry-source: hash = whatever the registry serves (must match
  what `blake3` of the served tarball would be, by registry
  construction).
- For translated crates loaded via path (their normal mode), the
  package-tarball hash is the source of truth; the translator's
  `deterministic_id` is preserved as `provenance_hash` (a separate
  field) to honor ADR-0007's chain-of-custody.

## Decision

Adopt all 8 sub-decisions A..H. Concretely:

- Manifest = TOML, root tables `[package]`, `[dependencies]`,
  `[dev-dependencies]`, `[bin]`, `[lib]`, `[[test]]` (A.1).
- Namespace collision = Option C (B.3): `cobrust.toml` shared, schema
  detected by root-table presence; `IsRouterConfig` rejection on
  cross-load.
- Lockfile = sorted-everything via deterministic-TOML serializer;
  BTreeMap-backed (C.1).
- Resolver = max-compatible greedy with trait-shape for future
  PubGrub upgrade (D.1).
- Sources = path + git CLI + registry stub (E).
- Registry = `~/.cobrust/registry/blake3/<hex>/` flat layout (F.1).
- Tarball = sorted entries + zeroed mtime/uid/gid + canonical perms
  (G.1).
- Provenance = lockfile carries `hash` (tarball blake3) +
  `provenance_hash` (translator's deterministic_id when applicable)
  (H).

### Public surface (binding)

```rust
// crates/cobrust-pkg/src/lib.rs

pub mod error;
pub mod manifest;
pub mod lockfile;
pub mod resolver;
pub mod registry;
pub mod sources;
pub mod tarball;

pub use error::PkgError;
pub use manifest::{Manifest, PackageTable, Dependency, BinTable, LibTable, TestTable};
pub use lockfile::{Lockfile, LockfilePackage, LockfileMetadata};
pub use resolver::{Resolver, Resolution, ResolutionStrategy};
pub use registry::{Registry, RegistryEntry};
pub use sources::{Source, SourceFetchOutput};

/// Convenience: load a manifest from disk.
pub fn load_manifest(path: &std::path::Path) -> Result<Manifest, PkgError>;

/// Convenience: resolve a manifest's deps end-to-end and emit a
/// canonical, deterministic lockfile string. Idempotent given
/// identical inputs.
pub fn resolve_and_lock(
    manifest: &Manifest,
    workspace_root: &std::path::Path,
    registry: &Registry,
) -> Result<Lockfile, PkgError>;
```

### Manifest schema (binding)

```toml
# /path/to/user-crate/cobrust.toml

[package]
name = "my_app"                          # required; [a-zA-Z][a-zA-Z0-9_-]*
version = "0.1.0"                        # required; semver
cobrust-version = "0.0.1"                # required at M12; pinned to workspace
authors = ["Alice <alice@example.com>"]  # optional
license = "Apache-2.0 OR MIT"            # optional but recommended
description = "Short description"        # optional

[dependencies]
cobrust-tomli = { path = "../cobrust-tomli" }       # path source
my_lib       = { git = "https://github.com/alice/my_lib", rev = "abc123" }  # git source
serde-like   = "1.2"                                # registry source (M12 stub)

[dev-dependencies]
test_helpers = { path = "./test_helpers" }

[bin]
name = "my_app"
path = "src/main.cb"

[lib]
name = "my_app_lib"
path = "src/lib.cb"

[[test]]
name = "smoke"
path = "tests/smoke.cb"

[[test]]
name = "deep"
path = "tests/deep.cb"
```

**Validation rules** (enforced by `manifest::parse_str`):

| Field | Rule |
|---|---|
| `package.name` | Match `^[a-zA-Z][a-zA-Z0-9_-]*$`; ≤ 64 chars |
| `package.version` | Valid semver per `semver::Version::parse` |
| `package.cobrust-version` | Valid semver; M12 pins to `0.0.1` (workspace version); other values warn |
| `dependencies.<key>` name | Same as `package.name` rule |
| `dependencies.<value>` | One of: bare-string semver `"X.Y.Z"`; table `{ path = ... }`; table `{ git = ..., rev = ... }`; table `{ version = ..., registry = ... }` |
| Mutual exclusion | Cannot specify both `[bin]` and `[lib]` empty (M12: at least one of `[bin]` / `[lib]` must be present); `[[test]]` is optional |
| `bin.path` / `lib.path` | Relative; must exist on disk at build time (validated lazily) |
| Top-level | If `[router]` present and `[package]` absent, return `PkgError::IsRouterConfig` |
| Unknown root keys | Warn (not error) — forward-compat |

### Lockfile schema (binding)

```toml
# /path/to/user-crate/cobrust.lock — AUTO-GENERATED.

[metadata]
manifest_hash = "blake3:c0ffee..."          # hash of canonical manifest TOML
lockfile_version = 1                        # bump on incompatible schema changes

[[package]]
name = "cobrust-tomli"
version = "2.0.1"
source = "path+file:///abs/path/to/cobrust-tomli"
hash = "blake3:39e9e2d3..."                 # tarball blake3
provenance_hash = "blake3:39e9e2d3..."      # ADR-0007 deterministic_id when present
dependencies = ["serde-like 1.2.3"]

[[package]]
name = "my_app"
version = "0.1.0"
source = "path+file:///abs/path/to/my_app"
hash = "blake3:abc..."
dependencies = ["cobrust-tomli 2.0.1"]
```

**Determinism contract**:

- `[[package]]` entries sorted by `(name, version)` ASCII-lexically.
- `dependencies = [...]` sorted lexically.
- Field order within a `[[package]]`: `name, version, source, hash,
  provenance_hash, dependencies`.
- `manifest_hash` in `[metadata]` recomputed from a canonical
  manifest serialization (via `Manifest::canonical_toml`) — order
  must not depend on user's table-key ordering in the on-disk
  manifest.
- LF-only line endings; trailing newline.
- Test gate `lockfile_determinism.rs` permutes dep insertion order
  + manifest field ordering, asserts byte-identical lockfile.

### Resolution algorithm

```
fn resolve(manifest, registry) -> Lockfile:
    // 1. Build the dep graph via DFS from manifest's [dependencies]
    // (and [dev-dependencies] if --include-dev).
    graph = DepGraph::new()
    walk(manifest.package, manifest.dependencies, graph)

    // 2. For each package, compute the version chosen.
    // Strategy = max_compatible: pick the highest version satisfying
    // every requirement targeting that package.
    chosen = BTreeMap::new()
    for (name, reqs) in graph.requirements_by_package():
        candidates = registry.candidates(name)  // sorted descending
        let v = candidates.find(|v| reqs.iter().all(|r| r.matches(v)))
        match v:
            Some(v) => chosen.insert(name, v)
            None    => return Err(PkgError::Resolution(
                          ResolutionError::Conflict { package: name, reqs }))

    // 3. Cycle detection (DFS-with-color).
    if graph.has_cycle():
        return Err(PkgError::Resolution(ResolutionError::Cycle {
            cycle: graph.cycle_path()
        }))

    // 4. Emit lockfile from `chosen`.
    Lockfile::from_resolution(manifest, chosen)
```

**Failure modes**:

| Error | Trigger |
|---|---|
| `Conflict { package, reqs }` | No version satisfies all sibling reqs |
| `Cycle { path }` | Strongly-connected component in dep graph |
| `MissingPackage { name }` | Registry has no entry for the requested name |
| `Offline` | Registry source consulted at M12 (stub) |

### Registry layout (binding)

```
~/.cobrust/registry/
├── blake3/
│   ├── 39e9e2d3...0069/      # cobrust-tomli 2.0.1
│   │   ├── Cargo.toml
│   │   ├── PROVENANCE.toml
│   │   └── src/...
│   ├── c0ffee...0042/        # my_app 0.1.0
│   │   └── ...
│   └── deadbeef.../          # ...
└── index/
    └── name-to-versions.toml  # cached index (M12 stub; empty)
```

`Registry::insert(blake3_hex, source_tree)`:
1. Build a deterministic tarball from `source_tree` (sorted entries,
   zeroed mtime/uid/gid, canonical perms).
2. Verify `blake3(tarball) == blake3_hex`.
3. Extract under `~/.cobrust/registry/blake3/<hex>/`.
4. Idempotent: if the directory already exists, re-verify the hash
   matches and return without re-extracting.

### Tarball determinism (binding — `tarball.rs`)

```rust
pub struct Tarball { /* opaque */ }

impl Tarball {
    /// Build a deterministic tar.gz from a source tree.
    /// - Walks `dir` depth-first; sorts every entry's path lexically.
    /// - Zeroes mtime, uid, gid, uname, gname.
    /// - Canonical perms: 0o644 for files, 0o755 for dirs.
    /// - Drops symlinks unconditionally (M12 pkg crate has no use case).
    pub fn build(dir: &Path) -> Result<Tarball, PkgError>;

    pub fn hash(&self) -> String;  // "blake3:<hex>"
    pub fn bytes(&self) -> &[u8];
    pub fn extract(&self, dest: &Path) -> Result<(), PkgError>;
}
```

### Source resolvers (binding — `sources.rs`)

```rust
pub enum Source {
    Path { path: PathBuf },
    Git { url: String, rev: String },
    Registry { name: String, version: VersionReq },
}

pub struct SourceFetchOutput {
    pub local_path: PathBuf,
    pub blake3_hex: String,
    pub provenance_hash: Option<String>,  // from PROVENANCE.toml if present
}

impl Source {
    pub fn fetch(&self, registry: &Registry) -> Result<SourceFetchOutput, PkgError>;
}
```

- `Path::fetch` — copies the source tree to `registry/blake3/<hex>/`
  and returns the cache location.
- `Git::fetch` — `git clone --depth=1 --branch=<rev>` into a temp
  directory + extract tarball + insert into registry. Falls back
  to plain `git clone` + `git checkout <rev>` if `--branch` fails
  (rev is a SHA, not a ref).
- `Registry::fetch` — at M12 returns `PkgError::Registry(Offline)`
  unless the entry is already in the cache (in which case we serve
  it).

### Public CLI surface impact (M12 — owned by TASK B)

`cobrust build` and `cobrust test`:

1. Locate the nearest `cobrust.toml` walking up from cwd.
2. If found, load via `pkg::load_manifest`; otherwise fall back to
   the M11 single-file mode (preserves M11 hello/fizzbuzz/fib
   regression).
3. Call `pkg::resolve_and_lock` to produce/refresh `cobrust.lock`.
4. Walk the dep graph in topological order; for each pkg, build
   its `[lib]` to an object via the M9 codegen surface.
5. Build the user `[bin]`'s `src/main.cb` linking against every
   transitive `lib` object + `libcobrust_stdlib.a` + the M11
   entry-shim object.
6. `cobrust test` adds the `[[test]]` array as additional
   `[bin]`-shaped artifacts; runs each; collects pass/fail.

`cobrust new`:
- Scaffolds the **full** ADR-0026 schema, not the M10 placeholder:
  `[package]`, `[dependencies] (empty)`, `[bin]` with `name = <name>`
  and `path = "src/main.cb"`. Optionally a `[[test]]` row.

`cobrust add <dep> [--path PATH | --git URL --rev REV | --version REQ]`:
- New subcommand. Appends a row to the manifest's `[dependencies]`
  table, preserving comments + formatting where possible.

### Workspace dependency wiring (binding)

Add to root `Cargo.toml` `[workspace.dependencies]`:

```toml
semver = "1"
flate2 = "1"
tar = "0.4"
```

`crates/cobrust-pkg/Cargo.toml` declares:

```toml
[dependencies]
serde       = { workspace = true }
toml        = { workspace = true }
thiserror   = { workspace = true }
tracing     = { workspace = true }
blake3      = { workspace = true }
hex         = { workspace = true }
semver      = { workspace = true }
flate2      = { workspace = true }
tar         = { workspace = true }
cobrust-translator = { path = "../cobrust-translator" }   # for PROVENANCE typing
```

### Test corpus (binding — `tests/`)

| File | Coverage |
|---|---|
| `manifest_corpus.rs` | ≥ 30 valid + ≥ 30 invalid manifest fixtures table-driven |
| `lockfile_determinism.rs` | Same `(manifest, registry)` → identical bytes; permutation harness |
| `resolution.rs` | Path-only graphs (small + medium + cyclic); conflict detection; cycle detection |
| `registry_cache.rs` | Insert + has + idempotent re-insert; tarball blake3 round-trip |
| `sources.rs` | Path source success; git source `#[ignore]`d; registry source returns `Offline` |
| `provenance.rs` | A path source carrying `PROVENANCE.toml` populates `provenance_hash` in the lockfile |

## Consequences

- **Positive**
  - ADR-0019 §"M12" + §"Definition of usable for most projects" line 2
    closed.
  - Constitution §2.2 "single canonical package format,
    content-addressed, one tool" delivered.
  - Constitution §2.4 "Deterministic build IDs" honored by lockfile +
    tarball determinism contracts.
  - ADR-0007 provenance chain preserved end-to-end (translated crate's
    `deterministic_id` flows into lockfile's `provenance_hash`).
  - Namespace collision (M3 router vs M12 user-crate) closed via the
    M10 disjoint-tables convention with explicit cross-load rejection.
  - The pkg crate is trait-shaped (`ResolutionStrategy`); a future
    Phase F PubGrub upgrade replaces the strategy without breaking
    the public surface.

- **Negative**
  - `git` CLI is now a build-time runtime dep for git-source
    resolution. Documented in pkg.md + the human docs.
  - Registry-source resolution at M12 returns `Offline` for any
    name not already in the local cache. Documented; full HTTP
    fetch is a Phase F follow-up.
  - The lockfile schema bumps `lockfile_version` on every
    incompatible change; we start at `1` at M12. Phase F changes
    that affect determinism (new fields, new sort keys) must bump.
  - The notebook example (`examples/notebook/`, ≥ 1000 LOC) lives
    outside this ADR but exercises every public surface this ADR
    creates; if pkg's surface lands incomplete, notebook fails first.

- **Neutral / unknown**
  - The interaction between M12 path-deps and M13's
    structured-concurrency runtime is unverified; M12 is
    single-threaded only. M13 will gate.
  - Cross-platform tarball determinism (Linux ↔ macOS): both honor
    POSIX tar; Windows is out of scope (already excluded by
    ADR-0019 §"Out of scope for Phase E").
  - The "max-compatible" resolver will misbehave on adversarial
    dep graphs (sibling conflicts that PubGrub would solve via
    backtracking). M12 surfaces these as `Conflict` errors with a
    diagnostic; users can pin versions explicitly. Real impact at
    M12 scope is zero (no graph deeper than 2 levels in
    `examples/notebook/`).

## Evidence

- Constitution `CLAUDE.md` §2.2 (single canonical package format,
  content-addressed, one tool), §2.3 (Cargo-style single-tool
  workflow), §2.4 (Deterministic build IDs), §6 (Provenance-or-it-
  didn't-happen).
- ADR-0019 §"M12 — Package format + dependency resolution" — the
  binding scope this ADR delivers.
- ADR-0024 §"Package config namespacing" — the M10 deferral this
  ADR closes (Option C).
- ADR-0025 §H "cobrust.toml user-package schema" — the M11 deferral
  this ADR closes.
- ADR-0007 §"PROVENANCE manifest" — the provenance chain this ADR
  preserves end-to-end through `provenance_hash`.
- ADR-0004 §"Configuration shape" — the router config namespace this
  ADR coexists with.
- `crates/cobrust-pkg/src/{lib,error,manifest,lockfile,resolver,registry,sources,tarball}.rs`
  — implementation pinned to this ADR.
- `crates/cobrust-pkg/tests/{manifest_corpus,lockfile_determinism,resolution,registry_cache,sources,provenance}.rs`
  — gate enforcement.
- `examples/notebook/` — ≥ 1000 LOC + ≥ 3 modules + uses `cobrust-tomli`
  through the new pkg surface; line 3 of ADR-0019 §"Definition of
  usable for most projects".
