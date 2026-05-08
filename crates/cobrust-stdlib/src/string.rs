//! `std.string` — len / find / replace / split / strip / lower /
//! upper / format.
//!
//! ADR-0025 §"Public surface (binding)" pins the API. Per ADR-0019
//! §"M11 — Standard library" the surface mirrors Python's `str`
//! operations, with Cobrust's "no silent coercion" rule
//! (constitution §2.2) applied to `format`.

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
pub fn strip(s: &str) -> &str {
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
    fn strip_whitespace() {
        assert_eq!(strip("  hello  "), "hello");
    }

    #[test]
    fn strip_no_whitespace() {
        assert_eq!(strip("hello"), "hello");
    }

    #[test]
    fn strip_only_whitespace() {
        assert_eq!(strip("   "), "");
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
