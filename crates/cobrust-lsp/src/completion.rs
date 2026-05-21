//! `textDocument/completion` handler — ADR-0057c §3.2.
//!
//! Sources three candidate tiers for completion:
//!
//! 1. **PRELUDE functions** (§3.3) — hardcoded catalogue of Cobrust
//!    built-ins from `build.rs` PRELUDE. `sortText` prefix `"0_"`.
//! 2. **In-scope bindings** from `TypeCheckCtx::bindings()` — every
//!    `let`-binding known to the incremental type context. `sortText`
//!    prefix `"1_"`.
//! 3. **Keywords** — Cobrust language keywords. `sortText` prefix `"2_"`.
//!
//! Filtering: case-sensitive prefix match. Empty prefix returns all items.
//!
//! TODO(#hover-prelude-sync): wave-3 should query the live `TypeCheckCtx`
//! for PRELUDE definitions instead of using this hardcoded catalogue, so
//! new intrinsics added to `build.rs` are reflected automatically.

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, CompletionResponse};

use cobrust_types::TypeCheckCtx;

/// A single PRELUDE function entry in the completion catalogue.
struct PreludeFn {
    name: &'static str,
    detail: &'static str,
}

/// The hardcoded PRELUDE catalogue per ADR-0057c §3.3.
static PRELUDE_FNS: &[PreludeFn] = &[
    PreludeFn {
        name: "print",
        detail: "(s: Any) -> None",
    },
    PreludeFn {
        name: "len",
        detail: "(x: List[T] | Str | Bytes) -> Int",
    },
    PreludeFn {
        name: "range",
        detail: "(start: Int, stop: Int) -> List[Int]",
    },
    PreludeFn {
        name: "input",
        detail: "(prompt: Str = \"\") -> Str",
    },
    PreludeFn {
        name: "int",
        detail: "(x: Any) -> Int",
    },
    PreludeFn {
        name: "float",
        detail: "(x: Any) -> Float",
    },
    PreludeFn {
        name: "str",
        detail: "(x: Any) -> Str",
    },
    PreludeFn {
        name: "bool",
        detail: "(x: Any) -> Bool",
    },
    PreludeFn {
        name: "list",
        detail: "(x: Any) -> List[Any]",
    },
    PreludeFn {
        name: "dict",
        detail: "() -> Dict[Any, Any]",
    },
    PreludeFn {
        name: "set",
        detail: "(x: Any) -> Set[Any]",
    },
    PreludeFn {
        name: "abs",
        detail: "(x: Int | Float) -> Int | Float",
    },
    PreludeFn {
        name: "max",
        detail: "(a: T, b: T) -> T",
    },
    PreludeFn {
        name: "min",
        detail: "(a: T, b: T) -> T",
    },
    PreludeFn {
        name: "sum",
        detail: "(xs: List[Int | Float]) -> Int | Float",
    },
    PreludeFn {
        name: "sorted",
        detail: "(xs: List[T]) -> List[T]",
    },
    PreludeFn {
        name: "reversed",
        detail: "(xs: List[T]) -> List[T]",
    },
    PreludeFn {
        name: "enumerate",
        detail: "(xs: List[T]) -> List[(Int, T)]",
    },
    PreludeFn {
        name: "zip",
        detail: "(a: List[A], b: List[B]) -> List[(A, B)]",
    },
    PreludeFn {
        name: "map",
        detail: "(f: (T) -> U, xs: List[T]) -> List[U]",
    },
    PreludeFn {
        name: "filter",
        detail: "(f: (T) -> Bool, xs: List[T]) -> List[T]",
    },
    PreludeFn {
        name: "open",
        detail: "(path: Str, mode: Str) -> FileHandle",
    },
    PreludeFn {
        name: "argv",
        detail: "() -> List[Str]",
    },
];

/// Cobrust keywords (ADR-0057c §3.2 keyword tier).
/// `pub` so `rename.rs` can reuse the same list for the keyword guard
/// (ADR-0057d §3.1 — keywords are not rename-able).
pub static KEYWORDS: &[&str] = &[
    "def", "let", "mut", "if", "else", "elif", "for", "while", "break", "continue", "return",
    "class", "enum", "match", "with", "and", "or", "not", "True", "False", "None", "pass", "raise",
    "try", "except", "finally", "import", "from", "as", "in", "is", "yield", "async", "await",
    "type",
];

/// Extract the identifier prefix immediately to the left of (or at)
/// `byte_offset` in `source`. Returns `""` if there is no identifier
/// character there.
///
/// An identifier character is `[A-Za-z_][A-Za-z0-9_]*` (ASCII heuristic).
#[must_use]
pub fn prefix_at_offset(source: &str, byte_offset: usize) -> &str {
    let bytes = source.as_bytes();
    let len = bytes.len();
    let offset = byte_offset.min(len);

    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';

    // Determine the anchor byte — the rightmost ident char that is part
    // of the prefix the user has typed so far:
    //   - If byte at `offset` is ident → cursor is inside a token.
    //   - If byte at `offset` is NOT ident AND byte at `offset-1` IS ident
    //     AND `offset-1` is not preceded by a space/non-ident → cursor is
    //     right after the token (common trigger position after "print|").
    //   - Otherwise → no prefix.
    // IMPORTANT: we only back up ONE position so that a space/punctuation
    // at `offset` that has a word to its left still counts as
    // "cursor right after word", but `"let x = 1"[3]` (space with 't'
    // at 2) does NOT — the space is an explicit break.
    let anchor: usize = if offset < len && is_ident(bytes[offset]) {
        offset
    } else if offset > 0 && is_ident(bytes[offset - 1]) {
        // Back up only if the byte immediately before is ident AND the
        // character before that is not also ident (handled by the scan).
        offset - 1
    } else {
        return "";
    };

    // Scan forward to the end of the word from `anchor`.
    let mut end = anchor + 1;
    while end < len && is_ident(bytes[end]) {
        end += 1;
    }

    // Walk backwards to find the start.
    let mut start = anchor;
    while start > 0 && is_ident(bytes[start - 1]) {
        start -= 1;
    }

    // Safety: ASCII range checks above ensure valid UTF-8 slice.
    &source[start..end]
}

/// Build the PRELUDE tier of completion items.
///
/// Each item has `kind = Function`, `detail` = the signature string,
/// and `sortText = "0_" + name` so PRELUDE items rank first.
#[must_use]
pub fn prelude_items(prefix: &str) -> Vec<CompletionItem> {
    PRELUDE_FNS
        .iter()
        .filter(|f| f.name.starts_with(prefix))
        .map(|f| CompletionItem {
            label: f.name.to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(f.detail.to_string()),
            sort_text: Some(format!("0_{}", f.name)),
            ..Default::default()
        })
        .collect()
}

/// Build the in-scope binding tier of completion items from the
/// `TypeCheckCtx`. Each item has `kind = Variable`, `detail` = the
/// type display string, and `sortText = "1_" + name`.
#[must_use]
pub fn scope_items(ctx: &TypeCheckCtx, prefix: &str) -> Vec<CompletionItem> {
    ctx.bindings()
        .filter(|(name, _)| name.starts_with(prefix))
        .map(|(name, ty)| CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some(format!("{ty}")),
            sort_text: Some(format!("1_{name}")),
            ..Default::default()
        })
        .collect()
}

/// Build the keyword tier of completion items. Each item has
/// `kind = Keyword` and `sortText = "2_" + keyword`.
#[must_use]
pub fn keyword_items(prefix: &str) -> Vec<CompletionItem> {
    KEYWORDS
        .iter()
        .filter(|kw| kw.starts_with(prefix))
        .map(|kw| CompletionItem {
            label: kw.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            sort_text: Some(format!("2_{kw}")),
            ..Default::default()
        })
        .collect()
}

/// Build the full completion response for a request at `byte_offset`
/// in `source`.
///
/// Combines PRELUDE + scope + keyword tiers, filtered by the
/// identifier prefix at the cursor. Returns a flat
/// `CompletionResponse::Array`.
#[must_use]
pub fn build_completion_response(
    source: &str,
    byte_offset: usize,
    ctx: &TypeCheckCtx,
) -> CompletionResponse {
    let prefix = prefix_at_offset(source, byte_offset);
    let mut items: Vec<CompletionItem> = Vec::new();
    items.extend(prelude_items(prefix));
    items.extend(scope_items(ctx, prefix));
    items.extend(keyword_items(prefix));
    CompletionResponse::Array(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_at_offset_middle_of_word() {
        let src = "let foo = 1";
        // 'o' is at index 6; prefix should be "foo".
        assert_eq!(prefix_at_offset(src, 6), "foo");
    }

    #[test]
    fn prefix_at_offset_after_word() {
        let src = "print(";
        // cursor at byte 5 — just before '(', last ident char is 't'.
        assert_eq!(prefix_at_offset(src, 5), "print");
    }

    #[test]
    fn prefix_at_offset_on_equals_sign() {
        let src = "let x = 1";
        // '=' at byte 6 — not an ident char, no ident immediately before '='.
        // Byte 5 is ' ' (space) — not ident.
        assert_eq!(prefix_at_offset(src, 6), "");
    }

    #[test]
    fn prefix_at_offset_empty() {
        assert_eq!(prefix_at_offset("", 0), "");
    }

    #[test]
    fn prelude_items_no_prefix_returns_all() {
        let items = prelude_items("");
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"print"), "print must be in PRELUDE");
        assert!(labels.contains(&"len"), "len must be in PRELUDE");
        assert!(labels.contains(&"range"), "range must be in PRELUDE");
    }

    #[test]
    fn prelude_items_prefix_filters() {
        let items = prelude_items("pr");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "print");
    }

    #[test]
    fn keyword_items_no_prefix_includes_let() {
        let items = keyword_items("");
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"let"), "let must be in keywords");
        assert!(labels.contains(&"def"), "def must be in keywords");
        assert!(labels.contains(&"if"), "if must be in keywords");
    }

    #[test]
    fn scope_items_from_ctx() {
        use cobrust_frontend::span::FileId;
        use cobrust_types::{TypeCheckCtx, check_incremental};

        let source = "let alpha = 42\n";
        let mut ctx = TypeCheckCtx::new();
        let ast = cobrust_frontend::parse_str(source, FileId::SYNTHETIC).unwrap();
        let mut hir_sess = cobrust_hir::lower::Session::new();
        let hir = cobrust_hir::lower::lower(&ast, &mut hir_sess).unwrap();
        let _ = check_incremental(&mut ctx, &hir, 1);

        let items = scope_items(&ctx, "");
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"alpha"), "alpha must appear as scope item");
    }

    #[test]
    fn build_completion_response_prefix_pri() {
        let ctx = TypeCheckCtx::new();
        let source = "pri";
        let resp = build_completion_response(source, 3, &ctx);
        if let CompletionResponse::Array(items) = resp {
            let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
            assert!(labels.contains(&"print"), "print should match prefix 'pri'");
            // No keyword starts with 'pri'.
            assert!(!labels.contains(&"let"), "'let' must not match 'pri'");
        } else {
            panic!("expected CompletionResponse::Array");
        }
    }
}
