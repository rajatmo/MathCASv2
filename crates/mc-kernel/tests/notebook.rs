//! The notebook runner on a real engine: the sample notebook of `examples/` round-trips and is up to date, its
//! text read as a script gives the same results, rewriting is idempotent, error positions are notebook lines, and
//! a stop records itself and clears the outputs after it.
use mc_bench::StopFlag;
use mc_engine::{CoreEngine, EngineConfig, Event, Origin};
use mc_kernel::notebook::{self, Cell, Notebook, OutputKind, OutputLine};

/// The output block of a cell, if it has one.
fn output(c: &Cell) -> Option<Vec<OutputLine>> {
    let lines: Vec<OutputLine> = c.output_lines().cloned().collect();
    (!lines.is_empty()).then_some(lines)
}
use mc_kernel::{CellEvent, Status};
use std::path::{Path, PathBuf};

fn examples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

fn engine() -> CoreEngine {
    CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() })
}

/// Run `nb` from a file in `dir`, rewriting it; returns the status and the events.
fn run(nb: &mut Notebook, dir: &Path, stop: &StopFlag) -> (Status, Vec<CellEvent>) {
    let mut events = vec![];
    let origin = Origin::file(&dir.join("t.mcn"));
    let status = notebook::run(&mut engine(), nb, &origin, stop, true, &mut |e| events.push(e.clone()));
    (status, events)
}

#[test]
fn the_sample_notebook_is_up_to_date() {
    let text = std::fs::read_to_string(examples().join("notebook.mcn")).unwrap();
    let nb = Notebook::parse(&text).unwrap();
    assert_eq!(nb.print(), text, "the sample is in the canonical layout");
    let mut again = nb.clone();
    let (status, events) = run(&mut again, &examples(), &StopFlag::new());
    assert_eq!(status, Status::Failure, "the sample shows a failure");
    assert_eq!(again.print(), text, "a run gives the outputs recorded in the sample");
    // every kind of output line occurs
    let kinds: Vec<OutputKind> = nb.cells.iter().filter_map(output).flatten().map(|l| l.kind).collect();
    for k in [OutputKind::Print, OutputKind::Value, OutputKind::Message] {
        assert!(kinds.contains(&k), "{k:?}");
    }
    assert!(events.contains(&CellEvent::Failure("failure(division_by_zero)".into())));
}

#[test]
fn a_notebook_reads_as_a_script() {
    let text = std::fs::read_to_string(examples().join("notebook.mcn")).unwrap();
    let nb = Notebook::parse(&text).unwrap();
    // the header of a script instead of a notebook's: every marker and output line is a comment
    let script = text.replacen(notebook::HEADER, "#! mathcas 2", 1);
    let mut e = engine();
    let origin = Origin::new("script", Some(examples()));
    let r = e.eval_in(&script, &origin, &StopFlag::new()).unwrap();
    let printed: Vec<String> = r
        .events
        .iter()
        .map(|e| match e {
            Event::Output(s) | Event::Failure(s) => s.clone(),
        })
        .collect();
    // the script's print output and failures are the notebook's #| and #! lines, in order
    let recorded: Vec<String> = nb
        .cells
        .iter()
        .filter_map(output)
        .flatten()
        .filter(|l| l.kind != OutputKind::Value)
        .map(|l| l.text.clone())
        .collect();
    assert_eq!(printed, recorded);
    // and its value is the last cell's value
    let last = nb.cells.iter().filter_map(output).next_back().unwrap();
    assert_eq!(r.last.printed, last.last().unwrap().text);
}

#[test]
fn rewriting_is_idempotent_and_positions_are_notebook_lines() {
    let dir = std::env::temp_dir();
    let src = "#! mathcas 2 notebook\n\n#% cell\nprint(\"a\")\nprint(\"b\")\nx = 2\n\n#% cell\nx + 1\n\n\
               #% cell\nx +\n\n#% cell\n5\n#% output\n#= stale\n";
    let mut nb = Notebook::parse(src).unwrap();
    let (status, events) = run(&mut nb, &dir, &StopFlag::new());
    assert_eq!(status, Status::Error);
    let CellEvent::Error(err) = events.last().unwrap() else { panic!("{events:?}") };
    let written = nb.print();
    // the error points at the end of `x +` in the file as written
    let lines: Vec<&str> = written.lines().collect();
    let line = err.line.unwrap() as usize;
    assert_eq!(lines[line - 1], "x +", "{written}");
    assert_eq!(err.column, Some(4));
    assert!(err.rendered.contains("t.mcn:"), "{}", err.rendered);
    // the cell after the error lost its stale output; the cells before got theirs
    assert_eq!(output(&nb.cells[3]), None);
    assert_eq!(output(&nb.cells[0]).unwrap().len(), 2);
    assert!(written.contains("#% cell\nx + 1\n#% output\n#= 3\n\n"), "{written}");
    // a second run changes nothing
    let mut again = Notebook::parse(&written).unwrap();
    run(&mut again, &dir, &StopFlag::new());
    assert_eq!(again.print(), written);
    // nor does fixing the error and running twice
    let fixed = written.replace("\nx +\n", "\nx + 10\n");
    let mut nb = Notebook::parse(&fixed).unwrap();
    assert_eq!(run(&mut nb, &dir, &StopFlag::new()).0, Status::Ok);
    let once = nb.print();
    assert!(once.ends_with("#% cell\nx + 10\n#% output\n#= 12\n\n#% cell\n5\n#% output\n#= 5\n"), "{once}");
    run(&mut nb, &dir, &StopFlag::new());
    assert_eq!(nb.print(), once);
}

#[test]
fn a_stop_is_recorded_and_clears_later_outputs() {
    let src = "#! mathcas 2 notebook\n#% text\n# t\n#% cell\n1\n#% output\n#= 1\n#% cell\n2\n#% output\n#= 2\n";
    let mut nb = Notebook::parse(src).unwrap();
    let stop = StopFlag::new();
    stop.raise();
    let (status, events) = run(&mut nb, &std::env::temp_dir(), &stop);
    assert_eq!((status, events.len()), (Status::Interrupted, 0));
    let out = output(&nb.cells[1]).expect("an output block");
    assert_eq!((out[0].kind, out[0].text.as_str()), (OutputKind::Message, "interrupted"));
    assert_eq!(output(&nb.cells[2]), None);
    assert_eq!(nb.cells[0].prose().unwrap(), "t");
}

#[test]
fn without_rewriting_the_notebook_is_left_alone() {
    let src = "#! mathcas 2 notebook\n#% cell\nprint(7)\n";
    let mut nb = Notebook::parse(src).unwrap();
    let mut shown = vec![];
    let origin = Origin::file(Path::new("t.mcn"));
    let status =
        notebook::run(&mut engine(), &mut nb, &origin, &StopFlag::new(), false, &mut |e| shown.push(e.clone()));
    assert_eq!(status, Status::Ok);
    assert_eq!(shown[0], CellEvent::Output("7".into()));
    assert_eq!(nb.print(), src);
}
