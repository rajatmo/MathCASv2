//! Tests of the number-sequence library (round 28): values checked against closed forms and
//! hand calculations, evaluated on the assembled core.
use mc_api::{Engine, StopFlag};

fn ev(src: &str) -> String {
    let mut e = mc_engine::CoreEngine::new();
    let stop = StopFlag::new();
    e.eval(src, &stop).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn check(cases: &[(&str, &str)]) {
    for (src, want) in cases {
        assert_eq!(ev(src), *want, "{src}");
    }
}

#[test]
fn sequences() {
    check(&[
        ("bernoulli_b([0, 1, 2, 4, 12, 3])", "[1, -1/2, 1/6, -1/30, -691/2730, 0]"),
        ("denominator(bernoulli_b(100))", "33330"),
        ("euler_e([0, 2, 4, 6, 3])", "[1, -1, 5, -61, 0]"),
        ("bell_b([0, 1, 2, 3, 10])", "[1, 1, 2, 5, 115975]"),
        ("bell_b(4, 2)", "94"),
        ("bernoulli_b(2, x)", "1/6 - x + x^2"),
        ("norlund_b(4, 1)", "-1/30"),
        ("norlund_b(3, a)", "(-1/8)*a^3 + (1/8)*a^2"),
    ]);
}

#[test]
fn integers_and_bits() {
    check(&[
        ("polygonal_number(5, [1, 2, 3, 4])", "[1, 5, 12, 22]"),
        ("polygonal_number(range(1, 5))", "[1, 3, 6, 10, 15]"),
        ("thue_morse(range(0, 7))", "[0, 1, 1, 0, 1, 0, 0, 1]"),
        ("bit_length([0, 1, 255, 256, -1, -256])", "[0, 1, 8, 9, 0, 8]"),
        ("[bit_shift_right(1000, 3), bit_shift_right(-9, 1), bit_flip(5, 1)]", "[125, -5, 7]"),
        ("roman_numeral([0, 4, 1994, 2023])", "[\"N\", \"IV\", \"MCMXCIV\", \"MMXXIII\"]"),
        ("from_roman_numeral(roman_numeral(range(1, 200))) == range(1, 200)", "true"),
        ("chinese_remainder([2, 3, 5], [3, 5, 7])", "68"),
        ("chinese_remainder([1, 2, 3], [4, 5, 6], 100)", "117"),
        ("chinese_remainder([1, 2], [4, 6])", "chinese_remainder([1, 2], [4, 6])"),
        ("farey_sequence(5)", "[0, 1/5, 1/4, 1/3, 2/5, 1/2, 3/5, 2/3, 3/4, 4/5, 1]"),
        ("length(farey_sequence(15)) == 1 + sum(euler_phi(k), [k, 1, 15])", "true"),
        ("convergents(pi, 4)", "[3, 22/7, 333/106, 355/113]"),
        ("convergents(golden_ratio, 6)", "[1, 2, 3/2, 5/3, 8/5, 13/8]"),
    ]);
}

#[test]
fn digits_and_expansions() {
    check(&[
        ("mantissa_exponent(123451)", "[123451/1000000, 6]"),
        ("mantissa_exponent(1027, 2)", "[1027/2048, 11]"),
        ("mantissa_exponent(0)", "[0, 0]"),
        ("number_digit(pi, range(2, -2, -1))", "[0, 0, 3, 1, 4]"),
        ("number_digit(2/3, range(-1, -3, -1))", "[6, 6, 6]"),
        ("number_expand(157)", "[100, 50, 7]"),
        ("number_expand(157, 2)", "[128, 0, 0, 16, 8, 4, 0, 1]"),
        ("number_expand(157, 2, 3)", "[128, 0, 29]"),
        ("number_expand(1111/7, 2)", "[128, 0, 0, 16, 8, 4, 2, 5/7]"),
        ("number_compose([1, 3, 22, 14], [86400, 3600, 60, 1])", "98534"),
        ("de_bruijn_sequence([0, 1], 3)", "[0, 0, 0, 1, 0, 1, 1, 1]"),
        ("de_bruijn_sequence(\"abcd\", 2)", "\"aabacadbbcbdccdd\""),
        ("mersenne_prime_exponent(range(1, 7))", "[2, 3, 5, 7, 13, 17, 19]"),
        ("select(range(2, 130), mersenne_prime_exponent_q)", "[2, 3, 5, 7, 13, 17, 19, 31, 61, 89, 107, 127]"),
    ]);
}

#[test]
fn groups_and_representations() {
    check(&[
        ("finite_abelian_group_count(range(1, 12))", "[1, 1, 1, 2, 1, 1, 1, 3, 2, 1, 1, 2]"),
        ("[finite_group_count(24), finite_group_count(2*3*5*7*11), finite_group_count(30)]", "[15, 36, 4]"),
        ("frobenius_solve([6, 9, 20], 44)", "[[1, 2, 1], [4, 0, 1]]"),
        ("frobenius_solve([5, 2, 1], 3)", "[[0, 0, 3], [0, 1, 1]]"),
        ("[frobenius_number([3, 5]), frobenius_number([6, 9, 20]), frobenius_number([4, 6])]", "[7, 43, inf]"),
        ("[squares_r(2, 45), squares_r(4, 20), squares_r(3, 10), squares_r(2, [1, 2, 5])]", "[8, 144, 24, [4, 4, 8]]"),
        ("powers_representations(1729, 2, 3)", "[[1, 12], [9, 10]]"),
        ("powers_representations(489886592769624989886788, 2, 2)", "[[179075995118, 676622775808], [180837148448, 676154212078]]"),
        ("continued_fraction_k(n^2, [n, 1, 10])", "13690825314204/17021645372933"),
    ]);
}

#[test]
fn angular_momentum() {
    check(&[
        ("clebsch_gordan([1/2, -1/2], [1/2, -1/2], [1, -1])", "1"),
        ("clebsch_gordan([2, 1], [2, 4], [4, 2])", "0"),
        ("clebsch_gordan([5, 0], [4, m], [1, 0])", "clebsch_gordan([5, 0], [4, m], [1, 0])"),
        ("clebsch_gordan([1/2, -1/2], [1, 0], [1/2, -1/2])^2", "1/3"),
        ("three_j_symbol([2, 1], [2, 2], [4, -3])^2", "1/18"),
        ("six_j_symbol([1, 2, 3], [2, 1, 2])^2", "1/525"),
        ("six_j_symbol([1, 2, 3], [4, 5, 12])", "0"),
        ("n(wigner_d([1, 0, 1], 0, 1.0, 0)) == n(-sin(1.0)/sqrt(2))", "true"),
    ]);
}

#[test]
fn expressions() {
    check(&[
        ("apply_sides(f, a != 5)", "f(a) != f(5)"),
        ("numerator_denominator((x - 1)/(y + 2))", "[-1 + x, 2 + y]"),
        ("[rational_expression_q((x^2 + 1)/(x - 1), x), rational_expression_q(sin(x)/x, x)]", "[true, false]"),
        ("expand_all((x + 1)^2/(x + 2))", "x^2/(2 + x) + 2*x/(2 + x) + (2 + x)^-1"),
        ("expand_denominator((x - y)^2/(u - v)^2)", "(x - y)^2/(-2*u*v + u^2 + v^2)"),
        ("factor_square_free_list(x^2*(x + 1)^3*(x - 2))", "[[1, 1], [-2 + x, 1], [x, 2], [1 + x, 3]]"),
        ("factor_terms_list(3 + 3*a + 6*a*x + 6*x + 12*a*x^2 + 12*x^2, x)", "[3, 1 + a, 1 + 2*x + 4*x^2]"),
        ("polynomial_extended_gcd(x^2 - 1, x^3 - 1, x)", "[-1 + x, [-x, 1]]"),
        ("polynomial_extended_gcd(x, (x - a)^2, x)", "[1, [(-x + 2*a)*a^-2, a^-2]]"),
        ("horner_form(1 + 2*x + 3*x^2)", "1 + x*(2 + 3*x)"),
        ("horner_form(x*y + 2*x^2*y + 2*x*y^2 + 4*x^2*y^2, [x, y])", "x*(y*(1 + 2*y) + x*y*(2 + 4*y))"),
        ("horner_form(x + 2*x^3 + 5*x^7)", "x*(1 + (2 + 5*x^4)*x^2)"),
        ("coefficient_rules(x^2 + y + 3, [x, y])", "[[2, 0] -> 1, [0, 1] -> 1, [0, 0] -> 3]"),
        ("symmetric_polynomial(2, [a, b, c])", "a*b + a*c + b*c"),
        ("[residue(1/(x*(x - 1)), [x, 0]), residue(exp(x)/x^3, [x, 0])]", "[-1, 1/2]"),
    ]);
}

#[test]
fn numeric_functions() {
    // machine values: Gram's series for R and the Dirichlet L-function of the character modulo 4 (Catalan's constant)
    assert_eq!(ev("abs(riemann_r(1000.) - 168.3594462811673) < 1e-10"), "true");
    assert_eq!(ev("riemann_r(1)"), "1");
    assert_eq!(ev("abs(dirichlet_l(4, 3, 2.) - 0.915965594177219) < 1e-12"), "true");
    assert_eq!(ev("dirichlet_l(1, 1, 2)"), "(1/6)*pi^2");
}
