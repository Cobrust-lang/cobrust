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

## 中间件(ADR-0078 第一阶段)

在 **serve 之前** 调用 `app` 上的方法即可启用一个固定的中间件预设。
每一个都是 `tower-http` 现成的 `Layer`,注册到 axum router 上:

```python
import pit

fn handle_root(req: pit.Request) -> pit.Response:
    return pit.text_response(200, "hello")

fn main() -> i64:
    let app = pit.App()
    let _ = app.use_cors()         # CORS —— 添加 Access-Control-Allow-Origin
    let _ = app.use_trace()        # 请求追踪/日志(副作用)
    let _ = app.use_compression()  # gzip/br/deflate/zstd 响应压缩
    let _ = app.route("GET", "/", handle_root)
    let _server = app.serve_in_background("127.0.0.1", 0)
    let i: i64 = 0
    while i < 10000000000:
        i = i + 1
    return 0
```

- **`app.use_cors()`** —— 应用 `CorsLayer::permissive()`;响应会带上
  `Access-Control-Allow-Origin`。对应 FastAPI/Flask-CORS 的形态
  (`app.add_middleware(CORSMiddleware, …)` / `CORS(app)`)。
- **`app.use_trace()`** —— 应用 `TraceLayer::new_for_http()`;产生
  tracing span/event(日志副作用,不是 HTTP 头)。
- **`app.use_compression()`** —— 应用 `CompressionLayer`;当客户端协商
  了可接受的编码时压缩响应体,否则原样透传。

三者都返回 `None`(用 `let _ = …` 形式),且 **必须在**
`serve_in_background` / `run` **之前** 调用:标志位在服务器构建 router
时只读取一次,之后再调用即为 no-op。

## 带校验的请求体(`route_validated`,ADR-0080)

`app.route_validated(method, path, handler)` 是 FastAPI 的标志性能力,
以 Cobrust 的方式实现:**请求体是一个带类型的 `class`,类型即契约。**
字段是否存在、字段类型在编译期检查;值层面的约束(如取值范围)在请求
边界处检查一次,失败时渲染成一个带类型的 **422** —— 既不抛异常,也不在
handler 内部二次校验。

```python
import pit

# 一个带校验的请求体就是一个字段带类型的 `class`。可选的 `where` 子句
# 加上一个值约束(这里是闭区间整数范围)。
class CreateScore:
    name: str
    rank: i64 where 0 <= self and self <= 100

# handler 把请求体作为带类型的第二个参数。pit 在 handler 运行之前就把
# JSON 请求体校验进它 —— 所以能进到 handler 就说明校验已通过。
# `body.rank` 静态为 `i64`;写错的 `body.nonexistent` 是编译期错误,
# 而不是运行期 KeyError。
fn create_score(req: pit.Request, body: CreateScore) -> pit.Response:
    return pit.text_response(201, "created")

fn main() -> i64:
    let app = pit.App()
    let _ = app.route_validated("POST", "/scores", create_score)
    let _exit = app.run("127.0.0.1", 8080)
    return 0
```

边界处的行为:

- `POST /scores {"name":"a","rank":50}` → **201**,handler 运行。
- `{"name":"a","rank":200}` → **422**(rank 超范围),handler **永不进入**。
- `{"rank":50}`(缺 `name`)或 `{"name":"a","rank":"x"}`(类型错误)
  → **422** —— 请求体必须与声明的形状完全一致(每个声明字段都在、类型
  正确、没有多余的键)。

本版本的 `where` 子句语法是 `i64` 字段上的固定整数范围形式:
`0 <= self`、`self <= 100` 或 `0 <= self and self <= 100`(`self` 是字段
的值;`>=` 也支持)。任何其他谓词都是编译错误,并会告诉你接受的形式。
字符串长度(`len(self) <= n`)与正则模式校验属于后续阶段。

为什么比 Flask/FastAPI 更优:结构由编译器捕获(你无法发布一个读取不存在
字段的 handler),422 是渲染成 `Response` 的 `Result`(而不是 unwind 的
异常),且连线是一次显式调用(没有隐藏的依赖注入注册表)。目前成功的
handler 返回一个固定响应 —— 把校验后的请求体回显出去是后续工作(需要
`.cb` 结构体 ↔ JSON 桥)。

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
- **中间件仅支持固定预设**(ADR-0078 第一阶段):
  `use_cors()`/`use_trace()`/`use_compression()` 均不接受参数。
  可配置的 CORS origin、自定义 `.cb` 中间件、自动 OpenAPI
  属于 ADR-0078 第二/三阶段。
- **带校验的请求体**(`route_validated`,ADR-0080):当前只支持 `i64`
  字段上的固定整数范围 `where` 校验;字符串长度 / 正则校验、嵌套类与
  列表字段请求体、自动 `/openapi.json` schema,以及把校验后的请求体回显
  到响应里(`json_response(body)`)都属于后续阶段。成功的 handler 目前
  返回一个固定响应。
- **`pit.Request` 访问器尚未接通**:handler 必须在不读取
  Request 的 path/method/body 的情况下构造 Response。配套 follow-up
  会补齐 borrow 接口。
- **单线程 handler**:axum 并发调度,但每个 handler 调用是单个
  tokio task;`.cb` handler 必须可重入且满足 Send + Sync(这点天然
  满足 —— extern fn 指针无条件是 Send + Sync + Copy)。
