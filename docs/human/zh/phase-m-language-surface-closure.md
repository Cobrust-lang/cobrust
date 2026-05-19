# Phase M — 语言层语法缺口闭合

Phase M 关闭 ADR-0058a §15 排队的 6 个源码层缺口。5 个缺口通过 3 个增量
子冲刺落地;第 6 个正式标记为超范围。

## 设计动机(LLM-first 设计 ROI)

依 CLAUDE.md §2.5(LLM-first 设计),目标是"LLM agent 写一遍就对的语言"。
Phase M 处理的是 LLM 训练语料中频繁出现的语法形态:

- `i32` / `i8` — Rust + C/C++ 语料的统治形态。
- `-> None` — Python 显式无返回的标准注解。
- 注解位置的 `&T` — Rust 习语;与 ADR-0052a 表达式位置的 `&` 配对。
- `[T; N]` — Rust 定长数组字面量类型。

经验锚点见 `finding:leetcode-corpus-parse-int-tok-use-after-move-fixture-debt
§5.1`:LC-100 压力语料 100 个夹具中有 84 个初次撰写时漏掉 `&`,共需补 226
处机械调用点。将 `&T` 提升到注解位置,使类型签名本身就编码借用契约,降
低 LLM 作者摩擦。

## 交付内容

```mermaid
flowchart LR
    A[ADR-0060<br>Phase M 框架] --> B[ADR-0060a<br>窄整数<br>i8 / i16 / i32]
    A --> C[ADR-0060b<br>语法三元组<br>-> None / &T / [T;N]]
    A --> D[ADR-0060c<br>匿名结构体<br>超范围]
    B --> E[Ty::IntN&lpar;width&rpar;]
    C --> F[Ty::Ref&lpar;inner&rpar;]
    C --> G[Ty::Array&lpar;elem,N&rpar;]
    D --> H[使用元组 / 记录]
```

## ADR-0060a — 窄整数类型

- `Ty::IntN(8 | 16 | 32)` — 与 `Ty::Int`(i64)是不同类型。
- 合一规则:`IntN(a) ⇔ IntN(b)` 当且仅当 `a == b`;**不会**隐式拓宽到
  `Ty::Int`。
- Copy:窄整数为 Copy(不进 drop 调度)。
- LLVM 下译:`i8_type()` / `i16_type()` / `i32_type()`。
- Cranelift 下译:`types::I8` / `types::I16` / `types::I32`。
- DI 下译:折叠到 `DW_ATE_signed` 的 "Int" 条目。

## ADR-0060b — 语法三元组

- **`-> None`** — `parse_type_atom` 入口处接受 `KwNone` token;通过
  既有的 `lower_named_type("None")` 解析为 `Ty::None`。LLVM 后端原
  有的 `Ty::None` → `i64` 回退路径覆盖返回值。
- **注解位 `&T`** — `parse_type_atom` 接受 `&` 前缀;AST
  `TypeKind::Ref(Box<Type>)` 下译到 `Ty::Ref(inner)`。Ref 在 LLVM
  层透明(递归进入 inner,见 `llvm_backend.rs:580`)。
- **`[T; N]`** — `parse_type_atom` 接受 `[` 前缀;AST
  `TypeKind::Array { elem, len: usize }` 下译到 `Ty::Array(elem, n)`。
  LLVM 下译为 `[N x T]` 数组类型;类型发射本次落地,索引留到后续子冲刺。

## ADR-0060c — 超范围备忘

匿名结构体字面量 `struct{T, U}` 不会加入。请用:

- `(T, U)` 元组类型(位置访问)。
- `class Foo: x: T`(命名字段访问)。

两者都已下译为 LLVM struct 类型;加入第三种拼写违反 CLAUDE.md §5.1
"一件事只有一种做法"。

## 留待后续(诚实债务)

三份 finding 记录 Phase M 的 wave-2 后续:

1. `finding:adr0060a-binop-on-intn-narrow-int-debt` — 窄整数上的 BinOp
   与字面量类型推导。Cast-surface 子冲刺将添加
   `(IntN(w), IntN(w)) -> IntN(w)` 算术 arm + 字面量大小检查。
2. `finding:adr0060b-array-indexing-mir-projection-debt` — `[T; N]`
   的 `a[0]` 索引。需扩展 typeck `NotIndexable` 谓词 + MIR `Place::index`
   + LLVM GEP。
3. `finding:adr0060b-empty-dict-annotation-k-flow-debt` — 空 `{}` 字面
   量遇到非 Hashable K 的 dict 注解。优先级 P3(非空生产路径已正确拒绝)。

## 验证(DG `1ff7921`)

- 5 / 5 缺口夹具 GREEN:
  - `llvm_type_02_i32`(窄整数签名)
  - `llvm_type_03_i8`(窄整数签名)
  - `llvm_type_06_none_return`(`-> None` 解析 + 下译)
  - `llvm_type_08_array_i64`(数组类型发射)
  - `llvm_operand_06_deref_ptr`(`&i64` 注解透传)
- 17 / 17 Phase M 解析器语料 GREEN。
- 11 / 14 Phase M 类型检查语料 GREEN;3 个按 F37 诚实 `#[ignore]`
  并附 finding 交叉引用。
- Phase H/I/J/K/L 基线零回归(DG 全套件确认)。
