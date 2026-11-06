//! The `.mcn` notebook format and its runner. The format itself (reading, writing, cells, output blocks,
//! [`write_atomic`]) lives in `mc-nbfile` and is re-exported here; this module adds [`run`], which evaluates the code
//! cells in one session and can rewrite their output blocks.
use crate::cell::{run_each, CellEvent, EachEvent, Status};
use mc_bench::StopFlag;
use mc_engine::{CoreEngine, EachOptions, Origin, Report};

pub use mc_nbfile::{
    parse_cells, print_cells, write_atomic, Attr, Attrs, Cell, Kind, Notebook, OutputBlock, OutputKind, OutputLine,
    ParseError, CELL, FRAGMENT, HEADER, OUTPUT, TEXT,
};

/// One reported thing of a cell's run.
enum Line {
    /// an output line: print output, a failure, a warning, an error
    Out(OutputKind, String),
    /// a statement's shown value (with the statement's number), which closes its statement's block
    Value(Option<u64>, String),
}

/// The output blocks of a run (gap G91): one per shown statement value. A cell that shows at most one value gets
/// one block without attributes, its lines in order (the format before G91, so such notebooks are written as
/// before); with several values each value's block carries its statement's number (`n=N`) and holds the print
/// output and warnings since the previous value; what follows the last value (and the stop line) goes in a block
/// of its own. Reading keeps both layouts.
fn output_blocks(lines: &[Line], status: Status) -> Vec<OutputBlock> {
    let values = lines.iter().filter(|l| matches!(l, Line::Value(..))).count();
    let mut blocks = vec![];
    let mut block = OutputBlock::default();
    let push = |b: &mut OutputBlock, kind, text: &str| {
        b.lines.extend(text.split('\n').map(|t| OutputLine { kind, text: t.to_string() }))
    };
    for l in lines {
        match l {
            Line::Out(kind, text) => push(&mut block, *kind, text),
            Line::Value(n, text) => {
                push(&mut block, OutputKind::Value, text);
                if values > 1 {
                    block.attrs.set("n", n.map(|n| n.to_string()));
                    blocks.push(std::mem::take(&mut block));
                }
            }
        }
    }
    if status == Status::Interrupted {
        push(&mut block, OutputKind::Message, "interrupted");
    }
    if !block.lines.is_empty() {
        blocks.push(block);
    }
    blocks
}

/// Evaluate the code cells of `nb` in order in `engine`, every cell from `origin` (the notebook file: imports are
/// looked up next to it) with its errors at their lines in the notebook, handing each cell's events (the value of
/// every statement whose value is shown, gap G91) to `show` as they happen. Returns how the run ended: the status of the cell where it stopped (`interrupted`, `limit`), else
/// `error` if some cell did not read (the cells after it still run), else `failure` if some cell had a failure,
/// else `ok`.
///
/// With `rewrite`, every cell that ran gets the output blocks of this run (none if it showed nothing), and the
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
        let mut lines = vec![];
        let options = EachOptions { numbered: true };
        let status = run_each(engine, &src, origin, line, stop, options, &mut |e| {
            let (event, line) = match e {
                EachEvent::Report(Report::Output { text, .. }) => {
                    (CellEvent::Output(text.clone()), Line::Out(OutputKind::Print, text))
                }
                EachEvent::Report(Report::Message { tag, text, .. }) => {
                    let text = format!("{tag}: {text}");
                    (CellEvent::Output(text.clone()), Line::Out(OutputKind::Note, text))
                }
                EachEvent::Report(Report::Value { n, printed, .. }) => {
                    (CellEvent::Value(printed.clone()), Line::Value(n, printed))
                }
                EachEvent::Report(Report::Failure { printed, .. }) => {
                    (CellEvent::Failure(printed.clone()), Line::Out(OutputKind::Message, printed))
                }
                EachEvent::Error(r) => {
                    let line = Line::Out(OutputKind::Message, r.rendered.clone());
                    (CellEvent::Error(r), line)
                }
            };
            show(&event);
            lines.push(line);
        });
        if rewrite {
            nb.cells[i].outputs = output_blocks(&lines, status);
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
