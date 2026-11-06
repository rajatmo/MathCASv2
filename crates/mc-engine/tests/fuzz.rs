//! A fuzz campaign (package robust): random and mutated inputs, taken from the documentation pages, the example
//! scripts, the REPL sessions and the inputs of the crates' tests, go through the engine with a fuel budget and a
//! time limit. The properties: no panic escapes an evaluation, no evaluation outlives its time limit by more than a
//! grace period, and no input makes a native panic (the guard would turn that into `failure(internal, ...)`; each
//! one is a bug).
//!
//! The suite runs a short campaign. A long one: `MC_FUZZ_CASES=<n>` (and `MC_FUZZ_SEED=<seed>`, `MC_FUZZ_SECONDS=<s>`
//! for a time budget; `MC_STATUS_DIR` for a `robust-fuzz.json` progress file), run through the guard with
//! `--nocapture`; it prints `[k/N pct%]` progress, writes each input
//! to `fuzz-log.txt` in the test's temporary folder before it runs (the last line names the input of a crash) and
//! ends with a summary of the internal failures found, one example per source location.
use mc_bench::StopFlag;
use mc_engine::{CoreEngine, EngineConfig, Event, Origin};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// The time one input may take before its stop flag is raised.
const LIMIT: Duration = Duration::from_secs(3);
/// How long an input may go on after its stop flag was raised before it counts as a hang.
const GRACE: Duration = Duration::from_secs(20);
/// Fuel of one statement.
const FUEL: i64 = 20_000_000;
/// Inputs per engine before a fresh one is made (a long session is part of what is tested).
const PER_ENGINE: usize = 400;

/// xorshift64*: deterministic, no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
    fn pick<'a, X>(&mut self, xs: &'a [X]) -> &'a X {
        &xs[self.below(xs.len())]
    }
}

/// Names whose calls touch the file system or the environment: never introduced by a mutation, and inputs that
/// name them are left out of the corpus.
fn excluded(name: &str) -> bool {
    ["export", "write", "save", "delete", "directory", "file", "run_", "system", "environment", "import_module"]
        .iter()
        .any(|p| name.contains(p))
}

/// Atoms and small terms that arguments are replaced with.
const ATOMS: &[&str] = &[
    "0", "1", "-1", "2", "3", "7", "-12", "100", "10^6", "2^64", "-2^63", "10^30", "1/3", "-7/2", "0.0", "-0.0",
    "1.5", "-2.25", "1.0e-300", "1.0e300", "10.0^-320", "3.14159", "inf", "-inf", "im", "1 + 2*im", "2.5 - im",
    "x", "y", "z", "t", "n", "a", "\"\"", "\"s\"", "\"abc def\"", "[]", "[[]]", "[1]", "[1, 2, 3]", "[0, 0]",
    "[x, y]", "[1.5, x]", "[[1, 2], [3, 4]]", "[[1, 2], [3]]", "[[1, 2, 3]]", "[[0, 0], [0, 0]]", "range(70)",
    "range(0.0, 1.0, 0.125)", "[[1.0, 2.0], [3.0, 4.0]]", "true", "false", "null", "missing()", "failure(test)",
    "x^2 + 1", "sin(x)", "1/x", "x -> 1", "[x, 0, 1]", "[x, 0, 1, 0.1]", "[x, -inf, inf]", "k => k + 1",
    "(a, b) => a*b", "assoc(\"a\" -> 1, \"b\" -> 2)", "1.5~30", "2~10", "1e-320", "1e308",
    "[1, 2, 3, 4, 5, 6, 7, 8]", "[[2, 1], [1, 2]]", "[[1, 2], [2, 4]]", "[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 10.0]]",
    "[1, x, x^2]", "x + y", "x*y - 1", "(x - 1)^3", "[[x, 1], [0, x]]", "table(k, k, 100)", "table(1.0*k, k, 65)", "pi", "e", "sqrt(2)", "0^0", "1/0",
];

/// Numbers that numbers of valid inputs are replaced with.
const NUMBERS: &[&str] = &[
    "0", "1", "-1", "2", "3", "5", "64", "65", "-3", "1000", "10^6", "2^31", "2^63", "2^64", "-2^63", "10^40", "1/2", "-1/3",
    "0.0", "-0.0", "0.5", "-1.5", "1e-310", "1e300", "2.0^1023", "1e-20", "3.0~25", "inf", "-inf",
];

/// Characters inserted at random (the reader's share of the campaign).
const NOISE: &[&str] = &["(", ")", "[", "]", ",", ";", "=", "->", "=>", ":=", "_", "...", "&", "#", "\"", "\\", "^", "'", ".", "@", "|", "{", "}", "λ", "\u{0}", "\n", "  "];

/// The inputs of the corpus: documentation cells (each page's cells in order, as one input each), REPL session
/// inputs, example scripts split at blank lines, and the string literals the crates' tests evaluate.
fn corpus(root: &Path) -> Vec<String> {
    let mut out = vec![];
    let files = |dir: &Path, ext: &str| -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == ext)).collect())
            .unwrap_or_default();
        v.sort();
        v
    };
    for p in files(&root.join("docs/functions"), "mcn") {
        let text = std::fs::read_to_string(&p).unwrap_or_default();
        let mut cell: Option<String> = None;
        for line in text.lines() {
            if let Some(kind) = line.strip_prefix("#% ") {
                out.extend(cell.take());
                if kind.trim() == "cell" {
                    cell = Some(String::new());
                }
            } else if let Some(c) = cell.as_mut() {
                c.push_str(line);
                c.push('\n');
            }
        }
        out.extend(cell);
    }
    for p in files(&root.join("crates/mc-repl/tests/sessions"), "txt") {
        let text = std::fs::read_to_string(&p).unwrap_or_default();
        out.extend(text.lines().filter_map(|l| l.strip_prefix("mc> ")).map(String::from));
    }
    for p in files(&root.join("examples"), "mc") {
        let text = std::fs::read_to_string(&p).unwrap_or_default();
        out.extend(text.split("\n\n").map(String::from));
    }
    // `("input", "expected")` pairs in the tests of every crate
    let crates = std::fs::read_dir(root.join("crates")).map(|d| d.filter_map(|e| e.ok()).map(|e| e.path()).collect::<Vec<_>>());
    for c in crates.unwrap_or_default() {
        for p in files(&c.join("tests"), "rs") {
            let text = std::fs::read_to_string(&p).unwrap_or_default();
            out.extend(test_inputs(&text));
        }
    }
    out.retain(|s| !s.trim().is_empty() && !excluded(s) && !s.contains("import") && s.len() < 2000);
    out
}

/// The first string of every `("...", "...")` pair in Rust source, unescaped.
fn test_inputs(text: &str) -> Vec<String> {
    let mut out = vec![];
    let b = text.as_bytes();
    let mut i = 0;
    while i + 2 < b.len() {
        if b[i] == b'(' && b[i + 1] == b'"' {
            if let Some((s, end)) = string_at(text, i + 1) {
                let rest = text[end..].trim_start();
                if rest.starts_with(',') && rest[1..].trim_start().starts_with('"') {
                    out.push(s);
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// The Rust string literal starting at byte `i` (a `"`), unescaped, and the byte after it.
fn string_at(text: &str, i: usize) -> Option<(String, usize)> {
    let mut s = String::new();
    let mut chars = text[i + 1..].char_indices();
    while let Some((j, c)) = chars.next() {
        match c {
            '"' => return Some((s, i + 1 + j + 1)),
            '\\' => match chars.next()?.1 {
                'n' => s.push('\n'),
                't' => s.push('\t'),
                'u' => return None,
                c => s.push(c),
            },
            c => s.push(c),
        }
    }
    None
}

/// A token of the mutator's crude lexer.
#[derive(Clone, PartialEq)]
enum Tok {
    Name(String),
    Num(String),
    Str(String),
    Other(String),
}

impl Tok {
    fn text(&self) -> &str {
        match self {
            Tok::Name(s) | Tok::Num(s) | Tok::Str(s) | Tok::Other(s) => s,
        }
    }
}

fn lex(src: &str) -> Vec<Tok> {
    let cs: Vec<char> = src.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        let start = i;
        if c.is_alphabetic() || c == '_' {
            while i < cs.len() && (cs[i].is_alphanumeric() || cs[i] == '_') {
                i += 1;
            }
            out.push(Tok::Name(cs[start..i].iter().collect()));
        } else if c.is_ascii_digit() {
            while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == '.') {
                i += 1;
            }
            out.push(Tok::Num(cs[start..i].iter().collect()));
        } else if c == '"' {
            i += 1;
            while i < cs.len() && cs[i] != '"' {
                i += if cs[i] == '\\' { 2 } else { 1 };
            }
            i = (i + 1).min(cs.len());
            out.push(Tok::Str(cs[start..i].iter().collect()));
        } else {
            i += 1;
            out.push(Tok::Other(c.to_string()));
        }
    }
    out
}

/// One mutation of `src`.
fn mutate(rng: &mut Rng, src: &str, functions: &[String]) -> String {
    let mut toks = lex(src);
    if toks.is_empty() {
        return src.to_string();
    }
    // half of the mutations change only numbers: the input stays readable and reaches the libraries
    if rng.chance(50) {
        let mut changed = false;
        for t in toks.iter_mut() {
            if matches!(t, Tok::Num(_)) && rng.chance(40) {
                *t = Tok::Other(format!("({})", rng.pick(NUMBERS)));
                changed = true;
            }
        }
        if changed {
            return toks.iter().map(Tok::text).collect();
        }
    }
    let rounds = 1 + rng.below(3);
    for _ in 0..rounds {
        let i = rng.below(toks.len());
        match rng.below(10) {
            // a number or a name becomes an atom
            0..=2 => {
                let candidates: Vec<usize> = (0..toks.len()).filter(|&j| matches!(toks[j], Tok::Num(_) | Tok::Name(_) | Tok::Str(_))).collect();
                if let Some(&j) = candidates.get(rng.below(candidates.len().max(1))) {
                    toks[j] = Tok::Other(format!("({})", rng.pick(ATOMS)));
                }
            }
            // a called function becomes another
            3 | 4 => {
                let calls: Vec<usize> =
                    (0..toks.len().saturating_sub(1)).filter(|&j| matches!(toks[j], Tok::Name(_)) && toks[j + 1].text() == "(").collect();
                if !calls.is_empty() {
                    let j = *rng.pick(&calls);
                    toks[j] = Tok::Name(rng.pick(functions).clone());
                }
            }
            // an argument is dropped or duplicated
            5 => {
                if toks[i].text() == "," {
                    toks.remove(i);
                    if i < toks.len() {
                        toks.remove(i);
                    }
                } else {
                    let t = toks[i].clone();
                    toks.insert(i, Tok::Other(", ".into()));
                    toks.insert(i, t);
                }
            }
            // reader noise
            6 => toks.insert(i, Tok::Other(rng.pick(NOISE).to_string())),
            7 => {
                toks.remove(i);
            }
            // a call wrapped around a token
            8 => {
                let f = rng.pick(functions).clone();
                let t = toks[i].text().to_string();
                if !matches!(toks[i], Tok::Other(_)) {
                    toks[i] = Tok::Other(format!("{f}({t})"));
                }
            }
            _ => {
                let n = rng.below(6);
                toks[i] = Tok::Other(n.to_string());
            }
        }
        if toks.is_empty() {
            break;
        }
    }
    toks.iter().map(Tok::text).collect()
}

/// A random call: a library function applied to random arguments (atoms, or random calls themselves).
fn random_call(rng: &mut Rng, functions: &[String], depth: u32) -> String {
    let f = rng.pick(functions);
    let n = rng.below(5);
    let args: Vec<String> = (0..n)
        .map(|_| match rng.below(100) {
            0..=24 if depth > 0 => random_call(rng, functions, depth - 1),
            25..=59 => rng.pick(NUMBERS).to_string(),
            _ => rng.pick(ATOMS).to_string(),
        })
        .collect();
    format!("{f}({})", args.join(", "))
}

/// A library function applied to a deep term (nested tens of thousands of levels): a native that recurses on the
/// term's structure overflows its stack, which ends the process (no guard can catch it).
fn deep_call(rng: &mut Rng, functions: &[String]) -> String {
    const NESTS: &[&str] = &["[x]", "g(x)", "x + 1", "sin(x)", "x^2", "-x", "x*y", "1/x", "[x, 1]", "x -> 1", "exp(x)"];
    const DEPTHS: &[u32] = &[300, 3000, 30000, 100000];
    let f = rng.pick(functions);
    let inner = format!("nest(x => {}, {}, {})", rng.pick(NESTS), rng.pick(&["0", "y", "1.5"]), rng.pick(DEPTHS));
    match rng.below(3) {
        0 => format!("{f}({inner})"),
        1 => format!("{f}({inner}, {})", rng.pick(ATOMS)),
        _ => format!("{f}({}, {inner})", rng.pick(ATOMS)),
    }
}

/// The progress file a long campaign keeps for `tools/status.ps1` when `MC_STATUS_DIR` is set: `robust-fuzz.json`,
/// rewritten (through a temporary file and a rename) at most every few seconds.
struct Status {
    path: Option<PathBuf>,
    started: String,
    last: Instant,
}

/// The current UTC time as `YYYY-MM-DDTHH:MM:SSZ` (civil date from the day count, Hinnant's algorithm).
fn utc_now() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem / 60 % 60, rem % 60)
}

/// A JSON string literal.
fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

impl Status {
    fn new() -> Status {
        let path = std::env::var_os("MC_STATUS_DIR").map(|d| PathBuf::from(d).join("robust-fuzz.json"));
        Status { path, started: utc_now(), last: Instant::now() - Duration::from_secs(60) }
    }

    fn write(&mut self, done: usize, total: usize, current: &str, state: &str, message: &str, force: bool) {
        let Some(path) = &self.path else { return };
        if !force && self.last.elapsed() < Duration::from_secs(5) {
            return;
        }
        self.last = Instant::now();
        let json = format!(
            "{{\"job\": \"robust-fuzz\", \"owner\": \"robust\", \"title\": \"fuzz campaign\", \"done\": {done}, \"total\": {total}, \
             \"unit\": \"inputs\", \"current\": {}, \"state\": \"{state}\", \"started\": \"{}\", \"updated\": \"{}\", \
             \"message\": {}}}",
            json_str(current),
            self.started,
            utc_now(),
            json_str(message)
        );
        let _ = std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")));
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

/// What one campaign found.
#[derive(Default)]
struct Findings {
    cases: usize,
    statements_failed: usize,
    read_errors: usize,
    stopped: usize,
    /// internal failures by source location: (count, first input, failure)
    internal: BTreeMap<String, (usize, String, String)>,
    /// inputs whose panic escaped the engine
    escaped: Vec<(String, String)>,
    slowest: (Duration, String),
    /// inputs that took more than a second past their time limit to stop
    late: Vec<(Duration, String)>,
}

impl Findings {
    fn note_internal(&mut self, src: &str, failure: &str) {
        // the location: the text in the last parentheses of the message
        let key = failure.rsplit_once(" (").map_or(failure, |(_, l)| l).split(')').next().unwrap_or("").to_string();
        let e = self.internal.entry(key).or_insert((0, src.to_string(), failure.to_string()));
        e.0 += 1;
    }
}

/// The watchdog: raises the stop flag of an input that passes [`LIMIT`], and ends the process on a hang.
struct Watchdog {
    tx: mpsc::Sender<Option<(StopFlag, String)>>,
}

impl Watchdog {
    fn new() -> Watchdog {
        let (tx, rx) = mpsc::channel::<Option<(StopFlag, String)>>();
        std::thread::spawn(move || {
            let mut current: Option<(StopFlag, String, Instant)> = None;
            loop {
                let wait = match &current {
                    Some((_, _, t)) => (*t + LIMIT + GRACE).saturating_duration_since(Instant::now()).min(Duration::from_millis(50)),
                    None => Duration::from_secs(3600),
                };
                match rx.recv_timeout(wait) {
                    Ok(Some((flag, src))) => current = Some((flag, src, Instant::now())),
                    Ok(None) => current = None,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
                if let Some((flag, src, t)) = &current {
                    let age = t.elapsed();
                    if age > LIMIT {
                        flag.raise();
                    }
                    if age > LIMIT + GRACE {
                        eprintln!("HANG: the input did not stop {GRACE:?} after its stop flag was raised:\n{src}");
                        std::process::exit(3);
                    }
                }
            }
        });
        Watchdog { tx }
    }

    fn start(&self, stop: &StopFlag, src: &str) {
        let _ = self.tx.send(Some((stop.clone(), src.to_string())));
    }

    fn end(&self) {
        let _ = self.tx.send(None);
    }
}

fn fresh_engine() -> CoreEngine {
    let mut cfg = EngineConfig { threads: 3, ..EngineConfig::default() };
    cfg.eval.fuel = FUEL;
    CoreEngine::with_config(cfg)
}

fn campaign(cases: usize, seed: u64, budget: Duration, log: &Path, progress: bool) -> Findings {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus = corpus(&root);
    assert!(corpus.len() > 100, "the corpus is read from the repository ({} inputs)", corpus.len());
    let mut log = std::io::BufWriter::new(std::fs::File::create(log).expect("the log file"));
    let mut rng = Rng(seed.max(1));
    let mut engine = fresh_engine();
    let functions: Vec<String> = engine.names().into_iter().filter(|n| !excluded(n) && !n.contains('.')).collect();
    let watchdog = Watchdog::new();
    let mut f = Findings::default();
    let mut used = 0;
    let started = Instant::now();
    let mut status = Status::new();
    for k in 0..cases {
        let found = |f: &Findings| format!("{} internal, {} escaped", f.internal.len(), f.escaped.len());
        status.write(k, cases, &format!("seed {seed}, input {k}"), "running", &found(&f), false);
        if started.elapsed() > budget {
            eprintln!("time budget reached after {k} inputs");
            break;
        }
        if progress && (k * 100 / cases.max(1)) != ((k + 1) * 100 / cases.max(1)) {
            eprintln!("[{}/{} {}%] internal {} escaped {}", k + 1, cases, (k + 1) * 100 / cases, f.internal.values().map(|v| v.0).sum::<usize>(), f.escaped.len());
        }
        if used == PER_ENGINE {
            engine = fresh_engine();
            used = 0;
        }
        used += 1;
        let src = match rng.below(20) {
            0..=4 => rng.pick(&corpus).clone(),
            5..=12 => {
                let base = rng.pick(&corpus).clone();
                mutate(&mut rng, &base, &functions)
            }
            13 => deep_call(&mut rng, &functions),
            _ => random_call(&mut rng, &functions, 2),
        };
        let _ = writeln!(log, "--- {k}\n{src}");
        let _ = log.flush();
        let stop = StopFlag::new();
        watchdog.start(&stop, &src);
        let t0 = Instant::now();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| engine.eval_in(&src, &Origin::input(), &stop)));
        let took = t0.elapsed();
        watchdog.end();
        if took > LIMIT + Duration::from_secs(1) {
            f.late.push((took, src.clone()));
        }
        if took > f.slowest.0 {
            f.slowest = (took, src.clone());
        }
        f.cases += 1;
        match r {
            Err(p) => {
                f.escaped.push((src.clone(), mc_engine::guard::payload_text(&*p)));
                engine = fresh_engine();
                used = 0;
            }
            Ok(Err(mc_engine::RunError::Read(_))) => f.read_errors += 1,
            Ok(Err(mc_engine::RunError::Engine(e))) => {
                let e = e.to_string();
                if e.contains("internal") {
                    f.note_internal(&src, &e);
                }
                f.stopped += 1;
            }
            Ok(Ok(script)) => {
                for ev in &script.events {
                    if let Event::Failure(s) = ev {
                        f.statements_failed += 1;
                        if s.starts_with("failure(internal") {
                            f.note_internal(&src, s);
                        }
                    }
                }
                if script.last.printed.starts_with("failure(internal") && !script.failed() {
                    f.note_internal(&src, &script.last.printed);
                }
            }
        }
    }
    let state = if f.internal.is_empty() && f.escaped.is_empty() { "done" } else { "failed" };
    let message = format!("{} internal, {} escaped", f.internal.len(), f.escaped.len());
    status.write(f.cases, cases, &format!("seed {seed}"), state, &message, true);
    f
}

fn report(f: &Findings) -> String {
    let mut s = format!(
        "{} inputs: {} read errors, {} stopped, {} failed statements; slowest {:?}\n",
        f.cases, f.read_errors, f.stopped, f.statements_failed, f.slowest.0
    );
    for (loc, (n, src, failure)) in &f.internal {
        s.push_str(&format!("internal x{n} at {loc}\n  input: {}\n  value: {}\n", src.trim(), failure.chars().take(300).collect::<String>()));
    }
    for (took, src) in &f.late {
        s.push_str(&format!("late stop after {took:?}
  input: {}
", src.trim()));
    }
    for (src, msg) in &f.escaped {
        s.push_str(&format!("ESCAPED: {msg}\n  input: {}\n", src.trim()));
    }
    s
}

/// The temporary folder of this test binary: plots go there (the engine renders graphics values to files).
fn tmp() -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join("fuzz");
    std::fs::create_dir_all(&d).expect("the temporary folder");
    d
}

/// Replays the inputs `from..=to` of a campaign log in one engine, printing each before it runs: set
/// `MC_FUZZ_REPLAY=<log>`, `MC_FUZZ_FROM`, `MC_FUZZ_TO` (a crash depends on what the same engine evaluated before;
/// the campaign makes a fresh engine every 400 inputs, at multiples of 400).
#[test]
fn replay() {
    let Some(log) = std::env::var_os("MC_FUZZ_REPLAY") else { return };
    let num = |v: &str| std::env::var(v).ok().and_then(|s| s.parse::<usize>().ok());
    let (from, to) = (num("MC_FUZZ_FROM").unwrap_or(0), num("MC_FUZZ_TO").unwrap_or(usize::MAX));
    let text = std::fs::read_to_string(log).expect("the log");
    let mut engine = fresh_engine();
    let mut entries: Vec<(usize, String)> = vec![];
    for line in text.split_inclusive('\n') {
        match line.strip_prefix("--- ").and_then(|l| l.trim().parse().ok()) {
            Some(k) => entries.push((k, String::new())),
            None => {
                if let Some(e) = entries.last_mut() {
                    e.1.push_str(line);
                }
            }
        }
    }
    for (k, src) in entries.into_iter().filter(|(k, _)| (from..=to).contains(k)) {
        eprintln!("--- {k}\n{src}");
        let r = engine.eval_in(&src, &Origin::input(), &StopFlag::new());
        eprintln!("=> {}", r.map_or_else(|e| e.to_string(), |s| s.last.printed.chars().take(200).collect()));
    }
}

#[test]
fn no_panic_escapes_and_no_native_panics() {
    let cases = std::env::var("MC_FUZZ_CASES").ok().and_then(|s| s.parse().ok()).unwrap_or(300);
    let seed = std::env::var("MC_FUZZ_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(0x5EED_2026);
    let long = cases > 300;
    // a wall-clock budget for long runs (the summary is printed when it ends)
    let budget = std::env::var("MC_FUZZ_SECONDS").ok().and_then(|s| s.parse().ok()).map_or(Duration::MAX, Duration::from_secs);
    let dir = tmp();
    // plots and other temporary files of the campaign stay in the test's own folder
    std::env::set_var("TMP", &dir);
    std::env::set_var("TEMP", &dir);
    std::env::set_var("TMPDIR", &dir);
    let f = campaign(cases, seed, budget, &dir.join("fuzz-log.txt"), long);
    let text = report(&f);
    eprintln!("{text}");
    assert!(f.escaped.is_empty() && f.internal.is_empty(), "{text}");
}
