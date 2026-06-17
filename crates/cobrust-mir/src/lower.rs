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
    DefId, DefKind, DictEntry, Expr, ExprKind, FnBody, FormatPart, IndexKind, Item, ItemKind,
    LetBody, Lit, LoopKind, MatchArm, Module as HirModule, Pattern, PatternKind, ResolvedName,
    Stmt, StmtKind, UnaryOp,
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
    /// ADR-0052d-prereq §"Decision" — method-form rewrite map.
    /// Maps a top-level fn name (PRELUDE-fn + user-declared) to its
    /// `DefId`. Populated at construction by walking `typed.hir.items`.
    /// The method-form lowering at `lower_call` consults this map to
    /// resolve the PRELUDE-fn target's `FnRef` for emitting a direct
    /// MIR Call (`s.len()` → `str_len(s)` → `Constant::FnRef(def_id)`).
    fn_name_to_def_id: HashMap<String, DefId>,
}

impl<'a> LowerCtx<'a> {
    fn new(typed: &'a TypedModule) -> Self {
        let def_ty = typed.def_types.clone();
        // Walk top-level items to build name → DefId for fns. Used by
        // method-form lowering at `lower_call` per ADR-0052d-prereq.
        let mut fn_name_to_def_id = HashMap::new();
        Self::collect_fn_names(&typed.hir.items, &mut fn_name_to_def_id);
        Self {
            typed,
            def_ty,
            fn_name_to_def_id,
        }
    }

    /// Recursive helper for collecting top-level fn name → DefId,
    /// including fns inside decorated items + class members.
    fn collect_fn_names(items: &[Item], map: &mut HashMap<String, DefId>) {
        for item in items {
            match &item.kind {
                ItemKind::Fn(f) => {
                    // First-wins (PRELUDE before user) for safety; M2
                    // user-shadowing of PRELUDE names is not Wave-2 scope.
                    map.entry(f.name.clone()).or_insert(f.def_id);
                }
                ItemKind::Class(c) => {
                    Self::collect_fn_names(&c.members, map);
                }
                ItemKind::Decorated { inner, .. } => {
                    Self::collect_fn_names(std::slice::from_ref(inner.as_ref()), map);
                }
                _ => {}
            }
        }
    }

    /// Look up the resolved type of a `DefId`.
    fn lookup_ty(&self, def_id: DefId) -> Ty {
        self.def_ty.get(&def_id.0).cloned().unwrap_or(Ty::None) // defense in depth — we expect it
    }

    /// ADR-0052d-prereq §"Decision" — resolve a PRELUDE-fn / user-fn
    /// name to its `DefId` for method-form rewrite at `lower_call`.
    fn lookup_fn_def_id(&self, name: &str) -> Option<DefId> {
        self.fn_name_to_def_id.get(name).copied()
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
        // ADR-0081 Phase-1b — the Q4 MARK. If THIS fn's `DefId` was recorded
        // by the checker as a `route_validated`-registered handler
        // (`TypedModule.validated_handlers`), mark its body-param local
        // `validated_body_of = Some(body_adt)`. `body_param_idx` is the
        // handler `FnTy`'s positional index of the validated-body slot (1 for
        // `fn(pit.Request, body: Body)`), which lines up with
        // `f.params.positional[idx]` (the checker counts the same positionals
        // — there is no implicit receiver on a top-level fn). This is the ONLY
        // local that gets the mark; every other local (incl. a non-registered
        // fn's `b: Body` param and a `let s = Body()` binding) keeps the
        // `declare_local` default `None` — the no-UB invariant (§5.2).
        if let Some(&(body_param_idx, body_adt)) = self.typed.validated_handlers.get(&f.def_id) {
            if let Some(param) = f.params.positional.get(body_param_idx) {
                if let Some(&local_id) = b.def_to_local.get(&param.def_id.0) {
                    if let Some(decl) = b.locals.get_mut(local_id.0 as usize) {
                        decl.validated_body_of = Some(body_adt);
                    }
                }
            }
        }
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
    /// Stack of `(continue_target_block, exit_block)` for `continue` / `break`.
    ///
    /// F89: `continue` must goto the enclosing loop's *continue target*, NOT
    /// the loop header directly. For a `while` loop the continue target IS the
    /// header (the user hand-writes any induction-var bump inside the body).
    /// For a `for` loop the continue target is a dedicated *increment latch*
    /// block that performs `__idx += 1` then `Goto(header)` — so the induction
    /// variable advances on EVERY path that re-enters the loop (body
    /// fall-through AND `continue` alike). Pointing `continue` at the header
    /// instead bypasses the increment and spins forever (F89 hang).
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
            // ADR-0081 Phase-1b — default: NOT a validated body. `lower_fn`
            // overwrites this to `Some(body_adt)` for the registered
            // handler's body-param local only (the Q4 mark).
            validated_body_of: None,
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
                suggestion: None,
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
                // F89: goto the innermost loop's CONTINUE TARGET, not the
                // header. For `while` this is the header; for `for` it is the
                // increment latch that bumps `__idx` before re-entering the
                // header (so `continue` does not bypass the induction-var
                // increment and infinite-loop).
                if let Some((continue_target, _)) = self.loop_stack.last().copied() {
                    self.ensure_open_block();
                    self.terminate(Terminator::Goto(continue_target));
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
            // ADR-0077 Q2 write-path (Phase 2a) — `a[i] = v` on a
            // `coil.Buffer`. Retarget to `__cobrust_coil_buffer_setitem(a,
            // i, v) -> ()`, the sibling of the Dict `d[k] = v` branch
            // below. The base handle is BORROWED (Move → Copy upgrade) so
            // the source local survives + drops once at scope exit; the
            // shim borrows `&mut Array` and writes `v` in place (sound —
            // the `.cb` scope owns the only handle to the box, ADR-0077 §4
            // / ADR-0072 Q4). NOT the legacy `Place::Index` projection
            // (lower_lvalue), which is a Wave-1 no-op on an opaque handle
            // pointer (the write would be silently dropped + the read-back
            // segfaults — the HEAD RED state). Bounds are invisible to the
            // type, so an out-of-bounds index traps at runtime in the shim
            // (ADR-0077 Q4 panic-on-violation), NOT here.
            if matches!(&base_ty, Ty::Adt(id, _) if *id == cobrust_types::COIL_BUFFER_ADT) {
                let base_op = upgrade_move_to_copy_handle(self.lower_expr(base)?);
                let idx_op = self.lower_index(index)?;
                let val_op = self.lower_expr(value)?;
                let scratch = self.declare_local("_coilset".to_string(), Ty::None, span, false);
                let cur = self.current_block_id();
                let next = self.start_new_block();
                self.cur_block = Some(cur.0 as usize);
                self.terminate(Terminator::Call {
                    func: Operand::Constant(Constant::Str(
                        cobrust_types::coil_buffer_setitem_symbol().to_string(),
                    )),
                    args: vec![base_op, idx_op, val_op],
                    destination: Place::local(scratch),
                    target: next,
                    unwind: None,
                });
                self.cur_block = Some(next.0 as usize);
                return Ok(());
            }
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

    /// ADR-0081 Phase-1b — resolve the BASE of a `body.field` read to a
    /// `(body-Value, body-class)` pair, gated on the Q4 registration MARK
    /// (§5.2). ADR-0081 **Phase-2 (nested)** makes this RECURSIVE so a
    /// `body.inner.x` chain resolves `body.inner` to the nested object +
    /// `Inner` class, then reads `x` off it.
    ///
    /// Returns `Some((value_op, body_adt))` where `value_op` is the operand
    /// holding the boxed (root) OR borrowed-interior (nested) `serde_json::
    /// Value`, and `body_adt` is the class whose `adt_fields` the FIELD must
    /// then be looked up in. Two recursive arms:
    ///
    /// 1. `base` is a bare `ExprKind::Name` whose local carries
    ///    `validated_body_of == Some(body_adt)` (the registration mark, set
    ///    in `lower_fn` for the handler's body param). The operand is the
    ///    lowered param read (the boxed root Value).
    /// 2. `base` is itself `Attr { inner_base, field }` where THIS resolver
    ///    succeeds on `inner_base` (yielding `Some((_, outer_adt))`) AND
    ///    `field` is a NESTED-class field of `outer_adt`
    ///    (`adt_fields[outer_adt][field] == Ty::Adt(nested_adt, _)`). It
    ///    emits the nested accessor (`__cobrust_pit_body_get_nested`) and
    ///    returns `Some((nested_value_op, nested_adt))` — so a further
    ///    `.field` recurses again.
    ///
    /// Returns `None` otherwise — for an UNMARKED local (a non-registered
    /// fn's body-shaped param, a `let s = Body()` binding), a base whose
    /// chain does not bottom out at a marked param, or a non-class
    /// intermediate field. The caller then takes the pre-existing `Field(0)`
    /// stub path — NEVER a serde cast (the no-UB invariant).
    ///
    /// `&mut self`: the nested arm declares an `_ecoret` temp + emits a Call
    /// (the recursion's machinery). The root arm declares nothing new.
    fn resolve_validated_body_base(
        &mut self,
        base: &Expr,
    ) -> Result<Option<(Operand, cobrust_types::AdtId)>, MirError> {
        match &base.kind {
            // Arm 1 (recursion base case) — a marked `body` param read.
            ExprKind::Name(rn) => {
                let Some(local_id) = self.def_to_local.get(&rn.def_id.0).copied() else {
                    return Ok(None);
                };
                let Some(decl) = self.locals.get(local_id.0 as usize) else {
                    return Ok(None);
                };
                // The Q4 GATE: the local must carry the registration mark. An
                // unmarked local (Ty::Adt-with-fields but `None` here) is
                // NEVER serde-cast.
                let Some(body_adt) = decl.validated_body_of else {
                    return Ok(None);
                };
                let op = self.lower_expr(base)?;
                Ok(Some((op, body_adt)))
            }
            // Arm 2 (recursive step) — `outer.<nested-class-field>`. Resolve
            // `outer` first; if it is a validated body AND `field` is a
            // nested-class field of it, emit the nested accessor.
            ExprKind::Attr {
                base: inner_base,
                name: field,
            } => {
                let Some((outer_op, outer_adt)) = self.resolve_validated_body_base(inner_base)?
                else {
                    return Ok(None);
                };
                // `field` must be a declared field of `outer_adt`, typed as
                // ANOTHER field-tracked validated class. (A scalar field here
                // is NOT a valid base for a further `.field` — the caller's
                // outer arm handles the terminal scalar read directly.)
                let Some(field_ty) = self
                    .ctx
                    .typed
                    .adt_fields
                    .get(&outer_adt)
                    .and_then(|fields| fields.get(field))
                else {
                    return Ok(None);
                };
                let cobrust_types::Ty::Adt(nested_adt, _) = field_ty else {
                    return Ok(None);
                };
                let nested_adt = *nested_adt;
                // The nested accessor returns the BORROWED interior Value for
                // the nested object. Pick it via the field's `Ty::Adt` (the
                // `__cobrust_pit_body_get_nested` arm of
                // `lookup_validated_body_accessor`). Borrowed receiver
                // (Move → Copy — the body box is freed once by the
                // trampoline); the field name is the compiler-synthesised Str.
                let Some(accessor) = cobrust_types::lookup_validated_body_accessor(field_ty) else {
                    return Ok(None);
                };
                let recv_op = upgrade_move_to_copy_handle(outer_op);
                let name_op = Operand::Constant(Constant::Str(field.clone()));
                let nested_op = self.emit_ecosystem_call(
                    accessor.runtime_symbol,
                    accessor.ret.clone(),
                    vec![recv_op, name_op],
                    base.span,
                );
                // Re-mark the `_ecoret` result temp `validated_body_of =
                // Some(nested_adt)` so a FURTHER `.field` on it (the next
                // recursion level, or the caller's terminal scalar read) fires
                // the registration-gated path again. The temp was declared by
                // `emit_ecosystem_call` and `nested_op` is a `Move`/`Copy` of
                // it; recover its `LocalId` to set the mark.
                if let Operand::Move(place) | Operand::Copy(place) = &nested_op {
                    if let Some(slot) = self.locals.get_mut(place.local.0 as usize) {
                        slot.validated_body_of = Some(nested_adt);
                    }
                }
                Ok(Some((nested_op, nested_adt)))
            }
            _ => Ok(None),
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

                // F88 / ADR-0101 — `for c in <str>:` codepoint iteration.
                // When the iter source is a `str`, the loop is bound by the
                // CODEPOINT count (`__cobrust_str_char_count`, NOT byte len)
                // and each iteration mints a fresh 1-codepoint OWNED `str`
                // via `__cobrust_str_char_at(__iter, __idx)` (the same
                // codepoint-addressed primitive `s[i]` uses, F79/ADR-0094).
                // The source `str` is BORROWED (read-only via char_at); its
                // own drop schedule frees it once at its scope. Everything
                // else mirrors the list arm 1:1.
                let iter_is_str = matches!(synth_expr_ty(self, iter), Ty::Str);

                // Step 1: evaluate iter expression → iter_local.
                //
                // F88 — a `str` iter source is BORROWED, not consumed:
                // `__cobrust_str_char_at` only READS it. If the iter is a
                // bare `Name` (`for c in s:`), read it as `Operand::Copy`
                // (NOT the default `Move` that `is_copy_type(Ty::Str) ==
                // false` would produce via `lower_expr`) so the source `s`
                // stays usable AFTER the loop — mirroring the list/dict
                // operand-level shared-borrow walk-back. A non-`Name` str
                // iter (literal / call result) is a fresh temp the loop
                // owns, so the default `lower_expr` move is correct there.
                let iter_val_op = if iter_is_str {
                    if let ExprKind::Name(rn) = &iter.kind {
                        let local = self.lookup_local_for_resolved(rn, iter.span)?;
                        Operand::Copy(Place::local(local))
                    } else {
                        self.lower_expr(iter)?
                    }
                } else {
                    self.lower_expr(iter)?
                };
                let iter_local = self.declare_local("_iter".to_string(), Ty::None, span, true);
                self.emit_assign(Place::local(iter_local), Rvalue::Use(iter_val_op), span);

                // Step 2: call the length primitive (str: codepoint count;
                // list: element count) → len_local.
                let len_symbol = if iter_is_str {
                    "__cobrust_str_char_count"
                } else {
                    "__cobrust_list_len"
                };
                let len_local = self.declare_local("_iter_len".to_string(), Ty::Int, span, true);
                let cur = self.current_block_id();
                let after_len = self.start_new_block();
                self.cur_block = Some(cur.0 as usize);
                self.terminate(Terminator::Call {
                    func: Operand::Constant(Constant::Str(len_symbol.to_string())),
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

                // F89: the increment LATCH. This is the loop's `continue`
                // target. It bumps `__idx` by 1 then re-enters `header`. BOTH
                // the body fall-through and any `continue` route through here,
                // so the induction variable advances on every re-entry path.
                // (Before F89 the increment lived only in the body
                // fall-through and `continue` gotoed `header` directly,
                // bypassing it → infinite loop.)
                let latch_block = self.start_new_block();
                self.cur_block = Some(latch_block.0 as usize);
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
                // F89: `continue` target is the increment latch, NOT `header`.
                self.loop_stack.push((latch_block, exit_block));
                self.cur_block = Some(body_block.0 as usize);
                if let Some(vl) = var_local {
                    let vl_ty = self
                        .locals
                        .get(vl.0 as usize)
                        .map(|d| d.ty.clone())
                        .unwrap_or(Ty::None);
                    if iter_is_str {
                        // F88 — `for c in <str>:`: fetch the __idx-th CODEPOINT
                        // as a FRESH owned 1-codepoint str directly into `vl`
                        // (no list_get→clone two-step; char_at already mints a
                        // fresh owned handle). `vl: Ty::Str` is drop-eligible →
                        // dropped each iteration by the scope drop schedule
                        // (per-iter LEAK under the pre-existing F82 loop-body
                        // drop gap; this arm guarantees NO double-free — the
                        // source `__iter` is only READ via char_at, never
                        // consumed, and `vl` owns its own fresh copy).
                        let after_get = self.start_new_block();
                        self.cur_block = Some(body_block.0 as usize);
                        self.terminate(Terminator::Call {
                            func: Operand::Constant(Constant::Str(
                                "__cobrust_str_char_at".to_string(),
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
                    } else if matches!(vl_ty, Ty::Str) {
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
                    // F89: body fall-through routes through the increment
                    // latch (which bumps `__idx` then gotos `header`), the
                    // SAME target as `continue`.
                    self.terminate(Terminator::Goto(latch_block));
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
                // ADR-0077 Q3 — `coil.Buffer` parens-free attribute access
                // (`a.shape` / `a.ndim` / `a.size`). When the base resolves
                // to a handle with a manifest attribute, retarget to the
                // runtime symbol via `emit_ecosystem_call` (BORROWED
                // receiver, Move → Copy upgrade — the handle drops once at
                // scope exit). `shape` returns an owned `list[i64]` (the
                // `_ecoret` local carries `Ty::List(Int)` so the drop pass
                // schedules the list-drop); `ndim`/`size` return by-value
                // `i64`. Falls through to the `Projection::Field(0)`
                // placeholder for non-handle bases.
                let base_ty = synth_expr_ty(self, base);
                if let Some(sig) = cobrust_types::lookup_handle_attr(&base_ty, name) {
                    let recv_op = upgrade_move_to_copy_handle(self.lower_expr(base)?);
                    let op_out = self.emit_ecosystem_call(
                        sig.runtime_symbol,
                        sig.ret.clone(),
                        vec![recv_op],
                        e.span,
                    );
                    return Ok(op_out);
                }
                // ADR-0081 Phase-1b — the REGISTRATION-GATED validated-body
                // field READ (the Q4 gate, §5.2). Fires ONLY when the base is
                // a `Name` resolving to a local MARKED
                // `validated_body_of == Some(body_adt)` (i.e. a
                // `route_validated`-registered handler's body param, recorded
                // by the checker + marked in `lower_fn`) AND `name` is a field
                // in that class's `adt_fields`. The base is then the boxed
                // `serde_json::Value` the validator left (`cabi.rs`), so the
                // typed accessor shim (`__cobrust_pit_body_get_*`, picked by
                // the field's DECLARED `Ty`) reads it safely.
                //
                // CRITICAL — this gates on the MARK, NOT on the `Ty`. A
                // non-registered fn's `b: Body` param and a `let s = Body()`
                // binding have the SAME `Ty::Adt(body_adt)` + the SAME field
                // table, but `validated_body_of == None`, so they NEVER reach
                // this arm — they fall through to the pre-existing
                // `Field(0)` stub below and are NEVER `cast::<Value>()`-ed
                // (the no-UB invariant). Gating on `Ty::Adt`-with-fields is
                // the UB bug this design forbids.
                //
                // ADR-0081 **Phase-2** widens the field-`Ty` reach (f64 /
                // bool) and makes the BASE resolution RECURSIVE (nested
                // bodies): `resolve_validated_body_base` walks a `body.inner`
                // chain down to the marked param, emitting a
                // `__cobrust_pit_body_get_nested` borrow at each nested hop +
                // re-marking the result temp, so `body.inner.x` reaches HERE
                // with `base` resolved to `(nested-Value, Inner)` and `name`
                // (`x`) read off it by the field's declared `Ty`.
                if let Some((base_value_op, body_adt)) = self.resolve_validated_body_base(base)? {
                    if let Some(field_ty) = self
                        .ctx
                        .typed
                        .adt_fields
                        .get(&body_adt)
                        .and_then(|fields| fields.get(name))
                    {
                        if let Some(accessor) =
                            cobrust_types::lookup_validated_body_accessor(field_ty)
                        {
                            // Borrowed receiver (Move → Copy, the
                            // `coil.Buffer.shape` discipline): the shim reads
                            // `&serde_json::Value`; the body box stays live +
                            // is freed exactly once by the `route_validated`
                            // trampoline (`cabi.rs:530`). The field name is the
                            // COMPILER-SYNTHESISED `Str` (footgun #1 — never
                            // author-written), passed as the 2nd arg.
                            let recv_op = upgrade_move_to_copy_handle(base_value_op);
                            let name_op = Operand::Constant(Constant::Str(name.clone()));
                            let op_out = self.emit_ecosystem_call(
                                accessor.runtime_symbol,
                                accessor.ret.clone(),
                                vec![recv_op, name_op],
                                e.span,
                            );
                            // If THIS read is itself a nested-class field
                            // (e.g. the `body.inner` in `body.inner.x`,
                            // reached directly rather than via the recursive
                            // base resolver — the terminal `.field` arm), mark
                            // its result temp so a further `.field` recurses.
                            if let cobrust_types::Ty::Adt(nested_adt, _) = field_ty {
                                if let Operand::Move(place) | Operand::Copy(place) = &op_out {
                                    if let Some(slot) = self.locals.get_mut(place.local.0 as usize)
                                    {
                                        slot.validated_body_of = Some(*nested_adt);
                                    }
                                }
                            }
                            return Ok(op_out);
                        }
                    }
                }
                // ADR-0083 — `math` module CONSTANT (`math.pi`, `math.e`,
                // `math.tau`, `math.inf`, `math.nan`): a parens-free attribute
                // on an ecosystem-module import alias. Emit the value as a
                // pure compile-time `Constant::Float` (bits via `f64::to_bits`,
                // the `lit_to_constant` Float convention) — NO runtime call (a
                // constant is just a number). The type checker already typed
                // this as `Ty::Float`; `synth_expr_ty`'s Attr arm agrees, so a
                // `let p: f64 = math.pi` drop schedule + a `print(math.pi)`
                // float-dispatch both see the right type.
                if let ExprKind::Name(rn) = &base.kind {
                    if rn.kind == DefKind::ImportAlias {
                        if let Some(v) = cobrust_types::lookup_module_const(rn.name.as_str(), name)
                        {
                            return Ok(Operand::Constant(Constant::Float(v.to_bits())));
                        }
                    }
                }
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
                    let elem_ty = if let Ty::List(elem) = &base_ty {
                        (**elem).clone()
                    } else {
                        Ty::None
                    };
                    // F81 / ADR-0096 — `xs[lo:hi]` slice on a `list` base
                    // yields a FRESH OWNED `list[T]` (the
                    // `__cobrust_str_slice`/`__cobrust_bytes_slice` mirror,
                    // ELEMENT-addressed). The base handle is BORROWED (Move →
                    // Copy upgrade) so the source local survives + drops once
                    // at scope exit. The `lo`/`hi` bounds are lowered DIRECTLY
                    // here (the generic `lower_index` collapses a Slice to
                    // `Constant::Int(0)` — the F81 BUG-2 UB stub that returned
                    // an integer 0 used as a list handle → `misaligned pointer
                    // dereference`). Only the contiguous `lo:hi` form (both
                    // bounds present, default step, non-negative) is supported;
                    // step / open-ended / negative shapes are REJECTED upstream
                    // by `TypeError::UnsupportedSliceShape`, so MIR never sees
                    // one on the accepted-program path. The `else` arm is
                    // DEFENSE-IN-DEPTH (constitution §6): an unsupported shape
                    // reaching here emits a hard `MirError`, NEVER the silent
                    // `Constant::Int(0)` UB fall-through this replaces.
                    if let IndexKind::Slice { start, stop, step } = index.as_ref() {
                        if let (None, Some(lo_e), Some(hi_e)) =
                            (step.as_ref(), start.as_ref(), stop.as_ref())
                        {
                            let base_op = upgrade_move_to_copy_handle(self.lower_expr(base)?);
                            let lo_op = self.lower_expr(lo_e)?;
                            let hi_op = self.lower_expr(hi_e)?;
                            let dest = self.declare_local(
                                "_listslice".to_string(),
                                Ty::List(Box::new(elem_ty.clone())),
                                e.span,
                                true,
                            );
                            let cur = self.current_block_id();
                            let next = self.start_new_block();
                            self.cur_block = Some(cur.0 as usize);
                            self.terminate(Terminator::Call {
                                func: Operand::Constant(Constant::Str(
                                    "__cobrust_list_slice".to_string(),
                                )),
                                args: vec![base_op, lo_op, hi_op],
                                destination: Place::local(dest),
                                target: next,
                                unwind: None,
                            });
                            self.cur_block = Some(next.0 as usize);
                            // `_listslice` owns a FRESHLY-allocated `list`
                            // (the slice shim's return). It MUST be MOVED out
                            // so the single owner (the consuming binding) drops
                            // it ONCE via `__cobrust_list_drop`; a Copy would
                            // leave BOTH `_listslice` + the consuming local in
                            // the drop schedule → a double-free. (Mirror of the
                            // `str`/`bytes` slice Move-out discipline.)
                            return Ok(Operand::Move(Place::local(dest)));
                        }
                        // DEFENSE-IN-DEPTH — an unsupported `list` slice shape
                        // (open-ended / stepped / negative) should have been
                        // rejected by `TypeError::UnsupportedSliceShape`
                        // upstream. Reaching here means the type checker was
                        // bypassed; a hard `MirError` is the §2.2-sound
                        // backstop (NEVER the silent `Constant::Int(0)` UB
                        // fall-through F81 BUG-2 documents).
                        return Err(MirError::Internal(format!(
                            "unsupported `list` slice shape reached MIR lowering at \
                             {:?} (only `xs[lo:hi]` with both non-negative bounds + \
                             default step is supported; this must be rejected by \
                             TypeError::UnsupportedSliceShape upstream)",
                            e.span
                        )));
                    }
                    let base_op = self.lower_expr(base)?;
                    let idx_op = self.lower_index(index)?;
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
                // ADR-0093 — `bytes` index read, beside the
                // Dict/List/Buffer arms. The base `bytes` handle is BORROWED
                // (Move → Copy upgrade) so the source local survives + drops
                // ONCE at scope exit (mirrors the coil.Buffer arm below +
                // ADR-0072 §5 risk 1). NOT the `Projection::Index`
                // fall-through below (a Wave-1 no-op stub that would mis-type
                // on an opaque bytes handle). Two index shapes dispatch here
                // (the coil.Buffer split, mirrored):
                //
                //   - SCALAR `b[i]` (`IndexKind::Expr`, Phase 1) →
                //     `__cobrust_bytes_get(b, i) -> i64` (the i-th byte as a
                //     0..255 int; CPython `b"abc"[0] == 97`). The result is a
                //     Copy SCALAR (`Operand::Copy`), so — unlike
                //     `list[str][i]` — no `__cobrust_*_clone` is emitted.
                //   - SLICE `b[lo:hi]` (`IndexKind::Slice`, Phase 2) →
                //     `__cobrust_bytes_slice(b, lo, hi) -> bytes` (a fresh
                //     OWNED `bytes` the `.cb` scope drops once — Python clamp
                //     on OOB, NOT abort). The `lo`/`hi` bounds are lowered
                //     DIRECTLY here (the generic `lower_index` collapses a
                //     Slice to `Constant::Int(0)`, so the slice arm must read
                //     the bounds itself — exactly the coil.Buffer slice
                //     pattern). Phase 2 is the contiguous `lo:hi` form (both
                //     bounds present, default step); step / open-ended /
                //     negative bounds are ADR-0093 §Phasing deferrals.
                //
                //     ADR-0093 Phase-2 §"Slice-shape soundness" — an
                //     unsupported slice shape is REJECTED at the type checker
                //     (`TypeError::UnsupportedSliceShape`), so MIR never sees
                //     one on the accepted-program path. The `else` arm below
                //     is DEFENSE-IN-DEPTH (constitution §6): if a non-`lo:hi`
                //     shape ever reaches here it emits a hard `MirError`
                //     rather than falling through to the generic index no-op,
                //     which would SILENTLY evaluate to the WHOLE base buffer
                //     (the §2.2 silent-miscompile this replaces). NEVER a
                //     silent fallthrough.
                if matches!(&base_ty, Ty::Bytes) {
                    if let IndexKind::Slice { start, stop, step } = index.as_ref() {
                        if let (None, Some(lo_e), Some(hi_e)) =
                            (step.as_ref(), start.as_ref(), stop.as_ref())
                        {
                            let base_op = upgrade_move_to_copy_handle(self.lower_expr(base)?);
                            let lo_op = self.lower_expr(lo_e)?;
                            let hi_op = self.lower_expr(hi_e)?;
                            let dest = self.declare_local(
                                "_bytesslice".to_string(),
                                Ty::Bytes,
                                e.span,
                                true,
                            );
                            let cur = self.current_block_id();
                            let next = self.start_new_block();
                            self.cur_block = Some(cur.0 as usize);
                            self.terminate(Terminator::Call {
                                func: Operand::Constant(Constant::Str(
                                    "__cobrust_bytes_slice".to_string(),
                                )),
                                args: vec![base_op, lo_op, hi_op],
                                destination: Place::local(dest),
                                target: next,
                                unwind: None,
                            });
                            self.cur_block = Some(next.0 as usize);
                            // `_bytesslice` owns a FRESHLY-allocated
                            // `bytes` buffer (the slice shim's return).
                            // It MUST be MOVED out so the single owner
                            // (the consuming binding) drops it ONCE; a
                            // Copy here would leave BOTH `_bytesslice` +
                            // the consuming local in the drop schedule →
                            // a double-free. (Mirror of the coil slice +
                            // str_concat Move-out discipline.)
                            return Ok(Operand::Move(Place::local(dest)));
                        }
                        // DEFENSE-IN-DEPTH — an unsupported `bytes` slice
                        // shape (open-ended / stepped / negative) should have
                        // been rejected by `TypeError::UnsupportedSliceShape`
                        // upstream. Reaching here means the type checker was
                        // bypassed; a hard `MirError` is the §2.2-sound
                        // backstop (NEVER the silent whole-buffer fallthrough
                        // this replaces).
                        return Err(MirError::Internal(format!(
                            "unsupported `bytes` slice shape reached MIR lowering at \
                             {:?} (only `b[lo:hi]` with both non-negative bounds + \
                             default step is supported; this must be rejected by \
                             TypeError::UnsupportedSliceShape upstream)",
                            e.span
                        )));
                    } else {
                        let base_op = upgrade_move_to_copy_handle(self.lower_expr(base)?);
                        let idx_op = self.lower_index(index)?;
                        let dest =
                            self.declare_local("_bytesidx".to_string(), Ty::Int, e.span, false);
                        let cur = self.current_block_id();
                        let next = self.start_new_block();
                        self.cur_block = Some(cur.0 as usize);
                        self.terminate(Terminator::Call {
                            func: Operand::Constant(Constant::Str(
                                "__cobrust_bytes_get".to_string(),
                            )),
                            args: vec![base_op, idx_op],
                            destination: Place::local(dest),
                            target: next,
                            unwind: None,
                        });
                        self.cur_block = Some(next.0 as usize);
                        return Ok(Operand::Copy(Place::local(dest)));
                    }
                }
                // ADR-0094 / F78 — `str` index read, beside the
                // Dict/List/Bytes arms. The base `str` handle is BORROWED
                // (Move → Copy upgrade) so the source local survives + drops
                // ONCE at scope exit (mirrors the `bytes` arm above). NOT the
                // `Projection::Index` fall-through below — that no-op stub was
                // the F78 §2.2 SILENT-MISCOMPILE: a `str` index collapsed to
                // the WHOLE base string (`"hello"[1:4]` printed `hello`, not
                // `ell`). Two index shapes dispatch here (the `bytes` split,
                // mirrored — but CODEPOINT-addressed, since `str` indexes by
                // Unicode scalar where `bytes` indexes by byte):
                //
                //   - SCALAR `s[i]` (`IndexKind::Expr`) →
                //     `__cobrust_str_char_at(s, i) -> str` (the i-th CODEPOINT
                //     as a fresh 1-codepoint `str`; CPython `"héllo"[1] ==
                //     "é"`, a `str` NOT a byte). The result is a FRESH OWNED
                //     `str` (Move-out, dropped once) — UNLIKE the `bytes`
                //     scalar (`b[i] -> int`, a Copy scalar).
                //   - SLICE `s[lo:hi]` (`IndexKind::Slice`) →
                //     `__cobrust_str_slice(s, lo, hi) -> str` (a fresh OWNED
                //     `str` the `.cb` scope drops once — codepoint range
                //     `[lo, hi)`, Python clamp on OOB, NEVER a mid-codepoint
                //     cut). The `lo`/`hi` bounds are lowered DIRECTLY here
                //     (the generic `lower_index` collapses a Slice to
                //     `Constant::Int(0)`, so the slice arm must read the
                //     bounds itself — the `bytes`/coil.Buffer slice pattern).
                //     Only the contiguous `lo:hi` form (both bounds present,
                //     default step, non-negative) is supported; step /
                //     open-ended / negative shapes are REJECTED upstream by
                //     `TypeError::UnsupportedSliceShape` (ADR-0094 extends the
                //     `bytes` reject to `Ty::Str`), so MIR never sees one on
                //     the accepted-program path. The `else` arm is
                //     DEFENSE-IN-DEPTH: an unsupported shape reaching here
                //     emits a hard `MirError` rather than the silent
                //     whole-string fall-through F78 documents.
                if matches!(&base_ty, Ty::Str) {
                    if let IndexKind::Slice { start, stop, step } = index.as_ref() {
                        if let (None, Some(lo_e), Some(hi_e)) =
                            (step.as_ref(), start.as_ref(), stop.as_ref())
                        {
                            let base_op = upgrade_move_to_copy_handle(self.lower_expr(base)?);
                            let lo_op = self.lower_expr(lo_e)?;
                            let hi_op = self.lower_expr(hi_e)?;
                            let dest =
                                self.declare_local("_strslice".to_string(), Ty::Str, e.span, true);
                            let cur = self.current_block_id();
                            let next = self.start_new_block();
                            self.cur_block = Some(cur.0 as usize);
                            self.terminate(Terminator::Call {
                                func: Operand::Constant(Constant::Str(
                                    "__cobrust_str_slice".to_string(),
                                )),
                                args: vec![base_op, lo_op, hi_op],
                                destination: Place::local(dest),
                                target: next,
                                unwind: None,
                            });
                            self.cur_block = Some(next.0 as usize);
                            // `_strslice` owns a FRESHLY-allocated `str`
                            // buffer (the slice shim's return). It MUST be
                            // MOVED out so the single owner (the consuming
                            // binding) drops it ONCE; a Copy here would leave
                            // BOTH `_strslice` + the consuming local in the
                            // drop schedule → a double-free. (Mirror of the
                            // `bytes` slice + str_concat Move-out discipline.)
                            return Ok(Operand::Move(Place::local(dest)));
                        }
                        // DEFENSE-IN-DEPTH — an unsupported `str` slice shape
                        // (open-ended / stepped / negative) should have been
                        // rejected by `TypeError::UnsupportedSliceShape`
                        // upstream. Reaching here means the type checker was
                        // bypassed; a hard `MirError` is the §2.2-sound
                        // backstop (NEVER the silent whole-string fallthrough
                        // F78 documents).
                        return Err(MirError::Internal(format!(
                            "unsupported `str` slice shape reached MIR lowering at \
                             {:?} (only `s[lo:hi]` with both non-negative bounds + \
                             default step is supported; this must be rejected by \
                             TypeError::UnsupportedSliceShape upstream)",
                            e.span
                        )));
                    }
                    let base_op = upgrade_move_to_copy_handle(self.lower_expr(base)?);
                    let idx_op = self.lower_index(index)?;
                    let dest = self.declare_local("_stridx".to_string(), Ty::Str, e.span, true);
                    let cur = self.current_block_id();
                    let next = self.start_new_block();
                    self.cur_block = Some(cur.0 as usize);
                    self.terminate(Terminator::Call {
                        func: Operand::Constant(Constant::Str("__cobrust_str_char_at".to_string())),
                        args: vec![base_op, idx_op],
                        destination: Place::local(dest),
                        target: next,
                        unwind: None,
                    });
                    self.cur_block = Some(next.0 as usize);
                    // The scalar `s[i]` mints a FRESH 1-codepoint `str`
                    // (Move-out, dropped once) — like the slice, UNLIKE the
                    // `bytes` scalar (a Copy `int`).
                    return Ok(Operand::Move(Place::local(dest)));
                }
                // ADR-0077 Q2 — `coil.Buffer` index read, beside the
                // Dict/List arms above. The base handle is BORROWED (Move →
                // Copy upgrade) so the source local survives + drops once at
                // scope exit (ADR-0072 §5 risk 1). NOT the
                // `Projection::Index` fall-through below (a Wave-1 no-op
                // stub that would segfault / mis-type on a Buffer — ADR-0077
                // §4 option (b) rejection). Two index shapes dispatch here:
                //
                //   - SCALAR `a[i]` (`IndexKind::Expr`, Phase 1) →
                //     `__cobrust_coil_buffer_getitem(a, i) -> f64` (a plain
                //     f64 scalar; numpy's 0-d scalar is not a Cobrust type,
                //     ADR-0077 §4).
                //   - SLICE `a[lo:hi]` (`IndexKind::Slice`, Phase 2a) →
                //     `__cobrust_coil_buffer_slice(a, lo, hi) -> Buffer` (a
                //     fresh OWNED Buffer the `.cb` scope drops once). The
                //     `lo`/`hi` bounds are lowered DIRECTLY from the
                //     `IndexKind::Slice` here — the generic `lower_index`
                //     collapses a Slice to `Constant::Int(0)` (its scalar-
                //     only contract), so the slice arm must read the bounds
                //     itself. Phase 2a is the simple contiguous `lo:hi` form
                //     (both bounds present, default step); step / open-ended
                //     / negative bounds are ADR-0077 §12 deferrals — an
                //     unsupported slice shape falls through (the typecheck
                //     catch-all returned `coil.Buffer`, so this stays a
                //     bounded gap rather than a miscompile).
                if matches!(&base_ty, Ty::Adt(id, _) if *id == cobrust_types::COIL_BUFFER_ADT) {
                    if let IndexKind::Slice { start, stop, step } = index.as_ref() {
                        if step.is_none() {
                            if let (Some(lo_e), Some(hi_e)) = (start.as_ref(), stop.as_ref()) {
                                let base_op = upgrade_move_to_copy_handle(self.lower_expr(base)?);
                                let lo_op = self.lower_expr(lo_e)?;
                                let hi_op = self.lower_expr(hi_e)?;
                                let dest = self.declare_local(
                                    "_coilslice".to_string(),
                                    cobrust_types::coil_buffer_ty(),
                                    e.span,
                                    true,
                                );
                                let cur = self.current_block_id();
                                let next = self.start_new_block();
                                self.cur_block = Some(cur.0 as usize);
                                self.terminate(Terminator::Call {
                                    func: Operand::Constant(Constant::Str(
                                        cobrust_types::coil_buffer_slice_symbol().to_string(),
                                    )),
                                    args: vec![base_op, lo_op, hi_op],
                                    destination: Place::local(dest),
                                    target: next,
                                    unwind: None,
                                });
                                self.cur_block = Some(next.0 as usize);
                                return Ok(Operand::Move(Place::local(dest)));
                            }
                        }
                    } else {
                        let base_op = upgrade_move_to_copy_handle(self.lower_expr(base)?);
                        let idx_op = self.lower_index(index)?;
                        let dest =
                            self.declare_local("_coilidx".to_string(), Ty::Float, e.span, false);
                        let cur = self.current_block_id();
                        let next = self.start_new_block();
                        self.cur_block = Some(cur.0 as usize);
                        self.terminate(Terminator::Call {
                            func: Operand::Constant(Constant::Str(
                                cobrust_types::coil_buffer_getitem_symbol().to_string(),
                            )),
                            args: vec![base_op, idx_op],
                            destination: Place::local(dest),
                            target: next,
                            unwind: None,
                        });
                        self.cur_block = Some(next.0 as usize);
                        return Ok(Operand::Copy(Place::local(dest)));
                    }
                }
                // F83 / ADR-0106 — `t[i]` read on a TUPLE base. A tuple is a
                // struct-like aggregate already lowered via
                // `Projection::Field(idx)` (construction §1457, the lvalue Attr
                // path, field reads). We mirror that EXISTING projection here
                // rather than the generic `Projection::Index` fall-through
                // below, whose `lower_index` returned a `Constant::Int(0)` STUB
                // — the §2.2 SILENT-0 MISCOMPILE this closes (`(7, "x")[0]`
                // built OK + ran returning `0`, CPython `7`).
                //
                // The type checker (check.rs `(Ty::Tuple, IndexKind::Expr)`)
                // has ALREADY rejected a NON-CONSTANT index and a constant-OOB
                // index (`NotIndexable`, §2.5-A compile-time-catch), so on the
                // accepted-program path the index is ALWAYS a literal int in
                // range. We re-derive the normalised field offset from that
                // literal (Python-negative `i<0 -> arity+i` against the static
                // arity) and read `Projection::Field(off)` as the per-position
                // element type the checker resolved.
                //
                // Drop discipline: the tuple owns its fields and drops them
                // ONCE at scope exit (the construction arm's `_tuple` local).
                // Reading a field as an `Operand::Copy` of the `Field`
                // projection does NOT move-out the field, so a non-Copy field
                // (e.g. `(s, 1)[0]` with `s: str`) stays owned by the tuple and
                // is dropped exactly once — no double-free / corruption of a
                // sibling owned field (mirrors the let-destructure §574 Copy of
                // `Projection::Field`).
                if let Ty::Tuple(elem_tys) = &base_ty {
                    if let IndexKind::Expr(idx_e) = index.as_ref() {
                        if let Some(lit) = literal_int_value_mir(idx_e) {
                            let arity = elem_tys.len() as i64;
                            let normalized = if lit < 0 { lit + arity } else { lit };
                            // The checker has bounds-checked; this is a
                            // §6 defense-in-depth clamp guard (an out-of-range
                            // value reaching here means the checker was
                            // bypassed — never silently read field 0).
                            if normalized >= 0 && normalized < arity {
                                let off = normalized as usize;
                                let elem_ty = elem_tys[off].clone();
                                let base_op = upgrade_move_to_copy_handle(self.lower_expr(base)?);
                                let tup_local = self.declare_local(
                                    "_tupidxbase".to_string(),
                                    base_ty.clone(),
                                    e.span,
                                    false,
                                );
                                self.emit_assign(
                                    Place::local(tup_local),
                                    Rvalue::Use(base_op),
                                    e.span,
                                );
                                let dest = self.declare_local(
                                    "_tupidx".to_string(),
                                    elem_ty,
                                    e.span,
                                    false,
                                );
                                let field_place = Place {
                                    local: tup_local,
                                    projections: vec![Projection::Field(off)],
                                };
                                self.emit_assign(
                                    Place::local(dest),
                                    Rvalue::Use(Operand::Copy(field_place)),
                                    e.span,
                                );
                                return Ok(Operand::Copy(Place::local(dest)));
                            }
                        }
                        // DEFENSE-IN-DEPTH — a non-literal / OOB tuple index
                        // reaching MIR means the checker's `NotIndexable`
                        // reject was bypassed. A hard `MirError` is the
                        // §2.2-sound backstop (NEVER the silent `Int(0)` stub).
                        return Err(MirError::Internal(format!(
                            "tuple index must be a constant int in range at {:?} \
                             (this must be rejected by TypeError::NotIndexable \
                             upstream — F83 / ADR-0106)",
                            e.span
                        )));
                    }
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
            // Python conditional expression (ternary): `<then> if <cond>
            // else <else>` (F93 / ADR-0105). Lowered as value-producing
            // control flow that reuses the `if`-statement machinery:
            //   eval cond in the current block → SwitchInt to then/else
            //   blocks, each assigning a fresh RESULT local then
            //   Goto(join). The expression evaluates to that result local.
            // The result local's type is `then`'s type (the checker has
            // already unified both arms), so a non-Copy ternary (e.g. a
            // `str` ternary) participates in drop dispatch correctly.
            ExprKind::IfExpr {
                cond,
                then_branch,
                else_branch,
            } => {
                let result_ty = synth_expr_ty(self, then_branch);
                let result = self.declare_local("_tern".to_string(), result_ty, e.span, false);

                // Evaluate the condition in the current block.
                let (cond_op, cond_end_block) = self.lower_condition(cond)?;

                // then-block: assign `result = <then>`, Goto(join).
                let then_block = self.start_new_block();
                let then_op = self.lower_expr(then_branch)?;
                self.emit_assign(Place::local(result), Rvalue::Use(then_op), e.span);
                let then_exit = self.current_block_id();

                // else-block: assign `result = <else>`, Goto(join).
                let else_block = self.start_new_block();
                let else_op = self.lower_expr(else_branch)?;
                self.emit_assign(Place::local(result), Rvalue::Use(else_op), e.span);
                let else_exit = self.current_block_id();

                // join-block: where both arms converge and the value is read.
                let join_block = self.start_new_block();

                // Wire the two arm exits into the join.
                self.cur_block = Some(then_exit.0 as usize);
                if !self.terminated() {
                    self.terminate(Terminator::Goto(join_block));
                }
                self.cur_block = Some(else_exit.0 as usize);
                if !self.terminated() {
                    self.terminate(Terminator::Goto(join_block));
                }

                // Branch from the cond-end block to then/else.
                self.cur_block = Some(cond_end_block.0 as usize);
                self.terminate(Terminator::SwitchInt {
                    operand: cond_op,
                    cases: vec![(SwitchValue::Bool(true), then_block)],
                    otherwise: else_block,
                });

                // Resume in the join block; the ternary's value is `result`.
                self.cur_block = Some(join_block.0 as usize);
                Ok(Operand::Copy(Place::local(result)))
            }
        }
    }

    fn lower_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        span: Span,
    ) -> Result<Operand, MirError> {
        // ADR-0052d-prereq §"Decision" — method-form lowering. When the
        // callee is `Attr { base, name }` and `base`'s type matches one
        // of the 5 recognised method-table receivers (Str / List /
        // Float / Int — Dict is sub-sprint d's stretch goal), rewrite
        // the call to its PRELUDE-fn equivalent (`s.len()` →
        // `str_len(s)`). The type checker has already validated the
        // (receiver, method, args) tuple; this is a pure syntactic
        // sugar lowering — no new MIR instruction kinds.
        //
        // The PRELUDE-fn target is resolved by name via
        // `self.ctx.lookup_fn_def_id(rewritten_name)`. If the name is
        // not declared (e.g. `is_nan` which is not in PRELUDE), the
        // rewrite is skipped and the fallthrough produces the original
        // (broken) `Attr` lowering — the type checker already accepted
        // the call so this is observable only at link / Cranelift
        // verification time. Phase H+ may add the missing PRELUDE-fns
        // to close this gap; Wave-2 ships the partial coverage with
        // the gap documented in ADR-0052d-prereq §"Consequences".
        // ADR-0072 §2/§3 — ecosystem-module call lowering fires first.
        // `den.connect(...)` / `conn.execute(...)` / `cur.fetchall()`
        // retarget onto the `__cobrust_den_*` C-ABI symbols. The type
        // checker has already validated the call against the manifest;
        // here we emit the `Call` with a `Constant::Str` callee and the
        // manifest's return type (so the handle local gets its nominal
        // `Ty::Adt`, driving drop scheduling).
        if let Some(op) = self.try_lower_ecosystem_call(callee, args, span)? {
            return Ok(op);
        }

        if let ExprKind::Attr { base, name } = &callee.kind {
            if let Some(rewritten_name) = method_form_rewrite_name(self, base, name.as_str()) {
                if let Some(prelude_def_id) = self.ctx.lookup_fn_def_id(&rewritten_name) {
                    return self.lower_rewritten_method_call(
                        base,
                        args,
                        prelude_def_id,
                        rewritten_name,
                        span,
                    );
                }
            }
        }

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
        // ADR-0090 + ADR-0107 / F94 — is the callee the LIST-REDUCER
        // builtin `min`/`max`/`sum` (vs a USER `fn min(a, b)` shadow)? The
        // PRELUDE reducer stubs declare a `list` FIRST param; a user
        // scalar-param `min`/`max` is a different function and must NOT be
        // routed through the reducer return-type override or the ADR-0107
        // variadic temp-list build. Gate on the resolved callee signature's
        // first positional being a `list` (mirrors the type-checker's
        // `reduce_defs` shape gate in `check.rs`).
        let callee_is_list_reducer = matches!(callee_name, Some("min" | "max" | "sum"))
            && if let ExprKind::Name(rn) = &callee.kind {
                matches!(
                    self.ctx.lookup_ty(rn.def_id),
                    Ty::Fn(ref ft) if matches!(ft.positional.first(), Some(Ty::List(_)))
                )
            } else {
                false
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
        // ADR-0093 — `len(b)` BORROWS its `bytes` arg (reads the handle
        // for `__cobrust_bytes_len`, never consumes it), so the source
        // `b` local stays live for a subsequent `b[i]` read + drops ONCE
        // at scope exit. A `bytes` value is operand-Move (Str-mirror), so
        // without this upgrade `len(b); b[0]` would `UseAfterMove`. The
        // same borrow-not-move discipline `len(list)` enjoys (List is
        // operand-Copy). Keyed on the prelude `len` name; a user-shadowed
        // `len` never reaches the sized-arg type-check arm.
        let is_len_bytes_borrow = callee_name == Some("len");
        // F47 fix (2026-05-25): synthesise the callee's return type so
        // the `_callret` destination carries the correct MIR `Ty` instead
        // of the bug-prone default `Ty::None`. Downstream consumers
        // (f-string `lower_aggregate_format_string`, drop scheduling,
        // etc.) inspect `body.locals[dest].ty` to decide dispatch — a
        // `Ty::None` _callret holding a Str pointer was being formatted
        // through the `__cobrust_fmt_int` integer-decimal arm (printing
        // the raw heap pointer as a number) instead of `__cobrust_fmt_str`.
        //
        // Pattern: `f"{count_word(0)}"` lowered the hole as
        // `Move(_callret_n)` where `_callret_n: Ty::None`; the codegen
        // saw `mir_ty = Ty::None` → `is_str = false` and dispatched the
        // int path. By declaring `_callret_n: Ty::Str` here, the codegen
        // recovers the correct `is_str` branch and emits
        // `__cobrust_str_ptr` / `__cobrust_str_len` / `__cobrust_fmt_str`.
        //
        // Sibling site `lower_rewritten_method_call` (line ~1796) also
        // declares `_callret: Ty::None` and gets the parallel patch.
        let callee_return_ty = if let ExprKind::Name(rn) = &callee.kind {
            let callee_ty = self.ctx.lookup_ty(rn.def_id);
            if let Ty::Fn(fn_ty) = callee_ty {
                (*fn_ty.return_ty).clone()
            } else {
                Ty::None
            }
        } else {
            Ty::None
        };
        // ADR-0089 §5 — type-PRESERVING `abs(x)` return-type override. The
        // PRELUDE `abs` stub declares `-> f64`, but Python's `abs` returns
        // the SAME type as its argument: `abs(-5)` is an `i64`. The
        // type-checker resolved the call to `Int`, yet this MIR layer
        // re-derives the return from the (f64) prelude sig — so without
        // this override the `_callret` alloca would be a `double` while
        // the int arm rewrites the call to `__cobrust_int_abs` (i64 ->
        // i64), corrupting `print(abs(-5))` (a `double` slot fed to
        // `__cobrust_println_int`). When the single arg synths to `Int`
        // (unwrapping one `Ref` for `abs(&n)`), the dest is `Int`;
        // otherwise it stays `Float` (the existing path). Mirrors the
        // intrinsic-rewrite `Kind::MathAbs` int dispatch (one source of
        // truth: the arg's resolved type).
        let callee_return_ty = if callee_name == Some("abs") {
            if let [CallArg::Positional(arg)] = args {
                let arg_ty = match synth_expr_ty(self, arg) {
                    Ty::Ref(inner) => *inner,
                    other => other,
                };
                if matches!(arg_ty, Ty::Int) {
                    Ty::Int
                } else {
                    callee_return_ty
                }
            } else {
                callee_return_ty
            }
        } else {
            callee_return_ty
        };
        // ADR-0090 §5 — list-reducer `min`/`max`/`sum` return-type
        // override. The PRELUDE stubs declare `-> i64` (a placeholder),
        // but Python's `min`/`max`/`sum` return the ELEMENT type:
        // `sum([1.5, 2.5]) == 4.0` (a `float`). The type-checker
        // resolved the call to `Float` for a `list[float]` arg, yet this
        // MIR layer re-derives the return from the (i64) prelude sig — so
        // without this override the `_callret` alloca would be an i64
        // while the float arm rewrites the call to
        // `__cobrust_{min,max,sum}_float` (-> f64), corrupting
        // `print(sum([1.5, 2.5]))`. We read the SINGLE list arg's element
        // type from the type-checker's record (`synth_expr_ty`, NOT the
        // arg's MIR temp — the ADR-0089 abs-miscompile lesson: a list
        // BUILT then passed has a fine `Ty::List` synth type but its
        // MIR-temp drop-bookkeeping is incidental). A `list[float]` →
        // `Float`; otherwise the dest stays `Int` (the existing path).
        // The intrinsic-rewrite `Kind::{Min,Max,Sum}` dispatch reads this
        // SAME dest type as its one source of truth.
        let callee_return_ty = if callee_is_list_reducer {
            if let [CallArg::Positional(arg)] = args {
                let arg_ty = match synth_expr_ty(self, arg) {
                    Ty::Ref(inner) => *inner,
                    other => other,
                };
                if matches!(arg_ty, Ty::List(elem) if matches!(*elem, Ty::Float)) {
                    Ty::Float
                } else {
                    Ty::Int
                }
            } else if matches!(callee_name, Some("min" | "max")) && args.len() >= 2 {
                // ADR-0107 / F94 — VARIADIC scalar `min`/`max`. The type-
                // checker promotes the whole call to `Float` if ANY arg
                // resolves to `Float`, else `Int`. We mirror that here so
                // the `_callret` alloca matches the runtime shim picked by
                // the intrinsic-rewrite pass (which reads this dest type).
                let any_float = args.iter().any(|a| {
                    if let CallArg::Positional(e) = a {
                        matches!(
                            match synth_expr_ty(self, e) {
                                Ty::Ref(inner) => *inner,
                                other => other,
                            },
                            Ty::Float
                        )
                    } else {
                        false
                    }
                });
                if any_float { Ty::Float } else { Ty::Int }
            } else {
                callee_return_ty
            }
        } else {
            callee_return_ty
        };
        // ADR-0108 / F95 — `sorted(xs)` return-type override. The PRELUDE
        // `sorted` stub declares `-> list[i64]` (a placeholder), but
        // Python's `sorted` returns a NEW list of the SAME element type:
        // `sorted(["b", "a"])` is a `list[str]`. The type-checker
        // (`try_synth_sorted_builtin`) resolved the call to `list[T]`, yet
        // this MIR layer re-derives the return from the (list[i64]) prelude
        // sig — so without this override the `_callret` alloca + its DROP
        // schedule would be `list[i64]` while the arg is `list[str]`,
        // LEAKING the fresh str clones (a `list[i64]` drop frees the spine
        // but NOT the element Str buffers `__cobrust_list_sort_str` minted).
        // We read the SINGLE list arg's element type from the type-checker's
        // record (`synth_expr_ty`, NOT the arg's MIR temp — the ADR-0089
        // abs-miscompile lesson). The intrinsic-rewrite `Kind::Sorted`
        // dispatch reads this SAME dest list element type as its one source
        // of truth (int / float / str shim selection + drop schedule).
        let sorted_is_intrinsic = callee_name == Some("sorted")
            && if let ExprKind::Name(rn) = &callee.kind {
                matches!(
                    self.ctx.lookup_ty(rn.def_id),
                    Ty::Fn(ref ft) if matches!(ft.positional.first(), Some(Ty::List(_)))
                )
            } else {
                false
            };
        let callee_return_ty = if sorted_is_intrinsic {
            if let [CallArg::Positional(arg)] = args {
                let arg_ty = match synth_expr_ty(self, arg) {
                    Ty::Ref(inner) => *inner,
                    other => other,
                };
                match arg_ty {
                    Ty::List(elem) => Ty::List(elem),
                    // A non-list arg is a type-checker reject; keep the
                    // prelude return so MIR stays well-formed (unreached
                    // at codegen).
                    _ => callee_return_ty,
                }
            } else {
                callee_return_ty
            }
        } else {
            callee_return_ty
        };
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
                    } else if is_len_bytes_borrow {
                        // ADR-0093 — `len(b)` borrows a `bytes` arg.
                        upgrade_move_to_copy_for_bytes(self, op)
                    } else {
                        op
                    };
                    arg_ops.push(op);
                }
            }
        }
        // ADR-0107 / F94 — VARIADIC scalar `min`/`max` lowering. The
        // type-checker (`try_synth_reduce_builtin`) accepts the >=2-arg
        // scalar form (`max(3, 5)` / `min(a, b, c)`) and promotes the call
        // to `Float` if ANY arg is `Float`, else `Int`. Here we MATERIALISE
        // a temp `list[T]` from the N scalar operands and replace `arg_ops`
        // with the single list operand, REUSING the proven ADR-0090 list-
        // consume path (intrinsic-rewrite → `__cobrust_{min,max}_{int,
        // float}` shim → once-drop schedule). The scalars are `Int`/`Float`
        // (Copy) — no element-drop concern; the temp list drops once in its
        // own scope. When the call promotes to `Float`, any `Int` arg is
        // cast i64→f64 FIRST so the homogeneous f64-bit list matches the
        // `*_float` shim (which reads raw f64 bits). The 1-arg form
        // (`max([list])`) has `args.len() == 1` and is untouched.
        if callee_is_list_reducer
            && matches!(callee_name, Some("min" | "max"))
            && arg_ops.len() >= 2
        {
            // Re-derive per-arg promotion: `Float` whole-call iff any arg's
            // resolved scalar type is `Float` (mirrors the type-checker +
            // the `callee_return_ty` override above).
            let elem_is_float = args.iter().any(|a| {
                if let CallArg::Positional(e) = a {
                    matches!(
                        match synth_expr_ty(self, e) {
                            Ty::Ref(inner) => *inner,
                            other => other,
                        },
                        Ty::Float
                    )
                } else {
                    false
                }
            });
            let elem_ty = if elem_is_float { Ty::Float } else { Ty::Int };
            // Cast each Int operand to f64 when the call promotes to Float.
            let mut elem_ops: Vec<Operand> = Vec::with_capacity(arg_ops.len());
            for (a, op) in args.iter().zip(arg_ops.iter()) {
                if elem_is_float {
                    let arg_is_int = if let CallArg::Positional(e) = a {
                        matches!(
                            match synth_expr_ty(self, e) {
                                Ty::Ref(inner) => *inner,
                                other => other,
                            },
                            Ty::Int
                        )
                    } else {
                        false
                    };
                    if arg_is_int {
                        let cdest =
                            self.declare_local("_minmax_f".to_string(), Ty::Float, span, false);
                        self.emit_assign(
                            Place::local(cdest),
                            Rvalue::Cast(CastKind::IntToFloat, op.clone(), Ty::Float),
                            span,
                        );
                        elem_ops.push(Operand::Copy(Place::local(cdest)));
                        continue;
                    }
                }
                elem_ops.push(op.clone());
            }
            let list_temp = self.declare_local(
                "_minmax_list".to_string(),
                Ty::List(Box::new(elem_ty)),
                span,
                false,
            );
            self.emit_assign(
                Place::local(list_temp),
                Rvalue::Aggregate(AggregateKind::List, elem_ops),
                span,
            );
            arg_ops.clear();
            arg_ops.push(Operand::Move(Place::local(list_temp)));
        }
        // ADR-0089 §4 — Python-canonical 1-arg `range(stop)` lowering.
        // The type-checker (`try_synth_range_builtin`) accepts the 1-arg
        // form (== `range(0, stop)`) but the PRELUDE `range` body has two
        // params (`start`, `stop`). Inject a `start = 0` operand at the
        // front so the unchanged 2-param body runs, materialising
        // `[0, 1, …, stop-1]`. Keyed on the prelude name `range` AND
        // exactly one lowered positional operand (the 2-arg `range(a, b)`
        // has two operands and is untouched). A user-shadowed `range`
        // never reaches the 1-arg type-check arm, so this is safe.
        if callee_name == Some("range") && arg_ops.len() == 1 {
            arg_ops.insert(0, Operand::Constant(Constant::Int(0)));
        }
        let dest = self.declare_local("_callret".to_string(), callee_return_ty, span, true);
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

    /// ADR-0052d-prereq §"Decision" — emit a MIR Call for a rewritten
    /// method-form call. The receiver `base` is prepended as the first
    /// argument (per the PRELUDE-fn signature: `str_len(s)`, `split(s,
    /// sep)`, `list_push(xs, v)`, etc.); subsequent `args` follow.
    /// Callee is `Constant::FnRef(prelude_def_id)` so codegen routes
    /// the call through the per-module forward-declaration table.
    fn lower_rewritten_method_call(
        &mut self,
        base: &Expr,
        args: &[CallArg],
        prelude_def_id: DefId,
        rewritten_name: String,
        span: Span,
    ) -> Result<Operand, MirError> {
        // ADR-0050f §"Copy-at-operand" — Str helpers borrow rather than
        // move their first arg. Apply the same upgrade to the receiver
        // when the rewritten fn is in the borrow-not-move set.
        let is_str_borrow_target = matches!(
            rewritten_name.as_str(),
            "str_len"
                | "split"
                | "replace"
                | "trim"
                // ADR-0085 — lstrip / rstrip / count READ the receiver
                // Str pointer without consuming it (same borrow discipline
                // as trim / find). The Python aliases strip / startswith /
                // endswith already arrive here as trim / starts_with /
                // ends_with (rewritten upstream), so they are covered.
                | "lstrip"
                | "rstrip"
                | "count"
                | "find"
                | "contains"
                | "starts_with"
                | "ends_with"
                | "lower"
                | "upper"
                // ADR-0093 Phase 2 — `s.encode()` reads the receiver Str
                // pointer (mints a fresh `bytes`) WITHOUT consuming it; the
                // source `str` local survives + drops once.
                | "bytes_from_str"
                // ADR-0093 Phase 2 — `b.decode()` / `b.hex()` read the
                // receiver `bytes` pointer (mint a fresh `str`) WITHOUT
                // consuming it; the source `bytes` local survives + drops
                // once. Same borrow-not-move discipline (the Move→Copy
                // operand upgrade is type-agnostic — it just flips the
                // operand variant so the borrow checker does not mark the
                // receiver consumed).
                | "bytes_decode"
                | "bytes_hex"
        );
        let base_op = self.lower_expr(base)?;
        let base_op = if is_str_borrow_target {
            // Borrow-not-move the receiver. `upgrade_move_to_copy_for_str`
            // handles a `Ty::Str` receiver (all the str methods + the
            // `bytes_from_str` encode receiver); the chained
            // `upgrade_move_to_copy_for_bytes` handles a `Ty::Bytes`
            // receiver (the ADR-0093 `bytes_decode` / `bytes_hex`
            // methods). Each is a no-op on the wrong type, so chaining is
            // safe — exactly one fires per receiver. (A `bytes` receiver is
            // operand-Move like Str, so without the bytes upgrade
            // `b.hex(); len(b)` would `UseAfterMove`.)
            let after_str = upgrade_move_to_copy_for_str(self, base_op);
            upgrade_move_to_copy_for_bytes(self, after_str)
        } else {
            base_op
        };
        let mut arg_ops = Vec::with_capacity(args.len() + 1);
        arg_ops.push(base_op);
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
        // F47 sibling-site fix: propagate the PRELUDE-fn's return type to
        // the method-form `_callret` so downstream f-string / drop /
        // print dispatch sees the correct `Ty` instead of `Ty::None`.
        let callee_return_ty = {
            let callee_ty = self.ctx.lookup_ty(prelude_def_id);
            if let Ty::Fn(fn_ty) = callee_ty {
                (*fn_ty.return_ty).clone()
            } else {
                Ty::None
            }
        };
        let dest = self.declare_local("_callret".to_string(), callee_return_ty, span, true);
        let cur = self.current_block_id();
        let target = self.start_new_block();
        self.cur_block = Some(cur.0 as usize);
        self.terminate(Terminator::Call {
            func: Operand::Constant(Constant::FnRef(prelude_def_id.0)),
            args: arg_ops,
            destination: Place::local(dest),
            target,
            unwind: None,
        });
        self.cur_block = Some(target.0 as usize);
        Ok(Operand::Move(Place::local(dest)))
    }

    /// ADR-0072 §2/§3 — lower an ecosystem-module call to a `Call`
    /// terminator whose callee is the `Constant::Str` C-ABI symbol.
    ///
    /// Two shapes, mirroring the type-checker's `try_synth_ecosystem_call`:
    ///
    /// 1. **Module function** — `den.connect(path)`: callee is
    ///    `Attr { base: Name(import-alias to den), name }`. The args are
    ///    the explicit call args.
    /// 2. **Handle method** — `conn.execute(sql)` / `cur.fetchall()`:
    ///    callee is `Attr { base, name }` where `synth_expr_ty(base)` is
    ///    an ecosystem-handle `Ty::Adt`. The receiver `base` is prepended
    ///    as the first arg.
    ///
    /// ## Ownership (ADR-0072 §5 prime risk)
    ///
    /// `connect` returns a freshly-Boxed handle the caller owns (drop at
    /// scope exit). `execute` / `fetchall` **borrow** their handle
    /// receiver, so the receiver operand is upgraded `Move → Copy` —
    /// otherwise the borrow checker would consume the handle local and
    /// the drop schedule would skip its scope-exit drop. The handle is
    /// freed exactly once by `__cobrust_den_*_drop` at scope exit. The
    /// `path` / `sql` str args are likewise Copy-at-operand (the shim
    /// reads the Str buffer without freeing it).
    ///
    /// Returns `Ok(None)` when the call is not an ecosystem call.
    fn try_lower_ecosystem_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        span: Span,
    ) -> Result<Option<Operand>, MirError> {
        let ExprKind::Attr { base, name } = &callee.kind else {
            return Ok(None);
        };

        // ADR-0079 Q4-a — sub-namespaced module function
        // (`coil.linalg.solve`). Mirrors the typecheck dotted-of-dotted
        // rule: `base` is itself `Attr { base: Name(rn import-alias),
        // name: subns }` where `(module, subns)` is a known sub-namespace.
        // The leaf resolves to a flat `__cobrust_coil_linalg_<fn>` symbol —
        // NO new MIR mechanism, the sub-namespace leaf is just a different
        // `runtime_symbol` string fed to the SAME `emit_ecosystem_call`
        // borrow-Buffer-args-return-fresh-handle path (the Buffer args
        // auto-borrow via `lower_eco_arg`'s `Value` handle Move→Copy
        // upgrade, so the input handles stay live + drop once). Checked
        // BEFORE Case 1, like the typechecker, so the inner-`Attr` base
        // shape is matched before the `Name(rn)` base path.
        if let ExprKind::Attr {
            base: ns_base,
            name: subns,
        } = &base.kind
        {
            if let ExprKind::Name(rn) = &ns_base.kind {
                if rn.kind == DefKind::ImportAlias
                    && cobrust_types::is_subnamespace(rn.name.as_str(), subns)
                {
                    let Some(sig) =
                        cobrust_types::lookup_subnamespace_fn(rn.name.as_str(), subns, name)
                    else {
                        return Ok(None);
                    };
                    let pos_args = collect_positional_args(args);
                    let mut arg_ops = Vec::with_capacity(pos_args.len());
                    for (a, p) in pos_args.iter().zip(sig.params.iter()) {
                        arg_ops.push(lower_eco_arg(self, a, p)?);
                    }
                    let op = self.emit_ecosystem_call(
                        sig.runtime_symbol,
                        sig.ret.clone(),
                        arg_ops,
                        span,
                    );
                    return Ok(Some(op));
                }
            }
        }

        // Case 1: module-level free function (`den.connect`).
        if let ExprKind::Name(rn) = &base.kind {
            if rn.kind == DefKind::ImportAlias
                && cobrust_types::is_ecosystem_module(rn.name.as_str())
            {
                let Some(sig) = cobrust_types::lookup_module_fn(rn.name.as_str(), name) else {
                    return Ok(None);
                };
                // Module fn: no receiver. Args lowered per param-kind:
                // `Value` → normal lower + Str copy upgrade; `Callback`
                // → `Constant::FnRef(def_id)` directly from the source
                // `ExprKind::Name(rn)` (ADR-0073 §2 D2).
                let pos_args = collect_positional_args(args);
                let mut arg_ops = Vec::with_capacity(pos_args.len());
                for (a, p) in pos_args.iter().zip(sig.params.iter()) {
                    arg_ops.push(lower_eco_arg(self, a, p)?);
                }
                // #numpy core constructor — `coil.array([list])` ELEMENT-DTYPE
                // dispatch (ADR-0091). The EcoSig's `runtime_symbol` is the
                // float shim (the manifest default); but `coil.array` is
                // element-dtype-polymorphic — a `list[int]` must build an
                // int64 Buffer (`__cobrust_coil_array_int`, raw i64 slots), a
                // `list[float]` a float64 Buffer (`__cobrust_coil_array_float`,
                // `from_bits` slots). We pick the shim from the SINGLE list
                // arg's STATIC element type (`synth_expr_ty` → `Ty::List(elem)`
                // — the type-checker's resolved record, NOT the arg's MIR temp:
                // the ADR-0089/0090 lesson, so a list BUILT then passed
                // — `coil.array(make_ints())` — routes by its real element
                // type, immune to the computed-arg miscompile). An empty /
                // unresolved elem stays on the FLOAT shim (numpy's empty-list
                // default dtype is float64 — `np.array([]).dtype == float64`),
                // matching the type-checker's `synth_coil_array` anchor. The
                // list arg already passes Copy-at-call (`is_copy_type(Ty::List)`
                // — the shim BORROWS it; the `.cb` scope drops it once).
                let runtime_symbol = if rn.name.as_str() == "coil" && name == "array" {
                    let elem_is_int = pos_args.first().is_some_and(|arg| {
                        matches!(
                            synth_expr_ty(self, arg),
                            Ty::List(elem) if matches!(*elem, Ty::Int)
                        )
                    });
                    if elem_is_int {
                        "__cobrust_coil_array_int"
                    } else {
                        "__cobrust_coil_array_float"
                    }
                } else {
                    sig.runtime_symbol
                };
                let op = self.emit_ecosystem_call(runtime_symbol, sig.ret.clone(), arg_ops, span);
                return Ok(Some(op));
            }
        }

        // Case 2: handle method (`conn.execute`, `cur.fetchall`,
        // `app.route`, `app.serve_in_background`).
        let base_ty = synth_expr_ty(self, base);
        if let Ty::Adt(id, _) = &base_ty {
            if cobrust_types::is_ecosystem_handle(*id) {
                let Some(sig) = cobrust_types::lookup_handle_method(&base_ty, name) else {
                    return Ok(None);
                };
                // Receiver is BORROWED: upgrade Move → Copy so the handle
                // local survives the call and is dropped once at scope
                // exit (ADR-0072 §5 risk 1).
                let recv_op = self.lower_expr(base)?;
                let recv_op = upgrade_move_to_copy_handle(recv_op);
                let pos_args = collect_positional_args(args);
                let mut arg_ops = Vec::with_capacity(pos_args.len() + 1);
                arg_ops.push(recv_op);
                for (a, p) in pos_args.iter().zip(sig.params.iter()) {
                    arg_ops.push(lower_eco_arg(self, a, p)?);
                }
                // ADR-0080 Phase-1b-ii — `route_validated` retargets onto a
                // DIFFERENT symbol (no new mechanism) but the trampoline
                // needs the validated-body SCHEMA, which the user never
                // writes. We SYNTHESISE it here from the handler's 2nd-param
                // body class (its field table + refinement side-table on
                // `TypedModule`, the SAME source the type checker resolved
                // field access against — footgun #4, cannot drift) and
                // append it as a trailing `Constant::Str` arg. The codegen
                // extern declares 5 params; the trampoline parses this
                // descriptor (ADR-0080 §5.4).
                if sig.runtime_symbol == "__cobrust_pit_app_route_validated" {
                    let schema = self.validated_body_schema_for_handler(&pos_args);
                    arg_ops.push(Operand::Constant(Constant::Str(schema)));
                }
                let op =
                    self.emit_ecosystem_call(sig.runtime_symbol, sig.ret.clone(), arg_ops, span);
                return Ok(Some(op));
            }
        }
        Ok(None)
    }

    /// Emit a `Terminator::Call` for an ecosystem call: callee is a
    /// `Constant::Str` runtime symbol, destination is a fresh `_ecoret`
    /// local carrying the manifest return type (so a handle-typed return
    /// is drop-scheduled).
    fn emit_ecosystem_call(
        &mut self,
        runtime_symbol: &str,
        ret_ty: Ty,
        arg_ops: Vec<Operand>,
        span: Span,
    ) -> Operand {
        let dest = self.declare_local("_ecoret".to_string(), ret_ty, span, true);
        let cur = self.current_block_id();
        let target = self.start_new_block();
        self.cur_block = Some(cur.0 as usize);
        self.terminate(Terminator::Call {
            func: Operand::Constant(Constant::Str(runtime_symbol.to_string())),
            args: arg_ops,
            destination: Place::local(dest),
            target,
            unwind: None,
        });
        self.cur_block = Some(target.0 as usize);
        Operand::Move(Place::local(dest))
    }

    /// ADR-0080 Phase-1b-ii — synthesise the validated-body SCHEMA
    /// descriptor for an `app.route_validated(method, path, handler)` call.
    ///
    /// `pos_args` are the call's positional args; the 3rd (`handler`) is a
    /// bare `Name` whose resolved `Ty::Fn` 2nd positional is the body class
    /// `Ty::Adt`. We read that class's field table + refinement side-table
    /// off `TypedModule` (the SAME source the type checker used) and render
    /// the compact line-per-field descriptor the trampoline parses
    /// (ADR-0080 §5.4), prefixed (ADR-0080 Phase-1b-iii) by a `# <BodyName>`
    /// header line naming the body class for the OpenAPI emitter:
    ///
    /// ```text
    /// # CreateScore
    /// name\tstr
    /// rank\ti64:0:100
    /// ```
    ///
    /// The first line `# <BodyName>` (Phase-1b-iii) names the body class so
    /// the OpenAPI emitter keys `components/schemas/<BodyName>` from the SAME
    /// descriptor; the validator skips it (no TAB). Each field line is
    /// `field<TAB>kind[suffix]` where `kind ∈ {str,i64,f64,bool}` and the
    /// optional int-range `suffix` is `:lo:hi` (an absent bound is the empty
    /// string). Fields are emitted in the `BTreeMap`'s deterministic name
    /// order. A field whose type is not a Phase-1b-ii scalar is rendered with
    /// kind `any` (the validator only checks presence for it). If the handler
    /// / body class cannot be resolved (defensive — the type checker already
    /// accepted it), an empty schema is emitted (the trampoline then
    /// validates JSON-object-ness only).
    ///
    /// # ADR-0080 §6 Phase-4 (b) / #156 — the MULTI-BLOCK descriptor (D1)
    ///
    /// A field whose declared `Ty` is ANOTHER validated class (a `Ty::Adt`
    /// present in `adt_fields`) is NO LONGER rendered as kind `any`
    /// (presence-only, UNVALIDATED). Instead its payload is the NEW kind token
    /// `obj:<NestedClassName>`, and the descriptor becomes MULTI-BLOCK: the
    /// ROOT class block first (`# Root` header + its field lines), then one
    /// `# Nested`-headed block per TRANSITIVELY-referenced validated class,
    /// collected by walking nested-class fields, deduplicated by `AdtId`, in
    /// deterministic discovery order (a BFS over the root's nested fields,
    /// each class's nested fields in `BTreeMap` name order):
    ///
    /// ```text
    /// # CreateUser
    /// address\tobj:Address
    /// name\tstr
    /// # Address
    /// city\tstr
    /// zip\ti64:0:99999
    /// ```
    ///
    /// The `obj:<Name>` token references the nested class by its SOURCE NAME
    /// (the `# <Name>` header of its block) — so `parse_schema` (the ONE
    /// decode source, footgun #4) resolves the nested field against the named
    /// block, and the OpenAPI emitter keys `components/schemas/<Name>` from
    /// the same name. A truly-unknown field type (not a scalar, not a
    /// validated class) still maps to `any`. A FLAT-only body (no nested
    /// field) emits exactly ONE block — BYTE-IDENTICAL to the pre-Phase-4
    /// single-block descriptor.
    fn validated_body_schema_for_handler(&self, pos_args: &[&Expr]) -> String {
        let Some(handler) = pos_args.get(2) else {
            return String::new();
        };
        let ExprKind::Name(rn) = &handler.kind else {
            return String::new();
        };
        let handler_ty = self.ctx.lookup_ty(rn.def_id);
        let Ty::Fn(fn_ty) = &handler_ty else {
            return String::new();
        };
        let Some(Ty::Adt(body_adt, _)) = fn_ty.positional.get(1) else {
            return String::new();
        };
        if !self.ctx.typed.adt_fields.contains_key(body_adt) {
            return String::new();
        }
        // The ROOT block first, then one block per transitively-referenced
        // validated class (BFS, deduplicated by `AdtId`). `emit_class_block`
        // appends the named nested classes it discovered to `queue`, so the
        // walk terminates on a finite class graph (a cyclic class graph would
        // re-visit, but `seen` dedups, so the queue drains). Lines are joined
        // by `\n` across ALL blocks into the one descriptor string.
        let mut blocks: Vec<String> = Vec::new();
        let mut seen: std::collections::HashSet<cobrust_types::AdtId> =
            std::collections::HashSet::new();
        let mut queue: std::collections::VecDeque<cobrust_types::AdtId> =
            std::collections::VecDeque::new();
        queue.push_back(*body_adt);
        seen.insert(*body_adt);
        while let Some(adt) = queue.pop_front() {
            let mut nested: Vec<cobrust_types::AdtId> = Vec::new();
            blocks.push(self.emit_class_block(adt, &mut nested));
            // Enqueue not-yet-seen nested classes in discovery order (the
            // root's nested fields first, then theirs — a deterministic BFS).
            for n in nested {
                if seen.insert(n) {
                    queue.push_back(n);
                }
            }
        }
        blocks.join("\n")
    }

    /// ADR-0080 §6 Phase-4 (b)+(c) / #156 (D1) — render ONE validated class's
    /// descriptor block: a `# <Name>` header line (from `adt_names`) followed
    /// by one `field<TAB>payload` line per declared field, in the field
    /// table's deterministic `BTreeMap` name order. The payload by field type:
    ///
    /// - a field whose declared `Ty` is ANOTHER validated class (a `Ty::Adt`
    ///   present in `adt_fields`) → `obj:<NestedClassName>`, PUSHING that
    ///   class's `AdtId` onto `nested` so the caller's BFS visits it;
    /// - a `list[T]` field (Phase-4 (c)) → `list:<elem-payload>`, where
    ///   elem-payload is T's own payload (a scalar kind, OR `obj:<ClassName>`
    ///   for `list[SomeClass]` — the element class is likewise enqueued onto
    ///   `nested`). Rendered by [`Self::element_payload`];
    /// - a scalar field → its base kind (+ any refinement suffix, via the ONE
    ///   encoder `Refinement::descriptor_payload`);
    /// - any other type (incl. `list[<deferred-elem>]` like `list[list[T]]`)
    ///   → `any` (presence-only).
    fn emit_class_block(
        &self,
        adt: cobrust_types::AdtId,
        nested: &mut Vec<cobrust_types::AdtId>,
    ) -> String {
        let Some(fields) = self.ctx.typed.adt_fields.get(&adt) else {
            // Defensive: a class with no recorded fields (the caller only
            // enqueues `AdtId`s discovered from `adt_fields`, so this is
            // unreachable for the root — guarded above — and for nested
            // classes, which are pushed only when found in `adt_fields`).
            return String::new();
        };
        let mut lines = Vec::with_capacity(fields.len() + 1);
        // The `# <Name>` header (Phase-1b-iii): names the block for the
        // OpenAPI emitter's `components/schemas/<Name>` key + the multi-block
        // decoder's per-class map key. The validator skips it (no TAB).
        if let Some(name) = self.ctx.typed.adt_names.get(&adt) {
            lines.push(format!("# {name}"));
        }
        for (field_name, ty) in fields {
            // A field typed as ANOTHER validated class → `obj:<NestedName>`
            // (replaces the pre-Phase-4 `_ => "any"` for this case). We read
            // the nested class's SOURCE NAME from `adt_names` so the token
            // matches its block's `# <Name>` header (the decode resolves it
            // against that key). A `Ty::Adt` with NO recorded source name
            // (defensive) falls back to `any` rather than emitting a dangling
            // `obj:` token the decoder cannot resolve.
            if let Ty::Adt(nested_adt, _) = ty {
                if let Some(obj_payload) = self.obj_element_payload(*nested_adt, nested) {
                    lines.push(format!("{field_name}\t{obj_payload}"));
                    continue;
                }
            }
            // ADR-0080 §6 Phase-4 (c) / #156 (D1) — a `list[T]` field's payload
            // is the NEW token `list:<elem-payload>` where elem-payload is T's
            // OWN payload: a scalar kind (`str`/`i64`/`f64`/`bool`/`pat:…`) OR
            // `obj:<ClassName>` for `list[SomeClass]` (whose block is emitted
            // into the multi-block descriptor exactly like a direct nested
            // field — REUSING the BFS collector via `obj_element_payload`,
            // which pushes the element class onto `nested`). The element
            // payload is rendered by `element_payload` so the encode mirrors
            // `parse_schema`'s recursive `list:<rest>` decode (footgun #4 —
            // cannot drift). A `list[<truly-unknown>]` (e.g. `list[list[T]]`,
            // out of #156 scope) falls back to `any` via `element_payload`
            // returning `None`. The field's OWN refinement side-table entry
            // (if any) is NOT consulted for a list field: element-level
            // refinement is a DEFERRED follow-up (the element class/scalar
            // carries its own constraints; a `list[i64]` has no element
            // refinement, so its element renders as bare `i64`).
            if let Ty::List(elem_ty) = ty {
                if let Some(elem_payload) = self.element_payload(elem_ty, nested) {
                    lines.push(format!("{field_name}\tlist:{elem_payload}"));
                    continue;
                }
            }
            let kind = match ty {
                Ty::Str => "str",
                Ty::Int => "i64",
                Ty::Float => "f64",
                Ty::Bool => "bool",
                _ => "any",
            };
            // The scalar payload (`kind[suffix]`) is rendered by the ONE
            // encoding source, `Refinement::descriptor_payload`
            // (cobrust-types), so it cannot drift from `parse_schema`, the ONE
            // decode source (ADR-0080 §3 footgun #4). A refinement may append
            // a suffix to `kind` (int range / str length) or replace the kind
            // token entirely (a `pat:<regex>` pattern). A field with no
            // refinement carries just its base kind.
            let payload = self
                .ctx
                .typed
                .adt_refinements
                .get(&(adt, field_name.clone()))
                .map_or_else(|| kind.to_string(), |r| r.descriptor_payload(kind));
            lines.push(format!("{field_name}\t{payload}"));
        }
        lines.join("\n")
    }

    /// ADR-0080 §6 Phase-4 (c) / #156 (D1) — render the `obj:<ClassName>`
    /// payload for a field/element typed as a validated class `adt`, PUSHING
    /// the class's `AdtId` onto `nested` so the caller's BFS visits it (its
    /// block is emitted into the multi-block descriptor). Returns `None` for a
    /// `Ty::Adt` with NO recorded source name OR no recorded field table
    /// (defensive — the caller then falls back to `any` rather than emitting a
    /// dangling `obj:` token the decoder cannot resolve). Shared by the
    /// direct-nested-field path AND the `list[SomeClass]` element path, so the
    /// two cannot diverge (one source for the `obj:` token + the BFS enqueue).
    fn obj_element_payload(
        &self,
        adt: cobrust_types::AdtId,
        nested: &mut Vec<cobrust_types::AdtId>,
    ) -> Option<String> {
        let name = self.ctx.typed.adt_names.get(&adt)?;
        if !self.ctx.typed.adt_fields.contains_key(&adt) {
            return None;
        }
        let payload = format!("obj:{name}");
        nested.push(adt);
        Some(payload)
    }

    /// ADR-0080 §6 Phase-4 (c) / #156 (D1) — render the ELEMENT payload of a
    /// `list[T]` field: T's own payload, the part after `list:` in the
    /// descriptor. The MIRROR of `parse_schema`'s recursive `list:<rest>`
    /// decode (footgun #4 — cannot drift):
    ///
    /// - a SCALAR element (`str`/`i64`/`f64`/`bool`) → its base kind token. A
    ///   list element carries NO refinement suffix (element-level refinement is
    ///   a DEFERRED follow-up; the refinement side-table is keyed by
    ///   `(adt, field_name)`, which an element type has no entry in), so a
    ///   `list[i64]` element is the bare `i64` token;
    /// - a validated-CLASS element (`list[SomeClass]`) → `obj:<ClassName>` via
    ///   [`Self::obj_element_payload`] (which PUSHES the element class onto
    ///   `nested` for the BFS, so its block is emitted exactly like a direct
    ///   nested field);
    /// - any OTHER element type (e.g. `list[list[T]]` / `list[dict]`, out of
    ///   #156 scope) → `None`, so the caller falls back to `any` (presence-only
    ///   — the DEFERRED forms stay UNVALIDATED rather than emitting a token the
    ///   decoder cannot resolve).
    fn element_payload(
        &self,
        elem_ty: &Ty,
        nested: &mut Vec<cobrust_types::AdtId>,
    ) -> Option<String> {
        match elem_ty {
            Ty::Str => Some("str".to_string()),
            Ty::Int => Some("i64".to_string()),
            Ty::Float => Some("f64".to_string()),
            Ty::Bool => Some("bool".to_string()),
            Ty::Adt(elem_adt, _) => self.obj_element_payload(*elem_adt, nested),
            // DEFERRED element forms (list-of-list, dict, …) — presence-only.
            _ => None,
        }
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
        let lhs_ty = synth_expr_ty(self, lhs);
        // ADR-0077 Phase-1 completion — `coil.Buffer ⊕ scalar`
        // (`a + 1` / `a * 2` / `a - 1` / `a / 2`). Checked BEFORE the
        // array-array Buffer guard below: that guard keys only on the LHS
        // type, so `a + 1` (LHS Buffer) would otherwise wrongly route to
        // the `(a, b: *Buffer)` array-array shim with `1` lowered as an
        // i64. When the LHS resolves to the Buffer handle AND the RHS is a
        // numeric scalar (`Ty::Int`/`Ty::Float`, bare or `&`-borrowed) AND
        // the op has a scalar shim, retarget to
        // `__cobrust_coil_buffer_<op>_scalar(a, k: f64)`: the Buffer is a
        // BORROWED handle (Move→Copy upgrade — survives + drops once at
        // scope exit), and the scalar `k` is passed as `f64` (an `Int`
        // operand is cast i64→f64 via `CastKind::IntToFloat`, mirroring the
        // `a[i]` f64 scalar contract). The typecheck `synth_bin` arm
        // already accepted this exact shape (Buffer ⊕ Int/Float), so a
        // `Some` scalar shim here is an accepted op.
        let lhs_handle_ty = match &lhs_ty {
            Ty::Ref(inner) => inner.as_ref().clone(),
            other => other.clone(),
        };
        if matches!(&lhs_handle_ty, Ty::Adt(id, _) if *id == cobrust_types::COIL_BUFFER_ADT) {
            let rhs_ty = synth_expr_ty(self, rhs);
            let rhs_scalar_ty = match &rhs_ty {
                Ty::Ref(inner) => inner.as_ref().clone(),
                other => other.clone(),
            };
            if matches!(rhs_scalar_ty, Ty::Int | Ty::Float) {
                if let Some(scalar_sym) = cobrust_types::lookup_buffer_scalar_binop(op) {
                    let lhs_op = upgrade_move_to_copy_handle(self.lower_expr(lhs)?);
                    let rhs_op = self.lower_expr(rhs)?;
                    // Pass the scalar as f64. An `Int` operand is cast
                    // i64→f64; a `Float` operand is already f64.
                    let k_op = if matches!(rhs_scalar_ty, Ty::Int) {
                        let kdest =
                            self.declare_local("_coilk".to_string(), Ty::Float, span, false);
                        self.emit_assign(
                            Place::local(kdest),
                            Rvalue::Cast(CastKind::IntToFloat, rhs_op, Ty::Float),
                            span,
                        );
                        Operand::Copy(Place::local(kdest))
                    } else {
                        rhs_op
                    };
                    let op_out = self.emit_ecosystem_call(
                        scalar_sym,
                        cobrust_types::coil_buffer_ty(),
                        vec![lhs_op, k_op],
                        span,
                    );
                    return Ok(op_out);
                }
            }
        }
        // ADR-0077 Phase-2/3 — LEFT-scalar `k ⊕ a` (`2 * a` / `6 / a` /
        // `1 + a` / `2 - a`). The MIRROR of the right-scalar block above,
        // with the operand roles SWAPPED: the LHS is the numeric scalar
        // and the RHS is the Buffer handle. Checked BEFORE the array-array
        // `lookup_buffer_binop` guard below (which keys on the LHS — but
        // here the LHS is a scalar, not a Buffer, so it would NOT fire;
        // this block is what gives `2 * a` a path at all). The retarget
        // depends on whether `⊕` commutes:
        //   - `+`/`*` commute → reuse the right-scalar shims
        //     `__cobrust_coil_buffer_{add,mul}_scalar(a, k)` — pass the
        //     BUFFER as the handle arg and the SCALAR as `k: f64`
        //     (`k + a == a + k`, so the right-scalar shim is correct);
        //   - `-`/`/` do NOT commute → REVERSED shims
        //     `__cobrust_coil_buffer_{rsub,rdiv}_scalar(a, k)`, which
        //     compute `k - a[i]` / `k / a[i]` (the cabi
        //     `buffer_binop_scalar_rev` puts `k` on the LEFT). `2 - a` is
        //     `2 - a[i]`, NOT `a[i] - 2`.
        // In ALL four cases the C-ABI shape is `(buffer_ptr, k: f64) ->
        // ptr` — `lookup_buffer_left_scalar_binop` picks the symbol; the
        // BUFFER (RHS here) is the borrowed handle (Move→Copy upgrade) and
        // the SCALAR (LHS) is cast i64→f64 like the right-scalar path. The
        // typecheck `synth_bin` arm already accepted this exact shape
        // (Int/Float ⊕ Buffer), so a `Some` left-scalar shim is accepted.
        let rhs_ty_for_left_scalar = synth_expr_ty(self, rhs);
        let rhs_handle_ty = match &rhs_ty_for_left_scalar {
            Ty::Ref(inner) => inner.as_ref().clone(),
            other => other.clone(),
        };
        let lhs_scalar_ty = match &lhs_ty {
            Ty::Ref(inner) => inner.as_ref().clone(),
            other => other.clone(),
        };
        if matches!(lhs_scalar_ty, Ty::Int | Ty::Float)
            && matches!(&rhs_handle_ty, Ty::Adt(id, _) if *id == cobrust_types::COIL_BUFFER_ADT)
        {
            if let Some(scalar_sym) = cobrust_types::lookup_buffer_left_scalar_binop(op) {
                // Argument ORDER matches the right-scalar shim ABI
                // `(buffer_ptr, k: f64)`: the BUFFER (the RHS expr) is the
                // borrowed handle, the SCALAR (the LHS expr) is `k`.
                let buf_op = upgrade_move_to_copy_handle(self.lower_expr(rhs)?);
                let scalar_op = self.lower_expr(lhs)?;
                // Pass the scalar as f64 (an `Int` is cast i64→f64; a
                // `Float` is already f64) — mirrors the right-scalar path.
                let k_op = if matches!(lhs_scalar_ty, Ty::Int) {
                    let kdest = self.declare_local("_coilk".to_string(), Ty::Float, span, false);
                    self.emit_assign(
                        Place::local(kdest),
                        Rvalue::Cast(CastKind::IntToFloat, scalar_op, Ty::Float),
                        span,
                    );
                    Operand::Copy(Place::local(kdest))
                } else {
                    scalar_op
                };
                let op_out = self.emit_ecosystem_call(
                    scalar_sym,
                    cobrust_types::coil_buffer_ty(),
                    vec![buf_op, k_op],
                    span,
                );
                return Ok(op_out);
            }
        }
        // ADR-0077 Q1 — `coil.Buffer` operator dispatch (the FIRST
        // ecosystem-handle operator). Sibling of the `in`/`not in` Dict
        // guard above: when the LHS resolves to the Buffer handle (bare
        // `a + b` → `Ty::Adt`, or borrowed `&a + &b` → `Ty::Ref(Adt)`),
        // retarget `+`/`-`/`*`/`/` to `__cobrust_coil_buffer_{add,sub,mul,
        // div}` via `emit_ecosystem_call` BEFORE the generic
        // `Rvalue::BinaryOp` tail. ADR-0077 Phase-2/3 extends the SAME
        // `lookup_buffer_binop` path to the six COMPARISON ops `<`/`<=`/
        // `>`/`>=`/`==`/`!=` → `__cobrust_coil_buffer_{lt,le,gt,ge,eq,ne}`
        // (returning a Bool-dtype Buffer); they reach here unintercepted
        // (the `str ==` guard below is gated on `Ty::Str`, and the Dict
        // `in`/`not in` guard above on `In`/`NotIn`), so no separate arm is
        // needed — the extended manifest row drives both typecheck and MIR.
        // Both operands are BORROWED handles (Move → Copy upgrade so the
        // source locals survive the call and drop once at scope exit per
        // ADR-0072 §5 risk 1); the shim returns a fresh handle the caller
        // owns. Because this emits a `Terminator::Call`, codegen's
        // `lower_binop` is never reached for Buffers (ADR-0077 §1.1) — no
        // codegen type-switch. The typecheck `synth_bin` arms already
        // rejected unsupported ops + non-Buffer operands, so a `Some` here
        // is an accepted op.
        if let Some(sig) = cobrust_types::lookup_buffer_binop(&lhs_ty, op) {
            let lhs_op = upgrade_move_to_copy_handle(self.lower_expr(lhs)?);
            let rhs_op = upgrade_move_to_copy_handle(self.lower_expr(rhs)?);
            let op_out = self.emit_ecosystem_call(
                sig.runtime_symbol,
                sig.ret.clone(),
                vec![lhs_op, rhs_op],
                span,
            );
            return Ok(op_out);
        }
        // ADR-0093 Phase 2 — `bytes + bytes` via the NATURAL `+` operator
        // (the `str + str` mirror, placed BEFORE it since both guard on
        // `op == Add`). Retargets to the always-linked
        // `__cobrust_bytes_concat(a, b) -> *mut Bytes`, which mints a fresh
        // `bytes` buffer (freed once by the `Ty::Bytes` drop schedule at
        // scope exit). Both operands are BORROWED (Move→Copy upgrade —
        // `__cobrust_bytes_concat` reads but does not consume, so the source
        // `bytes` locals survive for later uses + drop ONCE at scope exit,
        // per the `bytes` Move-only / non-Copy discipline). The result is
        // MOVED out (a Copy would double-free the freshly-minted buffer).
        if matches!(op, HirBinOp::Add) && matches!(lhs_ty, Ty::Bytes) {
            let lhs_op = upgrade_move_to_copy_handle(self.lower_expr(lhs)?);
            let rhs_op = upgrade_move_to_copy_handle(self.lower_expr(rhs)?);
            let dest = self.declare_local("_bytescat".to_string(), Ty::Bytes, span, false);
            let cur = self.current_block_id();
            let next = self.start_new_block();
            self.cur_block = Some(cur.0 as usize);
            self.terminate(Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_bytes_concat".to_string())),
                args: vec![lhs_op, rhs_op],
                destination: Place::local(dest),
                target: next,
                unwind: None,
            });
            self.cur_block = Some(next.0 as usize);
            return Ok(Operand::Move(Place::local(dest)));
        }
        // ADR-0106 / §2.5 — `str * int` / `int * str` REPETITION via the
        // NATURAL `*` operator (`"ab" * 3 == "ababab"`, the Python idiom an
        // LLM writes constantly; Maximize-training-data-overlap). The
        // typecheck `synth_bin` Mul arm accepted exactly one `Str` + one
        // `Int` (in EITHER order); retarget to the always-linked
        // `__cobrust_str_repeat(s: *mut Str, n: i64) -> *mut Str`, which
        // allocates a fresh Str buffer (freed once by the Str drop schedule
        // at scope exit). NORMALIZE both operand orders to `(str, int)`: the
        // `str` operand is the receiver `s`, the `int` operand the count
        // `n`. The `str` receiver is BORROWED (Move→Copy upgrade —
        // `__cobrust_str_repeat` reads but does not consume `s`, so the
        // source `str` local survives + drops ONCE at scope exit, the Str
        // non-Copy discipline); the `int` count is a Copy scalar lowered
        // directly (no borrow upgrade). Sibling of the `str + str` concat
        // arm below (same retarget-to-primitive + Move-out-fresh-buffer
        // shape). Codegen's `BinaryOp` Mul arm is never reached for this
        // shape (it assumes integer operands and would crash on the Str
        // PointerValue).
        if matches!(op, HirBinOp::Mul) {
            let rhs_ty = synth_expr_ty(self, rhs);
            // Determine which side is the `str` and which the `int`.
            let lhs_is_str = matches!(lhs_ty, Ty::Str);
            let rhs_is_str = matches!(rhs_ty, Ty::Str);
            if lhs_is_str ^ rhs_is_str {
                // Borrow the `str` receiver (Move→Copy); lower the `int`
                // count directly. Evaluate LHS then RHS to preserve
                // source-order side effects regardless of which is the str.
                let (s_op, n_op) = if lhs_is_str {
                    let s = upgrade_move_to_copy_handle(self.lower_expr(lhs)?);
                    let n = self.lower_expr(rhs)?;
                    (s, n)
                } else {
                    // `int * str`: evaluate the int LHS first, then the str.
                    let n = self.lower_expr(lhs)?;
                    let s = upgrade_move_to_copy_handle(self.lower_expr(rhs)?);
                    (s, n)
                };
                let dest = self.declare_local("_strrep".to_string(), Ty::Str, span, false);
                let cur = self.current_block_id();
                let next = self.start_new_block();
                self.cur_block = Some(cur.0 as usize);
                self.terminate(Terminator::Call {
                    func: Operand::Constant(Constant::Str("__cobrust_str_repeat".to_string())),
                    args: vec![s_op, n_op],
                    destination: Place::local(dest),
                    target: next,
                    unwind: None,
                });
                self.cur_block = Some(next.0 as usize);
                // `_strrep` owns a FRESHLY-allocated Str buffer (the repeat
                // shim's return). It must be MOVED out so the single owner
                // (the consuming binding) drops it ONCE; a Copy would leave
                // BOTH `_strrep` + the consuming local in the drop schedule
                // → a double-free. (Mirror of the `str + str` concat
                // Move-out discipline below.)
                return Ok(Operand::Move(Place::local(dest)));
            }
        }
        // ADR-0078 backend Phase 2 (fang JWT E2E sibling-fix) — `str + str`
        // via the NATURAL `+` operator (e.g. the append-tamper `t + "X"`).
        // The codegen `lower_binop` Add arm assumes integer operands
        // (`into_int_value()`), so concatenating two `Ty::Str` values would
        // crash codegen with "Found PointerValue but expected IntValue".
        // Retarget to the always-linked
        // `__cobrust_str_concat(a, b) -> *mut Str`, which allocates a fresh
        // Str buffer (freed once by the Str drop schedule at scope exit).
        // Sibling of the `str == str` arm below (same retarget-to-primitive
        // shape). Both operands are BORROWED (Move→Copy upgrade —
        // `__cobrust_str_concat` reads but does not consume, so the source
        // `str` locals survive for later uses and drop ONCE at scope exit,
        // per the Str non-Copy discipline).
        if matches!(op, HirBinOp::Add) && matches!(lhs_ty, Ty::Str) {
            let lhs_op = upgrade_move_to_copy_handle(self.lower_expr(lhs)?);
            let rhs_op = upgrade_move_to_copy_handle(self.lower_expr(rhs)?);
            let dest = self.declare_local("_strcat".to_string(), Ty::Str, span, false);
            let cur = self.current_block_id();
            let next = self.start_new_block();
            self.cur_block = Some(cur.0 as usize);
            self.terminate(Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_str_concat".to_string())),
                args: vec![lhs_op, rhs_op],
                destination: Place::local(dest),
                target: next,
                unwind: None,
            });
            self.cur_block = Some(next.0 as usize);
            // `_strcat` owns a FRESHLY-allocated Str buffer (the concat
            // shim's return). It must be MOVED out — exactly like
            // `emit_ecosystem_call`'s freshly-allocated Str returns — so
            // the single owner (the consuming binding) drops it ONCE. A
            // `Copy` here would leave BOTH `_strcat` and the consuming
            // local in the drop schedule → a double-free of the same
            // buffer → allocator corruption / hang at scope exit. (Contrast
            // the `str ==` arm below, which Copies an `Int`/`Bool` dest —
            // a Copy type with no drop.)
            return Ok(Operand::Move(Place::local(dest)));
        }
        // ADR-0078 backend Phase 2 (fang E2E sibling-fix) — `str == str` /
        // `str != str` via the NATURAL operator. The codegen `lower_binop`
        // Eq/NotEq arms assume integer operands (`into_int_value()`), so a
        // bare comparison of two `Ty::Str` LOCALS (e.g. `h1 != h2`) would
        // crash codegen with "Found PointerValue but expected IntValue".
        // Retarget to the always-linked `__cobrust_str_eq(a, b) -> i64`
        // (0/1) then materialise the bool: `!= 0` for Eq, `== 0` for NotEq.
        // Sibling of the Dict `in`/`not in` block above (same call-then-
        // compare shape). String-literal operands keep flowing through the
        // existing `str_eq_lit` PRELUDE path (this guard fires only when
        // the LHS resolves to a `Ty::Str` value); both operands are
        // BORROWED (Move→Copy upgrade — `__cobrust_str_eq` reads but does
        // not consume, so the source `str` locals survive for later uses
        // and drop ONCE at scope exit, per the Str non-Copy discipline).
        if matches!(op, HirBinOp::Eq | HirBinOp::NotEq) && matches!(lhs_ty, Ty::Str) {
            let lhs_op = upgrade_move_to_copy_handle(self.lower_expr(lhs)?);
            let rhs_op = upgrade_move_to_copy_handle(self.lower_expr(rhs)?);
            let raw_dest = self.declare_local("_streq".to_string(), Ty::Int, span, false);
            let cur = self.current_block_id();
            let next = self.start_new_block();
            self.cur_block = Some(cur.0 as usize);
            self.terminate(Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_str_eq".to_string())),
                args: vec![lhs_op, rhs_op],
                destination: Place::local(raw_dest),
                target: next,
                unwind: None,
            });
            self.cur_block = Some(next.0 as usize);
            // `__cobrust_str_eq` returns i64 1 (equal) / 0 (unequal). For
            // `==` the bool is `result != 0`; for `!=` it is `result == 0`.
            let cmp_op = if matches!(op, HirBinOp::NotEq) {
                BinOp::Eq
            } else {
                BinOp::NotEq
            };
            let bool_dest = self.declare_local("_streqb".to_string(), Ty::Bool, span, false);
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
        // F92 / ADR-0104 — `str < str` / `<=` / `>` / `>=` ORDERING via the
        // NATURAL operator (lexicographic, like Python). The codegen
        // `lower_binop` Lt/LtEq/Gt/GtEq arms assume integer/float operands
        // (`into_int_value()` / `into_float_value()`), so a bare ordering of
        // two `Ty::Str` values would crash codegen with "Found PointerValue
        // but expected IntValue" (the F85/F87/F92 codegen-panic class — the
        // typechecker ACCEPTS `str < str`, so it type-checks then crashes).
        // Retarget to the always-linked `__cobrust_str_cmp(a, b) -> i64`
        // (sign of `a.cmp(b)`: -1 / 0 / +1) then materialise the bool by
        // comparing that i64 against 0 with the SAME ordering op. Direct
        // sibling of the `str == str` arm above (call-then-compare shape);
        // both operands are BORROWED (Move→Copy upgrade — `__cobrust_str_cmp`
        // reads but does not consume, so the source `str` locals survive for
        // later uses and drop ONCE at scope exit, per the Str non-Copy
        // discipline).
        if matches!(
            op,
            HirBinOp::Lt | HirBinOp::LtEq | HirBinOp::Gt | HirBinOp::GtEq
        ) && matches!(lhs_ty, Ty::Str)
        {
            let lhs_op = upgrade_move_to_copy_handle(self.lower_expr(lhs)?);
            let rhs_op = upgrade_move_to_copy_handle(self.lower_expr(rhs)?);
            let raw_dest = self.declare_local("_strcmp".to_string(), Ty::Int, span, false);
            let cur = self.current_block_id();
            let next = self.start_new_block();
            self.cur_block = Some(cur.0 as usize);
            self.terminate(Terminator::Call {
                func: Operand::Constant(Constant::Str("__cobrust_str_cmp".to_string())),
                args: vec![lhs_op, rhs_op],
                destination: Place::local(raw_dest),
                target: next,
                unwind: None,
            });
            self.cur_block = Some(next.0 as usize);
            // `__cobrust_str_cmp` returns i64 -1/0/+1. The source ordering
            // `a OP b` is exactly `cmp(a, b) OP 0` (e.g. `a < b` ⇔ cmp < 0,
            // `a >= b` ⇔ cmp >= 0), so reuse the SAME comparison op against
            // the integer constant 0 — the int `lower_binop` arm handles it.
            let bool_dest = self.declare_local("_strcmpb".to_string(), Ty::Bool, span, false);
            self.emit_assign(
                Place::local(bool_dest),
                Rvalue::BinaryOp(
                    bin_to_mir(op),
                    Operand::Copy(Place::local(raw_dest)),
                    Operand::Constant(Constant::Int(0)),
                ),
                span,
            );
            return Ok(Operand::Copy(Place::local(bool_dest)));
        }
        // F90 / ADR-0102 — the `**` POWER operator. Codegen's `BinOp::Pow`
        // arm is an "unimplemented" reject (ADR-0041 §H3); we retarget the
        // accepted shapes to runtime shims here so codegen never reaches it
        // (sibling of the `str * int` / `str + str` retargets above). The
        // typecheck `synth_bin` arm already decided the result type:
        //   - `int ** int -> int`  → `__cobrust_ipow(base: i64, exp: i64)
        //     -> i64` (checked_pow; TRAPS on overflow / runtime-negative
        //     exponent — a negative LITERAL exponent was already a §2.5-A
        //     COMPILE-TIME reject at `synth_bin`).
        //   - any float operand `-> f64` → `__cobrust_math_pow(base: f64,
        //     exp: f64) -> f64` (libm `pow`). An `int` operand in the mixed
        //     `int ** float` / `float ** int` shape is cast i64→f64 via
        //     `CastKind::IntToFloat` (mirrors the coil buffer-scalar mixed-
        //     arg path above) so the shim receives genuine f64 values, NOT
        //     an i64 bit-pattern the f64 arg-coercion would mis-bitcast.
        if matches!(op, HirBinOp::Pow) {
            let lhs_num_ty = match &lhs_ty {
                Ty::Ref(inner) => inner.as_ref().clone(),
                other => other.clone(),
            };
            let rhs_ty = synth_expr_ty(self, rhs);
            let rhs_num_ty = match &rhs_ty {
                Ty::Ref(inner) => inner.as_ref().clone(),
                other => other.clone(),
            };
            let is_float_pow = matches!(lhs_num_ty, Ty::Float) || matches!(rhs_num_ty, Ty::Float);
            if is_float_pow {
                // float power → libm `pow`. Cast each `int` operand to f64
                // (narrow `IntN` too — it is `int` per check.rs `synth_bin`).
                let lhs_op = self.lower_expr(lhs)?;
                let base_op = if matches!(lhs_num_ty, Ty::Int | Ty::IntN(_)) {
                    let d = self.declare_local("_powbf".to_string(), Ty::Float, span, false);
                    self.emit_assign(
                        Place::local(d),
                        Rvalue::Cast(CastKind::IntToFloat, lhs_op, Ty::Float),
                        span,
                    );
                    Operand::Copy(Place::local(d))
                } else {
                    lhs_op
                };
                let rhs_op = self.lower_expr(rhs)?;
                let exp_op = if matches!(rhs_num_ty, Ty::Int | Ty::IntN(_)) {
                    let d = self.declare_local("_powef".to_string(), Ty::Float, span, false);
                    self.emit_assign(
                        Place::local(d),
                        Rvalue::Cast(CastKind::IntToFloat, rhs_op, Ty::Float),
                        span,
                    );
                    Operand::Copy(Place::local(d))
                } else {
                    rhs_op
                };
                let out = self.emit_ecosystem_call(
                    "__cobrust_math_pow",
                    Ty::Float,
                    vec![base_op, exp_op],
                    span,
                );
                return Ok(out);
            }
            // integer power → checked `__cobrust_ipow`. Operands are Copy
            // i64 scalars; no borrow upgrade.
            let lhs_op = self.lower_expr(lhs)?;
            let rhs_op = self.lower_expr(rhs)?;
            let out =
                self.emit_ecosystem_call("__cobrust_ipow", Ty::Int, vec![lhs_op, rhs_op], span);
            return Ok(out);
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
        // ADR-0089 §6 (F87) — type the `_bin` temp with the RESOLVED scalar
        // result type instead of the bug-prone `Ty::None`, so a downstream
        // per-arg-shape dispatch that reads `local_ty[_bin]` (the print
        // rewrite's `__cobrust_println_int`-vs-`_float` choice) sees `Float`
        // for `print(7.0 / 2.0)`. Without this the `_bin` temp stays
        // `Ty::None`, which the print rewrite maps to `Ty::Int` →
        // `__cobrust_println_int(i64)` is handed the f64 binop value →
        // LLVM module-verify fail ("Call parameter type does not match
        // function signature"). Mirrors the `lower_un` `_un` fix exactly,
        // and shares `synth_bin_result_ty` with `synth_expr_ty`'s `Bin` arm
        // (one source of truth — an inline binop arg and a declared-typed
        // var resolve identically). Non-scalar (Buffer/Str/Dict) shapes
        // already returned earlier; `synth_bin_result_ty` yields `Ty::None`
        // for any leftover non-scalar pair, preserving existing behaviour.
        let temp_ty = synth_bin_result_ty(op, &lhs_ty, &synth_expr_ty(self, rhs));
        let temp = self.declare_local("_bin".to_string(), temp_ty, span, false);
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
        // ADR-0089 §5 — type the unary temp with the result type (instead
        // of the bug-prone `Ty::None`) so downstream per-arg-shape
        // dispatch (e.g. the intrinsic-rewrite `Kind::MathAbs` int arm,
        // which reads `local_ty[_un]`) sees `Int` for `abs(-5)`. `-x` /
        // `+x` preserve the operand numeric type; `~x` is `Int`; `not x`
        // is `Bool`. Mirrors the `synth_expr_ty` `Un` arm exactly.
        let temp_ty = match op {
            UnaryOp::Neg | UnaryOp::Plus => synth_expr_ty(self, operand),
            UnaryOp::BitNot => Ty::Int,
            UnaryOp::Not => Ty::Bool,
        };
        let temp = self.declare_local("_un".to_string(), temp_ty, span, false);
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
/// Collect positional argument expressions from a `[CallArg]` slice.
/// Keyword / *args / **kwargs args are filtered out — ecosystem calls
/// (ADR-0072 / ADR-0073) accept positional args only.
fn collect_positional_args(args: &[CallArg]) -> Vec<&Expr> {
    args.iter()
        .filter_map(|a| match a {
            CallArg::Positional(e) => Some(e),
            _ => None,
        })
        .collect()
}

/// ADR-0073 §2 D2 — lower one ecosystem-call argument per its declared
/// [`cobrust_types::EcoParam`] kind.
///
/// - `Value(_)` slots use the existing path: `lower_expr` + Str
///   copy-upgrade. The receiver-borrow upgrade for handle args is
///   ecosystem-call-wide (the receiver itself, not the args).
/// - `Callback(_)` slots emit `Operand::Constant(Constant::FnRef(rn.def_id.0))`
///   directly from the source `ExprKind::Name(rn)`. The type-checker has
///   already verified that the argument is a top-level `fn` name with a
///   compatible signature ([`cobrust_types::check::Ctx::check_callback_arg`]),
///   so the MIR lowering can assume that shape.
fn lower_eco_arg(
    b: &mut BodyBuilder<'_>,
    arg: &Expr,
    kind: &cobrust_types::EcoParam,
) -> Result<Operand, MirError> {
    match kind {
        cobrust_types::EcoParam::Value(_) => {
            let op = b.lower_expr(arg)?;
            // Str args borrow (M-F.3.6). ADR-0077 Phase 2a: an
            // ecosystem-handle `Value` arg ALSO borrows — the coil shims
            // that take a handle by `Value` (`coil.broadcast_to(a, n)`,
            // `coil.mean(a)`, and the new `a.dot(b)` RHS) all take `&Array`
            // and never rebox/free it, exactly like a handle receiver
            // (ADR-0072 §5 risk 1). Upgrading Move→Copy keeps the source
            // local live so its single scope-exit drop still fires — and,
            // critically, lets the reused-handle form `a.dot(a)` pass the
            // SAME live `a` as both the (already-Copy) receiver and the
            // arg without a use-after-move / skipped-drop leak (the
            // Phase-1 `&a * &a` reused-handle contract, now in method-arg
            // form). No consuming shim takes a handle by `Value` (the App
            // is consumed as a RECEIVER, not an arg), so this is sound
            // across the whole manifest.
            Ok(upgrade_move_to_copy_for_eco_value(b, op))
        }
        cobrust_types::EcoParam::Callback(_) => {
            // The typechecker pre-checked that `arg` is a bare
            // `ExprKind::Name` whose `DefKind` is `Fn`. We mirror that
            // pattern in defense-in-depth: any deviation surfaces as a
            // codegen-time `MirError::UnsupportedExpr` (which the
            // typechecker should have caught first).
            match &arg.kind {
                ExprKind::Name(rn) if rn.kind == DefKind::Fn => {
                    Ok(Operand::Constant(Constant::FnRef(rn.def_id.0)))
                }
                _ => Err(MirError::Internal(
                    "ecosystem callback slot expects a top-level `fn` NAME at MIR (ADR-0073 §2 D2 — the typechecker should have rejected this)".to_string(),
                )),
            }
        }
    }
}

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

/// ADR-0093 — upgrade a `len(b)` argument operand `Move → Copy` when it
/// is a `Ty::Bytes` local (a `bytes` value is operand-Move, the
/// Str-mirror, so the bare read would move-out the local). `len` BORROWS
/// the handle for `__cobrust_bytes_len`; passing it by Copy keeps the
/// source `b` local live for a later `b[i]` read AND its single
/// scope-exit drop. The byte-typed sibling of
/// [`upgrade_move_to_copy_for_str`].
fn upgrade_move_to_copy_for_bytes(b: &BodyBuilder<'_>, op: Operand) -> Operand {
    match op {
        Operand::Move(ref p) => {
            if let Some(decl) = b.locals.get(p.local.0 as usize) {
                if matches!(decl.ty, Ty::Bytes) {
                    return Operand::Copy(p.clone());
                }
            }
            op
        }
        other => other,
    }
}

/// ADR-0077 Phase 2a — upgrade an ecosystem-call `Value` arg operand
/// `Move → Copy` when it is a `Str` (M-F.3.6 borrow-not-move) OR an
/// ecosystem-handle (the coil borrow-shims take `&Array`; see
/// `lower_eco_arg`). Subsumes [`upgrade_move_to_copy_for_str`] for the
/// `Value`-arg path: a handle arg must stay live so its scope-exit drop
/// fires and the reused-handle form `a.dot(a)` does not move-out the
/// local it also borrows as the receiver.
fn upgrade_move_to_copy_for_eco_value(b: &BodyBuilder<'_>, op: Operand) -> Operand {
    match op {
        Operand::Move(ref p) => {
            if let Some(decl) = b.locals.get(p.local.0 as usize) {
                let borrow = matches!(decl.ty, Ty::Str | Ty::Bytes)
                    || matches!(&decl.ty, Ty::Adt(id, _)
                        if cobrust_types::is_ecosystem_handle(*id));
                if borrow {
                    return Operand::Copy(p.clone());
                }
            }
            op
        }
        other => other,
    }
}

/// ADR-0072 §5 risk 1 — upgrade an ecosystem-handle receiver operand
/// `Move → Copy`. The `__cobrust_den_*` shims BORROW their handle
/// receiver (`&mut *(ptr as *mut T)`); they never rebox/free it. Passing
/// the receiver by Copy keeps the handle local live so the drop schedule
/// still inserts its single scope-exit drop. (A `Move` would consume the
/// local and the drop pass would treat it as moved-out — skipping the
/// drop and leaking the Boxed handle.)
fn upgrade_move_to_copy_handle(op: Operand) -> Operand {
    match op {
        Operand::Move(ref p) => Operand::Copy(p.clone()),
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

/// F83 / ADR-0106 — extract a compile-time integer value from an index
/// expression for a tuple `t[i]` read. Accepts a positive int literal and a
/// Python-style negated literal (`-1`, `-2`, …). Mirror of the type checker's
/// `literal_int_value` (check.rs) so the MIR `Projection::Field` offset matches
/// the element type the checker resolved. Returns `None` for any non-literal
/// (the checker rejects those with `NotIndexable`; MIR never folds them).
fn literal_int_value_mir(e: &Expr) -> Option<i64> {
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
        // F83 / ADR-0106 — a tuple literal synthesises `Ty::Tuple` with the
        // per-position element types, so the `ExprKind::Index` lowering can
        // route a `t[i]` read to the `Projection::Field` path (NOT the generic
        // `Projection::Index` stub that returned `Int(0)`). Mirrors the
        // construction arm above (`elem_tys` per position).
        ExprKind::Tuple(items) => Ty::Tuple(items.iter().map(|it| synth_expr_ty(b, it)).collect()),
        ExprKind::Index { base, index } => {
            // For `xs[i]`, the result is the element type of xs.
            match synth_expr_ty(b, base) {
                Ty::List(elem) => *elem,
                Ty::Dict(_, v) => *v,
                Ty::Str => Ty::Str,
                // ADR-0077 Q2 — `coil.Buffer` index: a SCALAR `a[i]`
                // (`IndexKind::Expr`) yields an `f64` (NOT a Buffer — the
                // drop schedule must not treat a scalar read as a
                // drop-eligible handle); a SLICE `a[lo:hi]`
                // (`IndexKind::Slice`) yields a fresh OWNED `coil.Buffer`
                // (drop-scheduled once at scope exit).
                Ty::Adt(id, args) if id == cobrust_types::COIL_BUFFER_ADT => match index.as_ref() {
                    IndexKind::Slice { .. } => Ty::Adt(id, args),
                    _ => Ty::Float,
                },
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
            // ADR-0072 §2/§3 — ecosystem call return types so a chained
            // `conn.execute(sql).fetchall()` resolves the inner call to
            // its handle `Ty::Adt` (driving the outer method dispatch +
            // the let-binding's drop schedule).
            if let ExprKind::Attr { base, name } = &callee.kind {
                // ADR-0079 Q4-a — sub-namespaced call return type
                // (`coil.linalg.solve(...) -> Buffer`) so the let-binding's
                // drop schedule sees the owned-handle return + drops it once
                // at scope exit. Matches the inner-`Attr` base before the
                // flat `Name(rn)` module-fn path below.
                if let ExprKind::Attr {
                    base: ns_base,
                    name: subns,
                } = &base.kind
                {
                    if let ExprKind::Name(rn) = &ns_base.kind {
                        if rn.kind == DefKind::ImportAlias
                            && cobrust_types::is_subnamespace(rn.name.as_str(), subns)
                        {
                            if let Some(sig) =
                                cobrust_types::lookup_subnamespace_fn(rn.name.as_str(), subns, name)
                            {
                                return sig.ret;
                            }
                        }
                    }
                }
                if let ExprKind::Name(rn) = &base.kind {
                    if rn.kind == DefKind::ImportAlias
                        && cobrust_types::is_ecosystem_module(rn.name.as_str())
                    {
                        if let Some(sig) = cobrust_types::lookup_module_fn(rn.name.as_str(), name) {
                            return sig.ret;
                        }
                    }
                }
                let base_ty = synth_expr_ty(b, base);
                if let Ty::Adt(id, _) = &base_ty {
                    if cobrust_types::is_ecosystem_handle(*id) {
                        if let Some(sig) = cobrust_types::lookup_handle_method(&base_ty, name) {
                            return sig.ret;
                        }
                    }
                }
                // ADR-0093 Phase 2 — str/bytes method-form call return type
                // (so a CHAINED `s.encode().decode()` resolves the inner
                // `.encode()` to `Ty::Bytes`, driving the outer `.decode()`
                // method dispatch + the let-binding's drop schedule). The
                // method rewrites to a PRELUDE fn whose return type is the
                // call's type. Without this, a chained method call's base
                // synthed to `Ty::None`, so `method_form_rewrite_name` on
                // the outer call did not match (returned `None`) and the
                // outer call fell through to the broken generic `Attr` path
                // (empty / wrong output). Sibling of the `Name`-callee
                // Fn-return resolution above.
                if let Some(rewritten) = method_form_rewrite_name(b, base, name.as_str()) {
                    if let Some(def_id) = b.ctx.lookup_fn_def_id(&rewritten) {
                        if let Ty::Fn(fn_ty) = b.ctx.lookup_ty(def_id) {
                            return (*fn_ty.return_ty).clone();
                        }
                    }
                }
            }
            Ty::None
        }
        // ADR-0052a Wave-1 — `&expr` borrow synthesises `Ty::Ref(inner)`
        // (mirrors the type checker, check.rs `ExprKind::Borrow` arm). The
        // `lower_bin` Buffer guard (ADR-0077 Q1) relies on this to detect
        // `&a + &b` where both operands resolve to `Ty::Ref(Buffer)`; and
        // `method_form_rewrite_name` already unwraps the `Ty::Ref` it
        // expects from this helper.
        ExprKind::Borrow(inner) => Ty::Ref(Box::new(synth_expr_ty(b, inner))),
        // ADR-0089 §5 — unary-op result type. `-x` / `+x` preserve the
        // operand's numeric type (so `abs(-5)`'s arg synths to `Int`,
        // driving the type-preserving int-abs dest + lowering); `~x` is
        // `Int`; `not x` is `Bool`. Without this, a literal-neg
        // `-5` synthesised `Ty::None` and `abs(-5)` fell through to the
        // f64 path (re-interpreting the i64 bits as a double → NaN).
        ExprKind::Un { op, operand } => match op {
            UnaryOp::Neg | UnaryOp::Plus => synth_expr_ty(b, operand),
            UnaryOp::BitNot => Ty::Int,
            UnaryOp::Not => Ty::Bool,
        },
        // ADR-0077 Q3 — parens-free handle attribute (`a.shape` etc.).
        // Resolve the manifest attr return type so the let-binding's drop
        // schedule sees the right type (e.g. `a.shape` is an owned
        // `list[i64]` that must drop once at scope exit).
        ExprKind::Attr { base, name } => {
            // ADR-0083 — a `math` module CONSTANT attribute synthesises
            // `Ty::Float` (so a `let p: f64 = math.pi` binding + a
            // `print(math.pi)` float-dispatch resolve correctly). Checked
            // BEFORE the handle-attr path: the base is a `Name` import alias,
            // never a Buffer handle, so the two cases never overlap.
            if let ExprKind::Name(rn) = &base.kind {
                if rn.kind == DefKind::ImportAlias
                    && cobrust_types::lookup_module_const(rn.name.as_str(), name).is_some()
                {
                    return Ty::Float;
                }
            }
            let base_ty = synth_expr_ty(b, base);
            if let Some(sig) = cobrust_types::lookup_handle_attr(&base_ty, name) {
                return sig.ret;
            }
            Ty::None
        }
        // ADR-0089 §5 (extended to BINARY ops) — a COMPUTED scalar binary
        // expression like `abs(a - b)` must synth its result to `Int` so the
        // type-preserving int-abs dest + lowering fire (NOT the f64 path that
        // re-interpreted the i64 bits as a double → NaN, the silent-miscompile
        // class of the unary `-5` above but for binary expressions). CONSERVATIVE
        // + Buffer-SAFE: only synthesises when BOTH operands already resolve to a
        // scalar `Int`/`Float` — a `coil.Buffer` operand is a `Ty::Adt`, so
        // Buffer arithmetic/comparison stays `Ty::None` and is resolved by the
        // existing Buffer-binop path. Arithmetic → Float if either operand is
        // Float else Int; scalar comparisons → Bool; bit/shift → Int.
        ExprKind::Bin { op, lhs, rhs } => {
            let lt = synth_expr_ty(b, lhs);
            let rt = synth_expr_ty(b, rhs);
            synth_bin_result_ty(*op, &lt, &rt)
        }
        // Ternary (F93 / ADR-0105) — both arms share a type (checker
        // unified them); the `then` arm's synth type is the result type,
        // so a `str`/`list` ternary participates in drop dispatch.
        ExprKind::IfExpr {
            then_branch,
            else_branch,
            ..
        } => {
            let then_ty = synth_expr_ty(b, then_branch);
            if matches!(then_ty, Ty::None) {
                synth_expr_ty(b, else_branch)
            } else {
                then_ty
            }
        }
        _ => Ty::None,
    }
}

/// ADR-0089 §6 (F87 extension) — scalar binary-op result type from the
/// two operand types. The SINGLE source of truth shared by
/// `synth_expr_ty`'s `Bin` arm AND `lower_bin`'s `_bin` temp declaration,
/// so an INLINE binop arg (`print(7.0 / 2.0)`) and a declared-typed var
/// (`let x: f64 = 7.0 / 2.0; print(x)`) resolve identically — the F87
/// print-dispatch miscompile fix (a Float `_bin` temp must NOT default to
/// `Ty::None`, which the print rewrite maps to `Ty::Int` →
/// `__cobrust_println_int(i64)` fed an f64 → LLVM verify fail).
///
/// CONSERVATIVE + Buffer-SAFE: only resolves when BOTH operands already
/// resolve to a scalar `Int`/`Float` (a `coil.Buffer` operand is a
/// `Ty::Adt`, so Buffer arithmetic/comparison stays `Ty::None` and is
/// resolved by the existing Buffer-binop path). Arithmetic → Float if
/// either operand is Float else Int; scalar comparisons → Bool; bit/shift
/// → Int; everything else → `Ty::None` (unchanged behaviour).
fn synth_bin_result_ty(op: HirBinOp, lt: &Ty, rt: &Ty) -> Ty {
    let both_scalar = matches!(lt, Ty::Int | Ty::Float) && matches!(rt, Ty::Int | Ty::Float);
    match op {
        HirBinOp::Eq
        | HirBinOp::NotEq
        | HirBinOp::Lt
        | HirBinOp::LtEq
        | HirBinOp::Gt
        | HirBinOp::GtEq
            if both_scalar =>
        {
            Ty::Bool
        }
        HirBinOp::Add
        | HirBinOp::Sub
        | HirBinOp::Mul
        | HirBinOp::Div
        | HirBinOp::FloorDiv
        | HirBinOp::Mod
        | HirBinOp::Pow
            if both_scalar =>
        {
            if matches!(lt, Ty::Float) || matches!(rt, Ty::Float) {
                Ty::Float
            } else {
                Ty::Int
            }
        }
        HirBinOp::Shl | HirBinOp::Shr | HirBinOp::BitAnd | HirBinOp::BitOr | HirBinOp::BitXor
            if matches!(lt, Ty::Int) && matches!(rt, Ty::Int) =>
        {
            Ty::Int
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

/// ADR-0052d-prereq §"Decision" — method-form rewrite-name resolver.
///
/// Given a method-call `base.method_name(...)`, return the PRELUDE-fn
/// name the method-form rewrites to (e.g. `("hello": Str, "len") ->
/// Some("str_len")`). Returns `None` when:
/// - `base`'s type is not one of the 5 recognised method-table
///   receivers (Str / List / Float / Int — Dict is sub-sprint d's
///   stretch goal).
/// - The (receiver, method) pair is not in the per-type table.
///
/// This mirrors `crates/cobrust-types/src/check.rs::try_synth_*_method`
/// exactly. Any divergence between the two sides is a Wave-2 ratification
/// bug.
fn method_form_rewrite_name(b: &BodyBuilder<'_>, base: &Expr, method_name: &str) -> Option<String> {
    let base_ty = synth_expr_ty(b, base);
    let resolved = match &base_ty {
        Ty::Ref(inner) => (**inner).clone(),
        other => other.clone(),
    };
    match resolved {
        Ty::Str => match method_name {
            "len" => Some("str_len".to_string()),
            "split" => Some("split".to_string()),
            "replace" => Some("replace".to_string()),
            // ADR-0085: Python-canonical aliases route to the SAME PRELUDE
            // fn (and thus the SAME runtime symbol) as their Rust twin —
            // `strip` -> trim, `startswith` -> starts_with, `endswith` ->
            // ends_with. No duplicate shim. `lstrip` / `rstrip` / `count`
            // are NEW PRELUDE fns with their own shims.
            "trim" | "strip" => Some("trim".to_string()),
            "lstrip" => Some("lstrip".to_string()),
            "rstrip" => Some("rstrip".to_string()),
            "count" => Some("count".to_string()),
            "find" => Some("find".to_string()),
            "contains" => Some("contains".to_string()),
            "starts_with" | "startswith" => Some("starts_with".to_string()),
            "ends_with" | "endswith" => Some("ends_with".to_string()),
            "lower" => Some("lower".to_string()),
            "upper" => Some("upper".to_string()),
            // ADR-0093 Phase 2 — `s.encode() -> bytes` (UTF-8 encode).
            // Routes to the `bytes_from_str` PRELUDE fn (whose `-> bytes`
            // return type drives the `_callret` drop schedule) ->
            // `__cobrust_bytes_from_str(s)`. The receiver `s` is borrowed
            // (the shim reads the Str bytes; see the `is_bytes_borrow`
            // set in `lower_rewritten_method_call`).
            "encode" => Some("bytes_from_str".to_string()),
            _ => None,
        },
        // ADR-0093 Phase 2 — the `bytes` method table: `.decode() -> str`
        // (UTF-8; traps on invalid UTF-8 — §2.2) + `.hex() -> str`
        // (lowercase hex). Both route to PRELUDE fns whose `-> str` return
        // type drives the `_callret` drop schedule, and whose `bytes`
        // receiver is borrowed (read-only) by the shim.
        Ty::Bytes => match method_name {
            "decode" => Some("bytes_decode".to_string()),
            "hex" => Some("bytes_hex".to_string()),
            _ => None,
        },
        Ty::List(_) => match method_name {
            // ADR-0052d-prereq §4: `xs.len()` rewrites to `len(xs)`
            // per the surface table, but at MIR-lower time we target
            // `list_len` directly because the intrinsic-rewrite of
            // `len` is dict-only (cobrust-cli intrinsics.rs:1567
            // dispatches `len` → `__cobrust_dict_len`). The two PRELUDE
            // names are arity-1 List receivers either way; `list_len`
            // is the codegen-safe route. f30wit_method_02 admits both
            // (it checks subset-of-prelude-fn-form callees, and the
            // PRELUDE-fn comparison source also reads `list_len`).
            "len" => Some("list_len".to_string()),
            "push" => Some("list_push".to_string()),
            "get" => Some("list_get".to_string()),
            "set" => Some("list_set".to_string()),
            "is_empty" => Some("list_is_empty".to_string()),
            _ => None,
        },
        Ty::Float => match method_name {
            "floor" => Some("floor".to_string()),
            "ceil" => Some("ceil".to_string()),
            "is_nan" => Some("is_nan".to_string()),
            "is_finite" => Some("is_finite".to_string()),
            "abs" => Some("abs_f".to_string()),
            _ => None,
        },
        Ty::Int => match method_name {
            "abs" => Some("abs".to_string()),
            "pow" => Some("pow".to_string()),
            "min" => Some("min".to_string()),
            "max" => Some("max".to_string()),
            "bit_count" => Some("bit_count".to_string()),
            _ => None,
        },
        _ => None,
    }
}
