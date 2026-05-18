# 编辑器配置 — Cobrust 语法高亮

## VSCode

1. 在 VSCode 扩展面板中搜索 **"Cobrust Language Support"**，点击 **安装**。
   - 或通过命令行：`code --install-extension cobrust-language-support-0.1.0.vsix`
2. 打开任意 `.cb` 文件 — 语法高亮自动激活。
3. 注释切换快捷键：`Ctrl+/`（Windows/Linux）或 `Cmd+/`（macOS）。
4. 括号匹配和自动补全括号开箱即用。

```mermaid
flowchart LR
    A[打开 .cb 文件] --> B{扩展是否已安装?}
    B -- 是 --> C[语法高亮生效]
    B -- 否 --> D[从 Marketplace 安装扩展]
    D --> C
```

## Vim / Neovim

### 使用 vim-plug

```vim
" 添加到 ~/.vimrc 或 ~/.config/nvim/init.vim
Plug 'cobrust-lang/vim-cobrust'
```

运行 `:PlugInstall`，然后重新打开任意 `.cb` 文件。

### 手动安装

```bash
# Vim
mkdir -p ~/.vim/pack/cobrust/start/vim-cobrust
cp -r tools/vim-cobrust/syntax   ~/.vim/pack/cobrust/start/vim-cobrust/
cp -r tools/vim-cobrust/ftdetect ~/.vim/pack/cobrust/start/vim-cobrust/

# Neovim
mkdir -p ~/.local/share/nvim/site/pack/cobrust/start/vim-cobrust
cp -r tools/vim-cobrust/syntax   ~/.local/share/nvim/site/pack/cobrust/start/vim-cobrust/
cp -r tools/vim-cobrust/ftdetect ~/.local/share/nvim/site/pack/cobrust/start/vim-cobrust/
```

验证方式：`vim -c 'syntax on' examples/fizzbuzz.cb`

## Helix

Helix 使用 Tree-sitter 语法。Cobrust 的 Tree-sitter 语法将在后续里程碑中支持。
目前可以使用 TextMate 回退方案：

1. 将 `tools/textmate-cobrust.tmbundle/Syntaxes/cobrust.tmLanguage` 复制到
   Helix 配置目录。
2. 在 `~/.config/helix/languages.toml` 中添加文件类型关联：

```toml
[[language]]
name = "cobrust"
scope = "source.cobrust"
file-types = ["cb"]
comment-token = "#"
indent = { tab-width = 4, unit = "    " }
```

> **注意**：完整的 Helix Tree-sitter 支持在路线图项目 **F.1.8**（语言服务器）中跟踪。
> TextMate 方案仅提供基础语法着色。

## TextMate / Sublime Text

1. 双击 `tools/textmate-cobrust.tmbundle` — TextMate 会自动安装。
2. 对于 Sublime Text：将 bundle 复制到 `Packages/User/` 并重启编辑器。

## 语言服务器 (LSP, wave-1:诊断)

Cobrust 提供语言服务器协议(LSP)实现 `cobrust-lsp`,可在编辑时直接
将编译器错误浮现在编辑器中。

**Wave-1 范围(根据 ADR-0057a):**

- `textDocument/publishDiagnostics` —— Cobrust 编译流水线(parse + lower +
  type-check)中的每个 `TypeError` / `MirError` / `LoweringError` 都会以
  LSP `Diagnostic` 形式发布,包含:
  - `cobrust check` 的规范错误信息;
  - 结构化的 `code` 字段(例如 `"implicit-truthiness"`),供编辑器侧
    code-action 路由使用;
  - ADR-0052b 的 `suggestion` 字段(若已设置)作为
    `relatedInformation[0].message` 附加 —— agent-LLM 直接消费的修复路径。

**Wave-2+(后续):** hover、补全、定义跳转、重命名、codeAction。
roster 参见 ADR-0057。

### 构建与运行

```bash
# 在仓库根目录
cargo build --release -p cobrust-lsp
# 产物路径:target/release/cobrust-lsp
```

### VSCode / Cursor 配置

在 `~/.vscode/extensions/<your-ext>/extension.js` 添加一个最小客户端,
通过 stdio 为 `.cb` 文件启动 `cobrust-lsp`:

```javascript
const { LanguageClient } = require('vscode-languageclient/node');
const serverOptions = { command: '/path/to/cobrust-lsp' };
const clientOptions = {
  documentSelector: [{ scheme: 'file', language: 'cobrust' }],
};
new LanguageClient('cobrust', 'Cobrust LSP', serverOptions, clientOptions).start();
```

### Neovim 配置 (nvim-lspconfig)

```lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')
configs.cobrust = {
  default_config = {
    cmd = { '/path/to/cobrust-lsp' },
    filetypes = { 'cobrust' },
    root_dir = lspconfig.util.root_pattern('cobrust.toml', '.git'),
  },
}
lspconfig.cobrust.setup{}
```

## 不包含的功能

- Wave-1 LSP 仅提供诊断。定义跳转、补全、悬浮提示、重命名、code-action
  快速修复均在 ADR-0057b/c/d 范围内。
- 格式化集成 — 参见 `cobrust fmt` CLI 工具。
