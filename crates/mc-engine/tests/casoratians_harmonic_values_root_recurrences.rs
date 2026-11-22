//! Casoratians of recurrences with periodic, polynomial and binomial coefficients; generating functions of sin and
//! cos terms, of conjugates and at compound points; interval, centered-interval and around values of the
//! hyperharmonic and alternating harmonic numbers; the properties of hyperharmonic numbers of a symbolic order;
//! sums of hyperharmonic numbers, harmonic sums from 0 and recurrences with harmonic right sides; recurrences in a root
//! of the index; nested numeric sums and products; infinite products of gamma ratios; the even and odd halves of the
//! binomial theorem; generating functions found for Euler products and rational terms. Each answer is checked by a
//! route that does not share its computation (iteration of the recurrence, determinants of iterated solutions,
//! Taylor coefficients, direct sums, values from 30-digit references, substitution into the equation), and calls
//! outside each new region are checked to end.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

/// `src` returns within `secs` seconds (generous: the calls take milliseconds) with an answer that is not a failure.
fn ends(e: &mut CoreEngine, src: &str, secs: u64) {
    let t0 = std::time::Instant::now();
    let r = ev(e, src);
    assert!(t0.elapsed() < std::time::Duration::from_secs(secs), "{src}: {:?}", t0.elapsed());
    assert!(!r.starts_with("failure(") && !r.starts_with("error"), "{src}: {r}");
}

/// An engine with `close(a, b)`: a and b agree to 12 digits (machine values).
fn engine() -> CoreEngine {
    let mut e = CoreEngine::new();
    ev(&mut e, "fn close(a, b) = { let dd = n(a) - n(b); abs(re(dd)) + abs(im_part(dd)) < 10^-12 * (1 + abs(n(b))) }");
    e
}

#[test]
fn casoratian_of_periodic_and_polynomial_coefficients() {
    let mut e = engine();
    // C(n) against y1(n) y2(n + 1) - y2(n) y1(n + 1) of the solutions iterated from the unit initial values
    ev(&mut e, "fn it2(c, n0, m) = { var a = [1, 0]; var b = [0, 1]; for j in range(n0, n0 + m) { a = append(a, c(j)*a[j - n0 + 1]); b = append(b, c(j)*b[j - n0 + 1]) }; map(i => a[i]*b[i + 1] - b[i]*a[i + 1], range(m)) }");
    ev(&mut e, "c30 = casoratian(y(n + 2) - sin(n*2*pi/5 + pi/3)*y(n) == 0, y, n)");
    holds(&mut e, "every(range(0, 11), j => close(rewrite(c30, n -> j), it2(i => sin(2*pi*i/5 + pi/3), 0, 12)[j + 1]))");
    // a_0(0) = 0: the set taken at n = 1
    ev(&mut e, "c31 = casoratian(y(n + 2) - n*a^n*sin(n*2*pi/5 + pi/3)*y(n) == 0, y, n)");
    holds(&mut e, "every(range(1, 11), j => close(rewrite(c31, [n -> j, a -> 13/10]), it2(i => i*(13/10)^i*sin(2*pi*i/5 + pi/3), 1, 11)[j]))");
    assert_eq!(ev(&mut e, "casoratian(y(n + 2) - n*y(n) == 0, y, n)"), "-1*factorial(-1 + n)*(-1)^n");
    // order 3 with a binomial coefficient: the determinant of the iterated solutions
    ev(&mut e, "c3 = casoratian(y(n + 3) - y(n + 2) + binomial(2*n, n)*y(n) == 0, y, n)");
    ev(&mut e, "sols = map(i => { var v = table(if j == i then 1 else 0, [j, 0, 2]); for m in range(0, 8) { v = append(v, v[m + 3] - binomial(2*m, m)*v[m + 1]) }; v }, range(0, 2))");
    holds(&mut e, "every(range(0, 6), m => close(rewrite(c3, n -> m), det(table(sols[i][m + j + 1], [j, 0, 2], [i, 1, 3]))))");
}

#[test]
fn generating_functions_of_trigonometric_terms_and_conjugates() {
    let mut e = engine();
    ev(&mut e, "g = generating_function(sin(2*n*pi/3)*(2/3)^n, n, z)");
    holds(&mut e, "every(range(0, 8), j => simplify(series_coefficient(g, [z, 0, j]) - sin(2*j*pi/3)*(2/3)^j) === 0)");
    ev(&mut e, "g2 = generating_function(cos(3*n + 1), n, z)");
    holds(&mut e, "every(range(0, 6), j => close(series_coefficient(g2, [z, 0, j]), cos(3*j + 1)))");
    assert_eq!(ev(&mut e, "generating_function(conjugate(f(n)), n, z)"), "conjugate(generating_function(f(n), n, conjugate(z)))");
    assert_eq!(ev(&mut e, "generating_function(sin(n*2*(pi/3))*(2/3)^n, n, exp(im*w))"), "3*3^(1/2)*exp(1im*w)/(9 + 4*exp(2im*w) + 6*exp(1im*w))");
    // a number for the point keeps the direct path (no closed form put at a pole)
    ends(&mut e, "generating_function(2^n, n, 1/2)", 5);
}

#[test]
fn harmonic_type_numbers_at_intervals_and_around() {
    let mut e = engine();
    // the two points of interval(2.1, 2.2) and the values there (30-digit references)
    ev(&mut e, "v = hyper_harmonic_number(4, interval(2.1, 2.2))");
    holds(&mut e, "length(v) == 2 and v[1][1] <= 5.0557170495867546 and v[1][2] >= 5.0557170495867546 and v[2][1] <= 5.6565501371460333 and v[2][2] >= 5.6565501371460333");
    ev(&mut e, "c = hyper_harmonic_number(4, centered_interval(3/4, 1/1000))");
    holds(&mut e, "abs(c[1] - 0.5807676479372268) <= c[2] and abs(c[1] - 0.5793662346855325) <= c[2] and abs(c[1] - 0.5821711211671652) <= c[2]");
    // around: the derivative 5.779421039490878 (30-digit central difference of the gamma form)
    ev(&mut e, "a = hyper_harmonic_number(4, around(2.1, 0.01))");
    holds(&mut e, "close(a[1], 5.0557170495867546) and abs(a[2] - 0.05779421039490878) < 10^-12");
    ev(&mut e, "b = alternating_harmonic_number(3/2, around(2.1, 0.01))");
    holds(&mut e, "close(b[1], 0.8323497836534473 + 0.0999429096598852im) and close(b[2], 0.0009636816317241271 + 0.0008047732383740231im)");
    ev(&mut e, "c2 = alternating_harmonic_number(1/2, centered_interval(3/4, 1/1000))");
    holds(&mut e, "abs(re(c2[1]) - 0.6511156799649283) <= re(c2[2]) and abs(im_part(c2[1]) + 0.4505401607389304) <= im_part(c2[2])");
    // poles in the range or at the centre: no value, and the calls end
    ends(&mut e, "hyper_harmonic_number(4, interval([-5.5, 2.2]))", 5);
    assert_eq!(ev(&mut e, "hyper_harmonic_number(4, around(-3, 0.01))"), "hyper_harmonic_number(4, around(-3.0, 0.01))");
    ends(&mut e, "alternating_harmonic_number(1/2, interval([0.5, 1.5]))", 5);
    ends(&mut e, "alternating_harmonic_number(interval([-3, 2]), 2)", 5);
}

#[test]
fn hyperharmonic_numbers_of_a_symbolic_order() {
    let mut e = engine();
    // H(p, 3) = (3 p^2 + 6 p + 2)/6: not onto; H(p, 2) = p + 1/2: onto
    assert_eq!(ev(&mut e, "function_surjective(hyper_harmonic_number(p, 3), p)"), "false");
    assert_eq!(ev(&mut e, "function_surjective(hyper_harmonic_number(p, 2), p)"), "true");
    holds(&mut e, "close(hyper_harmonic_number(0.3, 3), (3*0.3^2 + 6*0.3 + 2)/6)");
    assert_eq!(ev(&mut e, "function_meromorphic(hyper_harmonic_number(p, n), n)"), "true");
    // the domain at sample points: p = 1/3 and n = -4/3 (n + p = -1, a pole) is out; p = 1/2, n = -1 (H = 0) in;
    // p = -1 (H = -1/(n (n - 1))) has poles at n = 0, 1 only
    ev(&mut e, "dom = function_domain(hyper_harmonic_number(p, n), n)");
    holds(&mut e, "rewrite(dom, [p -> 1/3, n -> -4/3]) === false and rewrite(dom, [p -> 1/2, n -> -1]) === true");
    holds(&mut e, "rewrite(dom, [p -> -1, n -> 1]) === false and rewrite(dom, [p -> -1, n -> -1]) === true and rewrite(dom, [p -> -1, n -> 2]) === true");
    ev(&mut e, "sp = function_singularities(hyper_harmonic_number(p, n), p)");
    holds(&mut e, "rewrite(sp, [n -> 1/3, p -> -4/3]) === true and rewrite(sp, [n -> 2, p -> -5]) === false");
    assert_eq!(ev(&mut e, "function_sign([alternating_factorial(z), z > 0], z)"), "1");
    // z > -1 takes in (-1, 0], where alternating_factorial(0) = 0: no sign from the rule
    holds(&mut e, "not (function_sign([alternating_factorial(z), z > -1], z) === 1)");
}

#[test]
fn harmonic_sums_and_recurrences_with_harmonic_right_sides() {
    let mut e = engine();
    ev(&mut e, "s = sum(hyper_harmonic_number(2, m)*harmonic_number(m), [m, 1, k])");
    holds(&mut e, "every(range(1, 7), j => rewrite(s, k -> j) == total(map(i => (i + 1)*(harmonic_number(i + 1) - 1)*harmonic_number(i), range(j))))");
    ev(&mut e, "x1 = r_solve_value([x(k) - 2*x(k - 1) == harmonic_number(k, 2)^2, x(1) == 1], x(k), k)");
    holds(&mut e, "rewrite(x1, k -> 1) == 1 and every(range(2, 7), j => rewrite(x1, k -> j) - 2*rewrite(x1, k -> j - 1) == harmonic_number(j, 2)^2)");
    ev(&mut e, "x2 = r_solve_value([x(k) - 2*x(k - 1) == multiple_harmonic_number(k, [2, 3])/k, x(1) == 1], x(k), k)");
    holds(&mut e, "rewrite(x2, k -> 1) == 1 and every(range(2, 6), j => rewrite(x2, k -> j) - 2*rewrite(x2, k -> j - 1) == multiple_harmonic_number(j, [2, 3])/j)");
    ev(&mut e, "h = sum(harmonic_number(i)*stirling_s2(i, 2)*i^2, [i, 1, m])");
    holds(&mut e, "free_of(h, sum) and free_of(h, harmonic_number(m, -1, 2)) and every(range(1, 7), j => rewrite(h, m -> j) == total(map(i => harmonic_number(i)*stirling_s2(i, 2)*i^2, range(j))))");
    assert_eq!(ev(&mut e, "harmonic_number(m, -1, 2)"), "2 - 2^(1 + m) + m*2^(1 + m)");
    // the infinite sums of hyperharmonic numbers keep their quick answer
    ends(&mut e, "sum(hyper_harmonic_number(2, m)*x^m, [m, 0, inf])", 3);
    // a rational summand with complex poles is checked in machine numbers (it took 7 s at 30 digits)
    ends(&mut e, "sum((m + 3)/((m^2 + 2)*(m + 1)), [m, 1, k])", 2);
}

#[test]
fn recurrences_in_a_root_of_the_index() {
    let mut e = engine();
    ev(&mut e, "s = r_solve_value(a(n) == sqrt(n)*a(sqrt(n)) + n, a(n), n)");
    holds(&mut e, "every([3, 50, 10^9], v => close(rewrite(s, [n -> v, c(1) -> 137/100]) - sqrt(v)*rewrite(s, [n -> sqrt(v), c(1) -> 137/100]) - v, 0))");
    ev(&mut e, "s3 = r_solve_value(a(n) == 2*a(n^(1/3)) + log(n), a(n), n)");
    holds(&mut e, "every([5, 70], v => close(rewrite(s3, [n -> v, c(1) -> 3/7]) - 2*rewrite(s3, [n -> v^(1/3), c(1) -> 3/7]) - log(v), 0))");
}

#[test]
fn nested_numeric_sums_and_products_of_gamma_ratios() {
    let mut e = engine();
    // 30-digit references: sum (-1)^n Li2(2/n), and the pairwise-summed logarithms of the inner products
    holds(&mut e, "close(n_sum((-1)^n*(2/n)^k/k^2, [n, 2, inf], [k, 1, inf]), 1.1443442096919822)");
    holds(&mut e, "abs(n_product(1 + (-1)^n*(2/n)^k/k^2, [n, 2, inf], [k, 1, inf]) - 0.8106745111755890) < 10^-12");
    assert_eq!(ev(&mut e, "product(1 + 1/(1 + i + j)^3, [i, inf], [j, inf])"), "2*barnes_g(5/2 + complex(0, -1/2)*3^(1/2))*barnes_g(5/2 + complex(0, 1/2)*3^(1/2))");
    // prod_(m >= 3) (1 + 1/m^3)^(m - 2) = 1.270892163977646 (the same pairs of i and j)
    holds(&mut e, "close(product(1 + 1/(1 + i + j)^3, [i, inf], [j, inf]), 1.2708921639776461)");
    // moments that do not cancel: no closed form (the product diverges)
    holds(&mut e, "not free_of(product(gamma(i + 2)/gamma(i + 1), [i, 1, inf]), product)");
}

#[test]
fn binomial_halves_and_difference_roots() {
    let mut e = engine();
    ev(&mut e, "s = sum(binomial(2*m + 1, 2*k)*3^k, [k, 0, m])");
    holds(&mut e, "every(range(0, 8), j => expand(rewrite(s, m -> j)) == total(map(k => binomial(2*j + 1, 2*k)*3^k, range(0, j))))");
    ev(&mut e, "t = sum(binomial(2*m, 2*k + 1)*2^k, [k, 0, m - 1])");
    holds(&mut e, "every(range(1, 8), j => expand(rewrite(t, m -> j)) == total(map(k => binomial(2*j, 2*k + 1)*2^k, range(0, j - 1))))");
    ev(&mut e, "dr = difference_root_reduce(rewrite(sum(binomial(2*m + 1, 2*k)*(3^k/(5*2^m)), [k, 0, m]), m -> 3*n + 1), n)");
    holds(&mut e, "every(range(0, 6), j => rewrite(dr, n -> j) == total(map(k => binomial(6*j + 3, 2*k)*3^k, range(0, 3*j + 1)))/(5*2^(3*j + 1)))");
    ev(&mut e, "dl = difference_root_reduce([lucas_l(n), n], n)");
    holds(&mut e, "every(range(0, 8), j => rewrite(dl[2], n -> j) == j and rewrite(dl[1], n -> j) == lucas_l(j))");
}

#[test]
fn generating_functions_found_for_euler_products_and_rational_terms() {
    let mut e = engine();
    ev(&mut e, "g = find_generating_function([1, 1, 2, 2, 3, 4, 5, 6, 7, 8, 11, 12], x)");
    holds(&mut e, "coefficient_list(normal(series(g, x, 0, 11)), x) == [1, 1, 2, 2, 3, 4, 5, 6, 7, 8, 11, 12]");
    ev(&mut e, "g7 = find_generating_function([1, 3/2, 9/5, 2, 15/7, 9/4, 7/3, 12/5], x)");
    holds(&mut e, "every(range(0, 9), j => simplify(series_coefficient(g7, [x, 0, j]) - 3*(j + 1)/(j + 3)) === 0)");
}

#[test]
fn harmonic_sums_that_shift_end() {
    let mut e = engine();
    // sums the nested-sum method cannot read (an inner sum of k - 2 after the shift) stay sums and end: the sum from 1
    // and the one from 0 that other methods shift it to no longer call each other without end
    for src in [
        "sum(harmonic_number(j, 2)/(j + 2)^3, [j, 1, k])",
        "sum(harmonic_number(j, 2)/(j + 2)^3, [j, 0, k])",
        "sum(harmonic_number(j + 1, 2)/(j + 3)^3, [j, 1, k])",
        "sum(harmonic_number(j)/(j + 2)^3, [j, 1, k])",
    ] {
        ends(&mut e, src, 5);
        assert!(ev(&mut e, src).starts_with("sum("), "{src}");
    }
    // from 0: the term at 0 and the nested sums from 1, checked against direct sums
    ev(&mut e, "h0 = sum(harmonic_number(j + 1, 2)^2/2^(j + 1), [j, 0, k])");
    holds(&mut e, "every(range(0, 6), m => simplify(rewrite(h0, k -> m) - total(map(j => harmonic_number(j + 1, 2)^2/2^(j + 1), range(0, m)))) === 0)");
}
