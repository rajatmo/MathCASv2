//! Round 39: periodic functions. Extrema of sums of two periodic functions with incommensurable periods (the sum
//! of the extrema, by Kronecker's theorem), definite integrals of the unit waves, function properties of the waves
//! between a and b, and the shape of a function with jumps or kinks and a parameter. Values are checked by
//! quadrature, by dense sampling, or by the identity behind them.
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
fn extrema_of_sums_with_incommensurable_periods() {
    assert_eq!(ev("max_value(sin(sqrt(3) * x) - exp(cos(3 * x) + 2), x)"), "1 - e");
    assert_eq!(ev("min_value(sin(sqrt(3) * x) - exp(cos(3 * x) + 2), x)"), "-1 - exp(3)");
    // the supremum is approached along a dense sample (it is not attained)
    close("max(table(sin(sqrt(3)*k/100.) - exp(cos(3*k/100.) + 2), [k, 0, 200000]))", "1 - e", 1e-3);
    // terms with commensurable periods are one group: sin(x) + cos(x) has the maximum sqrt(2)
    assert_eq!(ev("max_value(sin(x) + cos(x) + sin(sqrt(2)*x), x)"), "1 + 2^(1/2)");
    assert_eq!(ev("min_value(sin(x)^2 + cos(sqrt(5)*x)^3, x)"), "-1");
    assert_eq!(ev("max_value(cos(x) + cos(sqrt(2)*x) + 5, x)"), "7");
    // a ratio of periods that is not shown irrational (pi), or a term that is not continuous: unevaluated
    assert_eq!(ev("head(max_value(sin(x) + sin(pi*x), x))"), "max_value");
    assert_eq!(ev("head(max_value(tan(x) + sin(sqrt(2)*x), x))"), "max_value");
}

#[test]
fn definite_integrals_of_the_waves() {
    assert_eq!(ev("integrate(sawtooth_wave(x), [x, 0, 1])"), "1/2");
    assert_eq!(ev("integrate(square_wave(x), [x, 0, 1])"), "0");
    assert_eq!(ev("integrate(triangle_wave(x), [x, 0, 1])"), "0");
    assert_eq!(ev("integrate(triangle_wave(x), [x, 1/8, 5/3])"), "47/288");
    close("integrate(triangle_wave(x), [x, 1/8, 5/3])", "n_integrate(triangle_wave(x), [x, 1/8, 5/3])", 1e-8);
    assert_eq!(ev("integrate(square_wave(x), [x, 1/3, 9/4])"), "-1/12");
    close("integrate(square_wave(x), [x, 1/3, 9/4])", "n_integrate(square_wave(x), [x, 1/3, 9/4])", 1e-8);
    assert_eq!(ev("integrate(sawtooth_wave(2*x + 1/3), [x, 0, 1])"), "1/2");
}

#[test]
fn properties_of_the_waves_between_two_values() {
    // a nonconstant periodic function is not injective, neither convex nor concave
    assert_eq!(ev("function_injective(sawtooth_wave([a, b], x), x, assumptions: a < b)"), "false");
    assert_eq!(ev("function_convexity(sawtooth_wave([a, b], x), x, assumptions: a < b)"), "indeterminate");
    assert_eq!(ev("function_injective(square_wave([a, b], x), x, assumptions: a < b)"), "false");
    assert_eq!(ev("function_injective(triangle_wave([a, b], x), x, assumptions: a < b)"), "false");
    assert_eq!(ev("function_convexity(triangle_wave([a, b], x), x, assumptions: a < b)"), "indeterminate");
    // an affine image alpha + beta g with the sign of beta known
    assert_eq!(ev("function_convexity(c*x^2 + 3, x, assumptions: c < 0)"), "-1");
    assert_eq!(ev("function_monotonicity(a + b*exp(x), x, assumptions: b > 0)"), "1");
    assert_eq!(ev("function_injective(a + b*exp(x), x, assumptions: b != 0)"), "true");
}

#[test]
fn shape_of_a_function_with_jumps_and_a_parameter() {
    // c unit_step(x) and c sawtooth_wave(x) are not linear (their second derivatives are 0 away from the jumps):
    // undecided without the sign of c, not 0
    assert_eq!(ev("head(function_convexity(c*unit_step(x), x))"), "function_convexity");
    assert_eq!(ev("head(function_convexity(c*sawtooth_wave(x), x))"), "function_convexity");
    assert_eq!(ev("head(function_convexity(abs(x - a), x))"), "function_convexity");
}
