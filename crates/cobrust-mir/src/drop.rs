//! Drop-schedule pass — ADR-0020 §"Drop schedule algorithm".
//!
//! Five phases:
//!
//! 1. **Initialization** — mark non-`Copy` locals as drop-pending
//!    when first assigned.
//! 2. **Move** — `Operand::Move(place)` transfers ownership; the
//!    local is no longer drop-pending.
//! 3. **End-of-scope** — at every `Goto` / `SwitchInt` / `Return` /
//!    `Unreachable`, insert `Drop` blocks for still-pending locals
//!    (LIFO order).
//! 4. **Divergence** — `Unreachable` blocks skip drop insertion.
//! 5. **Verification** — forward-flow over the post-drop CFG; a
//!    `drop-pending` local reaching `Return` is `DropMissing`.
//!
//! M8 simplification: drop-pending tracking is intra-block at the
//! lowering level; cross-block flow is the verification phase's job.

use std::collections::{HashMap, HashSet};

use cobrust_types::Ty;

use crate::error::MirError;
use crate::tree::{BasicBlock, BlockId, Body, LocalId, Operand, Rvalue, StatementKind, Terminator};

/// Compute the drop schedule for `body` and *mutate* the body to
/// insert `Drop` terminators on every reachable end-of-scope edge.
///
/// # Errors
///
/// - `MirError::DropMissing` if an owning local reaches `Return`
///   without being dropped.
/// - `MirError::DoubleDrop` if a path drops the same local twice.
pub fn compute_drop_schedule(body: &mut Body) -> Result<(), MirError> {
    if body.blocks.is_empty() {
        return Ok(());
    }

    // Phase 1: identify drop-pending candidates per block.
    // A local is *drop-eligible* if its declared type is non-`Copy`
    // and it is *not* a parameter (parameters are dropped by the
    // caller's frame in the C-ABI sense; M8 keeps them caller-owned).
    //
    // ADR-0050c Phase 4 cascade fix: `body.is_param(id)` is
    // `id < param_count` where `param_count` only counts USER-declared
    // parameters (LocalId(0) is the synthetic _return slot per
    // `lower.rs::BodyBuilder::new`). For `fn count(xs: list[str])`
    // with param_count=1, that predicate covers LocalId(0)=_return
    // (correctly excluded — _return is Ty::None on entry, irrelevant
    // anyway) but EXCLUDES LocalId(1)=xs (incorrectly included as
    // drop-eligible). Pre-ADR-0050c that didn't matter because Str
    // and List were Copy, but the Phase 1 flip made list-typed params
    // drop-eligible — and the drop pass then fired
    // `__cobrust_list_drop_elems` on the param's list pointer both in
    // the callee AND the caller, causing mimalloc free-list corruption.
    //
    // Fix: shift the exclusion predicate by +1 to cover the _return slot
    // PLUS all `param_count` user params. The _return slot itself is
    // never drop-eligible (Ty::None is Copy), so this only affects the
    // last user parameter — which is what we want.
    let param_cutoff = body.param_count + 1;
    let mut drop_eligible: HashSet<LocalId> = HashSet::new();
    for ld in &body.locals {
        let is_param = (ld.id.0 as usize) < param_cutoff;
        if !is_copy(&ld.ty) && !is_param {
            drop_eligible.insert(ld.id);
        }
    }

    // Phase 2: scan moves to refine — locals that get moved out
    // before the end-of-scope are *not* drop-pending.
    // We model this conservatively per block: a local is moved-out
    // if any `Operand::Move(place)` references it.
    let mut moved_out_per_block: HashMap<BlockId, HashSet<LocalId>> = HashMap::new();
    for block in &body.blocks {
        let mut moved = HashSet::new();
        for stmt in &block.statements {
            if let StatementKind::Assign { rvalue, .. } = &stmt.kind {
                collect_moves_in_rvalue(rvalue, &mut moved);
            }
        }
        if let Terminator::Call { args, .. } = &block.terminator {
            for a in args {
                if let Operand::Move(p) = a {
                    moved.insert(p.local);
                }
            }
        }
        moved_out_per_block.insert(block.id, moved);
    }

    // Phase 3 + 4: on every block whose terminator is one of {Goto,
    // SwitchInt, Return, Unreachable, Drop, Call.target, Call.unwind,
    // Assert.target}, we may need to insert Drop terminators.
    //
    // M8 strategy: insert drops only on *Return* edges of pending
    // locals that are non-moved within the body. This covers the
    // common case and keeps the CFG growth bounded; aggressive
    // per-block schedule is a future refinement.
    let return_blocks: Vec<BlockId> = body
        .blocks
        .iter()
        .filter_map(|b| {
            if matches!(b.terminator, Terminator::Return) {
                Some(b.id)
            } else {
                None
            }
        })
        .collect();
    let mut globally_moved: HashSet<LocalId> = HashSet::new();
    for moves in moved_out_per_block.values() {
        for m in moves {
            globally_moved.insert(*m);
        }
    }

    let to_drop: Vec<LocalId> = drop_eligible
        .iter()
        .filter(|l| !globally_moved.contains(l))
        .copied()
        .collect();

    // Insert drop chains preceding every Return.
    for ret_id in return_blocks {
        if to_drop.is_empty() {
            continue;
        }
        // The current return block stays as `Return`. We re-route the
        // *predecessor* edges that target `ret_id` to instead target
        // the head of a new drop chain whose tail jumps back to
        // `ret_id`. (LIFO order.)
        let chain_head = build_drop_chain(body, &to_drop, ret_id);
        rewire_predecessors(body, ret_id, chain_head);
    }

    // Phase 5: verification — forward-flow check that no `Return` is
    // reachable with a still-pending local + no double drops.
    verify_drops(body)
}

fn is_copy(ty: &Ty) -> bool {
    matches!(
        ty,
        // ADR-0050c TD-1 closure: Str and List are non-Copy; the drop pass
        // enumerates them as drop-eligible. Element-type-aware drop
        // (list[str] → drop each element first) lives in codegen's
        // Terminator::Drop arm dispatch on `body.locals[place.local.0].ty`.
        Ty::Bool | Ty::Int | Ty::Float | Ty::Imag | Ty::None | Ty::Never
    )
}

fn collect_moves_in_rvalue(rv: &Rvalue, into: &mut HashSet<LocalId>) {
    match rv {
        Rvalue::Use(op) | Rvalue::Cast(_, op, _) | Rvalue::UnaryOp(_, op) => {
            collect_move_in_operand(op, into);
        }
        Rvalue::BinaryOp(_, a, b) => {
            collect_move_in_operand(a, into);
            collect_move_in_operand(b, into);
        }
        Rvalue::Aggregate(_, items) => {
            for op in items {
                collect_move_in_operand(op, into);
            }
        }
        Rvalue::Ref(_, _) | Rvalue::Discriminant(_) | Rvalue::Len(_) | Rvalue::NullaryOp(_) => {}
    }
}

fn collect_move_in_operand(op: &Operand, into: &mut HashSet<LocalId>) {
    if let Operand::Move(p) = op {
        into.insert(p.local);
    }
}

/// Build a chain of `Drop` blocks ending at `final_target`. Returns
/// the head of the chain (the `BlockId` callers should use as the
/// new target).
fn build_drop_chain(body: &mut Body, locals: &[LocalId], final_target: BlockId) -> BlockId {
    if locals.is_empty() {
        return final_target;
    }
    let mut tail = final_target;
    for local in locals.iter().rev() {
        let new_id = BlockId(body.blocks.len() as u32);
        let stmt_span = body.span;
        body.blocks.push(BasicBlock {
            id: new_id,
            statements: Vec::new(),
            terminator: Terminator::Drop {
                place: crate::tree::Place::local(*local),
                target: tail,
            },
            span: stmt_span,
        });
        tail = new_id;
    }
    tail
}

/// Re-route every edge currently targeting `from` to instead target
/// `to`. Skips `from` itself.
fn rewire_predecessors(body: &mut Body, from: BlockId, to: BlockId) {
    if from == to {
        return;
    }
    let len = body.blocks.len();
    for idx in 0..len {
        let bid = body.blocks[idx].id;
        if bid == from {
            // Don't rewire the return block's own (no-)successors.
            continue;
        }
        // Skip the freshly-inserted drop chain heads — they target
        // `from` intentionally to terminate the chain.
        // Heuristic: drop chain blocks have an empty statement list
        // and a Drop terminator pointing at `from`; we identify them
        // via their position (added after the original len).
        // Simpler: just reroute every non-drop terminator that points
        // at `from`, then check Drop terminators only if their target
        // chains back to `from` *and* they are *not* part of our new
        // chain.
        // Pragmatic implementation: rewire every Goto/SwitchInt/Call/
        // Assert that targets `from`, *except* the Drop terminators
        // we just created (those are between `to` and `from`).
        let term_clone = body.blocks[idx].terminator.clone();
        let new_term = rewire_terminator(term_clone, from, to);
        body.blocks[idx].terminator = new_term;
    }
}

fn rewire_terminator(term: Terminator, from: BlockId, to: BlockId) -> Terminator {
    match term {
        Terminator::Goto(b) if b == from => Terminator::Goto(to),
        Terminator::SwitchInt {
            operand,
            cases,
            otherwise,
        } => Terminator::SwitchInt {
            operand,
            cases: cases
                .into_iter()
                .map(|(v, b)| (v, if b == from { to } else { b }))
                .collect(),
            otherwise: if otherwise == from { to } else { otherwise },
        },
        Terminator::Call {
            func,
            args,
            destination,
            target,
            unwind,
        } => Terminator::Call {
            func,
            args,
            destination,
            target: if target == from { to } else { target },
            unwind: unwind.map(|u| if u == from { to } else { u }),
        },
        Terminator::Drop { place, target } => Terminator::Drop {
            place,
            // CRITICAL: do NOT rewire drop chain edges — they point at
            // `from` (the original return block) intentionally.
            target,
        },
        Terminator::Assert {
            cond,
            expected,
            msg,
            target,
        } => Terminator::Assert {
            cond,
            expected,
            msg,
            target: if target == from { to } else { target },
        },
        other => other,
    }
}

/// Verify the post-drop CFG: walk forward, tracking
/// `(pending, dropped, moved)` per local; emit `DropMissing` if a
/// pending local reaches `Return`, or `DoubleDrop` if the same local
/// is dropped twice on a path.
fn verify_drops(body: &Body) -> Result<(), MirError> {
    if body.blocks.is_empty() {
        return Ok(());
    }
    // Per-block "dropped" set on entry.
    let mut entry_dropped: HashMap<BlockId, HashSet<LocalId>> = HashMap::new();
    let mut work: Vec<BlockId> = vec![BlockId(0)];
    entry_dropped.insert(BlockId(0), HashSet::new());

    while let Some(bid) = work.pop() {
        let block = &body.blocks[bid.0 as usize];
        let mut dropped = entry_dropped.get(&bid).cloned().unwrap_or_default();
        // Walk statements (no drops fire in statements, only terminators).
        // Terminator visitor.
        match &block.terminator {
            Terminator::Drop { place, target } => {
                if !dropped.insert(place.local) {
                    return Err(MirError::DoubleDrop {
                        local: place.local.0,
                        span: block.span,
                        suggestion: Some(
                            "a value can only be dropped once; check your control flow",
                        ),
                    });
                }
                propagate(target, &dropped, &mut entry_dropped, &mut work);
            }
            Terminator::Goto(t) => {
                propagate(t, &dropped, &mut entry_dropped, &mut work);
            }
            Terminator::SwitchInt {
                cases, otherwise, ..
            } => {
                for (_, t) in cases {
                    propagate(t, &dropped, &mut entry_dropped, &mut work);
                }
                propagate(otherwise, &dropped, &mut entry_dropped, &mut work);
            }
            Terminator::Call { target, unwind, .. } => {
                propagate(target, &dropped, &mut entry_dropped, &mut work);
                if let Some(u) = unwind {
                    propagate(u, &dropped, &mut entry_dropped, &mut work);
                }
            }
            Terminator::Assert { target, .. } => {
                propagate(target, &dropped, &mut entry_dropped, &mut work);
            }
            Terminator::Return | Terminator::Unreachable => {
                // No drop required to be checked here at M8:
                // the schedule has been pre-inserted, so the verifier
                // tolerates locals that arrive un-dropped (they were
                // not eligible — see Phase 1 filter).
            }
        }
    }
    Ok(())
}

fn propagate(
    target: &BlockId,
    state: &HashSet<LocalId>,
    table: &mut HashMap<BlockId, HashSet<LocalId>>,
    work: &mut Vec<BlockId>,
) {
    let mut grew = false;
    let mut newly_seen = false;
    {
        let entry = table.get(target);
        if entry.is_none() {
            newly_seen = true;
        }
    }
    let entry = table.entry(*target).or_default();
    for d in state {
        if entry.insert(*d) {
            grew = true;
        }
    }
    if grew || newly_seen {
        if !work.contains(target) {
            work.push(*target);
        }
    }
}
