//! Borrow check pass — discharges ADR-0020's B1..B5 obligations.
//!
//! Algorithm: forward dataflow over each [`Body`]'s CFG. We model
//! per-local **flow-state** as the lattice
//! `(active | moved | dropped)`. Borrow conflicts are tracked
//! *per-block*: M8 is intra-block precise; cross-block borrow
//! lifetime tracking is M9 territory once codegen materializes
//! calling conventions.
//!
//! The fixpoint:
//!
//! - **`moved`** is monotone-grow: once a local is moved on any
//!   reachable path, it stays moved. We compute the per-block
//!   `entry_moved` set; the work-list iterates until no entry set
//!   strictly grows.
//! - **`dropped`** is monotone-grow over the post-drop CFG (the
//!   drop-schedule pass will have inserted Drop terminators by the
//!   time we re-run this pass after dropping).
//! - **borrows** are walked locally per block; conflicts within a
//!   block surface immediately (B2 / B3).
//!
//! M8 is intra-procedural. Inter-procedural lifetime obligations
//! land at M9.

use std::collections::{HashMap, HashSet};

use crate::error::MirError;
use crate::tree::{
    BlockId, Body, BorrowKind, LocalId, Operand, Place, Rvalue, Statement, StatementKind,
    Terminator,
};

pub fn borrow_check(body: &Body) -> Result<(), MirError> {
    if body.blocks.is_empty() {
        return Ok(());
    }
    // Per-block entry state — monotone-grow.
    let mut entry_moved: HashMap<BlockId, HashSet<LocalId>> = HashMap::new();
    let mut entry_dropped: HashMap<BlockId, HashSet<LocalId>> = HashMap::new();
    let mut visited: HashSet<BlockId> = HashSet::new();
    entry_moved.insert(BlockId(0), HashSet::new());
    entry_dropped.insert(BlockId(0), HashSet::new());
    let mut work: Vec<BlockId> = vec![BlockId(0)];

    while let Some(bid) = work.pop() {
        let in_moved = entry_moved.get(&bid).cloned().unwrap_or_default();
        let in_dropped = entry_dropped.get(&bid).cloned().unwrap_or_default();
        let was_visited = !visited.insert(bid);
        let block = &body.blocks[bid.0 as usize];
        // Walk this block once with the entry state.
        let mut state = BlockState {
            moved: in_moved,
            dropped: in_dropped,
            borrows: HashMap::new(),
        };
        for stmt in &block.statements {
            check_statement(stmt, &mut state)?;
        }
        let succs = check_terminator(&block.terminator, &mut state)?;
        // Propagate to successors.
        for s in succs {
            let grew_moved = grow_set(&mut entry_moved, s, &state.moved);
            let grew_dropped = grow_set(&mut entry_dropped, s, &state.dropped);
            // Enqueue successor if (a) we just grew its entry state,
            // OR (b) the successor has never been visited.
            let needs_visit = !visited.contains(&s);
            if grew_moved || grew_dropped || needs_visit {
                if !work.contains(&s) {
                    work.push(s);
                }
            }
        }
        let _ = was_visited;
    }
    Ok(())
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct BlockState {
    /// Locals already moved on entry to this point.
    moved: HashSet<LocalId>,
    /// Locals already dropped (only meaningful after the drop pass).
    dropped: HashSet<LocalId>,
    /// In-flight borrow stacks, per local — block-local only at M8.
    borrows: HashMap<LocalId, Vec<BorrowKind>>,
}

fn grow_set(
    table: &mut HashMap<BlockId, HashSet<LocalId>>,
    block: BlockId,
    new_state: &HashSet<LocalId>,
) -> bool {
    let entry = table.entry(block).or_default();
    let mut grew = false;
    for l in new_state {
        if entry.insert(*l) {
            grew = true;
        }
    }
    grew
}

fn check_statement(stmt: &Statement, state: &mut BlockState) -> Result<(), MirError> {
    match &stmt.kind {
        StatementKind::Assign { place, rvalue } => {
            check_rvalue_reads(rvalue, state, stmt.span)?;
            // Assignment to `place` — clear any moved-out flag for
            // the destination's root local.
            state.moved.remove(&place.local);
            // `Use(Move(...))` consumes the source.
            if let Rvalue::Use(Operand::Move(p)) = rvalue {
                if !state.moved.insert(p.local) {
                    // Already moved.
                    return Err(MirError::UseAfterMove {
                        local: p.local.0,
                        span: stmt.span,
                        suggestion: Some(
                            "change to `&s` to borrow without consuming (ADR-0052a explicit shared borrow)",
                        ),
                    });
                }
            }
            // ADR-0050c Phase 4 note: `Rvalue::Aggregate` with multiple
            // `Operand::Move(p)` items pointing at the SAME local is a
            // pattern the codegen handles via per-element
            // `__cobrust_str_clone` (see `lower_aggregate_list` at
            // `cranelift_backend.rs`). The borrow check intentionally
            // tolerates this — every Move slot's read produces a fresh
            // owned clone via codegen, so the source local stays valid
            // for subsequent moves within the same Aggregate. This
            // matches the implicit-clone insertion pattern declared in
            // ADR-0050c §"Phase 4 Operand-lowering clone emission".
            // Take a borrow if Rvalue::Ref(...).
            if let Rvalue::Ref(kind, target) = rvalue {
                push_borrow(state, target, *kind, stmt.span)?;
            }
            Ok(())
        }
        StatementKind::StorageLive(_) | StatementKind::StorageDead(_) | StatementKind::Nop => {
            Ok(())
        }
    }
}

fn check_terminator(term: &Terminator, state: &mut BlockState) -> Result<Vec<BlockId>, MirError> {
    let synth_span =
        cobrust_frontend::span::Span::point(cobrust_frontend::span::FileId::SYNTHETIC, 0);
    match term {
        Terminator::Goto(b) => Ok(vec![*b]),
        Terminator::SwitchInt {
            operand,
            cases,
            otherwise,
        } => {
            check_operand_read(operand, state, synth_span)?;
            let mut v: Vec<BlockId> = cases.iter().map(|(_, b)| *b).collect();
            v.push(*otherwise);
            Ok(v)
        }
        Terminator::Call {
            func,
            args,
            destination,
            target,
            unwind,
        } => {
            check_operand_read(func, state, synth_span)?;
            for a in args {
                check_operand_read(a, state, synth_span)?;
                if let Operand::Move(p) = a {
                    state.moved.insert(p.local);
                }
            }
            state.moved.remove(&destination.local);
            let mut v = vec![*target];
            if let Some(u) = unwind {
                v.push(*u);
            }
            Ok(v)
        }
        Terminator::Drop { place, target } => {
            state.dropped.insert(place.local);
            Ok(vec![*target])
        }
        Terminator::Return | Terminator::Unreachable => Ok(vec![]),
        Terminator::Assert { cond, target, .. } => {
            check_operand_read(cond, state, synth_span)?;
            Ok(vec![*target])
        }
    }
}

fn check_rvalue_reads(
    rvalue: &Rvalue,
    state: &mut BlockState,
    span: cobrust_frontend::span::Span,
) -> Result<(), MirError> {
    match rvalue {
        Rvalue::Use(op) => check_operand_read(op, state, span),
        Rvalue::BinaryOp(_, a, b) => {
            check_operand_read(a, state, span)?;
            check_operand_read(b, state, span)
        }
        Rvalue::UnaryOp(_, a) => check_operand_read(a, state, span),
        Rvalue::Aggregate(_, items) => {
            for it in items {
                check_operand_read(it, state, span)?;
            }
            Ok(())
        }
        Rvalue::Cast(_, op, _) => check_operand_read(op, state, span),
        Rvalue::Ref(_, _) | Rvalue::Discriminant(_) | Rvalue::Len(_) | Rvalue::NullaryOp(_) => {
            Ok(())
        }
    }
}

fn check_operand_read(
    op: &Operand,
    state: &mut BlockState,
    span: cobrust_frontend::span::Span,
) -> Result<(), MirError> {
    match op {
        Operand::Constant(_) => Ok(()),
        Operand::Copy(p) | Operand::Move(p) => {
            if state.moved.contains(&p.local) {
                return Err(MirError::UseAfterMove {
                    local: p.local.0,
                    span,
                    suggestion: Some(
                        "change to `&s` to borrow without consuming (ADR-0052a explicit shared borrow)",
                    ),
                });
            }
            if state.dropped.contains(&p.local) {
                return Err(MirError::UseAfterDrop {
                    local: p.local.0,
                    span,
                    suggestion: Some(
                        "the value was already dropped; reorder code so the read precedes the drop",
                    ),
                });
            }
            Ok(())
        }
    }
}

fn push_borrow(
    state: &mut BlockState,
    place: &Place,
    kind: BorrowKind,
    span: cobrust_frontend::span::Span,
) -> Result<(), MirError> {
    let stack = state.borrows.entry(place.local).or_default();
    let has_mut = stack.contains(&BorrowKind::Mut);
    let has_shared = stack.contains(&BorrowKind::Shared);
    match kind {
        BorrowKind::Mut => {
            if has_mut {
                return Err(MirError::ConflictingMutBorrow {
                    local: place.local.0,
                    span,
                    suggestion: Some(
                        "only one mutable borrow can be active at a time; release the first borrow first",
                    ),
                });
            }
            if has_shared {
                return Err(MirError::SharedMutOverlap {
                    local: place.local.0,
                    span,
                    suggestion: Some(
                        "cannot borrow mutably while a shared borrow is active; release shared first",
                    ),
                });
            }
        }
        BorrowKind::Shared => {
            if has_mut {
                return Err(MirError::SharedMutOverlap {
                    local: place.local.0,
                    span,
                    suggestion: Some(
                        "cannot borrow mutably while a shared borrow is active; release shared first",
                    ),
                });
            }
        }
    }
    stack.push(kind);
    Ok(())
}
