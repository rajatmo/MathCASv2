//! Exact answers against independent checks, second pass. Four generators with fixed seeds make random inputs and
//! check each answer by another route:
//!
//! 1. **linear algebra**: decompositions multiplied back (`lu`, `qr`, `cholesky`, `svd`), eigen systems put back
//!    into `m v = lambda v`, `linear_solve`, `null_space`, `inverse`, `matrix_exp` and `matrix_function` against
//!    their power series, determinants against the Leibniz sum and ranks against the null space, for exact,
//!    symbolic, machine, 30-digit, sparse, interval and ill-conditioned matrices;
//! 2. **algebra and number theory**: `factor` multiplied back (and no fewer factors than the product was made of),
//!    gcds dividing, resultants against the roots, Groebner bases against the generators and their common zeros,
//!    `solve`, `reduce`, `find_instance` and `n_solve` answers put back, modular arithmetic, `factor_integer`,
//!    primality of products and pseudoprimes, continued fractions, linear Diophantine equations;
//! 3. **lists, associations, strings, statistics**: the answer against the same operation done here in Rust on the
//!    same random data (sorting, stable sorting, set operations, spans and part assignments, grouping and merging,
//!    tallies, string operations; means, variances, medians, quantiles, moments in exact rationals);
//! 4. **rewriting**: `simplify`, `full_simplify`, `refine`, `function_expand`, `together`, `apart`, `expand`,
//!    `factor` keep the value: input and result agree at random exact points of the domain the assumptions state.
//!
//! The suite runs a short version of each (`*_short`); the `campaign_*` tests (ignored) are the long runs by hand:
//! `MC_SOUNDNESS_INPUTS` inputs per generator (default 400), `MC_SOUNDNESS_SECONDS` a time cap (default 1200),
//! `MC_SOUNDNESS_SEED`, `MC_SOUNDNESS_LOG` (a folder for a log written as it grows), `MC_STATUS_DIR` (progress).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------------------------------------------
// common

/// A small deterministic generator (splitmix64).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a, T>(&mut self, s: &'a [T]) -> &'a T {
        &s[self.below(s.len())]
    }
    /// An integer in [lo, hi].
    fn int(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
}

thread_local! {
    /// The log file of the running generator (see [`drive`]).
    static TRACE: std::cell::RefCell<Option<std::fs::File>> = const { std::cell::RefCell::new(None) };
}

/// A line into the log of the running generator, if any.
fn trace(line: &str) {
    TRACE.with(|t| {
        if let Some(f) = t.borrow_mut().as_mut() {
            use std::io::Write;
            let _ = writeln!(f, "{line}");
            let _ = f.flush();
        }
    });
}

thread_local! {
    /// The slowest evaluation of the running input: its time and its source.
    static SLOWEST: std::cell::RefCell<(Duration, String)> = const { std::cell::RefCell::new((Duration::ZERO, String::new())) };
}

fn ev(e: &mut CoreEngine, src: &str) -> String {
    trace(&format!("> {src}"));
    let t0 = Instant::now();
    let out = e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"));
    let dt = t0.elapsed();
    trace(&format!("< {} ms", dt.as_millis()));
    SLOWEST.with(|s| {
        let mut s = s.borrow_mut();
        if dt > s.0 {
            *s = (dt, src.chars().take(300).collect());
        }
    });
    out
}

/// An evaluation over the time bound of the inputs is a fault: every `time_constrained` here asks for at most 60 s,
/// so an evaluation that takes far longer did not stop when asked.
const SLOW: Duration = Duration::from_secs(90);

/// `src` evaluated within `secs` seconds (`$aborted` after).
fn evt(e: &mut CoreEngine, src: &str, secs: u32) -> String {
    ev(e, &format!("time_constrained({{ {src} }}, {secs})"))
}

/// A printed real number (`1.25`, `-3`, `2/3`, `1.5*10^-7`, with a precision mark), or `None`.
fn real_of(s: &str) -> Option<f64> {
    let s = s.trim();
    let s = s.split('`').next().unwrap_or(s);
    if let Some((m, x)) = s.split_once("*10^") {
        let m: f64 = real_of(m)?;
        let x: f64 = x.trim_matches(|c| c == '(' || c == ')').parse().ok()?;
        return Some(m * 10f64.powf(x));
    }
    if let Some((p, q)) = s.split_once('/') {
        return Some(p.trim().parse::<f64>().ok()? / q.trim().parse::<f64>().ok()?);
    }
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// An answer that promises nothing (unevaluated, a failure, out of time): not checked.
fn gave_up(ans: &str, head: &str) -> bool {
    ans.starts_with(&format!("{head}(")) || ans.starts_with("failure") || ans.contains("$aborted") || ans.starts_with("error")
}

/// The settings of a run.
struct Run {
    inputs: usize,
    cap: Duration,
    seed: u64,
    job: &'static str,
    verbose: bool,
}

impl Run {
    fn short(job: &'static str, inputs: usize) -> Run {
        Run { inputs, cap: Duration::from_secs(200), seed: 45, job, verbose: false }
    }
    fn long(job: &'static str) -> Run {
        let var = |k: &str, d: u64| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        Run {
            inputs: var("MC_SOUNDNESS_INPUTS", 400) as usize,
            cap: Duration::from_secs(var("MC_SOUNDNESS_SECONDS", 1200)),
            seed: var("MC_SOUNDNESS_SEED", 4500),
            job,
            verbose: true,
        }
    }
    /// Progress: printed every 20 inputs and written to the status folder.
    fn progress(&self, done: usize, faults: usize, start: Instant) {
        if !self.verbose || (!done.is_multiple_of(20) && done != self.inputs) {
            return;
        }
        let pct = 100 * done / self.inputs.max(1);
        eprintln!("[{}] {done}/{} ({pct}%), {faults} faults, {:.0} s", self.job, self.inputs, start.elapsed().as_secs_f64());
        if let Ok(dir) = std::env::var("MC_STATUS_DIR") {
            let _ = std::fs::write(format!("{dir}/soundness2-{}.txt", self.job), format!("{done}/{} {pct}%\n", self.inputs));
        }
    }
}

/// One input of a generator: `Some(fault)` for a contradiction; every query and answer goes to the log.
type Input = fn(&mut CoreEngine, &mut Rng, &mut Vec<String>) -> Option<String>;

/// Runs a generator: `run.inputs` inputs (or until the time cap) in one session, with progress; the log goes to
/// `MC_SOUNDNESS_LOG/<job>.log` when that is set.
fn drive(run: &Run, salt: u64, input: Input) -> (usize, Vec<String>) {
    let mut e = CoreEngine::new();
    let mut r = Rng(run.seed ^ salt);
    let (start, mut faults, mut log) = (Instant::now(), vec![], vec![]);
    if run.verbose {
        if let Ok(dir) = std::env::var("MC_SOUNDNESS_LOG") {
            let file = std::fs::File::create(format!("{dir}/{}.log", run.job)).ok();
            TRACE.with(|t| *t.borrow_mut() = file);
        }
    }
    let mut done = 0;
    while done < run.inputs && start.elapsed() < run.cap {
        SLOWEST.with(|s| *s.borrow_mut() = (Duration::ZERO, String::new()));
        if let Some(f) = input(&mut e, &mut r, &mut log) {
            eprintln!("FAULT {f}");
            faults.push(f);
        }
        let (dt, src) = SLOWEST.with(|s| s.borrow().clone());
        if dt > SLOW {
            let f = format!("{src}: {:.0} s, beyond its time bound", dt.as_secs_f64());
            eprintln!("FAULT {f}");
            faults.push(f);
        }
        for line in log.drain(..) {
            trace(&line);
        }
        done += 1;
        run.progress(done, faults.len(), start);
    }
    TRACE.with(|t| *t.borrow_mut() = None);
    (done, faults)
}

fn report(name: &str, (done, faults): (usize, Vec<String>)) {
    eprintln!("{name}: {done} inputs, {} faults", faults.len());
    assert!(faults.is_empty(), "{name}: {} faults\n{}", faults.len(), faults.join("\n"));
}

/// Joins printed items with `, ` inside brackets.
fn list(items: impl IntoIterator<Item = String>) -> String {
    format!("[{}]", items.into_iter().collect::<Vec<_>>().join(", "))
}

fn ints(xs: &[i64]) -> String {
    list(xs.iter().map(|x| x.to_string()))
}

/// Is the printed answer `true`? Anything else (`false`, unevaluated, out of time) is not.
fn is_true(ans: &str) -> bool {
    ans == "true"
}

// ---------------------------------------------------------------------------------------------------------------
// 1. linear algebra

/// The kinds of matrices.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Integer,
    Rational,
    /// one entry is the symbol `a`; the answers are checked at a random value of it
    Symbolic,
    Machine,
    Digits30,
    Sparse,
    /// the Hilbert matrix (exact, machine or at 30 digits)
    Hilbert,
    /// complex Gaussian-integer entries
    Complex,
    /// the last row a combination of the others
    Singular,
}

/// A random square matrix of `n` rows: its entries as printed exact numbers (`a` for the symbolic one).
fn matrix_entries(r: &mut Rng, kind: Kind, n: usize) -> Vec<Vec<String>> {
    let mut m: Vec<Vec<String>> = (0..n)
        .map(|i| {
            (0..n)
                .map(|j| match kind {
                    Kind::Hilbert => format!("1/{}", i + j + 1),
                    Kind::Rational => {
                        let q = r.int(1, 5);
                        if q == 1 { r.int(-4, 4).to_string() } else { format!("{}/{q}", r.int(-7, 7)) }
                    }
                    Kind::Complex => format!("{} + ({})*im", r.int(-3, 3), r.int(-3, 3)),
                    _ => r.int(-4, 4).to_string(),
                })
                .collect()
        })
        .collect();
    if kind == Kind::Symbolic {
        let (i, j) = (r.below(n), r.below(n));
        m[i][j] = if r.below(2) == 0 { "a".into() } else { format!("a + {}", r.int(-2, 2)) };
    }
    if kind == Kind::Singular && n > 1 {
        let (c1, c2) = (r.int(-2, 2), r.int(-2, 2));
        let last: Vec<String> = (0..n).map(|j| format!("{c1}*({}) + {c2}*({})", m[0][j], m[1 % (n - 1)][j])).collect();
        m[n - 1] = last;
    }
    m
}

fn matrix_src(m: &[Vec<String>]) -> String {
    list(m.iter().map(|row| list(row.iter().cloned())))
}

/// The Leibniz sum of the determinant of the matrix `m` of the session (`n` rows), as a source.
fn leibniz(n: usize) -> String {
    fn perms(n: usize) -> Vec<Vec<usize>> {
        if n == 0 {
            return vec![vec![]];
        }
        let mut out = vec![];
        for p in perms(n - 1) {
            for k in 0..n {
                let mut q = p.clone();
                q.insert(k, n - 1);
                out.push(q);
            }
        }
        out
    }
    let terms: Vec<String> = perms(n)
        .into_iter()
        .map(|p| {
            let inversions = (0..n).flat_map(|i| (i + 1..n).map(move |j| (i, j))).filter(|&(i, j)| p[i] > p[j]).count();
            let sign = if inversions % 2 == 0 { "" } else { "-" };
            let prod: Vec<String> = (0..n).map(|i| format!("m[{}, {}]", i + 1, p[i] + 1)).collect();
            format!("{sign}{}", prod.join("*"))
        })
        .collect();
    format!("({})", terms.join(" + "))
}

/// A matrix check: the relative distance `dist(a, b)` of two arrays (scalars, vectors, matrices) at 50 digits is
/// below the tolerance `tol` of the kind of matrix; `m` is the matrix of the input (its size sets the scale).
const LINALG_SETUP: &str = "alltrue = l => all_true(l, t => t === true); num = z => { w = if precision(z) === inf then n(z, 50) else z; if is_failure(w) then 0*z else w }; dist = (a, b) => max(join([0], map(z => if z < tol then 0 else z, abs(map(num, flatten([a - b]))))))/(1 + n(max(abs(flatten([m]))), 50)); \
    upper = u => alltrue(flatten(table(if i > j then u[i, j] == 0 else true, [i, 1, length(u)], [j, 1, length(u[1])]))); \
    lower = u => alltrue(flatten(table(if i < j then u[i, j] == 0 else true, [i, 1, length(u)], [j, 1, length(u[1])]))); 0";

fn linalg_input(e: &mut CoreEngine, r: &mut Rng, log: &mut Vec<String>) -> Option<String> {
    ev(e, LINALG_SETUP);
    let kind = *r.pick(&[
        Kind::Integer, Kind::Integer, Kind::Rational, Kind::Symbolic, Kind::Machine, Kind::Digits30, Kind::Sparse,
        Kind::Hilbert, Kind::Singular, Kind::Complex, Kind::Integer,
    ]);
    let n = r.int(2, 4) as usize;
    let entries = matrix_entries(r, kind, n);
    let exact = matrix_src(&entries);
    // the numbers of the kind: machine, 30 digits, exact; the Hilbert matrix in any of them
    let numeric = match kind {
        Kind::Machine => 1,
        Kind::Digits30 => 2,
        Kind::Hilbert => r.below(3),
        _ => 0,
    };
    let m = match numeric {
        1 => format!("n({exact})"),
        2 => format!("n({exact}, 30)"),
        _ if kind == Kind::Sparse => format!("sparse_array({exact})"),
        _ => exact.clone(),
    };
    // tolerances: machine results relative to the scale (the Hilbert matrix is ill-conditioned: its inverse and
    // solutions lose about 1.5 n digits each), 30-digit results, exact results evaluated at 50 digits
    let ill = if kind == Kind::Hilbert { 2 * n as i32 } else { 0 };
    let tol = match numeric {
        1 => format!("10^-{}", 10 - ill.min(6)),
        2 => format!("10^-{}", 22 - ill),
        _ => "10^-40".to_string(),
    };
    let av = format!("{}{}/{}", if r.below(2) == 0 { "-" } else { "" }, r.int(1, 9), r.int(11, 23));
    let at = |s: &str| if kind == Kind::Symbolic { format!("({s}) /. a -> {av}") } else { s.to_string() };
    let x0: Vec<String> = (0..n).map(|_| r.int(-3, 3).to_string()).collect();
    let x0 = list(x0);
    // (operation, check); `f` is the answer, `m` the matrix (both with `a` replaced in the check)
    let (op, check): (String, String) = match r.below(19) {
        12 => (
            "pseudo_inverse(m)".into(),
            "dist(dot(m, f, m), m) < tol and dist(dot(f, m, f), f) < tol and dist(conjugate_transpose(dot(m, f)), dot(m, f)) < tol and dist(conjugate_transpose(dot(f, m)), dot(f, m)) < tol".into(),
        ),
        13 => (
            "characteristic_polynomial(m, x)".into(),
            format!("mm = m; m = x*identity_matrix(length(m)) - m; c = expand({}); m = mm; alltrue(table(dist(coefficient(f, x, k), coefficient(c, x, k)) < tol, [k, 0, length(m)]))", leibniz(n)),
        ),
        14 => (
            "matrix_minimal_polynomial(m, x)".into(),
            "cl = coefficient_list(f, x); dist(sum(cl[k + 1]*matrix_power(m, k), [k, 0, length(cl) - 1]), 0*m) < tol and (precision(f) < inf or polynomial_remainder(characteristic_polynomial(m, x), f, x) == 0)".into(),
        ),
        15 => (
            "row_reduce(m)".into(),
            "rank(f) == rank(m) and alltrue(map(v => dist(dot(f, v), 0*v) < tol, null_space(m)))".into(),
        ),
        16 if kind != Kind::Symbolic => (
            "matrix_log(dot(conjugate_transpose(m), m) + identity_matrix(length(m)))".into(),
            "b = dot(conjugate_transpose(m), m) + identity_matrix(length(m)); g = num(f); dist(sum(matrix_power(g, k)/factorial(k), [k, 0, 120]), b) < tol".into(),
        ),
        17 if kind != Kind::Symbolic => (
            "matrix_function(sqrt, dot(conjugate_transpose(m), m) + identity_matrix(length(m)))".into(),
            "b = dot(conjugate_transpose(m), m) + identity_matrix(length(m)); dist(dot(f, f), b) < tol".into(),
        ),
        18 if kind != Kind::Symbolic && kind != Kind::Complex && numeric == 0 => {
            // a matrix of intervals of width 1/500 around the entries: its determinant holds those of sampled
            // matrices
            let samples: Vec<String> = (0..6)
                .map(|_| format!("det(m + {})", list((0..n).map(|_| list((0..n).map(|_| format!("{}/1000", r.int(-1, 1))))))))
                .collect();
            (
                "det(map(row => map(t => t + interval([-1/1000, 1/1000]), row), m))".to_string(),
                format!("alltrue(map(v => is_interval_member(f, v), {}))", list(samples)),
            )
        }
        0 => ("lu(m)".into(), "dist(dot(f[1], f[2]), m[f[3]]) < tol and lower(f[1]) and upper(f[2])".into()),
        1 => (
            "qr(m)".into(),
            "dist(dot(f[1], f[2]), m) < tol and dist(dot(conjugate_transpose(f[1]), f[1]), identity_matrix(length(f[2]))) < tol and upper(f[2])".into(),
        ),
        2 => (
            "cholesky(dot(conjugate_transpose(m), m) + identity_matrix(length(m)))".into(),
            "b = dot(conjugate_transpose(m), m) + identity_matrix(length(m)); dist(dot(conjugate_transpose(f), f), b) < tol and upper(f)".into(),
        ),
        3 if numeric > 0 => (
            "svd(m)".into(),
            "s = table(f[2][k, k], [k, 1, length(m)]); dist(dot(f[1], f[2], conjugate_transpose(f[3])), m) < tol and alltrue(table(s[k] >= s[k + 1] - tol, [k, 1, length(s) - 1])) and min(s) >= -tol".into(),
        ),
        4 => (
            "eigensystem(m)".into(),
            format!(
                "length(f[1]) == length(m) and alltrue(table(dist(dot(m, f[2][k]), f[1][k]*f[2][k]) < tol, [k, 1, length(m)])) and dist(total(f[1]), trace(m)) < tol and dist(apply(mul, f[1]), {}) < tol",
                leibniz(n)
            ),
        ),
        5 => (format!("linear_solve(m, dot(m, {x0}))"), format!("dist(dot(m, f), dot(m, {x0})) < tol")),
        6 if kind != Kind::Symbolic => (
            "null_space(m)".into(),
            "alltrue(map(v => dist(dot(m, v), 0*v) < tol, f)) and length(f) + rank(m) == length(m) and (f == [] or rank(f) == length(f))".into(),
        ),
        7 => ("det(m)".into(), format!("dist(f, {}) < tol", leibniz(n))),
        8 => ("inverse(m)".into(), "dist(dot(f, m), identity_matrix(length(m))) < tol".into()),
        9 => (
            "matrix_exp(m/2)".into(),
            "dist(f, sum(matrix_power(m/2, k)/factorial(k), [k, 0, 90])) < tol".into(),
        ),
        10 => {
            let (fun, series) = *r.pick(&[
                ("sin", "(-1)^k*matrix_power(m/3, 2*k + 1)/factorial(2*k + 1)"),
                ("cos", "(-1)^k*matrix_power(m/3, 2*k)/factorial(2*k)"),
                ("exp", "matrix_power(m/3, k)/factorial(k)"),
            ]);
            (format!("matrix_function({fun}, m/3)"), format!("dist(f, sum({series}, [k, 0, 90])) < tol"))
        }
        _ if kind != Kind::Symbolic => (
            "rank(m)".into(),
            "f == rank(transpose(m)) and f == length(m) - length(null_space(m))".into(),
        ),
        // a symbolic matrix: rank and null space are generic answers, the determinant is not
        _ => ("det(m)".into(), format!("dist(f, {}) < tol", leibniz(n))),
    };
    let q = format!("m = {m}; {op}");
    let ans = evt(e, &format!("m = {m}; f = {op}; if leaf_count(f) > 20000 then large_answer else f"), 30);
    log.push(format!("{q} = {ans}"));
    let head = op.split('(').next().unwrap_or("");
    if gave_up(&ans, head) || ans.contains(&format!("{head}(")) || ans == "large_answer" {
        return None;
    }
    // the singular matrix: no inverse
    let singular = kind == Kind::Singular;
    if singular && head == "inverse" && numeric == 0 {
        return Some(format!("{q} = {ans}, an inverse of a singular matrix"));
    }
    // a 30-digit matrix whose answer came back in machine numbers (svd and the eigen systems of nonsymmetric
    // matrices) is checked at machine tolerance: it claims no more
    let tol = if numeric == 2 { format!("if precision(f) < 20 then 10^-{} else {tol}", 10 - ill.min(6)) } else { tol };
    let checked = evt(e, &format!("tol = {tol}; m = {}; f = {}; {{ {check} }} === true", at(&m), at("f")), 60);
    log.push(format!("check: {checked}"));
    if is_true(&checked) {
        return None;
    }
    Some(format!("{q} = {ans}: check {check} is {checked} (kind {kind:?}, a = {av})"))
}

// ---------------------------------------------------------------------------------------------------------------
// exact rationals (for the expected statistics)

#[derive(Clone, Copy, PartialEq, Debug)]
struct Q(i128, i128);

fn gcd(a: i128, b: i128) -> i128 {
    if b == 0 { a.abs() } else { gcd(b, a % b) }
}

impl Q {
    fn new(p: i128, q: i128) -> Q {
        let g = gcd(p, q).max(1);
        let s = if q < 0 { -1 } else { 1 };
        Q(s * p / g, s * q / g)
    }
    fn int(p: i64) -> Q {
        Q(p as i128, 1)
    }
    fn add(self, o: Q) -> Q {
        Q::new(self.0 * o.1 + o.0 * self.1, self.1 * o.1)
    }
    fn sub(self, o: Q) -> Q {
        self.add(Q(-o.0, o.1))
    }
    fn mul(self, o: Q) -> Q {
        Q::new(self.0 * o.0, self.1 * o.1)
    }
    fn div(self, o: Q) -> Q {
        Q::new(self.0 * o.1, self.1 * o.0)
    }
    fn pow(self, k: u32) -> Q {
        (0..k).fold(Q(1, 1), |a, _| a.mul(self))
    }
    fn printed(self) -> String {
        if self.1 == 1 { self.0.to_string() } else { format!("{}/{}", self.0, self.1) }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// 3. lists, associations, strings, statistics

/// A random list of small integers with repetitions.
fn small_list(r: &mut Rng, max_len: usize) -> Vec<i64> {
    let n = r.below(max_len + 1);
    let top = r.int(1, 6);
    (0..n).map(|_| r.int(-1, top)).collect()
}

fn dedup_first(xs: &[i64]) -> Vec<i64> {
    let mut out = vec![];
    for &x in xs {
        if !out.contains(&x) {
            out.push(x);
        }
    }
    out
}

fn sorted_unique(xs: &[i64]) -> Vec<i64> {
    let mut v = dedup_first(xs);
    v.sort();
    v
}

/// The elements of `xs` from `i` to `j` in steps of `s` (1-based), as a span gives them.
fn span(xs: &[i64], i: i64, j: i64, s: i64) -> Vec<i64> {
    let mut out = vec![];
    let mut k = i;
    while (s > 0 && k <= j) || (s < 0 && k >= j) {
        out.push(xs[(k - 1) as usize]);
        k += s;
    }
    out
}

/// An association as printed, its keys in order of first appearance (a later value replaces an earlier one).
fn show_assoc(v: &[(String, String)]) -> String {
    format!("assoc({})", v.iter().map(|(k, x)| format!("{k} -> {x}")).collect::<Vec<_>>().join(", "))
}

/// One list operation: the source and the expected printed answer.
fn list_case(r: &mut Rng) -> (String, String) {
    let xs = small_list(r, 12);
    let ys = small_list(r, 8);
    let n = xs.len() as i64;
    let (sx, sy) = (ints(&xs), ints(&ys));
    let count = |x: i64| xs.iter().filter(|&&y| y == x).count();
    match r.below(22) {
        0 => {
            let mut v = xs.clone();
            v.sort();
            (format!("sort({sx})"), ints(&v))
        }
        1 => {
            let mut v = xs.clone();
            v.extend(&ys);
            (format!("union({sx}, {sy})"), ints(&sorted_unique(&v)))
        }
        2 => {
            let v: Vec<i64> = sorted_unique(&xs).into_iter().filter(|x| ys.contains(x)).collect();
            (format!("intersection({sx}, {sy})"), ints(&v))
        }
        3 => {
            let v: Vec<i64> = sorted_unique(&xs).into_iter().filter(|x| !ys.contains(x)).collect();
            (format!("complement({sx}, {sy})"), ints(&v))
        }
        4 => (format!("unique({sx})"), ints(&dedup_first(&xs))),
        5 => (format!("tally({sx})"), list(dedup_first(&xs).into_iter().map(|x| format!("[{x}, {}]", count(x))))),
        6 => {
            let v: Vec<(String, String)> = dedup_first(&xs).into_iter().map(|x| (x.to_string(), count(x).to_string())).collect();
            (format!("counts({sx})"), show_assoc(&v))
        }
        7 => {
            let v = dedup_first(&xs).into_iter().map(|x| ints(&xs.iter().copied().filter(|&y| y == x).collect::<Vec<_>>()));
            (format!("gather({sx})"), list(v))
        }
        8 => {
            let mut runs: Vec<Vec<i64>> = vec![];
            for &x in &xs {
                match runs.last_mut() {
                    Some(run) if run[0] == x => run.push(x),
                    _ => runs.push(vec![x]),
                }
            }
            (format!("split({sx})"), list(runs.iter().map(|v| ints(v))))
        }
        9 => {
            let m = r.int(2, 3);
            let key = |x: i64| x.rem_euclid(m);
            let keys = dedup_first(&xs.iter().map(|&x| key(x)).collect::<Vec<_>>());
            let v: Vec<(String, String)> =
                keys.iter().map(|&k| (k.to_string(), ints(&xs.iter().copied().filter(|&x| key(x) == k).collect::<Vec<_>>()))).collect();
            (format!("group_by({sx}, x => mod(x, {m}))"), show_assoc(&v))
        }
        10 => {
            // pairs [key, tag] with the tags in random order: equal keys keep the canonical order of the elements
            let mut tags: Vec<i64> = (1..=n).collect();
            for k in (1..tags.len()).rev() {
                tags.swap(k, r.below(k + 1));
            }
            let pairs: Vec<(i64, i64)> = xs.iter().copied().zip(tags).collect();
            let show = |v: &[(i64, i64)]| list(v.iter().map(|(a, b)| format!("[{a}, {b}]")));
            let mut sorted = pairs.clone();
            sorted.sort();
            (format!("sort_by({}, p => p[1])", show(&pairs)), show(&sorted))
        }
        11 => {
            let mut idx: Vec<usize> = (0..xs.len()).collect();
            idx.sort_by_key(|&k| xs[k]);
            (format!("ordering({sx})"), ints(&idx.iter().map(|&k| k as i64 + 1).collect::<Vec<_>>()))
        }
        12 if n > 0 => {
            let k = r.int(-n, n);
            let (v, w) = if k >= 0 {
                (xs[..k as usize].to_vec(), xs[k as usize..].to_vec())
            } else {
                (xs[(n + k) as usize..].to_vec(), xs[..(n + k) as usize].to_vec())
            };
            (format!("[take({sx}, {k}), drop({sx}, {k})]"), list([ints(&v), ints(&w)]))
        }
        13 if n > 0 => {
            // a span i:j or span(i, j, s), each end written from the front or from the back
            let s = *r.pick(&[1i64, 1, 2, 3, -1, -2]);
            let a = r.int(1, n);
            let b = if s > 0 { r.int(a, n) } else { r.int(1, a) };
            let mut side = |k: i64| if r.below(2) == 0 { k.to_string() } else { (k - n - 1).to_string() };
            let (pa, pb) = (side(a), side(b));
            let src = if s == 1 { format!("{sx}[{pa}:{pb}]") } else { format!("part({sx}, span({pa}, {pb}, {s}))") };
            (src, ints(&span(&xs, a, b, s)))
        }
        14 if n > 1 => {
            // part assignments: a list of positions, a span set to one value
            let (i, j) = (r.int(1, n), r.int(1, n));
            let mut v = xs.clone();
            if i != j && r.below(2) == 0 {
                v[(i - 1) as usize] = 7;
                v[(j - 1) as usize] = 8;
                (format!("l = {sx}; l[[{i}, {j}]] = [7, 8]; l"), ints(&v))
            } else {
                let (a, b) = (i.min(j), i.max(j));
                for k in a..=b {
                    v[(k - 1) as usize] = 0;
                }
                (format!("l = {sx}; l[{a}:{b}] = 0; l"), ints(&v))
            }
        }
        15 if n > 0 => {
            let k = r.int(-2 * n, 2 * n);
            let mut v = xs.clone();
            v.rotate_left(k.rem_euclid(n) as usize);
            (format!("rotate_left({sx}, {k})"), ints(&v))
        }
        16 => {
            let k = r.int(1, 3) as usize;
            let d = r.int(1, 3) as usize;
            let mut out = vec![];
            let mut s = 0;
            while s + k <= xs.len() {
                out.push(ints(&xs[s..s + k]));
                s += d;
            }
            (format!("partition({sx}, {k}, {d})"), list(out))
        }
        17 => {
            let mut acc = 0;
            let v: Vec<i64> = xs.iter().map(|x| {
                acc += x;
                acc
            })
            .collect();
            let d: Vec<i64> = xs.windows(2).map(|w| w[1] - w[0]).collect();
            (format!("[accumulate({sx}), differences({sx})]"), list([ints(&v), ints(&d)]))
        }
        18 => {
            // merge of two associations with total
            let keys = ["a", "b", "c", "d", "e"];
            let to_assoc = |v: &[i64], scale: i64| {
                let mut out: Vec<(String, i64)> = vec![];
                for &x in v {
                    let k = keys[x.rem_euclid(5) as usize].to_string();
                    match out.iter_mut().find(|p| p.0 == k) {
                        Some(p) => p.1 = scale * x,
                        None => out.push((k, scale * x)),
                    }
                }
                out
            };
            let (a, b) = (to_assoc(&xs, 1), to_assoc(&ys, 10));
            let mut m = a.clone();
            for (k, x) in &b {
                match m.iter_mut().find(|p| &p.0 == k) {
                    Some(p) => p.1 += x,
                    None => m.push((k.clone(), *x)),
                }
            }
            let show = |v: &[(String, i64)]| show_assoc(&v.iter().map(|(k, x)| (k.clone(), x.to_string())).collect::<Vec<_>>());
            (format!("merge([{}, {}], total)", show(&a), show(&b)), show(&m))
        }
        19 => {
            // group_by of a join is the merge of the groups
            let src = format!(
                "merge([group_by({sx}, x => mod(x, 3)), group_by({sy}, x => mod(x, 3))], l => flatten(l, 1)) == group_by(join({sx}, {sy}), x => mod(x, 3))"
            );
            (src, "true".into())
        }
        20 => {
            // strings: reverse, take, split at a separator
            let s: String = (1..r.int(2, 10)).map(|_| *r.pick(&['a', 'b', ',', 'c'])).collect();
            let rev: String = s.chars().rev().collect();
            let k = r.int(0, s.len() as i64) as usize;
            let parts: Vec<String> = s.split(',').map(|p| format!("\"{p}\"")).collect();
            (
                format!("[string_reverse(\"{s}\"), string_take(\"{s}\", {k}), string_split(\"{s}\", \",\")]"),
                format!("[\"{rev}\", \"{}\", {}]", &s[..k], list(parts)),
            )
        }
        _ => {
            let mut v = xs.clone();
            v.sort();
            v.reverse();
            (format!("reverse_sort({sx})"), ints(&v))
        }
    }
}

/// Statistics of exact data against the formulas evaluated here in rationals.
fn stats_case(r: &mut Rng) -> (String, String) {
    let n = r.int(2, 9) as usize;
    let xs: Vec<i64> = (0..n).map(|_| r.int(-6, 9)).collect();
    let q: Vec<Q> = xs.iter().map(|&x| Q::int(x)).collect();
    let nq = Q::int(n as i64);
    let mean = q.iter().fold(Q(0, 1), |a, &b| a.add(b)).div(nq);
    let cm = |k: u32| q.iter().fold(Q(0, 1), |a, &b| a.add(b.sub(mean).pow(k))).div(nq);
    let mut s = xs.clone();
    s.sort();
    let median = if n % 2 == 1 { Q::int(s[n / 2]) } else { Q::int(s[n / 2 - 1]).add(Q::int(s[n / 2])).div(Q::int(2)) };
    let sx = ints(&xs);
    match r.below(7) {
        0 => (format!("mean({sx})"), mean.printed()),
        1 => (format!("variance({sx})"), cm(2).mul(nq).div(Q::int(n as i64 - 1)).printed()),
        2 => (format!("median({sx})"), median.printed()),
        3 => {
            let b = r.int(1, 8);
            let a = r.int(0, b);
            let rank = ((a * n as i64 + b - 1) / b).max(1);
            (format!("quantile({sx}, {a}/{b})"), s[(rank - 1) as usize].to_string())
        }
        4 if cm(2) != Q(0, 1) => (format!("kurtosis({sx})"), cm(4).div(cm(2).pow(2)).printed()),
        5 => {
            let k = r.int(1, 4) as u32;
            (format!("central_moment({sx}, {k})"), cm(k).printed())
        }
        _ => {
            let ys: Vec<i64> = (0..n).map(|_| r.int(-6, 9)).collect();
            let my = ys.iter().fold(Q(0, 1), |a, &b| a.add(Q::int(b))).div(nq);
            let c = q.iter().zip(&ys).fold(Q(0, 1), |a, (&x, &y)| a.add(x.sub(mean).mul(Q::int(y).sub(my))));
            (format!("covariance({sx}, {})", ints(&ys)), c.div(Q::int(n as i64 - 1)).printed())
        }
    }
}

/// Statistics of machine data and of degenerate data (one value, all values equal): a value or an honest
/// non-number, never a wrong number.
fn stats_numeric_case(e: &mut CoreEngine, r: &mut Rng, log: &mut Vec<String>) -> Option<String> {
    let n = r.int(1, 8) as usize;
    let constant = r.below(3) == 0;
    let c = r.int(-50, 50) as f64 / 8.0;
    let xs: Vec<f64> = (0..n).map(|_| if constant { c } else { r.int(-400, 400) as f64 / 16.0 }).collect();
    let sx = list(xs.iter().map(|x| format!("{x:?}")));
    let nf = n as f64;
    let mean = xs.iter().sum::<f64>() / nf;
    let cm = |k: i32| xs.iter().map(|x| (x - mean).powi(k)).sum::<f64>() / nf;
    let spread = cm(2) > 1e-12;
    let (src, want): (String, Option<f64>) = match r.below(5) {
        0 => (format!("mean({sx})"), Some(mean)),
        1 => (format!("variance({sx})"), (n > 1).then(|| cm(2) * nf / (nf - 1.0))),
        2 => (format!("std_dev({sx})"), (n > 1).then(|| (cm(2) * nf / (nf - 1.0)).sqrt())),
        3 => (format!("skewness({sx})"), spread.then(|| cm(3) / cm(2).powf(1.5))),
        _ => (format!("kurtosis({sx})"), spread.then(|| cm(4) / cm(2).powi(2))),
    };
    let ans = evt(e, &src, 20);
    log.push(format!("{src} = {ans}"));
    match (want, real_of(&ans)) {
        (Some(w), Some(g)) if (w - g).abs() > 1e-9 * (1.0 + w.abs()) => Some(format!("{src} = {ans}; directly {w}")),
        (Some(w), None) if !ans.contains("$aborted") => Some(format!("{src} = {ans}; directly {w}")),
        (None, Some(_)) => Some(format!("{src} = {ans}, a number where none is defined")),
        _ => None,
    }
}

fn lists_input(e: &mut CoreEngine, r: &mut Rng, log: &mut Vec<String>) -> Option<String> {
    let (src, want) = match r.below(10) {
        0..=5 => list_case(r),
        6..=7 => stats_case(r),
        _ => return stats_numeric_case(e, r, log),
    };
    let ans = evt(e, &src, 20);
    log.push(format!("{src} = {ans}"));
    (ans != want).then(|| format!("{src} = {ans}; expected {want}"))
}

// ---------------------------------------------------------------------------------------------------------------
// 2. polynomial algebra and number theory

/// A univariate polynomial from its coefficients (constant first), as a source in `v`.
fn poly_src(c: &[i64], v: &str) -> String {
    let terms: Vec<String> = c
        .iter()
        .enumerate()
        .filter(|(_, &a)| a != 0)
        .map(|(k, &a)| match k {
            0 => format!("({a})"),
            1 => format!("({a})*{v}"),
            _ => format!("({a})*{v}^{k}"),
        })
        .collect();
    if terms.is_empty() { "0".into() } else { format!("({})", terms.join(" + ")) }
}

/// A random nonconstant factor: in `x` (degree 1 or 2), or in `x` and `y`.
fn random_factor(r: &mut Rng, two_vars: bool, monic: bool) -> String {
    let d = r.int(1, 2) as usize;
    let mut c: Vec<i64> = (0..=d).map(|_| r.int(-3, 3)).collect();
    c[d] = if monic { 1 } else { *r.pick(&[1, 1, 2, -1, 3]) };
    let p = poly_src(&c, "x");
    if two_vars && r.below(2) == 0 {
        format!("({p} + ({})*y)", r.int(-2, 2).max(1))
    } else {
        p
    }
}

/// The determinant of an integer matrix (Bareiss, exact).
fn det_int(mut a: Vec<Vec<i128>>) -> i128 {
    let n = a.len();
    let (mut sign, mut prev) = (1i128, 1i128);
    for k in 0..n {
        if a[k][k] == 0 {
            match (k + 1..n).find(|&i| a[i][k] != 0) {
                Some(i) => {
                    a.swap(i, k);
                    sign = -sign;
                }
                None => return 0,
            }
        }
        for i in k + 1..n {
            for j in k + 1..n {
                a[i][j] = (a[i][j] * a[k][k] - a[i][k] * a[k][j]) / prev;
            }
        }
        prev = a[k][k];
    }
    sign * a[n - 1][n - 1]
}

/// The Sylvester matrix of two univariate polynomials (coefficients constant first).
fn sylvester(f: &[i64], g: &[i64]) -> Vec<Vec<i128>> {
    let (m, n) = (f.len() - 1, g.len() - 1);
    let mut s = vec![vec![0i128; m + n]; m + n];
    for i in 0..n {
        for (k, &c) in f.iter().rev().enumerate() {
            s[i][i + k] = c as i128;
        }
    }
    for i in 0..m {
        for (k, &c) in g.iter().rev().enumerate() {
            s[n + i][i + k] = c as i128;
        }
    }
    s
}

fn mul_mod(a: u64, b: u64, m: u64) -> u64 {
    ((a as u128 * b as u128) % m as u128) as u64
}

fn pow_mod(mut a: u64, mut e: u64, m: u64) -> u64 {
    let mut out = 1 % m;
    a %= m;
    while e > 0 {
        if e & 1 == 1 {
            out = mul_mod(out, a, m);
        }
        a = mul_mod(a, a, m);
        e >>= 1;
    }
    out
}

/// Deterministic Miller-Rabin for 64-bit numbers (the first twelve primes as bases suffice below 3.3e24).
fn is_prime_u64(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    for p in [2u64, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37] {
        if n.is_multiple_of(p) {
            return n == p;
        }
    }
    let (mut d, mut s) = (n - 1, 0);
    while d % 2 == 0 {
        d /= 2;
        s += 1;
    }
    'bases: for a in [2u64, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37] {
        let mut x = pow_mod(a, d, n);
        if x == 1 || x == n - 1 {
            continue;
        }
        for _ in 1..s {
            x = mul_mod(x, x, n);
            if x == n - 1 {
                continue 'bases;
            }
        }
        return false;
    }
    true
}

/// `[g, s, t]` with `s a + t b = g`.
fn ext_gcd(a: i128, b: i128) -> (i128, i128, i128) {
    if b == 0 { (a, 1, 0) } else {
        let (g, s, t) = ext_gcd(b, a.rem_euclid(b));
        (g, t, s - a.div_euclid(b) * t)
    }
}

/// Numbers whose primality is easy to get wrong: Carmichael numbers, strong pseudoprimes to small bases, Lucas
/// pseudoprimes, squares of primes, products of two primes p (2p - 1), primes near powers of two.
const TRICKY: &[u64] = &[
    561, 1105, 1729, 2465, 2821, 6601, 8911, 41041, 62745, 2047, 3277, 4033, 4681, 8321, 1373653, 25326001,
    3215031751, 2152302898747, 3474749660383, 341550071728321, 3825123056546413051, 5459, 5777, 10877, 16109, 18971,
    22499, 42799, 989, 3239, 5777, 49, 121, 169, 3481, 1046527, 4294967291, 4294967297, 18446744073709551557,
    18446744073709551533, 9223372036854775783, 2305843009213693951, 999999999989, 1000000007, 4611686018427387847,
];

/// A number-theory case: the source and the expected answer, computed here.
fn number_case(r: &mut Rng) -> (String, String) {
    match r.below(9) {
        0 => {
            let n = if r.below(2) == 0 { *r.pick(TRICKY) } else { r.next() >> r.int(1, 40) };
            (format!("is_prime({n})"), is_prime_u64(n).to_string())
        }
        1 => {
            // factor_integer of a product of small and 32-bit primes
            let mut n: u128 = 1;
            let mut fs: Vec<(u64, u32)> = vec![];
            for _ in 0..r.int(1, 3) {
                let mut p = if r.below(2) == 0 { r.int(2, 60) as u64 } else { (r.next() >> 33) | 1 };
                while !is_prime_u64(p) {
                    p += 1;
                }
                let e = r.int(1, 3) as u32;
                if n * (p as u128).pow(e) < (1u128 << 100) {
                    n *= (p as u128).pow(e);
                    match fs.iter_mut().find(|f| f.0 == p) {
                        Some(f) => f.1 += e,
                        None => fs.push((p, e)),
                    }
                }
            }
            fs.sort();
            if fs.is_empty() {
                fs.push((1, 1));
            }
            (format!("factor_integer({n})"), list(fs.iter().map(|(p, e)| format!("[{p}, {e}]"))))
        }
        2 => {
            let m = r.int(2, 10_000_000_000) as u64;
            let a = r.int(-1_000_000, 1_000_000_000);
            let b = r.int(-3, 1_000_000);
            let base = a.rem_euclid(m as i64) as u64;
            if b >= 0 {
                (format!("power_mod({a}, {b}, {m})"), pow_mod(base, b as u64, m).to_string())
            } else {
                let (g, s, _) = ext_gcd(base as i128, m as i128);
                if g != 1 {
                    // no inverse: no number
                    return (format!("is_integer(power_mod({a}, {b}, {m}))"), "false".into());
                }
                let inv = s.rem_euclid(m as i128) as u64;
                (format!("power_mod({a}, {b}, {m})"), pow_mod(inv, (-b) as u64, m).to_string())
            }
        }
        3 => {
            // the Chinese remainder theorem, with moduli that may share factors
            let k = r.int(2, 3) as usize;
            let ms: Vec<i128> = (0..k).map(|_| r.int(2, 40) as i128).collect();
            let rs: Vec<i128> = (0..k).map(|i| r.int(0, ms[i] as i64 - 1) as i128).collect();
            let (mut x, mut m) = (0i128, 1i128);
            let mut ok = true;
            for i in 0..k {
                let (g, s, _) = ext_gcd(m, ms[i]);
                if (rs[i] - x) % g != 0 {
                    ok = false;
                    break;
                }
                let l = m / g * ms[i];
                x = (x + m * ((rs[i] - x) / g * s).rem_euclid(ms[i] / g)).rem_euclid(l);
                m = l;
            }
            let src = format!("chinese_remainder({}, {})", list(rs.iter().map(|v| v.to_string())), list(ms.iter().map(|v| v.to_string())));
            if ok { (src, x.to_string()) } else { (format!("is_integer({src})"), "false".into()) }
        }
        4 => {
            // continued fractions of a rational and of the square root of a non-square
            if r.below(2) == 0 {
                let (mut p, mut q) = (r.int(-500, 500) as i128, r.int(1, 200) as i128);
                let src = format!("continued_fraction({p}/{q})");
                let mut cf = vec![];
                while q != 0 {
                    let a = p.div_euclid(q);
                    cf.push(a.to_string());
                    (p, q) = (q, p - a * q);
                }
                (src, list(cf))
            } else {
                let d = r.int(2, 300);
                let a0 = (d as f64).sqrt().floor() as i64;
                if a0 * a0 == d {
                    return (format!("continued_fraction(sqrt({d}))"), format!("[{a0}]"));
                }
                let (mut m, mut dd, mut a) = (0i64, 1i64, a0);
                let mut period = vec![];
                loop {
                    m = dd * a - m;
                    dd = (d - m * m) / dd;
                    a = (a0 + m) / dd;
                    period.push(a);
                    if a == 2 * a0 {
                        break;
                    }
                }
                (format!("continued_fraction(sqrt({d}))"), format!("[{a0}, {}]", ints(&period)))
            }
        }
        5 => {
            // mod and quotient with negative and rational divisors
            let (a, b) = (r.int(-60, 60), *r.pick(&[-7i64, -3, -2, 2, 3, 5]));
            let (c, d) = (r.int(1, 4), r.int(1, 4));
            // a/c mod b/d = a/c - (b/d) floor((a d)/(b c))
            let floor = |p: i64, q: i64| if q > 0 { p.div_euclid(q) } else { (-p).div_euclid(-q) };
            let m = Q::new(a as i128, c as i128).sub(Q::new(b as i128, d as i128).mul(Q::int(floor(a * d, b * c))));
            (format!("[mod({a}/{c}, {b}/{d}), quotient({a}, {b})]"), list([m.printed(), floor(a, b).to_string()]))
        }
        6 => {
            // linear Diophantine equations: frobenius_solve against the enumeration
            let k = r.int(2, 3) as usize;
            let w: Vec<i64> = (0..k).map(|_| r.int(2, 9)).collect();
            let n = r.int(0, 40);
            let mut sols: Vec<Vec<i64>> = vec![];
            let mut cur = vec![0i64; k];
            fn rec(i: usize, left: i64, w: &[i64], cur: &mut Vec<i64>, sols: &mut Vec<Vec<i64>>) {
                if i == w.len() {
                    if left == 0 {
                        sols.push(cur.clone());
                    }
                    return;
                }
                let mut c = 0;
                while c * w[i] <= left {
                    cur[i] = c;
                    rec(i + 1, left - c * w[i], w, cur, sols);
                    c += 1;
                }
                cur[i] = 0;
            }
            rec(0, n, &w, &mut cur, &mut sols);
            sols.sort();
            (format!("frobenius_solve({}, {n})", ints(&w)), list(sols.iter().map(|s| ints(s))))
        }
        7 => {
            let n = 2 * r.int(1, 2000) + 1;
            let a = r.int(-3000, 3000);
            // the Jacobi symbol by quadratic reciprocity
            let (mut a1, mut n1, mut t) = (a.rem_euclid(n), n, 1i64);
            while a1 != 0 {
                while a1 % 2 == 0 {
                    a1 /= 2;
                    if n1 % 8 == 3 || n1 % 8 == 5 {
                        t = -t;
                    }
                }
                std::mem::swap(&mut a1, &mut n1);
                if a1 % 4 == 3 && n1 % 4 == 3 {
                    t = -t;
                }
                a1 %= n1;
            }
            (format!("jacobi_symbol({a}, {n})"), if n1 == 1 { t.to_string() } else { "0".into() })
        }
        _ => {
            let (a, b) = (r.int(-10_000, 10_000), r.int(-10_000, 10_000));
            let src = format!("e = extended_gcd({a}, {b}); e[1] == gcd({a}, {b}) and e[2][1]*({a}) + e[2][2]*({b}) == e[1]");
            (src, "true".into())
        }
    }
}

/// The prime factors of `n` with their exponents (trial division).
fn factor_small(mut n: u64) -> Vec<(u64, u32)> {
    let mut out = vec![];
    let mut p = 2;
    while p * p <= n {
        let mut e = 0;
        while n.is_multiple_of(p) {
            n /= p;
            e += 1;
        }
        if e > 0 {
            out.push((p, e));
        }
        p += 1;
    }
    if n > 1 {
        out.push((n, 1));
    }
    out
}

fn mul_mod128(mut a: u128, mut b: u128, m: u128) -> u128 {
    // a b mod m by doubling (m < 2^127, so no sum overflows)
    let mut out = 0;
    a %= m;
    while b > 0 {
        if b & 1 == 1 {
            out = (out + a) % m;
        }
        a = (a + a) % m;
        b >>= 1;
    }
    out
}

/// Miller-Rabin for numbers below 2^127 with the first 20 primes as bases (deterministic below 3.3e24; above, a
/// composite passes with probability below 4^-20).
fn is_prime_u128(n: u128) -> bool {
    if n < 2 {
        return false;
    }
    let bases = [2u128, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71];
    for p in bases {
        if n.is_multiple_of(p) {
            return n == p;
        }
    }
    let (mut d, mut s) = (n - 1, 0);
    while d % 2 == 0 {
        d /= 2;
        s += 1;
    }
    'bases: for a in bases {
        let (mut x, mut e, mut base) = (1u128, d, a);
        while e > 0 {
            if e & 1 == 1 {
                x = mul_mod128(x, base, n);
            }
            base = mul_mod128(base, base, n);
            e >>= 1;
        }
        if x == 1 || x == n - 1 {
            continue;
        }
        for _ in 1..s {
            x = mul_mod128(x, x, n);
            if x == n - 1 {
                continue 'bases;
            }
        }
        return false;
    }
    true
}

/// Arithmetic functions against their definitions computed here.
fn arithmetic_case(r: &mut Rng) -> (String, String) {
    let n = r.int(1, 200_000) as u64;
    let fs = factor_small(n);
    match r.below(8) {
        0 => {
            let phi: u64 = fs.iter().map(|&(p, e)| (p - 1) * p.pow(e - 1)).product();
            (format!("euler_phi({n})"), phi.to_string())
        }
        1 => {
            let k = r.int(0, 3) as u32;
            let s: u128 = fs.iter().map(|&(p, e)| (0..=e).map(|i| (p as u128).pow(i * k)).sum::<u128>()).product();
            (format!("divisor_sigma({k}, {n})"), s.to_string())
        }
        2 => {
            let mu = if fs.iter().any(|f| f.1 > 1) { 0 } else if fs.len().is_multiple_of(2) { 1 } else { -1 };
            (format!("moebius_mu({n})"), mu.to_string())
        }
        3 => {
            // the multiplicative order of a modulo m, and the least primitive root of a prime
            let m = r.int(2, 5000) as u64;
            let a = r.int(1, 10_000) as u64;
            if gcd(a as i128, m as i128) != 1 {
                return (format!("is_integer(multiplicative_order({a}, {m}))"), "false".into());
            }
            let mut k = 1;
            let mut x = a % m;
            while x != 1 % m {
                x = x * a % m;
                k += 1;
            }
            (format!("multiplicative_order({a}, {m})"), k.to_string())
        }
        4 => {
            let mut p = r.int(3, 20_000) as u64;
            while !is_prime_u64(p) {
                p += 1;
            }
            let qs: Vec<u64> = factor_small(p - 1).iter().map(|f| f.0).collect();
            let g = (2..p).find(|&g| qs.iter().all(|&q| pow_mod(g, (p - 1) / q, p) != 1)).unwrap();
            (format!("primitive_root({p})"), g.to_string())
        }
        5 => {
            let b = r.int(2, 36) as u64;
            let (mut k, mut ds) = (n, vec![]);
            while k > 0 {
                ds.push((k % b) as i64);
                k /= b;
            }
            ds.reverse();
            (format!("integer_digits({n}, {b})"), ints(&ds))
        }
        6 => {
            // a square root modulo a prime: present exactly for the quadratic residues
            let mut p = r.int(3, 100_000) as u64;
            while !is_prime_u64(p) {
                p += 1;
            }
            let a = r.int(1, p as i64 - 1) as u64;
            let residue = pow_mod(a, (p - 1) / 2, p) == 1;
            let src = format!("s = power_mod({a}, 1/2, {p}); if is_integer(s) then mod(s^2 - {a}, {p}) == 0 else false");
            (src, residue.to_string())
        }
        _ => {
            // primality beyond 64 bits: products of two primes, and the primes after random 70- to 120-bit numbers
            let bits = r.int(65, 120) as u32;
            let mut q: u128 = ((r.next() as u128) << 64 | r.next() as u128) >> (128 - bits) | 1;
            if r.below(2) == 0 {
                while !is_prime_u128(q) {
                    q += 2;
                }
                (format!("is_prime({q})"), "true".into())
            } else {
                let mut p = (r.next() >> 34) | 1;
                while !is_prime_u64(p) {
                    p += 2;
                }
                q >>= 66;
                q |= 1;
                while !is_prime_u128(q) {
                    q += 2;
                }
                (format!("[is_prime({}), factor_integer({})]", p as u128 * q, p as u128 * q), {
                    let (a, b) = ((p as u128).min(q), (p as u128).max(q));
                    if a == b { format!("[false, [[{a}, 2]]]") } else { format!("[false, [[{a}, 1], [{b}, 1]]]") }
                })
            }
        }
    }
}

/// A polynomial case checked in the session: the operation (its answer `f`) and a check that must be `true`.
fn polynomial_case(r: &mut Rng) -> (String, String) {
    match r.below(19) {
        0 | 1 => {
            // factor over the integers: multiplied back, and at least as many factors as the product was made of
            let two = r.below(3) == 0;
            let k = r.int(2, 3) as usize;
            let fs: Vec<String> = (0..k).map(|_| random_factor(r, two, false)).collect();
            let p = fs.join("*");
            (
                format!("p = expand({p}); f = factor(p)"),
                format!("expand(f - p) == 0 and total(map(q => if variables(q[1]) == [] then 0 else q[2], factor_list(p))) >= {k}"),
            )
        }
        2 => {
            // factor modulo a prime
            let pr = *r.pick(&[2, 3, 5, 7, 11]);
            let k = r.int(2, 3) as usize;
            let fs: Vec<String> = (0..k).map(|_| random_factor(r, false, true)).collect();
            (
                format!("p = expand({}); f = factor(p, modulus: {pr})", fs.join("*")),
                format!("polynomial_mod(expand(f - p), {pr}) == 0 and total(map(q => if variables(q[1]) == [] then 0 else q[2], factor_list(p, modulus: {pr}))) >= {k}"),
            )
        }
        3 => {
            // factor over an extension
            let d = *r.pick(&[2, 3, 5, -1, -3]);
            let ext = if d == -1 { "im".to_string() } else { format!("sqrt({d})") };
            let g = random_factor(r, false, false);
            (
                format!("p = expand((x^2 - ({d}))*{g}); f = factor(p, extension: {ext})"),
                format!("expand(f - p) == 0 and total(map(q => if variables(q[1]) == [] then 0 else q[2], factor_list(p, extension: {ext}))) >= 3"),
            )
        }
        4 => {
            // gcd: divides both, and is divisible by the common factor
            let two = r.below(2) == 0;
            let (a, b, c) = (random_factor(r, two, false), random_factor(r, two, false), random_factor(r, two, false));
            (
                format!("pa = expand({a}*{c}); pb = expand({b}*{c}); f = polynomial_gcd(pa, pb)"),
                "is_polynomial(cancel(pa/f), [x, y]) and is_polynomial(cancel(pb/f), [x, y]) and is_polynomial(cancel(f/(".to_string() + &c + ")), [x, y])",
            )
        }
        5 => {
            // the resultant against the determinant of the Sylvester matrix (computed here)
            let (df, dg) = (r.int(1, 3) as usize, r.int(1, 3) as usize);
            let mut f: Vec<i64> = (0..=df).map(|_| r.int(-4, 4)).collect();
            let mut g: Vec<i64> = (0..=dg).map(|_| r.int(-4, 4)).collect();
            f[df] = r.int(1, 3);
            g[dg] = -r.int(1, 3);
            let want = det_int(sylvester(&f, &g));
            (format!("f = resultant({}, {}, x)", poly_src(&f, "x"), poly_src(&g, "x")), format!("f == {want}"))
        }
        6 => {
            // Groebner bases: the generators reduce to 0, and each element of the basis reduces to 0 by the basis of
            // the other order of the variables (so it lies in the ideal)
            let k = r.int(2, 3) as usize;
            let fs: Vec<String> = (0..k)
                .map(|_| {
                    let t = ["x^2", "x*y", "y^2", "x", "y", "1"];
                    let terms: Vec<String> = t.iter().map(|m| format!("({})*{m}", r.int(-2, 2))).collect();
                    format!("({})", terms.join(" + "))
                })
                .collect();
            let fsrc = list(fs);
            (
                format!("fs = expand({fsrc}); f = groebner_basis(fs, [x, y])"),
                "g2 = groebner_basis(fs, [y, x]); alltrue(map(p => polynomial_reduce(p, f, [x, y])[2] == 0, fs)) and alltrue(map(p => polynomial_reduce(p, g2, [y, x])[2] == 0, f)) and alltrue(map(p => polynomial_reduce(p, f, [x, y])[2] == 0, g2))".into(),
            )
        }
        7 => {
            // solve: each root puts the polynomial to 0, and as many roots as distinct roots
            let k = r.int(1, 3) as usize;
            let mut fs: Vec<String> = (0..k).map(|_| random_factor(r, false, false)).collect();
            if r.below(3) == 0 {
                let dup = fs[0].clone();
                fs.push(dup);
            }
            let p = fs.join("*");
            (
                format!("p = expand({p}); f = solve(p == 0, x)"),
                "alltrue(map(s => { zt = n(p /. s, 50); is_failure(zt) or abs(zt) < 10^-40 }, f)) and length(f) == exponent(p, x)".into(),
            )
        }
        8 => {
            // reduce: the answer and the condition agree at rational points and at the roots and near them
            let p = format!("expand({})", (0..r.int(1, 3)).map(|_| random_factor(r, false, false)).collect::<Vec<_>>().join("*"));
            let rel = *r.pick(&[">", ">=", "<", "<=", "!="]);
            let c = r.int(-3, 3);
            let cond = match r.below(3) {
                0 => format!("{p} {rel} 0"),
                1 => format!("{p} {rel} 0 and x > {c}"),
                _ => format!("{p} {rel} 0 or x <= {c}"),
            };
            (
                format!("cond = x => {cond}; f = reduce(cond(x), x, reals)"),
                format!("rts = select(x /. solve({p} == 0, x), t => im_part(t) == 0); pts = join(table(k/4, [k, -24, 24]), rts, rts + 10^-9, rts - 10^-9, [{c}, {c} + 10^-9, {c} - 10^-9]); alltrue(map(t => (f /. x -> t) === cond(t), pts))"),
            )
        }
        10 => {
            // factor in three variables, and the square-free factorization, multiplied back
            let k = r.int(2, 3) as usize;
            let fs: Vec<String> = (0..k)
                .map(|_| format!("({} + ({})*z)", random_factor(r, true, false), r.int(-2, 2)))
                .collect();
            let sq = if r.below(2) == 0 { format!("*{}", fs[0]) } else { String::new() };
            let op = *r.pick(&["factor", "factor_square_free"]);
            (
                format!("p = expand({}{sq}); f = {op}(p)", fs.join("*")),
                if op == "factor" {
                    format!("expand(f - p) == 0 and total(map(q => if variables(q[1]) == [] then 0 else q[2], factor_list(p))) >= {k}")
                } else {
                    "expand(f - p) == 0".to_string()
                },
            )
        }
        11 => {
            // systems: each solution put back into the equations
            let (a, b) = (random_factor(r, true, false), random_factor(r, true, false));
            let c = r.int(-3, 3);
            (
                format!("eqs = [{a} == 0, x - ({c})*y^2 + {} == 0]; f = solve(eqs, [x, y])", r.int(-3, 3)),
                format!("alltrue(map(s => alltrue(map(q => {{ zt = n(q /. s, 50); is_failure(zt) or abs(zt) < 10^-35 }}, [{a}, eqs[2][1]])), f)) and length(f) <= 4 + 0*({b})"),
            )
        }
        12 => {
            // n_solve: as many roots as the degree, each a root at its precision
            let d = r.int(2, 7) as usize;
            let mut c: Vec<i64> = (0..=d).map(|_| r.int(-9, 9)).collect();
            c[d] = r.int(1, 4);
            let p = poly_src(&c, "x");
            let wp = *r.pick(&["", ", working_precision: 30"]);
            (
                format!("p = {p}; f = n_solve(p == 0, x{wp})"),
                format!("length(f) == {d} and alltrue(map(s => abs(p /. s) < 10^-{}*(1 + max(abs(coefficient_list(p, x)))*max(1, abs(x /. s))^{d}), f))", if wp.is_empty() { 9 } else { 24 }),
            )
        }
        13 => {
            // reduce in two variables: the answer and the condition agree on a grid
            let p = random_factor(r, true, false);
            let rel = *r.pick(&[">", "<", ">=", "<="]);
            let cond = match r.below(3) {
                0 => format!("{p} {rel} 0 and x^2 + y^2 < 4"),
                1 => format!("{p} {rel} 0 or y > x"),
                _ => format!("{p} {rel} 0 and y >= x^2 - 1"),
            };
            (
                format!("cond = (x, y) => {cond}; f = reduce(cond(x, y), [x, y], reals)"),
                "alltrue(flatten(table((f /. [x -> i/3, y -> j/3]) === cond(i/3, j/3), [i, -7, 7], [j, -7, 7])))".into(),
            )
        }
        15 => {
            // sums of two squares in integers against the enumeration
            let n = r.int(0, 130);
            let mut sols = vec![];
            for a in -12i64..=12 {
                for b in -12i64..=12 {
                    if a * a + b * b == n {
                        sols.push(format!("[{a}, {b}]"));
                    }
                }
            }
            (
                format!("f = solve(x^2 + y^2 == {n}, [x, y], integers)"),
                format!("sort(map(s => [x, y] /. s, f)) == {}", list(sols)),
            )
        }
        16 | 17 => {
            // reduce and find_instance over the integers in a bounded region, against the enumeration
            let (a, b, c) = (r.int(-3, 3), r.int(-3, 3), r.int(1, 30));
            let rel = *r.pick(&["<", "<=", "=="]);
            let test = |x: i64, y: i64| {
                let q = x * x + y * y;
                let lin = a * x + b * y;
                let first = match rel {
                    "<" => q < c,
                    "<=" => q <= c,
                    _ => q == c,
                };
                first && lin >= 0
            };
            let mut pts = vec![];
            for x in -7i64..=7 {
                for y in -7i64..=7 {
                    if test(x, y) {
                        pts.push(format!("[{x}, {y}]"));
                    }
                }
            }
            let cond = format!("x^2 + y^2 {rel} {c} and ({a})*x + ({b})*y >= 0");
            if r.below(2) == 0 {
                (
                    format!("cond = {cond}; f = reduce(cond, [x, y], integers)"),
                    format!("select(flatten(table([i, j], [i, -7, 7], [j, -7, 7]), 1), p => (f /. [x -> p[1], y -> p[2]]) === true) == {}", list(pts)),
                )
            } else {
                (
                    format!("cond = {cond}; f = find_instance(cond, [x, y], integers)"),
                    format!("if f == [] then {} else (cond /. f[1]) === true", pts.is_empty()),
                )
            }
        }
        14 => {
            // continued fractions of quadratic irrationals: the periodic form gives the number back
            let (pp, d, q) = (r.int(-5, 5), *r.pick(&[2, 3, 5, 6, 7, 10, 11, 13]), *r.pick(&[1, 2, 3, 4]));
            (
                format!("v = ({pp} + sqrt({d}))/{q}; f = continued_fraction(v)"),
                "abs(n(from_continued_fraction(f) - v, 50)) < 10^-40".into(),
            )
        }
        _ => {
            // find_instance: the instance satisfies the condition
            let p = random_factor(r, true, false);
            let cond = match r.below(4) {
                0 => format!("{p} == 0 and x > 0"),
                1 => format!("{p} > 0 and x^2 + y^2 < 4"),
                2 => format!("x^2 + y^2 == {} and x > y", r.int(1, 30)),
                _ => format!("{p} < 0 and y > x^2"),
            };
            let dom = if r.below(3) == 0 { ", integers" } else { "" };
            (format!("cond = {cond}; f = find_instance(cond, [x, y]{dom})"), "f == [] or (cond /. f[1]) === true".into())
        }
    }
}

fn algebra_input(e: &mut CoreEngine, r: &mut Rng, log: &mut Vec<String>) -> Option<String> {
    ev(e, "alltrue = l => all_true(l, t => t === true); 0");
    if r.below(5) < 2 {
        let (src, want) = if r.below(2) == 0 { number_case(r) } else { arithmetic_case(r) };
        let ans = evt(e, &src, 20);
        log.push(format!("{src} = {ans}"));
        return (ans != want && !ans.contains("$aborted")).then(|| format!("{src} = {ans}; expected {want}"));
    }
    let (op, check) = polynomial_case(r);
    let ans = evt(e, &format!("{op}; if leaf_count(f) > 20000 then large_answer else f"), 30);
    log.push(format!("{op} = {ans}"));
    let head = op.rsplit("f = ").next().unwrap_or("").split('(').next().unwrap_or("");
    if gave_up(&ans, head) || ans.contains(&format!("{head}(")) || ans == "large_answer" {
        return None;
    }
    let checked = evt(e, &format!("{{ {check} }} === true"), 60);
    log.push(format!("check: {checked}"));
    (!is_true(&checked) && !checked.contains("$aborted")).then(|| format!("{op} = {ans}: check {check} is {checked}"))
}

// ---------------------------------------------------------------------------------------------------------------
// 4. rewriting keeps the value

/// Building blocks (templates in `X`) where rewriting rules need the domain: powers, logarithms, inverse functions,
/// removable singularities, gamma quotients.
const BLOCKS: &[&str] = &[
    "sqrt(X^2)", "abs(X)", "log(X^2)", "exp(log(X))", "log(exp(X))", "sqrt(X)^2", "(X^2)^(1/2)", "(X^3)^(1/3)",
    "X^(1/3)*X^(2/3)", "sqrt(X)*sqrt(X + 1)", "sqrt(X*(X + 1))", "log(X) + log(X + 2)", "log(X*(X + 2))",
    "exp(2*log(X))", "sin(X)^2 + cos(X)^2", "sin(2*X)", "cos(X)^2 - sin(X)^2", "tan(X)", "atan(X) + atan(1/X)",
    "asin(sin(X))", "acos(cos(X))", "atan(tan(X))", "cosh(X)^2 - sinh(X)^2", "(X^2 - 1)/(X - 1)",
    "1/(X + 1) + 1/(X - 1)", "gamma(X + 1)/gamma(X)", "binomial(X, 2)", "sign(X)*abs(X)", "erf(-X)", "sqrt(1/X)",
    "1/sqrt(X)", "log(1/X)", "(1/X)^(1/3)", "abs(X)^2", "sqrt(X^2 + 2*X + 1)", "floor(X) + ceiling(-X)",
    "arg(X)", "re(X)^2 + im_part(X)^2", "exp(X)*exp(-X)", "sinh(log(X))", "cos(acos(X))", "acosh(cosh(X))",
    "log(sqrt(X))", "sqrt(exp(X))", "(X^4)^(1/4)", "csc(X)*sin(X)", "x^2*abs(X)/X",
];

/// Blocks in two variables (`X`, `Y`): products and powers whose rules need signs.
const BLOCKS2: &[&str] = &[
    "sqrt(X*Y)", "sqrt(X)*sqrt(Y)", "log(X*Y)", "log(X) + log(Y)", "(X*Y)^(1/3)", "X^Y", "(X^Y)^2", "exp(Y*log(X))",
    "atan(X) + atan(Y)", "atan((X + Y)/(1 - X*Y))", "sqrt(X^2*Y^2)", "abs(X*Y)", "(X^2)^Y", "X^(2*Y)",
    "max(X, Y) + min(X, Y)", "sign(X*Y)", "log(X^Y)", "Y*log(X)", "sqrt(X/Y)", "sqrt(X)/sqrt(Y)", "(X/Y)^(1/2)*Y",
    "log(X/Y)", "abs(X)*abs(Y)", "X^(1/2)*Y^(1/2) - (X*Y)^(1/2)", "(X^3*Y^3)^(1/3)", "exp(log(X) + log(Y))",
];

/// Blocks in an integer `N` and a real `X`.
const BLOCKS_INT: &[&str] = &[
    "sin(N*pi + X)", "cos(N*pi)", "(-1)^(2*N)", "floor(N + X)", "mod(N, 1)", "exp(2*pi*im*N)", "binomial(N, 2)",
    "factorial(N + 1)/factorial(N)", "(-1)^N*(-1)^N", "sin(pi*N/2)^2", "round(N + 1/3)", "cos(2*pi*N + X)",
    "tan(N*pi + X)", "(-1)^(N + 1) + (-1)^N", "sqrt(N^2)", "exp(im*pi*N)", "sin(N*pi)", "ceiling(N/2) + floor(-N/2)",
    "mod(N^2, 2) - mod(N, 2)", "(N + X)^2 - N^2",
];

/// Domains: the assumption, its variables and exact sample points (a list of values for each variable).
fn domain(r: &mut Rng) -> (&'static str, Vec<&'static str>, Vec<Vec<String>>) {
    let kinds: &[(&str, &[&str], i64, i64)] = &[
        ("x > 0", &["x"], 1, 400),
        ("x < 0", &["x"], -400, -1),
        ("x > 1", &["x"], 101, 500),
        ("-1 < x and x < 1", &["x"], -99, 99),
        ("element(x, reals)", &["x"], -400, 400),
        ("", &["x"], -300, 300),
        ("x > 0 and y > 0", &["x", "y"], 1, 400),
        ("x < 0 and y > 0", &["x", "y"], -400, 400),
        ("x < 0 and y < 0", &["x", "y"], -400, -1),
        ("element(x, reals) and element(y, reals)", &["x", "y"], -400, 400),
        ("element(n, integers) and element(x, reals)", &["n", "x"], -400, 400),
        ("element(n, integers) and n > 0 and x > 0", &["n", "x"], 1, 400),
    ];
    let (a, vars, lo, hi) = *r.pick(kinds);
    let mut pts = vec![];
    for _ in 0..8 {
        let p: Vec<String> = vars
            .iter()
            .map(|&v| {
                let mut k = r.int(lo, hi);
                if a == "x < 0 and y > 0" {
                    k = if v == "x" { -k.abs() - 1 } else { k.abs() + 1 };
                }
                let k = if k == 0 { 1 } else { k };
                match (a, v) {
                    ("", _) => format!("({k}/100 + ({})*im/100)", r.int(-300, 300)),
                    (_, "n") => format!("({})", (k / 50).max(if a.contains("n > 0") { 1 } else { -8 })),
                    _ => format!("({k}/100)"),
                }
            })
            .collect();
        pts.push(p);
    }
    (a, vars.to_vec(), pts)
}

fn rewrite_input(e: &mut CoreEngine, r: &mut Rng, log: &mut Vec<String>) -> Option<String> {
    let (assume, vars, pts) = domain(r);
    let inner = *r.pick(&["x", "x", "x", "2*x", "x^2", "-x", "x/3", "x + 1"]);
    let one = |r: &mut Rng, inner: &str| -> String {
        if vars.contains(&"y") && r.below(3) > 0 {
            r.pick(BLOCKS2).replace('X', &format!("({inner})")).replace('Y', "y")
        } else if vars.contains(&"n") && r.below(3) > 0 {
            r.pick(BLOCKS_INT).replace('X', &format!("({inner})")).replace('N', "n")
        } else {
            r.pick(BLOCKS).replace('X', &format!("({inner})"))
        }
    };
    let b1 = one(r, inner);
    let ex = match r.below(4) {
        0 => b1,
        1 => format!("{b1} + {}", one(r, "x")),
        2 => format!("({b1})*({})", one(r, "x")),
        _ => format!("{b1} - x^2/({})", one(r, "x")),
    };
    let positive = assume == "x > 0 and y > 0" || assume == "x > 0" || assume == "x > 1";
    let op = if assume.is_empty() {
        r.pick(&[
            "simplify(E)", "full_simplify(E)", "function_expand(E)", "together(E)", "apart(E, x)", "expand(E)", "factor(E)",
            "trig_expand(E)", "trig_reduce(E)", "trig_to_exp(E)", "exp_to_trig(E)", "trig_factor(E)",
        ])
        .replace('E', &ex)
    } else {
        let o = *r.pick(&[
            "simplify(E, assumptions: A)", "full_simplify(E, assumptions: A)", "refine(E, A)", "simplify(E, assumptions: A)",
            "function_expand(E, assumptions: A)", "together(E)", "apart(E, x)", "complex_expand(E)", "trig_expand(E)",
            "power_expand(E, assumptions: A)", "full_simplify(E, assumptions: A)", "assuming(A, simplify(E))",
        ]);
        if o == "complex_expand(E)" && assume.contains("integers") {
            return None;
        }
        if o.contains("power_expand") && !positive && r.below(2) == 0 {
            return None;
        }
        o.replace('E', &ex).replace('A', assume)
    };
    let ans = evt(e, &format!("res = {op}; res"), 30);
    log.push(format!("{op} = {ans}"));
    let head = op.split('(').next().unwrap_or("");
    if ans.starts_with("failure") || ans.contains("$aborted") || ans.starts_with(&format!("{head}(")) {
        return None;
    }
    // the values at each point: a failure or a non-number in the input says nothing; a value of the input that the
    // result does not have (or has differently) is a fault
    let params = vars.join(", ");
    let rules = |p: &[String]| list(vars.iter().zip(p).map(|(v, x)| format!("{v} -> {x}")));
    let checks: Vec<String> = pts
        .iter()
        .map(|p| {
            format!(
                "{{ u = n(ex({}), 45); v = n(res /. {}, 45); if is_failure(u) or not is_number(u) then false else if is_failure(v) then true else (not is_number(v) or abs(u - v) > 10^-25*(1 + abs(u))) }}",
                p.join(", "),
                rules(p)
            )
        })
        .collect();
    let bad = evt(e, &format!("ex = ({params}) => {ex}; pick({}, {})", list(pts.iter().map(|p| rules(p))), list(checks)), 60);
    log.push(format!("points where the values differ: {bad}"));
    (bad != "[]" && !bad.contains("$aborted") && !bad.starts_with("failure")).then(|| format!("{op} = {ans}: different values at {bad}"))
}

// ---------------------------------------------------------------------------------------------------------------
// tests

/// Is `src` true?
fn holds(e: &mut CoreEngine, src: &str) -> bool {
    ev(e, src) == "true"
}

/// `src` evaluated within 20 s, and the time it took (every defect test below is bounded so).
fn timed(e: &mut CoreEngine, src: &str) -> String {
    let t0 = Instant::now();
    let out = evt(e, src, 20);
    assert!(t0.elapsed() < Duration::from_secs(20), "{src}: {:?}", t0.elapsed());
    out
}

#[test]
fn part_assignments_to_several_positions() {
    let mut e = CoreEngine::new();
    // a span, a list of positions, all: each position set (they were silently left as they were)
    assert_eq!(timed(&mut e, "l = [1, 2, 3, 4, 5, 6]; l[2:4] = 0; l"), "[1, 0, 0, 0, 5, 6]");
    assert_eq!(timed(&mut e, "l[2:4] = [7, 8, 9]; l"), "[1, 7, 8, 9, 5, 6]");
    assert_eq!(timed(&mut e, "l[[1, -1]] = [10, 11]; l"), "[10, 7, 8, 9, 5, 11]");
    assert_eq!(timed(&mut e, "m = [[1, 2], [3, 4]]; m[all, 2] = [5, 6]; m[1, all] = 0; m"), "[[0, 0], [3, 6]]");
    assert_eq!(timed(&mut e, "m[span(2, 1, -1), 1] = [a, b]; m"), "[[b, 0], [a, 6]]");
    // independent: the parts read back are the values put in
    assert!(holds(&mut e, "l = range(8); l[span(2, 8, 3)] = [x, y, z]; l[[2, 5, 8]] == [x, y, z] and l[[1, 3, 4, 6, 7]] == [1, 3, 4, 6, 7]"));
    // a position outside the list: the failure of reading it (it was a silent no-op)
    assert_eq!(timed(&mut e, "l = [1, 2, 3]; l[5] = 9"), "failure(index, 5, 3)");
    assert_eq!(timed(&mut e, "l"), "[1, 2, 3]");
    assert_eq!(timed(&mut e, "l[2:4] = 0"), "failure(index, 4, 3)");
    // variations: a var local, an association's list, a list of lists set element by element
    assert_eq!(timed(&mut e, "g = () => { var x = [[1, 2], [3, 4]]; x[1:2, 2] = 0; x[1, 1:2] = [5, 6]; x }; g()"), "[[5, 6], [3, 0]]");
    assert_eq!(timed(&mut e, "s = assoc(a -> [1, 2, 3]); s[a, 2:3] = 0; s"), "assoc(a -> [1, 0, 0])");
    assert_eq!(timed(&mut e, "m = [[1, 2], [3, 4]]; m[[1, 2], [1, 2]] = [[p, q], [r, w]]; m"), "[[p, q], [r, w]]");
}

#[test]
fn gamma_zero_test_is_not_formal() {
    let mut e = CoreEngine::new();
    // power_expand of the whole term rewrote sqrt(x^2), log(exp(x)), asin(sin(x)) to x: these were 0
    assert_eq!(timed(&mut e, "simplify(gamma(x + 1)/gamma(x) - sqrt(x^2))"), "x - (x^2)^(1/2)");
    assert_eq!(timed(&mut e, "simplify(gamma(x + 1)/gamma(x) - log(exp(x)))"), "x - log(exp(x))");
    assert_ne!(timed(&mut e, "full_simplify(asin(sin(-x)) + gamma(x + 1)/gamma(x), assumptions: x < 0)"), "0");
    // independent: at x = -12/5 the input is -12/5 + asin(sin(12/5)) = pi - 24/5, not 0
    assert!(holds(&mut e, "abs(n(asin(sin(12/5)) - 12/5 - (pi - 24/5), 30)) < 10^-25"));
    // the identities the test is for still prove 0 (duplication, triplication, the Catalan numbers)
    assert_eq!(timed(&mut e, "simplify(gamma(2 n)/(gamma(n)*gamma(n + 1/2)) - 2^(2 n - 1)/sqrt(pi))"), "0");
    assert_eq!(timed(&mut e, "simplify(gamma(3 n)/(gamma(n)*gamma(n + 1/3)*gamma(n + 2/3)) - 3^(3 n - 1/2)/(2 pi))"), "0");
    assert_eq!(timed(&mut e, "full_simplify(function_expand(catalan_number(n) == binomial(2 n, n) - binomial(2 n, n + 1)))"), "true");
}

#[test]
fn moduli_and_extremes_of_balls() {
    let mut e = CoreEngine::new();
    // the residue of an exact eigenvector check at 50 digits: complex balls around 0; their modulus was
    // ball(0, r)^(1/2), unevaluated, and max of such balls stayed unevaluated
    ev(&mut e, "m = [[-1/2, 5/2, -1], [-1, -2, 7/2], [7/2, 3/5, -1]]; f = eigensystem(m); d = n(dot(m, f[2][1]) - f[1][1]*f[2][1], 50)");
    assert!(holds(&mut e, "all_true(abs(d), t => t < 10^-40)"));
    assert!(holds(&mut e, "max(abs(d)) < 10^-40 and max(abs(d)) > -10^-40"));
    // variations: max of an exact number and a ball containing it, min of balls around 0
    assert!(holds(&mut e, "v = n(1, 30) + n(sqrt(2), 30) - n(sqrt(2), 30); abs(max(1, v) - 1) < 10^-25 and abs(min(v, 1) - 1) < 10^-25"));
    assert!(holds(&mut e, "z = d[1]; abs(min(abs(z), 0)) < 10^-40 and max(re(z), 0) < 10^-40 and min(im_part(z), 0) > -10^-40"));
    // decided comparisons keep the element itself
    assert_eq!(timed(&mut e, "[max(n(sqrt(2), 30), 2), min(1, 2, n(3, 30))]"), "[2, 1]");
}

#[test]
fn continued_fractions_of_quadratic_irrationals() {
    let mut e = CoreEngine::new();
    // (1 + sqrt(5))/2 is held as a rational multiple of a sum: it stayed unevaluated
    assert_eq!(timed(&mut e, "[continued_fraction((1 + sqrt(5))/2), continued_fraction((1 + sqrt(7))/3)]"), "[[1, [1]], [1, [4, 1, 1, 1]]]");
    // the integer part stands before the period also for a purely periodic expansion: [1, 4, 1, 1] repeated is
    // 1 followed by the period [4, 1, 1, 1]
    assert!(holds(&mut e, "v = (1 + sqrt(7))/3; abs(n(from_continued_fraction(continued_fraction(v)) - v, 40)) < 10^-35"));
    // independent: the periodic form gives the number back
    assert!(holds(&mut e, "v = -(2 + sqrt(3))/5; abs(n(from_continued_fraction(continued_fraction(v)) - v, 40)) < 10^-35"));
    assert_eq!(timed(&mut e, "continued_fraction((3 - sqrt(2))/4, 6)"), "[0, 2, 1, 1, 10, 1]");
}

#[test]
fn linear_algebra_short() {
    report("linalg", drive(&Run::short("linalg", 60), 0x11a1, linalg_input));
}

#[test]
fn algebra_short() {
    report("algebra", drive(&Run::short("algebra", 60), 0xa19e, algebra_input));
}

#[test]
fn lists_short() {
    report("lists", drive(&Run::short("lists", 150), 0x1157, lists_input));
}

#[test]
fn rewriting_short() {
    report("rewriting", drive(&Run::short("rewriting", 60), 0x5e11, rewrite_input));
}

#[test]
#[ignore]
fn campaign_linear_algebra() {
    report("linalg", drive(&Run::long("linalg"), 0x11a1, linalg_input));
}

#[test]
#[ignore]
fn campaign_algebra() {
    report("algebra", drive(&Run::long("algebra"), 0xa19e, algebra_input));
}

#[test]
#[ignore]
fn campaign_lists() {
    report("lists", drive(&Run::long("lists"), 0x1157, lists_input));
}

#[test]
#[ignore]
fn campaign_rewriting() {
    report("rewriting", drive(&Run::long("rewriting"), 0x5e11, rewrite_input));
}
