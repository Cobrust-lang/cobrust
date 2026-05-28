# Z.8 REST demo — `examples/z8_rest_blog/`

v0.7.0 §5 网络 MUST-ship 演示落地:一个最小博客 REST 服务,把 **pit(HTTP/axum)+
den(SQLite/rusqlite)+ std.json** 三件 `.cb` 生态接线串起来端到端可演示。

## 状态

**第一证草稿**(2026-05-28),依赖以下三件全部 CI 通绿后可跑:

- ADR-0073 pit "pong" first proof(commit `5153b35` + `8a3e8bf` 已 CI 绿);
- ADR-0072 den first proof(commit `b5b7318`);
- std.json 已通(commit `7f8396e`)。

当前 `main.cb` 是显式 `app.route(...)` 形式;ADR-0074(decorator desugar)接通后,
路由部分会改成 `@app.route("GET", "/posts")` 一行装饰器 — 那个改动 1:1,语义等价。

## 已知限制(等后续 sprint)

1. `cur.fetchall()` 当前返回 **canonical Str rendering** 的行数据,不是真
   `list[tuple]`(ADR-0072 §4 first proof scope)。后续 `den` row→list 改造之后,
   `list_posts` 可以返回真 JSON list。
2. `json.loads` 当前返回 canonical JSON Str(等价 dumps),不是真 `dict[str,any]`。
   `create_post` 因此用 placeholder 字段先证 chain;真字段抓取等结构化 JSON
   接线(配合 coil-deep 类型工作)。
3. 路由参数(`/posts/<id>`)走 `req.path_param("id")` borrow shim,已经有 — 但
   这个 demo 第一版只做 list + create,不做 by-id GET。

## 跑法(等所有依赖在 origin 绿后)

```bash
LLVM_SYS_181_PREFIX=/opt/homebrew/opt/llvm@18 \
  cargo run -p cobrust-cli --bin cobrust -- build \
    examples/z8_rest_blog/main.cb -o /tmp/blog

/tmp/blog &
SERVER=$!

curl -s http://127.0.0.1:8080/posts
curl -s -X POST http://127.0.0.1:8080/posts \
  -H 'Content-Type: application/json' \
  -d '{"title":"hello","body":"world"}'

kill $SERVER
```

## 后续 sprint

- E2E 自动化 harness:`crates/cobrust-cli/tests/z8_rest_blog_e2e.rs` 起一个子进程,
  curl-equivalent 客户端发请求 + 断言。等 #151 RAII tempdir sprint 合进来再加
  (避免 tempdir 模式撞)。
- 真实字段抓取:`json.loads` 返回真结构化 dict 之后,`create_post` 用
  `canon.title` / `canon.body`。
- 装饰器形式重写:ADR-0074 通之后改成 `@app.route("/posts", methods=["GET"])`。
