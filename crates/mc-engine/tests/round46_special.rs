//! Special functions in depth, third part: complex boxes of the Weierstrass and modular functions, the Ferrers and
//! Laguerre functions of a non-integer degree at 0, the Lerch transcendent beyond the unit circle at inexact integer
//! parameters. Each new enclosure is sampled at 40-digit points of its box and each new value is compared with an
//! independent route (the term path at more digits, a hypergeometric form, an integral representation).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn truth(e: &mut CoreEngine, src: &str) -> bool {
    let r = ev(e, src);
    assert!(r == "true" || r == "false", "{src} gave {r}");
    r == "true"
}

/// |a - b| < 10^-k (|a| + 1), both sides evaluated to 30 digits.
fn close(e: &mut CoreEngine, a: &str, b: &str, k: u32) -> bool {
    let d = format!("(n({a}, 30) - n({b}, 30))");
    truth(e, &format!("n(abs(re{d}) + abs(im_part{d})) < 10^-{k}*(1 + n(abs(n({a}, 30))))"))
}

/// Does the centered interval held by `r` contain the value `v` (evaluated to 30 digits)? The difference of a
/// 30-digit value and a machine centre is taken in machine numbers, so the test allows its rounding, 10^-15 |c|.
fn inside(e: &mut CoreEngine, r: &str, v: &str) -> bool {
    truth(
        e,
        &format!(
            "{{ let c = part({r}, 1); let w = part({r}, 2); let y = n({v}, 30); let s = 10^-15*abs(c); \
             abs(re(y) - re(c)) <= re(w) + s and abs(im_part(y) - im_part(c)) <= (if im_part(w) == 0 then re(w) else im_part(w)) + s }}"
        ),
    )
}

/// The point c + sx rx + i sy ry of a box (c, rx, ry exact) at 40 digits.
fn point(c: &str, r: &str, sx: i64, sy: i64) -> String {
    format!("set_precision(({c}) + ({sx})*re({r}) + ({sy})*im_part({r})*im, 40)")
}

const CORNERS: [(i64, i64); 5] = [(0, 0), (1, 1), (-1, -1), (1, -1), (-1, 1)];

#[test]
fn weierstrass_functions_of_complex_boxes() {
    let t0 = Instant::now();
    let mut e = CoreEngine::new();
    let (g2c, g2r, g3c, g3r) = ("1 + im", "(1 + im)/10^9", "3/4", "1/10^9");
    let (uc, ur) = ("1/2 + im/3", "(1 + im)/10^9");
    let g = format!("[centered_interval({g2c}, {g2r}), centered_interval({g3c}, {g3r})]");
    for (f, tight) in [("weierstrass_p", "10^-6"), ("weierstrass_p_prime", "10^-5"), ("weierstrass_zeta", "10^-7"), ("weierstrass_sigma", "10^-7")] {
        ev(&mut e, &format!("res = {f}(centered_interval({uc}, {ur}), {g})"));
        let res = ev(&mut e, "res");
        assert!(res.starts_with("centered_interval("), "{f}: {res}");
        // the mean value form: radii of the order of the derivative times the radii of the box
        assert!(truth(&mut e, &format!("re(part(res, 2)) < {tight} and im_part(part(res, 2)) < {tight}")), "{f}: {res}");
        // every argument at a corner of its box (g3 real: its two ends), against the term path at 40 digits
        for (i, &(sx, sy)) in CORNERS.iter().enumerate() {
            let (tx, ty) = CORNERS[(i + 2) % 5];
            let v = format!("{f}({}, [{}, set_precision({g3c} + ({sx})*{g3r}, 40)])", point(uc, ur, sx, sy), point(g2c, g2r, tx, ty));
            assert!(inside(&mut e, "res", &v), "{f} at {v}: {res}");
        }
    }
    // the half-periods: the reduced basis of the machine path, enclosed
    for f in ["weierstrass_half_period_w1", "weierstrass_half_period_w2", "weierstrass_half_period_w3"] {
        ev(&mut e, &format!("res = {f}({g})"));
        let res = ev(&mut e, "res");
        assert!(res.starts_with("centered_interval("), "{f}: {res}");
        assert!(truth(&mut e, "re(part(res, 2)) < 10^-8"), "{f}: {res}");
        for &(sx, sy) in &CORNERS {
            let v = format!("{f}([{}, set_precision({g3c} + ({sy})*{g3r}, 40)])", point(g2c, g2r, sx, sy));
            assert!(inside(&mut e, "res", &v), "{f} at {v}: {res}");
        }
    }
    let hp = ev(&mut e, &format!("weierstrass_half_periods({g})"));
    assert!(hp.starts_with("[centered_interval(") && hp.matches("centered_interval").count() == 2, "{hp}");
    // the invariants of boxes of half-periods: sampled, and the round trip to the invariants of the centre
    let (w1c, w3c, wr) = ("1.4730373604361375 - 0.11144815110355921*im", "0.6166094123545918 + 1.422267480800656*im", "(1 + im)/10^9");
    let w = format!("[centered_interval({w1c}, {wr}), centered_interval({w3c}, {wr})]");
    for f in ["weierstrass_invariant_g2", "weierstrass_invariant_g3"] {
        ev(&mut e, &format!("res = {f}({w})"));
        let res = ev(&mut e, "res");
        assert!(res.starts_with("centered_interval("), "{f}: {res}");
        for &(sx, sy) in &CORNERS {
            let v = format!("{f}([{}, {}])", point(w1c, wr, sx, sy), point(w3c, wr, sy, sx));
            assert!(inside(&mut e, "res", &v), "{f} at {v}: {res}");
        }
    }
    let inv = ev(&mut e, &format!("weierstrass_invariants({w})"));
    assert!(inv.starts_with("[centered_interval("), "{inv}");
    // the half-periods were those of the invariants [1 + i, 3/4] (to 12 digits)
    ev(&mut e, &format!("inv1 = part(weierstrass_invariants({w}), 1)"));
    assert!(inside(&mut e, "inv1", "1 + im"), "{inv}");
    // a second lattice: real invariants with a real box
    ev(&mut e, "res = weierstrass_p(centered_interval(0.3 + 0.2*im, (1 + im)/10^6), [centered_interval(2, 1/10^6), 3])");
    let res = ev(&mut e, "res");
    assert!(res.starts_with("centered_interval("), "{res}");
    for &(sx, sy) in &CORNERS {
        let v = format!("weierstrass_p({}, [set_precision(2 + ({sx})/10^6, 40), 3])", point("3/10 + im/5", "(1 + im)/10^6", sx, sy));
        assert!(inside(&mut e, "res", &v), "{v}: {res}");
    }
    assert!(t0.elapsed() < Duration::from_secs(120), "{:?}", t0.elapsed());
}

#[test]
fn modular_functions_of_complex_boxes() {
    let t0 = Instant::now();
    let mut e = CoreEngine::new();
    for (f, c, r) in [
        ("dedekind_eta", "-4 + im", "1/1000"),
        ("dedekind_eta", "1/3 + 3*im/4", "(1 + im)/100"),
        ("modular_lambda", "8 + 2*im", "1/1000"),
        ("modular_lambda", "1/5 + im", "(1 + im)/100"),
        ("klein_invariant_j", "2*im", "1/5"),
        ("klein_invariant_j", "1/2 + im", "(1 + im)/1000"),
    ] {
        ev(&mut e, &format!("res = {f}(centered_interval({c}, {r}))"));
        let res = ev(&mut e, "res");
        assert!(res.starts_with("centered_interval("), "{f}({c}): {res}");
        let r2 = if r.contains("im") { r.to_string() } else { format!("{r}*(1 + im)") };
        for &(sx, sy) in &CORNERS {
            let v = format!("{f}({})", point(c, &r2, sx, sy));
            assert!(inside(&mut e, "res", &v), "{v}: {res}");
        }
    }
    // the mean value form keeps a small box small
    ev(&mut e, "res = dedekind_eta(centered_interval(-4 + im, 1/1000))");
    assert!(truth(&mut e, "re(part(res, 2)) < 0.0005"), "{}", ev(&mut e, "res"));
    assert!(t0.elapsed() < Duration::from_secs(60), "{:?}", t0.elapsed());
}

#[test]
fn ferrers_and_laguerre_functions_of_a_fractional_degree_at_zero() {
    let t0 = Instant::now();
    let mut e = CoreEngine::new();
    // exact values against the hypergeometric route at an inexact 0
    for nu in ["1/2", "1/3", "-5/4", "7/3"] {
        let p = ev(&mut e, &format!("legendre_p({nu}, 0)"));
        assert!(p.contains("gamma") && !p.contains("legendre"), "{p}");
        assert!(close(&mut e, &format!("legendre_p({nu}, 0)"), &format!("legendre_p({nu}, n(0, 30))"), 20), "P {nu}");
        assert!(close(&mut e, &format!("legendre_q({nu}, 0)"), &format!("legendre_q({nu}, n(0, 30))"), 20), "Q {nu}");
        assert!(close(&mut e, &format!("laguerre_l({nu}, 2/5, 0)"), &format!("laguerre_l({nu}, 2/5, n(0, 30))"), 20), "L {nu}");
        assert_eq!(ev(&mut e, &format!("laguerre_l({nu}, 0)")), "1");
    }
    // the Taylor series about 0 are explicit; the Laguerre one has rational coefficients
    assert_eq!(
        ev(&mut e, "series(laguerre_l(1/2, x), x, 0, 5)"),
        "1 - x/2 - x^2/16 - x^3/96 - 5*x^4/3072 - 7*x^5/30720 + O(x^6)"
    );
    for f in ["legendre_p(1/2, x)", "legendre_q(1/2, x)"] {
        let s = ev(&mut e, &format!("series({f}, x, 0, 2)"));
        assert!(!s.contains("legendre"), "{s}");
        // the coefficients against the derivatives at an inexact 0 (30 digits)
        assert!(close(&mut e, &format!("series_coefficient(series({f}, x, 0, 2), 1)"), &format!("rewrite(d({f}, x), x -> n(0, 30))"), 15), "{f}");
    }
    // integer degrees keep their polynomials
    assert_eq!(ev(&mut e, "legendre_p(2, 0)"), "-1/2");
    assert!(t0.elapsed() < Duration::from_secs(60), "{:?}", t0.elapsed());
}

#[test]
fn lerch_transcendent_beyond_the_unit_circle_at_inexact_integer_parameters() {
    let t0 = Instant::now();
    let mut e = CoreEngine::new();
    let v = ev(&mut e, "n(lerch_phi(7/3, 5, 2), 50)");
    assert!(v.contains("0.0487344655404716263859242824558129285765386883887") && v.contains("-0.012391660298049164063685151037078868625719382904"), "{v}");
    // off the cut, against the integral 1/Gamma(s) int t^(s-1) e^(-a t)/(1 - z e^(-t)) dt (Re a > 0, z not in [1, inf))
    for (z, s, a) in [("2 + im", "3.", "2."), ("-3 + im/2", "2.", "1."), ("1.5 - 2*im", "4.", "3.")] {
        let lhs = format!("lerch_phi({z}, {s}, {a})");
        assert!(!ev(&mut e, &lhs).starts_with("lerch_phi"), "{lhs}");
        let rhs = format!("n_integrate(t^({s} - 1)*exp(-{a}*t)/(1 - ({z})*exp(-t)), [t, 0, inf])/gamma({s})");
        let d = format!("abs(({lhs}) - ({rhs})) < 10^-8*(1 + abs({lhs}))");
        assert!(truth(&mut e, &d), "{d}");
    }
    assert!(t0.elapsed() < Duration::from_secs(60), "{:?}", t0.elapsed());
}
