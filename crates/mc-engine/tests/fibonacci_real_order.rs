//! Fibonacci and Lucas functions of an order that is not an integer: `function_expand` gives Binet's forms continued
//! in the order, `d` in x follows from them, and `series` in x expands them (Taylor coefficients at analytic points,
//! Puiseux series at the branch points x = +-2i, the expansion at infinity). Every closed form is compared with the
//! numerical values at 30 digits (complex and negative x too), checked against the recurrence and the integer orders,
//! and every series coefficient against Cauchy's integral (the trapezoidal rule on a circle, exact to ~40 digits) or a
//! central difference. Each test has a time bound.
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

const ORDERS: [&str; 5] = ["1/2", "1/3", "-7/4", "5/2", "7/3"];
const POINTS: [&str; 7] = ["3/10", "-7/5", "5/2", "-4", "1/2 + im", "-3/2 - 2*im/3", "im/5"];

#[test]
fn documented_inputs() {
    timed("documented inputs", 120, || {
        let mut e = CoreEngine::new();
        for (src, want) in [
            ("series(fibonacci(1 / 2, x), x, 0, 5)", "1/2 + x/8 - 3*x^2/64 - 5*x^3/256 + 35*x^4/4096 + 63*x^5/16384"),
            ("series(lucas_l(1 / 2, x), x, 0, 5)", "1 + x/4 + x^2/32 - x^3/128 - 5*x^4/2048 + 7*x^5/8192"),
        ] {
            holds(&mut e, &format!("{{ let s = {src}; head(s) === series_data and s[5] == 6 and expand(normal(s) - ({want})) === 0 }}"));
        }
        for (src, want) in [
            ("function_expand(fibonacci(n_u, x))", "(2^-n_u*(x + (4 + x^2)^(1/2))^n_u - cos(n_u*pi)*2^n_u*(x + (4 + x^2)^(1/2))^-n_u)/(4 + x^2)^(1/2)"),
            ("function_expand(fibonacci(1 / 2, x))", "2^(1/2)*(x + (4 + x^2)^(1/2))^(1/2)/(2*(4 + x^2)^(1/2))"),
            ("function_expand(lucas_l(1 / 2, x))", "(1/2)*2^(1/2)*(x + (4 + x^2)^(1/2))^(1/2)"),
            ("d(fibonacci(n, x), x)", "(n*lucas_l(n, x) - x*fibonacci(n, x))/(4 + x^2)"),
            ("d(lucas_l(n, x), x)", "n*fibonacci(n, x)"),
        ] {
            let w = ev(&mut e, want);
            assert_eq!(ev(&mut e, src), w, "{src}");
        }
        // the table of Lucas functions at x = 0, 1, 2: 1 + cos(pi n) = 2 cos(pi n/2)^2 at 0
        holds(&mut e, "take(function_expand(table(lucas_l(n_u, x), x, range(0, 2))), 2) === [1 + cos(n_u*pi), 2^-n_u*(1 + 5^(1/2))^n_u + cos(n_u*pi)*2^n_u*(1 + 5^(1/2))^-n_u]");
        holds(&mut e, "full_simplify(1 + cos(n_u*pi) - 2*cos(n_u*pi/2)^2) == 0");
        // the integer orders: the polynomials again
        holds(&mut e, "full_simplify(rewrite(function_expand(fibonacci(n_u, x)), n_u -> 3) - fibonacci(3, x)) == 0");
        holds(&mut e, "full_simplify(rewrite(function_expand(lucas_l(n_u, x)), n_u -> 4) - lucas_l(4, x)) == 0");
        // natural orders stay polynomials; F_n(0) and L_n(0) for exact orders
        holds(&mut e, "d(fibonacci(5, x), x) === 6*x + 4*x^3");
        holds(&mut e, "[fibonacci(1/2, 0), lucas_l(1/2, 0), fibonacci(-3, 0), lucas_l(-3, 0), fibonacci(1/3, 0)] === [1/2, 1, 1, 0, 1/4]");
    });
}

/// The closed forms against the numerical values (30 digits), the recurrence F_(n+1) = x F_n + F_(n-1), and
/// fibonacci(n, 1), lucas_l(n, 1) against the numbers (a separate code path, the golden ratio).
#[test]
fn closed_forms_agree_numerically() {
    timed("closed forms", 180, || {
        let mut e = CoreEngine::new();
        ev(&mut e, "fib_form = function_expand(fibonacci(n_u, x)); lucas_form = function_expand(lucas_l(n_u, x))");
        for nu in ORDERS {
            for x in POINTS {
                for (f, cf) in [("fibonacci", "fib_form"), ("lucas_l", "lucas_form")] {
                    holds(&mut e, &format!("abs(n(rewrite({cf}, [n_u -> {nu}, x -> {x}]), 40) - {f}(n({nu}, 40), n({x}, 40))) < 10^-30"));
                    let fe = format!("function_expand({f}({nu}, x))");
                    holds(&mut e, &format!("abs(n(rewrite({fe}, x -> {x}), 40) - {f}(n({nu}, 40), n({x}, 40))) < 10^-30"));
                    // the recurrence of the polynomials holds for the continuation
                    holds(&mut e, &format!(
                        "{{ let z = n({x}, 40); let v = n({nu}, 40); abs({f}(v + 1, z) - z*{f}(v, z) - {f}(v - 1, z)) < 10^-30 }}"
                    ));
                }
            }
            holds(&mut e, &format!("abs(n(fibonacci({nu}), 40) - fibonacci(n({nu}, 40), 1)) < 10^-30"));
            holds(&mut e, &format!("abs(n(lucas_l({nu}), 40) - lucas_l(n({nu}, 40), 1)) < 10^-30"));
        }
    });
}

/// Taylor coefficients c_k at x0 against (1/N) sum f(x0 + r w^j) (r w^j)^-k, w = exp(2 pi i/N): the trapezoidal rule
/// for Cauchy's integral, with error about (r/R)^N, R the distance to the branch points +-2i.
#[test]
fn series_coefficients_by_cauchy_integrals() {
    timed("series coefficients", 240, || {
        let mut e = CoreEngine::new();
        for (f, nu) in [("fibonacci", "1/2"), ("lucas_l", "1/2"), ("fibonacci", "1/3"), ("lucas_l", "-5/4"), ("fibonacci", "7/3")] {
            for x0 in ["0", "1", "-1/2"] {
                ev(&mut e, &format!("ser_val = series({f}({nu}, x), x, {x0}, 5)"));
                holds(&mut e, "head(ser_val) === series_data");
                for k in 0..=5 {
                    holds(&mut e, &format!(
                        "{{ let N = 64; let r = 1/2; let ws = table(n(r*exp(2*pi*im*j/N), 45), [j, 0, N - 1]); \
                         let c = sum(map(w => {f}(n({nu}, 45), {x0} + w)*w^-{k}, ws))/N; \
                         abs(n(series_coefficient(ser_val, {k}), 45) - c) < 10^-30 }}"
                    ));
                }
            }
        }
        // the derivative against a central difference (h = 10^-16 at 70 digits: error about h^2)
        for (f, nu, x) in [("fibonacci", "1/2", "7/10"), ("lucas_l", "1/3", "-6/5"), ("fibonacci", "-7/4", "2 + im")] {
            holds(&mut e, &format!(
                "{{ let h = 10^-16; let v = n({nu}, 70); let z = n({x}, 70); \
                 let fd = ({f}(v, z + h) - {f}(v, z - h))/(2*h); \
                 abs(n(rewrite(d({f}({nu}, x), x), x -> {x}), 70) - fd) < 10^-29 }}"
            ));
        }
    });
}

/// At the branch points x = +-2i (Puiseux series in (x -+ 2i)^(1/2)) and at infinity: the truncated series against
/// the function near the point, in directions off the cut of sqrt(x^2 + 4) (the imaginary axis beyond +-2i).
#[test]
fn branch_points_and_infinity() {
    timed("branch points", 180, || {
        let mut e = CoreEngine::new();
        for (f, nu, x0, dirs) in [
            ("fibonacci", "1/2", "-2*im", ["1", "im", "1 + im"]),
            ("lucas_l", "1/2", "2*im", ["1", "-im", "1 - im"]),
            ("fibonacci", "1/3", "-2*im", ["1", "im", "-1 + im"]),
        ] {
            ev(&mut e, &format!("ser_val = normal(series({f}({nu}, x), x, {x0}, 3))"));
            holds(&mut e, "free_of(ser_val, fibonacci) and free_of(ser_val, lucas_l) and not free_of(ser_val, x)");
            for d in dirs {
                // the remainder is O(h^(7/2)) with h = 10^-3
                holds(&mut e, &format!(
                    "{{ let z = {x0} + ({d})/1000; abs(n(rewrite(ser_val, x -> z), 40) - {f}(n({nu}, 40), n(z, 40))) < 10^-9 }}"
                ));
            }
        }
        // a series at the branch point: it evaluates to a Puiseux series
        holds(&mut e, "{ let r = full_simplify(normal(series(fibonacci(1 / 2, x), x, -2 * im, 3, assumptions: x > 1))); free_of(r, fibonacci) and not free_of(r, (x + 2*im)^(1/2)) }");
        for (f, nu) in [("fibonacci", "1/2"), ("lucas_l", "1/3"), ("fibonacci", "-5/2")] {
            ev(&mut e, &format!("ser_val = normal(series({f}({nu}, x), x, inf, 4))"));
            holds(&mut e, "free_of(ser_val, fibonacci) and free_of(ser_val, lucas_l)");
            holds(&mut e, &format!(
                // the remainder is O(x^(-9/2))
                "abs(n(rewrite(ser_val, x -> 10000), 40) - {f}(n({nu}, 40), n(10000, 40))) < 10^-16"
            ));
        }
    });
}
