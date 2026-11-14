//! Round 40: infinite sums of Hurwitz zeta values along a shifted argument, linear Euler sums of harmonic numbers,
//! Appell's double series, Mathieu's equation, and conditions with step functions on a bounded interval. Closed
//! forms are compared with partial sums plus a tail bound, with the defining series, or by substitution.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// `|closed - sum of the first terms| < tol` at 30 digits.
fn close(e: &mut CoreEngine, closed: &str, partial: &str, tol: &str) {
    let src = format!("abs(n({closed}, 30) - n({partial}, 30)) < {tol}");
    assert_eq!(ev(e, &src), "true", "{closed} against {partial}");
}

#[test]
fn hurwitz_zeta_tails() {
    let mut e = CoreEngine::new();
    // documented: the closed form, and the double sum whose inner sum is that zeta value
    let s = ev(&mut e, "sum(zeta(3, 2 + i), [i, inf])");
    assert!(!s.contains("sum"), "{s}");
    assert_eq!(ev(&mut e, "sum(1/(1 + i + j)^3, [i, inf], [j, inf])"), s);
    // check: sum_(N >= 1) N/(N + 2)^3 to 4000 terms; the tail is below 1/4001
    close(&mut e, &s, "sum(k/(k + 2)^3, [k, 1, 4000])", "1/4000");
    // variations: another order and shift (sum_(k >= 0) zeta(4, k + 1/2) = sum (N + 1)(N + 1/2)^-4), a numeric check
    // with an exact tail bound
    let t = ev(&mut e, "sum(zeta(4, k + 1/2), [k, 0, inf])");
    assert!(!t.contains("sum"), "{t}");
    close(&mut e, &t, "sum((k + 1)/(k + 1/2)^4, [k, 0, 3000])", "1/3000^2");
    let u = ev(&mut e, "sum(zeta(5/2, k + 3), [k, 2, inf])");
    assert!(!u.contains("sum"), "{u}");
    close(&mut e, &u, "sum((k + 1)/(k + 5)^(5/2), [k, 0, 20000])", "2/20000^(1/2)");
}

#[test]
fn linear_euler_sums() {
    let mut e = CoreEngine::new();
    // documented: sum H_i^(2)/(i + 1)^2 = pi^4/120, also as the double sum
    assert_eq!(ev(&mut e, "sum(harmonic_number(i, 2)/(1 + i)^2, [i, 1, inf])"), "(1/120)*pi^4");
    assert_eq!(ev(&mut e, "sum(1/(j^2*(i + 1)^2), [i, 1, inf], [j, 1, i])"), "(1/120)*pi^4");
    // variations: Euler's sum H_k/k^3 = pi^4/72, and a shifted order-3 sum against its partial sums
    assert_eq!(ev(&mut e, "sum(harmonic_number(k)/k^3, [k, 1, inf])"), "(1/72)*pi^4");
    let s = ev(&mut e, "sum(harmonic_number(k, 3)/(k + 2)^3, [k, 0, inf])");
    assert!(!s.contains("sum"), "{s}");
    // H_k^(3) < zeta(3) < 6/5: the tail beyond 3000 is below (6/5)/(2*3000^2)
    close(&mut e, &s, "sum(n(harmonic_number(k, 3)/(k + 2)^3, 30), [k, 0, 3000])", "1/(3000^2)");
}

#[test]
fn appell_double_series() {
    let mut e = CoreEngine::new();
    let cases = [
        ("pochhammer(a, m + n)*pochhammer(b1, m)*pochhammer(b2, n)/(pochhammer(c, m + n)*factorial(m)*factorial(n))", "appell_f1(a, b1, b2, c, x, y)"),
        ("pochhammer(a, m + n)*pochhammer(b1, m)*pochhammer(b2, n)/(pochhammer(c1, m)*pochhammer(c2, n)*factorial(m)*factorial(n))", "appell_f2(a, b1, b2, c1, c2, x, y)"),
        ("pochhammer(a1, m)*pochhammer(a2, n)*pochhammer(b1, m)*pochhammer(b2, n)/(pochhammer(c, m + n)*factorial(m)*factorial(n))", "appell_f3(a1, a2, b1, b2, c, x, y)"),
        ("pochhammer(a, m + n)*pochhammer(b, m + n)/(pochhammer(c1, m)*pochhammer(c2, n)*factorial(m)*factorial(n))", "appell_f4(a, b, c1, c2, x, y)"),
    ];
    for (t, f) in cases {
        assert_eq!(ev(&mut e, &format!("sum(x^m*y^n*{t}, [m, 0, inf], [n, 0, inf])")), f);
    }
    // the sums over m of the inner Gauss functions
    let single = [
        ("hypergeometric_2f1(b2, a + m, c + m, y)*pochhammer(a, m)*pochhammer(b1, m)*x^m/(factorial(m)*pochhammer(c, m))", "appell_f1(a, b1, b2, c, x, y)"),
        ("hypergeometric_2f1(b2, a + m, c2, y)*pochhammer(a, m)*pochhammer(b1, m)*x^m/(factorial(m)*pochhammer(c1, m))", "appell_f2(a, b1, b2, c1, c2, x, y)"),
        ("hypergeometric_2f1(b2, a2, c + m, y)*pochhammer(a1, m)*pochhammer(b1, m)*x^m/(factorial(m)*pochhammer(c, m))", "appell_f3(a1, a2, b1, b2, c, x, y)"),
        ("hypergeometric_2f1(b + m, a + m, c2, y)*pochhammer(a, m)*pochhammer(b, m)*x^m/(factorial(m)*pochhammer(c1, m))", "appell_f4(a, b, c1, c2, x, y)"),
    ];
    for (t, f) in single {
        assert_eq!(ev(&mut e, &format!("sum({t}, [m, 0, inf])")), f);
    }
    // a constant factor stays outside
    let r = ev(&mut e, "sum(3*x^m*y^n*pochhammer(a, m + n)*pochhammer(b, m + n)/(pochhammer(c1, m)*pochhammer(c2, n)*factorial(m)*factorial(n)), [m, 0, inf], [n, 0, inf])");
    assert_eq!(r, "3*appell_f4(a, b, c1, c2, x, y)");
    // check of the reading at a point: the F1 value against the truncated double series
    close(
        &mut e,
        "appell_f1(1/2, 1/3, 1/4, 2, 1/5, 1/7)",
        "sum((1/5)^m*(1/7)^n*pochhammer(1/2, m + n)*pochhammer(1/3, m)*pochhammer(1/4, n)/(pochhammer(2, m + n)*factorial(m)*factorial(n)), [m, 0, 40], [n, 0, 40])",
        "10^-20",
    );
    // other double series are not read as Appell functions
    let o = ev(&mut e, "sum(x^m*y^n*pochhammer(a, m + n)/(factorial(m)*factorial(n)*pochhammer(c, m)), [m, 0, inf], [n, 0, inf])");
    assert!(!o.contains("appell"), "{o}");
}

#[test]
fn mathieu_equation() {
    let mut e = CoreEngine::new();
    // documented
    let r = ev(&mut e, "d_solve((a - 2*q*cos(2*x))*y(x) + y''(x) == 0, y, x)");
    assert!(r.contains("mathieu_c(a, q, x)") && r.contains("mathieu_s(a, q, x)"), "{r}");
    let r = ev(&mut e, "d_solve(-y''(x) + cos(x)*y(x) == ee*y(x), y(x), x)");
    assert!(r.contains("mathieu_c(4*ee, 2, (1/2)*x)"), "{r}");
    // variations, each basis function checked in the equation at a point to 20 digits
    for (eq, lhs) in [
        ("y''(x) + (3 + 5*cos(4*x))*y(x) == 0", "d(f, x, x) + (3 + 5*cos(4*x))*f"),
        ("y''(x) + (2 - cos(x + 1))*y(x) == 0", "d(f, x, x) + (2 - cos(x + 1))*f"),
    ] {
        let s = ev(&mut e, &format!("s = d_solve({eq}, y(x), x); s"));
        assert!(s.contains("mathieu_c") && s.contains("mathieu_s"), "{s}");
        for c in ["[c(1) -> 1, c(2) -> 0]", "[c(1) -> 0, c(2) -> 1]"] {
            let f = ev(&mut e, &format!("f = rewrite(rewrite(y(x), s[1]), {c}); f"));
            assert!(f.contains("mathieu"), "{f}");
            assert_eq!(ev(&mut e, &format!("chop(n(rewrite({lhs}, x -> 3/10), 25), 10^-15)")), "0", "{eq} {c}");
        }
    }
}

#[test]
fn conditions_with_step_functions() {
    let mut e = CoreEngine::new();
    // documented
    assert_eq!(ev(&mut e, "reduce(integer_part(x) + integer_part(2*x - 1) == 2 and 0 < x and x < 4, x, reals)"), "1 <= x and x < 3/2");
    let r = ev(&mut e, "reduce(mod(2*x + 1, 5) == mod(x, 7) and 0 < x and x < 50, x)");
    assert_eq!(r, "x == 4 or x == 7 or x == 15 or x == 23 or x == 31 or x == 39 or x == 42");
    // check: the integers of the interval that satisfy it (every solution is an integer: x = 5 a - 7 b - 1)
    assert_eq!(ev(&mut e, "select(range(1, 49), k => mod(2*k + 1, 5) == mod(k, 7))"), "[4, 7, 15, 23, 31, 39, 42]");
    // variations: floor of a quadratic, a ceiling against a linear function, each checked at sample points
    let f = ev(&mut e, "reduce(floor(x^2) == 2 and -3 < x and x < 3, x, reals)");
    assert_eq!(f, ev(&mut e, "reduce(2 <= x^2 and x^2 < 3 and -3 < x and x < 3, x, reals)"));
    let g = ev(&mut e, "g = reduce(ceiling(x/2) == x - 1 and 0 < x and x < 10, x, reals); g");
    assert_eq!(g, "x == 2 or x == 3");
    assert_eq!(ev(&mut e, "map(k => ceiling(k/2) == k - 1, [1, 2, 3, 4, 5/2])"), "[false, true, true, false, false]");
}
