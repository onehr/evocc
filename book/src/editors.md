# Editor diagnostics

`evo lsp` gives syntax diagnostics and safe repair actions for C files. It checks
the open buffer. It does not save the buffer, run the C code, write objects or
apply repairs on disk. Thus it does not find undefined behavior at run time and
does not prove the program: use `evo check` for an executed-path check. It gives
no completion, hover, formatting, navigation or rename service, and it has no
network listener.

## Zed using project settings

Build EvoCC using the [installation instructions](install.md), then merge these
keys into the project's `.zed/settings.json`. Replace the executable path with
your absolute `evo` path and keep the matching `evocc` executable beside it.

```json
{
  "lsp": {
    "clangd": {
      "binary": {
        "path": "/absolute/path/to/evocc/target/release/evo",
        "arguments": ["lsp"]
      }
    }
  },
  "languages": {
    "C": { "language_servers": ["clangd"] }
  }
}
```

Restart the project's language server after changing the setting, then open a
C file. This setting uses Zed's `clangd` adapter slot, but it replaces the
project's clangd executable with `evo`, so Zed does not run both servers.
[Zed's language configuration documentation](https://zed.dev/docs/configuring-languages)
describes binary overrides and project-local settings.

## VS Code from a checkout

Build EvoCC using the [installation instructions](install.md). The extension
requires desktop VS Code 1.85 or newer and Node.js for its build command.
From the repository root, run:

```sh
npm --prefix editors/vscode run build
code --extensionDevelopmentPath="$PWD/editors/vscode" "$PWD"
```

The build checks JavaScript syntax and copies `extension.ts` to the ignored
`output/extension.js`. The source deliberately uses JavaScript-valid TypeScript;
no TypeScript compiler, language-client library or downloaded package is needed.
This launches an Extension Development Host, not a Marketplace installation.

In that window, set **Evo: Path** (`evo.path`) to the absolute path of the `evo`
executable. For a source build, use
`/absolute/path/to/evocc/target/release/evo`. Keep its sibling `evocc`
executable available. Reload the window after changing this setting. The default
is `evo` from the extension host's `PATH`, which can differ from a terminal's.
The extension runs only in trusted workspaces and handles local `file:` C
documents; select C as the language mode when needed.

Open a `.c` file containing:

```c
enum { bad = 1 / 0 };
int main(void) { return 0; }
```

The constant-expression error appears as an editor diagnostic. Change the
denominator to `1`; the error clears after the change is checked, before saving.

For a safe Quick Fix, replace the buffer with:

```c
int f(int unused) { return 0; }
```

The `W0002` warning offers insertion of `(void)unused;`. Selecting the action
edits the buffer through VS Code's normal undoable edit mechanism. It does not
save the file. Review-only repairs are not offered. The client rejects action
responses for an older document version. Use the Problems panel to inspect
diagnostics and **Output > EvoCC** to inspect server failures.

## Neovim's built-in client

From Lua in an open C buffer, start the built-in LSP client without a plugin:

```lua
vim.lsp.start({
  name = "evo", cmd = { "/absolute/path/to/evo", "lsp" }, root_dir = vim.fn.getcwd(),
})
```

Use your actual executable path. The same diagnostics and safe actions are
available; invoke `vim.lsp.buf.code_action()` to request an action.

## Compilation context and protocol

The server uses the matching `compile_commands.json` entry when available,
including supported include-path and macro-definition flags. Without an entry,
it uses the ordinary syntax-check defaults with warnings enabled.
Included headers come from disk: separate unsaved header buffers are not merged
into another document's compilation. Save a changed header before rechecking
its source file.

Other clients can start `evo lsp` over standard input and output. It uses
[LSP 3.17](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/)
JSON-RPC messages with `Content-Length` headers, UTF-8 payload bytes and UTF-16
positions. Send `initialize`, then `initialized`; use full-text `didOpen` and
`didChange` synchronization, `didSave`, and `didClose`. Saves check the current
open buffer. The server publishes versioned diagnostics and answers
`textDocument/codeAction` with safe, versioned `WorkspaceEdit.documentChanges`.
End with `shutdown`, then `exit`.
