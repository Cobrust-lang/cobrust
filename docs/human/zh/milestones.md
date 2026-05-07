# 里程碑

| 里程碑 | 范围 | 验收标准 |
|---|---|---|
| **M0** | 仓库骨架、文档骨架（zh/en/agent）、CI、ADR 模板、lint 配置 | `cargo build` 通过；三棵文档树存在；ADR-0001（许可证）落地 |
| **M1** | Cobrust 核心语法的词法器 + 语法分析器 + AST | "核心 30 形式" round-trip；24h fuzz 测试无 crash |
| **M2** | 静态核心的类型检查器（暂不含 `dyn`） | 通过精选的"良类型 / 病类型程序"测试套件 |
| **M3** | LLM Router crate（独立可用） | OpenAI + Anthropic adapter 工作；缓存 + 账本工作；consensus 模式在合成任务上验证 |
| **M4** | L0 + L1 流水线在 `tomli` 上端到端跑通 | 完整来源清单；通过 PyO3 wrapper 跑过 `tomli` 测试套件 |
| **M5** | L2 + L3 gate 接通；翻译第二个库（`python-dateutil` 核心） | 差分测试失败自动路由到 repair；benchmark 报告 |
| **M6 ✅** | 第一个含原生扩展的库（`msgpack`）— Cython 词法 shim、perf-gate 失败即触发修复、dateutil L3 拓宽、PyO3 构建路径 | pack/unpack 字节级与 CPython oracle 对齐；Cython shim 解析 `_packer.pyx`/`_unpacker.pyx` 构件；`--features pyo3` 编译通过 |
| **M7+** | 数值层：`numpy` 核心子集 | 单独规划文档。**大头。仅在 M6 完成之后开始。** |

## 当前状态

**M0..M6 已交付。** 仓库骨架已就位；词法/语法/AST（M1）、HIR + 双向类型检查器（M2）、provider-agnostic LLM Router（M3）均已上线；**M4** 端到端跑通 L0+L1 翻译流水线（目标 `tomli`），生成的 `cobrust-tomli` crate 已提交以保障 gate 稳定。**M5** 完成闭环合龙：L2.perf 基准压测器、L2.behavior 修复循环（`BehaviorVerifier` 钩子 + 按 attempt 路由的合成提供商）、L3 下游依赖驱动器。第二个翻译库 `python-dateutil`（核心：`parse_iso` + `relativedelta_add`）作为 M5 交付物落地；2/5 依赖（croniter, freezegun）通过 L3 门禁，剩余 3/5（pandas, sqlalchemy, pendulum）按 ADR-0009 显式推迟到 M6。**M6** 是原生扩展里程碑：`cobrust-msgpack` 端到端翻译 msgpack-python 1.0.8（17 个纯 Python + 2 个 Cython 类型化入口），通过 Cython 词法 shim（`task = "translate_cython"`）；`PerfVerifier` 回调让 L2.perf 失败即触发修复，演示一次 `pack_uint` 故意做差的修复路径；dateutil L3 拓宽到 4/5 + 1 跳过（pendulum tz 越界，按 ADR-0010 §5）；`cobrust-dateutil` 与 `cobrust-msgpack` 均启用 `--features pyo3`（按 ADR-0011）。测试总数：306（基线 272；M6 净增 34）。

## 开发纪律（适用于所有里程碑）

- **测试先行**：编译器内部一律先写失败测试，再写实现
- **闭环验证**：每个翻译库的 L0–L3 gate 全部不可跳
- **ADR-or-it-didn't-happen**：影响两个及以上文件的决定都要写 ADR
- **doc-coverage 在 CI 强制**：任何 public item 缺 zh / en / agent 文档 → CI 红
- **Provenance-or-it-didn't-happen**：AI 翻译文件必须带清单头
- **原子提交**：代码 + 测试 + 文档（zh、en、agent）+ ADR（如适用）一次性提交

## 里程碑之间的依赖

```mermaid
flowchart LR
    M0 --> M1 --> M2 --> M3
    M3 --> M4 --> M5 --> M6 --> M7
    M0 -.lint+ci.-> M1
    M0 -.lint+ci.-> M2
    M0 -.lint+ci.-> M3
    M3 -.router.-> M4
    M3 -.router.-> M5
    M2 -.types.-> M4
```

- M0 是公共底座，所有后续里程碑共享
- M3（Router）是 M4+ 翻译流水线的前提
- M2（类型检查器）是 M4+ 验证翻译产物的前提
