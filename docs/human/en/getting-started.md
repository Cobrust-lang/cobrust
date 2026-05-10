# Getting started — 30-second install

## Step 1: install

**Option A — cargo install** (requires Rust toolchain):

```bash
cargo install cobrust-cli
```

**Option B — prebuilt binary** (no Rust needed):

```bash
# macOS arm64
curl -L https://github.com/Cobrust-lang/cobrust/releases/latest/download/cobrust-0.1.0-beta-aarch64-apple-darwin.tar.gz | tar xz
sudo mv cobrust /usr/local/bin/

# Linux x86_64
curl -L https://github.com/Cobrust-lang/cobrust/releases/latest/download/cobrust-0.1.0-beta-x86_64-unknown-linux-gnu.tar.gz | tar xz
sudo mv cobrust /usr/local/bin/
```

Verify: `cobrust --version` → `cobrust 0.1.0-beta`

## Step 2: hello, world

```bash
cobrust new hello && cd hello && cobrust run src/main.cb
```

Expected output:

```
hello, world
```

## Step 3: translate a Python library (optional)

```bash
cobrust translate tomli
```

See [translate.md](translate.md) for the full translation workflow and verification gates.

## Development workflows (contributor path)

```bash
# Clone and build from source
git clone https://github.com/Cobrust-lang/cobrust && cd cobrust
cargo build --workspace

# Run all tests
cargo test --workspace

# Run lints
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# Run doc-coverage
bash scripts/doc-coverage.sh
```

## Further reading

- [Overview](overview.md)
- [Design philosophy](design-philosophy.md)
- [Architecture](architecture.md)
- [Milestones](milestones.md)
- Project constitution [`CLAUDE.md`](../../../CLAUDE.md)
