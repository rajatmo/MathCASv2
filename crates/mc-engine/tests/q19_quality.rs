//! Round 19 quality checks: guards and known bugs found by reviewing wrong answers and by property checks of the
//! round 15-18 code (solve, reduce, sums, limits, definite integration, resultants, number fields, numeric
//! optimisation and oscillatory quadrature).
//!
//! The guard tests pass today and pin behaviour found correct (tight interval enclosures, `n_sum` and `n_product`
//! of shifted peaks; the stored numbers are expected values). Each known bug is one ignored
//! test named after the bug's id (`Q19-NN`); the expected values are the mathematically correct
//! ones, so a fix removes the `#[ignore]` and, where the printed form differs, adjusts only the form.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate each `(source, printed result)` pair in order in one engine session (two threads).
fn check(cases: &[(&str, &str)]) {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() });
    let mut bad = vec![];
    for (src, want) in cases {
        let got = match e.eval(src, &StopFlag::new()) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        if got != *want {
            bad.push(format!("{src}\n   got  {got}\n   want {want}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// A 64-bit linear congruential generator (Knuth's MMIX constants): reproducible inputs without a crate.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % ((hi - lo + 1) as u64)) as i64
    }
}

// ------------------------------------------------------------------------------------------------ guards

#[test]
fn polynomial_solve_reduce_and_refine() {
    check(&[
        ("solve(x^3 + (7/2)*x^2 - 7*x + 5/2 == 0, x)", "[[x -> -5], [x -> 1/2], [x -> 1]]"),
        ("solve(x^4 - 5*x^2 + 4 == 0, x)", "[[x -> -2], [x -> -1], [x -> 1], [x -> 2]]"),
        ("solve(x^2 + 10/3*x + 8/3 == 0, x)", "[[x -> -2], [x -> -4/3]]"),
        ("reduce(x^3 - x > 0, x, reals)", "-1 < x and x < 0 or 1 < x"),
        ("reduce((x - 1)^2*(x + 2) <= 0, x, reals)", "x <= -2 or x == 1"),
        ("refine(sqrt(x^2), x < 0)", "-x"),
        ("refine(abs(x)/x, x > 0)", "1"),
        ("refine(cos(x*pi), element(x, integers))", "(-1)^x"),
        ("refine(sqrt((x - 1)^2), x < 1)", "1 - x"),
    ]);
}

#[test]
fn recurrences_sums_and_limits() {
    check(&[
        ("r_solve([a(n + 1) == 2*a(n) + 3, a(0) == 1], a(n), n)", "[[a(n) -> -3 + 2^(2 + n)]]"),
        ("r_solve([a(n + 2) - 4*a(n + 1) + 4*a(n) == 0, a(0) == 1, a(1) == 2], a(n), n)", "[[a(n) -> 2^n]]"),
        ("sum(k^3, [k, 1, n])", "(1/4)*n^2*(1 + n)^2"),
        ("sum(1/(k*(k + 1)), [k, 1, n])", "n/(1 + n)"),
        ("sum(k*2^k, [k, 1, n])", "2 - 2^(1 + n) + n*2^(1 + n)"),
        ("sum(binomial(n, k)^2, [k, 0, n])", "binomial(2*n, n)"),
        ("sum(1/k^4, [k, 1, inf])", "(1/90)*pi^4"),
        ("sum(1/(2*k - 1)^2, [k, 1, inf])", "(1/8)*pi^2"),
        ("sum((-1)^(k + 1)/k, [k, 1, inf])", "log(2)"),
        ("limit((1 - cos(x))/x^2, x -> 0)", "1/2"),
        ("limit((tan(x) - sin(x))/x^3, x -> 0)", "1/2"),
        ("limit(x^(1/x), x -> inf)", "1"),
        ("limit(sqrt(x^2 + 3*x) - x, x -> inf)", "3/2"),
    ]);
}

#[test]
fn definite_and_indefinite_integrals() {
    check(&[
        ("integrate(x*exp(-x), [x, 0, inf])", "1"),
        ("integrate(cos(4*x)*exp(-x), [x, 0, inf])", "1/17"),
        ("integrate(exp(-2*x^2), [x, -inf, inf])", "(1/2)*2^(1/2)*pi^(1/2)"),
        ("integrate(1/(2 + cos(x)), [x, 0, 2*pi])", "(2/3)*pi*3^(1/2)"),
        ("integrate(log(x)/(1 + x^2), [x, 0, inf])", "0"),
        ("simplify(d(integrate(x*exp(x)*cosh(x), x), x) - x*exp(x)*cosh(x))", "0"),
        ("simplify(d(integrate(x^(1/3)*x/(x^2 + 1), x), x) - x^(1/3)*x/(x^2 + 1))", "0"),
    ]);
}

#[test]
fn resultants_and_minimal_polynomials() {
    check(&[
        ("resultant(x^2 + 1, x^3 + 2, x)", "5"),
        ("subresultants(x^3 + 2, x^2 + 1, x)", "[5, -1, 1]"),
        ("minimal_polynomial(sqrt(2) + sqrt(3), x) == x^4 - 10*x^2 + 1", "true"),
        ("minimal_polynomial(2^(1/3) + 1, x) == x^3 - 3*x^2 + 3*x - 3", "true"),
    ]);
}

/// Numerics with tight errors against expected values: the shifted peak of `n_sum`/`n_product` at k = 20, and
/// tight interval enclosures.
#[test]
fn numerics_with_tight_errors() {
    check(&[
        ("abs(n_sum(1/((k - 20)^2 + 1), [k, 0, inf]) - 3.1046158627214987) < 10^-12", "true"),
        ("abs(n_product(1 + 1/(1 + (k - 20)^2), [k, 0, inf]) - 12.90131515580485) < 10^-9", "true"),
        ("abs(n_integrate(cos(x^2), [x, 0, inf]) - sqrt(pi/8)) < 10^-10", "true"),
        ("abs(n_integrate(sin(3*x)/x, [x, 0, inf]) - pi/2) < 10^-9", "true"),
        ("n_minimize(x^4 - 2*x^3 + 6*x^2 + 6*x, x)", "[-1.286406839855701, [x -> -0.39914487741277127]]"),
        // machine ends one ulp outwards (round 36): a rounded-to-nearest end can lie inside the true range, so the
        // enclosure takes the next machine number out
        ("chebyshev_t(2, interval([1.9, 2.1]))", "interval([6.219999999999999, 7.820000000000001])"),
        ("beta_regularized(interval([1.2, 2.1]), 2, 1)", "interval([1.4399999999999997, 4.410000000000001])"),
        ("erf(interval([-2.1, -1.9]))", "interval([-0.9970205333436671, -0.9927904292352574])"),
    ]);
}

/// Property: a polynomial with random rational roots (repeated ones included) is solved into exactly as many
/// solutions, each of which substitutes to zero.
#[test]
fn solve_random_polynomials_property() {
    let mut r = Lcg(19);
    let mut cases = vec![];
    for _ in 0..30 {
        let n = r.range(1, 5);
        let factors: Vec<String> = (0..n)
            .map(|_| {
                let (p, q) = (r.range(-5, 5), r.range(1, 3));
                format!("({q}*x - ({p}))")
            })
            .collect();
        let p = format!("expand({})", factors.join("*"));
        cases.push((format!("length(solve({p} == 0, x))"), n.to_string()));
        let zeros = vec!["0"; n as usize].join(", ");
        cases.push((format!("map(&(expand(rewrite({p}, &1))), solve({p} == 0, x))"), format!("[{zeros}]")));
    }
    let refs: Vec<(&str, &str)> = cases.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    check(&refs);
}

/// Property: the closed form of a polynomial sum agrees with the explicit sum for n = 1..6.
#[test]
fn sum_random_polynomials_property() {
    let mut r = Lcg(1919);
    let mut cases = vec![];
    for _ in 0..12 {
        let cs: Vec<i64> = (0..4).map(|_| r.range(-4, 4)).collect();
        let f = format!("({})*k^3 + ({})*k^2 + ({})*k + ({})", cs[0], cs[1], cs[2], cs[3]);
        for m in 1..=6 {
            cases.push((format!("rewrite(sum({f}, [k, 1, n]), n -> {m}) - sum({f}, [k, 1, {m}])"), "0".to_string()));
        }
    }
    let refs: Vec<(&str, &str)> = cases.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    check(&refs);
}

/// Property: antiderivatives of random elementary integrands differentiate back (checked numerically at two points).
#[test]
fn integrate_random_elementary_property() {
    const ELEM: &[&str] = &["sin(x)", "cos(x)", "exp(x)", "log(x)", "x^2", "sqrt(x)", "1/(1 + x^2)", "1/(x + 2)",
        "exp(2*x)", "atan(x)", "x*exp(x)", "x*sin(x)", "sinh(x)", "x/(x^2 + 1)", "sqrt(1 + x)", "tan(x)"];
    let mut r = Lcg(77);
    let mut cases = vec![];
    for _ in 0..20 {
        let a = ELEM[r.range(0, ELEM.len() as i64 - 1) as usize];
        let b = ELEM[r.range(0, ELEM.len() as i64 - 1) as usize];
        let f = if r.range(0, 1) == 0 { format!("({a}) + ({b})") } else { format!("({a})*({b})") };
        for p in ["37/100", "13/10"] {
            cases.push((
                format!("{{ let g = integrate({f}, x); is_failure(g) or head(g) === integrate or abs(n(rewrite(d(g, x) - ({f}), x -> {p}))) < 10^-9 }}"),
                "true".to_string(),
            ));
        }
    }
    let refs: Vec<(&str, &str)> = cases.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    check(&refs);
}

#[test]
fn q19_37_reduce_equations_over_the_complexes() {
    check(&[
        ("reduce(x^2 + 1 == 0, x)", "x == -1im or x == 1im"),
        ("reduce(x^2 == 4, x)", "x == -2 or x == 2"),
        ("reduce(x^2 - 1 > 0, x)", "x < -1 or 1 < x"),
    ]);
}

#[test]
fn q19_38_reduce_one_variable_with_abs_and_radicals() {
    check(&[
        ("reduce(abs(x - 1) < 2, x, reals)", "-1 < x and x < 3"),
        ("reduce(sqrt(x) < 2, x, reals)", "0 <= x and x < 4"),
    ]);
}

// ------------------------------------------------------------------------------------- known bugs (Q19-NN)

#[test]
fn q19_01_n_integrate_boole_region() {
    check(&[(
        "abs(n_integrate(boole(x^2 + 2*y^2 + 3*z^2 < 1 and -1 < x and x < y and y < z and z < 1), [x, -1, 1], [y, -1, 1], [z, -1, 1]) - 0.30132771) < 10^-3",
        "true",
    )]);
}

#[test]
fn q19_02_special_functions_nan() {
    check(&[
        ("hermite_h(3., 5)", "940.0"),
        ("abs(elliptic_nome_q(-2.) + 0.06823782774533839) < 10^-12", "true"),
        ("abs(n(dawson_f(5 + pi*im)) - (0.0715080295058132 - 0.0462682549744226*im)) < 10^-10", "true"),
        ("abs(n(gamma_regularized(2 + 3*im, 4 - im)) - (-1.1522398554329465 - 0.23324343668111255*im)) < 10^-10", "true"),
    ]);
}

#[test]
fn q19_03_kelvin_ker_kei_numeric() {
    check(&[
        ("abs(kelvin_ker(2.5) + 0.06968797258904535) < 10^-12", "true"),
        ("abs(kelvin_kei(2.5) + 0.1106960991556748) < 10^-12", "true"),
    ]);
}

#[test]
fn q19_04_function_properties_piecewise() {
    check(&[
        ("function_monotonicity(floor(x), x)", "1"),
        ("function_monotonicity(unit_step(x), x)", "1"),
        ("function_convexity(abs(x), x)", "1"),
        ("function_injective(logistic_sigmoid(x), x)", "true"),
        ("function_analytic(tanh(x), x, complexes)", "false"),
        ("function_analytic(asinh(x), x, complexes)", "false"),
    ]);
}

#[test]
fn q19_05_polynomial_lcm_options() {
    check(&[("polynomial_lcm(x^2 - 2, x - sqrt(2), extension: automatic)", "-2 + x^2")]);
}

#[test]
fn q19_06_modulus_option() {
    check(&[
        ("polynomial_gcd((x^2 - 1)^2, x^3 - 1, modulus: 3) === 1", "false"),
        ("apart((x^2 + 1)/(x + 1), modulus: 2)", "1 + x"),
        ("reduce(x^2 == 2, x, modulus: 7)", "x == 3 or x == 4"),
    ]);
}

#[test]
fn q19_07_indefinite_sum_of_periodic() {
    check(&[("{ let s = sum(mod(k^2, 2), k); rewrite(s, k -> 5) - rewrite(s, k -> 4) }", "0")]);
}

#[test]
fn q19_08_is_square_free_polynomials() {
    check(&[("is_square_free(x^2 - 3)", "true"), ("is_square_free(6 + 6*x + x^2)", "true")]);
}

#[test]
fn q19_09_infinity_and_indeterminate_arithmetic() {
    check(&[
        ("complex_infinity/complex_infinity", "indeterminate"),
        ("[1, 2, 3] + indeterminate", "[indeterminate, indeterminate, indeterminate]"),
        ("polygamma(complex_infinity)", "indeterminate"),
    ]);
}

#[test]
fn q19_10_limit_direction_on_branch_cut() {
    check(&[("im_part(limit(log(-10 + t*im), t -> 0, direction: from_below)) == -pi", "true")]);
}

#[test]
#[ignore = "Q19-11: the comparisons are fixed; is_positive(im) is decided by mc-simplify's assumption wrapper (open there)"]
fn q19_11_exact_comparisons() {
    check(&[
        ("sqrt(2) + sqrt(3) == sqrt(5 + 2*sqrt(6))", "true"),
        ("root(&(-2 + &1^4), 1) < 1/10", "true"),
        ("is_positive(im)", "false"),
    ]);
}

#[test]
fn q19_12_discrete_shift_bound_variable() {
    check(&[("discrete_shift(sum(f(i), [i, a, b]), i)", "sum(f(i), [i, a, b])")]);
}

#[test]
fn q19_13_subresultants_degree_order() {
    check(&[("subresultants(x^2 + 1, x^3 + 2, x)", "[5, -1, 1]")]);
}

#[test]
fn q19_14_n_of_huge_exact_power() {
    check(&[("n((1 + 10^-12)^(10^12), 20) > 2.71", "true")]);
}

#[test]
fn q19_15_n_of_exact_zero() {
    check(&[("is_failure(n(rewrite(x^2 - 2*x - 5, x -> 1 + sqrt(6)), 30))", "false")]);
}

#[test]
fn q19_16_n_integrate_sinc_squared() {
    check(&[
        ("abs(n_integrate(sin(x)^2/x^2, [x, 0, inf]) - pi/2) < 10^-8", "true"),
        ("abs(n_integrate(sin(8*x)^2/x^2, [x, 0, inf]) - 4*pi) < 10^-8", "true"),
    ]);
}

#[test]
fn q19_17_n_integrate_boole_disk_hangs() {
    check(&[("abs(n_integrate(boole(x^2 + y^2 < 1), [x, -1, 1], [y, -1, 1]) - pi) < 10^-4", "true")]);
}

#[test]
fn q19_18_solve_abs_and_log_sums() {
    check(&[
        ("solve(abs(x - 5) == 4, x)", "[[x -> 1], [x -> 9]]"),
        ("length(solve(log(x) + log(x + 4) == 2, x))", "1"),
    ]);
}

#[test]
fn q19_19_special_function_derivatives() {
    check(&[
        ("d(jacobi_amplitude(x, m), x)", "jacobi_dn(x, m)"),
        ("simplify(d(logistic_sigmoid(x), x) - logistic_sigmoid(x)*(1 - logistic_sigmoid(x)))", "0"),
        ("simplify(d(hypergeometric_u(a, b, x), x) + a*hypergeometric_u(1 + a, 1 + b, x))", "0"),
    ]);
}

#[test]
fn q19_20_count_heads() {
    check(&[("count(1 + a + b^2, _, [-1], heads: true)", "6")]);
}

#[test]
fn q19_21_free_of_subsum() {
    check(&[("free_of(a + b + c, a + c)", "false")]);
}

#[test]
fn q19_22_association_updates() {
    check(&[
        ("prepend(assoc(1 -> a, 2 -> b), 2 -> c)", "assoc(2 -> c, 1 -> a)"),
        ("key_sort_by(assoc(1 -> x, 2 -> y, 3 -> z), minus)", "assoc(3 -> z, 2 -> y, 1 -> x)"),
    ]);
}

#[test]
fn q19_23_maximal_by_symbolic_key() {
    check(&[("maximal_by([[a, 1], [b, 1], [b, 3], [d, 2], [a, 3]], first)", "[[d, 2]]")]);
}

#[test]
fn q19_24_numerator_denominator() {
    check(&[("numerator(3/7 + im/11) == 33 + 7*im", "true"), ("denominator(3/7 + im/11)", "77"), ("denominator(1/(a^b*x))", "x*a^b")]);
}

#[test]
fn q19_25_coefficient_symbolic_power() {
    check(&[("coefficient(x^s*x, x^s)", "x")]);
}

#[test]
fn q19_26_sum_evaluates_body_per_term() {
    check(&[("reap(sum(sow(i^2) + 1, [i, 10]))", "[395, [[1, 4, 9, 16, 25, 36, 49, 64, 81, 100]]]")]);
}

#[test]
fn r20_08_circle_points_with_a_radius_and_start_angle() {
    check(&[
        ("circle_points([1, 0], 4)", "[[1, 0], [0, 1], [-1, 0], [0, -1]]"),
        ("circle_points([2, 0], 4)", "[[2, 0], [0, 2], [-2, 0], [0, -2]]"),
    ]);
}

#[test]
fn r21_g3_table_with_a_bare_range_and_count_says_so() {
    check(&[("table(k^2, k, 1, 3)", "[[1, 1, 1]]")]);
}

#[test]
#[ignore = "Q19-27: take of a deep array adds a level; circle_points with a radius pair; svd of a singular matrix"]
fn q19_27_list_and_geometry_shapes() {
    check(&[
        ("dimensions(take(array(a, [4, 4, 4, 4]), 3, 2))", "[3, 2, 4, 4]"),
        ("head(svd([[1, 2], [1, 2]])) === list", "true"),
    ]);
}

#[test]
fn q19_28_area_of_implicit_region() {
    check(&[("area(implicit_region(x^2 + y^2 <= 1, [x, y]))", "pi")]);
}

#[test]
fn q19_29_q_factorial_numeric() {
    check(&[("abs(q_factorial(10, 0.7) - 7657.445986770652) < 10^-6", "true")]);
}

#[test]
fn q19_30_spherical_bessel_y_negative() {
    check(&[("abs(spherical_bessel_y(2., -5) + 0.16499545760110443) < 10^-12", "true")]);
}

#[test]
fn q19_31_flat_matching() {
    check(&[("{ set_attributes(ff, flat); match_q(ff(1, 2), ff(_)) }", "true")]);
}

#[test]
fn r22_13_flat_only_matching() {
    check(&[
        ("{ set_attributes(ff, flat); replace(ff(1, 2, 3), ff(x_, y_) -> [x, y]) }", "[1, ff(2, 3)]"),
        ("{ set_attributes(fg, flat); fg(1, fg(2, 3), 4) }", "fg(1, 2, 3, 4)"),
        ("{ set_attributes(fh, flat); fh(3, 1, 2) }", "fh(3, 1, 2)"),
        ("{ set_attributes(fi, flat); match_q(fi(1), fi(_, _)) }", "false"),
        ("{ set_attributes(fj, flat); replace(fj(1, 2, 3), fj(x_, ___) -> x) }", "1"),
        ("attributes(add)", "[flat, listable, orderless]"),
    ]);
}

#[test]
fn q19_32_n_solve_complex_roots() {
    check(&[("length(n_solve(5 == x*2^(x^2), x, max_roots: 3))", "3")]);
}

#[test]
#[ignore = "Q19-33: closed forms missing in sums (alternating zeta, Fibonacci, shifted lower bounds)"]
fn q19_33_sum_closed_forms() {
    check(&[
        ("sum((-1)^k/k^2, [k, 1, inf])", "(-1/12)*pi^2"),
        ("sum(fibonacci(k), [k, 1, n])", "-1 + fibonacci(2 + n)"),
        ("sum(binomial(n, k)^2, [k, 1, n])", "-1 + binomial(2*n, n)"),
    ]);
}

#[test]
fn q19_34_definite_integral_gaps() {
    check(&[
        ("integrate(floor(x), [x, 0, 4])", "6"),
        ("integrate(x/sinh(x), [x, 0, inf])", "(1/4)*pi^2"),
        ("integrate(log(1 + x)/x, [x, 0, 1])", "(1/12)*pi^2"),
    ]);
}

#[test]
#[ignore = "Q19-36: the engine's printed form writes a negative term as `+ -10*x^2` (the kernel prints `- 10*x^2`)"]
fn q19_36_engine_printed_negative_terms() {
    check(&[("minimal_polynomial(sqrt(2) + sqrt(3), x)", "1 - 10*x^2 + x^4")]);
}

#[test]
fn q19_35_minimal_polynomial_trig() {
    check(&[("minimal_polynomial(cos(2*pi/7), x) == 8*x^3 + 4*x^2 - 4*x - 1", "true")]);
}
