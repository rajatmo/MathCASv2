//! Round 38: how the parts of a complex number, abs, sign and arg treat a numeric factor; the convergence condition of
//! an infinite sum with generate_conditions; the prelude's functions that moved to the lists and special-function
//! libraries.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn abs_and_sign_take_a_numeric_factor_out_together() {
    // abs(c y) = |c| abs(y) as sign(c y) = (c/|c|) sign(y), so abs(u) sign(u) = u still cancels in a transform:
    // the Fourier transform of p sqrt(pi) (cos t + sin t) exp(-p^2) with the kernel exp(-2 pi i k p)
    // (the same term, not only an equal one: no abs or sign is left)
    assert_eq!(
        ev("fourier_transform(p*sqrt(pi)*(cos(t) + sin(t))*exp(-p^2), p, k, fourier_parameters: [0, -2*pi]) === -im*k*(cos(t) + sin(t))*exp(-k^2*pi^2)*pi^2"),
        "true"
    );
    // integral of 3 p exp(-4 p^2) exp(-2 pi i k p): -i w/(2 a) sqrt(pi/a) exp(-w^2/(4 a)) times 3, w = 2 pi k, a = 4
    assert_eq!(
        ev("fourier_transform(3*p*exp(-4*p^2), p, k, fourier_parameters: [0, -2*pi]) === -3*im/8*k*exp(-k^2*pi^2/4)*pi^(3/2)"),
        "true"
    );
    // the default kernel exp(i k p)/sqrt(2 pi): i k/(2 sqrt 2) exp(-k^2/4)
    assert_eq!(ev("fourier_transform(p*exp(-p^2), p, k) === im*k*sqrt(2)/4*exp(-k^2/4)"), "true");
    // the product of the two keeps one argument; substituted, both sides are -2 z at z = 3 - 4 i
    assert_eq!(ev("abs(-2*z)*sign(-2*z)"), "-2*abs(z)*sign(z)");
    assert_eq!(ev("abs(-2*z)*sign(-2*z) /. z -> 3 - 4*im"), "complex(-6, 8)");
    assert_eq!(ev("[abs(-2*k*pi), abs((3 + 4*im)*z), abs(2*z) /. z -> 3 - 4*im]"), "[2*abs(k*pi), 5*abs(z), 10]");
}

#[test]
fn parts_take_a_numeric_factor_out() {
    // conjugate is multiplicative; re and im_part are real-linear: re((a + b i) y) = a re(y) - b im(y)
    assert_eq!(ev("d(conjugate(x^3), x)"), "3*conjugate(x^2)");
    assert_eq!(ev("[conjugate(2*z), conjugate(-5*im*z), re(2*z), im_part(2*z)]"), "[2*conjugate(z), 5im*conjugate(z), 2*re(z), 2*im_part(z)]");
    assert_eq!(ev("re((2 - 3*im)*z)"), "2*re(z) + 3*im_part(z)");
    assert_eq!(ev("im_part((2 - 3*im)*z)"), "-3*re(z) + 2*im_part(z)");
    // substitution against the product computed first: (2 - 3 i)(1 + 2 i) = 8 + i
    assert_eq!(ev("[re((2 - 3*im)*z), im_part((2 - 3*im)*z)] /. z -> 1 + 2*im"), "[8, 1]");
    // arg(c y) = arg(y) for c > 0 only: arg(3 z) at z = -1 - i is arg(-1 - i) = -3 pi/4; a negative c stays
    assert_eq!(ev("[arg(3*z), arg(-2*z)]"), "[arg(z), arg(-2*z)]");
    assert_eq!(ev("arg(3*z) /. z -> -1 - im"), "(-3/4)*pi");
}

#[test]
fn infinite_sums_carry_their_convergence_condition() {
    // sum 1/n^a = zeta(a) converges for re(a) > 1 (comparison with n^-p)
    assert_eq!(ev("sum(1/n^a, [n, inf], generate_conditions: true)"), "conditional_expression(zeta(a), re(a) > 1)");
    // from n = 2 the sum is zeta(a) - 1 under the same condition; with 2 a: re(2 a) > 1
    assert_eq!(ev("sum(1/n^a, [n, 2, inf], generate_conditions: true)"), "conditional_expression(-1 + zeta(a), re(a) > 1)");
    assert_eq!(ev("sum(1/n^(2*a), [n, 1, inf], generate_conditions: true)"), "conditional_expression(zeta(2*a), 2*re(a) > 1)");
    // the condition agrees with sum_convergence, and is dropped when it holds or is implied
    assert_eq!(ev("sum_convergence(1/n^a, n)"), "re(a) > 1");
    assert_eq!(ev("sum(1/n^2, [n, inf], generate_conditions: true)"), "(1/6)*pi^2");
    assert_eq!(ev("assuming(a > 2, sum(1/n^a, [n, inf], generate_conditions: true))"), "zeta(a)");
    // a geometric series keeps its ratio condition
    assert_eq!(ev("sum(a^k, [k, 0, inf], generate_conditions: true)"), "conditional_expression((1 - a)^-1, abs(a) < 1)");
}

#[test]
fn moved_library_functions_still_work() {
    // the Levenshtein distance kitten -> sitting is 3 (two substitutions, one insertion); of a list against itself 0
    assert_eq!(ev("[edit_distance(\"kitten\", \"sitting\"), edit_distance([1, 2, 3], [1, 2, 3]), hamming_distance(\"abc\", \"abd\")]"), "[3, 0, 1]");
    assert_eq!(ev("[palindrome_q(12321), palindrome_q(\"ab\"), chessboard_distance([1, 5], [4, 3]), squared_euclidean_distance([1, 5], [4, 3])]"), "[true, false, 3, 13]");
    assert_eq!(ev("[commonest([a, b, a, c, b, a], up_to(2)), reverse_sort_by([3, 1, 2], x => x), ordering_by([c, a, b], x => x)]"), "[[a, b], [3, 2, 1], [2, 3, 1]]");
    assert_eq!(ev("delete_missing([1, missing(), [2, missing()]], 2)"), "[1, [2]]");
    // prime_zeta_p(2) = 0.4522474200410654985..., the sum of 1/p^2 over the primes
    assert_eq!(ev("prime_zeta_p(2.0)"), "0.45224742004106544");
}

#[test]
fn limits_of_moduli_of_products() {
    // |u1 u2| = |u1| |u2|: (sqrt(z) - i)^3/(z + 1) at z = -1 - i t is about 8 i / t, so its modulus grows and the
    // cube is not o(z + 1) along that direction
    assert_eq!(ev("asymptotic_less((sqrt(z) - im)^3, z + 1, z -> -1, direction: im)"), "false");
    assert_eq!(ev("limit(abs((-im - im*(1 + im*t)^(1/2))^3/t), t -> 0, direction: from_above)"), "inf");
    // |(2 i + t)/t^2| = sqrt(4 + t^2)/t^2 grows; |(3 i + t)^2 t| = (9 + t^2) |t| tends to 0 from both sides
    assert_eq!(ev("limit(abs((2*im + t)/t^2), t -> 0, direction: from_above)"), "inf");
    assert_eq!(ev("limit(abs((3*im + t)^2*t), t -> 0)"), "0");
}
