//! Round 26: special-function bugs found in this round, one test each (ids B26-k); a test that still fails is kept
//! ignored until its bug is fixed, and removing the `ignore` is the proof of a fix.
//! Expected values come from identities (the Gegenbauer polynomial with parameter 1/2 is the Legendre
//! polynomial), the same function at higher precision, or an independent evaluation; each assertion states
//! its tolerance (relative to the modulus of the expected value unless said otherwise).
#![allow(dead_code, clippy::excessive_precision)]
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate in a fresh session on a detached thread; the stop flag is raised after `secs` seconds and the call is
/// abandoned (`None`) after `secs + 5`, so a hang that ignores the flag cannot block the test run.
fn ev_within(src: &str, secs: u64) -> Option<String> {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::spawn(move || {
        let stop = StopFlag::new();
        let flag = stop.clone();
        let (dtx, drx) = std::sync::mpsc::channel::<()>();
        let dog = std::thread::spawn(move || {
            if drx.recv_timeout(std::time::Duration::from_secs(secs)).is_err() {
                flag.raise();
            }
        });
        let mut e = CoreEngine::with_config(EngineConfig {
            threads: 1,
            ..EngineConfig::default()
        });
        let out = match e.eval(&s, &stop) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        let _ = dtx.send(());
        let _ = dog.join();
        let _ = tx.send(out);
    });
    rx.recv_timeout(std::time::Duration::from_secs(secs + 5)).ok()
}

fn ev(src: &str) -> String {
    ev_within(src, 30).unwrap_or_else(|| "hang".to_string())
}

/// A printed real or complex number (`a`, `a + b*im`, `a - bim`, `bim`) as (re, im).
fn cnum(s: &str) -> Option<(f64, f64)> {
    let t: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    // the forms without the display option: `complex(re, im)` and `machine_real("inf")`
    if let Some(inner) = t.strip_prefix("machine_real(\"").and_then(|r| r.strip_suffix("\")")) {
        return inner.parse().ok().map(|r| (r, 0.0));
    }
    if let Some(inner) = t.strip_prefix("complex(").and_then(|r| r.strip_suffix(')')) {
        let (re, im) = inner.split_once(',')?;
        return Some((re.parse().ok()?, im.parse().ok()?));
    }
    let Some(body) = t.strip_suffix("im") else {
        return t.parse::<f64>().ok().map(|r| (r, 0.0));
    };
    let body = body.strip_suffix('*').unwrap_or(body);
    let b = body.as_bytes();
    let mut pos = None;
    for i in (1..b.len()).rev() {
        if (b[i] == b'+' || b[i] == b'-') && b[i - 1] != b'e' {
            pos = Some(i);
            break;
        }
    }
    let coef = |s: &str| -> Option<f64> {
        match s {
            "" | "+" => Some(1.0),
            "-" => Some(-1.0),
            _ => s.parse().ok(),
        }
    };
    match pos {
        Some(i) => Some((body[..i].parse().ok()?, coef(&body[i..])?)),
        None => Some((0.0, coef(body)?)),
    }
}

/// `src` prints a number within `rel` (relative to |want|) of `re + im*i`.
fn assert_cr(src: &str, re: f64, im: f64, rel: f64) {
    let out = ev(src);
    let (gr, gi) = cnum(&out).unwrap_or((f64::NAN, f64::NAN));
    let err = ((gr - re).powi(2) + (gi - im).powi(2)).sqrt();
    let scale = (re * re + im * im).sqrt();
    assert!(err <= rel * scale, "{src}: got {out}, want {re} + {im}im");
}

#[test]
fn b26_14_laguerre_exact_machine() {
    assert_cr("n(laguerre_l(229, -1/2, -1))", 1.6094496519153265e11, 0.0, 1e-9);
    assert_cr("n(laguerre_l(220, 1, 3))", 1.7644127457502534, 0.0, 1e-9);
    assert_cr("n(laguerre_l(500, 0, 1))", 0.19658935236339936, 0.0, 1e-9);
    assert_cr("n(laguerre_l(287, -1/2, 7))", -0.062076356164187509, 0.0, 1e-9);
    assert_cr("n(laguerre_l(1064, 2, -3/5))", 6.51709827738703e23, 0.0, 1e-9);
}

#[test]
fn b26_15_gegenbauer_jacobi_exact_machine() {
    // C_n^(1/2) is the Legendre polynomial
    assert_cr("n(gegenbauer_c(300, 1/2, 1/7))", 0.028530408101215776, 0.0, 1e-9);
    assert_cr("n(gegenbauer_c(250, 1/2, 1/7))", 0.011042345343195944, 0.0, 1e-9);
    assert_cr("n(jacobi_p(200, 1/2, 1/3, 1/7))", -0.06074502250322046, 0.0, 1e-9);
    assert_cr("n(jacobi_p(511, 0, 5, 1/3))", -0.08954489944310923, 0.0, 1e-9);
    assert_cr("n(jacobi_p(173, 1, 2, -1/2))", 0.2868375886117163, 0.0, 1e-9);
    assert_cr("n(jacobi_p(518, -1/2, 3/2, 2))", 5.722287450742689e294, 0.0, 1e-9);
}

#[test]
fn b26_16_gegenbauer_30_digits() {
    let out = ev("n(gegenbauer_c(400, 1/2, 1/7), 30)");
    let (re, im) = cnum(&out).unwrap_or((f64::NAN, f64::NAN));
    assert!(!out.contains("ball"), "{out}");
    assert!((re - 0.02608347466435019754).abs() < 1e-18 && im.abs() < 1e-18, "{out}");
}

#[test]
fn b26_17_kelvin_kei_exact_order() {
    for (src, want) in [
        ("n(kelvin_kei(1, 1/3), 30)", -1.829475561630761),
        ("n(kelvin_kei(1, 1/3), 17)", -1.829475561630761),
        ("n(kelvin_kei(1/3, 2), 30)", -0.15526472432085353),
    ] {
        let out = ev(src);
        let (re, im) = cnum(&out).unwrap_or((f64::NAN, f64::NAN));
        assert!((re - want).abs() < 1e-12 * want.abs() && im == 0.0, "{src}: {out}");
    }
}

#[test]
fn b26_18_hypergeometric_2f1_beyond_the_unit_disc() {
    let out = ev_within("n(hypergeometric_2f1(-1/3, -1/3, 5, 3), 30)", 10).unwrap_or_default();
    let (re, im) = cnum(&out).unwrap_or((f64::NAN, f64::NAN));
    assert!((re - 1.0787346272817901).abs() < 1e-12 && (im + 0.004986407804591272).abs() < 1e-12, "{out}");
}

#[test]
#[ignore = "B26-19 (meijer_g part, deferred in round 27): G^{1,1}_{1,2}(z | 3/2; -1/2, -1/2) has a_1 - b_1 = 2, a pinched contour; the perturbed function diverges like 1/h (+-2.5e8 i at h = 1e-8 on either side), so neither the principal value nor either one-sided limit gives the value"]
fn b26_19_meijer_pinched_contour() {
    assert_cr("n(meijer_g([[3/2], []], [[-1/2], [-1/2]], -8))", 7.775045141777141, -35.95684287296805, 1e-9);
}

#[test]
fn b26_19_pole_failures_with_a_value() {
    assert_cr("gegenbauer_c(2.5, -1/2, 52.8)", -9723.72279538671, 0.0, 1e-9);
    assert_cr("gegenbauer_c(3, -1/2, 2.5)", -6.5625, 0.0, 1e-9);
}

#[test]
fn b26_20_incomplete_gamma_large_imaginary_argument() {
    assert_cr("gamma(0.7, 30000.*im)", -0.007579805341609528, 0.044742404025292615, 1e-9);
    assert_cr("gamma(0.7, 50000.*im)", 0.017051675034423158, 0.0349993849176309, 1e-9);
    // an elementary case: Gamma(2, z) = (1 + z) exp(-z)
    let out = ev("gamma(2, 30000.*im)");
    let (re, im) = cnum(&out).unwrap_or((f64::NAN, f64::NAN));
    let (c, s) = (30000f64.cos(), 30000f64.sin());
    // (1 + 30000 i)(cos - i sin) = (c + 30000 s) + i (30000 c - s)
    assert!((re - (c + 30000.0 * s)).abs() < 1e-6 && (im - (30000.0 * c - s)).abs() < 1e-6, "{out}");
}

#[test]
fn b26_21_zeta_large_imaginary_part() {
    for src in ["zeta(-8735.256*im)", "zeta(2 + 100000.*im)", "n(zeta(1/2 + 100000*im), 30)"] {
        let out = ev_within(src, 10);
        assert!(out.is_some_and(|o| cnum(&o).is_some()), "{src}");
    }
}

#[test]
fn b26_22_large_argument_machine_values() {
    for src in [
        "parabolic_cylinder_d(0.35, 278.9)",
        "bessel_k(1, 9664.923*im)",
        "struve_h(1/3, 309994.931)",
        "hypergeometric_u(2, 2, -8085.608)",
        "bessel_y(3/2, -30510.351)",
        "kelvin_ber(2, 75069.946)",
        "meijer_g([[0.3], [1/2]], [[2], [-1/2]], -0.83 + 0.85*im)",
    ] {
        let out = ev_within(src, 5);
        assert!(out.is_some_and(|o| cnum(&o).is_some()), "{src}");
    }
}

#[test]
fn b26_26_real_chebyshev_at_30_digits_is_real() {
    for src in ["n(chebyshev_t(3/2, 10), 30)", "n(chebyshev_u(3/2, 3), 30)", "n(chebyshev_t(1/2, 12345/1000), 30)"] {
        let out = ev(src);
        assert!(!out.contains("im") && !out.contains("ball"), "{src}: {out}");
    }
}
