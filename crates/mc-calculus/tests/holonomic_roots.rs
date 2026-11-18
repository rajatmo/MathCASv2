//! `difference_root` and `differential_root`: the written form of the defining function, and the derivative,
//! antiderivative, Taylor series and values of a differential_root. Each new answer is checked independently: the
//! derivative and the antiderivative against a closed-form solution or against each other, the series against the
//! series of the closed form, the values against closed forms in Bessel functions. Each test has a time bound.
mod common;
use common::Sys;
use std::time::{Duration, Instant};

fn timed(what: &str, limit: u64, f: impl FnOnce()) {
    let t0 = Instant::now();
    f();
    let took = t0.elapsed();
    assert!(took < Duration::from_secs(limit), "{what} took {took:?}");
}

#[test]
fn written_form() {
    timed("written form", 60, || {
        let mut s = Sys::new();
        let cases = [
            ("difference_root((y, n) => [y(n + 2) == y(n + 1) - y(n)/2, y(0) == 0, y(1) == 1])",
             "difference_root((FormalY, FormalN) => [FormalY(FormalN) - 2*FormalY(1 + FormalN) + 2*FormalY(2 + FormalN) == 0, FormalY(0) == 0, FormalY(1) == 1])"),
            ("difference_root((y, n) => [y(n + 1)/(n + 1) - y(n) == 0, y(1) == 1, y(0) == 0])",
             "difference_root((FormalY, FormalN) => [(-1 - FormalN)*FormalY(FormalN) + FormalY(1 + FormalN) == 0, FormalY(0) == 0, FormalY(1) == 1])"),
            ("difference_root((y, n) => [y(n + 1) - y(n) == n^2, y(0) == 0])",
             "difference_root((FormalY, FormalN) => [-FormalN^2 - FormalY(FormalN) + FormalY(1 + FormalN) == 0, FormalY(0) == 0])"),
            // a content free of n is divided out, the sign follows the highest shift
            ("difference_root((y, n) => [3*a*y(n) - 3*a*y(n + 1) == 0, y(0) == 2])",
             "difference_root((FormalY, FormalN) => [-FormalY(FormalN) + FormalY(1 + FormalN) == 0, FormalY(0) == 2])"),
            ("differential_root((y, x) => [y'''(x)/x + y(x) == 0, y(0) == 0, y''(0) == 0, y'(0) == 1])",
             "differential_root((FormalY, FormalX) => [FormalX*FormalY(FormalX) + FormalY'''(FormalX) == 0, FormalY(0) == 0, FormalY'(0) == 1, FormalY''(0) == 0])"),
            ("differential_root((y, x) => [x*y''(x) + y'(x) - y(x) == 0, y(1/2) == 1, y'(1/2) == -3/4])",
             "differential_root((FormalY, FormalX) => [-FormalY(FormalX) + FormalY'(FormalX) + FormalX*FormalY''(FormalX) == 0, FormalY(1/2) == 1, FormalY'(1/2) == -3/4])"),
        ];
        for (src, want) in cases {
            let got = s.run(src);
            assert_eq!(got, want, "{src}");
            // the written form is its own written form
            assert_eq!(s.run(&got), want, "again: {src}");
        }
        // the values do not change: the Fibonacci-like and factorial recurrences
        assert_eq!(s.run("difference_root((y, n) => [y(n + 1) == (n + 1)*y(n), y(1) == 1])([3, 5, 10])"), "[6, 120, 3628800]");
        assert_eq!(s.run("difference_root((y, n) => [y(n + 2) == y(n + 1) + y(n), y(0) == 0, y(1) == 1])(20)"), "6765");
        assert_eq!(s.run("difference_root((y, n) => [y(n + 2) == y(n + 1) + y(n), y(0) == 0, y(1) == 1])(20.0)"), "6765.0");
        // an inexact initial value keeps its precision
        assert_eq!(s.run("precision(differential_root((y, x) => [y'(x) - y(x) == 0, y(0) == 1.~20])[1](u, v)[2][2]) > 19"), "true");
        // not linear: left as it is
        assert_eq!(s.run("difference_root((y, n) => [y(n + 1) == y(n)^2, y(0) == 2])"), "difference_root((y, n) => [y(n + 1) == y(n)^2, y(0) == 2])");
        assert_eq!(s.run("difference_root((y, n) => [y(n + 1)/y(n) == n, y(0) == 2])"), "difference_root((y, n) => [y(n + 1)/y(n) == n, y(0) == 2])");
    });
}

#[test]
fn derivatives_and_antiderivatives() {
    timed("derivatives", 90, || {
        let mut s = Sys::new();
        // y' - y + 4 x^2 = 0, y(0) = 1 has the solution 8 + 8 x + 4 x^2 - 7 e^x
        s.run("f = differential_root((y, x) => [4*x^2 - y(x) + y'(x) == 0, y(0) == 1])");
        assert_eq!(s.run("d(f(x), x)"),
            "differential_root((FormalY, FormalX) => [8*FormalX - FormalY(FormalX) + FormalY'(FormalX) == 0, FormalY(0) == 1])(x)");
        for x in ["0.3", "0.9", "1.7"] {
            let got = s.run(&format!("abs((d(f(x), x) /. x -> {x}) - (8 + 8*{x} - 7*exp({x}))) < 10^-10"));
            assert_eq!(got, "true", "f'({x})");
            let got = s.run(&format!("abs((d(f(x), [x, 2]) /. x -> {x}) - (8 - 7*exp({x}))) < 10^-10"));
            assert_eq!(got, "true", "f''({x})");
        }
        // the derivative of the antiderivative is the function again (the same written form)
        s.run("g = differential_root((y, x) => [y'''(x) + 2*x*y''(x) - y(x) == 0, y(1/2) == 1, y'(1/2) == -3/4, y''(1/2) == 0])");
        assert_eq!(s.run("integrate(g(x), x)"),
            "differential_root((FormalY, FormalX) => [-FormalY(FormalX) - 2*FormalY'(FormalX) + 2*FormalX*FormalY''(FormalX) + FormalY'''(FormalX) == 0, FormalY(1/2) == -11/4, FormalY'(1/2) == 1, FormalY''(1/2) == -3/4])(x)");
        assert_eq!(s.run("d(integrate(g(x), x), x) === g(x)"), "true");
        // a non-exact operator: one more order, G(p) = 0, and again G' = y
        s.run("h = differential_root((y, x) => [y''(x) + x^3*y(x) == 0, y(0) == 1, y'(0) == 0])");
        assert_eq!(s.run("d(integrate(h(x), x), x) === h(x)"), "true");
        assert_eq!(s.run("d(integrate(f(x), x), x) === f(x)"), "true");
        // with a parameter: d^2/dx^2 of x + a y + y' = 0, y(0) = 1
        s.run("k = differential_root((y, x) => [x + a*y(x) + y'(x) == 0, y(0) == 1])");
        assert_eq!(s.run("d(k(x), [x, 2])"),
            "differential_root((FormalY, FormalX) => [a*FormalY(FormalX) + FormalY'(FormalX) == 0, FormalY(0) == -1 + a^2])(x)");
    });
}

#[test]
fn series_and_values() {
    timed("series and values", 90, || {
        let mut s = Sys::new();
        s.run("f = differential_root((y, x) => [y''(x) + y(x) == 0, y(0) == 0, y'(0) == 1])");
        assert_eq!(s.run("normal(series(f(x), x, 0, 9)) - normal(series(sin(x), x, 0, 9))"), "0");
        assert_eq!(s.run("f(pi/2)"), "1");
        assert_eq!(s.run("f(pi/6)"), "1/2");
        s.run("g = differential_root((y, x) => [y'''(x) + 2*x*y''(x) - y(x) == 0, y(1/2) == 1, y'(1/2) == -3/4, y''(1/2) == 0])");
        assert_eq!(s.run("normal(series(g(x), x, 1/2, 3))"), "1 + (-3/4)*(-1/2 + x) + (1/6)*(-1/2 + x)^3");
        // the series solves the equation to its order
        assert_eq!(s.run("p = normal(series(g(x), x, 1/2, 8)); expand(normal(series(d(p, [x, 3]) + 2*x*d(p, [x, 2]) - p, x, 1/2, 5)))"), "0");
        // complex points: x y'' + y' - y = 0 is solved by I_0(2 sqrt x) and K_0(2 sqrt x)
        s.run("h = differential_root((y, x) => [x*y''(x) + y'(x) - y(x) == 0, y(1/2) == 1, y'(1/2) == -3/4])");
        s.run("fn w(x) = c1*bessel_i(0, 2*sqrt(x)) + c2*bessel_k(0, 2*sqrt(x)); cs = solve([w(1/2) == 1, (d(w(x), x) /. x -> 1/2) == -3/4], [c1, c2])[1]");
        for z in ["0.5 + 1.0im", "1.5 - 0.5im", "2.0 + 2.0im"] {
            let got = s.run(&format!("abs(h({z}) - n(rewrite(w({z}), cs), 20)) < 10^-11"));
            assert_eq!(got, "true", "h({z})");
        }
        // complex coefficients: x y'' + i y' - y = 0 is solved by x^((1 - i)/2) Z_(i - 1)(2 sqrt x), Z = I, K
        s.run("h2 = differential_root((y, x) => [x*y''(x) + im*y'(x) - y(x) == 0, y(1/2) == 1 + 4*im, y'(1/2) == -3/4])");
        s.run("fn w2(x) = x^((1 - im)/2)*(c1*bessel_i(im - 1, 2*sqrt(x)) + c2*bessel_k(im - 1, 2*sqrt(x))); cs2 = solve([w2(1/2) == 1 + 4*im, (d(w2(x), x) /. x -> 1/2) == -3/4], [c1, c2])[1]");
        for z in ["0.5 + 1.0im", "1.5 - 0.5im"] {
            let got = s.run(&format!("abs(h2({z}) - n(rewrite(w2({z}), cs2), 20)) < 10^-11"));
            assert_eq!(got, "true", "h2({z})");
        }
    });
}

#[test]
fn hurwitz_sums() {
    timed("Hurwitz sums", 60, || {
        let mut s = Sys::new();
        assert_eq!(s.run("sum((-1)^k*(2*k + 1)^-s, [k, 0, inf])"), "(-zeta(s, 3/4) + zeta(s, 1/4))*4^-s");
        for (t, lo) in [("(-1)^k*(2*k + 1)^-s", "0"), ("(3*k + 2)^-s", "1"), ("cos(k*pi - s*pi/2)*(2*k + 1)^(-1 - s)", "0"), ("(-1)^k*(k + 1/2)^-s", "2")] {
            for v in ["2.5", "3.25"] {
                let src = format!("abs(n(sum({t}, [k, {lo}, inf]) /. s -> {v}) - n_sum({t} /. s -> {v}, [k, {lo}, inf])) < 10^-9");
                assert_eq!(s.run(&src), "true", "{t} at s = {v}");
            }
        }
        // zeta(s, 1/4) and zeta(s, 3/4) at odd s in closed form, against the numbers; the Euler numbers from the sum
        for v in ["3", "5", "9"] {
            for q in ["1/4", "3/4"] {
                let src = format!("abs(n(zeta({v}, {q}), 30) - zeta({v}, n({q}, 30))) < 10^-25");
                assert_eq!(s.run(&src), "true", "zeta({v}, {q})");
            }
        }
        assert_eq!(s.run("f = 2^(m + 2)*factorial(m)/pi^(m + 1)*sum(cos(k*pi - m*pi/2)/(2*k + 1)^(m + 1), [k, 0, inf]); full_simplify(table(rewrite(f, m -> j), j, 8)) === table(euler_e(j), j, 8)"), "true");
        // a factor free of the index comes out
        assert_eq!(s.run("sum(cos(n*pi/2)*k^-n, [k, 1, inf])"), "cos((1/2)*n*pi)*zeta(n)");
    });
}
