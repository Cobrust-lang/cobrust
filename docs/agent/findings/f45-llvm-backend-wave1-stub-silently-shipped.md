---
name: f45
status: ratified
family: F35-sibling (claim-vs-landed drift) + F37 (silent rot on accepted debt) + F44 (CI green != working)
last_verified_commit: 1adf3af
date: 2026-05-22
---

# F45 — LLVM backend wave-1 stub silently shipped

## §1 Pattern

When a backend has explicit `wave-N stub` comments in source code, the
surface MUST be excluded from feature-complete claims in
README / RELEASE_NOTES / skill docs until an end-to-end smoke test
verifies post-stub behaviour. Without that gate, claims of "feature
complete" cascade from sibling features (Drop, DI, IR-opt, JIT-conv)
while the stub-region collapses silently to a no-op at runtime.

## §2 Empirical

v0.5.0 LLVM backend was tagged + released 2026-05-18 with `--features llvm`
producing object code that **compiles + links** but emits **empty stdout**
on `print("hi")` and silently swallows the result of `print(fib(40))`.

Root cause: two stub regions in `crates/cobrust-codegen/src/llvm_backend.rs`
at v0.5.0 HEAD `c8ba2bd`:

- L1606-1616 `BodyLowerer::lower_call` wave-1 fallthrough: when `func`
  is `Operand::Constant(Constant::Str(name))` (the extern-name callee
  shape MIR emits for stdlib intrinsics), the call is dropped and the
  destination is written with `0`.
- L1746-1752 `BodyLowerer::lower_constant(Constant::Str | Bytes)`:
  returns `opaque_ptr_ty.const_null()`, leaving every `Str`-typed
  local null-pointered.

Both regions have **explicit** "Wave-1 stub" comments citing ADR-0058a §8
deferral language. The Cranelift backend (`cranelift_backend.rs:1383-1521`
+ `1130-1163`) shipped these surfaces fully at M11; the asymmetry was
never re-checked before tagging v0.5.0.

User report (2026-05-22) on a playground machine: `print("hi")` LLVM AOT
emits empty stdout. `print(fib(40))` computes (CPU spins) but never
prints. Cobrust v0.5.0 was supposed to be the "LLM-agent-correctness"
canonical surface (CLAUDE.md §2.5 north star). LLM emits `print(x)`,
gets no observable output, file silent defect.

## §3 Root Cause

**(a) "Wave-N stub" comments are not tracked tasks.**

ADR-0058a §8 was the deferral source. It listed "Runtime-helper /
extern-name Call lowering" as deferred to a follow-up sub-ADR but did
not file a tracking issue. Wave-2 work was implied by the comment
"sub-ADR 0058a-followup or 0058b" — neither materialised. Wave-1's
"deferred" language pretended to be temporary while it became permanent.

**(b) Backend differential gate ran only at compile time.**

`crates/cobrust-codegen/tests/codegen_diff_corpus.rs` 30 LLVM
fixtures all assert "object file emitted, non-empty" — not "binary
runs and prints what Cranelift's binary prints". The asymmetry stayed
invisible because object emission is necessary-but-not-sufficient.

**(c) Phase K feature-complete cascade.**

v0.5.0 README claimed "Phase K LLVM backend feature-complete" because:

- ADR-0058a (wave-1 core) shipped.
- ADR-0058b (PassBuilder + multi-version dispatch) shipped.
- ADR-0058c (DWARF + DI) shipped.
- ADR-0058d (JIT/AOT convergence) shipped.
- ADR-0058e (cranelift_backend substrate delegation) shipped.

Each adjacent landing got a docs update that did not re-check the
non-Drop / non-DI / non-pass / non-substrate surfaces. The stdlib I/O
surface (the most user-visible) was *never* audited at any of those
landings.

**(d) F35-sibling: commit-msg vs diff drift at release scope.**

`RELEASE_NOTES_v0.5.0.md` + `README.md` Phase K block claimed "feature-
complete" by referencing the LSP v1.3 + DAP v1.2 wave landings, NOT
the underlying backend stdlib surface. The release message reflected
the most-recent dispatch shape, not the cumulative state of the
backend's runtime surface.

## §4 Detection Rule (forward)

### §4.1 Pre-tag CI gate

`codegen_diff_corpus::stdlib_io_*` section (added in this sprint)
diffs Cranelift backend stdout vs LLVM backend stdout on hello /
print_int / print_fib_result and 4 other fixtures. ALL must PASS
before tagging a release.

### §4.2 Backend wave-N stub annotation contract

Every `Wave-N stub` comment in backend code MUST cross-reference one of:

- A tracked `#[ignore = "deferred to ADR-NNNN"]` test in the same crate,
- A specific issue URL,
- An open ADR with `status: proposed`,
- A finding URN like `finding:adr0058a-§8-llvm-extern-stub-debt`.

A bare `// Wave-N stub` comment with no cross-reference becomes a
silent debt that fossilizes. CI gate candidate: grep `Wave-\d+ stub`
across `crates/cobrust-codegen/src/` and flag any without one of the
above markers.

### §4.3 Honest-cite at release scope

Release notes claiming "feature-complete" or "feature-parity" MUST
list:

- Which backends are claimed to be at parity.
- For each backend, which extern callees are at parity vs wave-N stub.
- A pointer to the smoke test that proves stdout-equivalence (NOT
  "object file non-empty" — that's a necessary-but-not-sufficient
  gate).

## §5 How-to-apply forward

The post-author audit SOP (already in MEMORY:feedback_post_author_audit_mandatory)
must include a "backend differential stdout check" item for any
release tagged with a backend opt-in flag (`--features llvm`,
`--features cranelift-fallback`, future `--features wasm`, etc).

Static analyzer scan over the MIR for unknown extern names (call to
`Constant::Str(name)` where `name` is NOT in the runtime_helper_decls
table) — flag as wave-1 stub site. Future ADR may emit a compile-time
warning.

## §6 Resolution

This sprint:

1. ADR-0058f authored (filed 2026-05-22, commit `34e5aca`) scoping
   wave-2 = print system + str-buffer subroutines. Wave-3 surfaces
   explicitly tracked in §7 Open Questions with demonstrable-source
   cross-refs.
2. Implementation (commit `89de141`):
   - 9 runtime helpers declared in `declare_runtime_helpers`.
   - `intern_str_payloads` module-level walker.
   - `materialize_str_data` / `materialize_str_buffer` methods.
   - `lower_call` extern-name dispatch.
   - `lower_constant(Str | Bytes)` materialization.
3. 7 stdlib_io_* fixtures landed (commit `1adf3af`) — all pass on
   Mac arm64 + LLVM 18 + libcobrust_stdlib.a.
4. v0.5.1 hotfix release; README + RELEASE_NOTES claim "stdlib I/O
   hookup landed (wave-2); wave-3 surfaces tracked in ADR-0058f §7".

## §7 Cross-references

- ADR-0058a (the wave-1 deferral).
- ADR-0058f (the wave-2 mirror — this F45's resolution).
- F35-sibling — commit msg vs diff drift; same pattern at release scope.
- F37 — silent rot on accepted debt; "wave-N stub" with no `#[ignore]` is
  the rot signal.
- F44 — CI cache stale green false-pass; sibling pattern of "CI green
  doesn't prove working".
- MEMORY:feedback_post_author_audit_mandatory — the audit SOP this
  finding extends.
