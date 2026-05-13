# Getting started — 30-second install

## Step 1: install

**Option A — cargo install** (requires Rust toolchain):

```bash
cargo install --git https://github.com/Cobrust-lang/cobrust cobrust-cli
# (crates.io publish queued for v0.2.0)
```

**Option B — prebuilt binary** (no Rust needed):

```bash
# macOS arm64
curl -L https://github.com/Cobrust-lang/cobrust/releases/latest/download/cobrust-v0.1.2-aarch64-apple-darwin.tar.gz | tar xz
sudo mv cobrust /usr/local/bin/

# Linux x86_64
curl -L https://github.com/Cobrust-lang/cobrust/releases/latest/download/cobrust-v0.1.2-x86_64-unknown-linux-gnu.tar.gz | tar xz
sudo mv cobrust /usr/local/bin/
```

Verify: `cobrust --version` → `cobrust 0.1.2`

## Step 2: hello, world

```bash
cobrust new hello && cd hello && cobrust run src/main.cb
```

Expected output:

```
hello, world
```

## Step 3: try the AI alpha surfaces (optional)

1. Copy the router example and add your provider credentials:

```bash
cp cobrust.toml.example cobrust.toml
```

2. Configure the routes you need in `cobrust.toml`:
   - `[routing.structured]` for `llm_complete_structured(prompt, schema_json)`
   - `[routing.tools]` for `llm_complete_with_tools(prompt, registry_json)`
   - any custom `[routing.<task>]` for `llm_dispatch(task, prompt)`

3. Call the current AI surfaces as flat prelude functions:
   - `llm_complete(provider, model, prompt)`
   - `llm_dispatch(task, prompt)`
   - `llm_stream(provider, model, prompt)`
   - `llm_complete_structured(prompt, schema_json)`
   - `llm_complete_with_tools(prompt, registry_json)`

Current alpha note:
- These are not `cobrust.llm.*`, `cobrust.prompt.*`, or `cobrust.tool.*` module calls yet.
- If routing or provider configuration is missing, the current alpha returns `""` (or `[]` for `llm_stream`) instead of a detailed runtime error.

See [cobrust.toml.example](../../../cobrust.toml.example) for the config shape and [Architecture](architecture.md) for the full AI stdlib design notes.

## Step 4: translate a Python library (optional)

```bash
cobrust translate tomli
```

See [ADR-0007 translator pipeline](../../agent/adr/0007-translator-pipeline.md) for the full translation workflow and verification gates.

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
