//! Classic problems with known answers: accuracy of values and dense output, both machine methods, backwards
//! integration, systems, higher order, events, the stiff switch, and the forms of results.
mod common;
use common::{close, number, Sys};

/// The numbers of a printed list `[a, b, ...]`.
fn numbers(s: &str) -> Vec<f64> {
    let inner = s.strip_prefix('[').and_then(|r| r.strip_suffix(']')).unwrap_or_else(|| panic!("not a list: {s}"));
    inner.split(", ").map(number).collect()
}

/// The steps of the first rule's interpolating function in a printed `n_dsolve` result.
fn steps(s: &str) -> usize {
    let after = s.split("], ").nth(1).unwrap_or_else(|| panic!("no steps: {s}"));
    after.split(',').next().unwrap().trim().parse().unwrap_or_else(|_| panic!("no steps: {s}"))
}

#[test]
fn exponential_decay_with_every_method() {
    let mut s = Sys::new();
    for m in ["auto", "rk45", "stiff"] {
        let v = number(&s.run(&format!(
            "n_dsolve_value([y'(t) == -y(t), y(0) == 1], y(10), [t, 0, 10], method: {m}, accuracy_goal: 12, precision_goal: 12)"
        )));
        assert!(close(v, (-10f64).exp(), 1e-10), "{m}: {v}");
    }
    // the default tolerances: about 8 digits
    let v = number(&s.run("n_dsolve_value([y'(t) == -y(t), y(0) == 1], y(1), [t, 0, 1])"));
    assert!((v - (-1f64).exp()).abs() < 1e-7, "{v}");
}

#[test]
fn rules_and_their_evaluation() {
    let mut s = Sys::new();
    let r = s.run("sol = n_dsolve([y'(t) == -y(t), y(0) == 1], y, [t, 0, 10]); sol");
    assert!(r.starts_with("[y -> interpolating_function(\"y\", [0.0, 10.0], "), "{r}");
    assert!(close(number(&s.run("rewrite(y(2), sol)")), (-2f64).exp(), 1e-7));
    // a list of times, a symbolic time (left as it is), outside the domain
    let v = numbers(&s.run("rewrite(y([0, 5, 10]), sol)"));
    assert!(close(v[0], 1.0, 1e-12) && close(v[1], (-5f64).exp(), 1e-7) && close(v[2], (-10f64).exp(), 1e-7));
    assert!(s.run("rewrite(y(u), sol)").ends_with(")(u)"));
    assert_eq!(s.run("rewrite(y(11), sol)"), "failure(out_of_range, 11, [0.0, 10.0])");
    assert_eq!(s.run("rewrite(y(-0.5), sol)"), "failure(out_of_range, -0.5, [0.0, 10.0])");
    // the derivative of the dense output by name
    let f = s.run("rewrite(y, sol)").replace("(\"y\"", "(\"y'\"");
    assert!(close(number(&s.run(&format!("{f}(1)"))), -(-1f64).exp(), 1e-5));
    // equal solutions are equal terms (the key is the content's)
    assert_eq!(s.run("sol === n_dsolve([y'(t) == -y(t), y(0) == 1], y, [t, 0, 10])"), "true");
    assert_eq!(s.run("sol === n_dsolve([y'(t) == -y(t), y(0) == 2], y, [t, 0, 10])"), "false");
}

#[test]
fn dense_output_accuracy() {
    // y' = -y + sin t, y(0) = 1: y = 3/2 e^-t + (sin t - cos t)/2, checked between the steps
    let mut s = Sys::new();
    for m in ["rk45", "stiff"] {
        s.run(&format!(
            "s = n_dsolve([y'(t) == -y(t) + sin(t), y(0) == 1], y, [t, 0, 10], method: {m}, accuracy_goal: 10, precision_goal: 10)"
        ));
        let err = number(&s.run(
            "max(map(k => { let u = k/7 + 0.0123; abs(rewrite(y(u), s) - (1.5*exp(-u) + (sin(u) - cos(u))/2)) }, range(60)))",
        ));
        assert!(err < 1e-9, "{m}: dense output error {err}");
    }
}

#[test]
fn harmonic_oscillator_energy_and_higher_order() {
    let mut s = Sys::new();
    // y'' = -y over 100 periods / (2 pi): the energy y^2 + y'^2 drifts by less than the tolerance allows
    s.run("s = n_dsolve([y''(t) == -y(t), y(0) == 1, y'(0) == 0], y, [t, 0, 100], accuracy_goal: 12, precision_goal: 12)");
    let drift = number(&s.run(
        "max(map(k => abs(rewrite(y(k), s)^2 + n_dsolve_value([y''(t) == -y(t), y(0) == 1, y'(0) == 0], y'(k), [t, 0, 100], accuracy_goal: 12, precision_goal: 12)^2 - 1), range(0, 100, 10)))",
    ));
    assert!(drift < 1e-9, "energy drift {drift}");
    assert!(close(number(&s.run("rewrite(y(100), s)")), 100f64.cos(), 1e-9));
    // an equation that is linear in the highest derivative with a constant coefficient, of third order
    let v = number(&s.run("n_dsolve_value([2*y'''(t) + 2*y'(t) == 0, y(0) == 0, y'(0) == 1, y''(0) == 0], y(3), [t, 0, 3], accuracy_goal: 12, precision_goal: 12)"));
    assert!(close(v, 3f64.sin(), 1e-9), "{v}");
}

#[test]
fn systems_backwards_and_both_sides() {
    let mut s = Sys::new();
    // three components: a rotation and a decay
    let v = numbers(&s.run(
        "n_dsolve_value([x'(t) == -y(t), y'(t) == x(t), z'(t) == -z(t), x(0) == 1, y(0) == 0, z(0) == 1], [x(2), y(2), z(2)], [t, 0, 2], accuracy_goal: 12, precision_goal: 12)",
    ));
    assert!(close(v[0], 2f64.cos(), 1e-10) && close(v[1], 2f64.sin(), 1e-10) && close(v[2], (-2f64).exp(), 1e-10), "{v:?}");
    // the highest derivatives coupled through a constant matrix: x' + y' == y, y' == -x
    let w = numbers(&s.run(
        "n_dsolve_value([x'(t) + y'(t) == y(t), y'(t) == -x(t), x(0) == 1, y(0) == 0], [x(1), y(1)], [t, 0, 1], accuracy_goal: 12, precision_goal: 12)",
    ));
    let rk = numbers(&s.run(
        "n_dsolve_value([x'(t) == y(t) + x(t), y'(t) == -x(t), x(0) == 1, y(0) == 0], [x(1), y(1)], [t, 0, 1], accuracy_goal: 12, precision_goal: 12)",
    ));
    assert!(close(w[0], rk[0], 1e-10) && close(w[1], rk[1], 1e-10), "{w:?} {rk:?}");
    // backwards: t1 below t0
    let b = number(&s.run("n_dsolve_value([y'(t) == -y(t), y(0) == 1], y(-2), [t, 0, -2], accuracy_goal: 12, precision_goal: 12)"));
    assert!(close(b, 2f64.exp(), 1e-10), "{b}");
    // initial conditions inside the range: integrated to both ends
    let r = s.run("n_dsolve([y'(t) == -y(t), y(1) == 1], y, [t, 0, 2])");
    assert!(r.contains("[0.0, 2.0]"), "{r}");
    let both = numbers(&s.run("n_dsolve_value([y'(t) == -y(t), y(1) == 1], y([0, 2]), [t, 0, 2], accuracy_goal: 12, precision_goal: 12)"));
    assert!(close(both[0], 1f64.exp(), 1e-10) && close(both[1], (-1f64).exp(), 1e-10), "{both:?}");
}

#[test]
fn closures_of_n_dsolve_value() {
    let mut s = Sys::new();
    let f = s.run("f = n_dsolve_value([y'(t) == -y(t), y(0) == 1], y(t)^2 + t, [t, 0, 3]); f");
    assert!(f.starts_with("t => "), "{f}");
    assert!(close(number(&s.run("f(1)")), (-2f64).exp() + 1.0, 1e-7));
    // the highest derivative comes from the right-hand side along the solution
    let g = number(&s.run("n_dsolve_value([y'(t) == -y(t), y(0) == 1], y'(1) + y(1), [t, 0, 3])"));
    assert!(g.abs() < 1e-15, "{g}");
}

#[test]
fn lorenz_against_the_reference() {
    let mut s = Sys::new();
    let v = numbers(&s.run(
        "n_dsolve_value([x'(t) == 10*(y(t) - x(t)), y'(t) == x(t)*(28 - z(t)) - y(t), z'(t) == x(t)*y(t) - 8/3*z(t), x(0) == 1, y(0) == 1, z(0) == 1], [x(10), y(10), z(10)], [t, 0, 10], accuracy_goal: 10, precision_goal: 10)",
    ));
    let reference = [-4.902_687_541_134_646, -3.743_872_921_802_92, 24.690_858_102_790_557];
    for (a, b) in v.iter().zip(reference) {
        assert!((a - b).abs() < 1e-6, "{v:?}");
    }
}

#[test]
fn stiff_problems() {
    let mut s = Sys::new();
    // Van der Pol, mu = 1000, over two periods: the automatic switch makes it a few thousand steps
    let start = std::time::Instant::now();
    let auto = s.run("n_dsolve([x'(t) == y(t), y'(t) == 1000*(1 - x(t)^2)*y(t) - x(t), x(0) == 2, y(0) == 0], [x, y], [t, 0, 3000])");
    assert!(start.elapsed().as_secs_f64() < 5.0, "took {:?}", start.elapsed());
    assert!(steps(&auto) < 20_000, "{auto}");
    let vdp = "[x'(t) == y(t), y'(t) == 1000*(1 - x(t)^2)*y(t) - x(t), x(0) == 2, y(0) == 0]";
    let v = number(&s.run(&format!("n_dsolve_value({vdp}, x(3000), [t, 0, 3000], accuracy_goal: 12, precision_goal: 12)")));
    let stiff = number(&s.run(&format!(
        "n_dsolve_value({vdp}, x(3000), [t, 0, 3000], method: stiff, accuracy_goal: 12, precision_goal: 12)"
    )));
    assert!((v - stiff).abs() < 1e-8 && (v + 1.5106).abs() < 1e-3, "{v} {stiff}");
    // Robertson to 10^11: conserves the sum
    let rob = "[y1'(t) == -0.04*y1(t) + 10^4*y2(t)*y3(t), y2'(t) == 0.04*y1(t) - 10^4*y2(t)*y3(t) - 3*10^7*y2(t)^2, y3'(t) == 3*10^7*y2(t)^2, y1(0) == 1, y2(0) == 0, y3(0) == 0]";
    let r = numbers(&s.run(&format!(
        "n_dsolve_value({rob}, [y1(10^11), y2(10^11), y3(10^11)], [t, 0, 10^11], accuracy_goal: 14, precision_goal: 10)"
    )));
    assert!((r.iter().sum::<f64>() - 1.0).abs() < 1e-9 && close(r[0], 2.083340144135731e-08, 1e-5), "{r:?}");
    // a non-autonomous stiff equation (dF/dt from d): y' = -1000 (y - cos t)
    let c = number(&s.run(
        "n_dsolve_value([y'(t) == -1000*(y(t) - cos(t)), y(0) == 1], y(2), [t, 0, 2], method: stiff, accuracy_goal: 10, precision_goal: 10)",
    ));
    // the exact solution: the particular part plus a transient that is e^-2000 at t = 2
    let exact = 1000.0 * (1000.0 * 2f64.cos() + 2f64.sin()) / (1000.0 * 1000.0 + 1.0);
    assert!((c - exact).abs() < 1e-8, "{c} {exact}");
}

#[test]
fn events() {
    let mut s = Sys::new();
    // a falling body from 10 m stops when it reaches the ground
    let r = s.run("n_dsolve([y''(t) == -9.81, y(0) == 10, y'(0) == 0], y, [t, 0, 5], events: [y(t) == 0])");
    let te = (20.0f64 / 9.81).sqrt();
    let ev = r.split("events -> [[").nth(1).unwrap_or_else(|| panic!("{r}"));
    let t_event = ev.split(',').next().unwrap();
    assert!(close(number(t_event), te, 1e-10), "{r}");
    assert!(r.contains(&format!("[0.0, {t_event}]")), "the domain ends at the event: {r}");
    // recorded crossings of two event functions: zeros of sin and of cos
    let r = s.run(
        "n_dsolve([y''(t) == -y(t), y(0) == 0, y'(0) == 1], y, [t, 0, 10], events: [y(t) == 0, y'(t) == 0], event_action: record, accuracy_goal: 12, precision_goal: 12)",
    );
    let list = r.split("events -> [[").nth(1).unwrap().trim_end_matches("]]]");
    let times: Vec<(f64, usize)> = list
        .split("], [")
        .map(|p| {
            let (a, b) = p.split_once(", ").unwrap();
            (number(a), b.parse().unwrap())
        })
        .collect();
    assert_eq!(times.len(), 6, "{r}");
    for (k, &(t, e)) in times.iter().enumerate() {
        assert!(close(t, (k as f64 + 1.0) * std::f64::consts::FRAC_PI_2, 1e-9), "{times:?}");
        assert_eq!(e, if k % 2 == 0 { 2 } else { 1 }, "{times:?}");
    }
}

#[test]
fn failures() {
    let mut s = Sys::new();
    let cases = [
        ("n_dsolve([y'(t) == -y(t), y(0) == 1], y, [t, 0, 1], foo: 1)", "failure(unknown_option, foo)"),
        ("n_dsolve([y'(t) == -y(t), y(0) == 1], y, [t, 0, 1], method: bogus)", "failure(argument, n_dsolve, method)"),
        ("n_dsolve([y'(t) == -y(t), y(0) == 1], y, [t, 0, 1], digits: 0)", "failure(argument, n_dsolve, digits)"),
        ("n_dsolve([y'(t)^2 == y(t), y(0) == 1], y, [t, 0, 1])", "failure(argument, n_dsolve, \"implicit equation\")"),
        ("n_dsolve([y'(t) == y(t)], y, [t, 0, 1])", "failure(argument, n_dsolve, \"no initial conditions\")"),
        ("n_dsolve([y''(t) == y(t), y(0) == 1], y, [t, 0, 1])", "failure(argument, n_dsolve, \"missing initial condition for y'\")"),
        (
            "n_dsolve([y'(t) == y(t), y(0) == 1, y(1) == 2], y, [t, 0, 1])",
            "failure(argument, n_dsolve, \"1 boundary conditions for 0 unknown initial values\")",
        ),
        (
            "n_dsolve([y'(t) == y(t - 1), y(0) == 1], y, [t, 0, 1])",
            "failure(argument, n_dsolve, \"equation 1 is neither a differential equation in t nor an initial condition\")",
        ),
        (
            "n_dsolve([y(t) == t, y(0) == 0], y, [t, 0, 1])",
            "failure(argument, n_dsolve, \"no derivative of y occurs (algebraic equations are not supported)\")",
        ),
        ("n_dsolve([y'(t) == -y(t), y(0) == 1], 2, [t, 0, 1])", "failure(argument, n_dsolve, 2)"),
        ("n_dsolve([y'(t) == -y(t), y(0) == 1], y, [t])", "failure(argument, n_dsolve, 3)"),
        // [t, t1]: from the initial point to t1; chained initial conditions x(0) == y(0) == 1
        ("rewrite(round(10^6 * y(2)), n_dsolve([y'(t) == -y(t), y(0) == 1], y, [t, 2]))", "135335"),
        ("rewrite(round(10^6 * [x(1), y(1)]), n_dsolve([x'(t) == y(t), y'(t) == -x(t), x(0) == y(0) and y(0) == 1], [x, y], [t, 1]))", "[1381773, -301169]"),
        ("n_dsolve([y'(t) == log(y(t)), y(0) == -1], y, [t, 0, 1])", "failure(not_numeric, n_dsolve, [t -> 0.0, y -> -1.0])"),
        ("n_dsolve_value([y'(t) == t*y(t) + mu, y(0) == 1], y(1), [t, 0, 1])", "failure(not_numeric, n_dsolve_value, [t -> 0.0, y -> 1.0])"),
        ("n_dsolve([y'(t) == y(t), y(0) == 1], y, [t, 0, 1], events: [y(t)^2 == z])", "failure(not_numeric, n_dsolve, [t -> 0.0, y -> 1.0])"),
        (
            "n_dsolve([y'(t) == y(t), y(0) == 1], y, [t, 0, 1], digits: 30, events: [y(t) == 2])",
            "failure(argument, n_dsolve, \"events need machine precision\")",
        ),
        (
            "n_dsolve([y'(t) == gamma(y(t)), y(0) == 1], y, [t, 0, 1], digits: 30)",
            "failure(argument, n_dsolve, \"the Taylor method does not support gamma\")",
        ),
        (
            "interpolating_function(\"y\", [0.0, 1.0], 3, \"0000000000000001\")(0.5)",
            "failure(argument, interpolating_function, \"no such solution in this session\")",
        ),
    ];
    for (src, want) in cases {
        assert_eq!(s.run(src), want, "{src}");
    }
    // a blow-up at t = 1 and a step limit: the partial solution (with a printed warning), its domain ending where
    // the run stopped
    let r = s.run("n_dsolve([y'(t) == y(t)^2, y(0) == 1], y, [t, 0, 2])");
    assert!(r.starts_with("[y -> interpolating_function(\"y\", [0.0, 1.0"), "{r}");
    let r = s.run("n_dsolve([y'(t) == -y(t), y(0) == 1], y, [t, 0, 10], max_steps: 5)");
    assert!(r.starts_with("[y -> interpolating_function(\"y\", [0.0, 0."), "{r}");
}

/// Explicit equations (one `+-y^(m)` term per equation) are solved without differentiating; they give the
/// solutions of the same equations written so that the general path (a constant coefficient matrix inverted
/// exactly) runs, step for step.
#[test]
fn explicit_and_general_forms_agree() {
    let mut s = Sys::new();
    let pairs = [
        // y' + k y == s against 2 y' == 2 (s - k y)
        ("[y'(t) + 0.7*y(t) == 1.5, y(0) == 0]", "[2*y'(t) == 2*(1.5 - 0.7*y(t)), y(0) == 0]", "y"),
        ("[-y'(t) == y(t) - 1.5, y(0) == 0]", "[3*y'(t) == 3*(1.5 - y(t)), y(0) == 0]", "y"),
        ("[y''(t) == -y(t), y(0) == 1, y'(0) == 0]", "[2*y''(t) == -2*y(t), y(0) == 1, y'(0) == 0]", "y"),
        (
            "[x'(t) == z(t), z'(t) == -x(t), x(0) == 0, z(0) == 1]",
            "[x'(t) + z'(t) == z(t) - x(t), x'(t) - z'(t) == z(t) + x(t), x(0) == 0, z(0) == 1]",
            "x",
        ),
    ];
    for (fast, general, y) in pairs {
        let a = numbers(&s.run(&format!("n_dsolve_value({fast}, [{y}(0.5), {y}(2)], [t, 0, 2])")));
        let b = numbers(&s.run(&format!("n_dsolve_value({general}, [{y}(0.5), {y}(2)], [t, 0, 2])")));
        for (u, v) in a.iter().zip(&b) {
            assert!(close(*u, *v, 1e-12), "{fast}: {a:?} against {b:?}");
        }
    }
    // values: y = (1.5/0.7)(1 - e^-0.7t), cos t, sin t
    let v = numbers(&s.run("n_dsolve_value([y'(t) + 0.7*y(t) == 1.5, y(0) == 0], [y(1)], [t, 0, 1])"));
    assert!(close(v[0], 1.5 / 0.7 * (1.0 - (-0.7f64).exp()), 1e-7), "{v:?}");
    let v = numbers(&s.run("n_dsolve_value([x'(t) == z(t), z'(t) == -x(t), x(0) == 0, z(0) == 1], [x(1), z(1)], [t, 0, 1])"));
    assert!(close(v[0], 1f64.sin(), 1e-7) && close(v[1], 1f64.cos(), 1e-7), "{v:?}");
    // a highest derivative in two terms of one residual still goes the general way; a nonlinear one is refused
    assert!(s.run("n_dsolve_value([y'(t)^2 == 1, y(0) == 0], y(1), [t, 0, 1])").contains("implicit equation"));
}

/// Values that are numbers do not keep their solutions (N3.13); solutions that terms name stay usable.
#[test]
fn numeric_values_do_not_keep_named_solutions_from_working() {
    let mut s = Sys::new();
    let eqs = "[y'(t) == -y(t), y(0) == 1]";
    s.run(&format!("sol = n_dsolve({eqs}, y, [t, 0, 1]); g = n_dsolve_value({eqs}, y(t), [t, 0, 1])"));
    // the same solution again, as numbers, many times
    let v = number(&s.run(&format!("sum(table(n_dsolve_value({eqs}, y(1), [t, 0, 1]), i, 50))/50")));
    assert!(close(v, (-1f64).exp(), 1e-7), "{v}");
    let p = s.run("f = parametric_n_dsolve_value([y'(t) == -k*y(t), y(0) == 1], y(1), [t, 0, 1], [k]); f(1.0)");
    assert!(close(number(&p), (-1f64).exp(), 1e-7), "{p}");
    // the rules and the closure made before still evaluate
    assert!(close(number(&s.run("rewrite(y(0.5), sol)")), (-0.5f64).exp(), 1e-7));
    assert!(close(number(&s.run("g(0.5)")), (-0.5f64).exp(), 1e-7));
    // and the solution of a value is gone only when nothing names it: a closure keeps it
    assert!(close(number(&s.run("h = parametric_n_dsolve_value([y'(t) == -k*y(t), y(0) == 1], y(t), [t, 0, 1], [k])(2.0); h(1)")), (-2f64).exp(), 1e-7));
}

#[test]
fn max_step_fraction_bounds_the_step() {
    let mut s = Sys::new();
    let r = s.run("n_dsolve([y'(t) == -y(t), y(0) == 1], y, [t, 0, 1], max_step_fraction: 1/1000)");
    assert!(steps(&r) >= 1000, "{r}");
    let r = s.run("n_dsolve([y'(t) == -y(t), y(0) == 1], y, [t, 0, 2], max_step_fraction: 1/100, max_step_size: 0.001)");
    assert!(steps(&r) >= 2000, "{r}");
    let r = s.run("n_dsolve([y'(t) == -y(t), y(0) == 1], y, [t, 0, 1], max_step_fraction: 2)");
    assert!(r.starts_with("failure(argument"), "{r}");
}

#[test]
fn boundary_value_problems_by_shooting() {
    let mut s = Sys::new();
    let v = number(&s.run("n_dsolve_value([y''(x) == y(x), y(0) == 0, y(1) == 1], y(0.5), [x, 0, 1])"));
    assert!(close(v, 0.5f64.sinh() / 1f64.sinh(), 1e-7), "{v}");
    let v = number(&s.run("n_dsolve_value([y''(x) == -y(x), y'(0) == 1, y(2) == 0], y(1), [x, 0, 2])"));
    let want = -(2f64.tan()).recip() * 0.0 + (1f64.sin() - 2f64.tan() * 1f64.cos());
    assert!(close(v, want, 1e-7), "{v} {want}");
    let r = s.run("n_dsolve([y''(x) == y(x), y(0) == 0, y(1) == 1, y(2) == 3], y, [x, 0, 2])");
    assert!(r.starts_with("failure(argument"), "{r}");
}
