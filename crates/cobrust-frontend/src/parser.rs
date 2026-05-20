//! Cobrust parser.
//!
//! Recursive descent for statements, Pratt for expressions. No
//! external parser-generator dependency. Accepts the token stream
//! produced by [`crate::lex`] and yields an [`ast::Module`].
//!
//! See `docs/agent/adr/0003-core-30-forms.md` for the surface this
//! parser must accept (and the surface it must *not* accept — `is`,
//! `del`, `global`, `nonlocal`, `async def`, multi-base classes, and
//! mutable defaults are all rejected here).
//!
//! ## Pratt operator table (form 29)
//!
//! Higher precedence binds tighter. `R` = right-associative.
//!
//! | prec | ops |
//! |---|---|
//! | 100 | unary `+ - ~ not` |
//! | 95R | `**` |
//! | 90 | `* / // % @` |
//! | 85 | `+ -` |
//! | 80 | `<< >>` |
//! | 75 | `&` |
//! | 70 | `^` |
//! | 65 | `\|` |
//! | 60 | `< <= > >= == != in (not in)` |
//! | 50 | `not` (prefix only — see 100) |
//! | 45 | `and` |
//! | 40 | `or` |

#![allow(clippy::too_many_lines)]
#![allow(clippy::cast_possible_truncation)]

use crate::ast::{
    AccessKind, AssignOp, BinOp, Block, BreakKind, CallArg, ClassDef, CollectionLit, Comprehension,
    ComprehensionClause, ComprehensionElem, ComprehensionKind, DictEntry, ExceptHandler, Expr,
    ExprKind, FStrPart, FnDef, ImportStmt, ImportTarget, IndexKind, Literal, MatchArm, Module,
    Param, Params, Pattern, PatternKind, Stmt, StmtKind, Type, TypeAlias, TypeKind, UnaryOp::*,
    WithItem,
};
use crate::error::{LexError, ParseError};
use crate::lexer;
use crate::span::Span;
use crate::token::{FStringPiece, Token, TokenKind};

/// Parse a stream of tokens into a [`Module`].
///
/// # Errors
///
/// Returns [`ParseError`] on syntactic failure.
pub fn parse(tokens: &[Token]) -> Result<Module, ParseError> {
    let mut p = Parser::new(tokens);
    p.parse_module()
}

struct Parser<'a> {
    toks: &'a [Token],
    pos: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct Prec(u8);

const PREC_OR: Prec = Prec(40);
const PREC_AND: Prec = Prec(45);
const PREC_NOT: Prec = Prec(50);
const PREC_CMP: Prec = Prec(60);
const PREC_BITOR: Prec = Prec(65);
const PREC_BITXOR: Prec = Prec(70);
const PREC_BITAND: Prec = Prec(75);
const PREC_SHIFT: Prec = Prec(80);
const PREC_ADD: Prec = Prec(85);
/// `as` cast — tighter than ADD/SUB, looser than MUL/DIV.
const PREC_CAST: Prec = Prec(88);
const PREC_MUL: Prec = Prec(90);
const PREC_POW: Prec = Prec(95);

impl<'a> Parser<'a> {
    fn new(toks: &'a [Token]) -> Self {
        Self { toks, pos: 0 }
    }

    // -------- token helpers --------------------------------------------

    fn peek(&self) -> &Token {
        &self.toks[self.pos.min(self.toks.len() - 1)]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    fn peek_at(&self, off: usize) -> &TokenKind {
        let idx = (self.pos + off).min(self.toks.len() - 1);
        &self.toks[idx].kind
    }

    fn bump(&mut self) -> &Token {
        let t = &self.toks[self.pos.min(self.toks.len() - 1)];
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn at(&self, k: &TokenKind) -> bool {
        std::mem::discriminant(self.peek_kind()) == std::mem::discriminant(k)
    }

    fn eat(&mut self, k: &TokenKind) -> bool {
        if self.at(k) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek_kind(), TokenKind::Newline) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, k: &TokenKind) -> Result<&Token, ParseError> {
        if self.at(k) {
            Ok(self.bump())
        } else {
            let span = self.peek().span;
            Err(ParseError::Expected {
                expected: vec![k.clone()],
                found: self.peek_kind().clone(),
                span,
            })
        }
    }

    fn current_span(&self) -> Span {
        self.peek().span
    }

    // -------- module ---------------------------------------------------

    fn parse_module(&mut self) -> Result<Module, ParseError> {
        let start = self.current_span();
        self.skip_newlines();
        // Module-level docstring: leading bare string expr_stmt.
        let docstring = self.peek_module_docstring();
        let mut items = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::Eof) {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Eof) {
                break;
            }
            let stmt = self.parse_stmt()?;
            items.push(stmt);
        }
        let end_span = self.peek().span;
        Ok(Module {
            docstring,
            items,
            span: start.merge(end_span),
        })
    }

    fn peek_module_docstring(&mut self) -> Option<String> {
        if let TokenKind::Str { value, .. } = self.peek_kind().clone() {
            // Only treat as docstring if followed by Newline or Eof.
            if matches!(self.peek_at(1), TokenKind::Newline | TokenKind::Eof) {
                self.bump();
                self.skip_newlines();
                return Some(value);
            }
        }
        None
    }

    // -------- statements -----------------------------------------------

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        match self.peek_kind().clone() {
            TokenKind::At => self.parse_decorated(),
            TokenKind::KwImport | TokenKind::KwFrom => self.parse_import(),
            TokenKind::KwFn => self.parse_fn_def().map(|fd| {
                let span = fd.body.span;
                Stmt {
                    kind: StmtKind::Fn(fd),
                    span,
                }
            }),
            TokenKind::KwClass => self.parse_class_def().map(|cd| {
                let span = cd.body.span;
                Stmt {
                    kind: StmtKind::Class(cd),
                    span,
                }
            }),
            TokenKind::KwType => self.parse_type_alias(),
            TokenKind::KwLet => self.parse_let(),
            TokenKind::KwIf => self.parse_if(),
            TokenKind::KwWhile => self.parse_while(),
            TokenKind::KwFor => self.parse_for(),
            TokenKind::KwMatch => self.parse_match(),
            TokenKind::KwWith => self.parse_with(),
            TokenKind::KwTry => self.parse_try(),
            TokenKind::KwReturn => self.parse_return(),
            TokenKind::KwBreak => {
                let span = self.bump().span;
                self.expect_eos()?;
                Ok(Stmt {
                    kind: StmtKind::BreakContinue(BreakKind::Break),
                    span,
                })
            }
            TokenKind::KwContinue => {
                let span = self.bump().span;
                self.expect_eos()?;
                Ok(Stmt {
                    kind: StmtKind::BreakContinue(BreakKind::Continue),
                    span,
                })
            }
            TokenKind::KwRaise => self.parse_raise(),
            TokenKind::KwPass => {
                let span = self.bump().span;
                self.expect_eos()?;
                Ok(Stmt {
                    kind: StmtKind::Pass,
                    span,
                })
            }
            // Constitution-dropped keywords/identifiers (M1 still
            // tokenizes their identifiers for diagnostics).
            TokenKind::Ident(name)
                if matches!(name.as_str(), "is" | "global" | "nonlocal" | "del") =>
            {
                let span = self.peek().span;
                let n: &'static str = match name.as_str() {
                    "is" => "is",
                    "global" => "global",
                    "nonlocal" => "nonlocal",
                    "del" => "del",
                    _ => unreachable!(),
                };
                Err(ParseError::DroppedByConstitution { name: n, span })
            }
            _ => self.parse_expr_or_assign_stmt(),
        }
    }

    fn expect_eos(&mut self) -> Result<(), ParseError> {
        if matches!(
            self.peek_kind(),
            TokenKind::Newline | TokenKind::Eof | TokenKind::Semicolon
        ) {
            if matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Semicolon) {
                self.bump();
            }
            Ok(())
        } else {
            let span = self.current_span();
            Err(ParseError::Syntax {
                message: format!(
                    "expected end of statement, found {}",
                    self.peek_kind().classify()
                ),
                span,
            })
        }
    }

    fn parse_decorated(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current_span();
        let mut decorators = Vec::new();
        while matches!(self.peek_kind(), TokenKind::At) {
            self.bump(); // @
            let expr = self.parse_expr()?;
            decorators.push(expr);
            self.expect(&TokenKind::Newline)?;
            self.skip_newlines();
        }
        let inner = match self.peek_kind() {
            TokenKind::KwFn => self.parse_fn_def().map(|fd| {
                let span = fd.body.span;
                Stmt {
                    kind: StmtKind::Fn(fd),
                    span,
                }
            }),
            TokenKind::KwClass => self.parse_class_def().map(|cd| {
                let span = cd.body.span;
                Stmt {
                    kind: StmtKind::Class(cd),
                    span,
                }
            }),
            other => Err(ParseError::Syntax {
                message: format!(
                    "decorators must precede `fn` or `class`, got {}",
                    other.classify()
                ),
                span: self.current_span(),
            }),
        }?;
        let span = start.merge(inner.span);
        Ok(Stmt {
            kind: StmtKind::Decorated {
                decorators,
                inner: Box::new(inner),
            },
            span,
        })
    }

    fn parse_import(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current_span();
        if matches!(self.peek_kind(), TokenKind::KwImport) {
            self.bump();
            let path = self.parse_dotted_name()?;
            let alias = if self.eat(&TokenKind::KwAs) {
                Some(self.expect_ident()?)
            } else {
                None
            };
            self.expect_eos()?;
            return Ok(Stmt {
                kind: StmtKind::Import(ImportStmt::Import { path, alias }),
                span: start,
            });
        }
        // `from`
        self.bump();
        let path = self.parse_dotted_name()?;
        self.expect(&TokenKind::KwImport)?;
        if matches!(self.peek_kind(), TokenKind::Star) {
            let span = self.current_span();
            return Err(ParseError::Syntax {
                message: "`from … import *` is not supported".into(),
                span,
            });
        }
        let mut targets = Vec::new();
        loop {
            let name = self.expect_ident()?;
            let alias = if self.eat(&TokenKind::KwAs) {
                Some(self.expect_ident()?)
            } else {
                None
            };
            targets.push(ImportTarget { name, alias });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect_eos()?;
        Ok(Stmt {
            kind: StmtKind::Import(ImportStmt::From { path, targets }),
            span: start,
        })
    }

    fn parse_dotted_name(&mut self) -> Result<Vec<String>, ParseError> {
        let mut parts = Vec::new();
        parts.push(self.expect_ident()?);
        while self.eat(&TokenKind::Dot) {
            parts.push(self.expect_ident()?);
        }
        Ok(parts)
    }

    fn parse_fn_def(&mut self) -> Result<FnDef, ParseError> {
        self.expect(&TokenKind::KwFn)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LParen)?;
        let params = self.parse_params()?;
        self.expect(&TokenKind::RParen)?;
        let return_type = if self.eat(&TokenKind::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&TokenKind::Colon)?;
        let body = self.parse_block()?;
        Ok(FnDef {
            name,
            params,
            return_type,
            body,
        })
    }

    fn parse_class_def(&mut self) -> Result<ClassDef, ParseError> {
        self.expect(&TokenKind::KwClass)?;
        let name = self.expect_ident()?;
        let mut base = None;
        let mut traits = Vec::new();
        if self.eat(&TokenKind::LParen) {
            // Allow `class Foo()` (empty) or `class Foo(Base)` (single).
            if !matches!(self.peek_kind(), TokenKind::RParen) {
                let parsed_base = self.parse_expr()?;
                // ADR-0041 §H7: reject multi-base classes (no MRO).
                //
                // Two parser shapes can produce a multi-base form:
                //   1. `class Foo(A, B):` — Pratt returns `A`; the
                //      comma stays unconsumed, so we observe Comma
                //      next in the class-def parser.
                //   2. `class Foo((A, B)):` — the inner parens force
                //      the Pratt parser to build a tuple expression
                //      whose `kind` is `Collection(Tuple(_))`.
                //
                // Both surface here. Constitution §2.2 drops multi-
                // inheritance.
                if matches!(self.peek_kind(), TokenKind::Comma) {
                    let span = self.peek().span;
                    return Err(ParseError::Syntax {
                        message:
                            "multi-base class is forbidden (constitution §2.2: composition + traits, no MRO; ADR-0041 §H7)"
                                .to_string(),
                        span: parsed_base.span.merge(span),
                    });
                }
                if matches!(
                    &parsed_base.kind,
                    ExprKind::Collection(CollectionLit::Tuple(_))
                ) {
                    return Err(ParseError::Syntax {
                        message:
                            "multi-base class is forbidden (constitution §2.2: composition + traits, no MRO; ADR-0041 §H7)"
                                .to_string(),
                        span: parsed_base.span,
                    });
                }
                base = Some(parsed_base);
            }
            self.expect(&TokenKind::RParen)?;
        }
        if self.eat(&TokenKind::Colon)
            && !matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Indent)
        {
            // Trait list: `class Foo(Base): Trait1, Trait2:`
            // We have to be careful — a `:` here can also be the
            // start of the body. The grammar is: `(':' trait_list)? ':' block`.
            // We've already eaten the first `:`. Parse the trait list.
            traits.push(self.parse_type()?);
            while self.eat(&TokenKind::Comma) {
                traits.push(self.parse_type()?);
            }
            self.expect(&TokenKind::Colon)?;
        }
        // If we ate the first `:` above and didn't enter the trait
        // branch, we still need to be at the body. If we *didn't* eat
        // any `:` yet, this expects the single body colon.
        if traits.is_empty() && !self.is_block_start() {
            self.expect(&TokenKind::Colon)?;
        }
        let body = self.parse_block()?;
        Ok(ClassDef {
            name,
            base,
            traits,
            body,
        })
    }

    fn is_block_start(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Indent)
    }

    fn parse_type_alias(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::KwType)?;
        let name = self.expect_ident()?;
        let mut type_params = Vec::new();
        if self.eat(&TokenKind::LBracket) {
            if !matches!(self.peek_kind(), TokenKind::RBracket) {
                type_params.push(self.expect_ident()?);
                while self.eat(&TokenKind::Comma) {
                    type_params.push(self.expect_ident()?);
                }
            }
            self.expect(&TokenKind::RBracket)?;
        }
        self.expect(&TokenKind::Eq)?;
        let value = self.parse_type()?;
        let end = value.span;
        self.expect_eos()?;
        Ok(Stmt {
            kind: StmtKind::TypeAlias(TypeAlias {
                name,
                type_params,
                value,
            }),
            span: start.merge(end),
        })
    }

    fn parse_let(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::KwLet)?;
        let target = self.parse_pattern_simple()?;
        let annot = if self.eat(&TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&TokenKind::Eq)?;
        let value = self.parse_expr()?;
        let end = value.span;
        self.expect_eos()?;
        Ok(Stmt {
            kind: StmtKind::Let {
                target,
                annot,
                value,
            },
            span: start.merge(end),
        })
    }

    fn parse_if(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::KwIf)?;
        let cond = self.parse_expr()?;
        self.expect(&TokenKind::Colon)?;
        let then_block = self.parse_block()?;
        let mut elifs = Vec::new();
        let mut else_block = None;
        loop {
            self.skip_newlines();
            if self.eat(&TokenKind::KwElif) {
                let c = self.parse_expr()?;
                self.expect(&TokenKind::Colon)?;
                let b = self.parse_block()?;
                elifs.push((c, b));
            } else if matches!(self.peek_kind(), TokenKind::KwElse) {
                self.bump();
                self.expect(&TokenKind::Colon)?;
                else_block = Some(self.parse_block()?);
                break;
            } else {
                break;
            }
        }
        let span = start.merge(else_block.as_ref().map_or(then_block.span, |b| b.span));
        Ok(Stmt {
            kind: StmtKind::If {
                cond,
                then_block,
                elifs,
                else_block,
            },
            span,
        })
    }

    fn parse_while(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::KwWhile)?;
        let cond = self.parse_expr()?;
        self.expect(&TokenKind::Colon)?;
        let body = self.parse_block()?;
        let mut else_block = None;
        self.skip_newlines();
        if self.eat(&TokenKind::KwElse) {
            self.expect(&TokenKind::Colon)?;
            else_block = Some(self.parse_block()?);
        }
        let end = else_block.as_ref().map_or(body.span, |b| b.span);
        Ok(Stmt {
            kind: StmtKind::While {
                cond,
                body,
                else_block,
            },
            span: start.merge(end),
        })
    }

    fn parse_for(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::KwFor)?;
        let target = self.parse_for_target()?;
        self.expect(&TokenKind::KwIn)?;
        let iter = self.parse_expr()?;
        self.expect(&TokenKind::Colon)?;
        let body = self.parse_block()?;
        let mut else_block = None;
        self.skip_newlines();
        if self.eat(&TokenKind::KwElse) {
            self.expect(&TokenKind::Colon)?;
            else_block = Some(self.parse_block()?);
        }
        let end = else_block.as_ref().map_or(body.span, |b| b.span);
        Ok(Stmt {
            kind: StmtKind::For {
                target,
                iter,
                body,
                else_block,
            },
            span: start.merge(end),
        })
    }

    fn parse_for_target(&mut self) -> Result<Pattern, ParseError> {
        // `(a, b)` or `a` or `a, b` (without parens) — turn the
        // bare-comma form into a sequence pattern.
        let first = self.parse_pattern_simple()?;
        if matches!(self.peek_kind(), TokenKind::Comma) {
            let mut items = vec![first];
            while self.eat(&TokenKind::Comma) {
                if matches!(self.peek_kind(), TokenKind::KwIn) {
                    break;
                }
                items.push(self.parse_pattern_simple()?);
            }
            let span = items
                .first()
                .expect("at least one")
                .span
                .merge(items.last().expect("at least one").span);
            return Ok(Pattern {
                kind: PatternKind::Sequence { items, rest: None },
                span,
            });
        }
        Ok(first)
    }

    fn parse_match(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::KwMatch)?;
        let scrutinee = self.parse_expr()?;
        self.expect(&TokenKind::Colon)?;
        self.expect(&TokenKind::Newline)?;
        self.expect(&TokenKind::Indent)?;
        let mut arms = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Dedent) {
                break;
            }
            self.expect(&TokenKind::KwCase)?;
            let pattern = self.parse_pattern_or()?;
            let guard = if self.eat(&TokenKind::KwIf) {
                Some(self.parse_expr()?)
            } else {
                None
            };
            self.expect(&TokenKind::Colon)?;
            let body = self.parse_block()?;
            arms.push(MatchArm {
                pattern,
                guard,
                body,
            });
        }
        self.expect(&TokenKind::Dedent)?;
        let end = arms.last().map_or(start, |a| a.body.span);
        Ok(Stmt {
            kind: StmtKind::Match { scrutinee, arms },
            span: start.merge(end),
        })
    }

    fn parse_with(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::KwWith)?;
        let mut items = Vec::new();
        loop {
            let context = self.parse_expr()?;
            let target = if self.eat(&TokenKind::KwAs) {
                Some(self.parse_pattern_simple()?)
            } else {
                None
            };
            items.push(WithItem { context, target });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::Colon)?;
        let body = self.parse_block()?;
        Ok(Stmt {
            kind: StmtKind::With {
                items,
                body: body.clone(),
            },
            span: start.merge(body.span),
        })
    }

    fn parse_try(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::KwTry)?;
        self.expect(&TokenKind::Colon)?;
        let body = self.parse_block()?;
        let mut handlers = Vec::new();
        let mut else_block = None;
        let mut finally_block = None;
        loop {
            self.skip_newlines();
            if self.eat(&TokenKind::KwExcept) {
                let exc_type = self.parse_type()?;
                let binding = if self.eat(&TokenKind::KwAs) {
                    Some(self.expect_ident()?)
                } else {
                    None
                };
                self.expect(&TokenKind::Colon)?;
                let h_body = self.parse_block()?;
                handlers.push(ExceptHandler {
                    exc_type,
                    binding,
                    body: h_body,
                });
            } else if self.eat(&TokenKind::KwElse) {
                self.expect(&TokenKind::Colon)?;
                else_block = Some(self.parse_block()?);
            } else if self.eat(&TokenKind::KwFinally) {
                self.expect(&TokenKind::Colon)?;
                finally_block = Some(self.parse_block()?);
                break;
            } else {
                break;
            }
        }
        let end = finally_block.as_ref().or(else_block.as_ref()).map_or_else(
            || handlers.last().map_or(body.span, |h| h.body.span),
            |b| b.span,
        );
        Ok(Stmt {
            kind: StmtKind::Try {
                body,
                handlers,
                else_block,
                finally_block,
            },
            span: start.merge(end),
        })
    }

    fn parse_return(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::KwReturn)?;
        let value = if matches!(
            self.peek_kind(),
            TokenKind::Newline | TokenKind::Eof | TokenKind::Semicolon
        ) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        let end = value.as_ref().map_or(start, |e| e.span);
        self.expect_eos()?;
        Ok(Stmt {
            kind: StmtKind::Return(value),
            span: start.merge(end),
        })
    }

    fn parse_raise(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::KwRaise)?;
        let exc = if matches!(
            self.peek_kind(),
            TokenKind::Newline | TokenKind::Eof | TokenKind::Semicolon | TokenKind::KwFrom
        ) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        let cause = if self.eat(&TokenKind::KwFrom) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        let end = cause.as_ref().or(exc.as_ref()).map_or(start, |e| e.span);
        self.expect_eos()?;
        Ok(Stmt {
            kind: StmtKind::Raise { exc, cause },
            span: start.merge(end),
        })
    }

    fn parse_expr_or_assign_stmt(&mut self) -> Result<Stmt, ParseError> {
        let lhs = self.parse_expr()?;
        // Augmented assignment.
        if let Some(op) = self.peek_assign_op() {
            self.bump();
            let rhs = self.parse_expr()?;
            let span = lhs.span.merge(rhs.span);
            self.expect_eos()?;
            return Ok(Stmt {
                kind: StmtKind::Assign {
                    target: Box::new(lhs),
                    op,
                    value: rhs,
                },
                span,
            });
        }
        // Plain `=` assignment.
        if matches!(self.peek_kind(), TokenKind::Eq) {
            self.bump();
            let rhs = self.parse_expr()?;
            let span = lhs.span.merge(rhs.span);
            self.expect_eos()?;
            return Ok(Stmt {
                kind: StmtKind::Assign {
                    target: Box::new(lhs),
                    op: AssignOp::Eq,
                    value: rhs,
                },
                span,
            });
        }
        let span = lhs.span;
        self.expect_eos()?;
        Ok(Stmt {
            kind: StmtKind::Expr(lhs),
            span,
        })
    }

    fn peek_assign_op(&self) -> Option<AssignOp> {
        Some(match self.peek_kind() {
            TokenKind::PlusEq => AssignOp::PlusEq,
            TokenKind::MinusEq => AssignOp::MinusEq,
            TokenKind::StarEq => AssignOp::StarEq,
            TokenKind::StarStarEq => AssignOp::StarStarEq,
            TokenKind::SlashEq => AssignOp::SlashEq,
            TokenKind::SlashSlashEq => AssignOp::SlashSlashEq,
            TokenKind::PercentEq => AssignOp::PercentEq,
            TokenKind::AmpEq => AssignOp::AmpEq,
            TokenKind::PipeEq => AssignOp::PipeEq,
            TokenKind::CaretEq => AssignOp::CaretEq,
            TokenKind::ShlEq => AssignOp::ShlEq,
            TokenKind::ShrEq => AssignOp::ShrEq,
            _ => return None,
        })
    }

    // -------- block ----------------------------------------------------

    fn parse_block(&mut self) -> Result<Block, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::Newline)?;
        self.skip_newlines();
        self.expect(&TokenKind::Indent)?;
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Dedent | TokenKind::Eof) {
                break;
            }
            stmts.push(self.parse_stmt()?);
        }
        self.eat(&TokenKind::Dedent);
        let end = stmts.last().map_or(start, |s| s.span);
        Ok(Block {
            stmts,
            span: start.merge(end),
        })
    }

    // -------- expressions: Pratt --------------------------------------

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_pratt(Prec(0))
    }

    fn parse_pratt(&mut self, min: Prec) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_unary()?;
        loop {
            // `as` cast — M-F.3.3 gap (a). Higher precedence than ADD, lower than MUL.
            // Only parse `expr as T` when T is a recognized Cobrust type name
            // (i64, f64, str, bool, bytes, …). This prevents `with ctx as x`
            // and `import foo as bar` from being misinterpreted as cast
            // expressions (both are handled at statement level before the Pratt
            // parser runs on the expression). The look-ahead checks peek_at(1)
            // (token after `as`) for a known type identifier.
            if matches!(self.peek_kind(), TokenKind::KwAs)
                && PREC_CAST >= min
                && is_cast_type_token(self.peek_at(1))
            {
                self.bump(); // consume `as`
                let target = self.parse_type()?;
                let span = lhs.span.merge(target.span);
                lhs = Expr {
                    kind: ExprKind::Cast {
                        expr: Box::new(lhs),
                        target,
                    },
                    span,
                };
                continue;
            }

            // Comparison chain detection: a < b < c collapses to
            // `(a < b) and (b < c)` — but we keep it simple for M1
            // and parse left-associatively, returning a binary tree.
            let Some((op, prec, right_assoc)) = self.peek_binop() else {
                break;
            };
            if prec < min {
                break;
            }
            // Special-case `not in`: lookahead.
            let consumed = false;
            if matches!(op, BinOp::In) && matches!(self.peek_at(0), TokenKind::KwNot) {
                // Actually this only happens in `not in` form, handled
                // by parse_binop; falling through.
                let _ = consumed;
            }
            self.bump();
            let next_min = if right_assoc { prec } else { Prec(prec.0 + 1) };
            let rhs = self.parse_pratt(next_min)?;
            let span = lhs.span.merge(rhs.span);
            lhs = Expr {
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                span,
            };
        }
        Ok(lhs)
    }

    fn peek_binop(&self) -> Option<(BinOp, Prec, bool)> {
        Some(match self.peek_kind() {
            TokenKind::KwOr => (BinOp::Or, PREC_OR, false),
            TokenKind::KwAnd => (BinOp::And, PREC_AND, false),
            TokenKind::EqEq => (BinOp::Eq, PREC_CMP, false),
            TokenKind::NotEq => (BinOp::NotEq, PREC_CMP, false),
            TokenKind::Lt => (BinOp::Lt, PREC_CMP, false),
            TokenKind::LtEq => (BinOp::LtEq, PREC_CMP, false),
            TokenKind::Gt => (BinOp::Gt, PREC_CMP, false),
            TokenKind::GtEq => (BinOp::GtEq, PREC_CMP, false),
            TokenKind::KwIn => (BinOp::In, PREC_CMP, false),
            TokenKind::KwNot => {
                // Only valid as `not in` here.
                //
                // ADR-0050d sub-sprint a/b parser disposition (clarifies
                // Decision 4A): `BinOp::NotIn` is recognised when KwNot
                // sits in binary-op position and is followed by KwIn at
                // PREC_CMP. The canonical pre-Phase-G workaround for
                // negated membership is `not (k in d)` (unary-not over
                // the `BinOp::In` bool result) — the well_typed corpus
                // w130 + dict_e2e f3d18 ship that idiom. Both are
                // semantically `bool -> bool` and the type-checker accepts
                // either form without divergence. Phase G may revisit
                // `k not in d` Pratt-loop bookkeeping (the second
                // self.bump() after producing BinOp::NotIn is currently
                // unbalanced — see parse_pratt §"Special-case `not in`"
                // commentary at L909-L915); for Phase F.3 the workaround
                // is canonical to keep scope tight.
                if matches!(self.peek_at(1), TokenKind::KwIn) {
                    (BinOp::NotIn, PREC_CMP, false)
                } else {
                    return None;
                }
            }
            TokenKind::Pipe => (BinOp::BitOr, PREC_BITOR, false),
            TokenKind::Caret => (BinOp::BitXor, PREC_BITXOR, false),
            TokenKind::Amp => (BinOp::BitAnd, PREC_BITAND, false),
            TokenKind::Shl => (BinOp::Shl, PREC_SHIFT, false),
            TokenKind::Shr => (BinOp::Shr, PREC_SHIFT, false),
            TokenKind::Plus => (BinOp::Add, PREC_ADD, false),
            TokenKind::Minus => (BinOp::Sub, PREC_ADD, false),
            TokenKind::Star => (BinOp::Mul, PREC_MUL, false),
            TokenKind::At => (BinOp::MatMul, PREC_MUL, false),
            TokenKind::Slash => (BinOp::Div, PREC_MUL, false),
            TokenKind::SlashSlash => (BinOp::FloorDiv, PREC_MUL, false),
            TokenKind::Percent => (BinOp::Mod, PREC_MUL, false),
            TokenKind::StarStar => (BinOp::Pow, PREC_POW, true),
            _ => return None,
        })
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        match self.peek_kind() {
            TokenKind::KwNot => {
                let start = self.current_span();
                self.bump();
                // `not in` is handled by the binary parser as a single op.
                let operand = self.parse_pratt(PREC_NOT)?;
                let span = start.merge(operand.span);
                Ok(Expr {
                    kind: ExprKind::Unary {
                        op: Not,
                        operand: Box::new(operand),
                    },
                    span,
                })
            }
            TokenKind::Plus | TokenKind::Minus | TokenKind::Tilde => {
                let start = self.current_span();
                let op = match self.peek_kind() {
                    TokenKind::Plus => Plus,
                    TokenKind::Minus => Neg,
                    TokenKind::Tilde => BitNot,
                    _ => unreachable!(),
                };
                self.bump();
                let operand = self.parse_pratt(Prec(100))?;
                let span = start.merge(operand.span);
                Ok(Expr {
                    kind: ExprKind::Unary {
                        op,
                        operand: Box::new(operand),
                    },
                    span,
                })
            }
            // ADR-0052a Wave-1 — unary `&` immutable shared borrow.
            // Parsed at high precedence (Prec(100)) so any binary op after
            // the operand stays outside the borrow (`&s + 1` == `(&s) + 1`).
            // Wave-1 §8 cap restricts the operand shape to `Name`,
            // `Access(Attribute)`, `Access(Index)`, or a parenthesised
            // single sub-expression of those — rejected shapes:
            //   - `&literal`           — int/float/str/bytes/bool/None/f-string
            //   - `&[..]`/`&(..,..)`   — collection literals
            //   - `&call(...)`         — call-result borrow
            //   - `&&s`                — nested borrow
            //   - `&mut s`             — mutable borrow (Phase H)
            //   - bare `&` operand-less
            TokenKind::Amp => {
                let start = self.current_span();
                self.bump();
                let operand = self.parse_pratt(Prec(100))?;
                Self::validate_borrow_operand(&operand)?;
                let span = start.merge(operand.span);
                Ok(Expr {
                    kind: ExprKind::Borrow(Box::new(operand)),
                    span,
                })
            }
            TokenKind::KwAwait => {
                let start = self.current_span();
                self.bump();
                let operand = self.parse_pratt(Prec(100))?;
                let span = start.merge(operand.span);
                Ok(Expr {
                    kind: ExprKind::Await(Box::new(operand)),
                    span,
                })
            }
            TokenKind::KwYield => {
                let start = self.current_span();
                self.bump();
                if self.eat(&TokenKind::KwFrom) {
                    let inner = self.parse_expr()?;
                    let span = start.merge(inner.span);
                    return Ok(Expr {
                        kind: ExprKind::YieldFrom(Box::new(inner)),
                        span,
                    });
                }
                if matches!(
                    self.peek_kind(),
                    TokenKind::Newline
                        | TokenKind::Eof
                        | TokenKind::RParen
                        | TokenKind::RBracket
                        | TokenKind::RBrace
                        | TokenKind::Semicolon
                        | TokenKind::Comma
                ) {
                    return Ok(Expr {
                        kind: ExprKind::Yield(None),
                        span: start,
                    });
                }
                let inner = self.parse_expr()?;
                let span = start.merge(inner.span);
                Ok(Expr {
                    kind: ExprKind::Yield(Some(Box::new(inner))),
                    span,
                })
            }
            TokenKind::KwLambda => self.parse_lambda(),
            _ => self.parse_postfix(),
        }
    }

    /// ADR-0052a §8 — Wave-1 borrow operand validator.
    ///
    /// The accepted operand shapes are:
    ///   - `Name`                            — `&s`
    ///   - `Access(Attribute { base, .. })`  — `&p.field` (recurse on base)
    ///   - `Access(Index { base, .. })`      — `&xs[i]`   (recurse on base)
    ///   - parenthesised single sub-expression of the above (the
    ///     parser flattens parens, so a `(s)` is just `Name("s")`).
    ///
    /// Every other shape is rejected at parse time with
    /// `ParseError::Syntax`. Wave-1 deferred shapes (literal-borrow,
    /// collection-borrow, call-result-borrow, nested borrow, mutable
    /// borrow) all flow through this validator.
    fn validate_borrow_operand(operand: &Expr) -> Result<(), ParseError> {
        match &operand.kind {
            ExprKind::Name(_) => Ok(()),
            ExprKind::Access(AccessKind::Attribute { base, .. }) => {
                Self::validate_borrow_operand(base)
            }
            ExprKind::Access(AccessKind::Index { base, .. }) => {
                Self::validate_borrow_operand(base)
            }
            ExprKind::Borrow(_) => Err(ParseError::Syntax {
                message: "nested borrow `&&` is not supported in Wave-1 (ADR-0052a §8)"
                    .to_string(),
                span: operand.span,
            }),
            ExprKind::Literal(_) => Err(ParseError::Syntax {
                message: "borrow of a literal is not supported in Wave-1 \
                          (ADR-0052a §8 cap: borrow operand must be `Name`, `Name.field`, or `Name[idx]`)"
                    .to_string(),
                span: operand.span,
            }),
            ExprKind::FString(_) => Err(ParseError::Syntax {
                message: "borrow of an f-string is not supported in Wave-1 \
                          (ADR-0052a §8 cap)"
                    .to_string(),
                span: operand.span,
            }),
            ExprKind::Collection(_) | ExprKind::Comprehension(_) => Err(ParseError::Syntax {
                message: "borrow of a collection / comprehension literal is not \
                          supported in Wave-1 (ADR-0052a §8 cap)"
                    .to_string(),
                span: operand.span,
            }),
            // ADR-0052f §5 — relax §8 cap for `&Call(Attr(...))` form.
            // Method-form (`&recv.method(...)`) is admitted when the
            // receiver `base` is itself borrowable (recursive
            // validation). Free-fn `&Call(Name)` and other callee
            // shapes STILL reject per the §2.5 compile-time-catch
            // path — borrowing a free-fn temporary is almost always
            // wrong (no anchored place for the borrow).
            ExprKind::Call { callee, .. } => match &callee.kind {
                ExprKind::Access(AccessKind::Attribute { base, .. }) => {
                    Self::validate_borrow_operand(base)
                }
                _ => Err(ParseError::Syntax {
                    message: "borrow of a free-function call-result is not supported \
                              (ADR-0052f only relaxes method-form `&recv.method(...)`; \
                              borrow operand must be `Name`, `Name.field`, `Name[idx]`, \
                              or `Name.method(...)`)"
                        .to_string(),
                    span: operand.span,
                }),
            },
            _ => Err(ParseError::Syntax {
                message: "borrow operand must be `Name`, `Name.field`, or `Name[idx]` \
                          in Wave-1 (ADR-0052a §8 cap)"
                    .to_string(),
                span: operand.span,
            }),
        }
    }

    fn parse_lambda(&mut self) -> Result<Expr, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::KwLambda)?;
        let params = if matches!(self.peek_kind(), TokenKind::Colon) {
            Params::default()
        } else {
            self.parse_lambda_params()?
        };
        self.expect(&TokenKind::Colon)?;
        let body = self.parse_expr()?;
        let span = start.merge(body.span);
        Ok(Expr {
            kind: ExprKind::Lambda {
                params,
                body: Box::new(body),
            },
            span,
        })
    }

    fn parse_lambda_params(&mut self) -> Result<Params, ParseError> {
        // Lambda params are bare-name only — no type annotations, no
        // defaults that look like assignment expressions. Defaults
        // are still allowed but only as literal expressions.
        let mut params = Params::default();
        let mut after_star = false;
        loop {
            if matches!(self.peek_kind(), TokenKind::Star) {
                self.bump();
                if matches!(self.peek_kind(), TokenKind::Ident(_)) {
                    let p = self.parse_one_lambda_param()?;
                    params.var_positional = Some(p);
                }
                after_star = true;
            } else if self.eat(&TokenKind::StarStar) {
                let p = self.parse_one_lambda_param()?;
                params.var_keyword = Some(p);
            } else {
                let p = self.parse_one_lambda_param()?;
                if after_star {
                    params.keyword_only.push(p);
                } else {
                    params.positional.push(p);
                }
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
            if matches!(self.peek_kind(), TokenKind::Colon) {
                break;
            }
        }
        Ok(params)
    }

    fn parse_one_lambda_param(&mut self) -> Result<Param, ParseError> {
        let start = self.current_span();
        let name = self.expect_ident()?;
        let default = if self.eat(&TokenKind::Eq) {
            Some(self.parse_literal_default()?)
        } else {
            None
        };
        let end = self.peek().span;
        Ok(Param {
            name,
            annot: None,
            default,
            span: start.merge(end),
        })
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut e = self.parse_atom()?;
        loop {
            match self.peek_kind() {
                TokenKind::LParen => {
                    self.bump();
                    let args = self.parse_call_args()?;
                    let end = self.current_span();
                    self.expect(&TokenKind::RParen)?;
                    let span = e.span.merge(end);
                    e = Expr {
                        kind: ExprKind::Call {
                            callee: Box::new(e),
                            args,
                        },
                        span,
                    };
                }
                TokenKind::Dot => {
                    self.bump();
                    let name = self.expect_ident()?;
                    let end = self.current_span();
                    e = Expr {
                        kind: ExprKind::Access(AccessKind::Attribute {
                            base: Box::new(e.clone()),
                            name,
                        }),
                        span: e.span.merge(end),
                    };
                }
                TokenKind::LBracket => {
                    self.bump();
                    let idx = self.parse_index()?;
                    let end = self.current_span();
                    self.expect(&TokenKind::RBracket)?;
                    let span = e.span.merge(end);
                    e = Expr {
                        kind: ExprKind::Access(AccessKind::Index {
                            base: Box::new(e),
                            index: Box::new(idx),
                        }),
                        span,
                    };
                }
                _ => break,
            }
        }
        Ok(e)
    }

    fn parse_index(&mut self) -> Result<IndexKind, ParseError> {
        // Single, slice, or tuple of indices.
        let first = self.parse_index_one()?;
        if matches!(self.peek_kind(), TokenKind::Comma) {
            let mut items = vec![first];
            while self.eat(&TokenKind::Comma) {
                if matches!(self.peek_kind(), TokenKind::RBracket) {
                    break;
                }
                items.push(self.parse_index_one()?);
            }
            return Ok(IndexKind::Tuple(items));
        }
        Ok(first)
    }

    fn parse_index_one(&mut self) -> Result<IndexKind, ParseError> {
        // Detect slice. A slice contains `:`. Try to parse a slice
        // by examining whether `:` appears at top level inside the
        // index. We do a simple lookahead: parse the first piece as
        // an optional expression up to the first `:` or `,` or `]`.
        let start = self.peek().span.start;
        let _ = start;
        let mut start_expr: Option<Expr> = None;
        let mut stop_expr: Option<Expr> = None;
        let mut step_expr: Option<Expr> = None;
        let mut is_slice = false;

        if !matches!(self.peek_kind(), TokenKind::Colon) {
            let e = self.parse_expr()?;
            start_expr = Some(e);
        }
        if matches!(self.peek_kind(), TokenKind::Colon) {
            is_slice = true;
            self.bump();
            if !matches!(
                self.peek_kind(),
                TokenKind::Colon | TokenKind::RBracket | TokenKind::Comma
            ) {
                stop_expr = Some(self.parse_expr()?);
            }
            if matches!(self.peek_kind(), TokenKind::Colon) {
                self.bump();
                if !matches!(self.peek_kind(), TokenKind::RBracket | TokenKind::Comma) {
                    step_expr = Some(self.parse_expr()?);
                }
            }
        }
        if is_slice {
            Ok(IndexKind::Slice {
                start: start_expr,
                stop: stop_expr,
                step: step_expr,
            })
        } else if let Some(e) = start_expr {
            Ok(IndexKind::Expr(e))
        } else {
            Err(ParseError::Syntax {
                message: "empty index".into(),
                span: self.current_span(),
            })
        }
    }

    fn parse_call_args(&mut self) -> Result<Vec<CallArg>, ParseError> {
        let mut args = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RParen) {
            if self.eat(&TokenKind::StarStar) {
                let v = self.parse_expr()?;
                args.push(CallArg::StarStarKwargs(v));
            } else if self.eat(&TokenKind::Star) {
                let v = self.parse_expr()?;
                args.push(CallArg::StarArgs(v));
            } else {
                // keyword? lookahead `IDENT =`.
                let is_keyword = matches!(self.peek_kind(), TokenKind::Ident(_))
                    && matches!(self.peek_at(1), TokenKind::Eq);
                if is_keyword {
                    let name = self.expect_ident()?;
                    self.expect(&TokenKind::Eq)?;
                    let v = self.parse_expr()?;
                    args.push(CallArg::Keyword(name, v));
                } else {
                    let v = self.parse_expr()?;
                    args.push(CallArg::Positional(v));
                }
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        Ok(args)
    }

    fn parse_atom(&mut self) -> Result<Expr, ParseError> {
        let tok = self.peek().clone();
        match &tok.kind {
            TokenKind::Int(s) => {
                self.bump();
                Ok(Expr {
                    kind: ExprKind::Literal(Literal::Int(s.clone())),
                    span: tok.span,
                })
            }
            TokenKind::Float(s) => {
                self.bump();
                Ok(Expr {
                    kind: ExprKind::Literal(Literal::Float(s.clone())),
                    span: tok.span,
                })
            }
            TokenKind::Imag(s) => {
                self.bump();
                Ok(Expr {
                    kind: ExprKind::Literal(Literal::Imag(s.clone())),
                    span: tok.span,
                })
            }
            TokenKind::Str { value, .. } => {
                self.bump();
                Ok(Expr {
                    kind: ExprKind::Literal(Literal::Str(value.clone())),
                    span: tok.span,
                })
            }
            TokenKind::Bytes { value, .. } => {
                self.bump();
                Ok(Expr {
                    kind: ExprKind::Literal(Literal::Bytes(value.clone())),
                    span: tok.span,
                })
            }
            TokenKind::FString { pieces } => {
                self.bump();
                let parts = self.fstring_pieces_to_parts(pieces, tok.span)?;
                Ok(Expr {
                    kind: ExprKind::FString(parts),
                    span: tok.span,
                })
            }
            TokenKind::KwTrue => {
                self.bump();
                Ok(Expr {
                    kind: ExprKind::Literal(Literal::Bool(true)),
                    span: tok.span,
                })
            }
            TokenKind::KwFalse => {
                self.bump();
                Ok(Expr {
                    kind: ExprKind::Literal(Literal::Bool(false)),
                    span: tok.span,
                })
            }
            TokenKind::KwNone => {
                self.bump();
                Ok(Expr {
                    kind: ExprKind::Literal(Literal::None),
                    span: tok.span,
                })
            }
            TokenKind::Ident(name) => {
                self.bump();
                // ADR-0041 §H4: walrus operator is reserved-not-implemented.
                // The lexer emits `Walrus` for `:=`; rather than silently
                // dropping it (the prior behavior — parser zero-consumes
                // walrus → opaque "expected EOS" error far downstream),
                // raise an explicit `DroppedByConstitution` so the user
                // knows.  A future ADR will adopt walrus as part of
                // expression-binding; until then it's not part of Cobrust.
                if matches!(self.peek_kind(), TokenKind::Walrus) {
                    let walrus_span = self.peek().span;
                    return Err(ParseError::DroppedByConstitution {
                        name: "walrus :=",
                        span: tok.span.merge(walrus_span),
                    });
                }
                Ok(Expr {
                    kind: ExprKind::Name(name.clone()),
                    span: tok.span,
                })
            }
            TokenKind::LParen => self.parse_paren_expr(),
            TokenKind::LBracket => self.parse_bracket_expr(),
            TokenKind::LBrace => self.parse_brace_expr(),
            other => Err(ParseError::Syntax {
                message: format!("unexpected token {} in expression", other.classify()),
                span: tok.span,
            }),
        }
    }

    fn fstring_pieces_to_parts(
        &mut self,
        pieces: &[FStringPiece],
        outer_span: Span,
    ) -> Result<Vec<FStrPart>, ParseError> {
        let mut out = Vec::with_capacity(pieces.len());
        for p in pieces {
            match p {
                FStringPiece::Lit(s) => out.push(FStrPart::Lit(s.clone())),
                FStringPiece::Expr {
                    source,
                    debug_equals,
                    format_spec,
                } => {
                    let toks = lexer::lex(source, outer_span.file).map_err(|e| match e {
                        LexError::InvalidUtf8 { byte_offset } => ParseError::Syntax {
                            message: format!("invalid UTF-8 in f-string at byte {byte_offset}"),
                            span: outer_span,
                        },
                        other => ParseError::Syntax {
                            message: other.to_string(),
                            span: outer_span,
                        },
                    })?;
                    // Re-parse as a single expression.
                    let mut sub = Parser::new(&toks);
                    let expr = sub.parse_expr()?;
                    out.push(FStrPart::Expr {
                        expr: Box::new(expr),
                        debug_equals: *debug_equals,
                        format_spec: format_spec.clone(),
                    });
                }
            }
        }
        Ok(out)
    }

    fn parse_paren_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::LParen)?;
        // Empty tuple `()`.
        if matches!(self.peek_kind(), TokenKind::RParen) {
            let end = self.current_span();
            self.bump();
            return Ok(Expr {
                kind: ExprKind::Collection(CollectionLit::Tuple(Vec::new())),
                span: start.merge(end),
            });
        }
        let first = self.parse_expr()?;
        // Generator comprehension `(x for x in xs)`.
        if matches!(self.peek_kind(), TokenKind::KwFor) {
            let clauses = self.parse_comprehension_clauses()?;
            let end = self.current_span();
            self.expect(&TokenKind::RParen)?;
            let comp = Comprehension {
                kind: ComprehensionKind::Generator,
                element: ComprehensionElem::Single(first),
                clauses,
            };
            return Ok(Expr {
                kind: ExprKind::Comprehension(Box::new(comp)),
                span: start.merge(end),
            });
        }
        // Tuple or parenthesized expression.
        if self.eat(&TokenKind::Comma) {
            let mut items = vec![first];
            while !matches!(self.peek_kind(), TokenKind::RParen) {
                items.push(self.parse_expr()?);
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            let end = self.current_span();
            self.expect(&TokenKind::RParen)?;
            return Ok(Expr {
                kind: ExprKind::Collection(CollectionLit::Tuple(items)),
                span: start.merge(end),
            });
        }
        let end = self.current_span();
        self.expect(&TokenKind::RParen)?;
        Ok(Expr {
            span: start.merge(end),
            ..first
        })
    }

    fn parse_bracket_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::LBracket)?;
        if matches!(self.peek_kind(), TokenKind::RBracket) {
            let end = self.current_span();
            self.bump();
            return Ok(Expr {
                kind: ExprKind::Collection(CollectionLit::List(Vec::new())),
                span: start.merge(end),
            });
        }
        let first = self.parse_expr()?;
        if matches!(self.peek_kind(), TokenKind::KwFor) {
            let clauses = self.parse_comprehension_clauses()?;
            let end = self.current_span();
            self.expect(&TokenKind::RBracket)?;
            let comp = Comprehension {
                kind: ComprehensionKind::List,
                element: ComprehensionElem::Single(first),
                clauses,
            };
            return Ok(Expr {
                kind: ExprKind::Comprehension(Box::new(comp)),
                span: start.merge(end),
            });
        }
        let mut items = vec![first];
        while self.eat(&TokenKind::Comma) {
            if matches!(self.peek_kind(), TokenKind::RBracket) {
                break;
            }
            items.push(self.parse_expr()?);
        }
        let end = self.current_span();
        self.expect(&TokenKind::RBracket)?;
        Ok(Expr {
            kind: ExprKind::Collection(CollectionLit::List(items)),
            span: start.merge(end),
        })
    }

    fn parse_brace_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::LBrace)?;
        if matches!(self.peek_kind(), TokenKind::RBrace) {
            // Empty dict `{}`.
            let end = self.current_span();
            self.bump();
            return Ok(Expr {
                kind: ExprKind::Collection(CollectionLit::Dict(Vec::new())),
                span: start.merge(end),
            });
        }
        // Dict spread: `{**rest}`.
        if matches!(self.peek_kind(), TokenKind::StarStar) {
            return self.parse_dict_after_first(start, None);
        }
        let first = self.parse_expr()?;
        // Dict pair?
        if self.eat(&TokenKind::Colon) {
            let v = self.parse_expr()?;
            // dict comprehension?
            if matches!(self.peek_kind(), TokenKind::KwFor) {
                let clauses = self.parse_comprehension_clauses()?;
                let end = self.current_span();
                self.expect(&TokenKind::RBrace)?;
                let comp = Comprehension {
                    kind: ComprehensionKind::Dict,
                    element: ComprehensionElem::KeyValue(first, v),
                    clauses,
                };
                return Ok(Expr {
                    kind: ExprKind::Comprehension(Box::new(comp)),
                    span: start.merge(end),
                });
            }
            return self.parse_dict_after_first(start, Some(DictEntry::Pair(first, v)));
        }
        // Set comprehension?
        if matches!(self.peek_kind(), TokenKind::KwFor) {
            let clauses = self.parse_comprehension_clauses()?;
            let end = self.current_span();
            self.expect(&TokenKind::RBrace)?;
            let comp = Comprehension {
                kind: ComprehensionKind::Set,
                element: ComprehensionElem::Single(first),
                clauses,
            };
            return Ok(Expr {
                kind: ExprKind::Comprehension(Box::new(comp)),
                span: start.merge(end),
            });
        }
        let mut items = vec![first];
        while self.eat(&TokenKind::Comma) {
            if matches!(self.peek_kind(), TokenKind::RBrace) {
                break;
            }
            items.push(self.parse_expr()?);
        }
        let end = self.current_span();
        self.expect(&TokenKind::RBrace)?;
        Ok(Expr {
            kind: ExprKind::Collection(CollectionLit::Set(items)),
            span: start.merge(end),
        })
    }

    fn parse_dict_after_first(
        &mut self,
        start: Span,
        first: Option<DictEntry>,
    ) -> Result<Expr, ParseError> {
        let mut entries = Vec::new();
        let had_first = first.is_some();
        if let Some(e) = first {
            entries.push(e);
        }
        // If we already pushed a first entry, we must see `,` (or `}`).
        if had_first && !matches!(self.peek_kind(), TokenKind::RBrace) {
            self.expect(&TokenKind::Comma)?;
        }
        loop {
            if matches!(self.peek_kind(), TokenKind::RBrace) {
                break;
            }
            if self.eat(&TokenKind::StarStar) {
                let e = self.parse_expr()?;
                entries.push(DictEntry::Spread(e));
            } else {
                let k = self.parse_expr()?;
                self.expect(&TokenKind::Colon)?;
                let v = self.parse_expr()?;
                entries.push(DictEntry::Pair(k, v));
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        let end = self.current_span();
        self.expect(&TokenKind::RBrace)?;
        Ok(Expr {
            kind: ExprKind::Collection(CollectionLit::Dict(entries)),
            span: start.merge(end),
        })
    }

    fn parse_comprehension_clauses(&mut self) -> Result<Vec<ComprehensionClause>, ParseError> {
        let mut clauses = Vec::new();
        while self.eat(&TokenKind::KwFor) {
            let target = self.parse_for_target()?;
            self.expect(&TokenKind::KwIn)?;
            // Iter must not allow another `for` at same level — parse_expr stops at
            // `for` because it isn't a binop.
            let iter = self.parse_expr()?;
            let mut guards = Vec::new();
            while self.eat(&TokenKind::KwIf) {
                guards.push(self.parse_expr()?);
            }
            clauses.push(ComprehensionClause {
                target,
                iter,
                guards,
            });
        }
        Ok(clauses)
    }

    // -------- params --------------------------------------------------

    fn parse_params(&mut self) -> Result<Params, ParseError> {
        if matches!(self.peek_kind(), TokenKind::RParen) {
            return Ok(Params::default());
        }
        self.parse_params_no_paren()
    }

    fn parse_params_no_paren(&mut self) -> Result<Params, ParseError> {
        let mut params = Params::default();
        let mut after_star = false;
        loop {
            // `*` separator without name → keyword-only follow.
            if matches!(self.peek_kind(), TokenKind::Star) {
                self.bump();
                if matches!(self.peek_kind(), TokenKind::Ident(_)) {
                    let p = self.parse_one_param()?;
                    params.var_positional = Some(p);
                }
                after_star = true;
            } else if self.eat(&TokenKind::StarStar) {
                let p = self.parse_one_param()?;
                params.var_keyword = Some(p);
            } else {
                let p = self.parse_one_param()?;
                if after_star {
                    params.keyword_only.push(p);
                } else {
                    params.positional.push(p);
                }
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
            if matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Colon) {
                break;
            }
        }
        Ok(params)
    }

    fn parse_one_param(&mut self) -> Result<Param, ParseError> {
        let start = self.current_span();
        let name = self.expect_ident()?;
        let annot = if self.eat(&TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let default = if self.eat(&TokenKind::Eq) {
            Some(self.parse_literal_default()?)
        } else {
            None
        };
        let end = self.peek().span;
        Ok(Param {
            name,
            annot,
            default,
            span: start.merge(end),
        })
    }

    fn parse_literal_default(&mut self) -> Result<Literal, ParseError> {
        let span = self.current_span();
        match self.peek_kind().clone() {
            TokenKind::Int(s) => {
                self.bump();
                Ok(Literal::Int(s))
            }
            TokenKind::Float(s) => {
                self.bump();
                Ok(Literal::Float(s))
            }
            TokenKind::Imag(s) => {
                self.bump();
                Ok(Literal::Imag(s))
            }
            TokenKind::Str { value, .. } => {
                self.bump();
                Ok(Literal::Str(value))
            }
            TokenKind::Bytes { value, .. } => {
                self.bump();
                Ok(Literal::Bytes(value))
            }
            TokenKind::KwTrue => {
                self.bump();
                Ok(Literal::Bool(true))
            }
            TokenKind::KwFalse => {
                self.bump();
                Ok(Literal::Bool(false))
            }
            TokenKind::KwNone => {
                self.bump();
                Ok(Literal::None)
            }
            TokenKind::Minus => {
                // Allow `-N` as a literal default for ergonomics.
                self.bump();
                match self.peek_kind().clone() {
                    TokenKind::Int(s) => {
                        self.bump();
                        Ok(Literal::Int(format!("-{s}")))
                    }
                    TokenKind::Float(s) => {
                        self.bump();
                        Ok(Literal::Float(format!("-{s}")))
                    }
                    _ => Err(ParseError::NonLiteralDefault { span }),
                }
            }
            _ => Err(ParseError::NonLiteralDefault { span }),
        }
    }

    // -------- types ---------------------------------------------------

    fn parse_type(&mut self) -> Result<Type, ParseError> {
        let mut t = self.parse_type_atom()?;
        // Union: `A | B | C`.
        let mut union_parts: Option<Vec<Type>> = None;
        while matches!(self.peek_kind(), TokenKind::Pipe) {
            self.bump();
            let next = self.parse_type_atom()?;
            if let Some(parts) = union_parts.as_mut() {
                let span = parts.last().expect("non-empty").span.merge(next.span);
                parts.push(next);
                let kind = TypeKind::Union(parts.clone());
                t = Type { kind, span };
            } else {
                let span = t.span.merge(next.span);
                union_parts = Some(vec![t.clone(), next]);
                t = Type {
                    kind: TypeKind::Union(union_parts.clone().expect("just set")),
                    span,
                };
            }
        }
        Ok(t)
    }

    fn parse_type_atom(&mut self) -> Result<Type, ParseError> {
        let start = self.current_span();
        // ADR-0060b §3.1 — `None` keyword as a named type. The
        // `KwNone` token (Python's `None` literal) is accepted in
        // type-annotation position and resolves to `Ty::None` via
        // `lower_named_type("None")`. Implicit-None idiom
        // (`def f(): pass`, no annotation) is unaffected — that
        // path doesn't enter `parse_type_atom`.
        if matches!(self.peek_kind(), TokenKind::KwNone) {
            let span = self.peek().span;
            self.bump();
            return Ok(Type {
                kind: TypeKind::Name(vec!["None".to_string()]),
                span,
            });
        }
        // ADR-0060b §3.2 — `&T` immutable shared borrow type. The
        // expression-position `&` form is ADR-0052a Wave-1; this
        // adds the type-annotation companion. `&&T` is parser-legal
        // but currently fails at use-site type-check (no nested-Ref
        // call-site coercion in Wave-2; deferred).
        if self.eat(&TokenKind::Amp) {
            let inner = self.parse_type_atom()?;
            let span = start.merge(inner.span);
            return Ok(Type {
                kind: TypeKind::Ref(Box::new(inner)),
                span,
            });
        }
        // ADR-0060b §3.3 — `[T; N]` fixed-size array type. Length
        // is parsed as a non-negative integer literal at wave-2
        // (no const-expr arithmetic). Empty arrays (`[T; 0]`) are
        // permitted at parse time; codegen handles them as zero-
        // sized.
        if self.eat(&TokenKind::LBracket) {
            let elem = self.parse_type()?;
            self.expect(&TokenKind::Semicolon)?;
            let len_tok = self.peek().clone();
            let len: usize = match &len_tok.kind {
                TokenKind::Int(s) => s.parse::<usize>().map_err(|_| ParseError::Syntax {
                    message: format!(
                        "array length must be a non-negative integer literal, got `{s}`"
                    ),
                    span: len_tok.span,
                })?,
                _ => {
                    return Err(ParseError::Syntax {
                        message: "array type `[T; N]` expects an integer length after `;`".into(),
                        span: len_tok.span,
                    });
                }
            };
            self.bump(); // consume the Int token
            let end = self.current_span();
            self.expect(&TokenKind::RBracket)?;
            return Ok(Type {
                kind: TypeKind::Array {
                    elem: Box::new(elem),
                    len,
                },
                span: start.merge(end),
            });
        }
        // `(A, B)` tuple type or `(A) -> B` fn type.
        if self.eat(&TokenKind::LParen) {
            let mut params = Vec::new();
            if !matches!(self.peek_kind(), TokenKind::RParen) {
                params.push(self.parse_type()?);
                while self.eat(&TokenKind::Comma) {
                    if matches!(self.peek_kind(), TokenKind::RParen) {
                        break;
                    }
                    params.push(self.parse_type()?);
                }
            }
            self.expect(&TokenKind::RParen)?;
            if self.eat(&TokenKind::Arrow) {
                let ret = self.parse_type()?;
                let end = ret.span;
                return Ok(Type {
                    kind: TypeKind::Fn {
                        params,
                        return_type: Box::new(ret),
                    },
                    span: start.merge(end),
                });
            }
            let end = self.peek().span;
            return Ok(Type {
                kind: TypeKind::Tuple(params),
                span: start.merge(end),
            });
        }
        let mut path = Vec::new();
        path.push(self.expect_ident()?);
        while self.eat(&TokenKind::Dot) {
            path.push(self.expect_ident()?);
        }
        if self.eat(&TokenKind::LBracket) {
            let mut args = Vec::new();
            if !matches!(self.peek_kind(), TokenKind::RBracket) {
                args.push(self.parse_type()?);
                while self.eat(&TokenKind::Comma) {
                    if matches!(self.peek_kind(), TokenKind::RBracket) {
                        break;
                    }
                    args.push(self.parse_type()?);
                }
            }
            let end = self.current_span();
            self.expect(&TokenKind::RBracket)?;
            return Ok(Type {
                kind: TypeKind::Generic { base: path, args },
                span: start.merge(end),
            });
        }
        let end = self.peek().span;
        Ok(Type {
            kind: TypeKind::Name(path),
            span: start.merge(end),
        })
    }

    // -------- patterns ------------------------------------------------

    fn parse_pattern_or(&mut self) -> Result<Pattern, ParseError> {
        let mut first = self.parse_pattern_simple()?;
        if matches!(self.peek_kind(), TokenKind::Pipe) {
            let mut alts = vec![first];
            while self.eat(&TokenKind::Pipe) {
                alts.push(self.parse_pattern_simple()?);
            }
            let span = alts
                .first()
                .expect("non-empty")
                .span
                .merge(alts.last().expect("non-empty").span);
            first = Pattern {
                kind: PatternKind::Or(alts),
                span,
            };
        }
        Ok(first)
    }

    fn parse_pattern_simple(&mut self) -> Result<Pattern, ParseError> {
        let tok = self.peek().clone();
        match &tok.kind {
            TokenKind::Underscore => {
                self.bump();
                Ok(Pattern {
                    kind: PatternKind::Wildcard,
                    span: tok.span,
                })
            }
            TokenKind::Ident(name) => {
                // Could be class pattern `Name(...)` or capture.
                let start = tok.span;
                self.bump();
                let mut path = vec![name.clone()];
                while self.eat(&TokenKind::Dot) {
                    path.push(self.expect_ident()?);
                }
                if matches!(self.peek_kind(), TokenKind::LParen) {
                    self.bump();
                    let mut positional = Vec::new();
                    let mut keyword = Vec::new();
                    if !matches!(self.peek_kind(), TokenKind::RParen) {
                        loop {
                            // keyword: `name=pattern`
                            let is_kw = matches!(self.peek_kind(), TokenKind::Ident(_))
                                && matches!(self.peek_at(1), TokenKind::Eq);
                            if is_kw {
                                let kn = self.expect_ident()?;
                                self.bump(); // =
                                let p = self.parse_pattern_simple()?;
                                keyword.push((kn, p));
                            } else {
                                positional.push(self.parse_pattern_simple()?);
                            }
                            if !self.eat(&TokenKind::Comma) {
                                break;
                            }
                            if matches!(self.peek_kind(), TokenKind::RParen) {
                                break;
                            }
                        }
                    }
                    let end = self.current_span();
                    self.expect(&TokenKind::RParen)?;
                    return Ok(Pattern {
                        kind: PatternKind::Class {
                            base: path,
                            positional,
                            keyword,
                        },
                        span: start.merge(end),
                    });
                }
                if path.len() == 1 {
                    return Ok(Pattern {
                        kind: PatternKind::Binding(path.into_iter().next().expect("len 1")),
                        span: start,
                    });
                }
                // Multi-segment dotted name without parens — treat as
                // class pattern with no args.
                Ok(Pattern {
                    kind: PatternKind::Class {
                        base: path,
                        positional: Vec::new(),
                        keyword: Vec::new(),
                    },
                    span: start,
                })
            }
            TokenKind::LParen => {
                self.bump();
                let (items, rest) = self.parse_pattern_seq(TokenKind::RParen)?;
                let end = self.current_span();
                self.expect(&TokenKind::RParen)?;
                Ok(Pattern {
                    kind: PatternKind::Sequence { items, rest },
                    span: tok.span.merge(end),
                })
            }
            TokenKind::LBracket => {
                self.bump();
                let (items, rest) = self.parse_pattern_seq(TokenKind::RBracket)?;
                let end = self.current_span();
                self.expect(&TokenKind::RBracket)?;
                Ok(Pattern {
                    kind: PatternKind::Sequence { items, rest },
                    span: tok.span.merge(end),
                })
            }
            TokenKind::LBrace => {
                self.bump();
                let mut entries = Vec::new();
                let mut rest = None;
                while !matches!(self.peek_kind(), TokenKind::RBrace) {
                    if self.eat(&TokenKind::StarStar) {
                        rest = Some(self.expect_ident()?);
                        break;
                    }
                    let key = self.parse_expr()?;
                    self.expect(&TokenKind::Colon)?;
                    let p = self.parse_pattern_simple()?;
                    entries.push((key, p));
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                let end = self.current_span();
                self.expect(&TokenKind::RBrace)?;
                Ok(Pattern {
                    kind: PatternKind::Mapping { entries, rest },
                    span: tok.span.merge(end),
                })
            }
            // Literal patterns.
            TokenKind::Int(_)
            | TokenKind::Float(_)
            | TokenKind::Imag(_)
            | TokenKind::Str { .. }
            | TokenKind::Bytes { .. }
            | TokenKind::KwTrue
            | TokenKind::KwFalse
            | TokenKind::KwNone => {
                // Use the same parser as default-literal then wrap.
                let lit = self.parse_literal_default()?;
                Ok(Pattern {
                    kind: PatternKind::Literal(lit),
                    span: tok.span,
                })
            }
            TokenKind::Minus => {
                let lit = self.parse_literal_default()?;
                Ok(Pattern {
                    kind: PatternKind::Literal(lit),
                    span: tok.span,
                })
            }
            other => Err(ParseError::Syntax {
                message: format!("invalid pattern start: {}", other.classify()),
                span: tok.span,
            }),
        }
    }

    fn parse_pattern_seq(
        &mut self,
        terminator: TokenKind,
    ) -> Result<(Vec<Pattern>, Option<Box<Pattern>>), ParseError> {
        let mut items = Vec::new();
        let mut rest: Option<Box<Pattern>> = None;
        while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::RBracket)
            && std::mem::discriminant(self.peek_kind()) != std::mem::discriminant(&terminator)
        {
            if self.eat(&TokenKind::Star) {
                let inner = self.parse_pattern_simple()?;
                rest = Some(Box::new(inner));
                if self.eat(&TokenKind::Comma) {
                    continue;
                }
                break;
            }
            items.push(self.parse_pattern_simple()?);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        Ok((items, rest))
    }

    // -------- helpers --------------------------------------------------

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        match self.peek_kind().clone() {
            TokenKind::Ident(s) => {
                self.bump();
                Ok(s)
            }
            TokenKind::KwMatch => {
                self.bump();
                Ok("match".into())
            }
            other => {
                let span = self.current_span();
                Err(ParseError::Expected {
                    expected: vec![TokenKind::Ident(String::new())],
                    found: other,
                    span,
                })
            }
        }
    }
}

/// Return `true` if `tok` can start a Cobrust cast-target type.
/// Used by `parse_pratt` to distinguish `expr as T` (cast) from
/// `with ctx as x` (with-binding) and `import foo as bar` (alias).
///
/// Valid cast targets in M-F.3.3 are the scalar type names: `i64`,
/// `f64`, `str`, `bool`, `bytes`, plus generic containers like
/// `list[T]`, `dict[K, V]`. We check only the FIRST token, so we
/// accept `Ident("i64")`, `Ident("f64")`, `Ident("str")`,
/// `Ident("bool")`, `Ident("bytes")` and the bracket-started generics
/// `list[`, `dict[`, `set[`. Any other identifier is NOT a cast target
/// (it is a variable name used for with-binding or import aliases).
fn is_cast_type_token(tok: &TokenKind) -> bool {
    if let TokenKind::Ident(name) = tok {
        matches!(
            name.as_str(),
            "i64"
                | "f64"
                | "int"
                | "float"
                | "str"
                | "bool"
                | "bytes"
                | "None"
                | "Never"
                | "list"
                | "dict"
                | "set"
        )
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::span::FileId;

    fn parse_src(src: &str) -> Result<Module, ParseError> {
        let toks = lex(src, FileId::SYNTHETIC).expect("lex");
        parse(&toks)
    }

    #[test]
    fn empty_module() {
        let m = parse_src("").expect("parse");
        assert!(m.items.is_empty());
        assert!(m.docstring.is_none());
    }

    #[test]
    fn module_docstring() {
        let m = parse_src("\"hello world\"\n").expect("parse");
        assert_eq!(m.docstring.as_deref(), Some("hello world"));
    }

    #[test]
    fn pass_stmt() {
        let m = parse_src("pass\n").expect("parse");
        assert_eq!(m.items.len(), 1);
        assert!(matches!(m.items[0].kind, StmtKind::Pass));
    }

    #[test]
    fn rejects_is() {
        // `is` as a *statement-leader* must return ParseError::DroppedByConstitution.
        // The parse_stmt dispatch at the `Ident("is")` arm (parser.rs §230-244)
        // fires when `is` is the first token of a statement.
        //
        // Note: `"x = is\n"` would NOT exercise this path — the parser would
        // successfully parse `is` as an identifier expression (ExprKind::Name).
        // The constitution drops `is` at statement level, not in expressions.
        // That is intentional: the expression position would be caught later
        // by the type checker / HIR pass.
        let err = parse_src("is x\n").expect_err("expected parse error for `is` statement");
        assert!(
            matches!(
                &err,
                ParseError::DroppedByConstitution {
                    name: "is",
                    span: _
                }
            ),
            "expected DroppedByConstitution {{ name: \"is\" }}, got {err:?}"
        );
    }

    #[test]
    fn rejects_is_alone_on_line() {
        // `is` with nothing after it — still DroppedByConstitution, not Eof.
        let err = parse_src("is\n").expect_err("expected parse error for bare `is`");
        assert!(
            matches!(
                &err,
                ParseError::DroppedByConstitution {
                    name: "is",
                    span: _
                }
            ),
            "expected DroppedByConstitution {{ name: \"is\" }}, got {err:?}"
        );
    }

    #[test]
    fn rejects_del_statement() {
        // `del` is also DroppedByConstitution — shares the same parser arm.
        let err = parse_src("del x\n").expect_err("expected parse error for `del`");
        assert!(
            matches!(
                &err,
                ParseError::DroppedByConstitution {
                    name: "del",
                    span: _
                }
            ),
            "expected DroppedByConstitution {{ name: \"del\" }}, got {err:?}"
        );
    }

    #[test]
    fn rejects_global_statement() {
        let err = parse_src("global x\n").expect_err("expected parse error for `global`");
        assert!(
            matches!(
                &err,
                ParseError::DroppedByConstitution {
                    name: "global",
                    span: _
                }
            ),
            "expected DroppedByConstitution {{ name: \"global\" }}, got {err:?}"
        );
    }
}
