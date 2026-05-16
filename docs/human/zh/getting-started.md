# 入门 — 30 秒安装

## 第一步：安装 Cobrust

**方式 A — cargo install**（需要 Rust 工具链）：

```bash
cargo install --git https://github.com/Cobrust-lang/cobrust cobrust-cli
# （crates.io 发布计划在 v0.2.0）
```

**方式 B — 预编译二进制**（无需 Rust）：

```bash
# macOS arm64
curl -L https://github.com/Cobrust-lang/cobrust/releases/latest/download/cobrust-v0.1.2-aarch64-apple-darwin.tar.gz | tar xz
sudo mv cobrust /usr/local/bin/

# Linux x86_64
curl -L https://github.com/Cobrust-lang/cobrust/releases/latest/download/cobrust-v0.1.2-x86_64-unknown-linux-gnu.tar.gz | tar xz
sudo mv cobrust /usr/local/bin/
```

验证：`cobrust --version` → `cobrust 0.1.2`

## 第二步：Hello, world

```bash
cobrust new hello && cd hello && cobrust run src/main.cb
```

预期输出：

```
hello, world
```

## 第 2.5 步：for 循环（M-F.3.1）

Cobrust 提供 Python 风格的 `for ... in ...` 循环，可以遍历 `list[T]`，并通过
prelude 的 `range(start, stop)` 辅助函数生成整数序列。依据 ADR-0050b，
`range(start, stop)` 会物化一个包含 `start, start+1, ..., stop-1` 的
`list[i64]`；空区间（`start >= stop`）会跳过循环体。

```cobrust
fn main() -> i64:
    # 正向区间：依次打印 0 1 2 3 4
    for i in range(0, 5):
        print_int(i)

    # 空区间：循环体不会执行
    for i in range(0, 0):
        print_int(-1)

    # 遍历 list
    let xs: list[i64] = list_new(3)
    let _0 = list_set(xs, 0, 10)
    let _1 = list_set(xs, 1, 20)
    let _2 = list_set(xs, 2, 30)
    for v in xs:
        print_int(v)        # 10  20  30

    # 遍历 argv（list[str]）
    for arg in argv():
        print(arg)

    return 0
```

Phase F.3 提供两参数形式 `range(start, stop)`。三参数 `range(start, stop, step)`
延后至 Phase G，与完整迭代器协议一起落地。字符串遍历
（`for c in "hello":`）同样属于 Phase G 工作 —— 详见 ADR-0050b §"Iter source type checking"。

循环语义：
- 循环变量每次迭代都是全新绑定；在循环体内创建的闭包，在第 N 次迭代时
  捕获的是第 N 次迭代的值（宪法 §2.2 —— 拒绝 Python 的延迟绑定）。
- 允许 `for` 嵌套；变量遮蔽遵循 Rust 规则。
- 非 `list[T]` 的迭代源（例如 `for x in 42:`）在类型检查阶段就会被拒绝
  （`TypeError::NotIterable`）。

可运行示例见 [examples/for_range.cb](../../../examples/for_range.cb)
与 [examples/for_list.cb](../../../examples/for_list.cb)。

## 第 2.6 步：f64 与 `as` 类型转换（M-F.3.3）

Cobrust 支持一等公民 `f64`（IEEE-754 双精度浮点数）。
`i64` 与 `f64` 之间必须使用显式 `as` 转换——不允许隐式类型提升（宪法 §2.2）。

```cobrust
fn main() -> i64:
    # 浮点字面量
    let x: f64 = 3.14
    let y: f64 = 1e-3
    let big: f64 = inf      # IEEE 754 正无穷
    let nothing: f64 = nan  # IEEE 754 NaN

    # 显式 as 转换：i64 → f64 和 f64 → i64
    let n: i64 = 42
    let f: f64 = (n as f64)        # 42.0
    let back: i64 = (3.9 as i64)   # 3（向零截断）

    # 数学内建函数（全部返回 f64）
    let s: f64 = sqrt(4.0)         # 2.0
    let p: f64 = pow(2.0, 10.0)    # 1024.0
    let fl: f64 = floor(3.7)        # 3.0

    # f-string 浮点格式化
    print(f"{x:.2f}")               # "3.14"

    return 0
```

核心规则：
- `i64 → f64` 必须写 `(n as f64)`，不支持隐式提升。
- `f64 → i64` 向零截断（C 语义，不是 floor）。
- 浮点除以零不是陷阱——IEEE 754 定义为 ±inf。
- `nan != nan` 为 `true`（IEEE 754 语义）。
- 数学函数：`sqrt`、`floor`、`ceil`、`round`、`abs`、`pow`、`sin`、`cos`、`tan`、`log`、`exp`。
- f-string 格式说明符：`{x:.2f}`（定点）、`{x:e}`（科学计数）、`{x:g}`（通用）。

## 第 2.7 步：list[str] 与 Str 所有权（M-F.3.2）

Cobrust 现在端到端支持 `list[str]`,且遵循 ADR-0050c 规定的 Rust 风格
所有权调度。根据宪法 §2.3,每个 Str 都是拥有式值(`list[str]` 的槽位
拥有其元素);编译器在作用域退出时自动 drop,镜像 Rust 的 `String`。

```cobrust
fn main() -> i64:
    # 字面量 list[str] —— 每个元素都在堆上分配。
    let xs: list[str] = ["alpha", "beta", "gamma"]
    for s in xs:
        print(s)                       # alpha beta gamma
    # xs 在此 drop:每个 Str 槽位释放,然后释放列表容器。

    # `list_is_empty` 是 §2.2 强制的空判定函数
    # (`if xs:` 作为隐式真值判断会被拒绝)。
    let empty: list[str] = []
    if list_is_empty(empty):
        print("empty branch")
    else:
        print("non-empty branch")     # 不会执行

    return 0
```

M-F.3.2 的变化:
- 在 MIR drop pass 中,`Ty::Str` 和 `Ty::List(_)` 不再是 Copy
  (ADR-0050c §"Phase 1")。Codegen 在每个可达的作用域退出处
  emit `__cobrust_str_drop`(针对 Str 槽位)和
  `__cobrust_list_drop_elems`(针对 `list[str]`)。
- `list_len` / `list_get` / `list_set` / `list_new` / `list_is_empty`
  这些内置函数是行多态的 —— 它们接受任意元素类型的 `list[T]`,
  而不仅仅是 `list[i64]`。
- `for s in xs:` 遍历 `list[str]` 时,每个槽位都会克隆到循环变量
  (`__cobrust_str_clone`),因此循环变量拥有自己的副本;槽位的所有权
  仍然属于 `xs`。

编译期拒绝(根据 ADR-0050c "Decision"):
- 对 Str 类型局部变量的 use-after-move(`let a = s; let b = s` 需要
  显式克隆 —— Phase G 会引入 `clone(s)` 内建函数)。

已知 honest-debt(根据 Phase 2a walk-back):
- `list[T]` 在 operand 层面仍然是 Copy,所以将 `list[str]` 按值传给
  某个函数后,该 list 的后续使用并不会被拒绝;双重使用今天是被允许的,
  Phase G 引入显式借用语法时会关闭这个缝隙。

端到端 corpus 见 `crates/cobrust-cli/tests/list_str_e2e.rs`,
C-ABI 链接期测试见 `crates/cobrust-stdlib/tests/list_str_drop_corpus.rs`。

## 第 2.8 步：字符串标准库（M-F.3.5）

十一个 PRELUDE 函数让 Cobrust 可以应付日常字符串处理 —— 日志解析、
CSV 切片、简单文本变换(参见
[ADR-0050e](../../agent/adr/0050e-string-stdlib-m-f-3-5.md))。

surface:

- `split(s: str, sep: str) -> list[str]`
- `join(parts: list[str], sep: str) -> str`
- `replace(s: str, old: str, new: str) -> str`
- `trim(s: str) -> str`(两侧空白)
- `find(s: str, needle: str) -> i64`(找不到返回 `-1`)
- `contains(s: str, needle: str) -> bool`
- `starts_with(s: str, prefix: str) -> bool`
- `ends_with(s: str, suffix: str) -> bool`
- `lower(s: str) -> str` / `upper(s: str) -> str`
- `clone(s: str) -> str`(深拷贝;LC-100 honest-debt 缓解手段)

示例(`hello_csv.cb`):

```cobrust
fn main() -> i64:
    let line: str = "alpha,beta,gamma"
    let parts: list[str] = split(line, ",")
    for p in parts:
        let _ = print(upper(p))
    return 0
```

```bash
cobrust run hello_csv.cb
# ALPHA
# BETA
# GAMMA
```

`find` 返回 `i64`,用 `-1` 做哨兵(Decision 5 / Q2)。文档强制的用法
是 `if pos != -1:`,而 **不是** `if find(...):` —— Cobrust 不允许
隐式真假(§2.2):

```cobrust
let pos: i64 = find("hello world", "world")
if pos != -1:
    print_int(pos)
else:
    let _ = print("not found")
```

`clone(s)` 是 LC-100 honest-debt 的缓解手段。因为 ADR-0050c 让所有
Str 参数都是 Move 语义,所以 `let n = str_len(s); let c = str_at(s, 0)`
这种多次读取会被编译器以 use-after-move 拒绝。解决办法是 inline 调用
`clone()`,让每次调用拿到一份新 buffer:

```cobrust
let s: str = input("")
let n: i64 = str_len(clone(s))      # 拿一份新 s 给 str_len
let i: i64 = n - 1
while i >= 0:
    let c: str = str_at(clone(s), i)  # 每次循环再 clone 一份
    let _ = print(c)
    i = i - 1
let _ = print(upper(s))              # 最后一次使用,不需要 clone
return 0
```

M-F.3.5 的变化:
- `crates/cobrust-cli/src/build.rs` 的 PRELUDE 加了 11 个新 stub;
  `intrinsics.rs` 加了 11 条 intrinsic-rewrite 路径,把每一处调用
  改写成 C-ABI shim `__cobrust_str_<fn>`。
- `crates/cobrust-stdlib/src/string.rs` 提供 10 个新 C-ABI shim
  (`__cobrust_str_clone` 在 ADR-0050c 已经随 fmt.rs 一并落地)。
- Rust 侧 `string::strip` 按 Decision 4 改名为 `string::trim`。

边界用例(ADR-0050e Decision 8):
- `split("", ",") -> [""]`(单元素)
- `split(s, "") -> [s]`(参考 Rust `str::split` 的语义)
- `join([], sep) -> ""`
- `replace(s, "", new)` 在每个字节位置都插入 `new`
- `find(s, "") -> 0`
- `contains(s, "") -> true`(空子串总是命中)

端到端 corpus 见 `crates/cobrust-cli/tests/string_stdlib_e2e.rs`,
C-ABI shim 定义见 `crates/cobrust-stdlib/src/string.rs`。

## 第 2.9 步：dict（M-F.3.4）

Cobrust 的 dict 镜像 Python 的心智模型：`{}` 是 dict（不是 set）、
插入顺序迭代（Python 3.7+ 的承诺）、`d[k]` 在键缺失时 panic、
`.get(k, default)` 是安全转义惯用法。Phase F.3 的表面由 ADR-0050d
锁定;子冲刺 a+b（本里程碑）落地 parser + 类型检查器 + dict_is_empty
内建；子冲刺 c/d/e 接入 codegen + indexmap 后端 + 迭代去糖。

```cobrust
fn main() -> i64:
    # 字面量：空 {} 是 dict，不是 set。
    let empty: Dict[str, i64] = {}
    let scores: Dict[str, i64] = {"alice": 90, "bob": 85, "carol": 92}

    # 下标读取 —— 键缺失时 panic。
    let a: i64 = scores["alice"]                   # 90

    # 下标写入 —— 重绑或插入。
    scores["dave"] = 78

    # 成员判定 —— `in` 返回 bool;`not in` 的规范替代写法见下。
    if "alice" in scores:
        print("found alice")
    if not ("zoey" in scores):
        print("zoey absent")

    # dict_is_empty（规范的空判定 —— §2.2 拒绝 `if d:`）。
    if dict_is_empty(empty):
        print("empty is empty")

    # 方法内建表面（类型检查器已识别;codegen 由子冲刺 d/e 落地,
    # 见 ADR-0050d）：
    let ks: List[str] = scores.keys()              # 插入顺序
    let vs: List[i64] = scores.values()
    let kvs: List[Tuple[str, i64]] = scores.items()
    let v: i64 = scores.get("alice")               # 90
    let safe: i64 = scores.get("missing", 0)       # 0（哨兵对回退,scope cap)
    let copy: Dict[str, i64] = scores.copy()       # 浅克隆

    # 推导式。
    let xs: List[i64] = [1, 2, 3]
    let squares: Dict[i64, i64] = {x: (x * x) for x in xs}

    return 0
```

核心规则（M-F.3.4 / ADR-0050d）：
- `{}` 是空 dict（匹配 Python;空 set 字面量需要 `set()` ctor —— Phase G）。
- `d[k]` 在键缺失时 panic + abort（匹配 Python 的 `KeyError` 但走
  Rust 的 abort 路径 —— 见 `__cobrust_dict_keyerror_abort`）。
  用 `d.get(k, default)` 做安全转义（Phase F.3 不带 Option lowering
  —— Phase F.3-late 或 Phase G 接入类型化 Option）。
- `key in d` 返回 `bool`（Decision 4A）。负向成员判定的规范惯用法
  是 `not (k in d)` —— `BinOp::NotIn` 的 Pratt loop 簿记是 Phase G
  的后续工作。
- `len(d)` 返回 `i64`（Decision 5A —— 与 list / str 保持统一）。
- `dict_is_empty(d)` 是 `bool` 谓词,符合宪法 §2.2 隐式真值禁令
  （拒绝 `if d:`）。
- 迭代按插入顺序（Decision 6A —— 子冲刺 d 之后由
  `indexmap::IndexMap` 提供）。
- 类型参数：`K ∈ {i64, str}`（Phase F.3）;在类型检查阶段拒绝
  `f64` 键（NaN != NaN 破坏 Hash 不变式 —— 见 `TypeError::NotHashable`）。
- `d.copy()` 是浅克隆（Decision 10A）。
- `{**other}` dict 展开是 Phase G —— Phase F.3 在
  `TypeError::DictSpreadNotSupported` 处拒绝。

编译期拒绝（M-F.3.4）：
- `Dict[f64, V]` 与 `Dict[List[T], V]`（非 hashable 的 K）——
  见 `TypeError::NotHashable` 分类。
- `let d = {}`（无注解,也没有后续用法将 K/V 锁定）→
  最终解析过程触发 `TypeError::AmbiguousType`。请显式注解。
- `if d:`（隐式真值）—— 用 `dict_is_empty(d)` 或 `len(d) > 0`。
- `def f(d: Dict[K, V] = {})`（可变默认）—— 同
  `list = []` 规则（ADR-0006）。
- `{"a": 1, **other}`（dict 字面量中的 spread）—— dict-merge 是
  Phase G 范畴。

端到端 corpus 见 `crates/cobrust-cli/tests/dict_e2e.rs`
（许多在子冲刺 c/d codegen 关闭前为 ignored）;类型检查表面见
`crates/cobrust-types/tests/well_typed.rs` 中 w116..w145 的 dict 段。

## 第 2.10 步：文件 IO（M-F.3.6）

Cobrust 现在提供 7 个源码级平铺函数用于文件与 stdio IO
（[ADR-0050f](../../agent/adr/0050f-file-io-completion-m-f-3-6.md)）。

```cobrust
fn main() -> i64:
    # 写文件；成功返回 0（i64-sentinel Q1）。
    let rc: i64 = write_file("/tmp/hello.txt", "hello, cobrust\n")
    if rc != 0:
        return rc

    # 读取整个文件为 str。
    let contents: str = read_file("/tmp/hello.txt")
    let _ = print(contents)           # 输出：hello, cobrust

    # 按行读取 —— 每行去除 \n / \r\n（Q2 决议）。
    let lines: list[str] = read_file_lines("/tmp/hello.txt")
    let n: i64 = list_len(lines)
    print_int(n)                      # 输出：2（保留尾空行）

    # 追加写入；文件不存在时自动创建（Q3）。
    let rc2: i64 = append_file("/tmp/hello.txt", "more text")

    # 读取 stdin 至 EOF。
    let stdin_data: str = stdin_read_all()

    # 向 stdout 写入，不追加换行（与 print 不同）。
    let rc3: i64 = stdout_write("no newline here")

    # 向 stderr 写入，不追加换行；stdout 不受影响。
    let rc4: i64 = stderr_write("error note")

    return 0
```

### 7 个函数速览

| 函数 | 签名 | 返回 | 说明 |
|---|---|---|---|
| `read_file` | `(path: str) -> str` | 文件内容字符串 | I/O 错误返回空串（i64-sentinel Q1）。 |
| `read_file_lines` | `(path: str) -> list[str]` | 去除 `\n`/`\r\n` 的行列表 | 保留尾空行（Q2）：`"a\nb\n"` → `["a","b",""]`。 |
| `write_file` | `(path: str, contents: str) -> i64` | `0` = 成功, `1` = I/O 错误 | 创建或截断。两个参数均被 Move 消费。 |
| `append_file` | `(path: str, contents: str) -> i64` | `0` = 成功, `1` = I/O 错误 | 文件不存在时创建（Q3）。参数均被消费。 |
| `stdin_read_all` | `() -> str` | stdin 至 EOF | EOF 返回空串。 |
| `stdout_write` | `(s: str) -> i64` | `0`/`1` 哨兵 | 不追加换行；区别于 `print`。 |
| `stderr_write` | `(s: str) -> i64` | `0`/`1` 哨兵 | 仅写 stderr；stdout 不变。 |

### i64-sentinel 惯用法

`write_file` / `append_file` / `stdout_write` / `stderr_write` 成功返回 `0`，
失败返回非零。规范写法：

```cobrust
let rc: i64 = write_file("/tmp/out.txt", "data")
if rc != 0:
    return rc   # 传播错误
```

`read_file` 出错时返回空 `str`（无独立哨兵 —— 裸字符串返回 Q1）。
用 `str_len(contents)` 区分"文件为空"与"读取失败"。

### `read_file_lines` 尾空行规则（Q2）

`read_file_lines(p)` 按 `s.split('\n')` 语义切分 —— 不同于 Python 的
`readlines()`。以 `\n` 结尾的文件**总会**产生一个尾空字符串元素：

```
"alpha\nbeta\ngamma\n" → ["alpha", "beta", "gamma", ""]  （4 个元素）
"a\nb"                 → ["a", "b"]                       （2 个元素）
""                     → [""]                              （1 个元素）
```

计数满足 `s.count('\n') + 1`。

### `print` vs `stdout_write`（ADR-0050f 跨表面调度表）

| 调用 | 追加换行？ | i64 返回 |
|---|---|---|
| `print("literal")` | 是 | 恒为 0 |
| `print(s: str)` | 是 | 恒为 0 |
| `print_no_nl(s)` | 否 | 恒为 0 |
| `stdout_write(s)` | 否 | 0 = 成功, 1 = 错误 |
| `stderr_write(s)` | 否 | 0 = 成功, 1 = 错误 |

`print` / `print_no_nl` 是"即发即忘"；`stdout_write` /
`stderr_write` 将写入结果暴露给需要检测管道关闭的程序。

M-F.3.6 变更说明：
- 7 个新 PRELUDE stub：`read_file`、`read_file_lines`、`write_file`、
  `append_file`、`stdin_read_all`、`stdout_write`、`stderr_write`。
- 7 个新 C-ABI shim，位于 `crates/cobrust-stdlib/src/io.rs`。
- 7 个新 intrinsic-rewrite arm，位于
  `crates/cobrust-cli/src/build/intrinsics.rs`。
- str 参数采用 Copy-at-operand 策略（ADR-0050c Phase 2a walk-back 先例）：
  shim 仅读取 Str 缓冲区而不释放；调用方 scope 负责 drop。
- Phase G：`stdin().read_all()` / `stdout().write(s)` 方法形式，
  待 MIR 方法分发落地后再加入。

端到端 corpus 见 `crates/cobrust-cli/tests/file_io_e2e.rs`；
类型检查表面见 `crates/cobrust-types/tests/well_typed.rs` 中 w176..w195 的段。

## 第三步：试用 AI alpha 能力（可选）

1. 复制 router 示例配置，并填入你的 provider 凭据：

```bash
cp cobrust.toml.example cobrust.toml
```

2. 在 `cobrust.toml` 中声明你需要的路由：
   - `[routing.structured]`：用于 `llm_complete_structured(prompt, schema_json)`
   - `[routing.tools]`：用于 `llm_complete_with_tools(prompt, registry_json)`
   - 任意自定义 `[routing.<task>]`：用于 `llm_dispatch(task, prompt)`

3. 当前 AI 能力以平铺的 prelude 函数形式调用：
   - `llm_complete(provider, model, prompt)`
   - `llm_dispatch(task, prompt)`
   - `llm_stream(provider, model, prompt)`
   - `llm_complete_structured(prompt, schema_json)`
   - `llm_complete_with_tools(prompt, registry_json)`

当前 alpha 说明：
- 这些还不是 `cobrust.llm.*`、`cobrust.prompt.*`、`cobrust.tool.*` 这种模块路径调用。
- 如果缺少路由或 provider 配置，当前 alpha 会返回 `""`（`llm_stream` 则返回 `[]`），而不是更详细的运行时错误。

配置形状见 [cobrust.toml.example](../../../cobrust.toml.example)，完整设计说明见[架构](architecture.md)。

## 第三步半：循环与控制流

### `while` 循环

Cobrust 已经支持 `while` 循环;`for` 循环正在 M-F.3.1 sprint 中（参见 [ADR-0050](../../agent/adr/0050-phase-f3-language-completeness-batch.md)）。

```cobrust
fn main() -> i64:
    let i: i64 = 0
    while i < 5:
        print_int(i)
        i = i + 1
    return 0
```

输出:

```
0
1
2
3
4
```

### `break` 与 `continue`

- `break` 立即退出**最内层**的循环,跳过剩余循环体**以及**下一次条件判断。
- `continue` 跳过当前迭代剩余的循环体,直接回到循环头继续判断条件。
- 两个关键字必须单独成行(Cobrust 不支持 Python 的 `break <label>` —— 按宪章 §2.2 的极简主义,只保留裸关键字)。
- 仅在循环内部合法。若在函数体里没有外层循环,类型检查就会报错。

示例 —— 找到目标就立即跳出搜索循环:

```cobrust
fn first_multiple(n: i64, of: i64) -> i64:
    let i: i64 = 1
    while i <= n:
        if i % of == 0:
            return i        # 这里也可以 break + return,等价
        i = i + 1
    return -1
```

示例 —— 用 `continue` 跳过某些元素:

```cobrust
fn sum_skip_seven(limit: i64) -> i64:
    let i: i64 = 0
    let s: i64 = 0
    while i < limit:
        i = i + 1
        if i == 7:
            continue        # 跳过 7,直接进入下一次迭代
        s = s + i
    return s
```

示例 —— 嵌套循环;`break` 永远绑定最内层:

```cobrust
fn main() -> i64:
    let i: i64 = 0
    while i < 3:
        let j: i64 = 0
        while j < 3:
            if j == 1:
                break       # 仅退出内层;外层 i 循环继续
            j = j + 1
        i = i + 1
    return 0
```

完整的 `break` + `continue` 演示见 [`examples/early_exit.cb`](../../../examples/early_exit.cb),可用 `cobrust build` + `./early_exit` 验证预期输出。

设计动机:

- 一件事一种做法:裸 `break` / `continue` 已经覆盖了 "提早退出" 和 "跳过当前迭代" 两种最常见模式。带 label 的 break 是一把利刃;若真的需要,把内层循环抽成一个函数用 `return` 退出即可。
- 在循环外使用直接编译报错:防止 Python 把此类拼写错误推迟到运行时才暴露。

## 第四步：翻译 Python 库（可选）

```bash
cobrust translate tomli
```

完整的翻译工作流和验证门控见 [ADR-0007 translator pipeline](../../agent/adr/0007-translator-pipeline.md)。

## 开发工作流（贡献者路径）

```bash
# 克隆并从源码构建
git clone https://github.com/Cobrust-lang/cobrust && cd cobrust
cargo build --workspace

# 运行所有测试
cargo test --workspace

# 运行代码检查
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# 运行文档覆盖检查
bash scripts/doc-coverage.sh
```

## 进一步阅读

- [项目概览](overview.md)
- [设计哲学](design-philosophy.md)
- [架构](architecture.md)
- [里程碑](milestones.md)
- 项目宪章 [`CLAUDE.md`](../../../CLAUDE.md)
