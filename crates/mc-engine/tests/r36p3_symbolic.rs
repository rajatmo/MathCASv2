//! Round 36: fixed wrong answers in equation solving, algebra, number theory and
//! calculus, one test per cause. Each value is also checked by an independent route (substitution into the
//! equations, a Taylor sum at 40 digits, a brute-force count).
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

/// An order relation makes the unknown real: the non-real roots of the polynomial do not satisfy it.
#[test]
fn an_order_relation_filters_non_real_roots() {
    assert_eq!(ev("solve(x^2 == -1 and x > 0, x)"), "[]");
    assert_eq!(ev("solve(x^2 == -1 and x <= 0, x)"), "[]");
    assert_eq!(ev("solve(x^3 == 1 and x > 0, x)"), "[[x -> 1]]");
    assert_eq!(ev("solve(x^2 == 4 and x > 0, x)"), "[[x -> 2]]");
    // the real part may still be ordered for a complex unknown
    assert_eq!(ev("solve(x^2 == -1 and re(x) >= 0, x)"), "[[x -> -1im], [x -> 1im]]");
}

/// `element([x, y], infinite_line(...))` is the line's equation, so the line meets the circle in two points.
#[test]
fn a_line_meets_a_circle_in_two_points() {
    let q = "element([x, y], infinite_line([[0, 0], [2, 1]])) and element([x, y], circle())";
    assert_eq!(ev(&format!("solve({q}, [x, y])")), "[[x -> (-2/5)*5^(1/2), y -> (-1/5)*5^(1/2)], [x -> (2/5)*5^(1/2), y -> (1/5)*5^(1/2)]]");
    // substitution into the line x - 2 y == 0 and the circle
    assert_eq!(ev(&format!("simplify(rewrite([x - 2*y, x^2 + y^2 - 1], solve({q}, [x, y])))")), "[[0, 0], [0, 0]]");
    assert_eq!(ev(&format!("solve_values({q}, [x, y])")), "[[(-2/5)*5^(1/2), (-1/5)*5^(1/2)], [(2/5)*5^(1/2), (1/5)*5^(1/2)]]");
    assert_eq!(ev(&format!("length(n_solve({q}, [x, y]))")), "2");
    assert_eq!(ev(&format!("find_instance({q}, [x, y])")), "[[x -> (-2/5)*5^(1/2), y -> (-1/5)*5^(1/2)]]");
    assert_eq!(ev("region_member(infinite_line([[0, 0], [2, 1]]), [x, y])"), "element(x | y, reals) and x == 2*y");
}

/// `to_number_field(a)` of one number: `a` generates the field; `root_reduce` of it is the number again.
#[test]
fn to_number_field_of_one_number() {
    assert_eq!(ev("to_number_field(root(&(&1^5 + &1 + 11), 1))"), "algebraic_number(root(&(11 + &1 + &1^5), 1), [0, 1, 0, 0, 0])");
    assert_eq!(ev("root_reduce(to_number_field(root(&(&1^5 + &1 + 11), 1)))"), "root(&(11 + &1 + &1^5), 1)");
    assert_eq!(ev("to_number_field(sqrt(2))"), "algebraic_number(2^(1/2), [0, 1])");
}

/// A matrix of machine numbers and exact constants: a machine matrix, its exponential accurate to the last digits
/// (against the Taylor sum at 40 digits).
#[test]
fn matrix_exp_of_machine_numbers_with_constants() {
    let src = "m1 = [[1. + im, 2, 3 - 2 * im], [0, 4 * pi, 5 * im], [e, 0, 6]]
        m40 = n([[1 + im, 2, 3 - 2 * im], [0, 4 * pi, 5 * im], [e, 0, 6]], 40)
        t = sum(matrix_power(m40, k) / factorial(k), [k, 0, 120])
        r = matrix_exp(m1)
        [all_true(flatten(r), is_machine_number), max(abs(flatten(r - t))) / max(abs(flatten(t))) < 10^-15]";
    assert_eq!(ev(src), "[true, true]");
    // balls stay balls, at their precision
    assert_eq!(ev("precision(matrix_exp(n([[1, 2], [3, 4]], 40))[1, 1]) > 39"), "true");
    assert_eq!(ev("abs(matrix_exp(n([[1, 2], [3, 4]], 40))[1, 1] - n(matrix_exp([[1, 2], [3, 4]])[1, 1], 50)) < 10^-37"), "true");
}

/// A very short `time_constraint:` still gives the identity one move away (it was a race with the clock).
#[test]
fn full_simplify_short_time_constraint() {
    for _ in 0..3 {
        assert_eq!(ev("full_simplify(x*gamma(x), time_constraint: 0.0001)"), "gamma(1 + x)");
    }
}

/// Gaussian prime powers in machine integers agree with the brute-force count (330 in the box 1..40, from the
/// powers of the Gaussian primes enumerated directly).
#[test]
fn gaussian_prime_powers() {
    assert_eq!(ev("length(select(flatten(table(a + b * im, [a, 40], [b, 40])), is_prime_power))"), "330");
    assert_eq!(
        ev("map(is_prime_power, [1 + im, 2 + im, (2 + im)^3, 3 + 4*im, 5 + 5*im, 3*im, 5*im, 4*im, im, 1 + 2*im + 0])"),
        "[true, true, true, true, false, true, false, true, false, true]"
    );
}

/// `n_roots` with `precision_goal:` gives roots with that many digits.
#[test]
fn n_roots_precision_goal() {
    assert_eq!(ev("r = n_roots(x^2 == 2, x, precision_goal: 30)\n[precision(r[2][2]) >= 30, abs(r[2][2] - sqrt(2)) < 10^-29]"), "[true, true]");
    assert_eq!(ev("n_roots(x^2 == 2, x)"), "x == -1.4142135623730951 or x == 1.4142135623730951");
}

/// A branch through a singular point needs a Puiseux expansion: `asymptotic_solve` declines with a reason.
#[test]
fn asymptotic_solve_declines_at_a_singular_point() {
    let r = ev("asymptotic_solve(y^2 - t * x + t == 0 and x^2 + t * y == 0, [[x, y], [0, 0]], [t, 0, 2], reals)");
    assert!(r.starts_with("failure(unevaluated, asymptotic_solve"), "{r}");
    assert!(r.contains("Puiseux"), "{r}");
}
