---
doc_kind: adr
adr_id: 0050c
title: "M-F.3.2 prereq — Str ownership flip (TD-1 closure)"
status: accepted
date: 2026-05-16
last_verified_commit: aca5d87
supersedes: []
superseded_by: []
relates_to: [adr:0020, adr:0023, adr:0027, adr:0034, adr:0035, adr:0044, adr:0044a, adr:0049, adr:0050, adr:0050b, adr:0050d]
discovered_by: P9-E1 Wave 2 design sprint on `feature/f3-str-ownership` per ADR-0050 §"Sub-ADR slots / ADR-0050c" + §A1 verified-at-HEAD TD-1 confirmation
ratification_path: in-session review per ADR-0050 §"Audit model"
---

# ADR-0050c: M-F.3.2 prereq — Str ownership flip (TD-1 closure)

## Context

### TD-1 origin chain

TD-1 is the "`Ty::Str` and `Ty::List(_)` are Copy-typed in MIR" debt. The deferral chain:

- **ADR-0027 §"Drop schedule"** (M12.x codegen amendments, accepted 2026-05-09 at `3a81f90`): introduced `Rvalue::Aggregate(List)` lowering + `__cobrust_list_drop` C-ABI shim + theoretical drop schedule per ADR-0020 §"Drop schedule algorithm". The §"Negative" consequence "Heap allocator pressure increases — Aggregate literals now hit `__cobrust_alloc` per construction" implicitly assumed Drop would close. It did not.
- **ADR-0044 §"W2 Phase 3"** (W2 LeetCode wedge, accepted 2026-05-11 at `9caef99`): added `__cobrust_str_new` / `__cobrust_str_push_static` / `__cobrust_str_drop` / `__cobrust_str_len` / `__cobrust_str_ptr` C-ABI shims at `crates/cobrust-stdlib/src/fmt.rs:75,88,189,206,226` plus seven W2 Phase 3 Str helpers at `crates/cobrust-stdlib/src/io.rs:447-639`. To meet the W2 Phase 3 deadline ("刷不了 leetcode"), the MIR Copy semantics for `Ty::Str` and `Ty::List(_)` were preserved as a deliberate shortcut. The `__cobrust_str_drop` shim was therefore never called from codegen — every short-lived LeetCode program simply leaks every heap Str on exit. **This is TD-1.**
- **ADR-0049** (alpha honesty lanes): noted that the LC-100 Pattern B "`list[str] gap`" finding was deferred yet again because the M-AI corpus didn't exercise long-lived Str ownership.
- **ADR-0050 §"Sub-ADR slots / ADR-0050c"** (Phase F.3 batch frame, accepted 2026-05-16 at `891d235` then amended at `f566026`): pinned ADR-0050c as the load-bearing TD-1 closure inside Wave 2, with a recommendation lock "Drop-by-default with explicit clone, mirroring Rust's `String`." Amendment 2026-05-16 §A1 verified-at-HEAD that TD-1 is real (cited `mir/drop.rs:122-129` + `mir/lower.rs:1717-1725`).

### Verified-at-HEAD audit table (F27 SOP-compliant pre-dispatch grep)

Per `findings/adr-scope-reality-divergence.md` F27 SOP: every claim below was cross-checked by reading the file at `HEAD=f566026`.

| Claim | File:line | Verbatim shape |
|---|---|---|
| MIR drop pass treats `Ty::Str` as Copy | `crates/cobrust-mir/src/drop.rs:122-129` | `fn is_copy(ty: &Ty) -> bool { matches!(ty, Ty::Bool \| Ty::Int \| Ty::Float \| Ty::Imag \| Ty::None \| Ty::Never \| Ty::Str \| Ty::List(_)) }` with comment "ADR-0044 W2 Phase 3: Str and List are non-drop-eligible" |
| MIR lowering treats `Ty::Str` as Copy at operand-construction time | `crates/cobrust-mir/src/lower.rs:1716-1725` | identical `is_copy_type` predicate with comment "ADR-0044 W2 Phase 3: Str and List are treated as Copy for source-level ergonomics — runtime Drop is a no-op jump" |
| Codegen `Terminator::Drop` arm is a pure jump | `crates/cobrust-codegen/src/cranelift_backend.rs:1022-1026` | `Terminator::Drop { target, .. } => { let blk = self.block_id(target)?; self.builder.ins().jump(blk, &[]); Ok(()) }` — **drops the `place` field entirely** |
| Drop-pending eligibility predicate routes through `is_copy(&ld.ty)` | `crates/cobrust-mir/src/drop.rs:43-48` | `if !is_copy(&ld.ty) && !body.is_param(ld.id) { drop_eligible.insert(ld.id); }` — so every Str-typed local is excluded |
| f-string lowering creates Str-typed temp | `crates/cobrust-mir/src/lower.rs:1081-1087` | `let temp = self.declare_local("_fstr".to_string(), Ty::Str, e.span, false); self.emit_assign(Place::local(temp), Rvalue::Aggregate(AggregateKind::FormatString, ops), e.span)` |
| Aggregate List lowering declares `Ty::List(Box::new(Ty::None))` (elem type unresolved at MIR-build) | `crates/cobrust-mir/src/lower.rs:1114-1130` | `let temp = self.declare_local("_list".to_string(), Ty::List(Box::new(Ty::None)), e.span, false);` — type checker has separately recorded the real element type via `ctx.lookup_ty` |
| f-string codegen calls `__cobrust_str_new` | `crates/cobrust-codegen/src/cranelift_backend.rs:932,1365` | `let buf = if let Some(fr) = self.runtime_funcs.get("__cobrust_str_new").copied() { ... }` |
| `Aggregate(List)` codegen calls `__cobrust_list_new + list_set`, **no per-element drop emit** | `crates/cobrust-codegen/src/cranelift_backend.rs:1284-1305` | constructs list, sets slots; never emits `__cobrust_str_drop` or per-slot cleanup |
| `__cobrust_str_drop` already linked + signature-declared | `crates/cobrust-codegen/src/cranelift_backend.rs:1978` | `out.push(("__cobrust_str_drop", sig(call_conv, &[p], None)));` |
| `__cobrust_list_drop` already linked + signature-declared | `crates/cobrust-codegen/src/cranelift_backend.rs:1922` | `out.push(("__cobrust_list_drop", sig(call_conv, &[p], None)));` |
| `__cobrust_argv` materializes List<Str> with str pointers as i64 slots | `crates/cobrust-stdlib/src/env.rs:64-85` | wraps each captured arg via `__cobrust_str_new + __cobrust_str_push_static`, stores buffer pointer in `__cobrust_list_set(list, i, buf as i64)` — docstring at line 60 claims "M12.x convention: codegen owns the drop schedule" (which TD-1 violates) |
| `__cobrust_input / _input_no_prompt / _input_str_buf / _read_line` all return owned `*mut Str` | `crates/cobrust-stdlib/src/io.rs:194-317` | each builds via `alloc_str_buffer(&s)` (`io.rs:167-178`) wrapping `__cobrust_str_new + __cobrust_str_push_static` |
| `__cobrust_str_at` returns a freshly-allocated `*mut Str` | `crates/cobrust-stdlib/src/io.rs:480-491` | returns `alloc_str_buffer(&s[idx..=idx])` — every callsite leaks the result under TD-1 |
| LLM stdlib also produces owned Str via `alloc_str_buffer` | `crates/cobrust-stdlib/src/llm.rs:398-410,434,450` | mirror of io.rs:167; same TD-1 leak |
| Borrow check is intra-block at M8; no NLL machinery shipped | `crates/cobrust-mir/src/borrow.rs:1-23` + `:33-76` + module-comment "M8 is intra-procedural. Inter-procedural lifetime obligations land at M9" | confirms NLL last-use analysis is not on `main` |
| Comprehension lowering iter-protocol path | `crates/cobrust-mir/src/lower.rs:1493-1576` | open finding `comp-lowering-zero-sentinel-collision.md` (P2); same iter-protocol that Aggregate List feeds |
| For-loop lowering length-bound path | `crates/cobrust-mir/src/lower.rs:1717-1725 + 726-836` | ADR-0050b superseded iter-protocol for for-loop; uses `__cobrust_list_len + __cobrust_list_get` |
| `__cobrust_iter_init / _next / _drop` runtime backing | `crates/cobrust-stdlib/src/iter.rs:278-355` | iter-protocol over list-of-i64 slots; reads list pointers via `__cobrust_list_get` returning `i64` |
| Existing Str consumer count (grep `__cobrust_str_` non-test) | `crates/cobrust-codegen/src/cranelift_backend.rs` + `crates/cobrust-stdlib/src/{fmt,io,env,llm}.rs` + `crates/cobrust-cli/src/build/intrinsics.rs` | 18 distinct non-test references across 6 files; full list in §"Consequences" enumeration |

### LC-100 / Wave 2 / list[str] dependency framing

- LC-100 Pattern B (`docs/agent/findings/lc100-pattern-b-list-of-str-gap.md`) is the LeetCode wedge symptom: real solutions over `list[str]` cannot ship honestly while every Str leaks. M-F.3.2 list[str] is the closure milestone; ADR-0050c is M-F.3.2's prereq.
- Wave 2 in ADR-0050 §A2 revised wave timing puts ADR-0050c as a doc-only P9 solo sprint (this sprint), followed by M-F.3.2 list[str] P10-direct PAIR per A7.
- M-F.3.3 f64 (also Wave 2, sonnet D2 per §A2) is independent of TD-1; f64 is `Copy` and stays `Copy` regardless of how Str ownership resolves.
- M-F.3.4 dict (Wave 3, opus D5 per §A2) inherits the ownership shape ADR-0050c picks. ADR-0050d Decision 7 keys `K ∈ {i64, str}`; `str`-keyed dict pulls the same drop schedule the list[str] lowering does. If ADR-0050c picks Drop-by-default, dict[str, str] inherits a per-entry Str drop schedule for free.

### Constitution alignment

| Clause | This ADR's adherence |
|---|---|
| §2.2 "Drop from Python — silent coercion" | Str ownership is now visible: rebind, return, and arg-pass clearly transfer ownership (move) or duplicate (clone). No silent reference sharing. |
| §2.3 "Adopt from Rust — ownership, borrowing" | The Drop-by-default semantics is the canonical Rust shape for `String`. |
| §5.1 "elegant — one way to do each thing" | One ownership model for heap-allocated value types (Str + List + Dict + Set); Aggregate kinds all participate in the same drop schedule. No special-case for Str. |
| §5.1 "no `.unwrap()` in non-test code" | The flip introduces zero new error paths; ownership transfer is a static MIR concern, no `Option` to unwrap. |
| §5.3 "efficient — allocations visible via the type system" | `Ty::Str` becoming non-Copy makes every clone explicit in MIR — allocation count is now an audit-trivial property of the MIR body. |

## Options considered

### Option A — Full-Drop schedule (drop on every scope exit)

**Mechanism**: flip `is_copy(Ty::Str)` and `is_copy(Ty::List(_))` to `false` in both `mir/drop.rs:127` and `mir/lower.rs:1723`. Every `let s = expr` (Str-typed) becomes drop-eligible per the existing `drop.rs` Phase 1 enumeration. The existing five-phase drop pass at `crates/cobrust-mir/src/drop.rs:1-129` already inserts `Terminator::Drop` before every `Terminator::Return`; flipping the predicate lights up Str + List(_) without changing the algorithm. The codegen `Terminator::Drop` arm at `cranelift_backend.rs:1022-1026` is currently a no-op jump — it must learn to emit `__cobrust_str_drop(place_value)` (for `Ty::Str`) and `__cobrust_list_drop_elems(place_value)` (a new shim that frees each Str slot before dropping the list, for `Ty::List(Ty::Str)`) or `__cobrust_list_drop(place_value)` (for `Ty::List(Ty::Int)`). For mid-body rebind (`s = "new"` after `let s = "old"`), MIR's move semantics already cover this — the old assignment is `moved_out_per_block`-tracked (`drop.rs:53-70`); the rebind's RHS evaluation triggers a synthetic drop of the old `s` slot. (Mirror of Rust's `String` rebind drop.)

**Migration cost for M-F.3.2 P10-direct PAIR**:
- TEST: ≥40 well-typed corpus tests (per ADR-0050 §"M-F.3.2") covering (a) rebind drops old Str, (b) function-return moves Str, (c) function-arg pass-by-value moves Str, (d) `for s in xs:` over `list[str]` iterates without leak, (e) f-string composition drops the temp at scope exit. ≥20 ill-typed tests (drop-after-move detection if borrow check can see it; otherwise relegated to runtime test).
- DEV: flip the `is_copy_type` predicate (`mir/lower.rs:1723`); flip `is_copy` (`mir/drop.rs:127`); extend codegen `Terminator::Drop` to emit the actual drop call dispatched by `place.local`'s declared type from the `body.locals` table; add `__cobrust_str_clone` C-ABI shim for the explicit-clone path; thread `Operand::Copy` of Str-typed places through `__cobrust_str_clone` at the operand-lowering site (not at codegen — keeps codegen Cranelift-agnostic).

**Estimated wall-time**: 6-10 hours opus P10-direct PAIR (D4 — structural touch, well-scoped).

**Pros**:
- Trivially correct: every Str allocated gets dropped exactly once on every reachable return path. The existing drop pass already proves the soundness contract (`drop.rs:117-119` "verification" phase + `:264-310` forward-flow checker).
- Zero new MIR primitives. The drop schedule machinery (Phases 1-5 in `drop.rs:34-119`) already exists, ships with verification, and was designed for exactly this case (ADR-0020 §"Drop schedule algorithm").
- Codegen blast radius is one arm: `Terminator::Drop` at `cranelift_backend.rs:1022-1026`. The dispatch table is the `body.locals[place.local.0].ty` field already populated by MIR lowering. (Per F29 cross-surface scope check below, every existing Str consumer benefits.)
- Mirrors Rust's `String` ownership semantics one-to-one. User mental model is "Str is a heap-allocated owning value, like Rust's `String`". Constitution §2.3 "adopt from Rust — ownership, borrowing" satisfied.
- Composes monotonically with M-F.3.2 list[str]: a `list[str]` simply means every element drop runs before the list drop. The drop-elements-then-drop-list shim `__cobrust_list_drop_elems` is a 30-line addition to `crates/cobrust-stdlib/src/collections.rs`.
- Composes monotonically with M-F.3.4 dict[str, V] / dict[K, str]: ADR-0050d Decision 7's `str`-keyed dict inherits the same per-entry drop schedule.
- M8 borrow check (`borrow.rs:33-76`) is intra-block precise + ADR-0020 B1..B5 obligations; flipping a type to non-Copy makes more obligations visible without changing the algorithm. No NLL machinery required.

**Cons**:
- Drops are coarse (scope exit, not last-use). A long-lived Str held until the end of `main` extends its lifetime to `main`'s return; for the LC-100 wedge audience this is unchanged from today's "leak everything" semantics — there is no extra cost.
- Adds an explicit `__cobrust_str_clone` C-ABI shim and one operand-lowering site so that `let a = s; let b = s` (where `s: str` is `Copy`-used) becomes a compile-time clone. **This is exactly the precedent in Rust: `let a = s.clone()` is explicit when you want both bindings to own.** For one-pass-of-the-string LeetCode programs, the clone count is zero; for sharing programs, the clone is explicit and visible.
- The corpus must include drop-after-move negative cases to verify the borrow check catches use-after-move (ADR-0020 B1..B5).

**Blast radius**:
- 2 MIR predicate flips.
- 1 codegen arm rewrite (`Terminator::Drop`).
- 2 new C-ABI shims (`__cobrust_str_clone` + `__cobrust_list_drop_elems`).
- 1 operand-lowering site addition (clone emission for `Ty::Str` `Copy` reads).
- Every existing Str-producing C-ABI shim (`__cobrust_input` etc.) is **unchanged** — they already return owned `*mut Str`; the codegen's new responsibility is to drop them, which it never did before.

### Option B — NLL last-use Drop (drop at last use per borrow-tracking analysis)

**Mechanism**: introduce a flow-sensitive last-use analysis on the MIR CFG. For each non-Copy local, compute the program point of the last `Operand::Copy` / `Operand::Move`; insert `Terminator::Drop` immediately after that point. Mirrors Rust's NLL.

**Migration cost for M-F.3.2 P10-direct PAIR**:
- Net-new MIR pass: ≥500 LoC for the dataflow lattice (`active | moved | dropped | dead`), the fixpoint over the CFG, the post-pass drop-insertion edge-splitting. Must compose with the existing drop pass at `drop.rs:34-119` (which is currently end-of-scope only).
- Codegen change is identical to Option A (`Terminator::Drop` arm dispatches on local type).
- Corpus must cover both straight-line last-use AND control-flow-dependent last-use (`if cond: print(s)` — `s`'s last use varies by branch). Test surface ~3× Option A.
- DEV wall-time: 18-25 hours opus, plus a separate spike on the lattice design (D5, would justify its own sub-ADR).

**Pros**:
- Tighter lifetimes; long-lived locals get freed earlier. For Cobrust's audience this is **not yet measurable**: LC-100 program lifetimes are ms-scale; one extra `__cobrust_str_drop` call per local at end-of-`main` is invisible.
- Matches Rust 2018+ NLL semantics. Aspirational.

**Cons**:
- **No M8/M9 machinery for it.** `crates/cobrust-mir/src/borrow.rs:1-23` is explicit: "M8 is intra-procedural. Inter-procedural lifetime obligations land at M9." NLL last-use is the M9.x territory and is undisclosed scope.
- 3-5× the M-F.3.2 wall-time. Risks Wave 2 timeline slip (ADR-0050 §A2 revised: Wave 2 = 3-5 days; an 18-25 hour NLL spike + 6-10 hour drop-pass close exceeds that envelope).
- The dataflow correctness proof is non-trivial; without a passable proof, soundness regressions in already-shipped surfaces (f-strings, list iteration) become a regression vector.
- Constitution §5.1 "one way to do each thing": shipping two drop-insertion algorithms (end-of-scope for non-NLL types, last-use for NLL types) violates "one way." Worse: ADR-0050c would invent the second algorithm before the first is even used end-to-end.
- LC-100 user-facing benefit at Phase F.3: zero (program lifetimes too short to measure).
- The "obvious" payoff scenario (long-running programs holding many Strs) isn't reachable today — Cobrust's runtime has no service loop, no daemon, no long process. Phase F.5+ (REPL + LSP per ADR-0050 §"P1 wave / M-F.3.8") brings long lifetimes; that is the natural NLL trigger.

**Blast radius**: very high. ≥500 LoC new MIR pass + composes with the existing drop pass + needs its own verification phase + corpus for both shapes.

### Option C — Reference-counted Str (Rc-style runtime sharing)

**Mechanism**: change `StringBuffer` (`crates/cobrust-stdlib/src/fmt.rs:63-78`) to carry an atomic refcount. `__cobrust_str_new` returns refcount=1; a new `__cobrust_str_borrow(buf)` increments; `__cobrust_str_drop(buf)` decrements + frees at zero. Every `Operand::Copy` of a `Ty::Str` lowers to `__cobrust_str_borrow`. MIR `is_copy(Ty::Str)` returns **true again** (refcount makes Copy semantically free).

**Migration cost for M-F.3.2**:
- 1 new shim (`__cobrust_str_borrow`).
- `StringBuffer` layout change at `crates/cobrust-stdlib/src/fmt.rs:63-78` — add `count: AtomicUsize` field. Every consumer that reads/writes the buffer pointer must respect the new layout.
- All 18 existing Str consumers (see §"Consequences" enumeration) must be re-audited: returns of Str pointers across function boundaries are now "copy borrow"; existing call sites that aliased Str pointers (no current example, but argv() returns a List<Str> and the inner Str pointers are arguably shared between the list slot and any iteration-bound local) are now valid where they were not before.
- Atomic refcount means thread-safety contract changes; constitution §2.2 "drop GIL — ownership-based concurrency" implies multi-threaded Str sharing must be safe.
- The drop pass at `drop.rs:34-119` flips Str back to Copy and skips Str entirely; explicit `__cobrust_str_drop` is emitted at the *single* end-of-`main` for each newly-allocated Str instead of per-rebind. Bug: rebind `s = "new"` leaks the old Str's refcount because is_copy means "no drop on rebind."

**Pros**:
- No source-level surface change: Cobrust user code that today does `let a = s; let b = s` continues to compile with both bindings valid.
- Tracker matches Python's CPython refcount semantics; one Python translation surface mapping is trivial (`Py_IncRef ↔ __cobrust_str_borrow`).
- For sharing-heavy AI prompt-cache workloads (ADR-0048 / ADR-0049 M-AI surfaces), refcount avoids the explicit-clone tax.

**Cons**:
- **Atomic refcount is not free.** Every `Operand::Copy(Ty::Str)` becomes a `lock xadd` on aarch64/x86_64. The LC-100 wedge tight inner loops pay the tax on every iteration even though no sharing is needed.
- **The rebind bug above is fatal.** Refcount with Copy semantics in MIR means rebind `s = expr` doesn't decrement the old `s`'s refcount; every rebound Str leaks until end-of-`main`. Fixing this means non-Copy semantics for the rebind path while keeping Copy for the operand-read path; this is exactly Option A's design with extra runtime cost.
- Constitution §2.3 "Adopt from Rust" recommends `String` (owning + clone) not `Arc<String>` (refcounted) as the default. Rust has both; the *default* is `String`. Cobrust matches.
- §2.2 "GIL → ownership-based concurrency" suggests refcount could compose with multi-threading badly: `Arc<String>` is the Rust answer for shared-across-threads but is heavier than `String` for the common case.
- LLM Router (`crates/cobrust-llm-router/`, per constitution §4.3) uses Str heavily in tool prompts; refcount overhead measurable there.

**Blast radius**: very high. Touches StringBuffer layout, every consumer, ABI shape (refcount field shifts pointer alignment), drop schedule, codegen, runtime, all stdlib `alloc_str_buffer` callers.

### Option D — Status quo (keep the Copy hack; defer to Phase G)

**Mechanism**: leave `mir/drop.rs:127` and `mir/lower.rs:1723` as-is. M-F.3.2 list[str] ships with the existing "every Str leaks on program exit" semantics. Phase G picks up the work post-v0.2.0.

**Migration cost for M-F.3.2**: ~0 (the list[str] type checker work + codegen is independent of ownership flip; just inherits the leak).

**Pros**:
- Smallest possible Wave 2 sprint.
- LC-100 wedge audience doesn't observe the leak (program exits before OS cares).

**Cons**:
- **ADR-0050 §"Sub-ADR slots" explicitly names ADR-0050c as the Wave 2 TD-1 closure work.** Deferring TD-1 again rolls a fourth-deferral (ADR-0027 → ADR-0044 → ADR-0049 → ADR-0050c → Phase G); per `feedback_third_party_audit_2026_05_09.md`, the project owner has flagged this kind of forever-deferral as the dominant honesty risk.
- v0.2.0 stable tag binds to "language-half completeness" (ADR-0050 §"v0.2.0 stable tag binding"). A language whose only string type leaks unboundedly is not language-complete; future REPL / LSP / long-running programs immediately surface the leak.
- LC-100 Pattern B finding stays open — `list[str] gap` does not honestly close because the language cannot run a long-lived list[str] computation without leaking.
- F29 cross-surface enumeration shows every existing Str consumer is on the leak; deferral compounds.

**Blast radius**: zero, but the technical debt compounds.

## Decision

**Adopt Option A — Full-Drop schedule (drop on every reachable scope exit), with explicit `__cobrust_str_clone` for the shared-ownership escape hatch.**

Rationale chain in two paragraphs:

1. **Soundness × simplicity dominates the matrix.** Option A is provably correct with zero new MIR machinery: the drop pass already exists, the verification phase already runs (`drop.rs:117-119,264-310`), the C-ABI drop shims already ship (`__cobrust_str_drop` at `fmt.rs:226`, `__cobrust_list_drop` at `collections.rs:520`). The flip is two predicates + one codegen arm + two new C-ABI shims. Option B's NLL last-use needs an undisclosed 500-LoC dataflow pass that constitution §5.1 forbids ("one way"). Option C's refcount is heavier-per-operation than Option A's explicit-clone and carries a rebind correctness bug that requires re-importing Option A's mechanism anyway. Option D defers TD-1 for the fourth time; rejected on honesty grounds per `feedback_third_party_audit_2026_05_09.md`.

2. **The rationale lock from ADR-0050 §"Sub-ADR slots" is binding.** The recommendation was "Drop-by-default with explicit clone, mirroring Rust's `String`." Option A is the literal implementation of that lock. The two-option ambiguity (Full-Drop vs NLL last-use) within "Drop-by-default" resolves to Full-Drop because: (a) LC-100 measurable benefit of NLL is zero given short program lifetimes; (b) NLL is the natural Phase G consolidation alongside REPL + LSP per ADR-0050 §"P1 wave / M-F.3.8" (long-lived programs are the trigger); (c) shipping NLL pre-emptively would re-do the drop-pass verification work + introduce two drop-insertion algorithms, violating §5.1.

The decision composes monotonically forward: Phase G can introduce NLL last-use as a Wave-2-style optimization-pass that *refines* drop insertion (moving drops earlier without changing correctness), without forcing a re-audit of any existing surface.

### Implementation map

Every row below cites a file + line. Migration ordering = top of the list first.

#### Phase 1 — MIR predicate flip (single PAIR, ~1 hour)

| File | Line | Change |
|---|---|---|
| `crates/cobrust-mir/src/drop.rs` | 122-129 | Remove `Ty::Str` and `Ty::List(_)` from the `is_copy` match arm. Replace the existing ADR-0044 W2 Phase 3 comment with: `// ADR-0050c TD-1 closure: Str and List are non-Copy; the drop pass enumerates them as drop-eligible. Element-type-aware drop (list[str] → drop each element first) lives in codegen's Terminator::Drop arm.` |
| `crates/cobrust-mir/src/lower.rs` | 1716-1725 | Mirror the predicate flip in `is_copy_type`. Replace the same ADR-0044 comment block. |

After this phase, the MIR drop pass enumerates every Str-typed and List-typed local as drop-eligible; the codegen `Terminator::Drop` arm is *still a no-op jump*, so the only observable change is per-build MIR-verification output. The `body.is_param(ld.id)` exclusion at `drop.rs:45` preserves the parameter-vs-local distinction — Str parameters are NOT drop-eligible inside the callee (caller drops in C-ABI sense; matches Rust). This is critical: any cross-function Str return today (e.g. `fn first(xs: list[str]) -> str: for x in xs: return x` at `intrinsics_input.rs:1049`) preserves move-semantics; the callee returning a Str moves ownership to caller, who drops at the binding's scope exit.

#### Phase 2 — Codegen `Terminator::Drop` arm (single PAIR, ~3 hours)

| File | Line | Change |
|---|---|---|
| `crates/cobrust-codegen/src/cranelift_backend.rs` | 1022-1026 | Replace the no-op jump with a type-dispatched drop emit. Lookup `body.locals[place.local.0].ty`; emit `__cobrust_str_drop(local_value)` for `Ty::Str`, `__cobrust_list_drop_elems(local_value, element_drop_fn_id)` for `Ty::List(Ty::Str)`, `__cobrust_list_drop(local_value)` for `Ty::List(Ty::Int)`/etc. Fallback: emit no-op jump if the ty is something unrecognized (graceful degradation). After the drop call, emit the jump to `target` as today. |

The dispatch table is initially small (i64 / Str / List(Int) / List(Str)); Phase 3 extends as new types ship.

#### Phase 3 — New stdlib C-ABI shims (single PAIR, ~1.5 hours)

| File | Line (approx) | Change |
|---|---|---|
| `crates/cobrust-stdlib/src/fmt.rs` | after 232 | Add `__cobrust_str_clone(buf: *mut u8) -> *mut u8`: allocates a fresh `StringBuffer`, copies the bytes, returns the new pointer. Doc with `# Safety` clause matching `__cobrust_str_new` (line 71-73). |
| `crates/cobrust-stdlib/src/collections.rs` | after 532 | Add `__cobrust_list_drop_elems(list: *mut u8, elem_drop_fn: extern "C" fn(*mut u8))`: iterate the i64 slots, cast each to `*mut u8`, call `elem_drop_fn(slot)`, then `__cobrust_list_drop(list)`. (Element fn pointer threaded by codegen — the i64-typed list will pass a no-op; the str-typed list will pass `__cobrust_str_drop`.) |

Add signatures to `runtime_helper_signatures()`:

| File | Line | Change |
|---|---|---|
| `crates/cobrust-codegen/src/cranelift_backend.rs` | after 1978 | `out.push(("__cobrust_str_clone", sig(call_conv, &[p], Some(p))));` |
| `crates/cobrust-codegen/src/cranelift_backend.rs` | after 1922 | `out.push(("__cobrust_list_drop_elems", sig(call_conv, &[p, p], None)));` (`elem_drop_fn` is a function pointer, p-sized) |

#### Phase 4 — Operand-lowering clone emission (single PAIR, ~2 hours)

| File | Line | Change |
|---|---|---|
| `crates/cobrust-mir/src/lower.rs` | 1089-1097 | `ExprKind::Name` operand resolution: when the resolved type is non-Copy AND the read context is `Operand::Copy` (e.g. used as an arg to a fn that expects ownership AND another use exists in the same block), emit a clone temp. (Pragmatic Phase F.3 simplification: emit clone whenever an operand-read of a non-Copy local would conflict with the drop pass's move-tracking — i.e., the local is read in two places. The drop pass's `globally_moved` set at `drop.rs:91-96` is the source of truth.) |

The clone-emission rule mirrors Rust's `s.clone()` ergonomics with one Cobrust-specific difference: Cobrust users do not yet write `.clone()` source-side. Phase F.3 ships **implicit clone** at compile-time when the operand-use pattern requires it; Phase G will introduce an explicit `clone(s)` PRELUDE function alongside an *opt-in* `#[strict_ownership]` mode that disables implicit clone and surfaces type errors instead. This staging matches constitution §5.3 "allocations visible via the type system": Phase F.3 makes them visible in MIR (every `__cobrust_str_clone` call shows up in the MIR dump); Phase G makes them visible in source.

#### Phase 5 — Code-comment updates removing TD-1 disclosure (PAIR finalization)

| File | Line | Change |
|---|---|---|
| `crates/cobrust-mir/src/drop.rs` | 122-129 | (covered in Phase 1; comment rewrite is the TD-1 retirement marker) |
| `crates/cobrust-mir/src/lower.rs` | 1716-1725 | (covered in Phase 1) |
| `crates/cobrust-stdlib/src/env.rs` | 56-62 | Update `# Safety` clause on `__cobrust_argv`: replace "M12.x convention: codegen owns the drop schedule" (which TD-1 violated) with "ADR-0050c TD-1 closure: codegen emits per-element `__cobrust_str_drop` followed by `__cobrust_list_drop` via the `Terminator::Drop` arm dispatch in `cranelift_backend.rs:1022-1026`." |
| `crates/cobrust-stdlib/src/io.rs` | 158-166, 213-217, 309-317 | Update each `# Safety` clause that mentions "must eventually be freed via `__cobrust_str_drop`" — change "must" to "is automatically freed by the codegen's drop schedule per ADR-0050c at the binding's scope exit." Source-level user no longer manages Str ownership. |
| `crates/cobrust-stdlib/src/iter.rs` | 270-302 | iter-protocol path: docstring now declares (a) it remains live only for comprehensions (per ADR-0050b §"Maintenance burden"), (b) per-element drop responsibility is the caller's (the comprehension-lowering codegen emits drops as it consumes). |

#### Phase 6 — F5 audit cross-reference: list collection emptiness predicate

Per audit Finding F5 (cited in mission §"Audit Finding F5 cross-reference"), Wave-2 ships `__cobrust_list_is_empty` symmetric to `__cobrust_dict_is_empty` from ADR-0050d Decision 5 addendum. This honors §2.2 implicit-truthy ban uniformly across collections.

| File | Line | Change |
|---|---|---|
| `crates/cobrust-stdlib/src/collections.rs` | after `__cobrust_list_len` at line 459 | Add `__cobrust_list_is_empty(list: *mut u8) -> i64` — returns `i64::from(__cobrust_list_len(list) == 0)`. i64 0/1 matches the SwitchInt codegen convention (see ADR-0044 W2 Phase 3 `__cobrust_str_eq` precedent at `io.rs:502-515`). |
| `crates/cobrust-codegen/src/cranelift_backend.rs` | after line 1924 | `out.push(("__cobrust_list_is_empty", sig(call_conv, &[p], Some(i64))));` |

The shim ships in M-F.3.2 P9-E2 (list[str] impl PAIR), bundled with `__cobrust_list_drop_elems` since both surface during the same crate touch. ADR-0050d Decision 5 addendum's `__cobrust_dict_is_empty` ships in Wave 3 dict impl; M-F.3.3 f64 doesn't need an `is_empty` since `Ty::Float` is Copy + has no length concept.

#### Migration ordering rationale

Phase 1 first because flipping the predicates without changing codegen is **a safe no-op** (drop pass enumerates, codegen no-op-jumps); this lets the corpus author confirm the drop pass produces well-formed CFGs before codegen learns to act on them. Phase 2 then makes the drop calls real. Phase 3 adds the shims they call. Phase 4 closes the explicit-clone pattern. Phase 5 cleans the doc-strings. Phase 6 is the F5 §2.2 uniformity addendum (cheap +30 LoC; lands in the same M-F.3.2 PAIR).

#### Estimated wall-time for M-F.3.2 P10-direct PAIR (D4 opus, per ADR-0050 §A7)

- TEST agent (sonnet or opus per `cto_operations_runbook.md`): ~3-4 hours to author ≥40 well-typed + ≥20 ill-typed corpus including (a) rebind drops old Str, (b) function-return moves Str, (c) function-arg pass-by-value moves Str, (d) `for s in xs:` over `list[str]` end-to-end with valgrind-clean exit, (e) f-string composition drops the temp at scope exit, (f) list_is_empty short-circuits before list iteration.
- DEV agent (opus per `feedback_subagent_model_tier.md` D4 rule): ~4-6 hours for Phases 1-6 across 3 crates (`cobrust-mir`, `cobrust-codegen`, `cobrust-stdlib`).
- P10 coordinator review + merge: ~30-60 min including 5-gate green on DG workstation.

**Total: 8-12 hours** P10-direct PAIR for M-F.3.2 under Option A. Compares favorably to Option B (24-35 hours), Option C (15-20 hours with the rebind correctness retrofit), and Option D (zero but TD-1 stays open).

## Consequences

### F29 SOP-compliant enumeration — every consumer of shared Str / List infrastructure

Per `findings/adr-cross-surface-bug-fix-scope-creep.md` F29 SOP: when a sub-ADR resolves a soundness bug in shared infrastructure, the §"Consequences" must enumerate every consumer with `also-fixed` / `fixed-later-with-anchor` / `accepted-as-known-debt` per item. Below enumerates by symbol category.

#### Every `__cobrust_str_*` callsite (greppable)

| Symbol | Definition | Non-test consumers (file:line) | Status under ADR-0050c |
|---|---|---|---|
| `__cobrust_str_new` | `stdlib/fmt.rs:75` | `stdlib/io.rs:172` (alloc_str_buffer), `stdlib/env.rs:74` (argv shim), `stdlib/llm.rs:403` (LLM alloc), `codegen/cranelift_backend.rs:932` (f-string buf), `codegen/cranelift_backend.rs:1365` (Aggregate(FormatString) lowering) | **also-fixed** — every caller now produces an owned Str that the new drop pass tracks. No behavior change at the production site; behavior change at the consumer site. |
| `__cobrust_str_push_static` | `stdlib/fmt.rs:88` | `stdlib/io.rs:174`, `stdlib/env.rs:76`, `stdlib/llm.rs:405`, `codegen/cranelift_backend.rs:940`, `codegen/cranelift_backend.rs:1380` | **also-fixed** transitively — push doesn't transfer ownership; the buffer's drop responsibility is at the parent allocation site. |
| `__cobrust_fmt_int / _float / _bool / _str / _repr` | `stdlib/fmt.rs:105,121,137,154,172` | `codegen/cranelift_backend.rs:1394+` (f-string hole dispatch) | **also-fixed** transitively — the f-string buffer they write into is now drop-tracked. |
| `__cobrust_str_len` | `stdlib/fmt.rs:189` | `stdlib/io.rs:244,288,468,570,…` + `stdlib/llm.rs:385` | **also-fixed** transitively — readonly accessor, no ownership effect. |
| `__cobrust_str_ptr` | `stdlib/fmt.rs:206` | `stdlib/io.rs:244,287,574` + `stdlib/llm.rs:384` | **also-fixed** transitively — readonly accessor. |
| `__cobrust_str_drop` | `stdlib/fmt.rs:226` | declared in `codegen/cranelift_backend.rs:1978`; **currently UNCALLED from codegen** (TD-1 anchor) | **THIS IS THE FIX** — Phase 2 codegen amendment makes this the load-bearing call. |
| `__cobrust_str_len_src` | `stdlib/io.rs:463` | `codegen/cranelift_backend.rs:2004` (intrinsic rewrite for `str_len(s)`) | **also-fixed** transitively. |
| `__cobrust_str_at` | `stdlib/io.rs:480` | `codegen/cranelift_backend.rs:2006` (intrinsic rewrite for `str_at(s, i)`) | **also-fixed** — returns owned Str (already does today via `alloc_str_buffer`); per-call result now drop-tracked. Removes the existing leak. |
| `__cobrust_str_eq` | `stdlib/io.rs:502` | `codegen/cranelift_backend.rs:2008` | **also-fixed** transitively — returns i64, no ownership effect. |
| `__cobrust_str_eq_lit` | `stdlib/io.rs:529` | `codegen/cranelift_backend.rs:2012` | **also-fixed** transitively. |
| `__cobrust_str_ord` | `stdlib/io.rs:553` | `codegen/cranelift_backend.rs:2016` | **also-fixed** transitively. |
| `__cobrust_input` / `__cobrust_input_no_prompt` / `__cobrust_input_str_buf` | `stdlib/io.rs:195,218,238` | `codegen/cranelift_backend.rs` + `cli/src/build/intrinsics.rs:53-61` | **also-fixed** — each returns owned Str; now per-binding drop-tracked. |
| `__cobrust_read_line` | `stdlib/io.rs:312` | `cli/src/build/intrinsics.rs:66` (intrinsic rewrite) | **also-fixed**. |
| `__cobrust_parse_int / _parse_int_tok / _count_toks` | `stdlib/io.rs:447,592,612` | `cli/src/build/intrinsics.rs:74,101,105` | **also-fixed** transitively (return i64, no ownership effect; their Str arg is borrowed, not owned-by-shim). |
| `__cobrust_println_str_buf` / `__cobrust_print_no_nl` | `stdlib/io.rs:279,629` | `codegen/cranelift_backend.rs` print dispatch | **also-fixed** transitively — they read the buffer but do not consume ownership; the parent drop pass owns the lifetime. |
| `__cobrust_argv` | `stdlib/env.rs:64` | declared at `codegen` runtime_helper_signatures; emits a List<Str> where each slot is an owned Str pointer | **also-fixed** — but requires the new `__cobrust_list_drop_elems` shim (Phase 3) because the list's elements are each owned. |
| LLM stdlib `alloc_str_buffer` | `stdlib/llm.rs:398` | various LLM shims (~10 callsites within `llm.rs`) | **also-fixed** — same f-string-style buffer; same drop schedule. |

**Total Str consumers benefitted: 18 distinct shim definitions + ~30 consumer callsites across 6 crates.** All are `also-fixed` in this sprint. No `fixed-later-with-anchor` and no `accepted-as-known-debt` for Str.

#### Every Ty::Str-typed local in MIR lowering

| Site | File:line | Status |
|---|---|---|
| f-string temp `_fstr` (`Ty::Str`) | `mir/lower.rs:1081` | **also-fixed** — `_fstr` is now drop-eligible; codegen emits `__cobrust_str_drop` at the binding's scope exit. F-string composition no longer leaks. |
| `ExprKind::Name` operand path producing Str-typed `Operand::Copy` | `mir/lower.rs:1089-1097` | **also-fixed via Phase 4 clone emission** — when the operand resolves to a Str-typed local and the operand-read pattern requires copy semantics (the local is reused), Phase 4 emits an explicit `__cobrust_str_clone` temp. The drop pass then drops both the source and the clone at scope exit. |
| `ExprKind::Lit(Lit::Str(s))` → `Constant::Str(s.clone())` | `mir/lower.rs:1674` | **also-fixed transitively** — string literals materialize at codegen via `.rodata` symbols (`cranelift_backend.rs:materialize_str_data`); they are not heap-allocated and not Str-typed at the MIR-local level. The drop pass excludes them. |
| `Lit::Str` lowering doesn't directly produce a Str-typed local — type checker recognizes the literal as `Ty::Str` constant. | `types/check.rs:972` (`Lit::Str(_) => Ty::Str`) | **also-fixed transitively** — literals are `Constant`, not locals, so drop pass doesn't touch them. |

#### Every `Aggregate::List` containing Ty::Str

| Site | File:line | Status |
|---|---|---|
| `ExprKind::List` lowering (literal list `[a, b, c]`) | `mir/lower.rs:1114-1130` | **also-fixed** — when element type is `Ty::Str`, Phase 2 codegen emits `__cobrust_list_drop_elems(list, __cobrust_str_drop)` at scope exit. (Element type lookup uses the type checker's recorded `iter_element` for the list; the `Ty::List(Box::new(Ty::None))` placeholder in MIR's declaration is overridden by the type checker's recorded element type at codegen time. **This requires Phase 2 codegen to read the recorded element type from `ctx.lookup_ty(def_id)` lifted to the local — a small but explicit threading step the M-F.3.2 PAIR DEV must wire.**) |
| `argv()` → `List<Str>` materialization | `stdlib/env.rs:64-85` | **also-fixed** — the C-ABI shim is unchanged; the codegen calling site's drop schedule now correctly frees both the list and each element. |
| Comprehension lowering producing `Aggregate::List` of element type | `mir/lower.rs:1493-1576` | **fixed-later-with-anchor**: comprehension lowering still uses the iter-protocol (open finding `comp-lowering-zero-sentinel-collision.md`). When that finding closes in Phase G consolidation, list[str]-typed comprehensions inherit the same drop behavior. Until then, comprehensions over `list[str]` carry both the 0-sentinel bug AND a leak; the corpus must include a negative test that locks this honestly. |

#### F-string lowering

| Site | File:line | Status |
|---|---|---|
| HIR f-string → `Aggregate::FormatString` | `mir/lower.rs:1070-1087` | **also-fixed** — the temp local `_fstr: Ty::Str` is drop-eligible after Phase 1; codegen emits `__cobrust_str_drop` at the f-string's scope exit. |
| Codegen f-string buffer construction | `codegen/cranelift_backend.rs:1360-1450` | **also-fixed transitively** — the codegen produces a Str pointer that the new `Terminator::Drop` arm frees. |

#### Iter-protocol consumers (comprehensions only after ADR-0050b)

| Site | File:line | Status |
|---|---|---|
| Comprehension lowering iter init/next/drop | `mir/lower.rs:1493-1576` | **fixed-later-with-anchor** per `comp-lowering-zero-sentinel-collision.md`. Phase G consolidation ADR retires this path; M-F.3.2 corpus locks the current behavior + adds a negative test for the list[str] comprehension leak. |
| For-loop lowering (length-bound, post-ADR-0050b) | `mir/lower.rs:726-836` | **also-fixed** — for-loops now correctly drop loop-var Str at each iteration's scope exit (via the drop pass's per-block enumeration of the loop body block). |

#### Borrow check

| Site | File:line | Status |
|---|---|---|
| Intra-block borrow check at M8 | `mir/borrow.rs:33-76` | **also-fixed transitively** — `moved | dropped` lattice already exists. Phase 1 flip adds more drop-eligible locals; lattice grows monotonically. Use-after-move detection (B1..B5 obligations from ADR-0020) now triggers for Str rebind cases that previously slipped through as Copy. **Edge case**: if Phase 1 surfaces previously-undetected use-after-move in shipped tests, the M-F.3.2 PAIR TEST must flag it. The corpus author MUST grep `intrinsics_input.rs` + `for_protocol_corpus.rs` for any double-use of a single Str-typed local in the same scope. |

#### Comprehensive enumeration count

- 18 distinct `__cobrust_str_*` shim definitions, all `also-fixed` (no deferral, no debt).
- 4 Ty::Str-producing MIR sites, all `also-fixed`.
- 2 Aggregate::List producing Ty::List(Ty::Str), 1 `also-fixed` (literal + argv) and 1 `fixed-later-with-anchor` (comprehensions; existing finding).
- 1 f-string surface, `also-fixed`.
- 1 iter-protocol surface, `fixed-later-with-anchor` per existing finding.
- 1 borrow-check surface, `also-fixed` transitively.

**Total enumeration count: 27 consumers, 25 also-fixed, 2 fixed-later-with-anchor, 0 accepted-as-known-debt.**

This count makes ADR-0050c the broadest F29-SOP enumeration in the Cobrust ADR roster to date. The post-merge audit lane (`feedback_subagent_model_tier.md` + ADR-0050 §"Audit model") MUST verify the enumeration is complete by grep'ing `__cobrust_str_` and `Ty::Str` against the codebase post-merge.

### list[str] knock-on (audit Finding 1.3 carry-forward)

Constitution §2.2 forbids "mutable default arguments." With `Ty::Str` and `Ty::List(_)` now non-Copy, a hypothetical signature like:

```cobrust
fn f(xs: list[str] = []) -> ...
```

becomes a **fresh minefield**: every call site without a user-supplied `xs` would need to materialize a new empty list (otherwise Cobrust would either share-and-leak the default — refused — or evaluate at fn-definition site once and aliased — refused). The Python equivalent of this signature is the canonical mutable-default-argument bug.

**Cobrust resolution**: `list[str]` (and `list[T]` in general for any non-Copy `T`) as a default argument value is **rejected at type-check time** if/when the default-argument feature lands (ADR-0036 candidate / Phase F.4+ optional-param surface). The type checker would emit `TypeError::MutableDefaultArgument { ty: Ty::List(Box::new(Ty::Str)) }` at the `fn` declaration site. f-string composition + dict[str, str] defaults follow the same rule.

This resolution is **forward-looking** — default arguments are not in Phase F.3 scope (ADR-0050b §"`range(a, b, step)` 3-arg form" defers default-arg sugar to Phase G); the rule above is binding when the feature ships. The constitution §2.2 line is already binding today.

### Positive

- TD-1 closes. The drop schedule from ADR-0020 / ADR-0027 is honored end-to-end for the first time since M12.x.
- LC-100 Pattern B `list[str] gap` closes alongside M-F.3.2. The "刷不了 leetcode" wedge gains long-lived correctness.
- v0.2.0 stable tag binding (ADR-0050 §"v0.2.0 stable tag binding") gets its language-half completeness honestly: a Cobrust program can hold an unbounded number of Strs over an unbounded lifetime without leak.
- ADR-0050d dict[str, V] / dict[K, str] inherits the drop schedule monotonically. Wave 3 sprint cost stays as estimated.
- Constitution §2.3 "adopt from Rust — ownership" gains its second concrete win (after `Result<T, E>`).
- Cross-surface F29 enumeration is on the record; future ADRs can cite this enumeration shape as the F29 template.

### Negative

- Implicit-clone insertion at Phase 4 is a compile-time policy that is *not* yet surfaced to source. Phase G must introduce the explicit `clone(s)` PRELUDE function + `#[strict_ownership]` mode; until then, users have no way to see clone emissions except via `--emit mir` diagnostics. Mitigation: M-F.3.2 corpus includes a "MIR has N clones for program P" lock so the count is reproducible.
- The Phase 2 codegen `Terminator::Drop` arm's type-dispatch widens as new non-Copy types ship (dict in Wave 3; future types in Phase G). The dispatch is currently a flat match; if it grows past ~6 arms, refactor to a per-type vtable lookup. Out-of-scope this ADR; documented as a Phase G followup.
- Atomic refcount escape hatch (Option C) is rejected here; users who later want shared-ownership ergonomics for AI prompt-cache workloads will need an explicit `Rc[str]` newtype + `__cobrust_str_borrow` shim, design TBD in Phase G.
- The comprehension `fixed-later-with-anchor` carry-forward means comprehensions over `list[str]` will compile but leak until Phase G consolidation. M-F.3.2 corpus must lock this honestly (negative test "list[str] comprehension leaks N strs until Phase G consolidation; do not rely on this code path for long-lived programs"); the F29 enumeration covers it.

### Neutral / unknown

- Whether to expose `clone(s)` as an explicit source-level PRELUDE function in Phase F.3.5+ (P1 string stdlib bundle per ADR-0050 §"P1 follow-ups") is open. Likely deferred to Phase G alongside the optional-param surface.
- Whether `__cobrust_list_drop_elems` should fuse with `__cobrust_list_drop` (single shim that takes a fn-pointer and falls through to no-op if the pointer is null) is a Phase 3 micro-design question; the M-F.3.2 PAIR DEV decides at impl time. Either shape satisfies the ABI contract.
- Whether the Phase 4 clone-emission rule needs to widen for `Ty::List(_)` reads (today only proposed for `Ty::Str`) is open. The current rule says "list is moved, never copied"; if a corpus test surfaces a list[str] use pattern that requires duplication, the rule extends to `__cobrust_list_clone_elems` (a parallel shim). Tracked as a Phase 4 implementation risk for M-F.3.2 PAIR.

## Evidence

### Greppable anchors (every claim cross-checked at `HEAD=f566026`)

```
crates/cobrust-mir/src/drop.rs:122-129        # is_copy match arm — TD-1 anchor #1
crates/cobrust-mir/src/lower.rs:1716-1725     # is_copy_type match arm — TD-1 anchor #2
crates/cobrust-codegen/src/cranelift_backend.rs:1022-1026  # Terminator::Drop no-op jump — TD-1 anchor #3
crates/cobrust-codegen/src/cranelift_backend.rs:1922       # __cobrust_list_drop signature
crates/cobrust-codegen/src/cranelift_backend.rs:1924       # __cobrust_list_append signature (Phase 6 reference)
crates/cobrust-codegen/src/cranelift_backend.rs:1978       # __cobrust_str_drop signature
crates/cobrust-mir/src/drop.rs:34-129         # drop pass five-phase algorithm
crates/cobrust-mir/src/drop.rs:264-310        # post-drop CFG verifier
crates/cobrust-mir/src/borrow.rs:1-23         # M8 intra-block borrow check (no NLL)
crates/cobrust-mir/src/borrow.rs:33-76        # borrow_check fixpoint
crates/cobrust-mir/src/lower.rs:1081-1087     # f-string `_fstr` Ty::Str temp creation
crates/cobrust-mir/src/lower.rs:1114-1130     # Aggregate::List lowering with Ty::None placeholder
crates/cobrust-mir/src/lower.rs:1493-1576     # comprehension iter-protocol (open finding)
crates/cobrust-mir/src/lower.rs:726-836       # for-loop length-bound lowering (ADR-0050b)
crates/cobrust-stdlib/src/fmt.rs:63-78        # StringBuffer layout + __cobrust_str_new
crates/cobrust-stdlib/src/fmt.rs:189-232      # __cobrust_str_len / _ptr / _drop
crates/cobrust-stdlib/src/io.rs:158-317       # ADR-0044 W2 Phase 2 input/argv/read_line C-ABI
crates/cobrust-stdlib/src/io.rs:447-639       # ADR-0044 W2 Phase 3 parse_int / str_at / str_eq / ord helpers
crates/cobrust-stdlib/src/io.rs:167-178       # alloc_str_buffer helper (mirrored in llm.rs:398)
crates/cobrust-stdlib/src/env.rs:64-85        # __cobrust_argv list<str> materialization
crates/cobrust-stdlib/src/llm.rs:380-485      # LLM stdlib Str producer paths
crates/cobrust-stdlib/src/collections.rs:440-532  # list_get / list_len / list_drop
crates/cobrust-stdlib/src/collections.rs:459  # __cobrust_list_len (Phase 6 anchor for __cobrust_list_is_empty)
crates/cobrust-stdlib/src/iter.rs:278-355     # iter-protocol __cobrust_iter_init / _next / _drop
crates/cobrust-cli/src/build/intrinsics.rs:53-105  # input / read_line / argv / parse_int / str_* intrinsic-rewrite symbol consts
```

### Cross-references

- Constitution `CLAUDE.md` §2.2 (drop list — silent coercion / late binding), §2.3 (adopt from Rust — ownership, borrowing), §5.1 (elegant — one way to do each thing; newtypes where invariants exist), §5.3 (efficient — allocations visible via the type system).
- ADR-0020 §"Drop schedule algorithm" — the drop pass design ADR-0050c relies on (already shipped at M8).
- ADR-0023 §"M9 followups" — Aggregate / Ref / Cast stub deferral (ancestor of this work).
- ADR-0027 §"Drop schedule" + §"Negative" — original TD-1 disclosure site; Aggregate(List) heap-allocation contract.
- ADR-0034 — `Constant::FnRef` lowering, the cross-fn move semantics ADR-0050c preserves.
- ADR-0035 — `lower_condition` shared root primitive; the for-loop / comprehension common ancestor that the drop pass runs over.
- ADR-0044 — W2 Phase 2/3 Str ABI: every `__cobrust_str_*` shim originates here; ADR-0050c closes the W2 Phase 3 TD-1 shortcut.
- ADR-0044a (queued) — `Result<str, IoError>`-typed `read_line()`; orthogonal to ADR-0050c (Result-typed values are independent of Str ownership).
- ADR-0049 — alpha honesty lanes; the input-str-buf fix that surfaced the LC-100 Pattern B finding.
- ADR-0050 §"Sub-ADR slots / ADR-0050c" — recommendation lock that ADR-0050c implements.
- ADR-0050 §"Amendment 2026-05-16 §A1 + §A7" — verified-at-HEAD TD-1 confirmation + P10-direct PAIR dispatch pattern.
- ADR-0050b §"Maintenance burden" addendum — F29 precedent that this ADR's §"Consequences" enumeration follows.
- ADR-0050d Decision 5 addendum — `__cobrust_dict_is_empty` § implicit-truthy uniformity; F5 cross-reference at §"Phase 6".
- `findings/comp-lowering-zero-sentinel-collision.md` — P2 open finding; this ADR's `fixed-later-with-anchor` carry-forward target.
- `findings/adr-cross-surface-bug-fix-scope-creep.md` — F29 candidate methodology that this ADR's §"Consequences" enumeration implements.
- `findings/adr-scope-reality-divergence.md` — F27 candidate methodology that this ADR's §"Verified-at-HEAD audit table" implements.
- `findings/lc100-pattern-b-list-of-str-gap.md` — LC-100 wedge symptom; M-F.3.2 closure target.

## Open questions

1. **Element-type lookup at Phase 2 codegen**: the Aggregate(List) lowering at `mir/lower.rs:1119-1124` declares `Ty::List(Box::new(Ty::None))` as a placeholder; the real element type lives in the type checker's `def_id → ty` table. Phase 2 codegen must thread the element type from the type checker's recorded type for the local's `def_id` (not the MIR `body.locals[i].ty` which has the placeholder). M-F.3.2 PAIR DEV must wire this; if the threading is awkward, an alternative is to fix the MIR lowering at `lower.rs:1119` to materialize the real element type (likely better long-term — exposed in MIR for codegen and audit). **Recommended approach: fix at MIR lowering.**
2. **Comprehension over list[str] in Phase F.3 corpus**: should M-F.3.2 ship a positive comprehension test or only a negative "documented gap" test? Recommendation: **negative test only** until Phase G consolidation closes the open finding; positive test is misleading until both 0-sentinel + drop schedule are fixed for comprehensions.
3. **`Result<Str, E>` return ownership** when ADR-0044a lands: `Result<Str, IoError>` carries Str ownership through the variant tag. The drop schedule for tagged unions is not yet spec'd; ADR-0044a must address. Out-of-scope ADR-0050c.

## Why this ADR now

User prioritization 2026-05-16 + ADR-0050 batch + audit Finding 3.3 collectively force the design-only sprint NOW so M-F.3.2 P10-direct PAIR can dispatch immediately after merge with an unambiguous binding. Per `feedback_third_party_audit_2026_05_09.md` "the project owner has flagged forever-deferral as the dominant honesty risk", a fourth deferral of TD-1 would itself constitute an honesty regression; ADR-0050c is the closure document.

— P9-E1 opus tech-lead, 2026-05-16
