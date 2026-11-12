//! Round 32: crashes found in round 31 and the bugs after them.
mod common;
use common::{number, Sys};
use std::time::Instant;

fn timed(s: &mut Sys, src: &str, secs: f64) -> String {
    let t = Instant::now();
    let out = s.run(src);
    assert!(t.elapsed().as_secs_f64() < secs, "{src} took {:?}", t.elapsed());
    out
}

#[test]
fn kelvin_series_at_zero_are_closed_forms() {
    let mut s = Sys::new();
    // DLMF 10.61.1-2: ber x = sum (-1)^k (x^2/4)^(2k)/((2k)!)^2, bei x = sum (-1)^k (x^2/4)^(2k+1)/((2k+1)!)^2
    assert_eq!(
        timed(&mut s, "series(kelvin_ber(x), x, 0, 15)", 3.0),
        "1 - x^4/64 + x^8/147456 - x^12/2123366400 + O(x^16)"
    );
    assert_eq!(
        timed(&mut s, "series(kelvin_bei(x), x, 0, 15)", 3.0),
        "x^2/4 - x^6/2304 + x^10/14745600 - x^14/416179814400 + O(x^16)"
    );
    // an independent route: the truncated series against the function at x = 3/10 (30 digits)
    let want = number(&s.run("n(kelvin_bei(3/10), 30)"));
    let got = number(&s.run("n(x^2/4 - x^6/2304 + x^10/14745600 /. x -> 3/10, 30)"));
    assert!((want - got).abs() < 1e-14, "{want} {got}");
}

#[test]
fn kelvin_at_zero_with_a_negative_integer_order() {
    let mut s = Sys::new();
    // J_(-n) = (-1)^n J_n vanishes at 0, so ber and bei do (DLMF 10.61.5 with the rotated argument)
    for src in ["kelvin_ber(-1, 0)", "kelvin_bei(-1, 0)", "kelvin_bei(-2, 0)", "kelvin_ber(-3, 0)"] {
        assert_eq!(s.run(src), "0", "{src}");
    }
}

#[test]
fn mathieu_characteristic_a_at_large_parameters() {
    let mut s = Sys::new();
    // DLMF 28.8.1 at q = 1e30: -2 q + 2 (2r + 1) sqrt(q) - ((2r + 1)^2 + 1)/8 ...
    let v = number(&timed(&mut s, "mathieu_characteristic_a(1, 1e30)", 3.0));
    assert!(((v + 2e30 - 6e15) / 2e30).abs() < 1e-15, "{v}");
    // between the solver's reach and the expansion's: the expansion agrees with the solver one decade below
    let lo = number(&s.run("mathieu_characteristic_a(1, 99999.5)"));
    let hi = number(&timed(&mut s, "mathieu_characteristic_a(1, 100000.5)", 3.0));
    assert!((hi - lo + 2.0).abs() < 1e-2, "{lo} {hi}");
    // negative q, odd order: a_1(-q) = b_1(q), s = 1
    let v = number(&timed(&mut s, "mathieu_characteristic_a(1, -99999.5)", 3.0));
    assert!(((v + 199366.796148) / v).abs() < 1e-9, "{v}");
    // other precisions stay unevaluated instead of spiralling
    assert!(timed(&mut s, "n(mathieu_characteristic_a(1, 10^30), 30)", 3.0).starts_with("mathieu_characteristic_a"));
    // the property test that crashed the test run (allocation of 3.2e16 bytes)
    assert_eq!(timed(&mut s, "function_injective(mathieu_characteristic_a(1, x), x)", 8.0), "false");
}

#[test]
fn struve_and_weber_of_large_argument_fail_cleanly() {
    let mut s = Sys::new();
    for src in ["struve_h(1/3, -1e9)", "weber_e(1/2, 1e6)", "anger_j(1/2, 1e7)", "lommel_s1(1/2, 1/3, 1e8)"] {
        let out = timed(&mut s, src, 3.0);
        assert!(out.contains('('), "{src}: {out}");
    }
    assert_eq!(timed(&mut s, "function_injective(struve_h(1/3, x), x)", 8.0), "false");
    // this call needs most of the sampling limit of the function properties, which is in wall-clock time: on a
    // slower machine the samples stop early and the call declines. Either way it ends cleanly and never says `true`.
    // (Known defect: the limit should count work, not time, so that the answer is the same on every machine.)
    let out = timed(&mut s, "function_injective(weber_e(1/2, x), x)", 8.0);
    assert!(out == "false" || out == "function_injective(weber_e(1/2, x), x)", "{out}");
    // the values below the limit are unchanged: E_(1/2)(1000) from the series against Y-free closed form E_(1/2)
    let v = number(&s.run("weber_e(1/2, 1e3)"));
    assert!((v - 0.013871100569127558).abs() < 1e-12, "{v}");
}

#[test]
fn spheroidal_eigenvalue_of_a_huge_gamma_stays_unevaluated() {
    let mut s = Sys::new();
    let out = timed(&mut s, "spheroidal_eigenvalue(2, 0, 1e30)", 3.0);
    assert_eq!(out, "spheroidal_eigenvalue(2, 0, 1e30)");
}

#[test]
fn coulomb_f_at_complex_rho_matches_the_defining_series() {
    let mut s = Sys::new();
    // DLMF 33.6.5 at l = 0: F_0(eta, rho) = C_0(eta) sum_(k >= 1) A_k rho^k, A_1 = 1, A_2 = eta,
    // k (k - 1) A_k = 2 eta A_(k-1) - A_(k-2), C_0^2 = 2 pi eta/(exp(2 pi eta) - 1); evaluated in complex arithmetic here
    let (eta, re, im) = (0.5f64, 1.5f64, 0.5f64);
    let c0 = (2.0 * std::f64::consts::PI * eta / ((2.0 * std::f64::consts::PI * eta).exp() - 1.0)).sqrt();
    let mul = |a: (f64, f64), b: (f64, f64)| (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0);
    let mut a = vec![0.0, 1.0, eta];
    for k in 3..80usize {
        let next = (2.0 * eta * a[k - 1] - a[k - 2]) / ((k * (k - 1)) as f64);
        a.push(next);
    }
    let (mut pw, mut sum) = ((1.0, 0.0), (0.0, 0.0));
    for ak in a.iter().skip(1) {
        pw = mul(pw, (re, im));
        sum = (sum.0 + ak * pw.0, sum.1 + ak * pw.1);
    }
    let want = (c0 * sum.0, c0 * sum.1);
    let out = timed(&mut s, "coulomb_f(0, 0.5, 1.5 + 0.5*im)", 3.0);
    let inner = out.strip_prefix("complex(").and_then(|r| r.strip_suffix(')')).expect(&out);
    let (x, y) = inner.split_once(", ").expect(&out);
    let (x, y) = (number(x), number(y));
    assert!((x - want.0).abs() < 1e-14 && (y - want.1).abs() < 1e-14, "{out} vs {want:?}");
    // G solves the same equation: u'' + (1 - 2 eta/rho - l (l + 1)/rho^2) u = 0 (central second difference)
    let res = number(&s.run(
        "let f = z => n(coulomb_g(2, 1/3, z), 30); let r0 = 1 + 2*im; let h = 1/1000; \
         n(abs((f(r0 + h) - 2*f(r0) + f(r0 - h))/h^2 + (1 - 2/(3*r0) - 6/r0^2)*f(r0)), 5)",
    ));
    assert!(res < 1e-5, "{res}");
}

#[test]
fn spheroidal_ps_plot_reuses_the_eigenvalue() {
    let mut s = Sys::new();
    let t = Instant::now();
    let v = s.run("table(spheroidal_ps(2, 0, 1.5, x/100.0), [x, 1, 99])");
    assert!(t.elapsed().as_secs_f64() < 2.0, "{:?}", t.elapsed());
    // the values do not depend on the cache: the same call alone
    let one = s.run("spheroidal_ps(2, 0, 1.5, 50/100.0)");
    assert!(v.contains(&one), "{one}");
}

#[test]
fn zeta_zero_with_a_height_counts_along_the_whole_critical_line() {
    let mut s = Sys::new();
    // the zeros 1/2 +- i t_j ordered by imaginary part; t_1 = 14.1347..., t_2 = 21.0220..., t_3 = 25.0108...
    let at = |s: &mut Sys, src: &str| -> f64 {
        let out = s.run(src);
        let inner = out.strip_prefix("complex(").and_then(|r| r.strip_suffix(')')).expect(&out);
        number(inner.split_once(", ").expect(&out).1)
    };
    for (src, want) in [
        ("zeta_zero(2, 15.)", 25.01085758014569),
        ("zeta_zero(-1, 15.)", 14.134725141734695),
        ("zeta_zero(-2, 15.)", -14.134725141734695),
        ("zeta_zero(-3, 20.)", -21.022039638771556),
        ("zeta_zero(1, -20.)", -14.134725141734695),
        ("zeta_zero(2, -20.)", 14.134725141734695),
        ("zeta_zero(3, -20.)", 21.022039638771556),
        ("zeta_zero(-1, -20.)", -21.022039638771556),
    ] {
        let got = at(&mut s, src);
        assert!((got - want).abs() < 1e-9, "{src}: {got} vs {want}");
    }
}

#[test]
fn inverse_jacobi_functions_invert_at_complex_arguments() {
    let mut s = Sys::new();
    // the forward function of the inverse returns the argument, on every sheet the formulas of the real case may land on
    for f in ["cn", "dn", "cd", "sd", "nd", "dc", "nc", "sc", "ns", "ds", "cs"] {
        for (x, m) in [("0.2 + im", "0.1 - 0.3*im"), ("0.2 - 0.3*im", "0.1 + 0.5*im"), ("0.7", "0.4"), ("1.5 + 0.5*im", "0.8"), ("-0.4 + 0.9*im", "0.3 + 0.2*im")] {
            let r = s.run(&format!("abs(jacobi_{f}(inverse_jacobi_{f}({x}, {m}), {m}) - ({x}))"));
            let err = number(&r);
            assert!(err < 1e-9, "{f}({x}, {m}): {r}");
        }
    }
    // cd(u) = 0.2 + i for u = 1.492 - 1.018 i (2K - the formula's first answer)
    assert!(s.run("inverse_jacobi_cd(0.2 + im, 0.1 - 0.3*im)").contains("1.49215077803159"));
}

#[test]
fn coulomb_f_series_at_complex_rho_agrees_with_the_ball_path() {
    let mut s = Sys::new();
    let parse = |o: &str| -> (f64, f64) {
        let inner = o.strip_prefix("complex(").and_then(|r| r.strip_suffix(')')).unwrap_or_else(|| panic!("{o}"));
        let (a, b) = inner.split_once(", ").unwrap();
        (number(a), number(b))
    };
    // machine arguments (the series in double-double numbers) against 40-digit values of the same exact points
    for (l, eta, re, im) in [("0", "1/2", "2", "-1"), ("3/2", "1/4", "-2", "-2"), ("2", "-2/3", "8", "-3"), ("1/2", "1", "3", "3"), ("0", "3", "1/2", "1/10"), ("11/2", "1", "4", "1/2")] {
        let f = |x: &str| -> f64 { x.parse::<f64>().unwrap_or_else(|_| { let (p, q) = x.split_once('/').unwrap(); p.parse::<f64>().unwrap() / q.parse::<f64>().unwrap() }) };
        let machine = s.run(&format!("coulomb_f({}, {}, {} + ({})*im)", f(l), f(eta), f(re), f(im)));
        let exact = s.run(&format!("n(coulomb_f({l}, {eta}, ({re}) + ({im})*im), 40)"));
        let (m, e) = (parse(&machine), parse(&exact));
        let err = ((m.0 - e.0).powi(2) + (m.1 - e.1).powi(2)).sqrt() / e.0.hypot(e.1);
        assert!(err < 3e-16, "l={l} eta={eta} rho={re}+{im}i: {machine} vs {exact}");
    }
    // a plot's worth of values stays fast: 961 points
    let t = Instant::now();
    s.run("table(abs(coulomb_f(3/2, 1/4, x/10.0 + y/10.0*im)), [x, -30, 30, 2], [y, -30, 30, 2])");
    assert!(t.elapsed().as_secs_f64() < 2.0, "{:?}", t.elapsed());
}

#[test]
fn inverse_jacobi_cd_on_the_real_axis_beyond_the_first_quarter_period() {
    let mut s = Sys::new();
    // cd(u) = -0.5 at u in (K, 2K): 2K - 1.18325... = 2.24452... (the formula's first answer had cd(u) = +0.5), real
    let v = s.run("inverse_jacobi_cd(-0.5, 0.3)");
    assert!(number(&v) > 2.2445 && number(&v) < 2.2446, "{v}");
    assert!(number(&s.run("jacobi_cd(inverse_jacobi_cd(-0.5, 0.3), 0.3)")) + 0.5 < 1e-12);
}
