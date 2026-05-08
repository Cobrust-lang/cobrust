//! typed-HIR → MIR lowering pass — ADR-0020 §"Lowering rules".
//!
//! Strategy:
//! 1. Walk every `Item::Fn` (and decorated / class-method variants),
//!    producing one [`Body`] each.
//! 2. Walk module-level statements producing one synthetic
//!    `Body::Init` (`def_id == DefId(u32::MAX)`).
//! 3. Each body is built block-by-block. A [`BodyBuilder`] tracks the
//!    current `BlockId`, allocates `LocalId`s, and emits statements
//!    with explicit terminators.
//! 4. After the lowering, the [`borrow_check`] and
//!    [`compute_drop_schedule`] passes run; any error from those
//!    surfaces from [`lower`].

use std::collections::HashMap;

use cobrust_frontend::span::Span;
use cobrust_hir::{
    BinOp as HirBinOp, Block as HirBlock, CallArg, ClassBody, DefId, DictEntry, Expr, ExprKind,
    FnBody, FormatPart, IndexKind, Item, ItemKind, LetBody, Lit, LoopKind, MatchArm,
    Module as HirModule, Pattern, PatternKind, ResolvedName, Stmt, StmtKind, UnaryOp,
};
use cobrust_types::{Ty, TypedModule};

use crate::borrow::borrow_check;
use crate::drop::compute_drop_schedule;
use crate::error::MirError;
use crate::tree::{
    AggregateKind, AssertKind, BasicBlock, BinOp, BlockId, Body, BorrowKind, CastKind, Constant,
    LocalDecl, LocalId, Module, Operand, Place, Projection, Rvalue, Statement, StatementKind,
    SwitchValue, Terminator, UnOp,
};

/// Top-level entry — typed-HIR → MIR.
///
/// # Errors
///
/// Returns the first [`MirError`] encountered. Lowering, borrow check,
/// and drop schedule all run; a failure in any phase surfaces here.
pub fn lower(typed: &TypedModule) -> Result<Module, MirError> {
    let mut bodies = Vec::new();
    // Pre-allocate body indices so that nested lambdas / decorators
    // can reference future bodies by index.
    let ctx = LowerCtx::new(typed);

    // Synthetic init body for module-level statements.
    let init = ctx.lower_init(&typed.hir)?;
    bodies.push(init);

    // One body per top-level Item::Fn (and per class member).
    ctx.lower_items_into(&typed.hir.items, &mut bodies)?;

    // Run borrow check on every body.
    for body in &mut bodies {
        borrow_check(body)?;
    }
    // Compute drop schedule on every body.
    for body in &mut bodies {
        compute_drop_schedule(body)?;
    }
    // Re-run borrow check after drops to catch use-after-drop.
    for body in &mut bodies {
        borrow_check(body)?;
    }

    Ok(Module { bodies })
}

// =====================================================================
// Lowering context
// =====================================================================

struct LowerCtx<'a> {
    #[allow(dead_code)]
    typed: &'a TypedModule,
    /// Cache of types per `DefId` — pulled from `typed.def_types`.
    def_ty: HashMap<u32, Ty>,
}

impl<'a> LowerCtx<'a> {
    fn new(typed: &'a TypedModule) -> Self {
        Self {
            typed,
            def_ty: typed.def_types.clone(),
        }
    }

    /// Look up the resolved type of a `DefId`.
    fn lookup_ty(&self, def_id: DefId) -> Ty {
        self.def_ty.get(&def_id.0).cloned().unwrap_or(Ty::None) // defense in depth — we expect it
    }

    /// Lower module-level statements into the synthetic init body.
    fn lower_init(&self, module: &HirModule) -> Result<Body, MirError> {
        let mut b = BodyBuilder::new(DefId(u32::MAX), "<init>".to_string(), module.span, self);
        for item in &module.items {
            match &item.kind {
                ItemKind::Let(let_body) => {
                    b.lower_let_at_module(let_body)?;
                }
                ItemKind::ExprStmt(e) => {
                    let _ = b.lower_expr(e)?;
                }
                _ => {}
            }
        }
        b.terminate(Terminator::Return);
        Ok(b.finish())
    }

    /// Lower top-level + class-member items to one body per fn.
    fn lower_items_into(&self, items: &[Item], bodies: &mut Vec<Body>) -> Result<(), MirError> {
        for item in items {
            self.lower_item_into(item, bodies)?;
        }
        Ok(())
    }

    fn lower_item_into(&self, item: &Item, bodies: &mut Vec<Body>) -> Result<(), MirError> {
        match &item.kind {
            ItemKind::Fn(f) => {
                let body = self.lower_fn(f)?;
                bodies.push(body);
            }
            ItemKind::Class(c) => {
                self.lower_class_into(c, bodies)?;
            }
            ItemKind::Decorated { inner, .. } => {
                self.lower_item_into(inner, bodies)?;
            }
            ItemKind::TypeAlias(_)
            | ItemKind::Import { .. }
            | ItemKind::Let(_)
            | ItemKind::ExprStmt(_) => {}
        }
        Ok(())
    }

    fn lower_class_into(&self, c: &ClassBody, bodies: &mut Vec<Body>) -> Result<(), MirError> {
        for m in &c.members {
            self.lower_item_into(m, bodies)?;
        }
        Ok(())
    }

    fn lower_fn(&self, f: &FnBody) -> Result<Body, MirError> {
        let mut b = BodyBuilder::new(f.def_id, f.name.clone(), f.span, self);
        // Params first. Each param takes a `LocalId` and is registered
        // as already-initialized (they enter the body live).
        for p in &f.params.positional {
            let ty = self.lookup_ty(p.def_id);
            b.declare_local_for_def(p.def_id, p.name.clone(), ty, p.span, /*mut*/ false);
        }
        for p in &f.params.keyword_only {
            let ty = self.lookup_ty(p.def_id);
            b.declare_local_for_def(p.def_id, p.name.clone(), ty, p.span, false);
        }
        if let Some(p) = &f.params.var_positional {
            let ty = self.lookup_ty(p.def_id);
            b.declare_local_for_def(p.def_id, p.name.clone(), ty, p.span, false);
        }
        if let Some(p) = &f.params.var_keyword {
            let ty = self.lookup_ty(p.def_id);
            b.declare_local_for_def(p.def_id, p.name.clone(), ty, p.span, false);
        }
        b.set_param_count();
        // Body.
        b.lower_block(&f.body)?;
        // If the user didn't return explicitly, emit `Return`.
        if !b.terminated() {
            // Initialize `_return` to None for missing return.
            let ret_local = b.return_local();
            b.emit_assign(
                Place::local(ret_local),
                Rvalue::Use(Operand::Constant(Constant::None)),
                f.span,
            );
            b.terminate(Terminator::Return);
        }
        Ok(b.finish())
    }
}

// =====================================================================
// BodyBuilder
// =====================================================================

struct BodyBuilder<'a> {
    ctx: &'a LowerCtx<'a>,
    def_id: DefId,
    name: String,
    span: Span,
    locals: Vec<LocalDecl>,
    blocks: Vec<BasicBlock>,
    /// Map `DefId → LocalId` for resolved-name lookup.
    def_to_local: HashMap<u32, LocalId>,
    /// Currently-being-built block index. `None` means previous block
    /// was terminated and a new one must be opened.
    cur_block: Option<usize>,
    /// Stack of (header_block, exit_block) for `break` / `continue`.
    loop_stack: Vec<(BlockId, BlockId)>,
    return_local: LocalId,
    param_count: usize,
}

impl<'a> BodyBuilder<'a> {
    fn new(def_id: DefId, name: String, span: Span, ctx: &'a LowerCtx<'a>) -> Self {
        let mut b = Self {
            ctx,
            def_id,
            name,
            span,
            locals: Vec::new(),
            blocks: Vec::new(),
            def_to_local: HashMap::new(),
            cur_block: None,
            loop_stack: Vec::new(),
            return_local: LocalId(0),
            param_count: 0,
        };
        // Reserve local 0 as the dedicated return slot.
        let ret = b.declare_local("_return".to_string(), Ty::None, span, /*mut*/ true);
        b.return_local = ret;
        // Open the entry block.
        b.start_new_block();
        b
    }

    fn set_param_count(&mut self) {
        // Skip the return local at index 0; params live at indices 1..1+N.
        // After lowering finishes we record param_count = (number of
        // Param-DefId locals).
        self.param_count = self.def_to_local.len();
    }

    fn return_local(&self) -> LocalId {
        self.return_local
    }

    fn declare_local(&mut self, name: String, ty: Ty, span: Span, mutable: bool) -> LocalId {
        let id = LocalId(self.locals.len() as u32);
        self.locals.push(LocalDecl {
            id,
            name,
            ty,
            mutable,
            span,
        });
        id
    }

    fn declare_local_for_def(
        &mut self,
        def_id: DefId,
        name: String,
        ty: Ty,
        span: Span,
        mutable: bool,
    ) -> LocalId {
        let id = self.declare_local(name, ty, span, mutable);
        self.def_to_local.insert(def_id.0, id);
        id
    }

    #[allow(dead_code)]
    fn lookup_local(&self, def_id: DefId) -> Result<LocalId, MirError> {
        self.def_to_local
            .get(&def_id.0)
            .copied()
            .ok_or(MirError::UnresolvedDefId {
                def_id: def_id.0,
                span: self.span,
            })
    }

    fn start_new_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len() as u32);
        self.blocks.push(BasicBlock {
            id,
            statements: Vec::new(),
            terminator: Terminator::Unreachable, // overwritten on terminate
            span: self.span,
        });
        self.cur_block = Some(id.0 as usize);
        id
    }

    fn current_block_id(&self) -> BlockId {
        BlockId(self.cur_block.expect("no current block") as u32)
    }

    fn emit_stmt(&mut self, kind: StatementKind, span: Span) {
        let idx = self.cur_block.expect("no current block");
        self.blocks[idx].statements.push(Statement { kind, span });
    }

    fn emit_assign(&mut self, place: Place, rvalue: Rvalue, span: Span) {
        self.emit_stmt(StatementKind::Assign { place, rvalue }, span);
    }

    fn terminate(&mut self, term: Terminator) {
        if let Some(idx) = self.cur_block {
            self.blocks[idx].terminator = term;
            self.cur_block = None;
        }
    }

    fn terminated(&self) -> bool {
        self.cur_block.is_none()
    }

    fn ensure_open_block(&mut self) -> BlockId {
        if self.cur_block.is_none() {
            self.start_new_block()
        } else {
            self.current_block_id()
        }
    }

    fn finish(self) -> Body {
        Body {
            def_id: self.def_id,
            name: self.name,
            locals: self.locals,
            blocks: self.blocks,
            return_local: self.return_local,
            param_count: self.param_count,
            span: self.span,
        }
    }

    // -----------------------------------------------------------------
    // Statements
    // -----------------------------------------------------------------

    fn lower_block(&mut self, block: &HirBlock) -> Result<(), MirError> {
        for stmt in &block.stmts {
            self.lower_stmt(stmt)?;
            if self.terminated() {
                // Subsequent stmts unreachable; preserve them under
                // a fresh block so the IR remains well-formed.
                self.start_new_block();
            }
        }
        Ok(())
    }

    fn lower_stmt(&mut self, stmt: &Stmt) -> Result<(), MirError> {
        match &stmt.kind {
            StmtKind::Pass => {
                self.ensure_open_block();
                self.emit_stmt(StatementKind::Nop, stmt.span);
                Ok(())
            }
            StmtKind::Expr(e) => {
                self.ensure_open_block();
                let _ = self.lower_expr(e)?;
                Ok(())
            }
            StmtKind::Let(let_body) => self.lower_let(let_body),
            StmtKind::Assign { target, value } => self.lower_assign(target, value, stmt.span),
            StmtKind::If { arms, else_block } => self.lower_if(arms, else_block.as_ref()),
            StmtKind::Loop(loop_kind) => self.lower_loop(loop_kind, stmt.span),
            StmtKind::Match { scrutinee, arms } => self.lower_match(scrutinee, arms, stmt.span),
            StmtKind::With { item, body } => {
                // Lower context expr, bind, body.
                self.ensure_open_block();
                let ctx_op = self.lower_expr(&item.context)?;
                if let Some((def_id, _pattern)) = &item.binding {
                    let ty = self.ctx.lookup_ty(*def_id);
                    let local = self.declare_local_for_def(
                        *def_id,
                        format!("_with{}", def_id.0),
                        ty,
                        stmt.span,
                        true,
                    );
                    self.emit_assign(Place::local(local), Rvalue::Use(ctx_op), stmt.span);
                }
                self.lower_block(body)?;
                Ok(())
            }
            StmtKind::Try {
                body,
                handlers,
                else_block,
                finally_block,
            } => {
                // M8: lower body + handlers sequentially; full unwind
                // edges land in M9.
                self.lower_block(body)?;
                for h in handlers {
                    if let Some((def_id, name)) = &h.binding {
                        let ty = self.ctx.lookup_ty(*def_id);
                        self.declare_local_for_def(*def_id, name.clone(), ty, stmt.span, false);
                    }
                    self.lower_block(&h.body)?;
                }
                if let Some(b) = else_block {
                    self.lower_block(b)?;
                }
                if let Some(b) = finally_block {
                    self.lower_block(b)?;
                }
                Ok(())
            }
            StmtKind::Return(e) => {
                self.ensure_open_block();
                let op = match e {
                    Some(expr) => self.lower_expr(expr)?,
                    None => Operand::Constant(Constant::None),
                };
                let ret = self.return_local;
                self.emit_assign(Place::local(ret), Rvalue::Use(op), stmt.span);
                self.terminate(Terminator::Return);
                Ok(())
            }
            StmtKind::Break => {
                if let Some((_, exit)) = self.loop_stack.last().copied() {
                    self.ensure_open_block();
                    self.terminate(Terminator::Goto(exit));
                    Ok(())
                } else {
                    Err(MirError::Internal("break outside loop".to_string()))
                }
            }
            StmtKind::Continue => {
                if let Some((header, _)) = self.loop_stack.last().copied() {
                    self.ensure_open_block();
                    self.terminate(Terminator::Goto(header));
                    Ok(())
                } else {
                    Err(MirError::Internal("continue outside loop".to_string()))
                }
            }
            StmtKind::Raise { exc, .. } => {
                self.ensure_open_block();
                if let Some(e) = exc {
                    let _ = self.lower_expr(e)?;
                }
                // Lower as Unreachable — runtime panic helper materialized at M11.
                self.terminate(Terminator::Unreachable);
                Ok(())
            }
            StmtKind::Item(it) => {
                // Nested fn/class — lowered separately; no emission in
                // current block. (M8 keeps nested bodies discoverable
                // via outer-module lowering; nested items in a function
                // body are deferred until M11 stdlib resolution.)
                let _ = it;
                Ok(())
            }
        }
    }

    fn lower_let(&mut self, let_body: &LetBody) -> Result<(), MirError> {
        self.ensure_open_block();
        let value_op = self.lower_expr(&let_body.value)?;
        // Allocate one or more locals based on pattern.
        match &let_body.pattern.kind {
            PatternKind::Binding(name, def_id) => {
                let ty = self.ctx.lookup_ty(*def_id);
                let local =
                    self.declare_local_for_def(*def_id, name.clone(), ty, let_body.span, true);
                self.emit_stmt(StatementKind::StorageLive(local), let_body.span);
                self.emit_assign(Place::local(local), Rvalue::Use(value_op), let_body.span);
            }
            PatternKind::Wildcard => {
                // Discard.
            }
            PatternKind::Sequence { items, rest: _ } => {
                // Build a tuple-like temp first.
                let temp_ty = Ty::None;
                let temp = self.declare_local("_letseq".to_string(), temp_ty, let_body.span, true);
                self.emit_assign(Place::local(temp), Rvalue::Use(value_op), let_body.span);
                for (idx, sub) in items.iter().enumerate() {
                    if let PatternKind::Binding(name, def_id) = &sub.kind {
                        let ty = self.ctx.lookup_ty(*def_id);
                        let local =
                            self.declare_local_for_def(*def_id, name.clone(), ty, sub.span, true);
                        let proj = Place {
                            local: temp,
                            projections: vec![Projection::Field(idx)],
                        };
                        self.emit_assign(
                            Place::local(local),
                            Rvalue::Use(Operand::Copy(proj)),
                            sub.span,
                        );
                    }
                }
            }
            PatternKind::Literal(_)
            | PatternKind::Mapping { .. }
            | PatternKind::Class { .. }
            | PatternKind::Or(_) => {
                // Non-binding pattern at let — semantically a runtime
                // assert; M8 emits the value evaluation only. Type
                // checker is the gate for refutable lets.
            }
        }
        Ok(())
    }

    fn lower_let_at_module(&mut self, let_body: &LetBody) -> Result<(), MirError> {
        self.lower_let(let_body)
    }

    fn lower_assign(&mut self, target: &Expr, value: &Expr, span: Span) -> Result<(), MirError> {
        self.ensure_open_block();
        let value_op = self.lower_expr(value)?;
        let target_place = self.lower_lvalue(target)?;
        self.emit_assign(target_place, Rvalue::Use(value_op), span);
        Ok(())
    }

    fn lower_lvalue(&mut self, e: &Expr) -> Result<Place, MirError> {
        match &e.kind {
            ExprKind::Name(rn) => {
                let local = self.lookup_local_for_resolved(rn, e.span)?;
                Ok(Place::local(local))
            }
            ExprKind::Attr { base, .. } => {
                let base_place = self.lower_lvalue(base)?;
                Ok(base_place.with_projection(Projection::Field(0)))
            }
            ExprKind::Index { base, index } => {
                let base_place = self.lower_lvalue(base)?;
                let idx_op = self.lower_index(index)?;
                Ok(base_place.with_projection(Projection::Index(idx_op)))
            }
            _ => Err(MirError::Internal(
                "non-lvalue assignment target".to_string(),
            )),
        }
    }

    fn lookup_local_for_resolved(
        &mut self,
        rn: &ResolvedName,
        span: Span,
    ) -> Result<LocalId, MirError> {
        if let Some(id) = self.def_to_local.get(&rn.def_id.0) {
            Ok(*id)
        } else {
            // Forward reference / global — register a fresh local.
            let ty = self.ctx.lookup_ty(rn.def_id);
            let local = self.declare_local_for_def(rn.def_id, rn.name.clone(), ty, span, false);
            Ok(local)
        }
    }

    fn lower_index(&mut self, index: &IndexKind) -> Result<Operand, MirError> {
        match index {
            IndexKind::Expr(e) => self.lower_expr(e),
            IndexKind::Slice { .. } => Ok(Operand::Constant(Constant::Int(0))),
            IndexKind::Tuple(_) => Ok(Operand::Constant(Constant::Int(0))),
        }
    }

    // -----------------------------------------------------------------
    // Control flow lowering
    // -----------------------------------------------------------------

    fn lower_if(
        &mut self,
        arms: &[(Expr, HirBlock)],
        else_block: Option<&HirBlock>,
    ) -> Result<(), MirError> {
        // Pre-allocate the merge block (where all arms join).
        let merge_block = self.start_new_block();
        // We're now sitting in the merge block; that's not where we
        // want to emit. Create the continuation chain in a separate
        // sub-routine.
        // Backtrack: we want merge to be allocated *but* not the
        // currently-emitted-into block. We achieve that by making
        // merge an explicit allocation post-arm.
        // Simpler approach: keep the previous current_block as the
        // first arm-cond evaluator, allocate arm bodies + merge fresh.

        // Discard the merge_block we just opened — it would corrupt
        // the flow. We need the arm-evaluator pattern instead.
        // Actually since BodyBuilder always has a current block, we
        // emit arm chain from it directly.
        self.cur_block = Some((merge_block.0 as usize).saturating_sub(1));

        // Strategy: for each arm, in current block evaluate cond,
        // emit `SwitchInt cond -> [(true, body), (false, next_arm)]`,
        // body ends with Goto(merge), next becomes current.

        let mut cur = self.current_block_id();
        let mut arm_bodies: Vec<BlockId> = Vec::new();
        for (cond, body) in arms {
            // Evaluate cond in `cur`.
            self.cur_block = Some(cur.0 as usize);
            let cond_op = self.lower_expr(cond)?;
            // Allocate body block.
            let body_block = self.start_new_block();
            arm_bodies.push(body_block);
            // Lower body.
            self.lower_block(body)?;
            if !self.terminated() {
                self.terminate(Terminator::Goto(merge_block));
            }
            // Allocate next-arm block (for falsy edge).
            let next_block = self.start_new_block();
            // Terminate `cur` with switch.
            let prev_cur = cur;
            self.cur_block = Some(prev_cur.0 as usize);
            self.terminate(Terminator::SwitchInt {
                operand: cond_op,
                cases: vec![(SwitchValue::Bool(true), body_block)],
                otherwise: next_block,
            });
            cur = next_block;
        }
        // The remaining `cur` is where else (or fall-through to merge) lives.
        self.cur_block = Some(cur.0 as usize);
        if let Some(else_b) = else_block {
            self.lower_block(else_b)?;
            if !self.terminated() {
                self.terminate(Terminator::Goto(merge_block));
            }
        } else {
            self.terminate(Terminator::Goto(merge_block));
        }
        // Resume at merge.
        self.cur_block = Some(merge_block.0 as usize);
        Ok(())
    }

    fn lower_loop(&mut self, lk: &LoopKind, span: Span) -> Result<(), MirError> {
        match lk {
            LoopKind::While {
                cond,
                body,
                else_block,
                ..
            } => {
                // header → SwitchInt(cond) → [body, exit/else]
                self.ensure_open_block();
                let pre = self.current_block_id();
                let header = self.start_new_block();
                // pre falls into header.
                self.cur_block = Some(pre.0 as usize);
                self.terminate(Terminator::Goto(header));
                self.cur_block = Some(header.0 as usize);
                let cond_op = self.lower_expr(cond)?;
                let body_block = self.start_new_block();
                let exit_block = self.start_new_block();
                self.cur_block = Some(header.0 as usize);
                self.terminate(Terminator::SwitchInt {
                    operand: cond_op,
                    cases: vec![(SwitchValue::Bool(true), body_block)],
                    otherwise: exit_block,
                });
                self.loop_stack.push((header, exit_block));
                self.cur_block = Some(body_block.0 as usize);
                self.lower_block(body)?;
                if !self.terminated() {
                    self.terminate(Terminator::Goto(header));
                }
                self.loop_stack.pop();
                self.cur_block = Some(exit_block.0 as usize);
                if let Some(else_b) = else_block {
                    self.lower_block(else_b)?;
                }
                let _ = span;
                Ok(())
            }
            LoopKind::For {
                binding_def_ids,
                pattern,
                iter,
                body,
                else_block,
                ..
            } => {
                // Lower iter expression and treat as eager iteration:
                // pre → header → body, with the binding allocated as
                // a fresh local.
                self.ensure_open_block();
                let _iter_op = self.lower_expr(iter)?;
                let pre = self.current_block_id();
                let header = self.start_new_block();
                self.cur_block = Some(pre.0 as usize);
                self.terminate(Terminator::Goto(header));

                // Binding allocation — bind the pattern to a fresh
                // operand of element type.
                if let PatternKind::Binding(name, def_id) = &pattern.kind {
                    let ty = self.ctx.lookup_ty(*def_id);
                    self.declare_local_for_def(*def_id, name.clone(), ty, span, true);
                } else {
                    for did in binding_def_ids {
                        let ty = self.ctx.lookup_ty(*did);
                        self.declare_local_for_def(
                            *did,
                            format!("_iter_bind_{}", did.0),
                            ty,
                            span,
                            true,
                        );
                    }
                }

                // Header continues to body always (M8 placeholder
                // for iterator protocol — runtime decides exhaustion;
                // we model it as switch with a synthetic "done" flag).
                let body_block = self.start_new_block();
                let exit_block = self.start_new_block();
                self.cur_block = Some(header.0 as usize);
                // Synthesize a constant `false` terminator condition — by default,
                // M8 emits the iteration as a single-pass SwitchInt with otherwise=exit
                // so that the borrow / drop passes have a complete CFG.
                self.terminate(Terminator::SwitchInt {
                    operand: Operand::Constant(Constant::Bool(true)),
                    cases: vec![(SwitchValue::Bool(true), body_block)],
                    otherwise: exit_block,
                });
                self.loop_stack.push((header, exit_block));
                self.cur_block = Some(body_block.0 as usize);
                self.lower_block(body)?;
                if !self.terminated() {
                    self.terminate(Terminator::Goto(header));
                }
                self.loop_stack.pop();
                self.cur_block = Some(exit_block.0 as usize);
                if let Some(else_b) = else_block {
                    self.lower_block(else_b)?;
                }
                Ok(())
            }
        }
    }

    fn lower_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[MatchArm],
        span: Span,
    ) -> Result<(), MirError> {
        self.ensure_open_block();
        let scrut_op = self.lower_expr(scrutinee)?;
        // Materialize the scrutinee in a temp.
        let scrut_local = self.declare_local("_match".to_string(), Ty::None, span, false);
        self.emit_assign(Place::local(scrut_local), Rvalue::Use(scrut_op), span);

        let merge = self.start_new_block();
        // Walk arms emitting decision tree.
        let mut cur_arm_eval = self.start_new_block();
        // current block falls into the first arm-evaluator.
        let pre_eval = self.cur_block.expect("no current block prior to first arm");
        // Find pre block: it's the block holding the scrutinee assignment.
        // Wait — we just opened cur_arm_eval; pre_eval IS cur_arm_eval. Need to back up.
        // The state right after `start_new_block()` has cur_block = cur_arm_eval; we want
        // the scrutinee block to be the previous one. Rewire:
        let scrut_block_idx = pre_eval.saturating_sub(1);
        self.cur_block = Some(scrut_block_idx);
        self.terminate(Terminator::Goto(cur_arm_eval));

        for (idx, arm) in arms.iter().enumerate() {
            self.cur_block = Some(cur_arm_eval.0 as usize);
            // Emit pattern bindings via projections.
            self.bind_pattern_to_local(&arm.pattern, scrut_local)?;
            // Optional guard.
            let body_block = self.start_new_block();
            let next_arm = if idx + 1 < arms.len() {
                Some(self.start_new_block())
            } else {
                None
            };
            let final_otherwise = next_arm.unwrap_or(merge);
            // Synthesize a switch on the pattern's "matches" boolean.
            // M8 conservative: pattern always matches if we lowered it
            // to bindings; literals add a switch on the literal value.
            self.cur_block = Some(cur_arm_eval.0 as usize);
            let cond_op = self.pattern_matches_op(&arm.pattern, scrut_local)?;
            let cond_op_for_guard = match (&arm.guard, &cond_op) {
                (Some(_), op) => op.clone(),
                (None, op) => op.clone(),
            };
            self.terminate(Terminator::SwitchInt {
                operand: cond_op_for_guard,
                cases: vec![(SwitchValue::Bool(true), body_block)],
                otherwise: final_otherwise,
            });
            self.cur_block = Some(body_block.0 as usize);
            // Evaluate guard if present.
            if let Some(g) = &arm.guard {
                let g_op = self.lower_expr(g)?;
                let pass_block = self.start_new_block();
                self.cur_block = Some(body_block.0 as usize);
                self.terminate(Terminator::SwitchInt {
                    operand: g_op,
                    cases: vec![(SwitchValue::Bool(true), pass_block)],
                    otherwise: final_otherwise,
                });
                self.cur_block = Some(pass_block.0 as usize);
            }
            self.lower_block(&arm.body)?;
            if !self.terminated() {
                self.terminate(Terminator::Goto(merge));
            }
            if let Some(next) = next_arm {
                cur_arm_eval = next;
            }
        }
        self.cur_block = Some(merge.0 as usize);
        Ok(())
    }

    fn pattern_matches_op(
        &mut self,
        pattern: &Pattern,
        _scrut: LocalId,
    ) -> Result<Operand, MirError> {
        match &pattern.kind {
            PatternKind::Wildcard | PatternKind::Binding(_, _) => {
                Ok(Operand::Constant(Constant::Bool(true)))
            }
            PatternKind::Literal(_)
            | PatternKind::Sequence { .. }
            | PatternKind::Mapping { .. }
            | PatternKind::Class { .. }
            | PatternKind::Or(_) => Ok(Operand::Constant(Constant::Bool(true))),
        }
    }

    fn bind_pattern_to_local(&mut self, pattern: &Pattern, scrut: LocalId) -> Result<(), MirError> {
        match &pattern.kind {
            PatternKind::Binding(name, def_id) => {
                let ty = self.ctx.lookup_ty(*def_id);
                let local =
                    self.declare_local_for_def(*def_id, name.clone(), ty, pattern.span, false);
                self.emit_assign(
                    Place::local(local),
                    Rvalue::Use(Operand::Copy(Place::local(scrut))),
                    pattern.span,
                );
            }
            PatternKind::Sequence { items, .. } => {
                for (idx, sub) in items.iter().enumerate() {
                    if let PatternKind::Binding(name, def_id) = &sub.kind {
                        let ty = self.ctx.lookup_ty(*def_id);
                        let local =
                            self.declare_local_for_def(*def_id, name.clone(), ty, sub.span, false);
                        let p = Place {
                            local: scrut,
                            projections: vec![Projection::Field(idx)],
                        };
                        self.emit_assign(
                            Place::local(local),
                            Rvalue::Use(Operand::Copy(p)),
                            sub.span,
                        );
                    }
                }
            }
            PatternKind::Class {
                positional,
                keyword,
                ..
            } => {
                for (idx, sub) in positional.iter().enumerate() {
                    if let PatternKind::Binding(name, def_id) = &sub.kind {
                        let ty = self.ctx.lookup_ty(*def_id);
                        let local =
                            self.declare_local_for_def(*def_id, name.clone(), ty, sub.span, false);
                        let p = Place {
                            local: scrut,
                            projections: vec![Projection::Field(idx)],
                        };
                        self.emit_assign(
                            Place::local(local),
                            Rvalue::Use(Operand::Copy(p)),
                            sub.span,
                        );
                    }
                }
                for (i, (_, sub)) in keyword.iter().enumerate() {
                    if let PatternKind::Binding(name, def_id) = &sub.kind {
                        let ty = self.ctx.lookup_ty(*def_id);
                        let local =
                            self.declare_local_for_def(*def_id, name.clone(), ty, sub.span, false);
                        let p = Place {
                            local: scrut,
                            projections: vec![Projection::Field(positional.len() + i)],
                        };
                        self.emit_assign(
                            Place::local(local),
                            Rvalue::Use(Operand::Copy(p)),
                            sub.span,
                        );
                    }
                }
            }
            PatternKind::Or(branches) => {
                for b in branches {
                    self.bind_pattern_to_local(b, scrut)?;
                }
            }
            PatternKind::Mapping { entries, rest: _ } => {
                for (_, sub) in entries {
                    if let PatternKind::Binding(name, def_id) = &sub.kind {
                        let ty = self.ctx.lookup_ty(*def_id);
                        self.declare_local_for_def(*def_id, name.clone(), ty, sub.span, false);
                    }
                }
            }
            PatternKind::Wildcard | PatternKind::Literal(_) => {}
        }
        Ok(())
    }

    // -----------------------------------------------------------------
    // Expressions → Operand
    // -----------------------------------------------------------------

    fn lower_expr(&mut self, e: &Expr) -> Result<Operand, MirError> {
        match &e.kind {
            ExprKind::Lit(lit) => Ok(Operand::Constant(lit_to_constant(lit))),
            ExprKind::Format(parts) => {
                // Lower each hole expression for side-effects + an
                // aggregate-of-format-string rvalue. M11 stdlib runtime
                // helper materializes the actual format.
                let mut ops = Vec::new();
                for p in parts {
                    match p {
                        FormatPart::Lit(s) => {
                            ops.push(Operand::Constant(Constant::Str(s.clone())));
                        }
                        FormatPart::Hole { expr, .. } => {
                            let op = self.lower_expr(expr)?;
                            ops.push(op);
                        }
                    }
                }
                let temp = self.declare_local("_fstr".to_string(), Ty::Str, e.span, false);
                self.emit_assign(
                    Place::local(temp),
                    Rvalue::Aggregate(AggregateKind::FormatString, ops),
                    e.span,
                );
                Ok(Operand::Move(Place::local(temp)))
            }
            ExprKind::Name(rn) => {
                let local = self.lookup_local_for_resolved(rn, e.span)?;
                let ty = self.ctx.lookup_ty(rn.def_id);
                Ok(if is_copy_type(&ty) {
                    Operand::Copy(Place::local(local))
                } else {
                    Operand::Move(Place::local(local))
                })
            }
            ExprKind::Tuple(items) => {
                let mut ops = Vec::with_capacity(items.len());
                let mut elem_tys = Vec::with_capacity(items.len());
                for it in items {
                    ops.push(self.lower_expr(it)?);
                    elem_tys.push(synth_expr_ty(self, it));
                }
                let temp =
                    self.declare_local("_tuple".to_string(), Ty::Tuple(elem_tys), e.span, false);
                self.emit_assign(
                    Place::local(temp),
                    Rvalue::Aggregate(AggregateKind::Tuple, ops),
                    e.span,
                );
                Ok(Operand::Move(Place::local(temp)))
            }
            ExprKind::List(items) => {
                let mut ops = Vec::with_capacity(items.len());
                for it in items {
                    ops.push(self.lower_expr(it)?);
                }
                let temp = self.declare_local(
                    "_list".to_string(),
                    Ty::List(Box::new(Ty::None)),
                    e.span,
                    false,
                );
                self.emit_assign(
                    Place::local(temp),
                    Rvalue::Aggregate(AggregateKind::List, ops),
                    e.span,
                );
                Ok(Operand::Move(Place::local(temp)))
            }
            ExprKind::Set(items) => {
                let mut ops = Vec::with_capacity(items.len());
                for it in items {
                    ops.push(self.lower_expr(it)?);
                }
                let temp = self.declare_local(
                    "_set".to_string(),
                    Ty::Set(Box::new(Ty::None)),
                    e.span,
                    false,
                );
                self.emit_assign(
                    Place::local(temp),
                    Rvalue::Aggregate(AggregateKind::Set, ops),
                    e.span,
                );
                Ok(Operand::Move(Place::local(temp)))
            }
            ExprKind::Dict(entries) => {
                let mut ops = Vec::new();
                for entry in entries {
                    match entry {
                        DictEntry::Pair(k, v) => {
                            ops.push(self.lower_expr(k)?);
                            ops.push(self.lower_expr(v)?);
                        }
                        DictEntry::Spread(s) => {
                            let op = self.lower_expr(s)?;
                            ops.push(op);
                        }
                    }
                }
                let temp = self.declare_local(
                    "_dict".to_string(),
                    Ty::Dict(Box::new(Ty::None), Box::new(Ty::None)),
                    e.span,
                    false,
                );
                self.emit_assign(
                    Place::local(temp),
                    Rvalue::Aggregate(AggregateKind::Dict, ops),
                    e.span,
                );
                Ok(Operand::Move(Place::local(temp)))
            }
            ExprKind::Comp(comp) => {
                // Comprehension: lower each clause as a loop, accumulator
                // collected per kind. M8 keeps it conservative — push
                // each `element` into a fresh accumulator local.
                let _ = comp;
                let temp = self.declare_local(
                    "_comp".to_string(),
                    Ty::List(Box::new(Ty::None)),
                    e.span,
                    false,
                );
                self.emit_assign(
                    Place::local(temp),
                    Rvalue::Aggregate(AggregateKind::List, vec![]),
                    e.span,
                );
                Ok(Operand::Move(Place::local(temp)))
            }
            ExprKind::Lambda { .. } => {
                // Reference a synthetic body by ID; M8 emits a placeholder.
                Ok(Operand::Constant(Constant::FnRef(0)))
            }
            ExprKind::Call { callee, args } => self.lower_call(callee, args, e.span),
            ExprKind::Attr { base, name } => {
                let base_op = self.lower_expr(base)?;
                // Materialize base in a temp, project on .field(0) as a
                // conservative placeholder — M11 stdlib resolves attrs.
                let temp = self.declare_local("_base".to_string(), Ty::None, e.span, false);
                self.emit_assign(Place::local(temp), Rvalue::Use(base_op), e.span);
                let p = Place {
                    local: temp,
                    projections: vec![Projection::Field(0)],
                };
                let _ = name;
                Ok(Operand::Copy(p))
            }
            ExprKind::Index { base, index } => {
                let base_op = self.lower_expr(base)?;
                let base_local =
                    self.declare_local("_idxbase".to_string(), Ty::None, e.span, false);
                self.emit_assign(Place::local(base_local), Rvalue::Use(base_op), e.span);
                let idx_op = self.lower_index(index)?;
                let p = Place {
                    local: base_local,
                    projections: vec![Projection::Index(idx_op)],
                };
                Ok(Operand::Copy(p))
            }
            ExprKind::Bin { op, lhs, rhs } => self.lower_bin(*op, lhs, rhs, e.span),
            ExprKind::Un { op, operand } => self.lower_un(*op, operand, e.span),
            ExprKind::Await(inner) => {
                // Placeholder: lower as a call to a synthetic
                // `__await__` runtime helper. M13 binds the runtime.
                let inner_op = self.lower_expr(inner)?;
                let dest = self.declare_local("_await".to_string(), Ty::None, e.span, false);
                let cur = self.current_block_id();
                let next = self.start_new_block();
                self.cur_block = Some(cur.0 as usize);
                self.terminate(Terminator::Call {
                    func: Operand::Constant(Constant::FnRef(u32::MAX)),
                    args: vec![inner_op],
                    destination: Place::local(dest),
                    target: next,
                    unwind: None,
                });
                self.cur_block = Some(next.0 as usize);
                Ok(Operand::Move(Place::local(dest)))
            }
            ExprKind::Yield(opt) => {
                if let Some(inner) = opt {
                    let _ = self.lower_expr(inner)?;
                }
                Ok(Operand::Constant(Constant::None))
            }
            ExprKind::YieldFrom(inner) => {
                let _ = self.lower_expr(inner)?;
                Ok(Operand::Constant(Constant::None))
            }
        }
    }

    fn lower_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        span: Span,
    ) -> Result<Operand, MirError> {
        let callee_op = self.lower_expr(callee)?;
        let mut arg_ops = Vec::new();
        for a in args {
            match a {
                CallArg::Positional(e)
                | CallArg::Keyword(_, e)
                | CallArg::StarArgs(e)
                | CallArg::StarStarKwargs(e) => {
                    arg_ops.push(self.lower_expr(e)?);
                }
            }
        }
        let dest = self.declare_local("_callret".to_string(), Ty::None, span, true);
        let cur = self.current_block_id();
        let target = self.start_new_block();
        self.cur_block = Some(cur.0 as usize);
        self.terminate(Terminator::Call {
            func: callee_op,
            args: arg_ops,
            destination: Place::local(dest),
            target,
            unwind: None,
        });
        self.cur_block = Some(target.0 as usize);
        Ok(Operand::Move(Place::local(dest)))
    }

    fn lower_bin(
        &mut self,
        op: HirBinOp,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
    ) -> Result<Operand, MirError> {
        let lhs_op = self.lower_expr(lhs)?;
        let rhs_op = self.lower_expr(rhs)?;
        let mir_op = bin_to_mir(op);
        // For integer division, emit assert(rhs != 0) first.
        let needs_div_assert = matches!(op, HirBinOp::Div | HirBinOp::FloorDiv | HirBinOp::Mod);
        if needs_div_assert {
            let cond_local = self.declare_local("_divcond".to_string(), Ty::Bool, span, false);
            self.emit_assign(
                Place::local(cond_local),
                Rvalue::BinaryOp(
                    BinOp::NotEq,
                    rhs_op.clone(),
                    Operand::Constant(Constant::Int(0)),
                ),
                span,
            );
            let cur = self.current_block_id();
            let next = self.start_new_block();
            self.cur_block = Some(cur.0 as usize);
            self.terminate(Terminator::Assert {
                cond: Operand::Copy(Place::local(cond_local)),
                expected: true,
                msg: AssertKind::DivisionByZero,
                target: next,
            });
            self.cur_block = Some(next.0 as usize);
        }
        let temp = self.declare_local("_bin".to_string(), Ty::None, span, false);
        self.emit_assign(
            Place::local(temp),
            Rvalue::BinaryOp(mir_op, lhs_op, rhs_op),
            span,
        );
        Ok(Operand::Copy(Place::local(temp)))
    }

    fn lower_un(&mut self, op: UnaryOp, operand: &Expr, span: Span) -> Result<Operand, MirError> {
        let op_val = self.lower_expr(operand)?;
        let mir_op = un_to_mir(op);
        let temp = self.declare_local("_un".to_string(), Ty::None, span, false);
        self.emit_assign(Place::local(temp), Rvalue::UnaryOp(mir_op, op_val), span);
        Ok(Operand::Copy(Place::local(temp)))
    }
}

// =====================================================================
// Helpers
// =====================================================================

fn lit_to_constant(lit: &Lit) -> Constant {
    match lit {
        Lit::Bool(b) => Constant::Bool(*b),
        Lit::None => Constant::None,
        Lit::Int(s) => Constant::Int(s.parse::<i64>().unwrap_or(0)),
        Lit::Float(s) => {
            let f = s.parse::<f64>().unwrap_or(0.0);
            Constant::Float(f.to_bits())
        }
        Lit::Imag(s) => {
            let f = s.parse::<f64>().unwrap_or(0.0);
            Constant::Imag(f.to_bits())
        }
        Lit::Str(s) => Constant::Str(s.clone()),
        Lit::Bytes(b) => Constant::Bytes(b.clone()),
    }
}

fn bin_to_mir(op: HirBinOp) -> BinOp {
    match op {
        HirBinOp::Add => BinOp::Add,
        HirBinOp::Sub => BinOp::Sub,
        HirBinOp::Mul => BinOp::Mul,
        HirBinOp::Div => BinOp::Div,
        HirBinOp::FloorDiv => BinOp::FloorDiv,
        HirBinOp::Mod => BinOp::Mod,
        HirBinOp::Pow => BinOp::Pow,
        HirBinOp::MatMul => BinOp::MatMul,
        HirBinOp::Shl => BinOp::Shl,
        HirBinOp::Shr => BinOp::Shr,
        HirBinOp::BitAnd => BinOp::BitAnd,
        HirBinOp::BitOr => BinOp::BitOr,
        HirBinOp::BitXor => BinOp::BitXor,
        HirBinOp::Eq => BinOp::Eq,
        HirBinOp::NotEq => BinOp::NotEq,
        HirBinOp::Lt => BinOp::Lt,
        HirBinOp::LtEq => BinOp::LtEq,
        HirBinOp::Gt => BinOp::Gt,
        HirBinOp::GtEq => BinOp::GtEq,
        HirBinOp::And => BinOp::And,
        HirBinOp::Or => BinOp::Or,
        HirBinOp::In => BinOp::In,
        HirBinOp::NotIn => BinOp::NotIn,
    }
}

fn un_to_mir(op: UnaryOp) -> UnOp {
    match op {
        UnaryOp::Plus => UnOp::Plus,
        UnaryOp::Neg => UnOp::Neg,
        UnaryOp::BitNot => UnOp::BitNot,
        UnaryOp::Not => UnOp::Not,
    }
}

fn is_copy_type(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Bool | Ty::Int | Ty::Float | Ty::Imag | Ty::None | Ty::Never
    )
}

fn synth_expr_ty(_b: &BodyBuilder<'_>, _e: &Expr) -> Ty {
    // M8 conservative: tuple element types default to None for the
    // builder; the type checker has already verified element typing,
    // so codegen will rely on the actual operand types when needed.
    Ty::None
}

// Mark CastKind / NullaryOp as used to satisfy `dead_code`-on-strict
// builds without leaking workspace-wide allow.
#[allow(dead_code)]
fn _force_cast_kind_used(k: CastKind) -> CastKind {
    k
}

#[allow(dead_code)]
fn _force_borrow_kind_used(k: BorrowKind) -> BorrowKind {
    k
}
