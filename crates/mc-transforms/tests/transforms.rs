//! Integral transforms (item 14E): Laplace, Mellin, Fourier (exponential, sine, cosine), Z and their inverses.
//! Values from rules and tables passed the transforms' numeric check (sample points against `n_integrate`, inverses
//! against the forward integral).
mod common;
use common::Sys;

/// The cases in one session; with `MC_CAPTURE` set, the printed values are written instead of compared.
fn check(cases: &[(&str, &str)]) {
    let mut s = Sys::new();
    let capture = std::env::var_os("MC_CAPTURE").is_some();
    for (src, want) in cases {
        let got = s.run(src);
        if capture {
            println!("        ({src:?}, {got:?}),");
        } else {
            assert_eq!(got, *want, "{src}");
        }
    }
}

#[test]
fn laplace() {
    check(&[
        ("laplace_transform(t*unit_step(t - 1), t, s)", "(1 + s)*exp(-s)*s^-2"),
        ("laplace_transform(sin(w*t), t, s)", "w/(s^2 + w^2)"),
        ("laplace_transform(exp(a*t)*sin(w*t), t, s)", "w/(w^2 + (s - a)^2)"),
        ("laplace_transform(t*cosh(a*t), t, s)", "(a^2 + s^2)*(a + s)^-2*(a - s)^-2"),
        ("laplace_transform(t^n, t, s)", "gamma(1 + n)*s^(-1 - n)"),
        ("laplace_transform(log(t), t, s)", "(-euler_gamma - log(s))/s"),
        ("laplace_transform(bessel_j(0, a*t), t, s)", "(a^2 + s^2)^(-1/2)"),
        ("laplace_transform(sin_integral(t), t, s)", "atan(s^-1)/s"),
        ("laplace_transform(laguerre_l(n, t), t, s)", "s^(-1 - n)*(-1 + s)^n"),
        ("laplace_transform(x*exp(-y), [x, y], [p, q])", "p^-2/(1 + q)"),
        ("laplace_transform(sin(t), t, 1.2)", "0.4098360655737705"),
        ("laplace_transform(f''(t), t, s)", "-f'(0) + laplace_transform(f(t), t, s)*s^2 + -1*s*f(0)"),
        ("laplace_transform(a*g(t) + exp(-t), t, s)", "a*laplace_transform(g(t), t, s) + (1 + s)^-1"),
        ("laplace_transform(exp(-a*t), t, s, generate_conditions: true)", "conditional_expression((a + s)^-1, re(a + s) > 0)"),
        ("laplace_transform(g(t), t, s)", "laplace_transform(g(t), t, s)"),
    ]);
}

#[test]
fn inverse_laplace() {
    check(&[
        ("inverse_laplace_transform(1/(s^2 + 3*s + 2), s, t)", "-exp(-2*t) + exp(-t)"),
        ("inverse_laplace_transform(a/(s^2 + a^2), s, t)", "sin(a*t)"),
        ("inverse_laplace_transform(s/(s^2 + a^2)^2, s, t)", "(1/2)*t/a*sin(a*t)"),
        ("inverse_laplace_transform(a/(s^2 - a^2), s, t)", "sinh(a*t)"),
        ("inverse_laplace_transform(log((s + a)/(s + b)), s, t)", "(-exp(-1*a*t) + exp(-1*b*t))/t"),
        ("inverse_laplace_transform(1/sqrt(s + 1), s, t)", "exp(-t)*pi^(-1/2)*t^(-1/2)"),
        ("inverse_laplace_transform(exp(-2*s)/s^2, s, t)", "(-2 + t)*heaviside_theta(-2 + t)"),
        ("inverse_laplace_transform((s^2 + 1)/(s + 1), s, t)", "-dirac_delta(t) + 2*exp(-t) + dirac_delta'(t)"),
        ("inverse_laplace_transform(laplace_transform(g(t), t, s), s, t)", "g(t)"),
    ]);
}

#[test]
fn bilateral() {
    check(&[
        ("bilateral_laplace_transform(exp(-abs(t)), t, s)", "conditional_expression(-2/(-1 + s^2), -1 < re(s) and re(s) < 1)"),
        ("bilateral_laplace_transform(exp(-abs(t)), t, s, generate_conditions: false)", "-2/(-1 + s^2)"),
        ("bilateral_laplace_transform(dirac_delta(t - c), t, s)", "exp(-1*c*s)"),
        ("inverse_bilateral_laplace_transform(conditional_expression(1/(s*(s - 4)*(s - 8)), 0 < re(s) and re(s) < 4), s, t)", "piecewise([[1/32, t >= 0]], (-1/32)*(-2 + exp(4*t))*exp(4*t))"),
        ("inverse_bilateral_laplace_transform(exp(s^2/2), s, t)", "(1/2)*2^(1/2)*exp((-1/2)*t^2)*pi^(-1/2)"),
        ("inverse_bilateral_laplace_transform(1, s, t)", "dirac_delta(t)"),
    ]);
}

#[test]
fn mellin() {
    check(&[
        ("mellin_transform(exp(-a*x), x, s)", "gamma(s)*a^-s"),
        ("mellin_transform(1/(1 + x), x, s)", "pi*csc(pi*s)"),
        ("mellin_transform(bessel_j(1, x), x, s)", "gamma(1/2 + (1/2)*s)*2^(-1 + s)/gamma(3/2 + (-1/2)*s)"),
        ("mellin_transform(x^2 + 3*x, x, s)", "2*pi*dirac_delta(1im*(2 + s)) + 6*pi*dirac_delta(1im*(1 + s))"),
        ("mellin_transform(unit_step(x - 1), x, s)", "-s^-1"),
        ("mellin_transform(sin(log(x))*unit_step(x - 1), x, s)", "(1 + s^2)^-1"),
        ("mellin_transform(exp(-x - 3*y^2), [x, y], [s, t])", "(1/2)*gamma(s)*gamma((1/2)*t)*3^((-1/2)*t)"),
        ("inverse_mellin_transform(gamma(s), s, x)", "exp(-x)"),
        ("inverse_mellin_transform(gamma(2*s), s, x)", "(1/2)*exp(-x^(1/2))"),
        ("inverse_mellin_transform(gamma(s), s, x, assumptions: -1 < re(s) and re(s) < 0)", "-1 + exp(-x)"),
        ("inverse_mellin_transform(gamma(s), s, x, generate_conditions: true)", "conditional_expression(exp(-x), re(s) > 0)"),
        ("inverse_mellin_transform(2^(s - 1)*gamma(s/2)/gamma(1 - s/2), s, x)", "bessel_j(0, x)"),
        ("inverse_mellin_transform(1/(s^2 + a^2), s, x)", "-1*heaviside_theta(1 - x)/a*sin(a*log(x))"),
        ("inverse_mellin_transform(a^(s - 1), s, x)", "dirac_delta(x - a)"),
    ]);
}

#[test]
fn fourier() {
    check(&[
        ("fourier_transform(exp(-t^2), t, w)", "(1/2)*2^(1/2)*exp((-1/4)*w^2)"),
        ("fourier_transform(sin(a*t), t, w)", "complex(0, -1/2)*dirac_delta(a + w)*2^(1/2)*pi^(1/2) + complex(0, 1/2)*dirac_delta(w - a)*2^(1/2)*pi^(1/2)"),
        ("fourier_transform(heaviside_theta(t), t, w)", "complex(0, 1/2)*2^(1/2)*pi^(-1/2)/w + (1/2)*dirac_delta(w)*2^(1/2)*pi^(1/2)"),
        ("fourier_transform(t^2, t, w)", "-1*2^(1/2)*pi^(1/2)*dirac_delta''(w)"),
        ("fourier_transform(1/sqrt(abs(t)), t, w)", "abs(w)^(-1/2)"),
        ("fourier_transform(sin(3*t)/t, t, w)", "(1/4)*(sign(3 + w) + sign(3 - w))*2^(1/2)*pi^(1/2)"),
        ("inverse_fourier_transform(1/(1 + w^2), w, t)", "(1/2)*2^(1/2)*exp(-abs(t))*pi^(1/2)"),
        ("inverse_fourier_transform(1/(a + im*w), w, t, assumptions: a > 0)", "heaviside_theta(-t)*2^(1/2)*exp(a*t)*pi^(1/2)"),
        ("inverse_fourier_transform(1/w, w, t)", "complex(0, -1/2)*2^(1/2)*pi^(1/2)*sign(t)"),
        ("fourier_transform(dirac_delta(t), t, w, fourier_parameters: [1, -1])", "1"),
        ("fourier_sin_transform(exp(-t), t, w)", "w*2^(1/2)/(1 + w^2)*(pi^-1)^(1/2)"),
        ("fourier_cos_transform(1/(1 + t^2), t, w)", "(1/2)*2^(1/2)*exp(-w)*pi^(1/2)"),
        ("fourier_cos_transform(1/sqrt(t), t, w)", "w^(-1/2)"),
        ("fourier_sin_transform(exp(-t), t, w, fourier_parameters: [1, 2*pi])", "4*w*2^(1/2)*pi^(3/2)/(1 + 4*pi^2*w^2)"),
        ("inverse_fourier_cos_transform(exp(-w), w, t)", "2^(1/2)/(1 + t^2)*(pi^-1)^(1/2)"),
        ("fourier_transform(g(t), t, w)", "fourier_transform(g(t), t, w)"),
    ]);
}

#[test]
fn z() {
    check(&[
        ("z_transform(a^n, n, z)", "z/(z - a)"),
        ("z_transform(n^2, n, z)", "z*(1 + z)*(-1 + z)^-3"),
        ("z_transform(n*(n + 1)*(-1/2)^n, n, z)", "-8*z^2*(1 + 2*z)^-3"),
        ("z_transform([discrete_delta(n), discrete_delta(n - 5)], n, z)", "[1, z^-5]"),
        ("z_transform(unit_step(n - 3), n, z)", "z^-2/(-1 + z)"),
        ("z_transform((n - 3)*unit_step(n - 3), n, z)", "z^-2*(-1 + z)^-2"),
        ("z_transform(f(n + 1), n, z)", "z*(-f(0) + z_transform(f(n), n, z))"),
        ("z_transform(a^n, n, z, generate_conditions: true)", "conditional_expression(z/(z - a), abs(z) > abs(a))"),
        ("inverse_z_transform(z/(z - a), z, n)", "a^n"),
        ("inverse_z_transform(z/(z - 1)^2, z, n)", "n"),
        ("inverse_z_transform(1/z^3, z, n)", "discrete_delta(-3 + n)"),
        ("discrete_delta(0)", "1"),
        ("discrete_delta(3)", "0"),
        ("kronecker_delta(2, 2)", "1"),
        ("kronecker_delta(1, 2)", "0"),
        ("kronecker_delta(i, j)", "kronecker_delta(i, j)"),
    ]);
}
