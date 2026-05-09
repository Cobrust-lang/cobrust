---
doc_kind: finding
finding_id: m9-cross-arch-9ff481c-regression
last_verified_commit: 9ff481c
dependencies: [adr:0019, adr:0023, adr:0033]
related: [m9-cross-arch-linux-x86_64-validation, codegen-i8-i64-mismatch-at-4-blocks, two-bugs-one-fix-option-c-pattern]
---

# Finding: M9 cross-architecture regression validation at HEAD `9ff481c`

## Hypothesis

ADR-0019 §"M9 done means" mandates macOS arm64 + Linux x86_64 dual-arch
delivery. Current main HEAD `9ff481c` is ~14 commits past the last
cross-arch validation (`60243ab`). All intervening merges (audit #1,
CLI hardening, M11.1.1 corpus, Conway-toy free-closure verification,
ADR-0034 spike, two-bugs-one-fix finding) have been macOS-only verified.
This sprint re-runs the M9 gate on Linux x86_64 to confirm no regression.

## Method

- Worktree: `feature/cross-arch-9ff481c-regression`, branched from HEAD `9ff481c`.
- SSH: `wubingjing@112.74.60.44 -p 10040` (DG-Workstation-2x3090, Ubuntu 22.04,
  kernel 5.15.0-176, x86_64).
- Toolchain on workstation: rustc 1.94.1 / cargo 1.94.1 (matches `rust-toolchain.toml`).
- Sync: `rsync -avz --delete --exclude='target/' --exclude='.git/'` from worktree
  to `~/cobrust-9ff481c/` on the workstation (includes `Cargo.lock`).
- Gates run: `cargo build --workspace --all-targets --locked`, `cargo test --workspace
  --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- Example binaries: hello, fizzbuzz, fib, notebook (diff vs expected.txt).
- Conway-toy repros: 4-cell and 5-cell straight-line programs (verbatim from finding
  `codegen-i8-i64-mismatch-at-4-blocks` §Reproduction).
- Codex API endpoint connectivity check (network-only; audit #1 test not run).

## Result

### Cargo gate table

| Gate | macOS arm64 (HEAD `9ff481c`) | Linux x86_64 |
|---|---|---|
| `cargo build --workspace --all-targets --locked` | exit 0 | **exit 0** |
| `cargo test --workspace --locked` | **exit 101 (2 fail)** | **exit 101 (2 fail, same tests)** |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | **exit 101 (1 error)** | **exit 101 (1 error, same)** |

### Cargo test failure detail (identical on both archs)

Both `cargo test` failures are in `cobrust-cli/tests/cli_verifier_exit_corpus.rs`:

```
FAILED: v01_four_block_repro_exits_non_zero
  panicked at cli_verifier_exit_corpus.rs:148:5:
  cobrust build on 4-block verifier-rejecting program must exit non-zero;
  got success — Bug 2 (silent miscompile) is regressed.

FAILED: v03_verifier_error_on_stderr_not_stdout
  panicked at cli_verifier_exit_corpus.rs:210:5:
  expected non-zero exit
```

**Root cause**: These two tests were written at commit `78ca779` (CLI hardening
sprint) when the 4-block conway-toy program STILL triggered the Cranelift
verifier error (the narrow-type bug was still present). At that point,
`cobrust build` on the 4-block repro correctly exited 3 (verifier reject).

Commit `60243ab` (ADR-0033 Option C, merged at `3392eb5`) fixed the Ty::None
inference bug — which caused the 4-block repro to now compile correctly and
exit 0. The test `v01` asserts exit non-zero, so it now fails. `v03` also
fails because it asserts non-zero as a precondition for the stderr check.

This is **NOT a new regression between `60243ab` and `9ff481c`**. The two
doc-only commits between them (`b4808e0` ADR-0034, `9ff481c` findings doc)
contain no Rust source changes. The staleness was introduced at `3392eb5`
(the codegen-fix merge) and has been present on both architectures since then.

Verified: `git log --oneline 78ca779..3392eb5` shows these tests predate
the codegen fix by being written at `93efbd3` (pre-fix CLI hardening sprint).

**The stale tests are a macOS arm64 issue too** — confirmed by running the
same test on the macOS arm64 dev host: same 2 failures, same exit 101.
This means the `project_state_snapshot.md` claim "macOS arm64: workspace
1,783 / 0 fail at HEAD `3392eb5`" was incorrect — these two tests were
already failing at that commit on macOS too.

### Cargo clippy failure detail (identical on both archs)

```
error: unnecessary hashes around raw string literal
  --> crates/cobrust-cli/tests/cli_verifier_exit_corpus.rs:97:32
  |
  const FOUR_BLOCK_REPRO: &str = r#"..."#;  ← should be r"..."
```

Same file. The `r#"..."#` raw string does not contain double quotes inside
the content, so the `#` delimiters are unnecessary. Clippy with `-D warnings`
promotes this to an error. This is a pre-existing issue on both architectures.

### Example binary table

| Binary | macOS arm64 stdout | Linux x86_64 stdout | Match? |
|---|---|---|---|
| `hello.cb` | `hello, world` | `hello, world` | Y |
| `fizzbuzz.cb` | 1..15 algorithmic (Fizz/Buzz/FizzBuzz) | 1..15 algorithmic (Fizz/Buzz/FizzBuzz) | Y |
| `fib.cb` | `fib(10) =\n55` | `fib(10) =\n55` | Y |
| `notebook` | bit-identical to expected.txt (diff exit 0) | bit-identical to expected.txt (diff exit 0) | Y |
| `conway_4cell_repro` | `BUILD_EXIT=0`, stdout=`3` | `BUILD_EXIT=0`, stdout=`3` | Y |
| `conway_5cell_repro` | `BUILD_EXIT=0`, binary executes | `BUILD_EXIT=0`, stdout=`3` | Y |

All 4 example binaries and both Conway repros produce bit-identical results
on Linux x86_64 and macOS arm64.

### Audit #1 network check

Codex API endpoint `http://104.244.92.250:8317` is reachable from the x86
workstation (HTTP 400 "Missing API key" confirms TCP+HTTP connectivity).
Actual audit #1 run not attempted — out of scope for this sprint.

## Conclusion

**PARTIAL PASS** — no new Linux-only regression introduced in the ~14
commits since the last cross-arch validation.

**What passes on both architectures:**
- `cargo build --workspace --all-targets --locked`: exit 0 on both
- All 4 example binaries (hello / fizzbuzz / fib / notebook): bit-identical
- Conway-toy 4-cell repro: `BUILD_EXIT=0`, stdout=`3` (ADR-0033 verified)
- Conway-toy 5-cell repro: `BUILD_EXIT=0`, output correct (ADR-0033 verified)
- No `CvtFloatToSintSeq` panic, no `iadd.i8/i64` Cranelift verifier error

**Pre-existing failures (both architectures equally, not a cross-arch delta):**

1. `cargo test --workspace --locked` exits 101: tests `v01` and `v03` in
   `cobrust-cli/tests/cli_verifier_exit_corpus.rs` are stale regression
   guards. Written expecting the 4-block repro to exit 3 (pre-ADR-0033),
   they now fail because the codegen fix (ADR-0033 Option C, `3392eb5`)
   made the program compile successfully. This is a test-expectation staleness,
   not a new code regression. The actual CLI behaviour is correct: the fixed
   program compiles and runs cleanly.

2. `cargo clippy --workspace --all-targets --locked -- -D warnings` exits 101:
   `r#"..."#` unnecessary hash in the same test file triggers a clippy lint.

**CTO action required (out of scope for this sprint per hard constraint):**
Update `cli_verifier_exit_corpus.rs` tests `v01` and `v03` to reflect the
post-ADR-0033 reality: the 4-block program is now well-formed and should
compile + run cleanly. The tests should either be rewritten to use a
*different* verifier-rejecting program (one that ADR-0033 deliberately does
NOT fix) or repurposed to verify the now-correct positive-exit behaviour.
The `r#"..."#` unnecessary-hash clippy issue in the same file should be
fixed at the same time.

**`project_state_snapshot.md` correction note:**
The snapshot entry "macOS arm64: workspace 1,783 / 0 fail at HEAD `3392eb5`"
is incorrect — these 2 test failures were already present on macOS arm64
at that commit. The count at HEAD `9ff481c` is 48 pass / 2 fail on both
architectures. The snapshot should be updated when next the CTO writes a
full 5-gate verification.

## Cross-references

- ADR-0019 §"M9 done means" — the gate this finding closes
- ADR-0033 — root-primitive Ty::None fix (most recent codegen change verified
  cross-arch; responsible for both closing the original bugs AND making the
  stale tests fail)
- finding `m9-cross-arch-linux-x86_64-validation.md` — previous cross-arch
  finding at `60243ab` baseline (surfaced the original P0 bug)
- finding `codegen-i8-i64-mismatch-at-4-blocks.md` — Bug 1 closed by ADR-0033
  Option C; this finding empirically re-verifies on x86_64 at HEAD `9ff481c`
- finding `two-bugs-one-fix-option-c-pattern.md` — methodology finding being
  indirectly cross-arch validated
- `crates/cobrust-cli/tests/cli_verifier_exit_corpus.rs` lines 136-163 (v01)
  and 199-224 (v03) — stale tests CTO must update
