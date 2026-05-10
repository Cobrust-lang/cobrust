# Cobrust Language Support

Syntax highlighting, bracket matching, and comment toggling for the
[Cobrust](https://github.com/cobrust-lang/cobrust) programming language.

## Features

- **Syntax highlighting** for `.cb` files:
  - Keywords: `fn`, `let`, `if`, `elif`, `else`, `while`, `for`, `match`, `case`, `return`, `import`, `from`, `as`, `with`, `in`, `not`, `and`, `or`, `True`, `False`, `None`
  - Primitive types: `i64`, `f64`, `bool`, `str`, `i32`, `u64`, etc.
  - Built-in generic types: `List`, `Dict`, `Set`, `Option`, `Result`
  - String literals (plain, triple-quoted, f-strings with `{...}` interpolation)
  - Numeric literals (decimal, hex `0x...`, binary `0b...`, octal `0o...`, floats, imaginary `j`)
  - Comments (`#` to end of line)
  - Decorators (`@name`)
  - Operators: `+`, `-`, `*`, `/`, `%`, `==`, `!=`, `<`, `<=`, `>`, `>=`, `=`, `->`, `:`, `**`, `//`
- **Bracket matching**: `()`, `[]`, `{}`
- **Auto-closing pairs**: parentheses, brackets, braces, quotes
- **Comment toggle**: `#` line comment via `Ctrl+/` (or `Cmd+/` on macOS)
- **Indentation**: auto-indent after `:`, dedent on `elif`/`else`/`except`/`finally`
- **Code folding**: indentation-based (off-side rule)

## Installation

### From the Marketplace

Search for **"Cobrust Language Support"** in the VSCode Extensions panel
(`Ctrl+Shift+X` / `Cmd+Shift+X`) and click **Install**.

### From a `.vsix` File

```bash
code --install-extension cobrust-language-support-0.1.0.vsix
```

### Manual (development)

```bash
cd tools/vscode-extension
npm install -g @vscode/vsce
npx @vscode/vsce package
code --install-extension cobrust-language-support-0.1.0.vsix
```

## Quick Start

1. Open any `.cb` file — syntax highlighting activates automatically.
2. Toggle a line comment with `Ctrl+/` (`Cmd+/`).
3. Type `fn my_func(` — brackets auto-close.

## Example

```cobrust
# FizzBuzz in Cobrust
fn main() -> i64:
    let n: i64 = 1
    while n <= 15:
        if n % 15 == 0:
            print("FizzBuzz")
        elif n % 3 == 0:
            print("Fizz")
        elif n % 5 == 0:
            print("Buzz")
        else:
            print_int(n)
        n = n + 1
    return 0
```

## Language Reference Quick Card

| Category | Examples |
|---|---|
| Control flow | `if` `elif` `else` `while` `for` `match` `case` `return` `break` `continue` |
| Declarations | `fn` `let` `class` `type` |
| Operators | `and` `or` `not` `in` `as` `from` `import` |
| Literals | `True` `False` `None` |
| Primitives | `i64` `f64` `bool` `str` `i32` `u64` `f32` … |
| Generics | `List` `Dict` `Set` `Option` `Result` |
| Strings | `"..."` `'...'` `"""..."""` `f"..."` |
| Comments | `# comment to end of line` |

## What This Extension Is NOT

This is **syntax highlighting only** (TextMate grammar + language config).
Language server features (go-to-definition, type checking, completions,
diagnostics) are tracked in roadmap item **F.1.8** and are not yet available.

## License

Apache-2.0 OR MIT — same dual license as the Cobrust project.
