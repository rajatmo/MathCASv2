//! Every real root of one analytic equation on the real line, a half-line or up to a singular end: each root is
//! checked by substitution at 50 digits and against `find_root` at 40 digits, the counts against values known by
//! hand (sign changes of a function that is monotone between them); equations with a multiple root and calls over
//! the complex numbers stay unsolved.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// Runs `f` and checks that it took at most `secs` seconds.
fn timed(secs: u64, f: impl FnOnce(&mut CoreEngine)) {
    let mut e = CoreEngine::new();
    let t = Instant::now();
    f(&mut e);
    assert!(t.elapsed() < Duration::from_secs(secs), "took {:?}", t.elapsed());
}

/// The roots `solve_values(call)` gives: `count` of them, each a zero of `f` (a function of x) at 50 digits, each
/// equal to `find_root` from its machine value at 40 digits.
fn roots(e: &mut CoreEngine, call: &str, f: &str, count: usize) {
    ev(e, &format!("vs = solve_values({call})"));
    assert_eq!(ev(e, "length(vs)"), count.to_string(), "{call}: {}", ev(e, "vs"));
    for k in 1..=count {
        let at = format!("rewrite({f}, x -> n(vs[{k}], 50))");
        assert_eq!(ev(e, &format!("abs({at}) < 10^-40")), "true", "{call}, root {k}: {}", ev(e, &at));
        let fr = format!("x /. find_root({f}, [x, n(vs[{k}])], working_precision: 40)");
        assert_eq!(ev(e, &format!("abs(n(vs[{k}], 50) - ({fr})) < 10^-35")), "true", "{call}, root {k}: {}", ev(e, &fr));
    }
}

#[test]
fn whole_line_and_half_lines() {
    timed(60, |e| {
        // log(2x) has the domain x > 0, singular at 0; exp(x) - 5x grows on the right
        roots(e, "exp(x) - 5*x + log(2*x) + 1 == 0, x, reals", "exp(x) - 5*x + log(2*x) + 1", 3);
        // f > 0 far left (20|x| dominates) and far right (exp(2 exp(x))); f'' > 0, so at most two roots
        roots(e, "exp(2*exp(x)) - log(x^2 + 1) - 20*x == 11, x, reals", "exp(2*exp(x)) - log(x^2 + 1) - 20*x - 11", 2);
        // I_0 increases on x > 0 from 1 to inf: one root
        roots(e, "bessel_i(0, x) == 2 and x > 0, x", "bessel_i(0, x) - 2", 1);
        // K_0 decreases on x > 0 from inf to 0: one root; real values only for x > 0
        roots(e, "bessel_k(0, x) == 2, x, reals", "bessel_k(0, x) - 2", 1);
        // sqrt(x) + exp(-x) at 0 is 1, increases for x > 1 to inf, and is below 2 on [0, 1]: one root
        roots(e, "sqrt(x) + exp(-x) == 2, x, reals", "sqrt(x) + exp(-x) - 2", 1);
        // atan(x) = x/2: 0 and a pair symmetric about it
        roots(e, "atan(x) == x/2, x, reals", "atan(x) - x/2", 3);
        assert_eq!(ev(e, "reduce(exp(x) + x^2 + 1 == 0, x, reals)"), "false");
    });
}

#[test]
fn forms_and_counts() {
    timed(60, |e| {
        let r = ev(e, "reduce(exp(x) - 5*x + log(2*x) + 1 == 0, x, reals)");
        assert_eq!(r.matches(" or ").count(), 2, "{r}");
        assert!(r.starts_with("x == root([&("), "{r}");
        let r = ev(e, "n_solve(exp(2*exp(x)) - log(x^2 + 1) - 20*x == 11, x, reals)");
        assert!(r.starts_with("[[x -> -0.35162712906759") && r.contains("[x -> 0.3831825156791"), "{r}");
        assert_eq!(ev(e, "length(n_solve_values(exp(2*exp(x)) - log(x^2 + 1) - 20*x == 11, x, reals))"), "2");
        // sin(x) = x/100 (odd): for x > 0 one root in (0, pi), two in each [2 pi k, 2 pi k + pi] for k = 1..15 (there
        // the line is below 1, and at 2 pi 15 + pi/2 = 95.8 still below the sine's 1), none where sin < 0 or x > 100:
        // 31; with 0 and the mirror images 63
        assert_eq!(ev(e, "count_roots(sin(x) - x/100, x)"), "63");
        assert_eq!(ev(e, "count_roots(sin(x) - x/10, x)"), "7");
        assert_eq!(ev(e, "count_roots(exp(2*exp(x)) - log(x^2 + 1) - 20*x - 11, x)"), "2");
        // cos(1/(x^2 + 1)) - sin(exp(-x^2)) is even, cos(1) - sin(1) < 0 at 0, positive far out
        assert_eq!(ev(e, "count_roots(cos(1/(x^2 + 1)) - sin(exp(-x^2)), x)"), "2");
    });
}

#[test]
fn refusals() {
    timed(60, |e| {
        // a root of multiplicity 4 at 0: no proof that the list is complete
        assert!(ev(e, "reduce(cos(x) - 1 + x^2/2 == 0, x, reals)").starts_with("failure(unsolved"));
        assert_eq!(ev(e, "count_roots(cos(x) - 1 + x^2/2, x)"), "count_roots(-1 + cos(x) + (1/2)*x^2, x)");
        // over the complex numbers K_0 = 2 may have other roots: not the real list
        assert!(ev(e, "solve(bessel_k(0, x) == 2, x)").starts_with("failure(unsolved"));
        // a parameter
        assert!(ev(e, "solve(exp(x) - a*x == 3, x, reals)").starts_with("failure") || !ev(e, "solve(exp(x) - a*x == 3, x, reals)").contains("root([&"));
    });
}
