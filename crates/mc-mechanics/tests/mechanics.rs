//! Tests of the analytical mechanics toolkit, evaluated on the assembled core. Expected
//! values are the textbook equations (Goldstein, Poole and Safko, *Classical Mechanics*, 3rd ed.; Landau and
//! Lifshitz, *Mechanics*, 3rd ed.) and internal consistency checks (Hamilton's equations against the
//! Euler-Lagrange equations, conservation of Noether charges on solutions, energy along numerical solutions).
use mc_api::{Engine, StopFlag};

fn ev(src: &str) -> String {
    // the display printer, as the notebook and the documentation pages show results
    let mut e = mc_engine::CoreEngine::with_config(mc_engine::EngineConfig { display: true, ..mc_engine::EngineConfig::default() });
    let stop = StopFlag::new();
    e.eval(src, &stop).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn check(cases: &[(&str, &str)]) {
    for (src, want) in cases {
        assert_eq!(ev(src), *want, "{src}");
    }
}

#[test]
fn euler_lagrange_point_systems() {
    check(&[
        // harmonic oscillator, pendulum (Goldstein 1.4)
        ("euler_lagrange_equations(m*x'(t)^2/2 - k*x(t)^2/2, x(t), t)", "[-k*x(t) - m*x''(t) == 0]"),
        ("euler_lagrange_equations(m*l^2*th'(t)^2/2 + m*g*l*cos(th(t)), th, t)", "[-m*l^2*th''(t) - g*l*m*sin(th(t)) == 0]"),
        // central force in polar coordinates: the radial equation and the conserved angular momentum
        (
            "euler_lagrange_equations(m*(r'(t)^2 + r(t)^2*th'(t)^2)/2 - V(r(t)), [r, th], t)",
            "[-V'(r(t)) - m*r''(t) + m*th'(t)^2*r(t) == 0, -m*r(t)^2*th''(t) - 2*m*r(t)*r'(t)*th'(t) == 0]",
        ),
        // explicit time dependence
        ("euler_lagrange_equations(x'(t)^2/2 - t*x(t), x, t)", "[-t - x''(t) == 0]"),
        // higher derivatives: the Ostrogradsky form, and a Lagrangian equivalent to the oscillator up to a total derivative
        ("euler_lagrange_equations(x''(t)^2, x, t)", "[2*x''''(t) == 0]"),
        ("euler_lagrange_equations(-m*x(t)*x''(t)/2 - k*x(t)^2/2, x, t)", "[-k*x(t) - m*x''(t) == 0]"),
        // a charge in a magnetic field: the gauge term gives the Lorentz force
        (
            "euler_lagrange_equations(m*(x'(t)^2 + y'(t)^2)/2 + q*b*(x(t)*y'(t) - y(t)*x'(t))/2, [x, y], t)",
            "[-m*x''(t) + b*q*y'(t) == 0, -m*y''(t) - b*q*x'(t) == 0]",
        ),
    ]);
}

#[test]
fn euler_lagrange_fields() {
    check(&[
        // the wave equation and the Klein-Gordon equation
        (
            "euler_lagrange_equations(d(u(x, t), t)^2/2 - c^2*d(u(x, t), x)^2/2, u, [x, t])",
            "[-derivative(0, 2)(u)(x, t) + c^2*derivative(2, 0)(u)(x, t) == 0]",
        ),
        (
            "euler_lagrange_equations(d(f(x, t), t)^2/2 - d(f(x, t), x)^2/2 - mu^2*f(x, t)^2/2, f, [x, t])",
            "[-derivative(0, 2)(f)(x, t) - f(x, t)*mu^2 + derivative(2, 0)(f)(x, t) == 0]",
        ),
        // second derivatives of a field: the biharmonic (plate) equation
        (
            "euler_lagrange_equations((d(w(x, y), x, x) + d(w(x, y), y, y))^2/2, w, [x, y])",
            "[2*derivative(2, 2)(w)(x, y) + derivative(0, 4)(w)(x, y) + derivative(4, 0)(w)(x, y) == 0]",
        ),
    ]);
}

#[test]
fn constraints_with_multipliers() {
    check(&[
        // the spherical (here plane) pendulum in Cartesian coordinates
        (
            "euler_lagrange_equations(m*(x'(t)^2 + y'(t)^2)/2 - m*g*y(t), [x, y], t, constraints: [x(t)^2 + y(t)^2 == l^2])",
            "[-m*x''(t) + 2*x(t)*lambda(1)(t) == 0, -g*m - m*y''(t) + 2*y(t)*lambda(1)(t) == 0, -l^2 + x(t)^2 + y(t)^2 == 0]",
        ),
        // a constraint in the velocities (rolling without slipping) with a named multiplier
        (
            "euler_lagrange_equations(m*x'(t)^2/2 + j*th'(t)^2/2, [x, th], t, constraints: [x'(t) - a*th'(t)], multipliers: [mu])",
            "[mu(t) - m*x''(t) == 0, -a*mu(t) - j*th''(t) == 0, -a*th'(t) + x'(t) == 0]",
        ),
    ]);
}

#[test]
fn hamiltonian_mechanics() {
    check(&[
        ("generalized_momenta(m*x'(t)^2/2 - k*x(t)^2/2, x, t)", "[m*x'(t)]"),
        ("energy_function(m*x'(t)^2/2 - k*x(t)^2/2, x, t)", "(1/2)*k*x(t)^2 + (1/2)*m*x'(t)^2"),
        ("legendre_transform(m*x'(t)^2/2 - k*x(t)^2/2, x, p, t)", "(1/2)*k*x(t)^2 + p^2/(2*m)"),
        (
            "legendre_transform(m*(r'(t)^2 + r(t)^2*th'(t)^2)/2 - V(r(t)), [r, th], [pr, pth], t)",
            "V(r(t)) + pr^2/(2*m) + pth^2/(2*m*r(t)^2)",
        ),
        // the minimal coupling Hamiltonian (p - e A)^2/(2 m)
        (
            "legendre_transform(m*x'(t)^2/2 + e0*x'(t)*A(x(t)), x, p, t)",
            "p^2/(2*m) + e0^2*A(x(t))^2/(2*m) - e0*p*A(x(t))/m",
        ),
        // a singular Lagrangian has no Legendre transform
        ("legendre_transform(x'(t)*y(t), [x, y], [p1, p2], t)", "legendre_transform(y(t)*x'(t), [x, y], [p1, p2], t)"),
        ("legendre_transform(x^2/2, x, p)", "(1/2)*p^2"),
        ("legendre_transform(exp(x), x, p)", "-p + p*log(p)"),
        ("hamilton_equations(p^2/(2*m) + k*x^2/2, x, p, t)", "[x'(t) == p(t)/m, p'(t) == -k*x(t)]"),
        (
            "hamilton_equations(legendre_transform(m*(r'(t)^2 + r(t)^2*th'(t)^2)/2 - V(r(t)), [r, th], [pr, pth], t), [r, th], [pr, pth], t)",
            "[r'(t) == pr(t)/m, th'(t) == pth(t)/(m*r(t)^2), pr'(t) == -V'(r(t)) + pth(t)^2/(m*r(t)^3), pth'(t) == 0]",
        ),
        ("poisson_bracket(x, p, x, p)", "1"),
        // angular momentum: {L_x, L_y} = L_z
        ("poisson_bracket(y*pz - z*py, z*px - x*pz, [x, y, z], [px, py, pz])", "py*x - px*y"),
        ("poisson_bracket(p^2/(2*m) + k*x^2/2, x, x, p)", "-p/m"),
        // the Jacobi identity on three polynomials
        (
            "{ let pb = (f, g) => poisson_bracket(f, g, [x, y], [p, q]); let a = x^2*q; let b = p*y + x; let c = p^2*q - y^3; expand(pb(a, pb(b, c)) + pb(b, pb(c, a)) + pb(c, pb(a, b))) }",
            "0",
        ),
    ]);
}

#[test]
fn noether_charges() {
    check(&[
        // rotations: the angular momentum; time translation: minus the energy (Q = sum p (eta - q' xi) + L xi - F)
        ("noether_charge(m*(r'(t)^2 + r(t)^2*th'(t)^2)/2 - V(r(t)), [r, th], t, [0, 1])", "m*r(t)^2*th'(t)"),
        (
            "noether_charge(m*(r'(t)^2 + r(t)^2*th'(t)^2)/2 - V(r(t)), [r, th], t, [0, 0], time_shift: 1)",
            "-V(r(t)) - (1/2)*m*r'(t)^2 - (1/2)*m*r(t)^2*th'(t)^2",
        ),
        ("noether_charge(m*(x'(t)^2 + y'(t)^2)/2 - k*(x(t)^2 + y(t)^2)/2, [x, y], t, [-y(t), x(t)])", "m*x(t)*y'(t) - m*y(t)*x'(t)"),
        // a Galilean boost changes L by a total derivative (gauge term)
        ("noether_charge(m*(x'(t)^2 + y'(t)^2)/2 - g*m*y(t), [x, y], t, [t, 0], gauge: m*x(t))", "-m*x(t) + m*t*x'(t)"),
        // the dilatation of conformal mechanics (V = 1/x^2): t -> (1 + eps) t, x -> (1 + eps/2) x
        ("noether_charge(x'(t)^2/2 - 1/x(t)^2, x, t, [x(t)/2], time_shift: t)", "-t/x(t)^2 - (1/2)*t*x'(t)^2 + (1/2)*x(t)*x'(t)"),
        // ... conserved on the solutions: dQ/dt with x'' = 2/x^3 vanishes
        ("simplify(rewrite(d(noether_charge(x'(t)^2/2 - 1/x(t)^2, x, t, [x(t)/2], time_shift: t), t), x''(t) -> 2/x(t)^3))", "0"),
        // gravity breaks the vertical translation
        ("is_failure(noether_charge(m*(x'(t)^2 + y'(t)^2)/2 - g*m*y(t), [x, y], t, [0, 1]))", "true"),
    ]);
}

#[test]
fn equilibria_and_normal_modes() {
    check(&[
        (
            "equilibrium_points(m*l^2*th'(t)^2/2 + m*g*l*cos(th(t)), th, t)",
            "[[th -> conditional_expression(2*pi*c(1), element(c(1), integers))], [th -> conditional_expression(pi + 2*pi*c(1), element(c(1), integers))]]",
        ),
        ("equilibrium_points(x'(t)^2/2 + x(t)^2/2 - x(t)^4/4, x, t)", "[[x -> -1], [x -> 0], [x -> 1]]"),
        // two equal masses between three equal springs (Goldstein 6.4): sqrt(k/m) and sqrt(3 k/m)
        (
            "normal_modes(m*(x1'(t)^2 + x2'(t)^2)/2 - k*x1(t)^2/2 - k*(x2(t) - x1(t))^2/2 - k*x2(t)^2/2, [x1, x2], t)",
            "[[(k/m)^(1/2), 3^(1/2)*(k/m)^(1/2)], [[1, 1], [-1, 1]]]",
        ),
        // the modes solve (K - w^2 M) v = 0
        (
            "{ let Lc = m*(x1'(t)^2 + x2'(t)^2)/2 - k*x1(t)^2/2 - k*(x2(t) - x1(t))^2/2 - k*x2(t)^2/2; let nm = normal_modes(Lc, [x1, x2], t); let h = m31_hessians(Lc, [x1(t), x2(t)], [0, 0]); map(i => simplify(dot(h[2] - nm[1][i]^2*h[1], nm[2][i])), [1, 2]) }",
            "[[0, 0], [0, 0]]",
        ),
        // the double pendulum of equal masses and lengths: w^2 = (2 -+ sqrt(2)) g/l
        (
            "normal_modes(m*l^2*(2*a'(t)^2 + b'(t)^2 + 2*a'(t)*b'(t)*cos(a(t) - b(t)))/2 + m*g*l*(2*cos(a(t)) + cos(b(t))), [a, b], t)",
            "[[(-g*(-2 + 2^(1/2))/l)^(1/2), (g*(2 + 2^(1/2))/l)^(1/2)], [[(1/2)*2^(1/2), 1], [(-1/2)*2^(1/2), 1]]]",
        ),
        // about another equilibrium, and a degenerate pair
        ("normal_modes(x'(t)^2/2 + x(t)^2/2 - x(t)^4/4, x, t, at: [x -> 1])", "[[2^(1/2)], [[1]]]"),
        ("normal_modes(m*(x'(t)^2 + y'(t)^2)/2 - k*(x(t)^2 + y(t)^2)/2, [x, y], t)", "[[(k/m)^(1/2), (k/m)^(1/2)], [[0, 1], [1, 0]]]"),
        // a charge in a magnetic field and a harmonic trap: w = (sqrt(b^2 + 4 k m) -+ b)/(2 m) = 1, 2, circular modes
        (
            "normal_modes(x'(t)^2/2 + y'(t)^2/2 + (x(t)*y'(t) - y(t)*x'(t))/2 - (x(t)^2 + y(t)^2), [x, y], t)",
            "[[1, 2], [[im, 1], [-im, 1]]]",
        ),
    ]);
}

#[test]
fn phase_portraits_and_numerical_motion() {
    check(&[
        ("head(phase_portrait(th'(t)^2/2 + cos(th(t)), th, t, [x, -4, 4], [v, -3, 3]))", "image"),
        ("head(phase_portrait(th'(t)^2/2 + cos(th(t)), th, t, [x, -4, 4], [v, -3, 3], plot_points: 10))", "image"),
        // the pendulum: the energy is kept to about 1e-7
        (
            "{ let lp = th'(t)^2/2 + cos(th(t)); let s = n_dsolve_motion(lp, th, [t, 0, 20], 1, 0); energy_drift(lp, th, s, [t, 0, 20]) < 1.0e-6 }",
            "true",
        ),
        // the oscillator against its closed form cos(t)
        ("{ let s = n_dsolve_motion(x'(t)^2/2 - x(t)^2/2, x, [t, 0, 10], 1, 0); abs(rewrite(x(10.0), s) - cos(10.0)) < 1.0e-6 }", "true"),
        // the double pendulum (coupled accelerations) and Kepler's problem: energy and angular momentum
        (
            "{ let ld = (2*a'(t)^2 + b'(t)^2 + 2*a'(t)*b'(t)*cos(a(t) - b(t)))/2 + 9.81*(2*cos(a(t)) + cos(b(t))); let s = n_dsolve_motion(ld, [a, b], [t, 0, 10], [1, 0.5], [0, 0]); energy_drift(ld, [a, b], s, [t, 0, 10]) < 1.0e-5 }",
            "true",
        ),
        (
            "{ let lk = (r'(t)^2 + r(t)^2*f'(t)^2)/2 + 1/r(t); let s = n_dsolve_motion(lk, [r, f], [t, 0, 30], [1, 0], [0, 1.2]); [energy_drift(lk, [r, f], s, [t, 0, 30]) < 1.0e-6, abs(rewrite(r(30.0)^2*f'(30.0), s) - 1.2) < 1.0e-6] }",
            "[true, true]",
        ),
    ]);
}

#[test]
fn user_values_do_not_reach_the_library() {
    // global values of names the library uses for its parameters and helpers
    check(&[(
        "L = 3; q = 4; c = 5; vs = 6; al = 7; g = 9; k = 10; r = 11; s = 12; h = 13; euler_lagrange_equations(m*x'(u)^2/2 - w*x(u)^2/2, x(u), u)",
        "[-m*x''(u) - w*x(u) == 0]",
    )]);
}

#[test]
fn radicals_are_simplified() {
    check(&[
        // the free relativistic particle: energy m/sqrt(1 - v^2); the shortest path: zero curvature
        ("energy_function(-m*sqrt(1 - x'(t)^2), x, t)", "m/(1 - x'(t)^2)^(1/2)"),
        ("euler_lagrange_equations(sqrt(1 + y'(x)^2), y, x)", "[-y''(x)/(1 + y'(x)^2)^(3/2) == 0]"),
    ]);
}
