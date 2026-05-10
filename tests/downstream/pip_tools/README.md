# pip-tools downstream verification (T1.1)

`pip-tools` (https://github.com/jazzband/pip-tools) parses
`pyproject.toml` via the `tomli` library on Python < 3.11. This
directory vendors a representative subset of pip-tools' tomli usage
so the 0.1.0-beta release can validate the LLM-translated cobrust-tomli
under a real downstream consumer.

## What's exercised

`pip-tools` calls `tomli.load(fp)` (or `tomli.loads(s)`) when reading:
- `pyproject.toml` for `[project]` / `[tool.pip-tools]` sections.
- `requirements*.in` files referenced from `pyproject.toml`.

The `test_pyproject_parsing.py` fixture replays this:
1. Loads `tests/downstream/pip_tools/fixtures/pyproject.toml` via
   `cobrust_tomli` (the wrapper).
2. Asserts the parsed structure matches what stock CPython
   `tomllib.loads()` produces on the same input.
3. Repeats for `tests/downstream/pip_tools/fixtures/poetry-style.toml`,
   `setuptools-style.toml`, `pdm-style.toml` — three popular
   `pyproject.toml` shapes pip-tools encounters in the wild.

## Run it

```bash
COBRUST_TOMLI_BINARY=$(pwd)/target/release/cobrust-tomli-json \
    python3.11 tests/downstream/pip_tools/test_pyproject_parsing.py
```

Exit 0 = PASS. Exit 1 = at least one fixture diverged or raised.

The script writes `tests/downstream/pip_tools/result.md` with the
per-fixture verdict. The 0.1.0-beta finding cross-references this.

## Why subprocess instead of upstream pip-tools install

pip-tools itself bundles a vendor copy of tomli as `pip._vendor.tomli`,
not the user-facing `tomli` package. Testing the in-package vendor
copy would require shimming `pip._vendor`. We instead exercise the
exact tomli call surface pip-tools uses — `tomli.load(fp)` on real
pyproject.toml fixtures — which is the load-bearing API contract.
This isolates "does cobrust-tomli speak the tomli API correctly" from
"does pip-tools install correctly", where the latter is irrelevant to
the translation gate.
