//! Answers that claim to be proved, checked against numbers. Four generators with fixed seeds make random
//! inputs and check each answer by an independent route, at 30 digits or more:
//!
//! 1. **properties**: `function_sign`, `function_monotonicity`, `function_convexity`, `function_injective` of the
//!    functions of the property table and of random compositions (with polynomials, sums, products, restrictions to
//!    intervals, parameters); a claimed sign, direction, convexity or injectivity is tested on dense and random
//!    exact points of the domain, at its ends and near its poles;
//! 2. **enclosures**: `interval` and `centered_interval` images (real, and complex boxes): every sampled point of the
//!    argument has its value inside the result;
//! 3. **calculus**: limits (both sides), series coefficients (the remainder's order), definite integrals, sums and
//!    products in closed form, residues, solutions of differential equations put back into the equation;
//! 4. **exact against approximate**: an exact form of a special function at an exceptional parameter against the
//!    function evaluated directly at 30-digit arguments.
//!
//! The suite runs a short version of each (`*_short`); `campaign` (ignored) is the long run by hand:
//! `MC_SOUNDNESS_INPUTS` inputs per generator (default 400), `MC_SOUNDNESS_SECONDS` a time cap per generator
//! (default 1200), `MC_SOUNDNESS_SEED` the seed; it prints its progress and writes `done/total` to
//! `MC_STATUS_DIR/<job>.txt` when that is set, and lists every contradiction found (an answer that stays unevaluated
//! promises nothing and is not checked).
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

fn ev(e: &mut CoreEngine, src: &str) -> String {
    trace(&format!("> {src}"));
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

/// A printed real number (`1.25`, `-3`, `2/3`, `1.5*10^-7`, `1.5e-7`, with a precision mark), or `None`.
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

/// Is the printed `s` one call of `head` (not a larger expression that starts with one)?
fn single_call(s: &str, head: &str) -> bool {
    let Some(rest) = s.strip_prefix(head).and_then(|r| r.strip_prefix('(')) else { return false };
    let mut depth = 1;
    for (k, ch) in rest.char_indices() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => {
                depth -= 1;
                if depth == 0 {
                    return k + 1 == rest.len();
                }
            }
            _ => {}
        }
    }
    false
}

/// `v` rounded to a multiple of 2^-20 (exact sample points).
fn snap(v: f64) -> f64 {
    (v * 1048576.0).round() / 1048576.0
}

/// The rational number of `snap(v)`.
fn rat(v: f64) -> String {
    let k = (v * 1048576.0).round() as i64;
    if k % 1048576 == 0 {
        format!("({})", k / 1048576)
    } else {
        format!("({k}/1048576)")
    }
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
        Run { inputs, cap: Duration::from_secs(240), seed: 44, job, verbose: false }
    }
    fn long(job: &'static str) -> Run {
        let var = |k: &str, d: u64| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        Run {
            inputs: var("MC_SOUNDNESS_INPUTS", 400) as usize,
            cap: Duration::from_secs(var("MC_SOUNDNESS_SECONDS", 1200)),
            seed: var("MC_SOUNDNESS_SEED", 4400),
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
            let _ = std::fs::write(format!("{dir}/soundness-{}.txt", self.job), format!("{done}/{} {pct}%\n", self.inputs));
        }
    }
}

/// The values of `f` (a template in `X`) at the exact points, at `digits` digits: `None` where the value is not a
/// real number (outside the domain, a pole, unevaluated).
fn values(e: &mut CoreEngine, f: &str, pts: &[String], digits: u32) -> Vec<Option<f64>> {
    let list: Vec<String> = pts.iter().map(|p| format!("n({}, {digits})", f.replace('X', p))).collect();
    let out = ev(e, &format!("time_constrained([{}], 20)", list.join(", ")));
    let els = elements(&out);
    if els.len() != pts.len() {
        return vec![None; pts.len()];
    }
    els.iter().map(|s| real_of(s)).collect()
}

// ---------------------------------------------------------------------------------------------------------------
// 1. properties

/// Functions of one argument (templates in `X`), mostly from the property table, some with a parameter.
const PROP_FUNS: &[&str] = &[
    "exp(X)", "log(X)", "sqrt(X)", "sin(X)", "cos(X)", "tan(X)", "atan(X)", "acot(X)", "tanh(X)", "sinh(X)",
    "cosh(X)", "asinh(X)", "acosh(X)", "atanh(X)", "sech(X)", "csch(X)", "coth(X)", "erf(X)", "erfc(X)", "erfi(X)",
    "gamma(X)", "log_gamma(X)", "digamma(X)", "factorial(X)", "bessel_j(0, X)", "bessel_j(1, X)", "bessel_j(5/2, X)",
    "bessel_y(0, X)", "bessel_i(1, X)", "bessel_i(0, X)", "bessel_k(0, X)", "bessel_k(3/2, X)", "airy_ai(X)",
    "airy_bi(X)", "airy_ai_prime(X)", "scorer_hi(X)", "scorer_gi(X)", "dawson_f(X)", "fresnel_s(X)", "fresnel_c(X)",
    "sin_integral(X)", "cos_integral(X)", "sinh_integral(X)", "cosh_integral(X)", "exp_integral_ei(X)",
    "exp_integral_e(2, X)", "log_integral(X)", "product_log(X)", "elliptic_k(X)", "elliptic_e(X)", "zeta(X)",
    "polylog(2, X)", "polylog(3, X)", "logistic_sigmoid(X)", "gudermannian(X)", "inverse_gudermannian(X)",
    "inverse_erf(X)", "inverse_erfc(X)", "abs(X)", "floor(X)", "ceiling(X)", "round(X)", "fractional_part(X)",
    "unit_step(X)", "ramp(X)", "sign(X)", "clip(X)", "struve_h(0, X)", "struve_l(1, X)", "kelvin_ber(X)",
    "kelvin_kei(X)", "jacobi_sn(X, 1/3)", "jacobi_dn(X, 1/2)", "jacobi_nd(X, 1/2)", "inverse_jacobi_sn(X, 1/2)",
    "inverse_jacobi_cn(X, 1/3)", "jacobi_amplitude(X, 1/2)", "elliptic_f(X, 1/2)", "barnes_g(X)", "log_barnes_g(X)",
    "hypergeometric_1f1(1/2, 3/2, X)", "hypergeometric_1f1(2, 1, X)", "hypergeometric_0f1(2, X)",
    "hypergeometric_2f1(1, 1, 2, X)", "hypergeometric_u(1, 2, X)", "spherical_bessel_j(1, X)",
    "parabolic_cylinder_d(1/2, X)", "parabolic_cylinder_d(-1, X)", "mittag_leffler_e(1/2, X)", "lommel_s1(1/2, 3/2, X)",
    "elliptic_nome_q(X)", "gamma_regularized(2, X)", "beta_regularized(X, 2, 3)", "beta(X, 2)", "pochhammer(X, 3)",
    "binomial(X, 2)", "alternating_factorial(X)", "hyperfactorial(X)", "subfactorial(X)", "cube_root(X)",
    "surd(X, 3)", "real_abs(X)", "real_sign(X)", "triangle_wave(X)", "square_wave(X)", "sawtooth_wave(X)",
    "unit_box(X)", "unit_triangle(X)", "heaviside_theta(X)", "anger_j(1, X)", "weber_e(1, X)", "coulomb_f(0, 1, X)",
    "whittaker_m(1, 1/2, X)", "whittaker_w(1, 1/2, X)", "riemann_siegel_z(X)", "chebyshev_t(3, X)",
    "legendre_p(4, X)", "hermite_h(3, X)", "laguerre_l(2, X)", "gegenbauer_c(2, 1/2, X)", "jacobi_p(2, 1, 1, X)",
    "legendre_p(1/2, X)", "chebyshev_t(1/2, X)", "x_power(X)", "neville_theta_d(X, 1/2)", "klein_invariant_j(im*X)",
    "modular_lambda(im*X)", "elliptic_theta(3, 0, X)", "dedekind_eta(im*X)", "q_pochhammer(X, 1/2)",
    "harmonic_number(X)", "prime_zeta_p(X)", "polygamma(1, X)", "polygamma(2, X)", "inverse_beta_regularized(X, 2, 3)",
];

/// Inner arguments of compositions (templates in `X`).
const INNER: &[&str] = &[
    "X", "X", "X", "-X", "2*X + 1", "X^2", "X^3", "X/3 - 1", "X^2 + 1", "X^3 - X", "exp(X)", "1/(1 + X^2)", "sqrt(X^2 + 1)",
    "X - 1/2", "3 - X",
];

/// Random function of the property generator: a template in `X`.
fn prop_function(r: &mut Rng) -> String {
    let f = r.pick(PROP_FUNS).to_string();
    let g = *r.pick(INNER);
    let base = if g == "X" { f.clone() } else { f.replace('X', &format!("({g})")) };
    match r.below(10) {
        0 => {
            let f2 = r.pick(PROP_FUNS).to_string();
            let c = *r.pick(&["1", "-1", "2", "1/2", "-3"]);
            format!("{base} + {c}*{f2}")
        }
        1 => format!("{base} + {}", r.pick(&["X", "-X", "X^2", "X^3", "1/2*X", "-X^2", "3"])),
        2 => format!("{} * {base}", r.pick(&["2", "-1", "X", "X^2", "(X^2 + 1)", "exp(X)", "-1/3"])),
        3 => format!("{}({base})", r.pick(&["exp", "atan", "sin", "log", "sqrt", "tanh", "erf", "abs"])),
        _ => base,
    }
}

/// A random restriction of the domain: the condition (template in `X`, "" for none) and the sampled range with
/// whether its ends belong to it.
fn restriction(r: &mut Rng) -> (String, f64, f64, bool) {
    let ends = [-5.0, -3.0, -2.0, -1.0, -0.5, 0.0, 0.25, 0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 10.0];
    match r.below(5) {
        0 | 1 => (String::new(), -8.0, 8.0, false),
        2 => {
            let a = *r.pick(&ends);
            (format!("X > {}", rat(a)), a, a + 12.0, false)
        }
        3 => {
            let a = *r.pick(&ends);
            (format!("X >= {}", rat(a)), a, a + 12.0, true)
        }
        _ => {
            let (mut a, mut b) = (*r.pick(&ends), *r.pick(&ends));
            if a > b {
                std::mem::swap(&mut a, &mut b);
            }
            if a == b {
                b = a + 1.0;
            }
            (format!("{} <= X and X <= {}", rat(a), rat(b)), a, b, true)
        }
    }
}

/// The sorted exact sample points of [lo, hi]: equally spaced, random ones, points near the ends (the ends themselves
/// when `closed`), and far points on an unbounded side.
fn prop_points(r: &mut Rng, lo: f64, hi: f64, closed: bool, unbounded_lo: bool, unbounded_hi: bool) -> Vec<String> {
    let mut v: Vec<(f64, String)> = vec![];
    let n = 36;
    for k in 0..n {
        let x = snap(lo + (hi - lo) * (k as f64 + 0.5) / n as f64);
        v.push((x, rat(x)));
    }
    for _ in 0..8 {
        let x = snap(lo + (hi - lo) * (r.next() % 10_000) as f64 / 10_000.0);
        v.push((x, rat(x)));
    }
    // near the ends: lo + 2^-k, exact
    for k in [10, 20, 40] {
        let d = format!("2^-{k}");
        v.push((lo + 2f64.powi(-k), format!("({} + {d})", rat(lo))));
        v.push((hi - 2f64.powi(-k), format!("({} - {d})", rat(hi))));
    }
    if closed {
        v.push((lo, rat(lo)));
        v.push((hi, rat(hi)));
    }
    if unbounded_lo {
        for x in [-20.0, -60.0, -200.0] {
            if x < lo {
                v.push((x, rat(x)));
            }
        }
    }
    if unbounded_hi {
        for x in [20.0, 60.0, 200.0] {
            if x > hi {
                v.push((x, rat(x)));
            }
        }
    }
    // near the integers and half-integers inside (poles of gamma, tan, ...)
    for k in (lo.ceil() as i64)..=(hi.floor() as i64) {
        for (s, d) in [(-1.0, "-"), (1.0, "+")] {
            let x = k as f64 + s * 2f64.powi(-30);
            if x > lo && x < hi {
                v.push((x, format!("({k} {d} 2^-30)")));
            }
        }
    }
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    v.dedup_by(|a, b| a.0 == b.0);
    v.into_iter().map(|p| p.1).collect()
}

/// What a property answer claims.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Claim {
    Sign(i8),
    Mono(i8),
    Convex(i8),
    Injective(bool),
}

/// The contradiction of the claim by the values (sorted by their points), if any.
fn contradiction(claim: Claim, xs: &[f64], vs: &[Option<f64>]) -> Option<String> {
    let tol = |v: f64| 1e-18 * (1.0 + v.abs());
    let pts: Vec<(f64, f64)> = xs.iter().zip(vs).filter_map(|(x, v)| v.map(|v| (*x, v))).collect();
    match claim {
        Claim::Sign(s) => {
            for (x, v) in &pts {
                let bad = match s {
                    1 => *v < -tol(0.0),
                    -1 => *v > tol(0.0),
                    _ => v.abs() > tol(0.0),
                };
                if bad {
                    return Some(format!("value {v} at {x}"));
                }
            }
        }
        Claim::Mono(s) => {
            for w in pts.windows(2) {
                let ((x0, v0), (x1, v1)) = (w[0], w[1]);
                let bad = match s {
                    1 => v1 < v0 - tol(v0),
                    -1 => v1 > v0 + tol(v0),
                    _ => (v1 - v0).abs() > tol(v0),
                };
                if bad {
                    return Some(format!("values {v0} at {x0}, {v1} at {x1}"));
                }
            }
        }
        Claim::Convex(s) => {
            for w in pts.windows(3) {
                let ((x0, v0), (x1, v1), (x2, v2)) = (w[0], w[1], w[2]);
                let chord = ((x2 - x1) * v0 + (x1 - x0) * v2) / (x2 - x0);
                let t = 1e-12 * (1.0 + v0.abs() + v1.abs() + v2.abs());
                let bad = match s {
                    1 => v1 > chord + t,
                    -1 => v1 < chord - t,
                    _ => (v1 - chord).abs() > t,
                };
                if bad {
                    return Some(format!("values {v0} at {x0}, {v1} at {x1}, {v2} at {x2}"));
                }
            }
        }
        Claim::Injective(true) => {
            let mut s = pts.clone();
            s.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            for w in s.windows(2) {
                if (w[1].1 - w[0].1).abs() <= 1e-20 * w[0].1.abs().max(w[1].1.abs()) && (w[1].0 - w[0].0).abs() > 1e-6 {
                    return Some(format!("equal values {} at {} and {}", w[0].1, w[0].0, w[1].0));
                }
            }
        }
        Claim::Injective(false) => {}
    }
    None
}

/// The indices of two points whose values agree to 30 digits (the injectivity check of [`contradiction`]).
fn equal_pair(xs: &[f64], vs: &[Option<f64>]) -> Option<(usize, usize)> {
    let mut s: Vec<(usize, f64, f64)> = xs.iter().zip(vs).enumerate().filter_map(|(i, (x, v))| v.map(|v| (i, *x, v))).collect();
    s.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
    s.windows(2)
        .find(|w| (w[1].2 - w[0].2).abs() <= 1e-20 * w[0].2.abs().max(w[1].2.abs()) && (w[1].1 - w[0].1).abs() > 1e-6)
        .map(|w| (w[0].0, w[1].0))
}

/// The exact point strings back to their machine values (for ordering and the chords).
fn point_value(e: &mut CoreEngine, pts: &[String]) -> Vec<f64> {
    let out = ev(e, &format!("n([{}], 30)", pts.join(", ")));
    elements(&out).iter().map(|s| real_of(s).unwrap_or(f64::NAN)).collect()
}

/// One property input: the query and the claim, checked; `Some(fault)` for a contradiction.
/// Sequences (templates in `X`) for the properties over the integers.
const SEQUENCES: &[&str] = &[
    "harmonic_number(X)", "X^2 - 7*X", "fibonacci(X)", "(-1)^X", "1/(X + 1/2)", "binomial(2*X, X)/4^X", "factorial(X)",
    "2^X - X^3", "sin(X)", "X*(-1)^X", "floor(X/3)", "mod(X, 4)", "prime_pi(X)", "euler_phi(X)", "log(X^2 + 1)",
    "atan(X)", "X/(X^2 + 1)", "catalan_number(X)", "bernoulli_b(X)", "(X + 1)/(X + 2)", "exp(-X^2)", "zeta(X + 2)",
    "gamma(X + 1/2)", "bessel_j(0, X)", "X^3 - 30*X", "abs(X - 3)", "pochhammer(1/2, X)", "subfactorial(X)",
];

/// A property of a sequence: `function_sign(f, x, integers)`, the monotonicity (differences), the convexity (second
/// differences), checked at the integers of the restriction.
fn sequence_input(e: &mut CoreEngine, r: &mut Rng, log: &mut Vec<String>) -> Option<String> {
    let f = r.pick(SEQUENCES).to_string();
    let f = match r.below(4) {
        0 => format!("{f} + {}", r.pick(&["X", "-X", "1", "X^2", "-2"])),
        1 => format!("{} * ({f})", r.pick(&["-1", "2", "X"])),
        _ => f,
    };
    let (cond, lo, hi) = match r.below(4) {
        0 => (String::new(), -40, 40),
        1 => ("X >= 0".to_string(), 0, 80),
        2 => ("X >= 1".to_string(), 1, 81),
        _ => {
            let a = r.int(-10, 10);
            let b = a + r.int(1, 30);
            (format!("{a} <= X and X <= {b}"), a, b)
        }
    };
    let fx = f.replace('X', "x");
    let arg = if cond.is_empty() { fx.clone() } else { format!("[{fx}, {}]", cond.replace('X', "x")) };
    let (q, kind) = match r.below(3) {
        0 => (format!("function_sign({arg}, x, integers)"), 0),
        1 => (format!("function_monotonicity({arg}, x, integers)"), 1),
        _ => (format!("function_convexity({arg}, x, integers)"), 2),
    };
    let ans = ev(e, &format!("time_constrained({q}, 30)"));
    let claim = match (kind, ans.as_str()) {
        (0, "1") => Claim::Sign(1),
        (0, "-1") => Claim::Sign(-1),
        (0, "0") => Claim::Sign(0),
        (1, "1") => Claim::Mono(1),
        (1, "-1") => Claim::Mono(-1),
        (1, "0") => Claim::Mono(0),
        (2, "1") => Claim::Convex(1),
        (2, "-1") => Claim::Convex(-1),
        (2, "0") => Claim::Convex(0),
        _ => {
            log.push(format!("{q} = {ans}"));
            return None;
        }
    };
    let ks: Vec<i64> = (lo..=hi).collect();
    let pts: Vec<String> = ks.iter().map(|k| format!("({k})")).collect();
    let xs: Vec<f64> = ks.iter().map(|&k| k as f64).collect();
    let vs = values(e, &f, &pts, 30);
    log.push(format!("{q} = {ans} ({} real values)", vs.iter().flatten().count()));
    contradiction(claim, &xs, &vs).map(|c| format!("{q} = {ans}: {c}"))
}

/// `function_range(arg, x, y)`: every sampled value satisfies the claimed condition on y (a violation decided at
/// 30 digits; values on the boundary that the comparison does not decide are not counted).
fn range_input(
    e: &mut CoreEngine,
    r: &mut Rng,
    log: &mut Vec<String>,
    f: &str,
    arg: &str,
    (lo, hi, closed, unb_lo, unb_hi): (f64, f64, bool, bool, bool),
) -> Option<String> {
    let q = format!("function_range({arg}, x, y)");
    let ans = ev(e, &format!("time_constrained({q}, 30)"));
    log.push(format!("{q} = {ans}"));
    if ans.starts_with("function_range(") || ans.starts_with("failure") || ans.starts_with("$aborted") || ans.starts_with("error") {
        return None;
    }
    let pts = prop_points(r, lo, hi, closed, unb_lo, unb_hi);
    let reals = real_samples(e, f, &pts, 30);
    if reals.is_empty() {
        return None;
    }
    // a violation that a relative change of 10^-25 of the value does not remove (the samples are rounded to their
    // printed digits: a value at a closed end of the range may come back just outside it)
    let bad = ev(
        e,
        &format!(
            "time_constrained(select([{}], v => not(({ans}) /. y -> v + 10^-25*(1 + abs(v))) and not(({ans}) /. y -> v - 10^-25*(1 + abs(v)))), 60)",
            reals
        ),
    );
    (bad != "[]" && bad.starts_with('[')).then(|| format!("{q} = {ans}: values outside {bad}"))
}

fn property_input(e: &mut CoreEngine, r: &mut Rng, log: &mut Vec<String>) -> Option<String> {
    if r.below(6) == 0 {
        return sequence_input(e, r, log);
    }
    let f = prop_function(r);
    let (cond, lo, hi, closed) = restriction(r);
    let fx = f.replace('X', "x");
    let arg = if cond.is_empty() { fx.clone() } else { format!("[{fx}, {}]", cond.replace('X', "x")) };
    if r.below(6) == 0 {
        return range_input(e, r, log, &f, &arg, (lo, hi, closed, cond.is_empty(), cond.is_empty() || !cond.contains("<=")));
    }
    let (q, kind) = match r.below(5) {
        0 => (format!("function_sign({arg}, x)"), 0),
        1 => (format!("function_monotonicity({arg}, x)"), 1),
        2 => (format!("function_monotonicity({arg}, x, strict_inequalities: true)"), 1),
        3 => (format!("function_convexity({arg}, x)"), 2),
        _ => (format!("function_injective({arg}, x)"), 3),
    };
    let ans = ev(e, &format!("time_constrained({q}, 30)"));
    let claim = match (kind, ans.as_str()) {
        (0, "1") => Claim::Sign(1),
        (0, "-1") => Claim::Sign(-1),
        (0, "0") => Claim::Sign(0),
        (1, "1") => Claim::Mono(1),
        (1, "-1") => Claim::Mono(-1),
        (1, "0") => Claim::Mono(0),
        (2, "1") => Claim::Convex(1),
        (2, "-1") => Claim::Convex(-1),
        (2, "0") => Claim::Convex(0),
        (3, "true") => Claim::Injective(true),
        (3, "false") => Claim::Injective(false),
        _ => {
            log.push(format!("{q} = {ans}"));
            return None;
        }
    };
    let unb_lo = cond.is_empty();
    let unb_hi = cond.is_empty() || !cond.contains("<=");
    let pts = prop_points(r, lo, hi, closed, unb_lo, unb_hi);
    let xs = point_value(e, &pts);
    let vs = values(e, &f, &pts, 30);
    log.push(format!("{q} = {ans} ({} real values)", vs.iter().flatten().count()));
    let c = contradiction(claim, &xs, &vs)?;
    if claim == Claim::Injective(true) {
        // values equal to 30 digits (tanh far out is 1 to 30 digits): a contradiction only when their difference,
        // evaluated to 150 significant digits, is exactly 0
        let (i, j) = equal_pair(&xs, &vs)?;
        let d = ev(e, &format!("n(abs({} - ({})), 150)", f.replace('X', &pts[i]), f.replace('X', &pts[j])));
        if real_of(&d) != Some(0.0) {
            return None;
        }
    }
    Some(format!("{q} = {ans}: {c}"))
}

/// One input of a generator: `Some(fault)` for a contradiction; every query and answer goes to the log.
type Input = fn(&mut CoreEngine, &mut Rng, &mut Vec<String>) -> Option<String>;

/// Runs a generator: `run.inputs` inputs (or until the time cap) in one session, with progress; the log goes to
/// `MC_SOUNDNESS_LOG/<job>.log` when that is set.
fn drive(run: &Run, salt: u64, input: Input) -> (usize, Vec<String>) {
    let mut e = CoreEngine::new();
    let mut r = Rng(run.seed ^ salt);
    let (start, mut faults, mut log) = (Instant::now(), vec![], vec![]);
    // the log is written as it grows: every source before it is evaluated (`> `), every answer after (an input
    // that ends the process is the last line)
    if run.verbose {
        if let Ok(dir) = std::env::var("MC_SOUNDNESS_LOG") {
            let file = std::fs::File::create(format!("{dir}/{}.log", run.job)).ok();
            TRACE.with(|t| *t.borrow_mut() = file);
        }
    }
    let mut done = 0;
    while done < run.inputs && start.elapsed() < run.cap {
        if let Some(f) = input(&mut e, &mut r, &mut log) {
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

// ---------------------------------------------------------------------------------------------------------------
// 2. enclosures

/// Functions with a ball form (templates in `X`) and the real range their intervals are drawn from.
const ENCL_FUNS: &[(&str, f64, f64)] = &[
    ("sin(X)", -8.0, 8.0), ("cos(X)", -8.0, 8.0), ("tan(X)", -4.0, 4.0), ("cot(X)", -4.0, 4.0), ("sec(X)", -4.0, 4.0),
    ("csc(X)", -4.0, 4.0), ("exp(X)", -10.0, 10.0), ("log(X)", 0.0, 10.0), ("sqrt(X)", 0.0, 10.0),
    ("sinh(X)", -5.0, 5.0), ("cosh(X)", -5.0, 5.0), ("tanh(X)", -5.0, 5.0), ("coth(X)", -3.0, 3.0),
    ("sech(X)", -4.0, 4.0), ("csch(X)", -3.0, 3.0), ("asin(X)", -1.0, 1.0), ("acos(X)", -1.0, 1.0),
    ("atan(X)", -5.0, 5.0), ("acot(X)", -5.0, 5.0), ("asinh(X)", -5.0, 5.0), ("acosh(X)", 1.0, 6.0),
    ("atanh(X)", -1.0, 1.0), ("acoth(X)", 1.0, 5.0), ("asec(X)", 1.0, 5.0), ("acsc(X)", 1.0, 5.0),
    ("gamma(X)", -4.0, 6.0), ("log_gamma(X)", 0.0, 8.0), ("digamma(X)", -3.0, 5.0), ("polygamma(1, X)", -2.0, 5.0),
    ("factorial(X)", -3.0, 5.0), ("beta(2, X)", -2.0, 4.0), ("bessel_j(0, X)", -10.0, 10.0),
    ("bessel_j(1, X)", -10.0, 10.0), ("bessel_j(5/2, X)", 0.0, 10.0), ("bessel_y(0, X)", 0.0, 10.0),
    ("bessel_y(1, X)", 0.0, 10.0), ("bessel_i(0, X)", -4.0, 4.0), ("bessel_i(1, X)", -4.0, 4.0),
    ("bessel_k(0, X)", 0.0, 5.0), ("bessel_k(1, X)", 0.0, 5.0), ("airy_ai(X)", -8.0, 3.0), ("airy_bi(X)", -8.0, 2.0),
    ("airy_ai_prime(X)", -6.0, 3.0), ("airy_bi_prime(X)", -6.0, 2.0), ("erf(X)", -3.0, 3.0), ("erfc(X)", -3.0, 3.0),
    ("erfi(X)", -2.0, 2.0), ("dawson_f(X)", -4.0, 4.0), ("fresnel_s(X)", -3.0, 3.0), ("fresnel_c(X)", -3.0, 3.0),
    ("exp_integral_ei(X)", -3.0, 3.0), ("exp_integral_e(1, X)", 0.0, 4.0), ("log_integral(X)", 0.0, 4.0),
    ("sin_integral(X)", -10.0, 10.0), ("cos_integral(X)", 0.0, 10.0), ("sinh_integral(X)", -3.0, 3.0),
    ("cosh_integral(X)", 0.0, 3.0), ("zeta(X)", -6.0, 6.0), ("product_log(X)", -0.36, 4.0),
    ("elliptic_k(X)", -2.0, 1.0), ("elliptic_e(X)", -2.0, 1.0), ("hermite_h(3, X)", -3.0, 3.0),
    ("legendre_p(3, X)", -1.0, 1.0), ("chebyshev_t(4, X)", -1.2, 1.2), ("sinc(X)", -10.0, 10.0),
    ("inverse_erf(X)", -1.0, 1.0), ("logistic_sigmoid(X)", -6.0, 6.0), ("gudermannian(X)", -4.0, 4.0),
    ("abs(X)", -3.0, 3.0), ("floor(X)", -3.0, 3.0), ("round(X)", -3.0, 3.0), ("x_power(X)", 0.0, 3.0),
    ("polylog(2, X)", -3.0, 1.0), ("struve_h(0, X)", 0.0, 10.0), ("hypergeometric_1f1(1/2, 3/2, X)", -4.0, 4.0),
    ("hypergeometric_0f1(3/2, X)", -10.0, 4.0), ("gamma_regularized(2, X)", 0.0, 6.0), ("beta_regularized(X, 2, 3)", 0.0, 1.0),
    ("jacobi_sn(X, 1/3)", -4.0, 4.0), ("jacobi_cn(X, 1/2)", -4.0, 4.0), ("elliptic_f(X, 1/2)", -3.0, 3.0),
    ("harmonic_number(X)", -0.9, 6.0), ("barnes_g(X)", -2.0, 4.0), ("kelvin_ber(X)", 0.0, 8.0),
    ("spherical_bessel_j(1, X)", -8.0, 8.0), ("X^3 - 2*X", -2.0, 2.0), ("X^(1/3)", 0.0, 3.0), ("2^X", -3.0, 3.0),
    ("X^X", 0.0, 2.0), ("1/(X^2 - 1)", -2.0, 2.0), ("sin(X)/X", -4.0, 4.0), ("exp(-X^2)*cos(3*X)", -2.0, 2.0),
    ("sin(X)^2 + cos(X)", -4.0, 4.0), ("log(X)*sin(X)", 0.0, 10.0), ("bessel_j(X, 2)", -3.0, 4.0),
    ("gamma(X, 1)", -2.0, 4.0), ("legendre_p(X, 1/2)", -2.0, 3.0), ("fractional_part(X)", -3.0, 3.0),
    ("mod(X, 1)", -3.0, 3.0), ("mod(X, -2)", -5.0, 5.0), ("mod(5, X)", 1.0, 6.0), ("quotient(X, 2)", -5.0, 5.0),
    ("cantor_staircase(X)", -0.5, 1.5), ("sawtooth_wave(X)", -2.0, 2.0), ("triangle_wave(X)", -2.0, 2.0),
    ("arg(X)", -2.0, 2.0), ("inverse_jacobi_sn(X, 1/2)", -1.0, 1.0), ("inverse_jacobi_cs(X, 1/2)", -3.0, 3.0),
    ("spherical_bessel_y(1, X)", 0.0, 8.0), ("legendre_q(1, X)", -1.0, 1.0), ("bessel_j(X, 2)", -6.0, 1.0),
    ("polylog(3, X)", -5.0, 1.0), ("log_barnes_g(X)", 0.0, 5.0), ("hypergeometric_2f1(1, 1, 2, X)", -3.0, 1.0),
    ("elliptic_pi(X, 1/2)", -2.0, 1.0), ("jacobi_amplitude(X, 1/2)", -4.0, 4.0), ("acoth(X)", -3.0, 3.0),
    ("asec(X)", -3.0, 3.0), ("acsc(X)", -3.0, 3.0), ("asech(X)", 0.0, 1.0), ("acsch(X)", -3.0, 3.0),
    ("inverse_gudermannian(X)", -1.5, 1.5), ("sinc(X)^2 - X", -4.0, 4.0), ("clip(X)", -3.0, 3.0),
    ("unit_triangle(X)", -2.0, 2.0), ("ramp(X)", -2.0, 2.0), ("heaviside_lambda(X)", -2.0, 2.0),
    ("beta(X, X)", 0.0, 4.0), ("bessel_j(X, X)", 0.0, 6.0), ("hypergeometric_2f1(1, X, 2, X/10)", -5.0, 5.0),
    ("polylog(X, 1/2)", -3.0, 4.0), ("zeta(X, 2)", 1.0, 6.0), ("gamma(X, X)", 0.0, 4.0), ("X^sin(X)", 0.0, 5.0),
    ("bessel_k(X, 1)", -3.0, 3.0), ("hypergeometric_1f1(X, 3/2, 2)", -4.0, 4.0), ("legendre_p(X, 1, 1/2)", -3.0, 3.0),
];

/// Functions of a complex box (centered intervals with a complex radius).
const BOX_FUNS: &[&str] = &[
    "sin(X)", "cos(X)", "tan(X)", "cot(X)", "sec(X)", "csc(X)", "sinh(X)", "cosh(X)", "tanh(X)", "coth(X)", "sech(X)",
    "csch(X)", "exp(X)", "log(X)", "sqrt(X)", "asin(X)", "acos(X)", "atan(X)", "acot(X)", "asinh(X)", "acosh(X)",
    "atanh(X)", "acoth(X)", "gamma(X)", "log_gamma(X)", "digamma(X)", "erf(X)", "erfc(X)", "zeta(X)", "product_log(X)",
    "bessel_j(1/3, X)", "bessel_k(1, X)", "airy_ai(X)", "polylog(2, X)", "elliptic_k(X)", "hypergeometric_1f1(1/2, 3/2, X)",
    "exp_integral_ei(X)", "fresnel_s(X)", "X^(1/3)", "X^X", "sin(X)/X", "exp(X)*cos(2*X)", "polygamma(1, X)",
    "hypergeometric_2f1(1/2, 1, 3/2, X)", "dawson_f(X)", "erfi(X)", "sinc(X)", "log(1 + X^2)",
];

/// A random interval of [lo, hi] (its ends as exact rationals, or machine numbers), of a random width.
fn random_interval(r: &mut Rng, lo: f64, hi: f64) -> (String, String, f64, f64) {
    let w: f64 = *r.pick(&[1e-9, 1e-4, 0.01, 0.3, 1.0, 3.0, 10.0]);
    let span = (hi - lo).max(1e-9);
    let w = w.min(span);
    let a = lo + (span - w) * (r.next() % 10_000) as f64 / 10_000.0;
    let b = a + w;
    let edge = r.below(6);
    let (a, b) = match edge {
        0 => (lo, lo + w),
        1 => (hi - w, hi),
        _ => (a, b),
    };
    if r.below(2) == 0 {
        let (sa, sb) = (format!("{a:?}"), format!("{b:?}"));
        (sa, sb, a, b)
    } else {
        let q = |v: f64| {
            let d = 1u64 << 40;
            let k = (v * d as f64).round() as i64;
            format!("({k}/{d})")
        };
        (q(a), q(b), a, b)
    }
}

/// The exact sample points of [a, b] (given by their printed ends): ends, equally spaced, random.
fn interval_points(r: &mut Rng, sa: &str, sb: &str) -> Vec<String> {
    let mut pts = vec![format!("set_precision({sa}, inf)"), format!("set_precision({sb}, inf)")];
    for k in 1..24 {
        pts.push(format!("(set_precision({sa}, inf) + (set_precision({sb}, inf) - set_precision({sa}, inf))*{k}/24)"));
    }
    for _ in 0..8 {
        let t = r.next() % 1_000_000;
        pts.push(format!("(set_precision({sa}, inf) + (set_precision({sb}, inf) - set_precision({sa}, inf))*{t}/1000000)"));
    }
    pts
}

/// The values of `f` at the points at `digits` digits, kept in the session as `sv` (a literal decimal would be read
/// back as a machine number): the parts `sv[i]` that are real numbers, joined by commas (`""` for none).
fn real_samples(e: &mut CoreEngine, f: &str, pts: &[String], digits: u32) -> String {
    let vals: Vec<String> = pts.iter().map(|p| format!("n({}, {digits})", f.replace('X', p))).collect();
    let out = ev(e, &format!("time_constrained(sv = [{}], 60)", vals.join(", ")));
    let els = elements(&out);
    if els.len() != pts.len() {
        return String::new();
    }
    let idx: Vec<String> = els.iter().enumerate().filter(|(_, v)| real_of(v).is_some()).map(|(i, _)| format!("sv[{}]", i + 1)).collect();
    idx.join(", ")
}

/// The enclosure check: which sample values (at 40 digits) lie outside the result assigned to `ri` (an `interval`
/// of one or more pieces, its ends at 60 digits, with a margin of 10^-30 relative for the rounding of the samples).
/// Samples that are not real numbers (a pole, outside the domain) are left out.
fn outside_real(e: &mut CoreEngine, f: &str, pts: &[String]) -> String {
    let reals = real_samples(e, f, pts, 40);
    if reals.is_empty() {
        return "[]".into();
    }
    ev(
        e,
        &format!(
            "pcs = n(apply(list, ri), 60); select([{reals}], v => select(pcs, p => p[1] - 10^-30*(1 + abs(p[1])) <= v and v <= p[2] + 10^-30*(1 + abs(p[2]))) == [])"
        ),
    )
}

fn enclosure_input(e: &mut CoreEngine, r: &mut Rng, log: &mut Vec<String>) -> Option<String> {
    let kind = r.below(4);
    if kind == 3 {
        // a complex box
        let f = *r.pick(BOX_FUNS);
        let fz = f.replace('X', "z");
        let (x, y) = (r.int(-30, 30) as f64 / 10.0, r.int(-30, 30) as f64 / 10.0);
        let (rx, ry) = (*r.pick(&["1/1000", "1/100", "1/10", "1/2", "10^-6"]), *r.pick(&["1/1000", "1/100", "1/10", "1/2", "10^-6"]));
        let c = format!("({x:?} + ({y:?})*im)");
        let input = f.replace('X', &format!("centered_interval({c}, {rx} + {ry}*im)"));
        let res = ev(e, &format!("time_constrained({{ rc = {input}; rc }}, 30)"));
        log.push(format!("{input} = {res}"));
        if !single_call(&res, "centered_interval") {
            return None;
        }
        // a sample where the function has no value (a pole) is left out
        let bad = ev(
            e,
            &format!(
                "time_constrained({{ c0 = set_precision({x:?}, inf) + set_precision({y:?}, inf)*im; \
                 pts = flatten(table(c0 + (2 i/6 - 1) {rx} + (2 j/6 - 1) {ry} im, [i, 0, 6], [j, 0, 6])); \
                 select(pts, z => {{ let v = n({fz}, 40); \
                    if is_failure(v) === true then false else \
                    is_number(v) === true and not(abs(re(v) - re(rc[1])) <= re(rc[2]) and abs(im_part(v) - im_part(rc[1])) <= im_part(rc[2])) }}) }}, 60)"
            ),
        );
        return (bad != "[]" && bad.starts_with('[')).then(|| format!("{input} = {res}: outside at {bad}"));
    }
    let (f, lo, hi) = *r.pick(ENCL_FUNS);
    let (sa, sb, _, _) = random_interval(r, lo, hi);
    let (input, res) = if kind < 2 {
        let input = f.replace('X', &format!("interval([{sa}, {sb}])"));
        let res = ev(e, &format!("time_constrained({{ ri = {input}; ri }}, 30)"));
        (input, res)
    } else {
        let input = f.replace('X', &format!("centered_interval(({sa} + {sb})/2, ({sb} - ({sa}))/2)"));
        let res = ev(e, &format!("time_constrained({{ rc = {input}; rc }}, 30)"));
        if single_call(&res, "centered_interval") {
            ev(e, "ri = interval([rc[1] - rc[2], rc[1] + rc[2]])");
        }
        (input, res)
    };
    log.push(format!("{input} = {res}"));
    if !(single_call(&res, "interval") || single_call(&res, "centered_interval")) {
        return None;
    }
    let pts = interval_points(r, &sa, &sb);
    let bad = outside_real(e, f, &pts);
    (bad != "[]" && !bad.starts_with("$aborted")).then(|| format!("{input} = {res}: outside {bad}"))
}

// ---------------------------------------------------------------------------------------------------------------
// 3. calculus against numbers

/// Analytic functions for the calculus inputs (templates in `X`).
const ANALYTIC: &[&str] = &[
    "exp(X)", "sin(X)", "cos(X)", "sinh(X)", "atan(X)", "log(1 + X^2)", "erf(X)", "bessel_j(0, X)", "bessel_i(1, X)",
    "airy_ai(X)", "sin_integral(X)", "fresnel_c(X)", "dawson_f(X)", "exp(-X^2)", "1/(2 + X)", "sqrt(4 + X)",
    "cos(X)^2", "X*exp(X)", "tan(X)", "log(2 + X)", "asinh(X)", "erfc(X)", "hypergeometric_1f1(1/2, 3/2, X)",
    "bessel_j(1, X)", "struve_h(0, X)", "elliptic_k(X/4)", "product_log(X)", "gamma(2 + X)",
];

/// Exact points for limits and series.
const POINTS: &[&str] = &["0", "0", "1", "-1", "1/2", "2", "pi", "pi/2", "-3"];

/// `|E(a + h) - L|` (the template `E` in `X`, `L` a closed term) at 50 digits for the steps `hs` (signed).
fn distances(e: &mut CoreEngine, f: &str, a: &str, l: &str, hs: &[&str]) -> Vec<Option<f64>> {
    let list: Vec<String> = hs.iter().map(|h| format!("n(abs({} - ({l})), 50)", f.replace('X', &format!("({a} + {h})")))).collect();
    let out = ev(e, &format!("time_constrained([{}], 30)", list.join(", ")));
    elements(&out).iter().map(|s| real_of(s)).collect()
}

/// A limit, against values near the point.
fn limit_input(e: &mut CoreEngine, r: &mut Rng, log: &mut Vec<String>) -> Option<String> {
    let a = *r.pick(POINTS);
    let f = match r.below(4) {
        0 => {
            let (p, q) = (*r.pick(ANALYTIC), *r.pick(ANALYTIC));
            let (pa, qa) = (p.replace('X', &format!("({a})")), q.replace('X', &format!("({a})")));
            format!("({p} - {pa})/({q} - {qa})")
        }
        1 => {
            let p = *r.pick(ANALYTIC);
            let k = r.int(1, 3);
            format!("({p} - {})/(X - ({a}))^{k}", p.replace('X', &format!("({a})")))
        }
        2 => format!("{}*log(abs(X - ({a})))", r.pick(&["(X - (A))", "(X - (A))^2", "sin(X - (A))", "(X - (A))^(1/2)"]).replace('A', a)),
        _ => prop_function(r),
    };
    let fx = f.replace('X', "x");
    let (dir, sides): (&str, &[&str]) = match r.below(3) {
        0 => ("", &["10^-6", "-10^-6", "10^-10", "-10^-10"]),
        1 => (", direction: from_above", &["10^-6", "10^-10"]),
        _ => (", direction: from_below", &["-10^-6", "-10^-10"]),
    };
    let q = format!("limit({fx}, x -> {a}{dir})");
    let ans = ev(e, &format!("time_constrained({q}, 30)"));
    log.push(format!("{q} = {ans}"));
    if ans.starts_with("limit(") || ans.starts_with("failure") || ans.starts_with("$aborted") || ans.starts_with("error") {
        return None;
    }
    // a finite value: closer at the smaller step, and close
    let fin = ev(e, &format!("is_number(n({ans})) === true"));
    if fin == "true" {
        let d = distances(e, &f, a, &ans, sides);
        // pairs (h, h') with the same sign: the second step is 10^-12 times the first
        let (near, far): (Vec<_>, Vec<_>) = d.iter().enumerate().partition(|(i, _)| sides[*i].ends_with("^-10"));
        for ((_, dn), (_, df)) in near.iter().zip(far.iter()) {
            if let (Some(dn), Some(df)) = (dn, df) {
                let scale = 1.0 + real_of(&ev(e, &format!("n(abs({ans}))"))).unwrap_or(1.0);
                if *dn > 1e-3 * scale && *dn > 0.5 * *df {
                    return Some(format!("{q} = {ans}: distance {df} at 10^-6, {dn} at 10^-10"));
                }
            }
        }
        return None;
    }
    // an infinite value: the real part (or modulus) grows
    let sign = match ans.as_str() {
        "inf" => 1.0,
        "-inf" => -1.0,
        "complex_infinity" => 0.0,
        _ => return None,
    };
    // the value at the smaller step of each side is larger in modulus, with the sign claimed
    let at = |e: &mut CoreEngine, h: &str| {
        let v = ev(e, &format!("time_constrained(n({}, 50), 30)", f.replace('X', &format!("({a} + {h})"))));
        let m = real_of(&ev(e, &format!("n(abs({v}))")));
        (v, m)
    };
    for pair in sides.chunks(1).zip(sides.iter().skip(sides.len() / 2)) {
        let (far, near) = (pair.0[0], *pair.1);
        if far.ends_with("^-10") {
            break;
        }
        let ((vf, mf), (vn, mn)) = (at(e, far), at(e, near));
        let (Some(mf), Some(mn)) = (mf, mn) else { continue };
        let wrong_sign = sign != 0.0 && real_of(&vn).is_some_and(|v| v * sign < 0.0);
        if wrong_sign || mn <= mf {
            return Some(format!("{q} = {ans}: value {vf} at {far}, {vn} at {near}"));
        }
    }
    None
}

/// A series: the remainder after the terms through order n is O(h^(n+1)) (to a factor, with logarithms and
/// fractional powers): |E - P| at h = 10^-9 is at most 100 * 10^(-3(n+1)) times its value at h = 10^-6.
fn series_input(e: &mut CoreEngine, r: &mut Rng, log: &mut Vec<String>) -> Option<String> {
    let a = *r.pick(POINTS);
    let f = match r.below(3) {
        0 => r.pick(ANALYTIC).to_string(),
        1 => format!("{}/{}", r.pick(ANALYTIC), r.pick(&["X", "X^2", "sin(X)", "(exp(X) - 1)", "X^3"])),
        _ => prop_function(r),
    };
    let n = r.int(1, 5);
    let fx = f.replace('X', "x");
    let q = format!("series({fx}, x, {a}, {n})");
    let ans = ev(e, &format!("time_constrained({{ s = normal({q}); s }}, 30)"));
    log.push(format!("{q} -> {ans}"));
    if ans.contains("series(") || ans.starts_with("failure") || ans.starts_with("$aborted") || ans.starts_with("error") {
        return None;
    }
    let side = *r.pick(&["", "-"]);
    let err = |e: &mut CoreEngine, h: &str| {
        let p = format!("({a} + {side}{h})");
        real_of(&ev(e, &format!("time_constrained(n(abs({} - (s /. x -> {p})), 90), 30)", f.replace('X', &p))))
    };
    let (e1, e2) = (err(e, "10^-6"), err(e, "10^-9"));
    if let (Some(e1), Some(e2)) = (e1, e2) {
        let bound = 100.0 * 1e-3f64.powi(n as i32 + 1) * e1 + 1e-80;
        if e2 > bound && e2 > 1e-80 {
            return Some(format!("{q} = {ans}: remainder {e1} at {side}10^-6, {e2} at {side}10^-9"));
        }
    }
    None
}

/// A definite integral against `n_integrate`.
fn integral_input(e: &mut CoreEngine, r: &mut Rng, log: &mut Vec<String>) -> Option<String> {
    let f = match r.below(2) {
        0 => r.pick(ANALYTIC).to_string(),
        _ => prop_function(r),
    };
    let (a, b) = *r.pick(&[("0", "1"), ("-1", "1"), ("0", "pi"), ("1", "2"), ("0", "inf"), ("1", "inf"), ("-inf", "inf"), ("1/2", "3")]);
    let f = if b == "inf" { format!("({f})*exp(-X^2)") } else { f };
    let fx = f.replace('X', "x");
    let q = format!("integrate({fx}, [x, {a}, {b}])");
    let ans = ev(e, &format!("time_constrained({{ v = {q}; v }}, 40)"));
    log.push(format!("{q} = {ans}"));
    if ans.contains("integrate(") || ans.starts_with("failure") || ans.starts_with("$aborted") || ans.starts_with("error") || ans.contains("conditional") {
        return None;
    }
    let exact = ev(e, "time_constrained(n(v, 30), 20)");
    let num = ev(e, &format!("time_constrained(n_integrate({fx}, [x, {a}, {b}], working_precision: 30), 40)"));
    let d = real_of(&ev(e, &format!("n(abs(({exact}) - ({num}))/(1 + abs({num})))")));
    match d {
        Some(d) if d > 1e-12 => Some(format!("{q} = {ans} = {exact}; numerically {num}")),
        _ => None,
    }
}

/// Summands of closed-form sums over k (and their first index).
const SUMMANDS: &[(&str, &str)] = &[
    ("1/((k + P)*(k + Q))", "1"), ("1/(k^2 + P)", "1"), ("(-1)^k/(k + P)", "0"), ("Z^k/(k + P)", "0"),
    ("k^P*Z^k", "0"), ("Z^k/factorial(k + P)", "0"), ("1/k^(P + 1)", "1"), ("(-1)^k/(2*k + P)", "0"),
    ("cos(P*k)/k^2", "1"), ("sin(P*k)/k", "1"), ("harmonic_number(k)*Z^k", "1"), ("binomial(2*k, k)*Z^k/4^k", "0"),
    ("1/(k*(k + P)*(k + Q))", "1"), ("k/(k^4 + P)", "1"), ("Z^(2*k)/factorial(2*k)", "0"), ("1/(k^2 - 1/P)", "1"),
    ("(k + P)/2^k", "0"), ("1/binomial(k + P, P)", "0"), ("log(1 + 1/k^2)", "1"), ("(-1)^k*k/(k^2 + P)", "1"),
];

/// A sum or a product in closed form against the partial sums (finite range: the terms added; infinite: `n_sum`).
fn sum_input(e: &mut CoreEngine, r: &mut Rng, log: &mut Vec<String>) -> Option<String> {
    let (t, k0) = *r.pick(SUMMANDS);
    let p = r.int(1, 4).to_string();
    let q = (r.int(1, 4) + 4).to_string();
    let z = *r.pick(&["1/2", "-1/3", "1/5", "2/3", "-1/2"]);
    let t = t.replace('P', &p).replace('Q', &q).replace('Z', &format!("({z})"));
    let product = r.below(5) == 0;
    let (t, head) = if product { (format!("1 + 1/(k^2 + {p}*k + {q})"), "product") } else { (t, "sum") };
    let finite = r.below(3) == 0;
    let hi = if finite { r.int(3, 12).to_string() } else { "inf".to_string() };
    let qs = format!("{head}({t}, [k, {k0}, {hi}])");
    let ans = ev(e, &format!("time_constrained({{ v = {qs}; v }}, 40)"));
    log.push(format!("{qs} = {ans}"));
    if ans.contains("sum(") || ans.contains("product(") || ans.starts_with("failure") || ans.starts_with("$aborted") || ans.starts_with("error") {
        return None;
    }
    let exact = ev(e, "time_constrained(n(v, 30), 20)");
    let num = if finite {
        let op = if product { "mul" } else { "add" };
        ev(e, &format!("n(apply({op}, table({t}, [k, {k0}, {hi}])), 30)"))
    } else {
        let nf = if product { "n_product" } else { "n_sum" };
        ev(e, &format!("time_constrained({nf}({t}, [k, {k0}, inf], working_precision: 30), 40)"))
    };
    let d = real_of(&ev(e, &format!("n(abs(({exact}) - ({num}))/(1 + abs({num})))")));
    match d {
        Some(d) if d > 1e-10 => Some(format!("{qs} = {ans} = {exact}; numerically {num}")),
        _ => None,
    }
}

/// A residue against the integral over a circle of radius 1/4 around the point (the other singularities outside).
fn residue_input(e: &mut CoreEngine, r: &mut Rng, log: &mut Vec<String>) -> Option<String> {
    let h = *r.pick(ANALYTIC);
    let (f, a) = match r.below(5) {
        0 => {
            let a = *r.pick(&["0", "1", "-1/2", "im", "2"]);
            (format!("{}/(X - ({a}))^{}", h, r.int(1, 4)), a.to_string())
        }
        1 => {
            let n = r.int(0, 4);
            (format!("gamma(X)*{}", h.replace('X', "(X/10)")), format!("-{n}"))
        }
        2 => (format!("{}/sin(X)^{}", h.replace('X', "(X/10)"), r.int(1, 3)), "0".to_string()),
        3 => (format!("{}*exp(1/X)", h.replace('X', "(X/10)")), "0".to_string()),
        _ => (format!("{}*tan(X)", h.replace('X', "(X/10)")), "pi/2".to_string()),
    };
    let fx = f.replace('X', "x");
    let q = format!("residue({fx}, [x, {a}])");
    let ans = ev(e, &format!("time_constrained({{ v = {q}; v }}, 30)"));
    log.push(format!("{q} = {ans}"));
    if ans.starts_with("residue(") || ans.starts_with("failure") || ans.starts_with("$aborted") || ans.starts_with("error") {
        return None;
    }
    let z = format!("(({a}) + exp(im*t)/4)");
    let num = ev(
        e,
        &format!("time_constrained(n_integrate({}*exp(im*t)/4, [t, 0, 2*pi], working_precision: 25)/(2*pi), 40)", f.replace('X', &z)),
    );
    let d = real_of(&ev(e, &format!("n(abs(n(v, 30) - ({num}))/(1 + abs({num})))")));
    match d {
        Some(d) if d > 1e-10 => Some(format!("{q} = {ans}; numerically {num}")),
        _ => None,
    }
}

/// Differential equations: each solution (with random values for the constants) put back into the equation at
/// points, at 30 digits.
fn dsolve_input(e: &mut CoreEngine, r: &mut Rng, log: &mut Vec<String>) -> Option<String> {
    let (p, q) = (r.int(-3, 3), r.int(-3, 3));
    let rhs = *r.pick(&["0", "x", "exp(2*x)", "sin(x)", "x^2", "exp(x)", "cos(3*x)", "x*exp(-x)", "1/x", "log(x)"]);
    let eq = match r.below(8) {
        0 | 1 => format!("y''(x) + ({p})*y'(x) + ({q})*y(x) == {rhs}"),
        2 => format!("y'(x) + {}*y(x) == {rhs}", r.pick(&["1", "x", "1/x", "tan(x)", "2*x", "exp(x)"])),
        3 => format!("x^2*y''(x) + ({p})*x*y'(x) + ({q})*y(x) == {rhs}"),
        4 => format!("y'(x) == {}", r.pick(&["y(x)^2 + 1", "x*y(x)^2", "y(x)*(1 - y(x))", "(x + y(x))/x", "y(x)/x + x^2", "exp(y(x))*x", "y(x)^3"])),
        5 => format!("y'''(x) + ({p})*y'(x) + ({q})*y(x) == {rhs}"),
        6 => format!("y''(x) + {}*y(x) == 0", r.pick(&["x", "-x", "x^2", "1/x^2", "exp(2*x)", "1/(4*x^2)"])),
        _ => format!("x*y''(x) + ({p} + x)*y'(x) + ({q})*y(x) == 0"),
    };
    let qs = format!("d_solve({eq}, y, x)");
    let ans = ev(e, &format!("time_constrained({{ sol = {qs}; sol }}, 40)"));
    log.push(format!("{qs} = {ans}"));
    if !ans.starts_with("[[y -> ") {
        return None;
    }
    let (lhs, rhs) = eq.split_once(" == ").unwrap();
    let cs = "[c(1) -> 2/3, c(2) -> -5/7, c(3) -> 3/11]";
    let res = ev(
        e,
        &format!(
            "time_constrained(map(s => map(x0 => n(abs((({lhs}) - ({rhs})) /. s /. {cs} /. x -> x0), 30), [3/10, 1, 17/10]), sol), 40)"
        ),
    );
    for row in elements(&res) {
        for v in elements(&row) {
            if let Some(v) = real_of(&v) {
                if v > 1e-15 {
                    return Some(format!("{qs} = {ans}: residuals {res}"));
                }
            } else if !v.contains("indeterminate") && !v.contains("complex_infinity") {
                log.push(format!("  residual not a number: {v}"));
            }
        }
    }
    None
}

/// Integrals and sums with a parameter `p` whose answers are conditional (templates in `p`, with the variable `x` or
/// the index `k`).
const CONDITIONAL: &[&str] = &[
    "integrate(x^P*exp(-x), [x, 0, inf])", "integrate(x^(P - 1)/(1 + x), [x, 0, inf])", "integrate(exp(-P*x^2), [x, -inf, inf])",
    "integrate(sin(P*x)/x, [x, 0, inf])", "integrate(x^P*log(x), [x, 0, 1])", "integrate(1/(x^2 + P^2), [x, 0, inf])",
    "integrate(exp(-P*x)*cos(x), [x, 0, inf])", "integrate(x^P, [x, 1, inf])", "integrate(bessel_j(0, x)*exp(-P*x), [x, 0, inf])",
    "integrate(x^(P - 1)*exp(-x^2), [x, 0, inf])", "integrate((1 - x)^P*x, [x, 0, 1])", "integrate(exp(-x)/(x + P), [x, 0, inf])",
    "sum(P^k/k, [k, 1, inf])", "sum(1/(k^P), [k, 1, inf])", "sum(k*P^k, [k, 0, inf])", "sum(P^k/k^2, [k, 1, inf])",
    "sum(1/(k^2 + P^2), [k, 1, inf])", "sum((-1)^k/(k + P), [k, 0, inf])",
];

/// A conditional answer `conditional_expression(v, c)` checked at parameter values where `c` holds: `v` against the
/// numerical integral or sum.
fn conditional_input(e: &mut CoreEngine, r: &mut Rng, log: &mut Vec<String>) -> Option<String> {
    let tp = *r.pick(CONDITIONAL);
    let t = tp.replace('P', "p");
    let ans = ev(e, &format!("time_constrained({{ w = {t}; w }}, 40)"));
    log.push(format!("{t} = {ans}"));
    if !ans.starts_with("conditional_expression(") {
        return None;
    }
    // at 20 digits (the machine rule misses mass at a singular right end: see the notes)
    let numeric = if tp.starts_with("integrate") { format!("n_{}, working_precision: 20)", &tp[..tp.len() - 1]) } else { format!("n_{tp}") };
    for p in ["1/2", "3/2", "-1/2", "2", "-3/2", "1/3", "3", "-2/3", "5/2", "1/5", "-1", "7"] {
        let holds = ev(e, &format!("w[2] /. p -> ({p})"));
        if holds != "true" {
            continue;
        }
        let exact = ev(e, &format!("time_constrained(n(w[1] /. p -> ({p}), 20), 20)"));
        let num = ev(e, &format!("time_constrained({}, 40)", numeric.replace('P', &format!("({p})"))));
        if real_of(&num).is_none() && !num.contains("im") {
            continue;
        }
        let d = real_of(&ev(e, &format!("n(abs(({exact}) - ({num}))/(1 + abs({num})))")));
        if d.is_some_and(|d| d > 1e-8) {
            return Some(format!("{t} = {ans}: at p = {p} {exact}; numerically {num}"));
        }
    }
    None
}

fn calculus_input(e: &mut CoreEngine, r: &mut Rng, log: &mut Vec<String>) -> Option<String> {
    if r.below(7) == 0 {
        return conditional_input(e, r, log);
    }
    match r.below(6) {
        0 => limit_input(e, r, log),
        1 => series_input(e, r, log),
        2 => integral_input(e, r, log),
        3 => sum_input(e, r, log),
        4 => residue_input(e, r, log),
        _ => dsolve_input(e, r, log),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// 4. exact against approximate

/// Special functions (templates in `A`, `B`, `C` for parameters and `Z` for the argument) and the exceptional
/// parameter values they are tried at.
const EXCEPTIONAL: &[(&str, &[&str])] = &[
    ("bessel_j(A, Z)", &["1/2", "-1/2", "3/2", "-5/2", "0", "2", "-3"]),
    ("bessel_y(A, Z)", &["1/2", "-1/2", "3/2", "-3/2", "0", "1", "-2"]),
    ("bessel_i(A, Z)", &["1/2", "-1/2", "5/2", "-3/2", "0", "-1"]),
    ("bessel_k(A, Z)", &["1/2", "-1/2", "3/2", "5/2", "0", "-2"]),
    ("hankel_h1(A, Z)", &["1/2", "-1/2", "3/2"]),
    ("spherical_bessel_y(A, Z)", &["0", "1", "2", "-1"]),
    ("struve_h(A, Z)", &["1/2", "-1/2", "3/2", "-3/2", "-1"]),
    ("struve_l(A, Z)", &["1/2", "-1/2", "3/2"]),
    ("gamma(A, Z)", &["0", "-1", "-2", "1/2", "-1/2", "3", "-3/2"]),
    ("gamma_regularized(A, Z)", &["0", "1", "1/2", "-1", "3"]),
    ("exp_integral_e(A, Z)", &["0", "-1", "1", "2", "1/2", "-1/2", "3"]),
    ("polygamma(A, Z)", &["0", "1", "2", "3"]),
    ("polylog(A, Z)", &["0", "1", "-1", "-2", "2", "3"]),
    ("zeta(A, Z)", &["2", "3", "0", "-1", "-2", "4"]),
    ("hypergeometric_1f1(A, B, Z)", &["1", "-2", "1/2", "-1/2", "0", "2"]),
    ("hypergeometric_2f1(A, B, C, Z)", &["1", "-1", "1/2", "-3", "0", "3/2", "2"]),
    ("hypergeometric_0f1(A, Z)", &["1/2", "3/2", "-1/2", "1", "2"]),
    ("hypergeometric_u(A, B, Z)", &["1", "0", "-1", "1/2", "2", "-2"]),
    ("legendre_p(A, B, Z)", &["0", "1", "2", "1/2", "-1/2", "-1", "3"]),
    ("legendre_q(A, B, Z)", &["0", "1", "2", "1/2", "-1/2"]),
    ("gegenbauer_c(A, B, Z)", &["0", "1", "2", "1/2", "-1/2", "-1", "3"]),
    ("jacobi_p(A, B, C, Z)", &["0", "1", "2", "1/2", "-1/2", "-1"]),
    ("laguerre_l(A, B, Z)", &["0", "1", "2", "1/2", "-1/2", "-1", "3"]),
    ("hermite_h(A, Z)", &["0", "1", "2", "1/2", "-1/2", "-1", "3"]),
    ("chebyshev_t(A, Z)", &["0", "1", "2", "1/2", "-1/2", "-1"]),
    ("chebyshev_u(A, Z)", &["0", "1", "2", "1/2", "-1/2", "-1"]),
    ("beta(A, B)", &["1/2", "-1/2", "2", "3/2", "-3/2", "1"]),
    ("beta_regularized(Z, A, B)", &["1", "2", "1/2", "3"]),
    ("pochhammer(A, B)", &["0", "-2", "1/2", "-1/2", "3", "-3"]),
    ("binomial(A, B)", &["-1", "-2", "1/2", "-1/2", "3", "5/2"]),
    ("parabolic_cylinder_d(A, Z)", &["0", "1", "-1", "2", "-1/2", "1/2", "-2"]),
    ("whittaker_m(A, B, Z)", &["0", "1/2", "-1/2", "1", "3/2"]),
    ("whittaker_w(A, B, Z)", &["0", "1/2", "-1/2", "1", "3/2"]),
    ("elliptic_e(A, Z)", &["0", "1", "1/2"]),
    ("elliptic_f(Z, A)", &["0", "1", "-1", "1/2"]),
    ("elliptic_pi(A, Z)", &["0", "1/2", "-1"]),
    ("lerch_phi(Z, A, B)", &["1", "2", "1/2", "0", "-1"]),
    ("anger_j(A, Z)", &["0", "1", "1/2", "-1"]),
    ("weber_e(A, Z)", &["0", "1", "1/2", "-1"]),
    ("kelvin_ber(A, Z)", &["0", "1", "1/2", "-1"]),
    ("mittag_leffler_e(A, Z)", &["1", "2", "1/2", "1/3", "0"]),
    ("mittag_leffler_e(A, B, Z)", &["1", "2", "1/2", "-1", "0"]),
    ("spheroidal_ps(A, B, 0, Z)", &["0", "1", "2"]),
    ("coulomb_f(A, B, Z)", &["0", "1", "-1", "1/2"]),
    ("lommel_s1(A, B, Z)", &["1/2", "3/2", "-1/2", "1", "0"]),
    ("airy_ai(Z)", &["0"]),
    ("harmonic_number(Z, A)", &["1", "2", "0", "-1", "1/2"]),
    ("q_pochhammer(A, Z, 3)", &["1/2", "2", "0"]),
    ("jacobi_sn(Z, A)", &["0", "1", "-1", "2"]),
    ("jacobi_cn(Z, A)", &["0", "1", "-1", "2"]),
    ("jacobi_dn(Z, A)", &["0", "1", "-1", "2"]),
    ("jacobi_sc(Z, A)", &["0", "1"]),
    ("jacobi_cd(Z, A)", &["0", "1"]),
    ("jacobi_amplitude(Z, A)", &["0", "1", "-1"]),
    ("inverse_jacobi_sn(1/Z, A)", &["0", "1", "-1", "2"]),
    ("inverse_jacobi_cn(1/Z, A)", &["0", "1"]),
    ("inverse_jacobi_dc(Z, A)", &["0", "1"]),
    ("elliptic_f(Z, A)", &["1", "2", "-1"]),
    ("elliptic_e(Z, A)", &["1", "2", "-1"]),
    ("elliptic_pi(A, Z, 1/3)", &["0", "1/3", "1", "-1"]),
    ("elliptic_pi(A, 1/Z)", &["0", "1/2", "1", "-1"]),
    ("spherical_bessel_j(A, Z)", &["0", "1", "2", "-1", "-2", "1/2"]),
    ("spherical_hankel_h1(A, Z)", &["0", "1", "-1"]),
    ("kelvin_bei(A, Z)", &["0", "1", "1/2", "-1", "2"]),
    ("kelvin_ker(A, Z)", &["0", "1", "1/2", "2"]),
    ("hypergeometric_2f1_regularized(A, B, C, 1/Z)", &["1", "-1", "0", "-2", "1/2", "2"]),
    ("hypergeometric_1f1_regularized(A, B, Z)", &["1", "-1", "0", "-2", "1/2"]),
    ("hypergeometric_0f1_regularized(A, Z)", &["0", "-1", "-2", "1/2", "1"]),
    ("hypergeometric_pfq([A, B], [C], 1/Z)", &["1", "-1", "-2", "1/2", "2"]),
    ("bernoulli_b(A, Z)", &["0", "1", "2", "5"]),
    ("euler_e(A, Z)", &["0", "1", "2", "5"]),
    ("fibonacci(A, Z)", &["0", "1", "2", "5", "-3"]),
    ("lucas_l(A, Z)", &["0", "1", "2", "5", "-3"]),
    ("hurwitz_zeta(A, Z)", &["2", "3", "0", "-1", "-2", "1/2"]),
    ("polylog(A, 1/Z)", &["-1/2", "1/2", "-3/2", "0", "1", "-1"]),
    ("stieltjes_gamma(A, Z)", &["0", "1", "2"]),
    ("gamma_regularized(A, 0, Z)", &["1", "2", "1/2", "0", "-1"]),
    ("beta_regularized(1/Z, A, B)", &["1", "2", "1/2", "-1", "3"]),
    ("inverse_beta_regularized(1/Z, A, B)", &["1", "2", "1/2"]),
    ("sinc(A*Z)", &["0", "1", "1/2"]),
    ("struve_h(A, Z)", &["0", "1", "2", "-2", "3"]),
    ("weber_e(A, Z)", &["2", "-1", "3/2"]),
    ("mittag_leffler_e(A, B, Z)", &["1", "2", "3", "4", "0", "-2", "1/2"]),
    ("airy_bi(A*Z)", &["0", "1", "-1"]),
    ("scorer_gi(A*Z)", &["0", "1"]),
    ("polygamma(A, 1/Z)", &["0", "1", "2", "-1"]),
    ("log_gamma(A + Z)", &["0", "-10", "1/2"]),
    ("barnes_g(A + Z)", &["0", "-3", "1/2"]),
];

/// Arguments of the exact-against-approximate inputs.
const EX_ARGS: &[&str] = &["3", "1/3", "5/2", "-2", "1/2", "-1/3", "7", "2*im", "1 + im", "3/4", "-5/4", "10"];

/// An exact form of a special function at an exceptional parameter (when it evaluates to something else than
/// itself) against the function evaluated directly at 30-digit arguments.
fn exact_input(e: &mut CoreEngine, r: &mut Rng, log: &mut Vec<String>) -> Option<String> {
    let (t, ps) = *r.pick(EXCEPTIONAL);
    let mut exact = t.to_string();
    let mut approx = t.to_string();
    // the direct value at 40-digit arguments, or (one in three) at machine numbers
    let machine = r.below(3) == 0;
    let num = |v: &str| if machine { format!("n({v})") } else { format!("n({v}, 40)") };
    for k in ["A", "B", "C"] {
        let v = *r.pick(ps);
        exact = exact.replace(k, &format!("({v})"));
        approx = approx.replace(k, &num(v));
    }
    let z = *r.pick(EX_ARGS);
    exact = exact.replace('Z', &format!("({z})"));
    approx = approx.replace('Z', &num(z));
    let tol = if machine { 1e-9 } else { 1e-20 };
    let ans = ev(e, &format!("time_constrained({{ v = {exact}; v }}, 30)"));
    log.push(format!("{exact} = {ans}"));
    let head = t.split('(').next().unwrap();
    if ans.starts_with(&format!("{head}(")) || ans.starts_with("failure") || ans.starts_with("$aborted") || ans.starts_with("error") {
        return None;
    }
    let ne = ev(e, "time_constrained(n(v, 30), 30)");
    let na = ev(e, &format!("time_constrained({approx}, 30)"));
    if ne.contains(head) || na.contains(head) || na.starts_with("failure") || na.starts_with("$aborted") {
        return None;
    }
    // a direct value with an unevaluated part (zeta(-1., 4.) at 40 digits) is a gap, not a value
    let plain = |s: &str| s.replace("complex", "").replace("ball", "").replace("indeterminate", "").replace("inf", "").chars().all(|c| !c.is_alphabetic() || "eim".contains(c));
    if !plain(&na) {
        return None;
    }
    let d = ev(e, &format!("n(abs(({ne}) - ({na}))/(1 + abs({na})))"));
    match real_of(&d) {
        Some(d) if d > tol => Some(format!("{exact} = {ans} = {ne}; directly {na}")),
        Some(_) => None,
        None => {
            // one is infinite or not a number while the other is
            if ne != na && (ne.contains("inf") || na.contains("inf") || ne.contains("indeterminate") || na.contains("indeterminate")) {
                Some(format!("{exact} = {ans} = {ne}; directly {na}"))
            } else {
                None
            }
        }
    }
}

/// Functions for the precision check (templates in `Z`), derivatives among them.
const PRECISION_FUNS: &[&str] = &[
    "zeta(Z)", "zeta'(Z)", "zeta''(Z)", "prime_zeta_p(Z)", "prime_zeta_p'(Z)", "gamma(Z)", "gamma'(Z)", "log_gamma(Z)",
    "polygamma(3, Z)", "bessel_j(1/3, Z)", "derivative(1, 0)(bessel_j)(1/3, Z)", "derivative(0, 1)(bessel_j)(1/3, Z)",
    "bessel_k(Z, 2)", "derivative(1, 0)(bessel_k)(Z, 2)", "airy_ai(Z)", "airy_ai'(Z)", "erf(Z)", "erfc(Z)",
    "exp_integral_ei(Z)", "log_integral(Z)", "polylog(3, Z)", "derivative(1, 0)(polylog)(Z, 1/2)",
    "hypergeometric_2f1(1/3, 1/2, 5/4, 1/Z)", "hypergeometric_1f1(1/3, 3/2, Z)", "hypergeometric_u(1/3, 3/2, Z)",
    "struve_h(1/2, Z)", "anger_j(1/3, Z)", "lerch_phi(1/2, 2, Z)", "elliptic_k(1/Z)", "elliptic_e(1/Z)",
    "elliptic_f(Z, 1/3)", "jacobi_sn(Z, 1/3)", "mathieu_c(1, 1/2, Z)", "riemann_siegel_z(Z)", "riemann_siegel_theta(Z)",
    "dedekind_eta(im*Z)", "klein_invariant_j(im*Z)", "barnes_g(Z)", "log_barnes_g(Z)", "factorial2(Z)",
    "gamma(Z, 2)", "beta_regularized(1/Z, 2, Z)", "parabolic_cylinder_d(Z, 1/2)", "whittaker_w(1/2, Z, 3)",
    "legendre_q(1/3, 1/Z)", "spheroidal_ps(1, 0, Z, 1/2)", "coulomb_f(1, Z, 2)", "kelvin_kei(Z)", "zeta(1/2 + im*Z)",
    "stieltjes_gamma(2, Z)", "hurwitz_zeta(3, Z)", "mittag_leffler_e(1/2, Z)", "inverse_erf(1/Z)", "product_log(Z)",
];

/// A value at 30 digits claims a precision (`precision`); the value at 60 digits must lie within that many digits
/// of it (a factor 4 of margin).
fn precision_input(e: &mut CoreEngine, r: &mut Rng, log: &mut Vec<String>) -> Option<String> {
    let f = *r.pick(PRECISION_FUNS);
    let z = *r.pick(&["sqrt(5)", "3/2", "7/3", "pi", "11/4", "13/10", "exp(1)", "5", "17/7", "2^(1/3)"]);
    let x = f.replace('Z', &format!("({z})"));
    let digits = *r.pick(&[20, 30, 45]);
    let a = ev(e, &format!("time_constrained({{ va = n({x}, {digits}); [va, precision(va)] }}, 30)"));
    log.push(format!("n({x}, {digits}) -> {a}"));
    let els = elements(&a);
    let [va, pa] = els.as_slice() else { return None };
    let p = real_of(pa)?;
    if va.contains('(') || p > 1000.0 {
        return None;
    }
    // the midpoints compared exactly
    let d = ev(
        e,
        &format!(
            "time_constrained({{ vb = n({x}, {}); n(abs(set_precision(va, inf) - set_precision(vb, inf))/abs(set_precision(vb, inf)), 5) }}, 60)",
            digits * 2 + 10
        ),
    );
    let rel = real_of(&d)?;
    (rel > 4.0 * 10f64.powf(-p)).then(|| format!("n({x}, {digits}) = {va} claims {p} digits; relative difference {rel}"))
}

// ---------------------------------------------------------------------------------------------------------------
// tests

fn report(name: &str, (done, faults): (usize, Vec<String>)) {
    eprintln!("{name}: {done} inputs, {} faults", faults.len());
    assert!(faults.is_empty(), "{name}: {} faults\n{}", faults.len(), faults.join("\n"));
}

#[test]
fn properties_short() {
    report("properties", drive(&Run::short("properties", 40), 0, property_input));
}

#[test]
fn enclosures_short() {
    report("enclosures", drive(&Run::short("enclosures", 40), 0x5151, enclosure_input));
}

#[test]
fn calculus_short() {
    report("calculus", drive(&Run::short("calculus", 30), 0xca1c, calculus_input));
}

#[test]
fn exact_short() {
    report("exact", drive(&Run::short("exact", 60), 0xe8ac, exact_input));
}

// ---------------------------------------------------------------------------------------------------------------
// the defects the generators found, each with two variations checked by an independent route

/// Is `src` true (evaluated as given, or at 30 digits)?
fn holds(e: &mut CoreEngine, src: &str) -> bool {
    ev(e, src) == "true" || ev(e, &format!("n({src}, 30)")) == "true"
}

#[test]
fn images_across_a_jump_with_a_continuous_derivative() {
    let mut e = CoreEngine::new();
    // acot jumps from -pi/2 to pi/2 at 0 while acot' = -1/(1 + x^2) stays bounded: no image is proved across 0
    assert_eq!(ev(&mut e, "acot(interval([-5, 5]))"), "acot(interval([-5, 5]))");
    assert_eq!(ev(&mut e, "acot(centered_interval(0, 1))"), "acot(centered_interval(0.0, 1.0))");
    // independent: values on the two sides of 0 lie far outside the old result [-acot 5, acot 5]
    assert!(holds(&mut e, "acot(-1/10) < -acot(5) and acot(1/10) > acot(5)"));
    // an end at 0 from the right is continuous (acot(0) = pi/2): decreasing on [0, 2]
    assert_eq!(ev(&mut e, "acot(interval([0, 2]))"), "interval([acot(2), (1/2)*pi])");
    // fractional_part jumps at the integers other than 0, mod(x, m) at the multiples of m
    assert_eq!(ev(&mut e, "fractional_part(interval([0.5, 1.5]))"), "fractional_part(interval([0.5, 1.5]))");
    assert_eq!(ev(&mut e, "fractional_part(centered_interval(1, 0.25))"), "fractional_part(centered_interval(1.0, 0.25))");
    assert_eq!(ev(&mut e, "mod(interval([0.5, 1.5]), -1)"), "mod(interval([0.5, 1.5]), -1)");
    assert!(holds(&mut e, "fractional_part(99/100) > 1/2 and fractional_part(101/100) < 1/2"));
    // continuous pieces keep their images: [1, 3/2] from the right of 1, [-3/2, -1] from the left of -1, mod by 2
    // on [-7/2, -5/2] (x + 4)
    assert_eq!(
        ev(&mut e, "[fractional_part(interval([1, 1.5])), fractional_part(interval([-1.5, -1])), fractional_part(interval([-0.5, 0.5])), mod(interval([-3.5, -2.5]), 2)]"),
        "[interval([0, 1/2]), interval([-1/2, 0]), interval([-1/2, 1/2]), interval([1/2, 3/2])]"
    );
    assert!(holds(&mut e, "fractional_part(-6/5) == -1/5 and mod(-3, 2) == 1"));
}

#[test]
fn limits_whose_series_has_a_zero_constant_not_seen_as_zero() {
    let mut e = CoreEngine::new();
    // the series of sinh(x) - sinh(2) at 2 had the constant -sinh(2) - e^-2/2 + e^2/2 (zero), taken as its leading
    // term: the limit was 0; it is C'(2)/cosh(2) = 1/cosh(2)
    let l = ev(&mut e, "limit((fresnel_c(x) - fresnel_c(2))/(sinh(x) - sinh(2)), x -> 2)");
    assert!(holds(&mut e, &format!("abs(({l}) - 1/cosh(2)) < 10^-25")), "{l}");
    // independent: the difference quotient near 2
    assert!(holds(&mut e, &format!("abs(((fresnel_c(2 + 10^-12) - fresnel_c(2))/(sinh(2 + 10^-12) - sinh(2))) - ({l})) < 10^-10")));
    // variations: a Bessel numerator (-J_1(2)/cosh(2)), a cosh denominator (1/sinh(2)), at pi/2 from below
    let l = ev(&mut e, "limit((bessel_j(0, x) - bessel_j(0, 2))/(sinh(x) - sinh(2)), x -> 2)");
    assert!(holds(&mut e, &format!("abs(({l}) + bessel_j(1, 2)/cosh(2)) < 10^-25")), "{l}");
    let l = ev(&mut e, "limit((fresnel_c(x) - fresnel_c(2))/(cosh(x) - cosh(2)), x -> 2)");
    assert!(holds(&mut e, &format!("abs(({l}) - 1/sinh(2)) < 10^-25")), "{l}");
    let l = ev(&mut e, "limit((fresnel_c(x) - fresnel_c(pi/2))/(sinh(x) - sinh(pi/2)), x -> pi/2, direction: from_below)");
    assert!(holds(&mut e, &format!("abs(({l}) - cos(pi^3/8)/cosh(pi/2)) < 10^-25")), "{l}");
    assert!(holds(&mut e, &format!("abs(((fresnel_c(pi/2 - 10^-12) - fresnel_c(pi/2))/(sinh(pi/2 - 10^-12) - sinh(pi/2))) - ({l})) < 10^-10")));
}

#[test]
fn limits_onto_branch_cuts_stay_quick() {
    let mut e = CoreEngine::new();
    // the zero test of a leading coefficient decides a complex constant (elliptic_k(2) = 1.311 - 1.311 i) by its
    // enclosure; the sign of a complex constant took 12 s and gave nothing
    let t0 = Instant::now();
    assert_eq!(ev(&mut e, "limit(elliptic_k(2 + t*im), t -> 0, direction: from_below)"), "elliptic_k(2)");
    assert!(holds(&mut e, "abs(limit(log(-2 + t*im), t -> 0, direction: from_below) - (log(2) - im*pi)) < 10^-25"));
    assert!(holds(&mut e, "abs(n(elliptic_k(2 - 10^-20*im), 20) - n(elliptic_k(2), 20)) < 10^-15"));
    // variations: elliptic_e on its cut, and the zero constant of the sinh limit still seen
    // elliptic_e(2) = 0.5991 + 0.5991 i is the value from below (at 40 digits it agrees with elliptic_e(2 - 10^-30 im)
    // to 10^-27), while elliptic_e(2 + 10^-30 im) = 0.5991 - 0.5991 i: from above the limit is the conjugate
    assert_eq!(ev(&mut e, "limit(elliptic_e(2 + t*im), t -> 0, direction: from_above)"), "conjugate(elliptic_e(2))");
    assert!(holds(&mut e, "abs(n(elliptic_e(2 + 10^-30*im), 40) - n(conjugate(elliptic_e(2)), 40)) < 10^-25"));
    assert!(holds(&mut e, "abs(n(elliptic_e(2 - 10^-30*im), 40) - n(elliptic_e(2), 40)) < 10^-25"));
    let l = ev(&mut e, "limit((fresnel_c(x) - fresnel_c(2))/(sinh(x) - sinh(2)), x -> 2)");
    assert!(holds(&mut e, &format!("abs(({l}) - 1/cosh(2)) < 10^-25")), "{l}");
    let limit = Duration::from_secs(4).mul_f64(std::env::var("MC_TEST_TIME_FACTOR").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0));
    assert!(t0.elapsed() < limit, "{:?}", t0.elapsed());
}

#[test]
fn no_series_at_a_jump_of_a_periodic_function() {
    let mut e = CoreEngine::new();
    // sawtooth_wave(x) is x just right of 0 and 1 + x just left: there is no series at 0 (it was x for both sides)
    assert_eq!(ev(&mut e, "series(sawtooth_wave(x^3) + x, x, 0, 5)"), "series(x + sawtooth_wave(x^3), x, 0, 5)");
    assert!(holds(&mut e, "sawtooth_wave(-10^-3) > 9/10 and sawtooth_wave(10^-3) < 1/10"));
    // variations: the square wave at 0 (1 on the right, -1 on the left), mod(x, 1) at 1; away from the jumps the
    // series stay, and a side given by the assumptions gives that side's series
    assert_eq!(ev(&mut e, "series(square_wave(x), x, 0, 2)"), "series(square_wave(x), x, 0, 2)");
    assert_eq!(ev(&mut e, "series(mod(x, 1), x, 1, 2)"), "series(mod(x, 1), x, 1, 2)");
    assert_eq!(ev(&mut e, "series(mod(x, 3), x, 1, 2)"), "1 + (x - 1) + O((x - 1)^3)");
    assert_eq!(ev(&mut e, "series(sawtooth_wave(x), x, 0, 3, assumptions: x > 0)"), "x + O(x^4)");
    assert!(holds(&mut e, "mod(1 - 10^-3, 1) > 9/10 and mod(1 + 10^-3, 1) < 1/10"));
}

#[test]
fn monotonic_only_on_a_domain_of_one_piece() {
    let mut e = CoreEngine::new();
    // f' > 0 on the domain, which has gaps: tan on x > 0 (poles at pi/2 + k pi) and -1/x on x != 0 are not monotonic
    assert_eq!(ev(&mut e, "function_monotonicity([tan(x), x > 0], x)"), "indeterminate");
    assert_eq!(ev(&mut e, "function_monotonicity([-1/x, x != 0], x)"), "indeterminate");
    assert!(holds(&mut e, "tan(3/2) > tan(2) and -1/(-1) > -1/1"));
    // variations: the reported inputs (poles of tan(x - 3) and tan(x/3 - 1) inside the region), and the convexity
    // of 1/x^2 across 0
    assert_eq!(ev(&mut e, "function_monotonicity([-tan(3 - x), 1/2 <= x and x <= 10], x)"), "indeterminate");
    assert_eq!(ev(&mut e, "function_monotonicity([tan(x/3 - 1), x > 4], x, strict_inequalities: true)"), "indeterminate");
    assert!(holds(&mut e, "tan(15/2/3 - 1) > tan(8/3 - 1)"));
    assert_eq!(ev(&mut e, "function_convexity([1/x^2, x != 0], x)"), "indeterminate");
    // one piece keeps its proof; -1/x is injective on x != 0 (its pieces have disjoint images)
    assert_eq!(ev(&mut e, "[function_monotonicity([tan(x), -1 < x and x < 1], x), function_monotonicity([1/x, x > 0], x), function_injective([-1/x, x != 0], x)]"), "[1, -1, true]");
}

#[test]
fn ball_ends_far_apart_take_no_memory() {
    let mut e = CoreEngine::new();
    // the enclosures of exp(-1/x^2) near 0 have a midpoint near 2^-(10^12) and a far smaller radius: their exact ends
    // asked for 180 GB and ended the process
    assert_eq!(ev(&mut e, "function_injective([erf(1/x), x > 0], x)"), "true");
    assert_eq!(ev(&mut e, "function_injective(erf(1/x), x)"), "true");
    // independent: erf(1/x) decreases on each side of 0 and its images (0, 1) and (-1, 0) are disjoint
    assert!(holds(&mut e, "erf(1/2) > erf(1/3) and erf(1/3) > 0 and erf(-1/2) < 0"));
    // variations: the monotonicity of exp(-1/x^2) on x > 0, and a comparison of a ball with a tiny midpoint
    assert_eq!(ev(&mut e, "function_monotonicity([exp(-1/x^2), x > 0], x)"), "1");
    assert_eq!(ev(&mut e, "n(exp(-10^12), 30) < n(exp(-10^11), 30)"), "true");
}

#[test]
fn exact_forms_agree_with_the_numbers() {
    let mut e = CoreEngine::new();
    // 1F1(-n; -n; z) is the polynomial ending with z^n/n! (as 2F1(-n, b; -n; z) and the numerical value), not e^z
    assert_eq!(ev(&mut e, "[hypergeometric_1f1(-2, -2, z), hypergeometric_pfq([-2], [-2], z), hypergeometric_1f1(-2, -2, 3)]"), "[1 + z + (1/2)*z^2, 1 + z + (1/2)*z^2, 17/2]");
    assert!(holds(&mut e, "hypergeometric_1f1(-2, n(-2, 30), n(3, 30)) == 17/2"));
    assert_eq!(ev(&mut e, "[hypergeometric_1f1(a, a, z), hypergeometric_1f1(2, 2, z), hypergeometric_1f1(0, 0, z)]"), "[exp(z), exp(z), 1]");
    // the associated Legendre function beyond 1 at 30 digits has the sign of the exact and machine values
    // (P_2^1(x) = -3 x (1 - x^2)^(1/2))
    assert!(holds(&mut e, "abs(im_part(legendre_p(n(2, 30), n(1, 30), n(3, 30))) + 18*sqrt(2)) < 10^-25"));
    assert_eq!(ev(&mut e, "legendre_p(2, 1, 3)"), "-18im*2^(1/2)");
    assert!(holds(&mut e, "abs(im_part(legendre_p(n(2, 30), n(1, 30), n(-3, 30))) - 18*sqrt(2)) < 10^-25"));
    // Laguerre, Jacobi, Gegenbauer polynomials at an inexact natural degree: the polynomial (L_3^(-2)(x) = x^2/2 -
    // x^3/6, which is 4/81 at 1/3; it was 0 at 30 digits and indeterminate at machine precision)
    assert_eq!(ev(&mut e, "laguerre_l(3, -2, 1/3)"), "4/81");
    assert!(holds(&mut e, "abs(laguerre_l(n(3, 30), n(-2, 30), n(1/3, 30)) - 4/81) < 10^-28"));
    assert!(holds(&mut e, "abs(laguerre_l(3., -2., 1/3.) - 4/81) < 10^-14 and abs(laguerre_l(2., -1, 3/4) + 15/32) < 10^-14"));
    assert!(holds(&mut e, "abs(jacobi_p(n(2, 30), n(-1, 30), n(-1, 30), n(1/3, 30)) - (1/9 - 1)/4) < 10^-28"));
    // E_(1,2)(z) = (e^z - 1)/z: the closed form kept the term 1/z of negative index (it was e^z/z)
    assert_eq!(ev(&mut e, "[mittag_leffler_e(1, 2, z), mittag_leffler_e(1, 3, z), mittag_leffler_e(1, 0, z)]"), "[-z^-1 + exp(z)/z, -z^-2 - z^-1 + exp(z)*z^-2, z*exp(z)]");
    assert!(holds(&mut e, "abs(mittag_leffler_e(1, 2, 2*im) - mittag_leffler_e(1, 2, 2.*im)) < 10^-14"));
    assert!(holds(&mut e, "abs(mittag_leffler_e(2, 3, 2*im) - sum((2*im)^k/factorial(2*k + 2), [k, 0, 60])) < 10^-25"));
    assert!(holds(&mut e, "abs(mittag_leffler_e(3, 5, 2*im) - sum((2*im)^k/factorial(3*k + 4), [k, 0, 60])) < 10^-25"));
    // P_l^-m with m > l is not 0: P_0^-1(x) = ((1 - x)/(1 + x))^(1/2), as at machine precision
    assert_eq!(ev(&mut e, "[legendre_p(0, -1, x), legendre_p(1, -2, 1/3), legendre_p(0, 1, x)]"), "[(1 + x)^(-1/2)*(1 - x)^(1/2), 7/36, 0]");
    assert!(holds(&mut e, "abs(legendre_p(1., -3., 0.5) - legendre_p(1, -3, 1/2)) < 10^-14"));
    // a degree of 40 digits with an integer value: the polynomial (H_1(x) = 2 x)
    assert!(holds(&mut e, "abs(hermite_h(n(1, 40), n(3/4, 40)) - 3/2) < 10^-35 and abs(chebyshev_t(n(4, 30), n(1/3, 30)) - 17/81) < 10^-28"));
    // a numerical derivative claims only the digits it has (zeta'(sqrt(5)) at 40 digits claimed 49, right to 44)
    let p = real_of(&ev(&mut e, "precision(n(zeta'(sqrt(5)), 40))")).unwrap();
    assert!((40.0..46.0).contains(&p), "{p}");
    assert!(holds(&mut e, "v = n(zeta'(sqrt(5)), 40); w = n(zeta'(sqrt(5)), 80); abs(set_precision(v, inf) - set_precision(w, inf)) < 10^-(precision(v))"));
}

/// Property queries of the generator (the function, a restriction, the property).
fn property_queries(seed: u64, n: usize) -> Vec<String> {
    let mut r = Rng(seed);
    (0..n)
        .map(|_| {
            let f = prop_function(&mut r).replace('X', "x");
            let (cond, ..) = restriction(&mut r);
            let arg = if cond.is_empty() { f } else { format!("[{f}, {}]", cond.replace('X', "x")) };
            let p = *r.pick(&["function_sign", "function_monotonicity", "function_convexity", "function_injective"]);
            format!("{p}({arg}, x)")
        })
        .collect()
}

/// The answers of the property queries do not depend on the thread count or on the speed of the machine: the
/// queries that answer within 2 s on 4 threads give the same printed answers on 1 and 16 threads and with every
/// block of fuel delayed (a slower machine). Answers rest on budgets of work, not of time.
fn same_answers(seed: u64, n: usize) -> (usize, Vec<String>) {
    use mc_engine::EngineConfig;
    let fresh = |threads: usize| CoreEngine::with_config(EngineConfig { threads, ..EngineConfig::default() });
    let mut base = vec![];
    for q in property_queries(seed, n) {
        let t0 = Instant::now();
        let a = ev(&mut fresh(4), &q);
        if t0.elapsed() < Duration::from_secs(2) {
            base.push((q, a));
        }
    }
    let mut faults = vec![];
    for (threads, slow) in [(1, 0), (16, 0), (1, 20)] {
        mc_eval::set_test_slowdown(slow);
        for (q, a) in &base {
            let b = ev(&mut fresh(threads), q);
            if &b != a {
                faults.push(format!("{q}: {a} on 4 threads, {b} on {threads} (delay {slow} us)"));
            }
        }
        mc_eval::set_test_slowdown(0);
    }
    (base.len(), faults)
}

#[test]
fn same_answers_on_threads_and_load_short() {
    report("threads", same_answers(4401, 12));
}

#[test]
#[ignore]
fn campaign_threads() {
    let run = Run::long("threads");
    report("threads", same_answers(run.seed, run.inputs.min(200)));
}

#[test]
#[ignore]
fn campaign_properties() {
    report("properties", drive(&Run::long("properties"), 0, property_input));
}

#[test]
#[ignore]
fn campaign_enclosures() {
    report("enclosures", drive(&Run::long("enclosures"), 0x5151, enclosure_input));
}

#[test]
#[ignore]
fn campaign_calculus() {
    report("calculus", drive(&Run::long("calculus"), 0xca1c, calculus_input));
}

#[test]
fn precision_short() {
    report("precision", drive(&Run::short("precision", 30), 0x9ec1, precision_input));
}

#[test]
#[ignore]
fn campaign_precision() {
    report("precision", drive(&Run::long("precision"), 0x9ec1, precision_input));
}

#[test]
#[ignore]
fn campaign_exact() {
    report("exact", drive(&Run::long("exact"), 0xe8ac, exact_input));
}
