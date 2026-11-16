//! Numerical answers checked against exact values: five generators with fixed seeds make random inputs for the
//! numerical solvers and compare every answer with an exact value (or an independent method) at 100 digits. An
//! answer claims digits: a machine-precision answer the default goal (8 digits; a relative error of 1e-7 is
//! allowed), an answer at `working_precision: d` the precision it carries (`precision(v)`); the error must stay
//! within ten units of the last claimed digit.
//!
//! 1. **integrals**: `n_integrate` in one dimension (algebraic and logarithmic singularities at either end, at a
//!    stated interior point, slowly decaying and oscillatory tails, smooth integrands), two and three dimensions
//!    (products, triangles and simplices);
//! 2. **sums**: `n_sum` and `n_product` (zeta and eta values, logarithms, exponentials, telescoping sums, the
//!    sine and hyperbolic sine products, Euler's product for 1/(1 - x));
//! 3. **roots**: `find_root` (simple, double, two variables), `n_solve` (every root), `find_minimum`, `n_minimize`
//!    (with and without constraints);
//! 4. **ode**: `n_dsolve` (linear, nonlinear, oscillating, stiff, systems);
//! 5. **misc**: `interpolation` of cubic data, `fourier` of lists of 30 digits, derivatives in a parameter of
//!    special functions without a closed form (evaluated numerically) against a central difference at 100 digits.
//!
//! An answer that is a failure or stays unevaluated promises nothing and is not checked. The suite runs a short
//! version of each (`*_short`); the `campaign_*` tests (ignored) are the long runs by hand: `MC_SOUNDNESS_INPUTS`
//! inputs per generator (default 300), `MC_SOUNDNESS_SECONDS` a time cap per generator (default 1200),
//! `MC_SOUNDNESS_SEED` the seed; progress is printed and written to `MC_STATUS_DIR/numeric-<job>.txt` when that is
//! set.
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
    /// A random rational p/den with p in [lo, hi], written as it is (`7/12`, `-3/10`, `0/7`).
    fn q(&mut self, lo: i64, hi: i64, den: i64) -> String {
        let p = lo + (self.next() % (hi - lo + 1) as u64) as i64;
        format!("{p}/{den}")
    }
}

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// The elements of a printed list, split at the commas of its top level.
fn elements(printed: &str) -> Vec<String> {
    let s = printed.trim();
    let Some(inner) = s.strip_prefix('[').and_then(|s| s.strip_suffix(']')) else { return vec![] };
    let mut out = vec![];
    let (mut depth, mut cur) = (0i32, String::new());
    for ch in inner.chars() {
        match ch {
            '[' | '(' => depth += 1,
            ']' | ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
                continue;
            }
            _ => {}
        }
        cur.push(ch);
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// A printed real number (`1.25`, `-3`, `2/3`, `1.5*10^-7`, `1.5e-7`), or `None`.
fn real_of(s: &str) -> Option<f64> {
    let s = s.trim();
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
        Run { inputs, cap: Duration::from_secs(300), seed: 45, job, verbose: false }
    }
    fn long(job: &'static str) -> Run {
        let var = |k: &str, d: u64| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        Run {
            inputs: var("MC_SOUNDNESS_INPUTS", 300) as usize,
            cap: Duration::from_secs(var("MC_SOUNDNESS_SECONDS", 1200)),
            seed: var("MC_SOUNDNESS_SEED", 4500),
            job,
            verbose: true,
        }
    }
    /// Progress: printed every 10 inputs and written to the status folder.
    fn progress(&self, done: usize, faults: usize, checked: usize, start: Instant) {
        if !self.verbose || (!done.is_multiple_of(10) && done != self.inputs) {
            return;
        }
        let pct = 100 * done / self.inputs.max(1);
        eprintln!(
            "[{}] {done}/{} ({pct}%), {checked} checked, {faults} faults, {:.0} s",
            self.job,
            self.inputs,
            start.elapsed().as_secs_f64()
        );
        if let Ok(dir) = std::env::var("MC_STATUS_DIR") {
            let _ = std::fs::write(format!("{dir}/numeric-{}.txt", self.job), format!("{done}/{} {pct}%\n", self.inputs));
        }
    }
}

/// What one input gave: not checked (a failure, unevaluated), checked and right, or a fault.
enum Verdict {
    Skipped,
    Right,
    Fault(String),
}

/// One input of a generator.
type Input = fn(&mut CoreEngine, &mut Rng) -> Verdict;

/// Runs a generator: `run.inputs` inputs (or until the time cap) in one session, with progress. Gives the number
/// of inputs, of checked answers, and the faults.
fn drive(run: &Run, salt: u64, input: Input) -> (usize, usize, Vec<String>) {
    let mut e = CoreEngine::new();
    let mut r = Rng(run.seed ^ salt);
    let (start, mut faults, mut checked, mut done) = (Instant::now(), vec![], 0, 0);
    while done < run.inputs && start.elapsed() < run.cap {
        match input(&mut e, &mut r) {
            Verdict::Skipped => {}
            Verdict::Right => checked += 1,
            Verdict::Fault(f) => {
                eprintln!("FAULT {f}");
                checked += 1;
                faults.push(f);
            }
        }
        done += 1;
        run.progress(done, faults.len(), checked, start);
    }
    (done, checked, faults)
}

fn report(name: &str, (done, checked, faults): (usize, usize, Vec<String>)) {
    eprintln!("{name}: {done} inputs, {checked} answers checked, {} faults", faults.len());
    assert!(faults.is_empty(), "{name}: {} faults\n{}", faults.len(), faults.join("\n"));
}

/// Does the printed answer promise nothing (a failure, an unevaluated call of one of `heads`, an error)?
fn no_answer(a: &str, heads: &[&str]) -> bool {
    a.starts_with("error") || a.contains("failure(") || a.contains("indeterminate") || heads.iter().any(|h| a.contains(&format!("{h}(")))
}

/// The relative error allowed for an answer of precision `p` (`None`: machine precision).
fn allowed(p: Option<f64>) -> f64 {
    match p {
        Some(p) if p < 15.9 || p > 16.0 => 10f64.powf(1.0 - p),
        _ => 1e-7,
    }
}

/// Checks the value `v` (a variable of the session holding the answer) against the exact expression `exact`: the
/// relative error (absolute below magnitude 1e-30) against the error the answer's precision allows.
fn compare(e: &mut CoreEngine, label: &str, v: &str, exact: &str) -> Verdict {
    let p = real_of(&ev(e, &format!("precision({v})"))).filter(|&p| p < 1e6);
    // a machine answer also meets an absolute goal of 1e-8 (the default accuracy goal of the solvers)
    let floor = if allowed(p) == 1e-7 { "999/10000" } else { "10^-30" };
    let d = ev(
        e,
        &format!(
            "time_constrained({{ ex = n({exact}, 100); n(abs(set_precision({v}, inf) - ex)/max(abs(ex), {floor}), 5) }}, 60)"
        ),
    );
    let d = if d.starts_with("ball(0,") || d.starts_with("0.") && real_of(&d) == Some(0.0) { "0".to_string() } else { d };
    let Some(rel) = real_of(&d) else {
        return if d.contains("failure") || d.contains("time_constrained") {
            Verdict::Skipped
        } else {
            Verdict::Fault(format!("{label}: the difference to {exact} is {d}"))
        };
    };
    let tol = allowed(p);
    if rel > 10.0 * tol {
        let shown = ev(e, v);
        return Verdict::Fault(format!(
            "{label} = {shown} (precision {p:?}): relative error {rel:.3e} against {exact}, allowed {tol:.1e}"
        ));
    }
    Verdict::Right
}

/// Evaluates `call` into the session variable `va` (with a time limit) and checks it against `exact`, unless it
/// is no answer (`heads` the functions whose unevaluated calls promise nothing). A value with an error estimate
/// `around(v, d)` is checked as v within six times d.
fn check(e: &mut CoreEngine, call: &str, exact: &str, heads: &[&str]) -> Verdict {
    let a = ev(e, &format!("va = time_constrained({call}, 40); va"));
    if no_answer(&a, heads) || a.contains("time_constrained") {
        return Verdict::Skipped;
    }
    if a.starts_with("around(") {
        let d = ev(e, &format!("n(abs(va[1] - n({exact}, 60)) - 6*va[2], 5)"));
        return match real_of(&d) {
            Some(x) if x > 0.0 => Verdict::Fault(format!("{call} = {a}: off {exact} by more than six error estimates")),
            _ => Verdict::Right,
        };
    }
    compare(e, call, "va", exact)
}

/// A working precision option (`""` for machine precision), and the digits of that precision.
fn precision_option(r: &mut Rng) -> &'static str {
    r.pick(&["", "", ", working_precision: 20", ", working_precision: 30"])
}

// ---------------------------------------------------------------------------------------------------------------
// 1. integrals

/// A one-dimensional integral: the integrand, the range (after the variable) and the exact value. The parameters
/// are random rationals in the ranges where the exact value holds.
fn integral_1d(r: &mut Rng) -> (String, String, String) {
    let s = r.q(-21, 21, 7);
    let l = r.q(1, 24, 8);
    let b = format!("({s}) + ({l})");
    match r.below(12) {
        // algebraic singularities at both ends, the ends anywhere
        0 | 1 => {
            let (a, c) = (r.q(-9, 24, 12), r.q(-9, 24, 12));
            (
                format!("(x - ({s}))^({a})*({b} - x)^({c})"),
                format!("{s}, {b}"),
                format!("({l})^({a} + {c} + 1)*gamma({a} + 1)*gamma({c} + 1)/gamma({a} + {c} + 2)"),
            )
        }
        // a logarithm at the left or right end
        2 => {
            let a = r.q(-8, 12, 12);
            let f = if r.below(2) == 0 {
                format!("log(x - ({s}))*(x - ({s}))^({a})")
            } else {
                format!("log({b} - x)*({b} - x)^({a})")
            };
            (f, format!("{s}, {b}"), format!("({l})^({a} + 1)*(log({l})/({a} + 1) - 1/({a} + 1)^2)"))
        }
        // a singular point inside, stated or not
        3 => {
            let a = r.q(-9, -1, 12);
            let c = r.q(1, 10, 11);
            let (lo, hi) = ("0", "1");
            let range = if r.below(3) > 0 { format!("{lo}, {c}, {hi}") } else { format!("{lo}, {hi}") };
            (
                format!("abs(x - {c})^({a})"),
                range,
                format!("(({c})^({a} + 1) + (1 - {c})^({a} + 1))/({a} + 1)"),
            )
        }
        // slowly decaying algebraic tails
        4 => {
            let p = r.q(13, 40, 12);
            let c = r.q(0, 20, 7);
            let s = *r.pick(&["1", "2"]);
            (format!("1/(x + {c})^({p})"), format!("{s}, inf"), format!("1/(({p}) - 1)/({s} + {c})^(({p}) - 1)"))
        }
        // gamma integrals: a singular end and an infinite range
        5 => {
            let a = r.q(-9, 30, 12);
            let k = r.q(1, 30, 10);
            (format!("x^({a})*exp(-({k})*x)"), "0, inf".into(), format!("gamma({a} + 1)/({k})^({a} + 1)"))
        }
        // oscillatory tails
        6 | 7 => {
            let k = r.q(1, 40, 10);
            match r.below(4) {
                0 => (format!("sin({k}*x)/x"), "0, inf".into(), "pi/2".into()),
                1 => {
                    let c = r.q(1, 20, 10);
                    (format!("cos({k}*x)/(x^2 + ({c})^2)"), "0, inf".into(), format!("pi*exp(-({k})*({c}))/(2*({c}))"))
                }
                2 => {
                    let p = if r.below(2) == 0 { r.q(2, 11, 12) } else { r.q(13, 22, 12) };
                    (format!("sin({k}*x)*x^(-({p}))"), "0, inf".into(), format!("({k})^(({p}) - 1)*gamma(1 - ({p}))*cos(pi*({p})/2)"))
                }
                _ => (format!("cos({k}*x)*exp(-x)"), "0, inf".into(), format!("1/(1 + ({k})^2)")),
            }
        }
        // the whole line, a logarithm on the half-line
        8 => {
            if r.below(2) == 0 {
                let (a, m) = (r.q(1, 30, 10), r.q(-30, 30, 10));
                (format!("exp(-({a})*(x - ({m}))^2)"), "-inf, inf".into(), format!("sqrt(pi/({a}))"))
            } else {
                let c = r.q(1, 30, 10);
                (format!("log(x)/(x^2 + ({c})^2)"), "0, inf".into(), format!("pi*log({c})/(2*({c}))"))
            }
        }
        // both ends singular and rounding
        9 => {
            let a = r.q(-11, 20, 12);
            (format!("(1 - x^2)^({a})"), "-1, 1".into(), format!("sqrt(pi)*gamma({a} + 1)/gamma({a} + 3/2)"))
        }
        // a logarithm at a rounding end
        10 => {
            let a = r.q(1, 30, 10);
            (format!("x^(({a}) - 1)*log(1 - x)"), "0, 1".into(), format!("-(digamma({a} + 1) + euler_gamma)/({a})"))
        }
        // smooth
        _ => {
            let k = if r.below(2) == 0 { r.q(1, 30, 10) } else { r.q(-30, -1, 10) };
            if r.below(2) == 0 {
                (format!("exp({k}*x)"), format!("{s}, {b}"), format!("(exp({k}*({b})) - exp({k}*({s})))/({k})"))
            } else {
                (format!("cos({k}*x)"), format!("{s}, {b}"), format!("(sin({k}*({b})) - sin({k}*({s})))/({k})"))
            }
        }
    }
}

/// A smooth integrand of `v` on a finite range, with its exact integral.
fn smooth(r: &mut Rng, v: &str) -> (String, String, String) {
    let (lo, hi) = (*r.pick(&["0", "-1", "1/2"]), *r.pick(&["1", "2", "5/2"]));
    match r.below(3) {
        0 => {
            let k = *r.pick(&["1", "-1/2", "3"]);
            (format!("exp({k}*{v})"), format!("{lo}, {hi}"), format!("(exp({k}*({hi})) - exp({k}*({lo})))/({k})"))
        }
        1 => (format!("{v}^2"), format!("{lo}, {hi}"), format!("(({hi})^3 - ({lo})^3)/3")),
        _ => (format!("1/(1 + {v}^2)"), format!("{lo}, {hi}"), format!("atan({hi}) - atan({lo})")),
    }
}

fn integral_input(e: &mut CoreEngine, r: &mut Rng) -> Verdict {
    let heads = ["n_integrate"];
    match r.below(10) {
        0 | 1 => {
            // a product over a rectangle, or a triangle
            if r.below(2) == 0 {
                let (f, rf, xf) = smooth(r, "x");
                let (g, rg, xg) = smooth(r, "y");
                check(e, &format!("n_integrate({f}*{g}, [x, {rf}], [y, {rg}])"), &format!("({xf})*({xg})"), &heads)
            } else {
                let (a, b) = (*r.pick(&["0", "1", "2", "1/2"]), *r.pick(&["0", "1", "-1/2", "1/2"]));
                check(
                    e,
                    &format!("n_integrate(x^({a})*y^({b}), [x, 0, 1], [y, 0, x])"),
                    &format!("1/((({b}) + 1)*(({a}) + ({b}) + 2))"),
                    &heads,
                )
            }
        }
        2 => {
            // a simplex in three dimensions
            let (a, b, c) = (*r.pick(&["0", "1", "1/2"]), *r.pick(&["0", "2"]), *r.pick(&["0", "1", "-1/2"]));
            check(
                e,
                &format!("n_integrate(x^({a})*y^({b})*z^({c}), [x, 0, 1], [y, 0, x], [z, 0, y])"),
                &format!("1/((({c}) + 1)*(({b}) + ({c}) + 2)*(({a}) + ({b}) + ({c}) + 3))"),
                &heads,
            )
        }
        _ => {
            let (f, range, exact) = integral_1d(r);
            let wp = precision_option(r);
            check(e, &format!("n_integrate({f}, [x, {range}]{wp})"), &exact, &heads)
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// 2. sums and products

fn sum_input(e: &mut CoreEngine, r: &mut Rng) -> Verdict {
    let wp = precision_option(r);
    let (call, exact) = match r.below(11) {
        0 => {
            let s = r.q(11, 40, 10);
            (format!("n_sum(1/k^({s}), [k, 1, inf]{wp})"), format!("zeta({s})"))
        }
        1 => {
            let s = r.q(1, 30, 10);
            let exact = if s == "10/10" { "-log(2)".to_string() } else { format!("-(1 - 2^(1 - ({s})))*zeta({s})") };
            (format!("n_sum((-1)^k/k^({s}), [k, 1, inf]{wp})"), exact)
        }
        2 => {
            let x = r.q(-19, 19, 20);
            (format!("n_sum(({x})^k/k, [k, 1, inf]{wp})"), format!("-log(1 - ({x}))"))
        }
        3 => {
            let a = r.q(1, 30, 10);
            (format!("n_sum(1/(k^2 + ({a})^2), [k, 1, inf]{wp})"), format!("(pi*({a})*coth(pi*({a})) - 1)/(2*({a})^2)"))
        }
        4 => {
            let x = r.q(-60, 60, 7);
            (format!("n_sum(({x})^k/factorial(k), [k, 0, inf]{wp})"), format!("exp({x})"))
        }
        5 => {
            let a = r.q(1, 30, 10);
            (format!("n_sum(1/((k + {a})*(k + {a} + 1)), [k, 0, inf]{wp})"), format!("1/({a})"))
        }
        6 => {
            let a = r.q(1, 25, 10);
            (format!("n_product(1 + ({a})^2/k^2, [k, 1, inf]{wp})"), format!("sinh(pi*({a}))/(pi*({a}))"))
        }
        7 => {
            let a = r.q(1, 25, 11);
            (format!("n_product(1 - ({a})^2/k^2, [k, 1, inf]{wp})"), format!("sin(pi*({a}))/(pi*({a}))"))
        }
        8 => (format!("n_product(1 - 1/k^2, [k, 2, inf]{wp})"), "1/2".into()),
        9 => {
            let x = r.q(-9, 9, 10);
            (format!("n_product(1 + ({x})^(2^k), [k, 0, inf]{wp})"), format!("1/(1 - ({x}))"))
        }
        _ => {
            let x = r.q(-9, 9, 10);
            (format!("n_sum(k*({x})^k, [k, 1, inf]{wp})"), format!("({x})/(1 - ({x}))^2"))
        }
    };
    check(e, &call, &exact, &["n_sum", "n_product"])
}

// ---------------------------------------------------------------------------------------------------------------
// 3. roots and minima

/// Every element of the printed list of rules `[[x -> r1], ...]` lies near one of the exact roots, and every exact
/// root is found.
fn check_roots(e: &mut CoreEngine, call: &str, roots: &[String]) -> Verdict {
    let a = ev(e, &format!("sol = time_constrained({call}, 40); sol"));
    if no_answer(&a, &["n_solve"]) || a.contains("time_constrained") {
        return Verdict::Skipped;
    }
    let n = elements(&a).len();
    if n != roots.len() {
        return Verdict::Fault(format!("{call} = {a}: {n} roots, {} expected", roots.len()));
    }
    let exact = format!("[{}]", roots.join(", "));
    for k in 1..=n {
        // the exact root nearest to the k-th answer
        ev(e, &format!("rk = x /. sol[{k}]"));
        let near = ev(e, &format!("ex = n({exact}, 60); ex[ordering(map(z => abs(set_precision(rk, inf) - z), ex))[1]]"));
        if let Verdict::Fault(f) = compare(e, &format!("{call}, root {k}"), "rk", &near) {
            return Verdict::Fault(f);
        }
    }
    Verdict::Right
}

fn root_input(e: &mut CoreEngine, r: &mut Rng) -> Verdict {
    let wp = precision_option(r);
    match r.below(9) {
        0 => {
            let c = r.q(1, 80, 7);
            check(e, &format!("x /. find_root(exp(x) - {c}, [x, 1]{wp})"), &format!("log({c})"), &["find_root"])
        }
        1 => {
            let c = r.q(1, 80, 7);
            check(e, &format!("x /. find_root(x^3 - {c}, [x, 1]{wp})"), &format!("({c})^(1/3)"), &["find_root"])
        }
        2 => {
            // a double root: Newton's method converges linearly, the answer has about half the digits
            let c = r.q(-20, 20, 7);
            check(e, &format!("x /. find_root((x - ({c}))^2*(x + 5), [x, ({c}) + 1/2]{wp})"), &c, &["find_root"])
        }
        3 => {
            let c = r.q(8, 60, 7);
            check(e, &format!("x /. find_root(cosh(x) - {c}, [x, 1]{wp})"), &format!("acosh({c})"), &["find_root"])
        }
        4 => {
            let c = r.q(1, 30, 10);
            check(e, &format!("x /. find_root([x^2 + y^2 == ({c})^2, x == y], [[x, 1], [y, 1]]{wp})"), &format!("({c})/sqrt(2)"), &["find_root"])
        }
        5 => {
            let c = *r.pick(&["2", "5", "-3", "1/7"]);
            let w = "exp(2*pi*im/3)";
            let roots = vec![format!("({c})^(1/3)"), format!("({c})^(1/3)*{w}"), format!("({c})^(1/3)*conjugate({w})")];
            let roots = if let Some(m) = c.strip_prefix('-') {
                vec![format!("-({m})^(1/3)"), format!("-({m})^(1/3)*{w}"), format!("-({m})^(1/3)*conjugate({w})")]
            } else {
                roots
            };
            check_roots(e, &format!("n_solve(x^3 - ({c}) == 0, x{wp})"), &roots)
        }
        6 => {
            let (a, b) = (*r.pick(&["1/3", "2", "-5/2"]), *r.pick(&["7", "-1", "1/9"]));
            check_roots(e, &format!("n_solve((x - ({a}))*(x - ({b}))*(x^2 + 1) == 0, x{wp})"), &[a.into(), b.into(), "im".into(), "-im".into()])
        }
        7 => {
            let (c, m) = (r.q(-20, 20, 7), r.q(-20, 20, 7));
            if r.below(2) == 0 {
                check(e, &format!("first(find_minimum(cosh(x - ({c})) + ({m}), [x, 0]{wp}))"), &format!("1 + ({m})"), &["find_minimum"])
            } else {
                check(e, &format!("x /. last(find_minimum(cosh(x - ({c})) + ({m}), [x, 0]{wp}))"), &c, &["find_minimum"])
            }
        }
        _ => {
            let c = r.q(1, 30, 10);
            if r.below(2) == 0 {
                check(e, &format!("first(n_minimize([x + y, x^2 + y^2 <= ({c})^2], [x, y]{wp}))"), &format!("-({c})*sqrt(2)"), &["n_minimize"])
            } else {
                check(e, &format!("first(n_minimize((x - 1)^2 + (y - ({c}))^4 + ({c}), [x, y]{wp}))"), &c, &["n_minimize"])
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// 4. differential equations

fn ode_input(e: &mut CoreEngine, r: &mut Rng) -> Verdict {
    let wp = precision_option(r);
    let t = r.q(1, 50, 10);
    let (eqs, fun, at, exact) = match r.below(6) {
        0 => {
            let k = r.q(-30, 20, 10);
            (format!("[y'(t) == ({k})*y(t), y(0) == 3]"), "y", format!("y({t})"), format!("3*exp(({k})*({t}))"))
        }
        1 => {
            let w = r.q(1, 50, 10);
            (format!("[y''(t) + ({w})^2*y(t) == 0, y(0) == 1, y'(0) == 0]"), "y", format!("y({t})"), format!("cos(({w})*({t}))"))
        }
        2 => {
            let t = *r.pick(&["1", "3/2", "19/10"]);
            return ode_check(e, "[y'(t) == y(t)^2, y(0) == 1/2]", "y", &format!("y({t})"), &format!("1/(2 - ({t}))"), t, wp);
        }
        3 => ("[y'(t) == -2*t*y(t), y(0) == 1]".into(), "y", format!("y({t})"), format!("exp(-({t})^2)")),
        4 => ("[x'(t) == -y(t), y'(t) == x(t), x(0) == 1, y(0) == 0]".into(), "[x, y]", format!("y({t})"), format!("sin({t})")),
        _ => {
            // stiff: y' = -1000 (y - cos t), y(0) = 0
            let c = "1000";
            (
                format!("[y'(t) == -{c}*(y(t) - cos(t)), y(0) == 0]"),
                "y",
                format!("y({t})"),
                format!("({c}^2*cos({t}) + {c}*sin({t}) - {c}^2*exp(-{c}*({t})))/({c}^2 + 1)"),
            )
        }
    };
    ode_check(e, &eqs, fun, &at, &exact, &t, wp)
}

fn ode_check(e: &mut CoreEngine, eqs: &str, fun: &str, at: &str, exact: &str, t: &str, wp: &str) -> Verdict {
    let call = format!("{at} /. n_dsolve({eqs}, {fun}, [t, 0, {t}]{wp})");
    let v = check(e, &call, exact, &["n_dsolve"]);
    // a value that comes back as a list of one element is checked through it
    if let Verdict::Fault(f) = &v {
        if f.contains("the difference") {
            let a = ev(e, "va");
            if a.starts_with('[') && elements(&a).len() == 1 {
                ev(e, "va = va[1]");
                return compare(e, &call, "va", exact);
            }
        }
    }
    v
}

// ---------------------------------------------------------------------------------------------------------------
// 5. interpolation, Fourier transforms, numerical derivatives

fn misc_input(e: &mut CoreEngine, r: &mut Rng) -> Verdict {
    match r.below(3) {
        0 => {
            // cubic data: the piecewise cubic interpolation is the cubic itself
            let c: Vec<i64> = (0..4).map(|_| r.below(11) as i64 - 5).collect();
            let p = format!("({}) + ({})*X + ({})*X^2 + ({})*X^3", c[0], c[1], c[2], c[3]);
            let data: Vec<String> = (0..6).map(|k| format!("[{k}, {}]", p.replace('X', &format!("({k})")))).collect();
            let at = *r.pick(&["1/2", "13/7", "3.25", "4.9", "2"]);
            check(e, &format!("interpolation([{}])({at})", data.join(", ")), &p.replace('X', &format!("({at})")), &["interpolation"])
        }
        1 => {
            // the transform of 30-digit data: each element against the exact sum
            let n = 3 + r.below(6);
            let u: Vec<&str> = (0..n).map(|_| *r.pick(&["1", "-2", "1/3", "5/7", "0", "3"])).collect();
            let list = format!("[{}]", u.join(", "));
            let a = ev(e, &format!("va = fourier(n({list}, 30)); va"));
            if no_answer(&a, &["fourier"]) {
                return Verdict::Skipped;
            }
            for s in 0..n {
                let terms: Vec<String> = u.iter().enumerate().map(|(k, x)| format!("({x})*exp(2*pi*im*{}/{n})", k * s)).collect();
                let exact = format!("({})/sqrt({n})", terms.join(" + "));
                ev(e, &format!("vs = va[{}]", s + 1));
                if let Verdict::Fault(f) = compare(e, &format!("fourier(n({list}, 30)), element {}", s + 1), "vs", &exact) {
                    return Verdict::Fault(f);
                }
            }
            Verdict::Right
        }
        _ => {
            // a derivative in a parameter, evaluated numerically, against a central difference
            let f = *r.pick(&[
                "bessel_j(A, 2)", "zeta(A)", "polylog(A, 1/2)", "legendre_p(A, 1/3)", "hypergeometric_1f1(A, 3/2, 1)",
                "bessel_k(A, 3)", "gamma(A, 2)", "zeta(A, 1/3)",
            ]);
            let a0 = *r.pick(&["1/3", "sqrt(5)", "5/2", "7/4"]);
            let d = *r.pick(&["20", "30", "40"]);
            let call = format!("n(d({}, a) /. a -> {a0}, {d})", f.replace('A', "a"));
            let h = "10^-30";
            let exact = format!(
                "(({}) - ({}))/(2*{h})",
                f.replace('A', &format!("({a0} + {h})")),
                f.replace('A', &format!("({a0} - {h})"))
            );
            let a = ev(e, &format!("va = time_constrained({call}, 40); va"));
            if real_of(&a).is_none() {
                return Verdict::Skipped;
            }
            compare(e, &call, "va", &exact)
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// tests

#[test]
fn integrals_short() {
    report("integrals", drive(&Run::short("integrals", 40), 0x1a7e, integral_input));
}

#[test]
fn sums_short() {
    report("sums", drive(&Run::short("sums", 25), 0x5a5a, sum_input));
}

#[test]
fn roots_short() {
    report("roots", drive(&Run::short("roots", 25), 0x2007, root_input));
}

#[test]
fn ode_short() {
    report("ode", drive(&Run::short("ode", 20), 0x0de0, ode_input));
}

#[test]
fn misc_short() {
    report("misc", drive(&Run::short("misc", 20), 0x3150, misc_input));
}

#[test]
#[ignore]
fn campaign_integrals() {
    report("integrals", drive(&Run::long("integrals"), 0x1a7e, integral_input));
}

#[test]
#[ignore]
fn campaign_sums() {
    report("sums", drive(&Run::long("sums"), 0x5a5a, sum_input));
}

#[test]
#[ignore]
fn campaign_roots() {
    report("roots", drive(&Run::long("roots"), 0x2007, root_input));
}

#[test]
#[ignore]
fn campaign_ode() {
    report("ode", drive(&Run::long("ode"), 0x0de0, ode_input));
}

#[test]
#[ignore]
fn campaign_misc() {
    report("misc", drive(&Run::long("misc"), 0x3150, misc_input));
}

// ---------------------------------------------------------------------------------------------------------------
// the defects found, each with two variations checked by an independent route

/// Is `src` true?
fn holds(e: &mut CoreEngine, src: &str) -> bool {
    ev(e, src) == "true"
}

#[test]
fn integrals_keep_the_mass_at_singular_ends() {
    let mut e = CoreEngine::new();
    // the reported input: 3, not 2.9999879 as converged
    assert!(holds(&mut e, "abs(n_integrate((1 - x)^(-2/3), [x, 0, 1]) - 3) < 10^-12"));
    assert!(holds(&mut e, "abs(n_integrate((1 - x)^(-2/3), [x, 0, 1], working_precision: 30) - 3) < 10^-28"));
    // a singular right end at 20 digits (was right to 14, claiming 16.7): against sqrt(2)
    assert!(holds(&mut e, "v = n_integrate((3/2 - x)^(-1/2), [x, 1, 3/2], working_precision: 20); abs(set_precision(v, inf) - sqrt(2)) < 10^-(precision(v) - 1)"));
    // both ends singular, the cancellation 1 - (1 - u)^2 at each: against pi/2 and the beta function
    assert!(holds(&mut e, "abs(n_integrate(1/sqrt(1 - x^2), [x, 0, 1]) - n(pi/2)) < 10^-13"));
    assert!(holds(&mut e, "abs(n_integrate((1 - x^2)^(-3/4), [x, -1, 1]) - n(sqrt(pi)*gamma(1/4)/gamma(3/4))) < 10^-10"));
    // a stated singular point that rounds (1/3)
    assert!(holds(&mut e, "abs(n_integrate(abs(x - 1/3)^(-1/2), [x, 0, 1/3, 1]) - n(2/sqrt(3) + 2*sqrt(2/3))) < 10^-12"));
    // a divergent end is a failure, not a value
    assert!(ev(&mut e, "n_integrate(1/(1 - x), [x, 0, 1])").starts_with("failure(not_converged"));
}

#[test]
fn infinite_sums_with_parameters_carry_their_condition() {
    let mut e = CoreEngine::new();
    // on the unit circle the terms p^k/k converge everywhere but at p = 1 (Dirichlet's test)
    assert_eq!(ev(&mut e, "sum(p^k/k, [k, 1, inf])"), "conditional_expression(-log(1 - p), abs(p) <= 1 and p != 1)");
    // nowhere on the circle (the terms do not tend to 0), everywhere on it (absolutely)
    assert_eq!(ev(&mut e, "sum(p^k, [k, 1, inf])"), "conditional_expression(-1*p/(-1 + p), abs(p) < 1)");
    assert_eq!(ev(&mut e, "sum(p^k/k^2, [k, 1, inf])"), "conditional_expression(polylog(2, p), abs(p) <= 1)");
    assert_eq!(ev(&mut e, "sum(1/k^p, [k, 1, inf])"), "conditional_expression(zeta(p), re(p) > 1)");
    assert_eq!(ev(&mut e, "sum(p^k/k, [k, 1, inf], generate_conditions: false)"), "-log(1 - p)");
    assert_eq!(ev(&mut e, "sum(x^k, [k, 0, inf], assumptions: abs(x) < 1/2)"), "(1 - x)^-1");
    // inside the condition the value is the sum, at a point checked by n_sum
    assert!(holds(&mut e, "abs(n(sum(p^k/k, [k, 1, inf]) /. p -> -1/3) - n_sum((-1/3)^k/k, [k, 1, inf])) < 10^-12"));
    // a product: its factors less 1 sum with |x| < 1
    assert_eq!(ev(&mut e, "product(1 + x^k, [k, 1, inf])"), "conditional_expression((1/2)*q_pochhammer(-1, x), abs(x) < 1)");
    // no parameter: no condition
    assert_eq!(ev(&mut e, "sum(1/k^2, [k, 1, inf])"), "(1/6)*pi^2");
    // the bilateral geometric series converges for no x (it gave 1/(1 - x) + 1/(x - 1) = 0)
    assert_eq!(ev(&mut e, "sum(x^k, [k, -inf, inf])"), "sum(x^k, [k, -inf, inf])");
    // from -inf: x/(x - 1) for |x| > 1 (it gave x^-inf forms), at x = 3 against n_sum
    assert_eq!(ev(&mut e, "sum(x^k, [k, -inf, 0])"), "conditional_expression(x/(-1 + x), abs(x) > 1)");
    assert!(holds(&mut e, "abs(n(sum(x^k, [k, -inf, 0]) /. x -> 3) - n_sum(3^-k, [k, 0, inf])) < 10^-12"));
    // a bilateral sum that converges keeps its value: (1/2)^|k| = 3, checked by n_sum
    assert!(holds(&mut e, "abs(n(sum(1/(k^2 + 1), [k, -inf, inf])) - n_sum(1/(k^2 + 1), [k, -inf, inf])) < 10^-12"));
}

#[test]
fn convolutions_keep_their_values_at_the_breaks() {
    let mut e = CoreEngine::new();
    ev(&mut e, "c = convolve(pdf(uniform_distribution([0, 1]), x), pdf(uniform_distribution([0, 1]), x), x, y)");
    // the triangle: 1 at its peak y = 1 (it was 0 there), 0 at the ends; against the integral at the breaks
    assert_eq!(ev(&mut e, "[c /. y -> 0, c /. y -> 1, c /. y -> 2, c /. y -> 1/2]"), "[0, 1, 0, 1/2]");
    assert!(holds(&mut e, "abs(n_integrate(boole(0 < x and x < 1)*boole(0 < 1 - x and 1 - x < 1), [x, 0, 1]) - 1) < 10^-8"));
    // a box of width 2 with itself: 2 at y = 2, and a piecewise of two pieces with a jump: continuous at the breaks
    ev(&mut e, "c2 = convolve(piecewise([[1, 0 < x and x < 2]], 0), piecewise([[1, 0 < x and x < 2]], 0), x, y)");
    assert_eq!(ev(&mut e, "[c2 /. y -> 2, c2 /. y -> 4, c2 /. y -> 1]"), "[2, 0, 1]");
    ev(&mut e, "c3 = convolve(piecewise([[1, 0 < x and x < 1], [2, 1 < x and x < 2]], 0), piecewise([[1, 0 < x and x < 1]], 0), x, y)");
    for y in ["1", "2", "3"] {
        assert!(
            holds(&mut e, &format!("(c3 /. y -> {y}) == limit(c3, y -> {y}, direction: from_below)")),
            "{}",
            ev(&mut e, &format!("[c3 /. y -> {y}, limit(c3, y -> {y}, direction: from_below)]"))
        );
    }
}

#[test]
fn roots_at_zero_claim_only_the_digits_they_have() {
    let mut e = CoreEngine::new();
    // the residual exp(x) - 1 at x = -2.8e-46 is 0 +- 7e-46 (noise): the step from it is not a zero step, and the root
    // claimed 30 digits of -2.8e-46 (accuracy 76) where its error is 2.8e-46
    for f in ["exp(x) - 1", "log(1 + x)", "exp(2*x) - 1 - x"] {
        let src = format!("v = x /. find_root({f}, [x, 1/2], working_precision: 30); abs(set_precision(v, inf)) <= 10^-accuracy(v)");
        assert!(holds(&mut e, &src), "{f}: {}", ev(&mut e, "[v, accuracy(v)]"));
    }
    // a root away from 0 keeps its 30 digits
    assert!(holds(&mut e, "v = x /. find_root(exp(x) - 3, [x, 1], working_precision: 30); precision(v) >= 29 and abs(set_precision(v, inf) - log(3)) < 10^-29"));
}

#[test]
fn fourier_of_high_precision_data_is_evaluated() {
    let mut e = CoreEngine::new();
    // the imaginary parts 0 +- 9e-36 left the test for conjugate-even data (and the whole result) an unevaluated if
    assert!(holds(&mut e, "v = fourier(n([1/3, 0, 1, 0], 30)); is_list(v) and abs(set_precision(v[1], inf) - 2/3) < 10^-29 and abs(set_precision(v[2], inf) + 1/3) < 10^-29"));
    // data that is not conjugate-even, against the sum by definition; a machine list is unchanged
    assert!(holds(&mut e, "v = fourier(n([5/7, -2, 1, 3], 30)); abs(set_precision(v[2], inf) - (5/7 - 2*im - 1 - 3*im)/2) < 10^-29"));
    assert_eq!(ev(&mut e, "fourier([1., 2., 1.])[1]"), "complex(2.309401076758503, 0.0)");
}
