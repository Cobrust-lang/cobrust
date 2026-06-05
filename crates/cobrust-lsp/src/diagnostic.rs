//! Cobrust error → LSP `Diagnostic` conversion.
//!
//! Per ADR-0057a §3, every `TypeError + MirError + LoweringError`
//! construction site is mapped to a `Diagnostic` whose `message` is
//! the canonical `thiserror::#[error("...")]` diagnosis and whose
//! `related_information[0].message` is the ADR-0052b `suggestion`
//! field verbatim.
//!
//! Cannot impl `From<&Error> for Diagnostic` for the foreign
//! `lsp_types::Diagnostic` directly because the conversion needs the
//! per-document [`LineMap`] context. Instead this module exposes a
//! handful of free helpers (`type_error_to_diagnostics`,
//! `mir_error_to_diagnostic`, `lowering_error_to_diagnostic`,
//! `frontend_error_to_diagnostic`) that each take an `&Error` plus a
//! `&LineMap` and return a `Diagnostic` (or `Vec<Diagnostic>` for the
//! `TypeError::Multiple` variant).

use cobrust_frontend::error::{FrontendError, LexError, ParseError};
use cobrust_frontend::span::Span;
use cobrust_hir::LoweringError;
use cobrust_mir::MirError;
use cobrust_types::TypeError;
use serde_json::json;
use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, Location, NumberOrString, Range,
    Url,
};

use crate::span_convert::{LineMap, span_to_lsp_range};

/// JSON key under `Diagnostic.data` carrying the ADR-0062 FixSafety
/// tier code as `u8`. Read by `code_action.rs::build_code_actions` to
/// route quick-fix UI per tier without re-classifying the error.
///
/// ADR-0057e §3.2 wire-shape: `{"fix_safety": <u8>}`. Forward-compatible
/// — future codeAction extensions add sibling JSON keys without
/// breaking the tier read path.
pub const DIAG_DATA_FIX_SAFETY_KEY: &str = "fix_safety";

/// Source string written into every emitted `Diagnostic.source`.
pub const DIAG_SOURCE: &str = "cobrust";

/// Build a minimal `Diagnostic` shell. Callers fill in `code`,
/// `message`, and `related_information` as appropriate.
fn make_diagnostic(range: Range, message: String, code: &str) -> Diagnostic {
    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(NumberOrString::String(code.to_string())),
        code_description: None,
        source: Some(DIAG_SOURCE.to_string()),
        message,
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Attach a `suggestion` (when present) as `related_information[0]`.
///
/// Per ADR-0057a §3.1: the structured `suggestion` text lives in
/// `related_information[0].message` verbatim. Wave-1 uses a synthetic
/// URI (`cobrust://synthetic`) for the related location because the
/// per-document URI is not in scope inside the conversion helpers; the
/// agent-LLM consumer reads the message field, not the URI.
fn with_suggestion(
    mut diag: Diagnostic,
    suggestion: Option<&'static str>,
    range: Range,
) -> Diagnostic {
    if let Some(s) = suggestion {
        let placeholder_uri = Url::parse("cobrust://synthetic").expect("static URL parses");
        diag.related_information = Some(vec![DiagnosticRelatedInformation {
            location: Location {
                uri: placeholder_uri,
                range,
            },
            message: s.to_string(),
        }]);
    }
    diag
}

/// Map a `TypeError` to one or more LSP `Diagnostic`s.
///
/// Most variants emit exactly one `Diagnostic`. `TypeError::Multiple`
/// flattens its inner vector and emits one per inner error.
///
/// Per ADR-0057a §5 every emitted diagnostic uses `Error` severity;
/// the `code` field carries the variant discriminant string so
/// editor-side `code action` providers can route on it.
#[must_use]
pub fn type_error_to_diagnostics(err: &TypeError, line_map: &LineMap) -> Vec<Diagnostic> {
    match err {
        TypeError::Multiple(inner) => inner
            .iter()
            .flat_map(|e| type_error_to_diagnostics(e, line_map))
            .collect(),
        _ => vec![type_error_to_diagnostic_single(err, line_map)],
    }
}

/// Single-variant `TypeError` → `Diagnostic`. Panics on
/// `TypeError::Multiple`; callers should funnel through
/// [`type_error_to_diagnostics`] which flattens.
fn type_error_to_diagnostic_single(err: &TypeError, line_map: &LineMap) -> Diagnostic {
    use TypeError::*;
    let (span, suggestion, code) = match err {
        UnknownName {
            span, suggestion, ..
        } => (*span, *suggestion, "unknown-name"),
        ArityMismatch {
            span, suggestion, ..
        } => (*span, *suggestion, "arity-mismatch"),
        KeywordArgMismatch {
            span, suggestion, ..
        } => (*span, *suggestion, "keyword-arg-mismatch"),
        MissingArgument {
            span, suggestion, ..
        } => (*span, *suggestion, "missing-argument"),
        TypeMismatch {
            span, suggestion, ..
        } => (*span, *suggestion, "type-mismatch"),
        NonExhaustiveMatch {
            span, suggestion, ..
        } => (*span, *suggestion, "non-exhaustive-match"),
        RowConflict {
            span, suggestion, ..
        } => (*span, *suggestion, "row-conflict"),
        ImplicitTruthiness {
            span, suggestion, ..
        } => (*span, *suggestion, "implicit-truthiness"),
        UseOfDroppedFeature {
            span, suggestion, ..
        } => (*span, *suggestion, "dropped-feature"),
        MutableDefault { span, suggestion } => (*span, *suggestion, "mutable-default"),
        AmbiguousType { span, suggestion } => (*span, *suggestion, "ambiguous-type"),
        DuplicateField {
            span, suggestion, ..
        } => (*span, *suggestion, "duplicate-field"),
        OccursCheck {
            span, suggestion, ..
        } => (*span, *suggestion, "occurs-check"),
        NotCallable {
            span, suggestion, ..
        } => (*span, *suggestion, "not-callable"),
        NotIndexable {
            span, suggestion, ..
        } => (*span, *suggestion, "not-indexable"),
        NotIterable {
            span, suggestion, ..
        } => (*span, *suggestion, "not-iterable"),
        BreakOutsideLoop { span, suggestion } => (*span, *suggestion, "break-outside-loop"),
        ContinueOutsideLoop { span, suggestion } => (*span, *suggestion, "continue-outside-loop"),
        ReturnOutsideFn { span, suggestion } => (*span, *suggestion, "return-outside-fn"),
        YieldOutsideFn { span, suggestion } => (*span, *suggestion, "yield-outside-fn"),
        NotHashable {
            span, suggestion, ..
        } => (*span, *suggestion, "not-hashable"),
        DictSpreadNotSupported { span, suggestion } => {
            (*span, *suggestion, "dict-spread-unsupported")
        }
        BorrowOfNonPlace { span, suggestion } => (*span, *suggestion, "borrow-of-non-place"),
        UnknownMethod {
            span, suggestion, ..
        } => (*span, *suggestion, "unknown-method"),
        CallbackArgMustBeFnName { span, suggestion } => {
            (*span, *suggestion, "callback-arg-must-be-fn-name")
        }
        CallbackSignatureMismatch {
            span, suggestion, ..
        } => (*span, *suggestion, "callback-signature-mismatch"),
        // ADR-0080 Phase-1a — typed field access on a class instance.
        UnknownField {
            span, suggestion, ..
        } => (*span, *suggestion, "unknown-field"),
        // ADR-0080 Phase-1b-ii — non-fixed-grammar refinement predicate.
        UnsupportedRefinement {
            span, suggestion, ..
        } => (*span, *suggestion, "unsupported-refinement"),
        // ADR-0088 §3 — `len(x)` on a non-sized argument.
        LenArgNotSized {
            span, suggestion, ..
        } => (*span, *suggestion, "len-arg-not-sized"),
        Multiple(_) => unreachable!("Multiple flattened by type_error_to_diagnostics"),
    };
    let range = span_to_lsp_range(&span, line_map);
    let diag = make_diagnostic(range, err.to_string(), code);
    let diag = with_suggestion(diag, suggestion, range);
    let fix_safety_code: u8 = cobrust_types::type_error_fix_safety(err) as u8;
    attach_fix_safety_data(diag, fix_safety_code)
}

/// Map a `MirError` to a single LSP `Diagnostic`.
///
/// Per ADR-0057a §3.2: `MirError::UseAfterMove` returns a `Diagnostic`
/// whose `suggestion` is the canonical "change to `&s` to borrow
/// without consuming" text. Wave-1 emits the diagnostic only — the
/// paired `CodeAction` (Quickfix) is ADR-0057d wave-4 territory.
///
/// `MirError::Internal` has no span; we emit a sentinel
/// `Range::default()` at line 0 col 0 so the editor still surfaces
/// the message.
#[must_use]
pub fn mir_error_to_diagnostic(err: &MirError, line_map: &LineMap) -> Diagnostic {
    use MirError::*;
    let (span_opt, suggestion, code) = match err {
        UseAfterMove {
            span, suggestion, ..
        } => (Some(*span), *suggestion, "use-after-move"),
        UseAfterDrop {
            span, suggestion, ..
        } => (Some(*span), *suggestion, "use-after-drop"),
        ConflictingMutBorrow {
            span, suggestion, ..
        } => (Some(*span), *suggestion, "conflicting-mut-borrow"),
        SharedMutOverlap {
            span, suggestion, ..
        } => (Some(*span), *suggestion, "shared-mut-overlap"),
        EscapingBorrow {
            span, suggestion, ..
        } => (Some(*span), *suggestion, "escaping-borrow"),
        DropMissing {
            span, suggestion, ..
        } => (Some(*span), *suggestion, "drop-missing"),
        DoubleDrop {
            span, suggestion, ..
        } => (Some(*span), *suggestion, "double-drop"),
        FieldOutOfBounds {
            span, suggestion, ..
        } => (Some(*span), *suggestion, "field-out-of-bounds"),
        UnresolvedDefId {
            span, suggestion, ..
        } => (Some(*span), *suggestion, "unresolved-defid"),
        NonExhaustiveSwitch { span, suggestion } => {
            (Some(*span), *suggestion, "non-exhaustive-switch")
        }
        Internal(_) => (None, None, "internal-mir"),
    };
    let range = span_opt
        .map(|s| span_to_lsp_range(&s, line_map))
        .unwrap_or_default();
    let diag = make_diagnostic(range, err.to_string(), code);
    let diag = with_suggestion(diag, suggestion, range);
    let fix_safety_code = cobrust_mir::mir_error_fix_safety_code(err);
    attach_fix_safety_data(diag, fix_safety_code)
}

/// Map a `LoweringError` to a single LSP `Diagnostic`.
#[must_use]
pub fn lowering_error_to_diagnostic(err: &LoweringError, line_map: &LineMap) -> Diagnostic {
    use LoweringError::*;
    let (span, suggestion, code) = match err {
        UnknownName {
            span, suggestion, ..
        } => (*span, *suggestion, "lower-unknown-name"),
        DroppedFeature {
            span, suggestion, ..
        } => (*span, *suggestion, "lower-dropped-feature"),
        MutableDefault { span, suggestion } => (*span, *suggestion, "lower-mutable-default"),
        OrPatternBindingMismatch { span, suggestion } => {
            (*span, *suggestion, "lower-or-pattern-mismatch")
        }
        DuplicateBinding {
            second, suggestion, ..
        } => (*second, *suggestion, "lower-duplicate-binding"),
        AssignToUnknown {
            span, suggestion, ..
        } => (*span, *suggestion, "lower-assign-unknown"),
        EcosystemDecoratorShape {
            span, suggestion, ..
        } => (*span, *suggestion, "lower-eco-decorator-shape"),
    };
    let range = span_to_lsp_range(&span, line_map);
    let diag = make_diagnostic(range, err.to_string(), code);
    let diag = with_suggestion(diag, suggestion, range);
    let fix_safety_code = cobrust_hir::lowering_error_fix_safety_code(err);
    attach_fix_safety_data(diag, fix_safety_code)
}

/// Attach the ADR-0062 FixSafety tier code (as u8) to a `Diagnostic.data`
/// JSON object under the [`DIAG_DATA_FIX_SAFETY_KEY`] key.
///
/// ADR-0057e §3.2 wire-shape consumed by `code_action::build_code_actions`
/// — the codeAction handler reads this key to decide which CodeAction
/// kind to emit (or whether to skip emission entirely) without
/// re-classifying the underlying error variant.
fn attach_fix_safety_data(mut diag: Diagnostic, fix_safety_code: u8) -> Diagnostic {
    diag.data = Some(json!({ DIAG_DATA_FIX_SAFETY_KEY: fix_safety_code }));
    diag
}

/// Map a `FrontendError` (lex / parse) to a single LSP `Diagnostic`.
///
/// `FrontendError` predates ADR-0052b (it has no `suggestion` field),
/// so the diagnostic carries only the diagnosis message. Phase J+
/// frontend error suggestions are out of scope for wave-1.
#[must_use]
pub fn frontend_error_to_diagnostic(err: &FrontendError, line_map: &LineMap) -> Diagnostic {
    use FrontendError::*;
    let (span, code) = match err {
        Lex(lex_err) => {
            let span = lex_error_span(lex_err);
            (span, "lex-error")
        }
        Parse(parse_err) => {
            let span = parse_error_span(parse_err);
            (span, "parse-error")
        }
    };
    let range = span_to_lsp_range(&span, line_map);
    make_diagnostic(range, err.to_string(), code)
}

fn lex_error_span(err: &LexError) -> Span {
    use LexError::*;
    match err {
        InvalidUtf8 { byte_offset } => Span::new(
            cobrust_frontend::span::FileId::SYNTHETIC,
            *byte_offset,
            *byte_offset,
        ),
        UnexpectedChar { span, .. }
        | UnterminatedString { span }
        | UnterminatedFString { span }
        | MalformedNumber { span }
        | InconsistentIndent { span }
        | InvalidEscape { span } => *span,
    }
}

fn parse_error_span(err: &ParseError) -> Span {
    use ParseError::*;
    match err {
        Expected { span, .. }
        | Syntax { span, .. }
        | UnexpectedEof { span, .. }
        | DroppedByConstitution { span, .. }
        | NonLiteralDefault { span, .. }
        | IndentError { span, .. }
        | ExpressionTooDeep { span, .. } => *span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cobrust_frontend::span::FileId;
    use cobrust_types::ty::Ty;

    fn span(start: u32, end: u32) -> Span {
        Span::new(FileId::SYNTHETIC, start, end)
    }

    #[test]
    fn type_mismatch_carries_suggestion() {
        let err = TypeError::TypeMismatch {
            expected: Ty::Int,
            actual: Ty::Str,
            span: span(0, 5),
            suggestion: Some("change to `: str`"),
        };
        let lm = LineMap::from_source("hello");
        let diags = type_error_to_diagnostics(&err, &lm);
        assert_eq!(diags.len(), 1);
        let d = &diags[0];
        assert_eq!(d.severity, Some(DiagnosticSeverity::ERROR));
        assert!(d.message.contains("type mismatch"));
        let related = d.related_information.as_ref().expect("suggestion present");
        assert_eq!(related[0].message, "change to `: str`");
        assert_eq!(d.code, Some(NumberOrString::String("type-mismatch".into())));
    }

    #[test]
    fn implicit_truthiness_canonical_shape() {
        let err = TypeError::ImplicitTruthiness {
            actual: Ty::Int,
            span: span(3, 4),
            suggestion: Some("change to `if x != 0:` (use `.is_some()` for Option)"),
        };
        let lm = LineMap::from_source("if x:\n    pass\n");
        let diags = type_error_to_diagnostics(&err, &lm);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("truthiness"));
        assert_eq!(
            diags[0].code,
            Some(NumberOrString::String("implicit-truthiness".into()))
        );
    }

    #[test]
    fn type_error_multiple_flattens() {
        let err = TypeError::Multiple(vec![
            TypeError::ImplicitTruthiness {
                actual: Ty::Int,
                span: span(0, 1),
                suggestion: None,
            },
            TypeError::ImplicitTruthiness {
                actual: Ty::Str,
                span: span(2, 3),
                suggestion: None,
            },
        ]);
        let lm = LineMap::from_source("xxxxxx");
        let diags = type_error_to_diagnostics(&err, &lm);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn mir_use_after_move_emits_suggestion() {
        let err = MirError::UseAfterMove {
            local: 0,
            span: span(4, 5),
            suggestion: Some(
                "change to `&s` to borrow without consuming (ADR-0052a explicit shared borrow)",
            ),
        };
        let lm = LineMap::from_source("let s = String\n");
        let d = mir_error_to_diagnostic(&err, &lm);
        let related = d.related_information.as_ref().expect("suggestion present");
        assert!(related[0].message.contains("&s"));
        assert_eq!(
            d.code,
            Some(NumberOrString::String("use-after-move".into()))
        );
    }

    #[test]
    fn mir_internal_has_default_range() {
        let err = MirError::Internal("bug".to_string());
        let lm = LineMap::from_source("");
        let d = mir_error_to_diagnostic(&err, &lm);
        assert_eq!(d.range, Range::default());
        assert!(d.message.contains("internal MIR error"));
    }

    #[test]
    fn lowering_unknown_name_maps() {
        let err = LoweringError::UnknownName {
            name: "x".into(),
            span: span(0, 1),
            suggestion: Some("did you mean `y`?"),
        };
        let lm = LineMap::from_source("x\n");
        let d = lowering_error_to_diagnostic(&err, &lm);
        assert!(d.message.contains("unknown name"));
        let related = d.related_information.as_ref().expect("suggestion present");
        assert_eq!(related[0].message, "did you mean `y`?");
    }
}
