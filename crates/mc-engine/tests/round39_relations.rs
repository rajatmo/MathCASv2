//! Round 39: asymptotic relations with a parameter on the boundary of its assumption and with branch points; the
//! Wronskian of a linear first-order system; integrals and transforms of a unit mass at a symbolic point. Values are
//! checked at sample points, by series, or by Liouville's and the sifting identities.
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

#[test]
fn relations_with_a_parameter_on_the_boundary() {
    // a >= 0: at a = 0 x^a is the constant 1, for a > 0 it tends to 0; both cases agree
    assert_eq!(ev("asymptotic_equal(1 + x^a, 1, x -> 0, assumptions: a >= 0)"), "true");
    assert_eq!(ev("asymptotic_greater_equal(1, x^a, x -> 0, assumptions: a >= 0)"), "true");
    assert_eq!(ev("asymptotic_less(1, x^a, x -> 0, assumptions: a >= 0)"), "false");
    assert_eq!(ev("asymptotic_less_equal(x^a, x, x -> inf, assumptions: a <= 1)"), "true");
    // the cases disagree (x^a = o(1) only for a > 0): undecided
    assert!(ev("asymptotic_greater(1, x^a, x -> 0, assumptions: a >= 0)").starts_with("failure(undecided"));
    assert!(ev("asymptotic_equivalent(1 + x^n_u, x^n_u, x -> inf, assumptions: n_u >= 0)").starts_with("failure(undecided"));
}

#[test]
fn relations_at_a_branch_point() {
    // sqrt(z) - i = (z + 1)/(sqrt(z) + i) on both sides of -1 (the principal branch on the cut): f ~ c (z + 1)^3
    assert_eq!(ev("asymptotic_greater((sqrt(z) - im)^3, z + 1, z -> -1)"), "false");
    assert_eq!(ev("asymptotic_less(z + 1, (sqrt(z) - im)^2, z -> -1)"), "false");
    assert_eq!(ev("asymptotic_greater(sqrt(-z), z + 1, z -> -1)"), "true");
    assert_eq!(ev("abs(n(rewrite((sqrt(z) - im)^3/(z + 1)^3, z -> -1 - 10^-6))) < 1"), "true");
    // along x = i t: sqrt(x - 1) - i ~ t/2, sqrt(tanh(x)) ~ sqrt(i t)
    assert_eq!(ev("asymptotic_greater(sqrt(tanh(x)), sqrt(x - 1) - im, x -> 0, direction: -im)"), "true");
    assert_eq!(ev("asymptotic_less(sqrt(x - 1) - im, sqrt(tanh(x)), x -> 0, direction: -im)"), "true");
    assert_eq!(ev("abs(n(rewrite((sqrt(x - 1) - im)/sqrt(tanh(x)), x -> im*10^-8))) < 10^-3"), "true");
}

#[test]
fn wronskian_of_a_first_order_system() {
    // Liouville: exp(integrate(trace(A), x)), the determinant of the fundamental matrix that is the identity at 0
    assert_eq!(ev("wronskian([y'(x) == y(x) - z(x), z'(x) == y(x) + z(x)], [y, z], x)"), "exp(2*x)");
    assert_eq!(ev("wronskian([y'(x) == v(x), v'(x) == z(x), z'(x) == -z(x) + y(x)], [v, y, z], x)"), "exp(-x)");
    assert_eq!(ev("wronskian([y'(x) == x*y(x) + z(x), z'(x) == y(x) + sin(x)], [y, z], x)"), "exp((1/2)*x^2)");
    // the fundamental matrix exp(x A) of the first system: columns e^x (cos x, sin x), e^x (-sin x, cos x)
    assert_eq!(ev("simplify(det([[exp(x)*cos(x), -exp(x)*sin(x)], [exp(x)*sin(x), exp(x)*cos(x)]]))"), "exp(2*x)");
    // a nonlinear system has no Wronskian
    assert_eq!(ev("head(wronskian([y'(x) == y(x)*z(x), z'(x) == y(x)], [y, z], x))"), "wronskian");
}

#[test]
fn unit_mass_at_a_symbolic_point() {
    assert_eq!(ev("integrate(dirac_delta(x - a), [x, 0, y], assumptions: element(a, reals) and y > 0)"), "heaviside_theta(a)*heaviside_theta(y - a)");
    assert_eq!(ev("integrate(dirac_delta(x - a), [x, -inf, inf])"), "1");
    // the scaling rule: integrate(x^2 dirac_delta(2 x - a)) = (a/2)^2/2
    assert_eq!(ev("integrate(x^2*dirac_delta(2*x - a), [x, -inf, inf])"), "(1/8)*a^2");
    // the orientation of [0, y] is not known: unevaluated
    assert_eq!(ev("head(integrate(dirac_delta(x - a), [x, 0, y]))"), "integrate");
    assert_eq!(ev("laplace_transform(dirac_delta(x - a), x, t) === heaviside_theta(a)*exp(-a*t)"), "true");
    assert_eq!(ev("rewrite(laplace_transform(dirac_delta(x - a), x, t), a -> 2) - laplace_transform(dirac_delta(x - 2), x, t)"), "0");
    assert_eq!(ev("simplify(fourier_transform(dirac_delta(x - a), x, t) - (cos(a*t) + im*sin(a*t))*2^(1/2)/(2*pi^(1/2)))"), "0");
    assert_eq!(ev("simplify(rewrite(fourier_transform(dirac_delta(x - a), x, t), a -> 2) - fourier_transform(dirac_delta(x - 2), x, t))"), "0");
}
