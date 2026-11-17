//! `eliminate` of radicals of the unknowns (each radical a new unknown with its defining equation) and of
//! equations `f(u) == c` through the inverse of `f`. The eliminants are checked at a point that solves the system.
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
fn radicals_of_the_unknowns() {
    // x = 64, y = 5 solve the system: sqrt(64) + sqrt(4) == 10, 2 * 4 + 3 * 25 == 83
    let r = ev("r = eliminate(sqrt(x) + sqrt(y - 1) == 10 and 2 * x^(1/3) + 3 * y^2 == 83, y); [r, rewrite(r, x -> 64)]");
    assert!(r.ends_with(", true]") && r.contains("729*x^12"), "{r}");
    // a radical alone leaves nothing
    assert_eq!(ev("eliminate(sqrt(x) + sqrt(y) == 1, y)"), "true");
}

#[test]
fn equations_of_invertible_functions() {
    // x + y == pi/6 and x - y == -acos(1/3) or acos(1/3): the eliminant at the matching x
    let r = ev("r = eliminate(sin(x + y) == 1/2 and cos(x - y) == 1/3, y); [rewrite(r, x -> (pi/6 + acos(1/3))/2), rewrite(r, x -> (pi/6 - acos(1/3))/2), rewrite(r, x -> 1)]");
    assert_eq!(r, "[true, true, false]", "{r}");
    // functions without a definition: their inverses
    let r = ev("eliminate(f(x - y) == 0 and g(x + y) == 0, y)");
    assert!(r.contains("inverse_function(f, 1, 1)(0)") && r.contains("inverse_function(g, 1, 1)(0)") && r.contains("2*x"), "{r}");
}
