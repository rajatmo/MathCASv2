//! Limits onto branch cuts of special functions from either side, domains of step functions, asymptotic relations
//! with a parameter, convergence on the boundary circle of a power series, and `clip` with values outside its
//! bounds. Each reported input with variations checked by an independent route (values beside the cut, the series
//! at the boundary point, the definition of the relation at the special parameter value).
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate on a detached thread (one worker thread, fresh session); `TIMEOUT` after 60 s.
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

/// The limit `lim` agrees to 15 digits with the function at 10^-40 beside the point (`beside`).
fn agrees(lim: &str, beside: &str) -> String {
    ev(&format!("abs(n({lim}, 20) - n({beside}, 30)) < 10^-15"))
}

#[test]
fn limits_onto_a_cut_take_the_side_of_approach() {
    assert_eq!(
        ev("[limit(elliptic_e(2 + e1 * im), e1 -> 0, direction: from_below), limit(elliptic_e(2 + e1 * im), e1 -> 0, direction: from_above)]"),
        "[elliptic_e(2), conjugate(elliptic_e(2))]"
    );
    assert_eq!(ev("limit(elliptic_k(2 + e1 * im), e1 -> 0, direction: from_above)"), "conjugate(elliptic_k(2))");
    assert_eq!(agrees("limit(elliptic_k(2 + e1 * im), e1 -> 0, direction: from_above)", "elliptic_k(2 + 10^-40*im)"), "true");
    assert_eq!(agrees("limit(elliptic_e(3 + e1^3 * im), e1 -> 0, direction: from_below)", "elliptic_e(3 - 10^-40*im)"), "true");
    assert_eq!(agrees("limit(bessel_y(0, -2 + e1 * im), e1 -> 0, direction: from_below)", "bessel_y(0, -2 - 10^-40*im)"), "true");
    assert_eq!(agrees("limit(product_log(-1 + e1 * im), e1 -> 0, direction: from_below)", "product_log(-1 - 10^-40*im)"), "true");
    // off the cut, and along the cut itself, the value at the point
    assert_eq!(ev("limit(elliptic_k(1/2 + e1 * im), e1 -> 0, direction: from_above)"), "elliptic_k(1/2)");
    assert_eq!(ev("limit(elliptic_k(x), x -> 2)"), "elliptic_k(2)");
}

#[test]
fn domains_of_step_functions() {
    assert_eq!(ev("function_domain(heaviside_theta(x), x)"), "x < 0 or x > 0");
    assert_eq!(ev("function_domain(heaviside_theta(x), x, complexes)"), "x < 0 or x > 0");
    // heaviside_theta(0) and heaviside_theta(im) stay unevaluated
    assert_eq!(ev("[head(heaviside_theta(0)), head(heaviside_theta(im))]"), "[heaviside_theta, heaviside_theta]");
    assert_eq!(ev("function_domain(round(x, a), [x, a])"), "a != 0");
    assert_eq!(ev("function_domain(floor(x, a) + ceiling(x, 2), [x, a])"), "a != 0");
    assert_eq!(ev("is_failure(round(5/2, 0))"), "true");
    assert_eq!(ev("function_domain(unit_step(z), z, complexes)"), "element(z, reals)");
    assert_eq!(ev("[function_domain(clip(x), x, complexes), function_domain(clip(x, [0, 1]), x)]"), "[element(x, reals), true]");
    assert_eq!(ev("head(clip(im))"), "clip");
}

#[test]
fn asymptotic_relations_with_a_parameter() {
    // sinh(p t)/sin(t) -> p: o-small only where p == 0 (then the left side is 0), O-big of the other way round only
    // where p != 0 (sin is O(0) nowhere); sinh(t/p) is undefined at p == 0
    assert_eq!(ev("asymptotic_less(sinh(p * (x - 2)), sin(x - 2), x -> 2)"), "p == 0");
    assert_eq!(ev("asymptotic_greater_equal(sinh(p * (x - 2)), sin(x - 2), x -> 2)"), "p < 0 or 0 < p");
    assert_eq!(ev("asymptotic_less_equal(sin(x - 2), sinh(p * (x - 2)), x -> 2)"), "p < 0 or 0 < p");
    assert_eq!(ev("asymptotic_greater(sinh((x - 2) / p), sin(x - 2), x -> 2)"), "false");
    assert_eq!(ev("asymptotic_equivalent(sinh(p * x), sin(x), x -> 0)"), "p == 1");
    assert_eq!(ev("asymptotic_less(p*x^2 + x^3, x^2, x -> 0)"), "p == 0");
    // the special values checked directly
    assert_eq!(ev("[asymptotic_less(0, sin(x), x -> 0), asymptotic_less(sinh(x), sin(x), x -> 0), asymptotic_less(x^3, x^2, x -> 0)]"), "[true, false, true]");
}

#[test]
fn convergence_on_the_boundary_circle() {
    assert_eq!(
        ev("sum_convergence((-3)^n_u * (x^(2 * n_u) / n_u), n_u)"),
        "-3*x^2 != 1 and abs(x) <= (1/3)*3^(1/2) or abs(x) < (1/3)*3^(1/2)"
    );
    assert_eq!(ev("sum_convergence(x^n_u / n_u, n_u)"), "x != 1 and abs(x) <= 1 or abs(x) < 1");
    assert_eq!(ev("sum_convergence(x^n_u / n_u^2, n_u)"), "abs(x) <= 1");
    assert_eq!(ev("sum_convergence(x^n_u * n_u, n_u)"), "abs(x) < 1");
    // a pole in the range where the parameter is an integer from the lower end on
    assert_eq!(ev("sum_convergence(1 / (n_u - a)^2, [n_u, 5, inf])"), "not_element(a, integers) or a < 5");
    assert_eq!(ev("[sum_convergence(1 / (n_u - 7)^2, [n_u, 5, inf]), sum_convergence(1 / (n_u - 7/2)^2, [n_u, 5, inf]), sum_convergence(1 / (n_u - 3)^2, [n_u, 5, inf])]"), "[false, true, true]");
    // at the boundary points: the alternating harmonic series converges, the harmonic one does not
    assert_eq!(ev("[sum_convergence((-1)^n_u / n_u, n_u), sum_convergence(1 / n_u, n_u), sum_convergence(1 / n_u^2, n_u)]"), "[true, false, true]");
}

#[test]
fn clip_with_values_outside_its_bounds() {
    assert_eq!(ev("piecewise_expand(clip(x, [0, 4], [0, 2]))"), "piecewise([[2, x > 4], [x, 0 <= x and x <= 4]], 0)");
    assert_eq!(ev("piecewise_expand(clip(x, [-1, 1], [-5, 7]))"), "piecewise([[-5, x < -1], [7, x > 1]], x)");
    assert_eq!(ev("[clip(5, [0, 4], [0, 2]), clip(-1, [0, 4], [-3, 2])]"), "[2, -3]");
    assert_eq!(ev("function_discontinuities(clip(x, [min_u, max_u], [vmin, max_u]), x)"), "x == min_u");
    assert_eq!(ev("function_discontinuities(clip(x, [0, 1], [-1, 2]), x)"), "x == 0 or x == 1");
    assert_eq!(ev("function_discontinuities(clip(x, [0, 1]), x)"), "false");
}
