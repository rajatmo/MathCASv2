//! `n_minimize` and its relatives, and `find_minimum` with constraints: linear programs (exact vertices),
//! integers, regions, vector variables, disjunctions, the global search, the KKT polish, and the infeasible and
//! unbounded cases.
mod common;
use common::{close, number, Sys};

/// The value and the point of a printed `[f, [x -> a, ...]]`.
fn extremum(s: &mut Sys, src: &str) -> (f64, Vec<f64>) {
    let out = s.run(src);
    let (f, rules) = out.strip_prefix('[').and_then(|r| r.split_once(", ")).unwrap_or_else(|| panic!("{src}: {out}"));
    let rules = rules.strip_suffix(']').unwrap();
    let xs = common::rule_values(rules).unwrap_or_else(|| panic!("{src}: {out}")).iter().map(|v| number(v)).collect();
    (number(f), xs)
}

#[test]
fn linear_programs_are_exact() {
    let mut s = Sys::new();
    assert_eq!(s.run("n_minimize([x + y, x + 2*y >= 3, x >= -1], [x, y])"), "[1.0, [x -> -1.0, y -> 2.0]]");
    assert_eq!(s.run("find_minimum([x + y, 3*x + 2*y >= 7 and x >= 0 and y >= 0], [x, y])"), "[2.3333333333333335, [x -> 2.3333333333333335, y -> 0.0]]");
    assert_eq!(s.run("n_max_value([-x - y, x + 2*y >= 3, x >= -1], [x, y])"), "-1.0");
    assert_eq!(s.run("n_arg_min([x + y, x + 2*y >= 3, x >= -1], [x, y])"), "[-1.0, 2.0]");
    // integers by branch and bound, printed as integers
    assert_eq!(s.run("n_minimize([x + y, x + 2*y >= 3, x >= -2], [element(x, integers), element(y, integers)])"), "[1.0, [x -> -2, y -> 3]]");
    let (f, x) = extremum(&mut s, "find_minimum([x + y, 3*x + 2*y >= 7 and x >= 0 and y >= 0 and element(x, integers)], [x, y])");
    assert!(close(f, 2.5, 1e-15) && x == vec![2.0, x[1]] && close(x[1], 0.5, 1e-15), "{f} {x:?}");
}

#[test]
fn vector_variables_and_comparisons() {
    let mut s = Sys::new();
    assert_eq!(s.run("n_minimize([dot([1, 1], v), vector_greater_equal([dot([[1, 2], [1, 0]], v), [3, -1]])], v)"), "[1.0, [v -> [-1.0, 2.0]]]");
    assert_eq!(s.run("n_minimize([-total(v), vector_less_equal([dot([1, 2], v), -3])], element(v, vectors(2, nonpositive_reals)))"), "[1.5, [v -> [0.0, -1.5]]]");
    // the held arguments: dot(m, x) + [a, b] is formed after x is a vector
    assert_eq!(s.run("n_arg_min([dot([1, 1], x), vector_greater_equal([dot([[1, 2], [1, 0]], x) + [-3, 1], 0])], x)"), "[-1.0, 2.0]");
    assert_eq!(s.run("vector_greater_equal([[3, 2], [1, 2]])"), "true");
    assert_eq!(s.run("vector_less([[3, 2], [1, 2]])"), "false");
    // the norm cone: |(x, y)| <= 3
    let (f, _) = extremum(&mut s, "n_minimize([x + y, vector_greater_equal([[x, y, 3], 0], [\"NormCone\", 3])], [x, y])");
    assert!(close(f, -3.0 * 2f64.sqrt(), 1e-14), "{f}");
}

#[test]
fn nonlinear_and_global() {
    let mut s = Sys::new();
    // the global minimum of a quartic with two minima, to machine precision
    let (f, x) = extremum(&mut s, "n_minimize(x^4 - 3*x^2 - x, x)");
    assert!(close(f, -3.513_905_038_934_789, 1e-15) && close(x[0], 1.300_839_565_941_577, 1e-14), "{f} {x:?}");
    // regions and constraints met exactly by the KKT polish: -sqrt(2) on the disk, not -1.41421523...
    let (f, x) = extremum(&mut s, "n_minimize(x + y, element([x, y], disk()))");
    assert!(close(f, -(2f64.sqrt()), 1e-15) && close(x[0], -(0.5f64.sqrt()), 1e-15), "{f} {x:?}");
    let (f, x) = extremum(&mut s, "n_minimize([x^2 - (y - 1)^2, x^2 + y^2 <= 4], [x, y])");
    assert!(close(f, -9.0, 1e-15) && x[0] == 0.0 && close(x[1], -2.0, 1e-15), "{f} {x:?}");
    let (f, x) = extremum(&mut s, "n_minimize([x + 2*y, x^2 + 2*y^2 <= 3, x + y == 2, x >= 1], [x, y])");
    assert!(close(f, 7.0 / 3.0, 1e-15) && close(x[0], 5.0 / 3.0, 1e-15), "{f} {x:?}");
    // a disjunction: the better of the two disks
    let (f, _) = extremum(&mut s, "n_minimize([x + y, x^2 + y^2 <= 1 or (x + 2)^2 + (y + 2)^2 <= 1], [x, y])");
    assert!(close(f, -4.0 - 2f64.sqrt(), 1e-15), "{f}");
    // a region with variables in it: the smallest disk around three points
    let (f, x) = extremum(&mut s, "find_minimum([r, element([0, 0] | [1, 0] | [0, 1], disk([a, b], r))], [a, b, r])");
    assert!(close(f, 0.5f64.sqrt(), 1e-15) && close(x[0], 0.5, 1e-14), "{f} {x:?}");
    // a nonsmooth objective: Nelder-Mead after BFGS
    let (f, _) = extremum(&mut s, "n_minimize(abs(x + 1) + abs(x + 1.01) + abs(y + 1), [x, y])");
    assert!(close(f, 0.01, 1e-13), "{f}");
    // value and position forms of the local search
    assert_eq!(s.run("find_min_value(2*x^2 - 3*x + 5, x)"), "3.875");
    assert_eq!(s.run("find_arg_max(-2*x^2 - 3*x + 5, x)"), "[-0.75]");
    assert_eq!(s.run("n_arg_min(2*x^2 - 3*x + 5, x)"), "0.75");
}

#[test]
fn infeasible_and_unbounded() {
    let mut s = Sys::new();
    assert_eq!(s.run("n_minimize([x, x >= 1 and x <= 0], x)"), "[inf, [x -> indeterminate]]");
    assert_eq!(s.run("n_minimize([x, x <= 0], x)"), "[-inf, [x -> indeterminate]]");
    assert_eq!(s.run("n_max_value([x, x <= 0, x >= 3], x)"), "-inf");
}

#[test]
fn find_minimum_options() {
    let mut s = Sys::new();
    // goals beyond machine precision: still the machine-precision maximum of sin(x/2)
    let (f, x) = extremum(&mut s, "find_maximum(sin(x / 2), [x, 1], accuracy_goal: 20, precision_goal: 18)");
    assert!(f == 1.0 && close(x[0], std::f64::consts::PI, 1e-15), "{f} {x:?}");
    // working_precision is digits; gradient and monitors are accepted
    assert!(s.run("find_maximum(sin(x / 2), [x, 1], working_precision: 30)").contains("3.14159265358979323846264338"));
    let (f, _) = extremum(&mut s, "find_minimum(sin(x)*sin(2*y), [[x, 2], [y, 2]], gradient: [cos(x)*sin(2*y), 2*cos(2*y)*sin(x)], method: [\"Newton\"])");
    assert!(close(f, -1.0, 1e-15), "{f}");
}
