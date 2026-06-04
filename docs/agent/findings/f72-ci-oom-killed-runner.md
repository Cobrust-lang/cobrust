---
finding_id: F72
title: CI killed-runner flake — OOM on `cargo build --workspace --all-targets`
status: mitigated
first_observed: 2026-05-xx (a1c9d83-era)
recurred: 2026-06-05 (batch-19, run 26978770920 on 14c860c)
severity: flake (false-red; not a code defect)
sibling_findings: [F73 (libcoil.a build race), F44 (CI cache stale-green)]
related_memory: feedback_ci_killed_runner_flake
last_verified_commit: 14c860c
---

# F72 — CI killed-runner flake (OOM on the `--all-targets` build)

## Signature (how to recognize it — DO NOT misdiagnose as a code bug)

A CI failure is THIS flake, not a real error, when ALL of:

- The failing job is **`cargo build (ubuntu-latest)`** (the `--all-targets` build).
- The failing **step's `conclusion` is BLANK** (`""`), not `"failure"` — i.e.
  `gh run view <id> --json jobs` shows the build step with an empty conclusion,
  and the trailing `Post …` steps also blank.
- **`gh run view <id> --log-failed` is EMPTY** — no `error[E…]`, no `error:`, no
  `panicked`, no assertion. The process was killed before emitting any diagnostic.

This is the kernel OOM-killer reaping the build process mid-link. Contrast with a
REAL build/lockfile error (which prints `error[E…]` / a lockfile-mismatch help line
— see [[f64-dev-dep-cargo-lock-staging-miss]]) and with F73's `linker cc exited 256`
/ `ld: file cannot be open()ed` (the libcoil.a archive race).

## Root cause

`cargo build --workspace --all-targets --locked` compiles every test + bench binary.
Each test binary statically links LLVM-18 (ADR-0070 §X.3 flip) → a multi-GiB peak
RSS during the link phase. The ubuntu-latest runner ships ~7 GiB RAM. Two such
links in parallel (`--jobs 2`) transiently exceed RAM → the OOM killer terminates
the build, leaving the blank-step / empty-log signature.

`--jobs 2` (down from the default `-j <cores>`) was the first mitigation but was
**not sufficient alone** — the per-target link peak is large enough that even 2
concurrent links can spike past RAM intermittently.

## Diagnosis procedure

1. Confirm the signature above (blank step conclusion + empty `--log-failed`).
2. Local rule-out: `cargo build --workspace --locked` → exit 0 proves the code
   compiles (no real error); `--locked` also rules out a Cargo.lock mismatch.
3. `gh run rerun <id> --failed` → re-runs the killed job; it passes (the spike is
   probabilistic). Watch by **run id**, not short-SHA.

## Mitigation (ci.yml `build` job, 2026-06-05)

Keep `--jobs 2` for speed AND add a **12 GiB swapfile** (ubuntu only) before the
build for transient link-phase headroom (`swapoff -a; fallocate -l 12G /swapfile;
mkswap; swapon`). Swap absorbs the spill without OOM-killing; if unused it costs
nothing. macOS is unaffected (the step is `if: runner.os == 'Linux'`).

If the flake recurs despite swap, the next levers are `--jobs 1` (halve the
concurrent-link peak, ~2× slower) or splitting `--all-targets` into lib/bin + a
separate test-target build.

## Lesson

A blank-conclusion failing step + empty failed-log on a memory-heavy `cargo build`
is infra (OOM/timeout), not your code. Local-reproduce + rerun before touching the
diff. CI red ≠ code bug.
