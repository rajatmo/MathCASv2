//! `mc-docs-pages`: the documentation pages from the command line.
//!
//! - `mc-docs-pages update [name ...]` runs the pages (all of them without names) through the engine and writes
//!   their outputs and images back;
//! - `mc-docs-pages check [name ...]` validates them and compares a fresh run with the stored outputs (what the
//!   test `tests/pages.rs` does), exit code 1 on any problem;
//! - `mc-docs-pages missing` lists the catalogued functions without a page, by area and topic;
//! - `--dir <folder>` names the pages folder (default: `docs/functions` of the repository this was built from).
use mc_docs::catalogue::{Area, Catalogue};
use mc_docs::library::{source_dir, Library};
use mc_docs::runner;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(cmd) = args.next() else {
        eprintln!("usage: mc-docs-pages update|check|missing [--dir <folder>] [name ...]");
        return ExitCode::from(2);
    };
    let mut dir = source_dir();
    let mut names = vec![];
    while let Some(a) = args.next() {
        if a == "--dir" {
            dir = args.next().map(PathBuf::from).unwrap_or(dir);
        } else {
            names.push(a);
        }
    }
    let catalogue = Catalogue::new();
    if cmd == "missing" {
        // the catalogued functions without a page, by area and topic, with their one-line descriptions
        let pages = Library::from_dir(&dir).names();
        let mut count = 0;
        for area in Area::ALL {
            for (topic, entries) in catalogue.topics(area) {
                let without: Vec<_> = entries.iter().filter(|e| !pages.iter().any(|p| p == e.name)).collect();
                if !without.is_empty() {
                    println!("## {} / {topic}: {} of {} without a page", area.title(), without.len(), entries.len());
                }
                for e in without {
                    println!("{}\t{}", e.name, e.summary);
                    count += 1;
                }
            }
        }
        eprintln!("{count} of {} functions without a page", catalogue.entries.len());
        return ExitCode::SUCCESS;
    }
    if names.is_empty() {
        names = Library::from_dir(&dir).names();
    }
    let total = names.len();
    let mut bad = 0;
    let known = |n: &str| catalogue.get(n).is_some();
    for (k, name) in names.iter().enumerate() {
        let head = format!("[{}/{total} {:.0}%] {name}", k + 1, 100.0 * (k + 1) as f64 / total as f64);
        let mut progress = |i: usize, n: usize| eprint!("\r{head}: input {i}/{n}   ");
        match cmd.as_str() {
            "update" => match runner::update(name, &dir, &mut progress) {
                Ok(u) => eprintln!(
                    "\r{head}: {} inputs, {} images ({} removed), {}, slowest {:.2} s",
                    u.inputs,
                    u.images,
                    u.removed,
                    if u.changed { "changed" } else { "unchanged" },
                    u.slowest
                ),
                Err(e) => {
                    bad += 1;
                    eprintln!("\r{head}: {e}");
                }
            },
            "check" => {
                let c = runner::check(name, &dir, &known, &mut progress);
                eprintln!("\r{head}: {} inputs, slowest {:.2} s, {} problems", c.inputs, c.slowest, c.problems.len());
                for p in &c.problems {
                    eprintln!("  {name}.mcn: {p}");
                }
                bad += usize::from(!c.problems.is_empty());
            }
            other => {
                eprintln!("unknown command `{other}` (update, check)");
                return ExitCode::from(2);
            }
        }
    }
    if bad == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
