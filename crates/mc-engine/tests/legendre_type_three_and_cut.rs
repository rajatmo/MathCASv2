//! Legendre functions of type 3 (on the plane cut along (-inf, 1]) and of type 2 on the cuts: the closed forms at
//! integer degree and order against the hypergeometric evaluation of DLMF 14.3.7 at 30 digits, the hypergeometric
//! values off the integers against Legendre's equation (central differences at 60 digits), and the values of type 2
//! on the cut x > 1 against the limit from below. Each test has a time bound.
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

const POINTS: [&str; 4] = ["5/2", "1 + im", "-2 + im/2", "im/3 - 1/5"];

#[test]
fn integer_type_three_forms_agree_with_the_hypergeometric_values() {
    timed("type 3 forms", 120, || {
        let mut e = CoreEngine::new();
        for (n, m) in [(0, 0), (1, 1), (2, 1), (3, 2), (4, 0), (2, 2)] {
            for z in POINTS {
                // the symbolic form at an exact point, and the hypergeometric route at a 30-digit point
                holds(&mut e, &format!("abs(n(rewrite(legendre_q({n}, {m}, 3, z), z -> {z}), 30) - legendre_q({n}, {m}, 3, n({z}, 30))) < 10^-24"));
                holds(&mut e, &format!("abs(n(rewrite(legendre_p({n}, {m}, 3, z), z -> {z}), 30) - legendre_p({n}, {m}, 3, n({z}, 30))) < 10^-24"));
            }
        }
        // the documented forms
        holds(&mut e, "legendre_p(2, 1, 3, z) === 3*z*(-1 + z)^(1/2)*(1 + z)^(1/2)");
        holds(&mut e, "legendre_p(2, 1, 1, z) === legendre_p(2, 1, z)");
        holds(&mut e, "legendre_q(1, 1, 1, z) === legendre_q(1, 1, z) and legendre_q(1, 1, 2, z) === legendre_q(1, 1, z)");
        holds(&mut e, "abs(legendre_q(0, 0, 3, 2.) - n(log(3)/2)) < 10^-15");
        // the cut stays unevaluated
        holds(&mut e, "head(legendre_q(1/2, 2/3, 3, 0.5)) === legendre_q and head(legendre_q(1, 1, 3, 1/2)) === legendre_q");
    });
}

#[test]
fn type_three_values_off_the_integers_solve_legendres_equation() {
    timed("type 3 equation", 120, || {
        let mut e = CoreEngine::new();
        for (nu, mu) in [("1/2", "2/3"), ("-1/3", "1/4"), ("5/4", "-1/2"), ("2/5", "3/2")] {
            for z in POINTS {
                // (1 - z^2) w'' - 2 z w' + (nu (nu + 1) - mu^2/(1 - z^2)) w = 0 with h = 10^-10
                let w = |s: &str| format!("legendre_q({nu}, {mu}, 3, n({z} + ({s}), 60))");
                let src = format!(
                    "{{ let h = 10^-10; let z = n({z}, 60); let w0 = {}; let wp = {}; let wm = {}; \
                     let r = (1 - z^2)*(wp - 2*w0 + wm)/h^2 - 2*z*(wp - wm)/(2*h) + ({nu}*({nu} + 1) - ({mu})^2/(1 - z^2))*w0; \
                     abs(r) < 10^-12*(1 + abs(w0)) }}",
                    w("0"),
                    w("h"),
                    w("-h")
                );
                holds(&mut e, &src);
            }
        }
    });
}

#[test]
fn type_two_on_the_cut_is_the_limit_from_below() {
    timed("type 2 on the cut", 120, || {
        let mut e = CoreEngine::new();
        holds(&mut e, "abs(legendre_p(1/2, 2/3, n(2, 30)) - legendre_p(1/2, 2/3, n(2 - 10^-28*im, 30))) < 10^-20");
        // Q of order 0 off [-1, 1] at machine precision (complex balls stay with the interval forms)
        holds(&mut e, "abs(legendre_q(1/2, 2.) - legendre_q(1/2, 2. - 10.^-12*im)) < 10^-9");
        holds(&mut e, "abs(legendre_q(1/2, -2.5) - legendre_q(1/2, -2.5 - 10.^-12*im)) < 10^-9");
        holds(&mut e, "abs(legendre_q(1/2, n(2, 30)) - n(legendre_q(1/2, 2))) < 10^-14");
        // the documented values
        holds(&mut e, "abs(legendre_p(1/2, 2/3, 2.) - (0.5198554489507006 - 0.9004160501741421*im)) < 10^-12");
        holds(&mut e, "abs(n(legendre_q(1/2, 2)) - (0.2240142928364156 - 2.0878053429636796*im)) < 10^-12");
        // integer orders keep the form by derivatives
        holds(&mut e, "abs(legendre_p(2, 1, 3.) - n(-3*3*(1 - 9)^(1/2))) < 10^-12");
        // Legendre's equation for Q of degree 1/2 at x = 5/2 (40 digits, h = 10^-10)
        holds(
            &mut e,
            "{ let h = 10^-10; let x = n(5/2, 40); let w = t => legendre_q(1/2, t); \
             let r = (1 - x^2)*(w(x + h) - 2*w(x) + w(x - h))/h^2 - 2*x*(w(x + h) - w(x - h))/(2*h) + (3/4)*w(x); abs(r) < 10^-12 }",
        );
    });
}
