//! `casoratian(eq, y, n)`: the Casoratian of the fundamental set of a linear homogeneous difference equation with
//! the standard initial values (C(0) = 1), in closed form by Abel's lemma. Each closed form is checked against the
//! determinant of the shifted values of the standard basis, computed by running the recurrence, for n = 0..9.
//! Each test has a time bound.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

fn timed(what: &str, limit: u64, f: impl FnOnce()) {
    let t0 = Instant::now();
    f();
    let took = t0.elapsed();
    assert!(took < Duration::from_secs(limit), "{what} took {took:?}");
}

/// `casoratian(eq, y, n)` is free of `casoratian` and equals, at n = 0..9, the determinant of the k x k matrix of
/// the standard basis's values at n..n + k - 1; `top` gives y(m + k) from the list `v` of the values so far
/// (`v[m + j + 1]` is y(m + j)).
fn check(e: &mut CoreEngine, eq: &str, k: usize, top: &str) {
    let c = ev(e, &format!("cc = casoratian({eq}, y, n); cc"));
    assert!(!c.contains("casoratian"), "{eq}: {c}");
    let src = format!(
        "{{ let k = {k}; let step = (m, v) => {top}; var bs = map(i => map(j => if i == j then 1 else 0, range(1, k)), range(1, k)); for m in range(0, 9 + k) {{ bs = map(v => append(v, step(m, v)), bs) }}; every(range(0, 9), t => simplify(det(map(j => map(v => v[t + j + 1], bs), range(0, k - 1))) - rewrite(cc, n -> t)) === 0) }}"
    );
    holds(e, &src);
}

#[test]
fn constant_coefficients() {
    timed("constant coefficients", 60, || {
        let mut e = CoreEngine::new();
        check(&mut e, "y(n + 2) == y(n + 1) + y(n)", 2, "v[m + 2] + v[m + 1]");
        holds(&mut e, "cc === (-1)^n");
        check(&mut e, "y(n + 3) - 5*y(n + 2) + 11*y(n) == 0", 3, "5*v[m + 3] - 11*v[m + 1]");
        holds(&mut e, "cc === (-11)^n");
        // an equation written at another shift
        check(&mut e, "y(n + 1) - 2*y(n) + 7*y(n - 1) == 0", 2, "2*v[m + 2] - 7*v[m + 1]");
    });
}

#[test]
fn variable_coefficients() {
    timed("variable coefficients", 60, || {
        let mut e = CoreEngine::new();
        check(&mut e, "y(n + 2) - (n + 5)*y(n) == 0", 2, "(m + 5)*v[m + 1]");
        holds(&mut e, "cc === (-1)^n*pochhammer(5, n)");
        // the product of factorials is the Barnes G-function
        check(&mut e, "y(n + 2) - factorial(n)*y(n) == 0", 2, "factorial(m)*v[m + 1]");
        holds(&mut e, "cc === (-1)^n*barnes_g(1 + n)");
        // a periodic coefficient: P^floor(n/3) times a partial product
        check(&mut e, "y(n + 2) - (mod(n, 3) + 1)*y(n) == 0", 2, "(mod(m, 3) + 1)*v[m + 1]");
        // a leading coefficient
        check(&mut e, "(n + 1)*y(n + 2) - y(n + 1) + (n + 3)*y(n) == 0", 2, "(v[m + 2] - (m + 3)*v[m + 1])/(m + 1)");
        // a_0(0) = 0: the standard set is not fundamental from 0, so it is taken at n = 1: C(1) = 1 and C(n + 1) = -n C(n)
        // give C(n) = (-1)^(n - 1) (n - 1)!
        assert_eq!(ev(&mut e, "casoratian(y(n + 2) - n*y(n) == 0, y, n)"), "-1*factorial(-1 + n)*(-1)^n");
        // the list form is unchanged
        holds(&mut e, "rewrite(casoratian([1, 2^n, 3^n], n), n -> 3) === 432");
    });
}
