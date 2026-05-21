---
module_id: cobrust-registry
last_verified_commit: HEAD
phase: Phase O W4
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
| `generator::fetch_sha256sums(assets)` | `fn` | Download + parse `SHA256SUMS` asset → `HashMap<filename, hex>` |
| `generator::parse_wheel_asset(name)` | `fn` | Parse wheel filename → `Option<(triple, cpu_level)>` |
| `generator::generate_index(pkg, version, assets, sha_map)` | `fn` | Assemble `Index` from asset list + SHA map |
| `generator::write_index_json(index, path)` | `fn` | Serialize index to JSON on disk |
| `GENERATOR_ABI_VERSION` | `const u32` | Current ABI version stamped into every `WheelEntry` |
| `ReleaseAsset` | `struct` | GitHub Releases asset (name, url, size) |
| `WheelEntry` | `struct` | One wheel variant in the index |
| `Index` | `struct` | Full `wheels.json` document |
| `Error` | `enum` | Generator error variants |

## Wire format (`wheels.json`) — W4 shape

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
      "size": 4194304,
      "cobrust_abi_version": 1,
      "experimental": false
    },
    {
      "triple": "aarch64-unknown-linux-gnu",
      "cpu_level": "sve",
      "sha256": "b2c3d4...",
      "url": "...",
      "size": 4096000,
      "cobrust_abi_version": 1,
      "experimental": true
    }
  ]
}
```

## Wheel naming convention

Pattern: `cobrust-<pkg>-<version>-<triple>-<cpu_level>.tar.gz`

Known `cpu_level` values: `v1`, `v3`, `v4` (x86_64), `neon`, `sve` (aarch64-linux), `m1`, `m2` (aarch64-apple).

Non-matching assets (e.g. `SHA256SUMS`) are silently skipped by `parse_wheel_asset`.

## SHA-256 (W4 — CLOSED)

`fetch_sha256sums(assets)` looks for a `SHA256SUMS` asset in the same release, downloads it, and
parses each line (`<hex>  <filename>`). Returns `HashMap<String, String>` (empty map when asset absent).
`generate_index` accepts this map and populates `WheelEntry::sha256` for each matched filename.
`release.yml` now generates `SHA256SUMS` via `sha256sum cobrust-v*.tar.gz > SHA256SUMS` and uploads it
as a release asset.

## ABI version (W4 — ADR-0065 §6.4)

`GENERATOR_ABI_VERSION = 1` is stamped into every `WheelEntry::cobrust_abi_version`. The consumer
(`cobrust-pkg::wheel_select`) rejects wheels whose `cobrust_abi_version` differs from its own
`COBRUST_ABI_VERSION` constant before the tier-priority pass.

## SVE experimental tagging (W4 — ADR-0065 §3.1 / §6.5)

`generate_index` sets `experimental = true` for any entry whose `cpu_level == "sve"`.
All other entries are `experimental = false`.
Consumer: `cobrust-pkg::wheel_select` skips experimental wheels unless `allow_experimental = true`.

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

## Invariants

- `generate_index` is pure (no I/O); returns one `WheelEntry` per recognized wheel asset
- `parse_wheel_asset` is `#[must_use]`, pure, no allocation on `None` path
- `write_index_json` creates parent dirs; overwrites existing file
- `fetch_sha256sums` returns empty map (not error) when `SHA256SUMS` asset absent

## Done means (W4 acceptance)

- `cargo check -p cobrust-registry` passes
- `cargo test -p cobrust-registry` → 7 PASS (parse match, parse skip, round-trip, SVE experimental, SHA map populate, ABI version stamp, sha256sums text parse)
- `cargo clippy -p cobrust-registry --all-targets -- -D warnings` → clean
- `WheelEntry` has `cobrust_abi_version: u32` (default=1) and `experimental: bool` (default=false)
- `generate_index` signature: `(pkg, version, assets, sha256_map)` with `S: BuildHasher` bound
- `release.yml` generates and uploads `SHA256SUMS`
