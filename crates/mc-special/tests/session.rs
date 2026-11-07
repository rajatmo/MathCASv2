//! The functions through source text: exact values, symbolic arguments, machine and ball values, `n` inside
//! terms, comparisons, lists, poles and failures, derivatives, threads and interrupts.
mod common;
use common::{check, number, Sys};
use std::time::Duration;

#[test]
fn exact_values() {
    check(&[
        ("gamma(5)", "24"),
        ("gamma(1)", "1"),
        ("gamma(1/2)", "pi^(1/2)"),
        ("gamma(7/2)", "(15/8)*pi^(1/2)"),
        ("gamma(-3/2)", "(4/3)*pi^(1/2)"),
        ("gamma(-1/2)", "-2*pi^(1/2)"),
        ("gamma(1/3)", "gamma(1/3)"),
        ("log_gamma(1)", "0"),
        ("log_gamma(2)", "0"),
        ("log_gamma(10)", "log(362880)"),
        ("log_gamma(1/2)", "(1/2)*log(pi)"),
        ("digamma(1)", "-euler_gamma"),
        ("digamma(4)", "11/6 - euler_gamma"),
        ("digamma(1/2)", "-2*log(2) - euler_gamma"),
        ("digamma(5/2)", "8/3 + -2*log(2) - euler_gamma"),
        ("digamma(-3/2)", "8/3 + -2*log(2) - euler_gamma"),
        ("polygamma(0, 3)", "3/2 - euler_gamma"),
        ("polygamma(1, 1)", "(1/6)*pi^2"),
        ("polygamma(1, 1/2)", "(1/2)*pi^2"),
        ("polygamma(2, 1)", "-2*zeta(3)"),
        ("zeta(0)", "-1/2"),
        ("zeta(2)", "(1/6)*pi^2"),
        ("zeta(4)", "(1/90)*pi^4"),
        ("zeta(12)", "(691/638512875)*pi^12"),
        ("zeta(-1)", "-1/12"),
        ("zeta(-2)", "0"),
        ("zeta(-3)", "1/120"),
        ("zeta(-13)", "-1/12"),
        ("zeta(3)", "zeta(3)"),
        ("zeta(1/2)", "zeta(1/2)"),
        ("sum(1/k^2, [k, 1, inf])", "(1/6)*pi^2"),
        ("beta(2, 3)", "1/12"),
        ("beta(1/2, 1/2)", "pi"),
        ("beta(1/2, -1/2)", "0"),
        ("bessel_k(1/2, x)", "(1/2)*2^(1/2)*exp(-x)*pi^(1/2)*x^(-1/2)"),
        ("bessel_k(-1/2, x) == bessel_k(1/2, x)", "true"),
        ("bessel_k(3/2, x)", "(1/2)*(1 + x^-1)*2^(1/2)*exp(-x)*pi^(1/2)*x^(-1/2)"),
        ("bessel_k(1/2, 2)", "(1/2)*exp(-2)*pi^(1/2)"),
        ("bessel_k(1, 2)", "bessel_k(1, 2)"),
        ("bessel_i(0, 0)", "1"),
        ("bessel_i(2, 0)", "0"),
        ("bessel_i(1/2, x)", "2^(1/2)*(pi*x)^(-1/2)*sinh(x)"),
        ("gamma([1, 2, 3, 4])", "[1, 1, 2, 6]"),
        ("bessel_k([1/2, 3/2], 1) == [bessel_k(1/2, 1), bessel_k(3/2, 1)]", "true"),
    ]);
}

#[test]
fn symbolic_and_failures() {
    check(&[
        ("gamma(x)", "gamma(x)"),
        ("zeta(s + 1)", "zeta(1 + s)"),
        ("polygamma(0, x)", "digamma(x)"),
        ("polygamma(n, x)", "polygamma(n, x)"),
        ("bessel_k(nu, x)", "bessel_k(nu, x)"),
        ("beta(a, b)", "beta(a, b)"),
        ("gamma(0)", "failure(pole, gamma(0))"),
        ("gamma(-3)", "failure(pole, gamma(-3))"),
        ("log_gamma(-2)", "inf"),
        ("digamma(0)", "failure(pole, digamma(0))"),
        ("polygamma(1, -1)", "failure(pole, polygamma(1, -1))"),
        ("zeta(1)", "failure(pole, zeta(1))"),
        ("bessel_k(1, 0)", "failure(pole, bessel_k(1, 0))"),
        ("bessel_i(-1/3, 0)", "failure(pole, bessel_i(-1/3, 0))"),
        ("beta(-1, 2)", "failure(pole, beta(-1, 2))"),
        // outside the real domain or complex: unevaluated
        ("bessel_k(1, -2.0)", "bessel_k(1, -2.0)"),
        ("bessel_k(1, -2)", "bessel_k(1, -2)"),
        // outside the real domain: the principal complex value (round 14)
        ("bessel_i(1/3, -2.0)", "complex(1.0793912906864405, 1.8695605567162608)"),
        ("polygamma(1/2, 2.0)", "polygamma(1/2, 2.0)"),
        ("gamma(1 + 2im)", "gamma(complex(1, 2))"),
        ("gamma(1, 2)", "exp(-2)"),
        ("gamma(1, 2, 3)", "-exp(-3) + exp(-2)"),
        ("bessel_k([1, 2], [3])", "failure(shape_mismatch)"),
        // machine poles follow IEEE / the C library
        ("gamma(-2.0)", "machine_real(\"nan\")"),
        ("gamma(0.0)", "machine_real(\"inf\")"),
        ("gamma(200.0)", "machine_real(\"inf\")"),
        ("bessel_k(1, 800.0)", "0.0"),
        ("bessel_k(1, 0.0)", "machine_real(\"inf\")"),
        ("zeta(1.0)", "machine_real(\"nan\")"),
    ]);
}

#[test]
fn machine_and_balls() {
    let mut s = Sys::new();
    assert_eq!(s.run("gamma(0.5)"), "1.772453850905516");
    assert_eq!(s.run("zeta(3.0)"), "1.2020569031595942");
    // the plain double path (G94) is within a few ulps of 0.13986588181652243
    assert_eq!(s.run("bessel_k(1, 2.0)"), "0.13986588181652254");
    assert_eq!(s.run("bessel_i(0, 1.0)"), "1.2660658777520084");
    assert_eq!(s.run("log_gamma(-0.5)"), "complex(1.2655121234846454, -3.141592653589793)");
    assert_eq!(s.run("polygamma(1, 1.0)"), "1.6449340668482264");
    assert_eq!(s.run("beta(0.5, 0.5)"), "3.141592653589793");
    // n of exact calls, machine and digits (published values; the printed digits must agree)
    assert_eq!(s.run("n(gamma(1/3))"), "2.678938534707748");
    let refs = [
        ("gamma(1/3)", 50, "2.6789385347077476336556929409746776441286893779573011009504283"),
        ("gamma(1/4)", 40, "3.6256099082219083119306851558676720029951676828800654674333800"),
        ("zeta(3)", 50, "1.2020569031595942853997381615114499907649862923404988817922716"),
        ("zeta(5)", 40, "1.0369277551433699263313654864570341680570809195019128119741927"),
        ("bessel_k(0, 1)", 40, "0.42102443824070833333562737921260903613621974822666989"),
        ("bessel_k(1, 1)", 40, "0.60190723019723457473754000153561733926158688996810"),
        ("bessel_i(0, 1)", 40, "1.2660658777520083355982446252147175376076703113549622"),
        ("digamma(1/3)", 30, "-3.1320337800208063229964190742873"),
    ];
    for (e, d, want) in refs {
        let out = s.run(&format!("n({e}, {d})"));
        let k = d + 1;
        assert_eq!(&out[..k], &want[..k], "{e}: {out}");
    }
    // nested and mixed
    assert_eq!(s.run("n(bessel_k(1, 2) + 1)"), "1.1398658818165226");
    let g = s.run("n(gamma(1/3)*zeta(3), 30)");
    assert!(g.starts_with("3.2202365587856964116942454571"), "{g}");
    assert_eq!(s.run("n(log_gamma(-1/2), 20)"), "complex(1.265512123484645396488946, -3.14159265358979323846264)");
    assert_eq!(s.run("n([zeta(3), x + gamma(1/3)])"), "[1.2020569031595942, 2.678938534707748 + x]");
    // comparisons refine the special functions
    assert_eq!(s.run("bessel_k(1, 2) < 1/7"), "true");
    assert_eq!(s.run("gamma(1/3) > 2"), "true");
    assert_eq!(s.run("zeta(3) > 6/5"), "true");
    // ball arguments
    assert_eq!(s.run("gamma(1.0~30/3)"), "2.6789385347077476336556929410");
    assert_eq!(s.run("zeta(3.0~40)"), "1.202056903159594285399738161511449990765");
    assert_eq!(s.run("bessel_k(2, 1.0~30)"), "1.62483889863517748281070738228");
    // a wide ball argument: the monotone hull, rigorous but with fewer digits
    let w = s.run("bessel_k(1, 2.00~6)");
    assert!(w.starts_with("0.1") || w.starts_with("ball("), "{w}");
}

#[test]
fn identities() {
    let mut s = Sys::new();
    // an identity evaluates to a ball around zero: n reports that 50 digits are not reached, with a tiny ball
    let zero = |s: &mut Sys, e: &str| {
        let out = s.run(&format!("n({e}, 50)"));
        assert!(tiny(&out), "{e}: {out}");
    };
    for x in ["7/3", "1/5", "-13/4", "41/2 + 1/7"] {
        zero(&mut s, &format!("gamma({x} + 1) - ({x})*gamma({x})"));
        zero(&mut s, &format!("gamma({x})*gamma(1 - ({x})) - pi/sin(pi*({x}))"));
        zero(&mut s, &format!("digamma({x} + 1) - digamma({x}) - 1/({x})"));
        zero(&mut s, &format!("polygamma(2, {x} + 1) - polygamma(2, {x}) - 2/({x})^3"));
    }
    for (nu, x) in [("1/3", "2"), ("2", "7/2"), ("5/2", "1/10"), ("10", "30"), ("0", "12")] {
        // K_{nu+1} = K_{nu-1} + 2 nu/x K_nu, and the Wronskian I_nu K_{nu+1} + I_{nu+1} K_nu = 1/x
        zero(&mut s, &format!("(bessel_k({nu} + 1, {x}) - bessel_k({nu} - 1, {x}) - 2*({nu})/({x})*bessel_k({nu}, {x}))/bessel_k({nu}, {x})"));
        zero(
            &mut s,
            &format!(
                "({x})*(bessel_i({nu}, {x})*bessel_k({nu} + 1, {x}) + bessel_i({nu} + 1, {x})*bessel_k({nu}, {x})) - 1"
            ),
        );
    }
    zero(&mut s, "beta(7/3, 5/4) - gamma(7/3)*gamma(5/4)/gamma(7/3 + 5/4)");
    zero(&mut s, "zeta(-3/2) - 2*(2*pi)^(-5/2)*sin(-3*pi/4)*gamma(5/2)*zeta(5/2)");
    zero(&mut s, "n(zeta(2), 60) - pi^2/6");
    zero(&mut s, "log_gamma(19/3) - log(gamma(19/3))");
}

/// Is a printed `n(e, 50)` result a value below 10^-40 (a number, or `ball(m, r)` in a precision failure)?
fn tiny(out: &str) -> bool {
    let inner = out.trim_start_matches("failure(precision_not_reached, ").trim_end_matches(')');
    let parts: Vec<f64> = inner.trim_start_matches("ball(").split(", ").filter_map(|p| p.parse::<f64>().ok()).collect();
    !parts.is_empty() && parts.iter().map(|v| v.abs()).sum::<f64>() < 1e-40
}

#[test]
fn derivatives() {
    check(&[
        ("d(gamma(x), x)", "digamma(x)*gamma(x)"),
        ("d(log_gamma(x), x)", "digamma(x)"),
        ("d(digamma(x), x)", "polygamma(1, x)"),
        ("d(polygamma(2, x), x)", "polygamma(3, x)"),
        ("d(bessel_k(1, x), x)", "(-1/2)*(bessel_k(0, x) + bessel_k(2, x))"),
        ("d(bessel_i(nu, x), x)", "(1/2)*(bessel_i(-1 + nu, x) + bessel_i(1 + nu, x))"),
        ("d(beta(x, 2), x)", "(digamma(x) - digamma(2 + x))*beta(x, 2)"),
        ("d(zeta(s), s)", "zeta'(s)"),
        ("gamma'(x)", "digamma(x)*gamma(x)"),
        ("d(bessel_k(x, 2), x)", "derivative(1, 0)(bessel_k)(x, 2)"),
    ]);
    // the order derivative stays an operator also at higher orders (no endless resolution)
    let mut s = Sys::new();
    let r = s.run("d(d(bessel_k(x, x), x), x)");
    assert!(r.contains("derivative(2, 0)(bessel_k)(x, x)") && r.contains("derivative(1, 1)(bessel_k)(x, x)"), "{r}");
    // numerically at points: the derivative against a central difference (h = 10^-15, 45 digits)
    for (f, x) in [
        ("gamma(x)", "7/3"),
        ("gamma(x)", "-5/4"),
        ("log_gamma(x)", "11/3"),
        ("digamma(x)", "3/7"),
        ("polygamma(1, x)", "5/2"),
        ("bessel_k(1/3, x)", "9/4"),
        ("bessel_k(2, x)", "1/3"),
        ("bessel_i(5/2, x)", "3"),
        ("beta(x, 3/2)", "4/5"),
        ("bessel_k(1, x^2)*gamma(x)", "6/5"),
    ] {
        let e = format!(
            "abs(n(rewrite(d({f}, x), [x -> {x}]) - (rewrite({f}, [x -> {x} + 10^-15]) - rewrite({f}, [x -> {x} - 10^-15]))*10^15/2, 45)) < 10^-20"
        );
        assert_eq!(s.run(&e), "true", "{f} at {x}");
    }
}

#[test]
fn numeric_solvers_and_compiled_code() {
    let mut s = Sys::new();
    // n_integrate, find_root and compiled functions reach the machine path (the compiler declines these calls
    // and the interpreter evaluates them)
    let i = number(&s.run("n_integrate(bessel_k(1, x), [x, 1, 2])"));
    // the integral is K_0(1) - K_0(2)
    assert!((i - (0.421_024_438_240_708_3 - 0.113_893_872_749_533_4)).abs() < 1e-12, "{i}");
    let r = number(&s.run("rewrite(x, find_root(digamma(x), [x, 1.5]))"));
    assert!((r - 1.461_632_144_968_362_3).abs() < 1e-12, "{r}");
    s.run("@compiled fn g(x: Real) = gamma(x)*zeta(x)");
    let v = number(&s.run("g(2.5)"));
    assert!((v - 1.329_340_388_179_137 * 1.341_487_257_250_917).abs() < 1e-13, "{v}");
}

#[test]
fn threads_agree() {
    let inputs = [
        "n(gamma(1/3), 60)",
        "n(zeta(3)*bessel_k(1/3, 5), 40)",
        "n(digamma(-7/2) + polygamma(3, 1/7), 30)",
        "n([bessel_k(30, 700), bessel_i(10, 50), log_gamma(-21/2)], 25)",
        "gamma([0.3, 4.5, -2.5])",
        "n(beta(1/3, 1/5), 50)",
    ];
    let mut one = Sys::with_threads(1);
    let mut four = Sys::with_threads(4);
    for src in inputs {
        let a = one.run(src);
        assert_eq!(a, four.run(src), "{src}");
        assert!(!a.contains("failure"), "{src}: {a}");
    }
}

#[test]
fn interrupts() {
    let mut s = Sys::new();
    for src in ["n(gamma(1/3), 200000)", "n(zeta(7/3), 200000)", "n(bessel_k(1/3, 40), 200000)"] {
        let (r, took) = s.interrupted(src, Duration::from_millis(100));
        let shown = format!("{r:?}");
        assert!(
            r.is_err() || shown.contains("interrupted") || shown.contains("precision_not_reached"),
            "{src}: {shown}"
        );
        assert!(took < Duration::from_secs(10), "{src}: {took:?}");
    }
    // afterwards the session still computes
    assert!(s.run("n(gamma(1/3), 20)").starts_with("2.6789385347077476336"));
    assert!((number(&s.run("n(zeta(3))")) - 1.2020569031595942).abs() < 1e-15);
}
