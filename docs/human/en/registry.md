# Cobrust Registry Index Generator

## What is this?

The `cobrust-registry` crate generates the wheel index files that power
`cobrust install`. When a new release is tagged, a one-shot tool queries
GitHub Releases, discovers which wheel archives were uploaded, and writes
a structured `wheels.json` file — the registry index consumers download
to select the best wheel for their host CPU.

## Why this design?

- **No dynamic server required.** The registry is static JSON on GitHub
  Releases (and optionally a CDN mirror). Generation happens once at release
  time.
- **Mirrors `pip install` semantics.** `cobrust install numpy-cb` maps to the
  same mental model as `pip install numpy`, maximizing familiarity.
- **Clean separation of concerns.** Generation (`cobrust-registry`) and
  consumption (`cobrust-pkg::registry_client`) are separate crates with no
  circular dependency.

## `wheels.json` format

```json
{
  "name": "numpy-cb",
  "version": "0.1.0",
  "wheels": [
    {
      "triple": "x86_64-unknown-linux-gnu",
      "cpu_level": "v3",
      "sha256": "a1b2c3...",
      "url": "https://github.com/.../cobrust-numpy-0.1.0-x86_64-unknown-linux-gnu-v3.tar.gz",
      "size": 4194304
    }
  ]
}
```

- One entry per `(triple, cpu_level)` variant.
- `cpu_level` values: `v1` / `v3` / `v4` (x86-64), `neon` / `sve`
  (aarch64 Linux), `m1` / `m2` (Apple Silicon).

## Using `cobrust-registry-gen`

```bash
cobrust-registry-gen numpy-cb 0.1.0
# writes pkg-index/numpy-cb-0.1.0.json
```

Options:
- `--repo <owner/name>` — default: `Cobrust-lang/cobrust`
- `--out-dir <dir>` — default: `pkg-index/`
- Set `GITHUB_TOKEN` for authenticated API access (higher rate limits)

## Known gap: SHA-256

The GitHub Releases API does not expose SHA-256 in asset metadata. The
generated `wheels.json` leaves `sha256` as `""`. W4 will add a
post-download SHA computation step to the release pipeline.
