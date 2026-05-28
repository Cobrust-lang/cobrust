---
doc_kind: setup
title: Cross-compile toolchain (ADR-0075 Phase 1 + 2 — riscv64gc-unknown-linux-gnu + wasm32-wasip1)
status: active
last_verified_commit: WIP
relates_to: [adr:0075, "code:crates/cobrust-cli/src/build.rs"]
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

#### C compiler (clang LLVM 18+)

WASM has no GNU cross-prefix convention; clang is the canonical
wasm32-wasip1 driver. LLVM 18's `clang --target=wasm32-wasip1` bundles
the wasi-libc sysroot + `wasm-ld` linker automatically.

| Host | Install |
| --- | --- |
| Debian / Ubuntu | `sudo apt-get install clang-18` |
| macOS (Homebrew) | `brew install llvm@18` (provides `clang` under `$(brew --prefix llvm@18)/bin`) |

The CLI probes `clang-18` first, then plain `clang`; either works.

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
| `COBRUST_CC_WASM32_WASIP1` | Per-target CC override | `clang-18 --target=wasm32-wasip1 --sysroot=/opt/wasi-sysroot` |
| `COBRUST_STDLIB_ARCHIVE_WASM32_WASIP1` | Prebuilt cobrust-stdlib archive path | `/cache/libcobrust_stdlib.a` |
| `COBRUST_ECOSYSTEM_ARCHIVE_<MOD>_WASM32_WASIP1` | Prebuilt ecosystem archive (per module) | `COBRUST_ECOSYSTEM_ARCHIVE_DEN_WASM32_WASIP1=/cache/libden.a` |

### 6.3 Verification flow (wasm32)

```bash
# 1. Toolchain check
rustup target list --installed | grep wasm32-wasip1     # confirms it's installed
which clang-18                                          # or `which clang`
which wasmtime                                          # confirms wasmtime available

# 2. Build + run hello world
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

### 6.4 Known limitations (Phase 2 Sprint D scope)

- **stdlib default features**: `mimalloc-alloc`, `tokio-runtime`,
  `llm-router` all require threading / sockets / native allocators
  wasm32-wasip1 does not expose. The CI cross-build runs with
  `--no-default-features`; per-feature wasm32 enablement is **Sprint E**
  scope (F70 candidate). Until then, `cobrust build --target=wasm32-wasip1`
  on programs that import `std.task` / `std.sync` / `std.llm` will fail
  at the ecosystem-archive cross-build step with a `tokio`-mio-related
  compile error.
- **Network modules (`pit`, `strike`)**: WASI preview 1 has no socket
  API; importing them on wasm32 will fail at link / runtime. ADR-0075 §5
  Phase 2 spec'd `EcosystemUnavailableOnTarget` is a future-typecheck
  gate beyond Sprint D scope.
- **`task::spawn` silent-degrade**: per ADR-0075 §Q2 the policy is to
  silently degrade `task::spawn` to inline single-threaded execution on
  wasm32. Sprint D ships the build path; the spawn-degrade is Sprint E.
