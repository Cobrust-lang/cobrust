#!/usr/bin/env bash
# scripts/cli-tempdir-guard.sh — static guard against persistent /tmp/cobrust-* CLI test dirs.
#
# This check is intentionally cheap: it scans source text only and does not
# compile or run the workspace. It exists because on 2026-05-13 DG workstation
# test/gate runs accumulated 235G of leaked /tmp/cobrust-* directories.
#
# Allowed pattern:
#   tempfile::Builder::new().prefix("cobrust-").tempdir()
#
# Rejected patterns in crates/cobrust-cli/tests/*.rs:
#   std::env::temp_dir().join(format!("cobrust-..."))
#   std::env::temp_dir().join("cobrust-...")
#   HOME or other env paths hard-coded to /tmp/cobrust-...

set -euo pipefail

cd "$(dirname "$0")/.."

TEST_DIR="crates/cobrust-cli/tests"
VIOLATIONS=0

emit_violation() {
    local file="$1"
    local reason="$2"
    printf '::error file=%s::%s\n' "$file" "$reason" >&2
    VIOLATIONS=$((VIOLATIONS + 1))
}

if [[ ! -d "$TEST_DIR" ]]; then
    printf 'cli-tempdir-guard: %s not found; nothing to scan\n' "$TEST_DIR"
    exit 0
fi

while IFS= read -r file; do
    # tempfile RAII is explicitly allowed, including cobrust-* prefixes.
    # This guard only rejects manual persistent paths under the OS temp root.
    if LC_ALL=C perl -0ne 'exit(/std::env::temp_dir\s*\(\s*\).*?cobrust-/s ? 0 : 1)' "$file"; then
        emit_violation "$file" "manual std::env::temp_dir() + cobrust-* path in CLI tests is forbidden. Use tempfile::Builder/TempDir RAII instead. Incident context: on 2026-05-13 DG workstation test/gate runs leaked 235G under /tmp/cobrust-* directories."
    fi

    if grep -q '/tmp/cobrust-' "$file"; then
        emit_violation "$file" "hard-coded /tmp/cobrust-* path in CLI tests is forbidden. Use tempfile::Builder/TempDir RAII instead. Incident context: on 2026-05-13 DG workstation test/gate runs leaked 235G under /tmp/cobrust-* directories."
    fi

done < <(find "$TEST_DIR" -maxdepth 1 -type f -name '*.rs' | sort)

if [[ "$VIOLATIONS" -ne 0 ]]; then
    printf 'cli-tempdir-guard: %d violation(s) found. Replace manual /tmp-style cobrust-* dirs with tempfile::TempDir RAII before merging this guard.\n' "$VIOLATIONS" >&2
    exit 1
fi

printf 'cli-tempdir-guard: OK — CLI tests use RAII temp dirs, no manual /tmp/cobrust-* leaks detected\n'
