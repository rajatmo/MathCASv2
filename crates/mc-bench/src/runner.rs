//! The parent side: find the cases, run every measurement in a guarded child process, print progress, and write
//! results and a trace (02-benchmarks sections 1 and 7).
use crate::case::{Case, CaseError};
use crate::child::{ChildReport, Outcome};
use crate::guard::{Exit, Guard, GuardLimits};
use crate::results::{machine_header, ResultsFile, Row, Stats, Status, KILLED};
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// The extension of case files.
pub const CASE_EXTENSION: &str = "mc";

/// Everything a run needs besides the guard.
#[derive(Clone, Debug)]
pub struct RunConfig {
    /// The directory holding `<class>/<id>.mc`.
    pub cases_dir: PathBuf,
    /// Results go to `<results_dir>/<machine>/<date>.tsv`, the trace next to them.
    pub results_dir: PathBuf,
    /// Case ids or class names to run; empty runs everything.
    pub only: Vec<String>,
    /// The program and leading arguments that run one measurement: the runner appends `<case file> <index>`.
    pub child: (PathBuf, Vec<OsString>),
    /// Memory caps of cases are lowered to this (the shared machine allows 4 GB per process).
    pub memory_ceiling: u64,
    /// Time the guard allows past a case's time limit for startup, setup and the check before it kills the child.
    pub grace: Duration,
    /// The system name for rows whose child reported nothing (killed or crashed); normally the engine's name.
    pub system: String,
    pub machine: String,
    pub date: String,
}

/// What a run produced.
#[derive(Clone, Debug)]
pub struct RunOutput {
    pub rows: Vec<Row>,
    pub results_path: PathBuf,
    pub trace_path: PathBuf,
}

/// Find and parse every case file under `dir`, sorted by class and then by id in natural order (R2 before R10).
/// All malformed files are reported together.
pub fn discover(dir: &Path) -> Result<Vec<Case>, Vec<CaseError>> {
    let mut cases = Vec::new();
    let mut errors = Vec::new();
    let unreadable = |e: io::Error| vec![CaseError { path: dir.to_path_buf(), line: 0, message: e.to_string() }];
    let mut classes: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(unreadable)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    classes.sort();
    for class in classes {
        let entries = match fs::read_dir(&class) {
            Ok(entries) => entries,
            Err(e) => {
                errors.push(CaseError { path: class.clone(), line: 0, message: e.to_string() });
                continue;
            }
        };
        for path in entries.filter_map(|e| e.ok().map(|e| e.path())) {
            if path.extension().is_some_and(|x| x == CASE_EXTENSION) {
                match Case::load(&path) {
                    Ok(c) => cases.push(c),
                    Err(e) => errors.push(e),
                }
            }
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    cases.sort_by(|a, b| (a.class.as_str(), natural_key(&a.id)).cmp(&(b.class.as_str(), natural_key(&b.id))));
    Ok(cases)
}

/// Letters, then the number they are followed by, then the rest: `R10` sorts after `R2`.
fn natural_key(id: &str) -> (String, u64, String) {
    let letters: String = id.chars().take_while(|c| !c.is_ascii_digit()).collect();
    let rest = &id[letters.len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    (letters, digits.parse().unwrap_or(0), rest[digits.len()..].to_string())
}

fn selected(case: &Case, only: &[String]) -> bool {
    only.is_empty() || only.iter().any(|o| *o == case.id || *o == case.class)
}

/// Run the selected cases under `guard`, printing `[k/N pct%]` progress lines to `progress`, and merge the rows
/// into the results file of this machine and date.
pub fn run(config: &RunConfig, cases: &[Case], guard: &dyn Guard, progress: &mut dyn Write) -> io::Result<RunOutput> {
    let chosen: Vec<&Case> = cases.iter().filter(|c| selected(c, &config.only)).collect();
    let total: usize = chosen.iter().map(|c| c.measurement_count()).sum();
    let results_path = ResultsFile::path_for(&config.results_dir, &config.machine, &config.date);
    let trace_path = results_path.with_extension("trace");
    fs::create_dir_all(results_path.parent().unwrap_or(Path::new(".")))?;
    let mut trace = Trace::open(&trace_path)?;
    trace.event("-", "-", "run", &format!("{total} measurements, guard {}", guard.name()))?;
    if !guard.enforces_memory() {
        writeln!(progress, "note: guard '{}' does not enforce memory caps on this platform", guard.name())?;
    }
    let mut rows = Vec::new();
    let mut k = 0;
    for case in chosen {
        for index in 0..case.measurement_count() {
            k += 1;
            let size = case.size_label(index);
            trace.event(&case.id, size, "start", "")?;
            let row = measure(config, case, index, guard)?;
            let detail = if row.detail.is_empty() { String::new() } else { format!(" ({})", row.detail) };
            trace.event(&case.id, size, "end", &format!("{} {}{detail}", row.status, describe_time(&row)))?;
            writeln!(
                progress,
                "[{k}/{total} {}%] {}/{} {}: {} {}, check {}{detail}",
                k * 100 / total.max(1),
                case.class,
                case.id,
                size_text(case, index),
                row.status,
                describe_time(&row),
                row.check,
            )?;
            rows.push(row);
        }
    }
    let mut file = if results_path.exists() { ResultsFile::read(&results_path)? } else { ResultsFile::default() };
    file.header = machine_header();
    file.header.push(("date".into(), config.date.clone()));
    file.header.push(("guard".into(), guard.name().into()));
    file.merge(rows.clone());
    file.write(&results_path)?;
    // Only the file name: traces live in the repository, which must not hold absolute paths.
    let name = results_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    trace.event("-", "-", "done", &name)?;
    Ok(RunOutput { rows, results_path, trace_path })
}

fn size_text(case: &Case, index: usize) -> String {
    match &case.variable {
        Some(v) => format!("{v} = {}", case.size_label(index)),
        None => case.size_label(index).to_string(),
    }
}

fn describe_time(row: &Row) -> String {
    match &row.stats {
        Some(s) => format!("median {} ({} reps)", seconds(s.median), row.reps),
        None => "no time".into(),
    }
}

/// A time with a readable unit.
pub fn seconds(s: f64) -> String {
    if s >= 1.0 {
        format!("{s:.3} s")
    } else if s >= 1e-3 {
        format!("{:.3} ms", s * 1e3)
    } else {
        format!("{:.3} us", s * 1e6)
    }
}

/// Run one measurement in a guarded child and turn what happened into a row.
fn measure(config: &RunConfig, case: &Case, index: usize, guard: &dyn Guard) -> io::Result<Row> {
    let (exe, args) = &config.child;
    let mut cmd = Command::new(exe);
    cmd.args(args).arg(&case.path).arg(index.to_string());
    let limits = GuardLimits {
        memory_bytes: case.limits.memory_bytes.min(config.memory_ceiling),
        timeout: case.limits.time + config.grace,
    };
    let report = guard.run(&mut cmd, &limits)?;
    let mut row = Row {
        system: config.system.clone(),
        class: case.class.clone(),
        case: case.id.clone(),
        size: case.size_label(index).to_string(),
        stats: None,
        reps: 0,
        peak_bytes: None,
        proc_peak_bytes: report.peak_bytes,
        status: Status::Crash,
        check: "-".into(),
        detail: String::new(),
    };
    let child = ChildReport::decode(&report.stdout);
    if let Some(c) = &child {
        row.system = c.engine.clone();
    }
    match (report.exit, child) {
        (Exit::Timeout, _) => {
            row.status = Status::Limit;
            row.detail = format!("time: {KILLED} after {:.1} s", report.wall.as_secs_f64());
        }
        (Exit::MemoryCap, _) => {
            row.status = Status::Limit;
            row.detail = format!("memory cap {} MB", limits.memory_bytes >> 20);
        }
        (Exit::Code(0), Some(c)) => {
            match c.outcome {
                Outcome::Ok => {
                    row.status = Status::Ok;
                    row.stats = Stats::of(&c.times);
                    row.reps = c.times.len();
                    row.peak_bytes = c.peak_bytes;
                }
                Outcome::Limit(what) => {
                    row.status = Status::Limit;
                    row.detail = what;
                }
                Outcome::Error(e) => {
                    row.status = Status::Error;
                    row.detail = e;
                }
            }
            if let Some(check) = c.check {
                row.check = check.word().into();
                row.detail = [check.detail(), c.note.as_str()]
                    .iter()
                    .filter(|s| !s.is_empty())
                    .copied()
                    .collect::<Vec<_>>()
                    .join("; ");
            }
        }
        (exit, _) => {
            let last = report.stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
            let how = match exit {
                Exit::Code(code) => format!("exit code {code:#x}"),
                _ => "killed by a signal".into(),
            };
            row.detail = if last.is_empty() { how } else { format!("{how}: {}", crate::check::abbreviate(&last)) };
        }
    }
    Ok(row)
}

/// The per-run trace: one line per event (start, end, limits), with seconds since the run started.
struct Trace {
    file: fs::File,
    start: Instant,
}

impl Trace {
    fn open(path: &Path) -> io::Result<Trace> {
        Ok(Trace { file: OpenOptions::new().create(true).append(true).open(path)?, start: Instant::now() })
    }
    fn event(&mut self, case: &str, size: &str, event: &str, detail: &str) -> io::Result<()> {
        let t = self.start.elapsed().as_secs_f64();
        writeln!(self.file, "{t:.3}\t{case}\t{size}\t{event}\t{}", crate::results::clean(detail))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order() {
        let mut ids = vec!["R10", "R2", "R1b", "R1", "Q"];
        ids.sort_by_key(|s| natural_key(s));
        assert_eq!(ids, ["Q", "R1", "R1b", "R2", "R10"]);
    }

    #[test]
    fn readable_times() {
        assert_eq!(seconds(2.0), "2.000 s");
        assert_eq!(seconds(0.0025), "2.500 ms");
        assert_eq!(seconds(3e-7), "0.300 us");
    }
}
