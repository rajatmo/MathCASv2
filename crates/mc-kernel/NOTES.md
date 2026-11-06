# Package 2: `mc-kernel`, the kernel protocol and `.mcn` notebooks

Status: merged (branch `lib/p2-kernel`, commit 7361d61; merge 216e20c). The protocol works through the built binary
(interrupt and completion included); `.mcn` round-trips; `mc-repl notebook.mcn` and `mc-repl --update notebook.mcn`
work and `--update` is idempotent; sample `examples/notebook.mcn`. On the branch: workspace 469 passed, 0 failed, 6
ignored (guard, 2 threads, peak 2853 MB); clippy and `cargo doc` clean.

## What was built

- **`crates/mc-kernel`** (depends on `mc-bench`, `mc-engine`, `serde` with derive, `serde_json`; `Cargo.lock` gained
  serde, serde_json, itoa, memchr, zmij):
  - `protocol`: typed messages (`Request`, `Response`, `Body`, `ModuleInfo`), `PROTOCOL = 1`, `parse_request`.
  - `cell`: `run_cell(engine, src, origin, line, stop, sink) -> Status` with `CellEvent`, `ErrorReport`, `Status`,
    shared by the server and the notebook runner.
  - `server`: `serve(input, output, EngineConfig)`, with a reader thread and an engine thread.
  - `notebook`: `Notebook::parse`, `print`, `first_lines`; `Cell::{code, text, source, prose, output}`;
    `run(engine, nb, origin, stop, rewrite, show) -> Status`; `write_atomic`.
  - Binary `mc-kernel` with `--threads N`, `--no-prelude`, `--help`.
- **`mc-repl`**: module `notebook` (the headless runner); in `cli`, `Item::Notebook` (chosen by the `.mcn` extension),
  `Options::{update, threads}`, the flags `--update` and `--threads N`, and `run_with(engine, items, update, ...)`
  (`cli::run` keeps its signature and delegates); `main` uses `--threads`. The script output path
  (`host::shown_value`, event printing) is unchanged.

Edits outside the new crate (additions only):
- `mc-engine/src/engine.rs`: `CoreEngine::eval_streaming(src, origin, stop, sink: &mut dyn FnMut(&Event)) ->
  Result<Script, RunError>` (as `eval_in`, calling `sink` with each statement's events as soon as it finishes);
  `CoreEngine::names() -> Vec<String>` (completion names, sorted); the private `evaluate` and `run` take the sink, and
  `eval_in`/`eval_file` pass a no-op one; private helper `is_identifier`.
- `mc-repl`: depends on `mc-kernel`; `pub mod notebook`; `cli.rs`, `main.rs` as above.

## The kernel protocol, version 1 (also in the `mc_kernel` crate docs)

The client starts `mc-kernel` with pipes for stdin and stdout. Every message is one JSON object on one line (UTF-8,
ending in `\n`; a `\r` before it is accepted; blank lines are skipped). One process holds one session; evaluations run
one at a time in arrival order. When the session is ready (prelude evaluated) the kernel writes
`{"id":null,"type":"hello","protocol":1,"version":"0.0.0","engine":"mathcas-v2 0.0.0"}`. `protocol` changes only on an
incompatible change (new optional fields and message types keep it). Clients ignore unknown fields and response
types; the kernel ignores unknown request fields.

### Requests

Every request has a `type` and may have an `id` (any JSON value; `null` when left out); every response repeats it.

| `type` | fields | answer |
|---|---|---|
| `eval` | `src` (required); `cell` (string), `origin` (file path), `line` (number, default 1) | events, then `done` |
| `interrupt` | `id` (of the `eval` to stop; without it, whatever runs) | none |
| `complete` | `prefix` (default `""`) | `completions`, then `done` |
| `modules` | | `modules`, then `done` |
| `reset` | | `done` |
| `shutdown` | | `done`, then exit code 0 |

- `eval`: the whole text is read and resolved first; nothing runs on a read error. `origin` names the source in
  messages and is where `import` looks for module files (default: the current directory, messages say `<cell CELL>`
  or `<input>`). `line` is the file line of the first line of `src`, so positions are file positions. `cell` is echoed
  in a `cell` field of every response.
- `interrupt`: read on a reader thread while an evaluation runs. A running `eval` with that id stops at its next safe
  point (also inside long arithmetic or printing) and ends with `done` status `interrupted`; a queued one is cancelled
  (answers `done` `interrupted` without running); an unknown or finished id is ignored.
- `complete`: names starting with `prefix`, sorted: library functions and constants, built-ins, the prelude, session
  definitions, loaded module names, and public members as `M.f`.
- `modules`: `[{name, origin, public}]`.
- `reset`: a fresh session (definitions, modules, imports gone), prelude evaluated again.
- Closing stdin acts like `shutdown` once the requests already read are answered.
- A line that is not a valid request gets `error` (`bad request: ...`) then `done` status `error` (with its `id` if the
  line is a JSON object with one).

### Responses

Every response has `id` and `type`; responses to an `eval` with a `cell` also have `cell`.

| `type` | fields | meaning |
|---|---|---|
| `output` | `text` | one line of `print` output |
| `failure` | `printed` | a top-level statement's value is `failure(...)`; evaluation continues |
| `value` | `printed` | the last statement's value, unless `null` or a failure already sent |
| `error` | `message`, `line`, `column`, `origin`, `rendered` | a read, resolve or load error (nothing ran), a limit, or a bad request |
| `completions` | `names` | answer to `complete` |
| `modules` | `modules` | answer to `modules` |
| `done` | `status`, `millis` | the last response to every request except `interrupt` |

- Streaming: a top-level statement's `output` and `failure` responses are written as soon as it finishes, before the
  next one starts; print output inside one long statement arrives when it ends.
- `error`: `line`/`column` 1-based, columns in characters; errors without a position (limits, bad requests) have
  `line`, `column`, `origin` null. `origin` is the request's origin, or the imported module file the error is in.
  `rendered` is the terminal text: `origin:line:column: message`, the source line, a caret.
- `done.status`: `ok` (all ran, none failed), `failure` (all ran, some value a failure), `error` (nothing ran, or a bad
  request), `interrupted`, `limit` (preceded by an `error` `limit: evaluation depth` or `limit: fuel`). Requests other
  than `eval` end `ok`. `millis` is the request's wall-clock time.

```
> {"type":"eval","id":1,"cell":"a","src":"print(\"hi\")\nx = 6\nx * 7"}
< {"id":1,"cell":"a","type":"output","text":"hi"}
< {"id":1,"cell":"a","type":"value","printed":"42"}
< {"id":1,"cell":"a","type":"done","status":"ok","millis":0}
> {"type":"eval","id":2,"src":"1 +"}
< {"id":2,"type":"error","message":"expected an expression, found end of input","line":1,"column":4,"origin":"<input>","rendered":"<input>:1:4: expected an expression, found end of input\n  1 +\n     ^"}
< {"id":2,"type":"done","status":"error","millis":0}
> {"type":"complete","id":3,"prefix":"fac"}
< {"id":3,"type":"completions","names":["factor","factor_integer","factor_square_free","factorial"]}
< {"id":3,"type":"done","status":"ok","millis":0}
```

## The `.mcn` notebook format

Plain UTF-8 text; LF or CR LF (a rewrite keeps the file's line ends); a leading BOM is skipped.

- First line: `#! mathcas 2 notebook` (compared word by word).
- Each cell starts with a marker line: `#% cell` (code: the lines up to the next marker are MathCAS source),
  `#% text` (prose: each line `# text`, or `#` for an empty line), `#% output` (the output block of the code cell just
  above: its last run).
- Output lines are a prefix and a space: `#|` print output, `#=` a line of the last value, `#!` a failure, an error
  (with its position in the notebook) or why the run stopped (`interrupted`, `limit: ...`). Anything else in an output
  block is a parse error pointing to the line.
- A marker is exactly one of the three marker lines (trailing spaces allowed); any other `#%` line is a comment. Blank
  lines after a cell are kept. Source before the first marker is a code cell. An output block not after a code cell
  is an error.
- Every marker, text and output line is a comment, so with the header changed to `#! mathcas 2` the file is a script
  whose statements are the code cells in order (tested).
- Printing a parsed notebook gives back the same text for files in the canonical layout.

Running: `mc-repl notebook.mcn` runs the code cells in one session (imports next to the notebook) and prints like a
script (output and values to stdout; failures as `file: failure(...)` and errors to stderr). `mc-repl --update
notebook.mcn` also writes each cell's output block back (atomically: temporary file, then rename; not written if
unchanged; a cell that shows nothing gets no block). Exit codes as scripts: 0 ok; 2 a cell failed (the run continues);
1 read error or malformed notebook (stops); 130 interrupted (stops); 2 a limit (stops). A cell where the run stopped
records why, and later cells lose their output blocks. Notebooks mix with `.mc` files and `-e` in one session.

## Decisions

- **2.1** `interrupt` runs on a reader thread sharing `{running, queued, cancelled}` with the engine thread under one
  mutex (no race between dequeuing and interrupting); no response.
- **2.2** Every other request is answered in order on the engine thread (`complete` during a long evaluation waits).
- **2.3** End of input = shutdown after the requests already read; running evaluations are not interrupted.
- **2.4** Completion: every defined symbol whose name is a plain identifier, library names without definitions, module
  names, public `M.f`; private qualified members left out.
- **2.5** Output streams per statement (`eval_streaming`); streaming within a statement needs a hook in `mc-eval`'s
  output effect.
- **2.6** Markers, text and output lines are comments, so a notebook reads as a script.
- **2.7** A stop clears later output blocks (the file records one run); without `--update` the file is never touched.
- **2.8** File-coordinate error positions come from padding the cell text with blank lines (`line`), so `rendered` is
  right too.
- **2.9** Every request except `interrupt` ends with exactly one `done`.
- **2.10** Ids may be any JSON value.
- **2.11** The value is not sent when `null` or a failure already sent (as scripts).
- **2.12** Three output line kinds (`#|`, `#=`, `#!`).
- **2.13** With `--update`, positions refer to the file as written (earlier cells' new blocks counted), so a second run
  is byte-identical.
- **2.14** Notebooks are recognised by the `.mcn` extension.
- **2.15** `cli::run` keeps its signature; `run_with` was added.
- **2.16** CR LF files stay CR LF.

## Tests (37 new)

- `mc-kernel` unit (9): notebook round trip (LF, CR LF), built notebooks, layout normalisation, parse errors with
  lines, output blocks, atomic write; request/response serialisation; line/column counting.
- `mc-kernel/tests/protocol.rs` (4, through the binary with pipes, 60 s per response): values, print output and
  failures in order, `cell` echo; read errors with line/column/origin and offsets by `origin`/`line`; output streamed
  while `while true` runs, then interrupted; cancelling a queued eval; interrupt without id; ignored interrupt of a
  finished id; interrupt inside `3^(10^9)`; the session continues; completion (library, constants, prelude,
  built-ins, session, modules, private hidden), `modules`, `reset`; bad JSON, unknown type, missing `src`, unknown
  fields, end of input answering queued requests, `shutdown` exit 0.
- `mc-kernel/tests/notebook.rs` (5): the sample is canonical and up to date (a run reproduces it byte for byte) and
  reads as a script; rewriting is idempotent and error lines are notebook lines; a stop is recorded as
  `#! interrupted` and clears later outputs; no rewrite leaves the file untouched.
- `mc-repl/tests/notebooks.rs` (4, on the binary): the sample's stdout, stderr, exit 2; `--update` keeps CR LF, is
  idempotent, does not rewrite when unchanged, leaves no temporary file; errors (exit 1, the error at the written
  line, a later stale output removed, malformed notebook, missing file, `--update` without a notebook); notebooks and
  `-e` share one session with an import next to the notebook.
- `mc-repl` `cli` unit test extended for the new flags.

## Measurements (busy machine, release)

Small evals: 0-1 ms (`millis`); an interrupt ends a running loop within milliseconds (the four protocol tests take
0.06 s together); kernel start to `hello` about 1 ms (engine with prelude).

## Known gaps

- `print` inside one long statement streams only when it ends (2.5); the output of an interrupted statement is lost.
- No `limit` test through the binary (the mapping is in the code).
- `complete` blocks behind a running evaluation (2.2).
- The loader rejects the header `#! mathcas 2 notebook`, so a `.mcn` run directly as a script needs its header
  changed (a one-line addition to `mc-load`'s `header_error` would accept it).
- `reset` briefly holds two sessions.
- Ctrl+C on the binary during a notebook run is not tested (the library path is).

## Notebook additions (round NB, package P2; design 08-notebook.md section 5.2)

Protocol 1 stays compatible: a request without the new fields gets exactly the old responses (tested). mc-repl and
the Jupyter kernel needed no change.

- `hello` has `capabilities`: `values_each`, `numbered`, `history`, `display`, `message`, `names`,
  `shutdown_interrupt` (`protocol::CAPABILITIES`). Clients use an addition only when it is listed.
- `eval` takes `values: "last" | "each"` (default `last`) and `numbered: bool` (default false).
- `values: "each"`: a `value` per top-level statement whose value is shown (not `null`, not followed by `;` on its
  line, not already shown by the statement's own last `print`); one `output` per `print` call (text may hold
  newlines).
- `numbered: true`: each top-level statement of `src` takes the next number of the kernel's line counter (an
  `import` or `module` statement counts as one; starts at 1, again after `reset`); `output`, `message`, `value`,
  `display`, `failure`, `error` carry `n`; `done` carries `first` and `next`. An interrupted statement keeps its number
  (`next - 1` on `done` `interrupted`). `numbered` without `each` sends only the last shown value.
- `message` responses (`{"type":"message","text":...,"tag":"n_dsolve","n":1}`) for library warnings through the new
  native `message(tag, text)` (mc-ode and mc-quad use it); without `each` they are the old `tag: text` output line.
- `display` responses for plots: `{"type":"display","kind":"image","path":...,"width":640,"height":400,"printed":...,"n":5}`
  (`mc_plot::image_of` reads the PNG header; anything else is a plain `value`).
- `names` request (`prefix` optional): `{"type":"names","names":[{"name":"out","kind":"function"}, ...]}` then `done`;
  kinds `function`, `value`, `module`, `symbol`; public module members as `M.f`.
- History: `out(n)` the value of statement n (hidden values and failures too, not `null`), `out()` the previous,
  `out(-k)` the k-th previous; missing: `failure(history, n)`; `out(0)`: `failure(argument, out, 1)`. Only in numbered
  evaluations; `reset` clears it. No `%` syntax (design section 13). Stored in own values of hidden symbols
  (`out#line`, `out#n`), invisible to programs, `names` and `complete`.
- `shutdown` with `interrupt: true` interrupts the running evaluation, cancels the queued ones (`done` `interrupted`
  each), answers and exits; a plain `shutdown` keeps the old behaviour (answers what was read before it). Abort is
  the existing `interrupt`.

API: mc-engine `CoreEngine::eval_each(src, origin, stop, EachOptions { numbered }, sink: FnMut(Report)) -> Result<bool,
RunError>` with `Report::{Output, Message, Value { image }, Failure}`, `CoreEngine::line()`,
`CoreEngine::name_kinds()`; mc-kernel `cell::run_each`, `EachEvent`, `protocol::{Values, CAPABILITIES, NameInfo,
DisplayKind}`, new `Body` variants `Display`, `Message`, `Names`. `CellEvent`/`run_cell` unchanged. Outside: mc-load
`Read::ends` (so statements loaded from module files are not numbered), `out` and `message` in mc-lsp.

Tests: mc-engine `tests/each.rs` (5), mc-kernel `tests/protocol.rs` (+4 through the binary), unit tests for request
and response shapes and the PNG header.

Known gaps: history never trimmed; `print` output not streamed within a statement; an interrupt landing as a
statement finishes drops that statement's output (as before); `;` hides a value only when it follows the statement
on the same line; `names` without a prefix sends about 1,500 entries in one response; display knows PNG only (SVG is
M7); the kill-on-no-answer timers of 5.6 are client side.

## SVG displays (package nb-m4)

Capability `display_svg`: in `values: "each"` mode a value `image("path")` naming an SVG file (by extension or
content) is sent as `display` with `kind: "svg"` and its size in CSS pixels (`mc_nbfile::images::picture_size`: the
root element's `width`/`height` in px, pt, pc, mm, cm or in, else its `viewBox`, else 300 x 150). PNG displays are
unchanged (`kind: "image"`); clients that know only that kind show `printed`. Tests: `svg_pictures_are_displays_of_kind_svg`.

## Round 11 package robust

Each request is answered under `mc_engine::guard::catch`: a panic outside an evaluation's own guard (in the
collector, completion, building a response) answers that request with an `error` ("internal error: ...; the session
was reset"), `done` with status `error`, and a fresh engine; the kernel process goes on. Panics inside an evaluation
are ordinary `failure(internal, ...)` values of their statement and do not reset anything.
