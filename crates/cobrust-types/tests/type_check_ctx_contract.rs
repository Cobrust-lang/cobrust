//! ADR-0056b §6 Phase J handoff contract tests — `TypeCheckCtx`.
//!
//! These tests pin the Clone + Send + invalidate behaviours that
//! Phase J wave-1 (ADR-0057a §4 `did_change` flow) consumes. If any
//! test regresses, the LSP per-keystroke budget breaks (per ADR-0056b
//! §5 Risk 3 — default `Clone` is O(n) without Arc-COW).
//!
//! Coverage:
//! - `TypeCheckCtx: Clone` (compile-time assertion + runtime clone)
//! - `TypeCheckCtx: Send` (compile-time assertion via tokio-async LSP)
//! - `check_incremental` merges `let` rows into the ctx
//! - `TypeCheckCtx::invalidate(file_id)` drops file-owned DefIds
//! - `version()` bumps monotonically on every write
//! - Cross-thread snapshot survives `std::thread::spawn`

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::missing_panics_doc)]

use cobrust_frontend::{parse_str, span::FileId};
use cobrust_hir::{Session, lower};
use cobrust_types::{TypeCheckCtx, check_incremental};

const REPL_FILE: u32 = FileId::SYNTHETIC.0;

/// Helper: type-check `src` into the carried `ctx` under
/// `FileId::SYNTHETIC` (REPL's binding bucket).
fn merge(ctx: &mut TypeCheckCtx, src: &str) {
    let module = parse_str(src, FileId::SYNTHETIC).expect("parse");
    let mut sess = Session::new();
    let hir = lower(&module, &mut sess).expect("lower");
    let _ = check_incremental(ctx, &hir, REPL_FILE).expect("check_incremental");
}

#[test]
fn type_check_ctx_is_clone() {
    fn assert_clone<T: Clone>() {}
    assert_clone::<TypeCheckCtx>();
}

#[test]
fn type_check_ctx_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<TypeCheckCtx>();
}

/// ADR-0057 §9 — LSP async runtime requires `'static + Send`. tokio
/// async tasks demand `'static`. We don't take any non-`'static`
/// borrows in the public surface; verify this transitively.
#[test]
fn type_check_ctx_is_send_static() {
    fn assert_send_static<T: Send + 'static>() {}
    assert_send_static::<TypeCheckCtx>();
}

#[test]
fn empty_ctx_has_zero_bindings_and_version_zero() {
    let ctx = TypeCheckCtx::new();
    assert_eq!(ctx.binding_count(), 0);
    assert_eq!(ctx.version(), 0);
    assert!(ctx.lookup("anything").is_none());
}

#[test]
fn check_incremental_populates_let_bindings() {
    let mut ctx = TypeCheckCtx::new();
    merge(&mut ctx, "let x = 42\n");
    assert!(ctx.lookup("x").is_some(), "x should be bound after `let x = 42`");
    assert!(ctx.version() > 0, "version should bump after a merge");
}

#[test]
fn check_incremental_populates_fn_bindings() {
    let mut ctx = TypeCheckCtx::new();
    merge(&mut ctx, "fn double(n: i64) -> i64:\n    return n + n\n");
    assert!(
        ctx.lookup("double").is_some(),
        "fn name should be bound after `fn double(...)`"
    );
}

#[test]
fn redefine_replaces_row() {
    let mut ctx = TypeCheckCtx::new();
    merge(&mut ctx, "let x = 1\n");
    let first = ctx.version();
    merge(&mut ctx, "let x = 2.5\n");
    let second = ctx.version();
    assert!(second > first, "redef should bump version");
    // The row is replaced (not duplicated). Wave-2 doesn't validate
    // the *new type* row exactly because the synthetic-module pass
    // uses a fresh `HirSession::new()` per call (DefIds are
    // independent); the binding is present nonetheless.
    assert!(ctx.lookup("x").is_some());
}

#[test]
fn clone_is_independent_of_original() {
    let mut ctx = TypeCheckCtx::new();
    merge(&mut ctx, "let a = 1\n");
    let snapshot = ctx.clone();
    let snapshot_version = snapshot.version();

    // Mutate the original.
    merge(&mut ctx, "let b = 2\n");
    assert!(ctx.version() > snapshot_version);

    // Snapshot didn't observe the post-clone write.
    assert_eq!(snapshot.version(), snapshot_version);
    assert!(snapshot.lookup("b").is_none(), "snapshot should not see post-clone writes");
}

#[test]
fn clone_preserves_existing_bindings() {
    let mut ctx = TypeCheckCtx::new();
    merge(&mut ctx, "let a = 1\n");
    let snap = ctx.clone();
    assert!(snap.lookup("a").is_some(), "snapshot should preserve pre-clone bindings");
    assert_eq!(snap.binding_count(), ctx.binding_count());
}

#[test]
fn invalidate_drops_file_owned_rows() {
    let mut ctx = TypeCheckCtx::new();
    merge(&mut ctx, "let x = 1\n");
    assert!(ctx.lookup("x").is_some());

    ctx.invalidate(REPL_FILE);
    assert!(
        ctx.lookup("x").is_none(),
        "x should be dropped after invalidate(REPL_FILE)"
    );
}

#[test]
fn invalidate_unknown_file_still_bumps_version() {
    let mut ctx = TypeCheckCtx::new();
    let v0 = ctx.version();
    ctx.invalidate(99_999);
    assert!(ctx.version() > v0, "unknown-file invalidate must still bump version");
}

#[test]
fn version_monotone_across_writes() {
    let mut ctx = TypeCheckCtx::new();
    let mut prev = ctx.version();
    for src in ["let a = 1\n", "let b = 2\n", "let c = 3\n"] {
        merge(&mut ctx, src);
        let now = ctx.version();
        assert!(now > prev, "version should increase: {prev} -> {now}");
        prev = now;
    }
    ctx.invalidate(REPL_FILE);
    assert!(ctx.version() > prev, "invalidate should bump version too");
}

#[test]
fn ctx_can_be_sent_across_thread_boundary() {
    let mut ctx = TypeCheckCtx::new();
    merge(&mut ctx, "let n = 7\n");
    let snap = ctx.clone();
    let handle = std::thread::spawn(move || snap.binding_count());
    let count = handle.join().expect("worker thread panicked");
    assert!(count >= 1, "snapshot lost the n row across the thread");
}

#[test]
fn many_clones_share_arc_storage() {
    // The contract is "Clone is O(1)" — we can't measure wall-time
    // robustly in a unit test, but we can verify that producing many
    // clones doesn't crash or balloon memory. (Arc-COW means each
    // clone is one atomic ref-bump per inner Arc.)
    let mut ctx = TypeCheckCtx::new();
    for i in 0..16 {
        merge(&mut ctx, &format!("let v{i} = {i}\n"));
    }
    let mut snapshots: Vec<TypeCheckCtx> = Vec::new();
    for _ in 0..1024 {
        snapshots.push(ctx.clone());
    }
    // Every snapshot sees the same bindings; no aliasing bug.
    for s in &snapshots {
        assert_eq!(s.binding_count(), ctx.binding_count());
    }
}

#[test]
fn merge_module_is_idempotent_on_same_source() {
    let mut ctx = TypeCheckCtx::new();
    merge(&mut ctx, "let x = 1\n");
    let count = ctx.binding_count();
    let v1 = ctx.version();
    // Same input, fresh HirSession — DefIds differ but `name`
    // collides; the row replaces in-place.
    merge(&mut ctx, "let x = 1\n");
    assert_eq!(
        ctx.binding_count(),
        count,
        "re-merge of `let x = 1` should not duplicate the row"
    );
    assert!(ctx.version() > v1, "re-merge still bumps version");
}

#[test]
fn alias_and_subst_accessors_callable() {
    // Sanity — these accessors are part of the Phase J handoff but
    // wave-2 doesn't yet populate them via merge_module. Calling them
    // on a default ctx must not panic.
    let ctx = TypeCheckCtx::new();
    assert!(ctx.alias("Vec").is_none());
    // `subst()` returns a real Subst; just calling .get on it must
    // not panic.
    let _ = ctx.subst();
}
