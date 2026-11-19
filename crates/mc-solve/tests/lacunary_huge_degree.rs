//! Real roots (and complex ones under max_roots) of lacunary polynomials of huge degree, without building them.
//! Reference values: Newton's iteration in independent 50- and 60-digit arithmetic (mpmath), and hand values.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::spawn(move || {
        let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
        let _ = tx.send(e.eval(&s, &StopFlag::new()).map(|v| v.printed).unwrap_or_else(|err| format!("error: {err}")));
    });
    rx.recv_timeout(std::time::Duration::from_secs(60)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

/// `|n(e, 30) - v| < 10^-k`.
fn close(e: &str, v: &str, k: u32) {
    assert_eq!(ev(&format!("abs(n({e}, 30) - ({v})) < 10^-{k}")), "true", "{e} = {}", ev(&format!("n({e}, 30)")));
}

#[test]
fn one_real_root_near_a_root_of_the_low_part() {
    // f' = 1234567 x^1234566 + 5 > 0: one real root, -1/5 + (1/5)^1234567/5 (-0.2 to 800000 digits)
    let t = std::time::Instant::now();
    assert_eq!(ev("reduce(x^1234567 + 5*x + 1 == 0, x, reals)"), "x == root([&(1 + 5*&1 + &1^1234567), -0.2])");
    close("reduce(x^1234567 + 5*x + 1 == 0, x, reals)[2]", "-1/5", 28);
    assert!(t.elapsed().as_secs_f64() < 20.0, "{:.1} s", t.elapsed().as_secs_f64());
}

#[test]
fn every_real_root_by_the_sign_rule() {
    // x^1234567 + 9 x^2 + 7 x - 1: one positive root, two negative ones (the exact -1 and one near a root of q)
    let s = "solve_values(x^1234567 + 9*x^2 + 7*x - 1 == 0, x, reals)";
    assert_eq!(ev(&format!("length({s})")), "3");
    assert_eq!(ev(&format!("{s}[1]")), "-1");
    close(&format!("{s}[2]"), "-0.9010858031829381838890152", 25);
    close(&format!("{s}[3]"), "0.1233080254051604061112375", 25);
    // x^200000 - 3 x + 1: two positive roots, none negative
    let s = "solve_values(x^200000 - 3*x + 1 == 0, x, reals)";
    close(&format!("{s}[1]"), "0.333333333333333333333333333333", 28);
    close(&format!("{s}[2]"), "1.00000346576790175112299070057", 28);
    // exact roots and the root 0; no real root at all
    assert_eq!(ev("solve(x^200001 - x == 0, x, reals)"), "[[x -> -1], [x -> 0], [x -> 1]]");
    assert_eq!(ev("reduce(x^200000 + x^3 == 0, x, reals)"), "x == -1 or x == 0");
    assert_eq!(ev("solve(x^200000 + 1 == 0, x, reals)"), "[]");
}

#[test]
fn complex_roots_under_max_roots() {
    let s = "solve_values(x^1234567 + 9*x^2 + 7*x - 1 == 0, x, max_roots: 5)";
    assert_eq!(ev(&format!("length({s})")), "5");
    // the first complex root, by 60-digit Newton from the root of unity exp(2 pi i/n)
    close(&format!("{s}[4]"), "1.000002193524393459120654 + 0.000002544700924231246691351866*im", 23);
    // without max_roots the 1234567 roots are not listed
    assert_eq!(ev("solve(x^1234567 + 9*x^2 + 7*x - 1 == 0, x)"), "solve(-1 + 7*x + 9*x^2 + x^1234567 == 0, x)");
}
