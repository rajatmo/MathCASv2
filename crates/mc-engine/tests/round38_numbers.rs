//! Round 38: divisor sums over a symbolic n, Dirichlet series and Euler products of arithmetic functions,
//! binomials of rational indices differing by an integer, factorials of half-integers. Each test has an example
//! input, variations, and a check by another route (the sum over the divisors of numbers, partial sums of the
//! series, the gamma function).
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

fn num(src: &str) -> f64 {
    let v = ev(&format!("n({src})"));
    v.parse::<f64>().unwrap_or_else(|_| panic!("{src}: {v}"))
}

#[test]
fn divisor_sums_over_a_symbolic_n() {
    assert_same!("divisor_sum(x, mangoldt_lambda)", "log(x)");
    assert_same!("divisor_sum(n_u, moebius_mu)", "kronecker_delta(1 - n_u)");
    assert_same!("divisor_sum(n_u, &(moebius_mu(n_u / &1) * log(&1)))", "mangoldt_lambda(n_u)");
    assert_same!("divisor_sum(n_u, &(1))", "divisor_sigma(0, n_u)");
    assert_same!("divisor_sum(n_u, &(&1^k))", "divisor_sigma(k, n_u)");
    // variations: linear combinations, phi, mu^2; an unknown summand declines
    assert_same!("divisor_sum(n, &(3*&1^2 + moebius_mu(&1)/&1))", "3*divisor_sigma(2, n) + euler_phi(n)/n");
    assert_same!("divisor_sum(n, euler_phi)", "n");
    assert_same!("divisor_sum(n, &(moebius_mu(&1)^2))", "2^prime_nu(n)");
    assert_same!("divisor_sum(n, &(sin(&1)))", "divisor_sum(n, &(sin(&1)))");
    assert_same!("generating_function(divisor_sum(n_u, (m => euler_phi(m))), n_u, x)", "x/(-1 + x)^2");
    // the closed forms agree with the sums over the divisors of numbers
    for m in [1, 12, 30, 97, 360, 1001] {
        for (f, g) in [
            ("&(moebius_mu(n / &1) * log(&1))", "mangoldt_lambda(n)"),
            ("&(3*&1^2 + moebius_mu(&1)/&1)", "3*divisor_sigma(2, n) + euler_phi(n)/n"),
            ("&(moebius_mu(&1)^2)", "2^prime_nu(n)"),
            ("mangoldt_lambda", "log(n)"),
            ("moebius_mu", "kronecker_delta(1 - n)"),
        ] {
            let a = format!("rewrite(divisor_sum(n, {f}), n -> {m})");
            let b = format!("divisor_sum({m}, rewrite({f}, n -> {m}))");
            let d = num(&format!("({a}) - ({b})"));
            assert!(d.abs() < 1e-12, "{f} at {m}: {a} vs {b}");
            let c = num(&format!("({a}) - (rewrite({g}, n -> {m}))"));
            assert!(c.abs() < 1e-12, "{g} at {m}");
        }
    }
}

#[test]
fn dirichlet_series_and_euler_products() {
    assert_same!("sum(moebius_mu(n_u) / n_u^2, [n_u, 1, inf])", "6/pi^2");
    assert_same!("sum(moebius_mu(n_u) / n_u^s, [n_u, inf])", "1/zeta(s)");
    assert_same!("sum(divisor_sigma(k, n_u) / n_u^s, [n_u, inf])", "zeta(s)*zeta(s - k)");
    assert_same!("sum(liouville_lambda(k) / k^s, [k, 1, inf])", "zeta(2*s)/zeta(s)");
    assert_same!("sum(2^prime_nu(k) / k^s, [k, 1, inf])", "zeta(s)^2/zeta(2*s)");
    assert_same!("sum(1 / prime(n_u)^s, [n_u, inf])", "prime_zeta_p(s)");
    assert_same!("product(1 + 1 / prime(n_u)^s, [n_u, inf])", "zeta(s)/zeta(2*s)");
    // variations: numeric exponents, phi, the boundary of convergence declines
    assert_same!("sum(euler_phi(k) / k^3, [k, 1, inf])", "pi^2/(6*zeta(3))");
    assert_same!("sum(euler_phi(k) / k^2, [k, 1, inf])", "sum(euler_phi(k) / k^2, [k, 1, inf])");
    assert_same!("sum(moebius_mu(k) / k, [k, 1, inf])", "sum(moebius_mu(k) / k, [k, 1, inf])");
    assert_same!("product(1/(1 - prime(n_u)^-2), [n_u, inf])", "pi^2/6");
    // partial sums and products approach the values
    let checks = [
        ("sum(moebius_mu(k) / k^3, [k, 1, 3000])", "1/zeta(3)", 1e-6),
        ("sum(liouville_lambda(k) / k^3, [k, 1, 3000])", "zeta(6)/zeta(3)", 1e-6),
        ("sum(divisor_sigma(1, k) / k^4, [k, 1, 3000])", "zeta(4)*zeta(3)", 1e-6),
        ("sum(euler_phi(k) / k^4, [k, 1, 3000])", "zeta(3)/zeta(4)", 1e-6),
        ("sum(mangoldt_lambda(k) / k^3, [k, 1, 3000])", "-rewrite(d(zeta(t), t), t -> 3)/zeta(3)", 1e-5),
        ("product(1 + prime(k)^-3, [k, 1, 400])", "zeta(3)/zeta(6)", 1e-6),
    ];
    for (partial, closed, tol) in checks {
        let a = num(partial);
        let b = num(closed);
        assert!((a - b).abs() < tol, "{partial}: {a} vs {b}");
    }
    assert_same!("sum(moebius_mu(k) / k^3, [k, 1, inf])", "1/zeta(3)");
}

#[test]
fn binomials_of_rational_indices_and_half_integer_factorials() {
    assert_same!("binomial(9 / 2, 7 / 2)", "9/2");
    assert_same!("binomial(1 + im, 5)", "-1/12 - im/12");
    assert_same!("factorial(1 / 2)", "pi^(1/2)/2");
    // variations: a negative difference is a zero of 1/Gamma, another complex index, a negative half-integer
    assert_same!("binomial(1/2, 3/2)", "0");
    assert_same!("binomial(1/3, 5/6)", "binomial(1/3, 5/6)");
    assert_same!("binomial(2 - 3*im/2, 3)", "(2 - 3*im/2)*(1 - 3*im/2)*(-3*im/2)/6");
    assert_same!("factorial(-7/2)", "(-8/15)*pi^(1/2)");
    // by the gamma function
    for (a, b) in [("9/2", "7/2"), ("-5/3", "-11/3"), ("7/4", "-1/4")] {
        let v = num(&format!("binomial({a}, {b})"));
        let g = num(&format!("gamma({a} + 1.)/(gamma({b} + 1.)*gamma({a} - {b} + 1.))"));
        assert!((v - g).abs() < 1e-12 * (1.0 + g.abs()), "{a} {b}");
    }
    assert!((num("factorial(5/2)") - num("gamma(3.5)")).abs() < 1e-13);
}
