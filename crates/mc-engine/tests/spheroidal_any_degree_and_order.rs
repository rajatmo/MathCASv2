//! Spheroidal eigenvalues of a degree or an order that is not an integer (the two-sided Legendre expansion), the
//! angular function at gamma = 0 for every degree and order, and the angular function of order 0 beyond x = 1. The
//! eigenvalues are checked against the eigenvalues of the truncated two-sided matrix (another method: a dense
//! eigenvalue solver instead of the root of the continued fraction), the angular function outside [-1, 1] against
//! the spheroidal equation (central differences at 30 digits). Each test has a time bound.
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

/// The truncated matrix of the recurrence A_k a_(k-1) + B_k a_k + C_k a_(k+1) = lambda a_k, -kk <= k <= kk, as text.
fn matrix(nu: (f64, f64), mu: (f64, f64), g2: f64, kk: i64) -> String {
    type C = (f64, f64);
    let mul = |a: C, b: C| (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0);
    let div = |a: C, b: C| {
        let d = b.0 * b.0 + b.1 * b.1;
        ((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d)
    };
    let add = |a: C, s: f64| (a.0 + s, a.1);
    let sc = |a: C, s: f64| (a.0 * s, a.1 * s);
    let txt = |a: C| format!("({:e} + ({:e})*im)", a.0, a.1);
    let n = (2 * kk + 1) as usize;
    let mut rows = vec![];
    for (i, k) in (-kk..=kk).enumerate() {
        let k2 = 2.0 * k as f64;
        let l = add(nu, k2);
        let nm = add((nu.0 - mu.0, nu.1 - mu.1), k2);
        let np = add((nu.0 + mu.0, nu.1 + mu.1), k2);
        let t = add(sc(nu, 2.0), 4.0 * k as f64);
        let ll = mul(l, add(l, 1.0));
        let a = sc(div(mul(add(nm, -1.0), nm), mul(add(t, -3.0), add(t, -1.0))), -g2);
        let b = add(ll, 0.0);
        let b = (b.0, b.1);
        let corr = sc(div(add((ll.0 + mul(mu, mu).0, ll.1 + mul(mu, mu).1), -1.0), mul(add(t, -1.0), add(t, 3.0))), 2.0 * g2);
        let b = (b.0 - corr.0, b.1 - corr.1);
        let c = sc(div(mul(add(np, 1.0), add(np, 2.0)), mul(add(t, 3.0), add(t, 5.0))), -g2);
        let mut row = vec!["0".to_string(); n];
        row[i] = txt(b);
        if i > 0 {
            row[i - 1] = txt(a);
        }
        if i + 1 < n {
            row[i + 1] = txt(c);
        }
        rows.push(format!("[{}]", row.join(", ")));
    }
    format!("[{}]", rows.join(", "))
}

#[test]
fn eigenvalues_off_the_integers_are_eigenvalues_of_the_truncated_matrix() {
    timed("eigenvalues off the integers", 240, || {
        let mut e = CoreEngine::new();
        for (args, nu, mu, g2) in [
            ("2, 0.5, -5", (2.0, 0.0), (0.5, 0.0), 25.0),
            ("1, 0.25, 5", (1.0, 0.0), (0.25, 0.0), 25.0),
            ("0.2, 1, 5", (0.2, 0.0), (1.0, 0.0), 25.0),
            ("2.3, 0.4, 3", (2.3, 0.0), (0.4, 0.0), 9.0),
            ("7, 5. - im, 2", (7.0, 0.0), (5.0, -1.0), 4.0),
            ("2.3 + 0.1*im, 0.4, 1.5", (2.3, 0.1), (0.4, 0.0), 2.25),
        ] {
            let m = matrix(nu, mu, g2, 30);
            holds(&mut e, &format!("{{ let v = spheroidal_eigenvalue({args}); is_numeric(v) and min(map(t => abs(t - v), eigenvalues({m}))) < 10^-8*(1 + abs(v)) }}"));
        }
        // the documented values
        holds(&mut e, "abs(spheroidal_eigenvalue(2, 0.5, -5) - (-4.3221484967472925)) < 10^-12");
        holds(&mut e, "abs(n(spheroidal_eigenvalue(237, 5 - im, 2)) - (56403.99916672184 + 0.0003545863467686943*im)) < 10^-8");
        // 30 digits agree with machine numbers
        holds(&mut e, "abs(n(spheroidal_eigenvalue(2, 1/2, -5), 30) - spheroidal_eigenvalue(2, 0.5, -5)) < 10^-12");
        // between the points where two eigenvalues meet the value stays unevaluated
        holds(&mut e, "head(spheroidal_eigenvalue(0.2111111111111111, 1, 2)) === spheroidal_eigenvalue");
        // degree + 1/2 an integer: the recurrence divides by 0
        holds(&mut e, "head(spheroidal_eigenvalue(1.5, 0.5, 2)) === spheroidal_eigenvalue");
    });
}

#[test]
fn angular_function_at_gamma_zero_and_beyond_one() {
    timed("angular function", 120, || {
        let mut e = CoreEngine::new();
        holds(&mut e, "spheroidal_ps(1/2, 1/2, 0, x) === legendre_p(1/2, 1/2, x)");
        holds(&mut e, "spheroidal_ps(n_u, m, 0, x) === legendre_p(n_u, m, x)");
        // the Ferrers functions of order +-1/2 at exact points of (-1, 1) against their hypergeometric values
        for (nu, mu, x) in [("1/2", "1/2", "1/2"), ("3/2", "-1/2", "-1/3"), ("-5/2", "1/2", "2/7"), ("1/2", "1/2", "0")] {
            holds(&mut e, &format!("{{ let v = spheroidal_ps({nu}, {mu}, 0, {x}); free_of(v, legendre_p) and abs(n(v, 30) - legendre_p({nu}, {mu}, n({x}, 30))) < 10^-25 }}"));
        }
        holds(&mut e, "table(spheroidal_ps(n_u, m, 0, x), m, range(0, 2), n_u, range(0, 2))[3] === [0, 0, 3*(1 - x^2)]");
        holds(&mut e, "abs(spheroidal_ps(1, 0, 4, 2.) - (-0.026744686756678203)) < 10^-12");
        // the spheroidal equation (1 - x^2) w'' - 2 x w' + (lambda + gamma^2 (1 - x^2)) w = 0 at x = 2 and 3/2
        for (n, g, x) in [(1, 4, "2"), (2, 3, "3/2"), (0, 2, "5/2")] {
            holds(
                &mut e,
                &format!(
                    "{{ let h = 10^-8; let x = n({x}, 30); let w = t => spheroidal_ps({n}, 0, n({g}, 30), t); let lam = spheroidal_eigenvalue({n}, 0, n({g}, 30)); \
                     let r = (1 - x^2)*(w(x + h) - 2*w(x) + w(x - h))/h^2 - 2*x*(w(x + h) - w(x - h))/(2*h) + (lam + {g}^2*(1 - x^2))*w(x); \
                     abs(r) < 10^-6*(1 + abs(w(x))) }}"
                ),
            );
        }
        // the series inside (-1, 1) continues to the values outside: the point 1 - 10^-6 and 1 + 10^-6 nearly agree
        holds(&mut e, "abs(spheroidal_ps(1, 0, 4., 1 - 10^-6) - spheroidal_ps(1, 0, 4., 1 + 10^-6)) < 10^-4");
    });
}
