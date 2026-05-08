---
doc_kind: index
last_verified_commit: 62ef6bd
---

# Findings

Negative results, dead ends, and benchmark surprises live here. They
are first-class deliverables — capturing what *doesn't* work is as
valuable as capturing what does.

## How to add a finding

1. Create `<slug>.md` (kebab-case, descriptive).
2. Frontmatter:

   ```yaml
   ---
   doc_kind: finding
   finding_id: <slug>
   last_verified_commit: <sha>
   dependencies: [<stable-id>...]
   ---
   ```

3. Section structure (see `../conventions.md`):
   - **Hypothesis** — what we tried to prove
   - **Method** — what we did
   - **Result** — what happened
   - **Conclusion** — actionable takeaway
   - **Cross-references**

4. Update the index below.

## Index

| Finding ID | File | Status |
|---|---|---|
| `m1-fuzz-method` | [`m1-fuzz-method.md`](m1-fuzz-method.md) | M1 fuzz gate satisfied via proptest; one panic shrunk and fixed |
| `m5-m7-real-llm-validation` | [`m5-m7-real-llm-validation.md`](m5-m7-real-llm-validation.md) | M3 LLM Router validated against a real OpenAI-compatible endpoint; round-trip, cache replay, transport-failure isolation all green |
