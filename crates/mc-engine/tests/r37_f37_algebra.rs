//! Round 37, algebra: functions that gave no answer where one exists. Every test has an example
//! input and variations of its own, each checked by an independent route (an identity, a substitution, a second
//! algorithm, a hand computation written out in the comment).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// The last value of `setup` (statements on lines, the last one an expression) is the term `expected`.
fn same(setup: &str, expected: &str) {
    let got = ev(&format!("{{ {} }}", setup_as_block(setup, expected)));
    assert_eq!(got, "true", "{setup}\n  value: {}", ev(&format!("{{ {} }}", setup_as_block(setup, "0").replace(" === (0)", ""))));
}

/// `setup` as statements of a block whose value is `last === expected`.
fn setup_as_block(setup: &str, expected: &str) -> String {
    let mut lines: Vec<&str> = setup.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let last = lines.pop().unwrap_or("null");
    let mut s = String::new();
    for l in lines {
        s.push_str(l);
        s.push_str("; ");
    }
    s.push_str(&format!("({last}) === ({expected})"));
    s
}

// ------------------------------------------------------------------------------------------- irreducibility

#[test]
fn irreducible_polynomial_threads_over_lists() {
    // documented
    same("is_irreducible_polynomial([x^2 - 1, x^2 - 2])", "[false, true]");
    same("is_irreducible_polynomial([x^4 - 4 * y^2, x^4 - 2 * y^2])", "[false, true]");
    same("is_irreducible_polynomial([x^3 + 1, x^3 + x + 1], modulus: 2)", "[false, true]");
    // x^4 + 4 = (x^2 + 2 x + 2)(x^2 - 2 x + 2) (Sophie Germain), x^4 + x + 1 has no root in F_2 and is not
    // (x^2 + x + 1)^2 = x^4 + x^2 + 1, the only irreducible quadratic squared: irreducible
    same("is_irreducible_polynomial([x^4 + 4, x^4 + x + 1])", "[false, true]");
    same("expand((x^2 + 2 * x + 2) * (x^2 - 2 * x + 2))", "x^4 + 4");
    same("is_irreducible_polynomial([x^4 + x + 1, x^4 + x^2 + 1], modulus: 2)", "[true, false]");
    // over Q(2^(1/3)), of degree 3: x^3 - 2 has the root 2^(1/3); sqrt 3 has degree 2, which does not divide 3, so
    // x^2 - 3 stays irreducible
    same("is_irreducible_polynomial([x^3 - 2, x^2 - 3], extension: 2^(1 / 3))", "[false, true]");
}

#[test]
fn irreducible_polynomial_with_algebraic_coefficients() {
    // documented: (x + i)^2 over the Gaussian rationals; with sqrt 2 as a coefficient factor does not split
    // x^2 + 2 sqrt 2 x + 2 unless the extension is asked for, and the answer follows factor
    same("is_irreducible_polynomial(x^2 + 2 * im * x - 1)", "false");
    same("is_irreducible_polynomial(x^2 + 2 * sqrt(2) * x + 2, extension: automatic)", "false");
    same("is_irreducible_polynomial(x^2 + 2 * sqrt(2) * x + 2)", "true");
    // x^2 + 2 i x + 3 = (x - i)(x + 3 i): the product expands back
    same("is_irreducible_polynomial(x^2 + 2 * im * x + 3)", "false");
    same("expand((x - im) * (x + 3 * im))", "x^2 + 2 * im * x + 3");
    // x^2 + 1 + i: its roots +-sqrt(-1 - i) are not Gaussian rationals (the norm 2 of -1 - i is not a square)
    same("is_irreducible_polynomial(x^2 + 1 + im)", "true");
    // no multivariate factorisation over an extension: no answer, never a guess
    same("is_irreducible_polynomial(x^4 - 3 * y^2, extension: sqrt(3))", "is_irreducible_polynomial(x^4 - 3 * y^2, extension: sqrt(3))");
}

// ------------------------------------------------------------------------------------------- modulus: m

#[test]
fn expand_over_z_mod_n() {
    // documented; by hand (Frobenius) (1 + x)^10 = (1 + x^2)^5 = (1 + x^8)(1 + x^2) mod 2
    same("expand((1 + x)^10, modulus: 2)", "1 + x^2 + x^8 + x^10");
    same("expand((2 * y + 3 * x)^6, modulus: 4)", "x^6");
    // a composite modulus: the coefficients are the binomial coefficients mod 4
    same(
        "coefficient_list(expand((1 + x)^10, modulus: 4), x)",
        "map(k => mod(binomial(10, k), 4), range(0, 10))",
    );
    // Frobenius at p = 7 with several variables
    same("expand((x + y)^7, modulus: 7)", "x^7 + y^7");
    // 1/3 = 5 mod 7 (3 * 5 = 15 = 1)
    same("expand((x + 1/3)^2, modulus: 7)", "x^2 + 3 * x + 4");
    same("expand((x + 1/3)^2)", "x^2 + 2/3 * x + 1/9");
    // 1/2 has no inverse mod 4: no answer
    same("expand(x / 2, modulus: 4)", "expand(x / 2, modulus: 4)");
}

#[test]
fn expand_leaves_the_parts_free_of_a_pattern() {
    // documented
    same("expand((a + b)^2 * (1 + x)^2, x)", "x^2*(a + b)^2 + 2*x*(a + b)^2 + (a + b)^2");
    same("expand((1 + x)^2 + (2 + x)^2, 1 + x)", "1 + 2*x + x^2 + (2 + x)^2");
    same("expand((a(1) + a(2)) * (x(1) + x(2))^2, x(_))", "(a(1) + a(2))*x(1)^2 + (a(1) + a(2))*x(2)^2 + 2*(a(1) + a(2))*x(1)*x(2)");
    // the same value as the full expansion, and (c + d)^3 kept whole
    same("{ e = (c + d)^3 * (y - 1)^3 + y; expand(expand(e, y) - e) }", "0");
    same("expand((c + d)^3 * (y - 1)^3, y)", "(c + d)^3 * y^3 - 3 * (c + d)^3 * y^2 + 3 * (c + d)^3 * y - (c + d)^3");
}

#[test]
fn fractions_over_z_mod_n() {
    // documented: 8/9 = 8 * 9^-1 = 3 * 4 = 2 mod 5, the denominator made to lead with 1
    same("numerator((24 * x^y) / (3 * b)^3, modulus: 5)", "2*x^y");
    same("denominator((24 * x^y) / (3 * b)^3, modulus: 5)", "b^3");
    same(
        "numerator_denominator(((1 / 3) * x + (3 / 4) * y) / ((3 / 5) * x - (1 / 2) * y), modulus: 7)",
        "[3*y + 6*x, x + 5*y]",
    );
    // cross-multiplication: n * den(e) - d * num(e) vanishes mod m, for another fraction and modulus 11
    same(
        "{ e = (2 * x^2 + 7) / (3 * x + 5 * y); nd = numerator_denominator(e, modulus: 11); polynomial_mod(expand(nd[1] * denominator(e) - nd[2] * numerator(e)), 11) }",
        "0",
    );
    same("expand_all(6 * (z / (2 * x + 3 * y)^6), modulus: 4)", "2*z/y^6");
    same("expand_all(z / (x + y)^3, modulus: 3)", "z/(x^3 + y^3)");
    same("expand_numerator((x + y)^3 / z, modulus: 3)", "(x^3 + y^3)/z");
    // (x + y)^5 mod 5 = x^5 + y^5 in a denominator
    same("expand_denominator(z / (x + y)^5, modulus: 5)", "z/(x^5 + y^5)");
    same("coefficient_rules((x + 1)^5, x, modulus: 2)", "[[5] -> 1, [4] -> 1, [1] -> 1, [0] -> 1]");
    same("monomial_list((x + 1)^5, x, modulus: 2)", "[x^5, x^4, x, 1]");
    // the coefficients of (x + 1)^5 are 1 5 10 10 5 1: mod 3 they are 1 2 1 1 2 1
    same("coefficient_rules((x + 1)^5, x, modulus: 3)", "[[5] -> 1, [4] -> 2, [3] -> 1, [2] -> 1, [1] -> 2, [0] -> 1]");
}

#[test]
fn division_and_gcd_over_a_prime_field() {
    // documented
    same("polynomial_quotient(x^2 + 4 * x + 1, 2 * x + 1, x, modulus: 5)", "3 + 3*x");
    same("polynomial_quotient_remainder(x^2 + 4 * x + 1, 2 * x + 1, x, modulus: 3)", "[1 + 2*x, 0]");
    same("polynomial_mod(x^2, 2 * x - 1, modulus: 3)", "1");
    // the division identity a = q b + r with deg r < deg b, mod 5 and mod 7
    for (p, a, b) in [(5, "x^2 + 4 * x + 1", "2 * x + 1"), (7, "x^5 + 3 * x + 1", "3 * x^2 + 2")] {
        same(
            &format!(
                "{{ qr = polynomial_quotient_remainder({a}, {b}, x, modulus: {p}); [polynomial_mod(expand(qr[1] * ({b}) + qr[2] - ({a})), {p}), exponent(qr[2], x) < exponent({b}, x)] }}"
            ),
            "[0, true]",
        );
    }
    // documented extended gcd; the Bezout identity s a + t b = g mod p, also mod 5
    same("polynomial_extended_gcd((x - 1)^2 * (x - 2)^3, x^3 - 1, x, modulus: 3)", "[1 + x + x^2, [2, 1 + x + x^2]]");
    same("polynomial_extended_gcd((x - 1)^2 * (x - 2)^2, (x - 1) * (x^2 - 3), x, modulus: 2)", "[1 + x^2, [1, 1 + x]]");
    for (p, a, b) in [(3, "(x - 1)^2 * (x - 2)^3", "x^3 - 1"), (5, "x^4 + 2 * x + 3", "x^3 + x + 4")] {
        same(
            &format!(
                "{{ g = polynomial_extended_gcd({a}, {b}, x, modulus: {p}); [polynomial_mod(expand(g[2][1] * ({a}) + g[2][2] * ({b}) - g[1]), {p}), polynomial_remainder({a}, g[1], x, modulus: {p}), polynomial_remainder({b}, g[1], x, modulus: {p})] }}"
            ),
            "[0, 0, 0]",
        );
    }
}

#[test]
fn factor_lists_over_a_prime_field() {
    // documented
    same("factor_list(x^4 + 1, modulus: 2)", "[[1, 1], [1 + x, 4]]");
    same("factor_list(5 + x^12, modulus: 7)", "[[1, 1], [2 + x^3, 1], [5 + x^3, 1], [4 + x^6, 1]]");
    same("factor_square_free_list(x^6 + 1, modulus: 3)", "[[1, 1], [1 + x^2, 3]]");
    // the product of the factors is the polynomial mod p (x^8 - 1 mod 3, 3 x^5 + x + 2 mod 5)
    for (p, f) in [(3, "x^8 - 1"), (5, "3 * x^5 + x + 2"), (7, "5 + x^12")] {
        same(
            &format!("{{ l = factor_list({f}, modulus: {p}); polynomial_mod(expand(product(map(q => q[1]^q[2], l)) - ({f})), {p}) }}"),
            "0",
        );
    }
}

#[test]
fn irreducible_when_linear_in_a_variable() {
    // documented: x^7 + 12 x y - 11 = (12 x) y + (x^7 - 11) with gcd(12 x, x^7 - 11) = 1, irreducible over every field
    same("is_irreducible_polynomial(x^7 + 12 * x * y - 11, extension: all)", "true");
    same("polynomial_gcd(12 * x, x^7 - 11)", "1");
    // y (x^2 + 1) + x^3 is linear in y with coprime coefficients; x y + x = x (y + 1) is not irreducible: no answer
    same("is_irreducible_polynomial(y * (x^2 + 1) + x^3, extension: all)", "true");
    same("is_irreducible_polynomial(x * y + x, extension: all)", "is_irreducible_polynomial(x * y + x, extension: all)");
}

// ------------------------------------------------------------------------------------------- Gaussian integers

#[test]
fn gaussian_factorisation() {
    // documented
    same("factor_integer(36, gaussian_integers: true)", "[[-1, 1], [1 + im, 4], [3, 2]]");
    same("factor_integer(5, gaussian_integers: true)", "[[-im, 1], [1 + 2*im, 1], [2 + im, 1]]");
    same("factor_integer(2, gaussian_integers: true)", "[[-im, 1], [1 + im, 2]]");
    // the product of the factors is the number, and every prime has a prime norm or is a rational prime 3 mod 4
    for z in ["65", "3 + 4 * im", "-1001", "(7 + 2 * im)^3 * 11"] {
        same(&format!("{{ f = factor_integer({z}, gaussian_integers: true); expand(product(map(q => q[1]^q[2], f))) }}"), z);
        same(
            &format!(
                "every(factor_integer({z}, gaussian_integers: true), q => abs(q[1]) == 1 or is_prime(re(q[1])^2 + im_part(q[1])^2) or (im_part(q[1]) == 0 and mod(q[1], 4) == 3))"
            ),
            "true",
        );
    }
    // 3 + 4 i = (2 + i)^2 (by hand: 4 - 1 + 4 i)
    same("factor_integer(3 + 4 * im, gaussian_integers: true)", "[[2 + im, 2]]");
}

#[test]
fn gaussian_divisor_functions() {
    // documented
    same("divisors(2, gaussian_integers: true)", "[1, 1 + im, 2]");
    same("divisor_sigma(1, 6, gaussian_integers: true)", "8 + 12*im");
    same("divisor_sigma(2, 6, gaussian_integers: true)", "-30 + 20*im");
    same("prime_nu(105, gaussian_integers: true)", "4");
    same("liouville_lambda(8, gaussian_integers: true)", "1");
    same("liouville_lambda(2, gaussian_integers: false)", "-1");
    same("is_composite([5, 3], gaussian_integers: true)", "[true, false]");
    same("is_prime_power(5, gaussian_integers: true)", "false");
    // sigma_1(6) by hand over the first-quadrant primes 1 + i (twice) and 3: (1 + (1 + i) + (1 + i)^2)(1 + 3)
    same("expand((1 + (1 + im) + (1 + im)^2) * (1 + 3))", "8 + 12*im");
    // 13 = (2 + 3 i)(3 + 2 i): two primes, nu = 2, omega = 2, sigma_0 = 4 divisors 1, 2 + 3 i, 3 + 2 i, 13
    same("[prime_nu(13, gaussian_integers: true), prime_omega(13, gaussian_integers: true), length(divisors(13, gaussian_integers: true)), divisor_sigma(0, 13, gaussian_integers: true)]", "[2, 2, 4, 4]");
    // 9 = 3^2 with 3 inert: liouville 1, moebius 0; 7 (inert) is prime: not composite
    same("[liouville_lambda(9, gaussian_integers: true), moebius_mu(9, gaussian_integers: true), is_composite(7, gaussian_integers: true)]", "[1, 0, false]");
}

#[test]
fn gaussian_polynomial_factorisation() {
    // documented
    same("factor(x^2 + 1, gaussian_integers: true)", "(-im + x)*(im + x)");
    same("factor(x^4 - 16, gaussian_integers: true)", "(-2 + x)*(-2*im + x)*(2*im + x)*(2 + x)");
    same("factor_list(x^2 + 1, gaussian_integers: true)", "[[1, 1], [-im + x, 1], [im + x, 1]]");
    // x^4 + 1 = (x^2 - i)(x^2 + i), and x^2 + 4 = (x - 2 i)(x + 2 i): the products expand back
    same("factor(x^4 + 1, gaussian_integers: true)", "(-im + x^2)*(im + x^2)");
    same("expand(factor(x^2 + 4, gaussian_integers: true))", "x^2 + 4");
    same("factor(x^2 + 4, gaussian_integers: false)", "x^2 + 4");
}

// ------------------------------------------------------------------------------------------- sides of relations

#[test]
fn sides_with_generate_conditions() {
    // documented
    same("divide_sides(a * b > c, b, generate_conditions: true)", "conditional_expression(a > c/b, b > 0)");
    same("divide_sides(a * b > c, b, generate_conditions: false)", "a > c/b");
    same("divide_sides(a * x > b, generate_conditions: automatic)", "conditional_expression(a*x/b > 1, b > 0)");
    same("divide_sides(a * x == b, generate_conditions: automatic)", "a*x/b == 1");
    same("divide_sides(a * x == b, generate_conditions: true)", "conditional_expression(a*x/b == 1, b != 0)");
    same("multiply_sides((a * x) / b == 1, b, generate_conditions: true)", "conditional_expression(a*x == b, b != 0)");
    // a sign known under the assumptions needs no condition, and a negative one reverses the inequality
    same("multiply_sides(a / b > c, b, generate_conditions: true, assumptions: b > 0)", "a > b*c");
    same("multiply_sides(a / b > c, b, generate_conditions: true, assumptions: b < 0)", "a < b*c");
    // a positive constant: no condition; without the option the piecewise over the signs stays
    same("multiply_sides(x / 3 > 1, 3, generate_conditions: true)", "x > 3");
    same("multiply_sides(a / b > c, b)", "piecewise([[a > b*c, b > 0], [a < b*c, b < 0]], a/b > c)");
}

#[test]
fn apply_sides_through_connectives() {
    // documented
    same("apply_sides(log, a == b^2 and b^2 == c^3)", "log(a) == log(b^2) and log(b^2) == log(c^3)");
    same("apply_sides(log10, conditional_expression(a / c == b / d, c != 0))", "conditional_expression(log(a/c)/log(10) == log(b/d)/log(10), c != 0)");
    same("apply_sides(sin, true)", "true");
    // a chain of three sides, and an or: every side gets f (exp(log(2)) = 2 checks the values)
    same("apply_sides(exp, x < log(2) or y == log(3))", "exp(x) < 2 or exp(y) == 3");
    same("apply_sides(f, piecewise([[x > y, a > 0]], x < y))", "piecewise([[f(x) > f(y), a > 0]], f(x) < f(y))");
}

#[test]
fn divisor_sigma_of_any_order() {
    // documented
    same("divisor_sigma(-2, 10)", "13/10");
    same("divisor_sigma(1 / 2, 12)", "2*(-1 + 2*2^(1/2))/((-1 + 2^(1/2))*(-1 + 3^(1/2)))");
    same("divisor_sigma(k, 30)", "(-1 + 2^(2*k))*(-1 + 3^(2*k))*(-1 + 5^(2*k))/((-1 + 2^k)*(-1 + 3^k)*(-1 + 5^k))");
    // second route: the sum over the divisors, at 30 digits for k = 1/2 and exactly for k = -3, n = 360
    same("chop(n(divisor_sigma(1 / 2, 12) - total(map(sqrt, divisors(12))), 30))", "0");
    same("divisor_sigma(-3, 360)", "total(map(d => d^-3, divisors(360)))");
    // the symbolic formula at k = 2 is sigma_2(30) = 1300
    same("simplify(rewrite(divisor_sigma(k, 30), k -> 2))", "divisor_sigma(2, 30)");
    // the rule is for explicit integers only: a symbolic n assumed integer keeps the solvers' search working
    // (sigma(120) = 360 = 3 * 120; sigma_2(p) = p^2 + 1 for the primes p)
    same("reduce(divisor_sigma(1, n) == 3 * n and (0 < n and n < 200), n, integers)", "n == 120");
    same("solve(divisor_sigma(2, n) == n^2 + 1 and (0 < n and n < 10), n, integers)", "[[n -> 2], [n -> 3], [n -> 5], [n -> 7]]");
    same("assuming(element(n, integers) and n > 0, divisor_sigma(1 / 2, n))", "divisor_sigma(1 / 2, n)");
    same("prime_omega(5 + 9 * im)", "2");
    // Gaussian integers without the option: 3 + i = (1 + i)(1 + 2 i) up to a unit
    same("divisor_sigma(1, 3 + im)", "2 + 6*im");
    same("[liouville_lambda(2 + im), moebius_mu(5 + 6 * im), moebius_mu(2 * im)]", "[-1, -1, 0]");
}

#[test]
fn mixed_radix_digits() {
    // documented
    same("integer_digits(437, mixed_radix([6, 5, 4, 3, 2, 1]))", "[3, 3, 0, 2, 1, 0]");
    same("integer_digits(10^5, mixed_radix([inf, 10, 9, 8, 7]))", "[19, 8, 3, 5, 5]");
    same("integer_length(1024, mixed_radix([6, 4, 2]))", "4");
    same("integer_string(654321, mixed_radix([12, 11, 10, 9, 8, 7, 6]))", "\"1a63303\"");
    // the digits give the number back: n = d_m + b_m (d_(m-1) + b_(m-1) (...)), for other numbers and radices
    for (n, bs) in [("437", "[6, 5, 4, 3, 2, 1]"), ("10^5", "[inf, 10, 9, 8, 7]"), ("98765", "[3, 7, 2]"), ("123456789", "[24, 60, 60]")] {
        same(
            &format!("{{ bs = {bs}; ds = integer_digits({n}, mixed_radix(bs)); fold((acc, i) => if i == 1 then ds[1] else acc * bs[length(bs) - (length(ds) - i)] + ds[i], 0, range(length(ds))) }}"),
            n,
        );
    }
    // uniform radices are the ordinary digits
    same("integer_digits(2024, mixed_radix([10, 10, 10, 10]))", "integer_digits(2024)");
}

#[test]
fn sequence_functions_from_the_library() {
    // documented
    same("find_sequence_function([2, 3, 5, 7, 11, 13, 17, 19, 23, 29], n)", "prime(n)");
    same("find_sequence_function(table(bell_b(n), n, 10), n)", "bell_b(n)");
    same("find_sequence_function([1, 2, 3, 4, 0, 1, 2, 3, 4, 0], n)", "mod(n, 5)");
    // data made another way: the totatives counted directly, the Catalan numbers from binomials, a period of 3
    same("find_sequence_function(table(length(select(range(k), j => gcd(j, k) == 1)), k, 12), n)", "euler_phi(n)");
    same("find_sequence_function(table(binomial(2 * k, k) / (k + 1), k, 9), n)", "catalan_number(n)");
    same("find_sequence_function(table(mod(k, 3), k, 9), n)", "mod(n, 3)");
    // the number of ways to write k as a sum of two squares, counted over the lattice points
    same(
        "find_sequence_function(table(length(select(tuples(range(-4, 4), 2), p => p[1]^2 + p[2]^2 == k)), k, 12), n)",
        "squares_r(2, n)",
    );
    // nothing in the library fits: no answer
    assert!(ev("find_sequence_function([1, 2, 3, 4, 5, 7], n)").starts_with("failure(no_sequence_function"));
}

// ------------------------------------------------------------------------------------------- cellular automata

#[test]
fn cellular_automaton_rule_forms() {
    // documented: neighbourhood rules, a function of the neighbourhood and the step, symbolic cells
    same(
        "cellular_automaton([[a, _, b] -> a, [b, _, a] -> b, [x_, _, x_] -> a], [[b], a], 4)",
        "[[b, a, a, a, a], [a, b, a, a, a], [a, a, b, a, a], [a, a, a, b, a], [a, a, a, a, b]]",
    );
    same("cellular_automaton([f, [], 1], [a, b, c], 1)", "[[a, b, c], [f([c, a, b], 1), f([a, b, c], 1), f([b, c, a], 1)]]");
    same("cellular_automaton([&(total(&1)), [], 1 / 2], [[1], 0], 3)", "[[1, 0, 0, 0], [1, 1, 0, 0], [1, 2, 1, 0], [1, 3, 3, 1]]");
    // second route: the rules of elementary rule 90 written out, as pattern rules or as a function, give rule 90
    same(
        "cellular_automaton([[1, _, 1] -> 0, [1, _, 0] -> 1, [0, _, 1] -> 1, [0, _, 0] -> 0], [[1], 0], 6)",
        "cellular_automaton(90, [[1], 0], 6)",
    );
    same("cellular_automaton([&(mod(&1[1] + &1[3], 2)), [], 1], [[1], 0], 6)", "cellular_automaton(90, [[1], 0], 6)");
    // a function of the cells as arguments: xor of the outer cells and boolean_function(90, 3) are rule 90 too
    same("cellular_automaton(&(xor(&1, &3)), [[1], 0], 6)", "cellular_automaton(90, [[1], 0], 6)");
    same("cellular_automaton(boolean_function(90, 3), [[1], 0], 6)", "cellular_automaton(90, [[1], 0], 6)");
    // p xor (q or r) is rule 30 on booleans
    same(
        "boole(cellular_automaton([[p_, q_, r_] -> xor(p, q or r)], [[true], false], 5))",
        "cellular_automaton(30, [[1], 0], 5)",
    );
    // the half-range sum is Pascal's triangle: row t holds the binomial coefficients
    same("last(cellular_automaton([&(total(&1)), [], 1 / 2], [[1], 0], 6))", "map(k => binomial(6, k), range(0, 6))");
    // documented: the centre column of the range-2 rule 69540422 is the Thue-Morse sequence
    same("1 - cellular_automaton([69540422, 2, 2], [[1], 0], [15, [[0]]])", "map(thue_morse, range(0, 15))");
}

// ------------------------------------------------------------------------------------------- groebner_basis

#[test]
fn groebner_degree_reverse_lexicographic() {
    // documented: the degrevlex basis, x > y > z
    same(
        "polys = [x^2 + y^2 + z^2 - 1, x - z + 2, z^2 - x * y]\n\
         groebner_basis(polys, [x, y, z], monomial_order: degree_reverse_lexicographic)",
        "[2 + x - z, -2*y + y*z - z^2, 3 + y^2 - 4*z + 2*z^2, -6 + 4*y + 11*z - 6*z^2 + 3*z^3]",
    );
    // second route: the degrevlex and lex bases generate the same ideal, so each reduces the other to 0 (division
    // by a Groebner basis in its own order), and the inputs reduce to 0
    same(
        "polys = [3 * x^2 + y - 5 * x - 1, 2 * x + 3 * x * y + y^2]\n\
         g = groebner_basis(polys, [x, y], monomial_order: degree_reverse_lexicographic)\n\
         l = groebner_basis(polys, [x, y])\n\
         [map(p => polynomial_reduce(p, g, [x, y], monomial_order: degree_reverse_lexicographic)[2], join(polys, l)), map(p => polynomial_reduce(p, l, [x, y])[2], g)]",
        "[[0, 0, 0, 0], [0, 0, 0]]",
    );
    // cyclic 3 in degree lex: every element lies in the ideal of the lex basis
    same(
        "polys = [x + y + z, x * y + y * z + z * x, x * y * z - 1]\n\
         g = groebner_basis(polys, [x, y, z], monomial_order: degree_lexicographic)\n\
         map(p => polynomial_reduce(p, groebner_basis(polys, [x, y, z]), [x, y, z])[2], g)",
        "[0, 0, 0]",
    );
}

#[test]
fn groebner_weight_matrix() {
    // documented weight matrix order
    same(
        "polys = [-5 * x^2 + y * z - x - 1, 2 * x + 3 * x * y + y^2, x - 3 * y + x * z - 2 * z^2]\n\
         groebner_basis(polys, [x, y, z], monomial_order: [[1, 3, 1], [-1, 2, 0], [4, -3, 2]])",
        "[-21*x - 13*x^2 - 15*x^3 + 6*z + 6*x*z + 20*x^2*z + 20*x*z^2, 60 + 39*x + 287*x^2 - 15*x^3 + 6*z - 14*x*z + \
         20*x^2*z + 40*z^3, -x + 3*y - x*z + 2*z^2, 10 - 38*x + 141*x^2 - 305*x^3 + 325*x^4 - 12*z - 82*x*z - 50*x^2*z + \
         20*z^2, 30 + 913*x + 189*x^2 + 4545*x^3 + 42*z + 1132*x*z + 630*x^2*z + 1950*x^3*z - 200*z^2]",
    );
    // the matrix [[1, 1, 1], [0, 0, -1], [0, -1, 0]] is degrevlex on three variables: the same basis
    same(
        "polys = [x^2 + y^2 + z^2 - 1, x - z + 2, z^2 - x * y]\n\
         groebner_basis(polys, [x, y, z], monomial_order: [[1, 1, 1], [0, 0, -1], [0, -1, 0]])",
        "groebner_basis(polys, [x, y, z], monomial_order: degree_reverse_lexicographic)",
    );
    // the identity matrix is lex
    same(
        "polys = [x^2 - 2 * y^2, x * y - 3]\n\
         groebner_basis(polys, [x, y], monomial_order: [[1, 0], [0, 1]])",
        "groebner_basis(polys, [x, y])",
    );
    // a singular matrix or a negative leading column entry is not a monomial order: no answer
    assert!(ev("groebner_basis([x^2 + y^2 - 1, x - y], [x, y], monomial_order: [[1, 1], [1, 1]])").starts_with("failure(argument"));
    assert!(ev("groebner_basis([x^2 + y^2 - 1, x - y], [x, y], monomial_order: [[-1, 0], [0, 1]])").starts_with("failure(argument"));
}

#[test]
fn groebner_elimination() {
    // documented: z eliminated (lex, z first)
    same(
        "groebner_basis([x^2 + y^2 + z^2 - 1, x * y - z + 2, z^2 - 2 * x + 3 * y], [x, y], [z])",
        "[28 + 8*y + 41*y^2 - 26*y^3 + 55*y^4 - 8*y^5 + 7*y^6 - 6*y^7 + y^8, 16 + 20*x - 10*y + 28*y^2 - 57*y^3 + 4*y^4 - \
         6*y^5 + 6*y^6 - y^7]",
    );
    // the twisted cubic: x = t^2, y = t^3 gives x^3 - y^2 (substitution: t^6 - t^6 = 0), in both elimination orders
    same("groebner_basis([x - t^2, y - t^3], [x, y], [t])", "[x^3 - y^2]");
    same("groebner_basis([x - t^2, y - t^3], [x, y], t, monomial_order: elimination_order)", "[x^3 - y^2]");
    // the rational circle x = (1 - t^2)/(1 + t^2), y = 2 t/(1 + t^2): x^2 + y^2 - 1
    same("groebner_basis([x * (1 + t^2) - (1 - t^2), y * (1 + t^2) - 2 * t], [x, y], [t])", "[-1 + x^2 + y^2]");
    // documented elimination order (block degrevlex)
    same(
        "polys = [-5 * x^2 + y * z - x - 1, 2 * x + 3 * x * y + y^2, x - 3 * y + x * z - 2 * z^2]\n\
         groebner_basis(polys, [x, y], z, monomial_order: elimination_order)",
        "[2*x + 3*x*y + y^2, 486 + 1142*x + 5688*x^2 + 5670*x^3 + 12150*x^4 + 85*y^2 + 723*y^3 + 45*y^4, 2916 - 14944*x - \
         14760*x^2 - 16200*x^3 + 4374*y - 10388*y^2 + 16311*y^3 + 2187*y^4 + 1755*y^5]",
    );
}

#[test]
fn groebner_over_a_prime_field() {
    // documented
    same(
        "groebner_basis([x^3 - 2 * x * y + y^2, y^3 - x^2 * y + 3], [x, y], modulus: 5)",
        "[2 + 4*y^2 + 2*y^3 + 2*y^4 + y^5 + 4*y^6 + 3*y^7 + y^8 + y^9, 4 + x + y + 4*y^3 + y^7 + 4*y^6 + 4*y^8]",
    );
    // by hand: over Q the basis is [2 y^2 - 1, x - y]; mod 3, 2^-1 = 2, so 2 (2 y^2 - 1) = y^2 + 1 and x - y = x + 2 y
    same("groebner_basis([x^2 + y^2 - 1, x - y], [x, y], modulus: 3)", "[1 + y^2, x + 2*y]");
    // mod 2: x + y reduces x^2 + y^2 + 1 to 2 y^2 + 1 = 1, the whole ring
    same("groebner_basis([x^2 + y^2 - 1, x - y], [x, y], modulus: 2)", "[1]");
    // a large prime: the lex basis over Q reduced mod p and made monic (no leading coefficient vanishes mod p)
    same(
        "p = 1000003\n\
         q = groebner_basis([x^2 - 2 * y^2, x * y - 3], [x, y])\n\
         g = groebner_basis([x^2 - 2 * y^2, x * y - 3], [x, y], modulus: p)\n\
         [g[1] - polynomial_mod(expand(q[1] * power_mod(2, -1, p)), p), g[2] - polynomial_mod(expand(q[2] * power_mod(3, -1, p)), p)]",
        "[0, 0]",
    );
    assert!(ev("groebner_basis([x^2 - 1], [x], modulus: 4)").starts_with("failure(argument"));
}
