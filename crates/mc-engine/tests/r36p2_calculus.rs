//! Round 36: further fixed wrong answers in calculus and equation solving, one test
//! per cause. Each value is also checked by an independent route (a closed form, a direct sum, a substitution).
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

/// Evaluate `src` in a fresh engine, raise the stop flag `after_ms` after the evaluation starts, and return how
/// long the evaluation took in ms (the engine is built before the clock starts).
fn interrupted_after(src: &str, after_ms: u64) -> u128 {
    let (tx, rx) = std::sync::mpsc::channel::<u128>();
    let s = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let stop = StopFlag::new();
            let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
            let st = stop.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(after_ms));
                st.raise();
            });
            let t0 = std::time::Instant::now();
            let _ = e.eval(&s, &stop);
            let _ = tx.send(t0.elapsed().as_millis());
        })
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(60)).unwrap_or(60_000)
}

#[test]
fn an_interrupt_during_the_modular_gcd_of_a_lifting_stops() {
    // the modular gcd over a number field (cell lifting of the decomposition) skipped every prime after a stop, the
    // number kernels returning arbitrary values, and never polled: an interrupt between 3.0 and 3.3 s of this call
    // (the window where the lifting runs, unloaded) never returned. Every point of the run must stop within 1.5 s.
    let src = "zz = function_surjective(x^5 + a*x^4 + b*x^3 + c*x^2 + d*x, x)";
    for after in (0..14).map(|i| 300 + 300 * i) {
        let ms = interrupted_after(src, after) as u64;
        assert!(ms < after + 1500, "interrupt at {after} ms returned at {ms} ms");
    }
}

#[test]
fn a_divergent_double_sum_stays_quickly_and_truncates() {
    // the zeta test simplified k f'(k)/f(k) of a sum of ten cosine terms (18 s); it is only tried on monomials now
    let t0 = std::time::Instant::now();
    let r = ev("sum_u = sum(x * cos(x * y), [x, 1, inf], [y, 1, 10]); head(sum_u)");
    assert_eq!(r, "sum");
    assert!(t0.elapsed().as_secs() < 6, "{:?}", t0.elapsed());
    // against the finite double sum at 25 digits
    assert_eq!(
        ev("sum_u = sum(x * cos(x * y), [x, 1, inf], [y, 1, 10]); abs(truncate_sum(sum_u, 10, working_precision: 20) - n(sum(x*cos(x*y), [x, 1, 10], [y, 1, 10]), 25)) < 10^-18"),
        "true"
    );
    assert_eq!(ev("sum(1/k^2, [k, 1, inf])"), "(1/6)*pi^2");
    assert_eq!(ev("sum(3*(2*k)^-3, [k, 2, inf])"), "(3/8)*(-1 + zeta(3))");
}

#[test]
fn step_functions_on_a_jump_expand_on_their_side() {
    // series of a step function whose argument sits on a jump: a side from the assumptions, or no series
    assert_eq!(ev("series(floor(4 - t), t, 0, 2)"), "series(floor(4 - t), t, 0, 2)");
    assert_eq!(ev("assuming(t > 0, series(fractional_part((2 - t)^2), t, 0, 2))"), "1 - 4*t + t^2 + O(t^3)");
    assert_eq!(ev("assuming(t < 0, series(fractional_part((2 - t)^2), t, 0, 2))"), "-4*t + t^2 + O(t^3)");
    assert_eq!(ev("assuming(t > 0, series(unit_step(-t), t, 0, 2))"), "O(t^3)");
    assert_eq!(ev("series(floor(4 - t^2/2), t, 0, 2)"), "3 + O(t^3)");
    // independent: the values just beside the jump
    assert_eq!(ev("[fractional_part((2 - 10^-6)^2) - (1 - 4*10^-6 + 10^-12), floor(4 - 10^-12/2)]"), "[0, 3]");
}

#[test]
fn asymptotic_relations_respect_the_side_of_a_jump() {
    let fx = "fractional_part(x^2) * sin(x)";
    let cases = [
        (format!("asymptotic_equal({fx}, cos(x / 2), x -> 2, direction: from_below)"), "true"),
        (format!("asymptotic_greater_equal({fx}, cos(x / 2), x -> 2, direction: from_below)"), "true"),
        (format!("asymptotic_less_equal(cos(x / 2), {fx}, x -> 2, direction: from_below)"), "true"),
        (format!("asymptotic_greater({fx}, 4 * cos(x / 2) * (x - 2), x -> 2, direction: from_below)"), "true"),
        (format!("asymptotic_less(4 * cos(x / 2) * (x - 2), {fx}, x -> 2, direction: from_below)"), "true"),
        (format!("asymptotic_equivalent({fx}, 4 * sin(2) * (x - 2), x -> 2, direction: from_below)"), "false"),
        (format!("asymptotic_equivalent({fx}, 4 * sin(2) * (x - 2), x -> 2, direction: two_sided)"), "false"),
        (format!("asymptotic_equivalent({fx}, 4 * sin(2) * (x - 2), x -> 2, direction: from_above)"), "true"),
        ("asymptotic_equivalent(x^4, x^2 * (1 - cos(sqrt(2) * x)) * unit_step(x), x -> 0)".into(), "false"),
        ("asymptotic_equivalent(x^4, x^2 * (1 - cos(sqrt(2) * x)) * unit_step(x), x -> 0, direction: from_below)".into(), "false"),
        ("asymptotic_equivalent(x^4, x^2 * (1 - cos(sqrt(2) * x)) * unit_step(x), x -> 0, direction: from_above)".into(), "true"),
    ];
    for (src, want) in cases {
        assert_eq!(ev(&src), want, "{src}");
    }
    // independent: the ratios at points beside 2 (from below: a nonzero constant sin(2)/cos(1); from above ~ 1)
    assert_eq!(ev("x = 2 - 10^-8; abs(n((fractional_part(x^2) * sin(x)) / cos(x / 2), 20) - n(sin(2)/cos(1), 20)) < 10^-6"), "true");
    assert_eq!(ev("x = 2 + 10^-8; abs(n((fractional_part(x^2) * sin(x)) / (4 * sin(2) * (x - 2)), 20) - 1) < 10^-6"), "true");
}

#[test]
fn the_intersection_of_two_disks_has_the_exact_lens_area() {
    // was the area of the intersection of two 256-gons, 1.228159 (off by 2e-4)
    let r = "R = region_intersection(disk([0, 0], 1), disk([1, 0], 1)); ";
    assert_eq!(ev(&format!("{r}area(R)")), "(-1/2)*3^(1/2) + (2/3)*pi");
    assert_eq!(ev(&format!("{r}simplify(area(R) - (4*pi - 3*sqrt(3))/6)")), "0");
    assert_eq!(ev(&format!("{r}[region_member(R, [1/2, 0]), region_member(R, [-1/2, 0])]")), "[true, false]");
    assert_eq!(ev(&format!("{r}head(discretize_region(R))")), "mesh_region");
    assert_eq!(ev("area(region_intersection(disk([0, 0], 2), disk([1/2, 0], 1)))"), "pi");
    assert_eq!(ev("area(region_intersection(disk([0, 0], 1), disk([3, 0], 1)))"), "0");
    // independent: the two circular segments of r = 2, 1 at distance sqrt(5) by their own formula
    // 4 acos(2/sqrt(5)) + acos(1/sqrt(5)) - 2, and a Monte Carlo free check by the polygon area to 1e-3
    assert_eq!(ev("abs(n(area(region_intersection(disk([0, 0], 2), disk([2, 1], 1))) - (4*acos(2/sqrt(5)) + acos(1/sqrt(5)) - 2), 30)) < 10^-25"), "true");
    assert_eq!(ev("abs(area(region_intersection(disk([0, 0], 2.0), disk([2, 1], 1))) - 0.961739) < 10^-3"), "true");
}

#[test]
fn pade_approximants_of_series_in_fractional_powers() {
    // sin(sqrt(x)) = x^(1/2) (1 - x/6 + ...): x^(1/2) times the [2/3] approximant (was the series itself)
    assert_eq!(
        ev("p = pade_approximant(sin(sqrt(x)), [x, 0, 3]); simplify(p - (x^(1/2) - (2363/18183)*x^(3/2) + (12671/4363920)*x^(5/2))/(1 + (445/12122)*x + (601/872784)*x^2 + (121/16662240)*x^3))"),
        "0"
    );
    // independent: the approximant agrees with the series to the order 2 + 3 (x^(1/2) x^5), not beyond
    assert_eq!(
        ev("e = expand(normal(series((pade_approximant(sin(sqrt(x)), [x, 0, 3]) - sin(sqrt(x)))/sqrt(x), x, 0, 7))); [coefficient(e, x, 5), coefficient(e, x, 6) == 0]"),
        "[0, false]"
    );
    assert_eq!(ev("pade_approximant(exp(x), [x, 0, 2])"), "(1 + (1/12)*x^2 + (1/2)*x)/(1 + (-1/2)*x + (1/12)*x^2)");
    assert_eq!(ev("pade_approximant(1 + sqrt(x), [x, 0, 2])"), "failure(unevaluated, pade_approximant, \"the series mixes fractional powers of several classes\")");
}

#[test]
fn symbolic_series_coefficients_vanish_below_the_first_order() {
    assert_eq!(ev("series_coefficient(x / (x - 5), [x, 0, n])"), "piecewise([[-5^-n, n > 0]], 0)");
    assert_eq!(ev("series_coefficient(x^2/(1 - x), [x, 0, n])"), "piecewise([[1, n > 1]], 0)");
    // independent: against the coefficients of the series at the orders -2 .. 6
    for f in ["x/(x - 5)", "x^2/(1 - x)", "x*exp(3*x)", "1/(1 - x)"] {
        assert_eq!(
            ev(&format!("c = series_coefficient({f}, [x, 0, n]); s = normal(series({f}, x, 0, 6)); table((c /. n -> k) - (if k < 0 then 0 else coefficient(s, x, k)), [k, -2, 6])")),
            "[0, 0, 0, 0, 0, 0, 0, 0, 0]",
            "{f}"
        );
    }
}

#[test]
fn convexity_where_the_domain_leaves_out_a_jump_or_a_kink() {
    assert_eq!(ev("function_convexity([heaviside_theta(x), x != 0], x)"), "indeterminate");
    assert_eq!(ev("function_convexity([sign(x), x != 0], x)"), "indeterminate");
    assert_eq!(ev("function_convexity([abs(x), x != 0], x)"), "1");
    assert_eq!(ev("function_convexity([-abs(x), x != 0], x)"), "-1");
    assert_eq!(ev("function_convexity([x^2, x != 0], x)"), "1");
    assert_eq!(ev("limit(heaviside_theta(x), x -> 0, direction: from_above)"), "1");
    // independent: midpoints in the domain break both inequalities (above the chord of -1, 3; below that of -3, 1)
    assert_eq!(
        ev("[heaviside_theta(1) > (heaviside_theta(-1) + heaviside_theta(3))/2, heaviside_theta(-1) < (heaviside_theta(-3) + heaviside_theta(1))/2]"),
        "[true, true]"
    );
}

#[test]
fn asymptotic_sums_over_an_infinite_range_in_a_parameter() {
    // was the divergent term-by-term sum(1 - a k + ..., [k, 0, inf])
    assert_eq!(ev("asymptotic_sum(e^(-a * k), [k, 0, inf], [a, 0, 3])"), "1/2 + (-1/720)*a^3 + (1/12)*a + a^-1");
    // independent: 1/(1 - e^-a) at a = 0.7 against the four terms (error of the order a^5)
    assert_eq!(ev("abs(n(1/(1 - e^-0.7)) - rewrite(asymptotic_sum(e^(-a * k), [k, 0, inf], [a, 0, 3]), a -> 0.7)) < 10^-4"), "true");
    // term by term where every coefficient sum converges; a decline where none does and no closed form exists
    assert_eq!(ev("asymptotic_sum(1/(k^2 + a), [k, 1, inf], [a, 0, 2])"), "(1/6)*pi^2 + (-1/90)*a*pi^4 + (1/945)*a^2*pi^6");
    assert_eq!(ev("asymptotic_sum(exp(-a*k^2), [k, 0, inf], [a, 0, 2])"), "failure(unevaluated, asymptotic_sum, \"the expansion of the summand does not sum term by term over the infinite range\")");
}

#[test]
fn infinite_products_with_integer_gamma_pairs_and_series_term_goals() {
    // the reflection formula was tried on gamma(1) gamma(3) and divided by sin(pi) = 0
    assert_eq!(ev("product((k^2 - 1)/(k^2 + 2), [k, 2, inf])"), "(3/2)*pi*csch(pi*2^(1/2))*2^(1/2)");
    assert_eq!(ev("abs(n(product((k^2 - 1)/(k^2 + 2), [k, 2, inf])) - n_product((k^2 - 1)/(k^2 + 2), [k, 2, inf])) < 10^-12"), "true");
    assert_eq!(
        ev("rule f(a_) := (k^2 - 1) / (k^2 + a); asymptotic_product(f(a), [k, 2, inf], [a, 0, 2])"),
        "1/2 + a*(1/2 + (-1/12)*pi^2) + ((-1/12)*pi^2 + (7/720)*pi^4)*a^2"
    );
    assert_eq!(
        ev("rule f(a_) := (k^2 - 1) / (k^2 + a); abs(rewrite(asymptotic_product(f(a), [k, 2, inf], [a, 0, 2]), [a -> 0.02]) - product(f(0.02), [k, 2, inf])) < 10^-6"),
        "true"
    );
    // series_term_goal: 2 with n -> inf: three terms of Stirling's series; at 20 against 20! to 4e-7 (the next term, 139/(51840 n^3), is 3.4e-7)
    assert_eq!(
        ev("asymptotic_product(k, [k, 2, n_u], n_u -> inf, series_term_goal: 2)"),
        "(1 + (1/288)*n_u^-2 + (1/12)/n_u)*2^(1/2)*exp(-n_u)*n_u^(1/2 + n_u)*pi^(1/2)"
    );
    assert_eq!(ev("abs(rewrite(asymptotic_product(k, [k, 2, n_u], n_u -> inf, series_term_goal: 2), [n_u -> 20.])/factorial(20) - 1) < 4*10^-7"), "true");
}

#[test]
fn a_machine_overflow_and_the_symbol_inf_combine() {
    // an overflowed machine real minus inf was left as inf - inf, and divided by inf gave 0
    assert_eq!(ev("x = 1.0e300*1.0e100; y = n_product(k + 0.03, [k, 1, 200]); (x - y)/y"), "failure(indeterminate, machine_real(\"inf\") - inf)");
    assert_eq!(ev("inf + 2.0"), "inf");
}

#[test]
fn laplace_method_for_integrals_at_infinity() {
    // was series(integrate(...), x, inf, k) unevaluated; each against n_integrate at a large x, the error of the order
    // of the first term left out
    let cases = [
        ("asymptotic_integrate(e^(-ω * (x^2 - 2 * x)), [x, 0, inf], [ω, inf, 1])", "exp(ω)*pi^(1/2)/ω^(1/2)"),
        ("asymptotic_integrate((1 + t)^(5 / 2) / e^(x * (t^2 + t)), [t, 0, 2], [x, inf, 1])", "x^-1"),
        ("asymptotic_integrate(exp(-x * (t + 1 / t)) / (t + 1), [t, 1, 2], [x, inf, 1])", "exp(-2*x)*pi^(1/2)/(4*x^(1/2))"),
        ("asymptotic_integrate(exp(-x * (t^2 - 2 * t)) / (t + 1), [t, 0, 3], [x, inf, 1])", "exp(x)*pi^(1/2)/(2*x^(1/2))"),
    ];
    for (src, want) in cases {
        assert_eq!(ev(&format!("simplify(({src}) - ({want}))")), "0", "{src}");
    }
    // seven terms at an end where phi' = 0 (Watson's lemma in t^2), against the expected coefficients
    assert_eq!(
        ev("simplify(asymptotic_integrate(exp(-x * t^2) / (t + 2), [t, 0, 2], [x, inf, 7]) - (15/2048*pi^(1/2)*x^(-7/2) - 1/64/x^3 + 3/256*pi^(1/2)*x^(-5/2) - 1/32/x^2 + pi^(1/2)*x^(-3/2)/32 - 1/8/x + pi^(1/2)*x^(-1/2)/4))"),
        "0"
    );
    let num = [
        ("asymptotic_integrate(exp(-x * t^2) / (t + 2), [t, 0, 2], [x, inf, 7])", "n_integrate(exp(-300 * t^2) / (t + 2), [t, 0, 2])", "300", "10^-11"),
        ("asymptotic_integrate(exp(x * sin(t)), [t, 0, pi], [x, inf, 2])", "n_integrate(exp(200*sin(t)), [t, 0, pi])/exp(200)", "200", "10^-6"),
        ("asymptotic_integrate(exp(-x*cosh(t)), [t, -inf, inf], [x, inf, 2])*exp(2*x)", "2*bessel_k(0, 200.)*exp(200)", "200", "10^-6"),
        ("asymptotic_integrate(exp(-x*t^4), [t, -1, 1], [x, inf, 1])", "n_integrate(exp(-1000*t^4), [t, -1, 1])", "1000", "10^-10"),
    ];
    for (a, b, x, tol) in num {
        let scaled = if b.contains("exp(200)") { format!("({a})/exp(x)") } else { a.to_string() };
        assert_eq!(ev(&format!("abs(n(rewrite({scaled}, x -> {x})) - ({b})) < {tol}")), "true", "{a}");
    }
    // a minimum that is not single, or a phi of no known form: no expansion (unevaluated as before)
    assert_eq!(ev("head(asymptotic_integrate(exp(-x*sin(t)^2), [t, -pi, pi], [x, inf, 1]))"), "series");
}

#[test]
fn asymptotic_integrals_of_antiderivatives_without_closed_form() {
    assert_eq!(ev("asymptotic_integrate(x^x, x, x -> 0)"), "x");
    assert_eq!(
        ev("simplify(asymptotic_integrate(x^x^x, x, [x, 0, 4]) - ((1/2)*x^2 + (1/27)*(2 - 6*log(x) + 9*log(x)^2)*x^3 + (1/8)*x^4*log(x)^4))"),
        "0"
    );
    // independent: against the numeric integral from 0 to 0.0005 (the terms left out are of the order x^5 log(x)^6)
    assert_eq!(ev("abs(n(rewrite(asymptotic_integrate(x^x^x, x, [x, 0, 4]), x -> 0.0005)) - n_integrate(x^x^x, [x, 0, 0.0005])) < 10^-12"), "true");
}

#[test]
fn asymptotic_solve_declines_where_a_puiseux_expansion_is_needed() {
    let declined = "failure(unevaluated, asymptotic_solve, \"a branch through a singular point or to infinity needs a Puiseux expansion\")";
    for src in [
        "asymptotic_solve(x * y^4 - (x + 1) * y^2 + x == 1, [y], [x, 0, 3])",
        "asymptotic_solve(x * y^4 - (x + 1) * y^2 + x == 1, [y], [x, 0, 3], reals)",
        "asymptotic_solve(y^5 - x * y + x^2 == 0, [y, 0], [x, 0, 5])",
        "asymptotic_solve(sin(x * y) - y^2 + x == 0, [y, 0], [x, 0, 5])",
        "asymptotic_solve(x * y^4 + y^2 - x^2 * y == 1, [y], [x, 0, 3])",
        "asymptotic_solve(y^2 - t * x + t == 0 and x^2 + t * y == 0, [[x, y], [0, 0]], [t, 0, 2])",
        "asymptotic_solve(y^3 - t * x * y == 0 and x^2 - t^2 * y == 1, [[x, y]], [t, 0, 3])",
    ] {
        assert_eq!(ev(src), declined, "{src}");
    }
    // regular points still expand; reals keeps the real branches
    assert_eq!(ev("asymptotic_solve(y^2 + 1 - x == 0, [y], [x, 0, 2], reals)"), "[]");
    assert_eq!(ev("length(asymptotic_solve(y^2 - 1 - x == 0, [y], [x, 0, 2], reals))"), "2");
    // independent: the branch through 1 against sqrt(1 + x) to the order 2
    assert_eq!(ev("expand(rewrite(y, asymptotic_solve(y^2 - 1 - x == 0, [y, 1], [x, 0, 2])[1]) - normal(series(sqrt(1 + x), x, 0, 2)))"), "0");
}

#[test]
fn stability_of_recurrences_with_periodic_coefficients_declines() {
    // was a condition from the eigenvalues at one step ("stable"); the products over two steps are 1/4 and 9/4
    assert_eq!(
        ev("r_stability_conditions([x(n_u + 1) == (1 / 2) * (2 + (-1)^n_u) * y(n_u), y(n_u + 1) == (1 / 2) * (2 - (-1)^n_u) * x(n_u)], [x, y], n_u)"),
        "failure(unevaluated, r_stability_conditions, \"the equations are not autonomous: their coefficients depend on the variable\")"
    );
    // independent: iterating from (1, 1) the odd-step component grows like (9/4)^(n/2)
    assert_eq!(ev("v = nest(p => [(1/2)*(2 + (-1)^p[3])*p[2], (1/2)*(2 - (-1)^p[3])*p[1], p[3] + 1], [1, 1, 0], 40); abs(v[1]) + abs(v[2]) > 10^6"), "true");
    assert_eq!(ev("r_stability_conditions(x(n + 1) == x(n)^2 + 3*x(n), x, n)"), "[[[-2], true], [[0], false]]");
    // a coefficient b^n whose linearization at the point is 0 still decides
    assert_eq!(ev("r_stability_conditions(a(n_u + 2) == b^n_u * a(n_u + 1) * a(n_u)^2, a(n_u), n_u)"), "[[[0, 0], true]]");
}

#[test]
fn derivatives_of_inactive_transforms() {
    let e = "e_u = inactive(laplace_transform)(a*t^2, t, s); ";
    assert_eq!(ev(&format!("{e}activate(d(e_u, s))")), "-6*a*s^-4");
    assert_eq!(ev(&format!("{e}d(e_u, t)")), "0");
    assert_eq!(ev("d(inactive(fourier_transform)(f(t), t, w), w)"), "inactive(fourier_transform)(1im*t*f(t), t, w)");
    assert_eq!(ev("d(inactive(fourier_sin_transform)(f(t), t, w), w)"), "inactive(fourier_cos_transform)(t*f(t), t, w)");
    assert_eq!(ev("d(inactive(convolve)(f(t), g(t), t, s), s)"), "inactive(convolve)(f(t), g'(t), t, s)");
    // independent: against the derivative of the evaluated transforms of exp(-t^2) at w = 0.7
    for tr in ["fourier_transform", "fourier_cos_transform", "fourier_sin_transform", "laplace_transform"] {
        assert_eq!(
            ev(&format!("g = exp(-t^2); abs(n(activate(d(inactive({tr})(g, t, w), w)) /. w -> 0.7) - n(d({tr}(g, t, w), w) /. w -> 0.7)) < 10^-12")),
            "true",
            "{tr}"
        );
    }
}

#[test]
fn small_cases_indeterminate_complex_infinity_and_order_conditions() {
    assert_eq!(ev("d(indeterminate, z)"), "indeterminate");
    assert_eq!(ev("limit((x - 1) / (2 * x + 3), x -> complex_infinity)"), "1/2");
    assert_eq!(ev("limit(x^2/(x + 1), x -> complex_infinity)"), "complex_infinity");
    assert_eq!(ev("limit(1/(x^2 + 1), x -> complex_infinity)"), "0");
    // independent: the value along the imaginary axis at |x| = 10^8
    assert_eq!(ev("abs(n(((x - 1) / (2 * x + 3)) /. x -> im*10^8) - 1/2) < 10^-7"), "true");
    // an order relation makes the unknown real: only the real branch of acoth + i pi c(1)
    assert_eq!(ev("rule f(x_) := coth(x) - log(3); solve(f(x) == 0 and 0 < x, x)"), "[[x -> acoth(log(3))]]");
    assert_eq!(ev("solve(exp(x) == 2 and 0 < x, x)"), "[[x -> log(2)]]");
    assert_eq!(ev("abs(n(coth(acoth(log(3))) - log(3), 30)) < 10^-25"), "true");
}

#[test]
fn the_spot_check_of_full_simplify_is_reproducible() {
    // the points of the check were seeded by the term's handle: 0.15 s to 3 s from run to run
    let t0 = std::time::Instant::now();
    assert_eq!(ev("full_simplify(polylog(2, 1 - z) + polylog(2, z) - zeta(2))"), "-1*log(z)*log(1 - z)");
    assert!(t0.elapsed().as_secs() < 10);
}

#[test]
fn cardano_on_a_negative_s() {
    // independent: every value substituted into its cubic is 0 to 25 digits
    for p in ["x^3 - 2*x + 7", "x^3 - 2*x - 7", "2*x^3 + 3*x^2 - x + 11", "x^3 + x + 17"] {
        assert_eq!(
            ev(&format!("r = roots({p} == 0, x); map(v => re(v)^2 + im_part(v)^2 < 10^-60, map(q => rewrite({p}, x -> n(q[2], 40)), apply(list, r)))")),
            "[true, true, true]",
            "{p}"
        );
    }
    assert_eq!(ev("r = root(&(&1^3 - 2 * &1 + 7), 1); v = n(to_radicals(r), 40) - n(r, 40); re(v)^2 + im_part(v)^2 < 10^-60"), "true");
}

/// Probe: `P2_SRC` interrupted at each of `P2_AFTER` (comma-separated ms); prints the times.
#[test]
#[ignore]
fn probe_interrupt() {
    let src = std::env::var("P2_SRC").unwrap_or_default();
    let after = std::env::var("P2_AFTER").unwrap_or_else(|_| "5500".into());
    for a in after.split(',') {
        let a: u64 = a.trim().parse().unwrap();
        let ms = interrupted_after(&src, a);
        eprintln!("PROBE interrupt at {a} ms, returned at {ms} ms");
    }
}

/// Probe: evaluate `P2_SRC` and print the result.
#[test]
#[ignore]
fn probe_eval() {
    let src = std::env::var("P2_SRC").unwrap_or_default();
    let t0 = std::time::Instant::now();
    let out = ev(&src);
    eprintln!("PROBE {} ms: {out}", t0.elapsed().as_millis());
}
