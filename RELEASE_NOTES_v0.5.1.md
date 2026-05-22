# Cobrust v0.5.1 — LLVM backend stdlib I/O hotfix (ADR-0058f)

**Released:** 2026-05-22
**Commits since v0.5.0:** 6
**Tag:** v0.5.1
**Type:** Hotfix

---

## TL;DR

v0.5.0 LLVM backend was **unusable for any program that prints**. v0.5.1
fixes this. **The default Cranelift backend was never affected** — if you
build without `--features llvm`, no action is needed.

If you opted into `--features llvm` and saw `print("hi")` emit nothing,
or `print(fib(N))` compute silently with no integer output: upgrade to
v0.5.1.

---

## What was broken in v0.5.0

The LLVM backend's `--features llvm` AOT path had two stub regions in
`crates/cobrust-codegen/src/llvm_backend.rs`:

- `BodyLowerer::lower_call` — when the callee is an extern stdlib
  intrinsic (`Operand::Constant(Constant::Str(name))`), the call was
  silently dropped and the destination written with `0`.
- `BodyLowerer::lower_constant(Constant::Str | Bytes)` — returned a
  null pointer, leaving every `Str`-typed local null.

Both stubs were marked "Wave-1 stub" with comments referencing
ADR-0058a §8 deferral language. **The Wave-2 closure that ADR-0058a §8
referenced ("sub-ADR 0058a-followup or 0058b") was never written**
before v0.5.0 was tagged. F45 finding files the F35-sibling + F37 +
F44 composite pattern that masked this defect.

User-visible symptoms (only when `--features llvm`):

- `print("hi")` → object built, exe linked, empty stdout.
- `print(fib(40))` → `fib` executed (CPU spun) but no integer printed.
- `let s: str = "..."` → `s` held a null pointer; any subsequent
  consumer (`print(s)`, fn-arg pass-through) saw NULL.

---

## What v0.5.1 ships (ADR-0058f wave-2)

The LLVM backend now mirrors the Cranelift backend's extern-call +
str-materialize ABI for the print system.

### Runtime helpers wired

- `__cobrust_println_int(i64)` — `print(x: i64)`
- `__cobrust_println_bool(i8)` — `print(x: bool)` (i1 → i8 widened at the call site)
- `__cobrust_println_float(f64)` — `print(x: f64)`
- `__cobrust_println_str_buf(*mut Str)` — `print(s: str)` runtime path
- `__cobrust_println(*const u8, usize)` — `print("literal")` legacy path
- `__cobrust_print_no_nl(*mut Str)` + `__cobrust_print_no_nl_lit(ptr, len)`
- `__cobrust_str_new()` + `__cobrust_str_push_static(buf, ptr, len)` (str-buffer subroutines)

### Codegen changes

- New `LlvmEmitter::intern_str_payloads(module)` walks every body's
  Assign rvalues + Call args and registers each unique `Constant::Str`
  payload as a private `unnamed_addr` `[N x i8]` rodata global at
  module level.
- New `BodyLowerer::materialize_str_data(payload) -> (ptr, len)` and
  `materialize_str_buffer(payload) -> *mut Str` (mirror of Cranelift's
  per-EmitCtx fns).
- `lower_call` gains the extern-name dispatch branch with `(ptr, len)`
  arg-expansion for legacy literal-only callees and trailing-Str
  expansion. Inline `build_int_z_extend` for narrow → wider int
  helper params (lights up `__cobrust_println_bool(i8)` from MIR i1).
- `lower_constant(Str | Bytes)` materializes a heap `Str` pointer
  (replaces the wave-1 `const_null` stub).

### Tests

7 new `stdlib_io_*` fixtures in
`crates/cobrust-codegen/tests/codegen_diff_corpus.rs`. Each fixture
emits via LLVM backend, links against `libcobrust_stdlib.a` +
`runtime/cobrust_main.c`, runs the binary, asserts stdout matches a
golden line. All PASS on Mac arm64 + LLVM 18 + libcobrust_stdlib.a.
Pre-fix: 7/7 FAIL (empty stdout). Post-fix: 7/7 PASS.

---

## What is NOT in v0.5.1 — LLVM backend wave-3 stubs (experimental `--features llvm` users only)

**Default user path = Cranelift = full stdlib parity.** The section below
applies ONLY to the `--features llvm` experimental opt-in path. Release
wheels distributed via `cobrust install` or the GitHub release page do NOT
enable `--features llvm`. If you build without that flag, no action is needed
and none of the stubs below affect you.

Per ADR-0058f §7 + F45a playground audit (2026-05-22), the LLVM backend keeps
wave-1 stubs for the following extern callees. Programs that use them compile
under `--features llvm` but emit no observable side effect from those calls.
The **default Cranelift backend handles all of these correctly today**.

| Category | Helpers (silent under `--features llvm`) | Source-level shape |
|---|---|---|
| **input** | `__cobrust_input` / `__cobrust_input_str_buf` / `__cobrust_input_no_prompt` / `__cobrust_read_line` | `input("> ")`, `read_line()` |
| **argv** | `__cobrust_argv` / `__cobrust_capture_argv` | `sys.argv` |
| **list** | `__cobrust_list_new` / `_set` / `_get` / `_append` / `_len` / `_is_empty` | `[1, 2, 3]` construction + access |
| **dict** | `__cobrust_dict_new` / `_set_*` / `_get_*` / `_contains_*` | `{k: v}` construction + access |
| **set / tuple** | `__cobrust_set_*` / `__cobrust_tuple_*` | `{1, 2, 3}` / `(a, b)` |
| **panic** | `__cobrust_panic` | `panic("msg")` / `unwrap_err()` |
| **fmt** | `__cobrust_fmt_*` family | `f"x = {x}"` f-string runtime |
| **iter** | `__cobrust_iter_init` / `_next` / `_drop` | `for x in [1,2,3]` |
| **math** | `__cobrust_math_sqrt` / `_floor` / `_ceil` / `_round` / `_abs` / `_sin` / `_cos` / `_tan` / `_log` / `_exp` / `_pow` | `math.sqrt(2.0)` etc. |
| **parse_int / str parsing** | `__cobrust_parse_int` / `_str_eq` / `_str_at` / `_str_len_src` / `_str_ord` / `_count_toks` / `_parse_int_tok` / `_str_eq_lit` | `int(s)`, `s == t`, `s[i]` |
| **str methods (ADR-0050e)** | `__cobrust_str_split` / `_join` / `_replace` / `_trim` / `_find` / `_contains` / `_starts_with` / `_ends_with` / `_lower` / `_upper` / `_clone` | `s.split(",")` / `.join()` etc. |
| **LLM router** | `__cobrust_llm_complete` / `_llm_dispatch` / `_llm_stream` / `_prompt_*` / `_tool_*` | `cobrust.llm.*` α surface (ADR-0049) |

Wave-3 closure roadmap: [ADR-0058g](docs/agent/adr/0058g-llvm-backend-wave3-stdlib-hookup-roadmap.md)
Full catalogue finding: [F45a](docs/agent/findings/f45a-llvm-backend-wave3-scope-systemic.md)

---

## Honest-cite (F35-sibling)

This release LANDED **stdlib I/O hookup wave-2** for the LLVM backend.
This release does **not** make a "feature-parity with Cranelift" claim.
Other wave-3 surfaces (above) remain wave-1 stubs in the LLVM backend
until subsequent ADRs land their respective mirror work. F45 finding
ratifies the pattern of avoiding cumulative "feature-complete"
language without per-extern surface listing.

---

## Files changed

- ADR: `docs/agent/adr/0058f-llvm-backend-wave2-stdlib-io.md` (new)
- Finding: `docs/agent/findings/f45-llvm-backend-wave1-stub-silently-shipped.md` (new)
- Codegen impl: `crates/cobrust-codegen/src/llvm_backend.rs` (+487 lines)
- Tests: `crates/cobrust-codegen/tests/codegen_diff_corpus.rs` (+377 lines)
- README + README.zh.md (Phase K LLVM backend block updated with honest-cite)
- Skill doc: `docs/agent/skills/cobrust-first-try.md` §9k (new — explicit wave-3 stub list)

---

## Install

```bash
# Option A — cargo install (Rust 1.94+)
cargo install --git https://github.com/Cobrust-lang/cobrust cobrust-cli --tag v0.5.1

# Option B — prebuilt wheel (9 variants; release artifacts will publish on tag)
curl -L https://github.com/Cobrust-lang/cobrust/releases/download/v0.5.1/cobrust-v0.5.1-<variant>.tar.gz | tar xz && sudo mv cobrust /usr/local/bin/
```

---

## Cross-references

- ADR-0058a — wave-1 LLVM backend (the deferral this hotfix closes for the print surface).
- ADR-0058f — wave-2 mirror of Cranelift extern-call + str-materialize.
- F45 — silent-rot finding family (F35-sibling + F37 + F44).
