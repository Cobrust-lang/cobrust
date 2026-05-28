# `import pit` — 在 Cobrust 中提供 HTTP 服务 (回调跨语言首次证明)

> 状态:ADR-0073 首次证明。在 `den`(SQLite, 句柄模式) /
> `nest`(TOML, 纯值) / `strike`(HTTP 客户端, 句柄+自由函数) /
> `scale`(msgpack, 值) / `molt`(日期时间, 句柄)走通后,
> `pit`(Flask, Web 服务器)作为第六个模块加入链条 —
> 并首次将 **回调函数指针** 通过 C ABI 跨越。一个 `.cb` 顶层 fn
> 在编译产物里变成 fn 指针,在 Rust 蹦床里被 transmute 成
> `move |req| -> resp` 闭包,然后从 axum 内部被调用。

## 先看例子

```python
import pit

fn handle_ping(req: pit.Request) -> pit.Response:
    return pit.text_response(200, "pong")

fn main() -> i64:
    let app = pit.App()
    let _ = app.route("GET", "/ping", handle_ping)
    let _server = app.serve_in_background("127.0.0.1", 0)
    # 忙等待保活;进程退出前服务器一直绑定着端口
    let i: i64 = 0
    while i < 10000000000:
        i = i + 1
    return 0
```

构建并运行,然后用 curl 探测:

```bash
cobrust build prog.cb -o prog
./prog &
# 找到 ephemeral 端口并请求
curl http://127.0.0.1:<port>/ping
# pong
```

## 你能用到的(首次证明表面)

- **`pit.App() -> App`** — 构造一个空的 app。
- **`pit.text_response(status, body) -> Response`** — 用给定的 status
  与 body 构造一个文本响应。status 会被钳制到合法 HTTP 范围内,
  越界值返回 500。
- **`App.route(method, path, handler)`** — 将一个顶层 `fn` 注册为
  `method path` 的处理器。handler 必须是顶层
  `fn handler(req: pit.Request) -> pit.Response: …`。返回 `None`;
  规范用法是 `let _ = app.route(...)`。
- **`App.serve_in_background(host, port) -> ServerHandle`** — 绑定到
  `host:port`(port `0` 表示 ephemeral),把 axum server spawn 到
  pit 内置的 tokio 运行时上,返回 `ServerHandle`。其 drop 会中止
  server task。`pit.Request` 的访问器(path/method/body)是配套
  follow-up;当前 handler 可以忽略 Request 直接给出固定 Response。

## 为什么是这样的设计?

- **统一的回调 ABI 形状**:每个 handler 都以
  `extern "C" fn(*mut u8) -> *mut u8` 形式跨越。`.cb` codegen 通过
  `function_ids` 表把 handler 的 fn 指针物化;蹦床再 transmute 回来。
  ADR-0073 §2 D4。
- **编译期捕获回调形状错误(§2.5 约束)**:类型检查器只接受顶层
  `fn` NAME 这一种形态 —— lambda / fn 类型局部变量 / 调用结果 /
  括号表达式全部拒绝。诊断同时打印出 LLM 应当应用的修复建议
  (Direction B)。
- **跨 C 边界 panic 即 abort**:`.cb` handler 中的 panic 若 unwind
  进 Rust 是 UB。蹦床用 `catch_unwind` 包住调用,panic 时 abort
  并打印结构化 stderr(ADR-0073 §3 Q5)。
- **Drop 纪律(§2 D6)**:`Request` 句柄归 Rust 所有(蹦床每次回调
  前分配 box、回调返回后释放);`.cb` 源永不 drop `pit.Request` 局
  变量。`text_response` 返回的 `Response` 句柄经由 `Terminator::Return`
  传出,MIR drop pass 将其视为 moved-out,因此没有 double-free。

## 当前限制

- **不支持闭包 / lambda 作为 handler**:必须是顶层 `fn`。
- **没有装饰器糖**:`@app.route("/x")` 是 ADR-0074(下一个 sprint)。
- **`pit.Request` 访问器尚未接通**:handler 必须在不读取
  Request 的 path/method/body 的情况下构造 Response。配套 follow-up
  会补齐 borrow 接口。
- **单线程 handler**:axum 并发调度,但每个 handler 调用是单个
  tokio task;`.cb` handler 必须可重入且满足 Send + Sync(这点天然
  满足 —— extern fn 指针无条件是 Send + Sync + Copy)。
