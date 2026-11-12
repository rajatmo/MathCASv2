//! Tests of the differential equations and differential algebra library, evaluated on the
//! assembled core. Results are checked against the literature's worked examples and by substitution.
use mc_api::{Engine, StopFlag};

fn ev(src: &str) -> String {
    let mut e = mc_engine::CoreEngine::new();
    let stop = StopFlag::new();
    e.eval(src, &stop).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn check(cases: &[(&str, &str)]) {
    for (src, want) in cases {
        assert_eq!(ev(src), *want, "{src}");
    }
}

/// every generator satisfies the determining equations (substituted into xi and eta)
fn verify(eq: &str, u: &str, x: &str) {
    let src = format!("every(select(lie_point_symmetries({eq}, {u}, {x}), g => is_list(g)), g => dg_check_generator({eq}, {u}, {x}, g))");
    assert_eq!(ev(&src), "true", "{eq}");
}

#[test]
fn ode_symmetries() {
    check(&[
        // y'' = 0: the eight-dimensional sl(3) (Olver, Example 2.43)
        ("length(lie_point_symmetries(y''(x) == 0, y, x))", "8"),
        // the Ermakov-Pinney equation: sl(2)
        ("lie_point_symmetries(y''(x) == y(x)^(-3), y, x)", "[[2*x, y], [x^2, x*y], [1, 0]]"),
        // the Blasius equation: translation and scaling (Bluman and Anco, 3.3)
        ("lie_point_symmetries(y'''(x) + y(x)*y''(x) == 0, y, x)", "[[-x, y], [1, 0]]"),
        ("length(lie_point_symmetries(y''(x) + y(x) == 0, y, x))", "8"),
        ("symmetry_determining_equations(y''(x) == 0, y, x)",
         "[-derivative(0, 2)(xi)(x, y) == 0, -2*derivative(1, 1)(xi)(x, y) + derivative(0, 2)(eta)(x, y) == 0, -derivative(2, 0)(xi)(x, y) + 2*derivative(1, 1)(eta)(x, y) == 0, derivative(2, 0)(eta)(x, y) == 0]"),
    ]);
    verify("y''(x) == y(x)^(-3)", "y", "x");
    verify("y''(x) + y(x) == 0", "y", "x");
    verify("y'''(x) + y(x)*y''(x) == 0", "y", "x");
}

#[test]
fn commutator_tables() {
    check(&[
        ("lie_commutator_table([[2*x, y], [x^2, x*y], [1, 0]], [x, y])", "[[0, 2*v(2), -2*v(3)], [-2*v(2), 0, -v(1)], [2*v(3), v(1), 0]]"),
        ("symmetry_commutator([1, 0], [x^2, x*y], [x, y])", "[2*x, y]"),
        // the eight symmetries of y'' + y = 0 close (every commutator lies in their span)
        ("{ let g = lie_point_symmetries(y''(x) + y(x) == 0, y, x); every(flatten(lie_commutator_table(g, [x, y]), 1), q => not is_list(q)) }", "true"),
    ]);
}

#[test]
fn reduction_and_integration() {
    check(&[
        ("canonical_coordinates([x, y], [x, y])", "[y/x, log(x)]"),
        ("canonical_coordinates([x^2, x*y], [x, y])", "[y/x, -x^-1]"),
        // the Ermakov-Pinney equation reduced by the translation in x: x as a function of r = y, v = dx/dy
        ("{ let q = symmetry_reduce_order(y''(x) == y(x)^(-3), y, x, [1, 0], v, r); [expand(q[1][1] + v(r)^3 + r^3*v'(r)), q[2], q[3]] }", "[0, r == y, v == y'(x)^-1]"),
        ("lie_point_symmetries(y'(x) == (y(x)/x)^2 + y(x)/x, y, x, degree: 1)", "[[x, y]]"),
        ("symmetry_d_solve(y'(x) == (y(x)/x)^2 + y(x)/x, y, x)", "[[y(x) -> x/(c(1) - log(x))]]"),
    ]);
    for (eq, opt) in [("y'(x) == (y(x)/x)^2 + y(x)/x", ""), ("x*y''(x) + y'(x) == x*y'(x)^3", ", generator: [0, 1]")] {
        let src = format!("every(symmetry_d_solve({eq}, y, x{opt}), s => dg_check_solution({eq}, y, x, s))");
        assert_eq!(ev(&src), "true", "{eq}");
    }
}

#[test]
fn pde_symmetries_and_reductions() {
    let kdv = "derivative(0, 1)(u)(x, t) + u(x, t)*derivative(1, 0)(u)(x, t) + derivative(3, 0)(u)(x, t) == 0";
    let heat = "derivative(0, 1)(u)(x, t) == derivative(2, 0)(u)(x, t)";
    let burgers = "derivative(0, 1)(u)(x, t) + u(x, t)*derivative(1, 0)(u)(x, t) == derivative(2, 0)(u)(x, t)";
    let cases = [
        // Olver, Example 2.44 (KdV, four generators) and 2.41 (heat equation, six plus the linear superposition)
        (format!("lie_point_symmetries({kdv}, u, [x, t])"), "[[0, 1, 0], [(1/3)*x, t, (-2/3)*u], [1, 0, 0], [t, 0, 1]]"),
        (format!("length(lie_point_symmetries({heat}, u, [x, t]))"), "7"),
        (format!("lie_point_symmetries({burgers}, u, [x, t])"), "[[1, 0, 0], [t, 0, 1], [-x, -2*t, u], [-1*t*x, -t^2, -x + t*u], [0, 1, 0]]"),
        // travelling waves and the scaling reduction of KdV
        (format!("similarity_reduction({kdv}, u, [x, t], [1, 1, 0], f, z)[2:3]"), "[u(x, t) -> f(x - t), z -> x - t]"),
        (format!("expand(similarity_reduction({kdv}, u, [x, t], [1, 1, 0], f, z)[1][1] - (-f'(z) + f(z)*f'(z) + f'''(z)))"), "0"),
        (format!("expand(similarity_reduction({kdv}, u, [x, t], [x, 3*t, -2*u], f, z)[1][1] - (-3*f'''(z) + 2*f(z) + z*f'(z) - 3*f(z)*f'(z)))"), "0"),
        (format!("expand(similarity_reduction({heat}, u, [x, t], [x, 2*t, 0], f, z)[1][1] + 2*f''(z) + z*f'(z))"), "0"),
        (format!("expand(similarity_reduction({burgers}, u, [x, t], [t, 0, 1], f, z)[1][1] - f(z) - z*f'(z))"), "0"),
    ];
    for (src, want) in &cases {
        assert_eq!(ev(src), *want, "{src}");
    }
    verify(kdv, "u", "[x, t]");
    verify(heat, "u", "[x, t]");
    verify(burgers, "u", "[x, t]");
}

#[test]
fn conservation_laws_by_multipliers() {
    let kdv = "derivative(0, 1)(u)(x, t) + u(x, t)*derivative(1, 0)(u)(x, t) + derivative(3, 0)(u)(x, t) == 0";
    let cases = [
        // the first conservation laws of KdV (Olver, Example 4.36; Bluman, Cheviakov and Anco, 2010, 1.3)
        (format!("conservation_law_multipliers({kdv}, u, [x, t])"), "[u(x, t), -x + t*u(x, t), 1]"),
        (format!("length(conservation_law_multipliers({kdv}, u, [x, t], order: 2))"), "4"),
        (format!("conservation_laws({kdv}, u, [x, t])[3]"), "[1, u(x, t), (1/2)*u(x, t)^2 + derivative(2, 0)(u)(x, t)]"),
        // the multipliers of the heat equation are the solutions of the backward heat equation
        (
            "conservation_law_multipliers(derivative(0, 1)(u)(x, t) == derivative(2, 0)(u)(x, t), u, [x, t])".to_string(),
            "[conditional_expression(c(1)(x, t), -derivative(0, 1)(c(1))(x, t) - derivative(2, 0)(c(1))(x, t) == 0)]",
        ),
        ("expand(conservation_laws(y''(x) + y(x) == 0, y, x) - [[cos(x), cos(x)*y'(x) + sin(x)*y(x)], [sin(x), sin(x)*y'(x) - cos(x)*y(x)]])".to_string(), "[[0, 0], [0, 0]]"),
        // the energy of the anharmonic oscillator
        ("conservation_laws(y''(x) + y(x)^3 == 0, y, x, order: 1, degree: 1)".to_string(), "[[y'(x), (1/4)*y(x)^4 + (1/2)*y'(x)^2]]"),
        // every law holds on the KdV soliton 3 sech((x - t)/2)^2 (an independent route)
        (
            format!("{{ let sol = 3*sech((x - t)/2)^2; every(conservation_laws({kdv}, u, [x, t], order: 2), l => abs(n(rewrite(rewrite(d(l[2], t) + d(l[3], x), u -> dg_lambda([x, t], sol)), [x -> 0.3, t -> 0.7]))) < 1.0e-12) }}"),
            "true",
        ),
    ];
    for (src, want) in &cases {
        assert_eq!(ev(src), *want, "{src}");
    }
}

#[test]
fn differential_polynomials() {
    check(&[
        ("differential_leader(derivative(1, 0)(u)(x, t)^2 - 4*u(x, t)*derivative(0, 1)(u)(x, t), u, [x, t])", "derivative(1, 0)(u)(x, t)"),
        ("differential_leader(y'(x) + z''(x)*y(x), [y, z], x)", "z''(x)"),
        ("differential_leader(y'(x) + z''(x)*y(x), [y, z], x, ranking: elimination)", "y'(x)"),
        ("differential_initial(x*y(x)*y'(x)^2 + y(x)^3, y, x)", "x*y(x)"),
        ("differential_separant(y'(x)^2 - 4*y(x), y, x)", "2*y'(x)"),
        ("differential_reduce(y''(x), [y'(x)^2 - 4*y(x)], y, x)", "y'(x)"),
        ("differential_reduce(d(y'(x)^2 - 4*y(x), x, x), [y'(x)^2 - 4*y(x)], y, x)", "0"),
        ("delta_polynomial(derivative(1, 0)(u)(x, t) - u(x, t), derivative(0, 1)(u)(x, t) - x*u(x, t), u, [x, t])",
         "-derivative(0, 1)(u)(x, t) + x*derivative(1, 0)(u)(x, t) + u(x, t)"),
    ]);
}

#[test]
fn decompositions() {
    check(&[
        // Boulier, Lazard, Ollivier and Petitot (1995): the general and the singular component of y'^2 = 4 y
        ("rosenfeld_groebner(y'(x)^2 - 4*y(x), y, x)", "[[[-4*y(x) + y'(x)^2 == 0], [y'(x) != 0]], [[y(x) == 0], []]]"),
        ("rosenfeld_groebner([derivative(1, 0)(u)(x, t)^2 - 4*u(x, t), derivative(0, 1)(u)(x, t) - u(x, t)], u, [x, t])", "[[[u(x, t) == 0], []]]"),
        ("rosenfeld_groebner([derivative(1, 0)(u)(x, t) - u(x, t), derivative(0, 1)(u)(x, t) - x*u(x, t)], u, [x, t])", "[[[u(x, t) == 0], []]]"),
        ("rosenfeld_groebner([y'(x) - z(x), z'(x) + y(x)], [y, z], x, ranking: elimination)", "[[[z(x) + z''(x) == 0, y(x) + z'(x) == 0], []]]"),
        ("thomas_decomposition(y'(x)^2 - 4*y(x), y, x)", "[[[-4*y(x) + y'(x)^2 == 0], [y(x) != 0]], [[y(x) == 0], []]]"),
        ("thomas_decomposition(y(x)*y'(x) - 1, y, x)", "[[[-1 + y(x)*y'(x) == 0], [y(x) != 0]]]"),
        ("thomas_decomposition(y'(x)^2 - y(x)^2, y, x)", "[[[-y(x)^2 + y'(x)^2 == 0], [y(x) != 0]], [[y(x) == 0], []]]"),
    ]);
}

#[test]
fn painleve() {
    check(&[
        // Ablowitz, Ramani and Segur (1980): P-I and P-II pass, y'' = 6 y^2 + a x^2 needs a = 0, y'' = y^4 branches
        ("painleve_test(y''(x) == 6*y(x)^2 + x, y, x)", "[[leading_exponent -> -2, leading_coefficient -> 1, resonances -> [-1, 6], conditions -> [], passes -> true]]"),
        ("painleve_test(y''(x) == 6*y(x)^2 + a*x^2, y, x)", "[[leading_exponent -> -2, leading_coefficient -> 1, resonances -> [-1, 6], conditions -> [-a], passes -> false]]"),
        ("map(b => [leading_coefficient, resonances, passes] /. b, painleve_test(y''(x) == 2*y(x)^3 + x*y(x) + alpha, y, x))", "[[-1, [-1, 4], true], [1, [-1, 4], true]]"),
        ("map(b => [leading_exponent, passes] /. b, painleve_test(y''(x) == y(x)^4, y, x))", "[[-2/3, false], [-2/3, false], [-2/3, false]]"),
        // the Chazy equation: resonances -1, -2, -3
        ("painleve_test(y'''(x) == 2*y(x)*y''(x) - 3*y'(x)^2, y, x)", "[[leading_exponent -> -1, leading_coefficient -> -6, resonances -> [-3, -2, -1], conditions -> [], passes -> true]]"),
        // the Duffing oscillator (elliptic solutions)
        ("map(b => passes /. b, painleve_test(y''(x) + y(x) + y(x)^3 == 0, y, x))", "[true, true]"),
    ]);
}
