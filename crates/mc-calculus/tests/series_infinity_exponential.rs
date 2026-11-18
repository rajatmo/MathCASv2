//! Series at infinity with an exponential factor kept outside: exp(g) r with a polynomial g, and the expansions of
//! erfi and Dawson's integral. Checked numerically: the truncated expansion against the function at large x, with
//! an error below the size of the first omitted term. Each test has a time bound.
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
fn exponential_factor_outside() {
    timed("exponential factor", 60, || {
        let mut s = Sys::new();
        assert_eq!(s.run("normal(series(parabolic_cylinder_d(5, x), x, inf, 5)) === (15*x - 10*x^3 + x^5)*exp((-1/4)*x^2)"), "true");
        assert_eq!(s.run("normal(series(exp(-x)*x^2/(1 + x), x, inf, 3)) === (x - 1 + x^-1 - x^-2 + x^-3)*exp(-x)"), "true");
        assert_eq!(s.run("normal(series((-e - k + e*k)*exp(k)/(-1 + e)^2, k, inf, 3)) === (k/(-1 + e) - e/(-1 + e)^2)*exp(k)"), "true");
        // the series of r = x^2/(1 + x) agrees with r at x = 50 to the order
        assert_eq!(s.run("abs(n(normal(series(exp(-x)*x^2/(1 + x), x, inf, 3))*exp(x) - x^2/(1 + x) /. x -> 50)) < 2*50^-4"), "true");
        // an exponential that is not exp of a polynomial stays as it was
        assert_eq!(s.run("series(exp(sin(x))/(1 + x), x, inf, 3)"), "series(exp(sin(x))/(1 + x), x, inf, 3)");
    });
}

#[test]
fn erfi_and_dawson_at_infinity() {
    timed("erfi and Dawson", 60, || {
        let mut s = Sys::new();
        assert_eq!(s.run("normal(series(erfi(x), x, inf, 5)) === -im + (1/(pi^(1/2)*x) + 1/(2*pi^(1/2)*x^3) + 3/(4*pi^(1/2)*x^5))*exp(x^2)"), "true");
        assert_eq!(s.run("normal(series(dawson_f(x), x, inf, 5)) === 1/(4*x^3) + 3/(8*x^5) + 1/(2*x) - (im/2)*exp(-x^2)*pi^(1/2)"), "true");
        // relative errors at x = 10 below the first omitted term (1/2)_6/x^12 ~ 1.0e-10 (real parts: the -i and the
        // exponentially small imaginary term belong to the sector of the expansion)
        for (f, scale) in [("erfi", "exp(-100)*10"), ("dawson_f", "20")] {
            let src = format!("abs(re(n(normal(series({f}(x), x, inf, 11)) /. x -> 10, 30)) - n({f}(10), 30))*{scale} < 2*10^-10");
            assert_eq!(s.run(&src), "true", "{f}");
        }
    });
}
