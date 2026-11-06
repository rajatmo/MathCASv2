# Package 3: `mc-lsp` (language server) and `editors/vscode` (VS Code extension)

Status: merged (branch `lib/p3-lsp`, commit 004e9e1). On the branch: workspace 466 passed, 0 failed, 6 ignored (guard,
2 threads); clippy and `cargo doc` clean. The extension was never installed into a user's VS Code by the build.

## What was built

**`crates/mc-lsp`** (library and binary; depends on `mc-api`, `mc-syntax`, `mc-load`, the four library crates for their
`FUNCTIONS` lists, `serde_json`):
- `rpc.rs`: `Content-Length` framing (tolerates `Content-Type`; 64 MB cap); standard error codes.
- `server.rs`: the loop. Requests `initialize`, `shutdown`, `textDocument/definition`, `hover`, `completion` (trigger
  `.`), `documentSymbol`; notifications `initialized`, `exit`, `didOpen`, `didChange` (full sync), `didClose`,
  `didSave`. Errors -32601 unknown method, -32602 bad params, -32002 before `initialize`, -32700 bad JSON; exit code 0
  after `shutdown`, else 1; a panic in a handler becomes -32603 for that request and the session continues.
- `analysis.rs` (`Workspace`): open documents and all answers. `outline.rs`: definitions from the syntax tree.
  `library.rs`: names known before reading (library `FUNCTIONS`, prelude, evaluator natives) and `Signatures`
  (signature line plus the comment above). `text.rs`: UTF-16 positions, `file:` URIs (drive letters, UNC,
  percent-encoding), the code view of notebooks. `main.rs`: `--version`, `--help`, `--stdio`.

Features:
- **Diagnostics**: every syntax error with its range; else the loader's first resolution/load error with its range; an
  error inside an imported module file is shown at the `import` (message `file:line:col: ...`, `relatedInformation`
  into that file).
- **Definition**: top-level definitions, module members, `M.f`, imported names, module names in `import`; across files;
  open unsaved buffers win over disk.
- **Hover**: signature line, comment lines above it, file name when elsewhere; library functions show their `FUNCTIONS`
  description; prelude functions their signature and comment.
- **Completion**: after `M.` the public members of `M` (all of them inside `M`); elsewhere keywords, library and prelude
  names, the file's definitions (and its module's members), imported names, module names.
- **Document symbols**: functions, rules, `sym`, values, modules (members as children).
- **Notebooks** (`.mcn`): only code cells are checked.

**`editors/vscode`** (plain JavaScript, no dependencies): `package.json` (language for `.mc`/`.mcn` plus first-line
detection of `#! mathcas`, grammar, three commands, menus, three settings); `syntaxes/mathcas.tmLanguage.json` (header,
`#%` markers, comments, strings and backquoted names, decorators, `fn`/`rule`/`sym` names, `module`/`import` names,
all keywords, numbers incl. balls `1.25~40`, hex, binary, reals, `im`, patterns `x_`/`xs__`/`___`, slots `&1`, calls,
qualified names, operators); `language-configuration.json`; `extension.js` (an LSP client: framing, ids,
cancellation, server requests, document sync, every feature above; server path from the setting, else next to
`mc-repl`, else on PATH; commands "MathCAS: Run File", "MathCAS: Run Selection" (selection or current line as
`mc-repl -e`), "MathCAS: Restart Language Server"); `INSTALL.txt`.

**Tools**: `tools/vsix.ps1` packs the extension into `mathcas-<version>.vsix` with .NET only (zip entries with `/`;
Windows PowerShell 5.1's `Compress-Archive` writes backslashes). `tools/dist.ps1` builds and copies `mc-lsp.exe`,
copies `editors\vscode` into the dist and writes the `.vsix`; `tools/dist-README.txt` has "Editor support".

## Edits outside the crate (additions)

- `mc-load`: `pub mod sites` (`Sites { definitions, resolutions }` with `definitions_of(name, kind)`,
  `resolution_at(origin, offset)`; `Definition { name, kind, origin, span, name_span }`; `DefinitionKind::{Module,
  Value}`; `Resolution { origin, span, names }` with `get(written)`), re-exported; `Loader::read_recorded(store, text,
  origin, known) -> (Result<Read, LoadError>, Sites)` (`Loader::read` unchanged, delegating to a private
  `read_with`); the `Reader` carries an `Option<Recorder>`; `Walk::resolved()`; tests `tests/sites.rs` (2).
- `mc-syntax`: `pub use ident::KEYWORDS`.

## Decisions

- **3.1** Synchronous and stateless: one message at a time; every request re-parses (with recovery) and runs a fresh
  `Loader` with a recorder, so answers always match the buffers and files. Release, busy machine:

  | Lines | Open + diagnostics | Hover | Completion |
  |---|---|---|---|
  | 201 | 3 ms | 3 ms | 4 ms |
  | 2,001 | 23 ms | 29 ms | 27 ms |
  | 10,001 | 121 ms | 119 ms | 147 ms |

- **3.2** Name resolution is `mc-load`'s, as the engine does it; an overlay `SourceProvider` serves open buffers before
  the file system; `known` = library `FUNCTIONS`, prelude names, and a small list of evaluator/engine natives (`map`,
  `print`, `define`, `im`, `inf`). No dependency on `mc-engine`.
- **3.3** UTF-16 columns (the protocol default); an empty span widens to one character.
- **3.4** All syntax errors; only without them the loader's (single, first) error. Diagnostics are republished for
  every open document after any open, change or close.
- **3.5** Repair while typing: with syntax errors, a copy is resolved with each error's line blanked from the error on
  (later rounds whole lines; at most 4 rounds); byte offsets unchanged; load errors from a repaired copy are not
  reported.
- **3.6** If even the repaired copy does not parse, definition and hover use the current file's syntax tree.
- **3.7** Notebook code view: the `#!` line is blanked; a `#% text` or `#% output` line starts a non-code block; any
  other `#%` marker (`#% cell`) starts code; markers stay comments (consistent with package 2's `.mcn` specification).
- **3.8** The prelude and standard modules have no file: no definition target, but hover works from the embedded text.
- **3.9** Run commands use a terminal with `shellPath` = `mc-repl` and an argument array (no shell quoting), in the
  file's folder (imports resolve as for scripts).
- **3.10** The written name at the cursor is looked up in its statement's resolution map, then among the definitions.

## Tests (19 new)

| Where | What | Count |
|---|---|---|
| `mc-lsp` unit | framing; UTF-16 positions; URIs (Windows, Unix); notebook code view; outline; library and prelude entries; signatures and comments; `M.` qualifiers; repair | 9 |
| `mc-lsp/tests/lsp.rs` (binary, framed) | diagnostics with exact ranges (syntax error, private member, unknown module after a non-ASCII name, error inside an imported file shown at the import, cleared on close); definition across an import (imported, qualified, module name, same file, null for a parameter or keyword, unsaved buffer wins, fallback during a syntax error); hover; completion after `M.` (file module, module in the file, inside/outside), general; document symbols; notebooks; untitled document importing `stats`; error codes; request before initialize; exit without shutdown | 5 |
| `mc-lsp/tests/extension.rs` | manifest, grammar and configuration parse and fit; the grammar covers `mc_syntax::KEYWORDS`; `extension.js` passes `--check` and `tests/vscode/smoke.js` drives it with a stand-in editor API against the real `mc-lsp` (prints "skipped" without `node` or `MC_NODE`; run with VS Code's executable and `ELECTRON_RUN_AS_NODE=1`, it passed) | 3 |
| `mc-load/tests/sites.rs` | definition sites and resolutions across an import; sites kept after an error with the reader state restored | 2 |

## Install the extension

1. Put `mc-repl.exe` and `mc-lsp.exe` in one folder on PATH (e.g. the dist folder), or set `mathcas.repl.path` and
   `mathcas.server.path`.
2. Either `code --install-extension mathcas-0.2.0.vsix` (the `.vsix` is in the dist; `tools\vsix.ps1 -OutDir <folder>`
   builds one), or copy `editors\vscode` to `%USERPROFILE%\.vscode\extensions\mathcas.mathcas-0.2.0`.
3. Restart VS Code (or "Developer: Reload Window") and open a `.mc` file; server messages go to the "MathCAS" output
   panel.

Try without installing: `code --extensionDevelopmentPath=<repo>\editors\vscode`. Remove: delete the extension folder.
Settings: `mathcas.repl.path` (default `mc-repl`), `mathcas.server.path` (default next to `mc-repl`, else PATH),
`mathcas.server.enable` (default true).

## Known gaps

- Not tested in a real VS Code window (stand-in API and scratch install/list checks only).
- Full sync only; no references, rename or formatting (signature help and prefix filtering: round 12).
- Locals (parameters, `let`) have no definition target; a name used both locally and globally in one statement jumps
  to the global definition.
- The statement being typed is missing from symbols and completion until it parses.
- An error in an indirectly imported file is shown at the first import statement.
- The evaluator-native name list is kept in `mc-lsp` (a `FUNCTIONS` list in `mc-eval`/`mc-engine` would remove the
  duplication; `mc-kernel`'s completion needs the same names).
- `tools/dist.ps1` under Windows PowerShell 5.1 with stderr captured (`2>&1`) stops on cargo's progress output
  (`$ErrorActionPreference = "Stop"`); run normally it works.

## Round 12 (package nb-docs): documentation in the editor

- **Help index**: every documented function's entry of `mc_help` (usage lines of its page, else its catalogue
  description) is attached to the known names (`library::Entry::help`); names the index has and the lists here lack
  are added.
- **Completion**: documented functions carry their one-line usage as `detail` and, as `documentation`, their usage
  calls, what each gives and a link to the page; the list is filtered by the name being typed (`mc_help::score`, as
  the notebook ranks: prefix, word prefix, subsequence).
- **Signature help** (`textDocument/signatureHelp`, triggers `(` and `,`, retrigger `,`): inside the brackets of a
  call of a documented function (innermost first, an unclosed call counts) its usage calls, the one fitting the
  arguments active, the active parameter (parameter labels as UTF-16 offsets into the call).
- **Hover** of a documented library function: its usage calls and texts and a link
  `[Open the documentation page](command:mathcas.openDocs?["name"])` (percent-encoded).
- **VS Code extension**: signature help provider; command `mathcas.openDocs` (starts `mc-docs <name>`: the setting
  `mathcas.docs.path`, else next to the server or mc-repl, else on PATH; an open documentation window shows the
  page); markdown from the server is trusted for that one command only (`isTrusted.enabledCommands`).
- Tests: 9 unit tests; `tests/lsp.rs` 6 (new `signature_help`; hover with the link, completion filtered with usage
  lines); `tests/extension.rs` 3 (the stand-in API gained the signature help classes; the smoke script checks
  signature help, the trusted link and the new command; ran with VS Code's executable as `MC_NODE`).
