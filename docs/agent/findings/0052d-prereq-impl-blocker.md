---
doc_kind: finding
finding_id: 0052d-prereq-impl-blocker
title: "ADR-0052d-prereq DEV — parser blocker on `&s.method()` (f30wit_method_03)"
related_to: [adr:0052d-prereq, adr:0052a, adr:0052]
last_verified_commit: 1643776
date: 2026-05-17
status: identified
---

# ADR-0052d-prereq DEV — parser blocker on `&s.method()` (f30wit_method_03)

## Summary

DEV impl for ADR-0052d-prereq Wave-2 (method-dispatch infrastructure)
ships 25 well-typed + 13 ill-typed + 5 e2e + 2 of 3 f30wit_method
tests green at HEAD `74f17de` baseline (see Phase 3 cargo log).

**1 structural blocker remains**: `f30wit_method_03_borrow_precedence_
binds_tighter_than_method_call` requires the parser to admit
`&<call-result>` for the specific case where the call is a method-
form (`&s.method()` parses as `&(s.method())` per ADR-0052
F-G.3 amendment).

## Discrepancy

ADR-0052d-prereq §"Precedence with 0052a `&s`" line 117-121 states:

> No parser change needed: the existing `parser.rs:1239-1249`
> Attribute production + `parser.rs:1105-1110` borrow-operand
> validator already produce `Unary(Borrow, Call(Attr(s, "method"),
> args))` for `&s.method(args)`.

**Empirical reality** (verified at HEAD `1643776` worktree
`feature/0052d-prereq-dev`): the parser's `validate_borrow_operand`
at `crates/cobrust-frontend/src/parser.rs:1134-1139` explicitly
rejects `ExprKind::Call { .. }` as a borrow operand with the message:

```
"borrow of a call-result is not supported in Wave-1
 (ADR-0052a §8 cap: borrow operand must be `Name`, `Name.field`,
  or `Name[idx]`)"
```

This is the ADR-0052a Wave-1 §8 cap. Method-form parses as a
`Call(Attr(...), args)`, so `&s.method()` hits this cap and rejects
at parse time before the borrow can wrap the method-call.

## Test failure manifestation

`f30wit_method_03` uses source:

```cobrust
fn read_i64(n: i64) -> i64:
    return n
fn main() -> i64:
    let s: str = "hello"
    let r: i64 = read_i64(&s.len())
    return r
```

Parse error at `&s.len()`: "borrow of a call-result is not supported
in Wave-1".

## Scope-of-fix analysis

Three resolution paths:

### A. Parser change — admit `&<method-form-Call>` selectively
Modify `validate_borrow_operand` to accept `ExprKind::Call` when
the callee is `Attr` (i.e. the call is a method-form) AND we can
prove the method-table return type is Copy (Int / Bool / Float).
This is a parser-side scope expansion of the Wave-1 §8 cap.

**Concern**: this couples the parser to the type-checker's
method-table knowledge, which the ADR explicitly tried to avoid.
The cleaner design is to lift the §8 cap entirely (Phase G+ scope
per ADR-0052a §"Out of scope"), but that's a separate sub-ADR.

### B. Test rewrite — express the precedence assertion differently
The test could check precedence via a different surface, e.g.
`let n = s.len(); let r = read_i64(&n)`. But that no longer
witnesses `&s.method()` precedence — it witnesses two-step
read.

### C. Defer f30wit_method_03 to 0052d follow-up
Mark the test `#[ignore]` with a clear "deferred: parser cap-
expansion needed for &Call(method-form)" note, file this finding,
ship Wave-2 with 2/3 f30wit_method passing + a known follow-up.

## DEV's chosen path (Wave-2)

**Path C** — defer. Per the dispatch contract HARD-BANNED rule #1
(no test-file edits beyond `#[ignore]` removal), DEV cannot rewrite
the test source. Per the dispatch contract STOP-and-file rule
("If you discover the impl needs scope beyond §3 method-table
addition (e.g. parser change despite ADR claiming 'no parser change
needed'), STOP and file findings/0052d-prereq-impl-blocker.md..."),
DEV files this finding and leaves f30wit_method_03 failing.

The 0052d follow-up impl ADR (parent ADR-0052d, post-prereq) MUST
decide between Path A and Path B before consuming this corpus. The
recommendation is Path A coordinated with ADR-0052a follow-up-A
(tuple-field syntax) under a single "Wave-1+2 §8 cap relaxation"
sub-ADR.

## Empirical context

- `f30wit_method_01` (`s.split` rewrite) — GREEN.
- `f30wit_method_02` (`xs.len()` rewrite to `list_len`) — GREEN.
- `f30wit_method_03` (`&s.len()` precedence) — RED, parser-cap-blocked.

Properties (a) and (b) of the F30 shadow-flip witness are intact
for the two passing tests. Property (c) (precedence) is the one
deferred.

## Cross-ADR coordination

ADR-0052a §"Out of scope" line 220 already lists "`&` on literals /
complex expressions without parens" as Wave-1 cap. The §8 cap is
deliberate. The 0052d-prereq §"Precedence" text was authored as
forward-looking ("the existing path already produces the shape"),
but the path is forward-looking, not present-day. The ADR text is
inaccurate; this finding documents the inaccuracy so a future ADR
amendment can correct it.

## Recommended next step

File `ADR-0052d-followup-A` or amend ADR-0052d-prereq §"Precedence"
with a forward-reference to the parser-cap relaxation. Wave-2
ships green on 2/3 + this finding.
