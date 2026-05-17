# 设计哲学

> 核心张力：**保留 Python 的人体工学，去除 Python 的历史包袱，引入 Rust 的安全与性能，让 AI 翻译子系统作为编译器一等公民填平迁移成本。**

## 从 Python 保留

| 特性 | 为什么保留 |
|------|----------|
| 缩进式块 | 视觉清晰、仪式感低 |
| REPL 优先 | 反馈循环紧凑 |
| 迭代器协议、生成器 | 组合性 |
| 装饰器 | 组合的核心原语 |
| `with` 上下文管理器 | 资源纪律 |
| 推导式（comprehension） | 在有界场景下表达力强 |
| 结构化模式匹配 | Python 3.10+ 已经做对了 |
| f-strings | 全语言里最好的字符串格式化 |

## 从 Python 抛弃（不可妥协）

- **GIL** → 基于所有权的并发，无全局锁
- **默认动态类型** → 默认静态结构化类型；`dyn` 是显式选项，永远不是默认
- **可变默认参数** → 编译错误
- **闭包延迟绑定** → 显式 `copy` / `ref` / `move` 捕获
- **`__init__.py` / `sys.path` / 包管理混乱** → 单一规范包格式，内容寻址，单一工具
- **跨模块猴子补丁** → 禁止
- **隐式强制转换**（`"1" + 1`、`0 == False`、任意类型的真值性） → 类型错误
- **`is` vs `==` 混淆** → `is` 直接移除；如果你真的需要身份判断用 `same_object(a, b)`
- **异常作为默认错误路径** → `Result<T, E>` 是默认；异常仅保留给真正不可恢复的场景
- **async / sync 函数染色** → 单一结构化并发 runtime，没有"两色函数"问题
- **多继承 + MRO** → 组合 + Trait
- **Metaclass 作为逃生舱** → 编译期宏 + 反射
- **隐式真假性** → `if x` 要求 `x: bool`；否则用 `if x.is_some()`、`if !v.is_empty()` 等

## 从 Rust 引入

所有权、借用、Trait、`Result<T, E>` / `Option<T>`、穷尽模式匹配、Cargo 风格的单工具工作流。

## Cobrust 原创

- **`@py_compat` 标签**：标准库每一项标注 Python 兼容等级
  - `strict` — 行为逐字节一致
  - `numerical(rtol=1e-7)` — 数值容差内一致
  - `semantic` — 语义等价但表达可能不同
  - `none` — 显式不兼容（带原因）
- **翻译来源（provenance）**：每个翻译模块携带清单（来源库、版本、oracle 工件、验证种子、已知偏差）。**永远不允许静默翻译。**
- **确定性构建 ID**：源码 + 工具链 + LLM 路由决策的哈希，相同输入逐字节可重现

## 翻译层级系统（ADR-0052c）

`@py_compat` 标签不只是文档——它是 L2 行为验证器从头到尾强制执行的类型化契约。
ADR-0052c（2026-05-17）让该层级在每一层都是类型化枚举：

- **Spec 层**：`corpus/<lib>/spec.toml` 声明 `py_compat = "strict"`
  （或 `"semantic"`、`"numerical(rtol=1e-7)"`、`"none"`）。书写错误
  （例如 `"strikt"`）在 spec 加载时直接拒绝，错误信息包含拼错的标识符
  与所有合法选项——这是 `§2.5 编译期捕获` 规则在 spec 数据上的应用。
- **Verifier 层**：`TierVerifier` 按层级分发判定策略：
  - `Strict` → 逐字节恒等检查；任意偏差都拒绝。
  - `Semantic` → 结构等价（字典键序、空白、JSON 形状先归一化再比较）。
  - `Numerical { rtol }` → `numpy.testing.assert_allclose(rtol=...)`
    f64 语义。
  - `None` → 关闭闸门；按 ADR-0040 诚实记为 `Skip`。
- **Router 层**：按层级覆盖路由（`[routing.translate_<tier>]` 块）。
  `Strict` 走共识路径（默认 n=2）；`Numerical` 走 Cost 路径
  （单模型即可，因为 rtol 已吸收输出抖动）；`Semantic` 落回全局
  Quality 默认。
- **Prompt 层**：每个层级在 L1 翻译 prompt 中插入层级专属指令块，明确告
  诉 LLM 当前输出必须满足什么契约（"output MUST be bit-identical" /
  "output MUST satisfy assert_allclose(rtol=...)" /
  "output MUST match structurally"）。

M7+ numpy 语料的旁路形式（`py_compat = "numerical"` + 同级
`py_compat_rtol = 1e-7`）保持向后兼容；裸 `"numerical"` 无旁路时
默认 `rtol = 1e-7`。

## 设计的"为什么"

每一项决定背后都有一个真实代价。例子：

- 移除 `is`,因为它制造了大量初学者陷阱(`a is b` 在小整数缓存范围内意外为真),且 99% 的合法用途可以由 `==` 或显式 `same_object(a, b)` 替代
- 移除 async / sync 染色,因为它把生态切成两半,每个库要写两遍——结构化并发是更好的抽象,单一 runtime 让你不再被迫染色

## 为何用 `&s` 而非 `clone(s)`(ADR-0052a Direction A binding)

宪章 §2.5 把设计绑定到"LLM agent 第一次就能写对的语言"上。LC-100
多次读取场景 —— 把同一个 Str 读两次(例如 `let n = str_len(s);
let c = str_at(s, 0)`)—— 是当前 LLM-friendliness 最大的缺口:

- 编译器今天(ADR-0050c 之后)会用 `UseAfterMove` 拒绝第二次读取,
  这是一个真正的 §2.5 编译期信号。
- Phase F.3 M-F.3.5 引入了 `clone(s)` PRELUDE 内建作为 mitigation。
  那条 fix 路径让 LLM 学到的是错误信号:"用 `clone(s)` 包住第二次
  读取",而 `clone(s)` 会堆分配、让源码膨胀、且不是 Rust 风格的
  canonical 答案。
- 正确的信号是 **`&s`**:零代价共享借用,与 LLM 的 Rust priors
  对齐(`&str` 是训练语料中最高频的 token 之一)。根据 CLAUDE.md
  §2.5 Direction A binding,`&s` 是 LC-100 honest-debt 的
  §2.5-honest 收口路径。

ADR-0052a Wave-1 以一元前缀表达式形式提供 `&s`。类型检查器通过
**单向 call-site 强制转换** 接受 `str_len(&s)` 和 `str_at(&s, i)`
—— 局部、单向(只允许 `&Str → Str`)、仅作用于 call-arg binding。
`clone(s)` 在显式拷贝场景下仍然可用,但已不再是 stderr 推荐的
canonical fix 路径;新的诊断提示是 "use `&s` to borrow without
consuming"。

考虑过但被拒绝的替代符号(ADR-0052a §2):
- `borrow(s)` PRELUDE 形式:LLM 训练数据 overlap 更低、更长。
- 隐式借用推断:违反 §2.5 "compile-time-catch-errors" 规则 ——
  LLM 无法从 stderr 解码推断错失。
- `ref s` 关键字(Rust pattern position):与 Cobrust 的 let
  重绑定语法冲突。

## 方法形式作为 PRELUDE-fn 糖(ADR-0052d-prereq Direction D 绑定)

ADR-0052d-prereq 引入按类型的方法调用形式(`s.split(",")`),
在类型检查时重写为规范的 PRELUDE-fn 形式(`split(s, ",")`)。
该决策遵循 §2.5 "training-data-overlap" 规则:Python
(`str.split`) 和 Rust (`str::split`) 都使用方法调用语法表面;
"接收者.方法" 形状是 LLM 训练分布中最常见的人体工学。

关键设计属性:

- **静态解析**:方法形式在类型检查时被重写为以接收者作为第一
  参数的 PRELUDE-fn 调用。无 vtable,无动态调度,零运行时开销。
  方法形式与 PRELUDE-fn 形式产生完全相同的机器码(详见
  [ADR-0052d-prereq](../../agent/adr/0052d-prereq-method-dispatch-infra.md)
  §"Surface — method-table contents per type")。
- **PRELUDE-fn 形式仍为规范**:每个方法形式都有用户可直接书写的
  PRELUDE-fn 别名。方法形式是纯粹的糖 —— 不存在不能用 PRELUDE-fn
  调用表达的方法形式。
- **§2.5 "compile-time-catch" 锐化**:像 `s.splittt(",")` 这样的
  拼写错误在编译期通过 `TypeError::UnknownMethod` 暴露并附带
  "did you mean 'split'?" 提示。LLM 的编译错误反馈环现在解码为
  "我调用了 str 上不存在的方法;提示列出了可用方法" —— 比之前
  的行为(`Attr` callee 上无声的 fresh-var 推断)是更强的信号。

当前方法表覆盖(25 个方法):`str` (10)、`list[T]` (5)、`f64` (5)、
`i64` (5)。Dict 方法在独立的 ADR-0050d 子冲刺中落地。用户声明
类型的方法形式在 M11 之后(基于 trait 的调度)。

## 进一步阅读

- [架构](architecture.md)
- [里程碑](milestones.md)
- 项目宪章 `CLAUDE.md`（仓库根目录）
