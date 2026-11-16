//! `find_instance` over subsets of the integers, the primes, bounded integer problems with other functions, families
//! of solutions of periodic equations, several points of real sets of positive dimension and of finite complex sets,
//! and `minimize` over the integers. Every instance is checked by another route: the sum of the squares recomputed,
//! primality and the condition evaluated separately, the equation at 30 digits, the minimum against a brute-force
//! table over a box that holds the feasible set. Each test has a time bound.
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

const SEVEN: &str = "x^2 + y^2 + z^2 + t^2 + u^2 + v^2 + w^2 == 1234567890987654321, [x, y, z, t, u, v, w]";

#[test]
fn sums_of_seven_squares_in_integer_subsets() {
    timed("sums of seven squares", 60, || {
        let mut e = CoreEngine::new();
        for (dom, sign) in [
            ("integers", "true"),
            ("positive_integers", "every(vs, k => k > 0)"),
            ("nonnegative_integers", "every(vs, k => k >= 0)"),
            ("negative_integers", "every(vs, k => k < 0)"),
            ("nonpositive_integers", "every(vs, k => k <= 0)"),
        ] {
            // the squares summed again in integer arithmetic, and the signs of the domain
            holds(
                &mut e,
                &format!(
                    "{{ let r = find_instance({SEVEN}, {dom}); let vs = map(q => q[2], r[1]); length(r) == 1 and length(vs) == 7 and every(vs, is_integer) and total(map(k => k^2, vs)) == 1234567890987654321 and {sign} }}"
                ),
            );
        }
        // 3 is no sum of two squares: none (the bounded integer problem searched completely)
        assert_eq!(ev(&mut e, "find_instance(x^2 + y^2 == 3, [x, y], integers)"), "[]");
        holds(&mut e, "{ let r = find_instance(x^2 + y^2 + z^2 + t^2 == 10^18 + 7, [x, y, z, t], positive_integers); total(map(q => q[2]^2, r[1])) == 10^18 + 7 and every(r[1], q => q[2] >= 1) }");
    });
}

#[test]
fn primes_by_a_condition() {
    timed("primes", 60, || {
        let mut e = CoreEngine::new();
        // the first prime above 10^10: prime, above the bound, and no prime between (prime_pi)
        holds(&mut e, "{ let p = rewrite(p, find_instance(p > 10^10, p, primes)[1]); is_prime(p) and p > 10^10 and p == next_prime(10^10) }");
        // twin primes: both members prime, five distinct ones
        holds(&mut e, "{ let ps = rewrite(p, find_instance(element(p, primes) and element(p + 2, primes), p, 5)); length(union(ps)) == 5 and every(ps, q => is_prime(q) and is_prime(q + 2)) }");
        // an upper bound ends the search: every prime of the interval when fewer than asked exist
        holds(&mut e, "rewrite(p, find_instance(p > 20 and p < 40, p, primes, 10)) === [23, 29, 31, 37]");
        holds(&mut e, "find_instance(p > 24 and p < 28, p, primes) === []");
    });
}

#[test]
fn bounded_integer_problems_with_other_functions() {
    timed("bounded integer problems", 90, || {
        let mut e = CoreEngine::new();
        // power_mod recomputed from the power, the bounds checked
        holds(&mut e, "{ let r = find_instance(power_mod(a, b, 17) == 1 and 0 < a and a < 17 and 0 < b and b < 17, [a, b], integers, 2); length(r) == 2 and every(r, s => { let a0 = rewrite(a, s); let b0 = rewrite(b, s); mod(a0^b0, 17) == 1 and 0 < a0 and a0 < 17 and 0 < b0 and b0 < 17 }) }");
        // Euler's function from the factorization: phi(m) phi(n) == phi(m n) exactly when gcd(m, n) == 1
        holds(&mut e, "{ let r = find_instance(euler_phi(m) * euler_phi(n_u) == euler_phi(m * n_u) and 0 < m and m < n_u and n_u < 10, [n_u, m], integers, 3); length(r) == 3 and every(r, s => gcd(rewrite(m, s), rewrite(n_u, s)) == 1 and rewrite(0 < m and m < n_u and n_u < 10, s)) }");
        // all of them when fewer exist (the box is searched completely): gcd(m, n) == 1 with 0 < m < n < 4
        holds(&mut e, "length(find_instance(euler_phi(m) * euler_phi(n_u) == euler_phi(m * n_u) and 0 < m and m < n_u and n_u < 4, [n_u, m], integers, 10)) == 3");
    });
}

#[test]
fn periodic_and_finite_solution_sets() {
    timed("periodic and finite solution sets", 90, || {
        let mut e = CoreEngine::new();
        // the equation at 30 digits at the point found
        holds(&mut e, "{ let r = find_instance(sin(x) == 1/3, x); abs(n(sin(rewrite(x, r[1])) - 1/3, 30)) < 10^-25 }");
        holds(&mut e, "{ let r = find_instance(1 + cos(x) == 3/2, x); abs(n(1 + cos(rewrite(x, r[1])) - 3/2, 30)) < 10^-25 }");
        holds(&mut e, "{ let r = find_instance(cos(x) == 1/5, x, 3); length(union(r)) == 3 and every(r, s => abs(n(cos(rewrite(x, s)) - 1/5, 30)) < 10^-25) }");
        // the imaginary part of log is in (-pi, pi]: no x
        holds(&mut e, "find_instance(log(x) == (75/11)*im*pi + 17, x) === []");
        // a finite set: all of its points when fewer than asked
        holds(&mut e, "{ let r = find_instance(x^2 + y^2 == 1 and x == 2*y + 1, [x, y], 5); length(r) == 2 and every(r, s => rewrite(x^2 + y^2 == 1 and x == 2*y + 1, s)) }");
        // a curve over the complexes: three distinct points, the equations in machine numbers (the exact values are 0),
        // x y z != 0
        holds(&mut e, "{ let r = find_instance(x^2 + y^3 == z and x + 2*y == 3*z + 1 and x*y*z != 0, [x, y, z], 3); length(r) == 3 and length(union(n(r, 20))) == 3 and every(r, s => { let q = n(rewrite([x^2 + y^3 - z, x + 2*y - 3*z - 1, x*y*z], s)); abs(q[1]) < 10^-9 and abs(q[2]) < 10^-9 and abs(q[3]) > 10^-5 }) }");
    });
}

#[test]
fn several_real_points_of_sets_of_positive_dimension() {
    timed("real points", 120, || {
        let mut e = CoreEngine::new();
        // points of a surface and of an open set: the relations at 30 digits, distinct points
        holds(&mut e, "{ let r = find_instance(x^2 + 2*y^2 + 3*z^2 == 1 and z != 0, [x, y, z], reals, 3); length(union(r)) == 3 and every(r, s => abs(n(rewrite(x^2 + 2*y^2 + 3*z^2 - 1, s), 30)) < 10^-25 and rewrite(z, s) != 0) }");
        holds(&mut e, "{ let r = find_instance(x^2 + y*z == 1 and x + 2*y <= 3*z + 1 and x*y*z > 7, [x, y, z], reals, 4); length(union(r)) == 4 and every(r, s => rewrite(x^2 + y*z == 1 and x + 2*y <= 3*z + 1 and x*y*z > 7, s)) }");
        holds(&mut e, "{ let r = find_instance(x^3 + y^3 - 4*x*y == 0 and -3 < x and x < 3 and -3 < y and y < 3, [x, y], reals, 6); length(r) == 6 and every(r, s => abs(n(rewrite(x^3 + y^3 - 4*x*y, s), 30)) < 10^-25 and n(rewrite(abs(x) < 3 and abs(y) < 3, s))) }");
    });
}

#[test]
fn minimum_over_integers() {
    timed("integer minimum", 90, || {
        let mut e = CoreEngine::new();
        // the real relaxation attained at an integer point
        assert_eq!(ev(&mut e, "minimize([x^2 + x*y], [x, y], positive_integers)"), "[2, [x -> 1, y -> 1]]");
        assert_eq!(ev(&mut e, "minimize([x^2 + x*y], [x, y], negative_integers)"), "[2, [x -> -1, y -> -1]]");
        assert_eq!(ev(&mut e, "minimize([x^2 + x*y], [x, y], nonnegative_integers)"), "[0, [x -> 0, y -> 0]]");
        // a bounded feasible set: against the brute-force table of the box [-10, 10]^2
        holds(&mut e, "minimize([x - y, x^2 + y^2 <= 100 and element(x | y, integers)], [x, y])[1] == min(flatten(table(if x^2 + y^2 <= 100 then x - y else inf, [x, -10, 10], [y, -10, 10])))");
        holds(&mut e, "max_value([x + 2*y, x^2 + y^2 <= 10], [x, y], integers) == max(flatten(table(if x^2 + y^2 <= 10 then x + 2*y else -inf, [x, -4, 4], [y, -4, 4])))");
        holds(&mut e, "{ let p = arg_max([x + 2*y, x^2 + y^2 <= 10], [x, y], integers); p[1]^2 + p[2]^2 <= 10 and p[1] + 2*p[2] == 7 }");
    });
}
