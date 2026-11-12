//! Round 36, algebra: regression tests for fixed wrong answers and for calls that now
//! decline. Each value is checked by an independent route (an identity, a substitution, a second algorithm).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn convergents_of_a_periodic_continued_fraction_end_with_its_value() {
    // sqrt 7 = [2; 1, 1, 1, 4, ...]: the convergents of 2, 1, 1, 1 and then the value of the periodic fraction
    assert_eq!(ev("convergents([2, [1, 1, 1, 4]])"), "[2, 3, 5/2, 8/3, 7^(1/2)]");
    // the second route: the convergents of the sqrt 7 expansion itself agree on the common terms
    assert_eq!(ev("take(convergents(sqrt(7), 10), 4)"), "[2, 3, 5/2, 8/3]");
}

#[test]
fn primes_membership_of_an_expression_that_is_an_integer() {
    // ((e + 7)^2 - e^2)/7 - 2 e = 7
    assert_eq!(ev("map(&(element(&1, primes)), [7, a, (e + 7)^2 / 7 - e^2 / 7 - 2 * e, 7 / 2, sqrt(2)])"), "[true, element(a, primes), true, false, false]");
    assert_eq!(ev("simplify((e + 7)^2 / 7 - e^2 / 7 - 2 * e)"), "7");
}

#[test]
fn root_reduce_of_a_polynomial_keeps_its_degree() {
    assert_eq!(ev("exponent(root_reduce((x^2 + 2 * x - 1)^7 - 3), x)"), "14");
    // the zero sqrt 2 + sqrt 3 - sqrt(5 + 2 sqrt 6) as a coefficient vanishes
    assert_eq!(ev("root_reduce(x^2 + (sqrt(2) + sqrt(3) - sqrt(5 + 2 * sqrt(6))) * x^15)"), "x^2");
}

#[test]
fn norm_trace_and_characteristic_polynomial_over_an_extension() {
    // over Q(sqrt 5) the norm of 2 is 2 * 2 and of sqrt 5 is sqrt 5 * (-sqrt 5)
    assert_eq!(ev("algebraic_number_norm([2, sqrt(5)], extension: sqrt(5))"), "[4, -5]");
    assert_eq!(ev("algebraic_number_trace([2, sqrt(5)], extension: sqrt(5))"), "[4, 0]");
    // sqrt 2 in Q(e^(i pi/4)), degree 4: (x^2 - 2)^2
    assert_eq!(ev("factor(minimal_polynomial(sqrt(2), x, extension: e^(im * (pi / 4))))"), "(-2 + x^2)^2");
    assert!(ev("algebraic_number_norm(sqrt(2), extension: sqrt(3))").contains("not_in_field"));
}

#[test]
fn logarithms_of_complex_infinity() {
    assert_eq!(ev("[log(complex_infinity), log2(complex_infinity), log10(complex_infinity), log2(8)]"), "[inf, inf, inf, 3]");
}

#[test]
fn multinomial_with_many_arguments() {
    // the Bell numbers from the integer partitions: 21147 and 115975 are bell_b(9), bell_b(10)
    let s = "table(sum(apply(multinomial, p) / product(factorial(k), [k, map(length, split(p))]), [p, integer_partitions(n_u)]), n_u, 10)";
    assert_eq!(ev(s), "[1, 2, 5, 15, 52, 203, 877, 4140, 21147, 115975]");
    assert_eq!(ev("table(bell_b(n), n, 10)"), "[1, 2, 5, 15, 52, 203, 877, 4140, 21147, 115975]");
    assert_eq!(ev("multinomial(1, 1, 1, 1, 1, 1, 1, 1, 1, 1)"), "3628800");
}

#[test]
fn modulus_and_trig_options_of_polynomial_structure_functions() {
    assert_eq!(ev("collect((4 + 2 * a + x)^2, x, modulus: 3)"), "1 + a + x*(2 + a) + a^2 + x^2");
    assert_eq!(ev("collect((x + 1)^2 + (a * x + b)^2, x, modulus: 2)"), "1 + (1 + a^2)*x^2 + b^2");
    assert_eq!(ev("exponent(2 * x^2 - x + 1, x, modulus: 2)"), "1");
    assert_eq!(ev("exponent(tan(x), cos(x), trig: true)"), "-1");
    assert_eq!(ev("variables(x + 2 * y + 3 * z, modulus: 2)"), "[x, z]");
    assert_eq!(ev("factor_terms_list(8 * x^2 + 5, modulus: 3)"), "[2, 1 + x^2]");
    assert_eq!(ev("factor_terms_list(3 * x + 10, modulus: 7)"), "[3, 1 + x]");
    assert_eq!(ev("factor_terms_list(4 * sin(x) * cos(x) + 4 * cos(x)^2 - 4 * sin(x)^2, trig: true)"), "[2, 2*cos(2*x) + sin(2*x)]");
    // the second route: the product of the list is the reduced input
    assert_eq!(ev("polynomial_mod(expand(2 * (1 + x^2) - (8 * x^2 + 5)), 3)"), "0");
}

#[test]
fn factor_terms_list_gaussian_content_and_several_variables() {
    assert_eq!(ev("factor_terms_list(5 * im * x^2 + 20 * x * im + 10)"), "[5im, -2im + 4*x + x^2]");
    let p = "-6 * y - 6 * a * y + 2 * x^2 * y + 2 * a * x^2 * y + 4 * a * y^2 + 4 * a^2 * y^2";
    assert_eq!(ev(&format!("factor_terms_list({p}, [x, y])")), "[2, 1 + a, y, -3 + 2*a*y + x^2]");
    assert_eq!(ev(&format!("expand(apply(mul, factor_terms_list({p}, [x, y])) - ({p}))")), "0");
}

#[test]
fn square_free_list_over_the_gaussian_rationals_is_monic() {
    let p = "x^4 - 2 * im * x^3 - 2 * x^2 + 2 * im * x + 1";
    assert_eq!(ev(&format!("factor_square_free_list({p})")), "[[1, 1], [-1 + x^2, 1], [-1im + x, 2]]");
    assert_eq!(ev(&format!("expand((-1 + x^2) * (x - im)^2 - ({p}))")), "0");
}

#[test]
fn factor_list_over_a_finite_field_has_a_field_constant() {
    let s = "{ F = finite_field(17, 3); fl = factor_list(F(3) * x^2 + F(1771)); [fl[1, 1] === F(3), expand(apply(mul, map(&(&1[1]^&1[2]), fl)) - (F(3) * x^2 + F(1771)))] }";
    assert_eq!(ev(s), "[true, 0]");
}

#[test]
fn resultant_of_rational_functions_is_multiplicative() {
    let s = "{ f1 = 3*x^2 - 7; f2 = 2*x^3 - 3*x + 8; g1 = x^2 + x + 5; g2 = x - 4; [resultant(f1 / f2, g1 / g2^2, x), (resultant(f1, g1, x) * resultant(f2, g2, x)^2) / (resultant(f2, g1, x) * resultant(f1, g2, x)^2)] }";
    assert_eq!(ev(s), "[7119088/1894487, 7119088/1894487]");
}

#[test]
fn pell_equations_over_sign_restricted_integers() {
    // x + y sqrt 2 = (3 + 2 sqrt 2)^c
    let pos = "reduce(x^2 - 2 * y^2 == 1, [x, y], positive_integers) === (element(c(1), integers) and c(1) >= 1 and x == (1/2)*((3 - 2*2^(1/2))^c(1) + (3 + 2*2^(1/2))^c(1)) and y == (-1/4)*(-(3 + 2*2^(1/2))^c(1) + (3 - 2*2^(1/2))^c(1))*2^(1/2))";
    assert_eq!(ev(pos), "true");
    let neg = "reduce(x^2 - 2 * y^2 == 1, [x, y], negative_integers) === (element(c(1), integers) and c(1) >= 1 and x == (-1/2)*((3 - 2*2^(1/2))^c(1) + (3 + 2*2^(1/2))^c(1)) and y == (1/4)*(-(3 + 2*2^(1/2))^c(1) + (3 - 2*2^(1/2))^c(1))*2^(1/2))";
    assert_eq!(ev(neg), "true");
    assert!(ev("reduce(x^2 - 2 * y^2 == 1, [x, y], nonnegative_integers)").contains("c(1) >= 0"));
    assert!(ev("reduce(x^2 - 2 * y^2 == 1, [x, y], nonpositive_integers)").contains("c(1) >= 0"));
    // the second route: c = 2 gives (17, 12), and 17^2 - 2 * 12^2 = 1
    assert_eq!(ev("expand(rewrite([(1/2)*((3 - 2*2^(1/2))^c + (3 + 2*2^(1/2))^c), (-1/4)*(-(3 + 2*2^(1/2))^c + (3 - 2*2^(1/2))^c)*2^(1/2)], c -> 2))"), "[17, 12]");
    assert_eq!(ev("reduce(x^2 == 4, x, positive_integers)"), "x == 2");
}

#[test]
fn groebner_basis_ranks_the_other_symbols_as_variables() {
    // the elimination ideal of (x - a)(x - b), (x - c)(x - d)(x - e) is generated by the resultant
    assert_eq!(
        ev("factor(select(groebner_basis([(x - a) * (x - b), (x - c) * (x - d_u) * (x - e_u)], x), &(free_of(&1, x))))"),
        "[(a - c)*(a - d_u)*(a - e_u)*(b - c)*(b - d_u)*(b - e_u)]"
    );
    assert_eq!(ev("factor(resultant((x - a) * (x - b), (x - c) * (x - d_u) * (x - e_u), x))"), "(a - c)*(a - d_u)*(a - e_u)*(b - c)*(b - d_u)*(b - e_u)");
    assert_eq!(ev("groebner_basis([f1 - x^2 - y^2, f2 - x * y], [x, y]) === [-f1*y^2 + f2^2 + y^4, f2*x - f1*y + y^3, -f2 + x*y, -f1 + x^2 + y^2]"), "true");
    assert_eq!(ev("groebner_basis([a*x - 1, x*y - b], [x, y]) === [y - a*b, -1 + a*x]"), "true");
}

#[test]
fn find_instance_checks_the_whole_condition() {
    // z = 0 solves the equations but not x y z != 0; the instance found is checked numerically by substitution
    let s = "{ s = find_instance(x^2 + y^3 == z and x + 2 * y == 3 * z + 1 and x * y * z != 0, [x, y, z]); [chop(n(rewrite([x^2 + y^3 - z, x + 2 * y - 3 * z - 1], s[1]))), chop(n(rewrite(x * y * z, s[1]))) != 0] }";
    assert_eq!(ev(s), "[[0, 0], true]");
    assert_eq!(ev("find_instance(a and x < y and element(a, booleans), [a, x, y])"), "[[a -> true, x -> 0, y -> 1]]");
}

#[test]
fn solve_with_a_region_and_a_strip_of_the_real_part() {
    let s = ev("solve(x + y == 1 / 2 and element([x, y], circle()), [x, y])");
    assert_eq!(s, ev("solve(x + y == 1 / 2 and x^2 + y^2 == 1, [x, y])"));
    assert_eq!(ev("map(r => simplify(rewrite([x + y, x^2 + y^2], r)), solve(x + y == 1 / 2 and element([x, y], circle()), [x, y]))"), "[[1/2, 1], [1/2, 1]]");
    // cos(2x) = -3: x = +-acos(-3)/2 + k pi, re(acos(-3)) = pi; in 0 <= re(x) < pi one member of each family
    let t = ev("solve(3 + cos(2 * x) == 0 and (0 <= re(x) and re(x) < pi), x)");
    assert_eq!(ev(&format!("map(r => [chop(n(rewrite(3 + cos(2 * x), r))), n(rewrite(re(x), r))], {t})")), "[[0, 1.5707963267948966], [0, 1.5707963267948966]]");
    assert_eq!(ev(&format!("length({t})")), "2");
}

#[test]
fn subtract_an_inequality_from_an_equation() {
    // at z = 7 the result is y > b + m x again
    assert_eq!(ev("subtract_sides(z == 7, y > m * x + b)"), ev("z - b - m*x > 7 - y"));
    assert_eq!(ev("simplify(rewrite(subtract_sides(z == 7, y > m * x + b), z -> 7) === (7 - b - m*x > 7 - y))"), "true");
}

#[test]
fn infinite_sum_with_generated_conditions() {
    assert_eq!(ev("sum(a^i, [i, 0, inf], generate_conditions: true)"), "conditional_expression((1 - a)^-1, abs(a) < 1)");
    assert_eq!(ev("sum(1 / i^2, [i, 1, inf], generate_conditions: true)"), "(1/6)*pi^2");
    // the second route: the partial sums at a = 1/2 tend to 2
    assert_eq!(ev("sum((1/2)^i, [i, 0, inf])"), "2");
}

#[test]
fn continued_fraction_of_a_machine_real_and_of_an_exact_constant() {
    // every real that rounds to n(pi) has these 13 terms: those of pi -+ ulp/2 agree up to the 14
    assert_eq!(ev("continued_fraction(n(pi))"), "[3, 7, 15, 1, 292, 1, 1, 1, 2, 1, 3, 1, 14]");
    assert_eq!(ev("take(continued_fraction(pi, 20), 13)"), "[3, 7, 15, 1, 292, 1, 1, 1, 2, 1, 3, 1, 14]");
    assert_eq!(ev("continued_fraction(1.5)"), "[1, 2]");
    assert_eq!(ev("convergents(5.67567)"), "[5, 6, 17/3, 210/37, 26897/4739, 27107/4776, 54004/9515, 81111/14291, 135115/23806, 216226/38097]");
    // the Champernowne constant has a term of 411100 digits among the first 1000: beyond the precision cap the
    // call declines instead of giving terms of the rounding
    assert_eq!(ev("continued_fraction(champernowne_number(10), 1000)"), "continued_fraction(champernowne_number(10), 1000)");
    assert_eq!(ev("length(continued_fraction(pi, 1000))"), "1000");
}

#[test]
fn residues_at_root_objects_and_root_contagion() {
    // gamma(x^3 - x/3) in |x| <= 1: poles at u = 0 (x = 0, +-1/sqrt 3) and u = -1 (two complex roots of
    // 3 x^3 - x + 3); the residue sum times 2 pi i equals the contour integral over the unit circle
    let s = "{ f = gamma(x^3 - x / 3); pp = function_poles([f, abs(x) <= 1], x); r = root_reduce(2 * pi * im * total(map(&(residue(f, [x, &1[1]])), pp))); chop(n_integrate(rewrite(f, x -> exp(im * t)) * d(exp(im * t), t), [t, 0, 2 * pi]) - r) }";
    assert_eq!(ev(s), "0");
    // a root object with exact coefficients joins machine arithmetic like sqrt(2) does
    assert_eq!(ev("1.0 + root(&(&1^3 - 2), 1)"), ev("1.0 + 2^(1/3)"));
}

#[test]
fn solve_lists_family_members_in_a_bounded_interval() {
    // cos(x^2) = 0 in [-5, 5]: x^2 = (2k + 1) pi/2 for k = 0..7, 16 roots, all exact
    let s = "{ s = solve(cos(x^2) == 0 and (-5 <= x and x <= 5), x, reals); [length(s), sort(map(r => rewrite(x^2/pi, r), s))[16]] }";
    assert_eq!(ev(s), "[16, 15/2]");
    // sin(pi/(x^2 - 1)) = 0 with |1/(x^2 - 1)| < 7: x^2 = 1 + 1/k, 0 < |k| < 7 (x = +-1 excluded: 1/0)
    let t = "{ s = solve((x^2 == 1 or sin(pi/(-1 + x^2)) == 0) and (-2 <= x and x <= 2) and abs(1 / (x^2 - 1)) < 7, x, reals); [length(s), union(map(r => rewrite(x^2, r), s))] }";
    assert_eq!(ev(t), "[23, [0, 1/2, 2/3, 3/4, 4/5, 5/6, 7/6, 6/5, 5/4, 4/3, 3/2, 2]]");
}

#[test]
fn small_constants_compare_and_gaussian_quotients() {
    assert_eq!(ev("[2 < khinchin and khinchin < 3, khinchin < 2]"), "[true, false]");
    assert_eq!(ev("[quotient(9 + 6im, 1 + im), mod(9 + 6im, 1 + im)] === [8 - 2im, -1]"), "true");
    // the second route: a = b q + r
    assert_eq!(ev("(1 + im) * (8 - 2im) + (-1) === 9 + 6im"), "true");
    assert_eq!(ev("quotient_remainder(9.4 + 6 * im, 1. + im)[1] === 8 - 2im"), "true");
}

#[test]
fn digits_series_coefficients_and_rules() {
    assert_eq!(ev("from_digits([1, 2, 3, indeterminate])"), "1230.0");
    assert_eq!(ev("coefficient_list(series(exp(x^2), x, 0, 6), x)"), "[1, 0, 1, 0, 1/2, 0, 1/6]");
    assert_eq!(ev("to_rules((x == 1 or x == 2) and y == x - 1)"), "[[x -> 1, y -> -1 + x], [x -> 2, y -> -1 + x]]");
    assert_eq!(ev("to_rules(x == 3 and y == 4)"), "[x -> 3, y -> 4]");
}

#[test]
fn boole_arithmetic_of_every_connective() {
    assert_eq!(ev("simplify(-1 + boole(a) + boole(implies(a, b)) - boole(a)*boole(b))"), "0");
    assert_eq!(ev("simplify(-1 + boole(nand(a, b, c)) + boole(a)*boole(b)*boole(c))"), "0");
    assert_eq!(ev("simplify(boole(nor(a, b, c)) - (1 - boole(a))*(1 - boole(b))*(1 - boole(c)))"), "0");
    assert_eq!(ev("simplify(boole(xor(a, b, c)) - mod(boole(a) + boole(b) + boole(c), 2))"), "0");
    assert_eq!(ev("simplify([p, nand(p, p), nand(nand(p, p), nand(p, p))])"), "[p, not p, p]");
    // the second route: the truth table of xor against the parity
    assert_eq!(ev("union(flatten(table(boole(xor(u, v, w)) - mod(boole(u) + boole(v) + boole(w), 2), [u, [true, false]], [v, [true, false]], [w, [true, false]])))"), "[0]");
}

#[test]
fn finite_field_resultant_is_expanded_and_parametric_integer_reduce_declines() {
    let s = "{ F = finite_field(17, 3); r = resultant(F(1) * x^5 + F(123) * x * y + F(456), F(789) * x^3 + F(987) * x + F(654), x); [r === expand(r), exponent(r, y)] }";
    assert_eq!(ev(s), "[true, 3]");
    // u is a parameter: for u = 0 (0, 0, 0) is a solution, so false would be wrong
    assert!(ev("reduce([a + b + c == -u, a*b + a*c + b*c == u, a*b*c == -u], [a, b, c], integers)").contains("failure"));
}

#[test]
fn gaussian_primes() {
    // the Gaussian Mersenne exponents below 1000 (OEIS A057429)
    assert_eq!(ev("select(range(200), n => is_prime((1 + im)^n - 1, gaussian_integers: true))"), "[2, 3, 5, 7, 11, 19, 29, 47, 73, 79, 113, 151, 157, 163, 167]");
    // the second route: 3 is inert (3 mod 4), 5 = (2 + i)(2 - i) splits
    assert_eq!(ev("[is_prime(3, gaussian_integers: true), is_prime(5, gaussian_integers: true), expand((2 + im) * (2 - im))]"), "[true, false, 5]");
}

#[test]
fn angle_of_parallel_machine_vectors_is_exact() {
    // 0.2 = 2 * 0.1 in binary, so [1, 2] and [0.1, 0.2] are parallel: every 2 x 2 minor is exactly 0
    assert_eq!(ev("[vector_angle([1, 2], [0.1, 0.2]), possible_zero_q(vector_angle([1, 2], [0.1, 0.2]))]"), "[0.0, true]");
    assert_eq!(ev("vector_angle([1, 2], [-0.1, -0.2]) == n(pi)"), "true");
    assert_eq!(ev("abs(vector_angle([1, 2], [1.0, 2.0000001]) - 2.0e-8) < 1.0e-15"), "true");
}

#[test]
fn a_generated_condition_the_assumptions_imply_drops() {
    assert_eq!(ev("refine(sum(a^i, [i, 0, inf], generate_conditions: true), abs(a) < 1)"), "(1 - a)^-1");
    assert_eq!(ev("sum(a^i, [i, 0, inf], assumptions: -(1 / 2) < a and a < 1 / 2, generate_conditions: true)"), "(1 - a)^-1");
    assert_eq!(ev("refine(conditional_expression(x, a > 1), a < 0)"), "conditional_expression(x, a > 1)");
}
