//! Values beyond the old domains, each checked against an independent form: polygamma of a non-integer or complex
//! order (the derivative relation and the defining power series), the polylogarithm of a complex order beyond the
//! unit disk (the power series where it converges, the inversion relation), Appell's F1 outside the unit bidisk (the
//! Euler integral), spheroidal eigenvalues of order 1/2 (the two-sided recurrence at machine orders) and of order 1 at
//! gamma = n pi/2, the Jacobi epsilon and zeta functions at K + i K' and i K', the principal Lambert W on complex
//! boxes, the Mittag-Leffler function at negative arguments (its power series at 40 digits) and series at poles.
//! Every evaluation runs under a stop flag raised when it takes more than a minute.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::sync::mpsc;
use std::time::Duration;

const BOUND: Duration = Duration::from_secs(60);

struct Sys {
    engine: CoreEngine,
}

impl Sys {
    fn new() -> Sys {
        Sys { engine: CoreEngine::new() }
    }
}

/// Evaluate under a stop flag that a watcher raises when the evaluation takes longer than BOUND.
fn run(s: &mut Sys, src: &str) -> String {
    let stop = StopFlag::new();
    let (done, wait) = mpsc::channel::<()>();
    let flag = stop.clone();
    let watcher = std::thread::spawn(move || {
        if wait.recv_timeout(BOUND).is_err() {
            flag.raise();
        }
    });
    let r = s.engine.eval(src, &stop);
    let _ = done.send(());
    watcher.join().unwrap();
    match r {
        Ok(v) => v.printed,
        Err(e) => panic!("{src}: {e}"),
    }
}

/// A printed real number (machine digits, possibly with a power of ten).
fn real(s: &str) -> f64 {
    let t = s.trim().replace("*10^", "e");
    t.parse().unwrap_or_else(|_| panic!("not a number: {s}"))
}

/// The value of a numerical expression as (re, im).
fn value(s: &mut Sys, expr: &str) -> (f64, f64) {
    (real(&run(s, &format!("n(re({expr}))"))), real(&run(s, &format!("n(im_part({expr}))"))))
}

fn close(a: (f64, f64), b: (f64, f64), tol: f64) -> bool {
    let d = (a.0 - b.0).hypot(a.1 - b.1);
    d <= tol * b.0.hypot(b.1).max(1e-300)
}

/// `a - b` simplifies to 0.
fn same(s: &mut Sys, a: &str, b: &str) {
    assert_eq!(run(s, &format!("simplify(({a}) - ({b}))")), "0", "{a} against {b}");
}

#[test]
fn polygamma_fractional_order_power_series() {
    // |z| < 1: the term-by-term derivative of digamma's series (DLMF 5.7.4) at 20 digits; 150 terms leave 0.7^150
    let mut s = Sys::new();
    for (nu, z) in [("1/2", "1/2"), ("3/2", "3/10"), ("-1/2", "7/10"), ("13/4", "2/5")] {
        let series = format!(
            "n(-({z})^(-1 - {nu})*(log({z}) - digamma(-{nu}) - euler_gamma)/gamma(-{nu}) - euler_gamma*({z})^(-{nu})/gamma(1 - {nu}) + \
             sum(map(k => (-1)^(k + 1)*zeta(k + 1)*gamma(k + 1)/gamma(k + 1 - {nu})*({z})^(k - {nu}), range(1, 150))), 20)"
        );
        let want = value(&mut s, &series);
        let got = value(&mut s, &format!("polygamma(n({nu}), n({z}))"));
        assert!(close(got, want, 1e-13), "polygamma({nu}, {z}): {got:?} against {want:?}");
    }
}

#[test]
fn polygamma_fractional_order_derivative_relation() {
    // d/dz polygamma(nu - 1, z) = polygamma(nu, z) for the base-point-0 derivative, also beyond the unit disk:
    // a central difference of step 1e-4 (error about 1e-8 relative)
    let mut s = Sys::new();
    for (nu, z) in [("1.5", "3.7"), ("0.25", "12.0"), ("2.5 + 0.5*im", "2.5 + 3.0*im"), ("6.0 + 1.0*im", "1.5 - 2.0*im")] {
        let h = 1e-4;
        let left = value(&mut s, &format!("polygamma({nu} - 1, {z} - {h})"));
        let right = value(&mut s, &format!("polygamma({nu} - 1, {z} + {h})"));
        let diff = ((right.0 - left.0) / (2.0 * h), (right.1 - left.1) / (2.0 * h));
        let got = value(&mut s, &format!("polygamma({nu}, {z})"));
        assert!(close(got, diff, 1e-6), "polygamma({nu}, {z}): {got:?} against the difference {diff:?}");
    }
    // continuity into the integer orders
    let a = value(&mut s, "polygamma(1.9999999, 0.75)");
    let b = value(&mut s, "polygamma(2, 0.75)");
    assert!(close(a, b, 1e-5), "{a:?} {b:?}");
    // a real order and argument give a real value; exact arguments stay
    assert!(!run(&mut s, "polygamma(0.5, 0.5)").contains("im"));
    assert_eq!(run(&mut s, "polygamma(1/2, 2)"), "polygamma(1/2, 2)");
}

#[test]
fn polylog_complex_order_beyond_the_disk() {
    let mut s = Sys::new();
    // |z| = 0.95, where Jonquiere's relation takes over: the power series (2000 terms leave 0.95^2000 ~ 1e-45)
    for (sv, z) in [("0.3 + 0.5*im", "0.95*im"), ("-1.5 + 2.0*im", "-0.92 + 0.1*im")] {
        let want = value(&mut s, &format!("sum(map(k => ({z})^k/k^({sv}), range(1, 2000)))"));
        let got = value(&mut s, &format!("polylog({sv}, {z})"));
        assert!(close(got, want, 1e-11), "polylog({sv}, {z}): {got:?} against {want:?}");
    }
    // beyond the disk: Li_s(z) + e^(i pi s) Li_s(1/z) = (2 pi)^s e^(i pi s/2)/Gamma(s) zeta(1 - s, 1/2 + log(-z)/(2 pi i))
    // (the inversion relation, DLMF 25.12.12 for any s): z = 0.5 - i against 1/z inside the disk
    let lhs = value(&mut s, "polylog(0.2 + im, 0.5 - im) + exp(im*pi*(0.2 + im))*polylog(0.2 + im, 1/(0.5 - im))");
    let rhs = value(&mut s, "(2*pi)^(0.2 + im)*exp(im*pi*(0.2 + im)/2)/gamma(0.2 + im)*zeta(1 - (0.2 + im), 1/2 + log(-(0.5 - im))/(2*pi*im))");
    assert!(close(lhs, rhs, 1e-11), "{lhs:?} {rhs:?}");
}

#[test]
fn appell_f1_outside_the_bidisk() {
    let mut s = Sys::new();
    // c - a = -1: (1 - x)^-b1 (1 - y)^-b2 (1 - (b1 X + b2 Y)/c), X = x/(x - 1)
    let got = value(&mut s, "appell_f1(3, 2, 1, 2, 7, 5.2)");
    let want = value(&mut s, "(1 - 7)^-2*(1 - 5.2)^-1*(1 - (2*7/6 + 5.2/4.2)/2)");
    assert!(close(got, want, 1e-14), "{got:?} {want:?}");
    let got = value(&mut s, "n(appell_f1(3, 2, 5, 2, 3, 5), 10)");
    assert!(close(got, (29.0 / 32768.0, 0.0), 1e-9), "{got:?}");
    // the Euler integral Gamma(c)/(Gamma(a) Gamma(c - a)) int_0^1 u^(a-1) (1-u)^(c-a-1) (1-ux)^-b1 (1-uy)^-b2 du
    // (DLMF 16.15.1) for re c > re a > 0, x, y off the cuts
    let got = value(&mut s, "appell_f1(1/2, 1/3, 1, 2, -3.0, 2.0 + im)");
    let want = value(
        &mut s,
        "gamma(2)/(gamma(1/2)*gamma(3/2))*n_integrate(u^(-1/2)*(1 - u)^(1/2)*(1 + 3*u)^(-1/3)*(1 - u*(2 + im))^(-1), [u, 0, 1])",
    );
    assert!(close(got, want, 1e-9), "{got:?} {want:?}");
}

#[test]
fn spheroidal_eigenvalues_order_half_and_one() {
    let mut s = Sys::new();
    same(&mut s, "spheroidal_eigenvalue(n, 1/2, c)", "-1/4 + mathieu_characteristic_a(1/2 + n, c^2/4) - c^2/2");
    // the Mathieu form at m = 1/2 against the two-sided recurrence at the machine order 0.5
    for (n, g) in [("2", "1.3"), ("2/3", "1.3"), ("5", "2.0*im"), ("7/5", "0.4")] {
        let a = value(&mut s, &format!("spheroidal_eigenvalue({n}, 1/2, {g})"));
        let b = value(&mut s, &format!("spheroidal_eigenvalue({n}, 0.5, {g})"));
        assert!(close(a, b, 1e-12), "n = {n}, gamma = {g}: {a:?} {b:?}");
    }
    // lambda^1_n(n pi/2) = 0, and the recurrence agrees near it
    assert_eq!(run(&mut s, "table(spheroidal_eigenvalue(k, 1, k*pi/2), k, range(1, 4))"), "[0, 0, 0, 0]");
    for k in 1..=3 {
        let v = value(&mut s, &format!("spheroidal_eigenvalue({k}, 1, n({k}*pi/2))"));
        assert!(v.0.abs() < 1e-10 && v.1.abs() < 1e-10, "{k}: {v:?}");
    }
}

#[test]
fn jacobi_epsilon_zeta_at_quarter_periods() {
    let mut s = Sys::new();
    assert_eq!(run(&mut s, "jacobi_epsilon(im*elliptic_k(1 - m), m)"), "complex_infinity");
    assert_eq!(run(&mut s, "jacobi_zn(-3*im*elliptic_k(1 - m), m)"), "complex_infinity");
    // the machine functions at u +- h on the real line (K + i K' is a regular point: the mean is the value up to h^2;
    // at u itself the machine amplitude sits on its branch line for m < 0)
    for m in ["0.3", "2.7 + 0.4*im", "-1.5"] {
        for f in ["jacobi_epsilon", "jacobi_zn"] {
            let u = format!("(elliptic_k({m}) + im*elliptic_k(1 - ({m})))");
            let a = value(&mut s, &format!("({f}({u} + 0.0001, {m}) + {f}({u} - 0.0001, {m}))/2"));
            let b = value(&mut s, &format!("rewrite({f}(elliptic_k(mm) + im*elliptic_k(1 - mm), mm), mm -> {m})"));
            assert!(close(a, b, 1e-7), "{f} at m = {m}: {a:?} {b:?}");
        }
    }
    // a pole at i K': the values grow like 1/(u - i K')
    let v = value(&mut s, "jacobi_epsilon(im*elliptic_k(0.7) + 1.0e-6, 0.3)");
    assert!(v.0.abs() > 1e5, "{v:?}");
}

#[test]
fn product_log_complex_box_encloses_its_values() {
    let mut s = Sys::new();
    run(&mut s, "w = product_log(centered_interval(1 - 5*im, (1 + im)/10))");
    let c = value(&mut s, "w[1]");
    let r = value(&mut s, "w[2]");
    // the first-order width |W'| (|cos| + |sin|) rad is 0.0161; the enclosure stays within 20% of it
    assert!(r.0 < 0.0195 && r.1 < 0.0195, "{r:?}");
    for a in -4..=4 {
        for b in -4..=4 {
            let w = value(&mut s, &format!("product_log(1 - 5*im + {a}/40 + {b}*im/40.)"));
            assert!((w.0 - c.0).abs() <= r.0 && (w.1 - c.1).abs() <= r.1, "{a} {b}: {w:?} outside {c:?} +- {r:?}");
        }
    }
}

#[test]
fn mittag_leffler_negative_arguments() {
    // the integral form at machine x <= -1/2 against the power series at 40 digits (x exact)
    let mut s = Sys::new();
    for (a, x) in [("1/4", "-3.9"), ("1/4", "-0.5"), ("1/3", "-2.25"), ("7/10", "-6.5"), ("0.9", "-1.75")] {
        let got = value(&mut s, &format!("mittag_leffler_e({a}, {x})"));
        let want = value(&mut s, &format!("n(mittag_leffler_e(rationalize({a}, 0), rationalize({x}, 0)), 40)"));
        assert!(close(got, want, 4e-15), "E_{a}({x}): {got:?} against {want:?}");
    }
}

#[test]
fn series_at_poles() {
    let mut s = Sys::new();
    same(
        &mut s,
        "normal(series(zeta(x), x, 1, 2))",
        "euler_gamma - (x - 1)*stieltjes_gamma(1) + (x - 1)^2*stieltjes_gamma(2)/2 + 1/(x - 1)",
    );
    // log G near 0 (DLMF 5.17.3 with 5.7.3) and at infinity (DLMF 5.17.5): truncation errors of x^5 and x^-5
    let got = value(&mut s, "rewrite(normal(series(log_barnes_g(x), x, 0, 4)), x -> 0.01) - log_barnes_g(0.01)");
    assert!(got.0.abs() < 1e-9, "{got:?}");
    let got = value(&mut s, "rewrite(normal(series(log_barnes_g(x), x, inf, 4)), x -> 30.0) - log_barnes_g(30.0)");
    assert!(got.0.abs() < 1e-9, "{got:?}");
}
