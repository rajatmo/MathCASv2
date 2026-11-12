//! Round 23: special-function bugs found in this round, one test each (ids B23-k); a test that still fails is kept
//! ignored until its bug is fixed, and removing the `ignore` is the proof of a fix.
//! Expected values come from identities, a defining series or integral evaluated in the engine itself,
//! higher precision, or an independent evaluation; each assertion states its tolerance (relative to the modulus
//! of the expected value unless said otherwise).
#![allow(dead_code, clippy::excessive_precision)]
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

/// Evaluate on a detached thread and give up after `secs` seconds (a hang that ignores the stop flag cannot
/// block the test run). Returns `None` when the evaluation did not finish in time.
fn ev_within(src: &str, secs: u64) -> Option<String> {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::spawn(move || {
        let stop = StopFlag::new();
        let mut e = session();
        let out = match e.eval(&s, &stop) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        let _ = tx.send(out);
    });
    rx.recv_timeout(std::time::Duration::from_secs(secs)).ok()
}

/// A printed real or complex number (`a`, `a + b*im`, `a - bim`, `bim`) as (re, im).
fn cnum(s: &str) -> Option<(f64, f64)> {
    let t: String = s.chars().filter(|c| !c.is_whitespace()).collect();
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
    let mut e = session();
    let got = ev(&mut e, src);
    let want = (re * re + im * im).sqrt();
    let ok = match cnum(&got) {
        Some((r, i)) => ((r - re).powi(2) + (i - im).powi(2)).sqrt() <= rel * want,
        None => false,
    };
    assert!(
        ok,
        "{src}\n   got  {got}\n   want {re} + {im}im (rel {rel})"
    );
}

fn assert_rr(src: &str, want: f64, rel: f64) {
    assert_cr(src, want, 0.0, rel);
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

/// `src` stays unevaluated (starts with `head`) or fails with a message; it must not return a number.
fn assert_declines(src: &str, head: &str) {
    let mut e = session();
    let got = ev(&mut e, src);
    assert!(
        got.starts_with(head) || got.starts_with("failure"),
        "{src}\n   got  {got}"
    );
}

#[test]
fn b23_1_meijer_g_exact_parameters_outside_unit_disc() {
    // the same function with float parameters, the inversion G(1/z) and an independent evaluation agree
    assert_rr(
        "n(meijer_g([[1/3], [1/2]], [[], [3/4, 0]], 5))",
        -0.03238139377978986,
        1e-12,
    );
    assert_rr(
        "n(meijer_g([[1/3], [1/2]], [[], [3/4, 0]], 5), 30)",
        -0.03238139377978986,
        1e-12,
    );
    assert_rr(
        "n(meijer_g([[3/2], [3/2]], [[], [2, -1/2]], 100))",
        -2.863262136504863,
        1e-12,
    );
    assert_rr(
        "n(meijer_g([[-1/2, 1/3], [2]], [[], [-1/2, -3/2, -2/3]], 100))",
        0.032796078696730326,
        1e-12,
    );
    assert_rr(
        "n(meijer_g([[7/3, 2], [1/2]], [[], [-1/2, 1/3, -1]], 10))",
        2.937678650298886,
        1e-12,
    );
}

#[test]
fn b23_2_hypergeometric_u_positive_integer_parameters() {
    // U(a, b, z) = 1/Gamma(a) integral_0^inf exp(-z t) t^(a-1) (1+t)^(b-a-1) dt
    assert_rr("hypergeometric_u(1, 3, 3.)", 4.0 / 9.0, 1e-12);
    assert_rr("hypergeometric_u(1, 4, 3.)", 17.0 / 27.0, 1e-12);
    assert_rr("hypergeometric_u(2, 5, 3.)", 1.0 / 3.0, 1e-12);
    assert_rr("hypergeometric_u(1, 4, 10.)", 0.122, 1e-12);
    assert_rr("n(hypergeometric_u(1, 4, 3), 30)", 17.0 / 27.0, 1e-12);
    // W(k, m, z) = exp(-z/2) z^(m + 1/2) U(m - k + 1/2, 1 + 2 m, z) = exp(-3/2) 3^(5/2) U(2, 5, 3)
    assert_rr("whittaker_w(1/2, 2, 3.)", 1.159418322234182, 1e-12);
    // meijer_g reduces to 49 exp(-7) U(3, 5, 7) here and inherits the error; the Mellin-Barnes integral along
    // re(s) = 3 gives 1.8609836031724237e-4 (and the float-parameter evaluation 1.8609836031724812e-4)
    assert_rr(
        "n(meijer_g([[-1, 3], []], [[], [0]], 1/7), 30)",
        1.8609836031724812e-4,
        1e-9,
    );
}

#[test]
fn b23_3_kelvin_ber_bei_noninteger_order() {
    // expected: (z/2)^nu sum_k cos or sin((3 nu/4 + k/2) pi) (z^2/4)^k / (k! gamma(nu + k + 1)), summed in the engine
    assert_cr("kelvin_ber(1/2, 2.*im)", 0.7469318662118124, 0.7469318662118124, 1e-10);
    assert_cr("kelvin_bei(1/2, 2.*im)", 0.43969353680445183, 0.43969353680445183, 1e-10);
    assert_cr("kelvin_ber(1/2, -2.)", 0.0, -0.3072383294073605, 1e-10);
    assert_cr("kelvin_bei(1/2, -2.)", 0.0, 1.1866254030162642, 1e-10);
    assert_cr("kelvin_ber(-1/2, 0.084 + 1.09*im)", -0.05533489860243301, 0.145353943832095, 1e-10);
    assert_cr("kelvin_bei(-1/2, 0.084 + 1.09*im)", -0.6047427270529315, 0.5786626271794204, 1e-10);
    assert_cr("kelvin_ber(3/2, -1. + 2.*im)", 0.9485130873045652, -0.02076489417810362, 1e-10);
    assert_cr("n(kelvin_ber(1/2, 2*im), 30)", 0.7469318662118124, 0.7469318662118124, 1e-10);
}

#[test]
fn b23_4_bessel_k_machine_complex_large_real_part() {
    // the 30-digit values of the same arguments are right
    assert_cr(
        "bessel_k(3/2, 20. + 2.5*im)",
        -5.071300507834461e-10,
        -3.276002574663802e-10,
        1e-9,
    );
    assert_cr(
        "bessel_k(1/2, 21.9 + 1.4*im)",
        1.1417660564235702e-11,
        -8.168538488838727e-11,
        1e-9,
    );
    assert_cr(
        "bessel_k(0, 15.6 + 0.8*im)",
        3.583675353653003e-8,
        -3.8811917666212994e-8,
        1e-9,
    );
    // hankel_h2 at large negative imaginary part: the machine value is rounding noise (1e-8 instead of 6e-10)
    assert_cr(
        "hankel_h2(0, 4.5 - 19.4*im)",
        -6.641621943547719e-10,
        -6.649001106371843e-11,
        1e-9,
    );
    assert_cr(
        "hankel_h2(3/2, 4.5 - 19.4*im)",
        5.397345747878617e-10,
        -4.5257668851208167e-10,
        1e-9,
    );
    assert_cr(
        "bessel_k(5, 15. - 1.*im)",
        1.0198296695488606e-7,
        1.9249837899332810e-7,
        1e-9,
    );
}

#[test]
fn b23_5_kelvin_ker_kei_machine_large_argument() {
    // expected values: the 30-digit inner combination evaluated by hand (n of the exact argument)
    assert_rr("kelvin_ker(2, 19.)", -1.0818475837070729e-7, 1e-9);
    assert_rr("kelvin_kei(1, 14.8)", 1.1376693128668809e-6, 1e-9);
    assert_rr("kelvin_ker(2, 15.)", -7.691529016674997e-7, 1e-9);
    assert_rr("kelvin_kei(0, 10.5)", -0.0002285179865986643, 1e-9);
}

#[test]
fn b23_6_meijer_g_large_argument_infinite() {
    // G^{1,1}_{1,2}(z | 1; 1/2, 3/10): the Mellin-Barnes integral along re(s) = 0.2 gives 1.3656736612969564 at z = 1000
    assert_rr("meijer_g([[1.], []], [[0.5], [0.3]], 1000.)", 1.3656736612969564, 1e-8);
    assert_rr("meijer_g([[1], []], [[1/2], [3/10]], 1000.)", 1.3656736612969564, 1e-8);
    assert_rr("n(meijer_g([[1], []], [[1/2], [3/10]], 1000), 30)", 1.365673661296940857, 1e-12);
    // from 10^15 on the evaluation does not return and does not poll the stop flag
    let got = ev_within("meijer_g([[1.], []], [[0.5], [0.3]], 10.^15)", 10);
    assert!(got.is_some(), "meijer_g(..., 10.^15) did not return within 10 s");
}

#[test]
fn b23_10_polylog_noninteger_order_outside_unit_disc() {
    assert_cr("n(polylog(1/2, 5/2))", -1.660333320541378, -1.851647399057939, 1e-12);
    assert_cr("n(polylog(1/2, 5/2), 30)", -1.660333320541377514, -1.851647399057938961, 1e-15);
    assert_cr("n(polylog(3/2, 5/2))", 1.184000189908536, -3.393294700911349, 1e-12);
    assert_cr("n(polylog(5/2, 451/100))", 3.496507582153444, -4.368975970198797, 1e-9);
}

#[test]
fn b23_11_hypergeometric_huge_lower_parameter_hang() {
    let got = ev_within("hypergeometric_0f1(1e308, 0.5)", 10);
    assert!(got.is_some(), "hypergeometric_0f1(1e308, 0.5) did not return within 10 s");
    let got = ev_within("hypergeometric_pfq([1], [1.7976931348623157e308], 0.5)", 10);
    assert!(got.is_some(), "pfq with lower parameter 1.8e308 did not return within 10 s");
    let got = ev_within("meijer_g([[1 + im], []], [[1/10^50], [-2, 0.5]], 10^30)", 10);
    assert!(got.is_some(), "meijer_g with a float parameter at z = 10^30 did not return within 10 s");
}

#[test]
fn b23_12_hypergeometric_2f1_equal_b_c_slow() {
    let got = ev_within("n(hypergeometric_2f1(3/2, 1/3, 1/3, -3), 30)", 10);
    let got = got.expect("n(hypergeometric_2f1(3/2, 1/3, 1/3, -3), 30) did not return within 10 s");
    let (r, i) = cnum(&got).unwrap_or((f64::NAN, f64::NAN));
    assert!((r - 0.125).abs() < 1e-12 && i.abs() < 1e-12, "got {got}, want 0.125");
}

#[test]
fn b23_14_n_of_product_log_branch() {
    assert_rr("n(product_log(-1, -1/10))", -3.577152063957297, 1e-14);
    assert_rr("n(product_log(-1, -1/10), 30)", -3.5771520639572972, 1e-14);
    assert_rr("product_log(-1., -0.2)", -2.5426413577735265, 1e-14);
    assert_closed("n(solve(x*exp(x) == -1/10, x))", "product_log");
}

#[test]
fn b23_18_hypergeometric_u_complex_and_negative() {
    // on the negative real axis only the modulus is asserted (the side of the cut is a convention)
    assert_closed("hypergeometric_u(2, 1/2, -6.)", "hypergeometric_u");
    assert_cr("hypergeometric_u(3/2, 1, 5. + 7.*im)", 0.010365399571456185, -0.03227245599251449, 1e-10);
    assert_cr("hypergeometric_u(-1/2, 2, 1.418 - 6.*im)", 1.7126963255637107, -1.7336484131642178, 1e-10);
    assert_closed("n(hypergeometric_u(-1/2, 2, 1418/1000 - 6*im), 30)", "hypergeometric_u");
    assert_closed("whittaker_w(3, 3, 1.142 + 0.25*im)", "hypergeometric_u");
}

#[test]
fn b23_19_meijer_g_spurious_pole_failure() {
    assert_rr("n(meijer_g([[], [1/3, 4, -1/2]], [[2, 3/2, 2/3], []], 1/2))", 0.32763384708414434, 1e-9);
    assert_rr("n(meijer_g([[1/3, 1/2], [3]], [[], [7/3]], 2))", 0.18459551207164804, 1e-9);
    assert_rr("n(meijer_g([[], [3]], [[-1/2, 4, 2/3], []], 1/100))", 31.70396761778061, 1e-9);
}

#[test]
fn b23_20_polylog_negative_integer_order() {
    assert_closed("polylog(-2, 4)", "polylog");
    assert_rr("n(polylog(-2, 4), 30)", -20.0 / 27.0, 1e-14);
    assert_rr("n(polylog(-3, 3))", 3.0 * (1.0 + 4.0 * 3.0 + 9.0) / 16.0, 1e-14);
}

#[test]
fn b23_22_polygamma_machine_high_order_left_half_plane() {
    assert_cr("polygamma(10, -17.6 + 5.1*im)", 4.350736917981017e-6, 5.880669143049824e-6, 1e-13);
    assert_cr("polygamma(10, -1.5 - 6.1*im)", -0.0033278362956426079, -0.00033084617722857574, 1e-13);
}

#[test]
fn b23_23_thirty_digits_not_reached_without_failure() {
    // the 60-digit value agrees with the true value; the 30-digit result must agree to 28 digits or fail
    for (e30, e60) in [
        (
            "n(hankel_h2(1, 655/7 - 188/100*im), 30)",
            "n(hankel_h2(1, 655/7 - 188/100*im), 60)",
        ),
        (
            "n(hankel_h2(3, 456/10 - 123/100*im), 30)",
            "n(hankel_h2(3, 456/10 - 123/100*im), 60)",
        ),
        (
            "n(jacobi_cd(5 - 3*im, 1/3), 30)",
            "n(jacobi_cd(5 - 3*im, 1/3), 60)",
        ),
        (
            "n(whittaker_w(3, 1/2, 2), 30)",
            "n(whittaker_w(3, 1/2, 2), 60)",
        ),
        (
            "n(hypergeometric_u(1/2, 2, 1/1000), 30)",
            "n(hypergeometric_u(1/2, 2, 1/1000), 60)",
        ),
        (
            "n(hypergeometric_u(1/2, 2, 1/9284455), 30)",
            "n(hypergeometric_u(1/2, 2, 1/9284455), 60)",
        ),
    ] {
        // the parts apart: abs of a complex ball that straddles 0 stays a square root of a ball (mc-numtheory)
        let src = format!("abs(re(({e30}) - ({e60}))) < 10^-27*abs({e60}) and abs(im_part(({e30}) - ({e60}))) < 10^-27*abs({e60})");
        let mut e = session();
        let got = ev(&mut e, &src);
        assert!(
            got == "true" || got.starts_with("failure"),
            "{e30}\n   comparison gave {got}"
        );
        // and a failure is acceptable, a ball with no digits is not
        let v = ev(&mut e, e30);
        assert!(!v.contains("ball(") || v.starts_with("failure"), "{e30}\n   got {v}");
    }
}

#[test]
fn b23_25_jacobi_complex_thirty_digits_slow() {
    let t = std::time::Instant::now();
    assert_cr(
        "n(jacobi_sn(297/10 + 6/25*im, 7/10), 30)",
        -0.598996131743964690,
        -0.171858226629614301,
        1e-15,
    );
    assert!(
        t.elapsed().as_secs_f64() < 2.0,
        "took {:.1} s (the machine value takes 20 ms)",
        t.elapsed().as_secs_f64()
    );
    let t = std::time::Instant::now();
    assert_cr(
        "n(jacobi_sn(10 + im, 7/10), 30)",
        1.122890771187541426,
        0.0722756390403994652,
        1e-15,
    );
    assert!(t.elapsed().as_secs_f64() < 2.0, "took {:.1} s", t.elapsed().as_secs_f64());
}

#[test]
fn b23_28_hypergeometric_1f1_zero_zero_consistent() {
    assert_rr("hypergeometric_1f1(0., 0., 1.247)", 1.0, 1e-14);
    assert_rr("n(hypergeometric_1f1(0, 0, 1247/1000), 30)", 1.0, 1e-14);
}

#[test]
fn b23_29_n_of_exact_closed_form_with_cancellation() {
    assert_rr("n(polygamma(5, 981))", 2.6483335784351044e-14, 1e-12);
    assert_rr("n(struve_h(3/2, 1/1000))", 3.1539154773076685e-9, 1e-12);
    assert_rr("n(struve_h(3/2, 1/3298819))", 5.046076802531402e-18, 1e-12);
    assert_rr("n(polygamma(3, 5000))", 1.6004800639999987e-11, 1e-12);
}
