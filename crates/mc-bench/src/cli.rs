//! The command line of a benchmark binary. A binary supplies its engine and calls [`main`]; the same executable
//! is the parent (`run`) and, re-invoked by the guard, the child (`child`).
//!
//! ```text
//! <bin> list    [--cases DIR]                     check and list the case files
//! <bin> run     [--cases DIR] [--results DIR] [--only ID,CLASS,...] [--mem-ceiling SIZE] [--grace DUR]
//!               [--machine NAME] [--reference FILE]
//! <bin> summary FILE [--reference FILE]           summarise a results file
//! <bin> child   CASE INDEX                        (internal) one measurement, started by the guard
//! ```
use crate::case::{parse_bytes, parse_duration, Case};
use crate::child::{self, ChildReport, Outcome};
use crate::engine::EngineFactory;
use crate::guard::{platform_guard, wait_for_release};
use crate::results::{machine_name, today, ResultsFile};
use crate::runner::{self, RunConfig};
use crate::summary::summarize;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

/// The memory ceiling for one case on the shared development machine (the protocol's 8 GB is lowered to it).
const DEFAULT_CEILING: &str = "4GB";
const DEFAULT_GRACE: Duration = Duration::from_secs(5);

/// Run the command line in `args` (without the program name) with engines from `factory`.
pub fn main(args: Vec<String>, factory: &EngineFactory) -> ExitCode {
    match dispatch(args, factory) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}

fn dispatch(args: Vec<String>, factory: &EngineFactory) -> Result<ExitCode, String> {
    let mut args = args.into_iter();
    let command = args.next().unwrap_or_default();
    let rest: Vec<String> = args.collect();
    match command.as_str() {
        "child" => child_command(&rest, factory),
        "list" => list_command(&Options::parse(&rest)?),
        "run" => run_command(&Options::parse(&rest)?, factory),
        "summary" => summary_command(&Options::parse(&rest)?),
        "" | "help" | "--help" => {
            println!("{}", USAGE);
            Ok(ExitCode::SUCCESS)
        }
        other => Err(format!("unknown command {other}\n{USAGE}")),
    }
}

const USAGE: &str = "usage: list [--cases DIR] | run [--cases DIR] [--results DIR] [--only IDS] [--mem-ceiling SIZE] \
[--grace DUR] [--machine NAME] [--reference FILE] | summary FILE [--reference FILE]";

#[derive(Default)]
struct Options {
    positional: Vec<String>,
    cases: Option<String>,
    results: Option<String>,
    only: Vec<String>,
    mem_ceiling: Option<String>,
    grace: Option<String>,
    machine: Option<String>,
    reference: Option<String>,
}

impl Options {
    fn parse(args: &[String]) -> Result<Options, String> {
        let mut o = Options::default();
        let mut it = args.iter();
        while let Some(a) = it.next() {
            let mut value = || it.next().cloned().ok_or_else(|| format!("{a} needs a value"));
            match a.as_str() {
                "--cases" => o.cases = Some(value()?),
                "--results" => o.results = Some(value()?),
                "--only" => {
                    o.only = value()?.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
                }
                "--mem-ceiling" => o.mem_ceiling = Some(value()?),
                "--grace" => o.grace = Some(value()?),
                "--machine" => o.machine = Some(value()?),
                "--reference" => o.reference = Some(value()?),
                s if s.starts_with("--") => return Err(format!("unknown option {s}")),
                _ => o.positional.push(a.clone()),
            }
        }
        Ok(o)
    }

    fn cases_dir(&self) -> PathBuf {
        PathBuf::from(self.cases.as_deref().unwrap_or("bench/cases"))
    }
}

fn load_cases(dir: &Path) -> Result<Vec<Case>, String> {
    runner::discover(dir).map_err(|errors| errors.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n"))
}

fn list_command(o: &Options) -> Result<ExitCode, String> {
    let cases = load_cases(&o.cases_dir())?;
    for c in &cases {
        let sizes = if c.sizes.is_empty() { "-".to_string() } else { c.sizes.join(", ") };
        println!("{}/{:<6} {:<12} sizes {:<24} {}", c.class, c.id, c.checker.to_string(), sizes, c.title);
    }
    println!("{} cases, {} measurements", cases.len(), cases.iter().map(Case::measurement_count).sum::<usize>());
    Ok(ExitCode::SUCCESS)
}

fn run_command(o: &Options, factory: &EngineFactory) -> Result<ExitCode, String> {
    let cases = load_cases(&o.cases_dir())?;
    let exe = std::env::current_exe().map_err(|e| format!("cannot find this program: {e}"))?;
    let config = RunConfig {
        cases_dir: o.cases_dir(),
        results_dir: PathBuf::from(o.results.as_deref().unwrap_or("bench/results")),
        only: o.only.clone(),
        child: (exe, vec![OsString::from("child")]),
        memory_ceiling: parse_bytes(o.mem_ceiling.as_deref().unwrap_or(DEFAULT_CEILING)).ok_or("bad --mem-ceiling")?,
        grace: match &o.grace {
            Some(g) => parse_duration(g).ok_or("bad --grace")?,
            None => DEFAULT_GRACE,
        },
        system: factory().name(),
        machine: o.machine.clone().unwrap_or_else(machine_name),
        date: today(),
    };
    let guard = platform_guard();
    let mut out = io::stdout().lock();
    let output = runner::run(&config, &cases, guard.as_ref(), &mut out).map_err(|e| e.to_string())?;
    let reference = read_reference(o)?;
    let summary = summarize(&output.rows, reference.as_ref().map(|r| r.rows.as_slice()));
    let _ = writeln!(
        out,
        "\nresults: {}\ntrace: {}\n\n{}",
        output.results_path.display(),
        output.trace_path.display(),
        summary.text
    );
    Ok(ExitCode::SUCCESS)
}

fn read_reference(o: &Options) -> Result<Option<ResultsFile>, String> {
    o.reference.as_ref().map(|p| ResultsFile::read(Path::new(p)).map_err(|e| e.to_string())).transpose()
}

fn summary_command(o: &Options) -> Result<ExitCode, String> {
    let [file] = o.positional.as_slice() else { return Err("summary needs one results file".into()) };
    let results = ResultsFile::read(Path::new(file)).map_err(|e| e.to_string())?;
    let reference = read_reference(o)?;
    print!("{}", summarize(&results.rows, reference.as_ref().map(|r| r.rows.as_slice())).text);
    Ok(ExitCode::SUCCESS)
}

/// The child: wait until the guard has contained this process, run one measurement, print the result line.
fn child_command(args: &[String], factory: &EngineFactory) -> Result<ExitCode, String> {
    let [path, index] = args else { return Err("child needs CASE INDEX".into()) };
    wait_for_release().map_err(|e| format!("cannot read the release line: {e}"))?;
    let index: usize = index.parse().map_err(|_| format!("bad index {index}"))?;
    let report = match Case::load(Path::new(path)) {
        Ok(case) if index < case.measurement_count() => child::run(&case, index, factory),
        Ok(_) => return Err(format!("index {index} out of range")),
        Err(e) => ChildReport {
            engine: factory().name(),
            outcome: Outcome::Error(e.to_string()),
            times: Vec::new(),
            peak_bytes: None,
            check: None,
            note: String::new(),
        },
    };
    let mut out = io::stdout().lock();
    writeln!(out, "{}", report.encode()).and_then(|()| out.flush()).map_err(|e| e.to_string())?;
    Ok(ExitCode::SUCCESS)
}
