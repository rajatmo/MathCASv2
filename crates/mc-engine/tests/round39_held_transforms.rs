//! Round 39: derivatives of transforms and sums that stay unevaluated, transforms of equations with an undefined
//! function, and the Radon transform of a differential equation. Each held result is checked by substituting a
//! concrete function and comparing with the transform computed directly.
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
fn derivative_of_a_held_laplace_transform() {
    // d/ds of the transform of an undefined f stays; it is not the derivative of the transform of a constant
    assert_eq!(ev("d(laplace_transform(f(t), t, s), s)"), "d(laplace_transform(f(t), t, s), s)");
    assert_eq!(ev("derivative(0, 0, 1)(laplace_transform)"), "derivative(0, 0, 1)(laplace_transform)");
    // with f(t) = exp(-2 t) it is the transform of -t exp(-2 t), -1/(s + 2)^2
    assert_eq!(ev("simplify(rewrite(d(laplace_transform(f(t), t, s), s), f -> (t => exp(-2*t))) - laplace_transform(-t*exp(-2*t), t, s))"), "0");
    // the integration variable is bound: 0; a parameter differentiates under the integral sign
    assert_eq!(ev("d(laplace_transform(f(t), t, s), t)"), "0");
    assert_eq!(ev("d(fourier_transform(f(t), t, w), t)"), "0");
    assert_eq!(ev("d(laplace_transform(f(t, a), t, s), a)"), "laplace_transform(derivative(0, 1)(f)(t, a), t, s)");
    // the product rule goes through a derivative that stays
    assert_eq!(ev("d(s*laplace_transform(f(t), t, s), s)"), "laplace_transform(f(t), t, s) + s*d(laplace_transform(f(t), t, s), s)");
    assert_eq!(ev("d(laplace_transform(f(t), t, s), [s, 3])"), "d(laplace_transform(f(t), t, s), [s, 3])");
}

#[test]
fn derivative_of_a_held_sum() {
    // term by term in a parameter: with f(k) = 1/k! the sum is exp(s), whose derivative is exp(s)
    assert_eq!(ev("d(sum(f(k)*s^k, [k, 0, inf]), s)"), "sum(k*f(k)*s^(-1 + k), [k, 0, inf])");
    assert_eq!(ev("simplify(rewrite(d(sum(f(k)*s^k, [k, 0, inf]), s), f -> (k => 1/factorial(k))) - exp(s))"), "0");
    // the index is bound; a bound of the range and a product stay unevaluated
    assert_eq!(ev("d(sum(f(k), [k, 0, inf]), k)"), "0");
    assert_eq!(ev("d(sum(f(k)*s^k, [k, 0, n]), n)"), "d(sum(f(k)*s^k, [k, 0, n]), n)");
    assert_eq!(ev("d(product(f(k, s), [k, 1, n]), s)"), "d(product(f(k, s), [k, 1, n]), s)");
}

#[test]
fn z_transform_of_a_polynomial_times_an_undefined_function() {
    assert_eq!(
        ev("[z_transform(n_u * f(n_u), n_u, z), z_transform(n_u * (n_u + 1) * f(n_u), n_u, z)]"),
        "[-1*z*d(z_transform(f(n_u), n_u, z), z), d(z_transform(f(n_u), n_u, z), [z, 2])*z^2]"
    );
    // with f(n) = 2^n and 3^n the held forms give the transforms computed directly
    assert_eq!(ev("simplify(rewrite(z_transform(n_u*(n_u + 1)*f(n_u), n_u, z), f -> (k => 2^k)) - z_transform(n_u*(n_u + 1)*2^n_u, n_u, z))"), "0");
    assert_eq!(ev("simplify(rewrite(z_transform((n_u + 2)^2*f(n_u), n_u, z), f -> (k => 3^k)) - z_transform((n_u + 2)^2*3^n_u, n_u, z))"), "0");
}

#[test]
fn transforms_of_equations_and_rules_with_an_undefined_function() {
    assert_eq!(ev("z_transform(a^n_u == f(n_u), n_u, z)"), "z/(z - a) == z_transform(f(n_u), n_u, z)");
    assert_eq!(ev("z_transform(f(n_u) -> a^n_u, n_u, z)"), "z_transform(f(n_u), n_u, z) -> z/(z - a)");
    assert_eq!(ev("laplace_transform(f(t) == exp(t), t, s)"), "laplace_transform(f(t), t, s) == (-1 + s)^-1");
}

#[test]
fn radon_transform_of_a_poisson_equation() {
    // R[u_xx + u_yy] = d^2/dp^2 R[u]; the right side's transform, the ordinary equation in p, and the inverse
    // transform, which satisfies the equation
    let setup = "peqn = laplacian(u(x, y), [x, y]) == e^(-x^2 - y^2) * y * (-2 + x^2 + y^2); r1 = radon_transform(peqn, [x, y], [p, q]); ";
    assert_eq!(
        ev(&format!("{setup} r1")),
        "d(radon_transform(u(x, y), [x, y], [p, q]), [p, 2]) == (1/2)*p*(-3 + 2*p^2)*exp(-p^2)*pi^(1/2)*sin(q)"
    );
    let solved = "r2 = rewrite(d_solve_value(rewrite(r1, [radon_transform(u(x, y), [x, y], [p, q]) -> f(p)]), f(p), p), [c(i_) -> 0]); sol = inverse_radon_transform(r2, [p, q], [x, y]); ";
    assert_eq!(ev(&format!("{setup}{solved} sol")), "(1/4)*y*exp(-x^2 - y^2)");
    assert_eq!(ev(&format!("{setup}{solved} simplify(rewrite(peqn, [u -> ((x, y) => eval(sol))]))")), "true");
    // the other component, with x in place of y
    assert_eq!(
        ev("radon_transform(laplacian(u(x, y), [x, y]) == e^(-x^2 - y^2) * x * (-2 + x^2 + y^2), [x, y], [p, m])"),
        "d(radon_transform(u(x, y), [x, y], [p, m]), [p, 2]) == (1/2)*p*(-3 + 2*p^2)*cos(m)*exp(-p^2)*pi^(1/2)"
    );
    // a first-order term: R[u_x] = cos(phi) d/dp R[u]; and the inverse of a held transform
    assert_eq!(
        ev("radon_transform(d(u(x, y), x) + 3*u(x, y), [x, y], [p, q])"),
        "3*radon_transform(u(x, y), [x, y], [p, q]) + cos(q)*d(radon_transform(u(x, y), [x, y], [p, q]), p)"
    );
    assert_eq!(ev("inverse_radon_transform(radon_transform(f(x, y), [x, y], [p, q]), [p, q], [x, y])"), "f(x, y)");
    // a product of u with a function of x, y has no rule: it stays a failure, as before
    assert!(ev("radon_transform(x*u(x, y), [x, y], [p, q])").starts_with("failure(unevaluated, radon_transform"));
}
