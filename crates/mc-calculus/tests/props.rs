//! Properties of functions: domain, singularities, analyticity, sign, monotonicity, convexity, injectivity; and
//! the Fourier transforms.
mod common;
use common::check;

#[test]
fn domains_and_singularities() {
    check(&[
        ("function_domain(x/(x^2 - 1), x)", "x < -1 or -1 < x and x < 1 or 1 < x"),
        ("function_domain(sqrt(1 - x) + sqrt(1 + x), x)", "-1 <= x and x <= 1"),
        ("function_domain(acos(x), x)", "-1 <= x and x <= 1"),
        ("function_domain(x*log(x) + sqrt(1 - x) - e^x + 2, x)", "0 < x and x <= 1"),
        ("function_domain(x/(x^2 + 1), x, complexes)", "1 + x^2 != 0"),
        ("function_domain(exp(1/x), x, complexes)", "x != 0"),
        ("function_domain((x + y)/(x^2 - y^2), [x, y])", "-y^2 + x^2 != 0"),
        ("function_domain(sqrt(x^2 + y^2 - 9), [x, y])", "x^2 + y^2 >= 9"),
        ("function_domain(x^3 - x, x)", "true"),
        ("function_domain(g(x), x)", "function_domain(g(x), x)"),
        ("function_singularities(tan(x), x)", "cos(x) == 0"),
        ("function_singularities(sqrt(x), x)", "x <= 0"),
        ("function_singularities(exp(x) + sin(x), x)", "false"),
        ("function_analytic(sin(x), x)", "true"),
        ("function_analytic(tan(x), x)", "false"),
        ("function_analytic([tan(x), -pi/2 < x and x < pi/2], x)", "true"),
    ]);
}

#[test]
fn signs_monotonicity_convexity_injectivity() {
    check(&[
        ("function_sign(abs(x), x)", "1"),
        ("function_sign(sin(x) + 3/2, x)", "1"),
        ("function_sign(sin(x), x)", "indeterminate"),
        ("function_sign(x - sqrt(x^2), x)", "-1"),
        ("function_monotonicity(e^x, x)", "1"),
        ("function_monotonicity(sin(x), x)", "indeterminate"),
        ("function_monotonicity([log(x), x > 0], x)", "1"),
        ("function_monotonicity(sqrt(x^2) - x, x)", "-1"),
        ("function_convexity(x^2, x)", "1"),
        ("function_convexity(x^3, x)", "indeterminate"),
        ("function_injective(e^x, x)", "true"),
        ("function_injective(x^3 - x, x)", "false"),
        ("function_injective([x^3 - x, x > 1], x)", "true"),
    ]);
}

#[test]
fn fourier_transforms() {
    check(&[
        ("fourier_transform(exp(-t^2), t, w)", "(1/2)*2^(1/2)*exp((-1/4)*w^2)"),
        ("inverse_fourier_transform(exp(-w^2/4)/sqrt(2), w, t)", "exp(-t^2)"),
        ("fourier_transform(unit_step(t)*unit_step(1 - t), t, w)", "(1/2)*(1im/w + -1im*exp(1im*w)/w)*2^(1/2)*pi^(-1/2)"),
        ("fourier_transform(g(t), t, w)", "fourier_transform(g(t), t, w)"),
        // sums, sign flips inside a box, and empty boxes (G45)
        ("simplify(fourier_transform(unit_step(t)*unit_step(1 - t)*sign(t - 1/2), t, w) - fourier_transform(unit_step(t - 1/2)*unit_step(1 - t) - unit_step(t)*unit_step(1/2 - t), t, w))", "0"),
        ("fourier_transform(unit_step(t - 2)*unit_step(1 - t), t, w)", "0"),
    ]);
}
