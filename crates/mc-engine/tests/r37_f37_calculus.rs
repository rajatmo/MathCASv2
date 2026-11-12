//! Round 37: calculus and equation inputs that had no answer, one test per cause. Each test carries
//! an example input and variations of its own, each checked by an independent route (a numeric integral, a
//! numeric value of the function, an identity).
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

/// `a` equals `b`: their difference simplifies to 0.
fn same(a: &str, b: &str) -> String {
    ev(&format!("simplify(({a}) - ({b})) === 0"))
}

/// `a` equals `b` numerically at the given values of the variable `v` (to 1e-12).
fn same_at(a: &str, b: &str, v: &str, pts: &[&str]) -> String {
    let checks: Vec<String> =
        pts.iter().map(|p| format!("abs(n(rewrite(({a}) - ({b}), {v} -> {p}), 30)) < 10^-12")).collect();
    ev(&format!("{{ {} }}", checks.join(" and ")))
}

/// The Hilbert transform of f at s0 against the principal value written as a proper integral:
/// (1/pi) PV integrate(f(t)/(s0 - t), t) = (1/pi) integrate((f(s0 - u) - f(s0 + u))/u, [u, 0, inf]).
fn hilbert_matches_pv(f: &str, s0: &str) -> String {
    ev(&format!(
        "{{ let h = hilbert_transform({f}, t, s); let a = n(rewrite(h, s -> {s0})); \
         let b = n_integrate((rewrite({f}, t -> {s0} - u) - rewrite({f}, t -> {s0} + u))/u, [u, 0, inf])/pi; \
         abs(a - b) < 10^-8 }}"
    ))
}

#[test]
fn hilbert_transform_rational_functions_with_parameters_and_multiple_poles() {
    // documented inputs
    assert_eq!(same("hilbert_transform(t/(t^2 + a^2), t, s, assumptions: re(a) > 0)", "-a/(a^2 + s^2)"), "true");
    assert_eq!(same("hilbert_transform(1/(t^2 + a^2)^2, t, s, assumptions: a > 0)", "(3*s*a^2 + s^3)/(2*a^3*(a^2 + s^2)^2)"), "true");
    assert_eq!(same("inverse_hilbert_transform(1/(t^2 + 4)^2, t, s)", "-s*(12 + s^2)/(16*(4 + s^2)^2)"), "true");
    // variations: other numbers, a shifted double pole, a cubic numerator: against the principal value integral
    assert_eq!(hilbert_matches_pv("t/(t^2 + 4)", "0.7"), "true");
    assert_eq!(hilbert_matches_pv("1/((t - 1)^2 + 4)^2", "1.3"), "true");
    assert_eq!(hilbert_matches_pv("t^3/(t^2 + 9)^2", "-0.4"), "true");
    // a parameter of unknown sign leaves the transform
    assert_eq!(ev("head(hilbert_transform(t/(t^2 + a^2), t, s)) === hilbert_transform"), "true");
}

#[test]
fn hilbert_transform_real_poles_give_distributions() {
    assert_eq!(same("hilbert_transform(1/t, t, s)", "-pi*dirac_delta(s)"), "true");
    assert_eq!(same("hilbert_transform(1/t^3, t, s)", "(-1/2)*pi*d(dirac_delta(s), s, s)"), "true");
    assert_eq!(same("inverse_hilbert_transform(1/(pi*t), t, s)", "dirac_delta(s)"), "true");
    // H H = -1: H(dirac_delta) = 1/(pi s) undoes H(1/t) = -pi dirac_delta
    assert_eq!(same("hilbert_transform(dirac_delta(t), t, s)", "1/(pi*s)"), "true");
    // a shifted pole of order 2: (t - 1)^-2 = -d/dt (t - 1)^-1, so H = pi dirac_delta'(s - 1)
    assert_eq!(same("hilbert_transform(1/(t - 1)^2, t, s)", "pi*d(dirac_delta(s - 1), s)"), "true");
}

#[test]
fn hilbert_transform_of_oscillating_terms() {
    // documented inputs
    assert_eq!(same("hilbert_transform(sin(3*t)/t, t, t)", "(1 - cos(3*t))/t"), "true");
    assert_eq!(same("hilbert_transform(cos(-4*t)/t, t, t)", "sin(4*t)/t - pi*dirac_delta(t)"), "true");
    assert_eq!(same("hilbert_transform(cos(a*t)^3, t, s, assumptions: a > 0)", "(1/2)*(2 + cos(2*a*s))*sin(a*s)"), "true");
    assert_eq!(same("inverse_hilbert_transform(sin(pi*t)^2/t, t, s)", "sin(2*pi*s)/(2*s)"), "true");
    assert_eq!(
        same("hilbert_transform(cos(α*t + ϕ)*cos(β*t + Φ), t, s, assumptions: 0 < α and α < β)", "cos(ϕ + s*α)*sin(Φ + s*β)"),
        "true"
    );
    // variations: a sinusoid over a rational function with complex poles, exp(im t) over a pole in the upper half
    // plane, a cube: against the principal value integral
    assert_eq!(hilbert_matches_pv("sin(5*t)/(t^2 + 1)", "0.7"), "true");
    assert_eq!(hilbert_matches_pv("cos(2*t)^3/(t^2 + 1)", "0.7"), "true");
    assert_eq!(hilbert_matches_pv("exp(im*t)/(t - im)", "0.3"), "true");
    // the sign of the frequency undecided: no transform
    assert_eq!(ev("head(hilbert_transform(cos(a*t), t, s)) === hilbert_transform"), "true");
}

#[test]
fn hilbert_transform_of_piecewise_polynomials_of_compact_support() {
    // documented inputs
    assert_eq!(ev("abs(hilbert_transform(unit_box(t), t, 0.3) - 0.44127120030530315) < 10^-14"), "true");
    assert_eq!(same_at("hilbert_transform(unit_box(t), t, s)", "log(abs((1 + 2*s)/(1 - 2*s)))/pi", "s", &["3/10", "-7/5", "13/4"]), "true");
    assert_eq!(
        same_at(
            "hilbert_transform(piecewise([[1 - t^2, abs(t) <= 1], [0, abs(t) > 1]]), t, t)",
            "(2*t + (-1 + t^2)*log(abs((-1 + t)/(1 + t))))/pi",
            "t",
            &["3/10", "-7/5", "13/4"]
        ),
        "true"
    );
    // variations: a shifted and scaled box, a ramp, a scaled odd box: against the principal value integral
    assert_eq!(hilbert_matches_pv("unit_box(2*t - 1)", "0.3"), "true");
    assert_eq!(hilbert_matches_pv("piecewise([[t, 0 < t and t < 2]], 0)", "3.1"), "true");
    assert_eq!(hilbert_matches_pv("sign(t)*unit_box(t/3)", "0.8"), "true");
    // not of compact support: no transform
    assert_eq!(ev("head(hilbert_transform(unit_step(t), t, s)) === hilbert_transform"), "true");
}

#[test]
fn asymptotic_relations_of_separable_ratios_in_several_variables() {
    // documented inputs: the ratio (x - 1)^2/sin(x) (y - 2)/cos(3 y/4) tends to 0 at [1, 2]
    let f = "(x - 1)^2*(y - 2), sin(x)*cos(3*(y/4)), [x, y] -> [1, 2]";
    assert_eq!(ev(&format!("asymptotic_less({f})")), "true");
    assert_eq!(ev(&format!("asymptotic_less_equal({f})")), "true");
    assert_eq!(ev(&format!("asymptotic_greater({f})")), "false");
    assert_eq!(ev(&format!("asymptotic_equal({f})")), "false");
    assert_eq!(ev("asymptotic_less(exp(x)*log(y), x*y, [x, y] -> [-inf, inf])"), "true");
    assert_eq!(ev("asymptotic_equivalent(exp(x)*log(y), x*y, [x, y] -> [-inf, inf])"), "false");
    // variations: a ratio with limit 1 (x/sin(x) and y/(y + 1)), one with an unbounded factor (1/y at 0)
    assert_eq!(ev("asymptotic_equivalent(sin(x)*(y + 1), x*y, [x, y] -> [0, inf])"), "true");
    assert_eq!(ev("asymptotic_equal(sin(x)*(y + 1), x*y, [x, y] -> [0, inf])"), "true");
    assert_eq!(ev("asymptotic_less(x*y, sin(x)*(y + 1), [x, y] -> [0, inf])"), "false");
    assert_eq!(ev("asymptotic_less_equal(x/y, sin(x), [x, y] -> [0, 0])"), "false");
    assert_eq!(ev("asymptotic_greater(x/y, sin(x), [x, y] -> [0, 0])"), "true");
    // the same limits one variable at a time, by limit
    assert_eq!(ev("[limit(x/sin(x), x -> 0), limit(y/(y + 1), y -> inf)] === [1, 1]"), "true");
    // a ratio that does not separate stays undecided
    assert_eq!(ev("is_failure(asymptotic_equal(x*y, x*y + 1, [x, y] -> [0, 0]))"), "true");
}

#[test]
fn series_of_inverse_functions_at_infinity_and_branch_points() {
    // documented inputs
    assert_eq!(same("normal(series(atan(x), x, inf, 5))", "pi/2 - 1/x + 1/(3*x^3) - 1/(5*x^5)"), "true");
    assert_eq!(same("normal(series(acosh(x), x, inf, 3))", "log(2) - log(1/x) - 1/(4*x^2)"), "true");
    assert_eq!(same("normal(series(asinh(x), x, inf, 3))", "log(2) - log(1/x) + 1/(4*x^2)"), "true");
    assert_eq!(
        same("normal(series(acosh(x), x, 1, 3))", "2^(1/2)*(-1 + x)^(1/2) - (1/12)*2^(1/2)*(-1 + x)^(3/2) + (3/160)*2^(1/2)*(-1 + x)^(5/2)"),
        "true"
    );
    assert_eq!(
        same("normal(series(atanh(x), x, -1, 3))", "(1/48)*(1 + x)^3 + (1/16)*(1 + x)^2 + (1/4)*(1 + x) + (1/2)*(log(1 + x) - log(2))"),
        "true"
    );
    // variations: at -inf, a polynomial argument, against the function at a point to 30 digits (the error is the
    // first omitted term)
    assert_eq!(same("normal(series(atan(x), x, -inf, 3))", "-pi/2 - 1/x + 1/(3*x^3)"), "true");
    assert_eq!(same("normal(series(atan(x^2 + x), x, inf, 4))", "pi/2 - x^-2 + x^-3 - x^-4"), "true");
    assert_eq!(ev("abs(n(rewrite(normal(series(atan(3*x), x, -inf, 6)) - atan(3*x), x -> -100), 30)) < 10^-17"), "true");
    assert_eq!(ev("abs(n(rewrite(normal(series(acosh(x), x, inf, 6)) - acosh(x), x -> 100), 30)) < 10^-16"), "true");
    assert_eq!(ev("abs(n(rewrite(normal(series(asinh(x), x, -inf, 6)) - asinh(x), x -> -100), 30)) < 10^-16"), "true");
    assert_eq!(ev("abs(n(rewrite(normal(series(atanh(x), x, inf, 6)) - atanh(x), x -> 100), 30)) < 10^-14"), "true");
    assert_eq!(ev("abs(n(rewrite(normal(series(acosh(x), x, -1, 4)) - acosh(x), x -> -1 - 1/10^4), 30)) < 10^-20"), "true");
}

#[test]
fn coefficient_of_monomials_lists_of_variables_and_modulus() {
    // documented inputs
    assert_eq!(ev("coefficient((x + y)^4, x*y^3)"), "4");
    assert_eq!(ev("coefficient((x + y)*(x + 2*y)*(3*x + 4*y + 5), x*y^2)"), "18");
    assert_eq!(ev("coefficient((x + 1)^3, x, 2, modulus: 2)"), "1");
    assert_eq!(ev("coefficient_list((x + 1)^5, x, modulus: 2)"), "[1, 1, 0, 0, 1, 1]");
    // variations, against the multinomial coefficient and the coefficients reduced one by one
    assert_eq!(ev("coefficient((x + 2*y + z)^5, x^2*y*z^2) == multinomial(2, 1, 2)*2"), "true");
    assert_eq!(ev("coefficient((x + 3)^4, x, 1, modulus: 5) == mod(coefficient((x + 3)^4, x, 1), 5)"), "true");
    assert_eq!(ev("coefficient_list((2*x + 3)^3, x, modulus: 7) == mod(coefficient_list((2*x + 3)^3, x), 7)"), "true");
    assert_eq!(ev("coefficient(a*c(1) + b*c(2) + c(3), [c(1), c(2), c(3)])"), "[a, b, 1]");
}

#[test]
fn trivial_reductions_and_empty_comparisons() {
    assert_eq!(ev("[root_reduce(true), root_reduce(false), root_reduce(x)]"), "[true, false, x]");
    assert_eq!(ev("[unequal(), equal(), unequal(1), unequal(1, 2, 1)]"), "[true, true, true, false]");
}

#[test]
fn limits_of_real_roots_and_oscillations() {
    // documented inputs
    assert_eq!(ev("[max_limit(cube_root(x), x -> -1), min_limit(cube_root(x), x -> -1)]"), "[-1, -1]");
    assert!(ev("limit(sin(1/x), x -> 0)").starts_with("failure(no_limit"));
    // variations: a sum and a quotient through the real root (8^(1/3) = 2, x^(-2/3) on both sides of 0), an even
    // root at a positive point, another oscillation (cos at infinity) and a damped one with a limit
    assert_eq!(ev("limit(cube_root(x)^2 + x, x -> 8)"), "12");
    assert_eq!(ev("limit(cube_root(x)/x, x -> 0)"), "inf");
    assert_eq!(ev("limit(surd(x, 4), x -> 16)"), "2");
    assert!(ev("limit(cos(x), x -> inf)").starts_with("failure(no_limit"));
    assert_eq!(ev("limit(x*sin(1/x), x -> 0)"), "0");
    assert_eq!(ev("max_limit(x^2, x -> 3)"), "9");
}

#[test]
fn derivatives_through_root_objects_and_their_wronskian() {
    let r = "root(&(11 - 5*&1^2 + &1^3), 1)";
    assert_eq!(same(&format!("d(exp(x*{r}), x)"), &format!("{r}*exp(x*{r})")), "true");
    assert_eq!(ev(&format!("d({r}, x)")), "0");
    // the Wronskian of exp(r_i x) is the Vandermonde product times exp(5 x): |W(0)| = sqrt(discriminant) = sqrt(2233)
    let w = "wronskian(coefficient(rewrite(y(x), d_solve(y'''(x) - 5*y''(x) + 11*y(x) == 0, y, x)[1]), table(c(i), i, 3)), x)";
    assert_eq!(ev(&format!("abs(abs(n(rewrite({w}, x -> 0), 30)) - n(2233^(1/2), 30)) < 10^-20")), "true");
    // and its x dependence is exp(5 x), Abel's identity (cell: wronskian of the equation)
    assert_eq!(ev(&format!("abs(n(rewrite({w}, x -> 1/2)/rewrite({w}, x -> 0), 30) - n(exp(5/2), 30)) < 10^-20")), "true");
}

#[test]
fn periodic_one_variable_optimization_over_one_period() {
    // documented inputs: sin(x) + cos(x) = 2^(1/2) sin(x + pi/4)
    assert_eq!(same("max_value(sin(x) + cos(x), x)", "2^(1/2)"), "true");
    assert_eq!(same("minimize(sin(x) + cos(x), x)[1]", "-2^(1/2)"), "true");
    assert_eq!(ev("rewrite(sin(x) + cos(x), minimize(sin(x) + cos(x), x)[2]) == -2^(1/2)"), "true");
    assert_eq!(ev("maximize([sin(x), -1/2 <= cos(x) and cos(x) <= 1/2], x)"), "[1, [x -> (1/2)*pi]]");
    assert_eq!(ev("min_value([sin(x), -1/2 <= cos(x) and cos(x) <= 1/2], x)"), "-1");
    // variations: cos(2 x) + 2 sin(x) = 1 - 2 s^2 + 2 s with s = sin(x) in [-1, 1]: maximum 3/2 at s = 1/2, minimum
    // -3 at s = -1; a scaled frequency (period pi)
    assert_eq!(ev("max_value(cos(2*x) + 2*sin(x), x)"), "3/2");
    assert_eq!(ev("min_value(cos(2*x) + 2*sin(x), x)"), "-3");
    assert_eq!(same("max_value(sin(2*x) + cos(2*x), x)", "2^(1/2)"), "true");
}

#[test]
fn optimization_over_a_vector_variable() {
    // documented inputs
    let c = "vector_greater_equal([dot([[1, 2], [1, 0]], v), [3, -1]])";
    assert_eq!(ev(&format!("minimize([dot([1, 1], v), {c}], v)")), "[1, [v -> [-1, 2]]]");
    assert_eq!(ev(&format!("arg_max([dot([-1, -1], v), {c}], v)")), "[-1, 2]");
    assert_eq!(ev(&format!("max_value([dot([-1, -1], v), {c}], v)")), "-1");
    // variations: three entries (the dual point y = [1, 0, 1] has A^T y = [2, 1, 1] <= c and b.y = 4, so 4 is the
    // minimum by weak duality), an unbounded problem
    assert_eq!(
        ev("minimize([dot([2, 3, 1], w), vector_greater_equal([dot([[1, 1, 0], [0, 1, 1], [1, 0, 1]], w), [1, 2, 3]])], w)[1]"),
        "4"
    );
    assert_eq!(ev("dot([1, 2, 3], [1, 0, 1]) == 4 and dot(transpose([[1, 1, 0], [0, 1, 1], [1, 0, 1]]), [1, 0, 1]) == [2, 1, 1]"), "true");
    assert_eq!(ev(&format!("maximize([dot([1, 1], v), {c}], v)[1]")), "inf");
}

/// The inverse of a transform made of dirac_comb((w - a)/(2 pi)) terms at the integer m: each term c dirac_comb(...)
/// read back as c exp(im a m) (the synthesis sum), so a forward transform by discrete Fourier sums is checked by its
/// inverse.
fn fst_inverse_at(f_transform: &str, m: i64) -> String {
    format!("simplify(exp_to_trig(expand(rewrite({f_transform}, dirac_comb(u_) :> exp(-2*pi*im*({m})*rewrite(u, w -> 0))))))")
}

#[test]
fn fourier_sequence_transform_of_exponentials_and_periodic_sequences() {
    // documented inputs
    assert_eq!(same("fourier_sequence_transform(1, n, w)", "dirac_comb(w/(2*pi))"), "true");
    assert_eq!(same("fourier_sequence_transform(cos(5*n), n, w)", "(1/2)*(dirac_comb((-5 + w)/(2*pi)) + dirac_comb((5 + w)/(2*pi)))"), "true");
    assert_eq!(
        same("fourier_sequence_transform(sin(n*w0), n, w, assumptions: element(w0, reals))", "-im/2*(dirac_comb((w - w0)/(2*pi)) - dirac_comb((w + w0)/(2*pi)))"),
        "true"
    );
    assert_eq!(
        same("fourier_sequence_transform((1/2)^n1*(1/3)^n2*unit_step(n1, n2), [n1, n2], [w1, w2])", "6*exp(im*(w1 + w2))/((-1 + 2*exp(im*w1))*(-1 + 3*exp(im*w2)))"),
        "true"
    );
    assert_eq!(same("fourier_sequence_transform(a^n*unit_step(n), n, w, fourier_parameters: [1, -2*pi])", "-1/(-1 + a*exp(2*im*pi*w))"), "true");
    // the periodic sequence mod(n, 3) and the variations mod(2 n + 1, 4), cos(2 n)^2: read back by synthesis
    for (f, p) in [("mod(n, 3)", 3), ("mod(2*n + 1, 4)", 4)] {
        let t = ev(&format!("fourier_sequence_transform({f}, n, w)"));
        for m in [0, 1, 2, 3, 7] {
            assert_eq!(ev(&format!("{} == rewrite({f}, n -> {m})", fst_inverse_at(&t, m))), "true", "{f} at {m} (period {p})");
        }
    }
    assert_eq!(
        same("fourier_sequence_transform(cos(2*n)^2, n, w)", "(1/2)*dirac_comb(w/(2*pi)) + (1/4)*(dirac_comb((w - 4)/(2*pi)) + dirac_comb((w + 4)/(2*pi)))"),
        "true"
    );
    // growth (a real exponent) or an undecided frequency: no transform
    assert_eq!(ev("head(fourier_sequence_transform(exp(n), n, w)) === fourier_sequence_transform"), "true");
    assert_eq!(ev("head(fourier_sequence_transform(cos(a*n), n, w)) === fourier_sequence_transform"), "true");
}

#[test]
fn integrals_over_regions_by_cylindrical_decomposition() {
    // documented inputs
    assert_eq!(ev("integrate(boole(x^2 + y^2 <= 1), [x, -1, 1], [y, -1, 1])"), "pi");
    assert_eq!(ev("integrate(boole(-4*x^2 + 4*x^4 + y^2 <= 0), [x, -inf, inf], [y, -inf, inf])"), "8/3");
    assert_eq!(ev("integrate((x^2 + x*y + 3*y^4)*boole(x^2 + y^2 < 1 and y > x), [x, -inf, inf], [y, -inf, inf])"), "(5/16)*pi");
    assert_eq!(ev("integrate(piecewise([[1, x^2 + y^2 < 1]]), [x, -inf, inf], [y, -inf, inf])"), "pi");
    // variations: a triangle (integrate(x (1 - x)^2/2, [x, 0, 1]) = 1/24 by hand), an annulus (pi (4 - 1)), a
    // region with a parameter-free curved side against the numeric integral
    assert_eq!(ev("integrate(x*y*boole(x + y <= 1), [x, 0, 1], [y, 0, 1]) == integrate(x*(1 - x)^2/2, [x, 0, 1])"), "true");
    assert_eq!(ev("integrate(boole(1 <= x^2 + y^2 and x^2 + y^2 <= 4), [x, -inf, inf], [y, -inf, inf])"), "3*pi");
    assert_eq!(
        ev("abs(n(integrate(x^2*boole(y <= 1 - x^2), [x, -1, 1], [y, 0, 1])) - n_integrate(x^2*(1 - x^2), [x, -1, 1])) < 10^-12"),
        "true"
    );
}

#[test]
fn series_in_several_variables() {
    // documented inputs
    assert_eq!(ev("series_coefficient(series(exp(x + y), [x, 0, 3], [y, 0, 3]), [3, 3])"), "1/36");
    assert_eq!(ev("series_coefficient(series(sin(x + y), [x, 0, 3], [y, 0, 3]), [1, 2])"), "-1/2");
    // variations: 1/(1 - x - y) has the coefficients binomial(i + j, i); a single list specification is the
    // one-variable series
    assert_eq!(ev("series_coefficient(series(1/(1 - x - y), [x, 0, 2], [y, 0, 2]), [2, 2]) == binomial(4, 2)"), "true");
    assert_eq!(ev("series_coefficient(series(1/(1 - x - y), [x, 0, 3], [y, 0, 3]), [3, 1]) == binomial(4, 3)"), "true");
    assert_eq!(ev("series(exp(x), [x, 0, 2]) === series(exp(x), x, 0, 2)"), "true");
}

/// The general coefficient of f at x = 0 against the Taylor coefficients from series at the orders a..b.
fn sc_matches_series(f: &str, a: i64, b: i64) -> String {
    ev(&format!(
        "every(range({a}, {b}), j => simplify(rewrite(series_coefficient({f}, [x, 0, n]), n -> j) - series_coefficient({f}, [x, 0, j])) === 0)"
    ))
}

#[test]
fn general_series_coefficients_of_rational_and_hypergeometric_type() {
    // documented inputs
    assert_eq!(ev("series_coefficient(1/(1 + x), [x, 0, n])"), "piecewise([[(-1)^n, n >= 0]], 0)");
    assert_eq!(same("series_coefficient(x^3/((-3 + x)*(-2 + x)*(-1 + x)), [x, 0, n])", "piecewise([[-1/2 - (1/2)*3^(2 - n) + 2^(2 - n), n > 0]], 0)"), "true");
    assert_eq!(ev("series_coefficient(exp(x), [x, 0, n], assumptions: n >= 0)"), "factorial(n)^-1");
    assert_eq!(same("series_coefficient(polylog(-k, x), [x, 0, n])", "piecewise([[n^k, n >= 1]], 0)"), "true");
    // documented inputs and variations against the Taylor coefficients (a second route: series)
    for f in [
        "x^3/((-3 + x)*(-2 + x)*(-1 + x))",
        "1/(x^2 - (3/2)*x + 1)",
        "(2 - x)/(1 - 6*x + 5*x^2)",
        "(x^4 + 1)/((x - 2)^2*(x + 3))",
        "bessel_j(2, 3*x)*x",
        "polylog(-3, 2*x)",
        "hypergeometric_pfq([1, 2], [3, 4, 5], 6*x)",
        "hypergeometric_pfq([1/2, 2], [3], -x/4)",
    ] {
        assert_eq!(sc_matches_series(f, 0, 8), "true", "{f}");
    }
    assert_eq!(sc_matches_series("bessel_i(3, 2*x)/x^7", -8, 6), "true");
}

#[test]
fn series_with_a_pole_takes_enough_taylor_terms() {
    // bessel_i(3, 2 x)/x^7 = x^-4/6 + x^-2/24 + 1/240 + ...: the pole x^-7 lowers the orders of bessel_i's terms
    assert_eq!(same("normal(series(bessel_i(3, 2*x)/x^7, x, 0, -4))", "1/(6*x^4)"), "true");
    assert_eq!(same("normal(series(bessel_i(3, 2*x)/x^7, x, 0, 0))", "1/(6*x^4) + 1/(24*x^2) + 1/240"), "true");
    // variation: against the defining sum of bessel_j(1, x) = sum (-1)^j (x/2)^(2 j + 1)/(j! (j + 1)!)
    assert_eq!(
        same("normal(series(bessel_j(1, x)/x^5, x, 0, 1))", "sum((-1)^j*(1/2)^(2*j + 1)*x^(2*j - 4)/(factorial(j)*factorial(j + 1)), [j, 0, 2])"),
        "true"
    );
}
