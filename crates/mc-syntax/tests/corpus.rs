//! The round-trip corpus: every statement parses, lowers, prints, and the printed text reads back to the identical
//! term; printing is then stable (printing the re-read term gives the same text).
use mc_api::RefStore;
use mc_syntax::{read, render_all, Printer};

const STATEMENTS: &str = include_str!("corpus/statements.mc");
const PROGRAMS: &str = include_str!("corpus/programs.mc");

/// Reads `src`, prints it, reads the print back; returns the printed text.
fn round_trip(src: &str) -> Result<String, String> {
    let store = RefStore::new();
    let terms = read(&store, src).map_err(|e| format!("reading {src:?}:\n{}", render_all(&e, src)))?;
    let p = Printer::new(&store);
    let printed = p.program(&terms);
    let again = read(&store, &printed).map_err(|e| format!("re-reading {printed:?} (from {src:?}):\n{}", render_all(&e, &printed)))?;
    if again != terms {
        return Err(format!("{src:?} printed as {printed:?}, which reads back as a different term {:?}", p.program(&again)));
    }
    let printed2 = p.program(&again);
    if printed2 != printed {
        return Err(format!("printing is not stable: {printed:?} then {printed2:?}"));
    }
    Ok(printed)
}

fn corpus_lines() -> Vec<&'static str> {
    STATEMENTS.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect()
}

#[test]
fn corpus_has_at_least_200_statements() {
    let n = corpus_lines().len();
    assert!(n >= 200, "only {n} statements");
}

#[test]
fn every_statement_round_trips() {
    let mut failures = Vec::new();
    for line in corpus_lines() {
        if let Err(e) = round_trip(line) {
            failures.push(e);
        }
    }
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn statement_file_round_trips_as_one_program() {
    round_trip(STATEMENTS).unwrap();
}

#[test]
fn programs_round_trip() {
    let printed = round_trip(PROGRAMS).unwrap();
    assert!(printed.lines().count() >= 25);
}
