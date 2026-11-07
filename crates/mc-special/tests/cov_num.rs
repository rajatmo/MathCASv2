//! Round 14 (package cov-num): functions of truncated series, principal complex values at machine arguments
//! outside the real domain, the formula fallbacks for complex arguments, and the functions added for coverage.
use mc_bench::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn check(cases: &[(&str, &str)]) {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    let mut bad = vec![];
    for (src, want) in cases {
        let got = match e.eval(src, &StopFlag::new()) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        if got != *want {
            bad.push(format!("{src}\n    got  {got}\n    want {want}"));
        }
    }
    assert!(bad.is_empty(), "{} mismatches:\n{}", bad.len(), bad.join("\n"));
}

/// Numbers within a relative tolerance: `src` evaluates to a list of reals or complex numbers, compared to `want`.
fn close(src: &str, want: &[(f64, f64)], tol: f64) {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    let src = format!("map(z => [re(z), im_part(z)], n({src}))");
    let got = e.eval(&src, &StopFlag::new()).map(|v| v.printed).unwrap_or_else(|err| format!("error: {err}"));
    let nums: Vec<f64> = got
        .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == 'e'))
        .filter(|s| !s.is_empty() && s.chars().any(|c| c.is_ascii_digit()))
        .map(|s| s.parse().unwrap_or(f64::NAN))
        .collect();
    assert_eq!(nums.len(), 2 * want.len(), "{src}: {got}");
    for (i, &(re, im)) in want.iter().enumerate() {
        let (a, b) = (nums[2 * i], nums[2 * i + 1]);
        let scale = re.abs().max(im.abs()).max(1e-300);
        assert!((a - re).abs() <= tol * scale && (b - im).abs() <= tol * scale, "{src}: {got}, item {i} want {re} + {im} i");
    }
}

/// G59: elementary and special functions of a series give the series of the composition.
#[test]
fn functions_of_series() {
    check(&[
        ("s = series(sin(x), x, 0, 5); sin(s)", "x - x^3/3 + x^5/10 + O(x^6)"),
        ("sqrt(1 + s)", "1 + x/2 - x^2/8 - x^3/48 + x^4/384 + x^5/3840 + O(x^6)"),
        ("log(1 + s)", "x - x^2/2 + x^3/6 - x^4/12 + x^5/24 + O(x^6)"),
        ("bessel_j(1, s)", "x/2 - 7*x^3/48 + 73*x^5/1920 + O(x^6)"),
        ("fresnel_s(s)", "(1/6)*pi*x^3 + (-1/12)*pi*x^5 + O(x^6)"),
    ]);
}

/// G62 and the principal values: machine arguments outside a function's real domain give complex values, complex
/// arguments without an algorithm of their own go through formulas.
#[test]
fn complex_values_at_machine_arguments() {
    check(&[
        ("legendre_p(2, 1, 2, complex(0.3, 0.2))", "complex(-0.9158144671830879, -0.5306751721174108)"),
        ("legendre_p(2, 1, 3, 2.5) - 7.5*sqrt(5.25) < 1e-12", "true"),
        ("hypergeometric_2f1_regularized(-2, 3, 0, 0.35)", "-0.63"),
        ("hypergeometric_2f1(5, -2, 6, 0.5 + im)", "complex(-0.36904761904761907, -0.9523809523809524)"),
    ]);
    close("[bessel_j(0.5, -0.37), fibonacci(2.5, 0.3), legendre_p(2.5, 0.5, 0.3)]", &[(0.0, 0.47433607456439), (0.71844603470000, 0.0), (-0.64700091520583, 0.0)], 1e-12);
    close("[beta_regularized(0.5 + im, 5, 3), gamma(2, 1.0 + im), gamma_regularized(2.5, 1.0 + im)]", &[(-33.6796875, -16.421875), (0.70709209634594, -0.42035364095981), (0.90546717518752, -0.33654245335777)], 1e-10);
    close("matrix_function(z => bessel_j(0.5, z), [[1, 2], [3, 4]])[1]", &[(-0.06496896887316, 0.36203314552940), (-0.09468753639872, -0.16560377467144)], 1e-10);
}

/// max and min of intervals, centered intervals, around values and quantities (the io-num items of the gap queue).
#[test]
fn extremes_of_uncertain_values() {
    check(&[
        ("max(interval([1, 3]), interval([-3, 5]))", "interval([1, 5])"),
        ("min(centered_interval(2, 1), centered_interval(1, 4))", "centered_interval(0, 3)"),
        ("min([1, around(-1.10001, 0.01)])", "around(-1.10001, 0.01)"),
        ("max([1, around(-1.10001, 0.01)])", "1"),
        ("max([quantity(3, \"Meters\"), quantity(200, \"Centimeters\")])", "quantity(3, \"Meters\")"),
        ("min(interval([1, 3]), 2)", "interval([1, 2])"),
        ("max(1, 5, [2, 9])", "9"),
    ]);
}
