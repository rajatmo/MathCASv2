//! The precision contract (roadmap 16B) for the special functions at complex and negative arguments: `n(e, 30)`
//! agrees with `n(e, 60)` to 30 digits, and the 30-digit ball contains the 60-digit value (both parts).
mod common;
use common::Sys;

/// The checks of one expression, as a list of booleans in the language.
fn contract_src(e: &str) -> String {
    format!(
        "{{ let a = n({e}, 30); let b = n({e}, 60); \
         [precision(a) >= 29, abs(re(a) - re(b)) + abs(im_part(a) - im_part(b)) <= 10^-29*abs(b), \
          abs(set_precision(re(a), inf) - re(b)) <= 10^-min(accuracy(re(a)), 10000), \
          abs(set_precision(im_part(a), inf) - im_part(b)) <= 10^-min(accuracy(im_part(a)), 10000)] }}"
    )
}

/// Every expression meets the contract.
pub fn contract(exprs: &[&str]) {
    let mut s = Sys::new();
    let mut bad = vec![];
    for e in exprs {
        let got = s.run(&contract_src(e));
        if got != "[true, true, true, true]" {
            bad.push(format!("{e}: {got}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn bessel_at_negative_and_complex_arguments() {
    contract(&[
        "bessel_j(1/3, -2)",
        "bessel_j(1/3, 2 + im)",
        "bessel_j(2, 3 + 4*im)",
        "bessel_j(-3, 3 + 4*im)",
        "bessel_j(1/2 + im, -1 + im/2)",
        "bessel_i(1/3, -2)",
        "bessel_i(5/2, -3 + im)",
        "bessel_y(1/3, 2 + im)",
        "bessel_k(1/3, 2 + im)",
    ]);
}

#[test]
fn airy_at_complex_arguments() {
    contract(&["airy_ai(1 + im)", "airy_bi(1 + im)", "airy_ai_prime(1 + im)", "airy_bi_prime(-3 + 2*im)", "airy_ai(-5 - 7*im)"]);
}

#[test]
fn digamma_and_polygamma_at_complex_arguments() {
    contract(&[
        "digamma(1 + im)",
        "digamma(-5/2 + im/3)",
        "digamma(1/2 + 20*im)",
        "polygamma(2, 1 + im)",
        "polygamma(1, -3/2 + 2*im)",
        "sum(1/(k^3 + 1), [k, 1, inf])",
    ]);
}

#[test]
fn values_against_independent_digits() {
    // digits from an independent evaluation at 40 digits: J_(1/3)(-2), Ai(1 + i), psi(1 + i)
    let mut s = Sys::new();
    assert_eq!(
        s.run("abs(n(bessel_j(1/3, -2), 30) - (0.221469909074288106125211208868 + 0.383597134864326539636624364375*im)) < 10^-29"),
        "true"
    );
    assert_eq!(
        s.run("abs(n(airy_ai(1 + im), 30) - (0.0604583083718381491965329781166 - 0.151889565877181402354947912592*im)) < 10^-29"),
        "true"
    );
    assert_eq!(
        s.run("abs(n(digamma(1 + im), 30) - (0.0946503206224769772718784827219 + 1.07667404746858117413405079475*im)) < 10^-29"),
        "true"
    );
}
