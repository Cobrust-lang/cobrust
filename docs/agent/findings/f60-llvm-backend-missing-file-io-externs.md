---
finding_id: F60
title: LLVM backend never declared the file-IO runtime helpers — latent, surfaced by §X.4 Cranelift removal
status: RESOLVED (file-IO externs ported to llvm_backend.rs; doc-coverage repointed)
date: 2026-05-27
severity: low
siblings: [F53, F55, F56, F58]
last_verified_commit: 0fa6ef4
---

# F60 — LLVM backend lacked file-IO runtime-helper declarations

## §1 Context

Surfaced 2026-05-27 on the §X.4 (Cranelift AOT removal) CI run: `scripts/doc-coverage.sh`
failed its M-F.3.6 check —
`grep ... crates/cobrust-codegen/src/cranelift_backend.rs: No such file or directory`
(X.4 deleted that file). The script had THREE hardcoded greps against the deleted
`cranelift_backend.rs` (M-F.3.5 string, M-F.3.4 dict, M-F.3.6 file-IO) verifying
runtime symbols were declared in the AOT backend.

## §2 Root cause (two layers)

1. **doc-coverage hardcoded the deleted file.** M-F.3.5 + M-F.3.4 repointed cleanly
   to `llvm_backend.rs` (those symbols are in `declare_runtime_helpers`). M-F.3.6
   did NOT — `__cobrust_read_file` (+ the other 6 file-IO symbols) are absent from
   `llvm_backend.rs`.
2. **The LLVM backend never declared the file-IO runtime helpers.** The deleted
   `cranelift_backend.rs` declared all 7 (`read_file` / `read_file_lines` /
   `write_file` / `append_file` / `stdin_read_all` / `stdout_write` /
   `stderr_write`) in its `runtime_helper_signatures` table; `llvm_backend.rs`
   `declare_runtime_helpers` declares str/list/dict/llm/tool/prompt/json — but never
   file-IO. Latent because `crates/cobrust-cli/tests/file_io_e2e.rs` is **0 passed /
   18 ignored** ("M-F.3.6 pre-impl") — the feature was never exercised end-to-end,
   so the missing LLVM externs went unnoticed; only the Cranelift-grepping
   doc-coverage check (and the now-removed Cranelift backend) referenced them.

This is the §X.3/§X.4 detection-gate pattern (cf. F53/F58): the LLVM-default flip +
the Cranelift removal surface latent LLVM-backend gaps masked by the Cranelift path.

## §3 Resolution

1. Ported the 7 file-IO extern declarations to `llvm_backend.rs`
   `declare_runtime_helpers` (signatures verbatim from `cranelift_backend.rs` @
   `f16bdab`: `read_file`/`read_file_lines` `(ptr)->ptr`, `write_file`/`append_file`
   `(ptr,ptr)->i64`, `stdin_read_all` `()->ptr`, `stdout_write`/`stderr_write`
   `(ptr)->i64`) + `runtime_helper_param_counts`. The LLVM (sole AOT) backend can now
   lower file-IO calls — restoring the scaffolding parity the Cranelift backend had
   and a prerequisite for completing M-F.3.6 file-IO (e.g. Stream Z REST work).
2. Repointed all three `scripts/doc-coverage.sh` backend greps
   (`cranelift_backend.rs` → `llvm_backend.rs`).

`file_io_e2e.rs` stays `#[ignore]`'d — completing the file-IO feature end-to-end
(stdlib io.rs + intrinsic wiring) is separate pre-impl work; this finding only
restores the codegen-side declarations + the doc-coverage contract.

## §4 Process note

The §X.4 paired audit was GREEN but did NOT run `scripts/doc-coverage.sh` (the audit
spec omitted it). Lesson: codegen-removal audits MUST include the full CI gate set
(doc-coverage + the shell guards), not just build/test/clippy/fmt — a hardcoded
path in a shell gate is exactly the kind of reference a build-level audit misses.
