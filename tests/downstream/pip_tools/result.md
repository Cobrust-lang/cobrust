# pip-tools downstream verdict — 0.1.0-beta T1.1

OUTCOME: PASS

| Fixture | Result | Bytes | cobrust keys | oracle keys |
|---|---|---|---|---|
| `pyproject.toml` | PASS | 400 | `project, tool` | `project, tool` |
| `setuptools-style.toml` | PASS | 289 | `build-system, project, tool` | `build-system, project, tool` |
| `poetry-style.toml` | PASS | 361 | `build-system, tool` | `build-system, tool` |
| `pdm-style.toml` | PASS | 216 | `project, tool` | `project, tool` |
| `loads-on-pyproject.toml` | PASS | 400 | `project, tool` | `project, tool` |

## Method

Each fixture is loaded through `cobrust_tomli.load(fp)` and through CPython's `tomllib.load(fp)`. Outputs are normalised (recursive dict-key sort) then compared. A FAIL row means the LLM-translated cobrust-tomli would have produced a different parse than CPython's tomllib for a real pyproject.toml shape pip-tools encounters in the wild.
