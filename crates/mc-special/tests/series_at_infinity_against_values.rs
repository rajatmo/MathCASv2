//! Expansions of special functions at infinity, at a branch point and near a pole, checked against the numeric values
//! of the functions (computed by the evaluators, which share no code with the expansions): the relative error of the
//! truncated series at two points farther and farther out must be small and must fall as the point moves out.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// The relative error of `normal(series(f, x, x0, k))` against `f` at `x = s`, at 30 digits.
fn rel_err(e: &mut CoreEngine, f: &str, x0: &str, k: u32, s: &str) -> f64 {
    let src = format!(
        "ee = {f}; ss = normal(series(ee, x, {x0}, {k})); vv = n(rewrite(ee, [x -> ({s})]), 30); \
         n(abs(vv - n(rewrite(ss, [x -> ({s})]), 30))/abs(vv), 5)"
    );
    let got = e.eval(&src, &StopFlag::new()).map(|v| v.printed).unwrap_or_else(|err| format!("error: {err}"));
    got.trim_end_matches('.').parse::<f64>().unwrap_or_else(|_| panic!("{f} at {x0}, x = {s}: {got}"))
}

#[test]
fn expansions_agree_with_values() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    // (function, point, order, near sample, far sample, bound at the far sample)
    let cases: &[(&str, &str, u32, &str, &str, f64)] = &[
        ("elliptic_k(x)", "inf", 2, "1000", "10000", 1e-8),
        ("elliptic_e(x)", "inf", 2, "1000", "10000", 1e-11),
        ("elliptic_nome_q(x)", "inf", 2, "1000", "10000", 1e-12),
        ("exp_integral_ei(x)", "inf", 6, "100", "1000", 1e-14),
        ("sinh_integral(x)", "inf", 3, "100", "1000", 1e-7),
        ("beta(-1/2, x)", "inf", 4, "100", "1000", 1e-16),
        ("subfactorial(x)", "inf", 2, "100", "1000", 1e-10),
        ("pochhammer(x, 1/3)", "inf", 3, "100", "1000", 1e-14),
        ("hypergeometric_0f1_regularized(2, x)", "inf", 3, "1000", "10000", 1e-10),
        ("laguerre_l(1/2, x)", "inf", 4, "100", "1000", 1e-6),
        ("beta_regularized(x, 1, -1/3)", "inf", 3, "100", "1000", 1e-9),
        ("hypergeometric_pfq_regularized([1/2], [1/3], x)", "inf", 5, "100", "1000", 1e-16),
        ("hypergeometric_2f1(1/3, 1/2, 2, x)", "inf", 3, "-100", "-1000", 1e-9),
        ("hermite_h(1/3, x)", "inf", 3, "100", "1000", 1e-12),
        ("spherical_bessel_j(1/2, x)", "inf", 3, "100", "1000", 1e-8),
        ("spherical_hankel_h2(1/2, x)", "0", 2, "1/100", "1/1000", 1e-11),
        ("product_log(x)", "-exp(-1)", 3, "-exp(-1) + 1/1000", "-exp(-1) + 1/10000", 1e-12),
    ];
    let mut bad = vec![];
    for &(f, x0, k, near, far, bound) in cases {
        let (a, b) = (rel_err(&mut e, f, x0, k, near), rel_err(&mut e, f, x0, k, far));
        if !(b < a && b < bound) {
            bad.push(format!("{f} at {x0}: relative error {a} at {near}, {b} at {far} (bound {bound})"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// The expansions at infinity in every direction: at two radii on eight rays (the real axis on both sides included),
/// the relative error falls and is small. Ei is real on the negative axis (Ei(-30) = -3.02e-15), the complete
/// integrals and the nome are analytic there; off the axis they take the side's sign of the reciprocal-modulus
/// transformation.
#[test]
fn expansions_at_infinity_in_eight_directions() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    let cases: &[(&str, u32, &str, &str)] = &[
        ("exp_integral_ei(x)", 6, "30", "60"),
        ("sinh_integral(x)", 3, "30", "60"),
        ("elliptic_k(x)", 2, "1000", "10000"),
        ("elliptic_e(x)", 2, "1000", "10000"),
        ("elliptic_nome_q(x)", 2, "1000", "10000"),
    ];
    let mut bad = vec![];
    for &(f, k, r1, r2) in cases {
        for j in 0..8 {
            let d = format!("exp(im*pi*{j}/4)");
            let (a, b) = (rel_err(&mut e, f, "inf", k, &format!("{r1}*{d}")), rel_err(&mut e, f, "inf", k, &format!("{r2}*{d}")));
            if !(b < a && b < 1e-3) {
                bad.push(format!("{f} on the ray {j} pi/4: relative error {a} at {r1}, {b} at {r2}"));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
    // the constant of Ei on the real axis is 0 on both sides
    let v = e.eval("ss = normal(series(exp_integral_ei(x), x, inf, 6)); n(abs(rewrite(ss, [x -> -30]) - exp_integral_ei(-30)), 20)", &StopFlag::new()).map(|v| v.printed).unwrap();
    assert!(v.trim_end_matches('.').parse::<f64>().is_ok_and(|d| d < 1e-16), "{v}");
}

/// The dilogarithm at 1 from above takes the value on the cut from below (imaginary part -pi log x): the expansion
/// against the reflection formula summed directly, with log(1 - x) = log(x - 1) + i pi.
#[test]
fn dilogarithm_at_one_from_above() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    let src = "ss = normal(series(polylog(2, x), x, 1, 2, assumptions: x > 1)); \
               ref = pi^2/6 - log(11/10)*(log(1/10) + im*pi) - sum((-1/10)^j/j^2, [j, 1, 80]); \
               n(abs(rewrite(ss, [x -> 11/10]) - ref), 5)";
    let got = e.eval(src, &StopFlag::new()).map(|v| v.printed).unwrap();
    let d: f64 = got.parse().unwrap_or_else(|_| panic!("{got}"));
    // the first omitted term is of order (x - 1)^3 = 1e-3
    assert!(d < 2e-3, "{got}");
}
