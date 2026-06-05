# `import dora` — Cobrust 中的机器人数据流节点(回调封送第三验证)

> 状态: ADR-0076 Phase 2 (合成运行时, 多输入输出子集). 第九个生态系统
> 模块,也是第三个通过 C ABI 跨越回调的模块(继 pit 的
> `fn(Request) -> Response` 和 hood 的 `fn() -> i64` 之后). 这里的形状是
> `fn(dora.Event) -> i64`, 混合了 pit 的 Event 接收器借用模式与 hood 的
> i64 退出码意图。
>
> **默认**运行时故意采用合成方式 — `node.run()` 注入预设事件, 不依赖真正的
> dora-rs 守护进程。Phase 1 证明了单输入链路; Phase 2 增加 **多输入分发** 与
> **`event.send_output(...)`**。**#146 Phase A** 随后通过可选的 `dora-real`
> 特性使其变为真实 — 一个真正的 `dora-node-api` `DoraNode` + `events.recv()`
> 循环(见下文"走向真实")。真实的 Arrow 数组载荷, yaml 加载数据流, 以及
> ROS2 桥仍属后续阶段。

## 示例先行

```python
import dora

fn detect(event: dora.Event) -> i64:
    let frame: str = event.data_str()
    print_no_nl("got frame: ")
    print(frame)
    return 0

fn main() -> i64:
    let node = dora.Node("detector")
    let _ = dora.node(detect)
    let _ = node.run()
    return 0
```

构建并运行:

```bash
cobrust build prog.cb -o prog
./prog
# got frame: frame_001
```

## 你得到什么 (Phase 1 表面)

- **`dora.Node(name) -> dora.Node`** — 构造一个合成的数据流节点句柄.
  `name` 是节点标识符 (例如 `"detector"`, `"sensor"`). 句柄在作用域
  退出时通过 `__cobrust_dora_node_drop` 一次性释放。
- **`dora.node(handler) -> i64`** — 注册一个顶层的 `fn(event:
  dora.Event) -> i64` 回调作为节点的事件处理器。Phase 1 每个进程支持
  一个处理器(按输入 id 路由的多处理器是 Phase 2 的功能,届时配合
  `@dora.node(inputs=..., outputs=...)` 装饰器脱糖)。返回 0 (sentinel —
  注册是对全局槽的副作用); 使用 `let _ = dora.node(detect)` 来丢弃它。
- **`Node.run() -> i64`** — 以预设的 Phase 1 mock 事件
  (`id="camera"`, `data_str="frame_001"`) 调用已注册的处理器一次。
  成功返回 0; 若未注册处理器则返回 -1。Phase 2 将以由实际 dora 协调器
  驱动的真正 `EventStream::into_iter()` 循环替代。
- **`Node.shutdown() -> i64`** — 在 Node 上翻转一个软关闭标志.
  Phase 1 对真正的协调器是空操作; Phase 2 发送真实信号。返回 0。
- **`Event.id() -> str`** — 此事件到达的输入 id (例如 `"camera"`).
  借用 shim — 分配一个新的 Cobrust `str` 缓冲区。
- **`Event.data_str() -> str`** — 事件载荷作为 UTF-8 字符串。当前载荷
  表面仅 `str`; 支持带类型多元素载荷的 Arrow `RecordBatch` 访问器
  推迟(ADR-0076c)。

## 多输入输出: 多个输入, 一个输出 (Phase 2)

用 `@dora.node(inputs=[...], outputs=[...])` 装饰器声明节点的输入与输出
端口。处理器随后**对每个声明的输入触发一次** — 在 `event.id()` 上分发 —
并用 **`event.send_output(output_id, payload)`** 发射结果:

```python
import dora

@dora.node(inputs=["tick", "camera"], outputs=["reading"])
fn on_event(event: dora.Event) -> i64:
    if str_eq_lit(event.id(), "camera") == 1:
        let payload: str = event.data_str()
        let _ = event.send_output("reading", payload)
    print_no_nl("saw input: ")
    print(event.id())
    return 0

fn main() -> i64:
    let node = dora.Node("sensor")
    let _ = node.run()
    return 0
```

```bash
cobrust build prog.cb -o prog
./prog
# saw input: tick
# output[reading]=frame_001
# saw input: camera
```

- **多输入分发** — 声明两个输入会使合成运行时按声明顺序为每个输入 id
  注入一个预设事件, 因此处理器运行两次。`event.id()` 区分两者。(预设
  载荷对 `camera` 是 `frame_001`, 对其他输入是 `frame_<id>` — 真实
  broker 提供实际数据。)
- **`event.send_output(output_id, payload) -> i64`** — 在一个**已声明**的
  输出端口上发射 `str` 载荷。输出 id 会对照你声明的 `outputs=[...]`
  校验。当 id 是**字符串字面量**(`send_output("pose", ...)`)时, 未声明
  的 id 现在会在**编译期**被捕获(ADR-0092): `cobrust check` / `cobrust
  build` 报错 `unknown dora output id …; declared outputs: [...]`, 若是
  近似拼写错误还会给出 `did you mean …?` 建议 — 所以你在运行之前就能
  修正。**计算得到**的 id(变量)无法静态校验, 因此保留运行时守卫
  (清晰的 stderr 消息 + `-1` 返回, 绝不静默丢弃)。合成运行时将一次成功
  发射捕获到 stdout 为 `output[<id>]=<payload>`。成功发射返回 0。
  `send_output` 挂在 **Event**(而非 Node)上, 因为 Event 是处理器
  作用域内唯一的句柄。

> 为什么用 `str_eq_lit(event.id(), "camera") == 1` 而不是 `event.id() ==
> "camera"`? `str` 对 `str` 的 `==` 是一个独立的语言特性; `str_eq_lit(...)`
> 辅助函数是当前已证明的分发形式。

## 带类型的数值载荷 — 收 `coil.Buffer`, 发 `coil.Buffer`

`str` 载荷适合命令和标签, 但机器人的真实数据是**数字**: 状态向量,
传感器张量, 控制命令。dora 以带类型的 [Apache Arrow](https://arrow.apache.org/)
数组搬运它们 — Cobrust 则把它们交给你, 成为一个 **`coil.Buffer`**(就是
`import coil` 给你做数学用的同一种数组类型)。一种数组类型同时横跨数值
支柱**和** dora 线路 — 无需学第二种类型, 无需转换仪式:

```python
import dora
import coil

@dora.node(inputs=["state"], outputs=["action"])
fn policy(event: dora.Event) -> i64:
    let obs: coil.Buffer = event.data_buffer()   # 带类型的数值输入
    let m: f64 = coil.mean(obs)                   # 做真正的 numpy 式数学
    let action: coil.Buffer = coil.full(3, m)     # 构造带类型的输出
    let _ = event.send_output_buffer("action", action)  # 发射它
    return 0

fn main() -> i64:
    let node = dora.Node("policy_node")
    let _ = node.run()
    return 0
```

- **`event.data_buffer() -> coil.Buffer`** — 把带类型的数值输入载荷读成
  一个 `coil.Buffer`。支持的元素类型是 **`float64`、`float32`、`int64`、
  `int32`、`bool`** — Arrow 与 `coil` 重叠的那 5 种 dtype。`int64` 数组保持
  `int64`(**不会**被悄悄变成浮点); `float64` 数组保持 `float64`。这个
  Buffer 是**你的**: 离开作用域时自动释放(恰好一次 — 无泄漏, 无双重释放)。
  在合成默认构建上(无 broker)你得到预设的 `float64 [1.0, 2.0, 3.0]`, 使链路
  在测试中可运行; 在 `--features dora-real` 下你得到真实解码的数组。
- **`event.send_output_buffer(output_id, buffer) -> i64`** — 把一个
  `coil.Buffer` 作为带类型的 Arrow 数组在一个**已声明**的输出端口上发射。
  它是与 `send_output` **不同的方法**(不是重载), 这样编译器 — 以及为你
  写节点的 LLM — 总能明确知道你指的是哪一个。同样的编译期输出 id 检查
  适用: 字符串字面量拼错(声明了 `action` 却写 `send_output_buffer("acton",
  ...)`)会在 `cobrust check` 阶段被抓住。你传入的 `buffer` 是**借用**而非
  消耗 — 你的作用域仍然拥有它并只释放一次。

> **为什么用 `coil.Buffer` 而不是新的 `pa.array` 类型?** 数学与线路共用
> 一种数组类型是优雅的、一件事一种做法的选择(ADR-0076c)。机器人策略
> 收到一个 `Buffer`, 在其上跑 `coil` 数学, 再发出一个 `Buffer` — 无需
> `Frame ↔ Buffer` 来回倒腾。(这是可逆的: 若将来真的需要, 可以再加一个
> pyarrow 风格的表面。)

> **图像与文本被有意推迟。** 相机帧是 `uint8`, 命令是 `utf8` — 都不在
> 上面那 5 种数值 dtype 里。目前: 文本载荷用 `event.data_str()`; 原始图像
> 字节块的 `bytes` 访问器是近期跟进项。这些是**有名有姓的、诚实的缺口**,
> 而非静默失败 — 给 `data_buffer()` 一个非数值载荷会返回一个空 Buffer
> (并在真实构建上记录原因)。

> **带缺失值(null)的数组同样是有名有姓的缺口。** Arrow 数组可以把某些槽位
> 标记为 "null"(缺失); 而 `coil.Buffer` 是稠密数组, 没有 "缺失" 这个概念。
> 所以一个**带 null 的**输入数组**不会**往返 — `data_buffer()` 会返回一个空
> Buffer 并记录原因, 而不是把 null 静默地变成 `0` / `false`(那会在不告诉你
> 的情况下污染你的数据)。请发送一个不含 null 的数组(或者非数值载荷用
> `data_str`)。

## 走向真实 — `dora-real` 特性 (#146 Phase A)

`cobrust-dora` 的默认构建是**合成的**(`node.run()` 循环注入预设事件,
因此无需 dora 守护进程链路即可工作 — 适合快速测试 + wasm 目标)。用
**`dora-real`** 特性构建会把该循环替换为一个**真正的 `dora-node-api`
节点**:

```bash
# 构建真实 dora 运行时归档(重量级: 拉取 dora + arrow + tokio 栈 —
# 首次构建约 11 分钟)
cargo build -p cobrust-dora --features dora-real
```

开启该特性后, 上面**同一份** `.cb` 源码就成为一个真实的 dora 节点:

- `dora.Node(name)` 调用真正的 `DoraNode::init_from_env()`(节点加入真实的
  `dora start` 数据流 — dora 守护进程派生它并注入配置),
- `node.run()` 排空**真实的** `EventStream`, 对每个真实的 `Event::Input`
  触发你的处理器一次, 并在 `Event::Stop` 时停止,
- `event.data_str()` 解码**真实的**到达载荷(Apache Arrow Utf8 字符串);
  `event.data_buffer()` 把一个**真实的**带类型数值 Arrow 数组
  (`Float64Array`/`Int64Array`/…)解码成一个 `coil.Buffer`,
- `event.send_output(id, payload)` 发布**真实的** Arrow 字符串数组,
  `event.send_output_buffer(id, buffer)` 在节点的输出端口上发布**真实的**
  带类型数值 Arrow 数组(其他节点会收到)。

**你写的源码不变** — 同一份 `import dora` 程序默认是合成的, 在特性下是
真实的。C-ABI 表面, manifest 与 codegen 完全相同; 仅运行时主体被替换
(`cabi.rs` 局部改动, 而非编译器改动)。唯一的编译器侧新增是一个 macOS
`-framework CoreFoundation` 链接标志, 仅当在 macOS 上链接一个导入 `dora`
的程序时自动发出。

注意与限制:

- **仅原生.** 真实 dora 节点使用 `tokio` 网络, 在 wasm32 上不存在 — 因此
  `--features dora-real` 仅限原生。wasm 的 dora 故事保持合成(默认构建可
  交叉编译到 `wasm32-wasip1`)。
- **重量级.** 真实归档额外拉取约 100 个 crate; 二进制很大(剥离后约
  85 MB)。这就是该特性是可选而非默认的原因(对照 `coil` 把 `faer` 收在
  `coil-faer` 后面的做法)。
- **带类型数值数组可用**(`coil.Buffer ↔ Arrow`, ADR-0076c), 覆盖
  `float64/float32/int64/int32/bool`, 合成与真实构建均可 — 见上文"带类型
  的数值载荷"。

## 你不会得到什么 (推迟 — 诚实说明)

- ~~带类型的数值数组载荷~~ — **已交付(ADR-0076c)**, 覆盖 5 种 dtype
  `float64/float32/int64/int32/bool`, 经由 `event.data_buffer()` /
  `event.send_output_buffer(...)`。仍推迟: **`uint8`**(相机图像)+
  **`utf8`** 带类型数组 + **n 维形状元数据** — 文本用 `data_str()`,
  原始图像字节块(很快)用 `bytes` 访问器。下面这条是历史描述:
- 超出 `str`/`i64` 标量的 Arrow 数组 / `RecordBatch` 载荷 —
  `coil.Buffer ↔ Arrow` 桥(`pa.array_i64(...)` / `coil.Buffer`)属 Phase B
  (ADR-0076c)。(合成默认仅承载 `str`; `dora-real` 的 Phase-A 路径在真实
  Arrow 上承载标量 `str`。)
- yaml 加载的数据流 (`dora.run("dataflow.yml")`)。
- ~~对未声明输出 id 的编译期拒绝~~ — **已交付 (ADR-0092)。** 一个
  **字符串字面量**的未声明 `send_output` id 现在是 `cobrust check` /
  `cobrust build` 错误(`DoraUnknownOutputId`), 并附带声明列表 + 近似
  匹配的修复建议。仅**计算得到**(非字面量)的 id 仍依赖运行时 `-1`
  守卫。
- `for event in node:` 轮询迭代器形式。
- ROS2 桥发布表面 (子-ADR 0076a — Phase 3)。
- `cobrust-dora` 的 riscv64 交叉构建 (ADR-0075 Phase 1 依赖 — Phase 3
  延伸目标)。
- 真实机器人 CartPole 仿真演示 (Phase 3 交付物)。

## 为什么用 FFI 而不是翻译?

dora-rs 的热路径 (Zenoh 共享内存传输 + Arrow 零拷贝 + tokio 协调) 是
该运行时的核心竞争力。在 Cobrust 中重新实现其中任何一部分都会追逐一个
SemVer-0 移动靶标,同时浪费 dora-rs 的投入。Cobrust 节点在
`dora-node-api` Rust crate 边界参与; 性能与手写 Rust dora 节点一致.
设计理由见 ADR-0076 §3。

## 释放纪律

- `dora.Node` 是 `.cb` 拥有的句柄 — 作用域退出时通过
  `__cobrust_dora_node_drop` 释放一次。
- `dora.Event` 是 Rust 拥有的 — trampoline 在回调期间拥有 `Box<Event>`
  并在返回时释放。`.cb` 一侧绝对不能释放 `dora.Event` 局部变量;
  manifest 通过让 `handle_drop_symbol(DORA_EVENT_ADT)` 返回 `None`
  来强制这一点。

## 交叉引用

- [`import pit`](import-pit.md) — 姊妹第六模块 (第一次回调验证)。
- [`import hood`](import-hood.md) — 姊妹第七模块 (第二次回调验证)。
- ADR-0076 (`docs/agent/adr/0076-dora-cb-stream-y.md`) — Phase 1/2/3 架构。
- ADR-0072 (`docs/agent/adr/0072-cb-ecosystem-import-wiring.md`) — L1→L5 链。
- ADR-0073 (`docs/agent/adr/0073-cb-callback-marshalling.md`) — trampoline 模式。
- dora-rs 上游 — <https://github.com/dora-rs/dora>。
