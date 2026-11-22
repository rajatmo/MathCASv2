//! Answers for inputs that used to stay unevaluated: limits and asymptotic forms of the prime counting function
//! and the n-th prime (prime number theorem), Abel summation, recurrence systems with a constraint, numeric
//! integrals over regions, and rules given as an association. Each value is checked by a route that does not use
//! the code that computed it. Time-bounded.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn prime_number_theorem_forms() {
    let mut e = CoreEngine::new();
    let t0 = Instant::now();
    assert_eq!(ev(&mut e, "asymptotic(prime_pi(x), x -> inf)"), "x/log(x)");
    assert_eq!(ev(&mut e, "discrete_asymptotic(prime(n), n -> inf)"), "n*log(n)");
    assert_eq!(ev(&mut e, "discrete_limit(prime(n)/(n*log(n)), n -> inf)"), "1");
    assert_eq!(ev(&mut e, "asymptotic_equivalent(prime_pi(x), log_integral(x), x -> inf)"), "true");
    assert_eq!(ev(&mut e, "limit(prime_pi(x)*log(x)/x, x -> inf)"), "1");
    // the ratios do approach 1 (slowly): pi(10^7) log(10^7)/10^7 = 1.0712..., p_(10^6)/(10^6 log 10^6) = 1.1209...
    assert_eq!(ev(&mut e, "abs(n(prime_pi(10^7)*log(10^7)/10^7) - 1.0712) < 10^-3"), "true");
    assert_eq!(ev(&mut e, "abs(n(prime(10^6)/(10^6*log(10^6))) - 1.1209) < 10^-3"), "true");
    // a difference is decided by lower order terms: not replaced; prime of a real variable is not a sequence
    assert!(ev(&mut e, "limit(prime_pi(x) - x/log(x), x -> inf)").starts_with("limit("));
    assert!(ev(&mut e, "limit(prime(x)/x, x -> inf)").starts_with("limit("));
    assert!(t0.elapsed() < Duration::from_secs(60), "took {:?}", t0.elapsed());
}

#[test]
fn abel_summation() {
    let mut e = CoreEngine::new();
    // sum (-1)^i r^i = -r/(1 + r) -> -1/2; sum (-1)^i i r^i = -r/(1 + r)^2 -> -1/4 (r -> 1 from below)
    assert_eq!(ev(&mut e, "sum((-1)^i, [i, inf], regularization: \"Abel\")"), "-1/2");
    assert_eq!(ev(&mut e, "sum((-1)^i*i, [i, 0, inf], regularization: \"Abel\")"), "-1/4");
    // the partial sums at r = 0.999 agree with the values
    assert_eq!(ev(&mut e, "abs(sum((-1)^i*i*0.999^i, [i, 0, 20000]) + 1/4) < 10^-3"), "true");
    // a convergent series: its sum (Abel's theorem)
    assert_eq!(ev(&mut e, "sum(1/2^i, [i, inf], regularization: \"Abel\")"), "1");
    // no Abel sum: no answer; an unknown method: a failure
    assert!(ev(&mut e, "sum(i, [i, inf], regularization: \"Abel\")").starts_with("sum("));
    assert!(ev(&mut e, "sum(1/2^i, [i, inf], regularization: \"Borel\")").starts_with("failure(bad_option"));
}

#[test]
fn recurrence_systems_with_a_constraint() {
    let mut e = CoreEngine::new();
    let t0 = Instant::now();
    let sys = "[y(n + 1) + z(n + 1) == y(n) - 3*z(n) + 7, y(n) + 2*z(n) == 1, y(0) == 1/2, z(0) == 1/4]";
    // values by hand: z(n + 1) = 5 z(n) - 7 from the two equations, y = 1 - 2 z
    let table = "[[1/2, 1/4], [25/2, -23/4], [145/2, -143/4], [745/2, -743/4], [3745/2, -3743/4]]";
    assert_eq!(ev(&mut e, &format!("recurrence_table({sys}, [y, z], [n, 0, 4])")), table);
    let s = ev(&mut e, &format!("sol = r_solve({sys}, [y, z], n); sol"));
    assert!(s.starts_with("[[y -> "), "{s}");
    assert_eq!(ev(&mut e, "table(rewrite([y(k), z(k)], sol[1]), k, range(0, 4))"), table);
    // every equation holds identically
    assert_eq!(ev(&mut e, &format!("simplify(rewrite({sys}, sol[1]))")), "[true, true, true, true]");
    // the general solution satisfies the two recurrences for any c(1)
    let g = ev(&mut e, "gs = r_solve([y(n + 1) + z(n + 1) == y(n) - 3*z(n) + 7, y(n) + 2*z(n) == 1], [y, z], n); gs");
    assert!(g.contains("c(1)"), "{g}");
    assert_eq!(ev(&mut e, "simplify(rewrite([y(n + 1) + z(n + 1) == y(n) - 3*z(n) + 7, y(n) + 2*z(n) == 1], gs[1]))"), "[true, true]");
    // inconsistent initial values: no solution is invented
    assert!(ev(&mut e, "r_solve([y(n + 1) + z(n + 1) == y(n) - 3*z(n) + 7, y(n) + 2*z(n) == 1, y(0) == 1/2, z(0) == 1], [y, z], n)")
        .starts_with("r_solve("));
    // a list of one sequence
    assert_eq!(ev(&mut e, "r_solve([z(n + 1) == 2*z(n), z(0) == 1], [z], n)"), "[[z -> (n => 2^n)]]");
    assert!(t0.elapsed() < Duration::from_secs(60), "took {:?}", t0.elapsed());
}

#[test]
fn numeric_integrals_over_regions() {
    let mut e = CoreEngine::new();
    let close = |e: &mut CoreEngine, src: &str, v: &str| {
        let r = ev(e, &format!("abs({src} - {v}) < 10^-10"));
        assert_eq!(r, "true", "{src}: {}", ev(e, src));
    };
    // the cone of radius 1, height 2: pi/3 * 2
    close(&mut e, "n_integrate(1, element([x, y, z], cone()))", "n(2*pi/3)");
    // the ellipse with semi-axes 2 and 1: its perimeter, against the arc length of the parametrization
    close(
        &mut e,
        "n_integrate(1, element([x, y], circle([0, 0], [2, 1])))",
        "n_integrate(sqrt(4*sin(t)^2 + cos(t)^2), [t, 0, 2*pi])",
    );
    close(&mut e, "n_integrate(2, element([x, y, z], ball()))", "n(8*pi/3)");
    close(&mut e, "n_integrate(x^2, element([x, y], disk()))", "n(pi/4)");
    // more variables than the region has coordinates: an argument failure
    assert!(ev(&mut e, "n_integrate(1, element([x, y, z], disk()))").starts_with("failure("));
}

#[test]
fn rules_from_an_association() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "rewrite([a, b], assoc([a -> 1, c -> 2]))"), "[1, b]");
}
