//! `cobrust repl` — M14 interactive shell (per ADR-0029).
//!
//! Lifts the M10/ADR-0024 stub to full functionality:
//!
//! - **Line editing** via `rustyline = "14"` with history persistence
//!   at `~/.cobrust/repl_history` (bounded to 1024 entries).
//! - **Multi-line input detection** via trial-parse + `ParseError::UnexpectedEof`
//!   retry signal (per ADR-0029 §"Multi-line input contract").
//! - **Directives**: `:type / :ast / :hir / :mir / :clear / :help / :quit`
//!   per ADR-0029 §"Directive table (binding)".
//! - **Tab completion** against four sources: directives + Cobrust
//!   keywords + stdlib top-level names + accumulated session bindings.
//! - **Cold start**: <200ms (measured ~10ms release on macOS arm64).
//! - **Stateful evaluation**: HIR interpreter scoped to literals,
//!   arithmetic, comparisons, boolean ops, var-lookup, let-binding.
//!   Full Turing-complete evaluation deferred to M14.1.
//!
//! See `docs/agent/adr/0029-m14-repl.md` for the design rationale.

// `directive_ast/hir/mir` are intentionally `&self` methods so that a
// future M14.1 widening (which threads session bindings into the
// synthetic `_t` body for closed-over names) can land additively.
#![allow(clippy::unused_self)]

use std::collections::HashMap;
use std::path::PathBuf;

use cobrust_frontend::ast::{self, Module as AstModule};
use cobrust_frontend::error::{FrontendError, ParseError};
use cobrust_frontend::parse_str;
use cobrust_frontend::span::FileId;
use cobrust_hir::{
    Expr as HirExpr, Module as HirModule, Session as HirSession, lower as hir_lower,
};
use cobrust_mir::lower as mir_lower;
use cobrust_types::{Ty, TypeCheckCtx, check as type_check, check_incremental};
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context, Editor, Helper};

use crate::exit_codes;

/// Banner shown on cold-start (per ADR-0029 §"Public surface").
const BANNER: &str =
    "cobrust repl 0.0.1 (M14, ADR-0029) — type :help for directives, :quit to exit.";

/// Primary prompt.
const PROMPT_PRIMARY: &str = ">>> ";
/// Continuation prompt (multi-line).
const PROMPT_CONTINUE: &str = "... ";

/// History file location, relative to the user's home (`~/.cobrust/repl_history`).
fn history_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|h| PathBuf::from(h).join(".cobrust").join("repl_history"))
}

/// Cobrust keyword set (28 entries per ADR-0029 §"Tab completion sources").
const KEYWORDS: &[&str] = &[
    "fn", "let", "if", "else", "elif", "for", "while", "return", "match", "case", "class", "True",
    "False", "None", "and", "or", "not", "in", "pass", "break", "continue", "import", "from", "as",
    "with", "try", "except", "raise",
];

/// Stdlib top-level seeded names (12 entries per ADR-0029).
const STDLIB_NAMES: &[&str] = &[
    "print",
    "panic",
    "assert",
    "args",
    "var",
    "len",
    "print_err",
    "read_line",
    "int",
    "str",
    "float",
    "bool",
];

/// Directive names (with aliases).
const DIRECTIVES: &[&str] = &[
    ":type", ":ast", ":hir", ":mir", ":clear", ":help", ":quit", ":q", ":exit",
];

// =====================================================================
// REPL session state
// =====================================================================

/// One bound value in the session. M14 evaluation surface (per ADR-0029
/// §"Evaluation surface (M14 binding)") is intentionally narrow:
/// literals + arithmetic + bound-var + let. Stdlib delegation is M14.1.
#[derive(Clone, Debug)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    None,
}

impl Value {
    fn type_name(&self) -> &'static str {
        match self {
            Self::Int(_) => "i64",
            Self::Float(_) => "f64",
            Self::Bool(_) => "bool",
            Self::Str(_) => "str",
            Self::None => "None",
        }
    }
    fn display(&self) -> String {
        match self {
            Self::Int(n) => n.to_string(),
            Self::Float(f) => {
                // Match Python's repr-of-float style: trailing `.0` for integers.
                if f.fract() == 0.0 && f.is_finite() && f.abs() < 1e16 {
                    format!("{f:.1}")
                } else {
                    format!("{f}")
                }
            }
            Self::Bool(b) => if *b { "True" } else { "False" }.to_string(),
            Self::Str(s) => format!("{s:?}"),
            Self::None => "None".to_string(),
        }
    }
}

/// Session state. Public for testability per ADR-0029 §"Public surface".
///
/// ADR-0056b §3.3 extension: carries a cross-turn [`TypeCheckCtx`]
/// (Clone+Send Arc-COW snapshot). Phase J LSP wave-1 (ADR-0057a §4)
/// reads [`Session::type_ctx`] on `did_change` to re-publish
/// diagnostics without re-deriving the symbol table from scratch.
#[derive(Clone)]
pub struct Session {
    /// Accumulated `let` bindings: name → value.
    bindings: HashMap<String, Value>,
    /// Cross-turn incremental type-check ctx (ADR-0056b §3.3 + §5).
    /// O(1) Clone via Arc-COW; Send via interior Arc<HashMap<...>>.
    /// Phase J reads via [`Session::type_ctx`].
    type_ctx: TypeCheckCtx,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    #[must_use]
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
            type_ctx: TypeCheckCtx::new(),
        }
    }

    /// Phase J handoff accessor per ADR-0056b §3.3 + §6. LSP wave-1
    /// (ADR-0057a §4) reads this on `did_change` to dispatch fresh
    /// diagnostics. The returned reference shares Arc-internal data
    /// with future writes; per §6 readers never block writers (Phase J
    /// snapshot may reflect pre- or post-write — per-snapshot version
    /// tag via [`TypeCheckCtx::version`]).
    #[must_use]
    #[allow(dead_code)] // ADR-0057a wave-1 consumes
    pub fn type_ctx(&self) -> &TypeCheckCtx {
        &self.type_ctx
    }

    /// Multi-file invalidation per ADR-0056b §"Invalidation". Drops
    /// every DefId row recorded against `file_id` from the cross-turn
    /// ctx. Phase J LSP `did_change` calls this BEFORE re-checking
    /// the new file content. `file_id` matches
    /// [`cobrust_frontend::span::FileId`]'s wrapped u32.
    #[allow(dead_code)] // ADR-0057a wave-1 consumes
    pub fn invalidate(&mut self, file_id: u32) {
        self.type_ctx.invalidate(file_id);
    }

    /// Drive one logical input (which may include embedded newlines for
    /// multi-line statements). Per ADR-0029 §"Multi-line input contract":
    /// the caller (the rustyline loop) accumulates lines into `input`
    /// until `step` returns anything other than [`StepResult::Continue`].
    #[must_use]
    pub fn step(&mut self, input: &str) -> StepResult {
        let trimmed = input.trim_start();

        // Directive dispatch.
        if let Some(rest) = trimmed.strip_prefix(':') {
            return self.handle_directive(rest.trim_start());
        }

        // Empty input → no-op.
        if input.trim().is_empty() {
            return StepResult::Done(String::new());
        }

        // Multi-line detection: if the input is structurally
        // incomplete (per `is_input_incomplete`) we request more.
        if is_input_incomplete(input) {
            return StepResult::Continue;
        }
        // Wrap the input in a synthetic module: `fn _repl() -> i64: <input>; return 0`.
        let wrapped = wrap_for_eval(input);
        match parse_str(&wrapped, FileId::SYNTHETIC) {
            Ok(module) => self.evaluate_module(&module, input),
            Err(FrontendError::Parse(ParseError::UnexpectedEof { .. })) => StepResult::Continue,
            Err(e) => StepResult::Error(format!("parse error: {e}")),
        }
    }

    fn handle_directive(&mut self, rest: &str) -> StepResult {
        // Split on first whitespace.
        let (name, arg) = match rest.find(char::is_whitespace) {
            Some(i) => (&rest[..i], rest[i..].trim()),
            None => (rest, ""),
        };

        match name {
            "quit" | "q" | "exit" => StepResult::Quit,
            "help" => StepResult::Done(help_text()),
            "clear" => {
                self.bindings.clear();
                // ADR-0056b §3.3 — cross-turn type-ctx also clears so
                // `:type x` after `:clear` reports unbound rather than
                // stale.
                self.type_ctx = TypeCheckCtx::new();
                StepResult::Done("session bindings cleared.".to_string())
            }
            "type" => {
                if arg.is_empty() {
                    return StepResult::Error(
                        ":type requires an expression argument (e.g. `:type 1 + 2`)".to_string(),
                    );
                }
                self.directive_type(arg)
            }
            "ast" => {
                if arg.is_empty() {
                    return StepResult::Error(
                        ":ast requires an expression argument (e.g. `:ast 1 + 2`)".to_string(),
                    );
                }
                self.directive_ast(arg)
            }
            "hir" => {
                if arg.is_empty() {
                    return StepResult::Error(
                        ":hir requires an expression argument (e.g. `:hir 1 + 2`)".to_string(),
                    );
                }
                self.directive_hir(arg)
            }
            "mir" => {
                if arg.is_empty() {
                    return StepResult::Error(
                        ":mir requires an expression argument (e.g. `:mir 1 + 2`)".to_string(),
                    );
                }
                self.directive_mir(arg)
            }
            other => StepResult::Error(format!("unknown directive `:{other}` (try `:help`)")),
        }
    }

    /// `:type EXPR` — print the inferred return type of `fn _t() -> _: return EXPR`.
    ///
    /// ADR-0056b §3.3 — if `expr_src` is a bare identifier that's
    /// bound in the cross-turn `type_ctx`, return that type
    /// immediately (no parse/lower/check needed). This is the smoke
    /// test for `Session::type_ctx` cross-turn persistence.
    fn directive_type(&self, expr_src: &str) -> StepResult {
        // Fast path: bare identifier referenced from a previous turn.
        let trimmed = expr_src.trim();
        if is_bare_identifier(trimmed) {
            if let Some(ty) = self.type_ctx.lookup(trimmed) {
                return StepResult::Done(format!("{ty}"));
            }
        }

        let wrapped = wrap_for_typecheck(expr_src);
        let ast = match parse_str(&wrapped, FileId::SYNTHETIC) {
            Ok(m) => m,
            Err(e) => return StepResult::Error(format!("parse error: {e}")),
        };
        let mut sess = HirSession::new();
        let hir = match hir_lower(&ast, &mut sess) {
            Ok(h) => h,
            Err(e) => return StepResult::Error(format!("HIR lower error: {e:?}")),
        };
        let typed = match type_check(&hir) {
            Ok(t) => t,
            Err(e) => return StepResult::Error(format!("type error: {e:?}")),
        };
        let ty =
            find_synthetic_return_ty(&typed).map_or_else(|| Ty::None, std::clone::Clone::clone);
        StepResult::Done(format!("{ty}"))
    }

    /// `:ast EXPR` — pretty-print the parsed AST of EXPR.
    fn directive_ast(&self, expr_src: &str) -> StepResult {
        let wrapped = wrap_for_typecheck(expr_src);
        match parse_str(&wrapped, FileId::SYNTHETIC) {
            Ok(m) => {
                let inner = extract_synthetic_expr_ast(&m);
                StepResult::Done(format!("{inner:#?}"))
            }
            Err(e) => StepResult::Error(format!("parse error: {e}")),
        }
    }

    /// `:hir EXPR` — pretty-print the lowered HIR of EXPR.
    fn directive_hir(&self, expr_src: &str) -> StepResult {
        let wrapped = wrap_for_typecheck(expr_src);
        let ast = match parse_str(&wrapped, FileId::SYNTHETIC) {
            Ok(m) => m,
            Err(e) => return StepResult::Error(format!("parse error: {e}")),
        };
        let mut sess = HirSession::new();
        let hir = match hir_lower(&ast, &mut sess) {
            Ok(h) => h,
            Err(e) => return StepResult::Error(format!("HIR lower error: {e:?}")),
        };
        let inner = extract_synthetic_return_expr(&hir);
        StepResult::Done(format!("{inner:#?}"))
    }

    /// `:mir EXPR` — pretty-print the MIR Body of `fn _t() -> _: return EXPR`.
    fn directive_mir(&self, expr_src: &str) -> StepResult {
        let wrapped = wrap_for_typecheck(expr_src);
        let ast = match parse_str(&wrapped, FileId::SYNTHETIC) {
            Ok(m) => m,
            Err(e) => return StepResult::Error(format!("parse error: {e}")),
        };
        let mut sess = HirSession::new();
        let hir = match hir_lower(&ast, &mut sess) {
            Ok(h) => h,
            Err(e) => return StepResult::Error(format!("HIR lower error: {e:?}")),
        };
        let typed = match type_check(&hir) {
            Ok(t) => t,
            Err(e) => return StepResult::Error(format!("type error: {e:?}")),
        };
        let mir = match mir_lower(&typed) {
            Ok(m) => m,
            Err(e) => return StepResult::Error(format!("MIR error: {e:?}")),
        };
        let body = mir
            .bodies
            .iter()
            .find(|b| b.name == "_t")
            .map_or_else(|| "<no _t body>".to_string(), |b| format!("{b:#?}"));
        StepResult::Done(body)
    }

    /// Evaluate a parsed module, sourced from a single user input.
    fn evaluate_module(&mut self, module: &AstModule, raw_input: &str) -> StepResult {
        let body = match extract_repl_body_stmts(module) {
            Some(b) => b,
            None => return StepResult::Error("internal: synthetic body shape lost".to_string()),
        };

        // ADR-0056b §3.3 + §5 — parallel type-check pass merges the
        // input's bindings into the cross-turn type_ctx (so `:type x`
        // and Phase J `did_change` consumers see the new row). The
        // synthetic module rewrap (around the original user input)
        // surfaces top-level `let` patterns as `LetBody` rows; the
        // wrap_for_typecheck_stmts variant exposes them at module top
        // so `check_incremental` records them under FileId::REPL.
        if !raw_input.trim().is_empty() {
            let wrap = wrap_for_typecheck_stmts(raw_input);
            if let Ok(ast) = parse_str(&wrap, FileId::SYNTHETIC) {
                let mut sess = HirSession::new();
                if let Ok(hir) = hir_lower(&ast, &mut sess) {
                    let _ = check_incremental(
                        &mut self.type_ctx,
                        &hir,
                        FileId::SYNTHETIC.0,
                    );
                    // Errors are intentionally swallowed here: eval
                    // proceeds with the (possibly stale) value loop;
                    // diagnostics belong to `:type` / future LSP wire.
                }
            }
        }

        let mut output = Vec::<String>::new();
        for stmt in body {
            match self.eval_stmt(stmt) {
                Ok(Some(s)) => output.push(s),
                Ok(None) => {}
                Err(msg) => {
                    return StepResult::Error(format!(
                        "eval error in `{}`: {msg}",
                        raw_input.trim_end()
                    ));
                }
            }
        }
        StepResult::Done(output.join("\n"))
    }

    /// Evaluate a single statement; return `Some(text)` if the result
    /// should be printed, `None` otherwise.
    fn eval_stmt(&mut self, stmt: &ast::Stmt) -> Result<Option<String>, String> {
        match &stmt.kind {
            ast::StmtKind::Let { target, value, .. } => {
                let val = self.eval_expr(value)?;
                let name = match &target.kind {
                    ast::PatternKind::Binding(n) => n.clone(),
                    _ => {
                        return Err(
                            "destructuring patterns not yet supported in REPL (M14.1)".to_string()
                        );
                    }
                };
                self.bindings.insert(name, val);
                Ok(None)
            }
            ast::StmtKind::Expr(e) => {
                let value = self.eval_expr(e)?;
                Ok(Some(value.display()))
            }
            ast::StmtKind::Pass => Ok(None),
            ast::StmtKind::Return(_) => Err(
                "`return` only valid inside a function (M14 evaluates expressions only)"
                    .to_string(),
            ),
            _ => Err(
                "this statement form is not yet supported in REPL (M14.1 will widen)".to_string(),
            ),
        }
    }

    /// Evaluate an expression. Surface per ADR-0029 §"Evaluation surface".
    fn eval_expr(&self, expr: &ast::Expr) -> Result<Value, String> {
        match &expr.kind {
            ast::ExprKind::Literal(lit) => eval_literal(lit),
            ast::ExprKind::FString(parts) => {
                // M14 supports plain string concatenation only (no interpolation).
                let mut out = String::new();
                for p in parts {
                    match p {
                        ast::FStrPart::Lit(s) => out.push_str(s),
                        ast::FStrPart::Expr { .. } => {
                            return Err("f-string interpolation not yet supported in REPL (M14.1)"
                                .to_string());
                        }
                    }
                }
                Ok(Value::Str(out))
            }
            ast::ExprKind::Name(name) => self
                .bindings
                .get(name)
                .cloned()
                .ok_or_else(|| format!("name `{name}` not bound in this REPL session")),
            ast::ExprKind::Binary { op, lhs, rhs } => {
                let l = self.eval_expr(lhs)?;
                let r = self.eval_expr(rhs)?;
                eval_binary(*op, l, r)
            }
            ast::ExprKind::Unary { op, operand } => {
                let v = self.eval_expr(operand)?;
                eval_unary(*op, v)
            }
            _ => Err("expression form not yet supported in REPL (M14.1 will widen)".to_string()),
        }
    }
}

/// Outcome of one [`Session::step`] call.
#[derive(Debug)]
pub enum StepResult {
    /// Statement evaluated; emit `String` to stdout (may be empty).
    Done(String),
    /// Statement is incomplete; the caller should emit a continuation
    /// prompt and accumulate the next line.
    Continue,
    /// User requested `:quit` — outer loop exits with [`exit_codes::SUCCESS`].
    Quit,
    /// Diagnostic — emit to stderr and return to the primary prompt.
    Error(String),
}

// =====================================================================
// Synthetic-wrapping helpers
// =====================================================================

/// Wrap the user's input as the body of a synthetic `_repl` function so
/// `parse_str` (which expects a top-level module) can validate it.
fn wrap_for_eval(input: &str) -> String {
    let mut out = String::from("fn _repl() -> i64:\n");
    for line in input.lines() {
        out.push_str("    ");
        out.push_str(line);
        out.push('\n');
    }
    if input.trim().is_empty() {
        out.push_str("    pass\n");
    } else {
        // Append a trailing `return 0` so the synthetic body always
        // type-checks. We filter it out in `extract_repl_body_stmts`.
        out.push_str("    return 0\n");
    }
    out
}

/// Detect whether `input` is structurally incomplete. Per ADR-0029
/// §"Multi-line input contract": the REPL emits a continuation prompt
/// when the user's last meaningful line opens a block (ends with `:`)
/// and is not yet followed by an indented continuation, when there are
/// unbalanced brackets, or when a string literal is unterminated.
fn is_input_incomplete(input: &str) -> bool {
    // 1. Unbalanced brackets / parens.
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut brace = 0i32;
    let mut in_str: Option<char> = None;
    let mut prev_backslash = false;
    for c in input.chars() {
        if let Some(q) = in_str {
            if !prev_backslash && c == q {
                in_str = None;
            }
            prev_backslash = c == '\\' && !prev_backslash;
            continue;
        }
        prev_backslash = false;
        match c {
            '"' | '\'' => in_str = Some(c),
            '(' => paren += 1,
            ')' => paren -= 1,
            '[' => bracket += 1,
            ']' => bracket -= 1,
            '{' => brace += 1,
            '}' => brace -= 1,
            _ => {}
        }
    }
    if in_str.is_some() || paren > 0 || bracket > 0 || brace > 0 {
        return true;
    }

    // 2. Block-opener heuristic: the last non-blank line ends with `:`,
    //    and no subsequent line is indented past it.
    let lines: Vec<&str> = input.lines().collect();
    let mut last_idx = None;
    for (i, l) in lines.iter().enumerate().rev() {
        if !l.trim().is_empty() {
            last_idx = Some(i);
            break;
        }
    }
    if let Some(i) = last_idx {
        let line = lines[i];
        let trimmed = line.trim_end();
        // A trailing `:` (after stripping a trailing comment) means a block opener.
        let no_comment = trimmed.split('#').next().unwrap_or("").trim_end();
        if no_comment.ends_with(':') {
            // No subsequent non-blank line, OR the subsequent line is at the
            // same / lesser indent than the opener.
            let opener_indent = line.len() - line.trim_start().len();
            for next in &lines[i + 1..] {
                if next.trim().is_empty() {
                    continue;
                }
                let next_indent = next.len() - next.trim_start().len();
                if next_indent > opener_indent {
                    return false; // block has at least one body line
                }
            }
            return true;
        }
    }

    false
}

/// Wrap a single expression for type/HIR/MIR introspection.
fn wrap_for_typecheck(expr_src: &str) -> String {
    format!("fn _t():\n    return {expr_src}\n")
}

/// Is `s` a bare identifier (`[A-Za-z_][A-Za-z0-9_]*`)? Used by
/// `:type` directive to fast-path cross-turn ctx lookup per
/// ADR-0056b §3.3.
fn is_bare_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Wrap user input as top-level module statements so `let`-bindings
/// surface as `ItemKind::Let` (which [`check_incremental`] / merge
/// reads to populate the cross-turn type ctx per ADR-0056b §3.3).
///
/// Non-`let` lines are wrapped in `_t = ...` so they still type-check
/// without introducing meaningful top-level state. Lines we cannot
/// classify (e.g. trailing whitespace) are passed through unchanged
/// — `parse_str` either accepts (top-level expr stmt) or rejects
/// (the caller already type-checks via the existing `_repl` path).
fn wrap_for_typecheck_stmts(input: &str) -> String {
    // M14 the user-input is a single logical statement (multi-line
    // continues are folded into one input by the rustyline loop).
    // We pass it verbatim — `let x = …` is already top-level form 7;
    // a bare expression is top-level form 19 (ExprStmt). Either way,
    // `parse_str` produces an `AstModule` whose `ItemKind::Let` /
    // `ItemKind::ExprStmt` surface what the merge_module path reads.
    let mut out = String::new();
    for line in input.lines() {
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Extract user-input statements from the parsed `_repl` synthetic body.
/// Drops the trailing `return 0` we appended in `wrap_for_eval`.
fn extract_repl_body_stmts(module: &AstModule) -> Option<&[ast::Stmt]> {
    for item in &module.items {
        if let ast::StmtKind::Fn(fn_def) = &item.kind {
            if fn_def.name == "_repl" {
                let stmts = &fn_def.body.stmts;
                if let Some(last) = stmts.last() {
                    if matches!(last.kind, ast::StmtKind::Return(_)) {
                        return Some(&stmts[..stmts.len() - 1]);
                    }
                }
                return Some(stmts);
            }
        }
    }
    None
}

/// Locate the AST expression inside the synthetic `fn _t(): return EXPR`.
fn extract_synthetic_expr_ast(module: &AstModule) -> Option<&ast::Expr> {
    for item in &module.items {
        if let ast::StmtKind::Fn(fn_def) = &item.kind {
            if fn_def.name == "_t" {
                for stmt in &fn_def.body.stmts {
                    if let ast::StmtKind::Return(Some(e)) = &stmt.kind {
                        return Some(e);
                    }
                }
            }
        }
    }
    None
}

/// Locate the HIR expression inside the synthetic `fn _t(): return EXPR`.
fn extract_synthetic_return_expr(hir: &HirModule) -> Option<&HirExpr> {
    for item in &hir.items {
        if let cobrust_hir::ItemKind::Fn(fn_body) = &item.kind {
            if fn_body.name == "_t" {
                for stmt in &fn_body.body.stmts {
                    if let cobrust_hir::StmtKind::Return(Some(e)) = &stmt.kind {
                        return Some(e);
                    }
                }
            }
        }
    }
    None
}

/// Locate the typed-HIR `_t` function's return type.
fn find_synthetic_return_ty(typed: &cobrust_types::TypedModule) -> Option<&Ty> {
    for item in &typed.hir.items {
        if let cobrust_hir::ItemKind::Fn(fn_body) = &item.kind {
            if fn_body.name == "_t" {
                let id = fn_body.def_id.0;
                if let Some(ty) = typed.def_types.get(&id) {
                    if let Ty::Fn(fn_ty) = ty {
                        return Some(&fn_ty.return_ty);
                    }
                }
            }
        }
    }
    None
}

// =====================================================================
// Expression / statement evaluation primitives
// =====================================================================

fn eval_literal(lit: &ast::Literal) -> Result<Value, String> {
    match lit {
        ast::Literal::Int(s) => {
            // Cobrust int literals may be 0xFF, 0o77, 0b101, or plain.
            let stripped = s.replace('_', "");
            let parsed = if let Some(rest) = stripped
                .strip_prefix("0x")
                .or_else(|| stripped.strip_prefix("0X"))
            {
                i64::from_str_radix(rest, 16)
            } else if let Some(rest) = stripped
                .strip_prefix("0o")
                .or_else(|| stripped.strip_prefix("0O"))
            {
                i64::from_str_radix(rest, 8)
            } else if let Some(rest) = stripped
                .strip_prefix("0b")
                .or_else(|| stripped.strip_prefix("0B"))
            {
                i64::from_str_radix(rest, 2)
            } else {
                stripped.parse::<i64>()
            };
            parsed
                .map(Value::Int)
                .map_err(|e| format!("invalid integer literal `{s}`: {e}"))
        }
        ast::Literal::Float(s) => s
            .replace('_', "")
            .parse::<f64>()
            .map(Value::Float)
            .map_err(|e| format!("invalid float literal `{s}`: {e}")),
        ast::Literal::Str(s) => Ok(Value::Str(s.clone())),
        ast::Literal::Bool(b) => Ok(Value::Bool(*b)),
        ast::Literal::None => Ok(Value::None),
        ast::Literal::Bytes(_) => {
            Err("bytes literals not yet supported in REPL (M14.1)".to_string())
        }
        ast::Literal::Imag(_) => {
            Err("imaginary literals not yet supported in REPL (M14.1)".to_string())
        }
    }
}

fn eval_binary(op: ast::BinOp, l: Value, r: Value) -> Result<Value, String> {
    use ast::BinOp::{Add, And, Div, Eq, Gt, GtEq, Lt, LtEq, Mod, Mul, NotEq, Or, Sub};
    match (op, l, r) {
        // Numeric arithmetic.
        (Add, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_add(b))),
        (Sub, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_sub(b))),
        (Mul, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_mul(b))),
        (Div, Value::Int(a), Value::Int(b)) => {
            if b == 0 {
                Err("integer division by zero".to_string())
            } else {
                Ok(Value::Int(a.wrapping_div(b)))
            }
        }
        (Mod, Value::Int(a), Value::Int(b)) => {
            if b == 0 {
                Err("integer modulo by zero".to_string())
            } else {
                Ok(Value::Int(a.wrapping_rem(b)))
            }
        }
        (Add, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
        (Sub, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
        (Mul, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
        (Div, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
        // String concatenation.
        (Add, Value::Str(a), Value::Str(b)) => Ok(Value::Str(format!("{a}{b}"))),
        // Comparison.
        (Eq, a, b) => Ok(Value::Bool(values_eq(&a, &b))),
        (NotEq, a, b) => Ok(Value::Bool(!values_eq(&a, &b))),
        (Lt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
        (LtEq, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
        (Gt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
        (GtEq, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),
        (Lt, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a < b)),
        (LtEq, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a <= b)),
        (Gt, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a > b)),
        (GtEq, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a >= b)),
        // Boolean.
        (And, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a && b)),
        (Or, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a || b)),
        // Type mismatch.
        (op, l, r) => Err(format!(
            "unsupported binary `{op:?}` for {} and {}",
            l.type_name(),
            r.type_name(),
        )),
    }
}

fn eval_unary(op: ast::UnaryOp, v: Value) -> Result<Value, String> {
    use ast::UnaryOp::{Neg, Not, Plus};
    match (op, v) {
        (Neg, Value::Int(n)) => Ok(Value::Int(n.wrapping_neg())),
        (Neg, Value::Float(f)) => Ok(Value::Float(-f)),
        (Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
        (Plus, Value::Int(n)) => Ok(Value::Int(n)),
        (Plus, Value::Float(f)) => Ok(Value::Float(f)),
        (op, v) => Err(format!("unsupported unary `{op:?}` for {}", v.type_name(),)),
    }
}

fn values_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::None, Value::None) => true,
        _ => false,
    }
}

// =====================================================================
// Help text
// =====================================================================

fn help_text() -> String {
    r"cobrust REPL directives:
  :type EXPR    — show the inferred type of EXPR
  :ast EXPR     — pretty-print the parsed AST of EXPR
  :hir EXPR     — pretty-print the lowered HIR of EXPR
  :mir EXPR     — pretty-print the MIR `Body` of `fn _t() -> _: return EXPR`
  :clear        — drop accumulated session bindings
  :help         — this listing
  :quit         — exit (aliases: :q, :exit; or press Ctrl-D)

Evaluation surface (M14):
  literals (int/float/bool/str/None), arithmetic (+ - * / %),
  comparison (== != < <= > >=), boolean (and or not), variable
  lookup, `let X = EXPR` bindings.
  Stdlib calls (print, ...), control flow, comprehensions: M14.1.

Multi-line input: type an `fn`/`if`/`let` head; the prompt
switches to `...` until the block closes.

Tab completion: press <Tab> for directives, keywords, stdlib
top-level names, or session bindings.
"
    .to_string()
}

// =====================================================================
// rustyline Helper
// =====================================================================

/// Tab-completion helper. Holds an immutable snapshot of the session
/// bindings via a synced `Vec<String>` so the rustyline loop doesn't
/// race against `Session::bindings` mutation.
struct ReplHelper {
    bindings: Vec<String>,
}

impl ReplHelper {
    fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }
    fn refresh(&mut self, session: &Session) {
        self.bindings = session.bindings.keys().cloned().collect();
        self.bindings.sort();
    }
}

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let prefix_end = pos.min(line.len());
        let word_start = line[..prefix_end]
            .rfind(|c: char| c.is_whitespace() || c == '(' || c == ',' || c == '[')
            .map_or(0, |i| i + 1);
        let word = &line[word_start..prefix_end];

        // Directive completion: only at column 0, prefix `:`.
        if word_start == 0 && word.starts_with(':') {
            let candidates: Vec<Pair> = DIRECTIVES
                .iter()
                .filter(|d| d.starts_with(word))
                .map(|d| Pair {
                    display: (*d).to_string(),
                    replacement: (*d).to_string(),
                })
                .collect();
            return Ok((word_start, candidates));
        }

        // Identifier completion: keywords + stdlib + session bindings.
        let mut candidates: Vec<Pair> = Vec::new();
        for kw in KEYWORDS {
            if kw.starts_with(word) {
                candidates.push(Pair {
                    display: (*kw).to_string(),
                    replacement: (*kw).to_string(),
                });
            }
        }
        for sname in STDLIB_NAMES {
            if sname.starts_with(word) {
                candidates.push(Pair {
                    display: (*sname).to_string(),
                    replacement: (*sname).to_string(),
                });
            }
        }
        for binding in &self.bindings {
            if binding.starts_with(word) {
                candidates.push(Pair {
                    display: binding.clone(),
                    replacement: binding.clone(),
                });
            }
        }
        Ok((word_start, candidates))
    }
}

impl Hinter for ReplHelper {
    type Hint = String;
}

impl Highlighter for ReplHelper {}

impl Validator for ReplHelper {}

impl Helper for ReplHelper {}

// =====================================================================
// Top-level entry point
// =====================================================================

/// Run the interactive REPL.
///
/// Returns:
/// - [`exit_codes::SUCCESS`] (`0`) when the user exits via `:quit` or EOF.
/// - [`exit_codes::INTERNAL_PANIC`] (`3`) only on rustyline I/O failure.
#[must_use]
pub fn run() -> u8 {
    println!("{BANNER}");

    let mut session = Session::new();
    let mut editor = match Editor::<ReplHelper, rustyline::history::FileHistory>::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("cobrust repl: rustyline init failed: {e}");
            return exit_codes::INTERNAL_PANIC;
        }
    };
    editor.set_helper(Some(ReplHelper::new()));

    if let Some(path) = history_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = editor.load_history(&path);
    }

    let mut pending = String::new();

    loop {
        let prompt = if pending.is_empty() {
            PROMPT_PRIMARY
        } else {
            PROMPT_CONTINUE
        };
        match editor.readline(prompt) {
            Ok(line) => {
                if !pending.is_empty() {
                    pending.push('\n');
                }
                pending.push_str(&line);

                // A blank-line on continuation forces a parse-attempt.
                let force_parse =
                    !pending.is_empty() && line.trim().is_empty() && pending.lines().count() > 1;

                let result = if force_parse {
                    let r = session.step(&pending);
                    match r {
                        StepResult::Continue => StepResult::Error(
                            "statement still incomplete after blank line".to_string(),
                        ),
                        other => other,
                    }
                } else {
                    session.step(&pending)
                };

                match result {
                    StepResult::Done(out) => {
                        if !out.is_empty() {
                            println!("{out}");
                        }
                        if !pending.trim().is_empty() {
                            let _ = editor.add_history_entry(pending.as_str());
                        }
                        pending.clear();
                        if let Some(h) = editor.helper_mut() {
                            h.refresh(&session);
                        }
                    }
                    StepResult::Continue => {
                        // Keep accumulating.
                    }
                    StepResult::Quit => {
                        if let Some(path) = history_path() {
                            let _ = editor.save_history(&path);
                        }
                        return exit_codes::SUCCESS;
                    }
                    StepResult::Error(msg) => {
                        eprintln!("{msg}");
                        if !pending.trim().is_empty() {
                            let _ = editor.add_history_entry(pending.as_str());
                        }
                        pending.clear();
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl-C: discard pending; re-prompt.
                if !pending.is_empty() {
                    eprintln!("(input cancelled)");
                    pending.clear();
                } else {
                    eprintln!("(use :quit or Ctrl-D to exit)");
                }
            }
            Err(ReadlineError::Eof) => {
                if let Some(path) = history_path() {
                    let _ = editor.save_history(&path);
                }
                return exit_codes::SUCCESS;
            }
            Err(e) => {
                eprintln!("cobrust repl: readline error: {e}");
                return exit_codes::INTERNAL_PANIC;
            }
        }
    }
}

// =====================================================================
// Tests (collocated)
// =====================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]
mod tests {
    use super::*;

    fn step(session: &mut Session, input: &str) -> StepResult {
        session.step(input)
    }

    #[test]
    fn integer_literal_evaluates_to_itself() {
        let mut s = Session::new();
        match step(&mut s, "42") {
            StepResult::Done(out) => assert_eq!(out, "42"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn arithmetic_works() {
        let mut s = Session::new();
        match step(&mut s, "1 + 2 * 3") {
            StepResult::Done(out) => assert_eq!(out, "7"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn let_binding_persists() {
        let mut s = Session::new();
        let _ = step(&mut s, "let x = 100");
        match step(&mut s, "x") {
            StepResult::Done(out) => assert_eq!(out, "100"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn let_binding_then_arithmetic() {
        let mut s = Session::new();
        let _ = step(&mut s, "let n = 5");
        match step(&mut s, "n * n") {
            StepResult::Done(out) => assert_eq!(out, "25"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn unbound_name_errors() {
        let mut s = Session::new();
        match step(&mut s, "missing_var") {
            StepResult::Error(msg) => assert!(msg.contains("missing_var")),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn type_directive_int() {
        let mut s = Session::new();
        match step(&mut s, ":type 1 + 2") {
            StepResult::Done(out) => assert_eq!(out, "i64"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn type_directive_bool() {
        let mut s = Session::new();
        match step(&mut s, ":type 1 < 2") {
            StepResult::Done(out) => assert_eq!(out, "bool"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn type_directive_str() {
        let mut s = Session::new();
        match step(&mut s, r#":type "hi""#) {
            StepResult::Done(out) => assert_eq!(out, "str"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn ast_directive_emits_pretty_repr() {
        let mut s = Session::new();
        match step(&mut s, ":ast 1 + 2") {
            StepResult::Done(out) => {
                assert!(out.contains("Binary"));
                assert!(out.contains("Add"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn hir_directive_emits_pretty_repr() {
        let mut s = Session::new();
        match step(&mut s, ":hir 1 + 2") {
            StepResult::Done(out) => {
                assert!(out.contains("Bin") || out.contains("BinOp"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn mir_directive_emits_body() {
        let mut s = Session::new();
        match step(&mut s, ":mir 1 + 2") {
            StepResult::Done(out) => {
                assert!(out.contains("BasicBlock") || out.contains("blocks"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn clear_drops_bindings() {
        let mut s = Session::new();
        let _ = step(&mut s, "let v = 7");
        match step(&mut s, ":clear") {
            StepResult::Done(_) => {}
            other => panic!("unexpected: {other:?}"),
        }
        match step(&mut s, "v") {
            StepResult::Error(_) => {}
            other => panic!("v should be unbound after :clear, got: {other:?}"),
        }
    }

    #[test]
    fn quit_directive_returns_quit() {
        let mut s = Session::new();
        match step(&mut s, ":quit") {
            StepResult::Quit => {}
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn quit_aliases() {
        for alias in [":q", ":exit"] {
            let mut s = Session::new();
            match step(&mut s, alias) {
                StepResult::Quit => {}
                other => panic!("alias {alias} returned {other:?}"),
            }
        }
    }

    #[test]
    fn help_directive_lists_directives() {
        let mut s = Session::new();
        match step(&mut s, ":help") {
            StepResult::Done(out) => {
                assert!(out.contains(":type"));
                assert!(out.contains(":ast"));
                assert!(out.contains(":hir"));
                assert!(out.contains(":mir"));
                assert!(out.contains(":clear"));
                assert!(out.contains(":quit"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn unknown_directive_errors() {
        let mut s = Session::new();
        match step(&mut s, ":bogus") {
            StepResult::Error(msg) => assert!(msg.contains("unknown directive")),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn type_directive_no_arg_errors() {
        let mut s = Session::new();
        match step(&mut s, ":type") {
            StepResult::Error(_) => {}
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn boolean_ops() {
        let mut s = Session::new();
        match step(&mut s, "True and False") {
            StepResult::Done(out) => assert_eq!(out, "False"),
            other => panic!("unexpected: {other:?}"),
        }
        match step(&mut s, "True or False") {
            StepResult::Done(out) => assert_eq!(out, "True"),
            other => panic!("unexpected: {other:?}"),
        }
        match step(&mut s, "not True") {
            StepResult::Done(out) => assert_eq!(out, "False"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn comparison_chain() {
        let mut s = Session::new();
        match step(&mut s, "1 == 1") {
            StepResult::Done(out) => assert_eq!(out, "True"),
            other => panic!("unexpected: {other:?}"),
        }
        match step(&mut s, "5 != 3") {
            StepResult::Done(out) => assert_eq!(out, "True"),
            other => panic!("unexpected: {other:?}"),
        }
        match step(&mut s, "10 > 5") {
            StepResult::Done(out) => assert_eq!(out, "True"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn integer_division_by_zero_errors() {
        let mut s = Session::new();
        match step(&mut s, "1 / 0") {
            StepResult::Error(msg) => assert!(msg.contains("division by zero")),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn string_concatenation() {
        let mut s = Session::new();
        match step(&mut s, r#""foo" + "bar""#) {
            StepResult::Done(out) => assert_eq!(out, "\"foobar\""),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn multi_line_continuation_returns_continue() {
        // An incomplete `fn` head (no body) → `UnexpectedEof` → Continue.
        let mut s = Session::new();
        match step(&mut s, "fn f() -> i64:") {
            StepResult::Continue => {}
            other => panic!("expected Continue, got: {other:?}"),
        }
    }

    #[test]
    fn empty_input_is_noop() {
        let mut s = Session::new();
        match step(&mut s, "") {
            StepResult::Done(out) => assert_eq!(out, ""),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn unary_negation() {
        let mut s = Session::new();
        match step(&mut s, "-(1 + 2)") {
            StepResult::Done(out) => assert_eq!(out, "-3"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn float_arithmetic() {
        let mut s = Session::new();
        match step(&mut s, "1.5 + 2.5") {
            StepResult::Done(out) => assert_eq!(out, "4.0"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn rebind_overwrites() {
        let mut s = Session::new();
        let _ = step(&mut s, "let x = 1");
        let _ = step(&mut s, "let x = 99");
        match step(&mut s, "x") {
            StepResult::Done(out) => assert_eq!(out, "99"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    // ===== ADR-0056b §6 Phase J handoff contract — collocated smoke =====

    #[test]
    fn session_implements_clone_and_send() {
        fn assert_clone_send<T: Clone + Send + 'static>() {}
        assert_clone_send::<Session>();
        let s = Session::new();
        let _ = s.clone();
    }

    #[test]
    fn session_type_ctx_accessor_returns_reference() {
        let s = Session::new();
        // Wave-2 contract: type_ctx() returns &TypeCheckCtx; default
        // ctx has zero bindings and version 0.
        assert_eq!(s.type_ctx().binding_count(), 0);
        assert_eq!(s.type_ctx().version(), 0);
    }

    #[test]
    fn session_let_populates_type_ctx() {
        let mut s = Session::new();
        let _ = step(&mut s, "let x = 42");
        assert!(
            s.type_ctx().lookup("x").is_some(),
            "let x = 42 should populate type_ctx"
        );
    }

    #[test]
    fn session_type_directive_uses_cross_turn_ctx() {
        let mut s = Session::new();
        let _ = step(&mut s, "let n = 7");
        match step(&mut s, ":type n") {
            StepResult::Done(out) => assert_eq!(out, "i64"),
            other => panic!("expected :type n -> i64 from cross-turn ctx, got {other:?}"),
        }
    }

    #[test]
    fn session_invalidate_clears_ctx_rows() {
        let mut s = Session::new();
        let _ = step(&mut s, "let answer = 42");
        assert!(s.type_ctx().lookup("answer").is_some());
        s.invalidate(FileId::SYNTHETIC.0);
        assert!(s.type_ctx().lookup("answer").is_none());
    }

    #[test]
    fn session_clear_resets_type_ctx() {
        let mut s = Session::new();
        let _ = step(&mut s, "let v = 9");
        assert!(s.type_ctx().lookup("v").is_some());
        let _ = step(&mut s, ":clear");
        assert!(s.type_ctx().lookup("v").is_none(), ":clear must reset type_ctx");
    }

    #[test]
    fn session_clone_can_cross_thread() {
        let mut s = Session::new();
        let _ = step(&mut s, "let n = 5");
        let snap = s.clone();
        let h = std::thread::spawn(move || snap.type_ctx().binding_count());
        let count = h.join().unwrap();
        assert!(count >= 1);
    }
}
