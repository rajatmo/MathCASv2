//! Properties of special functions proved one by one: the range of the hyperfactorial on `x >= a`, of `elliptic_pi` in
//! its characteristic, of `cos(x)/sqrt(x)` (and of the Bessel functions of order 1/2 written with it), the sign in
//! several variables from the table (`inverse_gamma_regularized`), and the injectivity in the plane of `exp(p(x))`
//! times a rational function (the Coulomb wave functions of a spherical Hankel form), and that the bounded imaginary
//! part of the subfactorial on the line is not onto. Every new path is also called
//! outside its region (machine numbers, exact numbers on each side of every bound, intervals) under a time limit.
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

#[test]
fn hyperfactorial_range_from_minus_one() {
    let r = ev("n(function_range([hyperfactorial(x), x >= -1], x, y), 20)");
    let ns = numbers(&r);
    assert!(r.starts_with("y >="), "{r}");
    // the smallest value on a fine sample of [-1, 3] lies just above the bound
    let mn: f64 = ev("min(map(t => n(hyperfactorial(t/1000)), range(-1000, 3000)))").parse().unwrap();
    assert!(ns[0] <= mn + 1e-12 && mn - ns[0] < 1e-6, "{r} {mn}");
    assert!((ns[0] - 0.879786843059399).abs() < 1e-12, "{r}");
    // the same minimum from any end in [-1, 0], open or closed
    for c in ["x >= 0", "x > -1", "x > -1/2", "-1/3 <= x"] {
        let s = ev(&format!("n(function_range([hyperfactorial(x), {c}], x, y), 20)"));
        assert_eq!(s, r, "{c}");
    }
}

#[test]
fn hyperfactorial_range_outside_the_proved_ends_stays_or_ends() {
    // ends beyond [-1, 0], machine ends, symbolic and interval ends: no new answer, and every call ends
    for c in ["x >= -2", "x >= 1/2", "x >= 1", "x >= -1.0", "x >= 0.5", "x >= a", "x >= interval([-1, 0])", "x <= 0", "x >= -1 and x <= 2"] {
        let s = ev(&format!("function_range([hyperfactorial(x), {c}], x, y)"));
        assert_ne!(s, "TIMEOUT", "{c}");
        assert!(!s.contains("root("), "{c}: {s}");
    }
    assert_ne!(ev("function_range(hyperfactorial(x), x, y)"), "TIMEOUT");
}

#[test]
fn elliptic_pi_range_in_the_characteristic() {
    assert_eq!(ev("function_range(elliptic_pi(n_u, 1/5), n_u, y)"), "elliptic_k(1/5) - elliptic_pi(1/5, 1/5) < y and y < 0 or y > 0");
    // the lower end is the limit at n -> 1+ (the principal value K(m) - Pi(m/n, m), DLMF 19.6.5)
    let lo: f64 = ev("n(elliptic_k(1/5) - elliptic_pi(1/5, 1/5))").parse().unwrap();
    let near: f64 = ev("n(elliptic_pi(1 + 10^-9, 1/5))").parse().unwrap();
    assert!(lo < near && near - lo < 1e-6, "{lo} {near}");
    // an affine argument has the same range
    assert_eq!(ev("function_range(elliptic_pi(2*n_u + 3, 1/2), n_u, y)"), "elliptic_k(1/2) - elliptic_pi(1/2, 1/2) < y and y < 0 or y > 0");
}

#[test]
fn elliptic_pi_range_outside_zero_to_one_stays_or_ends() {
    for m in ["0", "1", "-1", "2", "3/2", "0.5", "1.0", "interval([1/4, 1/2])", "a"] {
        let s = ev(&format!("function_range(elliptic_pi(n_u, {m}), n_u, y)"));
        assert_ne!(s, "TIMEOUT", "{m}");
        assert!(!s.contains("elliptic_k(") || m == "0", "{m}: {s}");
    }
}

#[test]
fn cosine_over_square_root() {
    let r = ev("n(function_range(cos(x)/sqrt(x), x, y), 30)");
    assert!(r.starts_with("y >="), "{r}");
    let mn: f64 = ev("min(map(t => n(cos(t/100)/sqrt(t/100)), range(1, 6000)))").parse().unwrap();
    let lo = numbers(&r)[0];
    assert!(lo <= mn + 1e-12 && mn - lo < 1e-4, "{r} {mn}");
    // Y_(1/2)(z) = -cos(z) sqrt(2/(pi z)): bounded above, so not onto
    let r = ev("n(function_range(bessel_y(1/2, z), z, y), 30)");
    assert!(r.starts_with("y <="), "{r}");
    assert_eq!(ev("table(function_surjective(bessel_y(1 / n_u, z), z), n_u, 2)"), "[false, false]");
    // J_(1/2)(x) = sin(x) sqrt(2/(pi x)): both ends
    let r = ev("n(function_range(bessel_j(1/2, x), x, y), 30)");
    assert_eq!(numbers(&r).len(), 2, "{r}");
}

#[test]
fn trig_over_square_root_of_other_arguments_stays_or_ends() {
    for f in ["cos(x)/sqrt(-x)", "cos(x)/sqrt(a*x)", "cos(x)/sqrt(-pi*x)", "cos(2*x)/sqrt(x)", "cos(x)/sqrt(x + 1)", "cos(x)/sqrt(interval([1, 2])*x)", "cos(x)/sqrt(0.5*x)"] {
        let s = ev(&format!("function_range({f}, x, y)"));
        assert_ne!(s, "TIMEOUT", "{f}");
    }
}

#[test]
fn sign_of_the_inverse_regularized_gamma_in_both_arguments() {
    // nonnegative on 0 < x <= 1, a > 0 (0 at x = 1): 1, and no strict sign
    assert_eq!(ev("function_sign([inverse_gamma_regularized(a, x), (0 < x and x <= 1) and a > 0], [x, a])"), "1");
    assert_eq!(ev("inverse_gamma_regularized(3, 1)"), "0");
    let s = ev("function_sign([inverse_gamma_regularized(a, x), (0 < x and x <= 1) and a > 0], [x, a], strict_inequalities: true)");
    assert_ne!(s, "1");
    // a region leaving the domain: no sign from the table
    let s = ev("function_sign([inverse_gamma_regularized(a, x), 0 < x and x <= 2 and a > 0], [x, a])");
    assert_ne!(s, "1");
    assert_ne!(ev("function_sign([inverse_gamma_regularized(a, x), 0 < x and x <= 1], [x, a])"), "1");
}

#[test]
fn exponential_times_rational_is_not_injective_in_the_plane() {
    assert_eq!(ev("function_injective(coulomb_h1(2, 0, x), x, complexes)"), "false");
    assert_eq!(ev("function_injective(coulomb_h2(2, 0, x), x, complexes)"), "false");
    assert_eq!(ev("function_injective(exp(im*x)/x, x, complexes)"), "false");
    assert_eq!(ev("function_injective((1 + x)*exp(x^2), x, complexes)"), "false");
    // exp itself is periodic; a parameter or a non-polynomial exponent is not read here
    assert_eq!(ev("function_injective(exp(x), x, complexes)"), "false");
    for f in ["exp(a*x)/x", "exp(sqrt(x))", "exp(1/x)", "x*exp(0)"] {
        assert_ne!(ev(&format!("function_injective({f}, x, complexes)")), "TIMEOUT", "{f}");
    }
    assert_ne!(ev("function_injective(exp(a*x)/x, x, complexes)"), "false");
}

#[test]
fn imaginary_part_of_the_subfactorial_is_bounded_so_not_onto() {
    assert_eq!(ev("function_surjective(im_part(subfactorial(x)), x)"), "false");
    assert_eq!(ev("function_surjective([im_part(subfactorial(x)), x > 2], x)"), "false");
    assert_eq!(ev("function_bijective(im_part(subfactorial(x)), x)"), "false");
    // the bound |Im(!x)| <= pi at sample points on both sides of 0
    for k in -40..=40 {
        let v: f64 = ev(&format!("n(im_part(subfactorial({k}/7 + 1/13)))")).parse().unwrap();
        assert!(v.abs() <= std::f64::consts::PI, "{k} {v}");
    }
    // other forms are not read by this proof: they stay or end
    for f in ["re_part(subfactorial(x))", "im_part(subfactorial(2*x))", "im_part(subfactorial(x)) + x"] {
        let r = ev(&format!("function_surjective({f}, x)"));
        assert_ne!(r, "TIMEOUT", "{f}");
    }
}
