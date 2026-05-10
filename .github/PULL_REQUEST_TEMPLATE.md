<!-- Paste to: Cobrust/.github/PULL_REQUEST_TEMPLATE.md -->

## Summary

<!-- 1-3 bullets: what does this PR do? -->

-
-

## Why

<!-- What problem does this solve, or what feature does this add?
     Link the related issue/discussion if any. -->

Closes #

## Type of change

- [ ] Bug fix
- [ ] Feature
- [ ] Refactor (no behaviour change)
- [ ] Docs only
- [ ] CI / build / tooling
- [ ] Translation product (`cobrust-<lib>`)

## Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings` passes
- [ ] `cargo build --workspace --all-targets --locked` passes
- [ ] `cargo test --workspace --locked` passes (if you ran subset, list which)
- [ ] `bash scripts/doc-coverage.sh` passes
- [ ] Triple-tree docs updated (zh + en + agent) if behaviour changed
- [ ] ADR added under `docs/agent/adr/` if change affects >1 file
- [ ] Tests added / updated for new behaviour
- [ ] No `TODO` without linked issue: `// TODO(#123): ...`
- [ ] No `unwrap()` in non-test code

## Test plan

<!-- How did you verify this? Manual steps, tests run, examples checked. -->

-

## Notes for reviewers

<!-- Anything specific to look at, concern about, or alternative approach considered? -->
