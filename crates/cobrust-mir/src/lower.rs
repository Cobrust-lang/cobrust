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
    BinOp as HirBinOp, Block as HirBlock, CallArg, ClassBody, Comp, CompClause, CompElem, CompKind,
    DefId, DictEntry, Expr, ExprKind, FnBody, FormatPart, IndexKind, Item, ItemKind, LetBody, Lit,
    LoopKind, MatchArm, Module as HirModule, Pattern, PatternKind, ResolvedName, Stmt, StmtKind,
    UnaryOp,
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
        //
        // NOTE: this `param_count` reflects USER-DECLARED parameters
        // only (the return slot at LocalId(0) is not counted). Codegen
        // at `cranelift_backend.rs:561-565` uses
        // `body.locals.iter().skip(1).take(param_count)` to slice
        // params for block-arg binding, so the value must remain the
        // USER count. The drop pass's `is_param(id) = id < param_count`
        // therefore covers `[LocalId(0)=_return, LocalId(1)=param0,
        // ..., LocalId(N-1)=param_{N-2}]` but EXCLUDES LocalId(N) — the
        // last parameter. ADR-0050c Phase 4 cascade-fix sidesteps this
        // skew by changing the drop pass's exclusion predicate directly
        // (`crates/cobrust-mir/src/drop.rs:45`) to use a +1 offset.
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
                // ADR-0050c Phase 2 cascade fix: when the returned operand is
                // `Operand::Copy(p)` of a drop-eligible local (Str / List /
                // future non-Copy types), upgrade it to `Operand::Move(p)`.
                // Rationale: the Phase 2a Copy-at-operand walk-back for List
                // (`is_copy_type` returns true for `Ty::List(_)` so that fn-arg
                // shapes like `list_set(xs, i, v)` continue to read xs without
                // consuming it) interacts badly with the drop pass:
                // the drop pass enumerates list-typed locals as drop-eligible
                // and inserts a Drop on the predecessor edge of every Return
                // block (`drop.rs:104-115`). That Drop runs BEFORE the
                // ret_block's statements; if the ret_block contains
                // `return_local = Copy(xs)`, the post-drop borrow check
                // surfaces UseAfterDrop on `xs` (`borrow.rs:219-224`).
                //
                // The fix is to mark the returned operand as a Move so the
                // drop pass's `globally_moved` set contains `xs` and the Drop
                // is not inserted on this path. This matches Rust's NRVO /
                // return-value-move semantics and is sound because the return
                // statement is the last use of the local in the function body.
                let op = upgrade_return_to_move(self, op);
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
        // ADR-0050d sub-sprint c — Dict index-assign `d[k] = v`.
        //
        // Source-level `d[k] = v` on `d: Dict[K, V]` lowers to:
        //   __cobrust_dict_set_K_V(d, k, v)
        //
        // Without this dispatch, `lower_lvalue` would emit a
        // `Place::Index` projection which is a no-op at codegen for
        // dict-shaped bases (Cranelift can't write into a hashmap
        // slot directly).
        if let ExprKind::Index { base, index } = &target.kind {
            let base_ty = synth_expr_ty(self, base);
            if let Ty::Dict(k_ty, v_ty) = &base_ty {
                let key_is_str = matches!(**k_ty, Ty::Str);
                let val_is_str = matches!(**v_ty, Ty::Str);
                let set_symbol = match (key_is_str, val_is_str) {
                    (true, true) => "__cobrust_dict_set_str_str",
                    (true, false) => "__cobrust_dict_set_str_i64",
                    (false, true) => "__cobrust_dict_set_i64_str",
                    (false, false) => "__cobrust_dict_set_i64_i64",
                };
                let base_op = self.lower_expr(base)?;
                let key_op = self.lower_index(index)?;
                let val_op = self.lower_expr(value)?;
                // Set returns no value (signature has None return); we
                // sink the discard into a junk i64 dest. The
                // `Terminator::Call` ABI always carries a destination,
                // so we make a one-off scratch local.
                let scratch = self.declare_local("_dsetret".to_string(), Ty::None, span, false);
                let cur = self.current_block_id();
                let next = self.start_new_block();
                self.cur_block = Some(cur.0 as usize);
                self.terminate(Terminator::Call {
                    func: Operand::Constant(Constant::Str(set_symbol.to_string())),
                    args: vec![base_op, key_op, val_op],
                    destination: Place::local(scratch),
                    target: next,
                    unwind: None,
                });
                self.cur_block = Some(next.0 as usize);
                return Ok(());
            }
        }
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

    /// ADR-0035: shared condition-lowering root primitive used by both
    /// `if` and `while` heads. Lowers the condition expression `expr`
    /// (which may emit auxiliary blocks for division asserts on `%` /
    /// `/` / `//`, short-circuit boolean evaluation, etc.) starting
    /// from `self.cur_block`, and returns the condition `Operand` plus
    /// the `BlockId` where the operand's value is finally available
    /// — i.e. the block where any consumer (`Terminator::SwitchInt`)
    /// must be emitted.
    ///
    /// Pre-condition: `self.cur_block` is set to the block where the
    /// condition evaluation should begin.
    /// Post-condition: `self.cur_block == Some(cond_end_block)`. The
    /// caller is responsible for terminating `cond_end_block` with the
    /// appropriate branch terminator (typically `SwitchInt`).
    ///
    /// The bug closed by this primitive (LC 263 `while n % 2 == 0`):
    /// before extraction, `lower_loop`'s While arm reset `cur_block`
    /// back to `header` after `lower_expr(cond)` returned, causing
    /// the SwitchInt to be written into `header` while the actual
    /// condition assigns lived in a downstream block (the post-divassert
    /// successor). The header thus read a stale (zero-initialised)
    /// `_bin` temp every loop iteration and the body never entered.
    /// `lower_if` already used the correct `cond_end_block` pattern;
    /// extracting this helper aligns both heads on the same primitive
    /// per ADR-0035 §"Decision".
    fn lower_condition(&mut self, expr: &Expr) -> Result<(Operand, BlockId), MirError> {
        let cond_op = self.lower_expr(expr)?;
        let cond_end_block = self.current_block_id();
        Ok((cond_op, cond_end_block))
    }

    fn lower_if(
        &mut self,
        arms: &[(Expr, HirBlock)],
        else_block: Option<&HirBlock>,
    ) -> Result<(), MirError> {
        // Capture the caller's current block BEFORE allocating merge_block.
        // The old code used `(merge_block.0).saturating_sub(1)` to recover
        // the caller's block, but that assumed merge_block was allocated
        // immediately after the caller's block. When called from inside a
        // while-loop body (where `exit_block` was allocated after `body_block`
        // before `lower_block` is called), the saturating_sub erroneously
        // resolved to the exit_block instead of body_block, routing all
        // conditional logic into the exit_block and leaving body_block as
        // `Unreachable`. Fix: capture the caller's block id first, then
        // allocate merge_block without clobbering it. (ADR-0030 §Diagnosis)
        let caller_block = self.current_block_id();

        // Pre-allocate the merge block (where all arms join).
        // start_new_block() sets cur_block = merge_block; restore to caller.
        let merge_block = self.start_new_block();
        self.cur_block = Some(caller_block.0 as usize);

        // Strategy: for each arm, in current block evaluate cond via
        // `lower_condition` (ADR-0035 root primitive), emit
        // `SwitchInt cond -> [(true, body), (false, next_arm)]`,
        // body ends with Goto(merge), next becomes current.

        let mut cur = self.current_block_id();
        let mut arm_bodies: Vec<BlockId> = Vec::new();
        for (cond, body) in arms {
            // Evaluate cond starting from `cur` via the shared
            // `lower_condition` primitive. The primitive returns
            // `cond_end_block` — the block that holds the final cond
            // operand and where the SwitchInt must land (NOT the
            // starting `cur`, which may have been terminated by a
            // div-assert etc.).
            self.cur_block = Some(cur.0 as usize);
            let (cond_op, cond_end_block) = self.lower_condition(cond)?;
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
            // Terminate the cond-end block with the branch.
            self.cur_block = Some(cond_end_block.0 as usize);
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
                // header → [cond chain] → cond_end_block → SwitchInt → [body, exit/else]
                //
                // ADR-0035: condition lowering goes through the shared
                // `lower_condition` primitive used by both `if` and `while`
                // heads. The primitive may emit auxiliary blocks (e.g.
                // div-assert successor for `n % 2`); the SwitchInt must be
                // emitted in `cond_end_block`, NOT in `header`, otherwise
                // the cond's final assigns are orphaned in a separate block
                // and the SwitchInt reads a stale (zero-initialised) value
                // — the LC 263 `while n % 2 == 0` miscompile shape.
                //
                // The body's back-edge `Goto(header)` is correct: jumping to
                // header re-enters the full cond-eval chain (header still
                // ends with `Assert(divcond) -> assert_target`, and
                // assert_target's SwitchInt re-fires) so each iteration
                // recomputes the condition's value.
                self.ensure_open_block();
                let pre = self.current_block_id();
                let header = self.start_new_block();
                // pre falls into header.
                self.cur_block = Some(pre.0 as usize);
                self.terminate(Terminator::Goto(header));
                self.cur_block = Some(header.0 as usize);
                let (cond_op, cond_end_block) = self.lower_condition(cond)?;
                let body_block = self.start_new_block();
                let exit_block = self.start_new_block();
                // Terminate cond_end_block (where the cond operand is
                // available) with SwitchInt — NOT header, which may already
                // be terminated by a div-assert flowing into the cond chain.
                self.cur_block = Some(cond_end_block.0 as usize);
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
                // ADR-0050b §"Decision" — for-loop lowers to length-bound
                // index iteration over the iter source's list layout:
                //
                //   let __iter   = <iter_expr>
                //   let __len    = __cobrust_list_len(__iter)
                //   let __idx: i64 = 0
                //   let var      = <declared, type = element type>
                //   header:
                //     if __idx < __len: goto body  else: goto exit
                //   body:
                //     var = __cobrust_list_get(__iter, __idx)
                //     [lower body block]
                //     __idx = __idx + 1
                //     goto header
                //   exit:
                //     [optional else block]
                //
                // This supersedes the ADR-0027 §4 iter-protocol path
                // (`__cobrust_iter_init/next/drop`) because the protocol's
                // 0-as-None convention collides with list[i64] elements
                // that are legitimately 0 (the first iteration of
                // `for v in range(0, n):` immediately exits). The
                // length-bound index iteration is unambiguous and
                // composes monotonically with Phase G iter-protocol
                // expansion (when user `__iter__` lands, this primitive
                // becomes one of several iteration shapes the type
                // checker dispatches between).
                self.ensure_open_block();

                // Step 1: evaluate iter expression → iter_local.
                let iter_val_op = self.lower_expr(iter)?;
                let iter_local = self.declare_local("_iter".to_string(), Ty::None, span, true);
                self.emit_assign(Place::local(iter_local), Rvalue::Use(iter_val_op), span);

                // Step 2: call __cobrust_list_len(iter_local) → len_local.
                let len_local = self.declare_local("_iter_len".to_string(), Ty::Int, span, true);
                let cur = self.current_block_id();
                let after_len = self.start_new_block();
                self.cur_block = Some(cur.0 as usize);
                self.terminate(Terminator::Call {
                    func: Operand::Constant(Constant::Str("__cobrust_list_len".to_string())),
                    args: vec![Operand::Copy(Place::local(iter_local))],
                    destination: Place::local(len_local),
                    target: after_len,
                    unwind: None,
                });
                self.cur_block = Some(after_len.0 as usize);

                // Step 3: declare __idx and initialise to 0.
                let idx_local = self.declare_local("_iter_idx".to_string(), Ty::Int, span, true);
                self.emit_assign(
                    Place::local(idx_local),
                    Rvalue::Use(Operand::Constant(Constant::Int(0))),
                    span,
                );

                // Step 4: declare the loop-var binding.
                let var_local = if let PatternKind::Binding(name, def_id) = &pattern.kind {
                    let ty = self.ctx.lookup_ty(*def_id);
                    Some(self.declare_local_for_def(*def_id, name.clone(), ty, span, true))
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
                    None
                };

                // Step 5: header — emit `idx < len` via Rvalue::BinaryOp,
                // then SwitchInt on the bool.
                let pre = self.current_block_id();
                let header = self.start_new_block();
                self.cur_block = Some(pre.0 as usize);
                self.terminate(Terminator::Goto(header));
                self.cur_block = Some(header.0 as usize);
                let cond_local = self.declare_local("_iter_cond".to_string(), Ty::Bool, span, true);
                self.emit_assign(
                    Place::local(cond_local),
                    Rvalue::BinaryOp(
                        crate::tree::BinOp::Lt,
                        Operand::Copy(Place::local(idx_local)),
                        Operand::Copy(Place::local(len_local)),
                    ),
                    span,
                );
                let body_block = self.start_new_block();
                let exit_block = self.start_new_block();
                self.cur_block = Some(header.0 as usize);
                self.terminate(Terminator::SwitchInt {
                    operand: Operand::Copy(Place::local(cond_local)),
                    cases: vec![(SwitchValue::Bool(true), body_block)],
                    otherwise: exit_block,
                });

                // Step 6: body — fetch var via __cobrust_list_get, then
                // lower user body, then bump __idx, then goto header.
                //
                // ADR-0050c Phase 4 — clone emission for Str loop var.
                // For `for s in xs:` where `xs: list[str]`, the slots
                // are owned by `xs`. If the loop var `s: Ty::Str` got
                // a raw slot pointer via `__cobrust_list_get` and the
                // drop pass enumerated `s` as drop-eligible (Str is
                // non-Copy), then BOTH `s` and `xs`'s slot would call
                // `__cobrust_str_drop` on the same pointer at scope
                // exit. Double-free → segfault / abort / hang in
                // mimalloc's free-list walker.
                //
                // Resolution: when the loop-var type is `Ty::Str`,
                // fetch the raw pointer into a *throwaway i64 temp*
                // (no drop schedule), then materialise an owned clone
                // via `__cobrust_str_clone(raw) -> s`. The slot remains
                // owned by `xs`; the loop var owns its own fresh copy.
                self.loop_stack.push((header, exit_block));
                self.cur_block = Some(body_block.0 as usize);
                if let Some(vl) = var_local {
                    let vl_ty = self
                        .locals
                        .get(vl.0 as usize)
                        .map(|d| d.ty.clone())
                        .unwrap_or(Ty::None);
                    if matches!(vl_ty, Ty::Str) {
                        let raw_local =
                            self.declare_local("_iter_raw".to_string(), Ty::Int, span, false);
                        // body_block → Call(list_get → raw_local) → after_get
                        let after_get = self.start_new_block();
                        self.cur_block = Some(body_block.0 as usize);
                        self.terminate(Terminator::Call {
                            func: Operand::Constant(Constant::Str(
                                "__cobrust_list_get".to_string(),
                            )),
                            args: vec![
                                Operand::Copy(Place::local(iter_local)),
                                Operand::Copy(Place::local(idx_local)),
                            ],
                            destination: Place::local(raw_local),
                            target: after_get,
                            unwind: None,
                        });
                        // after_get → Call(str_clone(raw) → vl) → after_clone
                        let after_clone = self.start_new_block();
                        self.cur_block = Some(after_get.0 as usize);
                        self.terminate(Terminator::Call {
                            func: Operand::Constant(Constant::Str(
                                "__cobrust_str_clone".to_string(),
                            )),
                            args: vec![Operand::Copy(Place::local(raw_local))],
                            destination: Place::local(vl),
                            target: after_clone,
                            unwind: None,
                        });
                        self.cur_block = Some(after_clone.0 as usize);
                    } else {
                        let after_get = self.start_new_block();
                        self.cur_block = Some(body_block.0 as usize);
                        self.terminate(Terminator::Call {
                            func: Operand::Constant(Constant::Str(
                                "__cobrust_list_get".to_string(),
                            )),
                            args: vec![
                                Operand::Copy(Place::local(iter_local)),
                                Operand::Copy(Place::local(idx_local)),
                            ],
                            destination: Place::local(vl),
                            target: after_get,
                            unwind: None,
                        });
                        self.cur_block = Some(after_get.0 as usize);
                    }
                }
                self.lower_block(body)?;
                if !self.terminated() {
                    // Bump __idx and loop back to header.
                    self.emit_assign(
                        Place::local(idx_local),
                        Rvalue::BinaryOp(
                            crate::tree::BinOp::Add,
                            Operand::Copy(Place::local(idx_local)),
                            Operand::Constant(Constant::Int(1)),
                        ),
                        span,
                    );
                    self.terminate(Terminator::Goto(header));
                }
                self.loop_stack.pop();

                // Step 7: exit — optional else block.
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
                        FormatPart::Hole {
                            expr, format_spec, ..
                        } => {
                            let op = self.lower_expr(expr)?;
                            ops.push(op);
                            // M-F.3.3 gap (c): when a format spec is present
                            // (e.g. ".2f", "e", "g"), encode it as a special
                            // sentinel Constant::Str immediately after the
                            // value operand. The codegen's
                            // `lower_aggregate_format_string` detects the
                            // `FMTSPEC:` prefix and routes to the precision
                            // formatter instead of the plain `__cobrust_fmt_float`.
                            if let Some(spec) = format_spec {
                                if !spec.is_empty() {
                                    ops.push(Operand::Constant(Constant::Str(format!(
                                        "FMTSPEC:{spec}"
                                    ))));
                                }
                            }
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
                // ADR-0050c Phase 2 — TD-1 closure: synthesise the element
                // type from the first element so codegen's
                // `Terminator::Drop` arm can dispatch on Ty::List(elem).
                // For `["a", "b"]` this records `Ty::List(Ty::Str)`,
                // enabling the per-element `__cobrust_str_drop` schedule.
                let elem_ty = items.first().map_or(Ty::None, |it| synth_expr_ty(self, it));
                for it in items {
                    ops.push(self.lower_expr(it)?);
                }
                let temp = self.declare_local(
                    "_list".to_string(),
                    Ty::List(Box::new(elem_ty)),
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
                // ADR-0050d sub-sprint c — synthesize K/V types from the
                // first Pair entry so codegen's `lower_aggregate_dict`
                // can dispatch to typed `__cobrust_dict_set_K_V`. Same
                // pattern as the List arm's `synth_expr_ty(items[0])`.
                // For empty `{}` we fall back to (Ty::None, Ty::None);
                // the codegen treats this as the (i64, i64) shape (the
                // legacy `K_TAG_I64`/`V_TAG_I64` defaults).
                let (k_ty, v_ty) = entries
                    .iter()
                    .find_map(|entry| match entry {
                        DictEntry::Pair(k, v) => {
                            Some((synth_expr_ty(self, k), synth_expr_ty(self, v)))
                        }
                        DictEntry::Spread(_) => None,
                    })
                    .unwrap_or((Ty::None, Ty::None));
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
                    Ty::Dict(Box::new(k_ty), Box::new(v_ty)),
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
                // ADR-0041 §H6: comprehension desugaring.
                //
                // Lower `[elem for pat in iter (if g)*]` as:
                //
                //   __acc = __cobrust_list_new(8, 0)
                //   __it  = __cobrust_iter_init(iter)
                //   loop:
                //     __opt = __cobrust_iter_next(__it)
                //     if __opt == 0: goto exit
                //     pat = __opt   (binding)
                //     if all guards: __cobrust_list_append(__acc, elem)
                //     goto loop
                //   exit:
                //
                // For nested clauses (`[x*y for x in xs for y in ys]`),
                // the loops nest in left-to-right order. M12.x ships the
                // single-clause path; multi-clause is the recursive
                // generalization.
                self.lower_comprehension(comp, e.span)
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
                // ADR-0050c Phase 2 cascade fix: source-level `xs[i]` on a
                // `list[T]` base must go through the runtime helper
                // `__cobrust_list_get(xs, i) -> i64` rather than the
                // codegen-side `Projection::Index` (which at M12.x is a
                // no-op pass-through, surfacing as a segfault when the
                // user actually consumes the result — see f3ls09 / f3ls13
                // / f3ls29 in the list[str] corpus).
                //
                // The base's HIR-recorded type tells us whether the index
                // is a list lookup or some other shape (tuple / dict /
                // str). For now we only special-case `Ty::List(_)`; tuple
                // / dict are out of ADR-0050c scope.
                let base_ty = synth_expr_ty(self, base);
                // ADR-0050d sub-sprint c — Dict index read.
                //
                // Source-level `d[k]` on `d: Dict[K, V]` lowers to:
                //   __cobrust_dict_get_K_V(d, k) -> V
                //
                // The codegen-side `runtime_funcs` table already imports
                // these symbols (per Phase 3+4 wiring). For Str values,
                // the runtime returns a fresh `*mut u8` buffer (caller-
                // owned via the drop schedule). For i64 values, the
                // sentinel-on-missing is 0 (Decision 2A documents this
                // as the panic-on-missing path; an explicit abort
                // helper is sub-sprint c's stretch goal).
                if let Ty::Dict(k_ty, v_ty) = &base_ty {
                    let key_is_str = matches!(**k_ty, Ty::Str);
                    let val_is_str = matches!(**v_ty, Ty::Str);
                    let get_symbol = match (key_is_str, val_is_str) {
                        (true, true) => "__cobrust_dict_get_str_str",
                        (true, false) => "__cobrust_dict_get_str_i64",
                        (false, true) => "__cobrust_dict_get_i64_str",
                        (false, false) => "__cobrust_dict_get_i64_i64",
                    };
                    let base_op = self.lower_expr(base)?;
                    let key_op = self.lower_index(index)?;
                    let dest_ty = (**v_ty).clone();
                    let dest = self.declare_local("_didxget".to_string(), dest_ty, e.span, false);
                    let cur = self.current_block_id();
                    let next = self.start_new_block();
                    self.cur_block = Some(cur.0 as usize);
                    self.terminate(Terminator::Call {
                        func: Operand::Constant(Constant::Str(get_symbol.to_string())),
                        args: vec![base_op, key_op],
                        destination: Place::local(dest),
                        target: next,
                        unwind: None,
                    });
                    self.cur_block = Some(next.0 as usize);
                    if val_is_str {
                        return Ok(Operand::Move(Place::local(dest)));
                    }
                    return Ok(Operand::Copy(Place::local(dest)));
                }
                if matches!(base_ty, Ty::List(_)) {
                    let base_op = self.lower_expr(base)?;
                    let idx_op = self.lower_index(index)?;
                    let elem_ty = if let Ty::List(elem) = &base_ty {
                        (**elem).clone()
                    } else {
                        Ty::None
                    };
                    // ADR-0050c Phase 4 — clone emission for Str-indexed
                    // reads. For `xs[i]` where `xs: list[str]`, the raw
                    // slot pointer aliases the slot owned by `xs`. The
                    // drop pass enumerates the destination temp as
                    // drop-eligible (Str non-Copy), so without cloning
                    // BOTH the temp and `xs`'s slot would call
                    // `__cobrust_str_drop` on the same pointer at scope
                    // exit — double-free.
                    //
                    // Resolution: fetch the raw pointer into a throwaway
                    // i64 temp, then `__cobrust_str_clone` into the typed
                    // dest. Mirror of the for-loop body fix above.
                    if matches!(elem_ty, Ty::Str) {
                        // Step 1: list_get into raw i64 temp.
                        let raw_dest =
                            self.declare_local("_idxraw".to_string(), Ty::Int, e.span, false);
                        let cur = self.current_block_id();
                        let after_get = self.start_new_block();
                        self.cur_block = Some(cur.0 as usize);
                        self.terminate(Terminator::Call {
                            func: Operand::Constant(Constant::Str(
                                "__cobrust_list_get".to_string(),
                            )),
                            args: vec![base_op, idx_op],
                            destination: Place::local(raw_dest),
                            target: after_get,
                            unwind: None,
                        });
                        // Step 2: str_clone(raw) → owned Str dest.
                        let clone_dest =
                            self.declare_local("_idxget".to_string(), elem_ty, e.span, false);
                        let after_clone = self.start_new_block();
                        self.cur_block = Some(after_get.0 as usize);
                        self.terminate(Terminator::Call {
                            func: Operand::Constant(Constant::Str(
                                "__cobrust_str_clone".to_string(),
                            )),
                            args: vec![Operand::Copy(Place::local(raw_dest))],
                            destination: Place::local(clone_dest),
                            target: after_clone,
                            unwind: None,
                        });
                        self.cur_block = Some(after_clone.0 as usize);
                        // Return Move so the operand-consumer takes
                        // ownership of the freshly-cloned Str (and the
                        // drop pass excludes clone_dest from the auto-
                        // drop chain at this scope's return).
                        return Ok(Operand::Move(Place::local(clone_dest)));
                    }
                    // Non-Str elem types: simple list_get into typed dest.
                    let dest = self.declare_local("_idxget".to_string(), elem_ty, e.span, false);
                    let cur = self.current_block_id();
                    let next = self.start_new_block();
                    self.cur_block = Some(cur.0 as usize);
                    self.terminate(Terminator::Call {
                        func: Operand::Constant(Constant::Str("__cobrust_list_get".to_string())),
                        args: vec![base_op, idx_op],
                        destination: Place::local(dest),
                        target: next,
                        unwind: None,
                    });
                    self.cur_block = Some(next.0 as usize);
                    return Ok(Operand::Copy(Place::local(dest)));
                }
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
            // ADR-0052a Wave-1 §7 — `&expr` lowering. The borrow arm
            // always emits `Operand::Copy(place)` regardless of the
            // underlying type's Copy-ness. This is the §3 §13-honest
            // lowering: a borrow is a shared read, never a move, so
            // borrow.rs:114's `UseAfterMove` does not fire on borrowed
            // reads. Wave-1 cap: parser ensures inner is `Name`,
            // `Attr`, or `Index` of a place; we delegate to
            // `lower_borrow_inner` which mirrors the place projection
            // walk without crossing the move/copy boundary.
            ExprKind::Borrow(inner) => self.lower_borrow_inner(inner, e.span),
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
            ExprKind::Cast { expr, target } => {
                // M-F.3.3 gap (a): lower `expr as T` to `Rvalue::Cast(kind, op, ty)`.
                // Permitted pairs (constitution §2.2): i64↔f64.
                // The type checker has already validated the pair; we derive the
                // CastKind from the target type name.
                let op = self.lower_expr(expr)?;
                let target_name = match &target.kind {
                    cobrust_frontend::ast::TypeKind::Name(parts) => parts.join("."),
                    _ => String::new(),
                };
                let (cast_kind, ty) = match target_name.as_str() {
                    "f64" | "float" => (CastKind::IntToFloat, Ty::Float),
                    "i64" | "int" => (CastKind::FloatToInt, Ty::Int),
                    _ => {
                        // Unknown cast target — emit a no-op Move.
                        return Ok(op);
                    }
                };
                let dest = self.declare_local("_cast".to_string(), ty.clone(), e.span, false);
                self.emit_assign(Place::local(dest), Rvalue::Cast(cast_kind, op, ty), e.span);
                Ok(Operand::Copy(Place::local(dest)))
            }
        }
    }

    fn lower_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        span: Span,
    ) -> Result<Operand, MirError> {
        // ADR-0034 §"Decision" Option 3: when the callee is a `Name`
        // expression whose resolved type is `Ty::Fn(...)`, emit
        // `Operand::Constant(Constant::FnRef(rn.def_id.0))` so the
        // codegen layer can dispatch via the per-module
        // forward-declaration table (`CraneliftCtx.function_ids`).
        // Without this, a fn-typed Name lowers via `lower_expr` to
        // `Operand::Move(Place::local(L))` where L's `Ty::Fn` does not
        // map to any Cranelift scalar — codegen would then take the
        // M9 stub `iconst(I64, 0)` path and the call's return value
        // would be a constant zero (broken for any non-trivial recursion
        // or cross-fn dispatch).
        //
        // M-F.3.6 ADR-0050f: detect the 7 file-IO PRELUDE fns and record
        // whether their str args should be Copy-at-operand. These shims
        // READ the Str pointer without freeing it (borrow-not-move).
        // This mirrors the Phase 2a walk-back for List operands.
        let callee_name = if let ExprKind::Name(rn) = &callee.kind {
            Some(rn.name.as_str())
        } else {
            None
        };
        // File-IO fns whose str args are Copy-at-operand (borrow-not-move).
        let is_file_io_borrow = matches!(
            callee_name,
            Some(
                "read_file"
                    | "read_file_lines"
                    | "write_file"
                    | "append_file"
                    | "stdout_write"
                    | "stderr_write"
            )
        );
        let callee_op = if let ExprKind::Name(rn) = &callee.kind {
            let ty = self.ctx.lookup_ty(rn.def_id);
            if matches!(ty, Ty::Fn(_)) {
                Operand::Constant(Constant::FnRef(rn.def_id.0))
            } else {
                self.lower_expr(callee)?
            }
        } else {
            self.lower_expr(callee)?
        };
        let mut arg_ops = Vec::new();
        for a in args {
            match a {
                CallArg::Positional(e)
                | CallArg::Keyword(_, e)
                | CallArg::StarArgs(e)
                | CallArg::StarStarKwargs(e) => {
                    let op = self.lower_expr(e)?;
                    // M-F.3.6: upgrade Move→Copy for Str args of file-IO
                    // borrow fns so the caller's local remains live after
                    // the call (ADR-0050f §"Copy-at-operand" rationale).
                    let op = if is_file_io_borrow {
                        upgrade_move_to_copy_for_str(self, op)
                    } else {
                        op
                    };
                    arg_ops.push(op);
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
        // ADR-0041 §H2: `and` / `or` MUST short-circuit. We materialize
        // explicit control flow at MIR — evaluate LHS first, branch on
        // its bool value, and conditionally evaluate RHS. A merge block
        // assigns the unified bool result.
        if matches!(op, HirBinOp::And | HirBinOp::Or) {
            return self.lower_short_circuit_bool(op, lhs, rhs, span);
        }
        // ADR-0050d Decision 4A — `key in d` for Dict-typed RHS.
        //
        // Lowers `k in d` (where d: Dict[K, _]) to:
        //   __cobrust_dict_contains_K(d, k) -> i64 (0/1)
        // Then upcasts the i64 result to bool via a comparison.
        //
        // Codegen's `BinOp::In` arm errors out by design (the
        // language-level In for arbitrary iterables is not yet
        // implemented at codegen); the Dict-specific intrinsic-rewrite
        // here short-circuits that error before MIR reaches codegen.
        if matches!(op, HirBinOp::In | HirBinOp::NotIn) {
            let rhs_ty = synth_expr_ty(self, rhs);
            if let Ty::Dict(k_ty, _) = &rhs_ty {
                let key_is_str = matches!(**k_ty, Ty::Str);
                let contains_symbol = if key_is_str {
                    "__cobrust_dict_contains_str"
                } else {
                    "__cobrust_dict_contains_i64"
                };
                let key_op = self.lower_expr(lhs)?;
                let dict_op = self.lower_expr(rhs)?;
                let raw_dest = self.declare_local("_dctn".to_string(), Ty::Int, span, false);
                let cur = self.current_block_id();
                let next = self.start_new_block();
                self.cur_block = Some(cur.0 as usize);
                self.terminate(Terminator::Call {
                    func: Operand::Constant(Constant::Str(contains_symbol.to_string())),
                    args: vec![dict_op, key_op],
                    destination: Place::local(raw_dest),
                    target: next,
                    unwind: None,
                });
                self.cur_block = Some(next.0 as usize);
                // The raw i64 0/1 result is the bool value the
                // SwitchInt expects (per `__cobrust_dict_is_empty`
                // precedent — bool ABI = i64 0/1 lower-bit). Wrap as a
                // bool by comparing != 0 (NotIn inverts via Eq 0).
                let cmp_op = if matches!(op, HirBinOp::NotIn) {
                    BinOp::Eq
                } else {
                    BinOp::NotEq
                };
                let bool_dest = self.declare_local("_dctnb".to_string(), Ty::Bool, span, false);
                self.emit_assign(
                    Place::local(bool_dest),
                    Rvalue::BinaryOp(
                        cmp_op,
                        Operand::Copy(Place::local(raw_dest)),
                        Operand::Constant(Constant::Int(0)),
                    ),
                    span,
                );
                return Ok(Operand::Copy(Place::local(bool_dest)));
            }
        }
        let lhs_op = self.lower_expr(lhs)?;
        let rhs_op = self.lower_expr(rhs)?;
        let mir_op = bin_to_mir(op);
        // For integer division, emit assert(rhs != 0).
        // IEEE 754 float division by zero is defined (produces ±inf / NaN),
        // so skip the assert for float operands (constitution §2.2 / f64e21).
        let needs_div_assert = matches!(op, HirBinOp::Div | HirBinOp::FloorDiv | HirBinOp::Mod)
            && !hir_expr_is_float(lhs);
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

    /// ADR-0052a Wave-1 §7 — lower the inner of an `&expr` borrow as
    /// a non-consuming shared read. The key invariant is **never emit
    /// `Operand::Move`** on the inner place — borrowed reads are
    /// always `Operand::Copy`, so `borrow.rs:114`'s `UseAfterMove` does
    /// not flag the same local on subsequent reads.
    ///
    /// Wave-1 §8 cap restricts the inner to one of:
    ///   - `Name(rn)` — direct local read; produces
    ///     `Operand::Copy(Place::local(local))` regardless of
    ///     `is_copy_type` (the override is the whole point).
    ///   - `Attr { base, .. }` / `Index { base, index }` — Wave-1
    ///     accepts the existing `lower_expr` path for these shapes
    ///     because their lowering already emits `Operand::Copy` of a
    ///     projection (Attr) or a freshly-cloned owned value for
    ///     `list[str]` indexing. Semantically still a borrow at the
    ///     source level; the slight inefficiency for `&xs[i]` on
    ///     `list[str]` is acceptable Wave-1 (Phase H may revisit with
    ///     proper borrow-projection).
    fn lower_borrow_inner(&mut self, inner: &Expr, _span: Span) -> Result<Operand, MirError> {
        match &inner.kind {
            ExprKind::Name(rn) => {
                let local = self.lookup_local_for_resolved(rn, inner.span)?;
                // Override the move/copy dispatch: borrow is always Copy.
                Ok(Operand::Copy(Place::local(local)))
            }
            // For Attr / Index Wave-1 delegates to the standard
            // expression lowering. The Attr arm already emits
            // `Operand::Copy(projection)` and the Index arm either
            // emits `Operand::Copy(projection)` (non-Str) or
            // synthesises a fresh owned clone (str list element);
            // either way the inner is not consumed by Move.
            _ => self.lower_expr(inner),
        }
    }

    /// ADR-0041 §H2: short-circuit `and` / `or` at MIR.
    ///
    /// Lowers `a and b` as:
    ///   pre:                  result = lhs
    ///                         SwitchInt(result) -> [(true, eval_rhs), false: merge]
    ///   eval_rhs:             result = rhs;  Goto merge
    ///   merge:                — caller resumes here with `Copy(result)`
    ///
    /// Lowers `a or b` as:
    ///   pre:                  result = lhs
    ///                         SwitchInt(result) -> [(false, eval_rhs), true: merge]
    ///   eval_rhs:             result = rhs;  Goto merge
    ///   merge:                — caller resumes here with `Copy(result)`
    ///
    /// This matches CPython's documented evaluation order for `and` /
    /// `or` — both yield the LHS unchanged when the LHS already
    /// determines the result. Type-check (ADR-0003 §"Selected typing
    /// rules") restricts `and`/`or` to `bool`-typed operands, so the
    /// returned operand is always `Ty::Bool`; we type the merge local
    /// accordingly.
    fn lower_short_circuit_bool(
        &mut self,
        op: HirBinOp,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
    ) -> Result<Operand, MirError> {
        debug_assert!(matches!(op, HirBinOp::And | HirBinOp::Or));
        let result_local = self.declare_local("_sc_bool".to_string(), Ty::Bool, span, true);
        // Step 1 — evaluate LHS into result_local.
        let lhs_op = self.lower_expr(lhs)?;
        self.emit_assign(Place::local(result_local), Rvalue::Use(lhs_op), span);

        // Step 2 — branch on result_local; for `and`, evaluate RHS only
        // when LHS is true; for `or`, only when LHS is false.
        let cond_block = self.current_block_id();
        let eval_rhs_block = self.start_new_block();
        let merge_block = self.start_new_block();
        self.cur_block = Some(cond_block.0 as usize);
        let cases = match op {
            HirBinOp::And => vec![(SwitchValue::Bool(true), eval_rhs_block)],
            HirBinOp::Or => vec![(SwitchValue::Bool(false), eval_rhs_block)],
            _ => unreachable!(),
        };
        // For `and`, the otherwise branch (LHS=false) skips RHS — go
        // straight to merge with result already = false. For `or`, the
        // otherwise branch (LHS=true) skips RHS — go to merge with
        // result already = true.
        self.terminate(Terminator::SwitchInt {
            operand: Operand::Copy(Place::local(result_local)),
            cases,
            otherwise: merge_block,
        });

        // Step 3 — eval_rhs_block: overwrite result with RHS, fall through.
        self.cur_block = Some(eval_rhs_block.0 as usize);
        let rhs_op = self.lower_expr(rhs)?;
        self.emit_assign(Place::local(result_local), Rvalue::Use(rhs_op), span);
        if !self.terminated() {
            self.terminate(Terminator::Goto(merge_block));
        }

        // Step 4 — caller resumes at merge.
        self.cur_block = Some(merge_block.0 as usize);
        Ok(Operand::Copy(Place::local(result_local)))
    }

    // -----------------------------------------------------------------
    // ADR-0041 §H6: comprehension desugar
    // -----------------------------------------------------------------

    /// Lower `[elem for pat in iter (if g)*]` to a real loop+append,
    /// not the M8 empty-list placeholder. The strategy is identical to
    /// `LoopKind::For` in `lower_loop`, with two additions:
    ///
    /// 1. Allocate `__acc = __cobrust_list_new(8, 0)` upfront.
    /// 2. In the loop body, evaluate the element expression and call
    ///    `__cobrust_list_append(__acc, elem)`.
    ///
    /// Multi-clause comprehensions `[x*y for x in xs for y in ys]`
    /// recurse: the outer clause's body is the inner comprehension's
    /// body.  M12.x ships single-clause; nested clauses fold via the
    /// same recursion in this function.
    fn lower_comprehension(&mut self, comp: &Comp, span: Span) -> Result<Operand, MirError> {
        // Step 1 — allocate accumulator (List<i64> by ABI; ADR-0041 §H6
        // notes the i64 narrowing matches `__cobrust_list_*` ABI
        // convention used by the rest of the runtime).
        self.ensure_open_block();
        let acc_local = self.declare_local(
            "_comp_acc".to_string(),
            Ty::List(Box::new(Ty::None)),
            span,
            true,
        );
        // Allocate via __cobrust_list_new(8, 0): elem_size=8, len=0.
        let cur = self.current_block_id();
        let after_new = self.start_new_block();
        self.cur_block = Some(cur.0 as usize);
        self.terminate(Terminator::Call {
            func: Operand::Constant(Constant::Str("__cobrust_list_new".to_string())),
            args: vec![
                Operand::Constant(Constant::Int(8)),
                Operand::Constant(Constant::Int(0)),
            ],
            destination: Place::local(acc_local),
            target: after_new,
            unwind: None,
        });
        self.cur_block = Some(after_new.0 as usize);

        // Step 2 — emit nested clauses (for clause0; for clause1; ...
        // body). All clauses share the SAME accumulator `acc_local`.
        let element = comp.element.clone();
        let kind = comp.kind;
        self.lower_comp_clauses(&comp.clauses, &element, kind, acc_local, span)?;

        // Step 3 — return the accumulator. (Type-checker has already
        // resolved this to Ty::List(elem) etc; MIR ABI is i64 ptr.)
        Ok(Operand::Move(Place::local(acc_local)))
    }

    /// Emit the for-loop nest for a comprehension. Recurses on the
    /// clauses tail; at depth 0 the body is the element-collect.
    fn lower_comp_clauses(
        &mut self,
        clauses: &[CompClause],
        element: &CompElem,
        kind: CompKind,
        acc: LocalId,
        span: Span,
    ) -> Result<(), MirError> {
        let Some((first, rest)) = clauses.split_first() else {
            // No more clauses — emit guards and append.
            return self.lower_comp_body(element, kind, acc, span);
        };
        // Mirror of LoopKind::For lowering in `lower_loop`.
        let iter_val_op = self.lower_expr(&first.iter)?;
        let iter_local = self.declare_local("_comp_iter".to_string(), Ty::None, span, true);
        self.emit_assign(Place::local(iter_local), Rvalue::Use(iter_val_op), span);

        let it_local = self.declare_local("_comp_iter_handle".to_string(), Ty::None, span, true);
        let cur = self.current_block_id();
        let init_target = self.start_new_block();
        self.cur_block = Some(cur.0 as usize);
        self.terminate(Terminator::Call {
            func: Operand::Constant(Constant::Str("__cobrust_iter_init".to_string())),
            args: vec![Operand::Copy(Place::local(iter_local))],
            destination: Place::local(it_local),
            target: init_target,
            unwind: None,
        });
        self.cur_block = Some(init_target.0 as usize);

        // Bind the loop variable from the pattern.
        let var_local = if let PatternKind::Binding(name, def_id) = &first.target.kind {
            let ty = self.ctx.lookup_ty(*def_id);
            Some(self.declare_local_for_def(*def_id, name.clone(), ty, span, true))
        } else {
            for did in &first.binding_def_ids {
                let ty = self.ctx.lookup_ty(*did);
                self.declare_local_for_def(
                    *did,
                    format!("_comp_iter_bind_{}", did.0),
                    ty,
                    span,
                    true,
                );
            }
            None
        };

        let pre = self.current_block_id();
        let header = self.start_new_block();
        let opt_local = self.declare_local("_comp_iter_opt".to_string(), Ty::None, span, true);
        self.cur_block = Some(pre.0 as usize);
        self.terminate(Terminator::Goto(header));
        self.cur_block = Some(header.0 as usize);

        let after_next = self.start_new_block();
        self.cur_block = Some(header.0 as usize);
        self.terminate(Terminator::Call {
            func: Operand::Constant(Constant::Str("__cobrust_iter_next".to_string())),
            args: vec![Operand::Copy(Place::local(it_local))],
            destination: Place::local(opt_local),
            target: after_next,
            unwind: None,
        });
        self.cur_block = Some(after_next.0 as usize);

        if let Some(vl) = var_local {
            self.emit_assign(
                Place::local(vl),
                Rvalue::Use(Operand::Copy(Place::local(opt_local))),
                span,
            );
        }

        let body_block = self.start_new_block();
        let exit_block = self.start_new_block();
        self.cur_block = Some(after_next.0 as usize);
        self.terminate(Terminator::SwitchInt {
            operand: Operand::Copy(Place::local(opt_local)),
            cases: vec![(SwitchValue::Bool(false), exit_block)],
            otherwise: body_block,
        });

        // Inside body_block — evaluate guards (if any) then recurse.
        self.cur_block = Some(body_block.0 as usize);
        // Apply guards: chain `if !guard: continue (i.e., goto header)`.
        if !first.guards.is_empty() {
            for guard in &first.guards {
                let cur_b = self.current_block_id();
                let (g_op, g_end) = self.lower_condition(guard)?;
                let after_g = self.start_new_block();
                self.cur_block = Some(g_end.0 as usize);
                self.terminate(Terminator::SwitchInt {
                    operand: g_op,
                    cases: vec![(SwitchValue::Bool(true), after_g)],
                    otherwise: header,
                });
                let _ = cur_b;
                self.cur_block = Some(after_g.0 as usize);
            }
        }
        // Recurse into the remaining clauses (or emit the body at the
        // base case).
        self.lower_comp_clauses(rest, element, kind, acc, span)?;
        if !self.terminated() {
            self.terminate(Terminator::Goto(header));
        }

        self.cur_block = Some(exit_block.0 as usize);
        Ok(())
    }

    /// Innermost body of a comprehension — evaluate the element and
    /// append into the accumulator.
    fn lower_comp_body(
        &mut self,
        element: &CompElem,
        kind: CompKind,
        acc: LocalId,
        span: Span,
    ) -> Result<(), MirError> {
        // M12.x scope: list / set / generator collect a single value
        // per iteration; dict comprehensions emit two-arg-set. The
        // current ABI only ships `__cobrust_list_append`; set / dict
        // append are deferred to the same M11.x track that adds their
        // runtime helpers.
        let elem_op = match element {
            CompElem::Single(e) => self.lower_expr(e)?,
            CompElem::KeyValue(k, _v) => {
                // Dict comprehensions: M12.x emits the key only as a
                // placeholder until __cobrust_dict_set with computed
                // key/value lands. Records the body so type-check is
                // honored.
                self.lower_expr(k)?
            }
        };
        // Comprehension kinds we don't yet have an append path for
        // (Set / Dict) still emit a `Call` so the body is materialized;
        // the runtime will silently no-op when the helper does not
        // exist on the path. M11.x rolls in `__cobrust_set_insert`
        // and `__cobrust_dict_set` here.
        let helper = match kind {
            CompKind::List | CompKind::Generator => "__cobrust_list_append",
            CompKind::Set => "__cobrust_set_insert",
            CompKind::Dict => "__cobrust_list_append",
        };
        let cur = self.current_block_id();
        let after = self.start_new_block();
        self.cur_block = Some(cur.0 as usize);
        let dest = self.declare_local("_comp_appended".to_string(), Ty::None, span, false);
        self.terminate(Terminator::Call {
            func: Operand::Constant(Constant::Str(helper.to_string())),
            args: vec![Operand::Copy(Place::local(acc)), elem_op],
            destination: Place::local(dest),
            target: after,
            unwind: None,
        });
        self.cur_block = Some(after.0 as usize);
        Ok(())
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
            let f = parse_float_lit(s);
            Constant::Float(f.to_bits())
        }
        Lit::Imag(s) => {
            let f = parse_float_lit(s);
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

/// M-F.3.6 ADR-0050f: upgrade `Operand::Move(p)` → `Operand::Copy(p)` when
/// `p`'s declared type is `Ty::Str` and the call is to a file-IO PRELUDE fn
/// that borrows its str arguments (reads the pointer without freeing it).
///
/// This mirrors the Phase 2a walk-back for `Ty::List` (see `is_copy_type`
/// doc comment): List is Copy-at-operand so `list_set(xs, i, v)` doesn't
/// consume `xs`. File-IO fns adopt the same convention for their str args:
/// the C-ABI shim reads via `str_buf_as_str_phase3` without freeing; the
/// drop schedule handles freeing at the caller's scope exit.
///
/// Constant operands (string literals etc.) are returned unchanged.
fn upgrade_move_to_copy_for_str(b: &BodyBuilder<'_>, op: Operand) -> Operand {
    match op {
        Operand::Move(ref p) => {
            // Look up the declared type of the local.
            if let Some(decl) = b.locals.get(p.local.0 as usize) {
                if matches!(decl.ty, Ty::Str) {
                    return Operand::Copy(p.clone());
                }
            }
            op
        }
        other => other,
    }
}

fn is_copy_type(ty: &Ty) -> bool {
    // ADR-0050c TD-1 closure: Str is non-Copy at the operand-read level —
    // every `ExprKind::Name` reading a `Ty::Str` local produces
    // `Operand::Move(s)`, transferring ownership at MIR time.
    //
    // List is treated as Copy at the OPERAND level (so existing PRELUDE
    // helpers like `list_set(xs, i, v)` + `list_len(xs)` continue to
    // pass `xs` by shared-reference) but as non-Copy at the DROP level
    // (so the drop pass enumerates list-typed locals as drop-eligible
    // and the codegen `Terminator::Drop` arm calls
    // `__cobrust_list_drop_elems` for `list[str]`). This split mirrors
    // Rust's `Copy` vs `Drop` separation: a type can be `!Copy` (must
    // be moved or borrowed) AND `Drop` (frees resources), but here we
    // weaken the operand-level discipline for List so that read-only
    // borrow patterns work without explicit `&` syntax (which Cobrust
    // does not yet surface). Phase G consolidation will introduce
    // explicit borrow forms and bring List to the same operand-level
    // non-Copy semantics Str has today.
    //
    // f3ls22 (use-after-move on `list[str]`) is documented as
    // honest-debt under this split — the language detects use-after-
    // move on Str but not on List at Phase F.3.
    // ADR-0050d sub-sprint c+d closure — Dict joins List in the
    // operand-level-Copy walk-back. Without this, `let d: Dict[..] =
    // {...}; d[1] = 10; d[1]` triggers UseAfterMove on `d` (since the
    // first `d` read moves the local, leaving the second read invalid).
    // Same rationale as the List walk-back: dict-typed args / reads
    // are conceptually a shared borrow at the source surface; the
    // drop pass still enumerates dict locals as drop-eligible (via
    // `is_copy` in drop.rs).
    matches!(
        ty,
        Ty::Bool
            | Ty::Int
            | Ty::Float
            | Ty::Imag
            | Ty::None
            | Ty::Never
            | Ty::List(_)
            | Ty::Dict(_, _)
            // ADR-0052a Wave-1 §7 — `&T` is operand-level Copy. A
            // borrow is a shared read; reading the local that holds
            // a `Ref(T)` value (e.g. the rebound `s` in `let s = &s`)
            // emits `Operand::Copy`, not `Operand::Move`, so the
            // borrow stays valid across multiple use sites.
            | Ty::Ref(_)
    )
}

/// ADR-0050c Phase 2 cascade fix: upgrade `Operand::Copy(p)` to
/// `Operand::Move(p)` when `p`'s local has a drop-eligible declared type
/// (i.e., the type would be enumerated by `drop::is_copy` as non-Copy).
///
/// This is called only from the `StmtKind::Return` lowering, where the
/// operand IS the function return value. Forcing a Move here:
///
/// 1. Marks the local as moved-out in the drop pass's `moved_out_per_block`,
///    so the local is excluded from the auto-inserted Drop chain on the
///    predecessor edge of the ret_block.
/// 2. Matches Rust's return-value-move (NRVO-friendly) semantics: the local
///    is consumed by the return; the caller owns the dropped value.
///
/// This preserves correctness regardless of the `is_copy_type` walk-back
/// for List (Phase 2a) — the walk-back keeps fn-arg patterns
/// (`list_set(xs, i, v)` reads `xs` as shared-borrow) working without
/// regressing return-value ownership transfer.
fn upgrade_return_to_move(b: &BodyBuilder<'_>, op: Operand) -> Operand {
    if let Operand::Copy(ref p) = op {
        if let Some(decl) = b.locals.get(p.local.0 as usize) {
            // Same drop-eligibility predicate as `drop::is_copy`:
            // every type NOT in the Copy set is drop-eligible.
            let is_drop_eligible = !matches!(
                &decl.ty,
                Ty::Bool | Ty::Int | Ty::Float | Ty::Imag | Ty::None | Ty::Never
            );
            if is_drop_eligible {
                return Operand::Move(p.clone());
            }
        }
    }
    op
}

fn synth_expr_ty(b: &BodyBuilder<'_>, e: &Expr) -> Ty {
    // ADR-0050c Phase 2 — TD-1 closure. We need the element type at
    // Aggregate(List) MIR build time so the codegen `Terminator::Drop`
    // arm can dispatch correctly (list[str] → __cobrust_list_drop_elems
    // with __cobrust_str_drop). The type checker has already validated
    // the element typing; here we synthesise the surface form.
    //
    // Coverage: this synth-time inference handles the cases the Phase F.3
    // corpus exercises (literals, name references, indexing, calls into
    // PRELUDE-stub fns). Unknown shapes still fall through to `Ty::None`
    // (matches the M8 conservative default; codegen treats this as
    // "non-Copy but un-droppable", a safe no-op leak — same as today
    // for unrecognised cases).
    match &e.kind {
        ExprKind::Lit(Lit::Bool(_)) => Ty::Bool,
        ExprKind::Lit(Lit::Int(_)) => Ty::Int,
        ExprKind::Lit(Lit::Float(_)) => Ty::Float,
        ExprKind::Lit(Lit::Imag(_)) => Ty::Imag,
        ExprKind::Lit(Lit::Str(_)) => Ty::Str,
        ExprKind::Lit(Lit::None) => Ty::None,
        ExprKind::Lit(Lit::Bytes(_)) => Ty::Bytes,
        ExprKind::Format(_) => Ty::Str,
        ExprKind::Name(rn) => b.ctx.lookup_ty(rn.def_id),
        ExprKind::List(items) => {
            let elem_ty = items.first().map_or(Ty::None, |it| synth_expr_ty(b, it));
            Ty::List(Box::new(elem_ty))
        }
        ExprKind::Index { base, .. } => {
            // For `xs[i]`, the result is the element type of xs.
            match synth_expr_ty(b, base) {
                Ty::List(elem) => *elem,
                Ty::Dict(_, v) => *v,
                Ty::Str => Ty::Str,
                other => other,
            }
        }
        ExprKind::Call { callee, .. } => {
            // M-F.3.6 ADR-0050f: synthesise the return type for fn-call
            // expressions so that `argv()[1]` etc. get the correct
            // element-type on the base when lowering the subscript.
            // Without this, `argv()` has synth-type Ty::None, causing the
            // list-index special path to be skipped and the projection
            // to fall back to the unsafe M12.x Projection::Index path.
            //
            // Strategy: if the callee is a Name whose def_id resolves to
            // a Fn type, return the return type of that Fn.
            if let ExprKind::Name(rn) = &callee.kind {
                let callee_ty = b.ctx.lookup_ty(rn.def_id);
                if let Ty::Fn(fn_ty) = callee_ty {
                    return (*fn_ty.return_ty).clone();
                }
            }
            Ty::None
        }
        _ => Ty::None,
    }
}

// Mark CastKind / NullaryOp as used to satisfy `dead_code`-on-strict
// builds without leaking workspace-wide allow.
#[allow(dead_code)]
fn _force_cast_kind_used(k: CastKind) -> CastKind {
    k
}

/// Parse a Cobrust float literal string → f64.
/// Handles standard decimal forms, `inf`, `nan`, and exponential notation.
/// `f64::from_str` in Rust does not accept "inf"/"nan" (case-insensitive
/// match against std strings), so we special-case them here.
fn parse_float_lit(s: &str) -> f64 {
    match s {
        "inf" => f64::INFINITY,
        "nan" => f64::NAN,
        other => other.parse::<f64>().unwrap_or(0.0),
    }
}

/// Conservative check: is this HIR expression likely float-typed?
/// Used by `lower_bin` to skip the integer div-by-zero assertion for float
/// operands (IEEE 754 defines float/0.0 = ±inf, not a trap — f64e21).
/// Returns true when we can statically determine the expression is f64.
fn hir_expr_is_float(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Lit(Lit::Float(_)) => true,
        ExprKind::Bin { lhs, rhs, .. } => hir_expr_is_float(lhs) || hir_expr_is_float(rhs),
        ExprKind::Un { operand, .. } => hir_expr_is_float(operand),
        ExprKind::Cast { target, .. } => {
            let name = match &target.kind {
                cobrust_frontend::ast::TypeKind::Name(parts) => parts.join("."),
                _ => String::new(),
            };
            matches!(name.as_str(), "f64" | "float")
        }
        _ => false,
    }
}

#[allow(dead_code)]
fn _force_borrow_kind_used(k: BorrowKind) -> BorrowKind {
    k
}
