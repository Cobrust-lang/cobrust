# Cobrust v0.6.2 — patch release (2026-05-25)

LLVM backend wave-3 fully closed. LLVM-Cranelift feature parity achieved for `--features llvm` users.

## Highlights

### LLVM backend wave-3 closure (ADR-0058g)
All 12 stdlib runtime categories now hooked into the LLVM backend's `lower_call` extern dispatch path. End-users on `--features llvm` no longer encounter wave-1 stub no-ops for any F45a §2 category.

**91 helpers added across 6 sub-waves**:
- **sub-wave-1** (panic + argv) — `unwrap_err()` aborts properly; `sys.argv` populated
- **sub-wave-2** (list runtime, 6 helpers) — `list_new/get/set/len/is_empty/append` all work; LC-100 corpus unblocked
- **sub-wave-3** (dict + set + tuple, 25 helpers) — `Terminator::Drop Ty::Dict` arm added per TD-1 resolution
- **sub-wave-4** (input + read_line, 4 helpers) — stdin family fully wired
- **sub-wave-5** (fmt + iter + math + parse_int + str-methods, 41 helpers) — largest sub-wave
- **sub-wave-6** (LLM router, 13 helpers) — empty-fallback strategy at stdlib boundary, no API key needed for codegen-layer test

**45 cumulative empirical fixtures** (37 wave-3 + 8 wave-2 stdlib_io regression) verify LLVM-Cranelift parity per F50 discipline.

### Discoveries during wave-3 (filings)
- **F51** — `cargo clippy --features llvm` not in CI; feature-gated lints silent-rot. F44-sibling family.

## Backward compatibility

This release is fully backward-compatible:
- Default backend remains Cranelift (no behavior change for default `cobrust build` users)
- `--features llvm` builds gain stdlib runtime parity (previously wave-1 stub no-ops; now real call dispatch)
- Wheel layout unchanged from v0.6.1 (FHS bin/lib/share per ADR-0069)
- Extension v0.2.0 on Open VSX unchanged

## What is NOT in v0.6.2

- Open VSX extension v0.2.0 publish was completed during v0.6.1 cycle (not re-published)
- Single-binary subcommand collapse (ADR-0068) shim retirement deferred to v0.7.0
- LLVM as default backend (still Cranelift); parity unlocks future migration path

## Install paths

1. `brew tap cobrust-lang/cobrust && brew install cobrust` (Homebrew tap auto-bump within ~1h)
2. `cargo install cobrust` (Rust 1.94+)
3. Manual wheel from release artifacts

## Cross-references

- ADR-0058g: `docs/agent/adr/0058g-llvm-backend-wave3-stdlib-hookup-roadmap.md` (entire wave-3 closure)
- F45a: `docs/agent/findings/f45a-llvm-backend-wave3-scope-systemic.md` (12/12 RESOLVED)
- F51: `docs/agent/findings/f51-clippy-feature-flag-silent-rot.md` (new finding)
- v0.6.1 release notes: previous patch release context
