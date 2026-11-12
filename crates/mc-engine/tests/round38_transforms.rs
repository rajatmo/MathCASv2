//! Round 38: Hankel transforms and Mellin convolutions as inverse Mellin transforms of products of Mellin
//! transforms. Each value is checked by an independent route: the defining integral by numerical quadrature at a
//! sample point, or a closed form known from the literature.
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

/// `a` and `b` evaluate to the same expression.
fn same(a: &str, b: &str) {
    assert_eq!(ev(&format!("({a}) === ({b})")), "true", "{a}: {}", ev(a));
}

#[test]
fn hankel_transform_of_bessel_k_is_a_lorentzian() {
    // integrate(bessel_k(0, a r) bessel_j(0, s r) r, [r, 0, inf]) = 1/(a^2 + s^2)
    assert_eq!(ev("hankel_transform(bessel_k(0, r), r, s)"), "(1 + s^2)^-1");
    assert_eq!(ev("inverse_hankel_transform(bessel_k(0, s), s, r)"), "(1 + r^2)^-1");
    assert_eq!(ev("simplify(hankel_transform(bessel_k(0, 2*r), r, s) - 1/(4 + s^2))"), "0");
    close("rewrite(hankel_transform(bessel_k(0, 3*r), r, s), s -> 7/10)", "n_integrate(bessel_k(0, 3*r)*bessel_j(0, 7/10*r)*r, [r, 0, inf])", 1e-7);
}

#[test]
fn hankel_transform_of_erfc_and_exponential_integral() {
    assert_eq!(ev("hankel_transform(erfc(r)/r, r, s)"), "erf((1/2)*s)/s");
    // the hypergeometric form is asinh(s)/s
    close("rewrite(hankel_transform(exp_integral_e(1, r)/r, r, s), s -> 3/2)", "asinh(3/2)/(3/2)", 1e-12);
    close("rewrite(hankel_transform(exp_integral_e(1, r)/r, r, s), s -> 1/3)", "n_integrate(exp_integral_e(1, r)*bessel_j(0, r/3), [r, 0, inf])", 1e-7);
    // order 1, checked by quadrature
    close("rewrite(hankel_transform(erfc(r)/r, r, s, 1), s -> 4/5)", "n_integrate(erfc(r)*bessel_j(1, 4/5*r), [r, 0, inf])", 1e-7);
    close("rewrite(hankel_transform(bessel_k(0, r), r, s, 2), s -> 6/5)", "n_integrate(bessel_k(0, r)*bessel_j(2, 6/5*r)*r, [r, 0, inf])", 1e-7);
}

#[test]
fn hankel_transform_with_a_jump_declines() {
    // the transform of sin(r)/r is 1/sqrt(1 - s^2) for s < 1 and 0 for s > 1: one residue series cannot give it
    assert_eq!(ev("head(hankel_transform(sin(r)/r, r, s))"), "hankel_transform");
    assert_eq!(ev("head(hankel_transform(sin(2*r)/r, r, s))"), "hankel_transform");
}

#[test]
fn mellin_convolution_by_mellin_transforms() {
    // integrate(exp(-x)/(x + y), [x, 0, inf]) = exp(y) gamma(0, y)
    assert_eq!(ev("mellin_convolve(exp(-x), 1/(x + 1), x, y)"), "gamma(0, y)*exp(y)");
    assert_eq!(ev("mellin_convolve(exp(-x), 1/(x + 1), x, 1)"), "e*gamma(0, 1)");
    assert_eq!(ev("mellin_convolve(exp(-a*x), 1/(x + 1), x, y, generate_conditions: true)"), "conditional_expression(gamma(0, a*y)*exp(a*y), re(a) > 0)");
    close("rewrite(mellin_convolve(exp(-2*x), 1/(x + 1), x, y), y -> 3/2)", "n_integrate(exp(-2*x)/(x*(1 + 3/(2*x))), [x, 0, inf])", 1e-8);
    // integrate(exp(-x - y/x)/x, [x, 0, inf]) = 2 bessel_k(0, 2 sqrt(y))
    close("rewrite(mellin_convolve(exp(-x), exp(-x), x, y), y -> 5/7)", "2*bessel_k(0, 2*(5/7)^(1/2))", 1e-10);
    // Bessel functions: the Meijer G-function of type q > p, whose series converges everywhere
    assert_eq!(ev("simplify(mellin_convolve(bessel_j(0, x), bessel_j(1, x), x, y) - bessel_j(1, 2*y^(1/2))/y^(1/2))"), "0");
    close("rewrite(mellin_convolve(sin(x), cos(x), x, y), y -> 2)", "pi/2*bessel_j(0, 2*2^(1/2))", 1e-10);
    close("rewrite(mellin_convolve(sin(x), cos(x), x, y), y -> 9/4)", "pi/2*bessel_j(0, 3)", 1e-10);
}

#[test]
fn radon_transform_of_indicators_of_disks() {
    // the chord of the unit disk at distance |p| from its centre has length 2 sqrt(1 - p^2)
    same("radon_transform(unit_step(1 - x^2 - y^2), [x, y], [p, f])", "piecewise([[(4 - 4*p^2)^(1/2), -1 < p and p < 1]], 0)");
    same("radon_transform(unit_step(4 - x^2 - y^2), [x, y], [p, f])", "piecewise([[(16 - 4*p^2)^(1/2), -2 < p and p < 2]], 0)");
    close("rewrite(radon_transform(unit_step(1 - x^2 - y^2), [x, y], [p, f], assumptions: -1 < p and p < 1), p -> 3/5)", "8/5", 1e-14);
    // the disk of radius 1 about (1, 0): the line is at distance |p - cos(f)| from the centre
    close("rewrite(radon_transform(unit_step(1 - (x - 1)^2 - y^2), [x, y], [p, f]), [p -> 1/4, f -> 0])", "2*(1 - 9/16)^(1/2)", 1e-12);
    close("rewrite(radon_transform(unit_step(1 - (x - 1)^2 - y^2), [x, y], [p, f]), [p -> 3, f -> 0])", "0", 1e-14);
    // moments over the disk: integrate((p c - t s), t) and integrate(p^2 + t^2, t) over the chord
    close("rewrite(radon_transform(x*unit_step(1 - x^2 - y^2), [x, y], [p, f]), [p -> 1/2, f -> 1])", "2*(1/2)*cos(1)*(3/4)^(1/2)", 1e-12);
    close("rewrite(radon_transform((x^2 + y^2)*unit_step(1 - x^2 - y^2), [x, y], [p, f]), p -> 1/2)", "(2/3)*(1 + 2/4)*(3/4)^(1/2)", 1e-12);
}

#[test]
fn radon_transform_of_gaussians_with_options_and_deltas() {
    same(
        "radon_transform(y*exp(-a*(x^2 + y^2)), [x, y], [p, f], generate_conditions: true)",
        "conditional_expression(p*exp(-a*p^2)*pi^(1/2)*sin(f)/a^(1/2), re(a) > 0)"
    );
    same("radon_transform(y*exp(-a*(x^2 + y^2)), [x, y], [p, f], generate_conditions: true, assumptions: a > 0)", "p*exp(-a*p^2)*pi^(1/2)*sin(f)/a^(1/2)");
    // the line integral by quadrature at a sample line
    close(
        "rewrite(radon_transform(cos(x + y)*exp(-x^2 - y^2), [x, y], [p, f]), [p -> 3/10, f -> 7/10])",
        "n_integrate(rewrite(cos(x + y)*exp(-x^2 - y^2), [x -> 3/10*cos(7/10) - t*sin(7/10), y -> 3/10*sin(7/10) + t*cos(7/10)]), [t, -inf, inf])",
        1e-9,
    );
    close(
        "rewrite(radon_transform(sin(2*x + y)*exp(-x^2 - y^2), [x, y], [p, f]), [p -> -1/2, f -> 2])",
        "n_integrate(rewrite(sin(2*x + y)*exp(-x^2 - y^2), [x -> -1/2*cos(2) - t*sin(2), y -> -1/2*sin(2) + t*cos(2)]), [t, -inf, inf])",
        1e-9,
    );
    same("radon_transform(dirac_delta(x - a, y - b), [x, y], [p, f])", "dirac_delta(p - a*cos(f) - b*sin(f))");
    same("radon_transform(x^2*dirac_delta(x - 2, y + 1), [x, y], [p, f])", "4*dirac_delta(p - 2*cos(f) + sin(f))");
}

#[test]
fn radon_transform_inverse_pairs_and_linearity() {
    same("radon_transform(inverse_radon_transform(F(p, f), [p, f], [x, y]), [x, y], [p, f])", "F(p, f)");
    same(
        "inverse_radon_transform(a*g(p, f) + b*h(p, f), [p, f], [x, y])",
        "a*inverse_radon_transform(g(p, f), [p, f], [x, y]) + b*inverse_radon_transform(h(p, f), [p, f], [x, y])"
    );
    // a Gaussian term is inverted (radon_transform(exp(-x^2 - y^2)) = sqrt(pi) exp(-p^2)); a term of no known
    // preimage keeps the whole transform
    same("inverse_radon_transform(exp(-p^2) + 2*g(p, f), [p, f], [x, y])", "2*inverse_radon_transform(g(p, f), [p, f], [x, y]) + exp(-x^2 - y^2)/pi^(1/2)");
    assert_eq!(ev("head(inverse_radon_transform(sin(p) + 2*g(p, f), [p, f], [x, y]))"), "inverse_radon_transform");
    assert_eq!(ev("head(inverse_radon_transform(F(p, f), [p, f], [x, y]))"), "inverse_radon_transform");
}
