//! Item F2 through an engine session: count_roots over the reals, intervals and rectangles, with rational and
//! algebraic coefficients.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn run_all(cases: &[(&str, &str)]) {
    let mut e = CoreEngine::new();
    let stop = StopFlag::new();
    let mut bad = vec![];
    for (src, want) in cases {
        let got = match e.eval(src, &stop) {
            Ok(v) => v.printed,
            Err(err) => format!("error {err}"),
        };
        if got != *want {
            bad.push(format!("{src}\n   got  {got}\n   want {want}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn count_roots_cases() {
    run_all(&[
        ("count_roots((x - 1)*(x - 2)*(x - 3)*(x - 4), [x, 0, 5])", "4"),
        ("count_roots(x^17 - 1, [x, 0, 1 + im])", "5"),
        ("count_roots((x^2 - 2)*(x^2 - 5), x)", "4"),
        ("count_roots((x^2 - 2)*(x^2 - 5), [x, -2, 2])", "2"),
        ("count_roots((x^4 - 2)*(x^4 - 20), [x, -2 - 2*im, 2 + 2*im])", "4"),
        ("count_roots(1 + x^2, [x, -2*im, 2*im])", "2"),
        ("count_roots(1 + x^2, [x, -1 + im, 1 + im])", "1"),
        ("count_roots(x^3, [x, -1, 1])", "3"),
        ("count_roots((x - 1)*(x - 2), [x, 0, 2])", "2"),
        ("count_roots(x^4 - 1, [x, 0, 1 + im])", "2"),
        ("count_roots((x^2 - 2)^2*(x^4 - 3), [x, -inf, inf])", "6"),
        ("count_roots(x^2 - sqrt(2), x)", "2"),
        ("count_roots((x - sqrt(2))^2*(x + 1), [x, 0, 2])", "2"),
        ("count_roots(x^3 - sqrt(3)*x + 1/5, [x, -sqrt(3), sqrt(3)])", "3"),
        ("count_roots(x^5 - 2^(1/3)*x - 1, x)", "1"),
        ("count_roots(x^2 - 2, [x, sqrt(2), 2])", "1"),
        ("count_roots(x^2 + 1, x)", "0"),
    ]);
}

/// Timings of minimal polynomials (ignored; `--ignored --nocapture`): the mean of ten evaluations with distinct
/// variable names, best of three.
#[test]
#[ignore]
fn minimal_polynomial_timings() {
    let mut e = CoreEngine::new();
    let stop = StopFlag::new();
    for src in [
        "minimal_polynomial(sqrt(2) + sqrt(3) + sqrt(5) + sqrt(7), V)",
        "minimal_polynomial(2^(1/3) + 3^(1/4), V)",
        "minimal_polynomial(sqrt(2) + sqrt(3), V)",
        "minimal_polynomial((1 + sqrt(5))/2 * 2^(1/3), V)",
        "count_roots(x^7 - sqrt(3)*x^3 + 2^(1/3)*x - 1, V)",
    ] {
        let mut best = f64::INFINITY;
        let mut out = String::new();
        for round in 0..3 {
            let t = std::time::Instant::now();
            for i in 0..10 {
                let s = src.replace('V', &format!("v{round}x{i}")).replace("x^", &format!("v{round}x{i}^")).replace("*x", &format!("*v{round}x{i}"));
                out = e.eval(&s, &stop).unwrap().printed;
            }
            best = best.min(t.elapsed().as_secs_f64() * 100.0);
        }
        println!("{src}: {best:.2} ms  ({} chars)", out.len());
    }
}
