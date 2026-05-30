# `import fang` —— 在 Cobrust 中做密码哈希与签发 JSON Web Token

> 状态:ADR-0078 后端(backend)Phase 2,第一个后端 Phase-2 crate。
> 继九个 cobra 批次模块(den / nest / strike / scale / molt / pit /
> hood / coil / dora)之后,`fang`(认证/安全工具箱,对 `argon2` 与
> `jsonwebtoken` 两个 crate 的一层安全薄封装)是接到同一条生态导入链路
> 上的**第十个**模块 —— 像 `nest`/`scale` 一样的纯值模式模块,也是链路
> 上**第一个**带 `-> bool` 返回的值函数。它现在提供两套接口:**密码
> 哈希**(argon2id)与 **JSON Web Token**(HS256)。

## 先看例子

```python
import fang

fn main() -> i64:
    let h: str = fang.hash_password("hunter2")
    let ok: bool = fang.verify_password("hunter2", h)
    if ok:
        print(1)
    else:
        print(0)
    return 0
```

编译并运行:

```bash
cobrust build prog.cb -o prog
./prog
# 1
```

## 你能用到什么(Phase-2 首个接口面)

- **`fang.hash_password(pw: str) -> str`** —— 用 **argon2id** 对 `pw`
  做哈希(每次调用都用一个全新的随机盐),返回完整的
  [PHC 字符串](https://github.com/P-H-C/phc-string-format) ——
  即 `$argon2id$v=…$m=…,t=…,p=…$<盐>$<哈希>` 格式,盐和代价参数都内嵌
  在里面。把这整串存下来即可,不需要另外保存任何东西。
- **`fang.verify_password(pw: str, hash: str) -> bool`** —— 当且仅当
  `pw` 正是生成 `hash` 的那个密码时返回 `true`。比较是**常数时间**的。
  密码错误是正常的 `false`,而不是一个你必须去捕获的错误。

## JSON Web Token(HS256)

`fang` 的第二套接口签发并校验 **JSON Web Token** —— 在你的各个服务之间
传递一小撮带签名的声明(claims:用户是谁、令牌何时过期)的标准做法。

```python
import fang

fn main() -> i64:
    let token: str = fang.jwt_encode("{\"sub\":\"alice\"}", "s3cret")
    let ok: bool = fang.jwt_verify(token, "s3cret")
    if ok:
        print(1)
    else:
        print(0)
    return 0
```

```bash
cobrust build prog.cb -o prog
./prog
# 1
```

- **`fang.jwt_encode(claims_json: str, secret: str) -> str`** —— 用
  `secret` 以 **HS256** 对 `claims_json` 里的 JSON 声明对象签名,返回
  紧凑的 `header.payload.signature` 令牌。若 `claims_json` 不是合法
  JSON,你会拿回空字符串(绝不崩溃)。
- **`fang.jwt_verify(token: str, secret: str) -> bool`** —— 当且仅当
  `token` 是用 `secret` 签出的真正 HS256 令牌时返回 `true`。被篡改、
  密钥不对、格式错误或 `alg:none` 的令牌都得到干净的 `false`。
- **`fang.jwt_decode(token: str, secret: str) -> str`** —— 校验
  `token`,若为真令牌则返回其声明 JSON,否则返回空字符串。解码**绝不**
  把一个未通过校验的令牌的声明交给你。

```python
let claims: str = fang.jwt_decode(token, "s3cret")
# claims == "{\"sub\":\"alice\"}"(重新序列化;键顺序可能不同)
# 对伪造 / 被篡改的令牌,claims == ""(空哨兵)
```

这两套接口(上面的密码往返,加上这里的签发/校验令牌)就是目前的全部
接口面:哈希一个密码,再签发并校验一个令牌 —— 把链路自顶向下走通,作为
第一个安全模块的验证。

## 一个真实的登录校验

```python
import fang

fn check_login(stored_hash: str, attempt: str) -> bool:
    return fang.verify_password(attempt, stored_hash)
```

`stored_hash` 就是用户设置密码时 `fang.hash_password` 返回的那串值(把
整串存进你的用户表)。

## 为什么这样设计?(没有认证踩坑)

Cobrust 的生态接口面刻意丢掉了别的语言认证库背负的那些陷阱。`fang`
是一次干净的重新设计,而不是机械搬运:

- **唯一算法就是 argon2id,且把安全默认值焊死。**
  `fang.hash_password` 永远使用 argon2id(OWASP 推荐的密码哈希),并
  采用稳妥的默认参数。Phase 1 **不**暴露任何算法或代价参数旋钮 ——
  所以你不可能误选一个弱哈希(裸 `argon2i`/`argon2d`、无盐的 SHA、过低
  的工作因子)。安全选项是唯一选项。
- **盐就在哈希里。** 返回的 PHC 字符串自带随机盐和参数。没有另一份盐
  需要你去生成、存储或不小心复用 —— 最常见的密码存储 bug 之一在这里
  根本无从发生。
- **校验是常数时间的。** `fang.verify_password` 使用 argon2 的常数时间
  比较,所以时间侧信道无法泄露一次猜测对了多少。
- **密码错误是一个值,而不是异常。** 校验返回 `bool`。不匹配就是普通
  的控制流(`false`),符合 Cobrust「错误不是默认控制路径」的原则。
  你的代码永远不需要把登录校验包进异常处理里。
- **绝不记录明文。** 这层封装从不打印或记录密码与哈希。

### 至于 JWT:算法被钉死(那个经典踩坑,已堵死)

JSON Web Token 有一个臭名昭著的陷阱。令牌的头部自带一个「算法」字段,
而幼稚的校验器会去*信任*它,由此衍生两种攻击:

- **`alg:none`** —— 攻击者发来一个头部写着 `{"alg":"none"}`、签名段留空
  的令牌。一个听从头部的校验器会完全跳过签名校验,接受攻击者写下的
  *任意*声明(管理员、随便谁都行)。这就是 CVE-2015-9235 那一类。
- **算法替换** —— 攻击者拿一个校验 RS256(公钥)令牌的服务,改发一个
  HS256 令牌,诱使校验器把*公*钥当成 HMAC *密钥*来用。

`fang.jwt_verify` / `fang.jwt_decode` **把算法钉死为 HS256**,绝不去看
令牌自己的 `alg` 字段来决定怎样校验。`alg:none` 令牌、RS256 头部的令牌、
被篡改的载荷、错误的密钥,统统返回干净的 `false`(解码则返回空字符串)
—— 这套接口上根本没有任何能关掉签名校验的 API,所以这个踩坑哪怕想不小心
触发都做不到。而且因为 `fang.jwt_encode` 只会签出 HS256,你也无法签出
一个弱令牌。

## 哈希串非法时会怎样?

`fang.verify_password` **永远不会 panic**。如果 `hash` 参数为空或者不是
一个合法的 PHC 字符串,校验直接返回 `false`(它没法匹配)。所以惯用
写法就是:

```python
let ok: bool = fang.verify_password(attempt, stored_hash)
if ok:
    print("welcome")
else:
    print("nope")
```

这与 Cobrust 运行时对所有值模式 shim 的约定一致 —— 优雅失败,绝不
跨边界 panic。

## 为什么同一个密码的两次哈希不一样

把 `fang.hash_password("x")` 跑两遍,你会得到两串**不同**的字符串 ——
每次调用都取一个全新的随机盐。两串都仍然能对 `"x"` 校验为 TRUE。这正是
加盐的全部意义:相同的密码绝不能产生相同的存储哈希,这样即便数据库泄露
也看不出哪些用户用了同一个密码。

```python
let h1: str = fang.hash_password("x")
let h2: str = fang.hash_password("x")
# h1 != h2(盐不同),但两者 verify_password("x", …) 都为 true
```

## 当前的限制

- 把代码包在 `fn main() -> i64:` 里(裸的顶层语句是工具链另一块尚未
  完成的部分)。
- Phase 1 只暴露带默认参数的 argon2id。一个调参接口(按部署调整内存 /
  时间 / 并行度代价,面向慢硬件或高安全等级)是已记录的后续项 —— 把它
  挡在首个接口面之外,正是为了让默认值不会被不小心调弱。
- JWT 只支持 **HS256**(共享密钥)。非对称算法(RS256 / ES256)是已记录
  的后续项 —— 它们会被*加入*被钉死的算法列表,而绝不会取代这个钉死。
- JWT 校验器只校验**签名**;它尚未强制 `exp`(过期)声明,所以一个裸
  `{"sub":"alice"}` 令牌可以往返通过。一个过期策略接口是已记录的后续项。
- `hash_password` / `jwt_encode` / `jwt_decode` 的错误路径(非法输入)是
  空字符串哨兵;带类型的 `Result[str, FangError]` 接口是已记录的后续项。

这些都是已记录在案的后续项,而非死路 —— 这套接线方式从这里就可以推广
到其余安全接口。
