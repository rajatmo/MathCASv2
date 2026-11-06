//! The `.mcn` notebook format and its runner. The format itself (reading, writing, cells, output blocks,
//! [`write_atomic`]) lives in `mc-nbfile` and is re-exported here; this module adds [`run`], which evaluates the code
//! cells in one session and can rewrite their output blocks.
use crate::cell::{run_cell_numbered, CellEvent, Status};
use mc_bench::StopFlag;
use mc_engine::{CoreEngine, Origin};

pub use mc_nbfile::{
    parse_cells, print_cells, write_atomic, Attr, Attrs, Cell, Kind, Notebook, OutputBlock, OutputKind, OutputLine,
    ParseError, CELL, FRAGMENT, HEADER, OUTPUT, TEXT,
};

/// The output block of a run: its events, then a line for a stop.
fn output_block(events: &[CellEvent], status: Status) -> Option<Vec<OutputLine>> {
    let mut out = vec![];
    let mut push = |kind, text: &str| out.extend(text.split('\n').map(|t| OutputLine { kind, text: t.to_string() }));
    for e in events {
        match e {
            CellEvent::Output(s) => push(OutputKind::Print, s),
            CellEvent::Value(s) => push(OutputKind::Value, s),
            CellEvent::Failure(s) => push(OutputKind::Message, s),
            CellEvent::Error(r) => push(OutputKind::Message, &r.rendered),
        }
    }
    if status == Status::Interrupted {
        push(OutputKind::Message, "interrupted");
    }
    (!out.is_empty()).then_some(out)
}

/// Evaluate the code cells of `nb` in order in `engine`, every cell from `origin` (the notebook file: imports are
/// looked up next to it) with its errors at their lines in the notebook, handing each cell's events to `show` as
/// they happen. Returns how the run ended: the status of the cell where it stopped (`interrupted`, `limit`), else
/// `error` if some cell did not read (the cells after it still run), else `failure` if some cell had a failure,
/// else `ok`.
///
/// With `rewrite`, every cell that ran gets the output block of this run (none if it showed nothing), and the
/// cells after a stop lose theirs, so the notebook records one run. Error positions are those of the notebook as
/// rewritten, which is the file [`write_atomic`] then writes; without `rewrite` the notebook is left as it is and
/// positions are those of the file as read.
pub fn run(
    engine: &mut CoreEngine,
    nb: &mut Notebook,
    origin: &Origin,
    stop: &StopFlag,
    rewrite: bool,
    show: &mut dyn FnMut(&CellEvent),
) -> Status {
    let mut overall = Status::Ok;
    for i in 0..nb.cells.len() {
        let Some(src) = nb.cells[i].source() else { continue };
        // earlier cells' output blocks are final by now, so this is the cell's line in the file to be written
        let line = nb.first_lines()[i];
        let mut events = vec![];
        let status = run_cell_numbered(engine, &src, origin, line, stop, &mut |e| {
            show(&e);
            events.push(e);
        });
        if rewrite {
            nb.cells[i].set_output(output_block(&events, status));
        }
        match status {
            Status::Ok => {}
            Status::Failure if overall == Status::Ok => overall = Status::Failure,
            Status::Failure => {}
            // a cell that does not read (a syntax error) is reported in its output; the following cells still run
            // (gap G77)
            Status::Error => overall = Status::Error,
            Status::Interrupted | Status::Limit => {
                if rewrite {
                    for c in nb.cells[i + 1..].iter_mut().filter(|c| c.is_code()) {
                        c.set_output(None);
                    }
                }
                return status;
            }
        }
    }
    overall
}
