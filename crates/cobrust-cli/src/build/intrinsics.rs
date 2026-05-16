//! Print-intrinsic rewrite — M11 supersedes M10's narrowed contract.
//!
//! Per ADR-0024 §"Hello-world contract" (M10) the rewrite was narrowed
//! to the literal `"hello, world"` only; ADR-0025 §"Print-intrinsic
//! lift" lifts that narrowing — any `print(<string-literal>)` callsite
//! is rewritten to a runtime call into `__cobrust_println`, with the
//! literal argument preserved so codegen can emit a
//! `(*const u8, usize)` C-ABI call (per ADR-0025 §"Codegen amendments"
//! Constant::Str row + ADR-0025 §"Runtime ABI").
//!
//! ADR-0030 §Decision step 5 adds `print_int(n: i64)` — any
//! `print_int(<i64-operand>)` callsite is rewritten to
//! `__cobrust_println_int`, which takes the integer directly. This
//! avoids the string-formatting overhead for the bare-number FizzBuzz
//! `else` branch.
//!
//! The diagnostic `IntrinsicError::M10ScopeNarrowed` from M10 is
//! deleted; M11 accepts any string-literal argument. Non-literal
//! arguments to `print` (other than via `print_int`) emit
//! `IntrinsicError::PrintArgUnsupported` — they are M11.x scope (full
//! HIR-tier dispatch through stdlib FnRefs).
//!
//! Body removal: the prelude's `print` / `print_int` stub Bodies are
//! dropped from the MIR after the rewrite. Per ADR-0024 §"Consequences"
//! the M8 drop schedule for an unmoved `s: str` parameter is unsound;
//! ADR-0025 §"Drop-schedule fix" notes that the drop_eligible filter
//! exempts parameters (cobrust-mir/src/drop.rs:45) so the prelude body
//! would lower fine — but the body still has zero statements (a
//! `return 0` prelude that lowers to a stub) which produces a well-formed
//! but useless MIR Body. Dropping it is cleaner.

use std::collections::HashSet;

use cobrust_mir::{Constant, Module, Operand, Terminator};

/// Runtime symbol providing `__cobrust_println(*const u8, usize)`.
/// Per ADR-0025 §"Runtime ABI" this is exported by `cobrust-stdlib`.
pub const PRINTLN_RUNTIME_SYMBOL: &str = "__cobrust_println";

/// Runtime symbol providing `__cobrust_println_int(i64)`.
/// Per ADR-0030 §Decision step 5 — exported by `cobrust-stdlib`.
pub const PRINTLN_INT_RUNTIME_SYMBOL: &str = "__cobrust_println_int";

/// Runtime symbol providing `__cobrust_println_str_buf(*mut Str)` —
/// ADR-0044 W2 Phase 2 fallback for `print(s)` when `s` is a non-
/// literal heap-buffer str. Extracts (ptr, len) at runtime and
/// dispatches to `__cobrust_println`.
pub const PRINTLN_STR_BUF_RUNTIME_SYMBOL: &str = "__cobrust_println_str_buf";

/// Runtime symbol for source-level `input(prompt: str) -> str` when the
/// prompt is lowered as raw `(ptr, len)` bytes.
/// ADR-0044 W2 Phase 2 — exported by `cobrust-stdlib::io`.
pub const INPUT_RUNTIME_SYMBOL: &str = "__cobrust_input";

/// Runtime symbol for source-level `input(prompt: str) -> str` when the
/// prompt is a runtime `Str` buffer.
pub const INPUT_STR_BUF_RUNTIME_SYMBOL: &str = "__cobrust_input_str_buf";

/// Runtime symbol for source-level `input_no_prompt() -> str`.
/// ADR-0044 W2 Phase 2 — exported by `cobrust-stdlib::io`.
pub const INPUT_NO_PROMPT_RUNTIME_SYMBOL: &str = "__cobrust_input_no_prompt";

/// Runtime symbol for source-level `read_line() -> str` (W2 cap;
/// typed `Result[str, IoError]` deferred to ADR-0044a). Exported by
/// `cobrust-stdlib::io`.
pub const READ_LINE_RUNTIME_SYMBOL: &str = "__cobrust_read_line";

/// Runtime symbol for source-level `argv() -> list[str]`.
/// ADR-0044 W2 Phase 2 — exported by `cobrust-stdlib::env`.
pub const ARGV_RUNTIME_SYMBOL: &str = "__cobrust_argv";

/// Runtime symbol for source-level `parse_int(s: str) -> i64`.
/// ADR-0044 W2 Phase 3 — exported by `cobrust-stdlib::io`.
pub const PARSE_INT_RUNTIME_SYMBOL: &str = "__cobrust_parse_int";

/// Runtime symbol for source-level `str_len(s: str) -> i64`.
/// ADR-0044 W2 Phase 3 — exported by `cobrust-stdlib::io`.
pub const STR_LEN_RUNTIME_SYMBOL: &str = "__cobrust_str_len_src";

/// Runtime symbol for source-level `str_at(s: str, i: i64) -> str`.
/// ADR-0044 W2 Phase 3 — exported by `cobrust-stdlib::io`.
pub const STR_AT_RUNTIME_SYMBOL: &str = "__cobrust_str_at";

/// Runtime symbol for source-level `str_eq(a: str, b: str) -> i64`.
/// ADR-0044 W2 Phase 3 — exported by `cobrust-stdlib::io`.
pub const STR_EQ_RUNTIME_SYMBOL: &str = "__cobrust_str_eq";

/// Runtime symbol for source-level `str_eq_lit(s: str, lit: str) -> i64`.
/// Uses 3-param C ABI: (buf_ptr, lit_ptr, lit_len). Handles the case
/// where the second arg is a string literal known at compile time.
/// ADR-0044 W2 Phase 3.
pub const STR_EQ_LIT_RUNTIME_SYMBOL: &str = "__cobrust_str_eq_lit";

/// Runtime symbol for source-level `str_ord(s: str) -> i64`.
/// Returns ASCII byte value of first byte in the Str buffer.
/// ADR-0044 W2 Phase 3.
pub const STR_ORD_RUNTIME_SYMBOL: &str = "__cobrust_str_ord";

/// Runtime symbol for source-level `parse_int_tok(line: str, i: i64) -> i64`.
/// ADR-0044 W2 Phase 3.
pub const PARSE_INT_TOK_RUNTIME_SYMBOL: &str = "__cobrust_parse_int_tok";

/// Runtime symbol for source-level `count_toks(line: str) -> i64`.
/// ADR-0044 W2 Phase 3.
pub const COUNT_TOKS_RUNTIME_SYMBOL: &str = "__cobrust_count_toks";

/// Runtime symbol for source-level `list_set(lst, i, v)`.
/// Wraps `__cobrust_list_set`. ADR-0044 W2 Phase 3.
pub const LIST_SET_RUNTIME_SYMBOL: &str = "__cobrust_list_set";

/// Runtime symbol for source-level `list_get(lst, i)`.
/// Wraps `__cobrust_list_get`. ADR-0044 W2 Phase 3.
pub const LIST_GET_RUNTIME_SYMBOL: &str = "__cobrust_list_get";

/// Runtime symbol for source-level `list_len(lst)`.
/// Wraps `__cobrust_list_len`. ADR-0044 W2 Phase 3.
pub const LIST_LEN_RUNTIME_SYMBOL: &str = "__cobrust_list_len";

/// Runtime symbol for source-level `list_is_empty(lst)`.
/// Wraps `__cobrust_list_is_empty`. ADR-0050c §F5 / Phase 6 —
/// §2.2 implicit-truthy ban: returns `bool` at the source level
/// while the C-ABI returns `i64` (0/1) to match the SwitchInt
/// convention. Symmetric to `__cobrust_dict_is_empty` from
/// ADR-0050d Decision 5 addendum.
pub const LIST_IS_EMPTY_RUNTIME_SYMBOL: &str = "__cobrust_list_is_empty";

/// Runtime symbol for source-level `dict_is_empty(d)`.
/// Wraps `__cobrust_dict_is_empty`. ADR-0050d Decision 5 addendum —
/// §2.2 implicit-truthy ban: returns `bool` at the source level
/// while the C-ABI returns `i64` (0/1) to match the SwitchInt
/// convention. Mirrors [`LIST_IS_EMPTY_RUNTIME_SYMBOL`].
pub const DICT_IS_EMPTY_RUNTIME_SYMBOL: &str = "__cobrust_dict_is_empty";

/// Runtime symbol for source-level `list_new(capacity)`.
/// Wraps `__cobrust_list_new`. ADR-0044 W2 Phase 3.
pub const LIST_NEW_RUNTIME_SYMBOL: &str = "__cobrust_list_new";

/// Runtime symbol for source-level `print_no_nl(s: str)`.
/// Prints Str buffer without trailing newline. ADR-0044 W2 Phase 3.
pub const PRINT_NO_NL_RUNTIME_SYMBOL: &str = "__cobrust_print_no_nl";

/// Runtime symbol for source-level `print_no_nl(<string literal>)` —
/// the raw-bytes `(ptr, len)` variant introduced by ADR-0047 Option H
/// to close LC-100 Pattern A (`.rodata` literal misalignment in the
/// `StringBuffer` cast). Intrinsic-rewrite routes
/// `Constant::Str` arguments here; runtime-str arguments continue
/// to use [`PRINT_NO_NL_RUNTIME_SYMBOL`].
pub const PRINT_NO_NL_LIT_RUNTIME_SYMBOL: &str = "__cobrust_print_no_nl_lit";

/// Runtime symbol for source-level `llm_complete(provider, model, prompt) -> str`.
/// M-AI.0 (α Phase 2 ADR-0048 + spike 705f592) — exported by `cobrust-stdlib::llm`.
pub const LLM_COMPLETE_RUNTIME_SYMBOL: &str = "__cobrust_llm_complete";

/// Runtime symbol for source-level `llm_dispatch(task, prompt) -> str`.
/// M-AI.0 (α Phase 2) — exported by `cobrust-stdlib::llm`.
pub const LLM_DISPATCH_RUNTIME_SYMBOL: &str = "__cobrust_llm_dispatch";

/// Runtime symbol for source-level `llm_stream(provider, model, prompt) -> list[str]`.
/// M-AI.0 (α Phase 2) — exported by `cobrust-stdlib::llm`.
pub const LLM_STREAM_RUNTIME_SYMBOL: &str = "__cobrust_llm_stream";

/// Runtime symbol for source-level `prompt_render(system, user, vars) -> str`.
/// M-AI.1 (α Phase 3 ADR-0048 + spike m-ai-1) — exported by `cobrust-stdlib::prompt`.
pub const PROMPT_RENDER_RUNTIME_SYMBOL: &str = "__cobrust_prompt_render";

/// Runtime symbol for source-level `prompt_format_few_shot(examples_in, examples_out, current_input) -> str`.
/// M-AI.1 (α Phase 3) — exported by `cobrust-stdlib::prompt`.
pub const PROMPT_FORMAT_FEW_SHOT_RUNTIME_SYMBOL: &str = "__cobrust_prompt_format_few_shot";

/// Runtime symbol for source-level `prompt_format_system_user(system, user) -> str`.
/// M-AI.1 (α Phase 3) — exported by `cobrust-stdlib::prompt`.
pub const PROMPT_FORMAT_SYSTEM_USER_RUNTIME_SYMBOL: &str = "__cobrust_prompt_format_system_user";

/// Runtime symbol for source-level `prompt_escape_braces(text) -> str`.
/// M-AI.1 (α Phase 3) — exported by `cobrust-stdlib::prompt`.
pub const PROMPT_ESCAPE_BRACES_RUNTIME_SYMBOL: &str = "__cobrust_prompt_escape_braces";

/// Runtime symbol for source-level `llm_complete_structured(prompt, schema_json) -> str`.
/// M-AI.1 (α Phase 3) — exported by `cobrust-stdlib::prompt` (gated by `llm-router` feature).
pub const LLM_COMPLETE_STRUCTURED_RUNTIME_SYMBOL: &str = "__cobrust_llm_complete_structured";

/// Runtime symbol for source-level `tool_schema(name, description, parameters_json, return_type) -> str`.
/// M-AI.2 (α Phase 4) — exported by `cobrust-stdlib::tool`.
pub const TOOL_SCHEMA_RUNTIME_SYMBOL: &str = "__cobrust_tool_schema";

/// Runtime symbol for source-level `tool_registry_new() -> str`.
/// M-AI.2 (α Phase 4) — exported by `cobrust-stdlib::tool`.
pub const TOOL_REGISTRY_NEW_RUNTIME_SYMBOL: &str = "__cobrust_tool_registry_new";

/// Runtime symbol for source-level `tool_registry_register(registry_json, schema_json) -> str`.
/// M-AI.2 (α Phase 4) — exported by `cobrust-stdlib::tool`.
pub const TOOL_REGISTRY_REGISTER_RUNTIME_SYMBOL: &str = "__cobrust_tool_registry_register";

/// Runtime symbol for source-level `tool_invoke(tool_name, args_json) -> str`.
/// M-AI.2 (α Phase 4) — exported by `cobrust-stdlib::tool`.
pub const TOOL_INVOKE_RUNTIME_SYMBOL: &str = "__cobrust_tool_invoke";

/// Runtime symbol for source-level `llm_complete_with_tools(prompt, registry_json) -> str`.
/// M-AI.2 (α Phase 4) — exported by `cobrust-stdlib::tool`.
pub const LLM_COMPLETE_WITH_TOOLS_RUNTIME_SYMBOL: &str = "__cobrust_llm_complete_with_tools";

// ---- M-F.3.3 gap (b): math intrinsic symbols ----------------------------

/// `sqrt(x: f64) -> f64` → `__cobrust_math_sqrt(f64) -> f64`.
pub const MATH_SQRT_RUNTIME_SYMBOL: &str = "__cobrust_math_sqrt";
/// `floor(x: f64) -> f64` → `__cobrust_math_floor(f64) -> f64`.
pub const MATH_FLOOR_RUNTIME_SYMBOL: &str = "__cobrust_math_floor";
/// `ceil(x: f64) -> f64` → `__cobrust_math_ceil(f64) -> f64`.
pub const MATH_CEIL_RUNTIME_SYMBOL: &str = "__cobrust_math_ceil";
/// `round(x: f64) -> f64` → `__cobrust_math_round(f64) -> f64`.
pub const MATH_ROUND_RUNTIME_SYMBOL: &str = "__cobrust_math_round";
/// `abs(x: f64) -> f64` → `__cobrust_math_abs(f64) -> f64`.
pub const MATH_ABS_RUNTIME_SYMBOL: &str = "__cobrust_math_abs";
/// `pow(base: f64, exp: f64) -> f64` → `__cobrust_math_pow(f64, f64) -> f64`.
pub const MATH_POW_RUNTIME_SYMBOL: &str = "__cobrust_math_pow";
/// `sin(x: f64) -> f64` → `__cobrust_math_sin(f64) -> f64`.
pub const MATH_SIN_RUNTIME_SYMBOL: &str = "__cobrust_math_sin";
/// `cos(x: f64) -> f64` → `__cobrust_math_cos(f64) -> f64`.
pub const MATH_COS_RUNTIME_SYMBOL: &str = "__cobrust_math_cos";
/// `tan(x: f64) -> f64` → `__cobrust_math_tan(f64) -> f64`.
pub const MATH_TAN_RUNTIME_SYMBOL: &str = "__cobrust_math_tan";
/// `log(x: f64) -> f64` → `__cobrust_math_log(f64) -> f64`.
pub const MATH_LOG_RUNTIME_SYMBOL: &str = "__cobrust_math_log";
/// `exp(x: f64) -> f64` → `__cobrust_math_exp(f64) -> f64`.
pub const MATH_EXP_RUNTIME_SYMBOL: &str = "__cobrust_math_exp";

// ---- M-F.3.5 string stdlib (ADR-0050e) -------------------------------
/// M-F.3.5 — `split(s: str, sep: str) -> list[str]`.
pub const STR_SPLIT_RUNTIME_SYMBOL: &str = "__cobrust_str_split";
/// M-F.3.5 — `join(parts: list[str], sep: str) -> str`.
pub const STR_JOIN_RUNTIME_SYMBOL: &str = "__cobrust_str_join";
/// M-F.3.5 — `replace(s: str, old: str, new: str) -> str`.
pub const STR_REPLACE_RUNTIME_SYMBOL: &str = "__cobrust_str_replace";
/// M-F.3.5 — `trim(s: str) -> str`.
pub const STR_TRIM_RUNTIME_SYMBOL: &str = "__cobrust_str_trim";
/// M-F.3.5 — `find(s: str, needle: str) -> i64` (-1 sentinel).
pub const STR_FIND_RUNTIME_SYMBOL: &str = "__cobrust_str_find";
/// M-F.3.5 — `contains(s: str, needle: str) -> bool` (i64 0/1 at ABI).
pub const STR_CONTAINS_RUNTIME_SYMBOL: &str = "__cobrust_str_contains";
/// M-F.3.5 — `starts_with(s: str, prefix: str) -> bool`.
pub const STR_STARTS_WITH_RUNTIME_SYMBOL: &str = "__cobrust_str_starts_with";
/// M-F.3.5 — `ends_with(s: str, suffix: str) -> bool`.
pub const STR_ENDS_WITH_RUNTIME_SYMBOL: &str = "__cobrust_str_ends_with";
/// M-F.3.5 — `lower(s: str) -> str` (ASCII-fast Unicode-aware per Rust stdlib).
pub const STR_LOWER_RUNTIME_SYMBOL: &str = "__cobrust_str_lower";
/// M-F.3.5 — `upper(s: str) -> str`.
pub const STR_UPPER_RUNTIME_SYMBOL: &str = "__cobrust_str_upper";
/// M-F.3.5 — `clone(s: str) -> str` (LC-100 honest-debt mitigation;
/// shim already ships at `crates/cobrust-stdlib/src/fmt.rs:306`).
pub const STR_CLONE_RUNTIME_SYMBOL: &str = "__cobrust_str_clone";

// ---- M-F.3.6 file IO completion (ADR-0050f) --------------------------

/// M-F.3.6 — `read_file(path: str) -> str`.
/// Reads entire file as UTF-8; empty on error (i64-sentinel Q1).
pub const READ_FILE_RUNTIME_SYMBOL: &str = "__cobrust_read_file";

/// M-F.3.6 — `read_file_lines(path: str) -> list[str]`.
/// Splits file into newline-stripped lines per ADR-0050f Q2.
pub const READ_FILE_LINES_RUNTIME_SYMBOL: &str = "__cobrust_read_file_lines";

/// M-F.3.6 — `write_file(path: str, contents: str) -> i64`.
/// Creates or truncates; returns 0 on success, 1 on I/O error.
pub const WRITE_FILE_RUNTIME_SYMBOL: &str = "__cobrust_write_file";

/// M-F.3.6 — `append_file(path: str, contents: str) -> i64`.
/// Creates if absent, appends if present; returns 0/1 sentinel.
pub const APPEND_FILE_RUNTIME_SYMBOL: &str = "__cobrust_append_file";

/// M-F.3.6 — `stdin_read_all() -> str`.
/// Reads stdin until EOF; returns empty Str on EOF.
pub const STDIN_READ_ALL_RUNTIME_SYMBOL: &str = "__cobrust_stdin_read_all";

/// M-F.3.6 — `stdout_write(s: str) -> i64`.
/// Writes `s` to stdout WITHOUT trailing newline; returns 0/1 sentinel.
pub const STDOUT_WRITE_RUNTIME_SYMBOL: &str = "__cobrust_stdout_write";

/// M-F.3.6 — `stderr_write(s: str) -> i64`.
/// Writes `s` to stderr WITHOUT trailing newline; returns 0/1 sentinel.
pub const STDERR_WRITE_RUNTIME_SYMBOL: &str = "__cobrust_stderr_write";

/// Errors from the print-intrinsic rewrite.
#[derive(Debug, thiserror::Error)]
pub enum IntrinsicError {
    #[error(
        "M11: `print` accepts a string literal at this milestone; got {found}. \
         Non-literal arguments require full stdlib FnRef dispatch (M11.x scope, ADR-0025)."
    )]
    PrintArgUnsupported { found: String },
}

/// Recognized prelude intrinsics by name. Collected from `Module.bodies`
/// so the rewrite pass can both (a) identify FnRef callees that should
/// be redirected to runtime symbols and (b) drop the stub bodies after
/// rewrite (since no callsite references them anymore).
struct IntrinsicDefIds {
    print: HashSet<u32>,
    print_int: HashSet<u32>,
    /// ADR-0044 W2 Phase 2.
    input: HashSet<u32>,
    /// ADR-0044 W2 Phase 2.
    input_no_prompt: HashSet<u32>,
    /// ADR-0044 W2 Phase 2.
    read_line: HashSet<u32>,
    /// ADR-0044 W2 Phase 2.
    argv: HashSet<u32>,
    /// ADR-0044 W2 Phase 3.
    parse_int: HashSet<u32>,
    /// ADR-0044 W2 Phase 3.
    str_len: HashSet<u32>,
    /// ADR-0044 W2 Phase 3.
    str_at: HashSet<u32>,
    /// ADR-0044 W2 Phase 3.
    str_eq: HashSet<u32>,
    /// ADR-0044 W2 Phase 3.
    str_eq_lit: HashSet<u32>,
    /// ADR-0044 W2 Phase 3.
    str_ord: HashSet<u32>,
    /// ADR-0044 W2 Phase 3.
    parse_int_tok: HashSet<u32>,
    /// ADR-0044 W2 Phase 3.
    count_toks: HashSet<u32>,
    /// ADR-0044 W2 Phase 3.
    list_set: HashSet<u32>,
    /// ADR-0044 W2 Phase 3.
    list_get: HashSet<u32>,
    /// ADR-0044 W2 Phase 3.
    list_len: HashSet<u32>,
    /// ADR-0050c §F5 / Phase 6 — §2.2 implicit-truthy ban for lists.
    list_is_empty: HashSet<u32>,
    /// ADR-0050d Decision 5 addendum — §2.2 implicit-truthy ban for dicts.
    dict_is_empty: HashSet<u32>,
    /// ADR-0044 W2 Phase 3.
    list_new: HashSet<u32>,
    /// ADR-0044 W2 Phase 3.
    print_no_nl: HashSet<u32>,
    /// M-AI.0 (α Phase 2).
    llm_complete: HashSet<u32>,
    /// M-AI.0 (α Phase 2).
    llm_dispatch: HashSet<u32>,
    /// M-AI.0 (α Phase 2).
    llm_stream: HashSet<u32>,
    /// M-AI.1 (α Phase 3).
    prompt_render: HashSet<u32>,
    /// M-AI.1 (α Phase 3).
    prompt_format_few_shot: HashSet<u32>,
    /// M-AI.1 (α Phase 3).
    prompt_format_system_user: HashSet<u32>,
    /// M-AI.1 (α Phase 3).
    prompt_escape_braces: HashSet<u32>,
    /// M-AI.1 (α Phase 3).
    llm_complete_structured: HashSet<u32>,
    /// M-AI.2 (α Phase 4).
    tool_schema: HashSet<u32>,
    /// M-AI.2 (α Phase 4).
    tool_registry_new: HashSet<u32>,
    /// M-AI.2 (α Phase 4).
    tool_registry_register: HashSet<u32>,
    /// M-AI.2 (α Phase 4).
    tool_invoke: HashSet<u32>,
    /// M-AI.2 (α Phase 4).
    llm_complete_with_tools: HashSet<u32>,
    // ---- M-F.3.3 gap (b): math intrinsics ----
    /// M-F.3.3.
    math_sqrt: HashSet<u32>,
    /// M-F.3.3.
    math_floor: HashSet<u32>,
    /// M-F.3.3.
    math_ceil: HashSet<u32>,
    /// M-F.3.3.
    math_round: HashSet<u32>,
    /// M-F.3.3.
    math_abs: HashSet<u32>,
    /// M-F.3.3.
    math_pow: HashSet<u32>,
    /// M-F.3.3.
    math_sin: HashSet<u32>,
    /// M-F.3.3.
    math_cos: HashSet<u32>,
    /// M-F.3.3.
    math_tan: HashSet<u32>,
    /// M-F.3.3.
    math_log: HashSet<u32>,
    /// M-F.3.3.
    math_exp: HashSet<u32>,
    // ---- M-F.3.5 string stdlib (ADR-0050e) ----
    /// M-F.3.5.
    str_split: HashSet<u32>,
    /// M-F.3.5.
    str_join: HashSet<u32>,
    /// M-F.3.5.
    str_replace: HashSet<u32>,
    /// M-F.3.5.
    str_trim: HashSet<u32>,
    /// M-F.3.5.
    str_find: HashSet<u32>,
    /// M-F.3.5.
    str_contains: HashSet<u32>,
    /// M-F.3.5.
    str_starts_with: HashSet<u32>,
    /// M-F.3.5.
    str_ends_with: HashSet<u32>,
    /// M-F.3.5.
    str_lower: HashSet<u32>,
    /// M-F.3.5.
    str_upper: HashSet<u32>,
    /// M-F.3.5 — LC-100 honest-debt mitigation.
    str_clone: HashSet<u32>,
    // ---- M-F.3.6 file IO completion (ADR-0050f) ----
    /// M-F.3.6 — `read_file(path: str) -> str`.
    read_file: HashSet<u32>,
    /// M-F.3.6 — `read_file_lines(path: str) -> list[str]`.
    read_file_lines: HashSet<u32>,
    /// M-F.3.6 — `write_file(path: str, contents: str) -> i64`.
    write_file: HashSet<u32>,
    /// M-F.3.6 — `append_file(path: str, contents: str) -> i64`.
    append_file: HashSet<u32>,
    /// M-F.3.6 — `stdin_read_all() -> str`.
    stdin_read_all: HashSet<u32>,
    /// M-F.3.6 — `stdout_write(s: str) -> i64`.
    stdout_write: HashSet<u32>,
    /// M-F.3.6 — `stderr_write(s: str) -> i64`.
    stderr_write: HashSet<u32>,
}

impl IntrinsicDefIds {
    fn all(&self) -> HashSet<u32> {
        let mut out = HashSet::new();
        out.extend(&self.print);
        out.extend(&self.print_int);
        out.extend(&self.input);
        out.extend(&self.input_no_prompt);
        out.extend(&self.read_line);
        out.extend(&self.argv);
        out.extend(&self.parse_int);
        out.extend(&self.str_len);
        out.extend(&self.str_at);
        out.extend(&self.str_eq);
        out.extend(&self.str_eq_lit);
        out.extend(&self.str_ord);
        out.extend(&self.parse_int_tok);
        out.extend(&self.count_toks);
        out.extend(&self.list_set);
        out.extend(&self.list_get);
        out.extend(&self.list_len);
        out.extend(&self.list_is_empty);
        out.extend(&self.dict_is_empty);
        out.extend(&self.list_new);
        out.extend(&self.print_no_nl);
        out.extend(&self.llm_complete);
        out.extend(&self.llm_dispatch);
        out.extend(&self.llm_stream);
        out.extend(&self.prompt_render);
        out.extend(&self.prompt_format_few_shot);
        out.extend(&self.prompt_format_system_user);
        out.extend(&self.prompt_escape_braces);
        out.extend(&self.llm_complete_structured);
        out.extend(&self.tool_schema);
        out.extend(&self.tool_registry_new);
        out.extend(&self.tool_registry_register);
        out.extend(&self.tool_invoke);
        out.extend(&self.llm_complete_with_tools);
        out.extend(&self.math_sqrt);
        out.extend(&self.math_floor);
        out.extend(&self.math_ceil);
        out.extend(&self.math_round);
        out.extend(&self.math_abs);
        out.extend(&self.math_pow);
        out.extend(&self.math_sin);
        out.extend(&self.math_cos);
        out.extend(&self.math_tan);
        out.extend(&self.math_log);
        out.extend(&self.math_exp);
        // M-F.3.5 string stdlib.
        out.extend(&self.str_split);
        out.extend(&self.str_join);
        out.extend(&self.str_replace);
        out.extend(&self.str_trim);
        out.extend(&self.str_find);
        out.extend(&self.str_contains);
        out.extend(&self.str_starts_with);
        out.extend(&self.str_ends_with);
        out.extend(&self.str_lower);
        out.extend(&self.str_upper);
        out.extend(&self.str_clone);
        // M-F.3.6 file IO completion.
        out.extend(&self.read_file);
        out.extend(&self.read_file_lines);
        out.extend(&self.write_file);
        out.extend(&self.append_file);
        out.extend(&self.stdin_read_all);
        out.extend(&self.stdout_write);
        out.extend(&self.stderr_write);
        out
    }

    fn is_empty(&self) -> bool {
        self.print.is_empty()
            && self.print_int.is_empty()
            && self.input.is_empty()
            && self.input_no_prompt.is_empty()
            && self.read_line.is_empty()
            && self.argv.is_empty()
            && self.parse_int.is_empty()
            && self.str_len.is_empty()
            && self.str_at.is_empty()
            && self.str_eq.is_empty()
            && self.str_eq_lit.is_empty()
            && self.str_ord.is_empty()
            && self.parse_int_tok.is_empty()
            && self.count_toks.is_empty()
            && self.list_set.is_empty()
            && self.list_get.is_empty()
            && self.list_len.is_empty()
            && self.list_is_empty.is_empty()
            && self.dict_is_empty.is_empty()
            && self.list_new.is_empty()
            && self.print_no_nl.is_empty()
            && self.llm_complete.is_empty()
            && self.llm_dispatch.is_empty()
            && self.llm_stream.is_empty()
            && self.prompt_render.is_empty()
            && self.prompt_format_few_shot.is_empty()
            && self.prompt_format_system_user.is_empty()
            && self.prompt_escape_braces.is_empty()
            && self.llm_complete_structured.is_empty()
            && self.tool_schema.is_empty()
            && self.tool_registry_new.is_empty()
            && self.tool_registry_register.is_empty()
            && self.tool_invoke.is_empty()
            && self.llm_complete_with_tools.is_empty()
            && self.math_sqrt.is_empty()
            && self.math_floor.is_empty()
            && self.math_ceil.is_empty()
            && self.math_round.is_empty()
            && self.math_abs.is_empty()
            && self.math_pow.is_empty()
            && self.math_sin.is_empty()
            && self.math_cos.is_empty()
            && self.math_tan.is_empty()
            && self.math_log.is_empty()
            && self.math_exp.is_empty()
            && self.str_split.is_empty()
            && self.str_join.is_empty()
            && self.str_replace.is_empty()
            && self.str_trim.is_empty()
            && self.str_find.is_empty()
            && self.str_contains.is_empty()
            && self.str_starts_with.is_empty()
            && self.str_ends_with.is_empty()
            && self.str_lower.is_empty()
            && self.str_upper.is_empty()
            && self.str_clone.is_empty()
            && self.read_file.is_empty()
            && self.read_file_lines.is_empty()
            && self.write_file.is_empty()
            && self.append_file.is_empty()
            && self.stdin_read_all.is_empty()
            && self.stdout_write.is_empty()
            && self.stderr_write.is_empty()
    }
}

/// Identify Body def_ids whose name matches a recognized prelude
/// intrinsic — print / print_int (M11) + ADR-0044 W2 Phase 2 stdin/argv
/// surface (input / input_no_prompt / read_line / argv).
fn collect_print_def_ids(module: &Module) -> IntrinsicDefIds {
    let mut ids = IntrinsicDefIds {
        print: HashSet::new(),
        print_int: HashSet::new(),
        input: HashSet::new(),
        input_no_prompt: HashSet::new(),
        read_line: HashSet::new(),
        argv: HashSet::new(),
        parse_int: HashSet::new(),
        str_len: HashSet::new(),
        str_at: HashSet::new(),
        str_eq: HashSet::new(),
        str_eq_lit: HashSet::new(),
        str_ord: HashSet::new(),
        parse_int_tok: HashSet::new(),
        count_toks: HashSet::new(),
        list_set: HashSet::new(),
        list_get: HashSet::new(),
        list_len: HashSet::new(),
        list_is_empty: HashSet::new(),
        dict_is_empty: HashSet::new(),
        list_new: HashSet::new(),
        print_no_nl: HashSet::new(),
        llm_complete: HashSet::new(),
        llm_dispatch: HashSet::new(),
        llm_stream: HashSet::new(),
        prompt_render: HashSet::new(),
        prompt_format_few_shot: HashSet::new(),
        prompt_format_system_user: HashSet::new(),
        prompt_escape_braces: HashSet::new(),
        llm_complete_structured: HashSet::new(),
        tool_schema: HashSet::new(),
        tool_registry_new: HashSet::new(),
        tool_registry_register: HashSet::new(),
        tool_invoke: HashSet::new(),
        llm_complete_with_tools: HashSet::new(),
        math_sqrt: HashSet::new(),
        math_floor: HashSet::new(),
        math_ceil: HashSet::new(),
        math_round: HashSet::new(),
        math_abs: HashSet::new(),
        math_pow: HashSet::new(),
        math_sin: HashSet::new(),
        math_cos: HashSet::new(),
        math_tan: HashSet::new(),
        math_log: HashSet::new(),
        math_exp: HashSet::new(),
        // M-F.3.5 string stdlib.
        str_split: HashSet::new(),
        str_join: HashSet::new(),
        str_replace: HashSet::new(),
        str_trim: HashSet::new(),
        str_find: HashSet::new(),
        str_contains: HashSet::new(),
        str_starts_with: HashSet::new(),
        str_ends_with: HashSet::new(),
        str_lower: HashSet::new(),
        str_upper: HashSet::new(),
        str_clone: HashSet::new(),
        // M-F.3.6 file IO completion.
        read_file: HashSet::new(),
        read_file_lines: HashSet::new(),
        write_file: HashSet::new(),
        append_file: HashSet::new(),
        stdin_read_all: HashSet::new(),
        stdout_write: HashSet::new(),
        stderr_write: HashSet::new(),
    };
    // Track names already collected to detect user-defined shadowing of
    // PRELUDE stubs (M-F.3.3). For non-math intrinsics (print, parse_int,
    // etc.) any duplicate is a user bug and is ignored. For math intrinsics
    // (sqrt, pow, etc.) — if there are TWO bodies with the same name, the
    // FIRST is the PRELUDE stub (inserted first) and the SECOND is a
    // user-defined function. Only the PRELUDE stub should be in the
    // intrinsic set; the user's function must NOT be rewritten.
    let mut math_names_seen: HashSet<&'static str> = HashSet::new();
    // M-F.3.5 string stdlib (ADR-0050e): same first-body-wins guard
    // because names like `clone` / `find` / `trim` / `lower` / `upper`
    // are common enough that user-defined functions may collide.
    let mut str_names_seen: HashSet<&'static str> = HashSet::new();

    for body in &module.bodies {
        match body.name.as_str() {
            "print" => {
                ids.print.insert(body.def_id.0);
            }
            "print_int" => {
                ids.print_int.insert(body.def_id.0);
            }
            "input" => {
                ids.input.insert(body.def_id.0);
            }
            "input_no_prompt" => {
                ids.input_no_prompt.insert(body.def_id.0);
            }
            "read_line" => {
                ids.read_line.insert(body.def_id.0);
            }
            "argv" => {
                ids.argv.insert(body.def_id.0);
            }
            "parse_int" => {
                ids.parse_int.insert(body.def_id.0);
            }
            "str_len" => {
                ids.str_len.insert(body.def_id.0);
            }
            "str_at" => {
                ids.str_at.insert(body.def_id.0);
            }
            "str_eq" => {
                ids.str_eq.insert(body.def_id.0);
            }
            "str_eq_lit" => {
                ids.str_eq_lit.insert(body.def_id.0);
            }
            "str_ord" => {
                ids.str_ord.insert(body.def_id.0);
            }
            "parse_int_tok" => {
                ids.parse_int_tok.insert(body.def_id.0);
            }
            "count_toks" => {
                ids.count_toks.insert(body.def_id.0);
            }
            "list_set" => {
                ids.list_set.insert(body.def_id.0);
            }
            "list_get" => {
                ids.list_get.insert(body.def_id.0);
            }
            "list_len" => {
                ids.list_len.insert(body.def_id.0);
            }
            "list_is_empty" => {
                ids.list_is_empty.insert(body.def_id.0);
            }
            "dict_is_empty" => {
                ids.dict_is_empty.insert(body.def_id.0);
            }
            "list_new" => {
                ids.list_new.insert(body.def_id.0);
            }
            "print_no_nl" => {
                ids.print_no_nl.insert(body.def_id.0);
            }
            "llm_complete" => {
                ids.llm_complete.insert(body.def_id.0);
            }
            "llm_dispatch" => {
                ids.llm_dispatch.insert(body.def_id.0);
            }
            "llm_stream" => {
                ids.llm_stream.insert(body.def_id.0);
            }
            "prompt_render" => {
                ids.prompt_render.insert(body.def_id.0);
            }
            "prompt_format_few_shot" => {
                ids.prompt_format_few_shot.insert(body.def_id.0);
            }
            "prompt_format_system_user" => {
                ids.prompt_format_system_user.insert(body.def_id.0);
            }
            "prompt_escape_braces" => {
                ids.prompt_escape_braces.insert(body.def_id.0);
            }
            "llm_complete_structured" => {
                ids.llm_complete_structured.insert(body.def_id.0);
            }
            "tool_schema" => {
                ids.tool_schema.insert(body.def_id.0);
            }
            "tool_registry_new" => {
                ids.tool_registry_new.insert(body.def_id.0);
            }
            "tool_registry_register" => {
                ids.tool_registry_register.insert(body.def_id.0);
            }
            "tool_invoke" => {
                ids.tool_invoke.insert(body.def_id.0);
            }
            "llm_complete_with_tools" => {
                ids.llm_complete_with_tools.insert(body.def_id.0);
            }
            // M-F.3.3 gap (b): math intrinsics.
            // Only collect the FIRST body with each name — that is
            // always the PRELUDE stub. If a user defines their own
            // function with the same name (e.g. `fn pow(...)`), the
            // second body must NOT be added: the user's function
            // should be compiled normally, not rewritten to the math
            // C-ABI shim. The scope-shadowing fix in cobrust-hir/scope.rs
            // already makes the user's definition win for call resolution.
            "sqrt" => {
                if math_names_seen.insert("sqrt") {
                    ids.math_sqrt.insert(body.def_id.0);
                }
            }
            "floor" => {
                if math_names_seen.insert("floor") {
                    ids.math_floor.insert(body.def_id.0);
                }
            }
            "ceil" => {
                if math_names_seen.insert("ceil") {
                    ids.math_ceil.insert(body.def_id.0);
                }
            }
            "round" => {
                if math_names_seen.insert("round") {
                    ids.math_round.insert(body.def_id.0);
                }
            }
            "abs" => {
                if math_names_seen.insert("abs") {
                    ids.math_abs.insert(body.def_id.0);
                }
            }
            "pow" => {
                if math_names_seen.insert("pow") {
                    ids.math_pow.insert(body.def_id.0);
                }
            }
            "sin" => {
                if math_names_seen.insert("sin") {
                    ids.math_sin.insert(body.def_id.0);
                }
            }
            "cos" => {
                if math_names_seen.insert("cos") {
                    ids.math_cos.insert(body.def_id.0);
                }
            }
            "tan" => {
                if math_names_seen.insert("tan") {
                    ids.math_tan.insert(body.def_id.0);
                }
            }
            "log" => {
                if math_names_seen.insert("log") {
                    ids.math_log.insert(body.def_id.0);
                }
            }
            "exp" => {
                if math_names_seen.insert("exp") {
                    ids.math_exp.insert(body.def_id.0);
                }
            }
            // ---- M-F.3.5 string stdlib (ADR-0050e) ----
            "split" => {
                if str_names_seen.insert("split") {
                    ids.str_split.insert(body.def_id.0);
                }
            }
            "join" => {
                if str_names_seen.insert("join") {
                    ids.str_join.insert(body.def_id.0);
                }
            }
            "replace" => {
                if str_names_seen.insert("replace") {
                    ids.str_replace.insert(body.def_id.0);
                }
            }
            "trim" => {
                if str_names_seen.insert("trim") {
                    ids.str_trim.insert(body.def_id.0);
                }
            }
            "find" => {
                if str_names_seen.insert("find") {
                    ids.str_find.insert(body.def_id.0);
                }
            }
            "contains" => {
                if str_names_seen.insert("contains") {
                    ids.str_contains.insert(body.def_id.0);
                }
            }
            "starts_with" => {
                if str_names_seen.insert("starts_with") {
                    ids.str_starts_with.insert(body.def_id.0);
                }
            }
            "ends_with" => {
                if str_names_seen.insert("ends_with") {
                    ids.str_ends_with.insert(body.def_id.0);
                }
            }
            "lower" => {
                if str_names_seen.insert("lower") {
                    ids.str_lower.insert(body.def_id.0);
                }
            }
            "upper" => {
                if str_names_seen.insert("upper") {
                    ids.str_upper.insert(body.def_id.0);
                }
            }
            "clone" => {
                if str_names_seen.insert("clone") {
                    ids.str_clone.insert(body.def_id.0);
                }
            }
            // ---- M-F.3.6 file IO completion (ADR-0050f) ----
            "read_file" => {
                ids.read_file.insert(body.def_id.0);
            }
            "read_file_lines" => {
                ids.read_file_lines.insert(body.def_id.0);
            }
            "write_file" => {
                ids.write_file.insert(body.def_id.0);
            }
            "append_file" => {
                ids.append_file.insert(body.def_id.0);
            }
            "stdin_read_all" => {
                ids.stdin_read_all.insert(body.def_id.0);
            }
            "stdout_write" => {
                ids.stdout_write.insert(body.def_id.0);
            }
            "stderr_write" => {
                ids.stderr_write.insert(body.def_id.0);
            }
            _ => {}
        }
    }
    ids
}

/// Rewrite every prelude-intrinsic callsite (print / print_int +
/// ADR-0044 W2 Phase 2 input / input_no_prompt / read_line / argv) to
/// the appropriate runtime helpers.
///
/// - `print(s: str)` — string-literal arg → `__cobrust_println(ptr, len)`
///   via the M11 extern_funcs path.
/// - `print(s: str)` — non-literal arg → `__cobrust_println(ptr, len)`
///   with the runtime-helper `(ptr, len)` expansion handled by codegen
///   (heap-buffer pointer source → buffer ptr/len extracted at runtime).
///   *(Today this path errors via `PrintArgUnsupported`; future work
///   adds a `__cobrust_println_str_buf` shim or amends codegen's
///   runtime_funcs lowering.)*
/// - `print_int(n: i64)` → `__cobrust_println_int(n)`.
/// - `input(prompt: str)` — string-literal prompt →
///   `__cobrust_input(prompt_ptr, prompt_len)`.
/// - `input(prompt: str)` — non-literal prompt buffer →
///   `__cobrust_input_str_buf(prompt_buf)`.
/// - `input_no_prompt()` → `__cobrust_input_no_prompt()`.
/// - `read_line()` → `__cobrust_read_line()` (W2 cap; typed `Result`
///   surface deferred to ADR-0044a).
/// - `argv()` → `__cobrust_argv()`.
///
/// # Errors
///
/// Returns [`IntrinsicError::PrintArgUnsupported`] if any callsite has
/// a wrong arg count or a non-supported argument shape.
/// ADR-0044 W2 Phase 2: dispatch category for each intrinsic
/// callsite. Hoisted to module scope (out of `rewrite_print`'s body)
/// to satisfy clippy::items_after_statements.
#[derive(Copy, Clone, Eq, PartialEq)]
enum Kind {
    Print,
    PrintInt,
    Input,
    InputNoPrompt,
    ReadLine,
    Argv,
    ParseInt,
    StrLen,
    StrAt,
    StrEq,
    StrEqLit,
    StrOrd,
    ParseIntTok,
    CountToks,
    ListSet,
    ListGet,
    ListLen,
    ListIsEmpty,
    DictIsEmpty,
    ListNew,
    PrintNoNl,
    LlmComplete,
    LlmDispatch,
    LlmStream,
    PromptRender,
    PromptFormatFewShot,
    PromptFormatSystemUser,
    PromptEscapeBraces,
    LlmCompleteStructured,
    ToolSchema,
    ToolRegistryNew,
    ToolRegistryRegister,
    ToolInvoke,
    LlmCompleteWithTools,
    // ---- M-F.3.3 gap (b): math intrinsics ----
    MathSqrt,
    MathFloor,
    MathCeil,
    MathRound,
    MathAbs,
    MathPow,
    MathSin,
    MathCos,
    MathTan,
    MathLog,
    MathExp,
    // ---- M-F.3.5 string stdlib (ADR-0050e) ----
    StrSplit,
    StrJoin,
    StrReplace,
    StrTrim,
    StrFind,
    StrContains,
    StrStartsWith,
    StrEndsWith,
    StrLower,
    StrUpper,
    StrClone,
    // ---- M-F.3.6 file IO completion (ADR-0050f) ----
    ReadFile,
    ReadFileLines,
    WriteFile,
    AppendFile,
    StdinReadAll,
    StdoutWrite,
    StderrWrite,
}

fn kind_for_name(name: &str) -> Option<Kind> {
    match name {
        "print" => Some(Kind::Print),
        "print_int" => Some(Kind::PrintInt),
        "input" => Some(Kind::Input),
        "input_no_prompt" => Some(Kind::InputNoPrompt),
        "read_line" => Some(Kind::ReadLine),
        "argv" => Some(Kind::Argv),
        "parse_int" => Some(Kind::ParseInt),
        "str_len" => Some(Kind::StrLen),
        "str_at" => Some(Kind::StrAt),
        "str_eq" => Some(Kind::StrEq),
        "str_eq_lit" => Some(Kind::StrEqLit),
        "str_ord" => Some(Kind::StrOrd),
        "parse_int_tok" => Some(Kind::ParseIntTok),
        "count_toks" => Some(Kind::CountToks),
        "list_set" => Some(Kind::ListSet),
        "list_get" => Some(Kind::ListGet),
        "list_len" => Some(Kind::ListLen),
        "list_is_empty" => Some(Kind::ListIsEmpty),
        "dict_is_empty" => Some(Kind::DictIsEmpty),
        "list_new" => Some(Kind::ListNew),
        "print_no_nl" => Some(Kind::PrintNoNl),
        "llm_complete" => Some(Kind::LlmComplete),
        "llm_dispatch" => Some(Kind::LlmDispatch),
        "llm_stream" => Some(Kind::LlmStream),
        "prompt_render" => Some(Kind::PromptRender),
        "prompt_format_few_shot" => Some(Kind::PromptFormatFewShot),
        "prompt_format_system_user" => Some(Kind::PromptFormatSystemUser),
        "prompt_escape_braces" => Some(Kind::PromptEscapeBraces),
        "llm_complete_structured" => Some(Kind::LlmCompleteStructured),
        "tool_schema" => Some(Kind::ToolSchema),
        "tool_registry_new" => Some(Kind::ToolRegistryNew),
        "tool_registry_register" => Some(Kind::ToolRegistryRegister),
        "tool_invoke" => Some(Kind::ToolInvoke),
        "llm_complete_with_tools" => Some(Kind::LlmCompleteWithTools),
        // M-F.3.3 gap (b): math intrinsics.
        "sqrt" => Some(Kind::MathSqrt),
        "floor" => Some(Kind::MathFloor),
        "ceil" => Some(Kind::MathCeil),
        "round" => Some(Kind::MathRound),
        "abs" => Some(Kind::MathAbs),
        "pow" => Some(Kind::MathPow),
        "sin" => Some(Kind::MathSin),
        "cos" => Some(Kind::MathCos),
        "tan" => Some(Kind::MathTan),
        "log" => Some(Kind::MathLog),
        "exp" => Some(Kind::MathExp),
        // M-F.3.5 string stdlib (ADR-0050e).
        "split" => Some(Kind::StrSplit),
        "join" => Some(Kind::StrJoin),
        "replace" => Some(Kind::StrReplace),
        "trim" => Some(Kind::StrTrim),
        "find" => Some(Kind::StrFind),
        "contains" => Some(Kind::StrContains),
        "starts_with" => Some(Kind::StrStartsWith),
        "ends_with" => Some(Kind::StrEndsWith),
        "lower" => Some(Kind::StrLower),
        "upper" => Some(Kind::StrUpper),
        "clone" => Some(Kind::StrClone),
        // M-F.3.6 file IO completion (ADR-0050f).
        "read_file" => Some(Kind::ReadFile),
        "read_file_lines" => Some(Kind::ReadFileLines),
        "write_file" => Some(Kind::WriteFile),
        "append_file" => Some(Kind::AppendFile),
        "stdin_read_all" => Some(Kind::StdinReadAll),
        "stdout_write" => Some(Kind::StdoutWrite),
        "stderr_write" => Some(Kind::StderrWrite),
        _ => None,
    }
}

fn kind_for_def_id(ids: &IntrinsicDefIds, id: u32) -> Option<Kind> {
    if ids.print.contains(&id) {
        Some(Kind::Print)
    } else if ids.print_int.contains(&id) {
        Some(Kind::PrintInt)
    } else if ids.input.contains(&id) {
        Some(Kind::Input)
    } else if ids.input_no_prompt.contains(&id) {
        Some(Kind::InputNoPrompt)
    } else if ids.read_line.contains(&id) {
        Some(Kind::ReadLine)
    } else if ids.argv.contains(&id) {
        Some(Kind::Argv)
    } else if ids.parse_int.contains(&id) {
        Some(Kind::ParseInt)
    } else if ids.str_len.contains(&id) {
        Some(Kind::StrLen)
    } else if ids.str_at.contains(&id) {
        Some(Kind::StrAt)
    } else if ids.str_eq.contains(&id) {
        Some(Kind::StrEq)
    } else if ids.str_eq_lit.contains(&id) {
        Some(Kind::StrEqLit)
    } else if ids.str_ord.contains(&id) {
        Some(Kind::StrOrd)
    } else if ids.parse_int_tok.contains(&id) {
        Some(Kind::ParseIntTok)
    } else if ids.count_toks.contains(&id) {
        Some(Kind::CountToks)
    } else if ids.list_set.contains(&id) {
        Some(Kind::ListSet)
    } else if ids.list_get.contains(&id) {
        Some(Kind::ListGet)
    } else if ids.list_len.contains(&id) {
        Some(Kind::ListLen)
    } else if ids.list_is_empty.contains(&id) {
        Some(Kind::ListIsEmpty)
    } else if ids.dict_is_empty.contains(&id) {
        Some(Kind::DictIsEmpty)
    } else if ids.list_new.contains(&id) {
        Some(Kind::ListNew)
    } else if ids.print_no_nl.contains(&id) {
        Some(Kind::PrintNoNl)
    } else if ids.llm_complete.contains(&id) {
        Some(Kind::LlmComplete)
    } else if ids.llm_dispatch.contains(&id) {
        Some(Kind::LlmDispatch)
    } else if ids.llm_stream.contains(&id) {
        Some(Kind::LlmStream)
    } else if ids.prompt_render.contains(&id) {
        Some(Kind::PromptRender)
    } else if ids.prompt_format_few_shot.contains(&id) {
        Some(Kind::PromptFormatFewShot)
    } else if ids.prompt_format_system_user.contains(&id) {
        Some(Kind::PromptFormatSystemUser)
    } else if ids.prompt_escape_braces.contains(&id) {
        Some(Kind::PromptEscapeBraces)
    } else if ids.llm_complete_structured.contains(&id) {
        Some(Kind::LlmCompleteStructured)
    } else if ids.tool_schema.contains(&id) {
        Some(Kind::ToolSchema)
    } else if ids.tool_registry_new.contains(&id) {
        Some(Kind::ToolRegistryNew)
    } else if ids.tool_registry_register.contains(&id) {
        Some(Kind::ToolRegistryRegister)
    } else if ids.tool_invoke.contains(&id) {
        Some(Kind::ToolInvoke)
    } else if ids.llm_complete_with_tools.contains(&id) {
        Some(Kind::LlmCompleteWithTools)
    } else if ids.math_sqrt.contains(&id) {
        Some(Kind::MathSqrt)
    } else if ids.math_floor.contains(&id) {
        Some(Kind::MathFloor)
    } else if ids.math_ceil.contains(&id) {
        Some(Kind::MathCeil)
    } else if ids.math_round.contains(&id) {
        Some(Kind::MathRound)
    } else if ids.math_abs.contains(&id) {
        Some(Kind::MathAbs)
    } else if ids.math_pow.contains(&id) {
        Some(Kind::MathPow)
    } else if ids.math_sin.contains(&id) {
        Some(Kind::MathSin)
    } else if ids.math_cos.contains(&id) {
        Some(Kind::MathCos)
    } else if ids.math_tan.contains(&id) {
        Some(Kind::MathTan)
    } else if ids.math_log.contains(&id) {
        Some(Kind::MathLog)
    } else if ids.math_exp.contains(&id) {
        Some(Kind::MathExp)
    } else if ids.str_split.contains(&id) {
        Some(Kind::StrSplit)
    } else if ids.str_join.contains(&id) {
        Some(Kind::StrJoin)
    } else if ids.str_replace.contains(&id) {
        Some(Kind::StrReplace)
    } else if ids.str_trim.contains(&id) {
        Some(Kind::StrTrim)
    } else if ids.str_find.contains(&id) {
        Some(Kind::StrFind)
    } else if ids.str_contains.contains(&id) {
        Some(Kind::StrContains)
    } else if ids.str_starts_with.contains(&id) {
        Some(Kind::StrStartsWith)
    } else if ids.str_ends_with.contains(&id) {
        Some(Kind::StrEndsWith)
    } else if ids.str_lower.contains(&id) {
        Some(Kind::StrLower)
    } else if ids.str_upper.contains(&id) {
        Some(Kind::StrUpper)
    } else if ids.str_clone.contains(&id) {
        Some(Kind::StrClone)
    } else if ids.read_file.contains(&id) {
        Some(Kind::ReadFile)
    } else if ids.read_file_lines.contains(&id) {
        Some(Kind::ReadFileLines)
    } else if ids.write_file.contains(&id) {
        Some(Kind::WriteFile)
    } else if ids.append_file.contains(&id) {
        Some(Kind::AppendFile)
    } else if ids.stdin_read_all.contains(&id) {
        Some(Kind::StdinReadAll)
    } else if ids.stdout_write.contains(&id) {
        Some(Kind::StdoutWrite)
    } else if ids.stderr_write.contains(&id) {
        Some(Kind::StderrWrite)
    } else {
        None
    }
}

/// Convert `Operand::Move(place)` → `Operand::Copy(place)`.
///
/// Used by M-F.3.6 file-IO intrinsic dispatch to adopt the ADR-0050c
/// Phase 2a "Copy-at-operand" discipline for Str arguments whose C-ABI
/// shim only borrows the pointer (reads via `str_buf_as_str_phase3`)
/// without freeing it. Ownership of the named local stays with the
/// caller's scope; the drop schedule handles freeing at scope exit.
///
/// This mirrors how `list_set(xs, i, v)` passes `xs` as a shared
/// borrow — the list is `is_copy_type = true` at the operand level so
/// the local is not consumed by the call. File-IO shims adopt the same
/// convention for their `path` / `contents` / `s` arguments.
///
/// Constant operands are returned unchanged (they're immortal literals
/// with no local to upgrade).
fn move_to_copy(op: Operand) -> Operand {
    match op {
        Operand::Move(place) => Operand::Copy(place),
        other => other,
    }
}

pub fn rewrite_print(module: &mut Module) -> Result<(), IntrinsicError> {
    let ids = collect_print_def_ids(module);
    if ids.is_empty() {
        return Ok(());
    }
    let all_stub_ids = ids.all();

    for body in &mut module.bodies {
        if all_stub_ids.contains(&body.def_id.0) {
            continue;
        }
        // Build local-id → name map for this body (used to detect Place-
        // variant callees that point at a prelude stub by name).
        let mut local_name: std::collections::HashMap<u32, String> =
            std::collections::HashMap::new();
        for decl in &body.locals {
            local_name.insert(decl.id.0, decl.name.clone());
        }

        for block in &mut body.blocks {
            let term = &mut block.terminator;
            let Terminator::Call {
                func,
                args,
                destination: _,
                target: _,
                unwind: _,
            } = term
            else {
                continue;
            };
            let kind = match func {
                Operand::Copy(p) | Operand::Move(p) if p.projections.is_empty() => {
                    local_name.get(&p.local.0).and_then(|n| kind_for_name(n))
                }
                Operand::Constant(Constant::FnRef(d)) => kind_for_def_id(&ids, *d),
                _ => None,
            };
            let Some(kind) = kind else { continue };

            match kind {
                Kind::Print => {
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("{} arg(s)", args.len()),
                        });
                    }
                    match &args[0] {
                        Operand::Constant(Constant::Str(s)) => {
                            // Literal path: codegen materialises (ptr, len)
                            // from the runtime helper's `(*const u8, usize)`
                            // signature via the runtime_funcs single-Str-to-
                            // (ptr, len) expansion (ADR-0044 codegen
                            // amendment).
                            let lit = s.clone();
                            *func = Operand::Constant(Constant::Str(
                                PRINTLN_RUNTIME_SYMBOL.to_string(),
                            ));
                            args.clear();
                            args.push(Operand::Constant(Constant::Str(lit)));
                        }
                        _ => {
                            // Non-literal path (ADR-0044 W2 Phase 2 wedge):
                            // route to `__cobrust_println_str_buf` which
                            // takes the heap Str buffer pointer directly.
                            let arg = args[0].clone();
                            *func = Operand::Constant(Constant::Str(
                                PRINTLN_STR_BUF_RUNTIME_SYMBOL.to_string(),
                            ));
                            args.clear();
                            args.push(arg);
                        }
                    }
                }
                Kind::PrintInt => {
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("print_int: expected 1 arg, got {}", args.len()),
                        });
                    }
                    let int_arg = args[0].clone();
                    *func =
                        Operand::Constant(Constant::Str(PRINTLN_INT_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(int_arg);
                }
                Kind::Input => {
                    // input(prompt: str)
                    //   - string-literal prompt → __cobrust_input(ptr, len)
                    //   - runtime Str buffer   → __cobrust_input_str_buf(buf)
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("input: expected 1 arg, got {}", args.len()),
                        });
                    }
                    let prompt = args[0].clone();
                    *func = match &args[0] {
                        Operand::Constant(Constant::Str(_)) => {
                            Operand::Constant(Constant::Str(INPUT_RUNTIME_SYMBOL.to_string()))
                        }
                        _ => Operand::Constant(Constant::Str(
                            INPUT_STR_BUF_RUNTIME_SYMBOL.to_string(),
                        )),
                    };
                    args.clear();
                    args.push(prompt);
                }
                Kind::InputNoPrompt => {
                    if !args.is_empty() {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("input_no_prompt: expected 0 args, got {}", args.len()),
                        });
                    }
                    *func = Operand::Constant(Constant::Str(
                        INPUT_NO_PROMPT_RUNTIME_SYMBOL.to_string(),
                    ));
                    args.clear();
                }
                Kind::ReadLine => {
                    if !args.is_empty() {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("read_line: expected 0 args, got {}", args.len()),
                        });
                    }
                    *func = Operand::Constant(Constant::Str(READ_LINE_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                }
                Kind::Argv => {
                    if !args.is_empty() {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("argv: expected 0 args, got {}", args.len()),
                        });
                    }
                    *func = Operand::Constant(Constant::Str(ARGV_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                }
                Kind::ParseInt => {
                    // parse_int(s: str) -> i64 → __cobrust_parse_int(buf_ptr)
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("parse_int: expected 1 arg, got {}", args.len()),
                        });
                    }
                    let str_arg = args[0].clone();
                    *func = Operand::Constant(Constant::Str(PARSE_INT_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(str_arg);
                }
                Kind::StrLen => {
                    // str_len(s: str) -> i64 → __cobrust_str_len_src(buf_ptr)
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("str_len: expected 1 arg, got {}", args.len()),
                        });
                    }
                    let str_arg = args[0].clone();
                    *func = Operand::Constant(Constant::Str(STR_LEN_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(str_arg);
                }
                Kind::StrAt => {
                    // str_at(s: str, i: i64) -> str → __cobrust_str_at(buf_ptr, i)
                    if args.len() != 2 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("str_at: expected 2 args, got {}", args.len()),
                        });
                    }
                    let str_arg = args[0].clone();
                    let idx_arg = args[1].clone();
                    *func = Operand::Constant(Constant::Str(STR_AT_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(str_arg);
                    args.push(idx_arg);
                }
                Kind::StrEq => {
                    // str_eq(a: str, b: str) -> i64 → __cobrust_str_eq(a_ptr, b_ptr)
                    if args.len() != 2 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("str_eq: expected 2 args, got {}", args.len()),
                        });
                    }
                    let a_arg = args[0].clone();
                    let b_arg = args[1].clone();
                    *func = Operand::Constant(Constant::Str(STR_EQ_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(a_arg);
                    args.push(b_arg);
                }
                Kind::StrEqLit => {
                    // str_eq_lit(s: str, lit: str) -> i64
                    // → __cobrust_str_eq_lit(buf_ptr, lit_ptr, lit_len)
                    // The literal arg is a Constant::Str which codegen will
                    // expand to (ptr, len) via the single-arg expansion when
                    // arg count == 1 and sig count == 2. For str_eq_lit, the
                    // literal is arg[1] in a 2-arg call with a 3-param C sig.
                    // We rewrite to pass s + lit as 2 source args; codegen
                    // will see a 2-arg call to a 3-param function and expand
                    // the Constant::Str literal arg to (ptr, len).
                    if args.len() != 2 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("str_eq_lit: expected 2 args, got {}", args.len()),
                        });
                    }
                    let s_arg = args[0].clone();
                    let lit_arg = args[1].clone();
                    *func = Operand::Constant(Constant::Str(STR_EQ_LIT_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(s_arg);
                    args.push(lit_arg);
                }
                Kind::StrOrd => {
                    // str_ord(s: str) -> i64 → __cobrust_str_ord(buf_ptr)
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("str_ord: expected 1 arg, got {}", args.len()),
                        });
                    }
                    let str_arg = args[0].clone();
                    *func = Operand::Constant(Constant::Str(STR_ORD_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(str_arg);
                }
                Kind::ParseIntTok => {
                    // parse_int_tok(line: str, i: i64) -> i64
                    if args.len() != 2 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("parse_int_tok: expected 2 args, got {}", args.len()),
                        });
                    }
                    let line_arg = args[0].clone();
                    let idx_arg = args[1].clone();
                    *func =
                        Operand::Constant(Constant::Str(PARSE_INT_TOK_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(line_arg);
                    args.push(idx_arg);
                }
                Kind::CountToks => {
                    // count_toks(line: str) -> i64
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("count_toks: expected 1 arg, got {}", args.len()),
                        });
                    }
                    let line_arg = args[0].clone();
                    *func = Operand::Constant(Constant::Str(COUNT_TOKS_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(line_arg);
                }
                Kind::ListSet => {
                    // list_set(lst, i, v) → __cobrust_list_set(lst_ptr, i, v)
                    if args.len() != 3 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("list_set: expected 3 args, got {}", args.len()),
                        });
                    }
                    let lst = args[0].clone();
                    let idx = args[1].clone();
                    let val = args[2].clone();
                    *func = Operand::Constant(Constant::Str(LIST_SET_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(lst);
                    args.push(idx);
                    args.push(val);
                }
                Kind::ListGet => {
                    // list_get(lst, i) → __cobrust_list_get(lst_ptr, i) -> i64
                    if args.len() != 2 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("list_get: expected 2 args, got {}", args.len()),
                        });
                    }
                    let lst = args[0].clone();
                    let idx = args[1].clone();
                    *func = Operand::Constant(Constant::Str(LIST_GET_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(lst);
                    args.push(idx);
                }
                Kind::ListLen => {
                    // list_len(lst) → __cobrust_list_len(lst_ptr) -> i64
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("list_len: expected 1 arg, got {}", args.len()),
                        });
                    }
                    let lst = args[0].clone();
                    *func = Operand::Constant(Constant::Str(LIST_LEN_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(lst);
                }
                Kind::ListIsEmpty => {
                    // ADR-0050c §F5 / Phase 6 — §2.2 implicit-truthy ban.
                    // list_is_empty(lst) -> bool → __cobrust_list_is_empty(lst_ptr) -> i64.
                    // The C-ABI returns i64 (0/1) matching the SwitchInt convention;
                    // Cranelift sees a 1-byte bool slot fed by an i64-returning call
                    // (truncation handled by codegen's i64-to-bool coerce path).
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("list_is_empty: expected 1 arg, got {}", args.len()),
                        });
                    }
                    let lst = args[0].clone();
                    *func =
                        Operand::Constant(Constant::Str(LIST_IS_EMPTY_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(lst);
                }
                Kind::DictIsEmpty => {
                    // ADR-0050d Decision 5 addendum — §2.2 implicit-truthy ban.
                    // dict_is_empty(d) -> bool → __cobrust_dict_is_empty(d_ptr) -> i64.
                    // The C-ABI returns i64 (0/1) per the SwitchInt convention;
                    // Cranelift truncates to bool through the same coerce path as
                    // `list_is_empty`. Row-polymorphic on (K, V) via the
                    // `is_list_polymorphic_intrinsic_name` widening at the type
                    // checker (so any `Dict[K, V]` argument unifies).
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("dict_is_empty: expected 1 arg, got {}", args.len()),
                        });
                    }
                    let d = args[0].clone();
                    *func =
                        Operand::Constant(Constant::Str(DICT_IS_EMPTY_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(d);
                }
                Kind::PrintNoNl => {
                    // print_no_nl(s: str) — operand-aware dispatch
                    // (ADR-0047 Option H / LC-100 Pattern A fix):
                    //   - `Constant::Str` literal → __cobrust_print_no_nl_lit
                    //     (raw `(ptr, len)`; codegen's 1-arg-Str-to-(ptr,len)
                    //     expansion fires because the runtime sig has 2 params).
                    //   - non-literal runtime str → __cobrust_print_no_nl
                    //     (existing StringBuffer-pointer entry; safe for
                    //     heap-allocated str buffers from input/read_line/etc.).
                    // Mirrors `Kind::Print` (line ~473-505) which already uses
                    // this two-symbol pattern for `__cobrust_println` vs.
                    // `__cobrust_println_str_buf`.
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("print_no_nl: expected 1 arg, got {}", args.len()),
                        });
                    }
                    match &args[0] {
                        Operand::Constant(Constant::Str(s)) => {
                            // Literal path: codegen expands the single
                            // Constant::Str arg to a `(*const u8, usize)`
                            // pair against the 2-param sig of
                            // __cobrust_print_no_nl_lit. No StringBuffer
                            // cast — closes the `.rodata` alignment defect.
                            let lit = s.clone();
                            *func = Operand::Constant(Constant::Str(
                                PRINT_NO_NL_LIT_RUNTIME_SYMBOL.to_string(),
                            ));
                            args.clear();
                            args.push(Operand::Constant(Constant::Str(lit)));
                        }
                        _ => {
                            // Runtime-str path: forward the heap-buffer
                            // pointer to the existing single-param entry.
                            let str_arg = args[0].clone();
                            *func = Operand::Constant(Constant::Str(
                                PRINT_NO_NL_RUNTIME_SYMBOL.to_string(),
                            ));
                            args.clear();
                            args.push(str_arg);
                        }
                    }
                }
                Kind::ListNew => {
                    // list_new(len) → __cobrust_list_new(0, len) -> list_ptr
                    // Signature: __cobrust_list_new(_elem_size: i64, len: i64).
                    // Pass 0 for elem_size (M12.x ignores it), user-supplied
                    // len as the pre-allocated capacity+length.
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("list_new: expected 1 arg, got {}", args.len()),
                        });
                    }
                    let len = args[0].clone();
                    *func = Operand::Constant(Constant::Str(LIST_NEW_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    // First arg is _elem_size (0 = ignored), second is len.
                    args.push(Operand::Constant(Constant::Int(0)));
                    args.push(len);
                }
                Kind::LlmComplete => {
                    // llm_complete(provider, model, prompt) -> str
                    // → __cobrust_llm_complete(p_ptr, m_ptr, q_ptr) -> *mut u8
                    // All three Str args remain pointer-only; no expansion.
                    if args.len() != 3 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("llm_complete: expected 3 args, got {}", args.len()),
                        });
                    }
                    let p = args[0].clone();
                    let m = args[1].clone();
                    let q = args[2].clone();
                    *func =
                        Operand::Constant(Constant::Str(LLM_COMPLETE_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(p);
                    args.push(m);
                    args.push(q);
                }
                Kind::LlmDispatch => {
                    // llm_dispatch(task, prompt) -> str
                    // → __cobrust_llm_dispatch(t_ptr, q_ptr) -> *mut u8
                    if args.len() != 2 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("llm_dispatch: expected 2 args, got {}", args.len()),
                        });
                    }
                    let t = args[0].clone();
                    let q = args[1].clone();
                    *func =
                        Operand::Constant(Constant::Str(LLM_DISPATCH_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(t);
                    args.push(q);
                }
                Kind::LlmStream => {
                    // llm_stream(provider, model, prompt) -> list[str]
                    // → __cobrust_llm_stream(p_ptr, m_ptr, q_ptr) -> *mut u8
                    if args.len() != 3 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("llm_stream: expected 3 args, got {}", args.len()),
                        });
                    }
                    let p = args[0].clone();
                    let m = args[1].clone();
                    let q = args[2].clone();
                    *func = Operand::Constant(Constant::Str(LLM_STREAM_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(p);
                    args.push(m);
                    args.push(q);
                }
                Kind::PromptRender => {
                    // prompt_render(system, user, vars) -> str
                    // → __cobrust_prompt_render(sys_ptr, usr_ptr, vars_list_ptr) -> *mut u8
                    // All three args remain pointer-only (Str + List<Str>).
                    if args.len() != 3 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("prompt_render: expected 3 args, got {}", args.len()),
                        });
                    }
                    let sys = args[0].clone();
                    let usr = args[1].clone();
                    let vars = args[2].clone();
                    *func =
                        Operand::Constant(Constant::Str(PROMPT_RENDER_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(sys);
                    args.push(usr);
                    args.push(vars);
                }
                Kind::PromptFormatFewShot => {
                    // prompt_format_few_shot(examples_in, examples_out, current_input) -> str
                    // → __cobrust_prompt_format_few_shot(in_ptr, out_ptr, cur_ptr) -> *mut u8
                    if args.len() != 3 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!(
                                "prompt_format_few_shot: expected 3 args, got {}",
                                args.len()
                            ),
                        });
                    }
                    let xin = args[0].clone();
                    let xout = args[1].clone();
                    let cur = args[2].clone();
                    *func = Operand::Constant(Constant::Str(
                        PROMPT_FORMAT_FEW_SHOT_RUNTIME_SYMBOL.to_string(),
                    ));
                    args.clear();
                    args.push(xin);
                    args.push(xout);
                    args.push(cur);
                }
                Kind::PromptFormatSystemUser => {
                    // prompt_format_system_user(system, user) -> str
                    // → __cobrust_prompt_format_system_user(sys_ptr, usr_ptr) -> *mut u8
                    if args.len() != 2 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!(
                                "prompt_format_system_user: expected 2 args, got {}",
                                args.len()
                            ),
                        });
                    }
                    let sys = args[0].clone();
                    let usr = args[1].clone();
                    *func = Operand::Constant(Constant::Str(
                        PROMPT_FORMAT_SYSTEM_USER_RUNTIME_SYMBOL.to_string(),
                    ));
                    args.clear();
                    args.push(sys);
                    args.push(usr);
                }
                Kind::PromptEscapeBraces => {
                    // prompt_escape_braces(text) -> str
                    // → __cobrust_prompt_escape_braces(text_ptr) -> *mut u8
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!(
                                "prompt_escape_braces: expected 1 arg, got {}",
                                args.len()
                            ),
                        });
                    }
                    let text = args[0].clone();
                    *func = Operand::Constant(Constant::Str(
                        PROMPT_ESCAPE_BRACES_RUNTIME_SYMBOL.to_string(),
                    ));
                    args.clear();
                    args.push(text);
                }
                Kind::LlmCompleteStructured => {
                    // llm_complete_structured(prompt, schema_json) -> str
                    // → __cobrust_llm_complete_structured(prompt_ptr, schema_ptr) -> *mut u8
                    if args.len() != 2 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!(
                                "llm_complete_structured: expected 2 args, got {}",
                                args.len()
                            ),
                        });
                    }
                    let p = args[0].clone();
                    let s = args[1].clone();
                    *func = Operand::Constant(Constant::Str(
                        LLM_COMPLETE_STRUCTURED_RUNTIME_SYMBOL.to_string(),
                    ));
                    args.clear();
                    args.push(p);
                    args.push(s);
                }
                Kind::ToolSchema => {
                    // tool_schema(name, description, parameters_json, return_type) -> str
                    // → __cobrust_tool_schema(name_ptr, desc_ptr, params_ptr, return_ptr)
                    if args.len() != 4 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("tool_schema: expected 4 args, got {}", args.len()),
                        });
                    }
                    let name = args[0].clone();
                    let description = args[1].clone();
                    let parameters_json = args[2].clone();
                    let return_type = args[3].clone();
                    *func =
                        Operand::Constant(Constant::Str(TOOL_SCHEMA_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(name);
                    args.push(description);
                    args.push(parameters_json);
                    args.push(return_type);
                }
                Kind::ToolRegistryNew => {
                    if !args.is_empty() {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!(
                                "tool_registry_new: expected 0 args, got {}",
                                args.len()
                            ),
                        });
                    }
                    *func = Operand::Constant(Constant::Str(
                        TOOL_REGISTRY_NEW_RUNTIME_SYMBOL.to_string(),
                    ));
                    args.clear();
                }
                Kind::ToolRegistryRegister => {
                    if args.len() != 2 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!(
                                "tool_registry_register: expected 2 args, got {}",
                                args.len()
                            ),
                        });
                    }
                    let registry = args[0].clone();
                    let schema = args[1].clone();
                    *func = Operand::Constant(Constant::Str(
                        TOOL_REGISTRY_REGISTER_RUNTIME_SYMBOL.to_string(),
                    ));
                    args.clear();
                    args.push(registry);
                    args.push(schema);
                }
                Kind::ToolInvoke => {
                    if args.len() != 2 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("tool_invoke: expected 2 args, got {}", args.len()),
                        });
                    }
                    let tool_name = args[0].clone();
                    let args_json = args[1].clone();
                    *func =
                        Operand::Constant(Constant::Str(TOOL_INVOKE_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(tool_name);
                    args.push(args_json);
                }
                Kind::LlmCompleteWithTools => {
                    if args.len() != 2 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!(
                                "llm_complete_with_tools: expected 2 args, got {}",
                                args.len()
                            ),
                        });
                    }
                    let prompt = args[0].clone();
                    let registry = args[1].clone();
                    *func = Operand::Constant(Constant::Str(
                        LLM_COMPLETE_WITH_TOOLS_RUNTIME_SYMBOL.to_string(),
                    ));
                    args.clear();
                    args.push(prompt);
                    args.push(registry);
                }
                // ---- M-F.3.3 gap (b): math intrinsics — single-arg f64→f64 ----
                Kind::MathSqrt
                | Kind::MathFloor
                | Kind::MathCeil
                | Kind::MathRound
                | Kind::MathAbs
                | Kind::MathSin
                | Kind::MathCos
                | Kind::MathTan
                | Kind::MathLog
                | Kind::MathExp => {
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("math single-arg: expected 1 arg, got {}", args.len()),
                        });
                    }
                    let x_arg = args[0].clone();
                    let sym = match kind {
                        Kind::MathSqrt => MATH_SQRT_RUNTIME_SYMBOL,
                        Kind::MathFloor => MATH_FLOOR_RUNTIME_SYMBOL,
                        Kind::MathCeil => MATH_CEIL_RUNTIME_SYMBOL,
                        Kind::MathRound => MATH_ROUND_RUNTIME_SYMBOL,
                        Kind::MathAbs => MATH_ABS_RUNTIME_SYMBOL,
                        Kind::MathSin => MATH_SIN_RUNTIME_SYMBOL,
                        Kind::MathCos => MATH_COS_RUNTIME_SYMBOL,
                        Kind::MathTan => MATH_TAN_RUNTIME_SYMBOL,
                        Kind::MathLog => MATH_LOG_RUNTIME_SYMBOL,
                        Kind::MathExp => MATH_EXP_RUNTIME_SYMBOL,
                        _ => unreachable!(),
                    };
                    *func = Operand::Constant(Constant::Str(sym.to_string()));
                    args.clear();
                    args.push(x_arg);
                }
                Kind::MathPow => {
                    // pow(base: f64, exp: f64) -> f64
                    if args.len() != 2 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("pow: expected 2 args, got {}", args.len()),
                        });
                    }
                    let base_arg = args[0].clone();
                    let exp_arg = args[1].clone();
                    *func = Operand::Constant(Constant::Str(MATH_POW_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(base_arg);
                    args.push(exp_arg);
                }
                // ---- M-F.3.5 string stdlib (ADR-0050e) ----
                // Two-arg Str×Str dispatch (split / find / contains /
                // starts_with / ends_with) — all p×p signatures.
                Kind::StrSplit
                | Kind::StrFind
                | Kind::StrContains
                | Kind::StrStartsWith
                | Kind::StrEndsWith => {
                    if args.len() != 2 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!(
                                "M-F.3.5 string fn: expected 2 args, got {}",
                                args.len()
                            ),
                        });
                    }
                    let a = args[0].clone();
                    let b = args[1].clone();
                    let sym = match kind {
                        Kind::StrSplit => STR_SPLIT_RUNTIME_SYMBOL,
                        Kind::StrFind => STR_FIND_RUNTIME_SYMBOL,
                        Kind::StrContains => STR_CONTAINS_RUNTIME_SYMBOL,
                        Kind::StrStartsWith => STR_STARTS_WITH_RUNTIME_SYMBOL,
                        Kind::StrEndsWith => STR_ENDS_WITH_RUNTIME_SYMBOL,
                        _ => unreachable!(),
                    };
                    *func = Operand::Constant(Constant::Str(sym.to_string()));
                    args.clear();
                    args.push(a);
                    args.push(b);
                }
                // `join(parts: list[str], sep: str) -> str` — two-arg
                // p×p signature; first arg is a list[str] pointer, second
                // is a str pointer. C-ABI handles both as `*mut u8`.
                Kind::StrJoin => {
                    if args.len() != 2 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("join: expected 2 args, got {}", args.len()),
                        });
                    }
                    let parts = args[0].clone();
                    let sep = args[1].clone();
                    *func = Operand::Constant(Constant::Str(STR_JOIN_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(parts);
                    args.push(sep);
                }
                // `replace(s, old, new) -> str` — three-arg p×p×p→p.
                Kind::StrReplace => {
                    if args.len() != 3 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("replace: expected 3 args, got {}", args.len()),
                        });
                    }
                    let s = args[0].clone();
                    let old = args[1].clone();
                    let new_ = args[2].clone();
                    *func =
                        Operand::Constant(Constant::Str(STR_REPLACE_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(s);
                    args.push(old);
                    args.push(new_);
                }
                // Single-arg Str→Str dispatch (trim / lower / upper / clone).
                Kind::StrTrim | Kind::StrLower | Kind::StrUpper | Kind::StrClone => {
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!(
                                "M-F.3.5 string single-arg fn: expected 1 arg, got {}",
                                args.len()
                            ),
                        });
                    }
                    let s = args[0].clone();
                    let sym = match kind {
                        Kind::StrTrim => STR_TRIM_RUNTIME_SYMBOL,
                        Kind::StrLower => STR_LOWER_RUNTIME_SYMBOL,
                        Kind::StrUpper => STR_UPPER_RUNTIME_SYMBOL,
                        Kind::StrClone => STR_CLONE_RUNTIME_SYMBOL,
                        _ => unreachable!(),
                    };
                    *func = Operand::Constant(Constant::Str(sym.to_string()));
                    args.clear();
                    args.push(s);
                }
                // ---- M-F.3.6 file IO completion (ADR-0050f) ----
                //
                // Copy-at-operand discipline for str args (ADR-0050c Phase 2a
                // walk-back precedent): these shims READ the Str pointer without
                // freeing it (the C-ABI shim calls str_buf_as_str_phase3 which
                // only borrows the buffer). Ownership of the str local stays with
                // the caller's scope; the drop schedule handles freeing at scope
                // exit. This mirrors how list arguments work (list is Copy at
                // the operand level, non-Copy at drop level).
                //
                // Concretely: we upgrade Move(p) → Copy(p) for every named-local
                // str arg so the borrow checker doesn't flag the local as consumed.
                // Constant::Str args (literals) are passed as-is (they're already
                // immortal .rodata; no ownership transfer).
                Kind::ReadFile => {
                    // read_file(path: str) -> str
                    // → __cobrust_read_file(path_ptr) -> *mut u8
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("read_file: expected 1 arg, got {}", args.len()),
                        });
                    }
                    let path = move_to_copy(args[0].clone());
                    *func =
                        Operand::Constant(Constant::Str(READ_FILE_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(path);
                }
                Kind::ReadFileLines => {
                    // read_file_lines(path: str) -> list[str]
                    // → __cobrust_read_file_lines(path_ptr) -> *mut u8 (list ptr)
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("read_file_lines: expected 1 arg, got {}", args.len()),
                        });
                    }
                    let path = move_to_copy(args[0].clone());
                    *func = Operand::Constant(Constant::Str(
                        READ_FILE_LINES_RUNTIME_SYMBOL.to_string(),
                    ));
                    args.clear();
                    args.push(path);
                }
                Kind::WriteFile => {
                    // write_file(path: str, contents: str) -> i64
                    // → __cobrust_write_file(path_ptr, contents_ptr) -> i64
                    // Both path and contents are Copy-at-operand (shim reads only;
                    // caller scope drops at exit per ADR-0050c Phase 2a walk-back).
                    if args.len() != 2 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("write_file: expected 2 args, got {}", args.len()),
                        });
                    }
                    let path = move_to_copy(args[0].clone());
                    let contents = move_to_copy(args[1].clone());
                    *func =
                        Operand::Constant(Constant::Str(WRITE_FILE_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(path);
                    args.push(contents);
                }
                Kind::AppendFile => {
                    // append_file(path: str, contents: str) -> i64
                    // → __cobrust_append_file(path_ptr, contents_ptr) -> i64
                    // Copy-at-operand (shim reads only).
                    if args.len() != 2 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("append_file: expected 2 args, got {}", args.len()),
                        });
                    }
                    let path = move_to_copy(args[0].clone());
                    let contents = move_to_copy(args[1].clone());
                    *func =
                        Operand::Constant(Constant::Str(APPEND_FILE_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(path);
                    args.push(contents);
                }
                Kind::StdinReadAll => {
                    // stdin_read_all() -> str
                    // → __cobrust_stdin_read_all() -> *mut u8
                    if !args.is_empty() {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("stdin_read_all: expected 0 args, got {}", args.len()),
                        });
                    }
                    *func = Operand::Constant(Constant::Str(
                        STDIN_READ_ALL_RUNTIME_SYMBOL.to_string(),
                    ));
                    args.clear();
                }
                Kind::StdoutWrite => {
                    // stdout_write(s: str) -> i64
                    // → __cobrust_stdout_write(s_ptr) -> i64
                    // Does NOT append newline (f3fio14 lock per ADR-0050f).
                    // Copy-at-operand: shim reads s without freeing.
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("stdout_write: expected 1 arg, got {}", args.len()),
                        });
                    }
                    let s = move_to_copy(args[0].clone());
                    *func =
                        Operand::Constant(Constant::Str(STDOUT_WRITE_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(s);
                }
                Kind::StderrWrite => {
                    // stderr_write(s: str) -> i64
                    // → __cobrust_stderr_write(s_ptr) -> i64
                    // Goes to stderr; stdout empty (f3fio15 lock per ADR-0050f).
                    // Copy-at-operand: shim reads s without freeing.
                    if args.len() != 1 {
                        return Err(IntrinsicError::PrintArgUnsupported {
                            found: format!("stderr_write: expected 1 arg, got {}", args.len()),
                        });
                    }
                    let s = move_to_copy(args[0].clone());
                    *func =
                        Operand::Constant(Constant::Str(STDERR_WRITE_RUNTIME_SYMBOL.to_string()));
                    args.clear();
                    args.push(s);
                }
            }
        }
    }

    // Drop all prelude stub Bodies. After rewrite no callsite
    // references them.
    module
        .bodies
        .retain(|body| !all_stub_ids.contains(&body.def_id.0));

    Ok(())
}
