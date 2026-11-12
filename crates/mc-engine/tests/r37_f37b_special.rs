//! Round 37: fixed wrong values and hangs in special functions. Each test
//! carries the reported input and variations of its own, each checked by a route other than the code under test: the
//! defining series, a functional equation, a second algorithm, or substitution into the forward function.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use std::time::{Duration, Instant};

/// Evaluate every statement in one fresh session (one worker thread, big stack); `TIMEOUT` after 120 s.
fn ev(src: &str) -> String {
    ev_stop(src, None)
}

/// As [`ev`], raising the stop flag after `stop_after` when given.
fn ev_stop(src: &str, stop_after: Option<Duration>) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    let stop = StopFlag::new();
    let flag = stop.clone();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
            let out = match e.eval(&s, &flag) {
                Ok(v) => v.printed,
                Err(err) => format!("error: {err}"),
            };
            let _ = tx.send(out);
        })
        .expect("spawn");
    if let Some(d) = stop_after {
        if let Ok(v) = rx.recv_timeout(d) {
            return v;
        }
        stop.raise();
    }
    rx.recv_timeout(Duration::from_secs(120)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

/// A printed machine real.
fn num(s: &str) -> f64 {
    s.trim().parse().unwrap_or_else(|_| panic!("not a number: {s}"))
}

/// `abs(expr)` evaluated at 30 digits, as a machine real: the size of a difference that must vanish.
fn small(expr: &str, tol: f64) {
    let out = ev(&format!("n(n(abs({expr}), 30))"));
    // a ball around 0 (every digit cancelled) prints as ball(0, r)
    let v = match out.strip_prefix("ball(0, ").and_then(|r| r.strip_suffix(')')) {
        Some(r) => num(r),
        None => num(&out),
    };
    assert!(v <= tol, "{expr}: {v} > {tol}");
}

// ---------------------------------------------------------------- 1. half-integer Struve forms on the negative axis

/// The defining series H_nu(z) = sum (-1)^k (z/2)^(2k + nu + 1)/(Gamma(k + 3/2) Gamma(k + nu + 3/2)) (DLMF 11.2.1) and
/// L_nu likewise without the sign, with principal powers, summed exactly to 80 terms.
fn struve_series(l: bool, nu: &str, z: &str) -> String {
    let sign = if l { "" } else { "(-1)^k*" };
    format!("sum({sign}(({z})/2)^(2*k + {nu} + 1)*rg(k, {nu}), [k, 0, 80])")
}

/// The reported H_(5/2)(-17/10), then other orders (one stepped down through the zero of 1/Gamma(nu + 3/2)), L, and
/// a complex argument, each closed form against the defining series at 30 digits.
#[test]
fn struve_half_integer_negative_axis() {
    // 1/(Gamma(k + 3/2) Gamma(k + nu + 3/2)), 0 at the poles of the second factor
    let rg = "rg(k_, nu_) := if is_integer(k + nu + 3/2) and k + nu + 3/2 <= 0 then 0 else 1/(gamma(k + 3/2)*gamma(k + nu + 3/2))";
    let v = num(&ev("im_part(n(struve_h(5/2, -17/10)))"));
    assert!((v + 0.094_374_656_662_968_43).abs() < 1e-15, "{v}");
    for (l, nu, z) in [
        (false, "5/2", "-17/10"),
        (false, "-3/2", "-5/2"),
        (false, "9/2", "-3"),
        (true, "-7/2", "-17/10"),
        (true, "1/2", "-17/10"),
        (false, "5/2", "-3/2 + im/2"),
    ] {
        let f = if l { "struve_l" } else { "struve_h" };
        let out = ev(&format!("{rg}; n(n(abs({f}({nu}, {z}) - {}), 30))", struve_series(l, nu, z)));
        let d = match out.strip_prefix("ball(0, ").and_then(|r| r.strip_suffix(')')) {
            Some(r) => num(r),
            None => num(&out),
        };
        assert!(d < 1e-25, "{f}({nu}, {z}): {out}");
    }
    // the exact closed form and the machine series kernel agree on the axis
    small("struve_h(5/2, -17/10) - struve_h(5/2, -1.7)", 1e-14);
    small("lommel_s1(5/2, 5/2, -17/10) - lommel_s1(5/2, 5/2, -1.7)", 1e-13);
}

// ---------------------------------------------------------------- 2. Kelvin ker and kei at 0

/// ker_2(0) = 1/2 (reported -inf); the other integer orders: the sign of the infinity or the value, against the
/// function at small x (40-digit evaluation of the closed form), where the leading term (1/2)((n-1)!)(2/x)^n e^(-3 n
/// pi i/4) or the next one dominates.
#[test]
fn kelvin_at_zero() {
    assert_eq!(ev("kelvin_ker(2, 0)"), "1/2");
    assert_eq!(ev("kelvin_ker(2, 0.)"), "0.5");
    assert_eq!(ev("kelvin_ker(-2, 0)"), "1/2");
    assert_eq!(ev("kelvin_kei(0, 0.)"), "-0.7853981633974483");
    small("kelvin_ker(2, 10^-8) - 1/2", 1e-15);
    for (f, n, want) in [
        ("ker", 1, "-inf"),
        ("kei", 1, "-inf"),
        ("kei", 2, "inf"),
        ("ker", 3, "inf"),
        ("kei", 3, "-inf"),
        ("ker", 4, "-inf"),
        ("kei", 4, "inf"),
        ("ker", -3, "-inf"),
    ] {
        assert_eq!(ev(&format!("kelvin_{f}({n}, 0)")), want, "{f}_{n}(0)");
        let near = num(&ev(&format!("n(kelvin_{f}({n}, 10^-6), 20)")));
        assert!(near.abs() > 1e5 && (near > 0.0) == (want == "inf"), "{f}_{n}(1e-6) = {near}");
    }
}

/// Machine values near 0 lost the digits of the cancelling terms x^(-n) (found here: kelvin_ker(2, 1e-6) was 0.49988,
/// kelvin_ber(2, 1e-6) 0.2 % off): against 40-digit values and the leading term x^4/96 of ber_2.
#[test]
fn kelvin_machine_values_near_zero() {
    for (e, exact) in [("kelvin_ker(2, 1e-6)", "kelvin_ker(2, 10^-6)"), ("kelvin_ker(2, 1e-3)", "kelvin_ker(2, 10^-3)"), ("kelvin_ber(2, 1e-6)", "kelvin_ber(2, 10^-6)")] {
        let m = num(&ev(e));
        let t = num(&ev(&format!("n(n({exact}, 40))")));
        assert!((m - t).abs() <= 4e-16 * t.abs(), "{e}: {m} vs {t}");
    }
    let b = num(&ev("kelvin_ber(2, 1e-6)"));
    assert!((b / (1e-24 / 96.0) - 1.0).abs() < 1e-10, "{b}");
}

// ---------------------------------------------------------------- 3. hyperfactorial and Barnes G

/// H(z) = K(z + 1) = exp(zeta'(-1, z + 1) - zeta'(-1)): complex for non-integer z < -1 (reported real at -2.2),
/// checked by the functional equation H(z) = H(z - 1) z^z (principal power) from a point with Re(z + 1) > 0, at 30
/// digits; the integers H(-n) = (-1)^(n (n - 1)/2) H(n - 1); H(17/5) against the Maclaurin series of log G (DLMF
/// 5.17.3).
#[test]
fn hyperfactorial_continuation() {
    let re = num(&ev("re(hyperfactorial(-2.2))"));
    let im = num(&ev("im_part(hyperfactorial(-2.2))"));
    assert!((re + 0.304_955_095_763_523_7).abs() < 1e-14 && (im + 0.938_555_277_809_103).abs() < 1e-14, "{re} {im}");
    small("hyperfactorial(-11/5) - hyperfactorial(-1/5)/((-1/5)^(-1/5)*(-6/5)^(-6/5))", 1e-28);
    small("hyperfactorial(-3/2 + 3/10*im) - hyperfactorial(-1/2 + 3/10*im)/(-1/2 + 3/10*im)^(-1/2 + 3/10*im)", 1e-28);
    small("1 - hyperfactorial(-67/10)/(-67/10)^(-67/10)/hyperfactorial(-77/10)", 1e-28);
    assert_eq!(ev("[hyperfactorial(-1), hyperfactorial(-2), hyperfactorial(-3), hyperfactorial(-4)]"), "[1, -1, -4, 108]");
    assert_eq!(ev("hyperfactorial(-3.0)"), "-4.0");
    let series = "z = n(2/5, 40); lg = z/2*log(2*pi) - (z + (1 + euler_gamma)*z^2)/2 + sum((-1)^k*zeta(k)*z^(k + 1)/(k + 1), [k, 2, 200]); \
                  lg4 = lg + log_gamma(z + 1) + log_gamma(z + 2) + log_gamma(z + 3)";
    let out = ev(&format!("{series}; n(abs(hyperfactorial(17/5) - exp(log_gamma(z + 4)*(z + 3) - lg4)), 30)"));
    let d = match out.strip_prefix("ball(0, ").and_then(|r| r.strip_suffix(')')) {
        Some(r) => num(r),
        None => num(&out),
    };
    assert!(d < 1e-25, "{out}");
}

/// G real on the negative axis (reported: tiny imaginary parts), by the recurrence G(x + 1) = Gamma(x) G(x) at 30
/// digits and at machine numbers; machine integers are zeros; the machine value equals the 30-digit value of the
/// same binary number.
#[test]
fn barnes_g_negative_axis() {
    for x in ["-2.3", "-0.5", "-12.7", "-7.25"] {
        let v = ev(&format!("barnes_g({x})"));
        assert_eq!(ev(&format!("im_part(barnes_g({x}))")), "0", "barnes_g({x}) = {v}");
        let rel = num(&ev(&format!("abs(barnes_g({x})/(barnes_g({x} + 1)/gamma({x})) - 1)")));
        assert!(rel < 1e-14, "{x}: {rel}");
        let hi = num(&ev(&format!("n(barnes_g(set_precision({x}, 30)))")));
        assert!((num(&v) - hi).abs() <= 2.3e-16 * hi.abs(), "{x}: {v} vs {hi}");
    }
    small("barnes_g(-23/10) - barnes_g(-13/10)/gamma(-23/10)", 1e-30);
    assert_eq!(ev("barnes_g(-3.0)"), "0.0");
}

// ---------------------------------------------------------------- 4. Scorer Hi and the alternating factorial

/// Hi(x) = Bi(x) - Gi(x) at large x (reported: scorer_hi(1e6) never returned): the overflow at once, the machine values
/// against the 30-digit Maclaurin series of the ball path, and a complex argument the series cannot reach declines
/// quickly.
#[test]
fn scorer_hi_large_arguments() {
    let t = Instant::now();
    assert_eq!(ev("scorer_hi(1e6)"), "inf");
    assert_eq!(ev("scorer_hi_prime(1e5)"), "inf");
    assert!(ev("scorer_hi(1e6*im)").starts_with("scorer_hi("));
    assert!(t.elapsed() < Duration::from_secs(20), "{:?}", t.elapsed());
    for (f, x) in [("scorer_hi", "70"), ("scorer_hi_prime", "70"), ("scorer_hi", "45"), ("scorer_hi", "100")] {
        let m = num(&ev(&format!("{f}({x}.)")));
        let s = num(&ev(&format!("n(n({f}({x}), 30))")));
        assert!((m - s).abs() <= 1e-14 * s.abs(), "{f}({x}): {m} vs {s}");
    }
}

/// a(m) by the running factorial (reported: 1.5 s a value, no interrupt) and a(m) at Re m < -1 by the reflection of
/// the gamma product (an indeterminate 0 inf at m = -1000.5): the recurrence a(m) + a(m - 1) = Gamma(m + 1), the two
/// forms against each other where both hold, and an interrupt honoured within seconds.
#[test]
fn alternating_factorial_fast_and_interruptible() {
    let t = Instant::now();
    assert_eq!(ev("alternating_factorial(10000) + alternating_factorial(9999) == factorial(10000)"), "true");
    assert!(t.elapsed() < Duration::from_secs(30), "{:?}", t.elapsed());
    assert_eq!(ev("map(alternating_factorial, [0, 1, 2, 3, 4, 5])"), "[0, 1, 1, 5, 19, 101]");
    for m in ["-5.5", "-1000.5", "-2.5 + 0.5*im", "-30.25"] {
        let d = num(&ev(&format!("m = {m}; abs(alternating_factorial(m) + alternating_factorial(m - 1) - gamma(m + 1))")));
        // the machine formula: sin(pi m) and the incomplete gamma rounded at |m| ~ 30 (relative 1e-14)
        assert!(d < 1e-12, "{m}: {d}");
    }
    let a = num(&ev("alternating_factorial(-1000.5)"));
    assert!((a + 8.539_734_222_673_6).abs() < 1e-12, "{a}");
    small("alternating_factorial(-3/2) - (e*gamma(1/2)*gamma(1/2, 1) - cos(3*pi/2)*(1 + e*exp_integral_ei(-1)))", 1e-28);
    let t = Instant::now();
    let out = ev_stop("alternating_factorial(10^6)", Some(Duration::from_millis(500)));
    assert!(t.elapsed() < Duration::from_secs(15), "{:?}: {}", t.elapsed(), out.chars().take(60).collect::<String>());
}

// ---------------------------------------------------------------- 5. U(a, b, 0) and the inverse nome at 1

/// U(a, b, 0) = Gamma(1 - b)/Gamma(a - b + 1) for Re b < 1 (reported unevaluated; the rule for a = 1 gave 0 at
/// U(1, 1/2, 0)): against U at a small positive z by Tricomi's formula (30 digits), the pole case 0 and the kind of
/// an inexact zero.
#[test]
fn hypergeometric_u_at_zero() {
    assert_eq!(ev("hypergeometric_u(1/3, 1/2, 0)"), "pi^(1/2)/gamma(5/6)");
    assert_eq!(ev("hypergeometric_u(1, 1/2, 0)"), "2");
    assert_eq!(ev("hypergeometric_u(2, -1/2, 0.)"), "0.26666666666666666");
    for (a, b) in [("1/3", "1/2"), ("1", "1/2"), ("2", "-1/2"), ("1/2 + im", "1/4")] {
        let lim = format!("hypergeometric_u({a}, {b}, 0)");
        // U(a, b, z) - U(a, b, 0) = O(z^(1 - Re b)) for 0 < Re b < 1, O(z) for Re b <= 0
        small(&format!("{lim} - hypergeometric_u({a}, {b}, 10^-24)"), 1e-10);
    }
    assert_eq!(ev("hypergeometric_u(-1/2, 1/2, 0)"), "0");
    assert_eq!(ev("hypergeometric_u(1/3, 1/2, 0.)"), ev("n(pi^(1/2)/gamma(5/6))"));
}

/// m(q) -> 1 as q -> 1- (reported unevaluated at 1.): the value at 1 and the approach, through the nome q(m) of m
/// near 1 (a separate function).
#[test]
fn inverse_nome_at_one() {
    assert_eq!(ev("[inverse_elliptic_nome_q(1), inverse_elliptic_nome_q(1.)]"), "[1, 1.0]");
    let m = num(&ev("inverse_elliptic_nome_q(0.9)"));
    assert!(m > 0.999_999 && m <= 1.0, "{m}");
    small("elliptic_nome_q(inverse_elliptic_nome_q(1/2)) - 1/2", 1e-25);
    small("elliptic_nome_q(inverse_elliptic_nome_q(0.3)) - 0.3", 1e-13);
}

// ---------------------------------------------------------------- 6. inverse Jacobi functions for m > 1

/// For m > 1 the inverse goes through the reciprocal modulus (DLMF 22.17): real on the whole real range of the
/// function (reported: complex on half of it for dn, cd, nd, dc), checked by the forward function at 30 digits; the
/// documented end point nd^(-1)(-1 | m) = 2 K(1/m)/sqrt(m).
#[test]
fn inverse_jacobi_large_parameter() {
    for (w, v, m) in [
        ("dn", "-3/10", "3"),
        ("dn", "-9/10", "5/2"),
        ("dc", "-3/10", "3"),
        ("dc", "-7/10", "7"),
        ("cd", "-5/2", "3"),
        ("cd", "-4/3", "11/10"),
        ("nd", "-5/2", "3"),
        ("nd", "-6/5", "2"),
        ("ds", "1/5", "7"),
        ("ds", "-3/2", "3"),
    ] {
        let u = format!("n(inverse_jacobi_{w}({v}, {m}), 30)");
        assert_eq!(ev(&format!("im_part({u})")), "0", "inverse_jacobi_{w}({v}, {m}) = {}", ev(&u));
        small(&format!("jacobi_{w}({u}, {m}) - ({v})"), 1e-27);
        let um = ev(&format!("im_part(inverse_jacobi_{w}(n({v}), n({m})))"));
        assert!(um == "0" || um == "0.0", "machine inverse_jacobi_{w}({v}, {m}): im part {um}");
    }
    small("inverse_jacobi_nd(-1, 3.) - 2*elliptic_k(1/3)/sqrt(3)", 1e-14);
    // outside the real range the value is complex and still an inverse
    small("jacobi_dn(inverse_jacobi_dn(1.5, 3.), 3.) - 1.5", 1e-13);
}
