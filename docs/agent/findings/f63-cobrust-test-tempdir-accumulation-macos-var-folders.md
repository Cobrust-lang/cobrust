---
finding_id: F63
title: Cobrust test tempdir累积 — macOS /var/folders/<UUID>/T/cobrust-* 占 156G / 22928 个目录
status: PARTIALLY-RESOLVED (清理已做;long-term RAII fix 仍 deferred 至 cobrust-cli/cobrust-codegen 测试)
date: 2026-05-28
severity: medium
siblings: [feedback_heavy_build_offload_to_workstation]
last_verified_commit: 4089cd8
---

# F63 — Cobrust 测试 tempdir 在 macOS 上累积到 156G / 22928 dirs

## §1 Context

User 2026-05-28 提醒"注意磁盘空间"+"寻找以前的清理流程"。当时磁盘 10G/926G(99% 用)。
按 `feedback_heavy_build_offload_to_workstation.md` §SOP + `cto_operations_runbook.md` 的清
理 SOP,第 1 步 `cargo clean` 释放 37.6G(43G 空闲),仍远低于 user "20% 余量"标准。

挖 runbook 第 2 步"`/tmp/cobrust-*` cleanup"时发现:**macOS 上 `tempfile::tempdir()` 默
认根目录不是 `/tmp` 而是 `/var/folders/<random>/T/`**(macOS sandbox 约定)。在那里:

```
/var/folders/dv/mt2jd0h955qgwtwdnl1r2z2m0000gn/T = 156G
  cobrust-* dirs: 22928 个
```

每个 dir 是过去某次 Cobrust 测试运行的 per-(pid+test-name) tempdir(命名如
`cobrust-0058g-w6-prompt_escape_braces_then_str_len-66001` /
`cobrust-tier2-cpu-tier2_native-92773` / `cobrust-m9-p008-95358` 等)。包含编译输出 `.o`
+ 链接出来的可执行文件(每个 ~1MB+)。

跨过去几个月的测试运行 ×22928 次 ≈ 156G 累积。

## §2 Root cause

Cobrust 集成测试(`crates/cobrust-codegen/tests/*`、`crates/cobrust-cli/tests/*` 等)用
`std::env::temp_dir().join(format!("cobrust-…-{pid}"))` 模式创建测试用 tempdir,但
**多数测试不显式删除它**(用 `tempfile::TempDir` 的 RAII drop 是少数;多数是 raw `PathBuf`
+ `create_dir_all`)。测试退出后,目录留在 `/var/folders/<UUID>/T/`。

只要 macOS 不自动清(看 mtime,几个月没动可能进 macOS 的定期清理,但不保证),就累积。

## §3 Resolution (本次 + long-term)

- **本次清理**:`find /var/folders/dv/.../T -maxdepth 1 -name "cobrust-*" -type d -exec rm
  -rf {} +` 释放 **156G**。结合 cargo clean 的 37.6G,共释放 ~188G。磁盘 10G→198G 空闲
  (99%→78% 用),首次达到 user 标准的 20% 余量线。
- **long-term fix(deferred,non-trivial)**:Cobrust CLI / codegen 集成测试的 tempdir 模
  式改为 `tempfile::TempDir`(RAII drop)或在测试末尾 `std::fs::remove_dir_all`。横跨多
  crate 的批量重构,需独立 sprint;留作 task。

## §4 SOP 更新

`cto_operations_runbook.md` + `feedback_heavy_build_offload_to_workstation.md` 的清理 SOP
第 2 步 "`/tmp/cobrust-*` cleanup" 在 macOS 上必须扫:

```bash
# macOS: tempfile 的根
ROOTS=( "$TMPDIR" "/tmp" "/var/folders" )
for r in "${ROOTS[@]}"; do
  find "$r" -maxdepth 6 -name "cobrust-*" -type d -exec rm -rf {} +
done
```

(等 Cobrust 测试改成 RAII 之后这一步可以从 SOP 退役。)

## §5 Process note

User 一句"寻找以前的清理流程"暴露了 SOP 在 macOS 下的实际路径与 SOP 描述("/tmp")的
脱节 —— 这是 SOP 漂移(F35-sibling claim-vs-landed-drift,但发生在运维文档而非代码)。
SOP 文字应当显式列 macOS `/var/folders/<UUID>/T/` 路径(本 finding 已记入)。
