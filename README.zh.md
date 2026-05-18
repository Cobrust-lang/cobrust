<div align="center">

[English](README.md) · **中文**

# Cobrust

**AI 友好的 Python 继任者,用 Rust 实现,自带 LLM 驱动的翻译流水线和 AI 原生标准库(开发中)。**

*Cobra 🐍 + Rust 🦀 — Python 的人体工学,Rust 的安全性,零迁移成本。*

[![CI](https://github.com/Cobrust-lang/cobrust/actions/workflows/ci.yml/badge.svg)](https://github.com/Cobrust-lang/cobrust/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0%20%2F%20MIT-blue.svg)](#许可证)
[![Stage](https://img.shields.io/badge/stage-0.3.0-orange.svg)](https://github.com/Cobrust-lang/cobrust/releases)

[**为什么是 Cobrust?**](docs/post/why-cobrust.md) ·
[**快速开始**](#快速开始) ·
[**示例**](examples/) ·
[**路线图**](docs/agent/adr/0054-post-phase-g-roadmap.md) ·
[**讨论区**](https://github.com/Cobrust-lang/cobrust/discussions)

</div>

---

## ⚡ 30 秒演示

```bash
# 安装(从源码构建)
$ cargo install --git https://github.com/Cobrust-lang/cobrust cobrust-cli

# 把一个 Python 库翻译成已验证的 Rust
$ cobrust translate tomli
[L0] 从 tomli 2.0.1 提取规范
[L1] 用 codex gpt-5.5 翻译
[L2.build]    cargo build:  0 errors, 0 warnings
[L2.behavior] 1024 输入差异测试:99.71% 严格 PASS
[L2.perf]     1KB 13.8× / 100KB 10.8× / 10MB 9.05× 快于 CPython tomllib(ADR-0039)
[L3] 下游验证:pip-tools 中的 tomli 用法编译 + 测试通过

# 在 Python 里 drop-in 替换
$ pip install ./cobrust-tomli
$ python -c "import tomli; print(tomli.loads('foo=1'))"
{'foo': 1}    # 已经透明地由已验证的 Rust 在背后实现
```

就这样。现有 Python 代码不动,**tomli 上 9-14× 加速**(T1.1 实测对比 CPython 3.11 tomllib,见 ADR-0039),且内存安全。其他库等 Phase F.1 perf 门通过。

---

## Cobrust 是什么

Cobrust 是一门**静态类型**的语言,用 Rust 写成,语法对 Python 用户友好,语义经过净化。三个并发目标:

1. **写起来像 Python**:缩进块、迭代协议、生成器、装饰器、上下文管理器、推导式、模式匹配、f-string。
2. **跑起来像 Rust**:无 GIL、所有权 + 借用、`Result<T, E>` 默认错误路径、穷举模式匹配、Cargo 风格的单工具流水线。
3. **AI 翻译为头等公民**:`cobrust translate <python-lib>` 调度 LLM,把 Python 翻成 Cobrust,在多重门(构建 / 行为差分 / 性能 / 下游依赖)下闭环验证,bit-for-bit 可复现。

设计哲学和取舍详见 [CLAUDE.md](CLAUDE.md)(项目宪法)与 [docs/agent/adr/](docs/agent/adr/)(每一项重大决策)。

---

## 快速开始

### 安装

```bash
# Option A — 用 cargo install(需要 Rust 工具链)
$ cargo install --git https://github.com/Cobrust-lang/cobrust cobrust-cli
# (v0.3.0 已排上 crates.io 发布队列)

# 或下载预编译二进制(tier-1 平台,见 ADR-0046)
# macOS arm64
curl -L https://github.com/Cobrust-lang/cobrust/releases/latest/download/cobrust-v0.3.0-aarch64-apple-darwin.tar.gz | tar xz
sudo mv cobrust /usr/local/bin/
# Linux arm64
curl -L https://github.com/Cobrust-lang/cobrust/releases/latest/download/cobrust-v0.3.0-aarch64-unknown-linux-gnu.tar.gz | tar xz
sudo mv cobrust /usr/local/bin/
# Linux x86_64
curl -L https://github.com/Cobrust-lang/cobrust/releases/latest/download/cobrust-v0.3.0-x86_64-unknown-linux-gnu.tar.gz | tar xz
sudo mv cobrust /usr/local/bin/
```

### Hello world

```bash
$ cobrust new hello && cd hello
$ cat src/main.cb
fn main() -> i64:
    print("hello, world")
    return 0

$ cobrust run src/main.cb
hello, world
```

### 真实算法 — 递归 fib

```bash
$ cat src/main.cb
fn fib(n: i64) -> i64:
    if n < 2:
        return n
    return fib(n - 1) + fib(n - 2)

fn main() -> i64:
    print(f"fib(10) = {fib(10)}")
    return 0

$ cobrust run src/main.cb
fib(10) = 55
```

### 翻译一个 Python 库(招牌功能)

```bash
# 把 tomli 翻成已验证的 Rust + PyO3 wrapper
$ cobrust translate tomli

# 从 Python 透明使用结果
$ pip install ./cobrust-tomli
$ python -c "import tomli; tomli.loads('key = \"value\"')"
{'key': 'value'}
```

翻译流水线每阶段都有门:
- **L0 规范提取** — LLM 读源码 + 测试,产出机器可读规范
- **L1 翻译** — 函数级、自底向上、共识模式(多模型投票)
- **L2 验证** — 构建 + 行为(1000 例差分模糊测试对比 CPython oracle)+ 性能(≥ 0.8× 基线)
- **L3 集成** — PyO3 wrapper + 下游依赖验证(用这个库的库,测试也得通过)

每次翻译都带一份 **provenance manifest** — 源码 SHA、模型指纹、oracle 产物、divergence。bit-for-bit 可复现。

### 试 AI alpha 表面

如果想在读完完整架构文档之前先用上已合并的 AI 标准库 alpha:

- 在 `cobrust.toml` 里至少配一个 provider(见 [`cobrust.toml.example`](cobrust.toml.example))。
- 声明你要用的 route:
  - `[routing.structured]` 用于 `llm_complete_structured(...)`
  - `[routing.tools]` 用于 `llm_complete_with_tools(...)`
  - 任意自定义 `[routing.<task>]` 用于 `llm_dispatch(...)`
- 当前表面是**扁平 prelude 函数**:`llm_complete(...)`、`llm_dispatch(...)`、`llm_stream(...)`、`llm_complete_structured(...)`、`llm_complete_with_tools(...)`。
- 暂不要写 `cobrust.llm.*` / `cobrust.prompt.*` / `cobrust.tool.*` 模块路径语法 — 那是架构 framing,不是当前源码语法。
- Alpha 警示:当 routing 或 provider 配置缺失/失败时,这些 helper 当前返回 `""`(或 `[]` for `llm_stream(...)`),不是富运行时错误。

完整设置路径见 [docs/human/zh/getting-started.md](docs/human/zh/getting-started.md);设计细节见 [docs/human/zh/architecture.md](docs/human/zh/architecture.md)。

---

## LeetCode 快速开始

想用 Cobrust 刷 LeetCode?两步:

1. 装 Cobrust v0.3.0+(见上方 [安装](#安装))
2. 看指南:
   - 中文:[用 Cobrust 刷 LeetCode](docs/human/zh/getting-started-leetcode.md)
   - English: [LeetCode with Cobrust](docs/human/en/getting-started-leetcode.md)

10 个开箱即用的示例在 [`examples/leetcode/`](examples/leetcode/),涵盖:hash-map 模拟、字符串反转、递归/DP、栈式语法分析、归并排序、Kadane 算法、二分、爬楼梯、贪心炒股、罗马数字。

```bash
# 现在就试 Two Sum:
printf "4\n2\n7\n11\n15\n9\n" | cargo run -p cobrust-cli -- run examples/leetcode/two_sum.cb
# 预期输出:
# 0
# 1
```

完整题目目录 + 输入格式:[`examples/leetcode/README.md`](examples/leetcode/README.md)

---

## 当前状态

**0.3.0** — Phase G 全部关闭(四条 §2.5 方向 A/B/C/D 全部落地)。完整 release notes 见 [docs/releases/v0.3.0.md](docs/releases/v0.3.0.md)。

- ✅ **编译器核心** — lexer / parser / HIR / 类型检查 / MIR / Cranelift codegen;`-D warnings` 下零 clippy 警告。
- ✅ **Phase F.3 语言完整性**(v0.2.0) — `break` / `continue`、`for` 循环、`list[str]`、`f64`(完整 IEEE-754 + f-string `{:.Nf}`)、`dict[K, V]`(insertion-ordered,见 [ADR-0050d](docs/agent/adr/0050d-dict-design.md))、字符串标准库(split/join/replace/trim/find/contains/...)、文件 IO(read/write/append、stdin/stdout/stderr)。
- ✅ **Phase G LLM-first 表面**(v0.3.0,四条方向全部关闭):
  - **A — 显式 `&s` 借用** — 消除 `clone()` 杂讯;单向 call-site coercion,见 [ADR-0052a](docs/agent/adr/0052a-explicit-borrow-let-rebind.md) + [ADR-0052f](docs/agent/adr/0052f-borrow-of-call-relaxation.md) + [ADR-0052g](docs/agent/adr/0052g-borrow-of-call-result-type-check.md)。`&s.method()` 解析路径已解锁。
  - **B — 错误打印修复方法** — 共 41 个变体(24 `TypeError` + 11 `MirError` + 6 `LoweringError`)携带结构化 `suggestion: Option<&'static str>`;LSP `Diagnostic.relatedInformation` forward-compat,见 [ADR-0052b](docs/agent/adr/0052b-error-ux-fix-suggestions.md)。
  - **C — `@py_compat` tier 绑定 L2 verifier** — `Strict` / `Semantic` / `Numerical{rtol}` / `None` enum + `TierVerifier`;[ADR-0037](docs/agent/adr/0037-py-compat-hard-bind.md) 经由 [ADR-0052c](docs/agent/adr/0052c-py-compat-tier-l2-bind.md) 激活。
  - **D — 方法调用糖基础设施** — 4 种类型新增 25 个方法表项(Str×10 + List×5 + Float×5 + Int×5),见 [ADR-0052d-prereq](docs/agent/adr/0052d-prereq-method-dispatch-infra.md);完整 LC-100 corpus 迁移延至 v0.3.1(ADR-0052d-final)。
- ✅ **CLI tempdir RAII** — 关闭 Mac/DG `/tmp/cobrust-*` 泄漏(235G 临时文件泄漏事故根本原因);`tempfile::TempDir` RAII 保证 panic / 取消 / 信号时也能清理。
- ✅ **双语 README** — `README.zh.md` 与 `README.md` 完全并行,符合 CLAUDE.md §3 双轨文档规范。
- ✅ **标准库** — io / collections / string / math / panic / env / fmt / iter + structured concurrency runtime(M13)。AI-facing alpha:`cobrust.llm` / `.prompt` / `.tool` 扁平 prelude fn(见 [ADR-0049](docs/agent/adr/0049-alpha-honesty-and-onboarding-hardening.md) honesty hardening)。
- ✅ **包格式** — `cobrust.toml`、content-addressed registry、deterministic lockfile。
- ✅ **AI 翻译流水线** — 在 stateless + stateful tomli 函数上生产级验证通过(真实 LLM,12/12 + 14/14 严格确定性 5 次跑)。dateutil / msgpack:部分。
- 🚧 **工具链** — REPL 仍是 M14 stub(Phase I REPL JIT 已 scoping,约 1 周 wall,见 [ADR-0054](docs/agent/adr/0054-post-phase-g-roadmap.md))。无 LSP(Phase J 约 2-3 周,**最高 §2.5 ROI** — 把 ADR-0052b 结构化 suggestion 接进 IDE agents)。无 debugger(Phase L)。无 WASM target。
- 🚧 **LLVM backend** — Phase K(3-4 周);当前 release build 走 Cranelift。
- 🚧 **自托管** — 0%。Phase H scoping spike 已完成;dispatched 后约 2.5 周(ADR-0054 实证修正,原估 3 周)。[^phase-h-wall]

[^phase-h-wall]: Phase H wall-time 主要由 `check.rs`（约 1-2 周）主导;参见 [ADR-0055d §10.2](docs/agent/adr/0055d-check-rs-cb-port.md)——项目历史上最大单子冲刺披露。

**这意味着什么**:Cobrust 在语言核心 + AI 翻译流水线层面**机制已验证**。**Phase G LLM-friendliness 在 v0.3.0 全部关闭**,四条 §2.5 binding direction 均已落地(A 显式借用 ✅ / B 错误 UX ✅ / C @py_compat L2 ✅ / D 方法调用糖 ✅ 基础设施已 ship,corpus 迁移在 v0.3.1)。

**§2.5 宪法支柱**([CLAUDE.md §2.5](CLAUDE.md) + [ADR-0051](docs/agent/adr/0051-llm-first-design-principle.md)):"Cobrust 不是为人类写得最爽的语言,是为 LLM 一次写对的语言。" Agent 入门 skill:[`docs/agent/skills/cobrust-first-try.md`](docs/agent/skills/cobrust-first-try.md)。

下一步看 [post-Phase-G 路线图(ADR-0054)](docs/agent/adr/0054-post-phase-g-roadmap.md)。

---

## 示例

[`examples/`](examples/) 里渐进的示例:

| | |
|---|---|
| `examples/hello.cb` | 最小 hello world |
| `examples/fizzbuzz.cb` | 控制流(真 `if/elif/else` + `%`) |
| `examples/fib.cb` | 通过 `Constant::FnRef` Call lowering 实现递归 |
| `examples/wc.cb` | 文件 IO + 迭代 |
| `examples/cat.cb` | 文件流到 stdout |
| `examples/echo.cb` | argv echo |
| `examples/sort.cb` | 从 stdin 排序行 |
| `examples/unique_lines.cb` | 去重 |
| `examples/regex_grep.cb` | 在 stdin 上做 regex 过滤 |
| `examples/csv_sum.cb` | 聚合一个 CSV 列 |
| `examples/json_pretty.cb` | 美化 JSON |
| `examples/notebook/` | 多模块包 |
| `examples/notebook-config/` | 兄弟包(path dependency) |

---

## 架构(一段话)

前端(lexer → parser → AST → unparse 往返)是递归下降 + Pratt parser,纯 Rust。AST → HIR(去糖、名字解析)→ typed-HIR(双向类型检查,无 `dyn`,无隐式 truthy,穷举 match)→ MIR(控制流显式、drop schedule、borrow-check obligation 出清)→ Codegen(dev 用 Cranelift / `--release` 用 LLVM stub)→ linker(系统 `cc` 或 `lld`)。

AI 翻译子系统是**编译器一等组件**,不是插件。消费 Python 源码 + 测试,通过 LLM router(provider-agnostic — Anthropic、OpenAI-compatible、local vLLM 都直接 work)派遣,产出 Cobrust 源码再进入主流水线。每个门都强制;失败路由回 repair。

完整图见 [docs/human/zh/architecture.md](docs/human/zh/architecture.md)。

---

## 路线图

**Phase E — DONE**(M0..M14):语言核心、codegen、包格式、REPL stub。

**Phase F — DONE**(v0.1.x → v0.2.0):翻译流水线生产级验证(tomli 5/5 + dateutil 5/5 real-LLM);AI 原生标准库 alpha(`cobrust.llm` / `.prompt` / `.tool`);Phase F.3 语言完整性(break/continue, for, list[str], f64, dict, 字符串标准库,文件 IO)。

**Phase G — DONE**(v0.2.0 → v0.3.0):四条 §2.5 LLM-first binding direction — 全部落地
- ✅ A — 显式 `&s` 借用(ADR-0052a/f/g;LC-100 honest-debt 实证基线下最大的 LLM-friendliness deficit)
- ✅ B — 错误打印修复方法(共 41 个变体;结构化 `suggestion` 字段;LSP forward-compat)
- ✅ C — `@py_compat` tier 绑定 L2 verifier(ADR-0037 reserved → 经 ADR-0052c 激活)
- ✅ D — 方法调用糖基础设施(4 种类型新增 25 个方法表项;完整 LC-100 corpus 迁移延至 v0.3.1)

**Post-Phase-G 路线图**([ADR-0054](docs/agent/adr/0054-post-phase-g-roadmap.md),agent-velocity 共 ~10-12 周):

| Phase | 内容 | 时长 | §2.5 ROI |
|---|---|---|---|
| **H** | 自托管类型检查器(宪法 §4.4) | ~2.5 周 | 中 |
| **I** | REPL JIT(M14.1;复用 M11.2 FnRef Call lowering) | ~1 周 | 中 |
| **J** | **LSP server**(Cursor/Continue/Cody/Aider/VSCode 集成) | ~2-3 周 | **最高** |
| **K** | LLVM Backend(release 性能 + 跨平台 + DWARF) | ~3-4 周 | 中性 |
| **L** | Debugger(基于 K 的 DWARF + 断点 runtime + REPL 集成) | ~1 周 | 低 |

§2.5 ROI 重排理由:J 最高 — 因为编辑器内 LLM agents(Cursor / Continue / Cody)直接读 LSP 诊断 + suggestion + code-action,ADR-0052b 的结构化 `suggestion` 字段正是 Phase J 接进 `Diagnostic.relatedInformation` + `CodeAction.title` 的载荷。

完整 Phase-by-Phase 子 ADR 名册 + compression-ratio 实证依据:[ADR-0054](docs/agent/adr/0054-post-phase-g-roadmap.md)。

---

## 贡献

我们需要:
- 更多翻译过的库(看 `good-first-issue` label 找入门目标)
- LSP 工作(大头,基础性)
- 跨架构验证(windows-x86_64、linux-aarch64)
- AI router adapter(更多模型后端)

见 [CONTRIBUTING.md](CONTRIBUTING.md)。行为准则:[Contributor Covenant](CODE_OF_CONDUCT.md)。

加入:[GitHub Discussions](https://github.com/Cobrust-lang/cobrust/discussions) 讨论设计;[Issues](https://github.com/Cobrust-lang/cobrust/issues) 报 bug 或提需求。

---

## 许可证

双许可,以下任意一项:
- Apache License, Version 2.0([LICENSE-APACHE](LICENSE-APACHE))
- MIT 许可证([LICENSE-MIT](LICENSE-MIT))

按你的选择。理由见 [ADR-0001](docs/agent/adr/0001-license.md)。

---

## 致谢

Cobrust 站在以下肩膀上:
- **Cranelift** — codegen IR 和后端,纯 Rust
- **Mojo / Pyston / Cinder / Cython** — 更早的 Python 性能项目,我们从中学到的教训
- **PyO3** — 翻译输出里附带的 Rust↔Python FFI 绑定
- **Anthropic / OpenAI / DeepSeek** — 给翻译流水线供电的 LLM 提供商
- **Rust** 社区 — 安全 + 性能,让 Cobrust 成为可能

---

<div align="center">

**Cobrust 0.3.0** — 公开构建,由 AI agents 和人类协作完成。
*用过的话,告诉我们哪里坏了。*

</div>
