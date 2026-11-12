//! Round 25: Kelvin functions at 0 and at overflow, Meijer G with a vanishing Slater term.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn run(src: &str) -> String {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

fn real(src: &str) -> f64 {
    let s = run(src);
    s.parse().unwrap_or_else(|_| panic!("{src} gave {s}"))
}

#[test]
fn d23_26_kelvin_values_at_zero_and_overflow() {
    assert_eq!(run("kelvin_ber(1, 0)"), "0");
    assert_eq!(run("kelvin_bei(3/2, 0)"), "0");
    assert_eq!(run("kelvin_ker(0, 0)"), "inf");
    assert_eq!(run("kelvin_kei(0, 0)"), "(-1/4)*pi");
    // e^70710 overflows; the sign is that of cos(x/sqrt(2) - pi/8) = 0.776 (round 27: signed overflow)
    assert_eq!(run("kelvin_ber(1e5)"), "machine_real(\"inf\")");
    assert_eq!(run("kelvin_ber(3000.0)"), "machine_real(\"-inf\")");
    // ker underflows to 0
    assert_eq!(run("kelvin_ker(1e5)"), "0.0");
}

#[test]
fn d23_26_kelvin_derivatives() {
    // the derivative against a central difference at 1.7
    for f in ["kelvin_ber(x)", "kelvin_bei(x)", "kelvin_ker(x)", "kelvin_kei(x)", "kelvin_ber(2, x)", "kelvin_kei(1/2, x)", "kelvin_bei(-3/2, x)"] {
        let up = f.replace("x)", "1.7 + 1e-6)");
        let dn = f.replace("x)", "1.7 - 1e-6)");
        let got = real(&format!("replace(d({f}, x), x -> 1.7, [0, 12])"));
        let fd = real(&format!("({up} - {dn})/(2*1e-6)"));
        assert!((got - fd).abs() < 1e-8, "{f}: {got} vs {fd}");
    }
}

#[test]
fn b23_19_meijer_g_with_a_vanishing_slater_term() {
    // a_3 - b_2 = -2: 1/Gamma(-2) = 0
    let v = real("n(meijer_g([[], [1/3, 4, -1/2]], [[2, 3/2, 2/3], []], 1/2))");
    assert!((v - 0.32763384708414434).abs() < 1e-12, "{v}");
    let v = real("n(meijer_g([[1/3, 1/2], [3]], [[], [7/3]], 2))");
    assert!((v - 0.18459551207164804).abs() < 1e-12, "{v}");
    let v = real("n(meijer_g([[], [3]], [[-1/2, 4, 2/3], []], 1/100))");
    assert!((v - 31.70396761778061).abs() < 1e-9, "{v}");
}

#[test]
fn d23_22_hermite_non_integer_order_and_u_at_large_argument() {
    // H_5/2(100) = 2^(5/2) U(-5/4, 1/2, 10^4): the 1F1 forms overflowed (indeterminate) or did not return
    let v = real("hermite_h(2.5, 100.0)");
    assert!((v - 565632.3917749261).abs() < 1e-6, "{v}");
    let s = run("n(hermite_h(5/2, 100), 30)");
    assert!(s.starts_with("565632.391774926055110776193428"), "{s}");
    // the series against the connection formula at 400 and at 30 digits
    let s = run("n(2^(5/2)*hypergeometric_u(-5/4, 1/2, 400), 30)");
    assert!(s.starts_with("10095.5695786345137667142806280"), "{s}");
    let v = real("hypergeometric_u(0.3, 1.7, 120.0)");
    assert!((v - 0.2380555482227463).abs() < 1e-14, "{v}");
}
