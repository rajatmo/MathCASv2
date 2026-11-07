//! The multidimensional methods: adaptive cubature, VEGAS+, lattice and Sobol quasi-Monte Carlo, plain Monte
//! Carlo; automatic choice, goals, options, messages, failures, and results independent of the thread count.
mod common;
use common::{close, Sys};

/// `around(v, e)` as (v, e).
fn around(out: &str) -> (f64, f64) {
    let inner =
        out.strip_prefix("around(").and_then(|s| s.strip_suffix(')')).unwrap_or_else(|| panic!("not an around: {out}"));
    let (v, e) = inner.split_once(", ").unwrap();
    (v.parse().unwrap(), e.parse().unwrap())
}

/// Smooth integrands in 2 to 7 variables by the default method (cubature) against closed forms.
#[test]
fn cubature_against_closed_forms() {
    let mut s = Sys::new();
    let e1 = 1f64.exp() - 1.0;
    let cases: &[(&str, f64, f64)] = &[
        ("n_integrate(exp(x + y), [x, 0, 1], [y, 0, 1])", e1 * e1, 1e-12),
        ("n_integrate(cos(x + 2*y), [x, 0, 1], [y, 0, 1])", {
            // Re of (e^i - 1)/i (e^2i - 1)/(2i)
            let (c1, s1, c2, s2) = (1f64.cos(), 1f64.sin(), 2f64.cos(), 2f64.sin());
            // (e^ia - 1)/(ia) = (sin a + i (1 - cos a))/a
            let (ar, ai) = (s1, 1.0 - c1);
            let (br, bi) = (s2 / 2.0, (1.0 - c2) / 2.0);
            ar * br - ai * bi
        }, 1e-10),
        ("n_integrate(x*y*z, [x, 0, 1], [y, 0, 1], [z, 0, 2])", 0.5, 1e-14),
        // the fourth difference of t^3 log(t)/6 at 1, ..., 5
        ("n_integrate(1/(1 + x + y + z + w), [x, 0, 1], [y, 0, 1], [z, 0, 1], [w, 0, 1])", {
            let l = |k: f64| k * k * k * k.ln();
            (-4.0 * l(2.0) + 6.0 * l(3.0) - 4.0 * l(4.0) + l(5.0)) / 6.0
        }, 1e-8),
        ("n_integrate(exp(-(a1^2 + a2^2 + a3^2 + a4^2 + a5^2)), [a1, 0, 1], [a2, 0, 1], [a3, 0, 1], [a4, 0, 1], [a5, 0, 1])",
            0.746824132812427f64.powi(5), 1e-8),
        ("n_integrate(cos(a1 + a2 + a3 + a4 + a5 + a6 + a7), [a1, 0, 1], [a2, 0, 1], [a3, 0, 1], [a4, 0, 1], [a5, 0, 1], [a6, 0, 1], [a7, 0, 1])", {
            // Re (2 sin(1/2))^7 e^(7i/2)
            (2.0 * 0.5f64.sin()).powi(7) * 3.5f64.cos()
        }, 1e-7),
    ];
    for &(src, want, tol) in cases {
        close(src, s.real(src), want, tol);
    }
}

/// Inner ranges depending on outer variables, infinite ranges and split points under cubature.
#[test]
fn cubature_domains() {
    let mut s = Sys::new();
    let pi = std::f64::consts::PI;
    close("triangle", s.real("n_integrate(x*y, [x, 0, 1], [y, 0, x], method: cubature)"), 0.125, 1e-14);
    close("disc", s.real("n_integrate(1, [x, -1, 1], [y, -sqrt(1 - x^2), sqrt(1 - x^2)], method: cubature)"), pi, 1e-7);
    close("plane", s.real("n_integrate(exp(-x^2 - y^2), [x, -inf, inf], [y, -inf, inf], method: cubature)"), pi, 1e-8);
    close("half", s.real("n_integrate(exp(-x - y), [x, 0, inf], [y, x, inf], method: cubature)"), 0.5, 1e-8);
    close(
        "split",
        s.real("n_integrate(abs(x - 1/3)*y, [x, 0, 1/3, 1], [y, 0, 1], method: cubature)"),
        5.0 / 36.0,
        1e-12,
    );
    // a corner singularity (every rule point is inside its region)
    close(
        "corner",
        s.real("n_integrate(1/sqrt(x*y), [x, 0, 1], [y, 0, 1], method: cubature, precision_goal: 6)"),
        4.0,
        1e-5,
    );
}

/// The sampling methods give `around(value, error)` within a few errors of the integral, in 3 to 12 variables.
#[test]
fn sampling_methods() {
    let mut s = Sys::new();
    let e1 = 1f64.exp() - 1.0;
    let smooth3 = ("exp(x + y + z)", "[x, 0, 1], [y, 0, 1], [z, 0, 1]", e1.powi(3));
    let vars12: Vec<String> = (1..=12).map(|i| format!("[x{i}, 0, 1]")).collect();
    let sum12: Vec<String> = (1..=12).map(|i| format!("x{i}")).collect();
    let smooth12 = (
        format!("exp(-({})/12)", sum12.join(" + ")),
        vars12.join(", "),
        (12.0 * (1.0 - (-1.0f64 / 12.0).exp())).powi(12),
    );
    for method in ["vegas", "lattice", "sobol", "monte_carlo"] {
        for (f, r, want) in [
            (smooth3.0.to_string(), smooth3.1.to_string(), smooth3.2),
            (smooth12.0.clone(), smooth12.1.clone(), smooth12.2),
        ] {
            let src = format!("n_integrate({f}, {r}, method: {method})");
            let (v, e) = around(&s.run(&src));
            assert!(e > 0.0 && e <= 2e-3 * want.abs(), "{src}: error {e}");
            assert!((v - want).abs() <= 6.0 * e + 1e-12, "{src}: {v} +- {e} vs {want}");
        }
    }
    // the quasi-Monte Carlo errors on a smooth integrand are far below plain Monte Carlo's at the same goal
    let (_, el) =
        around(&s.run(&format!("n_integrate({}, {}, method: lattice, points: 40960)", smooth12.0, smooth12.1)));
    let (_, em) =
        around(&s.run(&format!("n_integrate({}, {}, method: monte_carlo, points: 40960)", smooth12.0, smooth12.1)));
    assert!(el < em / 20.0, "lattice {el} vs monte carlo {em}");
}

/// VEGAS adapts to a sharp peak and to integrable singularities that defeat plain sampling.
#[test]
fn vegas_adapts() {
    let mut s = Sys::new();
    // a narrow Gaussian peak in 4 variables: (sqrt(pi)/20 erf(10))^4
    let want = (std::f64::consts::PI.sqrt() / 20.0).powi(4);
    let src = "n_integrate(exp(-400*((x - 0.5)^2 + (y - 0.5)^2 + (z - 0.5)^2 + (w - 0.5)^2)), [x, 0, 1], [y, 0, 1], [z, 0, 1], [w, 0, 1], method: vegas)";
    let (v, e) = around(&s.run(src));
    assert!((v - want).abs() <= 6.0 * e && e < 2e-3 * want, "{v} +- {e} vs {want}");
    // a product of end singularities in 8 variables (infinite variance for uniform sampling): 2^8
    let vars: Vec<String> = (1..=8).map(|i| format!("[x{i}, 0, 1]")).collect();
    let prod: Vec<String> = (1..=8).map(|i| format!("x{i}")).collect();
    let src = format!("n_integrate(1/sqrt({}), {}, method: vegas, precision_goal: 2)", prod.join("*"), vars.join(", "));
    let (v, e) = around(&s.run(&src));
    assert!((v - 256.0).abs() <= 6.0 * e && e < 0.03 * 256.0, "{v} +- {e}");
}

/// Above 7 variables the default is the lattice rule for analytic integrands and VEGAS otherwise.
#[test]
fn automatic_choice() {
    let mut s = Sys::new();
    let vars: Vec<String> = (1..=8).map(|i| format!("[x{i}, 0, 1]")).collect();
    let sum: Vec<String> = (1..=8).map(|i| format!("x{i}")).collect();
    let (r, x) = (vars.join(", "), sum.join(" + "));
    let auto = s.run(&format!("n_integrate(exp(-({x})/8), {r})"));
    assert_eq!(auto, s.run(&format!("n_integrate(exp(-({x})/8), {r}, method: lattice)")));
    let kinked = s.run(&format!("n_integrate(abs({x} - 4), {r})"));
    assert_eq!(kinked, s.run(&format!("n_integrate(abs({x} - 4), {r}, method: vegas)")));
    // 2 to 7 variables: cubature
    let a = s.run("n_integrate(sin(x*y*z), [x, 0, 1], [y, 0, 1], [z, 0, 1])");
    assert_eq!(a, s.run("n_integrate(sin(x*y*z), [x, 0, 1], [y, 0, 1], [z, 0, 1], method: cubature)"));
    // the nested one-dimensional rules on request
    let g = s.real("n_integrate(sin(x*y*z), [x, 0, 1], [y, 0, 1], [z, 0, 1], method: gauss_kronrod)");
    close("nested", g, s.real(&a), 1e-9);
}

/// Seeds: the same seed gives the same result, another seed another one within the errors.
#[test]
fn seeds() {
    let mut s = Sys::new();
    for method in ["vegas", "lattice", "sobol", "monte_carlo"] {
        let src = |seed: i64| {
            format!("n_integrate(exp(x*y*z), [x, 0, 1], [y, 0, 1], [z, 0, 1], method: {method}, seed: {seed})")
        };
        let a = s.run(&src(1));
        assert_eq!(a, s.run(&src(1)), "{method}");
        let b = s.run(&src(2));
        assert_ne!(a, b, "{method}");
        let ((va, ea), (vb, eb)) = (around(&a), around(&b));
        assert!((va - vb).abs() <= 6.0 * (ea * ea + eb * eb).sqrt(), "{method}: {a} {b}");
    }
}

/// Goals, budgets, fixed sizes, error estimates for every method, messages.
#[test]
fn options_and_messages() {
    let mut s = Sys::new();
    // a budget too small for the goal: the estimate with a message (a printed warning) for cubature
    let out = s.run("n_integrate(1/sqrt(x*y + 0.001), [x, 0, 1], [y, 0, 1], [z, 0, 1], method: cubature, max_points: 2000, precision_goal: 12)");
    assert!(out.parse::<f64>().is_ok() || out.starts_with("failure(not_converged"), "{out}");
    // error_estimate: around(value, error) from the deterministic methods too
    let (v, e) = around(&s.run("n_integrate(exp(x + y), [x, 0, 1], [y, 0, 1], error_estimate: true)"));
    close("error_estimate 2d", v, (1f64.exp() - 1.0).powi(2), 1e-12);
    assert!(e > 0.0 && e < 1e-6);
    let (v, e) = around(&s.run("n_integrate(exp(x), [x, 0, 1], error_estimate: true)"));
    close("error_estimate 1d", v, 1f64.exp() - 1.0, 1e-15);
    assert!((0.0..1e-8).contains(&e), "{e}");
    // tighter goals give smaller errors
    let (_, e3) = around(&s.run("n_integrate(exp(x*y), [x, 0, 1], [y, 0, 1], method: sobol, precision_goal: 3)"));
    let (_, e6) = around(&s.run("n_integrate(exp(x*y), [x, 0, 1], [y, 0, 1], method: sobol, precision_goal: 6)"));
    assert!(e6 < e3 && e6 < 1.4e-6 && e3 < 1.4e-3, "{e3} {e6}");
    // VEGAS with fixed iterations and points
    let (_, e) =
        around(&s.run("n_integrate(exp(x*y), [x, 0, 1], [y, 0, 1], method: vegas, points: 1000, iterations: 4)"));
    assert!(e > 0.0);
    // monte_carlo with a fixed number of points
    let (_, e) = around(&s.run("n_integrate(x + y, [x, 0, 1], [y, 0, 1], method: monte_carlo, points: 100)"));
    assert!(e > 0.01, "{e}");
}

#[test]
fn failures() {
    let mut s = Sys::new();
    let cases: &[(&str, &str)] = &[
        ("n_integrate(x*y, [x, 0, 1], [y, 0, 1], method: vegas, digits: 20)", "failure(argument, n_integrate, method)"),
        ("n_integrate(x*y, [x, 0, 1], [y, 0, 1], max_points: 0)", "failure(argument, n_integrate, max_points)"),
        ("n_integrate(x*y, [x, 0, 1], [y, 0, 1], points: 3)", "failure(argument, n_integrate, points)"),
        ("n_integrate(x*y, [x, 0, 1], [y, 0, 1], iterations: 0)", "failure(argument, n_integrate, iterations)"),
        ("n_integrate(x*y, [x, 0, 1], [y, 0, 1], seed: 1.5)", "failure(argument, n_integrate, seed)"),
        ("n_integrate(x*y, [x, 0, 1], [y, 0, 1], error_estimate: 2)", "failure(argument, n_integrate, error_estimate)"),
        ("n_integrate(g(x, y), [x, 0, 1], [y, 0, 1], method: vegas)", "failure(not_numeric, g(x, y), [x -> "),
        ("n_integrate(g(x, y), [x, 0, 1], [y, 0, 1])", "failure(not_numeric, g(x, y), [x -> "),
    ];
    for (src, want) in cases {
        let out = s.run(src);
        assert!(out.starts_with(want), "{src}: {out}");
    }
    // more ranges than nested rules take
    let vars: Vec<String> = (1..=7).map(|i| format!("[x{i}, 0, 1]")).collect();
    assert_eq!(
        s.run(&format!("n_integrate(1, {}, method: gauss_kronrod)", vars.join(", "))),
        "failure(argument, n_integrate, 8)"
    );
    // a divergent double integral is a failure for cubature
    let out = s.run("n_integrate(1/(x*y), [x, 0, 1], [y, 0, 1], method: cubature)");
    assert!(out.starts_with("failure(not_converged, n_integrate, "), "{out}");
    // complex values: the automatic choice falls back to the nested rules
    let out = s.run("n_integrate(exp(im*(x + y)), [x, 0, 1], [y, 0, 1])");
    assert!(out.starts_with("complex("), "{out}");
}

/// The same printed results at 1, 2 and 4 threads for every method (parallel blocks, fixed reduction order).
#[test]
fn independent_of_threads() {
    let cases = [
        "n_integrate(exp(-x^2 - y^2 - z^2), [x, 0, 1], [y, 0, 1], [z, 0, 1], precision_goal: 12)",
        "n_integrate(cos(x + 2*y + 3*z + w), [x, 0, 1], [y, 0, 1], [z, 0, 1], [w, 0, 1], method: vegas)",
        "n_integrate(cos(x + 2*y + 3*z + w), [x, 0, 1], [y, 0, 1], [z, 0, 1], [w, 0, 1], method: lattice, precision_goal: 7)",
        "n_integrate(cos(x + 2*y + 3*z + w), [x, 0, 1], [y, 0, 1], [z, 0, 1], [w, 0, 1], method: sobol, precision_goal: 6)",
        "n_integrate(cos(x + 2*y + 3*z + w), [x, 0, 1], [y, 0, 1], [z, 0, 1], [w, 0, 1], method: monte_carlo)",
        "n_integrate(x*y, [x, 0, 1], [y, x^2, sqrt(x)], method: vegas)",
        "n_integrate(abs(x - y), [x, 0, 1], [y, 0, 1], method: monte_carlo, compile: false, points: 3000)",
    ];
    let run = |threads| {
        let mut s = Sys::with_threads(threads, false);
        cases.iter().map(|c| s.run(c)).collect::<Vec<_>>()
    };
    let one = run(1);
    assert_eq!(one, run(2));
    assert_eq!(one, run(4));
}
