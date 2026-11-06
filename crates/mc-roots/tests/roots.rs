//! `find_root` on classic problems with known answers: one variable (Newton, secant, Brent, constrained Newton),
//! complex roots, systems (Newton, Broyden), the forms of the arguments, and failures. Reference values are exact
//! (radicals, pi) or agree with the reference system to the digits given.
mod common;
use common::{check, close, complex, number, Sys};

const SQRT2: f64 = std::f64::consts::SQRT_2;

#[test]
fn one_variable_newton() {
    let mut s = Sys::new();
    let cases: &[(&str, f64)] = &[
        ("find_root(x^2 - 2, [x, 1])", SQRT2),
        ("find_root(x^2 - 2, [x, -1])", -SQRT2),
        // the Dottie number
        ("find_root(cos(x) == x, [x, 1])", 0.7390851332151607),
        ("find_root(sin(x), [x, 3])", std::f64::consts::PI),
        // Newton's own example
        ("find_root(x^3 - 2*x - 5, [x, 2])", 2.0945514815423265),
        // Kepler's equation E - e sin E = M with e = 1/2, M = 1
        ("find_root(x - sin(x)/2 == 1, [x, 1])", 1.4987011335178483),
        ("find_root(x^5 - x - 1, [x, 1])", 1.1673039782614187),
        // the omega constant, W(1)
        ("find_root(x*exp(x) == 1, [x, 1])", 0.5671432904097838),
        ("find_root(exp(x) - 3, [x, 10])", 3f64.ln()),
        ("find_root(log(x) == 1, [x, 1])", std::f64::consts::E),
        ("find_root(atan(x) - 1, [x, 0])", 1f64.tan()),
        // a flat start: the damping keeps Newton from overshooting to the other root
        ("find_root(x^3 - x - 1, [x, 0.6])", 1.324717957244746),
        // steep and badly scaled
        ("find_root(1e10*(x - 1), [x, 0])", 1.0),
        ("find_root(x - 1e10, [x, 0])", 1e10),
        // numeric expressions as starting values
        ("find_root(x^2 - 2, [x, sqrt(3)])", SQRT2),
        ("find_root(sin(x), [x, pi - 1/10])", std::f64::consts::PI),
        // constants inside the function
        ("find_root(x^2 - pi, [x, 1])", std::f64::consts::PI.sqrt()),
        // a double root converges linearly to the goals
        ("find_root((x - 1)^2, [x, 3])", 1.0),
    ];
    for &(src, want) in cases {
        let v = s.reals(src);
        let tol = if src.contains("(x - 1)^2") { 1e-7 } else { 4e-16 };
        assert!(v.len() == 1 && close(v[0], want, tol), "{src}: {v:?}, want {want}");
    }
}

#[test]
fn printed_results_substitute() {
    check(&[
        ("find_root(x^2 - 2, [x, 1])", "[x -> 1.4142135623730951]"),
        ("rewrite(x^2, find_root(x^2 - 2, [x, 1]))", "2.0000000000000004"),
        ("r = find_root(x^3 - 8, [x, 1]); rewrite(x + 1, r)", "3.0"),
        ("find_root(x - 3, [x, 0])", "[x -> 3.0]"),
        // an exact start that is already the root
        ("find_root(x^2 - 4, [x, 2])", "[x -> 2.0]"),
    ]);
}

#[test]
fn secant_and_brackets() {
    let mut s = Sys::new();
    let cases: &[(&str, f64)] = &[
        ("find_root(x^3 - 2*x - 5, [x, 2, 3])", 2.0945514815423265),
        ("find_root(cos(x) - x, [x, 0, 1])", 0.7390851332151607),
        ("find_root(x^2 - 2, [x, 1], method: secant)", SQRT2),
        // Brent's method in a bracket with a sign change
        ("find_root(x^3 - 2*x - 5, [x, 2, 2, 3])", 2.0945514815423265),
        ("find_root(exp(x) - 3, [x, 0, 0, 2])", 3f64.ln()),
        ("find_root(cos(x) == x, [x, 0.5, 0, 1])", 0.7390851332151607),
        ("find_root(x^2 - 2, [x, 1, 2, 0])", SQRT2),
        ("find_root(x - 1/3, [x, 0, 0, 1], method: brent)", 1.0 / 3.0),
        // gap G71: a bracketing method with [x, a, b] brackets [a, b]; the common names, as symbols or strings
        ("find_root(cos(x) - x, [x, 0, 1], method: brent)", 0.7390851332151607),
        ("find_root(cos(x) - x, [x, 0, 1], method: \"Brent\")", 0.7390851332151607),
        ("find_root(cos(x) - x, [x, 0, 1], method: bisection)", 0.7390851332151607),
        ("find_root(cos(x) - x, [x, 0.5], method: \"Newton\")", 0.7390851332151607),
        ("find_root(cos(x) - x, [x, 0.5], method: automatic)", 0.7390851332151607),
        // a root at an end of the bracket
        ("find_root(x - 1, [x, 0, 0, 1])", 1.0),
        // no sign change: Newton constrained to the bracket finds the root at 0.99, not the one at 1.01 beyond
        // the start's basin
        ("find_root((x - 1)^2 - 0.0001, [x, 0.6, 0.5, 1.5])", 0.99),
        // no symbolic derivative (a function defined only for reals): the secant fallback
        ("rule g(x_ : Real) := x^3 - 2; find_root(g(x), [x, 1])", 2f64.cbrt()),
        ("find_root(abs(x) - 1, [x, 0.5])", 1.0),
    ];
    for &(src, want) in cases {
        let v = s.reals(src);
        assert!(v.len() == 1 && close(v[0], want, 4e-16), "{src}: {v:?}, want {want}");
    }
}

#[test]
fn complex_roots() {
    let mut s = Sys::new();
    let cases: &[(&str, (f64, f64))] = &[
        ("find_root(x^2 + 1, [x, 1 + im])", (0.0, 1.0)),
        ("find_root(x^2 + 1, [x, 1 - im])", (0.0, -1.0)),
        ("find_root(x^3 - 1, [x, -1 + im])", (-0.5, 3f64.sqrt() / 2.0)),
        ("find_root(x^3 - 1, [x, -1 - im])", (-0.5, -(3f64.sqrt()) / 2.0)),
        // exp(z) = -1 at i pi
        ("find_root(exp(x) + 1, [x, 3*im])", (0.0, std::f64::consts::PI)),
        // a complex system
        ("find_root([x + y - 2, x*y - 2], [[x, 1 + im], [y, 1 - im]])", (1.0, 1.0)),
    ];
    for &(src, (re, im)) in cases {
        let v = s.rules(src);
        let (a, b) = complex(&v[0]);
        assert!(close(number(&a), re, 1e-15) && close(number(&b), im, 1e-15), "{src}: {v:?}");
    }
}

#[test]
fn systems() {
    let mut s = Sys::new();
    let r3 = 3f64.sqrt();
    let cases: &[(&str, &[f64], f64)] = &[
        // a circle and a hyperbola: x = sqrt(2 + sqrt 3), y = sqrt(2 - sqrt 3)
        ("find_root([x^2 + y^2 - 4, x*y - 1], [[x, 2], [y, 0.5]])", &[(2.0 + r3).sqrt(), (2.0 - r3).sqrt()], 1e-15),
        ("find_root([x^2 + y^2 == 4, x*y == 1], [[x, 2], [y, 0.5]], method: broyden)", &[(2.0 + r3).sqrt(), (2.0 - r3).sqrt()], 1e-14),
        // three equations (a textbook example): x = 1/2, y = 0, z = -pi/6
        (
            "find_root([3*x - cos(y*z) - 1/2, x^2 - 81*(y + 0.1)^2 + sin(z) + 1.06, exp(-x*y) + 20*z + (10*pi - 3)/3], [[x, 0.1], [y, 0.1], [z, -0.1]])",
            &[0.5, 0.0, -std::f64::consts::FRAC_PI_6],
            1e-15,
        ),
        // Powell's badly scaled problem (reference values)
        ("find_root([10^4*x*y - 1, exp(-x) + exp(-y) - 1.0001], [[x, 0], [y, 1]])", &[1.0981593296998174e-5, 9.106146739866524], 1e-13),
        // a non-differentiable residual: Broyden's fallback after the Jacobian of abs
        ("find_root([abs(x) - 1, y - x], [[x, 0.5], [y, 0]])", &[1.0, 1.0], 1e-15),
        // a linear system is one Newton step
        ("find_root([x + 2*y - 5, 3*x - y - 1], [[x, 0], [y, 0]])", &[1.0, 2.0], 1e-15),
        // one equation as a system
        ("find_root([x^2 - 2], [[x, 1]])", &[SQRT2], 1e-16),
    ];
    for &(src, want, tol) in cases {
        let v = s.reals(src);
        assert_eq!(v.len(), want.len(), "{src}");
        for (x, w) in v.iter().zip(want) {
            assert!(close(*x, *w, tol), "{src}: {v:?}, want {want:?}");
        }
    }
    // a system without symbolic derivatives at all
    let v = s.reals("rule q(x_ : Real, y_ : Real) := x^2 + y^2 - 4; find_root([q(x, y), x - y], [[x, 1], [y, 2]])");
    assert!(close(v[0], SQRT2, 1e-14) && close(v[1], SQRT2, 1e-14), "{v:?}");
}

#[test]
fn failures() {
    check(&[
        // no real root: the iteration stops at the stationary point of |f|
        ("find_root(x^2 + 1, [x, 1])", "failure(not_converged, find_root, [x -> 0.0], 1.0)"),
        ("find_root(x^2 - 2, [x, 1], max_iterations: 2)", "failure(not_converged, find_root, [x -> 1.4166666666666667], 0.08333333333333326)"),
        // a pole in the bracket: Brent's method bisects towards it until the bracket is within the goals, and
        // the residual (far larger than at the bracket's ends) refuses it
        ("find_root(tan(x), [x, 1, 1, 2])", "failure(not_converged, find_root, [x -> 1.5707963398388178], 8.831496201011646e-9)"),
        // no sign change and the root outside the bracket
        ("find_root(exp(x) - 3, [x, 0.5, 0, 1])", "failure(not_converged, find_root, [x -> 1.0], 0.5)"),
        // a free symbol
        ("find_root(x^2 - a, [x, 1])", "failure(not_numeric, -a + x^2, [x -> 1.0])"),
        ("find_root(1/(x - 1), [x, 0, 0, 2])", "failure(not_numeric, (-1 + x)^-1, [x -> 1.0])"),
        // malformed arguments
        ("find_root(x^2 - 2, x)", "failure(argument, find_root, 2)"),
        ("find_root(x^2 - 2, [1, 1])", "failure(argument, find_root, 2)"),
        ("find_root(x^2 - 2, [x, a])", "failure(argument, find_root, 2)"),
        ("find_root(x^2 - 2, [x, 1, 2, 3, 4])", "failure(argument, find_root, 2)"),
        ("find_root(x^2 - 2, [x, 1, 1, 1])", "failure(argument, find_root, 2)"),
        ("find_root(x^2 - 2, [x, im, 0, 1])", "failure(argument, find_root, 2)"),
        ("find_root([x - 1, y], [[x, 0], [x, 1]])", "failure(argument, find_root, 2)"),
        ("find_root([x - 1], [[x, 0], [y, 1]])", "failure(argument, find_root, 1)"),
        ("find_root(x - 1, [[x, 0], [y, 1]])", "failure(argument, find_root, 1)"),
        ("find_root([x - 1], [x, 0])", "failure(argument, find_root, 1)"),
        ("find_root([[x - 1], y], [[x, 0], [y, 0]])", "failure(argument, find_root, 1)"),
        ("find_root(x)", "find_root(x)"),
        // options
        ("find_root(x^2 - 2, [x, 1], foo: 3)", "failure(unknown_option, foo)"),
        ("find_root(x^2 - 2, [x, 1], digits: 0)", "failure(argument, find_root, digits)"),
        ("find_root(x^2 - 2, [x, 1], digits: 1.5)", "failure(argument, find_root, digits)"),
        ("find_root(x^2 - 2, [x, 1], max_iterations: 0)", "failure(argument, find_root, max_iterations)"),
        ("find_root(x^2 - 2, [x, 1], accuracy_goal: -1)", "failure(argument, find_root, accuracy_goal)"),
        ("find_root(x^2 - 2, [x, 1], method: magic)", "failure(argument, find_root, method)"),
        ("find_root(x^2 - 2, [x, 1], method: brent)", "failure(argument, find_root, method)"),
        ("find_root(x^2 - 2, [x, 1], verify: true)", "failure(argument, find_root, verify)"),
        ("find_root(x^2 - 2, [x, 1], verify: 3)", "failure(argument, find_root, verify)"),
        // failure arguments propagate
        ("find_root(1/0, [x, 1])", "failure(division_by_zero)"),
    ]);
}

#[test]
fn goals() {
    let mut s = Sys::new();
    // loose goals stop early; the result is still within them
    let v = s.reals("find_root(cos(x) - x, [x, 1], accuracy_goal: 3, precision_goal: 3)");
    assert!(close(v[0], 0.7390851332151607, 1e-3), "{v:?}");
    let v = s.reals("find_root(x^2 - 2, [x, 1], precision_goal: inf, accuracy_goal: 12)");
    assert!(close(v[0], SQRT2, 1e-15), "{v:?}");
    // max_steps is a synonym of max_iterations
    assert!(s.run("find_root(x^2 - 2, [x, 1], max_steps: 1)").starts_with("failure(not_converged"));
}
