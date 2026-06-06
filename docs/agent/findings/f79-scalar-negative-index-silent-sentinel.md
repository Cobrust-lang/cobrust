---
finding_id: F79
title: scalar negative / OOB index on str + bytes silently returns a sentinel instead of the value or a reject (§2.2 gap)
date: 2026-06-06
status: resolved
resolved_date: 2026-06-06
resolution: "Option A landed — a NEGATIVE-LITERAL scalar index on str + bytes now REJECTS at cobrust check (TypeError::UnsupportedSliceShape, reused — no new cascade), mirroring the slice path's negative-bound reject. Option B (non-literal-negative + from-end indexing + OOB-panic) named-deferred in ADR-0093/0094 §Phasing."
severity: major
relates_to: [adr:0093, adr:0094, "claude.md:§2.2", "claude.md:§2.5", "finding:f78"]
discovered_by: the F78 str-index fix (ADR-0094) adversarial audit
---

# F79 — scalar negative / OOB index silently returns a sentinel

## What (verified at HEAD 5248d8f / the F78 fix tree)

The SCALAR single-index operator on `str` and `bytes` silently returns a
sentinel for a negative or out-of-range index, instead of the Python value
(negatives) or a loud error (OOB):

```
# "hello"[-1]   -> ""  (len 0)   CPython: "o"   (last char)   <- SILENT WRONG
# "hello"[10]   -> ""  (len 0)   CPython: IndexError
# b"abc"[-1]    -> -1            CPython: 99    (last byte)    <- SILENT WRONG
# b"abc"[10]    -> -1            CPython: IndexError
```

`s[-1]` (last element) is the **#1 Python indexing idiom** — silently
returning an empty string / `-1` for it is a §2.2 silent-miscompile and a
§2.5 first-try trap (an LLM writes `s[-1]` constantly).

## Contrast — the SLICE path is already correct (F78 / ADR-0094)

The F78 fix made the SLICE path reject negative/stepped/open shapes at
`cobrust check` (`TypeError::UnsupportedSliceShape`, §2.5-A). The SCALAR
path was left as a documented deferral: `__cobrust_{str,bytes}_get` /
`_char_at` guard `i < 0 -> sentinel` (string.rs / bytes.rs), and the
`(Ty::Str/Bytes, IndexKind::Expr)` check.rs arm only unifies the index with
`Int` — it never rejects a negative. So the scalar arm silently diverges
where the slice arm loudly rejects. This is an INCONSISTENCY + a §2.2 hole.

It is a NAMED deferral in ADR-0093 §Phasing + ADR-0094 §Phasing
("bounds-PANIC / negative-index is a Phase-2 deferral"), so it is tracked,
not silently rotting — this finding elevates it to a finding for the
§2.2/§2.5 visibility it deserves (a common idiom).

## Fix (the queued increment — the cleaner of two options)

**Option A (smaller, §2.5-A compile-time-catch):** reject a NEGATIVE
LITERAL scalar index at `cobrust check` for both `str` and `bytes`
(mirror the slice path's negative-literal reject) — so `s[-1]` errors
loudly with a suggestion, never silently "". A non-literal runtime-negative
keeps the sentinel (a runtime trap is the further follow-up). Apply to BOTH
str + bytes in lockstep (they share the convention).

**Option B (full Python parity):** implement negative indexing
(`s[-1] == s[len-1]`) + an OOB trap (`s[10]` -> IndexError-style panic),
matching CPython. Larger; the honest long-term endpoint.

A regression e2e MUST pin `"hello"[-1]` (reject or "o") + `b"abc"[-1]` +
the OOB cases, CPython-differential.

## Verification note

Reproduced independently at the F78-fix tree: `print("hello"[-1])` prints
a blank line (len 0); the supported forms (`"hello"[1]`=="e",
`"hello"[1:4]`=="ell") are correct (F78/ADR-0094). bytes `b"abc"[-1]`
returns -1 (the existing scalar sentinel).

## Resolution (Option A — 2026-06-06, the negative-LITERAL scalar reject)

Landed **Option A** (the smaller, §2.5-A compile-time-catch). The two
scalar arms in `crates/cobrust-types/src/check.rs` —
`(Ty::Str, IndexKind::Expr)` (str `s[i] -> str`) and
`(Ty::Bytes, IndexKind::Expr)` (bytes `b[i] -> int`) — now detect a
NEGATIVE-INTEGER-LITERAL index (`literal_int_value(e).is_some_and(|v| v <
0)`, the SAME folding the slice path already uses — it catches both a
`Neg(IntLit)` and a folded `-1` `IntLit`) and REJECT it with
`TypeError::UnsupportedSliceShape` (REUSED, payload-free `{ span,
suggestion }` — no new error variant, so NO cascade through
error_cb/fix_safety/error_ux/lsp/types-parity; the byte-parity tripwire
stays GREEN). The diagnostic prints the §2.5-B fix: `s[len(s) - 1]` /
`b[len(b) - 1]` (a non-negative index). Applied to `str` + `bytes` in
LOCKSTEP. So:

```
# "hello"[-1]  -> REJECT at check (was silent "")   §2.2 hole CLOSED
# b"abc"[-1]   -> REJECT at check (was silent -1)    §2.2 hole CLOSED
# "hello"[0]   -> "h"   (non-negative literal: unbroken)
# b"abc"[0]    -> 97    (non-negative literal: unbroken, CPython-exact)
# s[i] (i a variable)   -> still type-checks (the deferred Option-B path)
```

**Differential e2e** (CPython-3 oracle): `str_slice_e2e_06` (`s[-1]`/
`s[-2]` reject, `s[0]`/`s[1]` work, a non-literal `s[i]` builds+runs) +
`bytes_ops_e2e_10` (the lockstep `bytes` twin: `b[-1]`/`b[-2]` reject,
`b[i]`/`b[0]`/`b[1]` give 97/97/98). Both green.

### Still deferred (Option B — the larger, honest endpoint)

A NON-LITERAL runtime-negative index (`s[i]` / `b[i]` where `i` carries a
negative *value*) cannot be caught statically and still hits the
`__cobrust_{str,bytes}` `i < 0` sentinel. Full Python-parity from-end
indexing (`s[-1] == s[len-1]`) + a scalar OOB-PANIC (`s[10]` ->
IndexError-style) are named in ADR-0093 §Phasing + ADR-0094 §Phasing. The
non-literal path is intentionally LEFT type-checking (asserted by
`str_slice_e2e_06c` / `bytes_ops_e2e_10c`) so this increment does not
break `s[i]` for a variable `i`.
