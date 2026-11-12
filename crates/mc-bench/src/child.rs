//! The child side: run one measurement of one case under the protocol of 02-benchmarks section 1, in a process of
//! its own, and report one result line on standard output.
//!
//! Protocol: the setup runs untimed; one warm-up evaluation; then repetitions until at least the case's minimum
//! count and minimum time are both reached, as long as another repetition fits in the time limit. A watchdog
//! raises the engine's stop flag at the time limit, so a well-behaved engine stops cleanly and the case is
//! reported as `limit`; the parent's guard kills the process only if it does not (decision D.6).
use crate::case::{Case, Checker};
use crate::check::{self, Check};
use mc_api::{Engine, EngineError, EngineFactory, Evaluated, StopFlag};
use crate::memory::PeakProbe;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Repetitions stop here even if the minimum time is not reached, which bounds the loop for very fast cases.
const MAX_REPS: usize = 100_000;

/// The prefix of the result line, which the parent looks for among anything the engine printed.
pub const RESULT_PREFIX: &str = "mc-bench-result";

/// How the child's measurement ended.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Ok,
    /// A limit, with what was hit (`time`, or the engine's own limit).
    Limit(String),
    Error(String),
}

/// What the child reports to the parent.
#[derive(Clone, Debug, PartialEq)]
pub struct ChildReport {
    pub engine: String,
    pub outcome: Outcome,
    /// Seconds per timed repetition.
    pub times: Vec<f64>,
    pub peak_bytes: Option<u64>,
    pub check: Option<Check>,
    /// Remarks on the measurement, such as repetitions cut short by the time limit.
    pub note: String,
}

/// Raises a stop flag when its time is up, unless dropped first.
struct Watchdog {
    cancel: Option<mpsc::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Watchdog {
    fn start(limit: Duration, stop: StopFlag) -> Watchdog {
        let (tx, rx) = mpsc::channel::<()>();
        let thread = thread::spawn(move || {
            if let Err(mpsc::RecvTimeoutError::Timeout) = rx.recv_timeout(limit) {
                stop.raise();
            }
        });
        Watchdog { cancel: Some(tx), thread: Some(thread) }
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        drop(self.cancel.take());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Run measurement `index` of `case` with engines from `factory`.
pub fn run(case: &Case, index: usize, factory: &EngineFactory) -> ChildReport {
    let stop = StopFlag::new();
    let start = Instant::now();
    let _watchdog = Watchdog::start(case.limits.time, stop.clone());
    let make = || {
        let mut e = factory();
        // a setting the engine does not know leaves the case measured with its defaults (the note says so)
        let unset = (!case.memo).then(|| e.configure("memo", "off").err()).flatten();
        (e, unset)
    };
    let (mut engine, unset) = make();
    let mut report = ChildReport {
        engine: engine.name(),
        outcome: Outcome::Ok,
        times: Vec::new(),
        peak_bytes: None,
        check: None,
        note: unset.map_or_else(String::new, |e| format!("engine setting not applied: {e}")),
    };
    let setup = case.setup_source(index);
    let fail = |report: &mut ChildReport, phase: &str, e: EngineError| {
        report.outcome = match e {
            EngineError::Stopped => Outcome::Limit(format!("time (stopped in the {phase})")),
            EngineError::Limit(what) => Outcome::Limit(format!("{what} (in the {phase})")),
            other => Outcome::Error(format!("{phase}: {other}")),
        };
    };
    if let Err(e) = engine.eval(&setup, &stop) {
        fail(&mut report, "setup", e);
        return report;
    }
    // Warm-up.
    let (warm, last) = match timed(engine.as_mut(), &case.measure, &stop) {
        Ok(x) => x,
        Err(e) => {
            fail(&mut report, "warm-up", e);
            return report;
        }
    };
    let mut peak = warm.peak;
    let mut last = last;
    let reps_start = Instant::now();
    loop {
        let enough = report.times.len() >= case.reps.min_reps as usize && reps_start.elapsed() >= case.reps.min_time;
        if enough || report.times.len() >= MAX_REPS {
            break;
        }
        let projected = start.elapsed() + Duration::from_secs_f64(report.times.last().copied().unwrap_or(warm.secs));
        if projected >= case.limits.time {
            report.note = format!("{} repetitions fit in the time limit", report.times.len());
            break;
        }
        if case.fresh_session {
            engine = make().0;
            if let Err(e) = engine.eval(&setup, &stop) {
                fail(&mut report, "setup", e);
                return report;
            }
        }
        match timed(engine.as_mut(), &case.measure, &stop) {
            Ok((t, value)) => {
                report.times.push(t.secs);
                peak = peak.max(t.peak);
                last = value;
            }
            Err(EngineError::Stopped) if !report.times.is_empty() => {
                report.note = format!("repetitions cut at {} by the time limit", report.times.len());
                break;
            }
            Err(e) => {
                fail(&mut report, "measurement", e);
                return report;
            }
        }
    }
    if report.times.is_empty() {
        // Not even one repetition fits after the warm-up: the warm-up is the only sample.
        report.times.push(warm.secs);
        report.note = "the warm-up is the only sample".into();
    }
    report.peak_bytes = peak;
    report.check = Some(check(engine.as_mut(), case, index, &last, &stop));
    report
}

struct Timing {
    secs: f64,
    peak: Option<u64>,
}

fn timed(engine: &mut dyn Engine, src: &str, stop: &StopFlag) -> Result<(Timing, Evaluated), EngineError> {
    let probe = PeakProbe::start();
    let t0 = Instant::now();
    let value = engine.eval(src, stop)?;
    let secs = t0.elapsed().as_secs_f64();
    Ok((Timing { secs, peak: probe.map(|p| p.peak_above_baseline()) }, value))
}

fn check(engine: &mut dyn Engine, case: &Case, index: usize, last: &Evaluated, stop: &StopFlag) -> Check {
    let expected = match case.expected(index) {
        Ok(e) => e,
        Err(e) => return Check::Unchecked(e),
    };
    let missing = || Check::Unchecked("no stored value".into());
    match &case.checker {
        Checker::None => Check::Unchecked("reported, not checked".into()),
        Checker::Exact => expected.map_or_else(missing, |e| check::exact(&last.printed, &e)),
        Checker::ExactHash => expected.map_or_else(missing, |e| check::hash(last.fingerprint, &e)),
        Checker::Digits(d) => expected.map_or_else(missing, |e| check::digits(&last.printed, &e, *d)),
        Checker::Property => {
            let outcome = engine.bind_last("result").and_then(|()| engine.eval(&case.property, stop));
            match outcome {
                Ok(v) if v.printed.trim() == "true" => Check::Pass,
                Ok(v) => Check::Fail(format!("property gave {}", check::abbreviate(v.printed.trim()))),
                Err(EngineError::Stopped) => Check::Unchecked("property stopped at the time limit".into()),
                Err(e) => Check::Fail(format!("property: {e}")),
            }
        }
    }
}

impl ChildReport {
    /// The single result line: `mc-bench-result` and tab-separated `key=value` fields.
    pub fn encode(&self) -> String {
        let (status, detail) = match &self.outcome {
            Outcome::Ok => ("ok", String::new()),
            Outcome::Limit(d) => ("limit", d.clone()),
            Outcome::Error(d) => ("error", d.clone()),
        };
        let times: Vec<String> = self.times.iter().map(|t| format!("{t:e}")).collect();
        let fields = [
            ("engine", self.engine.clone()),
            ("status", status.into()),
            ("detail", detail),
            ("times", times.join(",")),
            ("peak", self.peak_bytes.map(|b| b.to_string()).unwrap_or_default()),
            ("check", self.check.as_ref().map(|c| c.word().to_string()).unwrap_or_default()),
            ("check_detail", self.check.as_ref().map(|c| c.detail().to_string()).unwrap_or_default()),
            ("note", self.note.clone()),
        ];
        let mut line = RESULT_PREFIX.to_string();
        for (k, v) in fields {
            line.push('\t');
            line.push_str(k);
            line.push('=');
            line.push_str(&crate::results::clean(&v));
        }
        line
    }

    /// Find and decode the result line in a child's standard output.
    pub fn decode(stdout: &str) -> Option<ChildReport> {
        let line = stdout.lines().rev().find(|l| l.starts_with(RESULT_PREFIX))?;
        let mut get = std::collections::HashMap::new();
        for field in line.split('\t').skip(1) {
            let (k, v) = field.split_once('=')?;
            get.insert(k, v);
        }
        let text = |k: &str| get.get(k).copied().unwrap_or("").to_string();
        let detail = text("detail");
        let outcome = match get.get("status").copied()? {
            "ok" => Outcome::Ok,
            "limit" => Outcome::Limit(detail),
            "error" => Outcome::Error(detail),
            _ => return None,
        };
        let times = text("times");
        let times = if times.is_empty() {
            Vec::new()
        } else {
            times.split(',').map(str::parse).collect::<Result<_, _>>().ok()?
        };
        let check_detail = text("check_detail");
        let check = match get.get("check").copied().unwrap_or("") {
            "" => None,
            "pass" => Some(Check::Pass),
            "fail" => Some(Check::Fail(check_detail)),
            "unchecked" => Some(Check::Unchecked(check_detail)),
            _ => return None,
        };
        Some(ChildReport {
            engine: text("engine"),
            outcome,
            times,
            peak_bytes: get.get("peak").and_then(|p| p.parse().ok()),
            check,
            note: text("note"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stub::StubEngine;
    use std::path::Path;

    fn case(text: &str) -> Case {
        Case::parse(Path::new("cases/R/t.mc"), text).unwrap()
    }

    fn stub() -> Box<dyn Engine> {
        Box::new(StubEngine::new())
    }

    #[test]
    fn measures_and_checks() {
        let c =
            case("#@ sizes: n = 10, 20\n#@ check: exact\n#@ expect 20: 6765\n#@ reps: 3, 1ms\n#@ measure\nfib(n)\n");
        let r = run(&c, 1, &stub);
        assert_eq!(r.outcome, Outcome::Ok);
        assert!(r.times.len() >= 3);
        assert_eq!(r.check, Some(Check::Pass));
        assert_eq!(run(&c, 0, &stub).check.unwrap().word(), "unchecked");
    }

    #[test]
    fn property_and_fresh_sessions() {
        let c = case(
            "#@ check: property\n#@ session: fresh\n#@ reps: 2, 1ms\n#@ setup\nk = 4\n#@ measure\nsum(range(k))\n\
             #@ property\nresult == k*(k + 1)/2\n",
        );
        let r = run(&c, 0, &stub);
        assert_eq!((r.outcome, r.check), (Outcome::Ok, Some(Check::Pass)));
    }

    #[test]
    fn errors_and_limits() {
        let r = run(&case("#@ setup\n1 +\n#@ measure\n1\n"), 0, &stub);
        assert!(matches!(r.outcome, Outcome::Error(ref e) if e.starts_with("setup: syntax error")));
        let r = run(&case("#@ limits: time 100ms\n#@ measure\nbusy(10^15)\n"), 0, &stub);
        assert!(matches!(r.outcome, Outcome::Limit(ref e) if e.starts_with("time")), "{r:?}");
    }

    #[test]
    fn result_line_round_trip() {
        let r = ChildReport {
            engine: "stub 0".into(),
            outcome: Outcome::Limit("time\tx".into()),
            times: vec![0.5, 1e-7],
            peak_bytes: Some(42),
            check: Some(Check::Fail("got 1".into())),
            note: String::new(),
        };
        let back = ChildReport::decode(&format!("noise\n{}\n", r.encode())).unwrap();
        assert_eq!(back.outcome, Outcome::Limit("time x".into()));
        assert_eq!((back.times, back.peak_bytes, back.check), (r.times, r.peak_bytes, r.check));
        assert!(ChildReport::decode("nothing here").is_none());
    }
}
