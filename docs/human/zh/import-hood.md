# `import hood` — 在 Cobrust 中写 click 风格的 CLI 命令(回调跨语言第二次证明)

> 状态:ADR-0073 第二次证明。在 `pit`(Flask)走通了 `fn(Request) -> Response`
> 形状的回调跨 C ABI 之后,`hood`(cobra 对 Python `click` 的重命名)
> 作为第七个生态模块加入链条 —— 也是第二个跨回调的模块。这里的形
> 状是 `fn() -> i64`(零位置参数;i64 返回值就是用户的 exit-code 意
> 图),证明这条链可以从已被证明的蹦床模式泛化出去。

## 先看例子

```python
import hood

fn handle_greet() -> i64:
    print("hello from hood")
    return 0

fn main() -> i64:
    let cmd = hood.Command("greet", "Print a friendly greeting")
    let _ = cmd.handler(handle_greet)
    let _ = cmd.run()
    return 0
```

构建并运行:

```bash
cobrust build prog.cb -o prog
./prog
# hello from hood
```

## 你能用到的(首次证明表面)

- **`hood.Command(name, help) -> Command`** — 构造一个 click 风格的
  命令,`name` 是 CLI 动词,`help` 是帮助文本里展示的简介。两者都是
  普通 str。
- **`Command.handler(fn)`** — 把一个顶层 `fn` 绑定为命令的回调。
  handler 必须是顶层 `fn handler() -> i64: …`。返回 `i64` 零值哨
  兵;规范用法是 `let _ = cmd.handler(...)`。
- **`Command.run() -> i64`** — 调用已绑定的回调。回调真正被调用时返
  回 `0`;未绑定 handler 时返回 `-1`。`fn main() -> i64: return cmd.run()`
  是 hood-only 程序的自然形态。

## 为什么是这样的设计?

- **pit 与 hood 共享同一个回调 ABI 形状**:每个 handler 都以
  `extern "C" fn(*mut u8) -> *mut u8` 形式跨越(ADR-0073 §5.1)。
  hood 的零参 / i64 返回形状走 null 指针占位 + 返回指针丢弃 —— handler
  自身的副作用(比如 `print(...)`)就是用户的意图(首次证明的范围)。
- **编译期捕获回调形状错误(§2.5 约束)**:与 pit 共享同一个门 —— 只
  接受顶层 `fn` NAME。lambda / fn 类型局部变量 / 调用结果 / 括号表达
  式全部拒绝。诊断同时打印 LLM 应当应用的修复建议(Direction B)。
- **跨 C 边界 panic 即 abort**:蹦床用 `catch_unwind` 包住调用,panic
  时 abort 并打印结构化 stderr(ADR-0073 §3 Q5)。
- **Drop 纪律(§2 D6)**:`Command` 句柄归 `.cb` 所有;作用域退出执行
  `__cobrust_hood_command_drop`。Boxed click builder + 注册的闭包一起
  被释放,恰好一次。

## 当前限制

- **不支持闭包 / lambda 作为 handler**:必须是顶层 `fn`。
- **没有装饰器糖**:`@cmd.handler` 是 ADR-0074(下一个 sprint,
  click 装饰器栈 desugar 会处理)。
- **`.cb` 还没接通 clap 的 arg / option**:当下的 `Command.handler(fn)`
  只注册一个裸回调。`cobrust-hood/src/decorators.rs` 里的 clap 侧
  option / argument builders 在 Rust 端可用,但还没通过 `.cb` 生
  态清单暴露 —— 配套 follow-up 会在清单支持多方法句柄 builder 后,
  接通 `cmd.option(name, help)` 与 `cmd.argument(name)`。
- **`Command.run()` 不会回传 handler 的 i64 返回值**:当前 click 风
  格的 `fn() -> i64` 回调返回值在蹦床边界被丢弃;`cmd.run()` 成功
  返回 `0` / 未绑 handler 时返回 `-1`。未来通过 ABI 扩展把 handler
  的 i64 透传出去是一个跟踪项。
