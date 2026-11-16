//! Complete integrals of first-order equations by Charpit's method for F(u, p) = 0, by Lagrange's characteristics for
//! quasi-linear equations in two variables, and with conditions that fix the constants; sums of squares of
//! polynomials with inexact coefficients. Every complete integral is substituted into the equation and checked
//! symbolically and at 30 digits at a point with numeric constants; the squares are summed and compared with the
//! polynomial. Each test has a time bound.
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

/// The residual of `pde` (lhs - rhs) at the complete integral for the function `u`, at x = 13/10, y = 7/10 and the
/// constants c(1) = 2/3, c(2) = 5/4, to 30 digits.
fn residual_small(e: &mut CoreEngine, pde: &str, u: &str) {
    holds(
        e,
        &format!(
            "{{ let s = complete_integral({pde}, {u}, [x, y])[1]; let q = {pde}; let r = rewrite(rewrite(q[1] - q[2], s), [x -> 13/10, y -> 7/10, c(1) -> 2/3, c(2) -> 5/4]); abs(n(r, 30)) < 10^-20 }}"
        ),
    );
}

#[test]
fn charpit_equations_free_of_the_variables() {
    timed("Charpit", 120, || {
        let mut e = CoreEngine::new();
        let pde = "d(u(x, y), x) + d(u(x, y), y) == u(x, y)^2";
        assert_eq!(ev(&mut e, &format!("complete_integral({pde}, u(x, y), [x, y])")), "[[u(x, y) -> (-1 - c(1))/(y + c(2) + x*c(1))]]");
        assert_eq!(ev(&mut e, &format!("complete_integral({pde}, u, [x, y])")), "[[u -> ((x, y) => (-1 - c(1))/(y + c(2) + x*c(1)))]]");
        holds(&mut e, &format!("simplify(rewrite({pde}, complete_integral({pde}, u, [x, y])[1]))"));
        residual_small(&mut e, pde, "u");
        // u_x + 2 u_y = u (linear: the characteristics give (y - 2 x - c(1)) exp(x) / c(2))
        residual_small(&mut e, "d(u(x, y), x) + 2*d(u(x, y), y) == u(x, y)", "u");
    });
}

#[test]
fn lagrange_characteristics_of_quasi_linear_equations() {
    timed("Lagrange", 180, || {
        let mut e = CoreEngine::new();
        assert_eq!(ev(&mut e, "complete_integral(d(u(x, y), x) - u(x, y)*d(u(x, y), y) == 0, u(x, y), [x, y])"), "[[u(x, y) -> (c(1) - y)/(x - c(2))]]");
        for pde in [
            "d(w(x, y), x) - y*w(x, y)*d(w(x, y), y) == w(x, y) + x^2 - 1",
            "d(w(x, y), x) - y*w(x, y)*d(w(x, y), y) == x*w(x, y) + x^2 - 1",
            "d(w(x, y), x) - (1/sin(y))*w(x, y)*d(w(x, y), y) == (1/x)*w(x, y) + log(x)",
        ] {
            holds(&mut e, &format!("simplify(rewrite({pde}, complete_integral({pde}, w, [x, y])[1]))"));
            residual_small(&mut e, pde, "w");
            // two independent constants: the Jacobian of (u, u_x) in (c(1), c(2)) is not identically zero
            holds(&mut e, &format!("{{ let s = rewrite(w(x, y), complete_integral({pde}, w(x, y), [x, y])[1]); let j = det([[d(s, c(1)), d(s, c(2))], [d(s, x, c(1)), d(s, x, c(2))]]); abs(n(rewrite(j, [x -> 13/10, y -> 7/10, c(1) -> 2/3, c(2) -> 5/4]), 30)) > 10^-10 }}"));
        }
    });
}

#[test]
fn conditions_fix_the_constants() {
    timed("conditions", 120, || {
        let mut e = CoreEngine::new();
        let src = "complete_integral(d(u(x, y, z), x)^3 == d(u(x, y, z), y) + d(u(x, y, z), z) and u(0, 1, 1) == 1 and u(1, 0, 1) == 1 and u(0, 0, 1) == 1/2, u(x, y, z), [x, y, z])";
        holds(&mut e, &format!("expand(rewrite(u(x, y, z), {src}[1]) - (7 - 3*z + 4*x + 4*y)/8) === 0"));
        // the equation and the three conditions at the answer
        holds(&mut e, &format!("{{ let f = rewrite(u(x, y, z), {src}[1]); expand(d(f, x)^3 - d(f, y) - d(f, z)) == 0 and rewrite(f, [x -> 0, y -> 1, z -> 1]) == 1 and rewrite(f, [x -> 1, y -> 0, z -> 1]) == 1 and rewrite(f, [x -> 0, y -> 0, z -> 1]) == 1/2 }}"));
    });
}

#[test]
fn sums_of_squares_with_inexact_coefficients() {
    timed("inexact sums of squares", 120, || {
        let mut e = CoreEngine::new();
        let f = "9.87 + 1.23*x^2 + 7.89*y^4 + 3.21*x^2*z^6 + 4.56*x*y";
        // the squares summed: every coefficient within 10^-12 of the polynomial's
        holds(&mut e, &format!("{{ let q = polynomial_sum_of_squares_list({f}, [x, y, z]); is_list(q) and every(coefficient_rules(expand({f} - total(map(t => t^2, q))), [x, y, z]), r => abs(r[2]) < 10^-12) }}"));
        holds(&mut e, &format!("every(polynomial_sum_of_squares_list({f}, [x, y, z]), t => every(coefficient_rules(t, [x, y, z]), r => is_inexact(r[2])))"));
        holds(&mut e, "{ let q = polynomial_sum_of_squares_list(2.5*x^2 + 2*x*y + y^2, [x, y]); every(coefficient_rules(expand(2.5*x^2 + 2*x*y + y^2 - total(map(t => t^2, q))), [x, y]), r => abs(r[2]) < 10^-14) }");
        // not a sum of squares (negative at x = 0, y = 1): unevaluated
        assert_eq!(ev(&mut e, "head(polynomial_sum_of_squares_list(1.5*x^4 - 2.5*y^2 + 1, [x, y]))"), "polynomial_sum_of_squares_list");
    });
}
