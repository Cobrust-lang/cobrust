---
module_id: cobrust-registry
last_verified_commit: HEAD
phase: Phase O W3
adr: 0065
dependencies: [reqwest, serde_json, thiserror]
---

# cobrust-registry — Index Generator

## Purpose

Index-generation side of the Cobrust wheel registry (ADR-0065 §7.3).
Scans GitHub Release assets and emits canonical `wheels.json` per §3.4.
Consumer side: `cobrust-pkg::registry_client`.

## Public API surface

| Symbol | Kind | Description |
|---|---|---|
| `generator::fetch_release_assets(repo, version)` | `fn` | Query GitHub Releases API for tag `v{version}` |
| `generator::parse_wheel_asset(name)` | `fn` | Parse wheel filename → `Option<(triple, cpu_level)>` |
| `generator::generate_index(pkg, version, assets)` | `fn` | Assemble `Index` from asset list |
| `generator::write_index_json(index, path)` | `fn` | Serialize index to JSON on disk |
| `ReleaseAsset` | `struct` | GitHub Releases asset (name, url, size) |
| `WheelEntry` | `struct` | One wheel variant in the index |
| `Index` | `struct` | Full `wheels.json` document |
| `Error` | `enum` | Generator error variants |

## Wire format (`wheels.json`)

```json
{
  "name": "numpy-cb",
  "version": "0.1.0",
  "wheels": [
    {
      "triple": "x86_64-unknown-linux-gnu",
      "cpu_level": "v3",
      "sha256": "a1b2c3...",
      "url": "https://github.com/Cobrust-lang/cobrust/releases/download/v0.1.0/...",
      "size": 4194304
    }
  ]
}
```

## Wheel naming convention

Pattern: `cobrust-<pkg>-<version>-<triple>-<cpu_level>.tar.gz`

Known `cpu_level` values: `v1`, `v3`, `v4` (x86_64), `neon`, `sve` (aarch64-linux), `m1`, `m2` (aarch64-apple).

Non-matching assets (e.g. `sha256sums.txt`) are silently skipped by `parse_wheel_asset`.

## SHA-256 gap

`fetch_release_assets` does not compute SHA-256 (GitHub API does not expose it in asset metadata).
`WheelEntry::sha256` is `""` in generator output. W4 will add a post-download SHA computation step.

## Binary: `cobrust-registry-gen`

```
cobrust-registry-gen <pkg> <version> [--repo <owner/name>] [--out-dir <dir>]
```

Output: `<out-dir>/<pkg>-<version>.json`. Defaults: repo=`Cobrust-lang/cobrust`, out-dir=`pkg-index/`.
Reads `GITHUB_TOKEN` from env if present (higher rate limit). Exits 0 on success, 1 on error.

## Non-goals

- Not a chat UI or agent loop
- Does not host the registry (GitHub Releases = static CDN per §3.4)
- Does not resolve transitive dependencies
- W4 scope: SHA computation, release.yml integration, CDN push

## Invariants

- `generate_index` is pure (no I/O); returns one `WheelEntry` per recognized wheel asset
- `parse_wheel_asset` is `#[must_use]`, pure, no allocation on `None` path
- `write_index_json` creates parent dirs; overwrites existing file

## Done means (W3 acceptance)

- `cargo check -p cobrust-registry` passes
- `cargo test -p cobrust-registry` → 3 PASS (parse match, parse skip, round-trip)
- `cargo clippy -p cobrust-registry --all-targets -- -D warnings` → clean
- `cobrust-registry` member of workspace `Cargo.toml`
