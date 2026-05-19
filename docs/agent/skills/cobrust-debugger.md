---
doc_kind: skill
skill_id: cobrust-debugger
title: "Cobrust debugger: cobrust debug, cobrust-dap, lldb pretty-printers"
audience: any LLM agent helping users debug Cobrust programs
load_when: before using cobrust debug, cobrust-dap, or DAP editor integration
last_verified_commit: 396df70
maintainers: P10/user; updated atomically with ADR-0059a/b/c
relates_to: [adr:0059a, adr:0059b, adr:0059c, adr:0057a]
---

# Cobrust Debugger Reference (Phase L wave-1)

Phase L wave-1 is CLOSED. Three distinct user-facing surfaces: `cobrust debug` CLI (0059c), `cobrust-dap` server (0059b), and lldb pretty-printers (0059a).

## 1. cobrust debug — interactive CLI

Builds the source with DWARF info and launches `lldb-18` (or `lldb` on `$PATH`).

```bash
# Launch with debugger attached
cobrust debug src/main.cb

# Attach to running process
cobrust debug attach <pid>

# Set breakpoints at specific lines (repeatable)
cobrust debug src/main.cb --bp 10 --bp 25

# Quiet mode (suppress informational stderr)
cobrust debug src/main.cb --quiet

# Override lldb binary path
cobrust debug src/main.cb --lldb-path /usr/bin/lldb-18
```

**How it works**:
1. Invokes `cobrust build --debug src/main.cb` (DWARF enabled)
2. Auto-imports `tools/lldb/cobrust_printers.py` pretty-printers
3. Spawns `lldb-18` with inherited stdio
4. `--bp N` sets breakpoints before handing control to user

**Wave-1 honest-debt** (do NOT assume these work):
- Runtime frame variable inspection: NOT yet available
- Dict iteration display in lldb: NOT yet available
- Option/enum ADT deep inspection (DI): NOT yet available
- `watch` expressions on Cobrust values: NOT yet available

---

## 2. cobrust-dap — DAP stdio server

Editor DAP-stdio transport (ADR-0059b). Bridges any DAP-capable editor to the lldb backend.

```bash
# Start DAP server on stdio (editor spawns as child)
cobrust-dap

# Via cobrust CLI (same binary, --dap flag)
cobrust debug --dap src/main.cb
```

**Protocol**: DAP (Debug Adapter Protocol) over stdin/stdout.

### VSCode / Cursor launch.json

```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "cobrust",
      "request": "launch",
      "name": "Debug Cobrust",
      "program": "${workspaceFolder}/src/main.cb",
      "cobrust-dap": "${workspaceFolder}/target/debug/cobrust-dap"
    }
  ]
}
```

### Neovim nvim-dap

```lua
local dap = require('dap')
dap.adapters.cobrust = {
  type = 'executable',
  command = 'cobrust-dap',
}
dap.configurations.cobrust = {
  {
    type = 'cobrust',
    request = 'launch',
    name = 'Debug current file',
    program = function() return vim.fn.input('Path: ', vim.fn.getcwd() .. '/src/main.cb') end,
  }
}
```

**Wave-1 DAP support matrix**:

| Request | Status |
|---|---|
| `initialize` | Supported |
| `launch` | Supported |
| `setBreakpoints` | Supported |
| `stackTrace` | Supported |
| `continue` / `next` / `stepIn` | Supported |
| `variables` | **Partial**: primitives only; Cobrust aggregate types show raw pointer |
| `evaluate` | Not yet |
| `attach` | Not yet (Phase L wave-2) |

---

## 3. lldb pretty-printers (ADR-0059a)

Install once. Cobrust types then display readably in lldb and gdb.

### Install

```bash
# Add to ~/.lldbinit
echo "command script import /path/to/cobrust/tools/lldb/cobrust_printers.py" >> ~/.lldbinit
```

Replace `/path/to/cobrust` with the actual path to your cobrust checkout.

### Supported type display

```
(lldb) p my_list
CobList<i64>[1, 2, 3]           # instead of raw memory

(lldb) p my_dict
CobDict{"a": 1, "b": 2}         # insertion-ordered display

(lldb) p my_option
Some(42)                         # not raw ptr
None                             # not null ptr

(lldb) p my_result
Ok("hello")
Err(IoError::NotFound("config.toml"))

(lldb) p my_str
"hello, world"                   # not raw bytes

(lldb) p my_point                # struct
Point { x: 1.0, y: 2.0 }
```

**Wave-1 honest-debt**:
- Dict iteration display inside lldb loops: NOT yet (shows first entry only)
- Option ADT discriminant-inspect (DI): NOT yet (shows raw tag byte)
- gdb support: tested but not CI-gated; may diverge

### gdb pretty-printers (untested in wave-1)

```bash
# Add to ~/.gdbinit
source /path/to/cobrust/tools/gdb/cobrust_printers.py
```

---

## 4. DWARF debug info

Cobrust emits full DWARF info in debug builds. Key behaviors:

- `cobrust build src/main.cb` = debug build (DWARF enabled by default)
- `cobrust build src/main.cb --release` = release build (DWARF stripped)
- Line numbers map `.cb` source lines to DWARF `DW_AT_decl_line`
- Variable names in DWARF match Cobrust `let` binding names
- Struct fields use Cobrust field names (not mangled)

---

## 5. Common debugging workflows

### Print-debug pattern (no debugger needed)

```cobrust
fn debug_list(label: str, xs: &list[i64]) -> None:
    print(f"[DEBUG] {label}: len={xs.len()}")
    for i, v in enumerate(xs):
        print(f"  [{i}] = {v}")
```

### Breakpoint at line N via CLI

```bash
cobrust debug src/main.cb --bp 42
# lldb auto-stops at line 42 of main.cb
(lldb) p my_variable
(lldb) bt
(lldb) continue
```

### Post-mortem on panic

Cobrust panics emit to stderr and terminate with exit code 1. Inspect the panic message:
```
thread 'main' panicked at 'assertion failed: x > 0', src/main.cb:15:5
```

To get a stack trace on panic:
```bash
COBRUST_BACKTRACE=1 cobrust run src/main.cb
```

---

## 6. Scope cautions

These are NOT in wave-1. Do not claim they exist:
- `cobrust debug attach <pid>` works in wave-1 for lldb; DAP attach: NOT yet
- Runtime inspection of Dict / Option ADT in `variables` DAP request: NOT yet
- `watch` breakpoints on Cobrust values via DAP: NOT yet
- LSP hover showing runtime values: NOT a debugger feature; requires separate hover infra (Phase J wave-2+, not landed)

---

## 7. Relation to LSP (ADR-0057a)

The LSP server (`cobrust lsp` / `cobrust-lsp`) is separate from the debugger. LSP covers:
- `textDocument/publishDiagnostics` — compile errors
- `textDocument/hover` — NOT in wave-1
- `textDocument/completion` — NOT in wave-1

The DAP server (`cobrust-dap`) covers runtime debugging. They share the same binary but serve different editor protocols.
