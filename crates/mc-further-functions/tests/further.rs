//! Tests of the round 30 coverage library, evaluated on the assembled core.
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

#[test]
fn julia_counts() {
    check(&[
        ("julia_set_iteration_count(z^2 - 1, z, [0, 0.1 - 0.2*im, 0.5 + 0.5*im, 1 + im, 2 + 2*im])", "[0, 4, 5, 2, 1]"),
        ("julia_set_iteration_count(-1, 0.1 - 0.2*im)", "4"),
        ("julia_set_iteration_count(z^3 - z - 1, z, 0, max_iterations: 20)", "21"),
        ("julia_set_iteration_count(z^2/(z^2 - 1), z, [0, 1, 2, 3, 4, 7])", "[0, 1, 10, 6, 4, 4]"),
        // the attracting fixed point (sqrt(5) - 1)/2 of z^3 - z + 1
        ("julia_set_iteration_count(z^3 - z + 1, z, [-2, 1/4, im, 1/pi])", "[1, 3, 2, 3]"),
    ]);
}

#[test]
fn meijer_and_fox_reductions() {
    check(&[
        ("meijer_g_reduce(sin(x), x)", "pi^(1/2)*inactive(meijer_g)([[], []], [[1/2], [0]], (1/2)*x, 1/2)"),
        ("meijer_g_reduce(1/(2*x + 3), x)", "(1/3)*inactive(meijer_g)([[0], []], [[0], []], (2/3)*x)"),
        ("meijer_g_reduce(unit_box(3*x), x)", "inactive(meijer_g)([[], [1]], [[0], []], 6*x)"),
        ("fox_h_reduce(x^b*exp(-x), x)", "inactive(fox_h)([[], []], [[[b, 1]], []], x)"),
        ("fox_h_reduce(bessel_j(1, a*x), x)", "(1/2)*inactive(fox_h)([[], []], [[[1/2, 1/2]], [[-1/2, 1/2]]], (1/2)*a*x)"),
        ("activate(meijer_g_reduce(1/(1 + x^2), x))", "(1 + x^2)^-1"),
        ("meijer_g_reduce(f(x), x)", "meijer_g_reduce(f(x), x)"),
    ]);
}

#[test]
fn fox_h_values() {
    check(&[
        ("fox_h([[[0, 1]], []], [[[0, 1]], [[0, 1]]], 5)", "exp(-5)"),
        ("fox_h([[], []], [[[b, 1/2]], []], z)", "2*exp(-z^2)*z^(2*b)"),
        ("fox_h([[[1/2, 1]], []], [[[0, 1]], []], 0)", "pi^(1/2)"),
        ("fox_h([[[0, 1]], []], [[[0, 1]], [[0, 1/2]]], 0)", "1"),
        ("fox_h([[[a1, p1]], [[a2, p2], [a3, p3]]], [[[b1, q1], [a2, p2]], [[a1, p1], [b2, q2]]], z)", "fox_h([[], [[a3, p3]]], [[[b1, q1]], [[b2, q2]]], z)"),
        // the residue series (mu < 0, poles of the a's), machine and 30 digits; the expected value is
        // 0.27964621202571990071906197587122561401...
        ("abs(fox_h([[[1/10, 6/5], [13/10, 1]], [[17/5, 2]]], [[[7/5, 2]], [[1/5, 1]]], 0.2) - 0.2796462120257199) < 1.0e-14", "true"),
        ("abs(n(fox_h([[[1/10, 6/5], [13/10, 1]], [[17/5, 2]]], [[[7/5, 2]], [[1/5, 1]]], 1/5), 30) - 0.279646212025719900719061975871225614) < 10^-29", "true"),
        // equal scales against the Meijer G value: H(z | (a, 1/2)) = 2 G(z^2 | a)
        ("abs(fox_h([[[1/3, 1/2]], []], [[[1/4, 1/2], [0, 1/2]], []], 0.7) - 2*meijer_g([[1/3], []], [[1/4, 0], []], 0.49)) < 1.0e-12", "true"),
        // double poles (s = -k and s = -2 - 2l meet): the machine perturbation against the ball one
        ("abs(fox_h([[], []], [[[0, 1], [1, 1/2]], []], 0.3) - n(fox_h([[], []], [[[0, 1], [1, 1/2]], []], 3/10), 20)) < 1.0e-10", "true"),
    ]);
}

#[test]
fn spheroidal_radial() {
    check(&[
        // expected values: 0.5845072730036112, 0.39490832384781643, -3.9347055658212745e-12 im
        ("abs(spheroidal_s1(1, 0, 2.5, 1) - 0.5845072730036112) < 1.0e-14", "true"),
        ("abs(spheroidal_s1(1, 0, 1.5, 2) - 0.39490832384781643) < 1.0e-14", "true"),
        ("abs(spheroidal_s1(5, 1, 0.1, 0.3) + 3.9347055658212745e-12*im) < 1.0e-24", "true"),
        ("abs(n(spheroidal_s1(7, 5, 1, 2), 40) - 0.0000273898305176768507803827524416835523714104) < 10^-44", "true"),
        ("spheroidal_s1(1, 0, 0.2, 0)", "complex(0.0, 0.0)"),
        ("spheroidal_s1(1, 0, 1/5, x)", "spheroidal_s1(1, 0, 1/5, x)"),
    ]);
}

#[test]
fn trig_factoring() {
    check(&[
        ("trig_factor(sin(x)^2 + tan(x)^2)", "(1 + cos(x)^2)*cos(x)^-2*sin(x)^2"),
        ("trig_factor(sin(x)^2 - cos(x)^2)", "2*sin(x + (-1/4)*pi)*sin(x + (1/4)*pi)"),
        ("trig_factor(cos(x + y) + sin(x)*sin(y))", "cos(x)*cos(y)"),
        ("trig_factor([sin(x) + cos(x), q])", "[2^(1/2)*sin(x + (1/4)*pi), q]"),
        ("simplify(trig_factor(sin(2*x) + 2*cos(x)) - (sin(2*x) + 2*cos(x)))", "0"),
    ]);
}

#[test]
fn inertia() {
    check(&[
        ("moment_of_inertia(ball(), [1, 1, 0], [0, 0, 1])", "(16/5)*pi"),
        ("moment_of_inertia(ball([0, 0, 0], r), [0, 0, 0], [1, 1, 1])", "(8/15)*pi*r^5"),
        ("moment_of_inertia(implicit_region(x^2 + y^2 <= 1, [x, y]))", "[[(1/4)*pi, 0], [0, (1/4)*pi]]"),
        ("moment_of_inertia(implicit_region(x^2 + y^2 <= 1, [x, y, [z, 0, 2]]), [1, 1, 1], [1, 1, 1])", "(22/9)*pi"),
    ]);
}

#[test]
fn field_discriminants() {
    check(&[
        ("number_field_discriminant([sqrt(2) + sqrt(3), sqrt(3 + sqrt(2)), sqrt(5), 2^(1/3), 2^(1/4)])", "[2304, 7168, 5, -108, -2048]"),
        ("number_field_discriminant(root(x => x^3 + x + 1, 3))", "-31"),
        ("number_field_discriminant(root(x => 2*x^3 - x + 1, 2))", "-4"),
    ]);
}

#[test]
fn algebraic_numbers() {
    check(&[
        ("algebraic_number(3^(1/5), [1, 2, 1, 3, 3, 1])", "algebraic_number(root(&(-3 + &1^5), 1), [4, 2, 1, 3, 3])"),
        ("algebraic_number((1 + im)/2, [1, 3])", "algebraic_number(complex(1, 1), [1, 3/2])"),
        ("algebraic_number(sqrt(2) + sqrt(5), [1, 1/2])", "algebraic_number(root(&(9 + -14*&1^2 + &1^4), 4), [1, 1/2, 0, 0])"),
        ("[algebraic_number(3, [1, 2]), algebraic_number(sqrt(3), [5, 0])]", "[7, 5]"),
    ]);
}

#[test]
fn sums_of_squares() {
    check(&[
        ("polynomial_sum_of_squares_list(x^2 + x*y + y^2, [x, y])", "[y + (1/2)*x, (1/2)*x*3^(1/2)]"),
        ("expand(x^2 + 2*x + 5 - total(map(q => q^2, polynomial_sum_of_squares_list(x^2 + 2*x + 5, [x]))))", "0"),
        ("polynomial_sum_of_squares_list(x^2 - y^2, [x, y])", "polynomial_sum_of_squares_list(-y^2 + x^2, [x, y])"),
    ]);
}

#[test]
fn inverse_radon() {
    check(&[
        ("inverse_radon_transform(e^(-p^2), [p, f], [x, y]) === exp(-x^2 - y^2)/sqrt(pi)", "true"),
        ("inverse_radon_transform((1 + 2*p^2)/e^(p^2), [p, f], [x, y]) === 2*(x^2 + y^2)*exp(-x^2 - y^2)/sqrt(pi)", "true"),
        ("inverse_radon_transform(dirac_delta(p - a*cos(f) - b*sin(f)), [p, f], [x, y])", "dirac_delta(x - a, y - b)"),
    ]);
}

#[test]
fn complete_integrals() {
    check(&[
        ("complete_integral(d(u(x, y), x) - d(u(x, y), y)^2 == cos(x), u(x, y), [x, y]) === [[u(x, y) -> c(1)*y + c(1)^2*x + c(2) + sin(x)]]", "true"),
        ("rewrite(d(u(x, y), x)*d(u(x, y), y) - 1, complete_integral(d(u(x, y), x)*d(u(x, y), y) == 1, u, [x, y])[1])", "0"),
    ]);
}
