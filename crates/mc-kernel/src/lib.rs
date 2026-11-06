//! The MathCAS v2 kernel protocol and notebook format.
//!
//! - [`server`]: the protocol over a pair of streams; the binary `mc-kernel` serves it on standard input and
//!   output (options `--threads N`, `--no-prelude`);
//! - [`protocol`]: its messages; [`cell`]: running one source text and reporting its events;
//! - [`notebook`]: the `.mcn` format and the notebook runner (`mc-repl notebook.mcn`, `mc-repl --update`).
//!
//! Decisions of this package are numbered 2.1, 2.2, ... in the comments where they are applied.
//!
//! # The kernel protocol, version 1
//!
//! The client starts `mc-kernel` with pipes for standard input and output. Every message is one JSON object on one
//! line (UTF-8, ending with `\n`; a `\r` before it is accepted, blank lines are skipped). One process holds one
//! session; evaluations run one at a time, in the order the requests arrive.
//!
//! On start the kernel writes, once the session is ready (the prelude evaluated):
//!
//! ```text
//! {"id":null,"type":"hello","protocol":1,"version":"0.0.0","engine":"mathcas-v2 0.0.0"}
//! ```
//!
//! `protocol` changes only when a message changes incompatibly; new optional fields and new message types do not
//! change it. Clients ignore fields and response types they do not know; the kernel ignores unknown request fields.
//!
//! ## Requests
//!
//! Every request has a `type` and may have an `id`: any JSON value (a number or a string, typically), `null` when
//! left out. Every response to a request repeats its `id`.
//!
//! | `type` | fields | answer |
//! |---|---|---|
//! | `eval` | `src` (string, required); `cell` (string), `origin` (file path), `line` (number, default 1) | events, `done` |
//! | `interrupt` | `id` (of the `eval` to stop; without it, whatever runs) | none |
//! | `complete` | `prefix` (string, default `""`) | `completions`, `done` |
//! | `modules` | | `modules`, `done` |
//! | `reset` | | `done` |
//! | `shutdown` | | `done`, then the process ends |
//!
//! - **`eval`** evaluates `src` in the session: the whole text is read and resolved first, and nothing runs on a
//!   read error; then the statements run in order. `origin` names the file the text comes from: messages name
//!   it, and `import` looks for module files next to it (without it, the current directory, and messages name
//!   `<cell CELL>` or `<input>`). `line` is the line of `src`'s first line in that file, so that error positions
//!   are file positions (a notebook cell's errors point into the notebook). `cell` is the client's name for the
//!   cell; every response to the request repeats it in a `cell` field.
//! - **`interrupt`** is read and handled while an evaluation runs (decision 2.1: a reader thread). It stops the
//!   running `eval` with that `id` at its next safe point (also inside long arithmetic and printing), which then
//!   ends with `done` status `interrupted`; a queued `eval` with that id is cancelled (it answers `done` with
//!   status `interrupted` without running). An interrupt for an unknown or finished id does nothing. It has no
//!   response of its own.
//! - **`complete`** answers the names starting with `prefix`, sorted: the library functions and constants, the
//!   built-ins, the prelude, the session's definitions, the loaded modules and their public members as `M.f`
//!   (decision 2.4). It is answered after a running evaluation (decision 2.2).
//! - **`modules`** lists the loaded modules.
//! - **`reset`** replaces the session with a fresh one: definitions, modules and imports are gone, the prelude
//!   is evaluated again.
//! - **`shutdown`** answers `done` and ends the process (exit code 0). The end of standard input does the same
//!   once the requests read before it are answered (decision 2.3).
//!
//! A line that is not a valid request is answered with `error` (message `bad request: ...`, carrying the `id` if
//! the line is a JSON object with one) and `done` with status `error`.
//!
//! ## Responses
//!
//! Every response has `id` and `type`; responses to an `eval` with a `cell` also have `cell`.
//!
//! | `type` | fields | meaning |
//! |---|---|---|
//! | `output` | `text` | the text of one `print` call (the text of a string, other values printed) |
//! | `failure` | `printed` | a top-level statement's value is a `failure(...)`; the evaluation goes on |
//! | `value` | `printed` | the value of the last statement, unless it is `null` or a failure already sent |
//! | `error` | `message`, `line`, `column`, `origin`, `rendered` | a read error (nothing ran), a limit, a bad request |
//! | `completions` | `names` | the answer to `complete` |
//! | `modules` | `modules`: `[{name, origin, public}]` | the answer to `modules` |
//! | `done` | `status`, `millis` | the last response to every request except `interrupt` |
//!
//! - Output is streamed by statement (decision 2.5): the `output` and `failure` responses of a top-level
//!   statement are written, in the order they happened, as soon as that statement finishes, before the next one
//!   starts; `print` output inside one long statement arrives when it finishes.
//! - `error`: `line` and `column` are 1-based (columns count characters), `null` with `origin` `null` when the
//!   error has no position (a limit, a bad request). `origin` is the file the error is in: the request's origin,
//!   or a module file it imported. `rendered` is the full text a terminal shows: `origin:line:column: message`,
//!   the source line and a caret, for every error found.
//! - `done.status`: `ok` (every statement ran, no failure), `failure` (every statement ran, some value was a
//!   failure), `error` (a read, resolve or load error: nothing ran; or a bad request), `interrupted`, `limit`
//!   (the evaluation depth or fuel limit; an `error` with the message `limit: ...` comes first). Requests other
//!   than `eval` end with `ok`. `millis` is the wall-clock time of the request.
//!
//! An example exchange (`>` client, `<` kernel):
//!
//! ```text
//! > {"type":"eval","id":1,"cell":"a","src":"print(\"hi\")\nx = 6\nx * 7"}
//! < {"id":1,"cell":"a","type":"output","text":"hi"}
//! < {"id":1,"cell":"a","type":"value","printed":"42"}
//! < {"id":1,"cell":"a","type":"done","status":"ok","millis":0}
//! > {"type":"eval","id":2,"src":"1 +"}
//! < {"id":2,"type":"error","message":"expected an expression","line":1,"column":4,"origin":"<input>","rendered":"<input>:1:4: ..."}
//! < {"id":2,"type":"done","status":"error","millis":0}
//! > {"type":"complete","id":3,"prefix":"fac"}
//! < {"id":3,"type":"completions","names":["factor","factor_integer","factorial"]}
//! < {"id":3,"type":"done","status":"ok","millis":0}
//! ```
//!
//! ## Notebook additions
//!
//! Optional request fields and new responses for notebook front ends (design 08-notebook 5.2 and 5.6). A client
//! that does not ask for them sees exactly the responses above. `hello` lists them in `capabilities` (a client
//! uses an addition only when it is listed; a kernel without the field has none):
//!
//! ```text
//! {"id":null,"type":"hello","protocol":1,"version":"0.0.0","engine":"mathcas-v2 0.0.0","capabilities":["values_each","numbered","history","display","display_svg","message","names","shutdown_interrupt"]}
//! ```
//!
//! | capability | request | effect |
//! |---|---|---|
//! | `values_each` | `eval` with `values: "each"` (default `"last"`) | a `value` for **every** top-level statement whose value is shown: not `null` (definitions and assignments are `null`), not followed by `;` on its line, not what its own last `print` line already shows (`print(x)` returns `x`); `output` is one response per `print` call, its `text` may contain line ends |
//! | `numbered` | `eval` with `numbered: true` | each top-level statement of `src` (an `import` or a `module` is one) takes the next number of the session's line counter (1 in a fresh session, after `reset` too) when it starts; `output`, `message`, `value`, `display`, `failure` carry the statement's number `n`; `done` carries `first` (the first number of the request) and `next` (the counter after it); a read error takes one number, carried by its `error` as `n`, as is a limit's; an interrupted statement keeps its number, so on `done` status `interrupted` the statement `next - 1` was running |
//! | `history` | (numbered evaluations) | `out(n)` gives the value of statement `n` (hidden values and failures included, an assignment `x = v` as the value v, `null` not kept), `out()` the previous statement's, `out(-k)` the k-th previous; a number without a value gives `failure(history, n)` |
//! | `display` | (`values: "each"`) | a value `image("path")` naming a readable PNG file is sent as `display` with `kind: "image"`, `path`, `width`, `height` (pixels) and `printed` (the value as text), instead of `value` |
//! | `display_svg` | (`values: "each"`) | a value `image("path")` naming an SVG file (by its `.svg`/`.svgz` extension, or its content) is sent as `display` with `kind: "svg"`, `width` and `height` its size in CSS pixels (from the root element's `width`/`height`, else its `viewBox`, else 300 x 150); clients that know only `kind: "image"` show `printed` |
//! | `message` | (`values: "each"`) | a warning of library code (`message(tag, text)`) is sent as `message` with `text` and `tag` (the function's name); without `values: "each"` it is an `output` line `tag: text`, as before |
//! | `names` | `names` with `prefix` (default `""`) | `names`: `[{name, kind}]` sorted by name, the names `complete` offers with `kind` `function`, `value`, `module` or `symbol`; then `done` |
//! | `shutdown_interrupt` | `shutdown` with `interrupt: true` | the running evaluation is interrupted and the queued ones are cancelled (`done` `interrupted`) before the shutdown's `done`: quitting a busy kernel |
//!
//! `numbered: true` without `values: "each"` numbers the statements and sends only the last statement's value
//! (when shown). An example exchange:
//!
//! ```text
//! > {"type":"eval","id":1,"cell":"c1","src":"1+1\n3+3;\nprint(\"hi\")","values":"each","numbered":true}
//! < {"id":1,"cell":"c1","type":"value","printed":"2","n":1}
//! < {"id":1,"cell":"c1","type":"output","text":"hi","n":3}
//! < {"id":1,"cell":"c1","type":"done","status":"ok","millis":0,"first":1,"next":4}
//! > {"type":"eval","id":2,"cell":"c2","src":"out(2) + 1\nplot(sin(t), [t, 0, 1])","values":"each","numbered":true}
//! < {"id":2,"cell":"c2","type":"value","printed":"7","n":4}
//! < {"id":2,"cell":"c2","type":"display","kind":"image","path":"/tmp/mathcas-plots/plot-1-0.png","width":640,"height":400,"printed":"image(\"/tmp/mathcas-plots/plot-1-0.png\")","n":5}
//! < {"id":2,"cell":"c2","type":"done","status":"ok","millis":9,"first":4,"next":6}
//! > {"type":"names","id":3,"prefix":"ou"}
//! < {"id":3,"type":"names","names":[{"name":"out","kind":"function"},{"name":"outer","kind":"function"}]}
//! < {"id":3,"type":"done","status":"ok","millis":0}
//! ```
//!
//! # The notebook format `.mcn`
//!
//! A notebook is a plain text file (UTF-8, LF or CR LF line ends, kept when rewritten) made of cells. Its first
//! line is `#! mathcas 2 notebook`. Each cell starts with a marker line:
//!
//! - `#% cell`: a code cell; the lines up to the next marker are MathCAS source.
//! - `#% text`: a text cell; its lines are prose, each written as a comment (`# ` and the text; `#` for an empty
//!   line).
//! - `#% output`: the output block of the code cell just above it, holding the output of its last run. Each line
//!   starts with a two-character prefix and a space: `#|` a line of `print` output, `#=` a line of the value of
//!   the last statement, `#!` a line of a failure, an error (with its position in the notebook) or the reason the
//!   run stopped (`interrupted`, `limit: ...`). Nothing else may appear in an output block.
//!
//! A marker is exactly one of those three lines (trailing spaces allowed); any other line starting with `#%` is an
//! ordinary comment. Blank lines after a cell belong to the layout and are kept. Source before the first marker
//! is read as a code cell. Every marker and every output or text line is a comment, so the file with its header
//! changed to `#! mathcas 2` is a MathCAS script whose statements are the code cells in order (decision 2.6).
//!
//! ```text
//! #! mathcas 2 notebook
//!
//! #% text
//! # Exact arithmetic.
//!
//! #% cell
//! print(2^10)
//! 1/3 + 1/6
//! #% output
//! #| 1024
//! #= 1/2
//! ```
//!
//! Printing a parsed notebook gives back the text that was read when it is in this layout (markers as written by
//! the runner, a final line end), so files stay diffable.
//!
//! ## Running a notebook
//!
//! `mc-repl notebook.mcn` evaluates the code cells in order in one session (imports next to the notebook) and
//! prints like a script: `print` output and each cell's value (unless `null`) to standard output, failures and
//! errors to standard error. `mc-repl --update notebook.mcn` also writes each cell's output block back into the
//! file (atomically, and only if it changed): a cell that shows nothing gets no block. As for scripts, a failure
//! does not stop the run (exit code 2), nor does a cell that does not read: it records the read error and the
//! cells after it run (exit code 1; gap G77). An interrupt (130) or a limit (2) stops the run, the cell where it
//! stopped records why, and the cells after it lose their output blocks, so the file records one run (decision
//! 2.7). The statements are numbered across the cells, so `out(n)`, `%`, `%%` and `%n` reach earlier results. Positions in the written output blocks are lines of the written file, so a second `--update`
//! leaves the file unchanged.
#![warn(missing_docs)]
pub mod cell;
pub mod notebook;
pub mod protocol;
pub mod server;

pub use cell::{run_cell, run_cell_numbered, run_each, CellEvent, EachEvent, ErrorReport, Status};
pub use notebook::Notebook;
pub use protocol::{Body, Request, Response, Values, CAPABILITIES, PROTOCOL};
