//! Families of solutions over the complexes: equations in exponentials of the unknown in a strip of its real part
//! (roots of the equation in y = e^x in an annulus), x == e^(1/x) through x = 1/t, products of exponentials of
//! positive bases, the Jacobi functions (two-parameter lattices); and `roots(..., using: ...)`, zero leading
//! coefficients in other kernels, equations free of the unknowns, inverse functions of several arguments and of
//! functions given on a domain. The values are checked against numbers computed without this system (mpmath at 30
//! digits: Lambert W branches, Jacobi functions, contour counts) and by substitution at 30 digits.
mod common;
use common::Sys;
use std::time::{Duration, Instant};

/// `src` (a modulus at 30 digits) is below 10^-25.
fn assert_small(s: &mut Sys, src: &str) {
    let v = s.run(&format!("{src} < 10^-25"));
    assert_eq!(v, "true", "{src}");
}

#[test]
fn exponentials_in_a_strip_give_the_roots_in_the_annulus() {
    let mut s = Sys::new();
    // y = e^x: y cos(y) - y - 3 has three roots in |y| <= e and none in |y| < 1 (contour integrals, mpmath)
    s.run("sv = solve_values(cos(exp(x)) == 3 * exp(-x) + 1 and (0 <= re(x) and re(x) <= 1), x)");
    assert_eq!(s.run("length(sv)"), "3");
    for k in ["-1", "0", "2"] {
        for j in 1..=3 {
            let m = format!("n(rewrite(sv[{j}][1], c(1) -> {k}), 40)");
            assert_small(&mut s, &format!("abs(cos(exp({m})) - 3 exp(-{m}) - 1)"));
            let re = s.number(&format!("n(re({m}), 30)"));
            assert!((0.0..=1.0).contains(&re), "{m}: re = {re}");
        }
    }
    // the real member, mpmath: x = 0.718553377557305280... + pi i
    assert!((s.number("n(re(rewrite(sv[1][1], c(1) -> 0)))") - 0.718_553_377_557_305_3).abs() < 1e-14);
    assert!((s.number("n(im_part(rewrite(sv[1][1], c(1) -> 0)))") - std::f64::consts::PI).abs() < 1e-14);
    // the other two: 0.54654078399745... -+ 1.11114864249670... i
    assert!((s.number("n(re(rewrite(sv[2][1], c(1) -> 0)))") - 0.546_540_783_997_452_5).abs() < 1e-15);
    // a narrower strip holds none of them (all real parts exceed 1/2); strict and non-strict bounds
    assert_eq!(s.run("solve_values(cos(exp(x)) == 3 * exp(-x) + 1 and (0 < re(x) and re(x) < 1/2), x)"), "[]");
    assert_eq!(s.run("length(n_solve_values(cos(exp(x)) == 3 * exp(-x) + 1 and (0 <= re(x) and re(x) <= 1), x))"), "3");
    let r = s.run("reduce(cos(exp(x)) == 3 * exp(-x) + 1 and (0 <= re(x) and re(x) <= 1), x)");
    assert!(r.starts_with("element(c(1), integers) and (x == log(root("), "{r}");
    // a root ON the bound (e^x == e: y = e on |y| = e): kept by <=, dropped by <
    let on = s.run("solve_values(exp(x) == e and 0 <= re(x) and re(x) <= 1, x)");
    assert!(on.starts_with("[conditional_expression(1 + 2im*pi*c(1), "), "{on}");
    assert_eq!(s.run("solve_values(exp(x) == e and 0 <= re(x) and re(x) < 1, x)"), "[]");
}

#[test]
fn reciprocal_exponent_and_exponential_products() {
    let mut s = Sys::new();
    // x = 1/W_k(1) for every branch k (mpmath: k = 3 gives -0.00947983328053989170443... - 0.0568525725421056375947...i)
    s.run("sv = solve_values(x == e^(1 / x) and abs(x) < 5, x)");
    assert_eq!(s.run("length(sv)"), "1");
    assert!((s.number("n(re(rewrite(sv[1][1], c(1) -> 3)))") + 0.009_479_833_280_539_892).abs() < 1e-15);
    assert!((s.number("n(im_part(rewrite(sv[1][1], c(1) -> 3)))") + 0.056_852_572_542_105_64).abs() < 1e-15);
    assert!((s.number("n(rewrite(sv[1][1], c(1) -> 0))") - 1.763_222_834_351_896_7).abs() < 1e-14);
    for k in -3..=3 {
        assert_small(&mut s, &format!("{{ let v = n(rewrite(sv[1][1], c(1) -> {k}), 40); abs(v - e^(1/v)) }}"));
    }
    // without the condition: the principal branch, as documented
    assert_eq!(s.run("solve(x == e^(1 / x), x)"), "[[x -> product_log(1)^-1]]");
    // 4^(x^2) 2^x == 8: 2 x^2 log 2 + x log 2 == log 8 + 2 pi i k (mpmath: every member of k = -3..3 to 30 digits)
    s.run("sv = solve_values(4^x^2 * 2^x == 8, x)");
    assert_eq!(s.run("length(sv)"), "2");
    for k in -3..=3 {
        for j in 1..=2 {
            assert_small(&mut s, &format!("{{ let v = n(rewrite(sv[{j}][1], c(1) -> {k}), 40); abs(4^v^2 * 2^v - 8) }}"));
        }
    }
    let r = s.run("reduce(4^x^2 * 2^x == 8, x)");
    assert!(r.starts_with("element(c(1), integers) and (x == "), "{r}");
}

#[test]
fn jacobi_functions_have_two_families_on_their_lattice() {
    let mut s = Sys::new();
    // sn(u) = 1 only at K (a double point): one family K + 4 K c(1) + 2 i K' c(2)
    assert_eq!(
        s.run("reduce(jacobi_sn(x, y) == 1, x)"),
        "element(c(1), integers) and element(c(2), integers) and x == elliptic_k(y) + 2im*c(2)*elliptic_k(1 - y) + 4*c(1)*elliptic_k(y)"
    );
    // cs(u, 3) = 1 + i: u0 and 2 i K' - u0 on the lattice 2K, 4iK' (mpmath ellipfun: |cs - (1 + i)| < 4e-30 for
    // c(1) in -1, 0, 2 and c(2) in -1, 0, 1)
    s.run("sv = solve_values(jacobi_cs(x, 3) == 1 + im, x)");
    assert_eq!(s.run("length(sv)"), "2");
    for (a, b) in [(-1, -1), (0, 0), (2, 1), (1, -1)] {
        for j in 1..=2 {
            assert_small(&mut s, &format!("abs(jacobi_cs(n(rewrite(sv[{j}][1], [c(1) -> {a}, c(2) -> {b}]), 40), 3) - 1 - im)"));
        }
    }
    for f in ["sn", "cn", "dn", "cd", "sd", "nd", "dc", "nc", "sc", "ns", "ds", "cs"] {
        s.run(&format!("sv = solve_values(jacobi_{f}(x, 3/10) == 2/7 + im/5, x)"));
        assert_eq!(s.run("length(sv)"), "2", "{f}");
        for (a, b) in [(0, 0), (1, -1), (-2, 1)] {
            for j in 1..=2 {
                // (machine precision: the inverse functions are not evaluated to 40 digits at every complex value)
                let r = s.run(&format!("abs(jacobi_{f}(n(rewrite(sv[{j}][1], [c(1) -> {a}, c(2) -> {b}])), 3/10) - 2/7 - im/5) < 10^-9"));
                assert_eq!(r, "true", "{f} {j} {a} {b}");
            }
        }
    }
    // over the reals the families are not the answer: the call fails as before, quickly
    let t = Instant::now();
    let r = s.run("solve(jacobi_sn(x, 1/2) == 1/3, x, reals)");
    assert!(r.starts_with("failure("), "{r}");
    assert!(t.elapsed() < Duration::from_secs(20));
}

#[test]
fn roots_using_and_degenerate_coefficients() {
    let mut s = Sys::new();
    let r = s.run("roots(x^2 - a * x + 2 == 0, x, using: roots(a^2 - 5 == 0, a))");
    assert!(r.contains(") and a == 5^(1/2) or (") && r.ends_with(") and a == -5^(1/2)"), "{r}");
    // each pair satisfies the equation (exactly)
    assert_eq!(s.run("r = roots(x^2 - a * x + 2 == 0, x, using: a == 3 or a == 7); r"), "(x == 1 or x == 2) and a == 3 or (x == (1/2)*(7 - 41^(1/2)) or x == (1/2)*(7 + 41^(1/2))) and a == 7");
    assert!(s.run("roots(x^2 == 1, x, using: 3)").starts_with("failure(argument"));
    // 2 log(2) - log(4) is zero: a linear equation
    assert_eq!(s.run("reduce(2 * log(2) * x^2 - log(4) * x^2 + x - 1 == 0, x)"), "x == 1");
    assert_eq!(s.run("solve(2 * log(2) * x^2 - log(4) * x^2 + x - 1 == 0, x)"), "[[x -> 1]]");
    // a nonzero coefficient is kept
    assert_eq!(s.run("solve(log(4) * x^2 - log(2) * x^2 == 0, x)"), "[[x -> 0], [x -> 0]]");
    // an equation free of the unknown (q == 0, generic q) has no solution, whatever the others are
    assert_eq!(s.run("solve(sin(x)^2 + tan(x)^2 == 0 and q == 0, x)"), "[]");
}

#[test]
fn inverse_functions_of_several_arguments_and_on_a_domain() {
    let mut s = Sys::new();
    assert_eq!(
        s.run("g = inverse_function(&(&1^2 + log(&2) - 1), 2, 2); g"),
        "&(conditional_expression(exp(1 + &2 - &1^2), -pi < im_part(1 + &2 - &1^2) and im_part(1 + &2 - &1^2) <= pi))"
    );
    // f(3/10, g(3/10, 7/10)) == 7/10; a value with im(log y) = 4 > pi has no preimage
    assert_eq!(s.run("simplify((3/10)^2 + log(g(3/10, 7/10)) - 1)"), "7/10");
    assert_eq!(s.run("g(1, 4 im)"), "undefined");
    assert_eq!(s.run("h = inverse_function(&(conditional_expression(&1^2 + 2, &1 > 0))); h"), "&(conditional_expression((-2 + &1)^(1/2), &1 > 2))");
    // on the bound: 2 = f(0) and 0 is not in the domain
    assert_eq!(s.run("[h(3), h(6), h(2)]"), "[1, 2, undefined]");
}
