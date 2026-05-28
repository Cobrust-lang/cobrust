# Cross-compile to RISC-V Linux (Phase 1)

Cobrust v0.7.0 ships with `--target=riscv64gc-unknown-linux-gnu` support —
build your `.cb` programs on a desktop, run them under QEMU or copy them
to a real RISC-V Linux board (HiFive Unmatched, BeagleV, ESP32-C).

## Quick start

```bash
# 0. One-time host setup (install the cross-toolchain)
rustup target add riscv64gc-unknown-linux-gnu
# Debian / Ubuntu:
sudo apt-get install gcc-riscv64-linux-gnu qemu-user-static
# macOS (Homebrew):
brew install qemu
# (plus a riscv-gnu-toolchain tap — see docs/agent/setup/cross-toolchain.md)

# 1. Write a program
cat > hello_rv.cb <<'CB'
print("hello from riscv64")
CB

# 2. Cross-build
cobrust build --target=riscv64gc-unknown-linux-gnu hello_rv.cb -o hello_rv

# 3. Run under QEMU
qemu-riscv64 -L /usr/riscv64-linux-gnu ./hello_rv
# → hello from riscv64
```

The output is a stock Linux RV64 ELF — `scp` it to your board and run
it natively if you have one.

## What the CLI does under the hood

- `cobrust-stdlib` is cross-compiled via `cargo build --target` to
  `target/<triple>/<profile>/libcobrust_stdlib.a`.
- The C runtime shim is compiled with the cross-cc
  (`riscv64-linux-gnu-gcc`, or `clang --target=...`).
- Each imported ecosystem module (`den`, `coil`, `pit`, …) is similarly
  cross-built. Phase 1 supports all the workspace ecosystem modules
  because they're pure-Rust + transitively rusqlite/serde/time (which
  all have riscv64 Tier-2 support).
- The final link uses the cross-cc with `--start-group/--end-group`
  flags for GNU `ld` so the embedded-libstd de-dup works across the
  stdlib + ecosystem archives.

## Caveats

- Native socket APIs (used by `pit`, `strike`) work — riscv64 Linux has
  full POSIX. WASM (Phase 2) is where socket-using modules will be
  unavailable.
- Pass `--target-cpu=<riscv-cpu>` only when you mean a specific
  microarch (e.g. `sifive-u74`). `--target-cpu=native` is silently
  downgraded to `generic` for cross-targets — the CLI prints a one-line
  notice.

See `docs/agent/setup/cross-toolchain.md` for the full env-var override
hooks and CI-pinning recipes.

## Cross-compile to WebAssembly / WASI (Phase 2)

Cobrust v0.7.0 also ships with `--target=wasm32-wasip1` support — build
your `.cb` programs into a self-contained `.wasm` module runnable under
`wasmtime` (WASI preview 1) or any WASI-compatible host.

### Quick start (wasm32)

```bash
# 0. One-time host setup
rustup target add wasm32-wasip1
# Debian / Ubuntu:
sudo apt-get install clang-18
cargo install wasmtime-cli --locked
# macOS (Homebrew):
brew install llvm@18 wasmtime

# 1. Write a program
cat > hello_wasm.cb <<'CB'
fn main() -> i64:
    print("hello from wasm32")
    return 0
CB

# 2. Cross-build
cobrust build --target=wasm32-wasip1 hello_wasm.cb -o hello_wasm.wasm

# 3. Run under wasmtime
wasmtime run ./hello_wasm.wasm
# → hello from wasm32
```

### Caveats (wasm32)

- WASI preview 1 has **no threads** and **no sockets**. Importing
  `pit` / `strike` (network) or `std.task` / `std.sync` (concurrency)
  on wasm32 is not yet supported in v0.7.0 — Sprint E will add the
  silent `task::spawn` → inline degrade per ADR-0075 §Q2 plus
  ecosystem-availability typecheck gates.
- Sprint D ships **the build path**. The cobrust-stdlib cross-build for
  wasm32 currently uses `--no-default-features` to avoid mimalloc /
  tokio / llm-router (host-only surfaces); per-feature wasm32
  enablement is Sprint E scope.
- The produced `.wasm` is portable across any WASI preview 1 host
  (wasmtime / wasmer / wasmedge / browser polyfill).
