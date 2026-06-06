---
finding_id: F80
title: `cobrust build` Debug-renders type errors (raw struct) instead of routing through error_ux — a §2.5-B UX gap
date: 2026-06-06
status: open
severity: minor
relates_to: ["claude.md:§2.5", "claude.md:§5.1", adr:0094, "finding:f79"]
discovered_by: the F79 scalar-negative-index reject adversarial audit
---

# F80 — `cobrust build` Debug-renders type errors

## What (verified at HEAD 5cb205b)

The `cobrust build` command prints a type error as the **raw `Debug`
struct**, not the polished `error_ux` message that `cobrust check` emits:

```
$ cobrust build x.cb
cobrust build: type error: TypeMismatch { expected: Int, actual: Str, span: Span { file: FileId(0), start: 4413, end: 4430 }, suggestion: Some("change the expression type or add `: <expected>` annotation") }

$ cobrust build neg_index.cb
cobrust build: type error: UnsupportedSliceShape { span: Span { file: FileId(0), start: 4444, end: 4449 }, suggestion: Some("negative `str` indices are not yet supported; for the last codepoint write `s[len(s) - 1]` (a non-negative index)") }
```

It is **general** (verified on two distinct variants — TypeMismatch +
UnsupportedSliceShape), so the whole `cobrust build` type-error path
`Debug`-prints the `TypeError` rather than routing through `error_ux`
(the §2.5-B FIX-printing renderer `cobrust check` uses). The fix text IS
present, but wrapped in noisy `{ span: Span { file: FileId(0), start: ...,
end: ... }, suggestion: Some("...") }` struct syntax.

## Why it matters (§2.5)

§2.5: "the language LLM agents write correctly on the first try" — the
agent's strongest correction signal is the compile-error stderr it
consumes. A raw `Debug` struct (with a byte-offset `Span` + `Some("...")`
wrapper) is harder for an LLM/human to parse than the polished
`error_ux` line (`error[Type]: ... ; <suggestion>` with a real
line:col). Every `cobrust build` type error is affected, not just slices.

## Fix (the queued increment)

Route `cobrust build`'s type-error formatting through the SAME `error_ux`
renderer `cobrust check` uses (grep the `cobrust build` error path — likely
a `format!("{err:?}")` / `eprintln!("... {err:?}")` in
`crates/cobrust-cli/src/build.rs` or `build/mod.rs` — and replace it with
the `error_ux::render_type_error(...)` (or equivalent) call). A regression
e2e should assert a `cobrust build` type error renders the polished
`error[Type]:` form + a `line:col`, NOT `Span { file: FileId`.

## NOT introduced by F79

F79 reused `UnsupportedSliceShape` (no new variant); the Debug-render is a
pre-existing GENERAL `cobrust build` issue surfaced while probing the F79
reject's stderr. F79's reject + suggestion are correct; only the build-
command rendering wrapper is the wart.
