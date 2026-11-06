//! `element` decided for numbers and closed expressions, the signed domains, `refine` deciding membership and
//! comparisons from the facts, `simplify(e, facts)` and the `assumptions` option, and the rewritings `simplify`
//! applies before its search.
mod common;
use common::check;

#[test]
fn element_numbers_and_closed_expressions() {
    check(&[
        ("element(7, integers)", "true"),
        ("element(7/2, integers)", "false"),
        ("element(7/2, rationals)", "true"),
        ("element(2.5, rationals)", "false"),
        ("element(2.5, reals)", "true"),
        ("element(3 + 4im, reals)", "false"),
        ("element(3 + 4im, complexes)", "true"),
        ("element(pi, reals)", "true"),
        ("element(-pi, negative_rationals)", "false"),
        ("element(sqrt(2), rationals)", "false"),
        ("element(1 + sqrt(2), integers)", "false"),
        ("element(sin(1), negative_reals)", "false"),
        ("element(exp(im*sin(7)), complexes)", "true"),
        ("element(0, positive_rationals)", "false"),
        ("element(-21/32, negative_rationals)", "true"),
        ("element(1234567, negative_integers)", "false"),
        ("element(2*cos(0), integers)", "true"),
    ]);
}

#[test]
fn element_lists_alternatives_and_signed_domains() {
    check(&[
        ("element([x, y, pi], integers)", "false"),
        ("element([x, 1, 2], integers)", "element(x, integers)"),
        ("element([x, y], integers)", "element([x, y], integers)"),
        ("element(x, positive_integers)", "element(x, integers) and x > 0"),
        ("element(x, nonpositive_reals)", "element(x, reals) and x <= 0"),
        ("element(x | y | -1, negative_integers)", "element(x | y, integers) and x < 0 and y < 0"),
        ("element(x, foo)", "element(x, foo)"),
    ]);
}

#[test]
fn refine_decides_membership_and_comparisons() {
    check(&[
        ("refine(element(x, negative_reals), element(x, negative_integers))", "true"),
        ("refine(element(x, complexes), element(x, negative_rationals))", "true"),
        ("refine(element(x, positive_integers), element(x, negative_integers))", "false"),
        ("refine(element(x, positive_rationals), element(x, positive_integers))", "true"),
        ("refine(element(x^2 + 2*x*y + y^3/3, positive_rationals), element(x | y, positive_rationals))", "true"),
        ("refine(element(x, integers), x > 0)", "element(x, integers)"),
        // integers strictly above 0 are at least 1, and the factored difference has factors of known sign
        ("refine(n^3 >= n, element(n, positive_integers))", "true"),
        ("refine(n^3 <= n, element(n, negative_integers))", "true"),
        ("refine(n^3 >= n, n > 0)", "n^3 >= n"),
        // bounds of a power of a bounded base
        ("refine(sqrt((1 - x^2)^2), x > 1)", "-1 + x^2"),
        ("refine(sqrt((2 - x^2)^2), x > -1 and x < 1)", "2 - x^2"),
        // mod(m, 2) == 0 makes m even; (x^a)^c = x^(a c) for an integer c
        ("refine(cos(k*pi)^m, element(k, integers) and mod(m, 2) == 0)", "1"),
        ("refine((x^a)^n, element(n, integers))", "x^(a*n)"),
        // floor, ceiling, max and min decided by the bounds
        ("refine(floor(x), 0 < x and x < 1)", "0"),
        ("refine(ceiling(x), 2 < x and x <= 3)", "3"),
        ("refine(floor(x), 0 < x and x <= 1)", "floor(x)"),
        ("refine(max(a, b), a > b)", "a"),
        ("refine(max(x, x + 1, 3))", "max(1 + x, 3)"),
        ("refine(min(1 - x, x, 1 + x), x > 0)", "min(1 - x, x)"),
        // closed constants as bounds; sin and cos positive on their intervals
        ("refine(cos(x) > 0, -pi/2 < x and x < pi/2)", "true"),
        ("refine(sin(x) > 0, 0 < x and x < 1)", "true"),
    ]);
}

#[test]
fn simplify_with_facts_and_options() {
    check(&[
        ("simplify(sqrt(x^2), x > 0)", "x"),
        ("simplify(abs(x), x < 0)", "-x"),
        ("simplify(cos(k*pi)^m, assumptions: element(k, integers) and mod(m, 2) == 0)", "1"),
        ("refine(sqrt(x^2), assumptions: x < 0)", "-x"),
        ("assuming(x > 0, simplify(sqrt(x^2*y^2), y < 0))", "-1*x*y"),
        // options other than assumptions are accepted and ignored
        ("simplify(sin(x)^2 + cos(x)^2, trig: false)", "1"),
    ]);
}

#[test]
fn simplify_preparation() {
    check(&[
        ("simplify(sqrt(2) + sqrt(3) - sqrt(5 + 2*sqrt(6)))", "0"),
        ("simplify(sqrt(3 - 2*sqrt(2)))", "-1 + 2^(1/2)"),
        ("simplify(pi/degree)", "180"),
        ("simplify(180*degree)", "pi"),
        ("simplify(z and y and y and x)", "x and y and z"),
        ("simplify(z or x or z)", "x or z"),
        ("simplify(floor(x) + floor(x + 1))", "1 + 2*floor(x)"),
        ("simplify(ceiling(x - 2) - ceiling(x))", "-2"),
        ("simplify(gamma(conjugate(z)) == conjugate(gamma(z)))", "true"),
        ("simplify(sinh(conjugate(z)) == conjugate(sinh(z)))", "true"),
        ("simplify(binomial(conjugate(z), 2) == conjugate(binomial(z, 2)))", "true"),
        // not mirror-symmetric in general: left alone
        ("simplify(conjugate(f(z)))", "conjugate(f(z))"),
    ]);
}
