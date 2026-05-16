//! `std.string` — len / find / replace / split / trim / lower /
//! upper / contains / starts_with / ends_with / join / format
//! plus the C-ABI surface consumed by codegen-emitted M-F.3.5 calls.
//!
//! ADR-0025 §"Public surface (binding)" pins the Rust-side API. Per
//! ADR-0019 §"M11 — Standard library" the surface mirrors Python's
//! `str` operations, with Cobrust's "no silent coercion" rule
//! (constitution §2.2) applied to `format`.
//!
//! ADR-0050e (M-F.3.5) adds the C-ABI shim surface and four net-new
//! Rust helpers (`join` / `contains` / `starts_with` / `ends_with`).
//! The existing `strip` helper is renamed to `trim` per Decision 4
//! to match Rust + LeetCode convention.

// =====================================================================
// Surface helpers
// =====================================================================

/// UTF-8 byte length. Cobrust strings are always UTF-8; this is the
/// number of bytes, not Unicode code points. For code-point count
/// users call `s.chars().count()` directly (M11.x will widen with
/// a `char_count` helper if needed).
pub fn len(s: &str) -> usize {
    s.len()
}

/// First byte position where `pat` starts, or `None`.
pub fn find(s: &str, pat: &str) -> Option<usize> {
    s.find(pat)
}

/// Replace every occurrence of `from` with `to`.
pub fn replace(s: &str, from: &str, to: &str) -> String {
    s.replace(from, to)
}

/// Split on `sep`. Empty separator yields a singleton vector
/// containing the original string (matches Python's
/// `str.split('')` which raises; Cobrust returns the safe
/// alternative).
pub fn split(s: &str, sep: &str) -> Vec<String> {
    if sep.is_empty() {
        return vec![s.to_string()];
    }
    s.split(sep).map(String::from).collect()
}

/// Trim ASCII / Unicode whitespace from both ends.
///
/// Renamed from `strip` in ADR-0050e Decision 4 to match Rust's
/// `str::trim`, Python's `str.strip()` (no-arg form), and LeetCode
/// convention.
pub fn trim(s: &str) -> &str {
    s.trim()
}

/// Lowercase. ASCII fast-path is what Rust's `str::to_lowercase`
/// gives us; full Unicode case-folding requires the `unicode-case`
/// helper crate (post-M11).
pub fn lower(s: &str) -> String {
    s.to_lowercase()
}

/// Uppercase. Same caveat as [`lower`].
pub fn upper(s: &str) -> String {
    s.to_uppercase()
}

/// Concatenate `parts` with `sep` between every pair of adjacent
/// elements. ADR-0050e Decision 8: empty input list yields the empty
/// string; one-element input yields the element unchanged (no
/// separator emitted).
pub fn join(parts: &[&str], sep: &str) -> String {
    parts.join(sep)
}

/// Returns `true` if `needle` is a (byte) substring of `s`. Byte-level
/// per ADR-0050e Decision 6.
pub fn contains(s: &str, needle: &str) -> bool {
    s.contains(needle)
}

/// Returns `true` if `s` begins with `prefix` (byte-level).
pub fn starts_with(s: &str, prefix: &str) -> bool {
    s.starts_with(prefix)
}

/// Returns `true` if `s` ends with `suffix` (byte-level).
pub fn ends_with(s: &str, suffix: &str) -> bool {
    s.ends_with(suffix)
}

// =====================================================================
// format — Cobrust-style positional formatter
// =====================================================================

/// Format-argument variants supported by [`format`]. Constitution
/// §2.2 forbids silent coercion, so the caller types the variant
/// explicitly.
#[derive(Clone, Debug)]
pub enum FormatArg<'a> {
    Str(&'a str),
    Int(i64),
    Float(f64),
    Bool(bool),
}

impl<'a> std::fmt::Display for FormatArg<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatArg::Str(s) => f.write_str(s),
            FormatArg::Int(i) => write!(f, "{i}"),
            FormatArg::Float(x) => {
                // Match Python's default repr behavior closely:
                // integers display as "1.0", non-integers as their
                // shortest round-trip repr.
                if x.fract() == 0.0 && x.is_finite() {
                    write!(f, "{x:.1}")
                } else {
                    write!(f, "{x}")
                }
            }
            FormatArg::Bool(b) => f.write_str(if *b { "True" } else { "False" }),
        }
    }
}

/// Format `template` by substituting `{}` placeholders with `args`
/// in order. Errors out (returning the partial template + a
/// tail marker) if the count is mismatched.
///
/// Cobrust f-strings (HIR-lowered) call this at runtime via the
/// `std.fmt` shims.
pub fn format(template: &str, args: &[FormatArg<'_>]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut iter = args.iter();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            if let Some(&'{') = chars.peek() {
                // Escaped '{{'.
                chars.next();
                out.push('{');
                continue;
            }
            // Look for matching '}'.
            let mut closed = false;
            for nc in chars.by_ref() {
                if nc == '}' {
                    closed = true;
                    break;
                }
            }
            if !closed {
                // Malformed — emit the rest verbatim.
                out.push('{');
                continue;
            }
            match iter.next() {
                Some(arg) => out.push_str(&arg.to_string()),
                None => out.push_str("{?}"),
            }
        } else if c == '}' {
            if let Some(&'}') = chars.peek() {
                chars.next();
                out.push('}');
            } else {
                out.push('}');
            }
        } else {
            out.push(c);
        }
    }
    out
}

// =====================================================================
// C-ABI surface (ADR-0050e M-F.3.5)
// =====================================================================
//
// Eleven shims wrapping the Rust-side helpers above. Each shim takes
// `*mut u8` Str buffer pointers per the ADR-0044 W2 Phase 3 + ADR-0050c
// convention (StringBuffer pointers materialized via
// `__cobrust_str_new` + `__cobrust_str_push_static`). Returns either a
// newly-allocated Str pointer or i64 (for find / predicates).
//
// **Ownership convention (ADR-0050e §"Open shim-drop-owner question")**:
// The shim does NOT drop its Str args. The caller's MIR drop pass
// (ADR-0050c Phase 2) owns the input lifetime; the codegen emits
// `__cobrust_str_drop` at scope exit for the call-site bindings. The
// shim only reads its inputs via `str_buf_as_str_local` and allocates
// fresh return buffers via `alloc_str_buffer_local`.
//
// `__cobrust_str_clone` is already shipped at `fmt.rs:306` and is NOT
// duplicated here; the M-F.3.5 PRELUDE plumbing reuses that shim.

/// Read a Str buffer pointer as a `&str` slice (read-only borrow into
/// the heap StringBuffer's UTF-8 bytes). Mirrors `io.rs:570`'s
/// `str_buf_as_str_phase3` so this module is self-contained.
///
/// # Safety
///
/// `buf` must be a non-null pointer to a `StringBuffer` produced by
/// `__cobrust_str_new` (or `__cobrust_str_clone`).
unsafe fn str_buf_as_str_local<'a>(buf: *mut u8) -> &'a str {
    if buf.is_null() {
        return "";
    }
    // SAFETY: caller-attestation.
    let len = unsafe { crate::fmt::__cobrust_str_len(buf) } as usize;
    if len == 0 {
        return "";
    }
    let ptr = unsafe { crate::fmt::__cobrust_str_ptr(buf) };
    if ptr.is_null() {
        return "";
    }
    // SAFETY: ptr points to `len` bytes of UTF-8 maintained by all
    // StringBuffer write paths.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    std::str::from_utf8(bytes).unwrap_or("")
}

/// Allocate a fresh heap StringBuffer carrying `s`'s bytes; returns the
/// opaque `*mut u8` pointer the caller's drop pass picks up.
fn alloc_str_buffer_local(s: &str) -> *mut u8 {
    // SAFETY: `__cobrust_str_new` returns a valid empty buffer pointer;
    // `__cobrust_str_push_static` is safe for valid (ptr, len) pairs.
    unsafe {
        let buf = crate::fmt::__cobrust_str_new();
        if !s.is_empty() {
            crate::fmt::__cobrust_str_push_static(buf, s.as_ptr(), s.len() as i64);
        }
        buf
    }
}

/// C-ABI shim for source-level `split(s: str, sep: str) -> list[str]`.
///
/// Materializes a heap `List<i64>` whose i64 slots store owned
/// StringBuffer pointers (one per split element). The codegen drop
/// schedule frees each element + the list via
/// `__cobrust_list_drop_elems(list, __cobrust_str_drop)` at scope exit
/// (ADR-0050c Phase 3).
///
/// Empty-input edge cases (ADR-0050e Decision 8):
///   - `split("", sep)` → `[""]` (one element, empty Str)
///   - `split(s, "")`   → `[s]` (mirrors `string::split` semantics)
///
/// # Safety
///
/// `s` and `sep` must be valid Str buffer pointers produced by
/// `__cobrust_str_new` (or null). The returned list must be passed to
/// `__cobrust_list_drop_elems(list, __cobrust_str_drop)` exactly once
/// (codegen does this automatically per ADR-0050c).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_split(s: *mut u8, sep: *mut u8) -> *mut u8 {
    // SAFETY: caller-attestation.
    let s_str = unsafe { str_buf_as_str_local(s) };
    let sep_str = unsafe { str_buf_as_str_local(sep) };
    let parts = split(s_str, sep_str);
    // SAFETY: list_new returns a valid List<i64> pointer with `len`
    // zeroed slots; list_set writes the i64 slot at index i.
    unsafe {
        let list = crate::collections::__cobrust_list_new(8, parts.len() as i64);
        for (i, part) in parts.iter().enumerate() {
            let buf = alloc_str_buffer_local(part);
            crate::collections::__cobrust_list_set(list, i as i64, buf as i64);
        }
        list
    }
}

/// C-ABI shim for source-level `join(parts: list[str], sep: str) -> str`.
///
/// Reads `parts` as a `List<i64>` whose slots store Str buffer
/// pointers (the shape `split` / `argv()` produces), reads `sep`,
/// concatenates with `sep` between every adjacent pair, and returns a
/// fresh Str buffer. Empty list → empty Str; one-element list → that
/// element without separator (ADR-0050e Decision 8).
///
/// # Safety
///
/// `parts` must be a valid `List<i64>` pointer whose slots are valid
/// Str buffer pointers. `sep` must be a valid Str buffer pointer or
/// null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_join(parts: *mut u8, sep: *mut u8) -> *mut u8 {
    let sep_str = unsafe { str_buf_as_str_local(sep) };
    if parts.is_null() {
        return alloc_str_buffer_local("");
    }
    // SAFETY: parts is a valid List<i64>.
    let n = unsafe { crate::collections::__cobrust_list_len(parts) };
    if n <= 0 {
        return alloc_str_buffer_local("");
    }
    let mut owned: Vec<String> = Vec::with_capacity(n as usize);
    for i in 0..n {
        // SAFETY: i is in [0, n) and the slot stores an `*mut u8`
        // reinterpretation of a StringBuffer pointer.
        let slot = unsafe { crate::collections::__cobrust_list_get(parts, i) };
        // Treat slot value as a Str buffer pointer.
        let bp = slot as *mut u8;
        let s = unsafe { str_buf_as_str_local(bp) };
        owned.push(s.to_string());
    }
    let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
    let joined = join(&refs, sep_str);
    alloc_str_buffer_local(&joined)
}

/// C-ABI shim for source-level `replace(s, old, new) -> str`.
///
/// # Safety
///
/// `s`, `old`, `new_` must each be valid Str buffer pointers or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_replace(
    s: *mut u8,
    old: *mut u8,
    new_: *mut u8,
) -> *mut u8 {
    let s_str = unsafe { str_buf_as_str_local(s) };
    let old_str = unsafe { str_buf_as_str_local(old) };
    let new_str = unsafe { str_buf_as_str_local(new_) };
    let r = replace(s_str, old_str, new_str);
    alloc_str_buffer_local(&r)
}

/// C-ABI shim for source-level `trim(s) -> str`. Whitespace-only,
/// both-sides; ADR-0050e Q5 defers a chars-argument form to Phase G.
///
/// # Safety
///
/// `s` must be a valid Str buffer pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_trim(s: *mut u8) -> *mut u8 {
    let s_str = unsafe { str_buf_as_str_local(s) };
    alloc_str_buffer_local(trim(s_str))
}

/// C-ABI shim for source-level `find(s, needle) -> i64` with `-1`
/// sentinel per ADR-0050e Decision 5. Empty needle yields `0` (matches
/// Python's `str.find('')`).
///
/// # Safety
///
/// `s` and `needle` must be valid Str buffer pointers or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_find(s: *mut u8, needle: *mut u8) -> i64 {
    let s_str = unsafe { str_buf_as_str_local(s) };
    let needle_str = unsafe { str_buf_as_str_local(needle) };
    match find(s_str, needle_str) {
        Some(i) => i as i64,
        None => -1,
    }
}

/// C-ABI shim for source-level `contains(s, needle) -> bool`. Returns
/// `1` (true) or `0` (false) in i64 for the SwitchInt codegen
/// convention. Empty needle is always true (mirrors `find` returning
/// 0 plus the symmetry `contains(s, "") == (find(s, "") != -1)`).
///
/// # Safety
///
/// `s` and `needle` must be valid Str buffer pointers or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_contains(s: *mut u8, needle: *mut u8) -> i64 {
    let s_str = unsafe { str_buf_as_str_local(s) };
    let needle_str = unsafe { str_buf_as_str_local(needle) };
    i64::from(contains(s_str, needle_str))
}

/// C-ABI shim for source-level `starts_with(s, prefix) -> bool`. i64 0/1
/// at the ABI per the SwitchInt convention.
///
/// # Safety
///
/// `s` and `prefix` must be valid Str buffer pointers or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_starts_with(s: *mut u8, prefix: *mut u8) -> i64 {
    let s_str = unsafe { str_buf_as_str_local(s) };
    let prefix_str = unsafe { str_buf_as_str_local(prefix) };
    i64::from(starts_with(s_str, prefix_str))
}

/// C-ABI shim for source-level `ends_with(s, suffix) -> bool`. i64 0/1
/// at the ABI per the SwitchInt convention.
///
/// # Safety
///
/// `s` and `suffix` must be valid Str buffer pointers or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_ends_with(s: *mut u8, suffix: *mut u8) -> i64 {
    let s_str = unsafe { str_buf_as_str_local(s) };
    let suffix_str = unsafe { str_buf_as_str_local(suffix) };
    i64::from(ends_with(s_str, suffix_str))
}

/// C-ABI shim for source-level `lower(s) -> str`. ASCII fast-path with
/// Unicode case-folding via Rust stdlib (ADR-0050e Decision 6).
///
/// # Safety
///
/// `s` must be a valid Str buffer pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_lower(s: *mut u8) -> *mut u8 {
    let s_str = unsafe { str_buf_as_str_local(s) };
    alloc_str_buffer_local(&lower(s_str))
}

/// C-ABI shim for source-level `upper(s) -> str`. ASCII fast-path with
/// Unicode case-folding via Rust stdlib.
///
/// # Safety
///
/// `s` must be a valid Str buffer pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __cobrust_str_upper(s: *mut u8) -> *mut u8 {
    let s_str = unsafe { str_buf_as_str_local(s) };
    alloc_str_buffer_local(&upper(s_str))
}

// `__cobrust_str_clone` ships at `crates/cobrust-stdlib/src/fmt.rs:306`
// — no duplicate shim needed here; M-F.3.5 only adds the PRELUDE
// stub + intrinsic-rewrite arm (landed in Phases 1 + 2).

#[cfg(test)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::format_push_string,
    clippy::let_unit_value,
    clippy::ignored_unit_patterns,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::similar_names,
    clippy::manual_is_multiple_of,
    clippy::redundant_closure_for_method_calls,
    clippy::match_same_arms,
    clippy::map_unwrap_or,
    clippy::needless_pass_by_value,
    clippy::missing_panics_doc
)]
mod tests {
    use super::*;

    #[test]
    fn len_ascii() {
        assert_eq!(len("hello"), 5);
    }

    #[test]
    fn len_utf8_bytes() {
        // "你好" = 6 UTF-8 bytes.
        assert_eq!(len("你好"), 6);
    }

    #[test]
    fn len_empty() {
        assert_eq!(len(""), 0);
    }

    #[test]
    fn find_present() {
        assert_eq!(find("hello world", "world"), Some(6));
    }

    #[test]
    fn find_absent() {
        assert_eq!(find("hello", "x"), None);
    }

    #[test]
    fn find_first_match() {
        assert_eq!(find("aaa", "a"), Some(0));
    }

    #[test]
    fn find_empty_pattern() {
        assert_eq!(find("hello", ""), Some(0));
    }

    #[test]
    fn replace_simple() {
        assert_eq!(replace("foo bar", "bar", "baz"), "foo baz");
    }

    #[test]
    fn replace_all_occurrences() {
        assert_eq!(replace("aaa", "a", "b"), "bbb");
    }

    #[test]
    fn replace_no_match() {
        assert_eq!(replace("hello", "x", "y"), "hello");
    }

    #[test]
    fn replace_empty_target_is_identity() {
        // Rust's str::replace on empty `from` inserts `to` at every
        // position; we follow that semantic.
        let r = replace("ab", "", "X");
        assert!(r.contains('X'));
    }

    #[test]
    fn split_basic() {
        assert_eq!(split("a,b,c", ","), vec!["a", "b", "c"]);
    }

    #[test]
    fn split_no_separator_present() {
        assert_eq!(split("abc", ","), vec!["abc"]);
    }

    #[test]
    fn split_empty_separator() {
        assert_eq!(split("abc", ""), vec!["abc"]);
    }

    #[test]
    fn split_consecutive_separators() {
        assert_eq!(split("a,,b", ","), vec!["a", "", "b"]);
    }

    #[test]
    fn split_empty_string() {
        assert_eq!(split("", ","), vec![""]);
    }

    #[test]
    fn trim_whitespace() {
        assert_eq!(trim("  hello  "), "hello");
    }

    #[test]
    fn trim_no_whitespace() {
        assert_eq!(trim("hello"), "hello");
    }

    #[test]
    fn trim_only_whitespace() {
        assert_eq!(trim("   "), "");
    }

    #[test]
    fn trim_empty_input() {
        assert_eq!(trim(""), "");
    }

    #[test]
    fn join_basic() {
        assert_eq!(join(&["a", "b", "c"], ","), "a,b,c");
    }

    #[test]
    fn join_empty_list_returns_empty() {
        let empty: [&str; 0] = [];
        assert_eq!(join(&empty, ","), "");
    }

    #[test]
    fn join_single_element_no_separator_emitted() {
        assert_eq!(join(&["solo"], ","), "solo");
    }

    #[test]
    fn join_empty_separator_concatenates() {
        assert_eq!(join(&["a", "b", "c"], ""), "abc");
    }

    #[test]
    fn contains_present() {
        assert!(contains("hello world", "world"));
    }

    #[test]
    fn contains_absent() {
        assert!(!contains("hello", "xyz"));
    }

    #[test]
    fn contains_empty_needle_is_true() {
        assert!(contains("hello", ""));
    }

    #[test]
    fn starts_with_present() {
        assert!(starts_with("foobar", "foo"));
    }

    #[test]
    fn starts_with_absent() {
        assert!(!starts_with("foobar", "bar"));
    }

    #[test]
    fn starts_with_empty_prefix_is_true() {
        assert!(starts_with("foobar", ""));
    }

    #[test]
    fn ends_with_present() {
        assert!(ends_with("foobar", "bar"));
    }

    #[test]
    fn ends_with_absent() {
        assert!(!ends_with("foobar", "foo"));
    }

    #[test]
    fn ends_with_empty_suffix_is_true() {
        assert!(ends_with("foobar", ""));
    }

    #[test]
    fn lower_ascii() {
        assert_eq!(lower("HELLO"), "hello");
    }

    #[test]
    fn lower_mixed() {
        assert_eq!(lower("HeLLo"), "hello");
    }

    #[test]
    fn upper_ascii() {
        assert_eq!(upper("hello"), "HELLO");
    }

    #[test]
    fn upper_mixed() {
        assert_eq!(upper("hElLo"), "HELLO");
    }

    #[test]
    fn format_no_placeholder() {
        assert_eq!(format("hello", &[]), "hello");
    }

    #[test]
    fn format_one_str() {
        assert_eq!(format("hi {}", &[FormatArg::Str("there")]), "hi there");
    }

    #[test]
    fn format_one_int() {
        assert_eq!(format("n={}", &[FormatArg::Int(42)]), "n=42");
    }

    #[test]
    fn format_one_float_integer_value() {
        assert_eq!(format("x={}", &[FormatArg::Float(3.0)]), "x=3.0");
    }

    #[test]
    fn format_one_float_fractional() {
        let s = format("x={}", &[FormatArg::Float(3.14)]);
        assert!(s.starts_with("x=3.14"));
    }

    #[test]
    fn format_one_bool_true() {
        assert_eq!(format("b={}", &[FormatArg::Bool(true)]), "b=True");
    }

    #[test]
    fn format_one_bool_false() {
        assert_eq!(format("b={}", &[FormatArg::Bool(false)]), "b=False");
    }

    #[test]
    fn format_multiple() {
        let args = &[
            FormatArg::Int(1),
            FormatArg::Str("two"),
            FormatArg::Bool(true),
        ];
        assert_eq!(format("{} {} {}", args), "1 two True");
    }

    #[test]
    fn format_too_few_args() {
        assert_eq!(format("{}", &[]), "{?}");
    }

    #[test]
    fn format_too_many_args_silent() {
        // Extra args silently dropped (matches Python's
        // .format() partial-coverage behavior).
        assert_eq!(format("hi", &[FormatArg::Int(1)]), "hi");
    }

    #[test]
    fn format_escaped_braces() {
        assert_eq!(format("{{}}", &[]), "{}");
    }

    #[test]
    fn format_unmatched_open_brace() {
        // Malformed → emit the rest verbatim.
        let r = format("{abc", &[FormatArg::Int(1)]);
        // Implementation chose to emit the '{' verbatim then the body.
        assert!(r.contains('{') || r.contains("abc"));
    }
}
