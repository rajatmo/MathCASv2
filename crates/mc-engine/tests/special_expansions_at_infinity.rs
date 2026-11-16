//! Expansions of special functions at infinity and near 0 by `series`: Struve H and L, the Kelvin functions, the
//! Mittag-Leffler function, the Lommel functions s, S, t, T, Bessel functions of an order that is not an integer near 0,
//! and the basic hypergeometric series in its base q; and enclosures of the Coulomb functions on boxes.
//! Every expansion is checked against the function's numerical value at 40 digits: at a large x the error of the
//! truncated expansion is at most a small multiple of the first omitted terms (the difference between two truncations)
//! and falls when more terms are kept; near 0 likewise for small x. The boxes are sampled at 40-digit points.
//! Each test has a time bound.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

fn timed(what: &str, limit: u64, f: impl FnOnce()) {
    let t0 = Instant::now();
    f();
    let took = t0.elapsed();
    assert!(took < Duration::from_secs(limit), "{what} took {took:?}");
}

/// The truncations of order k and k + 2 of f at x0 (inf or 0), at the point p: the error of the first is at most
/// three times the difference of the two, and the second is closer.
fn remainder_check(e: &mut CoreEngine, f: &str, x0: &str, k: u32, p: &str) {
    let src = format!(
        "{{ let s1 = normal(series({f}, x, {x0}, {k})); let s2 = normal(series({f}, x, {x0}, {k2})); \
         let v = n(rewrite({f}, x -> {p}), 80); let e1 = abs(n(rewrite(s1, x -> {p}), 80) - v); \
         let e2 = abs(n(rewrite(s2, x -> {p}), 80) - v); let d = abs(n(rewrite(s2 - s1, x -> {p}), 80)); \
         e1 <= 3*d and e2 < e1 }}",
        k2 = k + 2
    );
    holds(e, &src);
}

#[test]
fn struve_expansions_at_infinity() {
    timed("Struve at infinity", 180, || {
        let mut e = CoreEngine::new();
        // H_0: the oscillating part of Y_0 and (2/pi)(1/x - 1/x^3 + ...)
        // (the same function of x: the difference vanishes at points, at 40 digits)
        holds(&mut e, "{ let s = normal(series(struve_h(0, x), x, inf, 3)); let w = -((-9 + 16*x*(1 + 8*x))*cos(x) + (9 + 16*x*(1 - 8*x))*sin(x))/(128*pi^(1/2)*x^(5/2)) + 2*(-1 + x^2)/(pi*x^3); abs(n(rewrite(s - w, x -> 3), 40)) < 10^-35 and abs(n(rewrite(s - w, x -> 11), 40)) < 10^-35 }");
        for f in ["struve_h(0, x)", "struve_h(1/3, x)", "struve_h(7/3, x)", "struve_l(0, x)", "struve_l(2/3, x)"] {
            remainder_check(&mut e, f, "inf", 4, "40");
        }
    });
}

#[test]
fn kelvin_expansions_at_infinity() {
    timed("Kelvin at infinity", 240, || {
        let mut e = CoreEngine::new();
        for f in ["kelvin_ber(x)", "kelvin_bei(x)", "kelvin_ker(x)", "kelvin_kei(x)", "kelvin_ber(2/5, x)", "kelvin_kei(1/3, x)"] {
            remainder_check(&mut e, f, "inf", 4, "40");
        }
        // the recessive part of ber and bei (DLMF 10.67.1-2): at x = 20 with 30 terms the dominant series is exact to
        // ~1e-11 while the recessive part is ~6e-8, so the sum agrees with the function only with it
        for f in ["kelvin_ber(x)", "kelvin_bei(x)"] {
            holds(&mut e, &format!("abs(n(rewrite(normal(series({f}, x, inf, 30)), x -> 20), 80) - n(rewrite({f}, x -> 20), 80)) < 10^-9"));
        }
        holds(&mut e, "abs(n(kelvin_kei(20), 80)/pi) > 10^-8");
    });
}

#[test]
fn mittag_leffler_expansion_at_infinity() {
    timed("Mittag-Leffler at infinity", 180, || {
        let mut e = CoreEngine::new();
        holds(&mut e, "{ let s = normal(series(mittag_leffler_e(1, 1/2, x), x, inf, 6)); full_simplify(s - (exp(x)*x^(1/2) + (-10395 + 2*x*(945 + 2*x*(-105 + 2*x*(15 - 6*x + 4*x^2))))/(64*pi^(1/2)*x^6))) == 0 }");
        for (f, p) in [("mittag_leffler_e(3/2, x)", "40"), ("mittag_leffler_e(1/2, 1/3, x)", "8"), ("mittag_leffler_e(4/3, 5/2, x)", "40")] {
            remainder_check(&mut e, f, "inf", 4, p);
        }
        // values beyond the range of machine numbers: the series of E sums to the end (2 z^(4/3) e^(z^2) dominates)
        holds(&mut e, "abs(n(mittag_leffler_e(1/2, 1/3, 30), 30)/n(2*30^(4/3)*exp(900), 30) - 1) < 10^-25");
    });
}

#[test]
fn lommel_expansions() {
    timed("Lommel expansions", 240, || {
        let mut e = CoreEngine::new();
        holds(&mut e, "normal(series(lommel_s1(1/2, 3, x), x, 0, 6)) === (-4/27)*x^(3/2) + (16/351)*x^(7/2) - (64/29835)*x^(11/2)");
        holds(&mut e, "normal(series(lommel_t1(1/2, 3, x), x, 0, 6)) === (-4/27)*x^(3/2) - (16/351)*x^(7/2) - (64/29835)*x^(11/2)");
        for f in ["lommel_s1(1/2, 1/3, x)", "lommel_s2(1/2, 1/3, x)", "lommel_t1(1/2, 1/3, x)", "lommel_t2(1/2, 1/3, x)", "lommel_s2(7/2, 3/4, x)"] {
            remainder_check(&mut e, f, "inf", 4, "40");
        }
        for f in ["lommel_s2(7/2, 3/4, x)", "lommel_t2(1/2, 1/3, x)", "lommel_s2(-1/2, 6/5, x)"] {
            remainder_check(&mut e, f, "0", 3, "1/10");
        }
        // the expansion of S at infinity terminates when mu - nu is an odd positive integer (an exact form)
        holds(&mut e, "{ let s = normal(series(lommel_s2(7/2, 1/2, x), x, inf, 4)); abs(n(rewrite(s, x -> 3), 80) - n(lommel_s2(7/2, 1/2, 3), 80)) < 10^-30 }");
        // symbolic orders: the power stays outside the series
        holds(&mut e, "{ let s = normal(series(lommel_t2(m, n_u, x), x, inf, 3)); full_simplify(s - x^(m - 1)*(1 + (1 - 2*m - n_u^2 + m^2)/x^2)) == 0 }");
    });
}

#[test]
fn bessel_noninteger_order_near_zero() {
    timed("Bessel near 0", 120, || {
        let mut e = CoreEngine::new();
        for f in ["bessel_y(3/4, x)", "bessel_k(1/3, x)", "bessel_j(-2/5, x)", "bessel_i(7/3, x)"] {
            remainder_check(&mut e, f, "0", 3, "1/10");
        }
        // no Taylor table of the infinite values Y(nu, 0)
        holds(&mut e, "free_of(series(bessel_y(3/4, x), x, 0, 3), bessel_y)");
    });
}

#[test]
fn basic_hypergeometric_series_in_the_base() {
    timed("q-hypergeometric in q", 120, || {
        let mut e = CoreEngine::new();
        holds(&mut e, "{ let s = normal(series(q_hypergeometric_pfq([a], [b], q, z), q, 0, 2)); full_simplify(s - ((-1 + b + z - a*z)/(-1 + b) + q*z*(-1 + a)*(-1 + z)/(-1 + b) - z*(-1 + a)*(1 + z*(-1 + a - b))*q^2/(-1 + b))) == 0 }");
        // against the sum at q = 1/50: the error of the order-4 truncation is O(q^5)
        for (a, b, z) in [("[1/3]", "[2/5]", "3/7"), ("[1/3]", "[2/5, -3]", "5"), ("[]", "[1/2]", "-2")] {
            holds(&mut e, &format!(
                "{{ let s = normal(series(q_hypergeometric_pfq({a}, {b}, q, {z}), q, 0, 4)); \
                 abs(n(rewrite(s, q -> 1/50), 80) - n(q_hypergeometric_pfq({a}, {b}, 1/50, {z}), 80)) < 10^4*(1/50)^5 }}"
            ));
        }
    });
}

#[test]
fn coulomb_functions_on_boxes() {
    timed("Coulomb boxes", 240, || {
        let mut e = CoreEngine::new();
        for f in ["coulomb_f", "coulomb_g", "coulomb_h1", "coulomb_h2"] {
            for (l, eta, rho) in [("3/4", "5/6", "7/8"), ("1/3", "-1/2", "2 + im/2"), ("5/4", "2", "3/2 - im/3")] {
                let r = "10^(-6)";
                ev(&mut e, &format!("bx = {f}(centered_interval({l}, {r}), centered_interval({eta}, {r}), centered_interval({rho}, {r}))"));
                holds(&mut e, "head(bx) === centered_interval");
                // the box contains the value at its centre and at corners of the parameter box (40 digits)
                for (dl, de, dr) in [("0", "0", "0"), ("1", "1", "1"), ("-1", "1", "-1"), ("1", "-1", "1"), ("-1", "-1", "-1")] {
                    holds(&mut e, &format!(
                        "{{ let v = n({f}({l} + {dl}*{r}*99/100, {eta} + {de}*{r}*99/100, {rho} + {dr}*{r}*99/100), 80); \
                         abs(re(v) - re(bx[1])) <= re(bx[2]) and abs(im_part(v) - im_part(bx[1])) <= im_part(bx[2]) }}"
                    ));
                }
            }
        }
    });
}
