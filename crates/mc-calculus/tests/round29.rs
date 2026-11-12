//! Round 29: the derivative rows of the round 28 special functions and the functions of
//! `lib/calculus.mc`.
mod common;
use common::check;

/// Derivative rows of the Scorer, Weierstrass, Coulomb, Lommel, Mathieu and Appell functions (each checked against
/// central differences when it was written, see NOTES.md).
#[test]
fn special_derivatives() {
    check(&[
        ("d(scorer_gi(x), x)", "scorer_gi_prime(x)"),
        ("d(scorer_gi(x), x, x)", "-pi^-1 + x*scorer_gi(x)"),
        ("d(scorer_hi_prime(x), x)", "x*scorer_hi(x) + pi^-1"),
        ("d(weierstrass_p(u, [g2, g3]), [u, 3])", "12*weierstrass_p(u, [g2, g3])*weierstrass_p_prime(u, [g2, g3])"),
        ("d(weierstrass_zeta(u, [g2, g3]), u)", "-weierstrass_p(u, [g2, g3])"),
        ("d(weierstrass_sigma(u, [g2, g3]), u)", "weierstrass_sigma(u, [g2, g3])*weierstrass_zeta(u, [g2, g3])"),
        ("d(appell_f4(a, b, c1, c2, x, y), x)", "a*b*appell_f4(1 + a, 1 + b, 1 + c1, c2, x, y)/c1"),
        ("d(appell_f1(a, b1, b2, c, x, y), y)", "a*b2*appell_f1(1 + a, b1, 1 + b2, 1 + c, x, y)/c"),
        ("d(mathieu_s(a, q, z), z)", "mathieu_s_prime(a, q, z)"),
        ("d(lommel_t1(5/2, 3/2, z), z)", "3*lommel_t1(3/2, 1/2, z) + (-3/2)*lommel_t1(5/2, 3/2, z)/z"),
        ("d(lommel_t1(1/3, 2/3, z), z)", "(-4/3)*lommel_t1(-2/3, 5/3, z) + (2/3)*lommel_t1(1/3, 2/3, z)/z"),
        ("d(lommel_t1(1, 0, z), z)", "bessel_i(1, z)"),
        (
            "d(coulomb_f(l, eta, r), r)",
            "(eta/(1 + l) + (1 + l)/r)*coulomb_f(l, eta, r) + -1*coulomb_f(1 + l, eta, r)*(1 + eta^2*(1 + l)^-2)^(1/2)",
        ),
        // numerical agreement with a central difference
        (
            "abs(n(rewrite(d(coulomb_g(0, 1/3, z), z), z -> 3/2), 20) - n((coulomb_g(0, 1/3, 3/2 + 10^-12) - coulomb_g(0, 1/3, 3/2 - 10^-12))/(2*10^-12), 20)) < 10^-9",
            "true",
        ),
    ]);
}

/// Between, the rational and Boolean domains, the order term and numerical roots.
#[test]
fn domains_between_order() {
    check(&[
        ("between(3, [2, 5])", "true"),
        ("select([5, 10, 15, 20, 25], between([10, 20]))", "[10, 15, 20]"),
        ("between(2, [e, pi])", "false"),
        ("between(interval([3, 4]), centered_interval(5, 3))", "true"),
        ("between(6, [[2, 5], [8, 12]])", "false"),
        ("element(true, booleans)", "true"),
        ("element(0, booleans)", "false"),
        ("element(2/3, nonnegative_rationals)", "true"),
        ("element(-1/2, nonnegative_rationals)", "false"),
        ("element(0, nonpositive_rationals)", "true"),
        ("reduce((2*x^2 - 1)*(4*x^2 - 1) == 0, x, nonnegative_rationals)", "x == 1/2"),
        ("normal(x + x^2/2 + o(x)^3 + x^5) - x - x^2/2", "0"),
        ("length(apply(list, n_roots((x - 1)*(x - 2)^2 == 0, x)))", "3"),
    ]);
}

/// Lyapunov, Sylvester and Riccati equations: the residuals vanish.
#[test]
fn matrix_equations() {
    check(&[
        ("lyapunov_solve([[1, 3], [2, 1]], [[-2, -1], [-1, -4]])", "[[11/10, -7/10], [-7/10, -3/5]]"),
        ("discrete_lyapunov_solve([[3, 2], [1, 1]], [[0, 2], [1, 2]], [[2, 2], [0, 1]])", "[[-6/11, 2/11], [7/11, 5/11]]"),
        ("discrete_lyapunov_solve([[-1/2, 0], [0, 1/2]], [[1, 0], [0, 1]])", "[[-4/3, 0], [0, -4/3]]"),
        (
            "{ let a = [[-3, 2], [1, 1]]; let b = [[0], [1]]; let x = riccati_solve([a, b], [[[1, -1], [-1, 1]], [[3]]]); max(map(abs, flatten(dot(transpose(a), x) + dot(x, a) - dot(x, b, [[1/3]], transpose(b), x) + [[1, -1], [-1, 1]]))) < 10^-10 }",
            "true",
        ),
        (
            "{ let x = discrete_riccati_solve([[[0, 2], [1, 0]], [[0], [1]]], [[[1, 0], [0, 4/9]], [[1]]]); max(map(abs, flatten(x - [[17/9, 0], [0, 8]]))) < 10^-10 }",
            "true",
        ),
    ]);
}

/// Pade approximants, square-free partial fractions, reductions and generating functions.
#[test]
fn rational_algebra() {
    check(&[
        ("together(pade_approximant(exp(x), [x, 0, [2, 3]]) - (1 + 2*x/5 + x^2/20)/(1 - 3*x/5 + 3*x^2/20 - x^3/60))", "0"),
        ("together(pade_approximant(log(1 + 1/x), [x, inf, 2]) - (1/x + 1/(2*x^2))/(1 + 1/x + 1/(6*x^2)))", "0"),
        ("together(apart_square_free((x^2 - 2)/(x^4 - 2*x^2 + 1)) - (1/(x^2 - 1) - 1/(x^2 - 1)^2))", "0"),
        ("partial_fraction_elements(x^2/(x^2 + x - 2), x, \"FractionalCoefficients\")", "[[-2, 1, [-4/3]], [1, 1, [1/3]]]"),
        ("polynomial_reduction(x^3 + y^3, [x^2 - y^2 - 1, x + 2*y - 7], [x, y])", "7 + -2*y - y^3 + 7*y^2"),
        ("symmetric_reduction(x^2 - y^2, [x, y], [s1, s2])", "[-2*s2 + s1^2, -2*y^2]"),
        ("together(find_generating_function([1, 1, 2, 3, 5, 8, 13], x) - 1/(1 - x - x^2))", "0"),
    ]);
}

/// Changes of variables, the Radon transform, asymptotic expansions and solutions.
#[test]
fn asymptotics_and_transforms() {
    check(&[
        ("expand(activate(integrate_change_variables(inactive(integrate)(exp(2*x), [x, 1, 3]), u, u == 2*x)) - integrate(exp(2*x), [x, 1, 3]))", "0"),
        ("integrate_change_variables(inactive(integrate)(sin(x)*cos(x), [x, 0, pi/2]), t, t == sin(x))", "inactive(integrate)(t, [t, 0, 1])"),
        ("simplify(radon_transform(exp(-x^2 - y^2), [x, y], [p, f]) - pi^(1/2)*exp(-p^2))", "0"),
        ("expand(asymptotic_integrate(exp(t*x), [t, 0, 1], [x, 0, 3]) - (1 + x/2 + x^2/6 + x^3/24))", "0"),
        ("expand(asymptotic_sum(1/(k^2 + a), [k, 1, inf], [a, 0, 1]) - (pi^2/6 - a*pi^4/90))", "0"),
        ("expand(rewrite(y, asymptotic_solve(exp(y) - cos(x*y) + x == 0, [y, 0], [x, 0, 3])[1]) + x + x^2/2 + x^3/3)", "0"),
        // two of the four branches leave to infinity at x = 0 (Puiseux): no incomplete list (round 36)
        ("asymptotic_solve(x*y^4 - (x + 1)*y^2 + x == 1, [y], [x, 0, 2])", "failure(unevaluated, asymptotic_solve, \"a branch through a singular point or to infinity needs a Puiseux expansion\")"),
    ]);
}

/// Stability of equilibria and identities of sequences.
#[test]
fn stability_and_identities() {
    check(&[
        ("d_stability_conditions(x'(t) == x(t)^2 - 5*x(t) + 6, x, t)", "[[[2], true], [[3], false]]"),
        ("d_stability_conditions([x'(t) == -x(t) - y(t), y'(t) == 2*x(t) - y(t)], [x, y], t)", "[[[0, 0], true]]"),
        ("r_stability_conditions(a(n + 1) == 2*(1 - a(n))*a(n), a(n), n)", "[[[0], false], [[1/2], true]]"),
        ("r_stability_conditions([y(n + 1) == 3 - z(n), z(n + 1) == (1 - y(n))/2], [y(n), z(n)], n)", "[[[5, -2], true]]"),
        ("difference_root_reduce(fibonacci(2*n) == fibonacci(n)*lucas_l(n), n)", "true"),
        ("difference_root_reduce(fibonacci(n) == n, n)", "false"),
        ("find_instance(xor(x, y), [x, y], booleans)", "[[x -> false, y -> true]]"),
    ]);
}

/// Green's functions of boundary and initial value problems.
#[test]
fn green_functions() {
    check(&[
        ("expand(green_function([-u''(x), u(0) == 0, u(1) == 0], u(x), [x, 0, 1], y) - ((x - x*y)*heaviside_theta(y - x) + (y - x*y)*heaviside_theta(x - y)))", "0"),
        ("expand(green_function([u''(x) + 5*u'(x) + 6*u(x), u(0) == 0, u'(0) == 0], u(x), [x, 0, inf], y) - (exp(2*y - 2*x) - exp(3*y - 3*x))*heaviside_theta(x - y))", "0"),
    ]);
}
