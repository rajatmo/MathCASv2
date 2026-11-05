//! `solve`, `roots`, `eliminate`, `groebner_basis`: exact forms, solutions substituted back, counts against the
//! reference values, parameters, failures and interrupts.
mod common;
use common::{check, Lcg, Sys};
use std::time::Duration;

/// The definitions the checks use: the number of solutions and the largest residual of the equations.
const PRELUDE: &str = "fn diff(e) = e[1] - e[2]\n\
    fn resid(eqs, xs) = { let s = solve(eqs, xs); [length(s), max(map(sol => max(map(e => abs(n(diff(e) /. sol)), eqs)), s))] }";

/// (count, residual) of `solve(eqs, xs)`.
fn residual(s: &mut Sys, eqs: &str, xs: &str) -> (usize, f64) {
    let out = s.run(&format!("resid({eqs}, {xs})"));
    let inner = out.trim_start_matches('[').trim_end_matches(']');
    let (a, b) = inner.split_once(", ").unwrap_or_else(|| panic!("{eqs}: {out}"));
    (a.parse().unwrap(), b.trim_end_matches('.').parse().unwrap_or_else(|_| panic!("{eqs}: {out}")))
}

#[test]
fn exact_forms() {
    check(&[
        ("solve(x^2 == 1, x)", "[[x -> -1], [x -> 1]]"),
        ("solve(x^2 + 1 == 0, x)", "[[x -> -1im], [x -> 1im]]"),
        ("solve(2*x + 3 == 7, x)", "[[x -> 2]]"),
        ("solve(x^2 == 0, x)", "[[x -> 0], [x -> 0]]"),
        ("solve(x^4 - 5*x^2 + 6 == 0, x)", "[[x -> -3^(1/2)], [x -> -2^(1/2)], [x -> 2^(1/2)], [x -> 3^(1/2)]]"),
        ("solve(x^3 - 3*x + 1 == 0, x)", "[[x -> 2*cos((10/9)*pi)], [x -> 2*cos((4/9)*pi)], [x -> 2*cos((2/9)*pi)]]"),
        (
            "solve(a*x^2 + b*x + c == 0, x)",
            "[[x -> (1/2)*(-b - (-4*a*c + b^2)^(1/2))/a], [x -> (1/2)*(-b + (-4*a*c + b^2)^(1/2))/a]]",
        ),
        (
            "solve(x^5 - x - 1 == 0, x)",
            "[[x -> root(x => -1 - x + x^5, 1)], [x -> root(x => -1 - x + x^5, 2)], [x -> root(x => -1 - x + x^5, 3)], \
             [x -> root(x => -1 - x + x^5, 4)], [x -> root(x => -1 - x + x^5, 5)]]",
        ),
        ("solve([x + y == 3, x - y == 1], [x, y])", "[[x -> 2, y -> 1]]"),
        ("solve([a*x + y == 3, x - y == b], [x, y])", "[[x -> (3 + b)/(1 + a), y -> (3 + -1*a*b)/(1 + a)]]"),
        ("solve([x + y == 1, x + y == 2], [x, y])", "[]"),
        ("solve([x + y + z == 1, x - y == 2], [x, y, z])", "[[x -> (1/2)*(3 - z), y -> (1/2)*(-1 - z)]]"),
        ("solve([x^2 + y^2 == 5, x*y == 2], [x, y])", "[[x -> -2, y -> -1], [x -> -1, y -> -2], [x -> 1, y -> 2], [x -> 2, y -> 1]]"),
        (
            "solve([x^2 == 2, y^2 == 3], [x, y])",
            "[[x -> -2^(1/2), y -> -3^(1/2)], [x -> -2^(1/2), y -> 3^(1/2)], [x -> 2^(1/2), y -> -3^(1/2)], [x -> 2^(1/2), y -> 3^(1/2)]]",
        ),
        ("solve(x/(x - 1) == 2, x)", "[[x -> 2]]"),
        ("solve((x^2 - 1)/(x - 1) == 0, x)", "[[x -> -1]]"),
        ("solve(1/x + 1/(x + 1) == 1, x)", "[[x -> (1/2)*(1 - 5^(1/2))], [x -> (1/2)*(1 + 5^(1/2))]]"),
        ("solve(x == x, x)", "[[]]"),
        ("solve(0 == 1, x)", "[]"),
        ("solve(false, x)", "[]"),
        ("solve(x^2 - 4, x)", "[[x -> -2], [x -> 2]]"),
        ("solve(x^2 + y == 1)", "[[x -> (-(-1 + y))^(1/2)], [x -> -(-(-1 + y))^(1/2)]]"),
    ]);
}

#[test]
fn transcendental_and_radical() {
    check(&[
        ("solve(exp(x) == 2, x)", "[[x -> log(2)]]"),
        ("solve(log(x) == 2, x)", "[[x -> exp(2)]]"),
        ("solve(sin(x) == 1/2, x)", "[[x -> (1/6)*pi]]"),
        ("solve(2^x == 8, x)", "[[x -> 3]]"),
        ("solve(exp(2*x) - 3*exp(x) + 2 == 0, x)", "[[x -> 0], [x -> log(2)]]"),
        ("solve(sqrt(x) == x - 2, x)", "[[x -> 4]]"),
        ("solve(x^(1/3) == 2, x)", "[[x -> 8]]"),
        ("solve(sqrt(x) == -1, x)", "[]"),
        (
            "solve(alpha*T^4 == k^2*Mp^(k - 2)*phi^(k - 2), phi)",
            "[[phi -> (alpha*Mp^-(-2 + k)*T^4*k^-2)^(-2 + k)^-1]]",
        ),
    ]);
}

#[test]
fn roots_eliminate_groebner() {
    check(&[
        ("roots(x^2 - 3*x + 2 == 0, x)", "x == 1 or x == 2"),
        ("roots(x^3 - 3*x^2 + 3*x - 1 == 0, x)", "x == 1 or x == 1 or x == 1"),
        ("roots(x^2 + 1 == 0, x)", "x == -1im or x == 1im"),
        ("roots(x - 5 == 0, x)", "x == 5"),
        ("eliminate([x == y + 1, y^2 == z], y)", "1 + -2*x - z + x^2 == 0"),
        ("eliminate([x^2 + y^2 == 1, y == x + a], x)", "-1 + 2*y^2 + -2*a*y + a^2 == 0"),
        ("eliminate([x == 1, x == 2], x)", "false"),
        ("groebner_basis([x^2 + y^2 - 1, x - y], [x, y])", "[-1 + 2*y^2, x - y]"),
        ("groebner_basis([x + y + z, x*y + y*z + z*x, x*y*z - 1], [x, y, z])", "[-1 + z^3, y*z + y^2 + z^2, x + y + z]"),
        ("groebner_basis([a*x - 1, x*y - b], [x, y])", "[y + -1*a*b, -1 + a*x]"),
    ]);
}

#[test]
fn univariate_substituted_back() {
    let mut s = Sys::new();
    s.run(PRELUDE);
    let mut g = Lcg(7);
    // random integer polynomials of degree 1 to 6: as many roots as the degree, each a root
    for round in 0..60 {
        let d = 1 + round % 6;
        let mut terms = vec![];
        for k in 0..=d {
            let c = if k == d { g.int(1, 5) } else { g.int(-9, 9) };
            terms.push(format!("({c})*x^{k}"));
        }
        let p = terms.join(" + ");
        let (count, r) = residual(&mut s, &format!("[{p} == 0]"), "x");
        assert_eq!(count, d as usize, "{p}");
        assert!(r < 1e-8, "{p}: residual {r}");
    }
}

#[test]
fn systems_against_reference_counts() {
    let mut s = Sys::new();
    s.run(PRELUDE);
    // the solution counts of the reference
    let cases: &[(&str, &str, usize)] = &[
        ("[x^2 + y^2 == 5, x*y == 2]", "[x, y]", 4),
        ("[x^2 - y == 0, y^2 - x == 0]", "[x, y]", 4),
        ("[x + y + z == 0, x*y + y*z + z*x == 0, x*y*z == 1]", "[x, y, z]", 6),
        ("[x^2 + y^2 == 1, x^2 - y == 1/2]", "[x, y]", 4),
        ("[x^3 - 2*x*y + y^2 == 1, x - y^2 == 3]", "[x, y]", 6),
        ("[a^2 + b^2 == 1, a*b == 1/4, c^2 == a]", "[a, b, c]", 8),
        ("[x^2 + y^2 + z^2 == 1, x + y + z == 0, x - y == 0]", "[x, y, z]", 2),
        (
            "[x0 + 2*x1 + 2*x2 + 2*x3 == 1, x0^2 + 2*x1^2 + 2*x2^2 + 2*x3^2 == x0, 2*x0*x1 + 2*x1*x2 + 2*x2*x3 == x1, \
             x1^2 + 2*x0*x2 + 2*x1*x3 == x2]",
            "[x0, x1, x2, x3]",
            8,
        ),
    ];
    for (eqs, xs, want) in cases {
        let (count, r) = residual(&mut s, eqs, xs);
        assert_eq!(count, *want, "{eqs}");
        assert!(r < 1e-9, "{eqs}: residual {r}");
    }
}

#[test]
fn parametric_systems_substituted_back() {
    // generic solutions: the residual simplifies to zero
    let mut s = Sys::new();
    for (eqs, xs) in [
        ("[x*y == 1, x + y == a]", "[x, y]"),
        ("[a*x + b*y == c, d*x + e2*y == f]", "[x, y]"),
        ("[x + y == a, x - y == b, z == x*y]", "[x, y, z]"),
    ] {
        let out = s.run(&format!(
            "let ee = {eqs}; map(sol => map(e => simplify(together((e[1] - e[2]) /. sol)), ee), solve(ee, {xs}))"
        ));
        assert!(out.chars().all(|c| "[0, ]".contains(c)), "{eqs}: {out}");
    }
}

#[test]
fn linear_fifty() {
    let mut s = Sys::new();
    let t = std::time::Instant::now();
    let out = s.run(
        "let xs = map(j => x(j), range(1, 50)); \
         let eqs = map(i => sum(map(j => (mod(i*i*j*j + i*i*i + 7*j, 101) - 50) * x(j), range(1, 50))) == i, range(1, 50)); \
         let s = solve(eqs, xs); \
         [length(s), length(s[1]), count(map(e => simplify(e /. s[1]), eqs), true)]",
    );
    assert_eq!(out, "[1, 50, 50]");
    assert!(t.elapsed() < Duration::from_secs(20), "{:?}", t.elapsed());
}

#[test]
fn failures() {
    check(&[
        ("solve(x^2 > 1, x)", "failure(argument, solve, \"solve takes equations (use reduce for inequalities)\")"),
        (
            "solve(sin(x) + x == 0, x)",
            "failure(unsolved, solve, \"the unknown occurs both inside and outside a function\")",
        ),
        ("solve(x == 1, 2)", "failure(argument, solve, \"a number is not an unknown\")"),
        ("solve()", "failure(argument, solve, \"expected equations and unknowns\")"),
        ("roots([x == 1, y == 2], [x, y])", "failure(argument, roots, \"expected one equation in one unknown\")"),
        ("roots(sin(x) == 0, x)", "failure(argument, roots, \"the equation is not polynomial in the unknown\")"),
    ]);
}

#[test]
fn interrupt() {
    let mut s = Sys::new();
    // a lex basis with parameters (no FGLM) that runs long
    let src = "solve([x^3 + a*y*z - b == 0, y^3 + b*x*z - c == 0, z^3 + c*x*y - a == 0, x*y*z*w == a + b + c + 1], [x, y, z, w])";
    let (r, took) = s.interrupted(src, Duration::from_millis(300));
    if took > Duration::from_millis(300) {
        assert!(took < Duration::from_secs(5), "{took:?}");
        let msg = format!("{r:?}");
        assert!(msg.contains("Stopped") || msg.contains("interrupted"), "{msg}");
    }
    assert_eq!(s.run("solve(x^2 == 4, x)"), "[[x -> -2], [x -> 2]]");
}
