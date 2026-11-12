//! An engine session (with this crate installed by `mc-engine`) driven by source text, and parsers for the printed
//! results: rules, machine reals, complex numbers and ball digits.
#![allow(dead_code)]
use mc_api::{Engine, EngineError, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use std::time::{Duration, Instant};

pub struct Sys {
    engine: CoreEngine,
    stop: StopFlag,
}

impl Sys {
    pub fn new() -> Sys {
        Sys::with_threads(1, false)
    }

    /// An engine with `threads` threads, under the stress collector if `stress`.
    pub fn with_threads(threads: usize, stress: bool) -> Sys {
        let mut cfg = EngineConfig { threads, ..EngineConfig::default() };
        if stress {
            cfg.heap = mc_core::Config::stress();
        }
        Sys { engine: CoreEngine::with_config(cfg), stop: StopFlag::new() }
    }

    /// Evaluate `src`; the printed value of its last statement.
    pub fn run(&mut self, src: &str) -> String {
        match self.engine.eval(src, &self.stop) {
            Ok(e) => e.printed,
            Err(err) => panic!("{src}: {err}"),
        }
    }

    /// Evaluate `src`, raising the stop flag from another thread after `after`; the result and the time it took.
    pub fn interrupted(&mut self, src: &str, after: Duration) -> (Result<String, EngineError>, Duration) {
        let stop = self.stop.clone();
        let start = Instant::now();
        let t = std::thread::spawn(move || {
            while start.elapsed() < after {
                std::thread::yield_now();
            }
            stop.raise();
        });
        let r = self.engine.eval(src, &self.stop).map(|e| e.printed);
        let took = start.elapsed();
        t.join().unwrap();
        self.stop.clear();
        (r, took)
    }

    /// The values of the rules `[x -> v, ...]` that `src` prints, as text.
    pub fn rules(&mut self, src: &str) -> Vec<String> {
        let out = self.run(src);
        rule_values(&out).unwrap_or_else(|| panic!("{src}: not a list of rules: {out}"))
    }

    /// The machine values of the rules `src` prints.
    pub fn reals(&mut self, src: &str) -> Vec<f64> {
        self.rules(src).iter().map(|v| number(v)).collect()
    }
}

/// Evaluate each `(source, printed result)` pair in one session.
pub fn check(cases: &[(&str, &str)]) {
    let mut s = Sys::new();
    for (src, want) in cases {
        assert_eq!(s.run(src), *want, "{src}");
    }
}

/// The right sides of a printed list of rules `[x -> a, y -> b]` (values without nested commas, or complex
/// numbers `complex(a, b)`).
pub fn rule_values(s: &str) -> Option<Vec<String>> {
    let inner = s.strip_prefix('[')?.strip_suffix(']')?;
    let mut out = Vec::new();
    let mut rest = inner;
    while !rest.is_empty() {
        let (_, r) = rest.split_once(" -> ")?;
        let end = if r.starts_with("complex(") { r.find(')')? + 1 } else { r.find(", ").unwrap_or(r.len()) };
        out.push(r[..end].to_string());
        rest = r[end..].trim_start_matches(", ");
    }
    Some(out)
}

/// Parse a printed number (machine or ball digits).
pub fn number(s: &str) -> f64 {
    s.trim_end_matches('.').parse::<f64>().unwrap_or_else(|_| panic!("not a number: {s}"))
}

/// Parse a printed complex number `complex(a, b)`.
pub fn complex(s: &str) -> (String, String) {
    let inner = s.strip_prefix("complex(").and_then(|r| r.strip_suffix(')')).unwrap_or_else(|| panic!("{s}"));
    let (a, b) = inner.split_once(", ").unwrap();
    (a.to_string(), b.to_string())
}

/// Is `x` within `rel` relative (or absolute, near zero) error of `want`?
pub fn close(x: f64, want: f64, rel: f64) -> bool {
    (x - want).abs() <= rel * want.abs().max(1.0)
}

/// The number of leading significant digits in which the printed decimal `got` agrees with the expected `want`
/// (both plain decimals with the same sign and exponent, such as `1.4142...`).
pub fn agreeing_digits(got: &str, want: &str) -> usize {
    let digits = |s: &str| s.chars().filter(|c| c.is_ascii_digit()).collect::<Vec<_>>();
    let (g, w) = (digits(got), digits(want));
    // skip leading zeros (of 0.567...) in both
    let lead = g.iter().take_while(|&&c| c == '0').count();
    g[lead..].iter().zip(&w[lead..]).take_while(|(a, b)| a == b).count()
}

/// The number of significant digits printed.
pub fn significant(got: &str) -> usize {
    let d: Vec<char> = got.chars().filter(|c| c.is_ascii_digit()).collect();
    d.len() - d.iter().take_while(|&&c| c == '0').count()
}

/// A printed ball agrees with the expected value in all its printed digits: they are that value rounded or
/// truncated to as many significant digits.
pub fn correct_digits(got: &str, want: &str) -> bool {
    let digits = |s: &str| s.chars().filter(|c| c.is_ascii_digit()).map(|c| c as u8 - b'0').collect::<Vec<u8>>();
    let (g, w) = (digits(got), digits(want));
    // leading zeros (of 0.567...) are not significant; both have the same ones
    let lead = g.iter().take_while(|&&c| c == 0).count();
    let n = g.len();
    if w.len() < n || w[..lead].iter().any(|&c| c != 0) {
        return false;
    }
    let truncated = w[..n].to_vec();
    let mut rounded = truncated.clone();
    if w.get(n).is_some_and(|&c| c >= 5) {
        // add one unit in the last place, with carries (a carry out of the first digit cannot match anyway)
        for d in rounded.iter_mut().rev() {
            if *d == 9 {
                *d = 0;
            } else {
                *d += 1;
                break;
            }
        }
    }
    g == truncated || g == rounded
}
