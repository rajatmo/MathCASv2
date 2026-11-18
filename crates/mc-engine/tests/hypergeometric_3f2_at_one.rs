//! 3F2 at argument 1: the shifted series of a pFq with a numerator 1 and a denominator 2 (exact and machine, also
//! at machine arguments near 1), and the sums of Dixon, Watson and Whipple for exact parameters. Each value is
//! checked against the partial sums of the series, extrapolated in N^-s (s the parameter excess), and divergent or
//! symbolic cases stay unevaluated. The Hankel transform whose check needs these values evaluates. Time-bounded.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn timed(what: &str, limit: u64, f: impl FnOnce()) {
    let t0 = Instant::now();
    f();
    let took = t0.elapsed();
    assert!(took < Duration::from_secs(limit), "{what} took {took:?}");
}

const PSUM: &str = "fn psum(as, bs, nn) = { var t = 1.0; var s = 1.0; var out = []; for k in range(0, nn - 1) { t = t*fold((p, a) => p*(a + k), 1.0, as)/(fold((p, b) => p*(b + k), 1.0, bs)*(k + 1)); s = s + t; if k + 1 == nn/2 then out = append(out, s) else null }; append(out, s) }
fn ext_ok(as, bs) = { let sx = n(total(bs) - total(as)); let r = psum(as, bs, 100000); let x = (r[2]*2^sx - r[1])/(2^sx - 1); let v = n(hypergeometric_pfq(as, bs, 1)); is_number(v) and abs(v - x) < 10^-6 }";

#[test]
fn closed_forms_at_one_agree_with_the_series() {
    let mut e = CoreEngine::new();
    ev(&mut e, PSUM);
    timed("3F2 at 1", 300, || {
        assert_eq!(ev(&mut e, "hypergeometric_pfq([3/2, 1, 1/2], [2, 2], 1)"), "-4*(-1 + 2/pi)");
        assert_eq!(ev(&mut e, "hypergeometric_pfq([1/2, 1/2, 1], [2, 2], 1)"), "4*(-1 + 4/pi)");
        for (a, b) in [
            ("[3/2, 1, 1/2]", "[2, 2]"),
            ("[1/3, 1, 1/5]", "[2, 7/3]"),
            ("[1/2, 1/3, 1/4]", "[7/6, 5/4]"),
            ("[1/2, 1/3, 1/4]", "[11/12, 1/2]"),
            ("[1/3, 2/3, 1/4]", "[1, 1/2]"),
        ] {
            let v = ev(&mut e, &format!("hypergeometric_pfq({a}, {b}, 1)"));
            assert!(!v.contains("hypergeometric"), "{a} {b}: {v}");
            assert_eq!(ev(&mut e, &format!("ext_ok({a}, {b})")), "true", "{a} {b}: {v}");
        }
    });
}

#[test]
fn machine_arguments_near_one() {
    let mut e = CoreEngine::new();
    timed("machine", 120, || {
        for z in ["1.0", "0.9999999999999991", "0.95", "0.9", "-0.95", "-1.0", "1.5"] {
            let v = ev(&mut e, &format!("n(hypergeometric_pfq([1, 3/2, 1/2], [2, 2], {z}))"));
            assert!(!v.contains("hypergeometric"), "{z}: {v}");
        }
        // the shifted form agrees with the series where both are summed
        assert_eq!(ev(&mut e, "abs(hypergeometric_pfq([1, 3/2, 1/2], [2, 2], 0.95) - 1.3679219234847642) < 10^-12"), "true");
        assert_eq!(ev(&mut e, "abs(hypergeometric_pfq([1, 3/2, 1/2], [2, 2], 1.0) - (4 - 8/pi)) < 10^-12"), "true");
    });
}

#[test]
fn other_parameters_stay_unevaluated() {
    let mut e = CoreEngine::new();
    timed("declined", 120, || {
        for (a, b) in [
            ("[1, 3, 1/2]", "[2, 2]"),
            ("[1, a, b]", "[2, c]"),
            ("[1/3, 1/4, 1/5]", "[3/2, 2]"),
            ("[1, 1, 1/2]", "[2, 1]"),
            ("[1/2, 1/3, 1/4]", "[7/6, 1/2]"),
        ] {
            let v = ev(&mut e, &format!("hypergeometric_pfq({a}, {b}, 1)"));
            assert!(v.starts_with("hypergeometric_pfq("), "{a} {b}: {v}");
        }
    });
}

#[test]
fn hankel_transform_of_one_minus_bessel_order_one() {
    let mut e = CoreEngine::new();
    timed("hankel", 120, || {
        let v = ev(&mut e, "vv = hankel_transform((1 - bessel_j(0, a*r))/r^2, r, s, 1); vv");
        assert!(!v.contains("hankel_transform") && !v.contains("failure"), "{v}");
        // the defining integral at a = 1, on each side of the jump at s = a
        for s in ["1/2", "2"] {
            let src = format!(
                "{{ let w = n(rewrite(vv, [a -> 1, s -> {s}]), 30); let i = n_integrate((1 - bessel_j(0, r))/r*bessel_j(1, {s}*r), [r, 0, inf], working_precision: 20); abs(w - i) < 10^-8 }}"
            );
            assert_eq!(ev(&mut e, &src), "true", "s = {s}: {v}");
        }
        let w = ev(&mut e, "inverse_hankel_transform((1 - bessel_j(0, a*s))/s^2, s, r, 1)");
        assert!(!w.contains("hankel_transform") && !w.contains("failure"), "{w}");
    });
}
