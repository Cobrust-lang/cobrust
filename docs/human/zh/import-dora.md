# `import dora` — Cobrust 中的机器人数据流节点(回调封送第三验证)

> 状态: ADR-0076 Phase 2 (合成运行时, 多输入输出子集). 第九个生态系统
> 模块,也是第三个通过 C ABI 跨越回调的模块(继 pit 的
> `fn(Request) -> Response` 和 hood 的 `fn() -> i64` 之后). 这里的形状是
> `fn(dora.Event) -> i64`, 混合了 pit 的 Event 接收器借用模式与 hood 的
> i64 退出码意图。
>
> 运行时故意采用合成方式 — `node.run()` 注入预设事件, 不依赖真正的
> dora-rs 守护进程或 zenoh broker。Phase 1 证明了单输入链路; Phase 2 增加
> **多输入分发**(处理器对每个声明的输入触发一次)与
> **`event.send_output(...)`**(在声明的输出端口上发射)。真正的 dora-rs
> 编排(真实 zenoh 传输, Arrow 列表/字典载荷, yaml 加载数据流, ROS2 桥)
> 仍属后续阶段。

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
  校验: 未声明的 id 会被拒绝, 给出清晰的 stderr 消息并返回 `-1`(绝不
  静默丢弃)。合成运行时将发射捕获到 stdout 为 `output[<id>]=<payload>`。
  成功发射返回 0。`send_output` 挂在 **Event**(而非 Node)上, 因为
  Event 是处理器作用域内唯一的句柄。

> 为什么用 `str_eq_lit(event.id(), "camera") == 1` 而不是 `event.id() ==
> "camera"`? `str` 对 `str` 的 `==` 是一个独立的语言特性; `str_eq_lit(...)`
> 辅助函数是当前已证明的分发形式。

## 你不会得到什么 (推迟 — 诚实说明)

- 真正的 dora-rs 守护进程集成 + 真实 zenoh broker(运行时保持合成;
  `dora-node-api` 依赖 + `tokio` 来宾模式属后续阶段)。
- 超出 `str`/`i64` 标量的 Arrow 列表/字典 `RecordBatch` 载荷
  (`pa.array_i64(...)` — ADR-0076c)。
- yaml 加载的数据流 (`dora.run("dataflow.yml")`)。
- 对未声明输出 id 的编译期拒绝 — 当前未声明的 `send_output` 在**运行时**
  被捕获(`-1` + stderr 消息); 编译期 `DoraUnknownOutputId` 错误是后续
  工作。
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
