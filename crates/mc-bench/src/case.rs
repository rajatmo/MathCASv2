//! Case files: one benchmark case per file, `bench/cases/<class>/<id>.mc` (02-benchmarks section 7).
//!
//! A case file is an ordinary v2 script (`#! mathcas 2`, 04-syntax section 11). The runner reads only its directive
//! lines, which start with `#@` and are therefore comments to the language (decision D.2):
//!
//! ```text
//! #! mathcas 2
//! #@ title: Naive recursive fib
//! #@ sizes: n = 20, 25, 30
//! #@ check: exact
//! #@ expect 20: 6765
//! #@ limits: time 60s, memory 8GB
//! #@ setup
//! rule fib(0) = 0
//! rule fib(1) = 1
//! rule fib(n_: Int) := fib(n - 1) + fib(n - 2)
//! #@ measure
//! fib(n)
//! ```
//!
//! Directives (`#@ key: value`):
//!
//! | Key | Meaning | Default |
//! |---|---|---|
//! | `case` | the id; must equal the file stem | the file stem |
//! | `class` | the class; must equal the directory name | the directory name |
//! | `title`, `stresses`, `note` | descriptive text | empty |
//! | `sizes` | `var = s1, s2, ...`: each size is concrete syntax, bound by `var = s` before the setup | one unsized run |
//! | `check` | `exact`, `exact hash`, `property`, `digits <d>` or `none` | `none` |
//! | `expect <size>` | the stored value for that size (`expect:` for an unsized case); `@file` reads it from a file next to the case | none (`unchecked`) |
//! | `limits` | `time <dur>, memory <size>` | `time 60s, memory 8GB` |
//! | `reps` | `<min reps>, <min time>` | `5, 2s` |
//! | `session` | `shared` (one session for all repetitions) or `fresh` (a new session and setup for each) | `shared` |
//! | `threads` | the thread count the case runs at | `1` |
//! | `memo` | `on` or `off`: whether the engine may memoise normal forms (off measures the work itself, not the cache; passed to [`Engine::configure`](mc_api::Engine::configure)) | `on` |
//!
//! Section markers `#@ setup`, `#@ measure` and `#@ property` start code sections; the code runs to the next marker.
//! The measured expression is the `measure` section; `property` is evaluated after timing with the result bound to
//! `result` and must evaluate to `true`.
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How a result is checked (02-benchmarks section 2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Checker {
    /// The printed result equals the stored value.
    Exact,
    /// The engine's fingerprint equals the stored hash (hexadecimal, `0x` optional).
    ExactHash,
    /// The `property` section evaluates to `true`.
    Property,
    /// The printed number agrees with the stored value to this many significant digits.
    Digits(usize),
    /// Reported, not checked (S9, S10).
    None,
}

impl fmt::Display for Checker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Checker::Exact => write!(f, "exact"),
            Checker::ExactHash => write!(f, "exact hash"),
            Checker::Property => write!(f, "property"),
            Checker::Digits(d) => write!(f, "digits {d}"),
            Checker::None => write!(f, "none"),
        }
    }
}

/// Time and memory limits of one case (02-benchmarks section 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub time: Duration,
    pub memory_bytes: u64,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { time: Duration::from_secs(60), memory_bytes: 8 << 30 }
    }
}

/// How many timed repetitions to run: at least `min_reps` and for at least `min_time`, whichever is longer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RepPolicy {
    pub min_reps: u32,
    pub min_time: Duration,
}

impl Default for RepPolicy {
    fn default() -> RepPolicy {
        RepPolicy { min_reps: 5, min_time: Duration::from_secs(2) }
    }
}

/// A parsed case file.
#[derive(Clone, Debug)]
pub struct Case {
    pub path: PathBuf,
    pub id: String,
    pub class: String,
    pub title: String,
    pub stresses: String,
    pub notes: Vec<String>,
    /// The name the size is bound to, when the case has sizes.
    pub variable: Option<String>,
    /// The sizes as concrete syntax; empty for an unsized case.
    pub sizes: Vec<String>,
    pub checker: Checker,
    /// Stored values, keyed by size text (the empty string for an unsized case).
    pub expect: Vec<(String, String)>,
    pub limits: Limits,
    pub reps: RepPolicy,
    pub fresh_session: bool,
    pub threads: u32,
    /// Whether the engine may memoise (the `memo` directive).
    pub memo: bool,
    pub setup: String,
    pub measure: String,
    pub property: String,
}

/// The size label used in results for an unsized case.
pub const UNSIZED: &str = "-";

impl Case {
    /// Read and parse a case file. The class defaults to the name of the directory holding it.
    pub fn load(path: &Path) -> Result<Case, CaseError> {
        let text = fs::read_to_string(path).map_err(|e| CaseError {
            path: path.to_path_buf(),
            line: 0,
            message: format!("cannot read: {e}"),
        })?;
        Case::parse(path, &text)
    }

    /// Parse case text; `path` names the file for ids, classes and error messages.
    pub fn parse(path: &Path, text: &str) -> Result<Case, CaseError> {
        Parser::new(path).run(text)
    }

    /// The number of measurements (one per size, or one for an unsized case).
    pub fn measurement_count(&self) -> usize {
        self.sizes.len().max(1)
    }

    /// The size label of measurement `index`.
    pub fn size_label(&self, index: usize) -> &str {
        self.sizes.get(index).map(String::as_str).unwrap_or(UNSIZED)
    }

    /// The setup program for measurement `index`: the size binding followed by the setup section.
    pub fn setup_source(&self, index: usize) -> String {
        match (&self.variable, self.sizes.get(index)) {
            (Some(var), Some(size)) => format!("{var} = {size}\n{}", self.setup),
            _ => self.setup.clone(),
        }
    }

    /// The stored value for measurement `index`, with `@file` references resolved relative to the case file.
    pub fn expected(&self, index: usize) -> Result<Option<String>, String> {
        let key = self.sizes.get(index).map(String::as_str).unwrap_or("");
        let Some((_, value)) = self.expect.iter().find(|(k, _)| k == key) else { return Ok(None) };
        match value.strip_prefix('@') {
            Some(file) => {
                let base = self.path.parent().unwrap_or(Path::new("."));
                fs::read_to_string(base.join(file.trim()))
                    .map(|s| Some(s.trim().to_string()))
                    .map_err(|e| format!("cannot read expected value {file}: {e}"))
            }
            None => Ok(Some(value.clone())),
        }
    }
}

/// A case file that does not follow the format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseError {
    pub path: PathBuf,
    /// One-based line number, or 0 for errors about the whole file.
    pub line: usize,
    pub message: String,
}

impl fmt::Display for CaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.path.display(), self.line, self.message)
    }
}

impl std::error::Error for CaseError {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Header,
    Setup,
    Measure,
    Property,
}

struct Parser {
    case: Case,
    line: usize,
    seen_sections: Vec<&'static str>,
}

impl Parser {
    fn new(path: &Path) -> Parser {
        let id = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let class =
            path.parent().and_then(Path::file_name).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        Parser {
            case: Case {
                path: path.to_path_buf(),
                id,
                class,
                title: String::new(),
                stresses: String::new(),
                notes: Vec::new(),
                variable: None,
                sizes: Vec::new(),
                checker: Checker::None,
                expect: Vec::new(),
                limits: Limits::default(),
                reps: RepPolicy::default(),
                fresh_session: false,
                threads: 1,
                memo: true,
                setup: String::new(),
                measure: String::new(),
                property: String::new(),
            },
            line: 0,
            seen_sections: Vec::new(),
        }
    }

    fn err(&self, message: impl Into<String>) -> CaseError {
        CaseError { path: self.case.path.clone(), line: self.line, message: message.into() }
    }

    fn run(mut self, text: &str) -> Result<Case, CaseError> {
        let mut section = Section::Header;
        for (i, raw) in text.lines().enumerate() {
            self.line = i + 1;
            if let Some(directive) = raw.strip_prefix("#@") {
                let directive = directive.trim();
                if let Some(next) = self.section_marker(directive)? {
                    section = next;
                } else {
                    self.directive(directive)?;
                }
                continue;
            }
            match section {
                Section::Header => {
                    let t = raw.trim();
                    if !(t.is_empty() || t.starts_with('#')) {
                        return Err(self.err("code before the first section marker (#@ setup or #@ measure)"));
                    }
                }
                Section::Setup => push_line(&mut self.case.setup, raw),
                Section::Measure => push_line(&mut self.case.measure, raw),
                Section::Property => push_line(&mut self.case.property, raw),
            }
        }
        self.line = 0;
        self.finish()
    }

    fn section_marker(&mut self, directive: &str) -> Result<Option<Section>, CaseError> {
        let (name, section) = match directive {
            "setup" => ("setup", Section::Setup),
            "measure" => ("measure", Section::Measure),
            "property" => ("property", Section::Property),
            _ => return Ok(None),
        };
        if self.seen_sections.contains(&name) {
            return Err(self.err(format!("section {name} appears twice")));
        }
        self.seen_sections.push(name);
        Ok(Some(section))
    }

    fn directive(&mut self, directive: &str) -> Result<(), CaseError> {
        let Some((key, value)) = directive.split_once(':') else {
            return Err(self.err(format!("directive without ':' : {directive}")));
        };
        let (key, value) = (key.trim(), value.trim());
        if let Some(size) = key.strip_prefix("expect") {
            let size = size.trim().to_string();
            if self.case.expect.iter().any(|(k, _)| *k == size) {
                return Err(self.err(format!("two expected values for size '{size}'")));
            }
            self.case.expect.push((size, value.to_string()));
            return Ok(());
        }
        match key {
            "case" if value != self.case.id => {
                return Err(self.err(format!("case id {value} does not match the file name {}", self.case.id)))
            }
            "case" => {}
            "class" if value != self.case.class => {
                return Err(self.err(format!("class {value} does not match the directory {}", self.case.class)))
            }
            "class" => {}
            "title" => self.case.title = value.to_string(),
            "stresses" => self.case.stresses = value.to_string(),
            "note" => self.case.notes.push(value.to_string()),
            "sizes" => self.sizes(value)?,
            "check" => self.case.checker = self.checker(value)?,
            "limits" => self.limits(value)?,
            "reps" => self.reps(value)?,
            "session" => {
                self.case.fresh_session = match value {
                    "fresh" => true,
                    "shared" => false,
                    _ => return Err(self.err(format!("session must be shared or fresh, not {value}"))),
                }
            }
            "memo" => {
                self.case.memo = match value {
                    "on" => true,
                    "off" => false,
                    _ => return Err(self.err(format!("memo must be on or off, not {value}"))),
                }
            }
            "threads" => {
                self.case.threads = value
                    .parse()
                    .ok()
                    .filter(|&t| t > 0)
                    .ok_or_else(|| self.err(format!("bad thread count {value}")))?
            }
            _ => return Err(self.err(format!("unknown directive {key}"))),
        }
        Ok(())
    }

    fn sizes(&mut self, value: &str) -> Result<(), CaseError> {
        let Some((var, list)) = value.split_once('=') else {
            return Err(self.err("sizes must be written `var = s1, s2, ...`"));
        };
        let var = var.trim();
        if var.is_empty() || !var.chars().all(|c| c.is_alphanumeric() || c == '_') || var.ends_with('_') {
            return Err(self.err(format!("bad size variable {var}")));
        }
        let sizes = split_top_level(list);
        if sizes.iter().any(String::is_empty) {
            return Err(self.err("empty size"));
        }
        self.case.variable = Some(var.to_string());
        self.case.sizes = sizes;
        Ok(())
    }

    fn checker(&self, value: &str) -> Result<Checker, CaseError> {
        let words: Vec<&str> = value.split_whitespace().collect();
        Ok(match words.as_slice() {
            ["exact"] => Checker::Exact,
            ["exact", "hash"] => Checker::ExactHash,
            ["property"] => Checker::Property,
            ["none"] => Checker::None,
            ["digits", d] => Checker::Digits(
                d.parse().ok().filter(|&d| d > 0).ok_or_else(|| self.err(format!("bad digit count {d}")))?,
            ),
            _ => return Err(self.err(format!("unknown checker {value}"))),
        })
    }

    fn limits(&mut self, value: &str) -> Result<(), CaseError> {
        for part in value.split(',') {
            let words: Vec<&str> = part.split_whitespace().collect();
            match words.as_slice() {
                ["time", d] => {
                    self.case.limits.time = parse_duration(d).ok_or_else(|| self.err(format!("bad time {d}")))?
                }
                ["memory", m] => {
                    self.case.limits.memory_bytes =
                        parse_bytes(m).ok_or_else(|| self.err(format!("bad memory size {m}")))?
                }
                _ => return Err(self.err(format!("bad limit {}", part.trim()))),
            }
        }
        Ok(())
    }

    fn reps(&mut self, value: &str) -> Result<(), CaseError> {
        let parts: Vec<&str> = value.split(',').map(str::trim).collect();
        let [reps, time] = parts.as_slice() else {
            return Err(self.err("reps must be written `<min reps>, <min time>`"));
        };
        self.case.reps = RepPolicy {
            min_reps: reps
                .parse()
                .ok()
                .filter(|&r| r > 0)
                .ok_or_else(|| self.err(format!("bad repetition count {reps}")))?,
            min_time: parse_duration(time).ok_or_else(|| self.err(format!("bad time {time}")))?,
        };
        Ok(())
    }

    fn finish(self) -> Result<Case, CaseError> {
        let case = &self.case;
        if case.measure.trim().is_empty() {
            return Err(self.err("no #@ measure section"));
        }
        if (case.checker == Checker::Property) != !case.property.trim().is_empty() {
            return Err(self.err("a property section is required by, and only allowed with, check: property"));
        }
        for (size, _) in &case.expect {
            let known = if case.sizes.is_empty() { size.is_empty() } else { case.sizes.contains(size) };
            if !known {
                return Err(self.err(format!("expected value for unknown size '{size}'")));
            }
        }
        Ok(self.case)
    }
}

fn push_line(code: &mut String, line: &str) {
    code.push_str(line);
    code.push('\n');
}

/// Split at commas that are not inside brackets or strings, trimming each part.
fn split_top_level(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let (mut depth, mut in_string, mut escaped) = (0i32, false, false);
    let mut current = String::new();
    for c in s.chars() {
        if in_string {
            in_string = !(c == '"' && !escaped);
            escaped = c == '\\' && !escaped;
        } else {
            match c {
                '"' => in_string = true,
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                ',' if depth == 0 => {
                    parts.push(current.trim().to_string());
                    current.clear();
                    continue;
                }
                _ => {}
            }
        }
        current.push(c);
    }
    parts.push(current.trim().to_string());
    parts
}

/// `500ms`, `2s`, `1.5s`, `3min`.
pub fn parse_duration(s: &str) -> Option<Duration> {
    let (number, scale) = if let Some(n) = s.strip_suffix("ms") {
        (n, 1e-3)
    } else if let Some(n) = s.strip_suffix("min") {
        (n, 60.0)
    } else {
        (s.strip_suffix('s')?, 1.0)
    };
    let v: f64 = number.parse().ok()?;
    (v.is_finite() && v > 0.0).then(|| Duration::from_secs_f64(v * scale))
}

/// `512KB`, `256MB`, `8GB` (binary multiples).
pub fn parse_bytes(s: &str) -> Option<u64> {
    let (number, shift) = if let Some(n) = s.strip_suffix("GB") {
        (n, 30)
    } else if let Some(n) = s.strip_suffix("MB") {
        (n, 20)
    } else {
        (s.strip_suffix("KB")?, 10)
    };
    let v: u64 = number.parse().ok()?;
    v.checked_mul(1 << shift).filter(|&b| b > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIB: &str = "#! mathcas 2\n#@ title: fib\n#@ sizes: n = 20, 10^2, f([1, 2])\n#@ check: exact\n\
#@ expect 20: 6765\n#@ limits: time 2s, memory 256MB\n#@ reps: 3, 500ms\n#@ setup\nrule fib(0) = 0\n\
#@ measure\nfib(n)\n";

    fn path() -> PathBuf {
        PathBuf::from("bench/cases/R/R1.mc")
    }

    #[test]
    fn parses_directives_and_sections() {
        let c = Case::parse(&path(), FIB).unwrap();
        assert_eq!((c.id.as_str(), c.class.as_str(), c.title.as_str()), ("R1", "R", "fib"));
        assert_eq!(c.sizes, ["20", "10^2", "f([1, 2])"]);
        assert_eq!(c.checker, Checker::Exact);
        assert_eq!(c.limits, Limits { time: Duration::from_secs(2), memory_bytes: 256 << 20 });
        assert_eq!(c.reps, RepPolicy { min_reps: 3, min_time: Duration::from_millis(500) });
        assert_eq!(c.setup_source(1), "n = 10^2\nrule fib(0) = 0\n");
        assert_eq!(c.measure, "fib(n)\n");
        assert_eq!(c.expected(0), Ok(Some("6765".into())));
        assert_eq!(c.expected(1), Ok(None));
        assert_eq!(c.size_label(2), "f([1, 2])");
    }

    #[test]
    fn unsized_case() {
        let c = Case::parse(&path(), "#@ check: exact\n#@ expect: 4\n#@ measure\n2+2\n").unwrap();
        assert_eq!((c.measurement_count(), c.size_label(0)), (1, UNSIZED));
        assert!(c.memo);
        assert!(!Case::parse(&path(), "#@ memo: off\n#@ measure\n2+2\n").unwrap().memo);
        assert_eq!(c.expected(0), Ok(Some("4".into())));
    }

    #[test]
    fn errors_name_the_line() {
        let cases = [
            ("#@ measure\n1\n#@ bogus: 1\n", 3, "unknown directive"),
            ("#@ memo: maybe\n#@ measure\n1\n", 1, "memo must be on or off"),
            ("1 + 1\n#@ measure\n1\n", 1, "code before"),
            ("#@ case: R2\n#@ measure\n1\n", 1, "does not match"),
            ("#@ check: property\n#@ measure\n1\n", 0, "property section"),
            ("#@ sizes: n = 1\n#@ expect 2: 3\n#@ measure\n1\n", 0, "unknown size"),
            ("#@ limits: time 0s\n#@ measure\n1\n", 1, "bad time"),
            ("#@ setup\n#@ setup\n", 2, "twice"),
        ];
        for (text, line, needle) in cases {
            let e = Case::parse(&path(), text).unwrap_err();
            assert_eq!(e.line, line, "{text}");
            assert!(e.message.contains(needle), "{} lacks {needle}", e.message);
        }
    }

    #[test]
    fn units() {
        assert_eq!(parse_duration("1.5s"), Some(Duration::from_millis(1500)));
        assert_eq!(parse_duration("2min"), Some(Duration::from_secs(120)));
        assert_eq!(parse_duration("20"), None);
        assert_eq!(parse_bytes("8GB"), Some(8 << 30));
        assert_eq!(parse_bytes("8"), None);
    }
}
