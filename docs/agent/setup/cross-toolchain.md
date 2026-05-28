---
doc_kind: setup
title: Cross-compile toolchain (ADR-0075 Phase 1 — riscv64gc-unknown-linux-gnu)
status: active
last_verified_commit: WIP
relates_to: [adr:0075, "code:crates/cobrust-cli/src/build.rs"]
---

# Cross-compile toolchain (riscv64gc-unknown-linux-gnu)

Phase 1 of ADR-0075 enables `cobrust build --target=riscv64gc-unknown-linux-gnu`
on a desktop host (macOS or Linux) producing a riscv64 Linux ELF that runs
under `qemu-riscv64`.

The `cobrust` CLI does not bundle the cross toolchain. This doc lists the
three host-machine prerequisites and the env-var override hooks the CLI
checks.

## 1. Required host tooling

### 1.1 Rust std for the target

```bash
rustup target add riscv64gc-unknown-linux-gnu
```

Tier-2 Rust target. Provides `libstd.rlib` etc. cross-compiled for riscv64
so `cargo build -p cobrust-stdlib --target=riscv64gc-unknown-linux-gnu`
(invoked by the CLI as a subprocess) succeeds.

The CLI fails fast with a clear error pointing at this command when the
target isn't installed.

### 1.2 Cross C compiler

The C runtime shim (`cobrust_main.c`) AND the final link step both need a
C compiler that targets riscv64-linux-gnu. Three install paths supported,
checked in order:

#### Option A — Debian / Ubuntu apt

```bash
sudo apt-get install gcc-riscv64-linux-gnu
```

Installs `riscv64-linux-gnu-gcc` on PATH. The CLI detects this binary by
its conventional GNU prefix and uses it automatically.

#### Option B — Homebrew (macOS)

```bash
brew install riscv-gnu-toolchain
# or
brew install --cask gcc-riscv64-elf  # bare-metal variant; works for linux-gnu via sysroot
```

Homebrew taps vary. If the binary lands as `riscv64-elf-gcc` (bare-metal
flavor) instead of `riscv64-linux-gnu-gcc`, set `$CC` to point at it plus
the sysroot path:

```bash
export CC="riscv64-elf-gcc --sysroot=/path/to/riscv-sysroot"
```

#### Option C — clang + sysroot (any host)

```bash
# macOS: brew install llvm  (LLVM 18+)
# Linux: apt install clang
```

The CLI falls back to `clang --target=riscv64-linux-gnu` when no
`riscv64-linux-gnu-gcc` is found. For this to actually link, clang must
have access to a riscv64 sysroot (libc headers + shared objects). Most
desktop distros need this wired manually:

```bash
export COBRUST_CC_RISCV64GC_UNKNOWN_LINUX_GNU="clang --target=riscv64-linux-gnu --sysroot=/opt/riscv-sysroot"
```

### 1.3 QEMU user-mode emulator (for running output)

```bash
# Debian / Ubuntu
sudo apt-get install qemu-user-static
# macOS
brew install qemu
```

Installs `qemu-riscv64` (user-mode) on PATH. The dynamic linker resolution
uses `qemu-riscv64 -L /usr/riscv64-linux-gnu <binary>`; override the
sysroot via `$COBRUST_QEMU_RV_SYSROOT` if you installed it elsewhere.

## 2. CLI env-var override hooks

Resolved in priority order inside `crates/cobrust-cli/src/build.rs`:

| Env var | Purpose | Example |
|---|---|---|
| `COBRUST_CC_RISCV64GC_UNKNOWN_LINUX_GNU` | Per-target CC override; highest priority | `clang --target=riscv64-linux-gnu --sysroot=/opt/riscv-sysroot` |
| `CC` | Global CC override; applies to host + cross | `riscv64-linux-gnu-gcc` |
| `COBRUST_STDLIB_ARCHIVE_RISCV64GC_UNKNOWN_LINUX_GNU` | Prebuilt cobrust-stdlib archive path | `/cache/libcobrust_stdlib.a` |
| `COBRUST_ECOSYSTEM_ARCHIVE_<MOD>_RISCV64GC_UNKNOWN_LINUX_GNU` | Prebuilt ecosystem archive (per module) | `COBRUST_ECOSYSTEM_ARCHIVE_DEN_RISCV64GC_UNKNOWN_LINUX_GNU=/cache/libden.a` |
| `COBRUST_QEMU_RV_SYSROOT` | qemu-riscv64 -L sysroot path | `/opt/riscv-sysroot` |

The general-form key is uppercased target triple with `-` → `_`. The same
pattern extends to future Phase 2 targets (`wasm32-wasip1`).

## 3. Verification flow

```bash
# 1. Toolchain check
rustc --print target-list | grep riscv64gc-unknown-linux-gnu  # confirms rustup knows the target
rustup target list --installed | grep riscv64gc                # confirms it's installed
which riscv64-linux-gnu-gcc                                    # confirms cross-cc available
which qemu-riscv64                                             # confirms qemu available

# 2. Build + run hello world
cat > /tmp/hello_rv.cb <<'CB'
print("hello from riscv64")
CB
LLVM_SYS_181_PREFIX=/opt/homebrew/opt/llvm@18 \
    cobrust build --target=riscv64gc-unknown-linux-gnu /tmp/hello_rv.cb -o /tmp/hello_rv
file /tmp/hello_rv                                # should report ELF 64-bit LSB executable, UCB RISC-V
qemu-riscv64 -L /usr/riscv64-linux-gnu /tmp/hello_rv
# → hello from riscv64
```

The E2E test `crates/cobrust-cli/tests/cross_compile_riscv64_e2e.rs`
performs the same flow with cleanup. It skips cleanly with a one-line
note when any of the toolchain pieces are absent — safe to commit and
ship to clean dev boxes; CI runners with the toolchain installed exercise
it for real.

## 4. F58-sibling safety (cross-target CPU tuning)

`--target-cpu=native` resolves to the *host* CPU via LLVM
`get_host_cpu_name`. That's meaningless on cross-targets (host might be
`apple-m1` while the target is `generic-rv64`). The CLI guards this:
when the triple isn't host AND `--target-cpu=native` was passed, the flag
is downgraded to `None` (= LLVM `generic` baseline) and a stderr note is
emitted so the override is auditable. Pass an explicit CPU name (e.g.
`--target-cpu=sifive-u74`) to opt into a specific riscv64 microarch.

## 5. Known limitations (Phase 1 scope)

- macOS host x riscv64 target via Homebrew LLVM: `lld` may not be in the
  default formula. Falling back to `riscv64-linux-gnu-gcc` (which uses
  GNU `ld`) is recommended on macOS.
- Phase 2 (wasm32-wasip1) is out of scope here; see ADR-0075 §5 Phase 2
  for the wasi sprint plan.
- Ecosystem modules with native deps (rusqlite, reqwest) cross-compile
  correctly via Rust target tier-2; opaque-pointer ABI (ADR-0072) is
  pointer-width-correct under riscv64 via `Box::into_raw`/`from_raw`.
