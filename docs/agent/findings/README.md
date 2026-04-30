---
doc_kind: index
last_verified_commit: TBD
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

_(empty — M0 has not produced any findings yet. The first ones are
expected during M1 fuzz testing.)_
