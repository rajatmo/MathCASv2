//! Round 39: integrals and transforms. Bessel functions of odd order, Mellin transforms of functions cut to an
//! interval, radial Fourier transforms in the plane, the inverse Laplace transform of a completed square under a
//! root; extrema with poles, upper and lower limits of functions homogeneous of degree 0; arithmetic on balls of very
//! different magnitudes. Each value is checked by quadrature, by a recurrence, or at a sample point.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn ev(src: &str) -> String {
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
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(120)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

/// `|a - b| < tol` for two numeric expressions.
fn close(a: &str, b: &str, tol: f64) {
    let v = ev(&format!("abs(n({a}) - n({b})) < {tol}"));
    assert_eq!(v, "true", "{a} vs {b}: {} vs {}", ev(&format!("n({a})")), ev(&format!("n({b})")));
}

#[test]
fn integrals_of_bessel_functions_of_odd_order() {
    assert_eq!(ev("integrate(bessel_j(3, y), y) === -bessel_j(2, y) - 2*bessel_j(1, y)/y"), "true");
    // the derivative of the antiderivative at a point, against the function there
    close("rewrite(d(integrate(bessel_j(5, y), y), y), y -> 17/10)", "bessel_j(5, 17/10)", 1e-12);
    close("rewrite(d(integrate(bessel_j(1, 3*y + 1), y), y), y -> 2/3)", "bessel_j(1, 3)", 1e-12);
    close("integrate(bessel_j(3, y) / (x + 1), [x, 0, 5], [y, 0, 5])", "-(-1 + bessel_j(2, 5) + (2/5)*bessel_j(1, 5))*log(6)", 1e-12);
    close("integrate(bessel_j(3, y), [y, 0, 5])", "n_integrate(bessel_j(3, y), [y, 0, 5])", 1e-10);
    close("integrate(bessel_j(5, y), [y, 1, 7])", "n_integrate(bessel_j(5, y), [y, 1, 7])", 1e-10);
    // the integral over the half line is 1 for every order; J(1, 2 y) gives 1/2
    assert_eq!(ev("integrate(bessel_j(1, 2*y), [y, 0, inf])"), "1/2");
    assert_eq!(ev("integrate(bessel_j(3, y), [y, 0, inf])"), "1");
}

#[test]
fn mellin_transforms_of_cut_functions() {
    assert_eq!(ev("mellin_transform(unit_step(x - a) / e^x, x, s, assumptions: a > 0)"), "gamma(s, a)");
    assert_eq!(ev("mellin_transform(unit_step(x - a) / e^x, x, s)"), "piecewise([[gamma(s), a <= 0]], gamma(s, a))");
    assert_eq!(ev("mellin_transform(piecewise([[e^(-x^2), 1 < x and x < 2]]), x, s)"), "(1/2)*(gamma((1/2)*s, 1) - gamma((1/2)*s, 4))");
    close("rewrite(mellin_transform(piecewise([[e^(-x^2), 1 < x and x < 2]]), x, s), s -> 3/2)", "n_integrate(x^(1/2)*exp(-x^2), [x, 1, 2])", 1e-10);
    close("rewrite(mellin_transform(unit_step(x - 2)*x*exp(-3*x), x, s), s -> 5/2)", "n_integrate(x^(5/2)*exp(-3*x), [x, 2, inf])", 1e-10);
    close("rewrite(mellin_transform(piecewise([[x^2, 1/2 < x and x < 3]]), x, s), s -> 1/3)", "n_integrate(x^(4/3), [x, 1/2, 3])", 1e-10);
}

#[test]
fn radial_fourier_transforms_in_the_plane() {
    assert_eq!(ev("fourier_transform(1 / sqrt(x^2 + y^2), [x, y], [u, v])"), "(u^2 + v^2)^(-1/2)");
    assert_eq!(ev("fourier_cos_transform(1 / sqrt(x^2 + y^2), [x, y], [u, v])"), "(u^2 + v^2)^(-1/2)");
    assert_eq!(ev("inverse_fourier_cos_transform(1 / sqrt(u^2 + v^2), [u, v], [x, y])"), "(x^2 + y^2)^(-1/2)");
    // the Gaussian transforms coordinate by coordinate; the radial route agrees
    assert_eq!(ev("simplify(fourier_transform(exp(-(x^2 + y^2)), [x, y], [u, v]) - exp(-(u^2 + v^2)/4)/2)"), "0");
    // (1 + r^2)^(-3/2) has the transform exp(-rho), checked at rho = 1 by the Hankel integral
    assert_eq!(ev("fourier_transform(1/(1 + x^2 + y^2)^(3/2), [x, y], [u, v])"), "exp(-(u^2 + v^2)^(1/2))");
    close("exp(-1)", "n_integrate(bessel_j(0, r)*r/(1 + r^2)^(3/2), [r, 0, inf])", 1e-6);
}

#[test]
fn inverse_laplace_of_a_completed_square() {
    assert_eq!(ev("inverse_laplace_transform(1 / sqrt(s^2 + b * s + a^2), s, t)"), "bessel_j(0, t*((-1/4)*b^2 + a^2)^(1/2))*exp((-1/2)*b*t)");
    // b = 2, a^2 = 5: exp(-t) J0(2 t), the transform of which is 1/sqrt((s + 1)^2 + 4)
    assert_eq!(ev("simplify(rewrite(inverse_laplace_transform(1 / sqrt(s^2 + b * s + a^2), s, t), [b -> 2, a -> 5^(1/2)]) - inverse_laplace_transform(1/sqrt((s + 1)^2 + 4), s, t))"), "0");
    close("laplace_transform(bessel_j(0, 2*t)*exp(-t), t, 3/2)", "1/sqrt((3/2)^2 + 2*(3/2) + 5)", 1e-12);
}

#[test]
fn extrema_with_poles() {
    // t/(1 + t) is unbounded both ways at -1 (it was taken as bounded by 1)
    assert_eq!(ev("max_value(t/(1 + t), t)"), "inf");
    assert_eq!(ev("min_value(t/(1 + t), t)"), "-inf");
    assert_eq!(ev("min_value(1/cos(x)^2, x)"), "1");
    assert_eq!(ev("minimize(x^2 + 1/x^2, x)"), "[2, [x -> -1]]");
    assert_eq!(ev("min_value(exp(-1/x^2)/x^2 + 1, x)"), "1");
}

#[test]
fn limits_superior_of_functions_of_the_direction() {
    assert_eq!(ev("max_limit(y / (x + y), [x, y] -> [0, 0])"), "inf");
    assert_eq!(ev("min_limit(y / (x + y), [x, y] -> [0, 0])"), "-inf");
    // x y/(x^2 + y^2) = sin(2 theta)/2
    assert_eq!(ev("max_limit(x*y/(x^2 + y^2), [x, y] -> [0, 0])"), "1/2");
    assert_eq!(ev("min_limit(x*y/(x^2 + y^2), [x, y] -> [0, 0])"), "-1/2");
    assert_eq!(ev("max_limit((x - 1)^2/((x - 1)^2 + (y - 2)^2), [x, y] -> [1, 2])"), "1");
    // not homogeneous: unevaluated
    assert_eq!(ev("head(max_limit(x*y/(x^2 + y), [x, y] -> [0, 0]))"), "max_limit");
}

#[test]
fn balls_of_very_different_magnitudes() {
    // 1 + exp(-10^12) at 30 digits: the tiny term joins the radius; comparisons decide without exact alignment
    assert_eq!(ev("n(1 + exp(-10^12), 30) > 0"), "true");
    assert_eq!(ev("n(exp(-10^12), 30) < n(exp(-10^11), 30)"), "true");
    assert_eq!(ev("n(1 + exp(-10^12), 30) < 2"), "true");
    assert_eq!(ev("limit(exp(-1/x^2) + 1, x -> 0)"), "1");
    assert_eq!(ev("limit(exp(-1/x^4) - 1, x -> 0)"), "-1");
}

#[test]
fn fundamental_solutions_of_heat_and_wave_operators() {
    assert_eq!(
        ev("green_function(d(u(x, t), t) - d(u(x, t), [x, 2]), u(x, t), [x, -inf, inf], t, [y, s]) === heaviside_theta(t - s)*exp(-(x - y)^2/(4*(t - s)))/(2*pi^(1/2)*(t - s)^(1/2))"),
        "true"
    );
    assert_eq!(
        ev("green_function([d(u(x, t), [t, 2]) - d(u(x, t), [x, 2])], u(x, t), [x, -inf, inf], t, [y, s]) === heaviside_theta(t - s - abs(x - y))/2"),
        "true"
    );
    // away from t = s the heat kernel solves the equation: G_t - k G_xx = 0 for t > s (k = 3)
    assert_eq!(ev("G = green_function(d(u(x, t), t) - 3*d(u(x, t), [x, 2]), u(x, t), [x, -inf, inf], t, [y, s]); simplify(rewrite(d(G, t) - 3*d(G, [x, 2]), dirac_delta(t - s) -> 0))"), "0");
    // its integral over x is 1 for t > s (the unit mass spreads out), at t - s = 1/2
    close("n_integrate(rewrite(green_function(2*d(u(x, t), t) - 6*d(u(x, t), [x, 2]), u(x, t), [x, -inf, inf], t, [y, s]), [y -> 0, s -> 0, t -> 1/2]), [x, -inf, inf])", "1/2", 1e-8);
    // the wave speed c = 2: (1/(2 a c)) heaviside_theta(t - s - abs(x - y)/c)
    assert_eq!(ev("green_function(d(u(x, t), [t, 2]) - 4*d(u(x, t), [x, 2]), u(x, t), [x, -inf, inf], t, [y, s]) === heaviside_theta(t - s - abs(x - y)/2)/4"), "true");
}

#[test]
fn series_of_roots_of_polynomials() {
    assert_eq!(
        ev("normal(series(root(&(&1^5 + a * &1 + 1), 1), a, 0, 10)) === -1 + (1/5)*a + (1/25)*a^2 + (1/125)*a^3 - (21/15625)*a^5 - (78/78125)*a^6 - (187/390625)*a^7 - (286/1953125)*a^8 + (9367/244140625)*a^10"),
        "true"
    );
    // against the root at a = 1/10, and sqrt(2 + a) by the binomial series
    close("rewrite(normal(series(root(&(&1^5 + a * &1 + 1), 1), a, 0, 10)), a -> 1/10)", "root(&(&1^5 + (1/10)*&1 + 1), 1)", 1e-12);
    close("rewrite(normal(series(root(&(&1^3 - a*&1 - 2), 1), a, 0, 8)), a -> 1/20)", "root(&(&1^3 - (1/20)*&1 - 2), 1)", 1e-12);
    assert_eq!(ev("simplify(normal(series(root(&(&1^2 - 2 - a), 2), a, 0, 3)) - normal(series(sqrt(2 + a), a, 0, 3)))"), "0");
    // a double root at the point: unevaluated
    assert_eq!(ev("head(series(root(&(&1^2 - a), 1), a, 0, 3))"), "series");
}
