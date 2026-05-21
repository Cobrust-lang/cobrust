# Cobrust 注册表索引生成器

## 这是什么？

`cobrust-registry` crate 负责生成支持 `cobrust install` 命令所需的 wheel 索引文件。
每次打标签发布新版本时，一个一次性工具会查询 GitHub Releases，发现上传的 wheel
归档文件，并生成结构化的 `wheels.json` 文件 —— 即消费方下载后用于为当前主机
CPU 选择最优 wheel 的注册表索引。

## 为什么这样设计？

- **无需动态服务器。** 注册表是 GitHub Releases 上的静态 JSON（可选 CDN 镜像）。
  生成操作仅在发布时执行一次。
- **镜像 `pip install` 语义。** `cobrust install numpy-cb` 与 `pip install numpy`
  的使用方式完全一致，最大化用户熟悉度。
- **关注点清晰分离。** 生成端（`cobrust-registry`）与消费端
  （`cobrust-pkg::registry_client`）是独立 crate，无循环依赖。

## `wheels.json` 格式

```json
{
  "name": "numpy-cb",
  "version": "0.1.0",
  "wheels": [
    {
      "triple": "x86_64-unknown-linux-gnu",
      "cpu_level": "v3",
      "sha256": "a1b2c3...",
      "url": "https://github.com/.../cobrust-numpy-0.1.0-x86_64-unknown-linux-gnu-v3.tar.gz",
      "size": 4194304
    }
  ]
}
```

- 每个 `(triple, cpu_level)` 变体对应一条记录。
- `cpu_level` 取值：`v1` / `v3` / `v4`（x86-64），`neon` / `sve`
  （aarch64 Linux），`m1` / `m2`（Apple Silicon）。

## 使用 `cobrust-registry-gen`

```bash
cobrust-registry-gen numpy-cb 0.1.0
# 输出至 pkg-index/numpy-cb-0.1.0.json
```

选项：
- `--repo <owner/name>` — 默认：`Cobrust-lang/cobrust`
- `--out-dir <dir>` — 默认：`pkg-index/`
- 设置 `GITHUB_TOKEN` 环境变量可使用认证 API（更高速率限制）

## 已知缺口：SHA-256

GitHub Releases API 的资产元数据中不包含 SHA-256。生成的 `wheels.json`
中 `sha256` 字段为 `""`。W4 阶段将在发布流水线中补充下载后的 SHA 计算步骤。
