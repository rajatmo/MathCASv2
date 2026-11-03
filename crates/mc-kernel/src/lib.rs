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
//! | `output` | `text` | a line of `print` output |
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
//! does not stop the run (exit code 2); a read error (1), an interrupt (130) or a limit (2) stops it, the cell
//! where it stopped records why, and the cells after it lose their output blocks, so the file records one run
//! (decision 2.7). Positions in the written output blocks are lines of the written file, so a second `--update`
//! leaves the file unchanged.
#![warn(missing_docs)]
pub mod cell;
pub mod notebook;
pub mod protocol;
pub mod server;

pub use cell::{run_cell, CellEvent, ErrorReport, Status};
pub use notebook::Notebook;
pub use protocol::{Body, Request, Response, PROTOCOL};
