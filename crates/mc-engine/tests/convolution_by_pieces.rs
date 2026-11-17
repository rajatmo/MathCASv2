//! Unilateral convolutions of functions given by pieces (boxes, triangles, steps): each value is checked against a
//! numerical integral of f(tau) g(t - tau) over [0, t] at points of every piece.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let stop = StopFlag::new();
            let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
            let out = match e.eval(&s, &stop) {
                Ok(v) => v.printed,
                Err(err) => format!("error: {err}"),
            };
            let _ = tx.send(out);
        })
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(60)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

/// |c(t0) - integral over [0, t0]| < 1e-8 at each t0, with c the closed form of `conv`.
fn agrees(conv: &str, f: &str, g: &str, points: &[&str]) {
    for p in points {
        let src = format!(
            "{{ let c = {conv}; abs(n(rewrite(c, t -> {p})) - n_integrate(rewrite({f}*{g}, [x -> s, y -> {p} - s]), [s, 0, {p}])) < 10^-8 }}"
        );
        assert_eq!(ev(&src), "true", "at t = {p}: {src}");
    }
}

#[test]
fn boxes_and_a_gaussian() {
    let c = "unilateral_convolve(unit_box(u - 1), unit_box(u - 1), u, t)";
    assert!(ev(c).starts_with("piecewise("), "{}", ev(c));
    agrees(c, "unit_box(x - 1)", "unit_box(y - 1)", &["1/4", "1.3", "2", "2.6", "3.5"]);
    let c = "unilateral_convolve(unit_box(u - 2), exp(-u^2), u, t)";
    assert!(ev(c).starts_with("piecewise("), "{}", ev(c));
    agrees(c, "unit_box(x - 2)", "exp(-y^2)", &["1", "1.7", "2.2", "3", "6"]);
}

#[test]
fn smooth_functions_keep_the_direct_integral() {
    assert_eq!(ev("unilateral_convolve(exp(-x), sin(x), x, t)"), "(1/2)*(-cos(t) + exp(-t) + sin(t))");
}
