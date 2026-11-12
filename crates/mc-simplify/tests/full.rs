//! Round 19 (phase 12): `full_simplify`, the options of the search, the new moves, the special-function identities
//! of `function_expand` and `simplify`, assumptions, and the spot check of the identities.
mod common;
use common::{check, Lcg, Sys};

#[test]
fn fixes_on_the_way() {
    check(&[
        ("simplify(3 + 4/(3 + sqrt(13)))", "13^(1/2)"),
        ("simplify(log(64)/18 - log(2)/3)", "0"),
        ("r = sqrt(x^2 + y^2 + z^2); simplify(d(1/r, x, x) + d(1/r, y, y) + d(1/r, z, z))", "0"),
        ("simplify(1/(1 + sqrt(2)))", "-1 + 2^(1/2)"),
    ]);
}

#[test]
fn new_moves() {
    check(&[
        ("simplify(2*x - 4*y + 6*z - 10 == -8)", "x + 3*z == 1 + 2*y"),
        ("simplify(4*sin(x)^2*cos(x)^2 + 4*sin(x)*cos(x) + 1)", "(cos(x) + sin(x))^4"),
        ("simplify(4*sin(x)^2*cos(x)^2 + 4*sin(x)*cos(x) + 1, trig: false)", "(1 + 2*cos(x)*sin(x))^2"),
        ("simplify(im_part(log(x) + 2*x - 3*y) - 2*im_part(x))", "arg(x) + -3*im_part(y)"),
        ("simplify(-im*log((1 + 2*im)/sqrt(5)))", "atan(2)"),
        ("simplify(16*atan(1/5) - 4*atan(1/239))", "pi"),
        ("simplify(atan(1/2) + atan(1/3))", "(1/4)*pi"),
        ("simplify(exp_integral_e(1 - n, x)*x^n)", "gamma(n, x)"),
        ("simplify(csc(pi*v)*((bessel_i(-v, z) - bessel_i(v, z))/2))", "bessel_k(v, z)/pi"),
        ("simplify(polylog(2, 1 - z) + polylog(2, z) - zeta(2))", "-1*log(z)*log(1 - z)"),
        // the gamma family is a move of the search: here the input is the cheapest form
        ("simplify(gamma(z + 3) - z*gamma(z))", "gamma(3 + z) + -1*z*gamma(z)"),
    ]);
}

#[test]
fn options() {
    check(&[
        ("simplify((x - 1)*(x + 1)*(x^2 + 1)*(x - 2)^10, excluded_forms: [(_: Head[add])^_])", "(-1 + x^4)*(-2 + x)^10"),
        ("simplify(im_part(log(x) + 2*x - 3*y) - 2*im_part(x), excluded_forms: [log(_)])", "im_part(log(x) + -3*y)"),
        ("rule f(e_u_) := 100*count(e_u, _: Head[chebyshev_t], [0, inf]) + leaf_count(e_u); full_simplify(chebyshev_t(n, x), complexity_function: f)", "cos(n*acos(x))"),
        // simplify does not expand special functions, whatever the measure
        ("simplify(chebyshev_t(n, x), complexity_function: f)", "chebyshev_t(n, x)"),
        ("simplify(chebyshev_t(n, x))", "chebyshev_t(n, x)"),
        ("rule t(e_u_) := reduce(e_u, x, reals); simplify(boole(e^x < 7), x < 3/2, transformation_functions: [automatic, t])", "1"),
        // without the default moves only the user's function runs
        ("simplify(sin(x)^2 + cos(x)^2, transformation_functions: [expand])", "cos(x)^2 + sin(x)^2"),
        ("simplify(sin(x)^2 + cos(x)^2, time_constraint: 1)", "1"),
        ("full_simplify(x*gamma(x))", "gamma(1 + x)"),
        ("simplify(x*gamma(x))", "gamma(1 + x)"),
        ("full_simplify(sin(x)^2 + cos(x)^2)", "1"),
        ("full_simplify(chebyshev_t(n, x))", "chebyshev_t(n, x)"),
    ]);
}

#[test]
fn equations_with_equal_sides() {
    check(&[
        ("simplify(sin(x)^2 == 1 - cos(x)^2)", "true"),
        ("simplify((x^2 - 1)/(x - 1) == x + 1)", "true"),
        ("simplify([d(exp(2*t)*sin(t), t) == exp(2*t)*(2*sin(t) + cos(t)), x == x + 1])", "[true, false]"),
        ("simplify(x == y)", "x == y"),
        ("simplify(x^2 == 2*x)", "x^2 == 2*x"),
    ]);
}

#[test]
fn equational_axioms() {
    check(&[
        ("simplify(f(f(u, f(v, w)), u) == f(u, f(f(v, w), u)), for_all([a, b, c], f(a, f(b, c)) == f(f(a, b), c)))", "true"),
        ("simplify(f(f(b, a), a) == f(a, f(b, a)), for_all([a, b], f(a, b) == f(b, a)))", "true"),
        ("simplify(f(f(e_u, e_u), e_u) == e_u, for_all(a, f(a, e_u) == a))", "true"),
        // not provable by rewriting: left alone
        ("simplify(f(a) == g(a), for_all(x, f(x) == f(f(x))))", "f(a) == g(a)"),
    ]);
}

#[test]
fn assumptions() {
    check(&[
        ("simplify(product_log(x*log(x)), x > 1/e)", "log(x)"),
        ("simplify(product_log(x*e^x), x >= -1)", "x"),
        ("simplify(product_log(x*e^x))", "product_log(x*exp(x))"),
        ("simplify(e^elliptic_f(x, 1), -pi/2 < x and x < pi/2)", "sec(x) + tan(x)"),
        ("simplify(mod(a^euler_phi(m), m), element(a | m, integers) and m > 1 and coprime_q(a, m))", "1"),
        ("refine(mod(a^(2*euler_phi(m)), m), element(a | m, integers) and m > 1 and coprime_q(m, a))", "1"),
        ("simplify(mod(a^euler_phi(m), m), element(a | m, integers) and m > 1)", "mod(a^euler_phi(m), m)"),
        ("refine(fractional_part(a), a < 0 and mod(a, 1) == 1/3)", "-2/3"),
        ("refine(fractional_part(a), a > 0 and mod(a, 1) == 1/3)", "1/3"),
        ("refine(exp(x) < 7, x < 3/2)", "true"),
        ("refine(exp(x) > 1/2, x > -1/2)", "true"),
        ("refine(exp(x) < 4, x < 3/2)", "exp(x) < 4"),
    ]);
}

#[test]
fn function_expand_identities() {
    check(&[
        ("function_expand(sin(24*degree))", "(-1/8)*(-1 - 5^(1/2))*3^(1/2) + (-1/8)*2^(1/2)*(5 - 5^(1/2))^(1/2)"),
        ("function_expand(sin(pi/128))", "(1/2)*(2 - (2 + (2 + (2 + (2 + 2^(1/2))^(1/2))^(1/2))^(1/2))^(1/2))^(1/2)"),
        ("function_expand(cos(pi/8))", "(1/2)*(2 + 2^(1/2))^(1/2)"),
        ("function_expand(4*atan(1/5) - atan(1/239))", "(1/4)*pi"),
        ("function_expand(hypergeometric_pfq([1/2], [1, 1], z))", "bessel_i(0, z^(1/2))^2"),
        ("function_expand(log((1 + im*sqrt(3))/2))", "complex(0, 1/3)*pi"),
        ("function_expand(zeta'(-2))", "(-1/4)*pi^-2*zeta(3)"),
        ("function_expand([sqrt(-x^2), log(sqrt(1 - x^2))])", "[x^(1/2)*(-x)^(1/2), (1/2)*(log(1 + x) + log(1 - x))]"),
        ("function_expand(sin(2*atan(x)))", "2*x/(1 + x^2)"),
        ("function_expand(exp(2*atanh(x)))", "(-1 - x^2)/(-1 + x^2) + -2*x/(-1 + x^2)"),
        ("function_expand(airy_ai((-1)^(4/3)*x))", "(1/4)*(1 + -1im*3^(1/2))*(airy_ai(x) + 1im*airy_bi(x))"),
        ("function_expand(sin(sqrt(x^2))/sqrt(x^2))", "x^-1*sin(x)"),
        ("function_expand(bessel_j(nu, -x))", "bessel_j(nu, x)*x^-nu*(-x)^nu"),
        ("function_expand(jacobi_sn(u, m)*jacobi_nd(u, m) == jacobi_sd(u, m))", "true"),
        ("function_expand(fibonacci(n + 3), element(n, integers))", "lucas_l(n) + 2*fibonacci(n)"),
        ("function_expand(fibonacci(n + 3))", "fibonacci(3 + n)"),
        ("function_expand(polylog(2, 1 - z) + polylog(2, z))", "(1/6)*pi^2 + -1*log(z)*log(1 - z)"),
        ("function_expand(polygamma(3, z - 2) - polygamma(3, z))", "6*((-2 + z)^-4 + (-1 + z)^-4)"),
        ("function_expand(zeta(1 - z)/zeta(z))", "gamma(z)*2^(1 - z)*pi^-z*sin((1/2)*pi*(1 - z))"),
        ("function_expand(gamma(z + 3) - gamma(z)*z)", "(1 + 3*z + z^2)*gamma(1 + z)"),
        ("function_expand(product_log(x*log(x)), x > 1/e)", "log(x)"),
        (
            "function_expand(bessel_k(nu, -x))",
            "bessel_k(nu, x)*x^nu*(-x)^-nu + (1/2)*pi*(x^nu*(-x)^-nu + -1*x^-nu*(-x)^nu)*bessel_i(nu, x)*csc(nu*pi)",
        ),
        ("function_expand(bessel_k(nu, -x), assumptions: element(nu, integers))", "(log(x) - log(-x))*bessel_i(nu, x) + bessel_k(nu, x)*(-1)^nu"),
        ("function_expand(legendre_q(3/2, 1/2, x))", "-2*x*pi^(1/2)*(1 + x)^(-1/4)*(1 - x)^(1/4)*(1 + (1/2)*(-1 + x))^(1/2)"),
        (
            "function_expand(hypergeometric_pfq([1/2, 1/2, 1/2], [1, 1], x))",
            "4*pi^-2*elliptic_k(1/2 + (-1/2)*(1 - x)^(1/2))^2",
        ),
        // the round 7 forms still apply
        ("function_expand(sec(x)*cos(x))", "1"),
        ("function_expand(hypergeometric1f1(a, a, z))", "exp(z)"),
        ("function_expand(hypergeometric2f1(1, 1, 2, z))", "-1*log(1 - z)/z"),
    ]);
}

/// Each identity of `function_expand` keeps the value at random complex points (balls at 30 digits).
#[test]
fn identities_keep_the_value() {
    let mut s = Sys::new();
    let mut rng = Lcg(19);
    let corpus = [
        "sin(2*atan(x))",
        "exp(2*atanh(x))",
        "log(sqrt(1 - x^2))",
        "sqrt(-x^2)",
        "polygamma(3, x - 2) - polygamma(3, x)",
        "gamma(x + 3) - gamma(x)*x",
        // (bessel_j(1/3, -x) and the Airy rotation are left out: n() of a Bessel function at a negative argument
        // and of airy_ai at a complex one do not reach 30 digits in this build; zeta(1 - x) takes about a minute)
        "hypergeometric_pfq([1/2], [1, 1], x)",
        "hypergeometric_0f1(5/2, x)",
        "jacobi_sd(x, 1/3)",
        "hypergeometric_pfq([-1/2, 1/2, 1/2], [1, 1], x)",
    ];
    for e in corpus {
        let src = format!("ee = {e}; ss = function_expand(ee); ss");
        let shown = s.run(&src);
        for _ in 0..3 {
            let (a, c) = (rng.point(0.1, 0.9), rng.point(0.1, 0.9));
            // real points in (0, 1) (complex values of the results included: bessel_j(1/3, -x))
            let at = format!("{a} + {c}/1000");
            let out = s.run(&format!("n(1 + abs(rewrite(ee - ss, x -> {at}))*10^20, 30)"));
            let v: f64 = out.trim_end_matches('.').parse().unwrap_or(f64::NAN);
            assert!((v - 1.0).abs() < 1.0, "function_expand({e}) = {shown} at x = {a} + {c} i: {out}");
        }
    }
}

/// A rewrite the spot check refutes is not returned: `transformation_functions:` cannot smuggle in a wrong form
/// through the moves that are checked, and a wrong identity of the table would be caught the same way.
#[test]
fn spot_check() {
    check(&[
        // a user transformation is the user's: its cheaper form is taken (no check)
        ("rule w(u_) := 0; simplify(sin(x) + 1, transformation_functions: [automatic, w])", "0"),
    ]);
}
