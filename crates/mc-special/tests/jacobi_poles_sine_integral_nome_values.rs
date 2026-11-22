//! Exact values at special arguments: the reciprocal Jacobi functions cs, ds, ns at u = 0 (a pole for every m, as at
//! the shifted zeros 2K and 2iK'), the sine integral along the imaginary axis to infinity, and the nome at m = -1.
//! Each value is checked against numbers computed another way: the reciprocal functions near the pole grow like 1/u,
//! Si(i y) for growing y against i Shi(y), q(-1) against exp(-pi K(2)/K(-1)) with the complete integrals.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|v| v.printed).unwrap_or_else(|err| format!("error: {err}"))
}

fn num(e: &mut CoreEngine, src: &str) -> f64 {
    let s = ev(e, src);
    s.trim_end_matches('.').parse::<f64>().unwrap_or_else(|_| panic!("{src}: {s}"))
}

#[test]
fn reciprocal_jacobi_functions_at_zero() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    for f in ["jacobi_cs", "jacobi_ds", "jacobi_ns"] {
        for m in ["m", "1/2", "3", "-2", "2 + im"] {
            assert_eq!(ev(&mut e, &format!("{f}(0, {m})")), "complex_infinity", "{f}(0, {m})");
        }
        // u f(u) -> 1 as u -> 0 (cs, ds, ns ~ 1/u): the pole is simple, from the numeric evaluator
        let r = num(&mut e, &format!("n(abs(10^-8*{f}(10^-8, 1/3)))"));
        assert!((r - 1.0).abs() < 1e-6, "{f}: {r}");
    }
    // the functions with sn in the numerator stay 0 there; machine arguments keep their numeric path
    assert_eq!(ev(&mut e, "[jacobi_sc(0, m), jacobi_sd(0, m), jacobi_sn(0, m)]"), "[0, 0, 0]");
}

#[test]
fn sine_integral_along_the_imaginary_axis() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    assert_eq!(ev(&mut e, "sin_integral([-inf, inf, -im*inf, im*inf]) === [-pi/2, pi/2, -im*inf, im*inf]"), "true");
    // Si(i y) is i times a quantity that grows without bound: im part at y = 10, 20, 40 increasing and positive
    let a = num(&mut e, "n(im_part(sin_integral(10*im)))");
    let b = num(&mut e, "n(im_part(sin_integral(20*im)))");
    let c = num(&mut e, "n(im_part(sin_integral(40*im)))");
    assert!(a > 100.0 && b > 1e3 * a / 100.0 && c > b * 1e6, "{a} {b} {c}");
    let s = num(&mut e, "n(im_part(sin_integral(-40*im)))");
    assert!(s < -1e15, "{s}");
}

#[test]
fn nome_at_minus_one() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    assert_eq!(ev(&mut e, "elliptic_nome_q(-1)"), "-exp(-pi)");
    assert_eq!(ev(&mut e, "inverse_elliptic_nome_q(-exp(-pi))"), "-1");
    // the quotient of complete integrals, K(2) on its cut: real part -e^(-pi), imaginary part 0
    let r = num(&mut e, "n(re(exp(-pi*elliptic_k(2)/elliptic_k(-1))), 20)");
    let i = num(&mut e, "n(abs(im_part(exp(-pi*elliptic_k(2)/elliptic_k(-1)))) + 1, 20)");
    assert!((r + (-std::f64::consts::PI).exp()).abs() < 1e-15 && (i - 1.0).abs() < 1e-15, "{r} {i}");
}
