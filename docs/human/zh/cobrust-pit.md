# cobrust-pit —— Cobrust 的 Flask 形态 Web 服务器

`cobrust-pit` 是 Python **Flask** Web 服务器接口的 Cobrust 翻译版。它给你
熟悉的形态——建一个 app、注册路由、返回文本或 JSON、跑起服务器——底层由成熟
的 Rust `axum` 栈(架在 `tokio` 之上)支撑。公共 API 是**同步**的:你永远不用
写 `async`,`app.run(...)` 就是阻塞直到进程被杀掉,和 Flask 一模一样。

Cobrust 这边的名字叫 `pit`(按 ADR-0071:"a snake pit handles many
callers" —— 蛇坑接待众多来客);它翻译的库是 Flask。它是 v0.7.0 的
"MUST-ship"(必须交付)HTTP 服务器(Stream Z.1.a)。

## 先看例子

一个极小的 REST 应用——一个首页、一个路径参数、一个 JSON 端点:

```rust
use pit::{App, Request, Response};
use serde_json::json;

let mut app = App::new();                       // == Flask(__name__)

// 纯文本路由。
app.get("/", |_req: Request| Response::text("hello, pit"))?;

// 路径参数,用 Flask 的 <name> 语法捕获。
app.get("/users/<id>", |req: Request| {
    let id = req.path_param("id").unwrap_or("?");
    Response::json(&json!({ "id": id }))        // == jsonify(...)
})?;

// 读取请求体的 POST。
app.post("/echo", |req: Request| {
    Response::text(req.text().unwrap_or_default())
})?;

// 阻塞并提供服务(端口 0 = 用 serve_in_background 选个空闲端口)。
app.run("127.0.0.1", 8080)?;
```

这个形态和你用 Flask 在 Python 里写的一致:

```python
from flask import Flask, jsonify, request
app = Flask(__name__)

@app.route("/")
def root():
    return "hello, pit"

@app.route("/users/<id>")
def user(id):
    return jsonify({"id": id})

@app.route("/echo", methods=["POST"])
def echo():
    return request.get_data(as_text=True)

app.run("127.0.0.1", 8080)
```

这个首版唯一可见的差别:你用**方法调用**(`app.get(path, handler)`)注册
路由,而不是 `@app.route` 装饰器。装饰器会在后面随 Cobrust 源码侧接线一起落地。

## 你能得到什么

- **`App`** —— `App::new()`,然后 `app.route(method, path, handler)`
  或简写 `app.get / post / put / delete(path, handler)`,再用
  `app.run(host, port)` 提供服务(阻塞)。`serve_in_background` 绑定到一个
  临时端口并在后台提供服务(测试用)。
- **`Request`** —— 每个 handler 收到的对象:`.method()`、`.path()`、
  `.path_param(name)`(`<name>` 捕获)、`.query(name)`(查询字符串)、
  `.header(name)`(大小写不敏感)、`.body()`、`.text()`、`.json()`。
- **`Response`** —— handler 返回的对象:`Response::text(body)`(200、
  `text/html`)、`Response::json(value)`(200、`application/json` ——
  这就是 `jsonify`),外加 `.with_status(code)` 和 `.with_header(k, v)`
  这两个构建器。

## 路由

路由按段(segment)逐段匹配。两种段:

- **字面段** —— `/users/list` 只匹配该路径。
- **捕获段** —— `/users/<id>` 匹配 `/users/42`,并通过
  `req.path_param("id")` 把 `id = "42"` 交给 handler。

匹配不到的路径返回 **404**。注册为 `GET` 却被 `POST` 请求的路径,在这个首版里
也返回 404(Flask 返回 405 —— 这点细化是延后项)。

## 错误是值,不是异常

Flask 抛 Python 异常(端口被占抛 `OSError`,同一路由注册两次抛
`AssertionError`)。Cobrust 改为返回 `Result<T, PitError>` —— 你用 `?` 或
`match` 处理失败,编译器保证你不会忘。错误种类:

| `PitErrorKind` | 何时 | Flask 对应 |
|---|---|---|
| `Bind` | 监听套接字无法绑定 | `app.run` 处的 `OSError` |
| `DuplicateRoute` | 同一 `(method, path)` 注册两次 | 端点覆盖的 `AssertionError` |
| `InvalidRoute` | 路径不合法(无前导 `/`、`<...>` 未闭合) | Werkzeug 规则错误 |
| `Runtime` | 服务任务失败 / 坏的请求体 | (内部) |

同一路由注册两次永不 panic —— 它返回 `Err`:

```rust
let mut app = App::new();
app.get("/x", |_r| Response::text("a"))?;
let err = app.get("/x", |_r| Response::text("b")).unwrap_err();
assert_eq!(err.kind, pit::PitErrorKind::DuplicateRoute);
```

## 为什么这样设计?

- **贴合 Python 的先验。** 宪法的 LLM-first 原则(§2.5)指出 Cobrust 是 AI
  智能体一次就能写对的语言。`@app.route("/path")` + `return jsonify(...)`
  是 Python 语料中训练得最充分的同步 Web 服务器模式,所以我们保留这个形态
  (只改名字,按 ADR-0071)。
- **同步,无 async 染色。** Flask 是同步(WSGI)框架。Cobrust 禁止 async/sync
  两色问题(§2.2),所以 `pit` 接口保持同步:`app.run(...)` 在底层通过
  `block_on` 桥接,在 `tokio` 运行时上驱动 `axum` 服务器 —— 你永远看不到
  `Future`。
- **`Result`,绝不用异常。** 宪法(§2.2)把 `Result<T, E>` 定为默认错误路径。
  封闭的 `PitErrorKind` 枚举意味着对失败分支的 `match` 是穷尽的 —— 类型检查器
  会抓住你漏掉的那个分支。

## 兼容层级:`semantic`

`cobrust-pit` 标记为 `@py_compat(semantic)`。它保留 Flask 的路由 / 请求 /
响应**形态**以及常见 REST 路径上的可观察行为,但并非与 Flask 逐字节一致。
已知差异:

- 路由用**方法调用**注册,而非 `@app.route` 装饰器(装饰器随 Cobrust 源码侧
  接线落地)。
- 接口**仅同步**(贴合 Flask 自身的 WSGI 模型)。
- 错误是 `Result::Err`,不是抛出的异常。
- 路由模式仅支持**字面段 + `<name>` 捕获段** —— 没有 `<int:id>` 转换器、
  正则规则或尾斜杠重定向。
- handler 返回值是**字符串**(文本)、**JSON 值**(`jsonify`)或显式的
  `(status, headers, body)` —— 而非 Flask 完整的返回值协议。
- 已知路径上的未知方法返回 404,而非 405。

## 暂未支持

- 从 Cobrust `.cb` 源码使用的 `@pit.route` 装饰器与 `import pit` —— 这部分
  接线是单独的延后步骤(以及建在其上的 Z.8 REST 演示)。
- Werkzeug 转换器(`<int:id>`、`<path:p>`)、正则规则、405 响应。
- 蓝图(Blueprint)、请求前后钩子、会话、cookie、Jinja 模板、静态文件、
  流式响应。
- WSGI/ASGI 应用协议与 `app.test_client()`。
