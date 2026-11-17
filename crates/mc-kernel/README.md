# mc-kernel

The kernel protocol and the notebook format: a server that evaluates source text in one engine session over a pair of
streams, the messages it speaks, the `.mcn` notebook format and its runner.

## Modules
- `server`: `serve` answers requests read from an input until `shutdown` or end of input. The binary `mc-kernel`
  serves the protocol on standard input and output (options `--threads N`, `--no-prelude`).
- `protocol`: the messages: `Request` read from the client, responses and events written to it.
- `cell`: runs one source text in an engine and reports `CellEvent`s and a `Status`.
- `notebook`: the `.mcn` format and the notebook runner (`mc-repl notebook.mcn`, `mc-repl --update`); reading and
  writing of the format itself is `mc-nbfile`'s.
- `front_end`: what a front end's session needs beyond the library, such as interactive `manipulate`.

## Design
- Every message is one JSON object on one line (UTF-8, ending in a newline). One process holds one session;
  evaluations run one at a time in the order requests arrive. Every response repeats the request's `id`.
- The kernel writes a `hello` message with the protocol number once the session is ready. The protocol number changes
  only when a message changes incompatibly: clients ignore unknown fields and response types, and the kernel ignores
  unknown request fields.
- Requests include `eval`, `interrupt`, `complete`, `modules`, `reset` and `picture`; the table is in the crate
  documentation.
- A client says in `eval`'s `pictures` which files of a picture it reads (`png`, `svg`, `json`, ...): those are
  written before the response that names the picture, the others when a `picture` request asks for them. A client
  that does not say gets every picture's PNG, SVG and the files beside them when the picture is made. A `reset`
  drops the files that still wait.
- Depends on `mc-engine`, `mc-eval`, `mc-api`, `mc-syntax`, `mc-nbfile`, `mc-plot`, `mc-graphics`, `serde` and
  `serde_json`.

## Tests
- Unit tests in `src`; integration tests in `tests/` for the protocol over in-memory streams, notebook running and
  updating, and display of results.
- Run: `cargo test --release -p mc-kernel`
