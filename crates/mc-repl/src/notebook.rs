//! `mc-repl notebook.mcn` and `mc-repl --update notebook.mcn`: the headless notebook runner on `mc-kernel`'s
//! format and runner (the format is specified in the `mc_kernel` crate documentation).
//!
//! The cells print like a script: `print` output and each cell's value to standard output, failures (as
//! `notebook.mcn: failure(...)`) and errors (with their positions in the notebook) to standard error.
use crate::cli::{FAILED, INTERRUPTED, OK, READ_ERROR};
use mc_bench::StopFlag;
use mc_engine::{CoreEngine, Origin};
use mc_kernel::notebook::{self, Notebook};
use mc_kernel::{CellEvent, Status};
use std::io::Write;
use std::path::Path;

/// Run the notebook at `path` in `engine`; with `update`, write the outputs back into the file. Returns the exit
/// code so far and whether the run may go on with the next item (not after a read error, a limit or an
/// interrupt).
pub fn run(
    engine: &mut CoreEngine,
    path: &Path,
    update: bool,
    stop: &StopFlag,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> (u8, bool) {
    let name = path.display().to_string();
    // a write error (a closed pipe) is not worth reporting: the output has nowhere to go
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            let _ = writeln!(err, "{name}: cannot read the file: {e}");
            return (READ_ERROR, false);
        }
    };
    let mut nb = match Notebook::parse(&text) {
        Ok(nb) => nb,
        Err(e) => {
            let _ = writeln!(err, "{name}:{}: {}", e.line, e.message);
            return (READ_ERROR, false);
        }
    };
    let status = notebook::run(engine, &mut nb, &Origin::file(path), stop, update, &mut |e| {
        let _ = match e {
            CellEvent::Output(s) | CellEvent::Value(s) => writeln!(out, "{s}"),
            CellEvent::Failure(f) => writeln!(err, "{name}: {f}"),
            CellEvent::Error(r) => writeln!(err, "{}", r.rendered),
        };
    });
    if status == Status::Interrupted {
        let _ = writeln!(err, "interrupted");
    }
    let _ = out.flush();
    if update {
        if let Err(e) = notebook::write_atomic(path, &nb.print()) {
            let _ = writeln!(err, "{name}: cannot write the file: {e}");
            return (READ_ERROR, false);
        }
    }
    match status {
        Status::Ok => (OK, true),
        Status::Failure => (FAILED, true),
        Status::Error => (READ_ERROR, false),
        Status::Interrupted => (INTERRUPTED, false),
        Status::Limit => (FAILED, false),
    }
}
