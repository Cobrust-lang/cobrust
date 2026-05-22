---
name: f48
status: RATIFIED
family: F35-sibling derivative
date: 2026-05-22
last_verified_commit: e23d66c
---

# F48 — Version bump must tag discipline

## §1 Pattern

Any commit that bumps `workspace.package.version = "X.Y.Z"` in `Cargo.toml` MUST
include one of:

1. A checklist line in the commit body: `[ ] Tag + publish GH release vX.Y.Z with
   wheel assets`; OR
2. An immediate follow-up tag (`git tag vX.Y.Z`) + GH release in the same push session.

A version bump commit that satisfies neither condition creates an ambiguous release
state: the version string in binaries diverges from the tag index, and downstream users
installing from the registry receive a binary whose announced version has no matching
release artifact.

## §2 Empirical

v0.6.0 retro audit (2026-05-22) surfaced that ADR-0067/0068/0069 were committed with
`last_verified_commit: TBD` — a documentation variant of the same drift pattern.
The version discipline gap is the packaging-layer parallel.

## §3 Detection Rule (CI Gate Candidate)

Post-merge hook (or PR check):

```bash
# pseudo-code for CI gate
OLD_VER=$(git show HEAD~1:Cargo.toml | grep '^version' | head -1)
NEW_VER=$(git show HEAD:Cargo.toml   | grep '^version' | head -1)
if [ "$OLD_VER" != "$NEW_VER" ]; then
    # version bump detected — verify tag exists
    TAG="v$(echo $NEW_VER | sed 's/version = "//;s/"//')"
    if ! git tag --list "$TAG" | grep -q "$TAG"; then
        echo "F48 VIOLATION: version bumped to $TAG but tag does not exist"
        exit 1
    fi
fi
```

Implementation target: `.github/workflows/release.yml` or a dedicated
`.github/workflows/version-tag-gate.yml`.

## §4 Discipline Rules (binding)

- CTO sprint that bumps version MUST push the matching tag before the session ends.
- If tag must be deferred (e.g., waiting for CI green), the version bump commit body
  MUST contain the checklist item as a visible reminder.
- Sub-agents doing doc-only updates MUST NOT bump `workspace.package.version`.

## §5 Status

RATIFIED 2026-05-22 per retro audit pattern recognition (v0.6.0 release session).

## §6 Cross-References

- Finding F35-sibling — commit msg vs diff drift (parent pattern family)
- Finding F36 — fixture name vs behavior drift
- Finding F44 — CI cache stale green false-pass
- `docs/agent/adr/0069-wheel-layout-standardization.md` — wheel publish SOP
