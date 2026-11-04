//! `find_minimum` and `find_maximum`: Brent's method in one variable, BFGS in several (with the symbolic gradient or
//! central differences), polishing by Newton on the gradient, `digits:`, and failures.
mod common;
use common::{check, close, correct_digits, number, Sys};

/// The value and the point of a printed `[f, [x -> a, ...]]`.
fn extremum(s: &mut Sys, src: &str) -> (f64, Vec<f64>) {
    let out = s.run(src);
    let (f, rules) = out
        .strip_prefix('[')
        .and_then(|r| r.split_once(", "))
        .unwrap_or_else(|| panic!("{src}: {out}"));
    let rules = rules.strip_suffix(']').unwrap();
    let xs = common::rule_values(rules).unwrap_or_else(|| panic!("{src}: {out}")).iter().map(|v| number(v)).collect();
    (number(f), xs)
}

#[test]
fn one_variable() {
    let mut s = Sys::new();
    let cases: &[(&str, f64, f64)] = &[
        ("find_minimum(x^2 - 2*x, [x, 3])", -1.0, 1.0),
        // a quartic with two minima: the one near the start (reference values)
        ("find_minimum(x^4 - 3*x^2 + x, [x, 1])", -1.070_230_181_776_154, 1.130_901_122_629_986),
        ("find_minimum(x^4 - 3*x^2 + x, [x, -1])", -3.513_905_038_934_789, -1.300_839_565_941_577),
        ("find_minimum(cos(x), [x, 3])", -1.0, std::f64::consts::PI),
        ("find_minimum(x*log(x), [x, 1])", -(-1f64).exp(), (-1f64).exp()),
        ("find_maximum(sin(x), [x, 1])", 1.0, std::f64::consts::FRAC_PI_2),
        ("find_maximum(x*exp(-x), [x, 3])", (-1f64).exp(), 1.0),
        ("find_minimum(x^2 - 2*x, [x, 3], method: newton)", -1.0, 1.0),
    ];
    for &(src, fmin, xmin) in cases {
        let (f, x) = extremum(&mut s, src);
        assert!(close(f, fmin, 1e-15) && close(x[0], xmin, 1e-14), "{src}: {f}, {x:?}");
    }
    // a kink: no polishing, half the digits
    let (f, x) = extremum(&mut s, "find_minimum(abs(x - 2) + 1, [x, 0])");
    assert!(close(f, 1.0, 1e-7) && close(x[0], 2.0, 1e-7), "{f}, {x:?}");
    // no derivative: Brent alone
    let (f, x) = extremum(&mut s, "rule g(x_ : Real) := (x - 1/3)^2 + 2; find_minimum(g(x), [x, 0])");
    assert!(close(f, 2.0, 1e-15) && close(x[0], 1.0 / 3.0, 1e-7), "{f}, {x:?}");
}

#[test]
fn several_variables() {
    let mut s = Sys::new();
    let cases: &[(&str, f64, &[f64], f64)] = &[
        // Rosenbrock's valley from the classic start
        ("find_minimum((1 - x)^2 + 100*(y - x^2)^2, [[x, -1.2], [y, 1]])", 0.0, &[1.0, 1.0], 1e-14),
        // a coupled quadratic: f = 25/133 at (120/133, 260/133, -3)
        (
            "find_minimum((x - 1)^2 + (y - 2)^2 + (z + 3)^2 + x*y/10, [[x, 0], [y, 0], [z, 0]])",
            25.0 / 133.0,
            &[120.0 / 133.0, 260.0 / 133.0, -3.0],
            1e-15,
        ),
        ("find_maximum(-(x - 1)^2 - (y + 1)^2 + 3, [[x, 0], [y, 0]])", 3.0, &[1.0, -1.0], 1e-15),
        ("find_minimum(x^2 + y^2, [[x, 1], [y, 1]], method: bfgs)", 0.0, &[0.0, 0.0], 1e-15),
        // central differences (no symbolic gradient): BFGS alone, about 2/3 of the digits
        (
            "rule h2(x_ : Real, y_ : Real) := (x - 1)^2 + (y + 2)^2 + x*y/10; find_minimum(h2(x, y), [[x, 0], [y, 0]])",
            -85.0 / 399.0,
            &[440.0 / 399.0, -820.0 / 399.0],
            1e-9,
        ),
    ];
    for &(src, fmin, xmin, tol) in cases {
        let (f, x) = extremum(&mut s, src);
        assert_eq!(x.len(), xmin.len(), "{src}");
        assert!(close(f, fmin, tol), "{src}: {f}");
        for (a, b) in x.iter().zip(xmin) {
            assert!(close(*a, *b, tol), "{src}: {x:?}");
        }
    }
}

#[test]
fn minima_with_balls() {
    let mut s = Sys::new();
    const PI: &str = "3.14159265358979323846264338327950288419716939937510582097494459";
    let out = s.run("find_minimum(cos(x), [x, 3], digits: 30)");
    let x = out.rsplit("x -> ").next().unwrap().trim_end_matches(']');
    assert!(correct_digits(x, PI) && common::significant(x) >= 29, "{out}");
    assert!(out.starts_with("[-1.00000000000000000000000000"), "{out}");
    let out = s.run("find_maximum(sin(x), [x, 1], digits: 40)");
    let x = out.rsplit("x -> ").next().unwrap().trim_end_matches(']');
    // pi/2
    assert!(correct_digits(x, "1.5707963267948966192313216916397514420985846996875529"), "{out}");
    let out = s.run("find_minimum((x - 1)^2 + (y - x)^2 + y^4, [[x, 0], [y, 0]], digits: 25)");
    assert!(out.starts_with("[0."), "{out}");
}

#[test]
fn failures() {
    check(&[
        ("find_minimum(x^2, x)", "failure(argument, find_minimum, 2)"),
        ("find_minimum(x^2, [x, 1, 2])", "failure(argument, find_minimum, 2)"),
        ("find_minimum(x^2, [x, im])", "failure(argument, find_minimum, 2)"),
        ("find_minimum([x^2], [x, 1])", "failure(argument, find_minimum, 1)"),
        ("find_minimum(x^2 == 1, [x, 1])", "failure(argument, find_minimum, 1)"),
        ("find_maximum(x^2, [x, 1], foo: 1)", "failure(unknown_option, foo)"),
        ("find_minimum(x^2, [x, 1], verify: true)", "failure(unknown_option, verify)"),
        ("find_minimum(x^2, [x, 1], method: secant)", "failure(argument, find_minimum, method)"),
        ("find_minimum(x^2 + a, [x, 1])", "failure(not_numeric, a + x^2, [x -> 1.0])"),
        ("rule g(x_ : Real) := x^2; find_minimum(g(x), [x, 1], digits: 20)", "failure(argument, find_minimum, digits)"),
    ]);
    let mut s = Sys::new();
    // unbounded below: the expansion gives up with its last point
    for src in ["find_minimum(x^3, [x, 0])", "find_minimum(exp(x), [x, 0])", "find_maximum(x^2, [[x, 1], [y, 1]])"] {
        let r = s.run(src);
        assert!(r.starts_with("failure(not_converged, find_m"), "{src}: {r}");
    }
}
