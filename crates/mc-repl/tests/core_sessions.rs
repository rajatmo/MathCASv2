//! The REPL with the real engine, on the transcripts in `tests/sessions/`: `core.txt` (the core), `language.txt`
//! (closures, blocks, loops, `match`, `rewrite`, definitions, output) and `modules.txt` (`:load`, `import`,
//! `:modules`, the prelude and the standard module).
use mc_bench::StopFlag;
use mc_engine::CoreEngine;
use mc_repl::Repl;

/// One input of a transcript (its lines) and the output expected for it.
struct Entry {
    input: Vec<String>,
    output: Vec<String>,
}

fn parse(transcript: &str) -> Vec<Entry> {
    let mut entries: Vec<Entry> = vec![];
    for line in transcript.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if let Some(input) = line.strip_prefix("mc> ") {
            entries.push(Entry { input: vec![input.to_string()], output: vec![] });
            continue;
        }
        let e = entries.last_mut().expect("the transcript starts with an input");
        match line.strip_prefix("..> ") {
            Some(more) if e.output.is_empty() => e.input.push(more.to_string()),
            _ => e.output.push(line.to_string()),
        }
    }
    entries
}

/// Run a transcript through the REPL and compare its output line by line.
fn run(transcript: &str) {
    let entries = parse(transcript);
    assert!(entries.len() > 20);
    let input: String = entries.iter().flat_map(|e| e.input.iter()).map(|l| format!("{l}\n")).collect();
    let mut repl = Repl::with_host(Box::new(CoreEngine::new()), StopFlag::new());
    let mut out = vec![];
    repl.run(&mut input.as_bytes(), &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    let mut want = String::from("mathcas-v2 0.0.0 (:help for commands)\n");
    for e in &entries {
        // the prompt, a continuation prompt per further input line, then the output
        want.push_str("mc> ");
        want.push_str(&"..> ".repeat(e.input.len() - 1));
        for l in &e.output {
            want.push_str(l);
            want.push('\n');
        }
    }
    want.push_str("mc> \n");
    for (i, (got, want)) in out.lines().zip(want.lines()).enumerate() {
        assert_eq!(got, want, "line {}", i + 1);
    }
    assert_eq!(out, want);
}

#[test]
fn the_core_session_runs_end_to_end() {
    run(include_str!("sessions/core.txt"));
}

#[test]
fn the_language_session_runs_end_to_end() {
    run(include_str!("sessions/language.txt"));
}

#[test]
fn the_modules_session_runs_end_to_end() {
    run(include_str!("sessions/modules.txt"));
}
