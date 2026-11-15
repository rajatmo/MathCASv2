//! Round 42: prime counting by the combinatorial method; (p+1)Fp beyond the unit circle and the bilateral series;
//! Lommel functions at exceptional orders.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// |a - b| < tol for two numeric expressions.
fn close(e: &mut CoreEngine, a: &str, b: &str, tol: &str) {
    let r = ev(e, &format!("abs(n({a}) - n({b})) < {tol}"));
    assert_eq!(r, "true", "{a} vs {b}: {}", ev(e, &format!("n({a})")));
}

#[test]
fn prime_counts_beyond_two_to_the_47() {
    let mut e = CoreEngine::new();
    // pi(10^15) from the tables of Deleglise and Rivat; pi(2^47 + 1) agrees with Meissel's pi(2^47) (2^47 + 1 is
    // composite: 3 divides it)
    assert_eq!(ev(&mut e, "prime_pi(10^15)"), "29844570422669");
    assert_eq!(ev(&mut e, "prime_pi(2^47 + 1) - prime_pi(2^47)"), "0");
    // the count just below and just above a prime: their difference is 1
    assert_eq!(ev(&mut e, "p = next_prime(10^14); prime_pi(p) - prime_pi(p - 1)"), "1");
    // prime(k) beyond 2^47 and back
    assert_eq!(ev(&mut e, "prime(10^13)"), "323780508946331");
    assert_eq!(ev(&mut e, "prime_pi(prime(5 * 10^12))"), "5000000000000");
}

#[test]
fn generalized_hypergeometric_beyond_the_circle() {
    let mut e = CoreEngine::new();
    // 3F2(a1, a2, a3; b1, b2; z) = Gamma(b2)/(Gamma(a3) Gamma(b2 - a3)) int_0^1 t^(a3-1) (1-t)^(b2-a3-1) 2F1(a1, a2; b1; z t) dt
    let euler = |z: &str| {
        format!(
            "gamma(3/2)/(gamma(1/2)*gamma(1))*n_integrate(t^(-1/2)*hypergeometric_2f1(1/3, 1/4, 2/5, ({z})*t), [t, 0, 1], working_precision: 30)"
        )
    };
    close(&mut e, "hypergeometric_pfq([1/3, 1/4, 1/2], [2/5, 3/2], -4.0)", &euler("-4"), "10^-12");
    close(&mut e, "hypergeometric_pfq([1/3, 1/4, 1/2], [2/5, 3/2], -7/2 + 2*im)", &euler("-7/2 + 2*im"), "10^-12");
    // a real argument above 1: the limit from below, as for 2F1
    close(&mut e, "hypergeometric_pfq([1/3, 1/4, 1/2], [2/5, 3/2], 3.0)", "n(hypergeometric_pfq([1/3, 1/4, 1/2], [2/5, 3/2], 3 - 10^-40*im), 30)", "10^-12");
}

#[test]
fn bilateral_series_at_one_and_beyond_the_circle() {
    let mut e = CoreEngine::new();
    // Dougall's sum, against direct summation of 2 * 10^5 terms on each side by the term ratios (the terms decay like
    // |n|^-(c + d - a - b)): -14.49118026213 and 24.39355978018668
    assert_eq!(ev(&mut e, "bilateral_hypergeometric_pfq([1 / 2, 3 / 2], [9 / 4, 13 / 4], 1.)"), "-14.491180262123457");
    close(&mut e, "bilateral_hypergeometric_pfq([1/3, 1/5], [7/2, 11/3], 1)", "24.39355978018668", "10^-11");
    close(&mut e, "bilateral_hypergeometric_pfq([1/2, 3/2], [9/4, 13/4], 1)", "-14.49118026213", "10^-9");
    // a or b a positive integer: the negative terms are infinite, no value
    assert_eq!(ev(&mut e, "bilateral_hypergeometric_pfq([2, 1/2], [5, 7/2], 1)"), "bilateral_hypergeometric_pfq([2, 1/2], [5, 7/2], 1)");
    // beyond the circle: the two halves continued
    close(&mut e, "bilateral_hypergeometric_pfq([im/2, 3*im/4], [im/4, im/3], 5.~20*im)", "0.25250384481228860 + 0.85544188185805774*im", "10^-12");
}

#[test]
fn lommel_functions_at_exceptional_orders() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "lommel_s1(-1, 1, z)"), ev(&mut e, "(-1/2)*pi*(-struve_h(1, z) + 2/pi)"));
    // checked by the series at a number, and at the order -1 (s depends on nu^2)
    close(&mut e, "lommel_s1(-1, 1, 23/10)", "lommel_s1(-1, 1, 2.3)", "10^-12");
    close(&mut e, "rewrite(lommel_s1(-1, -1, z), z -> 7/5)", "lommel_s1(-1, 1, 1.4)", "10^-12");
    // mu - nu or mu + nu a negative odd integer: a denominator of the series vanishes
    assert_eq!(ev(&mut e, "lommel_s1(2, 5, z)"), "complex_infinity");
    assert_eq!(ev(&mut e, "lommel_t1(-4, 1, z)"), "complex_infinity");
    assert_eq!(ev(&mut e, "lommel_s1(1/2, 3/2, z)"), "complex_infinity");
    // E_0 and E_1 by Struve functions, checked numerically
    close(&mut e, "weber_e(1, 0.7)", "2/pi - struve_h(1, 0.7)", "10^-14");
    close(&mut e, "rewrite(weber_e(0, z), z -> 13/10)", "weber_e(0, 1.3)", "10^-14");
}
