//! Every native through source text: normal cases, edge cases, failures, list threading and unevaluated calls.
mod common;
use common::{check_all, Nt};

#[test]
fn mod_and_quotient() {
    check_all(&[
        ("mod(7, 3)", "1"),
        ("mod(-7, 3)", "2"),
        ("mod(7, -3)", "-2"),
        ("mod(-7, -3)", "-1"),
        ("mod(6, 3)", "0"),
        ("mod(10, 3, 1)", "1"),
        ("mod(10, 3, -1)", "1"),
        ("mod(2^100, 7)", "2"),
        ("mod(-2^100, 2^64 + 1)", "68719476736"),
        ("mod(7/2, 2)", "3/2"),
        ("mod(-1/3, 1)", "2/3"),
        ("mod(5.5, 2)", "1.5"),
        ("mod(-5.5, 2)", "0.5"),
        ("mod(5, 0)", "failure(division_by_zero)"),
        ("mod(x, 3)", "mod(x, 3)"),
        ("mod([1, 2, 3, 4], 3)", "[1, 2, 0, 1]"),
        ("mod([1, 2], [2, 3, 4])", "failure(shape_mismatch)"),
        ("quotient(7, 2)", "3"),
        ("quotient(-7, 2)", "-4"),
        ("quotient(7, -2)", "-4"),
        ("quotient(-7, -2)", "3"),
        ("quotient(2^200 + 5, 2^100)", "1267650600228229401496703205376"),
        ("quotient(-2^200, 3)", "-535646014752996758513987364113720867507400997927597611767126"),
        ("quotient(7/2, 1/3)", "10"),
        ("quotient(7.5, 2)", "3"),
        ("quotient(1, 0)", "failure(division_by_zero)"),
    ]);
}

#[test]
fn gcd_and_lcm() {
    check_all(&[
        ("gcd(12, 18)", "6"),
        ("gcd(-12, 18)", "6"),
        ("gcd(0, 0)", "0"),
        ("gcd()", "0"),
        ("gcd(7)", "7"),
        ("gcd(-7)", "7"),
        ("gcd(12, 18, 8)", "2"),
        ("gcd(2^100, 6^50)", "1125899906842624"),
        ("gcd(1/2, 1/3)", "1/6"),
        ("gcd(2/3, 4/9)", "2/9"),
        ("lcm(4, 6)", "12"),
        ("lcm(4, 6, 10)", "60"),
        ("lcm()", "1"),
        ("lcm(0, 5)", "0"),
        ("lcm(-4, 6)", "12"),
        ("lcm(1/2, 1/3)", "1"),
        ("lcm(2/3, 4/9)", "4/3"),
        // polynomials go to mc-poly's gcd (the owner of the name)
        ("gcd(x, 2)", "1"),
        ("gcd(2x, 4)", "2"),
        ("extended_gcd(240, 46)", "[2, [-9, 47]]"),
        ("extended_gcd(0, 5)", "[5, [0, 1]]"),
        ("extended_gcd(-6, 4)", "[2, [1, 2]]"),
    ]);
}

#[test]
fn rounding() {
    check_all(&[
        ("floor(7/2)", "3"),
        ("floor(-7/2)", "-4"),
        ("floor(5)", "5"),
        ("floor(2.7)", "2"),
        ("floor(-2.7)", "-3"),
        ("floor(1.0e20)", "100000000000000000000"),
        ("floor(x)", "floor(x)"),
        ("floor(7, 2)", "6"),
        ("floor(7/2, 1/3)", "10/3"),
        ("ceiling(7/2)", "4"),
        ("ceiling(-7/2)", "-3"),
        ("ceiling(2.1)", "3"),
        ("ceiling(5)", "5"),
        ("round(5/2)", "2"),
        ("round(7/2)", "4"),
        ("round(-5/2)", "-2"),
        ("round(2.5)", "2"),
        ("round(3.5)", "4"),
        ("round(-3.5)", "-4"),
        ("round(2.4999)", "2"),
        ("round(17, 5)", "15"),
        ("round(2.6 + 3.5im)", "complex(3, 4)"),
        ("floor(3.7 - 1.2im)", "complex(3, -2)"),
        ("floor(1.25~30)", "1"),
        ("round(2.5~30)", "2"),
        ("floor([1/2, 3/2, -1/2])", "[0, 1, -1]"),
        ("floor(1/0)", "failure(division_by_zero)"),
    ]);
}

#[test]
fn abs_sign_numerator_denominator() {
    check_all(&[
        ("abs(-3)", "3"),
        ("abs(-2^70)", "1180591620717411303424"),
        ("abs(-3/4)", "3/4"),
        ("abs(-2.5)", "2.5"),
        ("abs(3 + 4im)", "5"),
        ("abs(1 + im)", "2^(1/2)"),
        ("abs(x)", "abs(x)"),
        ("sign(-3)", "-1"),
        ("sign(0)", "0"),
        ("sign(2/3)", "1"),
        ("sign(-0.5)", "-1"),
        ("sign(3im)", "1im"),
        ("sign(x)", "sign(x)"),
        ("numerator(6/4)", "3"),
        ("denominator(6/4)", "2"),
        ("numerator(-5)", "-5"),
        ("denominator(-5)", "1"),
        ("numerator(x/y)", "x"),
        ("denominator(x/y)", "y"),
        ("denominator(2*x/(3*y^2))", "3*y^2"),
        ("numerator(2*x/(3*y^2))", "2*x"),
        ("denominator(x)", "1"),
        ("numerator(2.5)", "2.5"),
    ]);
}

#[test]
fn factorials_and_binomials() {
    check_all(&[
        ("factorial(0)", "1"),
        ("factorial(5)", "120"),
        ("factorial(25)", "15511210043330985984000000"),
        ("integer_length(factorial(10000))", "35660"),
        ("factorial(-1)", "factorial(-1)"),
        ("factorial(1/2)", "factorial(1/2)"),
        ("factorial(10^9)", "failure(limit, memory, factorial(1000000000))"),
        ("factorial([3, 4])", "[6, 24]"),
        ("binomial(10, 3)", "120"),
        ("binomial(10, 0)", "1"),
        ("binomial(10, 11)", "0"),
        ("binomial(10, -1)", "0"),
        ("binomial(0, 0)", "1"),
        ("binomial(-3, 2)", "6"),
        ("binomial(-3, 3)", "-10"),
        ("binomial(-3, -5)", "6"),
        ("binomial(-3, -1)", "0"),
        ("binomial(100, 50)", "100891344545564193334812497256"),
        ("binomial(1/2, 3)", "1/16"),
        ("binomial(-1/2, 2)", "3/8"),
        ("binomial(x, 2)", "binomial(x, 2)"),
    ]);
}

#[test]
fn digits_lengths_exponents() {
    check_all(&[
        ("integer_digits(1234)", "[1, 2, 3, 4]"),
        ("integer_digits(-1234)", "[1, 2, 3, 4]"),
        ("integer_digits(0)", "[0]"),
        ("integer_digits(10, 2)", "[1, 0, 1, 0]"),
        ("integer_digits(255, 16)", "[15, 15]"),
        ("integer_digits(1234, 10, 6)", "[0, 0, 1, 2, 3, 4]"),
        ("integer_digits(1234, 10, 2)", "[3, 4]"),
        ("integer_digits(2^64, 2^32)", "[1, 0, 0]"),
        ("integer_digits(100, 1)", "integer_digits(100, 1)"),
        ("from_digits([1, 2, 3])", "123"),
        ("from_digits([1, 0, 1, 0], 2)", "10"),
        ("from_digits([1, 15], 16)", "31"),
        ("from_digits([])", "0"),
        ("integer_length(0)", "0"),
        ("integer_length(999)", "3"),
        ("integer_length(1000)", "4"),
        ("integer_length(-1000)", "4"),
        ("integer_length(255, 2)", "8"),
        ("integer_length(256, 16)", "3"),
        ("integer_length(10^100)", "101"),
        ("integer_exponent(1000)", "3"),
        ("integer_exponent(96, 2)", "5"),
        ("integer_exponent(96, 4)", "2"),
        ("integer_exponent(-96, 6)", "1"),
        ("integer_exponent(7, 2)", "0"),
        ("integer_exponent(0, 2)", "integer_exponent(0, 2)"),
        ("integer_exponent(3^1000, 9)", "500"),
    ]);
}

#[test]
fn primes() {
    check_all(&[
        ("is_prime(2)", "true"),
        ("is_prime(1)", "false"),
        ("is_prime(0)", "false"),
        ("is_prime(-7)", "true"),
        ("is_prime(91)", "false"),
        ("is_prime(2^61 - 1)", "true"),
        ("is_prime(2^127 - 1)", "true"),
        ("is_prime(2^128 + 1)", "false"),
        ("is_prime(7/2)", "false"),
        ("is_prime(x)", "false"),
        ("is_prime([2, 3, 4])", "[true, true, false]"),
        ("next_prime(1)", "2"),
        ("next_prime(-10)", "2"),
        ("next_prime(2)", "3"),
        ("next_prime(10^20)", "100000000000000000039"),
        ("next_prime(10.5)", "11"),
        ("next_prime(10, 3)", "17"),
        ("next_prime(10, -2)", "5"),
        ("next_prime(3, -2)", "next_prime(3, -2)"),
        ("prime(1)", "2"),
        ("prime(1000)", "7919"),
        ("prime(10^6)", "15485863"),
        ("prime(0)", "prime(0)"),
        ("prime_pi(1)", "0"),
        ("prime_pi(2)", "1"),
        ("prime_pi(100)", "25"),
        ("prime_pi(100.5)", "25"),
        ("prime_pi(-5)", "0"),
        ("prime_pi(10^9)", "50847534"),
        ("prime_pi(10^20)", "failure(limit, memory, prime_pi(100000000000000000000))"),
    ]);
}

#[test]
fn factorisations() {
    check_all(&[
        ("factor_integer(360)", "[[2, 3], [3, 2], [5, 1]]"),
        ("factor_integer(1)", "[[1, 1]]"),
        ("factor_integer(0)", "[[0, 1]]"),
        ("factor_integer(-12)", "[[-1, 1], [2, 2], [3, 1]]"),
        ("factor_integer(-1)", "[[-1, 1]]"),
        ("factor_integer(97)", "[[97, 1]]"),
        ("factor_integer(3/4)", "[[2, -2], [3, 1]]"),
        ("factor_integer(2^128 + 1)", "[[59649589127497217, 1], [5704689200685129054721, 1]]"),
        ("factor_integer(2^64 - 1)", "[[3, 1], [5, 1], [17, 1], [257, 1], [641, 1], [65537, 1], [6700417, 1]]"),
        ("factor_integer(10^30)", "[[2, 30], [5, 30]]"),
        ("factor_integer(x)", "factor_integer(x)"),
    ]);
}

#[test]
fn modular_functions() {
    check_all(&[
        ("power_mod(2, 10, 1000)", "24"),
        ("power_mod(2, -1, 7)", "4"),
        ("power_mod(-2, 3, 7)", "6"),
        ("power_mod(3, 2, -7)", "-5"),
        ("power_mod(2, -1, 4)", "power_mod(2, -1, 4)"),
        ("power_mod(2, 10, 0)", "failure(division_by_zero)"),
        ("power_mod(3, 10^100, 10^9 + 7)", "9102203"),
        ("power_mod(2, 2^127 - 2, 2^127 - 1)", "1"),
        ("power_mod(5, 0, 1)", "0"),
        ("primitive_root(7)", "3"),
        ("primitive_root(8)", "primitive_root(8)"),
        ("primitive_root(1)", "0"),
        ("primitive_root(2)", "1"),
        ("primitive_root(4)", "3"),
        ("primitive_root(18)", "11"),
        ("primitive_root(10)", "7"),
        ("primitive_root(10, 4)", "7"),
        ("primitive_root(40487^2)", "10"),
        ("primitive_root(prime(1000)^50)", "7"),
        ("euler_phi(36)", "12"),
        ("euler_phi(1)", "1"),
        ("euler_phi(0)", "0"),
        ("euler_phi(-36)", "12"),
        ("euler_phi(2^61 - 1)", "2305843009213693950"),
        ("divisors(12)", "[1, 2, 3, 4, 6, 12]"),
        ("divisors(-12)", "[1, 2, 3, 4, 6, 12]"),
        ("divisors(1)", "[1]"),
        ("divisors(0)", "divisors(0)"),
        ("divisor_sigma(1, 12)", "28"),
        ("divisor_sigma(0, 12)", "6"),
        ("moebius_mu(30)", "-1"),
        ("moebius_mu(12)", "0"),
        ("moebius_mu(1)", "1"),
        ("jacobi_symbol(2, 7)", "1"),
        ("jacobi_symbol(3, 7)", "-1"),
        ("jacobi_symbol(-1, 7)", "-1"),
        ("jacobi_symbol(1, 8)", "jacobi_symbol(1, 8)"),
        ("multiplicative_order(2, 7)", "3"),
        ("multiplicative_order(10, 7)", "6"),
        ("multiplicative_order(2, 6)", "multiplicative_order(2, 6)"),
    ]);
}

#[test]
fn rules_can_call_the_natives() {
    let mut nt = Nt::new();
    nt.eval("fn collatz(1) = 0");
    nt.eval("fn collatz(n: Int) = if mod(n, 2) == 0 then 1 + collatz(quotient(n, 2)) else 1 + collatz(3n + 1)");
    assert_eq!(nt.eval("collatz(27)"), "111");
    assert_eq!(nt.eval("map(euler_phi, [1, 2, 3, 4, 5, 6])"), "[1, 1, 2, 2, 4, 2]");
}

#[test]
#[ignore = "needs the lambdas of package H"]
fn natives_inside_lambdas() {
    check_all(&[("map(k => mod(k^2, 7), [1, 2, 3])", "[1, 4, 2]"), ("map(n => is_prime(n), [2, 4])", "[true, false]")]);
}
