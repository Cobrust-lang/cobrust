---
module_id: editor-support
last_verified_commit: feature/0.1.0-beta-syntax
milestone: T1.5
dependencies:
  - crates/cobrust-frontend/src/token.rs   # keyword ground truth
  - crates/cobrust-frontend/src/lexer.rs   # token pattern ground truth
  - tools/vscode-extension/
  - tools/vim-cobrust/
  - tools/textmate-cobrust.tmbundle/
---

# editor-support

## Purpose

Syntax highlighting for Cobrust `.cb` files in three editor targets.
NOT LSP — that is F.1.8.

## Deliverables

| Path | Type | Status |
|---|---|---|
| `tools/vscode-extension/` | VSCode extension (TextMate grammar + pkg) | done |
| `tools/vim-cobrust/` | Vim/Neovim syntax plugin | done |
| `tools/textmate-cobrust.tmbundle/` | TextMate bundle (optional) | done |
| `tests/syntax-corpus/*.cb` | 5 test files, one per syntax category | done |
| `docs/human/en/editor-setup.md` | Human install guide EN | done |
| `docs/human/zh/editor-setup.md` | Human install guide ZH | done |

## Grammar Ground Truth

Keywords defined in `token.rs` `match_keyword()`:

- **Control flow**: `if elif else while for match case return break continue pass try except finally raise with yield await`
- **Declaration**: `fn let class lambda type`
- **Operator words**: `and or not in as from import`
- **Literals**: `True False None`
- **Primitive types** (constitution §2.2): `i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 bool str bytes isize usize`
- **Built-in generics**: `List Dict Set Option Result Tuple`

## Token Scopes (VSCode TextMate)

| Category | scopeName |
|---|---|
| Control flow keywords | `keyword.control.cobrust` |
| Declaration keywords | `keyword.declaration.cobrust` |
| Word operators | `keyword.operator.word.cobrust` |
| True/False | `constant.language.true/false.cobrust` |
| None | `constant.language.null.cobrust` |
| Primitive types | `storage.type.cobrust` |
| Generic types | `support.type.cobrust` |
| Function definition name | `entity.name.function.cobrust` |
| Function call name | `entity.name.function.cobrust` |
| Decorator | `entity.name.function.decorator.cobrust` |
| String | `string.quoted.double/single/triple.cobrust` |
| F-string | `string.interpolated.cobrust` |
| F-string interpolation | `meta.interpolation.cobrust` |
| Escape sequence | `constant.character.escape.cobrust` |
| Numbers (all) | `constant.numeric.*.cobrust` |
| Comments | `comment.line.number-sign.cobrust` |
| Operators | `keyword.operator.*.cobrust` |

## Extension Metadata

- `name`: `cobrust-language-support`
- `version`: `0.1.0`
- `publisher`: `cobrust`
- `engines.vscode`: `^1.70.0`
- `contributes.languages[0].extensions`: `[".cb"]`
- `contributes.grammars[0].scopeName`: `source.cobrust`

## Build Gate

```bash
cd tools/vscode-extension
npx @vscode/vsce package
# Expected: cobrust-language-support-0.1.0.vsix
```

## Preconditions

- Node.js >= 16 (for `@vscode/vsce`)
- `npx @vscode/vsce package` must exit 0

## Postconditions

- `.vsix` artifact produced at `tools/vscode-extension/cobrust-language-support-0.1.0.vsix`
- Grammar JSON is valid (no JSON parse errors)
- All keywords from `match_keyword()` in `token.rs` covered

## Non-goals

- LSP (go-to-definition, type checking, completions) — deferred to F.1.8
- Tree-sitter grammar — deferred
- Formatter integration — out of scope
- Semantic highlighting — out of scope

## Syntax Corpus Files

| File | Categories |
|---|---|
| `tests/syntax-corpus/01_keywords.cb` | All keywords, control flow, import, class |
| `tests/syntax-corpus/02_strings_and_fstrings.cb` | All string forms, f-strings, escapes, prefixes |
| `tests/syntax-corpus/03_types_and_generics.cb` | Primitives, generics, Option, Result, decorators |
| `tests/syntax-corpus/04_numbers_and_operators.cb` | All numeric literals, all operator tokens |
| `tests/syntax-corpus/05_advanced_patterns.cb` | match/case, comprehensions, generators, lambdas |
