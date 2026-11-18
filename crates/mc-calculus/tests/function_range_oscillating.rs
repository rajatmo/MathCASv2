//! `function_range` of the oscillating Bessel and Airy functions, of sin(x)/sqrt(x), of Struve functions of
//! half-integer order and of polynomials of circular functions: the bounds are checked against the extreme values of
//! a fine sample of the function (independent of the critical points the rule names).
#![allow(dead_code)]
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

/// The decimal numbers of a printed condition.
fn numbers(s: &str) -> Vec<f64> {
    s.split(|c: char| c.is_whitespace() || c == '(' || c == ')').filter_map(|w| w.parse::<f64>().ok()).collect()
}

/// The 30-digit bounds of the range of `f` agree with the sampled extremes of `f` on `lo..hi` (step 1/100) to 1e-4 and
/// are never smaller than a sample; `both`: a lower and an upper bound, else only the upper (`upper`) or lower one.
fn bounds_match(f: &str, var: &str, lo: i64, hi: i64, kind: &str) {
    let r = ev(&format!("n(function_range({f}, {var}, y), 30)"));
    let ns = numbers(&r);
    let fs = f.replace(var, "(t/100)");
    let sample = |op: &str| -> f64 {
        let s = ev(&format!("{op}(map(t => n({fs}), range({lo}, {hi})))"));
        s.parse::<f64>().unwrap_or(f64::NAN)
    };
    match kind {
        "both" => {
            assert_eq!(ns.len(), 2, "{f}: {r}");
            let (mn, mx) = (sample("min"), sample("max"));
            assert!(ns[0] <= mn + 1e-12 && mn - ns[0] < 1e-4, "{f}: lower {r} vs {mn}");
            assert!(ns[1] >= mx - 1e-12 && ns[1] - mx < 1e-4, "{f}: upper {r} vs {mx}");
        }
        "upper" => {
            assert_eq!(ns.len(), 1, "{f}: {r}");
            assert!(r.starts_with("y <="), "{f}: {r}");
            let mx = sample("max");
            assert!(ns[0] >= mx - 1e-12 && ns[0] - mx < 1e-4, "{f}: upper {r} vs {mx}");
        }
        _ => {
            assert_eq!(ns.len(), 1, "{f}: {r}");
            assert!(r.starts_with("y >="), "{f}: {r}");
            let mn = sample("min");
            assert!(ns[0] <= mn + 1e-12 && mn - ns[0] < 1e-4, "{f}: lower {r} vs {mn}");
        }
    }
}

#[test]
fn bessel_j_of_integer_order() {
    // odd orders: symmetric; even orders: the minimum at the second critical point
    for n in [1, 2, 3, 4, 7] {
        bounds_match(&format!("bessel_j({n}, x)"), "x", -4000, 4000, "both");
    }
    bounds_match("anger_j(1, x)", "x", -3000, 3000, "both");
    // an affine image: a negative factor swaps the ends
    bounds_match("-3*bessel_j(2, x) + 5", "x", -3000, 3000, "both");
}

#[test]
fn bessel_y_of_integer_order() {
    // Y_n tends to -inf at 0+: bounded above by the value at the first critical point beyond the first zero
    for n in [1, 2, 3] {
        let r = ev(&format!("n(function_range(bessel_y({n}, x), x, y), 30)"));
        assert!(r.starts_with("y <="), "{r}");
        let ub = numbers(&r)[0];
        let s = ev(&format!("max(map(t => n(bessel_y({n}, t/100)), range(100, 6000)))"));
        let mx: f64 = s.parse().unwrap();
        assert!(ub >= mx - 1e-12 && ub - mx < 1e-4, "Y_{n}: {r} vs {mx}");
    }
    assert!(ev("n(function_range(bessel_y(1, x), x, y), 8)").contains("0.4167299"));
}

#[test]
fn airy_functions() {
    bounds_match("airy_ai(x)", "x", -1500, 1500, "both");
    bounds_match("airy_bi(x)", "x", -1500, 400, "lower");
    bounds_match("2*airy_ai(3*x - 1) - 1", "x", -1500, 1500, "both");
}

#[test]
fn sine_over_square_root() {
    let r = ev("n(function_range(sin(x)/sqrt(x), x, y), 30)");
    let ns = numbers(&r);
    assert_eq!(ns.len(), 2, "{r}");
    let mx: f64 = ev("max(map(t => n(sin(t/100)/sqrt(t/100)), range(1, 6000)))").parse().unwrap();
    let mn: f64 = ev("min(map(t => n(sin(t/100)/sqrt(t/100)), range(1, 6000)))").parse().unwrap();
    assert!(ns[0] <= mn + 1e-12 && mn - ns[0] < 1e-4, "{r} {mn}");
    assert!(ns[1] >= mx - 1e-12 && ns[1] - mx < 1e-4, "{r} {mx}");
}

#[test]
fn struve_functions_of_half_integer_order() {
    for src in ["struve_l(1/2, x)", "struve_l(3/2, x)", "struve_l(5/2, x)", "struve_h(3/2, x)", "struve_h(5/2, x)"] {
        assert_eq!(ev(&format!("function_range({src}, x, y)")), "y > 0", "{src}");
        // positive on a sample (at 50 digits: the elementary forms cancel at small x)
        let s = ev(&format!("min(map(t => n({src} /. x -> t/10, 50), range(1, 400)))"));
        assert!(s.parse::<f64>().unwrap() > 0.0, "{src}: {s}");
    }
    // H_(1/2) vanishes at 2 pi k: not claimed; no answer
    assert!(ev("function_range(struve_h(1/2, x), x, y)").starts_with("function_range("));
}

#[test]
fn undecided_cases_stay_unevaluated() {
    // a non-integer order: Anger's and Bessel's functions are not read (order 1/2 is the elementary form
    // sin(x) sqrt(2/(pi x)), whose range is proved); an order above the checked ones
    assert!(ev("function_range(bessel_j(1/3, x), x, y)").starts_with("function_range("));
    assert!(ev("function_range(bessel_j(20, x), x, y)").starts_with("function_range("));
    assert!(ev("function_range(bessel_j(a, x), x, y)").starts_with("function_range("));
}

#[test]
fn pochhammer_of_order_one_half_is_onto() {
    assert_eq!(ev("function_range(pochhammer(x, 1/2), x, y)"), "true");
}

#[test]
fn several_variables_and_circular_polynomials() {
    assert_eq!(ev("function_range(spherical_harmonic_y(2, 0, t, p), [t, p], y)"), "(-1/4)*5^(1/2)*pi^(-1/2) <= y and y <= (1/2)*5^(1/2)*pi^(-1/2)");
    assert_eq!(ev("function_range(spherical_harmonic_y(2, 0, t, p), [t, p], y, complexes)"), "true");
    assert_eq!(ev("function_range(1 + cos(2*x)^2, x, y, complexes)"), "true");
    assert_eq!(ev("function_range(cos(x) + sin(x), x, y, complexes)"), "function_range(cos(x) + sin(x), x, y, complexes)");
}

#[test]
fn hyperfactorial_derivative() {
    // H'(x) = H(x) (log gamma(x + 1) + x + 1/2 - log(2 pi)/2) against a difference quotient
    let d: f64 = ev("n(d(hyperfactorial(x), x) /. x -> 7/10)").parse().unwrap();
    let q: f64 = ev("(n(hyperfactorial(7001/10000)) - n(hyperfactorial(6999/10000)))/(2/10000)").parse().unwrap();
    assert!((d - q).abs() < 1e-6, "{d} {q}");
}
