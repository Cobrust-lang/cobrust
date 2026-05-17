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

use std::collections::{HashMap, HashSet};

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
    /// ADR-0050c §F5 / Phase 6 — row-polymorphic widening. DefIds of
    /// PRELUDE intrinsics whose `list[T]` parameters must be
    /// instantiated with a fresh type variable at every call site
    /// instead of unified with their declared `list[i64]` shape.
    /// Populated during `prebind_item` by name match against
    /// `is_list_polymorphic_intrinsic_name`.
    poly_intrinsic_defs: HashSet<DefId>,
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
                // ADR-0050c §F5 / Phase 6 — row-polymorphic widening.
                // PRELUDE intrinsics that operate over `list[T]` for any
                // element type `T` are recorded; `synth_call` will
                // instantiate fresh vars for the `T` slot at every call
                // site (instead of unifying with the declared
                // `list[i64]` shape in PRELUDE). See `build.rs` PRELUDE.
                if is_list_polymorphic_intrinsic_name(&f.name) {
                    self.poly_intrinsic_defs.insert(f.def_id);
                }
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
            ItemKind::TypeAlias(a) => {
                // ADR-0050d §"Type-checker amendments" item 1 — `type
                // Foo = Dict[f64, i64]` rejects at the alias site.
                self.validate_hashable_dict(&a.value)
            }
            ItemKind::Decorated { inner, .. } => self.check_item(inner),
            ItemKind::Import { .. } => Ok(()),
            ItemKind::Let(b) => {
                // ADR-0050d §"Type-checker amendments" item 1 — annotation
                // site rejection. Catches `let d: Dict[f64, i64] = {}`
                // (the empty-literal case where synth_dict_lit can't see
                // K from entries).
                if let Some(t) = &b.annot {
                    self.validate_hashable_dict(t)?;
                }
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
        // ADR-0050d §"Type-checker amendments" item 1 — fn signature
        // annotation rejection. Walks every param + return annotation
        // for `Dict[K, V]` with non-hashable K. Covers i118 / i119 /
        // i120 (Dict[f64,_], Dict[List[i64],_] surface).
        for p in &f.params.positional {
            if let Some(t) = &p.annot {
                self.validate_hashable_dict(t)?;
            }
        }
        for p in &f.params.keyword_only {
            if let Some(t) = &p.annot {
                self.validate_hashable_dict(t)?;
            }
        }
        if let Some(p) = &f.params.var_positional {
            if let Some(t) = &p.annot {
                self.validate_hashable_dict(t)?;
            }
        }
        if let Some(p) = &f.params.var_keyword {
            if let Some(t) = &p.annot {
                self.validate_hashable_dict(t)?;
            }
        }
        if let Some(t) = &f.return_type {
            self.validate_hashable_dict(t)?;
        }
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
        //
        // ADR-0050a §"Scope discipline": loop scope MUST NOT cross a
        // function boundary. A nested `fn` definition resets
        // `loop_depth` to 0 for the duration of its body, then
        // restores. Without this save/restore, `break` / `continue`
        // inside a nested fn whose outer scope sits in a loop would
        // erroneously type-check.
        self.return_stack.push((*fn_ty.return_ty).clone());
        let saved_loop_depth = std::mem::take(&mut self.loop_depth);
        let _ = self.check_block(&f.body)?;
        self.loop_depth = saved_loop_depth;
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
                    // ADR-0050d Decision 7A / sub-sprint b disposition for
                    // empty `{}` without annotation: synthesise fresh
                    // `Ty::Dict(?K, ?V)`. Later use sites (annotation,
                    // subscript, comparison, return-position) pin K/V via
                    // unification. If no use pins them by the end of the
                    // module, the final-resolution pass at `check()` top
                    // surfaces `TypeError::AmbiguousType` for the leaked
                    // free vars — this is the binding Phase F.3 contract
                    // (i125 ill_typed corpus). Fresh-K inference (without
                    // requiring annotation) is intentionally Phase G —
                    // Phase F.3 minimalism mandates explicit annotation
                    // for empty-literal disambiguation.
                    return Ok(Ty::Dict(
                        Box::new(self.fresh_var()),
                        Box::new(self.fresh_var()),
                    ));
                }
                // Use first non-spread to seed key/value types.
                let mut k_ty: Option<Ty> = None;
                let mut v_ty: Option<Ty> = None;
                // Track the span of the first concrete-key pair so a
                // NotHashable diagnostic can point at the actual key
                // expression, not the outer Dict literal span.
                let mut first_k_span: Option<Span> = None;
                for entry in entries {
                    match entry {
                        DictEntry::Pair(k, v) => {
                            let kt = self.synth_expr(k)?;
                            let vt = self.synth_expr(v)?;
                            match (&k_ty, &v_ty) {
                                (None, None) => {
                                    k_ty = Some(kt);
                                    v_ty = Some(vt);
                                    first_k_span = Some(k.span);
                                }
                                (Some(prev_k), Some(prev_v)) => {
                                    unify(prev_k, &kt, &mut self.subst, k.span)?;
                                    unify(prev_v, &vt, &mut self.subst, v.span)?;
                                }
                                _ => unreachable!(),
                            }
                        }
                        DictEntry::Spread(e) => {
                            // ADR-0050d §"Parser amendments" 1 +
                            // Decision 1 commentary — dict-merge
                            // `{**other}` is Phase G; Phase F.3 rejects
                            // any spread operand in a dict literal at
                            // type-check. Synth the spread operand for
                            // diagnostic completeness even though we
                            // abort here (so the user sees a single
                            // crisp DictSpreadNotSupported diagnostic
                            // and not a cascade of unify mismatches).
                            let _ = self.synth_expr(e)?;
                            return Err(TypeError::DictSpreadNotSupported { span: e.span });
                        }
                    }
                }
                // ADR-0050d §"Type-checker amendments" item 2 —
                // Hashable predicate. After all entries unify, resolve
                // K and reject if non-hashable (matches `Ty::is_hashable`
                // contract). Examples: `{1.0: 1}` → K resolves to f64
                // → NotHashable; `{xs: 1}` where xs: List[i64] → K
                // resolves to List → NotHashable. The annotation-side
                // analogue lives in `validate_hashable_dict` invoked
                // at `Let` / `check_fn` so the empty-literal case
                // `let d: Dict[f64, i64] = {}` is also caught.
                if let Some(k) = &k_ty {
                    let k_resolved = self.subst.apply(k);
                    if !k_resolved.is_hashable() {
                        return Err(TypeError::NotHashable {
                            actual: k_resolved,
                            span: first_k_span.unwrap_or(span),
                        });
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
            // ADR-0052a Wave-1 §6 — `&expr` synth. Type is
            // `Ty::Ref(synth_expr(inner))`. The parser already ensured
            // the operand shape obeys §8 (Name / field-access /
            // indexing); the type checker just synthesises the
            // borrowed type. The §3 Wave-1 transparency rule for
            // PRELUDE Str helpers lives at `synth_call` argument-
            // binding via one-way call-site coercion (NOT here, and
            // NOT in `infer::unify` — §13 "Design lesson 2026-05-17"
            // bans bidirectional `Ref(T) ↔ T` unify).
            ExprKind::Borrow(inner) => {
                let inner_ty = self.synth_expr(inner)?;
                Ok(Ty::Ref(Box::new(inner_ty)))
            }
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
            ExprKind::Cast { expr, target } => {
                // M-F.3.3 gap (a): `x as T`. Permitted casts (constitution §2.2):
                //   i64 → f64, f64 → i64 (numeric widening / truncating).
                // Rejected: str → anything, bool → f64, anything → str, list → anything.
                // The HIR cast target is an AST Type stored verbatim. Convert it to
                // a Ty by name-matching the target type parts directly.
                let from_ty = self.synth_expr(expr)?;
                let from_resolved = finalize(&from_ty, &self.subst, span)?;
                let target_name = match &target.kind {
                    cobrust_frontend::ast::TypeKind::Name(parts) => parts.join("."),
                    _ => String::new(),
                };
                let to_resolved = self.lower_named_type(&target_name);
                let allowed = matches!(
                    (&from_resolved, &to_resolved),
                    (Ty::Int, Ty::Float) | (Ty::Float, Ty::Int)
                );
                if allowed {
                    Ok(to_resolved)
                } else {
                    Err(TypeError::TypeMismatch {
                        expected: to_resolved,
                        actual: from_resolved,
                        span,
                    })
                }
            }
        }
    }

    /// ADR-0050d sub-sprint b §"Type-checker amendments" item 4 —
    /// dict method-intrinsic recognition for `.keys()` / `.values()`
    /// / `.items()` / `.get(k)` / `.copy()`.
    ///
    /// Returns `Ok(Some(ret_ty))` if the callsite matches a dict
    /// method on a `Ty::Dict(K, V)`-typed base; `Ok(None)` if the
    /// pattern doesn't match (callsite is `Call { callee: Attr ... }`
    /// but base is not Dict, or method name is unrecognised); errors
    /// propagate when the matched method has a wrong arity / K-type.
    ///
    /// Phase F.3 scope cap (per ADR-0050d §"Surface coverage matrix"):
    /// the codegen-emit for `.keys()` / `.values()` / `.items()` /
    /// `.copy()` ships in sub-sprint d (`__cobrust_dict_iter_*` +
    /// `__cobrust_dict_clone` shims). `.get(k)` ships in the
    /// sentinel-pair scope-cap form (returns V; the typed Option
    /// return is a Phase F.3-late or Phase G follow-on per ADR-0044a
    /// timeline). This function is the type-checker side only — the
    /// MIR / codegen surfaces stay as M12.x stubs for sub-sprint d's
    /// dispatch.
    fn try_synth_dict_method(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        span: Span,
    ) -> Result<Option<Ty>, TypeError> {
        let ExprKind::Attr { base, name } = &callee.kind else {
            return Ok(None);
        };
        let base_ty = self.synth_expr(base)?;
        let base_resolved = self.subst.apply(&base_ty);
        let Ty::Dict(k_box, v_box) = base_resolved else {
            return Ok(None);
        };
        let k = *k_box;
        let v = *v_box;
        let pos_args: Vec<&Expr> = args
            .iter()
            .filter_map(|a| match a {
                CallArg::Positional(e) => Some(e),
                _ => None,
            })
            .collect();
        match name.as_str() {
            "keys" => {
                if !pos_args.is_empty() {
                    return Err(TypeError::ArityMismatch {
                        expected: 0,
                        actual: pos_args.len(),
                        span,
                    });
                }
                Ok(Some(Ty::List(Box::new(k))))
            }
            "values" => {
                if !pos_args.is_empty() {
                    return Err(TypeError::ArityMismatch {
                        expected: 0,
                        actual: pos_args.len(),
                        span,
                    });
                }
                Ok(Some(Ty::List(Box::new(v))))
            }
            "items" => {
                if !pos_args.is_empty() {
                    return Err(TypeError::ArityMismatch {
                        expected: 0,
                        actual: pos_args.len(),
                        span,
                    });
                }
                // `d.items() -> List[Tuple[K, V]]`. Insertion-order
                // iteration is a Decision 6A guarantee enforced at
                // the storage backing (sub-sprint d indexmap), not
                // at the type universe.
                Ok(Some(Ty::List(Box::new(Ty::Tuple(vec![k, v])))))
            }
            "get" => {
                // ADR-0050d §"Surface coverage matrix" caveat —
                // `.get(k)` scope-caps to `V` (not `Option[V]`) for
                // Phase F.3 because typed Option lowering is not yet
                // wired (ADR-0044a Phase F.1.x candidate). Accept
                // both the 1-arg form (`.get(k)`) and the 2-arg
                // default-fallback form (`.get(k, default)`) — the
                // latter is the wedge-audience pre-Option idiom
                // covered by dict_e2e f3d19/f3d20.
                match pos_args.len() {
                    1 => {
                        let kt = self.synth_expr(pos_args[0])?;
                        unify(&k, &kt, &mut self.subst, pos_args[0].span)?;
                        Ok(Some(v))
                    }
                    2 => {
                        let kt = self.synth_expr(pos_args[0])?;
                        unify(&k, &kt, &mut self.subst, pos_args[0].span)?;
                        let dt = self.synth_expr(pos_args[1])?;
                        unify(&v, &dt, &mut self.subst, pos_args[1].span)?;
                        Ok(Some(v))
                    }
                    _ => Err(TypeError::ArityMismatch {
                        expected: 1,
                        actual: pos_args.len(),
                        span,
                    }),
                }
            }
            "copy" => {
                // `d.copy() -> Dict[K, V]` shallow clone — Decision 10A.
                if !pos_args.is_empty() {
                    return Err(TypeError::ArityMismatch {
                        expected: 0,
                        actual: pos_args.len(),
                        span,
                    });
                }
                Ok(Some(Ty::Dict(Box::new(k), Box::new(v))))
            }
            _ => {
                // Unknown dict method — fall through to the generic
                // Attr-fresh-var path (M2 conservative behaviour).
                Ok(None)
            }
        }
    }

    /// ADR-0052d-prereq §"Surface — method-table contents per type"
    /// — Str method-form table (10 methods).
    ///
    /// Returns `Ok(Some(ret_ty))` on a matched (`Str`, name) pair;
    /// `Ok(None)` if the callee is not `Attr` or base is not `Str`;
    /// `Err(UnknownMethod)` if base IS `Str` but the method name is
    /// unrecognised (typo case per i0052dpre_01 / i0052dpre_05).
    /// Per-method arity / arg-type guards mirror
    /// `try_synth_dict_method`'s pattern.
    fn try_synth_str_method(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        span: Span,
    ) -> Result<Option<Ty>, TypeError> {
        let ExprKind::Attr { base, name } = &callee.kind else {
            return Ok(None);
        };
        let base_ty = self.synth_expr(base)?;
        let base_resolved = self.subst.apply(&base_ty);
        if !matches!(base_resolved, Ty::Str) {
            return Ok(None);
        }
        let pos_args: Vec<&Expr> = args
            .iter()
            .filter_map(|a| match a {
                CallArg::Positional(e) => Some(e),
                _ => None,
            })
            .collect();
        // Per-method arms per ADR-0052d-prereq §4 Str row.
        match name.as_str() {
            "len" => {
                if !pos_args.is_empty() {
                    return Err(TypeError::ArityMismatch {
                        expected: 0,
                        actual: pos_args.len(),
                        span,
                    });
                }
                Ok(Some(Ty::Int))
            }
            "split" => {
                if pos_args.len() != 1 {
                    return Err(TypeError::ArityMismatch {
                        expected: 1,
                        actual: pos_args.len(),
                        span,
                    });
                }
                let at = self.synth_expr(pos_args[0])?;
                unify(&Ty::Str, &at, &mut self.subst, pos_args[0].span)?;
                Ok(Some(Ty::List(Box::new(Ty::Str))))
            }
            "replace" => {
                if pos_args.len() != 2 {
                    return Err(TypeError::ArityMismatch {
                        expected: 2,
                        actual: pos_args.len(),
                        span,
                    });
                }
                let a0 = self.synth_expr(pos_args[0])?;
                unify(&Ty::Str, &a0, &mut self.subst, pos_args[0].span)?;
                let a1 = self.synth_expr(pos_args[1])?;
                unify(&Ty::Str, &a1, &mut self.subst, pos_args[1].span)?;
                Ok(Some(Ty::Str))
            }
            "trim" => {
                if !pos_args.is_empty() {
                    return Err(TypeError::ArityMismatch {
                        expected: 0,
                        actual: pos_args.len(),
                        span,
                    });
                }
                Ok(Some(Ty::Str))
            }
            "find" => {
                if pos_args.len() != 1 {
                    return Err(TypeError::ArityMismatch {
                        expected: 1,
                        actual: pos_args.len(),
                        span,
                    });
                }
                let at = self.synth_expr(pos_args[0])?;
                unify(&Ty::Str, &at, &mut self.subst, pos_args[0].span)?;
                Ok(Some(Ty::Int))
            }
            "contains" | "starts_with" | "ends_with" => {
                if pos_args.len() != 1 {
                    return Err(TypeError::ArityMismatch {
                        expected: 1,
                        actual: pos_args.len(),
                        span,
                    });
                }
                let at = self.synth_expr(pos_args[0])?;
                unify(&Ty::Str, &at, &mut self.subst, pos_args[0].span)?;
                Ok(Some(Ty::Bool))
            }
            "lower" | "upper" => {
                if !pos_args.is_empty() {
                    return Err(TypeError::ArityMismatch {
                        expected: 0,
                        actual: pos_args.len(),
                        span,
                    });
                }
                Ok(Some(Ty::Str))
            }
            other => Err(TypeError::UnknownMethod {
                type_name: "str".to_string(),
                method_name: other.to_string(),
                span,
                suggestion: str_method_suggestion(other),
            }),
        }
    }

    /// ADR-0052d-prereq §"Surface — method-table contents per type"
    /// — List method-form table (5 methods).
    ///
    /// All five rewrite to polymorphic-intrinsic targets (`list_push`,
    /// `list_get`, `list_set`, `list_is_empty`, `len`); the element
    /// type `T` is whatever the receiver's `Ty::List(T)` carries.
    fn try_synth_list_method(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        span: Span,
    ) -> Result<Option<Ty>, TypeError> {
        let ExprKind::Attr { base, name } = &callee.kind else {
            return Ok(None);
        };
        let base_ty = self.synth_expr(base)?;
        let base_resolved = self.subst.apply(&base_ty);
        let Ty::List(elem_box) = base_resolved else {
            return Ok(None);
        };
        let elem = *elem_box;
        let pos_args: Vec<&Expr> = args
            .iter()
            .filter_map(|a| match a {
                CallArg::Positional(e) => Some(e),
                _ => None,
            })
            .collect();
        match name.as_str() {
            "len" => {
                if !pos_args.is_empty() {
                    return Err(TypeError::ArityMismatch {
                        expected: 0,
                        actual: pos_args.len(),
                        span,
                    });
                }
                Ok(Some(Ty::Int))
            }
            "push" => {
                if pos_args.len() != 1 {
                    return Err(TypeError::ArityMismatch {
                        expected: 1,
                        actual: pos_args.len(),
                        span,
                    });
                }
                let at = self.synth_expr(pos_args[0])?;
                unify(&elem, &at, &mut self.subst, pos_args[0].span)?;
                // Per test stubs `list_push(xs, v) -> i64`; the return
                // is a unit-stub (Phase G P0 wrapper). Wave-2 ships
                // the i64 stub.
                Ok(Some(Ty::Int))
            }
            "get" => {
                if pos_args.len() != 1 {
                    return Err(TypeError::ArityMismatch {
                        expected: 1,
                        actual: pos_args.len(),
                        span,
                    });
                }
                let it = self.synth_expr(pos_args[0])?;
                unify(&Ty::Int, &it, &mut self.subst, pos_args[0].span)?;
                Ok(Some(elem))
            }
            "set" => {
                if pos_args.len() != 2 {
                    return Err(TypeError::ArityMismatch {
                        expected: 2,
                        actual: pos_args.len(),
                        span,
                    });
                }
                let it = self.synth_expr(pos_args[0])?;
                unify(&Ty::Int, &it, &mut self.subst, pos_args[0].span)?;
                let vt = self.synth_expr(pos_args[1])?;
                unify(&elem, &vt, &mut self.subst, pos_args[1].span)?;
                Ok(Some(Ty::Int))
            }
            "is_empty" => {
                if !pos_args.is_empty() {
                    return Err(TypeError::ArityMismatch {
                        expected: 0,
                        actual: pos_args.len(),
                        span,
                    });
                }
                Ok(Some(Ty::Bool))
            }
            other => Err(TypeError::UnknownMethod {
                type_name: "list".to_string(),
                method_name: other.to_string(),
                span,
                suggestion: list_method_suggestion(other),
            }),
        }
    }

    /// ADR-0052d-prereq §"Surface — method-table contents per type"
    /// — Float method-form table (5 methods).
    fn try_synth_float_method(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        span: Span,
    ) -> Result<Option<Ty>, TypeError> {
        let ExprKind::Attr { base, name } = &callee.kind else {
            return Ok(None);
        };
        let base_ty = self.synth_expr(base)?;
        let base_resolved = self.subst.apply(&base_ty);
        if !matches!(base_resolved, Ty::Float) {
            return Ok(None);
        }
        let pos_args: Vec<&Expr> = args
            .iter()
            .filter_map(|a| match a {
                CallArg::Positional(e) => Some(e),
                _ => None,
            })
            .collect();
        match name.as_str() {
            "floor" | "ceil" | "abs" => {
                if !pos_args.is_empty() {
                    return Err(TypeError::ArityMismatch {
                        expected: 0,
                        actual: pos_args.len(),
                        span,
                    });
                }
                Ok(Some(Ty::Float))
            }
            "is_nan" | "is_finite" => {
                if !pos_args.is_empty() {
                    return Err(TypeError::ArityMismatch {
                        expected: 0,
                        actual: pos_args.len(),
                        span,
                    });
                }
                Ok(Some(Ty::Bool))
            }
            other => Err(TypeError::UnknownMethod {
                type_name: "f64".to_string(),
                method_name: other.to_string(),
                span,
                suggestion: float_method_suggestion(other),
            }),
        }
    }

    /// ADR-0052d-prereq §"Surface — method-table contents per type"
    /// — Int method-form table (5 methods).
    fn try_synth_int_method(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        span: Span,
    ) -> Result<Option<Ty>, TypeError> {
        let ExprKind::Attr { base, name } = &callee.kind else {
            return Ok(None);
        };
        let base_ty = self.synth_expr(base)?;
        let base_resolved = self.subst.apply(&base_ty);
        if !matches!(base_resolved, Ty::Int) {
            return Ok(None);
        }
        let pos_args: Vec<&Expr> = args
            .iter()
            .filter_map(|a| match a {
                CallArg::Positional(e) => Some(e),
                _ => None,
            })
            .collect();
        match name.as_str() {
            "abs" | "bit_count" => {
                if !pos_args.is_empty() {
                    return Err(TypeError::ArityMismatch {
                        expected: 0,
                        actual: pos_args.len(),
                        span,
                    });
                }
                Ok(Some(Ty::Int))
            }
            "pow" | "min" | "max" => {
                if pos_args.len() != 1 {
                    return Err(TypeError::ArityMismatch {
                        expected: 1,
                        actual: pos_args.len(),
                        span,
                    });
                }
                let at = self.synth_expr(pos_args[0])?;
                unify(&Ty::Int, &at, &mut self.subst, pos_args[0].span)?;
                Ok(Some(Ty::Int))
            }
            other => Err(TypeError::UnknownMethod {
                type_name: "i64".to_string(),
                method_name: other.to_string(),
                span,
                suggestion: int_method_suggestion(other),
            }),
        }
    }

    /// ADR-0052d-prereq §"Decision" — chain dispatcher.
    ///
    /// Tries each per-type method table in order: Dict → Str → List →
    /// Float → Int. The order is fixed (dict-first for diffability with
    /// M12.x) but irrelevant for correctness (each table guards on its
    /// own receiver type).
    ///
    /// Returns:
    /// - `Ok(Some(ret))` when one table matches the (receiver, method).
    /// - `Err(_)` propagated from a table (UnknownMethod for typo on a
    ///   recognised receiver; ArityMismatch / TypeMismatch from arg
    ///   validation).
    /// - `Ok(None)` when the receiver type is none of the 5 recognised
    ///   types (e.g. `Ty::Adt`, `Ty::Var`, etc.) so the caller can fall
    ///   through to the generic `Attr`-fresh-var path.
    fn try_synth_method_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        span: Span,
    ) -> Result<Option<Ty>, TypeError> {
        if let Some(t) = self.try_synth_dict_method(callee, args, span)? {
            return Ok(Some(t));
        }
        if let Some(t) = self.try_synth_str_method(callee, args, span)? {
            return Ok(Some(t));
        }
        if let Some(t) = self.try_synth_list_method(callee, args, span)? {
            return Ok(Some(t));
        }
        if let Some(t) = self.try_synth_float_method(callee, args, span)? {
            return Ok(Some(t));
        }
        if let Some(t) = self.try_synth_int_method(callee, args, span)? {
            return Ok(Some(t));
        }
        Ok(None)
    }

    fn synth_call(&mut self, callee: &Expr, args: &[CallArg], span: Span) -> Result<Ty, TypeError> {
        // ADR-0052d-prereq §"Decision" — method-form dispatch via per-
        // type method tables (Dict / Str / List / Float / Int). Each
        // table guards on its receiver type; the chain returns
        // `Some(ret)` on first match, propagates `UnknownMethod` /
        // `ArityMismatch` / `TypeMismatch` errors, or falls through
        // when the receiver type is not in the recognised set.
        if let Some(t) = self.try_synth_method_call(callee, args, span)? {
            return Ok(t);
        }
        let callee_ty = self.synth_expr(callee)?;
        let callee_ty = self.subst.apply(&callee_ty);
        // ADR-0050c §F5 / Phase 6 — row-polymorphic widening. When the
        // callee resolves to a PRELUDE intrinsic whose `list[T]` rows
        // should accept any element type, freshly-instantiate the fn
        // signature: walk every `Ty::List(elem)` inside the signature
        // and replace `elem` with a fresh `Ty::Var` so this call site
        // can unify it with `Ty::Str` / `Ty::Int` / etc. without
        // polluting other call sites' unifications.
        let callee_ty = if let ExprKind::Name(rn) = &callee.kind {
            if self.poly_intrinsic_defs.contains(&rn.def_id) {
                self.instantiate_list_polymorphic(&callee_ty)
            } else {
                callee_ty
            }
        } else {
            callee_ty
        };
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
                    self.unify_call_arg(p, &at, a.span)?;
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
                        self.unify_call_arg(&p, &et, e.span)?;
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

    /// ADR-0052a Wave-1 §3 + §6 — one-way call-site coercion.
    ///
    /// Unify `formal` against `actual` at a function-call argument-
    /// binding position. The Wave-1 transparency rule allows PRELUDE
    /// Str helpers (and any user fn taking `s: Str`) to accept `&s`:
    /// when the formal parameter is a concrete non-`Ref` type `T` and
    /// the actual argument resolves to `Ref(T_inner)`, drop the `Ref`
    /// wrapper locally and unify `formal` against `T_inner`.
    ///
    /// **Critical**: this coercion is (a) **scoped to call-arg
    /// binding only** — `let n: i64 = &s`, `(&n) + (&s)`, and `if &s:`
    /// all go through other unify paths and continue to reject; (b)
    /// **unidirectional** — `Ref(T) → T`, never `T → Ref(T)`; (c)
    /// **local** — does NOT extend the substitution table with a
    /// `Ref` interconversion entry (the v1+v2 cascade root per §13
    /// "Design lesson 2026-05-17"). The substitution side-effects of
    /// the inner `unify(formal, &inner_ty, ...)` call are the same as
    /// they would be if the user had written the unwrapped form
    /// directly.
    ///
    /// Coercion fires iff (after substitution application):
    /// - actual is `Ty::Ref(inner)`
    /// - formal is NOT `Ty::Ref(_)` (no Ref↔Ref shape change; the
    ///   structural `(Ref(a), Ref(b))` arm in `infer::unify` handles
    ///   that case directly).
    /// - formal is NOT `Ty::Var(_)` (let inference bind `?0 :=
    ///   Ref(T)` if the formal is genuinely under-determined).
    ///
    /// All other shapes fall through to plain `unify`, preserving the
    /// existing behaviour for non-borrow arguments and the i0052a_*
    /// rejection corpus (TypeMismatch where the inner types don't
    /// unify, e.g. `takes_int(&s)` with `s: Str`).
    fn unify_call_arg(&mut self, formal: &Ty, actual: &Ty, span: Span) -> Result<(), TypeError> {
        let formal_resolved = self.subst.apply(formal);
        let actual_resolved = self.subst.apply(actual);
        if let Ty::Ref(inner) = &actual_resolved {
            let formal_is_ref = matches!(formal_resolved, Ty::Ref(_));
            let formal_is_var = matches!(formal_resolved, Ty::Var(_));
            if !formal_is_ref && !formal_is_var {
                // One-way Ref(T) → T coercion at the call-arg boundary.
                return unify(formal, inner, &mut self.subst, span);
            }
        }
        unify(formal, actual, &mut self.subst, span)
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
                // ADR-0050d Decision 7A — dict comp K must be hashable.
                let kt_resolved = self.subst.apply(&kt);
                if !kt_resolved.is_hashable() {
                    return Err(TypeError::NotHashable {
                        actual: kt_resolved,
                        span: k.span,
                    });
                }
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

    /// ADR-0050d §"Type-checker amendments" item 1 — validate that
    /// every `Dict[K, V]` annotation inside `t` has a hashable K.
    /// Walks the HIR type tree (not the lowered `Ty`) so that spans
    /// are preserved on each sub-position; emits `NotHashable` with
    /// the actual non-hashable K type if any dict annotation rejects.
    ///
    /// Called at every annotation-lowering site: `Let`, `fn` param /
    /// return, `class` field, `TypeAlias` body. The literal-lit
    /// rejection at `synth_dict_lit` covers the value-position case
    /// where the user writes `{1.0: 1}` without an annotation.
    fn validate_hashable_dict(&self, t: &HirType) -> Result<(), TypeError> {
        match &t.kind {
            TypeKind::Name(_) => Ok(()),
            TypeKind::Generic { base, args } => {
                let base_s = base.join(".");
                // `Dict[K, V]` / `dict[K, V]` is the only generic that
                // requires K-hashability; per Decision 7A this is
                // Phase F.3's only Hash dispatch site.
                if matches!(base_s.as_str(), "Dict" | "dict") && args.len() == 2 {
                    let k_ty = self.lower_type(&args[0]);
                    let k_resolved = self.subst.apply(&k_ty);
                    if !k_resolved.is_hashable() {
                        return Err(TypeError::NotHashable {
                            actual: k_resolved,
                            span: args[0].span,
                        });
                    }
                }
                for a in args {
                    self.validate_hashable_dict(a)?;
                }
                Ok(())
            }
            TypeKind::Union(items) | TypeKind::Tuple(items) => {
                for it in items {
                    self.validate_hashable_dict(it)?;
                }
                Ok(())
            }
            TypeKind::Fn {
                params,
                return_type,
            } => {
                for p in params {
                    self.validate_hashable_dict(p)?;
                }
                self.validate_hashable_dict(return_type)
            }
        }
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

    /// ADR-0050c §F5 / Phase 6 + ADR-0050d Decision 5 addendum —
    /// row-polymorphic widening helper.
    ///
    /// Walk a type and replace every collection-type at the top level
    /// with fresh `Ty::Var` element types so each call to a
    /// collection-polymorphic intrinsic gets its own elem vars. The
    /// pre-Wave-3 incarnation widened only `Ty::List(elem)`; the
    /// Wave-3 amendment widens `Ty::Dict(K, V)` at the top level too,
    /// so `dict_is_empty(d: Dict[i64, i64])` accepts a call with
    /// `d: Dict[str, str]` etc. (Decision 5 addendum row-polymorphic
    /// dispatch).
    ///
    /// Recurses into Tuple / Set / Dict / Fn / Record / Adt / Alias
    /// so that nested collection types are instantiated too (e.g.
    /// `fn f(xs: list[list[T]]) -> ...`).
    fn instantiate_list_polymorphic(&self, ty: &Ty) -> Ty {
        match ty {
            Ty::List(_) => Ty::List(Box::new(self.fresh_var())),
            Ty::Tuple(items) => Ty::Tuple(
                items
                    .iter()
                    .map(|t| self.instantiate_list_polymorphic(t))
                    .collect(),
            ),
            Ty::Set(elem) => Ty::Set(Box::new(self.instantiate_list_polymorphic(elem))),
            // ADR-0050d Decision 5 addendum — top-level Dict widens to
            // fresh K + fresh V so `dict_is_empty(d: Dict[i64,i64])`
            // unifies with any `Dict[K,V]` at the callsite.
            Ty::Dict(_, _) => Ty::Dict(Box::new(self.fresh_var()), Box::new(self.fresh_var())),
            Ty::Fn(fn_ty) => Ty::Fn(FnTy {
                positional: fn_ty
                    .positional
                    .iter()
                    .map(|t| self.instantiate_list_polymorphic(t))
                    .collect(),
                named: fn_ty
                    .named
                    .iter()
                    .map(|(n, t)| (n.clone(), self.instantiate_list_polymorphic(t)))
                    .collect(),
                var_positional: fn_ty
                    .var_positional
                    .as_ref()
                    .map(|t| Box::new(self.instantiate_list_polymorphic(t))),
                var_keyword: fn_ty
                    .var_keyword
                    .as_ref()
                    .map(|t| Box::new(self.instantiate_list_polymorphic(t))),
                return_ty: Box::new(self.instantiate_list_polymorphic(&fn_ty.return_ty)),
            }),
            _ => ty.clone(),
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

/// ADR-0052d-prereq §"New error variant" — Str method-name suggestion
/// helper. Returns a hard-coded `&'static str` hint per Wave-2 stub
/// shape (ADR-0052b Direction B promotes it to a structured-suggestion
/// record post-Wave-2). When the typo is close to a known method,
/// return a "did you mean" hint; otherwise list the canonical surface
/// from ADR-0052d-prereq §4 Str row.
fn str_method_suggestion(typo: &str) -> Option<&'static str> {
    if typo.starts_with("split") || typo.contains("split") {
        Some("did you mean 'split'?")
    } else if typo.starts_with("len") || typo.contains("len") {
        Some("did you mean 'len'?")
    } else if typo.contains("trim") {
        Some("did you mean 'trim'?")
    } else if typo.contains("find") {
        Some("did you mean 'find'?")
    } else if typo.contains("replace") {
        Some("did you mean 'replace'?")
    } else if typo.contains("contain") {
        Some("did you mean 'contains'?")
    } else if typo.contains("start") {
        Some("did you mean 'starts_with'?")
    } else if typo.contains("end") {
        Some("did you mean 'ends_with'?")
    } else if typo.contains("low") {
        Some("did you mean 'lower'?")
    } else if typo.contains("up") {
        Some("did you mean 'upper'?")
    } else {
        Some(
            "str methods: len, split, replace, trim, find, contains, starts_with, ends_with, lower, upper",
        )
    }
}

/// ADR-0052d-prereq §"New error variant" — List method-name suggestion.
fn list_method_suggestion(typo: &str) -> Option<&'static str> {
    if typo.contains("len") {
        Some("did you mean 'len'?")
    } else if typo.contains("push") {
        Some("did you mean 'push'?")
    } else if typo.contains("get") {
        Some("did you mean 'get'?")
    } else if typo.contains("set") {
        Some("did you mean 'set'?")
    } else if typo.contains("empty") {
        Some("did you mean 'is_empty'?")
    } else {
        Some("list methods: len, push, get, set, is_empty")
    }
}

/// ADR-0052d-prereq §"New error variant" — Float method-name suggestion.
fn float_method_suggestion(typo: &str) -> Option<&'static str> {
    if typo.contains("floor") || typo.starts_with("flr") || typo.starts_with("flo") {
        Some("did you mean 'floor'?")
    } else if typo.contains("ceil") {
        Some("did you mean 'ceil'?")
    } else if typo.contains("nan") {
        Some("did you mean 'is_nan'?")
    } else if typo.contains("finite") {
        Some("did you mean 'is_finite'?")
    } else if typo.contains("abs") {
        Some("did you mean 'abs'?")
    } else {
        Some("f64 methods: floor, ceil, is_nan, is_finite, abs")
    }
}

/// ADR-0052d-prereq §"New error variant" — Int method-name suggestion.
fn int_method_suggestion(typo: &str) -> Option<&'static str> {
    if typo.contains("abs") {
        Some("did you mean 'abs'?")
    } else if typo.contains("pow") {
        Some("did you mean 'pow'?")
    } else if typo.contains("min") {
        Some("did you mean 'min'?")
    } else if typo.contains("max") {
        Some("did you mean 'max'?")
    } else if typo.contains("bit") || typo.contains("count") {
        Some("did you mean 'bit_count'?")
    } else {
        Some("i64 methods: abs, pow, min, max, bit_count")
    }
}

/// ADR-0050c §F5 / Phase 6 — row-polymorphic widening name list.
///
/// PRELUDE intrinsics declared with `list[i64]` parameters that
/// SHOULD accept `list[T]` for any `T`. The type checker tracks each
/// matching fn's `DefId` during `prebind_item` and re-instantiates a
/// fresh `Ty::List(Ty::Var(_))` per call site in `synth_call`.
///
/// This matches the existing CLI intrinsic-rewrite pass at
/// `crates/cobrust-cli/src/build/intrinsics.rs` which already routes
/// these names to their C-ABI runtime symbols (`__cobrust_list_len`,
/// etc.), and the symbols themselves take a `*mut u8` list pointer
/// (no element-type-specific path at the ABI level).
///
/// Synchronisation: this list must stay aligned with the PRELUDE
/// definitions at `crates/cobrust-cli/src/build.rs::PRELUDE`. When
/// PRELUDE adds a new `list[i64]`-typed intrinsic that should be
/// row-polymorphic, add the name here.
fn is_list_polymorphic_intrinsic_name(name: &str) -> bool {
    matches!(
        name,
        "list_len"
            | "list_get"
            | "list_set"
            | "list_new"
            | "list_is_empty"
            // ADR-0050d Decision 5 addendum — `dict_is_empty(d) -> bool`
            // accepts any `Dict[K, V]` at the callsite (widening
            // delegates to `instantiate_list_polymorphic` which widens
            // Dict to `Dict[?, ?]`).
            | "dict_is_empty"
            // ADR-0050d Decision 5 — `len(d)` / `len(xs)` polymorphic
            // builtin. Intrinsic-rewrite at the CLI tier picks the
            // right runtime symbol per arg shape (Dict / List). The
            // PRELUDE stub declares `len: dict[i64,i64] -> i64`; the
            // widening here allows any (K, V) shape AND any List elem.
            | "len"
    )
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
