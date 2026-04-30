---
doc_kind: convention
last_verified_commit: TBD
---

# Agent doc conventions

## Frontmatter (required on every file)

```yaml
---
doc_kind: module | adr | finding | convention | index
module_id: <stable-id>          # for module docs
adr_id: NNNN                    # for ADRs (zero-padded, monotonic)
finding_id: <slug>              # for findings
last_verified_commit: <sha>     # commit at which this was last verified accurate
dependencies: [<stable-id>...]  # other docs this one builds on
---
```

`last_verified_commit: TBD` is acceptable until the first squash-on-merge.
After that, every doc edit updates this field.

## Section structure for module docs

1. **Purpose** — one sentence
2. **Status** — current milestone status (M-stub, M1-delivered, etc.)
3. **Public surface** — type signatures, no narrative
4. **Invariants** — what must always hold
5. **Preconditions / Postconditions** — function-level if relevant
6. **Done means** — verifiable success criteria, as a checklist
7. **Non-goals** — explicit list
8. **Cross-references** — by stable ID

## Section structure for ADRs

1. **Context** — what motivates the decision
2. **Options considered** — bullet list, brief
3. **Decision** — what we chose, in one paragraph
4. **Consequences** — pros / cons / unknowns
5. **Evidence** — links to experiments, benchmarks, prior art

ADR statuses: `proposed` | `accepted` | `superseded` | `deprecated`.
Implementation lands → status flips to `accepted` in the same commit.

## Section structure for findings

1. **Hypothesis** — what we tried to prove
2. **Method** — what we did
3. **Result** — what happened
4. **Conclusion** — actionable takeaway
5. **Cross-references**

Negative results are first-class citizens — log them, do not hide them.

## Style

- **Dense.** No narrative. Tables, bullets, schemas.
- Type signatures over English descriptions.
- Cross-references by stable IDs only.
- "Done means" criteria after every task description.
- One file per stable ID. Never split a module spec across files.

## When the human and agent docs disagree

- For "what does Cobrust do," the human docs are authoritative (they are
  what users read).
- For "how does the implementation work," the agent docs are authoritative
  (they are what implementers read).
- For "why was this decision made," ADRs are authoritative.

If you change behavior, update all three (human zh, human en, agent
module + ADR if applicable) in the same commit. CI's doc-coverage check
exists to catch divergence.
