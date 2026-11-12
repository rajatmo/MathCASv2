//! Round 21: engine bugs found in this round, one test each (ids B21-k); a test that still fails is kept ignored until
//! its bug is fixed, and removing the `ignore` is the proof of a fix.
//! Expected values come from identities, 30 against 45 digits, numeric integrals or independent evaluations;
//! tolerances are stated in each assertion.
#![allow(dead_code)]
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn session() -> CoreEngine {
    CoreEngine::with_config(EngineConfig {
        threads: 1,
        ..EngineConfig::default()
    })
}

/// Evaluate with a 30 s watchdog (the stop flag is raised when the evaluation has not returned by then).
fn ev(e: &mut CoreEngine, src: &str) -> String {
    let stop = StopFlag::new();
    let flag = stop.clone();
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let dog = std::thread::spawn(move || {
        if rx.recv_timeout(std::time::Duration::from_secs(30)).is_err() {
            flag.raise();
        }
    });
    let out = match e.eval(src, &stop) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    };
    let _ = tx.send(());
    let _ = dog.join();
    out
}

/// A printed real or complex number (`a`, `a + b*im`, `a - bim`, `bim`) as (re, im).
fn cnum(s: &str) -> Option<(f64, f64)> {
    let t: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    // the engine prints a machine complex number as `complex(re, im)`
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

/// `src` prints a number within `tol` (relative to max(1, |want|)) of `re + im*i`.
fn assert_c(src: &str, re: f64, im: f64, tol: f64) {
    let mut e = session();
    let got = ev(&mut e, src);
    let ok = match cnum(&got) {
        Some((r, i)) => {
            (r - re).abs() <= tol * re.abs().max(1.0) && (i - im).abs() <= tol * im.abs().max(1.0)
        }
        None => false,
    };
    assert!(
        ok,
        "{src}\n   got  {got}\n   want {re} + {im}im (tol {tol})"
    );
}

fn assert_real(src: &str, want: f64, tol: f64) {
    assert_c(src, want, 0.0, tol);
}

/// `src` prints exactly `want`.
fn assert_eq_printed(src: &str, want: &str) {
    let mut e = session();
    let got = ev(&mut e, src);
    assert_eq!(got, want, "{src}");
}

/// `src` evaluates to `true`.
fn assert_true(src: &str) {
    assert_eq_printed(src, "true");
}

/// The printed value of `src` is an answer: no failure, and the unevaluated head `head` does not remain.
fn assert_closed(src: &str, head: &str) {
    let mut e = session();
    let got = ev(&mut e, src);
    assert!(
        !got.starts_with("failure") && !got.starts_with("error") && !got.contains(head),
        "{src}\n   got  {got}"
    );
}

#[test]
fn b21_1_polygamma_complex_machine() {
    assert_c(
        "polygamma(1, 0.5 + 1.8*im)",
        0.00024186009793900222,
        -0.5720193411726974,
        1e-12,
    );
    assert_c(
        "polygamma(1, 0.3 + 0.2*im)",
        4.046478446199779,
        -7.33135315536603,
        1e-12,
    );
    assert_c(
        "polygamma(2, -1.5 + 0.5*im)",
        -0.20018927983590307,
        8.932770107933065,
        1e-12,
    );
    assert_c(
        "polygamma(3, 0.505 - 1.8*im)",
        -0.01360878191517665,
        -0.41978061758854474,
        1e-12,
    );
    assert_c(
        "polygamma(1, 1.08*im)",
        -0.4510184350864318,
        -0.7639902204218788,
        1e-12,
    );
}

#[test]
fn b21_2_hermite_h_noninteger_order() {
    assert_real("hermite_h(1/2, 3.)", 2.465719198833116, 1e-13);
    assert_real("hermite_h(1/2, 5.)", 3.1700422855740022, 1e-13);
    assert_real("hermite_h(1/2, 8.)", 4.0038782069784435, 1e-13);
    assert_real("hermite_h(1/2, 10.)", 4.47491810624921, 1e-13);
    assert_c(
        "hermite_h(1/2, 24. + 2.*im)",
        6.934945771942091,
        0.2883324533564176,
        1e-12,
    );
    // 30 digits requested: the value must agree with the 45-digit one to 28 digits (today 8 digits are right)
    assert_true("abs(n(hermite_h(1/2, 8), 30) - n(hermite_h(1/2, 8), 45)) < 10^-28");
}

#[test]
fn b21_3_n_exact_arguments_give_nan() {
    // each prints machine_real("nan") today
    assert_c(
        "n(cot(1 + im))",
        0.21762156185440273,
        -0.8680141428959249,
        1e-13,
    );
    assert_c(
        "n(sech(5 + im))",
        0.007281662290728377,
        -0.011339487429296047,
        1e-13,
    );
    assert_c(
        "n(bessel_y(1, -1))",
        0.7812128213002887,
        -0.880101171489867,
        1e-13,
    );
    assert_c(
        "n(cos_integral(-1))",
        0.33740392290096816,
        std::f64::consts::PI,
        1e-13,
    );
    assert_c(
        "n(exp_integral_e(2, -1))",
        0.8231640121031086,
        -std::f64::consts::PI,
        1e-13,
    );
    assert_c(
        "n(cosh_integral(-4))",
        9.813547558823187,
        std::f64::consts::PI,
        1e-13,
    );
    assert_c(
        "n(elliptic_k(12))",
        0.4633669148376202,
        -0.7690859426000861,
        1e-13,
    );
    assert_c(
        "n(product_log(-4 + 3/2*im))",
        0.828323956622252,
        1.671994973096705,
        1e-13,
    );
    assert_c(
        "n(jacobi_dn(4 + 3*im, 1/3))",
        -1.058579749625772,
        -0.30178241122203864,
        1e-12,
    );
    assert_c(
        "n(hypergeometric_2f1(1, 1, 2, 2/3 - 2.64*im))",
        0.4266045998465813,
        -0.4784432718206772,
        1e-12,
    );
}

#[test]
fn b21_4_beta_negative_integer() {
    assert_real("beta(-36., 2.)", 1.0 / 1260.0, 1e-13);
    assert_real("n(beta(-36, 2))", 1.0 / 1260.0, 1e-13);
    assert_true("abs(n(beta(-36, 2), 30) - 1/1260) < 10^-28");
}

#[test]
fn b21_5_kelvin_complex_negative_real_part() {
    assert_c(
        "kelvin_ker(0, -3. + im)",
        4.51349921605174,
        -1.6262439196085854,
        1e-9,
    );
    assert_c(
        "kelvin_kei(0, -3. + im)",
        -3.4372125332103547,
        -6.822836072157492,
        1e-9,
    );
    assert_c(
        "kelvin_ker(1, -2.679 + 2.38*im)",
        -10.66062024963891,
        -2.0638276316217317,
        1e-9,
    );
    assert_c(
        "kelvin_ker(1, -1.378 + 2*im)",
        -3.344777891788126,
        1.1573115124054132,
        1e-9,
    );
    assert_c(
        "kelvin_kei(0, 1.*im)",
        -1.0512686495746146,
        0.3920174189823332,
        1e-9,
    );
}

#[test]
fn b21_6_elementary_complex_30_digits() {
    for e in [
        "asinh(3 + 2*im)",
        "cot(3 + 2*im)",
        "sech(5 + im)",
        "acosh(3 + 2*im)",
        "atanh(3 + 2*im)",
        "csch(3 + 2*im)",
        "sec(3 + 2*im)",
        "acot(3 + 2*im)",
    ] {
        assert_true(&format!("abs(re(n({e}, 30) - n({e}, 45))) + abs(im_part(n({e}, 30) - n({e}, 45))) < 10^-28"));
    }
    assert_c(
        "n(asinh(3 + 2*im), 30)",
        1.9833870299165354,
        0.5706527843210994,
        1e-14,
    );
    assert_c(
        "n(cot(3 + 2*im), 30)",
        -0.010604783470337102,
        -1.0357466377649954,
        1e-14,
    );
}

#[test]
fn b21_7_special_complex_30_digits() {
    assert_c(
        "n(cos_integral(3 + 2*im), 30)",
        -0.16836286832772047,
        -1.0543195536564284,
        1e-14,
    );
    assert_c(
        "n(exp_integral_ei(3 + 2*im), 30)",
        2.807489082166904,
        9.101927958386792,
        1e-14,
    );
    assert_c(
        "n(elliptic_k(3 + 2*im), 30)",
        1.0170435642865263,
        0.8062531787336744,
        1e-14,
    );
    assert_c(
        "n(bessel_y(1, 3 + 2*im), 30)",
        1.2858493414635994,
        0.7250812532419391,
        1e-14,
    );
    assert_c(
        "n(polylog(2, 3 + 2*im), 30)",
        0.5557336284055069,
        3.449106803945985,
        1e-14,
    );
    assert_c(
        "n(hypergeometric_2f1(1, 1, 2, 3 + 2*im), 30)",
        0.122555897528072,
        0.703694231712067,
        1e-14,
    );
    assert_c(
        "n(jacobi_cn(3 + 2*im, 1/3), 30)",
        -0.22470154890182456,
        -3.7374620015784292,
        1e-14,
    );
    assert_c(
        "n(product_log(3 + 2*im), 30)",
        1.1260540225846734,
        0.3151299376586585,
        1e-14,
    );
    assert_c(
        "n(beta(3 + 2*im, 2), 30)",
        0.03076923076923077,
        -0.05384615384615385,
        1e-14,
    );
}

#[test]
fn b21_8_machine_unevaluated() {
    assert_c(
        "elliptic_k(12.)",
        0.4633669148376202,
        -0.7690859426000861,
        1e-13,
    );
    assert_c(
        "elliptic_e(4.429)",
        0.3847426110406327,
        1.4696061244066343,
        1e-13,
    );
    assert_c(
        "bessel_k(2, -27.)",
        4.853285968502157e-13,
        -1.1956492644360313e11,
        1e-12,
    );
    assert_c(
        "elliptic_theta(3, 3. + 2.*im, 1/3.)",
        55.083522413794405,
        31.227678945809192,
        1e-12,
    );
    assert_c(
        "parabolic_cylinder_d(0.5, -4/9. + im)",
        0.3897560443028764,
        0.9515698808366814,
        1e-12,
    );
}

#[test]
fn b21_9_fresnel_hp_argument_slow() {
    use std::time::Instant;
    let t = Instant::now();
    let mut e = session();
    let got = ev(&mut e, "fresnel_f(n(pi, 50))");
    assert!(!got.contains("fresnel_f"), "unevaluated: {got}");
    assert!(t.elapsed().as_secs_f64() < 2.0, "took {:?}", t.elapsed());
}

#[test]
#[ignore = "B21-8 (rest): 3F2 beyond the unit disc needs analytic continuation of a pFq with p = q + 1; deferred"]
fn b21_8_pfq_beyond_the_unit_disc() {
    assert_c(
        "hypergeometric_pfq([1, 1, 1], [2, 2], 1.5 + 1.14*im)",
        0.9879739051652374,
        0.5720606436928775,
        1e-12,
    );
}

/// `n(e, 30)` and `n(e, 45)` agree to 28 digits (real and imaginary parts).
fn same_digits(e: &str) {
    assert_true(&format!("abs(re(n({e}, 30) - n({e}, 45))) + abs(im_part(n({e}, 30) - n({e}, 45))) < 10^-28"));
}

#[test]
fn r22_polylog_and_incomplete_gamma_complex_30_digits() {
    assert_c("n(polylog(2, 3 + 2*im), 30)", 0.5557336284055069, 3.449106803945985, 1e-14);
    same_digits("gamma(1/3, 2 + im)");
    same_digits("hypergeometric_2f1(1/3, 1/2, 5/7, 3/2 - 2*im)");
    same_digits("bessel_k(2, 3 + 2*im)");
    same_digits("elliptic_e(2 + im)");
    same_digits("jacobi_sn(3 + 2*im, 1/3)");
    same_digits("product_log(3 + 2*im)");
}

#[test]
fn r22_kelvin_and_beta_values() {
    assert_c("kelvin_ker(0, -3. + im)", 4.51349921605174, -1.6262439196085854, 1e-9);
    assert_eq_printed("beta(-4, 2)", "1/12");
    assert_eq_printed("beta(2, -4)", "1/12");
}
