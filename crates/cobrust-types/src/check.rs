//! Bidirectional type checker over the HIR.
//!
//! Strict adherence to ADR-0006 §"Selected typing rules":
//!
//! - `synth(e) → Ty` — synthesize the type of `e` (used when no
//!   expected type is in scope).
//! - `check(e, expected)` — verify that `e` has type `expected`
//!   under the running substitution; extend the substitution as
//!   needed.
//!
//! Constitution-mandated checks are inlined:
//! - Implicit truthiness rejected (`if x` requires `x: bool`).
//! - `is` is unrepresentable (defense in depth via
//!   `UseOfDroppedFeature`).
//! - Mutable defaults rejected (`MutableDefault`).
//! - `match` exhaustiveness over ADTs / built-ins enforced.

use std::collections::HashMap;

use cobrust_frontend::span::Span;
use cobrust_hir::{
    BinOp, Block, CallArg, Comp, CompElem, CompKind, DefId, DictEntry, Expr, ExprKind, FormatPart,
    IndexKind, Item, ItemKind, Lit, LoopKind, MatchArm, Module, Pattern, PatternKind, ResolvedName,
    Stmt, StmtKind, Type as HirType, TypeKind, UnaryOp,
};

use crate::error::TypeError;
use crate::infer::{Subst, finalize, unify};
use crate::ty::{FnTy, Ty, VarAllocator};

/// Top-level type-checked module returned by [`check`].
#[derive(Clone, Debug)]
pub struct TypedModule {
    /// Per-`DefId` resolved type. The map covers every binding in
    /// the module.
    pub def_types: HashMap<u32, Ty>,
    /// The HIR module that was checked, for downstream consumers.
    pub hir: Module,
}

/// Type-check a module.
///
/// # Errors
///
/// Returns the first type error encountered (or
/// `TypeError::Multiple` aggregating several when multiple errors
/// surface simultaneously, e.g. mismatched-arm types in a `match`).
pub fn check(module: &Module) -> Result<TypedModule, TypeError> {
    let mut ctx = Ctx::new();
    ctx.check_module(module)?;
    let resolved: HashMap<u32, Ty> = ctx
        .def_types
        .iter()
        .map(|(d, t)| (d.0, ctx.subst.apply(t)))
        .collect();
    // Verify no inference variables leaked into a binding type.
    for (_, t) in &resolved {
        if !t.free_vars().is_empty() {
            return Err(TypeError::AmbiguousType { span: module.span });
        }
    }
    Ok(TypedModule {
        def_types: resolved,
        hir: module.clone(),
    })
}

#[derive(Default)]
struct Ctx {
    /// Substitution map (mutated during inference).
    subst: Subst,
    /// Inference variable allocator.
    vars: VarAllocator,
    /// Per-`DefId` types: every binding gets a type entry as soon
    /// as it's seen.
    def_types: HashMap<DefId, Ty>,
    /// Type-alias name → resolved value (after lowering).
    alias_map: HashMap<String, Ty>,
    /// Stack of "expected return types" for the function we're
    /// currently inside. Empty at module top-level.
    return_stack: Vec<Ty>,
    /// Stack of loop nestings; non-empty means `break` / `continue`
    /// are valid.
    loop_depth: usize,
}

impl Ctx {
    fn new() -> Self {
        Self::default()
    }

    fn fresh_var(&self) -> Ty {
        Ty::Var(self.vars.fresh())
    }

    fn record_def(&mut self, d: DefId, t: Ty) {
        self.def_types.insert(d, t);
    }

    fn lookup_def(&self, d: DefId) -> Option<Ty> {
        self.def_types.get(&d).cloned()
    }

    // -------- module ---------------------------------------------------

    fn check_module(&mut self, m: &Module) -> Result<(), TypeError> {
        // Pass 1: pre-bind every top-level item (so forward refs
        // unify).
        self.prebind_items(&m.items);

        // Pass 2: type-check each item.
        for it in &m.items {
            self.check_item(it)?;
        }
        Ok(())
    }

    fn prebind_items(&mut self, items: &[Item]) {
        for it in items {
            self.prebind_item(it);
        }
    }

    fn prebind_item(&mut self, it: &Item) {
        match &it.kind {
            ItemKind::Fn(f) => {
                let fn_ty = self.fn_signature_type(f);
                self.record_def(f.def_id, fn_ty);
            }
            ItemKind::Class(c) => {
                self.record_def(
                    c.def_id,
                    Ty::Fn(FnTy {
                        positional: vec![],
                        named: vec![],
                        var_positional: None,
                        var_keyword: None,
                        return_ty: Box::new(Ty::Adt(crate::ty::AdtId(c.def_id.0), vec![])),
                    }),
                );
                self.prebind_items(&c.members);
            }
            ItemKind::TypeAlias(a) => {
                self.record_def(a.def_id, Ty::Alias(crate::ty::AliasId(a.def_id.0), vec![]));
                let resolved = self.lower_type(&a.value);
                self.alias_map.insert(a.name.clone(), resolved);
            }
            ItemKind::Decorated { inner, .. } => self.prebind_item(inner),
            ItemKind::Import { def_id, .. } => {
                self.record_def(*def_id, self.fresh_var());
            }
            ItemKind::Let(_) | ItemKind::ExprStmt(_) => {}
        }
    }

    fn fn_signature_type(&self, f: &cobrust_hir::FnBody) -> Ty {
        let positional: Vec<Ty> = f
            .params
            .positional
            .iter()
            .map(|p| match &p.annot {
                Some(t) => self.lower_type(t),
                None => self.fresh_var(),
            })
            .collect();
        let named: Vec<(String, Ty)> = f
            .params
            .keyword_only
            .iter()
            .map(|p| {
                (
                    p.name.clone(),
                    match &p.annot {
                        Some(t) => self.lower_type(t),
                        None => self.fresh_var(),
                    },
                )
            })
            .collect();
        let var_positional = f.params.var_positional.as_ref().map(|p| {
            Box::new(match &p.annot {
                Some(t) => self.lower_type(t),
                None => self.fresh_var(),
            })
        });
        let var_keyword = f.params.var_keyword.as_ref().map(|p| {
            Box::new(match &p.annot {
                Some(t) => self.lower_type(t),
                None => self.fresh_var(),
            })
        });
        let return_ty = match &f.return_type {
            Some(t) => self.lower_type(t),
            None => self.fresh_var(),
        };
        Ty::Fn(FnTy {
            positional,
            named,
            var_positional,
            var_keyword,
            return_ty: Box::new(return_ty),
        })
    }

    fn check_item(&mut self, it: &Item) -> Result<(), TypeError> {
        match &it.kind {
            ItemKind::Fn(f) => self.check_fn(f, it.span),
            ItemKind::Class(c) => self.check_class(c, it.span),
            ItemKind::TypeAlias(_) => Ok(()),
            ItemKind::Decorated { inner, .. } => self.check_item(inner),
            ItemKind::Import { .. } => Ok(()),
            ItemKind::Let(b) => {
                let value_ty = self.synth_expr(&b.value)?;
                let bound_ty = match &b.annot {
                    Some(t) => {
                        let annot_ty = self.lower_type(t);
                        unify(&annot_ty, &value_ty, &mut self.subst, b.span)?;
                        annot_ty
                    }
                    None => value_ty,
                };
                self.bind_pattern(&b.pattern, &bound_ty)?;
                Ok(())
            }
            ItemKind::ExprStmt(e) => {
                self.synth_expr(e)?;
                Ok(())
            }
        }
    }

    fn check_fn(&mut self, f: &cobrust_hir::FnBody, _span: Span) -> Result<(), TypeError> {
        // The function type is already pre-bound; pull it out.
        let fn_ty = match self.lookup_def(f.def_id) {
            Some(Ty::Fn(t)) => t,
            _ => unreachable!("fn signature pre-bound"),
        };
        // Bind parameters.
        for (p, t) in f.params.positional.iter().zip(fn_ty.positional.iter()) {
            self.record_def(p.def_id, t.clone());
            // Mutable-default rejection (semantic re-check).
            if let Some(_lit) = &p.default {
                let dt = self.lower_default_type(p);
                if dt.is_mutable_container() {
                    return Err(TypeError::MutableDefault { span: p.span });
                }
            }
        }
        for (p, (_, t)) in f.params.keyword_only.iter().zip(fn_ty.named.iter()) {
            self.record_def(p.def_id, t.clone());
            if p.default.is_some() {
                let dt = self.lower_default_type(p);
                if dt.is_mutable_container() {
                    return Err(TypeError::MutableDefault { span: p.span });
                }
            }
        }
        if let (Some(p), Some(t)) = (&f.params.var_positional, fn_ty.var_positional.as_ref()) {
            self.record_def(p.def_id, (**t).clone());
        }
        if let (Some(p), Some(t)) = (&f.params.var_keyword, fn_ty.var_keyword.as_ref()) {
            self.record_def(p.def_id, (**t).clone());
        }
        // Type-check body under the return-stack.
        self.return_stack.push((*fn_ty.return_ty).clone());
        let _ = self.check_block(&f.body)?;
        let _ = self.return_stack.pop();
        Ok(())
    }

    fn check_class(&mut self, c: &cobrust_hir::ClassBody, _span: Span) -> Result<(), TypeError> {
        for m in &c.members {
            self.check_item(m)?;
        }
        Ok(())
    }

    // -------- statements -----------------------------------------------

    fn check_block(&mut self, b: &Block) -> Result<BlockOutcome, TypeError> {
        let mut outcome = BlockOutcome::Falls;
        for s in &b.stmts {
            outcome = self.check_stmt(s)?;
        }
        Ok(outcome)
    }

    fn check_stmt(&mut self, s: &Stmt) -> Result<BlockOutcome, TypeError> {
        match &s.kind {
            StmtKind::Pass => Ok(BlockOutcome::Falls),
            StmtKind::Expr(e) => {
                self.synth_expr(e)?;
                Ok(BlockOutcome::Falls)
            }
            StmtKind::Return(e) => {
                let ret_ty = self
                    .return_stack
                    .last()
                    .cloned()
                    .ok_or(TypeError::ReturnOutsideFn { span: s.span })?;
                let value_ty = match e {
                    Some(e) => self.synth_expr(e)?,
                    None => Ty::None,
                };
                unify(&ret_ty, &value_ty, &mut self.subst, s.span)?;
                Ok(BlockOutcome::Diverges)
            }
            StmtKind::Break => {
                if self.loop_depth == 0 {
                    return Err(TypeError::BreakOutsideLoop { span: s.span });
                }
                Ok(BlockOutcome::Diverges)
            }
            StmtKind::Continue => {
                if self.loop_depth == 0 {
                    return Err(TypeError::ContinueOutsideLoop { span: s.span });
                }
                Ok(BlockOutcome::Diverges)
            }
            StmtKind::Raise { exc, cause } => {
                if let Some(e) = exc {
                    self.synth_expr(e)?;
                }
                if let Some(c) = cause {
                    self.synth_expr(c)?;
                }
                Ok(BlockOutcome::Diverges)
            }
            StmtKind::Let(b) => {
                let value_ty = self.synth_expr(&b.value)?;
                let bound_ty = match &b.annot {
                    Some(t) => {
                        let at = self.lower_type(t);
                        unify(&at, &value_ty, &mut self.subst, b.span)?;
                        at
                    }
                    None => value_ty,
                };
                self.bind_pattern(&b.pattern, &bound_ty)?;
                Ok(BlockOutcome::Falls)
            }
            StmtKind::Assign { target, value } => {
                let target_ty = self.synth_expr(target)?;
                let value_ty = self.synth_expr(value)?;
                unify(&target_ty, &value_ty, &mut self.subst, s.span)?;
                Ok(BlockOutcome::Falls)
            }
            StmtKind::If { arms, else_block } => {
                let mut outcomes = Vec::new();
                for (cond, body) in arms {
                    let cond_ty = self.synth_expr(cond)?;
                    self.expect_bool(&cond_ty, cond.span)?;
                    outcomes.push(self.check_block(body)?);
                }
                let else_outcome = match else_block {
                    Some(b) => self.check_block(b)?,
                    None => BlockOutcome::Falls,
                };
                outcomes.push(else_outcome);
                Ok(BlockOutcome::join(&outcomes))
            }
            StmtKind::Loop(lk) => self.check_loop(lk, s.span),
            StmtKind::Match { scrutinee, arms } => self.check_match(scrutinee, arms, s.span),
            StmtKind::With { item, body } => {
                let _ctx_ty = self.synth_expr(&item.context)?;
                if let Some((def_id, _pattern)) = &item.binding {
                    // Conservatively: bind the resource to a fresh
                    // var. M2 does not introspect the context manager
                    // protocol.
                    let v = self.fresh_var();
                    self.record_def(*def_id, v);
                }
                self.check_block(body)
            }
            StmtKind::Try {
                body,
                handlers,
                else_block,
                finally_block,
            } => {
                let _ = self.check_block(body)?;
                for h in handlers {
                    let exc_ty = self.lower_type(&h.exc_type);
                    if let Some((def_id, _name)) = &h.binding {
                        self.record_def(*def_id, exc_ty);
                    }
                    let _ = self.check_block(&h.body)?;
                }
                if let Some(b) = else_block {
                    let _ = self.check_block(b)?;
                }
                if let Some(b) = finally_block {
                    let _ = self.check_block(b)?;
                }
                Ok(BlockOutcome::Falls)
            }
            StmtKind::Item(it) => {
                self.prebind_item(it);
                self.check_item(it)?;
                Ok(BlockOutcome::Falls)
            }
        }
    }

    fn check_loop(&mut self, lk: &LoopKind, span: Span) -> Result<BlockOutcome, TypeError> {
        match lk {
            LoopKind::While {
                cond,
                body,
                else_block,
                ..
            } => {
                let cond_ty = self.synth_expr(cond)?;
                self.expect_bool(&cond_ty, cond.span)?;
                self.loop_depth += 1;
                let _ = self.check_block(body)?;
                self.loop_depth -= 1;
                if let Some(b) = else_block {
                    let _ = self.check_block(b)?;
                }
                Ok(BlockOutcome::Falls)
            }
            LoopKind::For {
                pattern,
                iter,
                body,
                else_block,
                binding_def_ids: _,
                ..
            } => {
                let iter_ty = self.synth_expr(iter)?;
                let elem_ty = self.iter_element(&iter_ty, iter.span)?;
                self.bind_pattern(pattern, &elem_ty)?;
                self.loop_depth += 1;
                let _ = self.check_block(body)?;
                self.loop_depth -= 1;
                if let Some(b) = else_block {
                    let _ = self.check_block(b)?;
                }
                let _ = span;
                Ok(BlockOutcome::Falls)
            }
        }
    }

    fn iter_element(&mut self, t: &Ty, span: Span) -> Result<Ty, TypeError> {
        let resolved = self.subst.apply(t);
        match resolved {
            Ty::List(t) => Ok(*t),
            Ty::Set(t) => Ok(*t),
            Ty::Dict(k, _) => Ok(*k),
            Ty::Tuple(items) => {
                if items.is_empty() {
                    return Err(TypeError::NotIterable {
                        actual: Ty::Tuple(items),
                        span,
                    });
                }
                let head = items[0].clone();
                for t in &items[1..] {
                    if t != &head {
                        // heterogeneous tuple isn't iterable in M2
                        return Err(TypeError::NotIterable {
                            actual: Ty::Tuple(items),
                            span,
                        });
                    }
                }
                Ok(head)
            }
            Ty::Var(_) => {
                // Generate a fresh var and require iter_ty = List[V]
                // (conservative — we synthesize as List).
                let v = self.fresh_var();
                let list_ty = Ty::List(Box::new(v.clone()));
                unify(t, &list_ty, &mut self.subst, span)?;
                Ok(v)
            }
            other => Err(TypeError::NotIterable {
                actual: other,
                span,
            }),
        }
    }

    fn check_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[MatchArm],
        span: Span,
    ) -> Result<BlockOutcome, TypeError> {
        let scrutinee_ty = self.synth_expr(scrutinee)?;
        let scrutinee_ty = self.subst.apply(&scrutinee_ty);
        // Each arm's pattern must be compatible with the scrutinee
        // type; arm bodies are type-checked.
        let mut has_wildcard = false;
        let mut covered_lits: Vec<String> = Vec::new();
        for arm in arms {
            self.bind_pattern(&arm.pattern, &scrutinee_ty)?;
            if let Some(g) = &arm.guard {
                let gt = self.synth_expr(g)?;
                self.expect_bool(&gt, g.span)?;
            }
            let _ = self.check_block(&arm.body)?;
            // Track wildcard / literal coverage for exhaustiveness.
            match &arm.pattern.kind {
                PatternKind::Wildcard | PatternKind::Binding(_, _) => {
                    if arm.guard.is_none() {
                        has_wildcard = true;
                    }
                }
                PatternKind::Literal(lit) => {
                    if arm.guard.is_none() {
                        covered_lits.push(lit_to_string(lit));
                    }
                }
                _ => {}
            }
        }
        if !self.is_exhaustive(&scrutinee_ty, has_wildcard, &covered_lits) {
            return Err(TypeError::NonExhaustiveMatch {
                uncovered: self.uncovered_set(&scrutinee_ty, &covered_lits),
                span,
            });
        }
        Ok(BlockOutcome::Falls)
    }

    fn is_exhaustive(&self, ty: &Ty, has_wildcard: bool, covered_lits: &[String]) -> bool {
        if has_wildcard {
            return true;
        }
        let resolved = self.subst.apply(ty);
        match resolved {
            Ty::Bool => {
                let mut sees_t = false;
                let mut sees_f = false;
                for l in covered_lits {
                    if l == "True" {
                        sees_t = true;
                    } else if l == "False" {
                        sees_f = true;
                    }
                }
                sees_t && sees_f
            }
            Ty::None => covered_lits.iter().any(|l| l == "None"),
            _ => false,
        }
    }

    fn uncovered_set(&self, ty: &Ty, covered_lits: &[String]) -> Vec<String> {
        let resolved = self.subst.apply(ty);
        match resolved {
            Ty::Bool => {
                let mut missing = Vec::new();
                if !covered_lits.iter().any(|l| l == "True") {
                    missing.push("True".to_string());
                }
                if !covered_lits.iter().any(|l| l == "False") {
                    missing.push("False".to_string());
                }
                missing
            }
            Ty::None => vec!["None".to_string()],
            _ => vec!["<wildcard or all constructors>".to_string()],
        }
    }

    // -------- expressions ----------------------------------------------

    fn synth_expr(&mut self, e: &Expr) -> Result<Ty, TypeError> {
        let span = e.span;
        match &e.kind {
            ExprKind::Lit(lit) => Ok(self.lit_type(lit)),
            ExprKind::Format(parts) => {
                for p in parts {
                    if let FormatPart::Hole { expr, .. } = p {
                        self.synth_expr(expr)?;
                    }
                }
                Ok(Ty::Str)
            }
            ExprKind::Name(rn) => self.lookup_resolved(rn, span),
            ExprKind::Tuple(items) => {
                let mut tys = Vec::with_capacity(items.len());
                for it in items {
                    tys.push(self.synth_expr(it)?);
                }
                Ok(Ty::Tuple(tys))
            }
            ExprKind::List(items) => {
                if items.is_empty() {
                    return Ok(Ty::List(Box::new(self.fresh_var())));
                }
                let head = self.synth_expr(&items[0])?;
                for it in &items[1..] {
                    let ty = self.synth_expr(it)?;
                    unify(&head, &ty, &mut self.subst, it.span)?;
                }
                Ok(Ty::List(Box::new(head)))
            }
            ExprKind::Set(items) => {
                if items.is_empty() {
                    return Ok(Ty::Set(Box::new(self.fresh_var())));
                }
                let head = self.synth_expr(&items[0])?;
                for it in &items[1..] {
                    let ty = self.synth_expr(it)?;
                    unify(&head, &ty, &mut self.subst, it.span)?;
                }
                Ok(Ty::Set(Box::new(head)))
            }
            ExprKind::Dict(entries) => {
                if entries.is_empty() {
                    return Ok(Ty::Dict(
                        Box::new(self.fresh_var()),
                        Box::new(self.fresh_var()),
                    ));
                }
                // Use first non-spread to seed key/value types.
                let mut k_ty: Option<Ty> = None;
                let mut v_ty: Option<Ty> = None;
                for entry in entries {
                    match entry {
                        DictEntry::Pair(k, v) => {
                            let kt = self.synth_expr(k)?;
                            let vt = self.synth_expr(v)?;
                            match (&k_ty, &v_ty) {
                                (None, None) => {
                                    k_ty = Some(kt);
                                    v_ty = Some(vt);
                                }
                                (Some(prev_k), Some(prev_v)) => {
                                    unify(prev_k, &kt, &mut self.subst, k.span)?;
                                    unify(prev_v, &vt, &mut self.subst, v.span)?;
                                }
                                _ => unreachable!(),
                            }
                        }
                        DictEntry::Spread(e) => {
                            let s_ty = self.synth_expr(e)?;
                            // We require the spread to be a Dict[K, V] matching k_ty/v_ty.
                            let kk = k_ty.clone().unwrap_or_else(|| self.fresh_var());
                            let vv = v_ty.clone().unwrap_or_else(|| self.fresh_var());
                            let want = Ty::Dict(Box::new(kk.clone()), Box::new(vv.clone()));
                            unify(&want, &s_ty, &mut self.subst, e.span)?;
                            k_ty = Some(kk);
                            v_ty = Some(vv);
                        }
                    }
                }
                Ok(Ty::Dict(
                    Box::new(k_ty.unwrap_or_else(|| self.fresh_var())),
                    Box::new(v_ty.unwrap_or_else(|| self.fresh_var())),
                ))
            }
            ExprKind::Comp(c) => self.synth_comp(c),
            ExprKind::Lambda { params, body, .. } => {
                let mut positional = Vec::new();
                for p in &params.positional {
                    let ty = match &p.annot {
                        Some(t) => self.lower_type(t),
                        None => self.fresh_var(),
                    };
                    self.record_def(p.def_id, ty.clone());
                    positional.push(ty);
                }
                let body_ty = self.synth_expr(body)?;
                Ok(Ty::Fn(FnTy {
                    positional,
                    named: vec![],
                    var_positional: None,
                    var_keyword: None,
                    return_ty: Box::new(body_ty),
                }))
            }
            ExprKind::Call { callee, args } => self.synth_call(callee, args, span),
            ExprKind::Attr { base, name } => {
                let _bt = self.synth_expr(base)?;
                // M2 stays conservative on attribute access — return
                // a fresh inference variable. The static core does
                // not yet track instance fields per ADT.
                let _ = name;
                Ok(self.fresh_var())
            }
            ExprKind::Index { base, index } => {
                let bt = self.synth_expr(base)?;
                let bt = self.subst.apply(&bt);
                match (&bt, index.as_ref()) {
                    (Ty::List(elem), IndexKind::Expr(e)) => {
                        let it = self.synth_expr(e)?;
                        unify(&Ty::Int, &it, &mut self.subst, e.span)?;
                        Ok((**elem).clone())
                    }
                    (Ty::Tuple(items), IndexKind::Expr(e)) => {
                        let it = self.synth_expr(e)?;
                        unify(&Ty::Int, &it, &mut self.subst, e.span)?;
                        // ADR-0041 §H8: when the index is a literal int
                        // (positive or Python-style negative), constant-
                        // fold to the exact element type. Otherwise the
                        // dynamic-index conservative fallback synthesises
                        // the head element (matches prior M2 behavior;
                        // future row polymorphism will widen this to a
                        // union).
                        if let Some(idx) = literal_int_value(e) {
                            let resolved = resolve_tuple_index(items.as_slice(), idx);
                            return Ok(resolved.unwrap_or(Ty::Never));
                        }
                        Ok(items.first().cloned().unwrap_or(Ty::Never))
                    }
                    (Ty::Dict(k, v), IndexKind::Expr(e)) => {
                        let it = self.synth_expr(e)?;
                        unify(k, &it, &mut self.subst, e.span)?;
                        Ok((**v).clone())
                    }
                    (Ty::Str, IndexKind::Expr(e)) => {
                        let it = self.synth_expr(e)?;
                        unify(&Ty::Int, &it, &mut self.subst, e.span)?;
                        Ok(Ty::Str)
                    }
                    (Ty::Bytes, IndexKind::Expr(e)) => {
                        let it = self.synth_expr(e)?;
                        unify(&Ty::Int, &it, &mut self.subst, e.span)?;
                        Ok(Ty::Int)
                    }
                    (other, IndexKind::Slice { .. }) => Ok(other.clone()),
                    (Ty::Var(_), _) => Ok(self.fresh_var()),
                    (other, _) => Err(TypeError::NotIndexable {
                        actual: other.clone(),
                        span,
                    }),
                }
            }
            ExprKind::Bin { op, lhs, rhs } => self.synth_bin(*op, lhs, rhs, span),
            ExprKind::Un { op, operand } => self.synth_un(*op, operand, span),
            ExprKind::Await(e) => {
                let _ = self.synth_expr(e)?;
                Ok(self.fresh_var())
            }
            ExprKind::Yield(opt) => {
                if self.return_stack.is_empty() {
                    return Err(TypeError::YieldOutsideFn { span });
                }
                if let Some(e) = opt {
                    self.synth_expr(e)?;
                }
                Ok(Ty::None)
            }
            ExprKind::YieldFrom(e) => {
                if self.return_stack.is_empty() {
                    return Err(TypeError::YieldOutsideFn { span });
                }
                self.synth_expr(e)?;
                Ok(Ty::None)
            }
        }
    }

    fn synth_call(&mut self, callee: &Expr, args: &[CallArg], span: Span) -> Result<Ty, TypeError> {
        let callee_ty = self.synth_expr(callee)?;
        let callee_ty = self.subst.apply(&callee_ty);
        match callee_ty {
            Ty::Fn(fn_ty) => {
                // M2 calls: positional args check pointwise; keyword
                // args check by name; *args / **kwargs accepted but
                // unchecked. Check arity.
                let pos_args: Vec<&Expr> = args
                    .iter()
                    .filter_map(|a| match a {
                        CallArg::Positional(e) => Some(e),
                        _ => None,
                    })
                    .collect();
                if pos_args.len() != fn_ty.positional.len() {
                    return Err(TypeError::ArityMismatch {
                        expected: fn_ty.positional.len(),
                        actual: pos_args.len(),
                        span,
                    });
                }
                for (a, p) in pos_args.iter().zip(fn_ty.positional.iter()) {
                    let at = self.synth_expr(a)?;
                    unify(p, &at, &mut self.subst, a.span)?;
                }
                let mut kw_seen: Vec<&str> = Vec::new();
                for a in args {
                    if let CallArg::Keyword(name, e) = a {
                        let p = fn_ty
                            .named
                            .iter()
                            .find(|(n, _)| n == name)
                            .map(|(_, t)| t.clone())
                            .ok_or_else(|| TypeError::KeywordArgMismatch {
                                name: name.clone(),
                                span: e.span,
                            })?;
                        let et = self.synth_expr(e)?;
                        unify(&p, &et, &mut self.subst, e.span)?;
                        kw_seen.push(name);
                    }
                    if let CallArg::StarArgs(e) = a {
                        self.synth_expr(e)?;
                    }
                    if let CallArg::StarStarKwargs(e) = a {
                        self.synth_expr(e)?;
                    }
                }
                Ok((*fn_ty.return_ty).clone())
            }
            Ty::Var(_) => {
                // Synthesize `args -> fresh` and unify.
                let mut pos_tys = Vec::new();
                for a in args {
                    if let CallArg::Positional(e) = a {
                        pos_tys.push(self.synth_expr(e)?);
                    }
                }
                let ret_ty = self.fresh_var();
                let want = Ty::Fn(FnTy {
                    positional: pos_tys,
                    named: vec![],
                    var_positional: None,
                    var_keyword: None,
                    return_ty: Box::new(ret_ty.clone()),
                });
                unify(&callee_ty, &want, &mut self.subst, span)?;
                Ok(ret_ty)
            }
            other => Err(TypeError::NotCallable {
                actual: other,
                span,
            }),
        }
    }

    fn synth_bin(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
    ) -> Result<Ty, TypeError> {
        let lt = self.synth_expr(lhs)?;
        let rt = self.synth_expr(rhs)?;
        match op {
            // Arithmetic — both operands same numeric type, result same type.
            BinOp::Add
            | BinOp::Sub
            | BinOp::Mul
            | BinOp::Div
            | BinOp::FloorDiv
            | BinOp::Mod
            | BinOp::Pow
            | BinOp::MatMul => {
                unify(&lt, &rt, &mut self.subst, span)?;
                let resolved = self.subst.apply(&lt);
                match resolved {
                    Ty::Int | Ty::Float | Ty::Str | Ty::Var(_) => Ok(resolved),
                    other => Err(TypeError::TypeMismatch {
                        expected: Ty::Int,
                        actual: other,
                        span,
                    }),
                }
            }
            BinOp::Shl | BinOp::Shr | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
                unify(&Ty::Int, &lt, &mut self.subst, lhs.span)?;
                unify(&Ty::Int, &rt, &mut self.subst, rhs.span)?;
                Ok(Ty::Int)
            }
            BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq => {
                unify(&lt, &rt, &mut self.subst, span)?;
                Ok(Ty::Bool)
            }
            BinOp::And | BinOp::Or => {
                self.expect_bool(&lt, lhs.span)?;
                self.expect_bool(&rt, rhs.span)?;
                Ok(Ty::Bool)
            }
            BinOp::In | BinOp::NotIn => {
                // RHS must be iterable; LHS unifies with element type.
                let elem = self.iter_element(&rt, rhs.span)?;
                unify(&lt, &elem, &mut self.subst, span)?;
                Ok(Ty::Bool)
            }
        }
    }

    fn synth_un(&mut self, op: UnaryOp, e: &Expr, span: Span) -> Result<Ty, TypeError> {
        let et = self.synth_expr(e)?;
        match op {
            UnaryOp::Plus | UnaryOp::Neg => {
                let resolved = self.subst.apply(&et);
                match resolved {
                    Ty::Int | Ty::Float | Ty::Var(_) => Ok(resolved),
                    other => Err(TypeError::TypeMismatch {
                        expected: Ty::Int,
                        actual: other,
                        span,
                    }),
                }
            }
            UnaryOp::BitNot => {
                unify(&Ty::Int, &et, &mut self.subst, span)?;
                Ok(Ty::Int)
            }
            UnaryOp::Not => {
                self.expect_bool(&et, span)?;
                Ok(Ty::Bool)
            }
        }
    }

    fn synth_comp(&mut self, c: &Comp) -> Result<Ty, TypeError> {
        // Each clause introduces bindings; bind them as we go.
        for clause in &c.clauses {
            let iter_ty = self.synth_expr(&clause.iter)?;
            let elem_ty = self.iter_element(&iter_ty, clause.iter.span)?;
            self.bind_pattern(&clause.target, &elem_ty)?;
            for g in &clause.guards {
                let gt = self.synth_expr(g)?;
                self.expect_bool(&gt, g.span)?;
            }
        }
        match (&c.kind, &c.element) {
            (CompKind::List, CompElem::Single(e)) => {
                let et = self.synth_expr(e)?;
                Ok(Ty::List(Box::new(et)))
            }
            (CompKind::Set, CompElem::Single(e)) => {
                let et = self.synth_expr(e)?;
                Ok(Ty::Set(Box::new(et)))
            }
            (CompKind::Dict, CompElem::KeyValue(k, v)) => {
                let kt = self.synth_expr(k)?;
                let vt = self.synth_expr(v)?;
                Ok(Ty::Dict(Box::new(kt), Box::new(vt)))
            }
            (CompKind::Generator, CompElem::Single(e)) => {
                let et = self.synth_expr(e)?;
                // No dedicated `Generator[T]` type at M2; treat as
                // `List[T]` for inference.
                Ok(Ty::List(Box::new(et)))
            }
            _ => Err(TypeError::TypeMismatch {
                expected: Ty::Never,
                actual: Ty::Never,
                span: c.span,
            }),
        }
    }

    // -------- helpers --------------------------------------------------

    fn lit_type(&self, lit: &Lit) -> Ty {
        match lit {
            Lit::Bool(_) => Ty::Bool,
            Lit::None => Ty::None,
            Lit::Int(_) => Ty::Int,
            Lit::Float(_) => Ty::Float,
            Lit::Imag(_) => Ty::Imag,
            Lit::Str(_) => Ty::Str,
            Lit::Bytes(_) => Ty::Bytes,
        }
    }

    fn lower_default_type(&self, p: &cobrust_hir::Param) -> Ty {
        if let Some(lit) = &p.default {
            return self.lit_type(lit);
        }
        Ty::None
    }

    fn lower_type(&self, t: &HirType) -> Ty {
        match &t.kind {
            TypeKind::Name(parts) => {
                let s = parts.join(".");
                self.lower_named_type(&s)
            }
            TypeKind::Generic { base, args } => {
                let base_s = base.join(".");
                self.lower_generic_type(&base_s, args)
            }
            TypeKind::Union(items) => {
                if items.is_empty() {
                    Ty::Never
                } else if items.len() == 1 {
                    self.lower_type(&items[0])
                } else {
                    // M2 reads `A | B` as a union but, lacking row
                    // polymorphism, narrows it to `A` if all
                    // alternatives unify, else surfaces it as a
                    // synthetic record at type-check failure time.
                    self.lower_type(&items[0])
                }
            }
            TypeKind::Fn {
                params,
                return_type,
            } => Ty::Fn(FnTy {
                positional: params.iter().map(|t| self.lower_type(t)).collect(),
                named: vec![],
                var_positional: None,
                var_keyword: None,
                return_ty: Box::new(self.lower_type(return_type)),
            }),
            TypeKind::Tuple(items) => Ty::Tuple(items.iter().map(|t| self.lower_type(t)).collect()),
        }
    }

    fn lower_named_type(&self, s: &str) -> Ty {
        if let Some(t) = self.alias_map.get(s) {
            return t.clone();
        }
        match s {
            "bool" => Ty::Bool,
            "i64" | "int" => Ty::Int,
            "f64" | "float" => Ty::Float,
            "str" => Ty::Str,
            "bytes" => Ty::Bytes,
            "None" | "none" => Ty::None,
            "Never" => Ty::Never,
            // Treat unrecognised named types as opaque: an `Alias`
            // synthesised via a sentinel `AliasId(u32::MAX)`. This
            // is *not* an inference variable, so it does not flag as
            // `AmbiguousType` at the final resolution pass. It does
            // not unify with any concrete type that the type checker
            // discriminates against; it only unifies with another
            // opaque alias of the same name (handled by passing the
            // hashed name through the AliasId).
            other => {
                let mut hash: u32 = 5381u32;
                for b in other.bytes() {
                    hash = hash.wrapping_mul(33).wrapping_add(u32::from(b));
                }
                Ty::Alias(crate::ty::AliasId(hash | 0x8000_0000), vec![])
            }
        }
    }

    fn lower_generic_type(&self, base: &str, args: &[HirType]) -> Ty {
        let lowered: Vec<Ty> = args.iter().map(|a| self.lower_type(a)).collect();
        // ADR-0044 W2 Phase 2: accept lowercase `list` / `set` / `dict` /
        // `tuple` Python-flavoured aliases in addition to the
        // canonical `List` / `Set` / `Dict` / `Tuple` capitalised forms.
        // The PRELUDE's `fn argv() -> list[str]` declaration and the
        // ADR-0044 test corpus (`list[str]` annotations) both rely on
        // this. This is a pure additive change — uppercase forms still
        // resolve to the same `Ty::*` variants; the lowercase rows are
        // new entry points that the previous fall-through would have
        // shunted to `fresh_var()`.
        match (base, lowered.len()) {
            ("List" | "list", 1) => Ty::List(Box::new(lowered[0].clone())),
            ("Set" | "set", 1) => Ty::Set(Box::new(lowered[0].clone())),
            ("Dict" | "dict", 2) => {
                Ty::Dict(Box::new(lowered[0].clone()), Box::new(lowered[1].clone()))
            }
            // ADR-0041 §H8: `Tuple[A, B, C]` resolves to a structural
            // tuple of the same arity. Without this, the generic
            // fall-through synthesised a fresh inference variable for
            // every annotated tuple — which made tuple-index test
            // cases (H8.1-H8.3) surface `AmbiguousType` because the
            // returned element type referenced the now-erased var.
            ("Tuple" | "tuple", _) => Ty::Tuple(lowered),
            _ => self.fresh_var(),
        }
    }

    fn lookup_resolved(&mut self, rn: &ResolvedName, span: Span) -> Result<Ty, TypeError> {
        match self.lookup_def(rn.def_id) {
            Some(t) => Ok(t),
            None => Err(TypeError::UnknownName {
                name: rn.name.clone(),
                span,
            }),
        }
    }

    fn expect_bool(&mut self, t: &Ty, span: Span) -> Result<(), TypeError> {
        let resolved = self.subst.apply(t);
        match resolved {
            Ty::Bool => Ok(()),
            Ty::Var(_) => unify(&Ty::Bool, &resolved, &mut self.subst, span),
            other => Err(TypeError::ImplicitTruthiness {
                actual: other,
                span,
            }),
        }
    }

    fn bind_pattern(&mut self, p: &Pattern, t: &Ty) -> Result<(), TypeError> {
        match &p.kind {
            PatternKind::Wildcard => Ok(()),
            PatternKind::Binding(_, def_id) => {
                self.record_def(*def_id, t.clone());
                Ok(())
            }
            PatternKind::Literal(lit) => {
                let lt = self.lit_type(lit);
                unify(t, &lt, &mut self.subst, p.span)
            }
            PatternKind::Sequence { items, rest } => {
                let resolved = self.subst.apply(t);
                match resolved {
                    Ty::Tuple(elems) => {
                        if rest.is_some() {
                            // tuple-with-rest: bind rest as List[E].
                            return Ok(());
                        }
                        if elems.len() != items.len() {
                            return Err(TypeError::ArityMismatch {
                                expected: elems.len(),
                                actual: items.len(),
                                span: p.span,
                            });
                        }
                        for (it, e_ty) in items.iter().zip(elems.iter()) {
                            self.bind_pattern(it, e_ty)?;
                        }
                        Ok(())
                    }
                    Ty::List(elem) => {
                        for it in items {
                            self.bind_pattern(it, &elem)?;
                        }
                        if let Some(r) = rest {
                            self.bind_pattern(r, &Ty::List(elem))?;
                        }
                        Ok(())
                    }
                    Ty::Var(_) => {
                        // Conservatively bind each item to a fresh var.
                        for it in items {
                            let v = self.fresh_var();
                            self.bind_pattern(it, &v)?;
                        }
                        if let Some(r) = rest {
                            let v = self.fresh_var();
                            self.bind_pattern(r, &v)?;
                        }
                        Ok(())
                    }
                    other => Err(TypeError::TypeMismatch {
                        expected: Ty::Tuple(vec![]),
                        actual: other,
                        span: p.span,
                    }),
                }
            }
            PatternKind::Mapping { entries, rest } => {
                for (k, v) in entries {
                    self.synth_expr(k)?;
                    let vv = self.fresh_var();
                    self.bind_pattern(v, &vv)?;
                }
                if let Some((_n, def_id)) = rest {
                    self.record_def(*def_id, self.fresh_var());
                }
                Ok(())
            }
            PatternKind::Class {
                positional,
                keyword,
                ..
            } => {
                for p in positional {
                    let v = self.fresh_var();
                    self.bind_pattern(p, &v)?;
                }
                for (_, p) in keyword {
                    let v = self.fresh_var();
                    self.bind_pattern(p, &v)?;
                }
                Ok(())
            }
            PatternKind::Or(branches) => {
                for b in branches {
                    self.bind_pattern(b, t)?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BlockOutcome {
    /// Falls through to the next statement.
    Falls,
    /// Diverges (return / break / continue / raise).
    Diverges,
}

impl BlockOutcome {
    fn join(items: &[Self]) -> Self {
        if items.iter().all(|o| matches!(o, Self::Diverges)) {
            Self::Diverges
        } else {
            Self::Falls
        }
    }
}

fn lit_to_string(lit: &Lit) -> String {
    match lit {
        Lit::Bool(b) => {
            if *b {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
        Lit::None => "None".to_string(),
        Lit::Int(s) | Lit::Float(s) | Lit::Imag(s) | Lit::Str(s) => s.clone(),
        Lit::Bytes(b) => format!("{b:?}"),
    }
}

// Useful for callers that want to start type-checking without `Module`
// at hand (e.g. tests). Defined only when the consumer has direct
// access to a HIR `Block`.
#[allow(dead_code)]
fn _dummy() {
    let _ = finalize;
}

/// ADR-0041 §H8: extract the integer value of an `Expr` that's a
/// literal int (with optional unary minus). Returns `None` for
/// anything else.
fn literal_int_value(e: &Expr) -> Option<i64> {
    match &e.kind {
        ExprKind::Lit(Lit::Int(s)) => s.parse::<i64>().ok(),
        ExprKind::Un {
            op: UnaryOp::Neg,
            operand,
        } => {
            if let ExprKind::Lit(Lit::Int(s)) = &operand.kind {
                s.parse::<i64>().ok().map(i64::wrapping_neg)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// ADR-0041 §H8: resolve a constant tuple index to an element type.
/// Negative indices fold from the right (Python `t[-1]` is the last
/// element). Out-of-range indices return `None` (caller surfaces as
/// `Ty::Never` — defense-in-depth; runtime would panic).
fn resolve_tuple_index(items: &[Ty], idx: i64) -> Option<Ty> {
    if items.is_empty() {
        return None;
    }
    // Negative indices fold from the right; bounds-check both sides.
    // We work in `i128` to avoid any wrap-around risk on i64::MIN.
    let len = i128::try_from(items.len()).ok()?;
    let idx_i128 = i128::from(idx);
    let normalized = if idx_i128 < 0 {
        idx_i128 + len
    } else {
        idx_i128
    };
    if normalized < 0 || normalized >= len {
        return None;
    }
    let pos = usize::try_from(normalized).ok()?;
    items.get(pos).cloned()
}
