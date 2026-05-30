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
- **`pit.json_response(status, body) -> Response`**(ADR-0081)—— 把一个
  **已校验的请求体**用给定 status 重新序列化为 JSON 响应。这里的 `body`
  就是你的 `route_validated` handler 收到的那个带类型的请求体参数;响应以
  `application/json` 原样回显它。因为它重新序列化的是校验本身产出的同一个
  值,所以响应体不可能与已校验的请求体发生漂移。详见下文「带校验的请求体」。
- **`body.field` 读取**(ADR-0081)—— 在 `route_validated` 的 handler 内部,
  以带类型的属性访问读取已校验请求体的字段(`body.rank` → `i64`,`body.name`
  → `str`),而不是字符串键。写错的字段是编译错误。详见下文「读取请求体字段」。
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
    return pit.json_response(201, body)   # 把已校验的请求体作为 JSON 回显

fn main() -> i64:
    let app = pit.App()
    let _ = app.route_validated("POST", "/scores", create_score)
    let _exit = app.run("127.0.0.1", 8080)
    return 0
```

边界处的行为:

- `POST /scores {"name":"a","rank":50}` → **201**,响应体为
  `{"name":"a","rank":50}`(由 `json_response` 重新序列化的已校验请求体),
  handler 运行。
- `{"name":"a","rank":200}` → **422**(rank 超范围),handler **永不进入**。
- `{"rank":50}`(缺 `name`)或 `{"name":"a","rank":"x"}`(类型错误)
  → **422** —— 请求体必须与声明的形状完全一致(每个声明字段都在、类型
  正确、没有多余的键)。

`where` 子句语法是一小组固定形式,按字段类型区分:

- `i64` 字段上的**整数范围**:`0 <= self`、`self <= 100` 或
  `0 <= self and self <= 100`(`self` 是字段的值;`>=` 也支持);
- `str` 字段上的**字符串长度**:`len(self) <= 20`、`len(self) >= 1` 或
  `1 <= len(self) and len(self) <= 20`(见下一节);
- `str` 字段上的**字符串模式**:`pattern(self, "<正则>")`(见下一节)。

任何其他谓词 —— 或在错误的字段类型上写长度/模式形式 —— 都是编译错误,
并会告诉你接受的形式。

为什么比 Flask/FastAPI 更优:结构由编译器捕获(你无法发布一个读取不存在
字段的 handler),422 是渲染成 `Response` 的 `Result`(而不是 unwind 的
异常),且连线是一次显式调用(没有隐藏的依赖注入注册表)。

### 读取请求体字段(`body.rank`,ADR-0081)

在 `route_validated` 的 handler 内部,你现在可以**读取请求体的字段**并据此
处理。`body.rank` 读出一个 `i64`,`body.name` 读出一个 `str` —— 是带类型的
属性访问,绝不是字符串键 `body["rank"]`:

```python
import pit

class CreateScore:
    name: str
    rank: i64 where 0 <= self and self <= 100

fn create_score(req: pit.Request, body: CreateScore) -> pit.Response:
    let r: i64 = body.rank        # 读出已校验的 rank,例如 50
    let n: str = body.name        # 读出已校验的 name,例如 "alice"
    if r >= 50:
        return pit.text_response(200, "high")
    return pit.json_response(201, body)   # 或把整个请求体回显出去

fn main() -> i64:
    let app = pit.App()
    let _ = app.route_validated("POST", "/scores", create_score)
    let _exit = app.run("127.0.0.1", 8080)
    return 0
```

- `body.rank` 静态为 `i64`;写错的 `body.nonexistent` 是**编译期**错误
  (它会列出真实字段),绝不是运行期 `KeyError`。
- 读取是**全量**的:在你的 handler 运行之前,校验已经证明字段存在、类型
  正确、且在范围内 —— 所以没有要 unwrap 的 `None`,没有缺键的意外。
- 没有静默强转:`i64` 字段读成整数;`i64` 字段收到 JSON `1.5` 早已在 422
  边界被拒,所以读取永远不会截断浮点数。

第 1 阶段提供 `i64` + `str` 字段读取;`f64`/`bool` 以及嵌套类 / 列表字段
属于后续阶段。字段读取**只**对你的 handler 从 `route_validated` 收到的请求
体参数生效 —— 手动构造的 `CreateScore()` 值目前还没有字段存储(那是原生
结构体的后续工作)。编译器会跟踪二者的区别,所以你不会遇到意外。

## 字符串校验:长度 + 模式(ADR-0080 第 2 阶段)

`str` 字段可以再带两种 `where` 约束 —— **长度边界**与**正则模式**:

```python
import pit

class SignupBody:
    # 长度边界:1..=20 个字符(闭区间)。`len(self)` 是字段的长度;
    # 单边形式 `len(self) <= 20` / `len(self) >= 1` 同样支持。
    username: str where 1 <= len(self) and len(self) <= 20
    # 模式:值必须匹配此正则(一个字符串字面量)。
    email: str where pattern(self, ".+@.+")

fn signup(req: pit.Request, body: SignupBody) -> pit.Response:
    return pit.text_response(201, "created")

fn main() -> i64:
    let app = pit.App()
    let _ = app.route_validated("POST", "/signup", signup)
    let _ = app.serve_openapi("/openapi.json")
    let _exit = app.run("127.0.0.1", 8080)
    return 0
```

在边界处:

- `{"username":"bob","email":"b@x.com"}` → **201**,handler 运行。
- 21 字符的 username → **422**(超过上限 20),handler **永不进入**。
- 空的 username → **422**(低于下限 1)。
- `"email":"notanemail"` → **422**(未匹配 `.+@.+` 模式)。

两点遵循优雅法则的说明:

- **错误的正则是编译错误,而非运行期意外。** 若你写 `pattern(self, "[")`
  (未闭合的字符类),编译器会带着修复建议拒绝它 —— 你永远不会发布一个
  每次请求都 panic 的服务器。
- **OpenAPI schema 始终保持一致。** 长度边界呈现为 `minLength`/`maxLength`,
  模式呈现为 `pattern` —— 与校验器检查的同一来源(见下一节),因此不会漂移。

## 自动 OpenAPI(`serve_openapi`,ADR-0080 第 1b-iii 阶段)

FastAPI 的另一个标志性能力是免费的 `/docs` —— 一份从你的模型派生出来的
OpenAPI schema。Cobrust 做同样的事,并带有一个关键性质:schema 派生自
**校验器读取的同一个来源**,因此它与服务器实际强制执行的约束**不会漂移**。

```python
fn main() -> i64:
    let app = pit.App()
    let _ = app.route_validated("POST", "/scores", create_score)
    # 显式开启 OpenAPI 文档服务。这不是魔法自动路由:你写下这一行,
    # 因此“是否提供文档”在调用点一目了然。
    let _ = app.serve_openapi("/openapi.json")
    let _exit = app.run("127.0.0.1", 8080)
    return 0
```

之后 `GET /openapi.json` 会返回一份 OpenAPI 3.1 文档。对上面的
`CreateScore` 请求体:

```json
{
  "openapi": "3.1.0",
  "components": {
    "schemas": {
      "CreateScore": {
        "type": "object",
        "properties": {
          "name": { "type": "string" },
          "rank": { "type": "integer", "minimum": 0, "maximum": 100 }
        }
      }
    }
  }
}
```

这里 `rank.maximum` 的 `100` 与校验器强制执行的边界**完全一致**(它会用
422 拒绝 `rank: 200`)—— 二者都从同一个字段表 + refinement 侧表读取。
不存在第二份手工维护、会逐渐脱节的 schema 声明(utoipa/drf-spectacular
的漂移坑被丢弃)。

`serve_openapi` 是一个**显式开关**(优雅法则:没有 import 期副作用,
没有隐藏全局)。请在它要记录的那些 `route_validated` 注册之后调用它。
映射:`str → {type:string}`、`i64 → {type:integer}`、
`f64 → {type:number}`、`bool → {type:boolean}`;整数范围 refinement 追加
`minimum`/`maximum`,字符串长度 refinement 追加 `minLength`/`maxLength`,
模式追加 `pattern`。对于上面的 `SignupBody`,文档会显示
`username: {type:string, minLength:1, maxLength:20}` 与
`email: {type:string, pattern:".+@.+"}` —— 与校验器强制执行的边界一致。
列表字段的 `maxItems` 形式属于后续阶段。

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
  可配置的 CORS origin、自定义 `.cb` 中间件属于 ADR-0078 第二/三阶段。
  (自动 OpenAPI 现已落地 —— 见上面的 `serve_openapi`。)
- **带校验的请求体**(`route_validated`,ADR-0080):`i64` 字段上的
  固定整数范围 refinement,以及 `str` 字段上的字符串长度(`len(self)`)与
  模式(`pattern(self, "…")`)refinement 现已支持。把校验后的请求体回显到
  响应里(`json_response(status, body)`)、以及从请求体上读取单个 `i64` /
  `str` 字段(`body.rank`、`body.name`)现也已落地(ADR-0081)。`f64` /
  `bool` 字段读取与嵌套类 / 列表字段请求体属于后续阶段。
- **OpenAPI**(`serve_openapi`,ADR-0080):文档覆盖每个带校验路由的请求体
  schema —— 类型,加上整数范围 `minimum`/`maximum`、字符串长度
  `minLength`/`maxLength`、以及 `pattern`。列表字段的 `maxItems` 形式跟随
  校验器的后续阶段;当前提供的文档是 Rust 组装出来的 JSON 字符串
  (尚未是 `.cb` 结构体序列化)。
- **`pit.Request` 访问器尚未接通**:handler 必须在不读取
  Request 的 path/method/body 的情况下构造 Response。配套 follow-up
  会补齐 borrow 接口。
- **单线程 handler**:axum 并发调度,但每个 handler 调用是单个
  tokio task;`.cb` handler 必须可重入且满足 Send + Sync(这点天然
  满足 —— extern fn 指针无条件是 Send + Sync + Copy)。
