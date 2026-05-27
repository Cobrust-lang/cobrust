//! Synthetic PRELUDE source prepended to every Cobrust program before
//! parsing.
//!
//! The PRELUDE declares the source-language signatures of every
//! intrinsic visible to user code (`print`, `range`, `parse_int`,
//! `list_*`, `str_*`, math, IO, LLM helpers). It is a real Cobrust
//! source fragment — the parser, HIR lowering, and type checker all
//! see it as if the user had written it themselves; the intrinsic-
//! rewrite MIR pass (per ADR-0024 §"Hello-world contract" + ADR-0050b
//! M-F.3.x) then retargets matching `Call` operands to the runtime
//! helper symbols (`__cobrust_println_static`, `__cobrust_list_set`,
//! `__cobrust_iter_*`, etc.).
//!
//! Per finding `f50-lsp-cli-diagnostic-divergence.md` (2026-05-22):
//! the PRELUDE lives here (not in `cobrust-cli`) so both the CLI
//! `cobrust check` / `cobrust build` driver AND the `cobrust-lsp`
//! `textDocument/publishDiagnostics` pipeline prepend the SAME source
//! before invoking the frontend. Without this, every LSP client (Cursor
//! / VSCode / Cody / Aider) surfaces a `lower-unknown-name` red
//! squiggle on every `print(...)` callsite even though `cobrust check`
//! reports `ok`.
//!
//! ADR-0064: `print_int` removed from the source-face PRELUDE. The single
//! `print(s: str)` stub is kept; the type-checker treats it as a
//! polymorphic intrinsic (accepting any type) via
//! `is_print_polymorphic_intrinsic_name`. The intrinsic-rewrite pass
//! dispatches to `__cobrust_println_int` / `__cobrust_println_bool` /
//! `__cobrust_println_float` at MIR time based on the resolved arg type.

/// The synthetic PRELUDE source prepended to every Cobrust program
/// before parsing.
///
/// See module docs for the rationale and ADR cross-refs.
pub const PRELUDE: &str = "fn print(s: str) -> i64:\n    return 0\n\nfn input(prompt: str) -> str:\n    return \"\"\n\nfn input_no_prompt() -> str:\n    return \"\"\n\nfn read_line() -> str:\n    return \"\"\n\nfn argv() -> list[str]:\n    let xs: list[str] = []\n    return xs\n\nfn parse_int(s: str) -> i64:\n    return 0\n\nfn str_len(s: str) -> i64:\n    return 0\n\nfn str_at(s: str, i: i64) -> str:\n    return \"\"\n\nfn str_eq(a: str, b: str) -> i64:\n    return 0\n\nfn str_eq_lit(s: str, lit: str) -> i64:\n    return 0\n\nfn str_ord(s: str) -> i64:\n    return 0\n\nfn parse_int_tok(line: str, i: i64) -> i64:\n    return 0\n\nfn count_toks(line: str) -> i64:\n    return 0\n\nfn list_set(lst: list[i64], i: i64, v: i64) -> i64:\n    return 0\n\nfn list_get(lst: list[i64], i: i64) -> i64:\n    return 0\n\nfn list_len(lst: list[i64]) -> i64:\n    return 0\n\nfn list_is_empty(lst: list[i64]) -> bool:\n    return False\n\nfn dict_is_empty(d: dict[i64, i64]) -> bool:\n    return False\n\nfn len(d: dict[i64, i64]) -> i64:\n    return 0\n\nfn list_new(capacity: i64) -> list[i64]:\n    let xs: list[i64] = []\n    return xs\n\nfn print_no_nl(s: str) -> i64:\n    return 0\n\nfn range(start: i64, stop: i64) -> list[i64]:\n    let n: i64 = stop - start\n    let xs: list[i64] = list_new(n)\n    let i: i64 = 0\n    while i < n:\n        let _ = list_set(xs, i, start + i)\n        i = i + 1\n    return xs\n\nfn sqrt(x: f64) -> f64:\n    return 0.0\n\nfn floor(x: f64) -> f64:\n    return 0.0\n\nfn ceil(x: f64) -> f64:\n    return 0.0\n\nfn round(x: f64) -> f64:\n    return 0.0\n\nfn abs(x: f64) -> f64:\n    return 0.0\n\nfn pow(base: f64, exp: f64) -> f64:\n    return 0.0\n\nfn sin(x: f64) -> f64:\n    return 0.0\n\nfn cos(x: f64) -> f64:\n    return 0.0\n\nfn tan(x: f64) -> f64:\n    return 0.0\n\nfn log(x: f64) -> f64:\n    return 0.0\n\nfn exp(x: f64) -> f64:\n    return 0.0\n\nfn llm_complete(provider: str, model: str, prompt: str) -> str:\n    return \"\"\n\nfn llm_dispatch(task: str, prompt: str) -> str:\n    return \"\"\n\nfn llm_stream(provider: str, model: str, prompt: str) -> list[str]:\n    let xs: list[str] = []\n    return xs\n\nfn prompt_render(system: str, user: str, vars: list[str]) -> str:\n    return \"\"\n\nfn prompt_format_few_shot(examples_in: list[str], examples_out: list[str], current_input: str) -> str:\n    return \"\"\n\nfn prompt_format_system_user(system: str, user: str) -> str:\n    return \"\"\n\nfn prompt_escape_braces(text: str) -> str:\n    return \"\"\n\nfn llm_complete_structured(prompt: str, schema_json: str) -> str:\n    return \"\"\n\nfn tool_schema(name: str, description: str, parameters_json: str, return_type: str) -> str:\n    return \"\"\n\nfn tool_registry_new() -> str:\n    return \"\"\n\nfn tool_registry_register(registry_json: str, schema_json: str) -> str:\n    return \"\"\n\nfn tool_invoke(tool_name: str, args_json: str) -> str:\n    return \"\"\n\nfn llm_complete_with_tools(prompt: str, registry_json: str) -> str:\n    return \"\"\n\nfn split(s: str, sep: str) -> list[str]:\n    let xs: list[str] = []\n    return xs\n\nfn join(parts: list[str], sep: str) -> str:\n    return \"\"\n\nfn replace(s: str, old: str, new: str) -> str:\n    return \"\"\n\nfn trim(s: str) -> str:\n    return \"\"\n\nfn find(s: str, needle: str) -> i64:\n    return -1\n\nfn contains(s: str, needle: str) -> bool:\n    return False\n\nfn starts_with(s: str, prefix: str) -> bool:\n    return False\n\nfn ends_with(s: str, suffix: str) -> bool:\n    return False\n\nfn lower(s: str) -> str:\n    return \"\"\n\nfn upper(s: str) -> str:\n    return \"\"\n\nfn clone(s: str) -> str:\n    return s\n\nfn read_file(path: str) -> str:\n    return \"\"\n\nfn read_file_lines(path: str) -> list[str]:\n    let xs: list[str] = []\n    return xs\n\nfn write_file(path: str, contents: str) -> i64:\n    return 0\n\nfn append_file(path: str, contents: str) -> i64:\n    return 0\n\nfn stdin_read_all() -> str:\n    return \"\"\n\nfn stdout_write(s: str) -> i64:\n    return 0\n\nfn stderr_write(s: str) -> i64:\n    return 0\n\nfn json_dumps(json_input: str) -> str:\n    return \"\"\n\nfn json_dumps_indent(json_input: str, indent: i64) -> str:\n    return \"\"\n\nfn json_loads(s: str) -> str:\n    return \"\"\n\n";

/// Byte length of [`PRELUDE`] — the offset at which user-source bytes
/// begin in a composed `format!("{PRELUDE}{user_source}")` string.
///
/// Computed at compile time via const-fn so it can never drift from
/// the literal above. Consumed by `cobrust-lsp` to filter/shift LSP
/// `Diagnostic` ranges back into user-source coordinates after running
/// the pipeline against the composed source.
#[allow(
    clippy::cast_possible_truncation,
    reason = "PRELUDE compile-time literal well under u32::MAX"
)]
pub const PRELUDE_BYTE_LEN: u32 = PRELUDE.len() as u32;

/// Number of newline-terminated lines in [`PRELUDE`].
///
/// Since PRELUDE always ends with a trailing `"\n"`, user source begins
/// at line index `PRELUDE_LINE_COUNT` (0-indexed) when concatenated.
/// `cobrust-lsp` subtracts this constant from every emitted LSP
/// `Diagnostic.range.start.line` / `end.line` to shift back into user
/// coordinates.
pub const PRELUDE_LINE_COUNT: u32 = count_newlines(PRELUDE);

/// Count `\n` bytes in `s` at const-eval time.
const fn count_newlines(s: &str) -> u32 {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    let mut count = 0u32;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            count += 1;
        }
        i += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prelude_ends_with_newline() {
        assert!(PRELUDE.ends_with('\n'));
    }

    #[test]
    fn prelude_byte_len_matches_literal() {
        assert_eq!(PRELUDE_BYTE_LEN as usize, PRELUDE.len());
    }

    #[test]
    fn prelude_line_count_matches_literal() {
        let actual = u32::try_from(PRELUDE.bytes().filter(|&b| b == b'\n').count())
            .expect("PRELUDE line count fits in u32");
        assert_eq!(PRELUDE_LINE_COUNT, actual);
    }

    #[test]
    fn prelude_declares_print() {
        assert!(PRELUDE.contains("fn print(s: str) -> i64:"));
    }

    #[test]
    fn prelude_declares_range() {
        assert!(PRELUDE.contains("fn range(start: i64, stop: i64) -> list[i64]:"));
    }
}
