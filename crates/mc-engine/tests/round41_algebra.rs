//! Round 41: strong Groebner bases over the integers and bases over rational functions; root sums by structure
//! (linearity, multiplicities, parameters, logarithms) and term by term in a parameter; interpolation in several
//! variables; generating functions of quasi-polynomial sequences; elimination with inequations.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// The value of `src` against the canonical form of `expected` (both printed the same way).
fn same(e: &mut CoreEngine, src: &str, expected: &str) {
    let got = ev(e, src);
    let want = ev(e, expected);
    assert_eq!(got, want, "{src}");
}

#[test]
fn strong_bases_over_the_integers() {
    let mut e = CoreEngine::new();
    ev(&mut e, "polys = [a*x^2 + 5*y - 1, 2*x + x*y - y^2]");
    ev(&mut e, "gb = groebner_basis(polys, [x, y], coefficient_domain: integers)");
    same(
        &mut e,
        "gb",
        "[-4 + 5*y^3 + 16*y + 19*y^2 + a*y^4, -2 + 5*y^2 + 9*y + a*y^3 - 2*a*y^2 + 4*a*x, -y^2 + 2*x + x*y, -1 + 5*y + a*x^2]",
    );
    // the same ideal: over Q, and modulo primes (also those dividing a leading coefficient)
    assert_eq!(ev(&mut e, "groebner_basis(gb, [x, y]) === groebner_basis(polys, [x, y])"), "true");
    assert_eq!(
        ev(&mut e, "map(p => groebner_basis(gb, [x, y, a], modulus: p) === groebner_basis(polys, [x, y, a], modulus: p), [2, 3, 5, 7])"),
        "[true, true, true, true]"
    );
    assert_eq!(ev(&mut e, "polynomial_reduce(poly, gb, [x, y], coefficient_domain: integers)"), "[[0, 0, 0, 0], poly]");
    assert_eq!(ev(&mut e, "polynomial_reduction(poly, gb, [x, y], coefficient_domain: integers)"), "poly");
    // variations: an ideal containing the integer 11, and <2x, 3y> (which contains x y but not x or y)
    ev(&mut e, "q = [4*x^2 + 3*y, 6*x*y - 1, 10*y^2 + x]");
    ev(&mut e, "zq = groebner_basis(q, [x, y], coefficient_domain: integers)");
    same(&mut e, "zq", "[11, 9 + y^3, x + 10*y^2]");
    assert_eq!(
        ev(&mut e, "map(p => groebner_basis(zq, [x, y], modulus: p) === groebner_basis(q, [x, y], modulus: p), [2, 3, 5, 7, 11])"),
        "[true, true, true, true, true]"
    );
    assert_eq!(ev(&mut e, "map(g => polynomial_reduce(g, zq, [x, y], coefficient_domain: integers)[2], q)"), "[0, 0, 0]");
    same(&mut e, "groebner_basis([2*x, 3*y], [x, y], coefficient_domain: integers)", "[3*y, 2*x, x*y]");
    // the integer polynomial x y is in the ideal: -y (2x) + x (3y)
    same(&mut e, "expand(-y*(2*x) + x*(3*y))", "x*y");
    // rational coefficients are not integer polynomials
    assert!(ev(&mut e, "groebner_basis([x/2 - 1], [x], coefficient_domain: integers)").starts_with("failure(argument"));
}

#[test]
fn bases_over_rational_functions() {
    let mut e = CoreEngine::new();
    ev(&mut e, "polys = [a*x^2 + 5*y - 1, 2*x + x*y - y^2]");
    ev(&mut e, "gb = groebner_basis(polys, [x, y], coefficient_domain: rational_functions)");
    same(&mut e, "gb", "[-4 + 5*y^3 + 16*y + 19*y^2 + a*y^4, -2 + 5*y^2 + 9*y + a*y^3 - 2*a*y^2 + 4*a*x]");
    assert_eq!(ev(&mut e, "polynomial_reduction(poly, gb, [x, y])"), "poly");
    // specialised at a = 3 and a = -2 (where the leading coefficients do not vanish) it is the basis over Q
    for a in ["3", "-2"] {
        assert_eq!(
            ev(&mut e, &format!("groebner_basis(rewrite(gb, a -> {a}), [x, y]) === groebner_basis(rewrite(polys, a -> {a}), [x, y])")),
            "true"
        );
    }
}

#[test]
fn root_sums_by_structure() {
    let mut e = CoreEngine::new();
    same(
        &mut e,
        "root_sum(&(&1^2 - &1 + a), &(sin(&1)))",
        "sin((1/2)*(1 - (1 - 4*a)^(1/2))) + sin((1/2)*(1 + (1 - 4*a)^(1/2)))",
    );
    ev(&mut e, "s = root_sum(&(&1^5 - a * &1 + b), &((&1^2 - 1) / (&1^3 - 2 * &1 + c)))");
    same(
        &mut e,
        "s",
        "(-16*a - 10*b^2 - a^3 + 5*c^4 + 8*a^2 + a*b^2 - 5*b*c^3 - 4*a^2*c^2 + 8*a*c^2 + 8*a*b*c)/(-32*b - c^5 + c*a^3 - 10*c*b^2 - 8*a*c^3 - 8*c*a^2 - 2*b*a^2 + 16*a*b + 16*a*c + 20*b*c^2 + 3*a*b*c^2 + b^3)",
    );
    // checked against the numeric sum over the roots at two parameter points
    for (a, b, c) in [("2", "1", "3"), ("-1", "5", "1/2")] {
        let num = format!(
            "n(sum(rewrite((r^2 - 1)/(r^3 - 2*r + ({c})), r -> root(&(&1^5 - ({a})*&1 + ({b})), k)), [k, 1, 5]), 30)"
        );
        let exact = format!("n(rewrite(s, [a -> ({a}), b -> ({b}), c -> ({c})]), 30)");
        assert_eq!(
            ev(&mut e, &format!("abs(re({num} - {exact})) < 10^-20 and abs(im_part({num} - {exact})) < 10^-20")),
            "true",
            "{a} {b} {c}"
        );
    }
    // multiplicities and linearity; the polynomials are written with a positive leading coefficient
    same(
        &mut e,
        "root_sum(&((&1^3 - a)^2 * (&1^4 - b)^3), &(5 * tan(&1) + 7))",
        "126 + 5*(2*root_sum(&(-a + &1^3), &(tan(&1))) + 3*root_sum(&(-b + &1^4), &(tan(&1))))",
    );
    assert_eq!(ev(&mut e, "root_sum(&(a - &1^3), &(tan(&1))) - root_sum(&(&1^3 - a), &(tan(&1)))"), "0");
    // linear factors with parameters
    same(&mut e, "root_sum(&((&1 - a)^2*(&1 + b)), &(exp(&1)))", "2*exp(a) + exp(-b)");
}

#[test]
fn root_sums_of_logarithms() {
    let mut e = CoreEngine::new();
    same(&mut e, "root_sum(&(&1^5 - 2 * &1 + 3 / 7), &(log(2 * &1 + 1)))", "log(313/7) + im*pi");
    // variations: checked against the numeric sum of the principal logarithms
    for (p, q, deg) in [("&1^4 + &1 + 1", "r^2 - 3*r + 5", 4), ("&1^3 - 3*&1 + 7", "1 - 4*r", 3)] {
        let exact = ev(&mut e, &format!("root_sum(&({p}), &(log(rewrite({q}, r -> &1))))"));
        assert!(!exact.contains("root_sum"), "{exact}");
        let num = format!("n(sum(log(rewrite({q}, r -> root(&({p}), k))), [k, 1, {deg}]), 30)");
        let exact = format!("n(root_sum(&({p}), &(log(rewrite({q}, r -> &1)))), 30)");
        assert_eq!(
            ev(&mut e, &format!("abs(re({num} - {exact})) < 10^-20 and abs(im_part({num} - {exact})) < 10^-20")),
            "true",
            "{p}"
        );
    }
}

#[test]
fn root_sums_in_a_parameter() {
    let mut e = CoreEngine::new();
    same(
        &mut e,
        "integrate(root_sum(&(&1^5 + 11 * &1 + 1), &(exp(-a * &1))), a)",
        "-root_sum(&(1 + 11*&1 + &1^5), &(exp(-a*&1)/&1))",
    );
    same(
        &mut e,
        "integrate(root_sum(&(&1^5 + 11 * &1 + 1), &(sin(a * &1))), [a, 0, 1])",
        "-11 - root_sum(&(1 + 11*&1 + &1^5), &(cos(&1)/&1))",
    );
    same(
        &mut e,
        "normal(series(root_sum(&(&1^5 + 2 * &1^4 + 11 * &1 + 1), &(sin(a * &1) / a)), a, 0, 5))",
        "-2 + 4*a^2/3 + 73*a^4/120",
    );
    // variations: the antiderivative differentiates back; a definite integral and a series checked numerically
    assert_eq!(
        ev(&mut e, "simplify(d(integrate(root_sum(&(&1^3 + 2*&1 + 5), &(cos(a*&1))), a), a) - root_sum(&(&1^3 + 2*&1 + 5), &(cos(a*&1))))"),
        "0"
    );
    assert_eq!(
        ev(&mut e, "abs(n(integrate(root_sum(&(&1^3 + 2*&1 + 5), &(exp(a*&1))), [a, 0, 2]), 20) - n_integrate(sum(exp(a*root(&(&1^3 + 2*&1 + 5), k)), [k, 1, 3]), [a, 0, 2])) < 10^-8"),
        "true"
    );
    // power sums of x^3 - x + 1: p1 = 0, p2 = 2, p3 = -3, so 3 + 0 a + 2 a^2/2 - 3 a^3/6
    same(&mut e, "normal(series(root_sum(&(&1^3 - &1 + 1), &(exp(a*&1))), a, 0, 3))", "3 + a^2 - (1/2)*a^3");
}

#[test]
fn interpolation_in_several_variables() {
    let mut e = CoreEngine::new();
    ev(&mut e, "p = interpolating_polynomial([[[0, 0], 1], [[1, 0], 7], [[0, 1], 10], [[2, 1], 40], [[3, 3], 151], [[1, 2], 47]], [x, y])");
    same(&mut e, "p", "1 + 2*x + 3*y + 4*x^2 + 6*y^2 + 5*x*y");
    assert_eq!(
        ev(&mut e, "map(q => rewrite(p, [x -> q[1], y -> q[2]]), [[0, 0], [1, 0], [0, 1], [2, 1], [3, 3], [1, 2]])"),
        "[1, 7, 10, 40, 151, 47]"
    );
    same(&mut e, "interpolating_polynomial([[[1, 2, 3], 4, [5, 6, 7]], [[3, 2, 1], 0]], [x, y, z])", "-34 + 5*x + 6*y + 7*z");
    // variations: symbolic values on three points (degree 1); gradient data checked by differentiation
    same(&mut e, "interpolating_polynomial([[[0, 0], a], [[1, 0], b], [[0, 1], c]], [x, y])", "a + b*x + c*y - a*x - a*y");
    // the data of 1 + x + y + x^2 - x y: values and gradients (1 + 2x - y, 1 - x) at three points (7 conditions)
    ev(&mut e, "q = interpolating_polynomial([[[0, 0], 1, [1, 1]], [[1, 0], 3, [3, 0]], [[0, 1], 2]], [x, y])");
    same(&mut e, "q", "1 + x + y + x^2 - x*y");
    assert_eq!(
        ev(&mut e, "[rewrite(q, [x -> 0, y -> 0]), rewrite(d(q, x), [x -> 1, y -> 0]), rewrite(d(q, y), [x -> 1, y -> 0]), rewrite(q, [x -> 0, y -> 1])]"),
        "[1, 3, 0, 2]"
    );
    // four points in the plane: the least degree (2) is not unique, so no answer
    assert!(ev(&mut e, "interpolating_polynomial([[[0, 0], 1], [[1, 0], 2], [[0, 1], 3], [[1, 1], 5]], [x, y])").starts_with("interpolating_polynomial("));
}

#[test]
fn quasi_polynomial_generating_functions() {
    let mut e = CoreEngine::new();
    same(&mut e, "generating_function(mod(n, 4), n, x)", "(-x - 2*x^2 - 3*x^3)/(-1 + x^4)");
    same(&mut e, "generating_function(quotient(n, 5), n, x)", "x^5/(1 - x - x^5 + x^6)");
    same(
        &mut e,
        "generating_function(kronecker_symbol(n, 7), n, x)",
        "(x + 2*x^2 + x^3 + 2*x^4 + x^5)/(1 + x + x^2 + x^3 + x^4 + x^5 + x^6)",
    );
    // variations checked against the terms: floor, a product with n, an alternating sign and two symbols
    for f in ["floor(n/3)", "n*mod(n, 3) + quotient(n, 2)^2", "(-1)^n*mod(n, 3)*n", "kronecker_symbol(n, 8) + jacobi_symbol(n, 5)"] {
        assert_eq!(
            ev(&mut e, &format!("table(coefficient(normal(series(generating_function({f}, n, x), x, 0, 20)), x, k), [k, 0, 20]) === table({f}, [n, 0, 20])")),
            "true",
            "{f}"
        );
    }
}

#[test]
fn elimination_with_inequations() {
    let mut e = CoreEngine::new();
    ev(&mut e, "eqns = x*z == y and x^2*z^2 + x^2 == 1");
    assert_eq!(ev(&mut e, "eliminate(eqns and t != 0, z)"), "-1 + x^2 + y^2 == 0");
    // variations: off z = 0 the line x z = y gives nothing; off w = 0, (z - 1) w = 0 forces z = 1
    assert_eq!(ev(&mut e, "eliminate(x*z == y and z != 0, z)"), "true");
    assert_eq!(ev(&mut e, "eliminate(x == y*z and (z - 1)*w == 0 and w != 0, [z, w])"), "x - y == 0");
    // an unknown inside a function that is not a polynomial is not eliminated
    assert!(ev(&mut e, "eliminate(sin(x^2 - y) == 1 and cos(x - y^2) == 2, y, inverse_functions: false)").starts_with("failure("));
    assert_eq!(ev(&mut e, "eliminate(x == y^2 and y == z^3, [y], inverse_functions: false)"), "x - z^6 == 0");
}
