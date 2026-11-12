//! End-to-end runs of the sample cases with the stub engine, each measurement in a guarded child process.
use mc_bench::case::Case;
use mc_bench::guard::{platform_guard, Guard, PortableGuard};
use mc_bench::results::{ResultsFile, Row, Status};
use mc_bench::runner::{self, RunConfig};
use mc_bench::summary::{summarize, Milestone};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn samples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("samples")
}

fn config(name: &str, cases_dir: PathBuf, only: &[&str]) -> RunConfig {
    let results_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&results_dir);
    RunConfig {
        cases_dir,
        results_dir,
        only: only.iter().map(|s| s.to_string()).collect(),
        child: (PathBuf::from(env!("CARGO_BIN_EXE_mc-bench-stub")), vec![OsString::from("child")]),
        memory_ceiling: 4 << 30,
        grace: Duration::from_secs(1),
        system: "stub 0".into(),
        machine: "test-machine".into(),
        date: "2026-01-02".into(),
    }
}

fn run(config: &RunConfig, guard: &dyn Guard) -> (Vec<Row>, String) {
    let cases = runner::discover(&config.cases_dir).expect("cases parse");
    let mut progress = Vec::new();
    let out = runner::run(config, &cases, guard, &mut progress).expect("run");
    assert_eq!(out.results_path, config.results_dir.join("test-machine").join("2026-01-02.tsv"));
    (out.rows, String::from_utf8(progress).unwrap())
}

fn find<'a>(rows: &'a [Row], case: &str, size: &str) -> &'a Row {
    rows.iter().find(|r| r.case == case && r.size == size).unwrap_or_else(|| panic!("no row {case} {size}"))
}

#[test]
fn samples_run_end_to_end_under_the_platform_guard() {
    let config = config("samples", samples(), &[]);
    let guard = platform_guard();
    let (rows, progress) = run(&config, guard.as_ref());

    // Progress: one `[k/N pct%]` line per measurement, in class and natural id order.
    let lines: Vec<&str> = progress.lines().filter(|l| l.starts_with('[')).collect();
    assert_eq!(lines.len(), 13, "{progress}");
    assert!(lines[0].starts_with("[1/13 7%] N/digits"), "{}", lines[0]);
    assert!(lines[12].starts_with("[13/13 100%] S/sum n = 10^5"), "{}", lines[12]);

    for (case, size) in [("fib", "20"), ("fib", "50"), ("fib", "90"), ("fact", "10"), ("fact", "30"), ("digits", "-")] {
        let r = find(&rows, case, size);
        assert_eq!((r.status, r.check.as_str()), (Status::Ok, "pass"), "{r:?}");
        assert!(r.reps >= 5 && r.stats.is_some(), "{r:?}");
    }
    assert!(rows.iter().all(|r| r.system == "stub 0"));
    let sum = find(&rows, "sum", "10^5");
    assert_eq!((sum.status, sum.check.as_str()), (Status::Ok, "pass"), "{sum:?}");
    assert!(sum.peak_bytes.unwrap() > 1 << 20, "the counting allocator sees the list: {sum:?}");
    assert_eq!(find(&rows, "wrong", "-").check, "fail");

    // Limits are reported as `limit`, never as times.
    let timeout = find(&rows, "timeout", "-");
    assert_eq!((timeout.status, timeout.stats), (Status::Limit, None), "{timeout:?}");
    assert!(timeout.detail.starts_with("time") && !timeout.is_hang(), "{timeout:?}");
    let hang = find(&rows, "hang", "-");
    assert_eq!((hang.status, hang.stats), (Status::Limit, None), "{hang:?}");
    assert!(hang.is_hang(), "{hang:?}");
    if guard.enforces_memory() {
        let cap = find(&rows, "memcap", "-");
        assert_eq!((cap.status, cap.stats), (Status::Limit, None), "{cap:?}");
        assert!(cap.detail.starts_with("memory cap 64 MB"), "{cap:?}");
    }

    // The results file round-trips, and the summary sees the hang.
    let file = ResultsFile::read(&config.results_dir.join("test-machine").join("2026-01-02.tsv")).unwrap();
    assert_eq!(file.rows, rows);
    assert!(file.header.iter().any(|(k, _)| k == "cpu"));
    let summary = summarize(&file.rows, None);
    assert!(matches!(&summary.milestones[0], Milestone::NotMet(why) if why.contains("R/hang")), "{}", summary.text);
    assert!(summary.text.contains("class N") && summary.text.contains("geometric mean"), "{}", summary.text);
    assert!(config.results_dir.join("test-machine").join("2026-01-02.trace").exists());
}

#[test]
fn partial_rerun_merges_into_the_same_file() {
    let config = config("rerun", samples(), &["fib"]);
    let guard = platform_guard();
    run(&config, guard.as_ref());
    let only_fact = RunConfig { only: vec!["fact".into()], ..config.clone() };
    run(&only_fact, guard.as_ref());
    let file = ResultsFile::read(&config.results_dir.join("test-machine").join("2026-01-02.tsv")).unwrap();
    assert_eq!(file.rows.len(), 6);
    let s = summarize(&file.rows, None);
    assert_eq!(s.milestones[0], Milestone::Met, "{}", s.text);
}

#[test]
fn portable_guard_kills_at_the_timeout() {
    let config = config("portable", samples(), &["hang", "timeout"]);
    let (rows, progress) = run(&config, &PortableGuard);
    assert!(progress.contains("does not enforce memory caps"));
    assert!(find(&rows, "hang", "-").is_hang());
    assert_eq!(find(&rows, "timeout", "-").status, Status::Limit);
}

#[test]
fn engine_errors_and_bad_case_files() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("error-cases");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("R")).unwrap();
    fs::write(dir.join("R").join("E1.mc"), "#@ measure\n1 +\n").unwrap();
    fs::write(dir.join("R").join("E2.mc"), "#@ setup\nundefined_name\n#@ measure\n1\n").unwrap();
    let config = config("errors", dir.clone(), &[]);
    let (rows, _) = run(&config, platform_guard().as_ref());
    let e1 = find(&rows, "E1", "-");
    assert_eq!(e1.status, Status::Error);
    assert!(e1.detail.contains("syntax error"), "{e1:?}");
    assert!(find(&rows, "E2", "-").detail.starts_with("setup:"));

    fs::write(dir.join("R").join("E3.mc"), "#@ check: exactly\n#@ measure\n1\n").unwrap();
    let errors = runner::discover(&dir).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].to_string().contains("E3.mc:1: unknown checker"), "{}", errors[0]);
}

#[test]
fn repository_cases_parse() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/cases");
    let cases = runner::discover(&dir)
        .unwrap_or_else(|e| panic!("{}", e.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n")));
    assert!(cases.len() >= 15, "{} cases", cases.len());
    for class in ["R", "P", "N", "S"] {
        assert!(cases.iter().any(|c: &Case| c.class == class), "no class {class} case");
    }
    assert!(cases.iter().all(|c| c.checker != mc_bench::case::Checker::None), "every repository case has a checker");
}
