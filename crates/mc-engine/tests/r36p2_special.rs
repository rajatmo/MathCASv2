//! Round 36: regressions of earlier fixes and further fixed wrong values of special functions. Each value is checked by a route other than the code under test: an identity (DLMF), the closed form at 30
//! digits, a central difference, or the round trip through the forward function.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate every statement in one fresh session (one worker thread, big stack); `TIMEOUT` after `secs` seconds.
fn ev_within(src: &str, secs: u64) -> String {
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
        .expect("spawn");
    rx.recv_timeout(std::time::Duration::from_secs(secs)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

fn ev(src: &str) -> String {
    ev_within(src, 120)
}

/// A printed machine number or complex number `a`, `a + bim`, `a - bim`, `bim` as (re, im).
fn cx(s: &str) -> (f64, f64) {
    let s = s.trim();
    if let Some(body) = s.strip_prefix("complex(").and_then(|b| b.strip_suffix(')')) {
        let (re, im) = body.split_once(',').expect("complex");
        return (re.trim().parse().unwrap_or(f64::NAN), im.trim().parse().unwrap_or(f64::NAN));
    }
    if let Some(body) = s.strip_suffix("im") {
        let chars: Vec<char> = body.chars().collect();
        let mut cut = None;
        for i in (1..chars.len()).rev() {
            if (chars[i] == '+' || chars[i] == '-') && chars[i - 1] != 'e' {
                cut = Some(i);
                break;
            }
        }
        return match cut {
            Some(i) => {
                let re: String = chars[..i].iter().collect();
                let im: String = chars[i..].iter().collect();
                (re.trim().parse().unwrap_or(f64::NAN), im.replace(' ', "").parse().unwrap_or(f64::NAN))
            }
            None => (0.0, body.trim().parse().unwrap_or(f64::NAN)),
        };
    }
    (s.parse().unwrap_or(f64::NAN), 0.0)
}

fn close(a: (f64, f64), b: (f64, f64), tol: f64, what: &str) {
    let scale = 1.0 + b.0.hypot(b.1);
    assert!((a.0 - b.0).hypot(a.1 - b.1) <= tol * scale, "{what}: {a:?} vs {b:?}");
}

/// `|x| < tol` for a ball-valued difference (an exact zero up to its ball radius counts).
fn tiny(src: &str, tol: f64) {
    let out = ev(&format!("abs({src}) < {tol:e}"));
    assert_eq!(out, "true", "{src}");
}

// ---------------------------------------------------------------- group 1: regressions of earlier fixes

#[test]
fn neville_theta_at_complex_machine_arguments_is_numeric() {
    // the four Neville theta functions at complex machine arguments (expected values), and the identities theta_c/theta_n = cn,
    // theta_d/theta_n = dn, theta_s/theta_n = sn (DLMF 22.2.4-6 with Neville's normalization) at 30 digits
    for (f, z, want) in [
        ("c", "2 + im", (57.28714521295863, 9.470737791620527)),
        ("d", "2 + im", (-133.51301063488265, 154.54239439943234)),
        ("n", "2 + im", (86.09089755161946, 67.33795068718084)),
        ("s", "1 + im", (4.84562471533888, 2.9019533289456483)),
    ] {
        let got = ev(&format!("n(neville_theta_{f}({z}, 5 - im))"));
        close(cx(&got), want, 1e-12, &format!("neville_theta_{f}: {got}"));
    }
    for (p, j) in [("c", "cn"), ("d", "dn"), ("s", "sn")] {
        let (z, m) = ("n(2 + im, 30)", "n(5 - im, 30)");
        tiny(&format!("neville_theta_{p}({z}, {m})/neville_theta_n({z}, {m}) - jacobi_{j}({z}, {m})"), 1e-25);
    }
    // a matrix function of theta_d: through the eigenvalues 1.2 and -0.8 of the matrix
    let m = ev("n(matrix_function(&(neville_theta_d(pi * (im / 3), &1)), [[0.2, 1], [1, 0.2]]))");
    assert!(!m.contains("neville"), "{m}");
    let d = |x: &str| cx(&ev(&format!("neville_theta_d(n(pi im/3), {x})")));
    let (a, b) = (d("1.2"), d("-0.8"));
    let first = ev("first(first(n(matrix_function(&(neville_theta_d(pi * (im / 3), &1)), [[0.2, 1], [1, 0.2]]))))");
    close(cx(&first), ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0), 1e-12, "matrix_function [1, 1]");
}

#[test]
fn neville_theta_at_an_inexact_one_and_the_plots_over_m() {
    // theta_d, theta_n, theta_s at m = 1.: the limit (theta_s, theta_c, theta_d, theta_n) = (sinh, 1, 1, cosh), and the
    // plots over m through 1 finish (they timed out)
    close(cx(&ev("neville_theta_s(1, 1.0)")), (1f64.sinh(), 0.0), 1e-15, "theta_s(1 | 1)");
    close(cx(&ev("neville_theta_n(1, 1.0)")), (1f64.cosh(), 0.0), 1e-15, "theta_n(1 | 1)");
    assert_eq!(ev("neville_theta_d(1, 1.0)"), "1.0");
    assert_eq!(ev("elliptic_nome_q(n(1, 40))"), ev("n(1, 40)"));
    for (f, hi) in [("d", 1), ("n", 5), ("s", 5)] {
        let out = ev_within(&format!("head(plot([neville_theta_{f}(1, m), neville_theta_{f}(2, m), neville_theta_{f}(3, m)], [m, -10, {hi}], plot_range: automatic))"), 20);
        assert_eq!(out, "image", "neville_theta_{f} plot");
    }
}

#[test]
fn jacobi_functions_and_inverses_at_an_inexact_parameter_zero_or_one() {
    // the parameter 0. or 1.: sn(u | 0) = sin u, sn(u | 1) = tanh u, cn(u | 1) = sech u (DLMF 22.5.3-4) at a complex u
    close(cx(&ev("jacobi_sn(0.3 + 1.7im, 0.0)")), cx(&ev("sin(0.3 + 1.7im)")), 1e-15, "sn | 0");
    close(cx(&ev("jacobi_cn(0.3 + 1.7im, 0.0)")), cx(&ev("cos(0.3 + 1.7im)")), 1e-15, "cn | 0");
    close(cx(&ev("jacobi_sn(1.0 + 1.0im, 1.0)")), cx(&ev("tanh(1.0 + 1.0im)")), 1e-15, "sn | 1");
    close(cx(&ev("jacobi_dn(1.0 + 1.0im, 1.0)")), cx(&ev("sech(1.0 + 1.0im)")), 1e-15, "dn | 1");
    // the inverses map back
    for (f, x, m) in [("dc", "1/3", "0.0"), ("cs", "0.3", "0.0"), ("cn", "0.4", "0.0"), ("dn", "-1/2", "1.0"), ("ns", "2.5", "1.0"), ("sc", "0.7", "1.0")] {
        let back = ev(&format!("jacobi_{f}(inverse_jacobi_{f}({x}, {m}), {m})"));
        let want = cx(&ev(&format!("n({x})")));
        close(cx(&back), want, 1e-14, &format!("inverse_jacobi_{f}({x}, {m}): {back}"));
    }
    // dn(u | 0) = 1 has no inverse: unevaluated, not elliptic_f(indeterminate)
    assert!(ev("inverse_jacobi_dn(-1/2, 0.0)").starts_with("inverse_jacobi_dn"));
    for (f, v, lo) in [("dc", "1 / 3", "0"), ("dn", "-(1 / 2)", "1 / 2")] {
        let src = format!(
            "derivs = simplify(table(d(inverse_jacobi_{f}(ν, m), [ν, n_u]), n_u, range(1, 3))); head(plot(eval(rewrite(derivs, ν -> {v})), [m, {lo}, 2]))"
        );
        assert_eq!(ev_within(&src, 30), "image", "inverse_jacobi_{f} plot");
    }
}

#[test]
fn modular_lambda_near_the_real_axis_declines_and_the_contour_plot_finishes() {
    // near the real axis, machine points the reduction cannot take stay unevaluated (0.3 + 1e-5 i gave -2.6e-105 from the
    // nome series at |q| = 0.99997 in 35 ms; 2.1 + 1e-300 i gave a finite value of no meaning)
    assert!(ev("modular_lambda(0.3 + 1.0e-5 im)").starts_with("modular_lambda"));
    assert!(ev("modular_lambda(2.1 + 1.0e-300 im)").starts_with("modular_lambda"));
    // a point the reduction takes: lambda(tau) + lambda(-1/tau) = 1 (DLMF 23.15.6 with lambda = k^2)
    let s = cx(&ev("modular_lambda(0.3 + 0.5 im) + modular_lambda(-1/(0.3 + 0.5 im))"));
    close(s, (1.0, 0.0), 1e-13, "lambda(tau) + lambda(-1/tau)");
    let out = ev_within("head(complex_contour_plot(re(modular_lambda(z)), [z, -4 - 0.1 * im, 4 + 3 * im], contours: 20))", 20);
    assert_eq!(out, "image");
}

#[test]
fn regularized_2f1_at_a_pole_parameter_is_correctly_rounded_and_real() {
    // the regularized 2F1 at a pole parameter: 2F1~(1, 2; -3; z) = 120 z^4 (1 - z)^-6 (DLMF 15.2.3_5 and 15.4.6) = 3149280/117649
    assert_eq!(ev("hypergeometric_2f1_regularized(1, 2, -3, 4.5)"), ev("n(3149280/117649)"));
    // F(a, b; a; z) = (1 - z)^-b on the cut: real for an integer b, no imaginary noise in balls
    assert_eq!(ev("hypergeometric_2f1(5, 6, 5, 4.5)"), ev("n((-7/2)^-6)"));
    let b = ev("hypergeometric_2f1(5, 6, 5, 4.5~30)");
    assert!(!b.contains("im"), "{b}");
    tiny("hypergeometric_2f1(5, 1/3, 5, 4.5~30) - (1 - 4.5~30)^(-1/3)", 1e-28);
}

#[test]
fn around_through_a_function_with_parameter_lists_is_first_order() {
    // a Meijer G function of an around value: G = x^(9/2) J_1(2 sqrt x), the radius |G'(0.2)| 0.01 with G' from a central difference at 30 digits
    let out = ev("meijer_g([[], []], [[5], [4]], around(0.2, 0.01))");
    let r = out.trim_end_matches(')').rsplit(", ").next().unwrap_or("").parse::<f64>().unwrap_or(f64::NAN);
    let g = "(x0 => meijer_g([[], []], [[5], [4]], x0))";
    let slope = cx(&ev(&format!("n(({g}(1/5 + 1/10^8) - {g}(1/5 - 1/10^8))/(2/10^8), 20)"))).0;
    assert!((r - slope.abs() * 0.01).abs() <= 1e-12, "{out} vs {slope}");
}

#[test]
fn complex_balls_take_constants_into_the_ball() {
    // (2 + i)~40 pi was a product: the constant joins at the ball's precision, as for a real ball
    let out = ev("y = n(2 + im, 40); y*pi");
    assert!(!out.contains("pi"), "{out}");
    tiny("n(2 + im, 40)*pi - n((2 + im)*pi, 45)", 1e-38);
}

// ---------------------------------------------------------------- group 2: further fixed values

#[test]
fn bessel_zero_index_derivative_uses_the_modulus() {
    // the zeros of J and Y as functions of their index: d j_(nu, k)/dk = pi^2 j M^2/2 with M^2 = J^2 + Y^2 (DLMF 10.18.8, 10.21(iii)); between
    // integers J(j) is not 0 (the row with Y^2 alone gave 1e-30 at k = 7/2)
    for f in ["bessel_j_zero", "bessel_y_zero"] {
        let exact = cx(&ev(&format!("n(rewrite(d({f}(2, k), k), k -> 7/2), 20)")));
        let diff = cx(&ev(&format!("n(({f}(2, 7/2 + 1/10^6) - {f}(2, 7/2 - 1/10^6))/(2/10^6), 20)")));
        close(exact, diff, 1e-9, f);
    }
    assert_eq!(ev("bessel_j(2, bessel_j_zero(2, 3))"), "0");
    assert!(ev("bessel_j(2, bessel_j_zero(2, 7/2))").starts_with("bessel_j"));
}

#[test]
fn function_analytic_at_pole_parameters() {
    // 0F1(-1; z) is defined nowhere; j_(-2) = y_1 has a pole at 0 (DLMF 10.47.10)
    assert_eq!(ev("function_analytic(hypergeometric_0f1(-1, z), z)"), "false");
    assert_eq!(
        ev("table(function_analytic(spherical_bessel_j(n_u, z), z), n_u, [-2, -(1 / 2), sqrt(2), pi])"),
        "[false, false, false, false]"
    );
    assert_eq!(ev("function_analytic(hypergeometric_0f1(2, z), z)"), "true");
}

#[test]
fn elliptic_log_derivative_in_the_point() {
    // d/dx elliptic_log([x, y], [a, b]) = 1/(2 y), against a central difference along the curve
    assert_eq!(ev("simplify(d(elliptic_log([x, y], [a, b]), x) - 1/(2 y))"), "0");
    let diff = cx(&ev(
        "e = (t => elliptic_log([t, sqrt(t^3 + 2 t^2 + 3 t)], [2, 3])); n((e(13/10 + 1/10^8) - e(13/10 - 1/10^8))/(2/10^8), 20)",
    ));
    close(diff, (1.0 / (2.0 * (1.3f64.powi(3) + 2.0 * 1.69 + 3.9).sqrt()), 0.0), 1e-9, "elliptic_log derivative");
}

#[test]
fn difference_root_below_the_initial_values() {
    // y(n + 2) = y(n + 1) - y(n), y(0) = 1, y(1) = 1/3 run backward: y(-5) = 1/3 (it returned the equations)
    let f = "difference_root(((y, n_u) => [y(n_u + 2) == y(n_u + 1) - y(n_u), y(0) == 1, y(1) == 1 / 3]))";
    assert_eq!(ev(&format!("{f}(-5)")), "1/3");
    // the values from -6 to 6 satisfy the recurrence and the initial values
    assert_eq!(
        ev(&format!("v = table({f}(k), [k, -6, 6]); table(v[j + 2] - v[j + 1] + v[j], [j, 1, 11])")),
        "[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]"
    );
    assert_eq!(ev(&format!("[{f}(0), {f}(1)]")), "[1, 1/3]");
    // Fibonacci backward: F(-7) = 13 = (-1)^8 F(7)
    assert_eq!(ev("difference_root(((y, n) => [y(n + 2) == y(n + 1) + y(n), y(0) == 0, y(1) == 1]))(-7)"), "13");
    // initial values at -1 and 1 of a recurrence singular at n = -1 start nothing: unevaluated
    let out = ev("difference_root(((y, n_u) => [y(n_u + 1) / (n_u + 1) - y(n_u) == 0, y(-1) == 1, y(1) == 1]))(3)");
    assert!(out.starts_with("difference_root"), "{out}");
}

#[test]
fn find_root_at_a_multiple_root_and_without_a_root() {
    // sin(t) - t has a triple root at t = 0; the machine residual rounds to 0 at t = 1.6e-5, Newton
    // in balls continues to |t| < 1e-9
    for opts in ["", ", accuracy_goal: inf, precision_goal: 8"] {
        let t = cx(&ev(&format!("rewrite(10 - x, find_root(sin(x - 10) - x + 10, [x, 0]{opts}))"))).0;
        assert!(t.abs() < 1e-9, "{opts}: {t}");
    }
    // the ball refinement of a multiple root: (x - 2)^3 to 30 digits (it stopped after one step without shrinking)
    assert_eq!(ev("abs(rewrite(x, find_root((x - 2)^3, [x, 0], digits: 30)) - 2) < 1e-20"), "true");
    // exp(1/(x^2 - 1)) > 0 has no zero, the residual only underflows: not converged (was a root at 0.9987)
    assert!(ev("find_root(exp(1 / (x^2 - 1)), [x, 0.1], max_iterations: 1000)").starts_with("failure(not_converged"));
    assert_eq!(ev("find_root(x^2 - 2, [x, 1])"), "[x -> 1.4142135623730951]");
}

#[test]
fn theta_near_q_one_by_the_imaginary_transformation() {
    // theta_3(pi/2, e^(-pi/1000)) = theta_4(0, q) = sqrt(1000) theta_2(0, e^(-1000 pi)) = 2 sqrt(1000)
    // e^(-250 pi) (1 + e^(-2000 pi) + ...) (DLMF 20.7.33) to 40 digits; it was a ball around 0
    tiny("n(elliptic_theta(3, pi / 2, exp(-pi / 10^3)), 40)/n(2 sqrt(1000) exp(-250 pi) (1 + exp(-2000 pi)), 40) - 1", 1e-34);
    // nomes in (e^-pi, 1) against the defining series
    tiny("elliptic_theta(4, n(3/10, 30), n(9/10, 30)) - (1 + 2 sum((-1)^k (9/10)^(k^2) cos(2 k 3/10), [k, 1, 400]))", 1e-25);
    tiny("elliptic_theta(1, n(3/10, 30), n(1/2, 30)) - 2 (1/2)^(1/4) sum((-1)^k (1/2)^(k (k + 1)) sin((2 k + 1) 3/10), [k, 0, 60])", 1e-25);
}

#[test]
fn riemann_siegel_z_series_at_zero() {
    // Z is even; the x^2 coefficient (zeta', zeta'' at 1/2 by differences at 60 digits) against a second
    // difference of Z itself
    let s = ev("normal(series(riemann_siegel_z(x), x, 0, 2))");
    assert!(!s.contains("riemann_siegel_z"), "{s}");
    let c2 = cx(&ev("h = 1/10^12; z1 = (zeta(1/2 + h) - zeta(1/2 - h))/(2 h); z2 = (zeta(1/2 + h) - 2 zeta(1/2) + zeta(1/2 - h))/h^2; c = coefficient(normal(series(riemann_siegel_z(x), x, 0, 2)), x, 2); n(rewrite(c, [derivative(2)(zeta)(1/2) -> z2, derivative(1)(zeta)(1/2) -> z1]), 20)")).0;
    let c1 = ev("coefficient(normal(series(riemann_siegel_z(x), x, 0, 2)), x, 1)");
    let zz = cx(&ev("h = 1/10^12; n((riemann_siegel_z(h) - 2 riemann_siegel_z(0) + riemann_siegel_z(-h))/(2 h^2), 60)")).0;
    assert!((c2 - zz).abs() < 1e-12 && (zz - 2.735884869827086).abs() < 1e-12, "{c2} vs {zz}");
    assert_eq!(c1, "0");
}

#[test]
fn bilateral_series_derivative_and_its_value_at_zero() {
    // z H' = a1 (H(a1 + 1) - H), and (b1 - 1)(H(b1 - 1) - H) without upper parameters,
    // against a central difference of the function itself
    let d1 = cx(&ev("rewrite(d(bilateral_hypergeometric_pfq([1 / 2, 3 / 4], [1 / 4, 1 / 3], z), z), z -> 0.3 + 0.2im)"));
    let f1 = cx(&ev(
        "h = (t => bilateral_hypergeometric_pfq([1/2, 3/4], [1/4, 1/3], t)); (h(0.3 + 0.2im + 1e-6) - h(0.3 + 0.2im - 1e-6))/2e-6",
    ));
    close(d1, f1, 1e-8, "bilateral 2/2");
    let d2 = cx(&ev("n(rewrite(d(bilateral_hypergeometric_pfq([], [3, 5/2], z), z), z -> 7/10), 20)"));
    let f2 = cx(&ev("h = (t => bilateral_hypergeometric_pfq([], [3, 5/2], t)); n((h(7/10 + 1/10^8) - h(7/10 - 1/10^8))/(2/10^8), 20)"));
    close(d2, f2, 1e-9, "bilateral 0/2");
    // no power series at 0 (terms in z^-k)
    assert!(ev("series(bilateral_hypergeometric_pfq([1 / 2, 3 / 4], [1 / 4, 1 / 3], z), z, 0, 1)").starts_with("series("));
}

#[test]
fn unsupported_inputs_decline() {
    // find_root: options that are not supported answer a failure naming them
    assert!(ev("find_root(sin(x) - 1, [x, 1.5], step_monitor: 1)").starts_with("failure(unknown_option, step_monitor"));
    assert!(ev("find_root(sin(x) - 1, [x, 1.5], damping_factor: 2)").starts_with("failure(unknown_option, damping_factor"));
    // function_range of exp_integral_e is unevaluated and reduce declines
    assert!(ev("reduce(function_range(exp_integral_e(2, x), x, y), y, reals)").starts_with("failure("));
    // differential_root: complex initial values are refused by n_dsolve_value
    let out = ev(r"differential_root(((y, x) => [x * y''(x) + im * y'(x) - y(x) == 0, y(1 / 2) == 1 + 4 * im, y'(1 / 2) == -3 / 4]))(0.2)");
    assert!(out.contains("not real"), "{out}");
}
