//! The package-3C functions through an engine session: exact values and poles, symbolic arguments, machine and
//! complex machine values, `n` at 30-40 digits against reference digits, nested `n`, list threading, the
//! generalized hypergeometric native, derivatives checked by central differences, 1 vs 4 threads, interrupts.
mod common;
use common::{check, number, Sys};
use std::time::Duration;

#[test]
fn exact_values_and_poles() {
    check(&[
        ("erf(0)", "0"),
        ("erfc(0)", "1"),
        ("inverse_erf(0)", "0"),
        ("inverse_erf(1)", "inf"),
        ("inverse_erfc(1)", "0"),
        ("gamma(3, x)", "(2 + 2*x + x^2)*exp(-x)"),
        ("gamma(1/2, x)", "erfc(x^(1/2))*pi^(1/2)"),
        ("gamma(5/2, 0)", "(3/4)*pi^(1/2)"),
        ("gamma(-2, 0)", "failure(pole, gamma(-2, 0))"),
        ("gamma_regularized(1, x)", "exp(-x)"),
        ("beta(1, 2, 3)", "1/12"),
        ("beta(0, a, b)", "0"),
        ("beta_regularized(1, a, b)", "1"),
        ("exp_integral_ei(0)", "-inf"),
        ("exp_integral_e(0, x)", "exp(-x)/x"),
        ("exp_integral_e(3, 0)", "1/2"),
        ("log_integral(0)", "0"),
        ("log_integral(1)", "-inf"),
        ("sin_integral(0)", "0"),
        ("cos_integral(0)", "-inf"),
        ("polylog(2, 1/2)", "(-1/2)*log(2)^2 + (1/12)*pi^2"),
        ("polylog(2, 1)", "(1/6)*pi^2"),
        ("polylog(3, 1)", "zeta(3)"),
        ("polylog(1, x)", "-log(1 - x)"),
        ("polylog(0, x)", "x/(1 - x)"),
        ("polylog(2, -1)", "(-1/12)*pi^2"),
        ("hypergeometric_2f1(1/2, 1/2, 2, 1)", "4/pi"),
        ("hypergeometric_1f1(a, a, x)", "exp(x)"),
        ("hypergeometric_pfq([], [b], 0)", "1"),
        ("hypergeometric_pfq([1], [2], x)", "hypergeometric_1f1(1, 2, x)"),
        ("bessel_j(0, 0)", "1"),
        ("bessel_j(1/2, x)", "2^(1/2)*(pi*x)^(-1/2)*sin(x)"),
        ("bessel_y(1, 0)", "failure(pole, bessel_y(1, 0))"),
        ("airy_ai(0)", "(1/3)*3^(1/3)/gamma(2/3)"),
        ("elliptic_k(0)", "(1/2)*pi"),
        ("elliptic_k(1)", "failure(pole, elliptic_k(1))"),
        ("elliptic_e(1)", "1"),
        ("elliptic_f(x, 0)", "x"),
        ("elliptic_pi(0, m)", "elliptic_k(m)"),
        ("jacobi_sn(u, 0)", "sin(u)"),
        ("jacobi_cn(u, 1)", "cosh(u)^-1"),
        ("jacobi_dn(0, m)", "1"),
        // no closed form: the exact call stays for n
        ("erf(1/2)", "erf(1/2)"),
        ("polylog(5/2, 1/3)", "polylog(5/2, 1/3)"),
        ("bessel_j(1, x)", "bessel_j(1, x)"),
        // wrong arities stay
        ("erf(1, 2)", "erf(2) - erf(1)"),
        ("elliptic_pi(1, 2, 3, 4)", "elliptic_pi(1, 2, 3, 4)"),
    ]);
}

#[test]
fn machine_values() {
    let mut s = Sys::new();
    let cases: &[(&str, f64)] = &[
        ("erf(0.5)", 0.5204998778130465),
        ("erfc(10.0)", 2.088487583762545e-45),
        ("inverse_erf(0.5)", 0.4769362762044699),
        ("gamma(2.5, 1.5)", 0.9305194427867924),
        ("gamma_regularized(2.5, 1.5)", 0.6999858358786275),
        ("beta(0.3, 2, 3)", 0.029025),
        ("exp_integral_ei(1.0)", 1.8951178163559368),
        ("log_integral(2.0)", 1.045163780117493),
        ("sin_integral(1.0)", 0.946083070367183),
        ("cos_integral(1.0)", 0.33740392290096816),
        ("polylog(2, 0.5)", 0.5822405264650125),
        ("hypergeometric_2f1(1, 1, 2, 0.5)", 1.3862943611198906),
        ("hypergeometric_pfq([1, 2, 3], [4, 5], 0.5)", 1.189874754256423),
        ("bessel_j(0, 1.0)", 0.7651976865579666),
        ("bessel_y(0, 1.0)", 0.08825696421567696),
        ("airy_ai(1.0)", 0.13529241631288141),
        ("elliptic_k(0.5)", 1.8540746773013719),
        ("elliptic_e(1.0, 0.5)", 0.92732988362444),
        ("jacobi_sn(1.0, 0.5)", 0.8030018248956439),
    ];
    for (src, want) in cases {
        let got = number(&s.run(src));
        assert!(((got - want) / want).abs() < 4e-16, "{src}: {got} vs {want}");
    }
    // complex machine arguments: erf, erfc, erfi; polylog of integer order (also on the cut z > 1)
    s.run("z = 0.5 + 0.5*sqrt(-1)");
    assert_eq!(s.run("erf(z)"), "complex(0.6426129148548205, 0.4578813944351922)");
    assert_eq!(s.run("erfc(z)"), "complex(0.3573870851451795, -0.4578813944351922)");
    assert_eq!(s.run("erfi(z)"), "complex(0.4578813944351922, 0.6426129148548205)");
    assert_eq!(s.run("polylog(2, 2.0)"), "complex(2.4674011002723395, -2.177586090303602)");
    // complex-valued real points stay unevaluated
    assert_eq!(s.run("bessel_y(0, -1.0)"), "bessel_y(0, -1.0)");
    assert_eq!(s.run("elliptic_k(2.0)"), "elliptic_k(2.0)");
    assert_eq!(s.run("polylog(5/2, z)"), "polylog(5/2, complex(0.5, 0.5))");
    // lists thread
    assert_eq!(s.run("erf([0.1, 0.2])"), "[0.1124629160182849, 0.22270258921047847]");
    assert_eq!(s.run("hypergeometric_pfq([1], [2], [0, x])"), "[1, hypergeometric_1f1(1, 2, x)]");
}

/// `n(src, d)` agrees with the reference digits in its first `k` characters.
fn agrees(s: &mut Sys, src: &str, reference: &str, k: usize) {
    let got = s.run(src);
    assert_eq!(&got[..k], &reference[..k], "{src}: {got}");
}

#[test]
fn n_against_references() {
    let mut s = Sys::new();
    let cases: &[(&str, &str)] = &[
        ("n(erf(1/2), 30)", "0.520499877813046537682746653891964"),
        ("n(inverse_erf(1/3), 25)", "0.30457019417398562519729068698"),
        ("n(gamma(1/3, 2), 30)", "0.068136414441459136165995924909970"),
        ("n(gamma_regularized(5/2, 3/2), 30)", "0.699985835878627509099801551624668"),
        ("n(beta_regularized(3/10, 5/2, 3), 30)", "0.241237553921181600095704128712328"),
        ("n(beta(1/4, 2, 3/2), 30)", "0.0285096806259460388066427947096092"),
        ("n(erfi(2), 30)", "18.5648024145755525987042919132409"),
        ("n(inverse_erfc(10^-20), 30)", "6.60158062235514256151639163241870"),
        ("n(cos_integral(1), 40)", "0.33740392290096813466264620388915076999757803"),
        ("n(exp_integral_e(5/2, 3), 30)", "0.00971815524378401623705062211679011"),
        ("n(log_integral(10), 30)", "6.16559950478729793752298175266952"),
        ("n(sin_integral(20), 30)", "1.54824170104343984016364334212951"),
        ("n(polylog(3, 1/3), 30)", "0.348827861154840084214251935386992"),
        ("n(hypergeometric_pfq([1, 2, 3], [4, 5], 1/2), 30)", "1.18987475425642293182568311809197"),
        ("n(hypergeometric_2f1(1/3, 2/3, 5/2, -3), 30)", "0.848365585009581696696099448893200"),
        ("n(hypergeometric_1f1(-3/2, 5/2, -7), 30)", "6.92504640940595309759079321527444"),
        ("n(bessel_j(1, 10), 30)", "0.0434727461688614366697487680268252"),
        ("n(bessel_y(5/2, 3), 30)", "-0.369040730073797897345226869879039"),
        ("n(airy_bi(1), 30)", "1.20742359495287125943637881702828"),
        ("n(elliptic_k(1/2), 40)", "1.85407467730137191843385034719526004621759882"),
        ("n(elliptic_pi(1/3, 1/2), 30)", "2.31079499075428181544231468427818"),
        ("n(elliptic_e(1, 1/2), 30)", "0.927329883624440066965904164960760"),
        ("n(jacobi_dn(2, 1/3), 30)", "0.825998304800579469340248451564081"),
        // nested, mixed with other functions
        ("n(erf(1/3) + gamma(1/2, 2)*polylog(2, 1/3), 20)", "0.3921821533226521231582"),
    ];
    for (src, reference) in cases {
        // the requested digits plus the leading "0." or sign
        let d: usize = src.rsplit(", ").next().unwrap().trim_end_matches(')').parse().unwrap();
        agrees(&mut s, src, reference, d);
    }
    // comparisons refine through the library functions
    assert_eq!(s.run("erf(1/3) > 1/3"), "true");
    assert_eq!(s.run("elliptic_k(1/2) < 2"), "true");
}

/// The derivative of `f` at x by d(), against a central difference at 60 digits.
fn derivative_matches(s: &mut Sys, f: &str, x: &str) {
    let h = "10^-25";
    let exact = s.run(&format!("n(d({f}, x) /. x -> {x}, 30)"));
    let diff = s.run(&format!("n(((({f}) /. x -> {x} + {h}) - (({f}) /. x -> {x} - {h})) / (2*{h}), 60)"));
    let (a, b) = (number(&exact), number(&diff));
    assert!(((a - b) / b).abs() < 1e-14, "d({f}) at {x}: {exact} vs {diff}");
}

#[test]
fn derivatives_by_differences() {
    let mut s = Sys::new();
    for (f, x) in [
        ("erf(x)", "1/3"),
        ("erfc(x)", "2"),
        ("erfi(x)", "1/2"),
        ("inverse_erf(x)", "1/3"),
        ("inverse_erfc(x)", "1/5"),
        ("gamma(5/2, x)", "3/2"),
        ("gamma_regularized(7/3, x)", "2"),
        ("beta(x, 5/2, 3/2)", "1/3"),
        ("beta_regularized(x, 2, 7/2)", "2/5"),
        ("exp_integral_ei(x)", "3/2"),
        ("exp_integral_e(5/2, x)", "3/4"),
        ("log_integral(x)", "3"),
        ("sin_integral(x)", "2"),
        ("cos_integral(x)", "2"),
        ("sinh_integral(x)", "1/2"),
        ("cosh_integral(x)", "1/2"),
        ("polylog(3, x)", "1/3"),
        ("hypergeometric_2f1(1/3, 3/2, 5/2, x)", "1/4"),
        ("hypergeometric_1f1(1/3, 5/2, x)", "-2"),
        ("hypergeometric_0f1(5/3, x)", "3"),
        ("bessel_j(1/3, x)", "5/2"),
        ("bessel_y(2, x)", "3/2"),
        ("airy_ai(x)", "1/2"),
        ("airy_bi_prime(x)", "-2"),
        ("elliptic_k(x)", "1/3"),
        ("elliptic_e(x)", "1/3"),
        ("elliptic_f(1/2, x)", "1/3"),
        ("elliptic_e(x, 1/3)", "1/2"),
        ("elliptic_pi(1/4, x, 1/3)", "1/2"),
        ("jacobi_sn(x, 1/3)", "1/2"),
        ("jacobi_cn(x, 1/3)", "1/2"),
        ("jacobi_dn(x, 1/3)", "1/2"),
        ("inverse_jacobi_sn(x, 1/3)", "1/2"),
    ] {
        derivative_matches(&mut s, f, x);
    }
    // parameter derivatives without a closed form stay operators
    assert_eq!(s.run("d(polylog(s, 1/2), s)"), "derivative(1, 0)(polylog)(s, 1/2)");
}

#[test]
fn threads_agree() {
    let src = "n([erf(1/7), gamma(1/3, 5), polylog(5/2, -3), bessel_y(1/3, 9), jacobi_sn(7, 4/5)], 40)";
    let one = Sys::with_threads(1).run(src);
    let four = Sys::with_threads(4).run(src);
    assert_eq!(one, four);
}

#[test]
fn interrupts_stop_long_evaluations() {
    let mut s = Sys::new();
    let (r, took) = s.interrupted("n(polylog(5/2, 3/4), 20000)", Duration::from_millis(150));
    let printed = r.map(|v| v.to_string()).unwrap_or_else(|e| e.to_string());
    assert!(took < Duration::from_secs(10), "took {took:?}: {printed}");
}
