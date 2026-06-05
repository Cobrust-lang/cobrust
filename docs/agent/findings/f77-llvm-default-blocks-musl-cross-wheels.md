---
finding_id: F77
title: LLVM-default backend blocks the static-musl + cross-aarch64 prebuilt wheels (v0.7.0 X.6 deferral)
date: 2026-06-06
status: ratified
severity: medium
relates_to: [adr:0070, adr:0065, adr:0046, adr:0069, "claude.md:§5.2"]
---

# F77 — LLVM-default blocks the static-musl + cross-aarch64 wheels

## What

ADR-0070 X.6 ("release.yml + wheel matrix re-baseline for LLVM-default")
forced a v0.7.0 wheel-set reduction. The v0.6.x release matrix shipped 9
wheels across 4 targets; v0.7.0 ships **5** (x86_64-unknown-linux-gnu
{v1,v3,v4} + aarch64-apple-darwin {m1,m2}). The other **4** — the two
`x86_64-unknown-linux-musl` (v1/v3) and the two `aarch64-unknown-linux-gnu`
(neon/sve) — are **deferred** to v0.7.x.

## Why (the root cause)

ADR-0070 X.3 flipped `cobrust-codegen`'s `default = ["llvm"]`, and X.4
**removed the Cranelift AOT backend entirely** (no fallback). The release
binary now links **system LLVM 18** via `llvm-sys` (inkwell `llvm18-1`,
reading `LLVM_SYS_181_PREFIX`). Two of the v0.6.x build paths cannot
satisfy that link:

1. **`x86_64-unknown-linux-musl` (fully-static)** — a static-musl binary
   has, by construction, **no dynamic dependencies**. The `apt`-provided
   `libLLVM` is a **glibc dynamic** library; there is no static-musl
   `libLLVM` in the Ubuntu package set. So the static-musl link cannot
   pull LLVM. (A working musl wheel needs a `libLLVM.a` built against musl
   — a from-source LLVM build, out of v0.7.0 scope.)

2. **`aarch64-unknown-linux-gnu` (via `cross`)** — the build cross-compiles
   on an x86_64 host using `cross`'s default Docker image, which has **no
   target-arch (aarch64) libLLVM**, and the host's x86_64 `llvm-sys`
   cannot satisfy an aarch64 link. (The default `cross` images predate the
   LLVM-default flip.)

Under the pre-X.4 Cranelift-default backend neither was a problem (Cranelift
is a pure-Rust crate, no system-library link).

## Decision (per ADR-0070 §5 "deferred with finding URN")

Ship the 5 wheels that a **straightforward system LLVM 18** satisfies; defer
the 4 that do not. The deferral is recorded here + in:

- `.github/workflows/release.yml` (matrix comment + the header platform
  contract + the wheel README's Option B/C notes),
- ADR-0070 substream_status X.6 + the §X.6 closure.

## Restoration path (v0.7.x)

- **aarch64-unknown-linux-gnu**: switch from `cross` to a **native
  `ubuntu-24.04-arm` GitHub runner** (GA since 2025) + `apt-get install
  llvm-18` + `use_cross: false`. This is the clean fix — no cross-LLVM
  image needed. (The likeliest first v0.7.x release-eng task.)
- **x86_64-unknown-linux-musl**: build a **static `libLLVM.a` against
  musl** (a from-source LLVM build in the musl job) OR accept a
  partially-dynamic musl binary. Heavier; lower priority (Alpine users can
  `cargo install` with `llvm18-dev` from the `community` repo, or use a
  glibc base image + the gnu wheel).

## Runtime requirement (the shipped wheels)

The shipped `x86_64-unknown-linux-gnu` wheel **dynamically links
`libLLVM-18`** → end users must have it installed
(`sudo apt-get install -y libllvm18`). Documented in the wheel README
(release.yml) + the top-level `README.md` install section (ADR-0070 §4
mandate). `cargo install` (from-source) likewise needs `llvm-18` +
`LLVM_SYS_181_PREFIX`.

## Verification note

This finding's release.yml changes are **not locally CI-verifiable** — the
release workflow only runs on a `v*` git tag. The YAML was validated
(`yaml.safe_load`) and the matrix reduced to 5 entries; the actual wheel
**build** is gated on the v0.7.0-rc tag run. If a shipped target
nonetheless fails at tag-time, it follows the same defer-with-finding
pattern.
