#!/usr/bin/env bash
# scripts/snapshot-lint.sh — verify schema invariants in project_state_snapshot.md
#
# Usage:
#   bash scripts/snapshot-lint.sh [SNAPSHOT_PATH] [REPO_ROOT]
#   bash scripts/snapshot-lint.sh --ci-mode [SNAPSHOT_PATH] [REPO_ROOT]
#
# Flags:
#   --ci-mode   Skip Invariant 1 (HEAD freshness). Use when the snapshot
#               lives outside the CI runner's filesystem (user memory dir).
#               Enforces Invariants 2-4 only.
#
# Arguments (positional, after any flags):
#   SNAPSHOT_PATH  path to project_state_snapshot.md
#                  default: $HOME/.claude/projects/-Users-hakureirm-codespace-Study-Cobrust/memory/project_state_snapshot.md
#   REPO_ROOT      path to repo root (must contain docs/agent/adr/ and docs/agent/findings/)
#                  default: $(git rev-parse --show-toplevel)
#
# Exit codes:
#   0  all enforced invariants pass
#   1  one or more invariant violations (::error:: lines emitted to stderr)
#   2  usage error (bad arguments)
#
# Invariants enforced:
#   Inv 1  snapshot HEAD field == `git log -1 --format=%h main` (skipped in --ci-mode)
#   Inv 2  every ADR file docs/agent/adr/00*.md has a row in the snapshot ADR roster;
#          compact range rows (e.g. "| 0013..0018 |") are recognised for the range they cover
#   Inv 3  every finding file docs/agent/findings/*.md (except README) is mentioned
#          in the snapshot by backtick-quoted basename (without .md extension)
#   Inv 4  binary verification list marker appears exactly once in the snapshot
#
# Designed for macOS bash 3.2 compatibility (no bashisms beyond standard POSIX+bash3).

set -euo pipefail

# --------------------------------------------------------------------------
# Argument parsing
# --------------------------------------------------------------------------

CI_MODE=0

while [ $# -gt 0 ]; do
  case "$1" in
    --ci-mode)
      CI_MODE=1
      shift
      ;;
    --help|-h)
      sed -n '/^# Usage:/,/^[^#]/p' "$0" | grep '^#' | sed 's/^# \?//'
      exit 0
      ;;
    -*)
      echo "::error::Unknown flag: $1" >&2
      exit 2
      ;;
    *)
      break
      ;;
  esac
done

SNAPSHOT="${1:-$HOME/.claude/projects/-Users-hakureirm-codespace-Study-Cobrust/memory/project_state_snapshot.md}"
REPO_ROOT="${2:-$(git rev-parse --show-toplevel 2>/dev/null || echo "$(cd "$(dirname "$0")/.." && pwd)")}"

# --------------------------------------------------------------------------
# Validation helpers
# --------------------------------------------------------------------------

VIOLATIONS=0

emit_error() {
  echo "::error::$1" >&2
  VIOLATIONS=$((VIOLATIONS + 1))
}

# adr_covered_by_snapshot NUM SNAPSHOT_FILE
# Returns 0 if ADR number NUM appears in the snapshot ADR roster, 1 otherwise.
# Recognises:
#   (a) exact row:  "| 0042 |" or "| [0042](...) |"
#   (b) range row:  "| 0013..0018 |" covers any num in [13..18]
adr_covered_by_snapshot() {
  local num="$1"
  local snap="$2"
  local num_int

  # Strip leading zeros for arithmetic
  num_int=$(echo "$num" | sed 's/^0*//')
  num_int="${num_int:-0}"

  # (a) Exact match: table cell contains the zero-padded number or linked form
  if grep -qE "^\|[[:space:]]*\[?${num}\]?" "$snap"; then
    return 0
  fi

  # (b) Range match: scan all "| NNNN..MMMM |" rows in the snapshot
  # Extract lines matching the range pattern and check if num falls in [lo..hi]
  while IFS= read -r range_line; do
    # Extract lo and hi from pattern like "| 0013..0018 |"
    lo=$(echo "$range_line" | grep -oE '[0-9]{4}\.\.[0-9]{4}' | cut -d. -f1)
    hi=$(echo "$range_line" | grep -oE '[0-9]{4}\.\.[0-9]{4}' | sed 's/.*\.\.//')
    if [ -z "$lo" ] || [ -z "$hi" ]; then
      continue
    fi
    lo_int=$(echo "$lo" | sed 's/^0*//')
    lo_int="${lo_int:-0}"
    hi_int=$(echo "$hi" | sed 's/^0*//')
    hi_int="${hi_int:-0}"
    if [ "$num_int" -ge "$lo_int" ] && [ "$num_int" -le "$hi_int" ]; then
      return 0
    fi
  done < <(grep -E '^\|[[:space:]]*[0-9]{4}\.\.[0-9]{4}' "$snap" || true)

  return 1
}

# --------------------------------------------------------------------------
# Pre-flight checks
# --------------------------------------------------------------------------

if [ ! -f "$SNAPSHOT" ]; then
  emit_error "snapshot file not found: $SNAPSHOT"
  exit 1
fi

if [ ! -d "$REPO_ROOT/docs/agent/adr" ]; then
  emit_error "REPO_ROOT does not look like a Cobrust repo (missing docs/agent/adr): $REPO_ROOT"
  exit 1
fi

# --------------------------------------------------------------------------
# Invariant 1: HEAD freshness (skipped in --ci-mode)
# --------------------------------------------------------------------------

if [ "$CI_MODE" -eq 0 ]; then
  REAL_HEAD=$(cd "$REPO_ROOT" && git log -1 --format=%h main 2>/dev/null || true)
  if [ -z "$REAL_HEAD" ]; then
    emit_error "Inv 1: could not read git HEAD for main branch in $REPO_ROOT"
  else
    # Match the HEAD field format: **HEAD**: `4186c8e` or HEAD: `4186c8e`
    SNAPSHOT_HEAD=$(grep -oE '\*\*HEAD\*\*:[[:space:]]*`[a-f0-9]+`' "$SNAPSHOT" \
                    | head -1 | grep -oE '[a-f0-9]{6,}' || true)
    if [ -z "$SNAPSHOT_HEAD" ]; then
      emit_error "Inv 1: could not parse HEAD field from snapshot (expected '**HEAD**: \`<sha>\`')"
    elif [ "$SNAPSHOT_HEAD" != "$REAL_HEAD" ]; then
      emit_error "Inv 1 (F1.1): snapshot HEAD '$SNAPSHOT_HEAD' != real HEAD '$REAL_HEAD' — update project_state_snapshot.md"
    else
      echo "snapshot-lint: Inv 1 OK (HEAD $REAL_HEAD)"
    fi
  fi
else
  echo "snapshot-lint: Inv 1 SKIPPED (--ci-mode)"
fi

# --------------------------------------------------------------------------
# Invariant 2: every ADR file on disk has a row in the snapshot ADR roster
# --------------------------------------------------------------------------

ADR_PASS=1
for adr_file in "$REPO_ROOT"/docs/agent/adr/00*.md; do
  [ -f "$adr_file" ] || continue
  adr_base=$(basename "$adr_file")
  # Extract zero-padded 4-digit number from filename (e.g. 0042 from 0042-foo.md)
  adr_num=$(echo "$adr_base" | grep -oE '^[0-9]{4}')
  if [ -z "$adr_num" ]; then
    continue
  fi
  if ! adr_covered_by_snapshot "$adr_num" "$SNAPSHOT"; then
    emit_error "Inv 2: ADR ${adr_num} (${adr_base}) exists on disk but has no row in snapshot ADR roster"
    ADR_PASS=0
  fi
done
if [ "$ADR_PASS" -eq 1 ]; then
  echo "snapshot-lint: Inv 2 OK (all on-disk ADRs appear in roster)"
fi

# --------------------------------------------------------------------------
# Invariant 3: every finding on disk is mentioned in snapshot
# --------------------------------------------------------------------------

FINDING_PASS=1
for finding_file in "$REPO_ROOT"/docs/agent/findings/*.md; do
  [ -f "$finding_file" ] || continue
  base=$(basename "$finding_file" .md)
  # Skip index file
  [ "$base" = "README" ] && continue
  # Snapshot must contain backtick-quoted basename (without .md)
  if ! grep -qF "\`${base}\`" "$SNAPSHOT"; then
    emit_error "Inv 3: finding '${base}' exists on disk but is not mentioned (as \`${base}\`) in snapshot"
    FINDING_PASS=0
  fi
done
if [ "$FINDING_PASS" -eq 1 ]; then
  echo "snapshot-lint: Inv 3 OK (all on-disk findings mentioned in snapshot)"
fi

# --------------------------------------------------------------------------
# Invariant 4: binary verification list appears exactly once
# --------------------------------------------------------------------------

# Match the canonical binary verification marker line
MARKER='cobrust build examples/hello.cb'
COUNT=$(grep -c "${MARKER}" "$SNAPSHOT" || true)
if [ "$COUNT" -ne 1 ]; then
  emit_error "Inv 4: binary verification list marker ('${MARKER}') appears ${COUNT} time(s); must appear exactly once"
else
  echo "snapshot-lint: Inv 4 OK (binary verification list appears exactly once)"
fi

# --------------------------------------------------------------------------
# Summary
# --------------------------------------------------------------------------

if [ "$VIOLATIONS" -eq 0 ]; then
  echo "snapshot-lint: all enforced invariants OK"
  exit 0
else
  echo "snapshot-lint: ${VIOLATIONS} violation(s) found — see ::error:: lines above" >&2
  exit 1
fi
