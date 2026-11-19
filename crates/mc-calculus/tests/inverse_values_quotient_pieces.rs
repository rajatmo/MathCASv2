//! Values of inverse functions without a closed form at real numbers (the one real solution), and the pieces of
//! `quotient` and `mod` by a variable where the assumptions bound the ratio. Expected numbers come from Newton's
//! iteration at 35 digits (outside this system) and from the definition `mod(k, m) = k - m floor(k/m)`.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::spawn(move || {
        let stop = StopFlag::new();
        let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
        let out = match e.eval(&s, &stop) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        let _ = tx.send(out);
    });
    rx.recv_timeout(std::time::Duration::from_secs(60)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

#[test]
fn inverse_without_closed_form_at_a_real_number() {
    // sin(y) - y = 2: y = -2.5541959528370430378296661737918779 (the only real solution: the derivative
    // cos(y) - 1 <= 0 vanishes at isolated points only)
    assert_eq!(ev("inverse_function(&(sin(&1) - &1))(2)"), "root([&(2 + &1 - sin(&1)), -2.554195952837043])");
    assert!(ev("n(inverse_function(&(sin(&1) - &1))(2), 30)").starts_with("-2.55419595283704303782966617379"));
    // beta(x, 4) - x + 4 = 3, a quintic with one real root 1.1765901078827981711353549758989095
    let g = "g = inverse_function((x, y) => beta(x, y) - x + y, 1, 2); ";
    assert_eq!(ev(&format!("{g}g(3, 4) === root(&(-6 - 6*&1 - 5*&1^2 + 5*&1^3 + 5*&1^4 + &1^5), 1)")), "true");
    assert!(ev(&format!("{g}n(g(3, 4), 30)")).starts_with("1.17659010788279817113535497589"));
    assert_eq!(ev("inverse_function((x, y) => x^3 + x + y, 1, 2)(1, 2)"), "root(&(1 + &1 + &1^3), 1)");
    // several real solutions (sin(y) y == 1/2), a symbol, or a wrong number of arguments: unevaluated
    assert_eq!(ev("inverse_function(&(sin(&1)*&1))(1/2)"), "inverse_function(&(sin(&1)*&1))(1/2)");
    assert_eq!(ev("inverse_function(&(sin(&1) - &1))(z)"), "inverse_function(&(sin(&1) - &1))(z)");
    assert_eq!(ev("inverse_function((x, y) => x + y, 1, 2)(3)"), "inverse_function((x, y) => x + y, 1, 2)(3)");
    // a closed-form inverse is unchanged
    assert_eq!(ev("inverse_function(&(2*&1 + 3))(5)"), "1");
}

#[test]
fn quotient_and_mod_by_a_variable_under_bounds() {
    let pw = "p = assuming(0 <= k and k <= 3*m, [piecewise_expand(mod(k, m)), piecewise_expand(quotient(k, m))]); ";
    let want = "[piecewise([[k, k/m < 1], [k - 3*m, k/m >= 3], [k - 2*m, 2 <= k/m and k/m < 3]], k - m), \
                piecewise([[1, 1 <= k/m and k/m < 2], [2, 2 <= k/m and k/m < 3], [3, k/m >= 3]], 0)]";
    assert_eq!(ev(&format!("{pw}p === {want}")), "true", "{}", ev(&format!("{pw}p")));
    // k/m = 1/2, 3/2, 5/2, 3 at m = 1/5: mod = k - m floor(k/m), quotient = floor(k/m)
    for (k, want) in [("1/10", "[1/10, 0]"), ("3/10", "[1/10, 1]"), ("1/2", "[1/10, 2]"), ("3/5", "[0, 3]")] {
        assert_eq!(ev(&format!("{pw}rewrite(p, [k -> {k}, m -> 1/5])")), want, "k = {k}");
    }
    // a ratio the assumptions do not bound stays
    assert_eq!(ev("assuming(k >= 0 and m > 0, piecewise_expand(mod(k, m)))"), "mod(k, m)");
}
