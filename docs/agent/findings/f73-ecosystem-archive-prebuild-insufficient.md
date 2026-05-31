---
finding_id: F73
title: Pre-building ecosystem staticlibs does NOT close the concurrent-rebuild race; the Phase-1 env override does
status: resolved
date: 2026-05-31
severity: medium
siblings: [F44, F64, F72]
evidence_ci: [26709832072, 26693573627, 26694248379, 26580282088]
governs: .github/workflows/ci.yml, crates/cobrust-cli/src/build.rs
---

# F73 — ecosystem-archive pre-build is insufficient; wire the Phase-1 env override

## Symptom

`cargo test (macos-latest)` red on `c94df69` (coil matmul `@` milestone). One
coil e2e linker-failed while every sibling coil e2e in the same run passed:

```
test test_e2e_left_scalar_div_by_zero_is_inf ... FAILED
ld: file cannot be open()ed, errno=2 path=.../target/debug/libcoil.a
cobrust build: linker `cc` exited with status ExitStatus(256)
```

`coil_left_scalar_e2e.rs` was **not touched** by `c94df69`. `errno=2` =
`ENOENT` — the archive **vanished** at the instant the linker `open()`ed it,
not a missing symbol (which would be `Undefined symbols: _…`). This is the
ecosystem `lib<mod>.a` concurrent-rebuild race — its **third** sighting
(libcoil ×2: 26693573627/26694248379; sibling libscale ×1: 26580282088).

## Root cause — the #165 mitigation rested on a false assumption

`cobrust build` resolves an imported ecosystem module's staticlib via
`cli/src/build.rs::locate_ecosystem_archive`:

- **Phase 1** — honour a `COBRUST_ECOSYSTEM_ARCHIVE_<MOD>` env override
  (returns the path, **no cargo**).
- **Phase 2** (dev fallback) — shell out to `cargo build -p cobrust-<mod>`
  (F44 staleness arbitration), then resolve the now-fresh archive.

`cargo test --workspace` runs ecosystem e2e **test binaries in parallel**; each
test's `cobrust build` hits Phase 2 and spawns its own `cargo build -p
cobrust-<mod>`. Two concurrent cargo invocations → one removes+rewrites
`lib<mod>.a` while a sibling's `clang` links it → `errno=2`.

#165 (`72aa625`) added a **pre-build step** (`cargo build --workspace` before
`cargo test`) on the theory that "pre-building makes the on-demand builds
NO-OPS." **That theory is false**: `cargo test`'s own compile phase
re-fingerprints the staticlib crates, so the first wave of `cobrust build`s
still judge them stale and all race to rebuild. `26709832072` recurred
*after* the pre-build had landed — and `c94df69` adding a 7th coil e2e
(`coil_matmul_e2e.rs`) widened the window enough to manifest again.

The irony: #165 itself documents Phase 1 as "CI / tests swap in a prebuilt
archive" — it **built the right mechanism but never wired it in CI**, relying
on the weaker (and incorrect) no-op assumption instead.

## Resolution

Pre-build, then **stage the fresh archives into the Phase-1 env** so every
parallel `cobrust build` short-circuits to the prebuilt `.a` and **never spawns
cargo** — eliminating the race at its source rather than narrowing its window:

```yaml
- run: cargo build --workspace --locked --jobs 2
- name: Stage prebuilt ecosystem archives (close cobrust-build race)
  run: |
    for a in target/debug/lib*.a; do
      mod=$(basename "$a" .a); mod=${mod#lib}
      case "$mod" in *stdlib*) continue;; esac
      key="COBRUST_ECOSYSTEM_ARCHIVE_$(printf '%s' "$mod" | tr a-z A-Z)"
      echo "$key=$PWD/$a" >> "$GITHUB_ENV"
    done
- run: cargo test --workspace --locked
```

Safe by construction: vars are read only for actually-imported modules
(over-setting is inert; auto-covers future ecosystem crates with zero
per-module maintenance). In-job + same-commit ⇒ the staged archive is fresh,
so **no F44 staleness** is reintroduced. Degrades to current behaviour if an
archive is absent (`p.exists()` + `[ -e ]` guards).

## Lesson (sibling F44 / F64 / F72)

- **A mitigation that narrows a race window is not a fix.** errno=2 is
  timing-dependent; "it passed N times" is not evidence the window is closed.
  Prefer eliminating the *class* (no concurrent writers) over shrinking the
  *window* (pre-build hoping for a no-op).
- **If a mechanism exists to make a code path unreachable, USE it** — do not
  reimplement a weaker guard alongside it. Phase 1 existed; #165 left it
  unwired.

## Queued product-hardening (NOT CI-only)

The CI env-staging closes the race for CI. The **same race bites a real
`.cb` user** running parallel `cobrust build` (e.g. `make -j` over many files
importing coil): N concurrent `cobrust build`s all hit Phase 2 and race. The
principled product fix (per "no legacy debt") is an **advisory file-lock around
the Phase-2 `cargo build -p cobrust-<mod>`** so concurrent `cobrust build`s
serialise the on-demand archive build. Deferred to its own ADSD pass (touches
the hot build path; needs cross-platform flock + poisoning review). Tracked as
the F73 follow-up.
