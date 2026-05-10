# vim-cobrust

Vim syntax highlighting for the [Cobrust](https://github.com/cobrust-lang/cobrust)
programming language (`.cb` files).

## Features

- All Cobrust keywords from the official lexer (`token.rs`)
- Primitive and built-in types (`i64`, `f64`, `bool`, `str`, `List`, `Dict`, etc.)
- String literals: plain, triple-quoted, f-strings with `{...}` interpolation
- Numeric literals: decimal, hex, binary, octal, float, imaginary
- Comments (`#`-style)
- Decorators (`@name`)
- Function definition and call site highlighting
- Operator highlighting

## Installation

### Using vim-plug

Add to your `~/.vimrc` or `~/.config/nvim/init.vim`:

```vim
Plug 'cobrust-lang/vim-cobrust'
```

Then run `:PlugInstall`.

### Using Vundle

```vim
Plugin 'cobrust-lang/vim-cobrust'
```

Then run `:PluginInstall`.

### Using packer.nvim (Neovim)

```lua
use 'cobrust-lang/vim-cobrust'
```

### Manual

```bash
mkdir -p ~/.vim/pack/cobrust/start/vim-cobrust
cp -r syntax   ~/.vim/pack/cobrust/start/vim-cobrust/
cp -r ftdetect ~/.vim/pack/cobrust/start/vim-cobrust/
```

For Neovim:

```bash
mkdir -p ~/.local/share/nvim/site/pack/cobrust/start/vim-cobrust
cp -r syntax   ~/.local/share/nvim/site/pack/cobrust/start/vim-cobrust/
cp -r ftdetect ~/.local/share/nvim/site/pack/cobrust/start/vim-cobrust/
```

## Usage

Open any `.cb` file — syntax highlighting activates automatically:

```bash
vim examples/fizzbuzz.cb
```

To enable explicitly in an already-open buffer:

```vim
:set syntax=cobrust
```

## Verify

```bash
vim -c 'syntax on' examples/fizzbuzz/src/main.cb
```

You should see keywords in a different color from identifiers and strings.

## License

Apache-2.0 OR MIT — same dual license as the Cobrust project.
