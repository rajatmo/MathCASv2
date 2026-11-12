//! The rule table through the whole engine: integrands the tower engine and the elementary rules leave, answered
//! by the rules (or by the substitutions after them), each checked here again by differentiation; the dispatch's
//! candidate sets; bounded time.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use std::time::{Duration, Instant};

struct Sys {
    engine: CoreEngine,
    stop: StopFlag,
}

impl Sys {
    fn new() -> Sys {
        Sys {
            engine: CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() }),
            stop: StopFlag::new(),
        }
    }
    fn run(&mut self, src: &str) -> String {
        match self.engine.eval(src, &self.stop) {
            Ok(e) => e.printed,
            Err(err) => panic!("{src}: {err}"),
        }
    }
    /// `integrate(f, x)` found and `d(F, x) - f` zero (simplified, or numerically at the points).
    fn check(&mut self, f: &str, points: &[&str]) -> Result<String, String> {
        let shown = self.run(&format!("ff = {f}; FF = integrate(ff, x); FF"));
        if shown.contains("integrate(") {
            return Err(format!("integrate({f}, x) not found: {shown}"));
        }
        if self.run("simplify(d(FF, x) - ff)") == "0" {
            return Ok(shown);
        }
        for p in points {
            let out = self.run(&format!("n(1 + rewrite((d(FF, x) - ff)*10^20, [x -> {p}]), 30)"));
            let ok = out.trim_end_matches('.').parse::<f64>().is_ok_and(|v| (v - 1.0).abs() < 1.0);
            if !ok {
                return Err(format!("integrate({f}, x) = {shown}: d(F, x) - f at x = {p} is {out}"));
            }
        }
        Ok(shown)
    }
}

const POINTS: &[&str] = &["(3/7)", "(5/11)", "(2/9)"];

fn all_found(fs: &[&str]) {
    let mut s = Sys::new();
    let mut failures = vec![];
    for f in fs {
        if let Err(e) = s.check(f, POINTS) {
            failures.push(e);
        }
    }
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}

/// Fractional powers of sines and cosines: reduction formulas down to elliptic integrals, substitutions for odd
/// powers, hypergeometric functions otherwise (section 4.1 of the table).
#[test]
fn sine_cosine_powers() {
    all_found(&[
        "sin(x)^(1/2)",
        "cos(x)^(-1/2)",
        "sin(x)^(3/2)",
        "cos(2*x - 1/2)^(5/2)",
        "sin(x)^(1/3)",
        "cos(x)^(1/3)*sin(x)^5",
        "sin(x)^(2/3)*cos(x)^3",
        "sin(x)^(1/2)*cos(x)^(1/2)",
        "(3*sin(x))^(3/2)",
    ]);
}

/// Powers of a + b sin, a + b cos: elementary for integer and half-integer powers when a^2 = b^2, the continuous
/// arctangent form, elliptic square roots (section 4.1.1.1).
#[test]
fn linear_in_sine() {
    all_found(&[
        "1/(2 + sin(x))",
        "1/(2 + sin(x))^2",
        "1/(3 + cos(x))",
        "(1 + sin(x))^(1/2)",
        "(1 - cos(x))^(3/2)",
        "1/(1 + sin(x))^2",
        "(2 + sin(x))^(1/2)",
        "cos(x)*(3 + 2*sin(x))^(1/3)",
        "1/(a + b*sin(x))",
    ]);
}

/// Tangents, secants and their reciprocals: fractional powers by u = tan, products with secants (sections 4.3,
/// 4.5) and rational functions by Bioche's substitutions.
#[test]
fn tangents_and_secants() {
    all_found(&[
        "tan(x)^(1/2)",
        "cot(x)^(1/3)",
        "1/(2 + tan(x))",
        "1/(a + b*tan(x))",
        "sec(x)^2/(2 + sin(x))",
        "sin(x)^2/(1 + cos(x)^2)",
        "1/(3 + 5*sin(x))^3",
        "tan(x)^3*sec(x)^(1/2)",
        "sec(x)^(3/2)",
        "sec(x)^3/(2*cos(x))^(3/2)",
        "(3*cot(x)^2)^(3/2)",
        "(5*sec(x))^(1/2)",
    ]);
}

/// Hyperbolic powers the tower engine does not take (fractional exponents), rational functions of hyperbolic
/// functions by the hyperbolic forms of Bioche's substitutions.
#[test]
fn hyperbolic_powers() {
    all_found(&[
        "sinh(x)^(1/2)",
        "cosh(x)^(3/2)",
        "tanh(x)^(1/2)",
        "cosh(x)^3*sinh(x)^(1/2)",
        "1/(3 + 5*cosh(2*x + 1))",
        "cosh(x)/(2 + 3*tanh(x)^2)",
        "sech(x)^4/(2 + cosh(x)^2)",
    ]);
}

/// Inverse functions times powers of x: integration by parts, the algebraic rest by the binomial rules.
#[test]
fn inverse_functions() {
    all_found(&["acos(x)/x^2", "asec(x)", "x*asec(x)", "acsc(x)/x^2", "acoth(x)", "x^2*acot(x)", "asin(x)/x^3"]);
}

/// Binomials: Chebyshev's cases to linear radicals, hypergeometric functions otherwise.
#[test]
fn binomials() {
    all_found(&[
        "(1 + x^2)^(1/3)",
        "x^(1/2)/(1 + x^2)",
        "(1 + x)^(1/3)/x",
        "x^3*(1 + x^2)^(1/3)",
        "1/(x*(1 - x^2)^(1/2))",
        "x^2*(1 + x^3)^(1/4)",
    ]);
}

/// Logarithms of sines and cosines (dilogarithms), exponentials times powers of sines.
#[test]
fn logarithms_and_exponentials() {
    all_found(&["log(sin(x))", "log(cos(2*x))", "x*cot(x)", "exp(x)*sin(x)^2", "exp(2*x)*cos(x)^3"]);
}

/// The answers of a few rules, as printed.
#[test]
fn printed_forms() {
    let mut s = Sys::new();
    assert_eq!(s.run("integrate(sin(x)^(1/2), x)"), "2*elliptic_e((1/2)*(x + (-1/2)*pi), 2)");
    assert_eq!(s.run("integrate(1/(a + b*tan(x)), x)"), "a*x/(a^2 + b^2) + b*log(a*cos(x) + b*sin(x))/(a^2 + b^2)");
    assert_eq!(s.run("integrate((1 + sin(x))^(1/2), x)"), "-2*cos(x)*(1 + sin(x))^(-1/2)");
}

/// Integrals outside the table stay unevaluated, quickly.
#[test]
fn bounded() {
    let mut s = Sys::new();
    for f in ["sin(x)^(1/2)*log(x)", "sin(cos(x))", "(1 + sin(x)^3)^(1/2)", "x^x"] {
        let t = Instant::now();
        let out = s.run(&format!("integrate({f}, x)"));
        assert!(out.starts_with("integrate("), "{f}: {out}");
        assert!(t.elapsed() < Duration::from_secs(20), "{f}: {:?}", t.elapsed());
    }
}

/// The dispatch: a rule is a candidate only when the functions it names occur; thousands of dispatches take
/// microseconds each.
#[test]
fn dispatch() {
    use mc_integrate_rules::table::{bit, table};
    let t = table().unwrap();
    let mut c = vec![];
    let sin_cos = bit("sin") | bit("cos");
    t.candidates(sin_cos, &mut c);
    assert!(!c.is_empty());
    assert!(c.iter().all(|&i| t.rules[i].mask & !sin_cos == 0));
    let n_sin_cos = c.len();
    t.candidates(bit("atan"), &mut c);
    assert!(c.len() < n_sin_cos);
    t.candidates(0, &mut c);
    assert!(c.iter().all(|&i| t.rules[i].mask == 0), "with no function only the algebraic rules");
    let start = Instant::now();
    let mut total = 0;
    for k in 0..100_000u64 {
        t.candidates(k.rotate_left(7) & ((1 << 30) - 1), &mut c);
        total += c.len();
    }
    assert!(total > 0);
    assert!(start.elapsed() < Duration::from_secs(5), "{:?}", start.elapsed());
}

/// Optional runs `optional(xs__, d)` in patterns (the matcher support the rules use): under a product or a sum
/// the run stands for the product or sum of the arguments it takes, and for `d` when there are none; under other
/// heads it is the run, and `[d]` when absent.
#[test]
fn optional_runs() {
    let mut s = Sys::new();
    for (src, want) in [
        ("rewrite(-x^-2/c^2, optional(b__, 1)*x^n_ -> [b, n])", "[-c^-2, -2]"),
        ("rewrite(x^-2, optional(b__, 1)*x^n_ -> [b, n])", "[1, -2]"),
        ("rewrite(3*y*x^2, optional(b__, 1)*x^n_ -> [b, n])", "[3*y, 2]"),
        ("rewrite(1 + y + x, optional(a__, 0) + x -> a)", "1 + y"),
        ("rewrite(f(1, 2, 3), f(optional(a__, 0), z_) -> [a, z])", "[[1, 2], 3]"),
        ("rewrite(f(3), f(optional(a__, 0), z_) -> [a, z])", "[[0], 3]"),
    ] {
        assert_eq!(s.run(src), want, "{src}");
    }
}
