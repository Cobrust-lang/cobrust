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
| **M7.0 ✅** | numpy 核心子集第一个子里程碑：ndarray 基础（按 ADR-0012 + ADR-0013）— closed `Dtype` 枚举（`Int32 / Int64 / Float32 / Float64 / Bool`）、tagged-union `Array`、四个构造器（`array` / `zeros` / `ones` / `arange`） | ≥ 50 良类型 + ≥ 50 病类型程序；≥ 1000 fuzz 不 panic；与 upstream numpy 2.0.2 差分（int/bool 字节级、float `rtol=1e-12`） |
| **M7.1 ✅** | universal functions + 广播 + NEP 50 类型晋升（按 ADR-0014）；类型化构造器 + 多维 nested-list 解析；关闭 ADR-0013 follow-up #1-#4（单态化分发、类型化构造器、L2.perf flip、多维 nested-list） | 50 良类型 + 50 病类型 ufunc 程序；每个 ufunc >= 1200 fuzz 输入差分 vs upstream numpy 2.0.2（int/bool 字节级、float `rtol=1e-7`）；广播表（22 条）；L2.perf gate 翻为强制 |
| **M7.2 ✅** | 索引（基本切片、整数数组、布尔掩码）；`np.where`；视图（`ArrayView<'a>` / `ArrayViewMut<'a>`，按 ADR-0015）；闭合 `Index` enum + `SliceSpec`；4 个新错误变体（`IndexError`、`OutOfBoundsIndex`、`BoolMaskShapeMismatch`、`IndexDtypeNotInteger`） | ≥ 50 良类型 + ≥ 50 病类型索引程序；每种索引方式 ≥ 1024 fuzz 输入差分 vs upstream numpy 2.0.2（int/bool 字节级、float `rtol=1e-7`）；视图与拷贝语义可观测（mutate-through-view + 高级索引返回独立拷贝）；L2.perf gate 继承 M7.1 的"强制"状态 |
| **M7.3 ✅** | 归约（`sum / prod / mean / std / var / min / max / argmin / argmax`，按 ADR-0016）支持 `axis: Option<i64>`；浮点用成对求和（pairwise summation）；std/var 带 `ddof: u32`；空数组语义与 numpy 一致（sum/prod 返回单位元、mean/std/var 返回 NaN、min/max/argmin/argmax 返回 `ReductionEmptyArray`）；1 个新错误变体 | ≥ 50 良类型 + ≥ 50 病类型归约程序；每个归约 ≥ 1024 fuzz 输入差分 vs upstream numpy 2.0.2（int/bool 字节级、float `rtol=1e-7`、argmin/argmax 完全一致）；成对精度测试（10⁶ 个微浮点 `rtol=1e-12`）；L2.perf gate 继承"强制"状态 |
| **M7.5 ✅** | 随机（`Generator` newtype struct 基于 `rand_pcg::Pcg64`；`default_rng / seed / integers / random / normal / uniform / choice`，按 ADR-0018）；4 个新错误变体（`InvalidIntegerRange`, `InvalidDistributionParams`, `InvalidProbabilities`, `EmptyChoicePopulation`）；按 ADR-0012 §"Sequencing rules"与 M7.4 linalg 并行 | ≥ 50 良类型 + ≥ 50 病类型随机程序；Cobrust 内种子可复现性（12 个表驱动测试覆盖 8 种子 × 5 分布）；每分布 ≥ 10000 个样本差分 vs upstream numpy 2.0.2（连续分布 `normal` / `uniform` / `random` 用 KS-test p > 0.01；离散分布 `integers` / `choice` 均值/方差箱在 ±2σ 内）；L2.perf gate 继承"强制"状态 |
| **M7.4** | linalg 子集（`matmul / dot / det / solve / inv / svd / eigh / cholesky`，按 ADR-0017）；绑定 `ndarray-linalg` | 按 ADR-0017——与 M7.5 并行交付 |
| **M7.6+** | 数值层后续：FFT（rustfft）/ polynomial / datetime64 / 结构化数组——开放式 | 各子里程碑独立 ADR；按 ADR-0012 §"Sub-milestones"分阶段推进 |

## 当前状态

**M0..M7.3 + M7.5 已交付。M7.4（linalg）并行落地。** 仓库骨架已就位；词法/语法/AST（M1）、HIR + 双向类型检查器（M2）、provider-agnostic LLM Router（M3）均已上线；**M4** 端到端跑通 L0+L1 翻译流水线（目标 `tomli`），生成的 `cobrust-tomli` crate 已提交以保障 gate 稳定。**M5** 完成闭环合龙：L2.perf 基准压测器、L2.behavior 修复循环（`BehaviorVerifier` 钩子 + 按 attempt 路由的合成提供商）、L3 下游依赖驱动器。第二个翻译库 `python-dateutil`（核心：`parse_iso` + `relativedelta_add`）作为 M5 交付物落地；2/5 依赖（croniter, freezegun）通过 L3 门禁，剩余 3/5（pandas, sqlalchemy, pendulum）按 ADR-0009 显式推迟到 M6。**M6** 是原生扩展里程碑：`cobrust-msgpack` 端到端翻译 msgpack-python 1.0.8（17 个纯 Python + 2 个 Cython 类型化入口），通过 Cython 词法 shim（`task = "translate_cython"`）；`PerfVerifier` 回调让 L2.perf 失败即触发修复，演示一次 `pack_uint` 故意做差的修复路径；dateutil L3 拓宽到 4/5 + 1 跳过（pendulum tz 越界，按 ADR-0010 §5）；`cobrust-dateutil` 与 `cobrust-msgpack` 均启用 `--features pyo3`（按 ADR-0011）。**M7.0** 是 numpy 数值层的第一个子里程碑（按 ADR-0012 §"translate the surface, bind the core"）：新建 `cobrust-numpy` parent crate（按 ADR-0013 决定使用单一父 crate 而非按子 ms 拆分），封装 `ndarray = "0.16"` 提供数据后端；闭合 `Dtype` 枚举（5 个变体）+ tagged-union `Array`（5 个变体，按 ADR-0013 §4 不在公共 API 暴露 `dyn`，符合宪法 §2.2）；四个构造器 `array / zeros / ones / arange` + 观测面 `shape / ndim / size / dtype / repr / to_json`；L0 差分门禁通过子进程跑 upstream numpy 2.0.2 oracle（int/bool 字节级、float `rtol=1e-12`，1024+ 个 fuzz 输入）；`tests/numpy_fuzz.rs` 4200 个 panic-free fuzz 输入；55 个良类型 + 56 个病类型程序通过；`--features pyo3` 构建路径就绪（按 ADR-0011）。测试总数：501（基线 376；M7.0 净增 125）。**M7.1** 落地 numpy ufunc 层（按 ADR-0014）：二元 ops（`add / sub / mul / div / pow`）、比较 ufuncs（一律返回 `Dtype::Bool`）、逐元素数学（`sin / cos / exp / log / sqrt`）、numpy 2.x 广播规则（`broadcast_shape`）、NEP 50 类型晋升（`result_type` 25 条目表）、类型化构造器（`array_i32 / i64 / f32 / f64 / bool`，关闭 ADR-0013 follow-up #2）、nested-list 解析（`NestedList`, `array_from_nested`，关闭 follow-up #4）。三个新错误变体（`IntegerDivisionByZero`, `BroadcastShapeMismatch`, `TypePromotionFailure`）覆盖新失败路径。分发是单态化（内联 match 分支，关闭 follow-up #1；`ndarray::Zip` 内循环自动向量化）。差分门禁针对每个 ufunc 跑 >= 1200 个 fuzz 输入对比 upstream numpy 2.0.2：int/bool 字节级、float `rtol=1e-7`。**L2.perf gate 翻为强制**（关闭 follow-up #3）：`corpus/numpy/M7.1/perf.toml` 按 ADR-0010 §3 设数值层 0.5x floor，`ufunc_pipeline_escalates_when_perf_always_fails` 演示 perf-fail → repair → `EscalationExceeded`，与 M6 的 msgpack escalation 测试同构。**NEP 50 具体例子**：`int32 + float32 → float64`（i32 尾数不能放进 f32），所以 `array_i32(&[1,2,3], &[3]).add(&array_f32(&[0.5,1.5,2.5], &[3]))` 产出 `Float64` 数组 `[1.5, 3.5, 5.5]`，与 numpy 2.0.2 字节级一致。cobrust-numpy 测试总数：223（M7.0 时为 75；M7.1 净增 148）。**M7.2** 落地索引层（按 ADR-0015）：闭合 `Index` enum（5 个变体——`Single`、`Slice(SliceSpec)`、`IntArray`、`BoolMask`、`NewAxis`）、`SliceSpec` 结构、`Array::slice / slice_mut`（基本切片 → 视图）、`Array::take`（整数数组 → 拷贝）、`Array::mask`（布尔掩码 → 拷贝）、`Array::index_get`（顶层多轴分发器）、`np_where(cond, x, y)`（带广播的三元选择）。视图通过 `ArrayView<'a>` / `ArrayViewMut<'a>` 落地——按 dtype 闭合的 enum，所有权由生命周期编码（不引入 `dyn`，符合宪法 §2.2；Rust 借用检查器在编译期保证 mutate-through-view 安全）。`NumpyErrorKind` 新增 4 个变体：`IndexError`（伞型）、`OutOfBoundsIndex`、`BoolMaskShapeMismatch`、`IndexDtypeNotInteger`。**视图 vs 拷贝规则与 numpy 文档约定一致**：`a[1:3]` 返回视图（`Array::slice` → `ArrayView<'a>`；通过 `slice_mut` 修改会传播到父数组）；`a[[0, 2]]` 返回拷贝（`Array::take` → 独立的 `Array`；修改拷贝不影响父数组）；`a[a > 0]` 返回拷贝（`Array::mask`）；`np.where(cond, x, y)` 始终物化为新数组。差分门禁针对每种索引方式（基本切片、单整数、整数数组、布尔掩码、np.where）跑 ≥ 1024 fuzz 输入对比 upstream numpy 2.0.2：int/bool 字节级、float `rtol=1e-7`。L2.perf gate 继承 M7.1 的"强制"状态；`index_pipeline_escalates_when_perf_always_fails` 演示 perf-fail → repair → `EscalationExceeded`。cobrust-numpy 测试总数：**356**（M7.1 时为 223；M7.2 净增 133：55 良类型 + 55 病类型 + 14 视图语义 + 5 流水线 + 4 性能 + 6 差分）。**M7.3** 落地归约层（按 ADR-0016）：九个归约（`sum / prod / mean / std / var / min / max / argmin / argmax`），既以自由函数也以 `Array::*` 方法暴露。轴语义用 `axis: Option<i64>`（None = 全轴；Some(k) = 沿 k 轴；支持负轴）。std/var 携带 `ddof: u32`（默认 0 表示总体；传 1 得到 Bessel 校正的样本）。浮点 `sum / mean / std / var` 用块大小 8 的成对求和——与 numpy 算法一致；`pairwise_sum_f32 / f64` 公开。空数组语义贴合 numpy：`sum([])` = 0、`prod([])` = 1、`mean / std / var ([])` = NaN、`min / max / argmin / argmax ([])` = `Err(ReductionEmptyArray)`。min/max 中 NaN 传播（任意 NaN → NaN）；argmin/argmax 首次出现 tie-breaking、结果 dtype `Int64`（匹配 numpy 的 `intp`）。差分门禁针对每个归约跑 ≥ 1024 fuzz 输入（共 12 个差分测试）对比 upstream numpy 2.0.2：int/bool 字节级、float `rtol=1e-7`、argmin/argmax 完全一致。成对精度测试验证 `pairwise_sum_f64` 对 10⁶ 个微浮点的求和与期望值在 `rtol=1e-12` 内一致 —— 与 numpy 精度下限持平。cobrust-numpy 测试总数：**524**（M7.2 时为 356；M7.3 净增 168：55 良类型 + 51 病类型 + 25 corpus + 12 差分 + 6 性能 + 5 流水线 + 14 单元）。

**M7.5** 落地随机层（按 ADR-0018）——按 ADR-0012 §"Sequencing rules"与 M7.4 linalg 并行交付。cobrust-numpy 现在搭载基于 `rand_pcg::Pcg64`（匹配 numpy `default_rng()` 算法族 PCG64）的闭合 `Generator` newtype struct。落地七个公共方法：`default_rng(seed: Option<u64>)`（自由函数）、`Generator::seed`、`Generator::integers`（[low, high) 区间内的均匀 Int64）、`Generator::random`（[0, 1) 区间内的均匀 Float64）、`Generator::normal`（高斯，通过 `rand_distr::Normal`）、`Generator::uniform`（[low, high) 区间内的 Float64，通过 `rand_distr::Uniform`）、`Generator::choice`（均匀 / 加权 / 不放回 Fisher-Yates；保留输入 dtype）。四个新错误变体：`InvalidIntegerRange`、`InvalidDistributionParams`、`InvalidProbabilities`、`EmptyChoicePopulation`。**种子可复现性契约**（按 ADR-0018 §3）：相同 `u64` 种子 → 在任何主机架构上、Cobrust 内位级一致的流（PCG64 是代数的——状态中无主机字节序）；通过 `tests/random_seed_corpus.rs` 验证，12 个表驱动测试覆盖 integers、random、normal、uniform、choice 放回、choice 不放回、加权 choice 和 re-seed 语义。**与 numpy 2.0.2 的分布一致性**（按 ADR-0018 §5）：连续分布（`normal`, `uniform`, `random`）KS-test 在 p > 0.01；离散分布（`integers`, `choice`）均值/方差箱在 ±2σ 内。每分布每种子 ≥ 10000 个样本（3 个种子：42、1337、0xDEADBEEF）。**与 numpy 不是字节级一致**——numpy 对其 PCG64 后端使用特定的 SeedSequence 布局，我们不复刻该布局；作为已知差异记录在 `PROVENANCE.toml` 中。L2.perf gate 继承 M7.1..M7.3 的"强制"状态；`random_pipeline_escalates_when_perf_always_fails` 演示 perf-fail → repair → `EscalationExceeded`。M7.5 新增三个 Cargo 依赖（`rand = "0.8"`、`rand_pcg = "0.3"`、`rand_distr = "0.4"`——均为 MIT-OR-Apache-2.0）。

**为什么是"翻译表面，绑定内核"**：上游 numpy 的核心是 `numpy/core/src/multiarray/*.c` 的手工 SIMD/BLAS 路径——纯 Rust 重写不切实际。Rust 生态已有 `ndarray` 提供同样的 `(dtype, shape, strides, data)` 模型。M7.0 的工程实践是把 cobrust-numpy 的"表面"（dtype 字符串解析、错误分类、numpy 兼容的 `repr`、Python-shaped 构造器签名）当作翻译目标，把"内核"（`ArrayD::zeros` / `from_shape_vec`）当作绑定目标。**例子**：`cobrust_numpy::zeros(&[3, 4], Dtype::Float64)` 在 cobrust-numpy 这一层做 dtype 路由（`match dtype { Dtype::Float64 => ... }`），最终 `ArrayD::<f64>::zeros(IxDyn(&[3, 4]))` 由 ndarray 实际分配 + 零填充。我们不重写 `zeros`，我们调用它。这条原则贯穿整个 M7+：M7.4 linalg 绑定 `ndarray-linalg`，M7.5 random 绑定 `rand_pcg::Pcg64` + `rand_distr`（按 ADR-0018 已交付），M7.6 FFT 会绑定 `rustfft`。

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
