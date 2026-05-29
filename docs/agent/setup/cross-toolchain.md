---
doc_kind: setup
title: Cross-compile toolchain (ADR-0075 Phase 1 + 2 — riscv64gc-unknown-linux-gnu + wasm32-wasip1)
status: active
last_verified_commit: WIP
relates_to: [adr:0075, "code:crates/cobrust-cli/src/build.rs", "finding:F70"]
---

# Cross-compile toolchain (riscv64gc-unknown-linux-gnu + wasm32-wasip1)

Phase 1 of ADR-0075 enables `cobrust build --target=riscv64gc-unknown-linux-gnu`
on a desktop host (macOS or Linux) producing a riscv64 Linux ELF that runs
under `qemu-riscv64`.

Phase 2 Sprint D adds `cobrust build --target=wasm32-wasip1` producing a
`.wasm` module that runs under `wasmtime` (WASI preview 1).

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
- Phase 2 (wasm32-wasip1) — see §6 below.
- Ecosystem modules with native deps (rusqlite, reqwest) cross-compile
  correctly via Rust target tier-2; opaque-pointer ABI (ADR-0072) is
  pointer-width-correct under riscv64 via `Box::into_raw`/`from_raw`.

## 6. Phase 2 — wasm32-wasip1 (Sprint D)

Phase 2 Sprint D ships the cross-build + live-smoke plumbing for
`wasm32-wasip1`. The CLI accepts `--target=wasm32-wasip1` and emits a
self-contained `.wasm` module runnable under `wasmtime` (WASI preview 1).

### 6.1 Required host tooling (wasm32)

#### Rust std for the target

```bash
rustup target add wasm32-wasip1
```

Tier-2 Rust target. Provides `libstd.rlib` etc. cross-compiled for
wasm32-wasi so `cargo build -p cobrust-stdlib --target=wasm32-wasip1`
(invoked by the CLI as a subprocess) succeeds.

#### C compiler + wasi-libc sysroot (wasi-sdk — REQUIRED, Sprint E)

WASM has no GNU cross-prefix convention; clang is the canonical
wasm32-wasip1 driver. **A wasi-libc sysroot is mandatory.** Sprint D
assumed `clang --target=wasm32-wasip1` bundles the sysroot automatically —
this is FALSE for apt's `clang-18` (`/usr/lib/llvm-18`): without a sysroot
it falls back to host glibc headers and fails with
`'bits/libc-header-start.h' file not found` (the Sprint D live-CI break).

The fix is a real [wasi-sdk](https://github.com/WebAssembly/wasi-sdk/releases),
which ships a version-matched clang + a `wasm32-wasip1` sysroot:

| Host | Install |
| --- | --- |
| Linux / CI | Download `wasi-sdk-<N>.0-x86_64-linux.tar.gz`, extract to `/opt`, set `WASI_SDK_PATH=/opt/wasi-sdk-<N>.0-x86_64-linux` |
| macOS (Homebrew) | `brew install llvm@18` for the `cobrust-cli` LLVM link; download `wasi-sdk-<N>.0-x86_64-macos.tar.gz` (or arm64) for the sysroot, set `WASI_SDK_PATH` |

Two ways to wire the cross-cc:

- **Bundled clang (recommended, version-matched):**
  `export COBRUST_CC_WASM32_WASIP1="$WASI_SDK_PATH/bin/clang"`. The CLI uses
  it directly and additionally appends `--sysroot` (idempotent — the
  bundled clang already defaults to its own sysroot).
- **apt/brew clang + sysroot:** the CLI falls back to `clang-18` / `clang`
  and appends `--sysroot=$WASI_SDK_PATH/share/wasi-sysroot`
  (resolved from `WASI_SDK_PATH`). Works as long as the sysroot is set,
  but the clang version may not match the sysroot's wasi-libc release.

The CLI resolves the sysroot via `COBRUST_WASI_SYSROOT` (direct) then
`WASI_SDK_PATH` (sysroot auto-derived at `<SDK>/share/wasi-sysroot`), and
fails fast with a fix-shaped error naming both env vars when neither is set.

#### `wasmtime` runtime

```bash
# Linux/CI:
cargo install wasmtime-cli --locked
# macOS:
brew install wasmtime
```

Installs `wasmtime` on PATH. The cross-test + CI smoke step both run
the produced `.wasm` via plain `wasmtime run <module>.wasm`.

### 6.2 CLI env-var override hooks (wasm32)

| Env var | Purpose | Example |
| --- | --- | --- |
| `WASI_SDK_PATH` | wasi-sdk install root; sysroot auto-derived at `<SDK>/share/wasi-sysroot` | `/opt/wasi-sdk-25.0-x86_64-linux` |
| `COBRUST_WASI_SYSROOT` | Direct wasi-libc sysroot path (highest priority) | `/opt/wasi-sdk-25.0-x86_64-linux/share/wasi-sysroot` |
| `COBRUST_CC_WASM32_WASIP1` | Per-target CC override (bare program path; CLI appends `--target` + `--sysroot`) | `/opt/wasi-sdk-25.0-x86_64-linux/bin/clang` |
| `COBRUST_STDLIB_ARCHIVE_WASM32_WASIP1` | Prebuilt cobrust-stdlib archive path | `/cache/libcobrust_stdlib.a` |
| `COBRUST_ECOSYSTEM_ARCHIVE_<MOD>_WASM32_WASIP1` | Prebuilt ecosystem archive (per module) | `COBRUST_ECOSYSTEM_ARCHIVE_DEN_WASM32_WASIP1=/cache/libden.a` |

Sysroot resolution order: `COBRUST_WASI_SYSROOT` → `WASI_SDK_PATH`
(deriving `<SDK>/share/wasi-sysroot`). When the target is wasm and neither
is set, `cobrust build` errors with an actionable message before invoking
clang. The `COBRUST_CC_WASM32_WASIP1` value is a bare program path (not a
command line) — the CLI passes `--target=<triple>` + `--sysroot=<path>` as
separate args, so do NOT embed flags in the env value.

### 6.3 Verification flow (wasm32)

```bash
# 1. Toolchain check
rustup target list --installed | grep wasm32-wasip1     # confirms it's installed
test -d "$WASI_SDK_PATH/share/wasi-sysroot"             # confirms sysroot present
which wasmtime                                          # confirms wasmtime available

# 2. Build + run hello world
export WASI_SDK_PATH=/opt/wasi-sdk-25.0-x86_64-linux    # adjust to your install
export COBRUST_CC_WASM32_WASIP1="$WASI_SDK_PATH/bin/clang"
cat > /tmp/hello_wasm.cb <<'CB'
fn main() -> i64:
    print("hello from wasm32")
    return 0
CB
LLVM_SYS_181_PREFIX=/opt/homebrew/opt/llvm@18 \
    cobrust build --target=wasm32-wasip1 /tmp/hello_wasm.cb -o /tmp/hello_wasm.wasm
file /tmp/hello_wasm.wasm                               # should report WebAssembly (wasm) binary
wasmtime run /tmp/hello_wasm.wasm
# → hello from wasm32
```

The E2E test `crates/cobrust-cli/tests/cross_compile_wasm32_e2e.rs`
performs the same flow with cleanup, gated on toolchain availability.

### 6.4 Known status (Phase 2 Sprint E)

**Resolved by Sprint E:**

- **wasi-sysroot wiring** — the hello-world cross-cc no longer fails with
  `bits/libc-header-start.h file not found`. The CLI resolves the sysroot
  from `COBRUST_WASI_SYSROOT` / `WASI_SDK_PATH` and passes `--sysroot` to
  clang. CI installs wasi-sdk-25 (see `.github/workflows/ci.yml`
  `wasm32-cross-smoke`).
- **stdlib `--no-default-features` for wasm** — the CLI now auto-passes
  `--no-default-features` when cross-building `cobrust-stdlib` for a wasm
  target, so the default feature trio (which doesn't build for wasm)
  doesn't block a clean-machine hello-world build.

**Deferred (a future sprint — see `docs/agent/findings/f70-cobrust-stdlib-wasm32-feature-flag-gap.md`):**

- **stdlib default features on wasm** — `mimalloc-alloc` / `tokio-runtime`
  / `llm-router` still don't build for wasm32-wasip1 (native mimalloc + mio
  sockets + TLS network stack). The hello path needs none of them, so this
  is deferred. A `.cb` program importing `std.task` / `std.sync` / `std.llm`
  won't link on wasm until the feature-on-wasm matrix is wired (F70).
- **Network modules (`pit`, `strike`)** — WASI preview 1 has no socket
  API; importing them on wasm32 fails at link / runtime. The ADR-0075 §5
  spec'd `EcosystemUnavailableOnTarget` typecheck gate is deferred (F70).
- **`task::spawn` silent-degrade** — per ADR-0075 §Q2 the eventual policy
  is inline single-threaded degrade on wasm32; not yet implemented (F70).
