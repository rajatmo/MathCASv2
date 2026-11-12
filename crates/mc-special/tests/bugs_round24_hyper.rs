//! Round 24: tests of the hypergeometric and Meijer G fixes that have no earlier ignored test
//! (D23-03, D23-10, R22-01, the logarithmic Tricomi case at 30 digits, huge parameters and arguments).
#![allow(dead_code, clippy::excessive_precision)]
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn session() -> CoreEngine {
    CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() })
}

fn ev(src: &str) -> String {
    let stop = StopFlag::new();
    let flag = stop.clone();
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let dog = std::thread::spawn(move || {
        if rx.recv_timeout(std::time::Duration::from_secs(40)).is_err() {
            flag.raise();
        }
    });
    let mut e = session();
    let out = match e.eval(src, &stop) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    };
    let _ = tx.send(());
    let _ = dog.join();
    out
}

fn real(s: &str) -> f64 {
    s.trim().parse::<f64>().unwrap_or(f64::NAN)
}

fn close(src: &str, want: f64, rel: f64) {
    let got = real(&ev(src));
    assert!((got - want).abs() <= rel * want.abs(), "{src}: got {got}, want {want}");
}

#[test]
fn d23_03_symbolic_reductions() {
    assert_eq!(ev("hypergeometric_2f1(a, b, b, x)"), "(1 - x)^-a");
    assert_eq!(ev("hypergeometric_2f1(b, a, b, x)"), "(1 - x)^-a");
    assert_eq!(ev("hypergeometric_2f1(1, 1, 2, -x)"), "log(1 + x)/x");
    assert_eq!(ev("hypergeometric_0f1(1/2, -x^2/4)"), "cos(x)");
    assert!(ev("hypergeometric_0f1(3/2, -x^2/4)").replace(" ", "") == "sin(x)/x" || ev("hypergeometric_0f1(3/2, -x^2/4)").replace(" ", "") == "x^-1*sin(x)");
    assert_eq!(ev("hypergeometric_0f1(1/2, x^2/4)"), "cosh(x)");
    let g = ev("meijer_g([[], []], [[0], [1/2]], x^2/4)").replace(" ", "");
    assert!(g == "cos(x)/pi^(1/2)" || g == "cos(x)*pi^(-1/2)", "{g}");
}

#[test]
fn d23_10_meijer_g_machine_large_argument() {
    // G^{1,0}_{0,1}(z | ; 0) = exp(-z)
    close("meijer_g([[], []], [[0], []], 700.0)", 9.85967654375977e-305, 1e-11);
    // G^{2,0}_{0,2}(z | ; 0, 0) = 2 K_0(2 sqrt z)
    close("meijer_g([[], []], [[0, 0], []], 1.0e5)", real(&ev("n(2*bessel_k(0, 2*sqrt(10^5)), 30)")), 1e-10);
    // underflow is 0, not inf or garbage
    assert_eq!(ev("meijer_g([[], []], [[0], []], 1.0e5)"), "0.0");
    // G^{1,1}_{1,2}(z | 1/2; 0, 0): a power-law value of the order 1/sqrt(z) needing 14000 bits of cancellation
    close("meijer_g([[1/2], []], [[0], [0]], 700.)", real(&ev("n(meijer_g([[1/2], []], [[0], [0]], 700), 30)")), 1e-11);
    close("meijer_g([[1/2], []], [[0], [0]], 1.0e4)", 0.0100002500281308611702, 1e-11);
    // beyond the cancellation cap: unevaluated, never inf, indeterminate or a wrong number
    let v = ev("meijer_g([[1/2], []], [[0], [0]], 1.0e8)");
    assert!(v.starts_with("meijer_g"), "{v}");
}

#[test]
fn huge_arguments_answer_quickly() {
    let t = std::time::Instant::now();
    for src in [
        "n(hypergeometric_0f1(10^400 + im, 1/3), 30)",
        "n(hypergeometric_1f1(1 + im, 2, 10^400), 30)",
        "hypergeometric_1f1(1, 1.7976931348623157e308, 0.5)",
        "hypergeometric_2f1(1, 1, 1.7976931348623157e308, 0.5)",
        "hypergeometric_0f1(1e308, 0.5)",
        "meijer_g([[], []], [[0, 0], []], 1.0e9)",
    ] {
        let _ = ev(src);
    }
    assert!(t.elapsed().as_secs_f64() < 20.0, "took {:?}", t.elapsed());
    close("hypergeometric_0f1(1e308, 0.5)", 1.0, 1e-14);
    close("hypergeometric_pfq([1], [1e308], 0.5)", 1.0, 1e-14);
}

#[test]
fn r22_01_complex_series_at_high_precision() {
    // the complex series stops at any precision (it used to run to the term limit above about 1000 bits)
    let t = std::time::Instant::now();
    let v = ev("n(hypergeometric_1f1(1 + im, 2, 1/3), 3000)");
    assert!(v.starts_with("complex(1.17587848187480121384"), "{}", &v[..40.min(v.len())]);
    assert!(t.elapsed().as_secs_f64() < 20.0, "took {:?}", t.elapsed());
}

#[test]
fn tricomi_logarithmic_case_keeps_its_digits() {
    // integer b >= 3 at several precisions (the mean of the two sides of b = n carries about 1/eps^2 of cancellation
    // that the arguments' own radii must not multiply)
    let v = ev("n(hypergeometric_u(5, 3, 1/100), 40)");
    assert!(v.starts_with("405.04705887683537437228413606647601193849"), "{v}");
    let v = ev("n(hypergeometric_u(1/2, 2, 1/1000), 30)");
    assert!(v.starts_with("566.6491366422755949605489949"), "{v}");
    let v = ev("n(whittaker_w(3, 1/2, 2), 60)");
    assert!(v.starts_with("-1.47151776468576928638209508064584346978"), "{v}");
    close("hypergeometric_u(2, 2, 3.)", real(&ev("n(hypergeometric_u(2, 2, 3), 30)")), 1e-13);
}

#[test]
fn meijer_g_two_sided_for_p_equal_q() {
    // G^{1,1}_{2,2} with m + n < p vanishing or not on the two sides of the unit circle
    assert_eq!(ev("meijer_g([[], [2/3, -3/2]], [[1], [2/3]], 5)"), "0");
    let v = real(&ev("n(meijer_g([[1/3], [1/2]], [[], [3/4, 0]], 5))"));
    assert!((v + 0.03238139377978986).abs() < 1e-12, "{v}");
    // inside the unit disc the sum with no terms is 0
    assert_eq!(ev("meijer_g([[1/3], [1/2]], [[], [3/4, 0]], 1/5)"), "0");
}
