# mc-lsp

The MathCAS language server: the Language Server Protocol over stdio, written directly on `serde_json` with the
protocol's `Content-Length` framing. The `mc-lsp` binary is started by editors.

## Modules
- `rpc`: JSON-RPC messages, base framing and error codes.
- `server`: the message loop, dispatching requests and notifications to the workspace.
- `analysis`: open documents and the answers: diagnostics, go to definition, hover, completion, document symbols.
- `outline`: the definitions of a text, read from its syntax tree.
- `library`: names known before a file is read (keywords, library `FUNCTIONS`, prelude).
- `text`: UTF-16 positions, `file:` URIs and the code of notebooks.
- `main`: the stdin/stdout entry point.

## Design
- Analysis uses `mc-syntax` (parsing with recovery, spans) and `mc-load` (resolution with collected definition sites,
  imports through open buffers and the file system).
- Completion and hover use every library's `FUNCTIONS` table (`mc-libraries`) and help text (`mc-help`).
- The server never evaluates code.
- Positions are exchanged in UTF-16 units as the protocol requires and converted to and from byte offsets.

## Tests
- Integration tests in `tests/` drive the server over the protocol; a script under `tests/vscode` is a smoke test for
  the editor extension.
- Run the Rust tests with `cargo test --release -p mc-lsp`.
