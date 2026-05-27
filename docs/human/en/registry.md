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
      "url": "https://github.com/.../cobrust-coil-0.1.0-x86_64-unknown-linux-gnu-v3.tar.gz",
      "size": 4194304
    }
  ]
}
```

- One entry per `(triple, cpu_level)` variant.
- `cpu_level` values: `v1` / `v3` / `v4` (x86-64), `neon` / `sve`
  (aarch64 Linux), `m1` / `m2` (Apple Silicon).
- `cobrust_abi_version` — numeric ABI version (default `1`). `cobrust install` rejects wheels
  with a mismatching ABI version before tier selection.
- `experimental` — `true` for SVE wheels (ADR-0065 §6.5). Requires `--allow-experimental` to install.

## Using `cobrust-registry-gen`

```bash
cobrust-registry-gen numpy-cb 0.1.0
# writes pkg-index/numpy-cb-0.1.0.json (with sha256 populated from SHA256SUMS asset)
```

Options:
- `--repo <owner/name>` — default: `Cobrust-lang/cobrust`
- `--out-dir <dir>` — default: `pkg-index/`
- Set `GITHUB_TOKEN` for authenticated API access (higher rate limits)

## SHA-256 (W4 — resolved)

`release.yml` now generates `SHA256SUMS` via `sha256sum cobrust-v*.tar.gz > SHA256SUMS` and uploads
it as a release asset. `cobrust-registry-gen` downloads `SHA256SUMS` from the same release and
populates each `WheelEntry::sha256` field. If `SHA256SUMS` is absent, the generator proceeds with
`sha256 = ""` (warning printed to stderr).

## Installing an SVE (experimental) wheel

```bash
cobrust install svecalc-cb --version 0.1.0 --allow-experimental
# warning: experimental SVE wheel; only use if you understand the risks
```

SVE wheels are marked `experimental: true` because SVE ABI is not yet declared stable (ADR-0065 §6.5).
Without `--allow-experimental`, `cobrust install` falls back to the `neon` baseline wheel if available,
or returns an error if SVE is the only option.
