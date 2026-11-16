//! Systems of linear recurrences (several unknown sequences) and a partial recurrence in two indices, solved by
//! `r_solve` and `r_solve_value`. Every answer is checked twice, independently of the solver:
//!
//! - symbolically: the solution put into the recurrences, every power `z^e` (`e` linear in `n`) written
//!   `z^e(0) K(z, de/dn)` with an inert `K` (`z^(u + v) = z^u z^v` for principal powers), on each parity class
//!   `n = 2k + r` (where `(-1)^(2k)` is `1^k`), then `simplify` must give 0 for symbolic `k`;
//! - numerically: the recurrence run forward in exact rational arithmetic from the initial values, compared with the
//!   closed form at `n = 0, ..., 15`.
//!
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

/// `residuals` (a list of expressions in `n` that the solution makes 0, already rewritten) is 0 for every integer
/// `n`: on both parity classes, with the powers split as described in the header.
fn zero_for_all_n(e: &mut CoreEngine, residuals: &str) {
    let src = format!(
        "{{ let res = {residuals}; table(simplify(rewrite(expand(rewrite(rewrite(res, n -> 2*k47 + r47), \
         z_^w_ when not free_of(w, k47) :> z^rewrite(w, k47 -> 0)*kk47(z^d(w, k47)))), kk47(1) -> 1)), [r47, 0, 1]) }}"
    );
    let out = ev(e, &src);
    assert!(out.chars().all(|ch| "[], 0".contains(ch)), "{residuals}: {out}");
}

/// The closed forms `forms` (a list in `n`) equal the states of `step` (a function of `[n, x1, x2, ...]` giving the
/// next state) run from `[0, start...]`, for n = 0, ..., 15.
fn agrees_forward(e: &mut CoreEngine, forms: &str, step: &str, start: &str) {
    let src = format!(
        "{{ let it = map(s => drop(s, 1), nest_list({step}, join([0], {start}), 15)); \
         let cf = table(eval({forms}), [n, 0, 15]); simplify(cf - it) === table(0*it[1], [n, 0, 15]) }}"
    );
    holds(e, &src);
}

#[test]
fn implicit_constant_coefficient_systems() {
    timed("implicit systems", 120, || {
        let mut e = CoreEngine::new();
        // the documented system: a(n + 1) - b(n) == 2, a(n) - 3 b(n + 1) == 5
        ev(&mut e, "eqns = [a(n + 1) - b(n) == 2, a(n) - 3 * b(n + 1) == 5, a(0) == 3, b(0) == 1]");
        ev(&mut e, "sol = r_solve(eqns, [a, b], n)");
        holds(&mut e, "sol[1][1][1] === a and sol[1][2][1] === b and length(sol) == 1");
        zero_for_all_n(&mut e, "rewrite([a(n + 1) - b(n) - 2, a(n) - 3 * b(n + 1) - 5], sol[1])");
        agrees_forward(&mut e, "rewrite([a(n), b(n)], sol[1])", "s => [s[1] + 1, s[3] + 2, (s[2] - 5)/3]", "[3, 1]");
        // r_solve_value: pure functions for heads, expressions for a(n)
        ev(&mut e, "t_rhs = r_solve_value(eqns, [a, b], n)");
        holds(&mut e, "simplify([t_rhs[1](n), t_rhs[2](n)] - rewrite([a(n), b(n)], sol[1])) === [0, 0]");
        holds(&mut e, "simplify(r_solve_value(eqns, [a(n), b(n)], n) - [t_rhs[1](n), t_rhs[2](n)]) === [0, 0]");
        holds(&mut e, "r_solve(eqns, [a(n), b(n)], n)[1][1][1] === a(n)");
        holds(&mut e, "head(discrete_plot(eval([t_rhs[1](n), t_rhs[2](n)]), [n, 0, 20])) === image");

        // y(n + 1) + z(n) == 3, y(n) + 2 z(n + 1) == 1
        ev(&mut e, "sol = r_solve([y(n + 1) + z(n) == 3, y(n) + 2 * z(n + 1) == 1, y(0) == 4, z(0) == 1], [y, z], n)");
        zero_for_all_n(&mut e, "rewrite([y(n + 1) + z(n) - 3, y(n) + 2 * z(n + 1) - 1], sol[1])");
        agrees_forward(&mut e, "rewrite([y(n), z(n)], sol[1])", "s => [s[1] + 1, 3 - s[3], (1 - s[2])/2]", "[4, 1]");
        holds(&mut e, "head(discrete_plot(eval(rewrite([y(n), z(n)], sol[1])), [n, 0, 20])) === image");
    });
}

#[test]
fn forcing_shifts_and_repeated_roots() {
    timed("forcing and shifts", 120, || {
        let mut e = CoreEngine::new();
        // a polynomial forcing term
        ev(&mut e, "sol = r_solve([a(n + 1) == b(n) + n, b(n + 1) == a(n), a(0) == 0, b(0) == 1], [a(n), b(n)], n)");
        // the rule form a(n) -> ... agrees with the pure functions for the heads, which give the shifted values
        ev(&mut e, "solh = r_solve([a(n + 1) == b(n) + n, b(n + 1) == a(n), a(0) == 0, b(0) == 1], [a, b], n)");
        holds(&mut e, "simplify(rewrite([a(n), b(n)], sol[1]) - rewrite([a(n), b(n)], solh[1])) === [0, 0]");
        zero_for_all_n(&mut e, "rewrite([a(n + 1) - b(n) - n, b(n + 1) - a(n)], solh[1])");
        agrees_forward(&mut e, "rewrite([a(n), b(n)], sol[1])", "s => [s[1] + 1, s[3] + s[1], s[2]]", "[0, 1]");
        // an exponential forcing term
        ev(&mut e, "sol = r_solve([a(n + 1) == 2^n + b(n), b(n + 1) == a(n), a(0) == 1, b(0) == 0], [a, b], n)");
        zero_for_all_n(&mut e, "rewrite([a(n + 1) - 2^n - b(n), b(n + 1) - a(n)], sol[1])");
        agrees_forward(&mut e, "rewrite([a(n), b(n)], sol[1])", "s => [s[1] + 1, 2^s[1] + s[3], s[2]]", "[1, 0]");
        // backward shifts: a(n) == b(n - 1) + 1, b(n) == 4 a(n - 1)
        ev(&mut e, "sol = r_solve([a(n) == b(n - 1) + 1, b(n) == 4*a(n - 1), a(0) == 1, b(0) == 1], [a, b], n)");
        zero_for_all_n(&mut e, "rewrite([a(n) - b(n - 1) - 1, b(n) - 4*a(n - 1)], sol[1])");
        agrees_forward(&mut e, "rewrite([a(n), b(n)], sol[1])", "s => [s[1] + 1, s[3] + 1, 4*s[2]]", "[1, 1]");
        // a triple eigenvalue (Jordan block) in three unknowns
        ev(&mut e, "v = r_solve_value([a(n + 1) == a(n) + b(n) + w(n), b(n + 1) == b(n) + w(n), w(n + 1) == w(n), a(0) == 0, b(0) == 0, w(0) == 1], [a(n), b(n), w(n)], n)");
        holds(&mut e, "simplify(v - [n*(n + 1)/2, n, 1]) === [0, 0, 0]");
        // no initial values: a(n + 1) == 2 a(n) + b(n), b(n + 1) == 2 b(n) with constants c(1), c(2)
        ev(&mut e, "sol = r_solve([a(n + 1) == 2*a(n) + b(n), b(n + 1) == 2*b(n)], [a, b], n)");
        zero_for_all_n(&mut e, "rewrite([a(n + 1) - 2*a(n) - b(n), b(n + 1) - 2*b(n)], sol[1])");
        agrees_forward(&mut e, "rewrite(rewrite([a(n), b(n)], sol[1]), [c(1) -> 3, c(2) -> -2])", "s => [s[1] + 1, 2*s[2] + s[3], 2*s[3]]", "[3, -2]");
    });
}

#[test]
fn coefficients_of_period_two() {
    timed("period two", 120, || {
        let mut e = CoreEngine::new();
        ev(&mut e, "sol = r_solve([x(n + 1) == (1 / 2) * (2 + (-1)^n) * y(n), y(n + 1) == (1 / 2) * (2 - (-1)^n) * x(n)], [x, y], n)");
        holds(&mut e, "not free_of(sol, c(1)) and not free_of(sol, c(2)) and free_of(sol, c(3))");
        zero_for_all_n(&mut e, "rewrite([x(n + 1) - (1 / 2) * (2 + (-1)^n) * y(n), y(n + 1) - (1 / 2) * (2 - (-1)^n) * x(n)], sol[1])");
        ev(&mut e, "s3 = rewrite(sol, [c(1) -> 1, c(2) -> 2])");
        ev(&mut e, "x0 = rewrite(x(0), s3[1]); y0 = rewrite(y(0), s3[1])");
        agrees_forward(&mut e, "rewrite([x(n), y(n)], s3[1])", "s => [s[1] + 1, (2 + (-1)^s[1])/2*s[3], (2 - (-1)^s[1])/2*s[2]]", "[x0, y0]");
        holds(&mut e, "head(discrete_plot(eval(rewrite([x(n), y(n)], s3[1])), [n, 0, 20])) === image");
    });
}

#[test]
fn partial_recurrence_in_two_indices() {
    timed("partial recurrence", 120, || {
        let mut e = CoreEngine::new();
        ev(&mut e, "out = r_solve(7 * a(m + 1, n) - 6 * a(m, n + 1) == m, a(m, n), [m, n])");
        ev(&mut e, "g = rewrite(a(m, n), out[1])");
        holds(&mut e, "simplify(g - (m - 7 + (7/6)^n*c(1)(m + n))) === 0");
        holds(&mut e, "simplify(r_solve_value(7 * a(m + 1, n) - 6 * a(m, n + 1) == m, a(m, n), [m, n]) - g) === 0");
        // symbolic check with c(1) an arbitrary function
        ev(&mut e, "outh = r_solve(7 * a(m + 1, n) - 6 * a(m, n + 1) == m, a, [m, n])");
        holds(&mut e, "simplify(rewrite(a(m, n), outh[1]) - g) === 0");
        holds(&mut e, "simplify(expand(rewrite(rewrite(7*a(m + 1, n) - 6*a(m, n + 1) - m, outh[1]), z_^w_ when not free_of(w, n) :> z^rewrite(w, n -> 0)*kk47(z, d(w, n))))) === 0");
        // the recurrence at every grid point for several choices of the arbitrary function, exactly
        for f in ["s => s^2 + 1", "s => 2^s", "s => 1/(s + 1)"] {
            holds(&mut e, &format!("{{ let h = rewrite(rewrite(7*a(i, j) - 6*a(i - 1, j + 1) - (i - 1), outh[1]), c(1) -> ({f})); union(flatten(table(simplify(h), [i, 1, 8], [j, 0, 8]))) === [0] }}"));
        }
        // any first column a(0, n) = v(n) is reached (the arbitrary function is the whole freedom)
        holds(&mut e, "{ let h = rewrite(g, c(1) -> (s => (6/7)^s*(s^3 + 7))); union(table(simplify(rewrite(h, [m -> 0, n -> j]) - j^3), [j, 0, 10])) === [0] }");
        holds(&mut e, "head(discrete_plot3d(eval(rewrite(g, c(1) -> cos)), [m, 0, 10], [n, 0, 10])) === image");
    });
}

#[test]
fn not_handled_stays_unevaluated() {
    timed("declined systems", 60, || {
        let mut e = CoreEngine::new();
        // singular system matrix, inconsistent initial values, a partial recurrence along a non-primitive step
        for src in [
            "r_solve([a(n + 1) == b(n), b(n + 1) == b(n)], [a, b], n)",
            "r_solve([a(n + 1) == b(n), b(n + 1) == a(n), a(0) == 1, a(1) == 1, b(0) == 2], [a, b], n)",
            "r_solve(a(m + 2, n) - a(m, n) == 1, a(m, n), [m, n])",
        ] {
            let out = ev(&mut e, src);
            assert!(out.starts_with("r_solve("), "{src}: {out}");
        }
    });
}
