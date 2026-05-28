# 交叉编译到 RISC-V Linux(Phase 1)

Cobrust v0.7.0 支持 `--target=riscv64gc-unknown-linux-gnu` 交叉编译 ——
你可以在桌面上构建 `.cb` 程序,然后在 QEMU 中运行,或者拷贝到真实的
RISC-V Linux 开发板(HiFive Unmatched / BeagleV / ESP32-C)上执行。

## 快速开始

```bash
# 0. 一次性的宿主环境准备(安装交叉工具链)
rustup target add riscv64gc-unknown-linux-gnu
# Debian / Ubuntu:
sudo apt-get install gcc-riscv64-linux-gnu qemu-user-static
# macOS(Homebrew):
brew install qemu
#(还需要 riscv-gnu-toolchain tap —— 详见 docs/agent/setup/cross-toolchain.md)

# 1. 写一个程序
cat > hello_rv.cb <<'CB'
print("hello from riscv64")
CB

# 2. 交叉构建
cobrust build --target=riscv64gc-unknown-linux-gnu hello_rv.cb -o hello_rv

# 3. 在 QEMU 中运行
qemu-riscv64 -L /usr/riscv64-linux-gnu ./hello_rv
# → hello from riscv64
```

输出是标准的 Linux RV64 ELF —— 如果你有真实开发板,直接 `scp` 过去即可
原生运行。

## CLI 在幕后做了什么

- `cobrust-stdlib` 通过 `cargo build --target` 交叉编译到
  `target/<triple>/<profile>/libcobrust_stdlib.a`。
- C 运行时桥(C runtime shim)使用交叉 cc 编译
  (`riscv64-linux-gnu-gcc`,或 `clang --target=...`)。
- 每个被导入的生态模块(`den`、`coil`、`pit`、……)同样会被交叉构建。
  Phase 1 支持工作区内所有生态模块 —— 因为它们都是纯 Rust + 依赖
  rusqlite / serde / time,这三个都有 riscv64 Tier-2 支持。
- 最终链接使用交叉 cc,并对 GNU `ld` 加上
  `--start-group/--end-group`,确保 stdlib 与生态归档之间的
  「内嵌 libstd 去重」正常工作。

## 注意事项

- 原生 socket API(`pit`、`strike` 使用)在 RV64 Linux 上完全可用 ——
  POSIX 全套。WASM(Phase 2)才是 socket 模块不可用的目标。
- 只有当你真正需要特定微架构(例如 `sifive-u74`)时才传
  `--target-cpu=<riscv-cpu>`。`--target-cpu=native` 在交叉编译时会
  被静默降级为 `generic` —— CLI 会打印一行说明。

完整的环境变量覆写钩子与 CI 钉版配方见
`docs/agent/setup/cross-toolchain.md`。
