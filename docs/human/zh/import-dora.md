# `import dora` — Cobrust 中的机器人数据流节点(回调封送第三验证)

> 状态: ADR-0076 Phase 1 (合成运行时). 第九个生态系统模块,也是第三个通过
> C ABI 跨越回调的模块(继 pit 的 `fn(Request) -> Response` 和 hood 的
> `fn() -> i64` 之后). 这里的形状是 `fn(dora.Event) -> i64`, 混合了 pit
> 的 Event 接收器借用模式与 hood 的 i64 退出码意图。
>
> Phase 1 故意采用合成运行时 — `node.run()` 模拟一次预设的
> `("camera", "frame_001")` 事件到达, 不依赖真正的 dora-rs 守护进程。
> 链路已证明; Phase 2 接入真正的 dora-rs 编排(多输入输出, yaml 加载
> 数据流, ROS2 桥访问)。

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
- **`Event.data_str() -> str`** — 事件载荷作为 UTF-8 字符串。Phase 1
  仅 `str`; Phase 2 通过 `event.data_arrow()` 扩展到 Arrow `RecordBatch`
  访问器以支持带类型的多元素载荷。

## 你不会得到什么 (Phase 1 — 推迟)

- 多输入 / 多输出编排 (Phase 2 配合
  `@dora.node(inputs=["a", "b"], outputs=["c"])` 装饰器)。
- 真正的 dora-rs 守护进程集成 (Phase 2 配合 `dora-node-api` 依赖 +
  `tokio` 运行时来宾模式)。
- yaml 加载的数据流 (`dora.run("dataflow.yml")` — Phase 2)。
- Arrow `RecordBatch` 载荷访问器 (`event.data_arrow()` + 原语扩展 —
  Phase 2)。
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
