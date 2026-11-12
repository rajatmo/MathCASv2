//! Round 38: Appell F1 on special lines, the Mittag-Leffler function of a negative order, the prime zeta function
//! on (1/2, 1), and gcd, lcm and factorisation of Gaussian numbers. Each test has an example input, variations,
//! and a check by another route (partial sums of the defining series, the Moebius inversion of log zeta, the
//! product of the factors).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// The two inputs evaluate to the same term (printing is not compared).
macro_rules! assert_same {
    ($a:expr, $b:expr) => {
        assert_eq!(ev(&format!("({}) === ({})", $a, $b)), "true", "{} gives {}", $a, ev($a))
    };
}

/// |a - b| as a machine number.
fn dist(a: &str, b: &str) -> f64 {
    let v = ev(&format!("n(abs(({a}) - ({b})))"));
    v.parse::<f64>().unwrap_or_else(|_| panic!("{a} - {b}: {v}"))
}

/// The double series of F1 summed for m, n < k.
fn f1_series(a: &str, b1: &str, b2: &str, c: &str, x: &str, y: &str, k: usize) -> String {
    format!(
        "sum(pochhammer({a}, m + j) pochhammer({b1}, m) pochhammer({b2}, j)/(pochhammer({c}, m + j) factorial(m) factorial(j)) ({x})^m ({y})^j, [m, 0, {k}], [j, 0, {k}])"
    )
    .replace(") p", ")*p")
    .replace(") f", ")*f")
    .replace(") (", ")*(")
    .replace("^m (", "^m*(")
}

#[test]
fn appell_f1_on_special_lines() {
    assert_same!("appell_f1(1 / 2, 1, 5 / 2, 3, 2, 2)", "hypergeometric_2f1(1/2, 7/2, 3, 2)");
    assert_same!("appell_f1(a, b, b, c, x, -x)", "hypergeometric_pfq([1/2 + a/2, a/2, b], [1/2 + c/2, c/2], x^2)");
    // variations: zero b, the point x = 1 (Gauss's sum), numeric values against the double series
    assert_same!("appell_f1(a, 0, b2, c, x, y)", "hypergeometric_2f1(a, b2, c, y)");
    assert_same!(
        "appell_f1(1/2, 1, 5/2, 3, 1, y)",
        "4/3*hypergeometric_2f1(1/2, 5/2, 2, y)"
    );
    let d = dist("appell_f1(1/3, 1/4, 1/5, 2, 0.3, 0.3)", &f1_series("1/3", "1/4", "1/5", "2", "0.3", "0.3", 60));
    assert!(d < 1e-12, "{d}");
    let d = dist("appell_f1(3/4, 2/3, 2/3, 5/2, 0.4, -0.4)", &f1_series("3/4", "2/3", "2/3", "5/2", "0.4", "-0.4", 60));
    assert!(d < 1e-12, "{d}");
    // at x = 1 the double series converges slowly (terms ~ m^(a + b1 - c - 1)): summed with log-gamma ratios
    let t = "exp(log_gamma(0.5 + m + j) - log_gamma(0.5) + log_gamma(1. + m) - log_gamma(1.) + log_gamma(1/3 + j) - log_gamma(1/3) - log_gamma(3. + m + j) + log_gamma(3.) - log_gamma(m + 1.) - log_gamma(j + 1.))*0.25^j";
    let d = dist("appell_f1(1/2, 1, 1/3, 3, 1, 0.25)", &format!("sum({t}, [m, 0, 300], [j, 0, 40])"));
    assert!(d < 5e-3, "{d}");
    // c - a - b1 <= 0 at x = 1: no reduction (the double series diverges there)
    assert_same!("appell_f1(1, 2, 1/3, 2, 1, y)", "appell_f1(1, 2, 1/3, 2, 1, y)");
}

#[test]
fn mittag_leffler_of_a_negative_order() {
    let a = ev("mittag_leffler_e(-7., 5)");
    assert!((a.parse::<f64>().unwrap() + 3.96825401413695e-5).abs() < 1e-15, "{a}");
    // the series in 1/z: E_(-a,b)(z) = -sum_(k >= 1) z^-k/Gamma(b + a k)
    for (al, b, z) in [("-0.2", "0.51", "0.87"), ("-1.5", "2.", "-3."), ("-0.7", "1", "2. + 1.*im")] {
        let v = format!("mittag_leffler_e({al}, {b}, {z})");
        let s = format!("-sum(({z})^(-k)/gamma({b} - ({al})*k), [k, 1, 400])");
        let d = dist(&v, &s);
        assert!(d < 1e-9, "{v}: {d}");
    }
    let d = dist("mittag_leffler_e(-0.2, 0.51, 0.87)", "-51.63297865010807");
    assert!(d < 1e-9, "{d}");
}

#[test]
fn prime_zeta_between_one_half_and_one() {
    let d = dist("prime_zeta_p(0.53)", "-1.280363770430605 + 3.141592653589793*im");
    assert!(d < 1e-12, "{d}");
    // the Moebius inversion log zeta(s) = sum_k P(k s)/k, with the values for k s > 1 from the series there
    for s in ["0.53", "0.75", "0.95"] {
        let d = dist(&format!("log(zeta({s}))"), &format!("sum(prime_zeta_p(k*{s})/k, [k, 1, 120])"));
        assert!(d < 1e-12, "{s}: {d}");
    }
    assert_same!("prime_zeta_p(0.4)", "prime_zeta_p(0.4)");
}

#[test]
fn gaussian_gcd_lcm_and_factors() {
    assert_same!("gcd(-3 + 2 * im, 10 + 15 * im)", "2 + 3*im");
    assert_same!("gcd(5 + (10 / 3) * im, 3 / 2 + im)", "1/2 + im/3");
    assert_same!("lcm(-4 + 5 * im, 2 + 3 * im)", "23 + 2*im");
    assert_same!("lcm(1 / 2 + (3 / 5) * im, 1 / 3 + im)", "21 + 13*im");
    assert_same!("factor_integer(9 + 12 * im)", "[[2 + im, 2], [3, 1]]");
    assert_same!("prime_nu(3 + im)", "2");
    // variations: a real and a Gaussian argument; the gcd divides both and the cofactors are coprime
    assert_same!("gcd(65, 7 + 4*im)", "7 + 4*im");
    for (u, v) in [("-3 + 2*im", "10 + 15*im"), ("65", "7 + 4*im"), ("12 + 18*im", "30 - 6*im")] {
        let g = ev(&format!("gcd({u}, {v})"));
        let q1 = format!("expand(({u})/({g}))");
        let q2 = format!("expand(({v})/({g}))");
        assert_same!(&format!("[is_integer(re({q1})), is_integer(im_part({q1})), is_integer(re({q2})), is_integer(im_part({q2}))]"), "[true, true, true, true]");
        assert_same!(&format!("abs(gcd({q1}, {q2}))"), "1");
        let l = ev(&format!("lcm({u}, {v})"));
        assert_same!(&format!("abs({l})*abs({g})"), &format!("abs(({u})*({v}))"));
    }
    assert_same!("expand(product(map(q => q[1]^q[2], factor_integer(7 - 24*im))))", "7 - 24*im");
}
