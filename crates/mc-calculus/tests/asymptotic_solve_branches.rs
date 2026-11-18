//! `asymptotic_solve` at singular points: Newton's polygon for analytic equations, the side x < x0
//! (`direction: from_below`), the scale 1/log(x), and solutions in several variables.
mod common;
use common::Sys;

/// The branches of an analytic equation through a singular start, by the polygon of its Taylor polynomial: each
/// branch makes the equation vanish beyond the order asked.
#[test]
fn analytic_equation_puiseux_branches() {
    let mut s = Sys::new();
    s.run("br = asymptotic_solve(sin(x*y) - y^2 + x == 0, [y, 0], [x, 0, 5]); null");
    assert_eq!(s.run("length(br)"), "2");
    assert_eq!(
        s.run("expand(rewrite(y, br[1]) - (-x^(1/2) + (1/2)*x - (1/8)*x^(3/2) + (1/128)*x^(5/2) - (1/1024)*x^(7/2) - (1/12)*x^4 + (4101/32768)*x^(9/2) - (1/12)*x^5))"),
        "0"
    );
    // the residual of each branch: x = t^2, the equation is O(t^12) (y to t^10, the equation's x y term adds t^2)
    for i in 1..=2 {
        let res = s.run(&format!(
            "{{ let b = rewrite(y, asymptotic_solve(sin(x*y) - y^2 + x == 0, [y, 0], [x, 0, 5])[{i}]); expand(normal(series(rewrite(sin(x*b) - b^2 + x, x -> t^2), t, 0, 11))) }}"
        ));
        assert_eq!(res, "0", "branch {i}");
    }
}

/// `direction: from_below` expands in powers of -x; with `reals` only the real branches there.
#[test]
fn branches_from_below() {
    let mut s = Sys::new();
    assert_eq!(s.run("asymptotic_solve(y^5 - x*y + x^2 == 0, [y, 0], [x, 0, 5], reals, direction: from_below)"), "[[y -> x + x^4]]");
    assert_eq!(s.run("length(asymptotic_solve(y^5 - x*y + x^2 == 0, [y, 0], [x, 0, 5], direction: from_below))"), "5");
    // from above, three real branches (as before)
    assert_eq!(s.run("length(asymptotic_solve(y^5 - x*y + x^2 == 0, [y, 0], [x, 0, 5], reals))"), "3");
}

/// x only through log(x) at 0: a series in 1/log(x).
#[test]
fn logarithmic_scale() {
    let mut s = Sys::new();
    let r = s.run("rewrite(y, asymptotic_solve(sin(y) - y^2 + cos(y)/log(x) == 0, [y, 0], [x, 0, 5])[1])");
    assert_eq!(
        s.run(&format!("expand(rewrite({r}, log(x) -> 1/w) - (-w + w^2 - (5/3)*w^3 + (23/6)*w^4 - (51/5)*w^5))")),
        "0"
    );
}

/// Several variables [[x, y], [0, 0], k]: the terms of total degree up to k.
#[test]
fn several_variables() {
    let mut s = Sys::new();
    assert_eq!(
        s.run("expand(rewrite(z, asymptotic_solve(z^5 - 2*z^3 + 2*z + x^2 - y == 0, [z, 0], [[x, y], [0, 0], 5])[1]) - ((-1/2)*x^2 + (5/64)*y^5 + (1/8)*y^3 + (1/2)*y - (3/8)*x^2*y^2 + (3/8)*y*x^4))"),
        "0"
    );
    s.run("uv = asymptotic_solve(x^2 - u*(y + 1) == y^2 - v*(x + 1) and u^2 - u*(x + 1) == v^2 + v*(y + 1), [[u, v], [0, 0]], [[x, y], [0, 0], 3]); null");
    assert_eq!(s.run("expand(rewrite([u, v], uv[1]) - [(-1/2)*x^3 - (1/2)*y^2 + (1/2)*x^2 + (1/2)*x*y^2, (-1/2)*x^2 - (1/2)*y^3 + (1/2)*y^2 + (1/2)*y*x^2])"), "[0, 0]");
    // a point other than the origin: in powers of x - 1
    let r = s.run("rewrite(z, asymptotic_solve(z^2 + z - x*y == 0, [z, 0], [[x, y], [1, 0], 2])[1])");
    assert_eq!(s.run(&format!("expand({r} - (y + (-1 + x)*y - y^2))")), "0");
}
