//! AST → HIR lowering.
//!
//! The lowering is total in the sense documented at ADR-0005:
//! every well-formed AST yields either a well-formed HIR or a
//! [`LoweringError`]. The lowering never panics on any AST that the
//! frontend (`cobrust-frontend`) emits.
//!
//! Each AST form has a dedicated `lower_<form>` method on
//! [`Lowerer`]. The methods follow the desugaring tables in
//! ADR-0005 row-for-row.

use cobrust_frontend::ast;
use cobrust_frontend::span::Span;

use crate::desugar;
use crate::error::LoweringError;
use crate::scope::{DefAllocator, DefId, DefKind, ResolvedName, Scope};
use crate::tree as h;

/// Mutable lowering session. Owns the [`DefId`] counter and any
/// global state we'll need to thread through the compiler later
/// (file table, diagnostic sink, etc.). Shared across milestones.
#[derive(Debug, Default)]
pub struct Session {
    pub defs: DefAllocator,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Lower an AST [`ast::Module`] into a HIR [`h::Module`].
///
/// # Errors
///
/// Returns the first lowering failure encountered. Lowering failures
/// are span-bearing and structured per ADR-0005's error taxonomy.
pub fn lower(module: &ast::Module, sess: &mut Session) -> Result<h::Module, LoweringError> {
    let mut lw = Lowerer::new(sess);
    lw.lower_module(module)
}

struct Lowerer<'s> {
    sess: &'s mut Session,
    /// Stack of active scopes. `scopes.last()` is the innermost.
    scopes: Vec<Scope>,
    /// Maps `stmt.span.start` → the [`DefId`] that was allocated for
    /// that specific statement in [`Lowerer::prebind_items`]. Used by
    /// [`Lowerer::lower_module_stmt`] so that each `fn` definition gets
    /// the DefId that was assigned to IT specifically, even when the
    /// scope was later shadowed by a same-name function (M-F.3.3 Fn→Fn
    /// shadowing: user code can override PRELUDE stubs).
    stmt_def_ids: std::collections::HashMap<u32, DefId>,
}

impl<'s> Lowerer<'s> {
    fn new(sess: &'s mut Session) -> Self {
        Self {
            sess,
            scopes: vec![Scope::new()],
            stmt_def_ids: std::collections::HashMap::new(),
        }
    }

    // -------- scope plumbing -------------------------------------------

    fn enter_scope(&mut self) {
        self.scopes.push(Scope::new());
    }

    fn leave_scope(&mut self) {
        let _ = self.scopes.pop();
    }

    fn fresh(&mut self) -> DefId {
        self.sess.defs.fresh()
    }

    fn bind(
        &mut self,
        name: &str,
        def_id: DefId,
        kind: DefKind,
        span: Span,
    ) -> Result<(), LoweringError> {
        let res = self
            .scopes
            .last_mut()
            .expect("scope stack underflow")
            .bind(name, def_id, kind, span);
        match res {
            Ok(()) => Ok(()),
            Err(prior) => Err(LoweringError::DuplicateBinding {
                name: name.to_string(),
                first: prior,
                second: span,
            }),
        }
    }

    fn resolve_name(&self, name: &str) -> Option<(DefId, DefKind)> {
        for s in self.scopes.iter().rev() {
            if let Some(hit) = s.resolve(name) {
                return Some(hit);
            }
        }
        None
    }

    // -------- module ----------------------------------------------------

    fn lower_module(&mut self, m: &ast::Module) -> Result<h::Module, LoweringError> {
        // First pass: pre-bind every top-level item name so that
        // forward references (mutual recursion at module scope)
        // type-check.
        self.prebind_items(&m.items)?;

        let mut items = Vec::new();
        for stmt in &m.items {
            if let Some(it) = self.lower_module_stmt(stmt)? {
                items.extend(it);
            }
        }

        Ok(h::Module {
            docstring: m.docstring.clone(),
            items,
            span: m.span,
        })
    }

    fn prebind_items(&mut self, stmts: &[ast::Stmt]) -> Result<(), LoweringError> {
        for stmt in stmts {
            match &stmt.kind {
                ast::StmtKind::Fn(f) => {
                    let id = self.fresh();
                    // Record the DefId for this specific statement so that
                    // `lower_module_stmt` can find it even after scope shadowing.
                    self.stmt_def_ids.insert(stmt.span.start, id);
                    self.bind(&f.name, id, DefKind::Fn, stmt.span)?;
                }
                ast::StmtKind::Class(c) => {
                    let id = self.fresh();
                    self.bind(&c.name, id, DefKind::Class, stmt.span)?;
                }
                ast::StmtKind::TypeAlias(a) => {
                    let id = self.fresh();
                    self.bind(&a.name, id, DefKind::TypeAlias, stmt.span)?;
                }
                ast::StmtKind::Decorated { inner, .. } => {
                    self.prebind_items(std::slice::from_ref(inner))?;
                }
                ast::StmtKind::Import(imp) => match imp {
                    ast::ImportStmt::Import { path, alias } => {
                        let local = alias
                            .clone()
                            .or_else(|| path.last().cloned())
                            .unwrap_or_default();
                        let id = self.fresh();
                        self.bind(&local, id, DefKind::ImportAlias, stmt.span)?;
                    }
                    ast::ImportStmt::From { targets, .. } => {
                        for t in targets {
                            let local = t.alias.clone().unwrap_or_else(|| t.name.clone());
                            let id = self.fresh();
                            self.bind(&local, id, DefKind::ImportAlias, stmt.span)?;
                        }
                    }
                },
                _ => {}
            }
        }
        Ok(())
    }

    fn lookup_top_level(&self, name: &str) -> Option<(DefId, DefKind)> {
        // The module scope is `scopes[0]`. Top-level lookup peeks
        // there directly.
        self.scopes.first().and_then(|s| s.resolve(name))
    }

    fn lower_module_stmt(
        &mut self,
        stmt: &ast::Stmt,
    ) -> Result<Option<Vec<h::Item>>, LoweringError> {
        match &stmt.kind {
            ast::StmtKind::Fn(f) => {
                // Prefer the DefId that was allocated for THIS specific
                // statement in `prebind_items`. When Fn→Fn shadowing is
                // active (user overrides a PRELUDE math stub), `lookup_top_level`
                // would return the LAST-bound DefId (the user's), not the
                // PRELUDE stub's DefId — so we use `stmt_def_ids` instead.
                let def_id = self
                    .stmt_def_ids
                    .get(&stmt.span.start)
                    .copied()
                    .or_else(|| self.lookup_top_level(&f.name).map(|(id, _)| id))
                    .unwrap_or_else(|| self.fresh());
                let body = self.lower_fn_body_with_id(f, def_id, stmt.span)?;
                Ok(Some(vec![h::Item {
                    span: stmt.span,
                    kind: h::ItemKind::Fn(body),
                }]))
            }
            ast::StmtKind::Class(c) => {
                let def_id = self
                    .lookup_top_level(&c.name)
                    .map(|(id, _)| id)
                    .unwrap_or_else(|| self.fresh());
                let body = self.lower_class_body_with_id(c, def_id, stmt.span)?;
                Ok(Some(vec![h::Item {
                    span: stmt.span,
                    kind: h::ItemKind::Class(body),
                }]))
            }
            ast::StmtKind::TypeAlias(a) => {
                let def_id = self
                    .lookup_top_level(&a.name)
                    .map(|(id, _)| id)
                    .unwrap_or_else(|| self.fresh());
                let body = self.lower_type_alias_body_with_id(a, def_id, stmt.span)?;
                Ok(Some(vec![h::Item {
                    span: stmt.span,
                    kind: h::ItemKind::TypeAlias(body),
                }]))
            }
            ast::StmtKind::Decorated { decorators, inner } => {
                let inner_items =
                    self.lower_module_stmt(inner)?
                        .ok_or(LoweringError::DroppedFeature {
                            name: "decorated-non-item",
                            span: stmt.span,
                        })?;
                let mut decorator_exprs = Vec::with_capacity(decorators.len());
                for d in decorators {
                    decorator_exprs.push(self.lower_expr(d)?);
                }
                let mut wrapped = Vec::new();
                for inner in inner_items {
                    wrapped.push(h::Item {
                        span: stmt.span,
                        kind: h::ItemKind::Decorated {
                            decorators: decorator_exprs.clone(),
                            inner: Box::new(inner),
                        },
                    });
                }
                Ok(Some(wrapped))
            }
            ast::StmtKind::Import(imp) => {
                let mut items = Vec::new();
                match imp {
                    ast::ImportStmt::Import { path, alias } => {
                        let local = alias
                            .clone()
                            .or_else(|| path.last().cloned())
                            .unwrap_or_default();
                        let def_id = self
                            .lookup_top_level(&local)
                            .map(|(id, _)| id)
                            .unwrap_or_else(|| self.fresh());
                        items.push(h::Item {
                            span: stmt.span,
                            kind: h::ItemKind::Import {
                                def_id,
                                path: path.clone(),
                                local_name: local,
                                from_name: None,
                            },
                        });
                    }
                    ast::ImportStmt::From { path, targets } => {
                        for t in targets {
                            let local = t.alias.clone().unwrap_or_else(|| t.name.clone());
                            let def_id = self
                                .lookup_top_level(&local)
                                .map(|(id, _)| id)
                                .unwrap_or_else(|| self.fresh());
                            items.push(h::Item {
                                span: stmt.span,
                                kind: h::ItemKind::Import {
                                    def_id,
                                    path: path.clone(),
                                    local_name: local,
                                    from_name: Some(t.name.clone()),
                                },
                            });
                        }
                    }
                }
                Ok(Some(items))
            }
            ast::StmtKind::Let {
                target,
                annot,
                value,
            } => {
                let value_h = self.lower_expr(value)?;
                let pattern_h = self.lower_pattern_with_bindings(target, &mut Vec::new())?;
                let primary_def = match &pattern_h.kind {
                    h::PatternKind::Binding(_, id) => *id,
                    _ => self.fresh(),
                };
                let annot_h = annot.as_ref().map(|t| self.lower_type(t));
                Ok(Some(vec![h::Item {
                    span: stmt.span,
                    kind: h::ItemKind::Let(h::LetBody {
                        def_id: primary_def,
                        pattern: pattern_h,
                        annot: annot_h,
                        value: value_h,
                        span: stmt.span,
                    }),
                }]))
            }
            ast::StmtKind::Expr(e) => {
                let lowered = self.lower_expr(e)?;
                Ok(Some(vec![h::Item {
                    span: stmt.span,
                    kind: h::ItemKind::ExprStmt(lowered),
                }]))
            }
            ast::StmtKind::Pass => Ok(None),
            // Statements that are not module-items at the top level
            // (assignments, control flow, etc.) round-trip as
            // `ItemKind::ExprStmt(Expr::...)` once the type checker
            // has wider semantics; for now we surface them via the
            // module-stmt path that mirrors function-local stmts.
            other => Err(LoweringError::DroppedFeature {
                name: ast_kind_name(other),
                span: stmt.span,
            }),
        }
    }

    fn lower_fn_body_with_id(
        &mut self,
        f: &ast::FnDef,
        def_id: DefId,
        span: Span,
    ) -> Result<h::FnBody, LoweringError> {
        let return_type = f.return_type.as_ref().map(|t| self.lower_type(t));

        // ADR-0041 §H5: snapshot the next DefId BEFORE entering the
        // function scope. Every DefId allocated between this snapshot
        // and the leave_scope() below is a function-local binding;
        // names referenced inside the body whose DefId is *strictly
        // less* than this snapshot — and whose DefKind is not a
        // module-level item (`Fn` / `Class` / `TypeAlias` /
        // `ImportAlias`) — are captures.
        let local_def_id_start = self.sess.defs.count();
        self.enter_scope();
        let params = self.lower_params(&f.params)?;
        let body = self.lower_block(&f.body)?;
        let captures = self.collect_captures_block(&body, local_def_id_start);
        self.leave_scope();

        Ok(h::FnBody {
            def_id,
            name: f.name.clone(),
            params,
            return_type,
            body,
            captures,
            span,
        })
    }

    fn lower_class_body_with_id(
        &mut self,
        c: &ast::ClassDef,
        def_id: DefId,
        span: Span,
    ) -> Result<h::ClassBody, LoweringError> {
        let base = c.base.as_ref().map(|e| self.lower_expr(e)).transpose()?;
        let traits = c.traits.iter().map(|t| self.lower_type(t)).collect();

        // Class members are lowered in a fresh scope so that nested
        // method names don't leak into the enclosing module scope.
        // Free names inside method bodies still resolve through the
        // module scope via the parent chain (modulo `self` access,
        // which is type-check work — not lowering work).
        self.enter_scope();
        // Pre-bind member names so that mutual reference inside the
        // class works the same way module-level mutual recursion
        // works.
        self.prebind_items(&c.body.stmts)?;
        let mut members = Vec::new();
        for s in &c.body.stmts {
            if let Some(items) = self.lower_module_stmt(s)? {
                members.extend(items);
            }
        }
        self.leave_scope();

        Ok(h::ClassBody {
            def_id,
            name: c.name.clone(),
            base,
            traits,
            members,
            span,
        })
    }

    fn lower_type_alias_body_with_id(
        &mut self,
        a: &ast::TypeAlias,
        def_id: DefId,
        span: Span,
    ) -> Result<h::TypeAliasBody, LoweringError> {
        // Type parameters are *purely* annotations at M2; we still
        // allocate `DefId`s for them so that future ADRs can refer
        // back to them stably.
        self.enter_scope();
        let mut type_params = Vec::with_capacity(a.type_params.len());
        for name in &a.type_params {
            let id = self.fresh();
            self.bind(name, id, DefKind::TypeParam, span)?;
            type_params.push(id);
        }
        let value = self.lower_type(&a.value);
        self.leave_scope();

        Ok(h::TypeAliasBody {
            def_id,
            name: a.name.clone(),
            type_params,
            type_param_names: a.type_params.clone(),
            value,
            span,
        })
    }

    fn lower_params(&mut self, p: &ast::Params) -> Result<h::Params, LoweringError> {
        let mut out = h::Params::default();
        for ap in &p.positional {
            out.positional.push(self.lower_param(ap)?);
        }
        if let Some(vp) = &p.var_positional {
            out.var_positional = Some(self.lower_param(vp)?);
        }
        for kp in &p.keyword_only {
            out.keyword_only.push(self.lower_param(kp)?);
        }
        if let Some(vk) = &p.var_keyword {
            out.var_keyword = Some(self.lower_param(vk)?);
        }
        Ok(out)
    }

    fn lower_param(&mut self, p: &ast::Param) -> Result<h::Param, LoweringError> {
        let id = self.fresh();
        self.bind(&p.name, id, DefKind::Param, p.span)?;
        let annot = p.annot.as_ref().map(|t| self.lower_type(t));
        let default = p.default.clone().map(desugar::lower_literal);
        Ok(h::Param {
            def_id: id,
            name: p.name.clone(),
            annot,
            default,
            span: p.span,
        })
    }

    // -------- statements (function-local) --------------------------------

    fn lower_block(&mut self, b: &ast::Block) -> Result<h::Block, LoweringError> {
        // A block opens a sub-scope so that `let` introduced by the
        // block doesn't leak out, but we *don't* `prebind_items` in
        // function-local blocks — local fn definitions get bound
        // when they're encountered in source order, matching the
        // user's intuition.
        self.enter_scope();
        // Pre-bind nested fn/class/type-alias only — `let` is
        // sequence-bound.
        let mut prebind = Vec::new();
        for s in &b.stmts {
            match &s.kind {
                ast::StmtKind::Fn(_) | ast::StmtKind::Class(_) | ast::StmtKind::TypeAlias(_) => {
                    prebind.push(s.clone());
                }
                ast::StmtKind::Decorated { inner, .. } => match &inner.kind {
                    ast::StmtKind::Fn(_)
                    | ast::StmtKind::Class(_)
                    | ast::StmtKind::TypeAlias(_) => prebind.push((**inner).clone()),
                    _ => {}
                },
                _ => {}
            }
        }
        self.prebind_items(&prebind)?;

        let mut out = Vec::with_capacity(b.stmts.len());
        for s in &b.stmts {
            self.lower_stmt_into(s, &mut out)?;
        }
        self.leave_scope();
        Ok(h::Block {
            stmts: out,
            span: b.span,
        })
    }

    fn lower_stmt_into(
        &mut self,
        s: &ast::Stmt,
        out: &mut Vec<h::Stmt>,
    ) -> Result<(), LoweringError> {
        let span = s.span;
        match &s.kind {
            ast::StmtKind::Pass => out.push(h::Stmt {
                kind: h::StmtKind::Pass,
                span,
            }),
            ast::StmtKind::Expr(e) => {
                let lowered = self.lower_expr(e)?;
                out.push(h::Stmt {
                    kind: h::StmtKind::Expr(lowered),
                    span,
                });
            }
            ast::StmtKind::Return(e) => {
                let lowered = match e {
                    Some(e) => Some(self.lower_expr(e)?),
                    None => None,
                };
                out.push(h::Stmt {
                    kind: h::StmtKind::Return(lowered),
                    span,
                });
            }
            ast::StmtKind::BreakContinue(b) => out.push(h::Stmt {
                kind: match b {
                    ast::BreakKind::Break => h::StmtKind::Break,
                    ast::BreakKind::Continue => h::StmtKind::Continue,
                },
                span,
            }),
            ast::StmtKind::Raise { exc, cause } => {
                let exc = match exc {
                    Some(e) => Some(self.lower_expr(e)?),
                    None => None,
                };
                let cause = match cause {
                    Some(e) => Some(self.lower_expr(e)?),
                    None => None,
                };
                out.push(h::Stmt {
                    kind: h::StmtKind::Raise { exc, cause },
                    span,
                });
            }
            ast::StmtKind::Let {
                target,
                annot,
                value,
            } => {
                let value_h = self.lower_expr(value)?;
                let pattern_h = self.lower_pattern_with_bindings(target, &mut Vec::new())?;
                let primary_def = match &pattern_h.kind {
                    h::PatternKind::Binding(_, id) => *id,
                    _ => self.fresh(),
                };
                let annot_h = annot.as_ref().map(|t| self.lower_type(t));
                out.push(h::Stmt {
                    kind: h::StmtKind::Let(h::LetBody {
                        def_id: primary_def,
                        pattern: pattern_h,
                        annot: annot_h,
                        value: value_h,
                        span,
                    }),
                    span,
                });
            }
            ast::StmtKind::Assign { target, op, value } => {
                let target_h = self.lower_expr(target)?;
                let value_h = self.lower_expr(value)?;
                let op_bin = desugar::assign_op_to_bin(*op);
                let final_value = match op_bin {
                    None => value_h,
                    Some(bin_op) => h::Expr {
                        span,
                        kind: h::ExprKind::Bin {
                            op: bin_op,
                            lhs: Box::new(target_h.clone()),
                            rhs: Box::new(value_h),
                        },
                    },
                };
                self.check_assign_target(&target_h)?;
                out.push(h::Stmt {
                    kind: h::StmtKind::Assign {
                        target: Box::new(target_h),
                        value: final_value,
                    },
                    span,
                });
            }
            ast::StmtKind::If {
                cond,
                then_block,
                elifs,
                else_block,
            } => {
                let cond_h = self.lower_expr(cond)?;
                let then_h = self.lower_block(then_block)?;
                let mut arms = vec![(cond_h, then_h)];
                for (c, b) in elifs {
                    arms.push((self.lower_expr(c)?, self.lower_block(b)?));
                }
                let else_h = match else_block {
                    Some(b) => Some(self.lower_block(b)?),
                    None => None,
                };
                out.push(h::Stmt {
                    kind: h::StmtKind::If {
                        arms,
                        else_block: else_h,
                    },
                    span,
                });
            }
            ast::StmtKind::While {
                cond,
                body,
                else_block,
            } => {
                let cond_h = self.lower_expr(cond)?;
                let body_h = self.lower_block(body)?;
                let else_h = match else_block {
                    Some(b) => Some(self.lower_block(b)?),
                    None => None,
                };
                out.push(h::Stmt {
                    kind: h::StmtKind::Loop(h::LoopKind::While {
                        cond: cond_h,
                        body: body_h,
                        else_block: else_h,
                        span,
                    }),
                    span,
                });
            }
            ast::StmtKind::For {
                target,
                iter,
                body,
                else_block,
            } => {
                let iter_h = self.lower_expr(iter)?;
                self.enter_scope();
                let mut bindings = Vec::new();
                let pat_h = self.lower_pattern_with_bindings(target, &mut bindings)?;
                let body_h = self.lower_block(body)?;
                self.leave_scope();
                let else_h = match else_block {
                    Some(b) => Some(self.lower_block(b)?),
                    None => None,
                };
                out.push(h::Stmt {
                    kind: h::StmtKind::Loop(h::LoopKind::For {
                        binding_def_ids: bindings,
                        pattern: pat_h,
                        iter: iter_h,
                        body: body_h,
                        else_block: else_h,
                        span,
                    }),
                    span,
                });
            }
            ast::StmtKind::Match { scrutinee, arms } => {
                let scrutinee_h = self.lower_expr(scrutinee)?;
                let mut arms_h = Vec::with_capacity(arms.len());
                for arm in arms {
                    self.enter_scope();
                    let mut bindings = Vec::new();
                    let pat_h = self.lower_pattern_with_bindings(&arm.pattern, &mut bindings)?;
                    let guard_h = match &arm.guard {
                        Some(g) => Some(self.lower_expr(g)?),
                        None => None,
                    };
                    let body_h = self.lower_block(&arm.body)?;
                    self.leave_scope();
                    arms_h.push(h::MatchArm {
                        pattern: pat_h,
                        binding_def_ids: bindings,
                        guard: guard_h,
                        body: body_h,
                        span: arm.body.span,
                    });
                }
                out.push(h::Stmt {
                    kind: h::StmtKind::Match {
                        scrutinee: scrutinee_h,
                        arms: arms_h,
                    },
                    span,
                });
            }
            ast::StmtKind::With { items, body } => {
                // ADR-0005 row 13: left-fold multi-binding `with`.
                self.lower_with_chain(items, body, span, out)?;
            }
            ast::StmtKind::Try {
                body,
                handlers,
                else_block,
                finally_block,
            } => {
                let body_h = self.lower_block(body)?;
                let mut handlers_h = Vec::with_capacity(handlers.len());
                for h_ast in handlers {
                    let exc_type = self.lower_type(&h_ast.exc_type);
                    self.enter_scope();
                    let binding = match &h_ast.binding {
                        Some(name) => {
                            let id = self.fresh();
                            self.bind(name, id, DefKind::ExceptBinding, h_ast.body.span)?;
                            Some((id, name.clone()))
                        }
                        None => None,
                    };
                    let body_h = self.lower_block(&h_ast.body)?;
                    self.leave_scope();
                    handlers_h.push(h::ExceptHandler {
                        exc_type,
                        binding,
                        body: body_h,
                        span: h_ast.body.span,
                    });
                }
                let else_h = match else_block {
                    Some(b) => Some(self.lower_block(b)?),
                    None => None,
                };
                let finally_h = match finally_block {
                    Some(b) => Some(self.lower_block(b)?),
                    None => None,
                };
                out.push(h::Stmt {
                    kind: h::StmtKind::Try {
                        body: body_h,
                        handlers: handlers_h,
                        else_block: else_h,
                        finally_block: finally_h,
                    },
                    span,
                });
            }
            // Nested function / class / type-alias / decorator
            // become `Stmt::Item`.
            ast::StmtKind::Fn(f) => {
                let def_id = self
                    .lookup_top_level(&f.name)
                    .or_else(|| self.scopes.last().and_then(|s| s.resolve(&f.name)))
                    .map(|(id, _)| id)
                    .unwrap_or_else(|| self.fresh());
                let body = self.lower_fn_body_with_id(f, def_id, span)?;
                out.push(h::Stmt {
                    kind: h::StmtKind::Item(h::Item {
                        span,
                        kind: h::ItemKind::Fn(body),
                    }),
                    span,
                });
            }
            ast::StmtKind::Class(c) => {
                let def_id = self
                    .scopes
                    .last()
                    .and_then(|s| s.resolve(&c.name))
                    .map(|(id, _)| id)
                    .unwrap_or_else(|| self.fresh());
                let body = self.lower_class_body_with_id(c, def_id, span)?;
                out.push(h::Stmt {
                    kind: h::StmtKind::Item(h::Item {
                        span,
                        kind: h::ItemKind::Class(body),
                    }),
                    span,
                });
            }
            ast::StmtKind::TypeAlias(a) => {
                let def_id = self
                    .scopes
                    .last()
                    .and_then(|s| s.resolve(&a.name))
                    .map(|(id, _)| id)
                    .unwrap_or_else(|| self.fresh());
                let body = self.lower_type_alias_body_with_id(a, def_id, span)?;
                out.push(h::Stmt {
                    kind: h::StmtKind::Item(h::Item {
                        span,
                        kind: h::ItemKind::TypeAlias(body),
                    }),
                    span,
                });
            }
            ast::StmtKind::Decorated { decorators, inner } => {
                let mut decorator_exprs = Vec::with_capacity(decorators.len());
                for d in decorators {
                    decorator_exprs.push(self.lower_expr(d)?);
                }
                let mut tmp = Vec::new();
                self.lower_stmt_into(inner, &mut tmp)?;
                for inner_stmt in tmp {
                    if let h::StmtKind::Item(item) = inner_stmt.kind {
                        out.push(h::Stmt {
                            kind: h::StmtKind::Item(h::Item {
                                span,
                                kind: h::ItemKind::Decorated {
                                    decorators: decorator_exprs.clone(),
                                    inner: Box::new(item),
                                },
                            }),
                            span,
                        });
                    } else {
                        return Err(LoweringError::DroppedFeature {
                            name: "decorated-non-item",
                            span,
                        });
                    }
                }
            }
            ast::StmtKind::Import(imp) => {
                // Function-local imports are valid: lower them like
                // module-level imports.
                let mut tmp = Vec::new();
                let pseudo = ast::Stmt {
                    kind: ast::StmtKind::Import(imp.clone()),
                    span,
                };
                self.prebind_items(std::slice::from_ref(&pseudo))?;
                let items = self.lower_module_stmt(&pseudo)?.unwrap_or_default();
                for it in items {
                    tmp.push(h::Stmt {
                        kind: h::StmtKind::Item(it),
                        span,
                    });
                }
                out.extend(tmp);
            }
        }
        Ok(())
    }

    fn lower_with_chain(
        &mut self,
        items: &[ast::WithItem],
        body: &ast::Block,
        outer_span: Span,
        out: &mut Vec<h::Stmt>,
    ) -> Result<(), LoweringError> {
        if items.is_empty() {
            // Empty `with` — surface as plain block lowering.
            let body_h = self.lower_block(body)?;
            for s in body_h.stmts {
                out.push(s);
            }
            return Ok(());
        }
        let head = &items[0];
        let context = self.lower_expr(&head.context)?;
        self.enter_scope();
        let binding = match &head.target {
            Some(pat) => {
                let mut bindings = Vec::new();
                let pat_h = self.lower_pattern_with_bindings(pat, &mut bindings)?;
                let id = match &pat_h.kind {
                    h::PatternKind::Binding(_, id) => *id,
                    _ => *bindings.first().unwrap_or(&self.sess.defs.fresh()),
                };
                Some((id, pat_h))
            }
            None => None,
        };
        let inner_block = if items.len() == 1 {
            self.lower_block(body)?
        } else {
            // Build a block that contains a single nested `with`
            // for the remaining items, lowered recursively.
            let mut inner = Vec::new();
            self.lower_with_chain(&items[1..], body, outer_span, &mut inner)?;
            h::Block {
                span: body.span,
                stmts: inner,
            }
        };
        self.leave_scope();
        out.push(h::Stmt {
            kind: h::StmtKind::With {
                item: h::WithItem {
                    context,
                    binding,
                    span: outer_span,
                },
                body: inner_block,
            },
            span: outer_span,
        });
        Ok(())
    }

    fn check_assign_target(&self, target: &h::Expr) -> Result<(), LoweringError> {
        match &target.kind {
            h::ExprKind::Name(_) | h::ExprKind::Attr { .. } | h::ExprKind::Index { .. } => Ok(()),
            h::ExprKind::Tuple(_) | h::ExprKind::List(_) => Ok(()),
            _ => Err(LoweringError::AssignToUnknown {
                name: "<non-l-value>".to_string(),
                span: target.span,
            }),
        }
    }

    // -------- expressions ----------------------------------------------

    fn lower_expr(&mut self, e: &ast::Expr) -> Result<h::Expr, LoweringError> {
        let span = e.span;
        let kind = match &e.kind {
            ast::ExprKind::Literal(l) => h::ExprKind::Lit(desugar::lower_literal(l.clone())),
            ast::ExprKind::FString(parts) => {
                let mut out = Vec::with_capacity(parts.len());
                for p in parts {
                    match p {
                        ast::FStrPart::Lit(s) => out.push(h::FormatPart::Lit(s.clone())),
                        ast::FStrPart::Expr {
                            expr,
                            debug_equals,
                            format_spec,
                        } => {
                            let lowered = self.lower_expr(expr)?;
                            out.push(h::FormatPart::Hole {
                                expr: lowered,
                                debug_equals: *debug_equals,
                                format_spec: format_spec.clone(),
                            });
                        }
                    }
                }
                h::ExprKind::Format(out)
            }
            ast::ExprKind::Name(n) => match self.resolve_name(n) {
                Some((def_id, kind)) => h::ExprKind::Name(ResolvedName {
                    name: n.clone(),
                    def_id,
                    kind,
                }),
                None => {
                    return Err(LoweringError::UnknownName {
                        name: n.clone(),
                        span,
                    });
                }
            },
            ast::ExprKind::Collection(c) => match c {
                ast::CollectionLit::Tuple(es) => {
                    let mut out = Vec::with_capacity(es.len());
                    for e in es {
                        out.push(self.lower_expr(e)?);
                    }
                    h::ExprKind::Tuple(out)
                }
                ast::CollectionLit::List(es) => {
                    let mut out = Vec::with_capacity(es.len());
                    for e in es {
                        out.push(self.lower_expr(e)?);
                    }
                    h::ExprKind::List(out)
                }
                ast::CollectionLit::Set(es) => {
                    let mut out = Vec::with_capacity(es.len());
                    for e in es {
                        out.push(self.lower_expr(e)?);
                    }
                    h::ExprKind::Set(out)
                }
                ast::CollectionLit::Dict(es) => {
                    let mut out = Vec::with_capacity(es.len());
                    for entry in es {
                        match entry {
                            ast::DictEntry::Pair(k, v) => {
                                out.push(h::DictEntry::Pair(
                                    self.lower_expr(k)?,
                                    self.lower_expr(v)?,
                                ));
                            }
                            ast::DictEntry::Spread(e) => {
                                out.push(h::DictEntry::Spread(self.lower_expr(e)?));
                            }
                        }
                    }
                    h::ExprKind::Dict(out)
                }
            },
            ast::ExprKind::Comprehension(c) => {
                self.enter_scope();
                let mut clauses = Vec::with_capacity(c.clauses.len());
                for cl in &c.clauses {
                    let iter = self.lower_expr(&cl.iter)?;
                    let mut bindings = Vec::new();
                    let pat = self.lower_pattern_with_bindings(&cl.target, &mut bindings)?;
                    let mut guards = Vec::with_capacity(cl.guards.len());
                    for g in &cl.guards {
                        guards.push(self.lower_expr(g)?);
                    }
                    clauses.push(h::CompClause {
                        binding_def_ids: bindings,
                        target: pat,
                        iter,
                        guards,
                    });
                }
                let element = match &c.element {
                    ast::ComprehensionElem::Single(e) => h::CompElem::Single(self.lower_expr(e)?),
                    ast::ComprehensionElem::KeyValue(k, v) => {
                        h::CompElem::KeyValue(self.lower_expr(k)?, self.lower_expr(v)?)
                    }
                };
                self.leave_scope();
                let kind = match c.kind {
                    ast::ComprehensionKind::List => h::CompKind::List,
                    ast::ComprehensionKind::Set => h::CompKind::Set,
                    ast::ComprehensionKind::Dict => h::CompKind::Dict,
                    ast::ComprehensionKind::Generator => h::CompKind::Generator,
                };
                h::ExprKind::Comp(Box::new(h::Comp {
                    kind,
                    element,
                    clauses,
                    span,
                }))
            }
            ast::ExprKind::Lambda { params, body } => {
                // ADR-0041 §H5: same capture-detection scheme as
                // fn-body — snapshot `next_def_id` before opening the
                // lambda scope; any DefId allocated for params/locals
                // is `>= snapshot`. Names referenced whose DefId is
                // `< snapshot` (and not a module-level global) capture.
                let local_def_id_start = self.sess.defs.count();
                self.enter_scope();
                let params_h = self.lower_params(params)?;
                let body_h = self.lower_expr(body)?;
                let captures = self.collect_captures_expr(&body_h, local_def_id_start);
                self.leave_scope();
                h::ExprKind::Lambda {
                    params: params_h,
                    body: Box::new(body_h),
                    captures,
                }
            }
            ast::ExprKind::Call { callee, args } => {
                let callee_h = self.lower_expr(callee)?;
                let mut args_h = Vec::with_capacity(args.len());
                for a in args {
                    args_h.push(match a {
                        ast::CallArg::Positional(e) => h::CallArg::Positional(self.lower_expr(e)?),
                        ast::CallArg::Keyword(k, e) => {
                            h::CallArg::Keyword(k.clone(), self.lower_expr(e)?)
                        }
                        ast::CallArg::StarArgs(e) => h::CallArg::StarArgs(self.lower_expr(e)?),
                        ast::CallArg::StarStarKwargs(e) => {
                            h::CallArg::StarStarKwargs(self.lower_expr(e)?)
                        }
                    });
                }
                h::ExprKind::Call {
                    callee: Box::new(callee_h),
                    args: args_h,
                }
            }
            ast::ExprKind::Access(a) => match a {
                ast::AccessKind::Attribute { base, name } => {
                    let base_h = self.lower_expr(base)?;
                    h::ExprKind::Attr {
                        base: Box::new(base_h),
                        name: name.clone(),
                    }
                }
                ast::AccessKind::Index { base, index } => {
                    let base_h = self.lower_expr(base)?;
                    let idx_h = self.lower_index(index)?;
                    h::ExprKind::Index {
                        base: Box::new(base_h),
                        index: Box::new(idx_h),
                    }
                }
            },
            ast::ExprKind::Binary { op, lhs, rhs } => {
                let lhs_h = self.lower_expr(lhs)?;
                let rhs_h = self.lower_expr(rhs)?;
                h::ExprKind::Bin {
                    op: desugar::lower_bin_op(*op),
                    lhs: Box::new(lhs_h),
                    rhs: Box::new(rhs_h),
                }
            }
            ast::ExprKind::Unary { op, operand } => {
                let inner = self.lower_expr(operand)?;
                h::ExprKind::Un {
                    op: desugar::lower_unary_op(*op),
                    operand: Box::new(inner),
                }
            }
            // ADR-0052a Wave-1 — `&expr` borrow lowering: 1:1 AST→HIR mirror.
            ast::ExprKind::Borrow(inner) => {
                let inner_h = self.lower_expr(inner)?;
                h::ExprKind::Borrow(Box::new(inner_h))
            }
            ast::ExprKind::Await(e) => h::ExprKind::Await(Box::new(self.lower_expr(e)?)),
            ast::ExprKind::Yield(opt) => {
                let lowered = match opt {
                    Some(e) => Some(Box::new(self.lower_expr(e)?)),
                    None => None,
                };
                h::ExprKind::Yield(lowered)
            }
            ast::ExprKind::YieldFrom(e) => h::ExprKind::YieldFrom(Box::new(self.lower_expr(e)?)),
            ast::ExprKind::Cast { expr, target } => h::ExprKind::Cast {
                expr: Box::new(self.lower_expr(expr)?),
                target: target.clone(),
            },
        };
        Ok(h::Expr { kind, span })
    }

    fn lower_index(&mut self, ik: &ast::IndexKind) -> Result<h::IndexKind, LoweringError> {
        Ok(match ik {
            ast::IndexKind::Expr(e) => h::IndexKind::Expr(self.lower_expr(e)?),
            ast::IndexKind::Slice { start, stop, step } => h::IndexKind::Slice {
                start: match start {
                    Some(e) => Some(self.lower_expr(e)?),
                    None => None,
                },
                stop: match stop {
                    Some(e) => Some(self.lower_expr(e)?),
                    None => None,
                },
                step: match step {
                    Some(e) => Some(self.lower_expr(e)?),
                    None => None,
                },
            },
            ast::IndexKind::Tuple(items) => {
                let mut out = Vec::with_capacity(items.len());
                for it in items {
                    out.push(self.lower_index(it)?);
                }
                h::IndexKind::Tuple(out)
            }
        })
    }

    fn lower_pattern_with_bindings(
        &mut self,
        p: &ast::Pattern,
        out: &mut Vec<DefId>,
    ) -> Result<h::Pattern, LoweringError> {
        let span = p.span;
        let kind = match &p.kind {
            ast::PatternKind::Wildcard => h::PatternKind::Wildcard,
            ast::PatternKind::Binding(name) => {
                // ADR-0003 lists `_` as a soft keyword for wildcard
                // patterns. The frontend currently lexes a lone `_`
                // followed by ASCII non-identifier as an `Ident("_")`
                // (M1 self-consistency held via round-trip but the
                // AST shape did not match the constitution). The HIR
                // canonicalises: a binding pattern named exactly `_`
                // is a wildcard, full stop.
                if name == "_" {
                    h::PatternKind::Wildcard
                } else {
                    let id = self.fresh();
                    self.bind(name, id, DefKind::PatternBinding, span)?;
                    out.push(id);
                    h::PatternKind::Binding(name.clone(), id)
                }
            }
            ast::PatternKind::Literal(l) => {
                h::PatternKind::Literal(desugar::lower_literal(l.clone()))
            }
            ast::PatternKind::Sequence { items, rest } => {
                let mut items_h = Vec::with_capacity(items.len());
                for it in items {
                    items_h.push(self.lower_pattern_with_bindings(it, out)?);
                }
                let rest_h = match rest {
                    Some(r) => Some(Box::new(self.lower_pattern_with_bindings(r, out)?)),
                    None => None,
                };
                h::PatternKind::Sequence {
                    items: items_h,
                    rest: rest_h,
                }
            }
            ast::PatternKind::Mapping { entries, rest } => {
                let mut entries_h = Vec::with_capacity(entries.len());
                for (k, v) in entries {
                    let k_h = self.lower_expr(k)?;
                    let v_h = self.lower_pattern_with_bindings(v, out)?;
                    entries_h.push((k_h, v_h));
                }
                let rest_h = match rest {
                    Some(name) => {
                        let id = self.fresh();
                        self.bind(name, id, DefKind::PatternBinding, span)?;
                        out.push(id);
                        Some((name.clone(), id))
                    }
                    None => None,
                };
                h::PatternKind::Mapping {
                    entries: entries_h,
                    rest: rest_h,
                }
            }
            ast::PatternKind::Class {
                base,
                positional,
                keyword,
            } => {
                let mut pos_h = Vec::with_capacity(positional.len());
                for p in positional {
                    pos_h.push(self.lower_pattern_with_bindings(p, out)?);
                }
                let mut kw_h = Vec::with_capacity(keyword.len());
                for (k, p) in keyword {
                    kw_h.push((k.clone(), self.lower_pattern_with_bindings(p, out)?));
                }
                h::PatternKind::Class {
                    base: base.clone(),
                    positional: pos_h,
                    keyword: kw_h,
                }
            }
            ast::PatternKind::Or(branches) => {
                if branches.is_empty() {
                    h::PatternKind::Or(Vec::new())
                } else {
                    // Each branch must bind the *same set* of names.
                    // Lower each branch in its own *temporary* scope
                    // — collect the bindings of each branch — then
                    // union-check that all branches agree, and bind
                    // exactly once in the outer scope.
                    let mut branch_outs: Vec<Vec<(String, DefId)>> = Vec::new();
                    let mut branch_pats: Vec<h::Pattern> = Vec::new();
                    for b in branches {
                        self.enter_scope();
                        let mut local = Vec::new();
                        let pat = self.lower_pattern_with_bindings(b, &mut local)?;
                        let names: Vec<(String, DefId)> = self
                            .scopes
                            .last()
                            .map(|sc| {
                                sc.local_names()
                                    .map(|(n, id)| (n.clone(), id))
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        self.leave_scope();
                        branch_outs.push(names);
                        branch_pats.push(pat);
                        let _ = local;
                    }
                    let first: Vec<String> =
                        branch_outs[0].iter().map(|(n, _)| n.clone()).collect();
                    for other in branch_outs.iter().skip(1) {
                        let names: Vec<String> = other.iter().map(|(n, _)| n.clone()).collect();
                        if !same_set(&first, &names) {
                            return Err(LoweringError::OrPatternBindingMismatch { span });
                        }
                    }
                    // Bind canonical set in the outer scope and rewrite
                    // the inner-scope `DefId`s of each branch to the
                    // outer ones for downstream consumers — for M2 we
                    // simply emit the lowered patterns (each branch
                    // already holds its own `DefId`s inside the inner
                    // scope; the type checker treats or-patterns as
                    // structurally homogeneous and looks at the
                    // canonical names exposed by `binding_def_ids`).
                    for name in &first {
                        let id = self.fresh();
                        self.bind(name, id, DefKind::PatternBinding, span)?;
                        out.push(id);
                    }
                    h::PatternKind::Or(branch_pats)
                }
            }
        };
        Ok(h::Pattern { kind, span })
    }

    // -------- types -----------------------------------------------------

    fn lower_type(&mut self, t: &ast::Type) -> h::Type {
        let span = t.span;
        let kind = match &t.kind {
            ast::TypeKind::Name(parts) => h::TypeKind::Name(parts.clone()),
            ast::TypeKind::Generic { base, args } => h::TypeKind::Generic {
                base: base.clone(),
                args: args.iter().map(|a| self.lower_type(a)).collect(),
            },
            ast::TypeKind::Union(items) => {
                h::TypeKind::Union(items.iter().map(|a| self.lower_type(a)).collect())
            }
            ast::TypeKind::Fn {
                params,
                return_type,
            } => h::TypeKind::Fn {
                params: params.iter().map(|a| self.lower_type(a)).collect(),
                return_type: Box::new(self.lower_type(return_type)),
            },
            ast::TypeKind::Tuple(items) => {
                h::TypeKind::Tuple(items.iter().map(|a| self.lower_type(a)).collect())
            }
        };
        h::Type { kind, span }
    }

    // -------- captures -------------------------------------------------

    /// ADR-0041 §H5: collect free-variable captures of a function or
    /// lambda body.
    ///
    /// Algorithm:
    /// - Every `DefId` allocated *during* this body's lowering is in
    ///   the half-open range `[local_def_id_start, defs.count())`. We
    ///   only know `local_def_id_start` here; the upper bound is the
    ///   current allocator state, but irrelevant to the predicate
    ///   (captures are determined by `def_id < local_def_id_start`).
    /// - A `ResolvedName` whose `def_id.0 < local_def_id_start` was
    ///   bound *before* this body opened. Of those, the ones whose
    ///   `kind` is a module-level item (`Fn` / `Class` / `TypeAlias`
    ///   / `ImportAlias`) are global references, NOT captures. The
    ///   remainder are captures from an enclosing fn / lambda /
    ///   block scope.
    /// - The walker dedups by `(name, def_id)` so a captured variable
    ///   referenced twice yields one `CaptureSpec`.
    #[allow(clippy::unused_self)]
    fn collect_captures_block(
        &self,
        block: &h::Block,
        local_def_id_start: u32,
    ) -> Vec<h::CaptureSpec> {
        let mut out: Vec<h::CaptureSpec> = Vec::new();
        let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
        walk_block_for_captures(block, local_def_id_start, &mut out, &mut seen);
        out
    }

    /// Same algorithm as [`Self::collect_captures_block`] but for a
    /// lambda body (which is an `Expr`, not a `Block`).
    #[allow(clippy::unused_self)]
    fn collect_captures_expr(&self, e: &h::Expr, local_def_id_start: u32) -> Vec<h::CaptureSpec> {
        let mut out: Vec<h::CaptureSpec> = Vec::new();
        let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
        walk_expr_for_captures(e, local_def_id_start, &mut out, &mut seen);
        out
    }
}

// ----- ADR-0041 §H5 capture walkers ---------------------------------

fn capture_kind_is_global(k: DefKind) -> bool {
    matches!(
        k,
        DefKind::Fn | DefKind::Class | DefKind::TypeAlias | DefKind::ImportAlias
    )
}

fn record_name_capture(
    rn: &ResolvedName,
    span: Span,
    local_def_id_start: u32,
    out: &mut Vec<h::CaptureSpec>,
    seen: &mut std::collections::HashSet<u32>,
) {
    if rn.def_id.0 >= local_def_id_start {
        return;
    }
    if capture_kind_is_global(rn.kind) {
        return;
    }
    if !seen.insert(rn.def_id.0) {
        return;
    }
    out.push(h::CaptureSpec {
        name: rn.name.clone(),
        def_id: rn.def_id,
        span,
    });
}

fn walk_block_for_captures(
    block: &h::Block,
    local_def_id_start: u32,
    out: &mut Vec<h::CaptureSpec>,
    seen: &mut std::collections::HashSet<u32>,
) {
    for stmt in &block.stmts {
        walk_stmt_for_captures(stmt, local_def_id_start, out, seen);
    }
}

fn walk_stmt_for_captures(
    stmt: &h::Stmt,
    local_def_id_start: u32,
    out: &mut Vec<h::CaptureSpec>,
    seen: &mut std::collections::HashSet<u32>,
) {
    match &stmt.kind {
        h::StmtKind::Let(let_body) => {
            walk_expr_for_captures(&let_body.value, local_def_id_start, out, seen);
        }
        h::StmtKind::Assign { target, value } => {
            walk_expr_for_captures(target, local_def_id_start, out, seen);
            walk_expr_for_captures(value, local_def_id_start, out, seen);
        }
        h::StmtKind::If { arms, else_block } => {
            for (cond, body) in arms {
                walk_expr_for_captures(cond, local_def_id_start, out, seen);
                walk_block_for_captures(body, local_def_id_start, out, seen);
            }
            if let Some(b) = else_block {
                walk_block_for_captures(b, local_def_id_start, out, seen);
            }
        }
        h::StmtKind::Loop(lk) => match lk {
            h::LoopKind::While {
                cond,
                body,
                else_block,
                ..
            } => {
                walk_expr_for_captures(cond, local_def_id_start, out, seen);
                walk_block_for_captures(body, local_def_id_start, out, seen);
                if let Some(b) = else_block {
                    walk_block_for_captures(b, local_def_id_start, out, seen);
                }
            }
            h::LoopKind::For {
                iter,
                body,
                else_block,
                ..
            } => {
                walk_expr_for_captures(iter, local_def_id_start, out, seen);
                walk_block_for_captures(body, local_def_id_start, out, seen);
                if let Some(b) = else_block {
                    walk_block_for_captures(b, local_def_id_start, out, seen);
                }
            }
        },
        h::StmtKind::Match { scrutinee, arms } => {
            walk_expr_for_captures(scrutinee, local_def_id_start, out, seen);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    walk_expr_for_captures(g, local_def_id_start, out, seen);
                }
                walk_block_for_captures(&arm.body, local_def_id_start, out, seen);
            }
        }
        h::StmtKind::With { item, body } => {
            walk_expr_for_captures(&item.context, local_def_id_start, out, seen);
            walk_block_for_captures(body, local_def_id_start, out, seen);
        }
        h::StmtKind::Try {
            body,
            handlers,
            else_block,
            finally_block,
        } => {
            walk_block_for_captures(body, local_def_id_start, out, seen);
            for h in handlers {
                walk_block_for_captures(&h.body, local_def_id_start, out, seen);
            }
            if let Some(b) = else_block {
                walk_block_for_captures(b, local_def_id_start, out, seen);
            }
            if let Some(b) = finally_block {
                walk_block_for_captures(b, local_def_id_start, out, seen);
            }
        }
        h::StmtKind::Return(opt) => {
            if let Some(e) = opt {
                walk_expr_for_captures(e, local_def_id_start, out, seen);
            }
        }
        h::StmtKind::Break | h::StmtKind::Continue | h::StmtKind::Pass => {}
        h::StmtKind::Raise { exc, cause } => {
            if let Some(e) = exc {
                walk_expr_for_captures(e, local_def_id_start, out, seen);
            }
            if let Some(c) = cause {
                walk_expr_for_captures(c, local_def_id_start, out, seen);
            }
        }
        h::StmtKind::Expr(e) => walk_expr_for_captures(e, local_def_id_start, out, seen),
        h::StmtKind::Item(_) => {
            // Nested fn/class/type-alias items have their own
            // capture analysis at their own lowering site; skip.
        }
    }
}

fn walk_expr_for_captures(
    e: &h::Expr,
    local_def_id_start: u32,
    out: &mut Vec<h::CaptureSpec>,
    seen: &mut std::collections::HashSet<u32>,
) {
    match &e.kind {
        h::ExprKind::Lit(_) => {}
        h::ExprKind::Format(parts) => {
            for p in parts {
                if let h::FormatPart::Hole { expr, .. } = p {
                    walk_expr_for_captures(expr, local_def_id_start, out, seen);
                }
            }
        }
        h::ExprKind::Name(rn) => {
            record_name_capture(rn, e.span, local_def_id_start, out, seen);
        }
        h::ExprKind::Tuple(items) | h::ExprKind::List(items) | h::ExprKind::Set(items) => {
            for i in items {
                walk_expr_for_captures(i, local_def_id_start, out, seen);
            }
        }
        h::ExprKind::Dict(entries) => {
            for ent in entries {
                match ent {
                    h::DictEntry::Pair(k, v) => {
                        walk_expr_for_captures(k, local_def_id_start, out, seen);
                        walk_expr_for_captures(v, local_def_id_start, out, seen);
                    }
                    h::DictEntry::Spread(s) => {
                        walk_expr_for_captures(s, local_def_id_start, out, seen);
                    }
                }
            }
        }
        h::ExprKind::Comp(comp) => {
            for clause in &comp.clauses {
                walk_expr_for_captures(&clause.iter, local_def_id_start, out, seen);
                for g in &clause.guards {
                    walk_expr_for_captures(g, local_def_id_start, out, seen);
                }
            }
            match &comp.element {
                h::CompElem::Single(e) => {
                    walk_expr_for_captures(e, local_def_id_start, out, seen);
                }
                h::CompElem::KeyValue(k, v) => {
                    walk_expr_for_captures(k, local_def_id_start, out, seen);
                    walk_expr_for_captures(v, local_def_id_start, out, seen);
                }
            }
        }
        h::ExprKind::Lambda { body, .. } => {
            // A nested lambda has its OWN capture analysis at its
            // construction site (collect_captures_expr); from this
            // lambda's perspective, names referenced inside the
            // nested lambda's body are still captures of THIS body
            // if they were bound before THIS body opened.
            walk_expr_for_captures(body, local_def_id_start, out, seen);
        }
        h::ExprKind::Call { callee, args } => {
            walk_expr_for_captures(callee, local_def_id_start, out, seen);
            for a in args {
                match a {
                    h::CallArg::Positional(e)
                    | h::CallArg::Keyword(_, e)
                    | h::CallArg::StarArgs(e)
                    | h::CallArg::StarStarKwargs(e) => {
                        walk_expr_for_captures(e, local_def_id_start, out, seen);
                    }
                }
            }
        }
        h::ExprKind::Attr { base, .. } => {
            walk_expr_for_captures(base, local_def_id_start, out, seen);
        }
        h::ExprKind::Index { base, index } => {
            walk_expr_for_captures(base, local_def_id_start, out, seen);
            match index.as_ref() {
                h::IndexKind::Expr(e) => walk_expr_for_captures(e, local_def_id_start, out, seen),
                h::IndexKind::Slice { start, stop, step } => {
                    if let Some(e) = start {
                        walk_expr_for_captures(e, local_def_id_start, out, seen);
                    }
                    if let Some(e) = stop {
                        walk_expr_for_captures(e, local_def_id_start, out, seen);
                    }
                    if let Some(e) = step {
                        walk_expr_for_captures(e, local_def_id_start, out, seen);
                    }
                }
                h::IndexKind::Tuple(items) => {
                    for i in items {
                        walk_index_for_captures(i, local_def_id_start, out, seen);
                    }
                }
            }
        }
        h::ExprKind::Bin { lhs, rhs, .. } => {
            walk_expr_for_captures(lhs, local_def_id_start, out, seen);
            walk_expr_for_captures(rhs, local_def_id_start, out, seen);
        }
        h::ExprKind::Un { operand, .. } => {
            walk_expr_for_captures(operand, local_def_id_start, out, seen);
        }
        // ADR-0052a Wave-1 — `&inner` recurses into inner for capture tracking.
        h::ExprKind::Borrow(inner) => {
            walk_expr_for_captures(inner, local_def_id_start, out, seen);
        }
        h::ExprKind::Await(e) => walk_expr_for_captures(e, local_def_id_start, out, seen),
        h::ExprKind::Yield(opt) => {
            if let Some(e) = opt {
                walk_expr_for_captures(e, local_def_id_start, out, seen);
            }
        }
        h::ExprKind::YieldFrom(e) => walk_expr_for_captures(e, local_def_id_start, out, seen),
        h::ExprKind::Cast { expr, .. } => {
            walk_expr_for_captures(expr, local_def_id_start, out, seen);
        }
    }
}

fn walk_index_for_captures(
    idx: &h::IndexKind,
    local_def_id_start: u32,
    out: &mut Vec<h::CaptureSpec>,
    seen: &mut std::collections::HashSet<u32>,
) {
    match idx {
        h::IndexKind::Expr(e) => walk_expr_for_captures(e, local_def_id_start, out, seen),
        h::IndexKind::Slice { start, stop, step } => {
            if let Some(e) = start {
                walk_expr_for_captures(e, local_def_id_start, out, seen);
            }
            if let Some(e) = stop {
                walk_expr_for_captures(e, local_def_id_start, out, seen);
            }
            if let Some(e) = step {
                walk_expr_for_captures(e, local_def_id_start, out, seen);
            }
        }
        h::IndexKind::Tuple(items) => {
            for i in items {
                walk_index_for_captures(i, local_def_id_start, out, seen);
            }
        }
    }
}

fn same_set(a: &[String], b: &[String]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut a_sorted = a.to_vec();
    let mut b_sorted = b.to_vec();
    a_sorted.sort();
    b_sorted.sort();
    a_sorted == b_sorted
}

fn ast_kind_name(k: &ast::StmtKind) -> &'static str {
    use ast::StmtKind::*;
    match k {
        Import(_) => "import",
        Fn(_) => "fn",
        Class(_) => "class",
        Decorated { .. } => "decorated",
        TypeAlias(_) => "type_alias",
        Let { .. } => "let",
        Assign { .. } => "assign",
        If { .. } => "if",
        While { .. } => "while",
        For { .. } => "for",
        Match { .. } => "match",
        With { .. } => "with",
        Try { .. } => "try",
        Return(_) => "return",
        BreakContinue(_) => "break/continue",
        Raise { .. } => "raise",
        Pass => "pass",
        Expr(_) => "expr",
    }
}
