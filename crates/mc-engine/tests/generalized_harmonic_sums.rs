//! Sums of non-integer powers as generalized harmonic numbers, and the asymptotic expansions of
//! H(n, r) and of the Hurwitz zeta function at infinity (Euler-Maclaurin, DLMF 25.11.43). Sums are checked
//! against the terms added up at a number; expansions against the function at a large argument to 30 digits.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn sums_of_powers() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "sum(1/sqrt(k), [k, 1, n])"), "harmonic_number(n, 1/2)");
    // variations: another exponent and start, a shifted base with a factor; each closed form at n = 12 equals the
    // terms added up
    for (f, lo) in [("k^(3/2)", "2"), ("3*(k + 2)^(-1/3)", "1"), ("(k - 1)^(5/7)", "2")] {
        let s = ev(&mut e, &format!("s = sum({f}, [k, {lo}, n]); s"));
        assert!(s.contains("harmonic_number"), "{f}: {s}");
        let d = ev(&mut e, &format!("abs(n(rewrite(s, n -> 12) - sum({f}, [k, {lo}, 12]), 30)) < 10^-25"));
        assert_eq!(d, "true", "{f}: {s}");
    }
}

#[test]
fn expansions_at_infinity() {
    let mut e = CoreEngine::new();
    assert_eq!(
        ev(&mut e, "asymptotic_sum(1/sqrt(k), [k, 1, n_u], [n_u, inf, 3])"),
        "(-1/24)*n_u^(-3/2) + (1/2)*n_u^(-1/2) + 2*n_u^(1/2) + zeta(1/2)"
    );
    // variations: H(n, 3/2), H(n, -1/3) and the Hurwitz zeta(5/2, n); the error of each truncated expansion at
    // n = 10^4 is far below its last term
    for (f, tol) in [("harmonic_number(n, 3/2)", "10^-17"), ("harmonic_number(n, -1/3)", "10^-15"), ("zeta(5/2, n)", "10^-20")] {
        let s = ev(&mut e, &format!("s = normal(series({f}, n, inf, 4)); s"));
        assert!(!s.contains("series") && !s.contains(f), "{f}: {s}");
        let d = ev(&mut e, &format!("abs(n(rewrite(s - {f}, n -> 10000), 40)) < {tol}"));
        assert_eq!(d, "true", "{f}: {s}");
    }
    // the limits through the same expansion
    assert_eq!(ev(&mut e, "limit(harmonic_number(n, 1/2)/sqrt(n), n -> inf)"), "2");
    assert_eq!(ev(&mut e, "limit(harmonic_number(n, 3/2), n -> inf)"), "zeta(3/2)");
}
