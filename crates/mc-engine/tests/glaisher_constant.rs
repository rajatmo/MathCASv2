//! Glaisher's constant `glaisher`: its digits at any precision, its facts and its identities with `zeta'(-1)`,
//! `barnes_g` and `log_barnes_g`. Each value is checked by a second route: a published value, the Barnes
//! G-function computed numerically by its own algorithm, or the hyperfactorial asymptotics.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// `ev` within `limit` of wall time.
fn ev_within(src: &str, limit: Duration) -> String {
    let t = Instant::now();
    let r = ev(src);
    assert!(t.elapsed() < limit, "{src} took {:?}", t.elapsed());
    r
}

#[test]
fn digits_agree_with_the_published_value() {
    // 1.28242712910062263687534256886979172776768892732500119206374002174040630885882646112973649196 (OEIS A074962)
    let published = "128242712910062263687534256886979172776768892732500119206374002174040630885882646112973649196";
    let got = ev_within("n(glaisher, 100)", Duration::from_secs(10));
    let digits: String = got.chars().filter(char::is_ascii_digit).collect();
    assert_eq!(&digits[..90], &published[..90], "{got}");
    assert_eq!(ev("n(glaisher)"), "1.2824271291006226");
}

#[test]
fn many_digits_are_fast_and_consistent() {
    // 2000 digits within the bound, and their first 1000 equal to the 1000-digit value
    let long = ev_within("n(glaisher, 2000)", Duration::from_secs(20));
    let short = ev_within("n(glaisher, 1000)", Duration::from_secs(10));
    let d = |s: &str| s.chars().filter(char::is_ascii_digit).take(990).collect::<String>();
    assert_eq!(d(&long), d(&short));
    assert!(long.chars().filter(char::is_ascii_digit).count() >= 2000, "{long}");
}

#[test]
fn value_inside_expressions() {
    // n of the expression against the expression of n's value
    assert_eq!(ev("abs(n(glaisher^2 + log(glaisher), 30) - (n(glaisher, 40)^2 + log(n(glaisher, 40)))) < 10^-28"), "true");
    assert_eq!(ev("glaisher + 1.0"), "2.282427129100623");
    assert_eq!(ev("[is_real(glaisher), is_positive(glaisher), is_numeric(glaisher)]"), "[true, true, true]");
    assert_eq!(ev("[1 < glaisher, glaisher < 2, glaisher < catalan, floor(glaisher)]"), "[true, true, false, 1]");
}

#[test]
fn zeta_derivative_at_minus_one() {
    assert_eq!(ev("zeta'(-1)"), "1/12 - log(glaisher)");
    assert_eq!(ev("derivative(1)(zeta)(-1)"), "1/12 - log(glaisher)");
    // the numerical derivative of zeta at an inexact -1 (finite differences of zeta itself)
    assert_eq!(ev("abs(n(zeta'(-1), 25) - zeta'(n(-1, 40))) < 10^-22"), "true");
}

#[test]
fn barnes_g_at_half_integers() {
    assert_eq!(ev("barnes_g(1/2)"), "2^(1/24)*exp(1/8)*glaisher^(-3/2)*pi^(-1/4)");
    assert_eq!(ev("log_barnes_g(1/2)"), "1/8 + (-3/2)*log(glaisher) + (-1/4)*log(pi) + (1/24)*log(2)");
    // the closed forms against the Barnes G-function at inexact arguments (its own algorithm: the asymptotic
    // expansion with zeta'(-1) from log G(1) = 0)
    for z in ["1/2", "5/2", "-3/2", "11/2"] {
        let src = format!("abs(n(barnes_g({z}), 30) - barnes_g(n({z}, 40))) < 10^-25");
        assert_eq!(ev_within(&src, Duration::from_secs(20)), "true", "{z}");
    }
    for z in ["1/2", "3/2", "7/2"] {
        let src = format!("abs(n(log_barnes_g({z}), 30) - log_barnes_g(n({z}, 40))) < 10^-25");
        assert_eq!(ev_within(&src, Duration::from_secs(20)), "true", "{z}");
    }
    // G(z + 1) = gamma(z) G(z)
    assert_eq!(ev("simplify(barnes_g(3/2) - gamma(1/2)*barnes_g(1/2))"), "0");
}

#[test]
fn hyperfactorial_asymptotics() {
    // 1^1 2^2 ... n^n = A n^(n^2/2 + n/2 + 1/12) e^(-n^2/4) (1 + 1/(720 n^2) + O(n^-4)), the next term from B_4
    let src = "abs(n(product(k^k, [k, 1, 300])/(300^(300^2/2 + 300/2 + 1/12)*exp(-300^2/4)), 30)/n(glaisher, 30) - 1 - 1/(720*300^2)) < 10^-12";
    assert_eq!(ev_within(src, Duration::from_secs(20)), "true");
}
