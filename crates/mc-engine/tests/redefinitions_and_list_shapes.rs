//! A definition that differs from an old one only by the names of its blanks replaces it; the shapes of
//! `gather_by` with several functions, `numerator_denominator` of a list and `implicit_d` of a list; `collect` by a
//! pattern; `inverse_weierstrass_p` with mixed arguments; `function_expand` of `anger_j` and `q_factorial`. Each
//! reported input with variations checked by an independent route.
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

#[test]
fn a_blank_named_once_is_the_same_left_side() {
    assert_eq!(ev("@on(g) rule f(g) = h(1); @on(g) rule f(g(x_)) := h(x); @on(g) rule f(g(_)) = 17; [up_values(g), f(g(5))]"), "[[f(g) :> h(1), f(g(_)) :> 17], 17]");
    assert_eq!(ev("@on(g) rule f(g(_)) = 17; @on(g) rule f(g(x_)) := h(x); [up_values(g), f(g(5))]"), "[[f(g(x_)) :> h(x)], h(5)]");
    assert_eq!(ev("rule k(x_) := 1; rule k(_) := 2; [down_values(k), k(3)]"), "[[k(_) :> 2], 2]");
    // a variable used twice is a condition (equal arguments): not the same left side as two blanks
    assert_eq!(ev("rule m(x_, x_) := 1; rule m(_, _) := 2; [m(1, 1), m(1, 2), length(down_values(m))]"), "[1, 2, 2]");
    // a variable read by a condition is not dropped
    assert_eq!(ev("rule q(x_) when x > 0 := 1; rule q(_) := 2; [q(1), q(-1), length(down_values(q))]"), "[1, 2, 2]");
}

#[test]
fn gather_by_several_functions_nests() {
    let xs = "[[a, 1, x], [a, 1, y], [a, 2, x], [a, 2, y], [b, 1, x], [b, 1, y], [b, 2, x], [b, 2, y]]";
    assert_eq!(
        ev(&format!("gather_by({xs}, [first, last])")),
        "[[[[a, 1, x], [a, 2, x]], [[a, 1, y], [a, 2, y]]], [[[b, 1, x], [b, 2, x]], [[b, 1, y], [b, 2, y]]]]"
    );
    // the same as gathering by the first function and each group by the second
    assert_eq!(ev(&format!("gather_by({xs}, [first, last]) == map(g => gather_by(g, last), gather_by({xs}, first))")), "true");
    assert_eq!(ev("gather_by(range(10), [&(mod(&1, 2)), &(mod(&1, 3))], length)"), "[[2, 2, 1], [2, 2, 1]]");
}

#[test]
fn numerator_denominator_of_a_list() {
    assert_eq!(ev("numerator_denominator([1, 2, 3, 4, 5, 6] / 3)"), "[[1, 3], [2, 3], [1, 1], [4, 3], [5, 3], [2, 1]]");
    assert_eq!(ev("numerator_denominator([x/y, (a + 1)/2])"), "[[x, y], [1 + a, 2]]");
    assert_eq!(ev("numerator_denominator(2/3)"), "[2, 3]");
}

#[test]
fn implicit_d_of_a_list_over_a_variable_list() {
    let r = ev("implicit_d([y^2, sin(x * y)], log(x + y) == t^2 * y^3, y, [[t, x]])");
    // each element's derivatives in t and x, in that order
    assert_eq!(ev(&format!("dimensions({r})")), "[2, 2]");
    assert_eq!(ev(&format!("simplify({r}[1] - implicit_d(y^2, log(x + y) == t^2 * y^3, y, [[t, x]]))")), "[0, 0]");
    assert_eq!(ev(&format!("simplify({r}[2][2] - implicit_d(sin(x * y), log(x + y) == t^2 * y^3, y, x))")), "0");
}

#[test]
fn collect_by_a_pattern() {
    assert_eq!(ev("collect(trig_reduce(cos(x)^50), _: Head[cos], &(mod(numerator(&1) * power_mod(denominator(&1), -1, 7), 7)))"), "4*cos(48*x) + 4*cos(50*x)");
    assert_eq!(ev("collect(7 + 3*cos(x) + 5*cos(2*x) + 2*cos(x)*y, _: Head[cos], &(&1^2))"), "49 + 25*cos(2*x) + cos(x)*(3 + 2*y)^2");
    assert_eq!(ev("collect(a*sin(x) + b*sin(x) + c*sin(2*x) + d, _: Head[sin])"), "d + c*sin(2*x) + (a + b)*sin(x)");
}

#[test]
fn inverse_weierstrass_p_with_mixed_arguments() {
    let z = "inverse_weierstrass_p([2.0, 2*7^(1/2)], [1, 2])";
    assert_eq!(ev(&format!("is_number({z})")), "true");
    // the point found has the asked values of p and p'
    assert_eq!(ev(&format!("abs(weierstrass_p({z}, [1, 2]) - 2) < 10^-10")), "true");
    assert_eq!(ev(&format!("abs(weierstrass_p_prime({z}, [1, 2]) - 2*7^(1/2)) < 10^-10")), "true");
}

#[test]
fn function_expand_of_anger_j_and_q_factorial() {
    let a = "function_expand(anger_j(ν, x))";
    assert_eq!(ev(&format!("free_of({a}, anger_j)")), "true");
    // against the function itself at two points
    assert_eq!(ev(&format!("abs(n(rewrite({a}, [ν -> 3/7, x -> 2]) - anger_j(3/7, 2), 20)) < 10^-15")), "true");
    assert_eq!(ev(&format!("abs(n(rewrite({a}, [ν -> -13/5, x -> 7/2]) - anger_j(-13/5, 7/2), 20)) < 10^-15")), "true");
    let q = "function_expand(q_factorial(n_u, 1/2))";
    assert_eq!(ev(q), "(1/2)^-n_u/q_pochhammer((1/2)^(1 + n_u), 1/2)*q_pochhammer(1/2, 1/2)");
    assert_eq!(ev(&format!("abs(n(rewrite({q}, n_u -> 5) - q_factorial(5, 1/2), 20)) < 10^-15")), "true");
    assert_eq!(ev("abs(n(rewrite(function_expand(q_factorial(n_u, 1/3)), n_u -> 7/2) - q_factorial(7/2, 1/3), 20)) < 10^-15"), "true");
    // a negative q stays
    assert_eq!(ev("function_expand(q_factorial(n_u, -1/2))"), "q_factorial(n_u, -1/2)");
}
