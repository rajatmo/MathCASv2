//! Wrong values found in round 27 and fixed one by one. Expected values come from
//! identities or hand derivations.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate on a detached thread (one worker thread, fresh session); `TIMEOUT` after 30 s.
fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::spawn(move || {
        let stop = StopFlag::new();
        let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
        let out = match e.eval(&s, &stop) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        let _ = tx.send(out);
    });
    rx.recv_timeout(std::time::Duration::from_secs(30)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

#[test]
fn arg_abs_sign_of_complex_arguments_are_not_constant() {
    // d(arg(u)) = 0 and d(abs(u)) = sign(u) u' hold on the real line only
    assert_eq!(ev("d(arg(1 + im*x), x)"), "1im*arg'(1 + 1im*x)");
    assert_eq!(ev("d(abs(1 + im*x), x)"), "1im*abs'(1 + 1im*x)");
    assert_eq!(ev("d(abs(x), x)"), "sign(x)");
    assert_eq!(ev("d(arg(x), x)"), "0");
}

#[test]
fn complex_expand_of_arg_is_the_two_argument_arctangent() {
    assert_eq!(ev("complex_expand(arg(a + im*b))"), "atan(a, b)");
    assert_eq!(ev("d(complex_expand(arg(1 + im*x), target_functions: [re, im_part]), x)"), "(1 + x^2)^-1");
    assert_eq!(ev("d(atan(x^2, y), x)"), "-2*x*y/(x^4 + y^2)");
}

#[test]
fn part_assignment_rebinds_lists_and_associations() {
    assert_eq!(ev("u = assoc(a -> x, b -> y, c -> z); u[b] = w; u"), "assoc(a -> x, b -> w, c -> z)");
    assert_eq!(ev("u = assoc(a -> x); u[k] = 5; u"), "assoc(a -> x, k -> 5)");
    assert_eq!(ev("v = [[1, 2], [3, 4]]; v[2, 1] = 9; v"), "[[1, 2], [9, 4]]");
    assert_eq!(ev("{ var v = [1, 2]; v[1] = 5; v }"), "[5, 2]");
    assert_eq!(ev("g[2] = 7; g[2]"), "7");
}

#[test]
fn replace_part_takes_association_keys_and_value_positions() {
    assert_eq!(ev("replace_part(assoc(a -> x, b -> y), b -> w)"), "assoc(a -> x, b -> w)");
    assert_eq!(ev("replace_part(assoc(a -> x, b -> y), 1 -> w)"), "assoc(a -> w, b -> y)");
    assert_eq!(ev("replace_part(assoc(a -> [1, 2]), [a, 2] -> 9)"), "assoc(a -> [1, 9])");
}

#[test]
fn puiseux_series_at_a_branch_point_keeps_its_order() {
    // asin(-1 + u) = -pi/2 + sqrt(2u) (1 + u/12 + 3u^2/160 + 5u^3/896 + 35u^4/18432 + ...)
    let s = ev("series(asin(x), x, -1, 5)");
    assert!(s.contains("(35/18432)*2^(1/2)*(1 + x)^(9/2)") && s.ends_with("O((1 + x)^(11/2))"), "{s}");
}

#[test]
fn bin_counts_of_quantities() {
    let d = "data = quantity([-1.5, -0.5, -1, 0.5, 1, 1.5], \"Grams\"); ";
    assert_eq!(ev(&format!("{d}bin_counts(data, [[quantity(-2, \"Grams\"), quantity(0, \"Grams\"), quantity(2, \"Grams\")]])")), "[3, 3]");
    assert_eq!(ev(&format!("{d}bin_counts(data, quantity(0.5, \"Grams\"))")), "[1, 1, 1, 0, 1, 1, 1]");
    assert_eq!(ev(&format!("{d}bin_counts(data, [[quantity(-2000, \"Milligrams\"), quantity(0, \"Grams\"), quantity(2, \"Grams\")]])")), "[3, 3]");
}

#[test]
fn irreducibility_over_number_fields() {
    assert_eq!(ev("is_irreducible_polynomial(x^2 + 1, gaussian_integers: true)"), "false");
    assert_eq!(ev("is_irreducible_polynomial(x^2 + 2, extension: [im, sqrt(2)])"), "false");
    assert_eq!(ev("is_irreducible_polynomial(x^2 - 3, extension: sqrt(2))"), "true");
    assert_eq!(ev("is_irreducible_polynomial(x^2 + 1)"), "true");
}

#[test]
fn inverse_functions_are_injective() {
    assert_eq!(ev("function_injective(inverse_jacobi_cd(x, 1/3), x)"), "true");
    assert_eq!(ev("function_injective(inverse_elliptic_nome_q(x), x)"), "true");
    assert_eq!(ev("function_injective(inverse_jacobi_sn(x^2, 1/3), x)"), "false");
}

#[test]
fn coefficient_with_symbolic_exponents() {
    assert_eq!(ev("coefficient(x^s * x^t, x^s)"), "x^t");
    assert_eq!(ev("coefficient(x^(n + 2) + 3*x^n, x^n)"), "3 + x^2");
    assert_eq!(ev("coefficient(x^t, x^s)"), "0");
}

#[test]
fn default_tetrahedron_is_regular_with_edge_one() {
    assert_eq!(ev("volume(tetrahedron())"), "(1/12)*2^(1/2)");
    assert_eq!(ev("region_centroid(tetrahedron())"), "[0, 0, 0]");
    assert_eq!(ev("integrate(x^4 * y^5 + z^10, element([x, y, z], tetrahedron()))"), "(1385/1518206976)*2^(1/2)");
}

#[test]
fn dedekind_eta_reduces_the_real_part_mod_24() {
    // eta(tau + 24) = eta(tau); eta(16 + i) = e^(4 pi i/3) eta(i)
    let v = ev("n(dedekind_eta(10^22 + im), 20)");
    assert!(v.contains("-0.38411271116302832950") && v.contains("-0.66530273156739412621"), "{v}");
    let w = ev("n(dedekind_eta(16 + im), 20)");
    assert!(w.contains("-0.38411271116302832950"), "{w}");
}

#[test]
fn free_of_tests_the_association_itself_then_its_values() {
    let a = "a = assoc(x -> 1); ";
    assert_eq!(
        ev(&format!("{a}[free_of(a, x), free_of(a, 1), free_of(a, a), free_of(a, key_value_pattern([x -> _])), free_of(a, 2)]")),
        "[true, false, false, false, true]"
    );
}

#[test]
fn gaussian_composites() {
    assert_eq!(ev("[is_composite(3 + im), is_composite(3 + 2*im), is_composite(im), is_composite(2 + 2*im), is_composite(6)]"), "[true, false, false, true, true]");
}

#[test]
fn is_ordered_of_an_association_looks_at_its_values() {
    assert_eq!(ev("[is_ordered(assoc(1 -> 10, 2 -> 5, 3 -> 0)), is_ordered(assoc(a -> 1, b -> 2))]"), "[false, true]");
}

#[test]
fn find_instance_with_an_element_integers_conjunct() {
    assert_eq!(ev("find_instance(x^2 - 2*y^2 == 1 and x > 10 and element(x | y, integers), [x, y])"), "[[x -> 17, y -> 12]]");
}

#[test]
fn to_number_field_of_rationals_and_combinations() {
    // round 36: the result is an algebraic_number term (a rational stays a rational)
    assert_eq!(ev("to_number_field(3, sqrt(2))"), "3");
    assert_eq!(ev("to_number_field(5 - 3*sqrt(2), sqrt(2))"), "algebraic_number(2^(1/2), [5, -3])");
    assert_eq!(ev("last(to_number_field(sqrt(2), 2^(1/4)))"), "[0, 0, 1, 0]");
}

#[test]
fn numerator_and_denominator_split_exponentials_by_sign() {
    let e = "expr = (a * x^n * exp(a - b - 2 * c + 3 * d)) / y^m; ";
    assert_eq!(ev(&format!("{e}numerator(expr)")), "a*exp(a + 3*d)*x^n");
    assert_eq!(ev(&format!("{e}denominator(expr)")), "exp(b + 2*c)*y^m");
    assert_eq!(ev("denominator(exp(-x))"), "exp(x)");
}

#[test]
fn primitive_polynomials_are_monic() {
    assert_eq!(ev("[is_primitive_polynomial(x^2/2 + 2*x + 5/2, 7), is_primitive_polynomial(x^2 + 4*x + 5, 7), is_primitive_polynomial(4*x^2 + 2*x + 6, 7)]"), "[false, true, false]");
}

#[test]
fn parametric_region_with_a_bare_parameter_among_three() {
    assert_eq!(ev("volume(parametric_region([r * ((1 - t^2) / (1 + t^2)), r * ((2 * t) / (1 + t^2)), z], [t, [r, 0, 1], [z, 0, 2]]))"), "2*pi");
}

#[test]
fn shape_tests_with_a_predicate_accept_list_elements() {
    assert_eq!(ev("[is_vector([[1], [2, 3]], is_list), is_vector([[1], [2]]), is_matrix(array(a, [2, 2, 2]), is_list), is_matrix(array(a, [2, 2, 2]))]"), "[true, false, true, false]");
}

#[test]
fn gaussian_prime_powers() {
    assert_eq!(
        ev("[is_prime_power(2 + im), is_prime_power((1 + im)^3), is_prime_power(3 + im), is_prime_power((2 + im)^2), is_prime_power(5*im), is_prime_power(9*im), is_prime_power(im)]"),
        "[true, true, false, true, false, true, false]"
    );
}

#[test]
fn parallelepiped_in_four_dimensions() {
    let p = "p = parallelepiped([0, 0, 0, 0], [[1, 0, 0, 0], [1, 1, 0, 0], [0, 1, 1, 0], [0, 0, 1, 1]]); ";
    assert_eq!(ev(&format!("{p}[is_bounded_region(p), region_dimension(p), region_measure(p)]")), "[true, 4, 1]");
}

#[test]
fn spherical_bessel_on_the_negative_axis_is_continuous_from_above() {
    // sqrt(pi/2) z^(-1/2) J_(nu + 1/2)(z): j_(1/2)(-0.37228) = +0.37577 i
    let v = ev("spherical_bessel_j(0.5, -0.37228)");
    assert!(v.contains("0.37576749212658"), "{v}");
    assert!(!v.contains("-0.37576749212658"), "{v}");
}

#[test]
fn gaussian_square_free() {
    assert_eq!(
        ev("[is_square_free(3 + 2*im), is_square_free(2, gaussian_integers: true), is_square_free(5, gaussian_integers: true), is_square_free(9, gaussian_integers: true), is_square_free((2 + im)^2), is_square_free(2 + 2*im)]"),
        "[true, false, true, false, false, false]"
    );
}

#[test]
fn real_digits_from_a_given_exponent() {
    assert_eq!(
        ev("[real_digits(sqrt(20), 10, 1, -5), real_digits(pi, 10, 3, -2), real_digits(123.456, 10, 4, 1), real_digits(pi, 10, 3, 2), real_digits(1/7, 10, 4, -3)]"),
        "[[[3], -4], [[4, 1, 5], -1], [[2, 3, 4, 5], 2], [[0, 0, 3], 3], [[2, 8, 5, 7], -2]]"
    );
}

#[test]
fn real_digits_beyond_precision_and_deep_positions() {
    let v = ev("real_digits(5.635, 10, 3, -15)");
    assert_eq!(v, "[[0, indeterminate, indeterminate], -14]");
    assert_eq!(ev("real_digits(pi, 10, 1, -10000)"), "[[8], -9999]");
}

#[test]
fn from_digits_of_strings_and_roman_numerals() {
    assert_eq!(ev("[from_digits(\"892\"), from_digits(\"ff\", 16), from_digits(\"DCCCXCII\", \"Roman\"), from_digits(\"MCMXCIV\", \"Roman\")]"), "[892, 255, 892, 1994]");
}

#[test]
fn from_digits_of_digit_lists_with_exponent_and_repeating_blocks() {
    assert_eq!(ev("[from_digits([[1, [5, 7, 1, 4, 2, 8]], 1]), from_digits([[[1, 2, 3]], 0]), from_digits([[1, 2, 3], 2]), from_digits([[1, 0, 1], 1], 2), from_digits(real_digits(1/7))]"), "[11/7, 41/333, 123/10, 5/4, 1/7]");
}

#[test]
fn subfactorial_at_non_integers() {
    let v = ev("subfactorial(4.5)");
    assert!(v.contains("19.2558318407425") && v.contains("0.157179122964615"), "{v}");
    assert_eq!(ev("subfactorial(5.0)"), "44.0");
}

#[test]
fn hyperfactorial_at_non_integers() {
    let v = ev("hyperfactorial(3.4)");
    assert!(v.starts_with("745.97695773194"), "{v}");
    assert!(ev("n(hyperfactorial(1/2), 20)").starts_with("0.880449235173423393279"));
}

#[test]
fn generalized_harmonic_numbers() {
    let v = ev("harmonic_number(0.8, 3)");
    assert!(v.starts_with("0.940124048948776"), "{v}");
    assert_eq!(ev("harmonic_number(n_u, -1)"), "(1/2)*n_u*(1 + n_u)");
}

#[test]
fn refine_splits_on_disjunctive_assumptions() {
    assert_eq!(ev("refine(sqrt((x^2 - 1)^2), x <= -1 or x >= 1)"), "-1 + x^2");
    assert_eq!(ev("refine(abs(x^2 - 1), x < -1 or x > 1)"), "-1 + x^2");
    // alternatives that disagree leave the expression
    assert_eq!(ev("refine(abs(x), x < -1 or x > 1)"), "abs(x)");
}

#[test]
fn simplify_of_propositional_and_boole_terms() {
    assert_eq!(ev("simplify(implies(p, implies(q, p)))"), "true");
    assert_eq!(ev("simplify(xor(true, x))"), "not x");
    assert_eq!(ev("simplify(xor(x, x, y))"), "y");
    assert_eq!(ev("simplify(p and (p or q))"), "p");
    assert_eq!(ev("simplify(boole(a and b) - min(boole(a), boole(b)))"), "0");
    assert_eq!(ev("simplify(boole(a or b) - max(boole(a), boole(b)))"), "0");
    assert_eq!(ev("simplify(boole(a and b) - boole(a) * boole(b))"), "0");
    assert_eq!(ev("simplify(boole(a or b) - (boole(a) + boole(b) - boole(a) * boole(b)))"), "0");
    assert_eq!(ev("simplify(boole(not a) - (1 - boole(a)))"), "0");
    // an expression that is already minimal stays
    assert_eq!(ev("simplify(boole(a and b))"), "boole(a and b)");
}

#[test]
fn trig_reduce_hyperbolic_products_and_trig_expand_pythagoras() {
    assert_eq!(ev("trig_reduce(2 * sinh(x) * cosh(y))"), "sinh(x + y) + sinh(x - y)");
    assert_eq!(ev("trig_reduce(cosh(x)^2)"), "1/2 + (1/2)*cosh(2*x)");
    assert_eq!(ev("trig_expand(sin(x)^2 + cos(x)^2)"), "1");
    assert_eq!(ev("trig_expand(3 * cos(x)^2 + 3 * sin(x)^2 + y)"), "3 + y");
    assert_eq!(ev("trig_expand(cosh(x)^2 - sinh(x)^2)"), "1");
}

#[test]
fn exp_to_trig_keeps_real_exponentials() {
    // the real-exponent rule (exp(x) = cosh(x) + sinh(x)) is off: transforms, d_solve and simplify rely on
    // exp(-t) staying (round 28)
    assert_eq!(ev("exp_to_trig(exp(x))"), "exp(x)");
    assert_eq!(ev("simplify(1 - exp(-t))"), "1 - exp(-t)");
    assert_eq!(ev("exp_to_trig(exp(im*x))"), "cos(x) + 1im*sin(x)");
}

#[test]
fn bounded_region_of_unevaluated_set_operations() {
    assert_eq!(ev("is_bounded_region(region_difference(mesh_region([[0.], [2.]], [line([[1, 2]])]), mesh_region([[0.5], [1.]], [line([[1, 2]])])))"), "true");
    assert_eq!(ev("is_bounded_region(region_union(mesh_region([[0.], [2.]], [line([[1, 2]])]), mesh_region([[0.5], [3.]], [line([[1, 2]])])))"), "true");
    assert_eq!(ev("is_bounded_region(region_difference(half_space([1, 0], [0, 0]), mesh_region([[0.], [2.]], [line([[1, 2]])])))"), "false");
}

#[test]
fn distance_to_an_annulus() {
    assert_eq!(ev("[region_distance(annulus(), [1, 2]), region_distance(annulus(), [0, 0])]"), "[-1 + 5^(1/2), 1/2]");
    assert_eq!(ev("signed_region_distance(annulus(), [3/4, 0])"), "-1/4");
    assert_eq!(ev("region_distance(annulus(), [3/4, 0])"), "0");
    assert_eq!(ev("region_nearest(annulus(), [0, 3])"), "[0, 1]");
}

#[test]
fn normal_of_transformed_primitives() {
    assert_eq!(ev("normal(translate(point([x, y, z]), [x0, y0, z0]))"), "point([x + x0, y + y0, z + z0])");
    assert_eq!(ev("normal(scale(cuboid([x, y, z]), [1, 1, -1], [0, 0, 0]))"), "cuboid([x, y, -z], [1 + x, 1 + y, -1 - z])");
    assert_eq!(ev("normal(rotate(cylinder(), pi / 4, [1, 0, 0]))"), "cylinder([[0, (1/2)*2^(1/2), (-1/2)*2^(1/2)], [0, (-1/2)*2^(1/2), (1/2)*2^(1/2)]], 1)");
    assert_eq!(ev("normal(translate(line([[0, 0], [1, 1]]), [1, 2]))"), "line([[1, 2], [2, 3]])");
    // a cuboid does not survive a rotation: left as it was
    assert_eq!(ev("normal(rotate(cuboid([0, 0, 0]), pi / 4, [1, 0, 0]))"), "rotate(cuboid([0, 0, 0]), (1/4)*pi, [1, 0, 0])");
}

#[test]
fn dot_of_transformation_functions_composes_them() {
    assert_eq!(
        ev("transformation_matrix(dot(translation_transform([1, 2]), scaling_transform([2, 3])))"),
        "[[2, 0, 1], [0, 3, 2], [0, 0, 1]]"
    );
    assert_eq!(ev("dot([[1, 2], [3, 4]], [[1, 0], [0, 1]])"), "[[1, 2], [3, 4]]");
}

#[test]
fn library_series_is_not_shadowed_by_a_global_t() {
    assert_eq!(ev("t = 5; series((f(x + h) - 2*f(x) + f(x - h))/h^2, h, 0, 3)"), "f''(x) + (1/12)*f''''(x)*h^2 + O(h^4)");
}

#[test]
fn numeric_parameter_type_waits_for_numbers() {
    assert_eq!(ev("fn pq(w: numeric) = w^2; [pq(3), pq(pi), pq(x), pq(2.5)]"), "[9, pi^2, pq(x), 6.25]");
    assert_eq!(ev("fn pq(w: numeric) = w^2; pq(\"a\")"), "failure(no_match, pq, [\"a\"])");
    assert_eq!(ev("rule rq(w_: numeric) := 1; [rq(e), rq(y)]"), "[1, rq(y)]");
}

#[test]
fn f32_expand_of_a_large_trinomial_power_ends_cleanly() {
    // (x + y + z)^1000 has 501501 terms; the result is either exact or a clean failure, never an abort
    let r = ev("length(expand((x + y + z)^1000))");
    assert!(r == "501501" || r.starts_with("failure(limit"), "{r}");
}

#[test]
fn f32_trinomial_power_by_the_recurrence_is_exact() {
    assert_eq!(ev("length(expand((x + y + z)^100))"), "5151");
    // the sum of the coefficients is 3^100, and a coefficient is a multinomial
    assert_eq!(ev("rewrite(expand((x + y + z)^100), [x -> 1, y -> 1, z -> 1]) == 3^100"), "true");
    assert_eq!(ev("coefficient(coefficient(coefficient(expand((x + y + z)^100), x, 50), y, 30), z, 20) == multinomial(50, 30, 20)"), "true");
}

#[test]
fn f32_nested_radicals_with_symbolic_parts_denest_under_facts() {
    // x + 2 sqrt(x - 1) = (1 + sqrt(x - 1))^2 for x > 1; checked at x = 3 below (1 + sqrt 2 = 2.4142...)
    assert_eq!(ev("simplify(sqrt(x + 2*sqrt(x - 1)), assumptions: x > 1)"), "1 + (-1 + x)^(1/2)");
    let z = ev("n(simplify(sqrt(x + 2*sqrt(x - 1)), assumptions: x > 1) - sqrt(x + 2*sqrt(x - 1)), 30) /. x -> 3");
    assert!(z == "0" || z.starts_with("ball(0,"), "{z}");
    assert_eq!(
        ev("simplify(sqrt(2*x + 2*sqrt(x^2 - 1)), assumptions: x > 1)"),
        "(-1 + x)^(1/2) + (1 + x)^(1/2)"
    );
    // without the facts nothing is done
    assert_eq!(ev("simplify(sqrt(x + 2*sqrt(x - 1)))"), "(x + 2*(-1 + x)^(1/2))^(1/2)");
    // sqrt of squares under a real fact
    assert_eq!(ev("simplify(sqrt(x^2 + 2*x + 1), assumptions: element(x, reals))"), "abs(1 + x)");
}

#[test]
fn f32_implicit_d_with_a_variable_list_gives_the_gradient() {
    // x^2 + 2 y^2 + 3 z^2 = 1: z_x = -x/(3 z), z_y = -2 y/(3 z)
    assert_eq!(
        ev("surface = x^2 + 2*y^2 + 3*z^2 == 1; implicit_d(surface, z, [[x, y]])"),
        "[(-1/3)*x/z, (-2/3)*y/z]"
    );
}

#[test]
fn f32_factor_sees_x_and_its_roots_as_powers_of_one_root() {
    assert_eq!(ev("factor(x + 2*sqrt(x) + 1)"), "(1 + x^(1/2))^2");
    assert_eq!(ev("factor_square_free_list(x + 2*sqrt(x) + 1)"), "[[1, 1], [1 + x^(1/2), 2]]");
    assert_eq!(ev("factor(x^(3/2) - x^(1/2))"), "(-1 + x^(1/2))*(1 + x^(1/2))*x^(1/2)");
    // the product is the input
    assert_eq!(ev("expand(factor(x - y + sqrt(x) - sqrt(y))) == x - y + sqrt(x) - sqrt(y)"), "true");
    assert_eq!(ev("factor(x - 1)"), "-1 + x");
}

#[test]
fn f32_exp_to_trig_writes_logarithm_pairs_as_inverse_functions() {
    assert_eq!(ev("exp_to_trig(log(1 + im*x) - log(1 - im*x))"), "2im*atan(x)");
    assert_eq!(ev("exp_to_trig(log(2 + x) - log(2 - x))"), "2*atanh((1/2)*x)");
    assert_eq!(ev("exp_to_trig(log(z + sqrt(z^2 + 1)))"), "asinh(z)");
    // no positive constant in the middle: nothing to do (log(x + 1) - log(x - 1) is not an atanh of a quotient with
    // a positive midpoint)
    assert_eq!(ev("exp_to_trig(log(x + 1) - log(x - 1))"), "log(1 + x) - log(-1 + x)");
    // the value agrees numerically: 2 i atan(0.3) = log(1 + 0.3 i) - log(1 - 0.3 i)
    assert_eq!(ev("abs(n(2*im*atan(3/10)) - n(log(1 + im*3/10) - log(1 - im*3/10))) < 10^-12"), "true");
}

#[test]
fn f32_power_expand_cancels_an_inverse_function_applied_to_its_function() {
    assert_eq!(ev("power_expand(atan(tan(x)))"), "x");
    assert_eq!(ev("power_expand(asin(sin(x)) + acosh(cosh(y)))"), "x + y");
}

#[test]
fn f32_piecewise_expand_keeps_the_jump_at_a_closed_end_of_the_region() {
    // floor(1) = 1 at the right end of [-1, 1]
    assert_eq!(ev("piecewise_expand(floor(x), -1 <= x and x <= 1)"), "piecewise([[-1, x < 0], [1, x >= 1]], 0)");
    assert_eq!(ev("piecewise_expand(floor(x), -1 < x and x < 1)"), "piecewise([[-1, x < 0]], 0)");
    assert_eq!(ev("piecewise_expand(ceiling(2*x), 0 <= x and x <= 1)"), "piecewise([[1, 0 < x and x <= 1/2], [2, x > 1/2]], 0)");
}

#[test]
fn f32_piecewise_expand_of_the_wave_functions() {
    assert_eq!(ev("piecewise_expand(sawtooth_wave(x), -1 <= x and x <= 1)"), "piecewise([[x, 0 <= x and x < 1], [1 + x, x < 0]], -1 + x)");
    assert_eq!(ev("piecewise_expand(square_wave(x), 0 < x and x < 1)"), "piecewise([[1, x < 1/2]], -1)");
    assert_eq!(
        ev("piecewise_expand(triangle_wave(x), 0 < x and x < 1)"),
        "piecewise([[2 + -4*x, 1/4 <= x and x < 3/4], [4*x, x < 1/4]], -4 + 4*x)"
    );
}

#[test]
fn f32_generating_functions_of_the_delta_and_the_step() {
    assert_eq!(ev("generating_function(discrete_delta(n_u - 5), n_u, z)"), "z^5");
    assert_eq!(ev("generating_function(unit_step(n_u - 3), n_u, z)"), "z^3/(1 - z)");
    assert_eq!(ev("generating_function(discrete_delta(n_u + 2), n_u, z)"), "0");
}

#[test]
fn f32_refine_heaviside_theta_of_several_arguments() {
    assert_eq!(ev("assuming(a > 0, refine(heaviside_theta(a, a, a, a)))"), "1");
    assert_eq!(ev("assuming(a < 0 and b > 0, refine(heaviside_theta(b, b, a)))"), "0");
    assert_eq!(ev("assuming(a > 0, refine(heaviside_theta(a, x)))"), "heaviside_theta(x)");
}

#[test]
fn f32_function_expand_of_the_heaviside_shapes() {
    assert_eq!(ev("function_expand(heaviside_pi(x))"), "heaviside_theta(1/2 + x) - heaviside_theta(-1/2 + x)");
    // the triangle is the second difference of x theta(x): at x = 1/2 it is 1/2
    assert_eq!(ev("rewrite(function_expand(heaviside_lambda(x)), x -> 1/2)"), "1/2");
}

#[test]
fn f32_polynomial_extended_gcd_with_symbolic_coefficients_ends() {
    assert_eq!(ev("polynomial_extended_gcd(a*x^2 + b*x + c, x - r, x)"), "[1, [(c + a*r^2 + b*r)^-1, (-b + -1*a*r + -1*a*x)/(c + a*r^2 + b*r)]]");
    // the gcd is x - a, and g = s p + t q (checked with numbers for the symbols)
    assert_eq!(
        ev("{ let r = polynomial_extended_gcd((x - a)*(b*x - c)^2, (x - a)*(x^2 - b*c), x); [r[1], simplify(r[2][1]*(x - a)*(b*x - c)^2 + r[2][2]*(x - a)*(x^2 - b*c) - r[1])] }"),
        "[x - a, 0]"
    );
    assert_eq!(ev("polynomial_extended_gcd(a*(x + b)^2, (x + a)*(x + b), x)[1]"), "b + x");
}

#[test]
fn r32_real_digits_of_long_expansions_do_not_exhaust_memory() {
    // a million binary digits of pi (499722 ones) and 100000 base 47 digits, by one big division
    assert_eq!(ev("count(first(real_digits(pi, 2, 10^6)), 1)"), "499722");
    assert_eq!(ev("length(first(real_digits(pi, 47, 100000)))"), "100000");
    // base 47 digits of pi checked against floor(pi 47^8)
    assert_eq!(ev("real_digits(pi, 47, 9)"), "[[3, 6, 30, 36, 26, 46, 6, 2, 13], 1]");
    assert_eq!(ev("integer_digits(floor(n(pi, 30)*47^8), 47)"), "[3, 6, 30, 36, 26, 46, 6, 2, 13]");
    assert_eq!(ev("[real_digits(1/7, 10, 8), real_digits(1/1024, 2, 5), real_digits(7/2, 3, 4)]"), "[[[1, 4, 2, 8, 5, 7, 1, 4], 0], [[1, 0, 0, 0, 0], -9], [[1, 0, 1, 1], 2]]");
}

#[test]
fn r32_bin_counts_with_two_bounds_and_split_of_digit_runs() {
    assert_eq!(ev("bin_counts([0, 1, 1, 2, 3, 3], [0, 4])"), "[1, 2, 1, 2]");
    assert_eq!(ev("length(tally(map(length, split(first(real_digits(pi, 2, 10^6))))))"), "19");
}

#[test]
fn r32_permanent_of_symbolic_matrices_follows_the_number_of_terms() {
    assert_eq!(ev("permanent([[a, b], [c, d]])"), "a*d + b*c");
    assert_eq!(ev("permanent([[a, b, c], [d, e, f]])"), "a*e + a*f + b*d + b*f + c*d + c*e");
    assert_eq!(ev("permanent([[1, 2, 3], [4, 5, 6], [7, 8, 10]])"), "463");
    // the number of ways to place n non-attacking queens
    assert_eq!(
        ev("table(rewrite(expand(permanent(array(&(FormalX(&1 - &2, 1) * FormalX(&1 + &2, 2)), [n_u, n_u]))), [FormalX(__)^_ :> 0, FormalX(__) :> 1]), n_u, range(1, 7))"),
        "[1, 0, 0, 2, 10, 4, 40]"
    );
}

#[test]
fn r32_fixed_point_list_without_count_stops_cleanly() {
    assert_eq!(ev("fix_point(pf, x0)"), "failure(limit, memory, fixed_point_list(pf, x0))");
}

#[test]
fn r32_matrix_exp_action_on_a_nilpotent_matrix_and_negative_sparse_positions() {
    // exp(m) v = v + m v + m^2 v / 2 for the strictly triangular m, exact
    assert_eq!(ev("matrix_exp([[0, 2, 1], [0, 0, 3], [0, 0, 0]], [1, 1, 1])"), "[7, 4, 1]");
    assert_eq!(ev("matrix_exp([[0, 2, 1], [0, 0, 3], [0, 0, 0]], [1, 1, 1]) == dot(matrix_exp([[0, 2, 1], [0, 0, 3], [0, 0, 0]]), [1, 1, 1])"), "true");
    // a 1000 x 1000 band: exp(m) v at the end is -3/2 times the series 2^j/j!
    assert_eq!(
        ev("m = sparse_array(band([1, 2]) -> 2, [1000, 1000]); v = sparse_array([[1] -> 1, [-1] -> -1.5], [1000]); r = matrix_exp(m, v); [r[1000], r[999], r[998], r[997]]"),
        "[-1.5, -3.0, -3.0, -2.0]"
    );
    assert_eq!(ev("sparse_array([[1] -> a, [-1] -> b], [4])"), "[a, 0, 0, b]");
}

#[test]
fn r32_eigensystem_of_exact_matrices_with_radical_entries_stays_exact() {
    assert_eq!(
        ev("eigensystem([[sqrt(2), 1], [1, 0]])"),
        "[[(1/2)*(2^(1/2) + 6^(1/2)), (1/2)*(-6^(1/2) + 2^(1/2))], [[(1/2)*2^(1/2) + (1/2)*6^(1/2), 1], [(-1/2)*6^(1/2) + (1/2)*2^(1/2), 1]]]"
    );
    // A v = lambda v (machine residuals) for each eigenpair
    for m in ["[[sqrt(2), 1], [1, 0]]", "[[sqrt(2), sqrt(3)], [sqrt(3), 0]]", "[[1, 2], [3, sqrt(2)]]", "[[im, 1], [1, 0]]", "[[1, im], [-im, 1]]"] {
        let src = format!("m = {m}; s = eigensystem(m); all_true(map(&(all_true(flatten(n(dot(m, s[2][&1]) - s[1][&1]*s[2][&1])), x => abs(x) < 10^(-12))), range(2)), x => x)");
        assert_eq!(ev(&src), "true", "{m}");
    }
}

#[test]
fn r32_array_pad_continues_by_differences() {
    assert_eq!(ev("array_pad([a, b, c], 3, \"ReflectedDifferences\")"), "[b + -2*c + 2*a, -c + 2*a, -b + 2*a, a, b, c, -b + 2*c, -a + 2*c, b + -2*a + 2*c]");
    assert_eq!(ev("array_pad([a, b, c], 4, \"ReversedDifferences\")"), "[-c + 2*a, -c + 2*a, -b + 2*a, a, a, b, c, c, -b + 2*c, -a + 2*c, -a + 2*c]");
    assert_eq!(ev("array_pad([a, b, c], 3, \"Extrapolated\")"), "[-3*b + 4*a, -2*b + 3*a, -b + 2*a, a, b, c, -b + 2*c, -2*b + 3*c, -3*b + 4*c]");
    assert_eq!(ev("[array_pad([1, 5, 7, 8], [0, 3], \"Extrapolated\"), center_array([1, 2, 3], 6, \"Extrapolated\")]"), "[[1, 5, 7, 8, 9, 10, 11], [0, 1, 2, 3, 4, 5]]");
    assert_eq!(ev("array_pad([[1, 2, 4]], [[0, 0], [1, 2]], \"ReflectedDifferences\")"), "[[0, 1, 2, 4, 6, 7]]");
}

#[test]
fn r32_sparse_band_values_repeat_and_negative_ends_count_from_the_back() {
    assert_eq!(ev("sparse_array(band([1, 1], [5, 5]) -> [x, y, z], [5, 5])"), "[[x, 0, 0, 0, 0], [0, y, 0, 0, 0], [0, 0, z, 0, 0], [0, 0, 0, x, 0], [0, 0, 0, 0, y]]");
    assert_eq!(ev("sparse_array(band([1, 1], [5, 5]) -> [[[a, b], [c, d]]], [5, 5])"), "[[a, b, 0, 0, 0], [c, d, 0, 0, 0], [0, 0, a, b, 0], [0, 0, c, d, 0], [0, 0, 0, 0, a]]");
    assert_eq!(ev("eigenvalues(sparse_array(band([1, 1], [-1, -1]) -> [[[1, 2], [2, 1]]], [4, 4]))"), "[3, 3, -1, -1]");
}

#[test]
fn r32_trig_of_inverse_trig_is_algebraic() {
    assert_eq!(ev("[cos(atan(4/3)), cos(atan(2)), sin(atan(x)), cos(asin(x)), sin(acos(1/3)), tan(asin(x)), tan(acos(3/5))]"), "[3/5, (1/5)*5^(1/2), x*(1 + x^2)^(-1/2), (1 - x^2)^(1/2), (2/3)*2^(1/2), x*(1 - x^2)^(-1/2), 4/3]");
    assert_eq!(ev("re_im(3 + 4*im) === from_polar_coordinates(abs_arg(3 + 4*im))"), "true");
    // the poles stay unevaluated, inexact arguments are evaluated numerically
    assert_eq!(ev("[cos(atan(im)), cos(atan(0.5))]"), "[cos(atan(1im)), 0.8944271909999159]");
}

#[test]
fn r32_numerical_order_puts_non_numbers_after_numbers() {
    assert_eq!(ev("[numerical_order(sqrt(2) + 5, e2), numerical_order(e2, sqrt(2) + 5), numerical_order(1/2, e2), numerical_order(6, pi)]"), "[1, -1, 1, -1]");
}

#[test]
fn r32_orthogonalize_of_ill_conditioned_numbers_stays_orthogonal() {
    assert_eq!(ev("m = n(hilbert_matrix(10)); gs = orthogonalize(m); max(abs(dot(gs, transpose(gs)) - identity_matrix(10))) < 1e-14"), "true");
    assert_eq!(ev("orthogonalize([[1, 2], [3, 4]])"), "[[(1/5)*5^(1/2), (2/5)*5^(1/2)], [(2/5)*5^(1/2), (-1/5)*5^(1/2)]]");
}

#[test]
fn r32_total_of_machine_reals_is_compensated_on_request() {
    // the default is the canonical plain sum (the same packed, unpacked and compiled); compensation is an option
    assert_eq!(ev("x = constant_array(0.1, 10^6); total(x, method: \"CompensatedSummation\") - 10^5"), "0.0");
}

#[test]
fn r32_karhunen_loeve_standardized_takes_the_row_means_off() {
    // the second row of the transformed data is the same with and without the sign choice of the basis
    assert_eq!(ev("round(100 * abs(karhunen_loeve_decomposition([[1, 2, 3, 4], [0.8, 2.1, 3.2, 3.9]], standardized: true)[1][2][1]))"), "9");
}

#[test]
fn r32_replace_part_with_conditioned_position_patterns_and_association_keys() {
    assert_eq!(
        ev("replace_part(constant_array(0, [5, 5]), [x_, y_] when is_coprime(x, y) :> x + y)"),
        "[[2, 3, 4, 5, 6], [3, 0, 5, 0, 7], [4, 5, 0, 7, 8], [5, 0, 7, 0, 9], [6, 7, 8, 9, 0]]"
    );
    assert_eq!(ev("replace_part([assoc(\"x\" -> 1, \"y\" -> 2), assoc(\"x\" -> 3, \"y\" -> 4)], [_, \"x\"] -> f)"), "[assoc(\"x\" -> f, \"y\" -> 2), assoc(\"x\" -> f, \"y\" -> 4)]");
    assert_eq!(ev("replace_part(constant_array(0, [3, 3]), [i_, i_] -> 1)"), "[[1, 0, 0], [0, 1, 0], [0, 0, 1]]");
}

#[test]
fn r32_numeric_arrays_of_single_precision_round_to_24_bits() {
    assert_eq!(ev("normal(numeric_array([0.1, 1.1, 2.1], \"Real32\"))"), "[0.10000000149011612, 1.100000023841858, 2.0999999046325684]");
    assert_eq!(ev("normal(numeric_array([1 / 3 + (2 / 3) * im], \"ComplexReal32\", \"Coerce\"))"), "[complex(0.3333333432674408, 0.6666666865348816)]");
    assert_eq!(ev("normal(numeric_array([0.1, 1.1], \"Real64\"))"), "[0.1, 1.1]");
}

#[test]
fn r32_fixed_point_list_stops_at_a_two_cycle_of_neighbouring_doubles() {
    // Newton's step for sqrt(3) alternates between 1.7320508075688772 and 1.7320508075688774
    assert_eq!(ev("last(fixed_point_list(&((&1 + 3/&1)/2), 1.))"), "1.7320508075688772");
    assert_eq!(ev("length(fixed_point_list(&((&1 + 3/&1)/2), 1.))"), "7");
}

#[test]
fn r32_sparse_band_cycles_only_with_an_explicit_end() {
    assert_eq!(ev("sparse_array(band([1, 1]) -> [x, y], [3, 3])"), "[[x, 0, 0], [0, y, 0], [0, 0, 0]]");
    assert_eq!(ev("sparse_array(band([1, 1], [3, 3]) -> [x, y], [3, 3])"), "[[x, 0, 0], [0, y, 0], [0, 0, x]]");
}

#[test]
fn r32_fourier_of_long_vectors_is_fast_and_matches_the_definition() {
    // lengths 1000 (mixed radix) and 4099 (prime): a round trip, and one coefficient against the sum
    assert_eq!(ev("x = table(n(sin(i)) + 0.5*i, i, 1000); max(abs(inverse_fourier(fourier(x)) - x)) < 1e-9"), "true");
    assert_eq!(ev("x = table(n(sin(i)), i, 4099); max(abs(inverse_fourier(fourier(x)) - x)) < 1e-10"), "true");
    assert_eq!(
        ev("x = table(n(cos(i*i)), i, 700); f = fourier(x); abs(f[7] - total(table(x[r + 1]*n(exp(2*pi*im*r*6/700)), r, range(0, 699)))/sqrt(700.)) < 1e-10"),
        "true"
    );
    // chop of a million zeros: seconds, not minutes
    assert_eq!(ev("x = constant_array(0., 10^6); length(fourier(inverse_fourier(x)))"), "1000000");
}

#[test]
fn r32_unbounded_nest_while_list_stops_cleanly_and_quantities_have_a_numerical_order() {
    // a test that never fails (the exact comparison of a two-cycle of doubles): the list is cut, not grown to memory
    assert_eq!(ev("nest_while_list(&((&1 + 3/&1)/2), 1., not_same, 2)"), "failure(limit, memory, nest_while_list(&((&1 + 3/&1)/2), 1.0, not_same, 2))");
    assert_eq!(ev("[numerical_order(quantity(1, \"Meters\"), quantity(3, \"Feet\")), numerical_order(-inf, 3), numerical_order(inf, 3)]"), "[-1, 1, -1]");
}

#[test]
fn r32_find_root_with_an_exactly_zero_residual_ends() {
    let v = ev("r = find_root(sqrt(c - 1 - im) - sqrt(c - 1 - im), [c, 2 + im], working_precision: 30); 1");
    assert_eq!(v, "1");
}

#[test]
fn r32_differential_root_is_accurate_to_machine_digits() {
    assert_eq!(ev("f = differential_root((y, x) => [y'(x) - y(x) == 0, y(0) == 1]); abs(f(0.5) - exp(0.5)) < 1e-13"), "true");
}

#[test]
fn r33_bitwise_operations_on_integers_of_any_size() {
    assert_eq!(ev("[bit_xor(4*2^62, 7), bit_or(2^70, 1), bit_and(2^70 + 3, 2^70 + 1), bit_not(2^70)]"), "[18446744073709551623, 1180591620717411303425, 1180591620717411303425, -1180591620717411303425]");
    // against the two's-complement identities
    assert_eq!(ev("[bit_xor(-2^70, 5), bit_and(-2^70, 2^71 - 1), bit_or(-2^65, 2^64), bit_and(-3, -2^80)]"), "[-1180591620717411303419, 1180591620717411303424, -18446744073709551616, -1208925819614629174706176]");
    assert_eq!(ev("length(integer_digits(nest_list(&(bit_xor(4*&1, bit_or(2*&1, bit_not(&1)))), 1, 60), 2)[60])"), "119");
}

#[test]
fn r33_root_approximant_uses_the_precision_asked_for() {
    // n(pi, 4) carries 16 guard bits: 22/7 is what 4 digits support, not 355/113
    assert_eq!(ev("root_approximant(n(pi, 4))"), "22/7");
    assert_eq!(ev("root_approximant(n(pi, 1))"), "3");
    assert_eq!(ev("root_approximant(n(sqrt(2) + sqrt(3), 15))"), "2^(1/2) + 3^(1/2)");
}

#[test]
fn r33_find_minimum_with_constraints_from_the_default_start_tries_the_sign_patterns() {
    // sin(x) sin(2y) on a disk: from (1, 1) alone a shallow boundary minimum, the lowest one is near -1
    assert_eq!(ev("v = find_minimum([sin(x)*sin(2*y), x^2 + y^2 < 3], [x, y]); abs(v[1] + 0.99965627590) < 1e-8"), "true");
    assert_eq!(ev("v = find_maximum([sin(x)*sin(2*y), x^2 + y^2 < 3], [x, y]); abs(v[1] - 0.99965627590) < 1e-8"), "true");
}

#[test]
fn r33_find_peaks_interpolation_order_refines_with_the_clamped_spline() {
    // values and positions of the maxima of the cubic spline with zero end slopes (scipy CubicSpline 'clamped' gives
    // 5.5661 / 6.251185 and 10.884034 / 7.101697 for the two inner peaks)
    assert_eq!(
        ev("v = [2, 1, 3, 5, 6, 6, 4, 3, 2, 4, 7, 3]; p = find_peaks(v, 0, interpolation_order: 3); [abs(p[2][1] - 5.566075300) < 1e-8, abs(p[2][2] - 6.2511846549) < 1e-8, abs(p[3][1] - 10.88403371) < 1e-7, abs(p[3][2] - 7.1016969570) < 1e-8, length(p)]"),
        "[true, true, true, true, 3]"
    );
}

#[test]
fn r33_svd_takes_a_shape_thin_full_compact_a_count_or_up_to() {
    assert_eq!(
        ev("m = [[1.2, 3.4], [5.6, 7.8], [9., 1.2]]; f = svd(m, \"Full\"); [map(dimensions, f), max(abs(dot(f[1], f[2], transpose(f[3])) - m)) < 1e-13, max(abs(dot(transpose(f[1]), f[1]) - identity_matrix(3))) < 1e-13]"),
        "[[[3, 3], [3, 2], [2, 2]], true, true]"
    );
    assert_eq!(ev("m = [[1.2, 3.4], [5.6, 7.8], [9., 1.2]]; [map(dimensions, svd(m, \"Thin\")), map(dimensions, svd(m, -1)), map(dimensions, svd(m, up_to(5)))]"), "[[[3, 2], [2, 2], [2, 2]], [[3, 1], [1, 1], [2, 1]], [[3, 2], [2, 2], [2, 2]]]");
    assert_eq!(ev("map(dimensions, svd([[1., 2, 3], [2, 4, 6], [3, 6, 9]], \"Compact\"))"), "[[3, 1], [1, 1], [3, 1]]");
}

#[test]
fn r33_cellular_automaton_on_a_background_keeps_the_columns_that_differ() {
    // the widths of rules 1 to 20 from one cell, ten steps: a pattern drifting left, dying, flipping the background
    assert_eq!(ev("table(dimensions(cellular_automaton(r, [[1], 0], 10))[2], r, 20)"), "[3, 11, 6, 1, 3, 11, 2, 1, 12, 11, 11, 1, 12, 11, 11, 11, 6, 21, 1, 11]");
}

#[test]
fn r33_sequence_position_of_a_literal_run_in_a_long_list() {
    // a million digits of pi: the first two places of 314159 (the second at 176452, a well-known position)
    assert_eq!(ev("d = first(real_digits(n(pi, 10^6))); sequence_position(d, [3, 1, 4, 1, 5, 9])"), "[[1, 6], [176452, 176457]]");
    assert_eq!(ev("[sequence_position([1, 1, 1, 1], [1, 1]), sequence_count([1, 1, 1, 1], [1, 1]), sequence_position([1, 2, 1, 2], [2, 1])]"), "[[[1, 2], [2, 3], [3, 4]], 2, [[2, 3]]]");
}

#[test]
fn r33_sparse_array_with_pattern_positions_is_fast() {
    // 999 x 999 tridiagonal from two pattern rules (a million positions): about a second, not ten
    assert_eq!(
        ev("s = sparse_array([[i_, i_] -> -2., [i_, j_] when abs(i - j) == 1 -> 1.], [999, 999]); [s[1][1], s[1][2], s[2][1], s[2][3], s[5][9], s[500][500]]"),
        "[-2.0, 1.0, 1.0, 1.0, 0, -2.0]"
    );
    assert_eq!(ev("sparse_array([[i_, j_] when mod(i + j, 3) == 0 -> i*j], [3, 3])"), "[[0, 2, 0], [2, 0, 0], [0, 0, 9]]");
}

#[test]
fn r33_number_theory_threading_options_and_irrational_constants() {
    assert_eq!(ev("[mod(sqrt(28), 3), mod(pi, 2), mod(-pi, 2), mod(x, 2), quotient(e^e^e, pi), quotient(7, 3, 1), quotient(x, pi)]"), "[-3 + 2*7^(1/2), -2 + pi, 4 - pi, mod(x, 2), 1214122, 2, quotient(x, pi)]");
    assert_eq!(ev("[divisors([6, 10]), divisor_sum([2, 5, 10], &(&1^2)), prime_pi(10^6, method: \"Lehmer\")]"), "[[[1, 2, 3, 6], [1, 2, 5, 10]], [5, 26, 130], 78498]");
}

#[test]
fn r33_linear_algebra_options_counts_and_ball_matrices() {
    assert_eq!(ev("[hilbert_matrix(3, target_structure: \"Dense\"), rotation_matrix(pi/2, target_structure: \"Dense\"), cholesky([[4, 2], [2, 3]], target_structure: \"Dense\")[1][1]]"), "[[[1, 1/2, 1/3], [1/2, 1/3, 1/4], [1/3, 1/4, 1/5]], [[0, -1], [1, 0]], 2]");
    assert_eq!(ev("hilbert_matrix(2, working_precision: machine_precision)"), "[[1.0, 0.5], [0.5, 0.3333333333333333]]");
    // eigenvalues of a matrix of 12-digit numbers: the smallest three (machine arithmetic), the Hilbert-type matrix
    assert_eq!(ev("h = table(n(1/(i + j + 1), 12), i, 6, j, 6); e = eigenvalues(h, -3); [length(e), abs(e[3]/5.374160219636232e-9 - 1) < 1e-6]"), "[3, true]");
    assert_eq!(ev("length(eigenvalues([[1/3, 1/4, 1/5], [1/4, 1/5, 1/6], [1/5, 1/6, 1/7]], up_to(4)))"), "3");
}

#[test]
fn r33_nearest_with_radius_distance_function_and_result_forms() {
    assert_eq!(ev("[nearest([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 5, [3, 2]), nearest([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 5, [all, 2])]"), "[[5, 4, 6], [5, 4, 6, 3, 7]]");
    assert_eq!(ev("nearest([[1.5, 0.6], [2, 0], [1.25, 1.25]], [0, 0], distance_function: manhattan_distance)"), "[[2, 0]]");
    assert_eq!(ev("[nearest([\"Im\", \"sorry\", \"Dave\"] -> \"Index\", \"David\"), nearest([1, 2, 4, 8, 16, 32] -> [\"Element\", \"Distance\"], 20, 3)]"), "[[3], [[16, 4.0], [8, 12.0], [32, 12.0]]]");
    assert_eq!(ev("[nearest([1.5, 3, 2.5, 1, e, 7] -> [\"a\", \"b\", \"c\", \"d\", \"e\", \"f\"], 2), nearest([1.5, 3, 2.5, 1, e, 7] -> automatic, 2)]"), "[[\"a\", \"c\"], [1, 3]]");
}

#[test]
fn r33_bin_lists_of_a_rule_gives_the_labels() {
    assert_eq!(ev("bin_lists([1.5, 3, 2.5, 1, e, 7] -> [\"a\", \"b\", \"c\", \"d\", \"e\", \"f\"], 2)"), "[[\"a\", \"d\"], [\"b\", \"c\", \"e\"], [], [\"f\"]]");
    assert_eq!(ev("bin_lists([1.5, 3, 2.5, 1, e, 7] -> automatic, 2)"), "[[1, 4], [2, 3, 5], [], [6]]");
}

// Inputs whose fix belongs to another crate (round 33): kept ignored.
#[test]
#[ignore = "needs mc-numfn: `surd` is not one of the primitives its prep step rewrites (find_root(surd(x, 2) == 1.5, [x, 1]))"]
fn r33_find_root_of_surd() {
    assert_eq!(ev("find_root(surd(x, 2) == 1.5, [x, 1])"), "[x -> 2.25]");
}

#[test]
#[ignore = "needs mc-calculus: minimize of a 2-norm residual takes over six seconds"]
fn r33_minimize_of_a_least_squares_norm() {
    assert_eq!(ev("m = [[1, 1], [1, 2], [1, 3]]; b = [7, 7, 8]; minimize(norm(dot(m, [x, y]) - b), [x, y])"), "[6^(-1/2), [x -> 19/3, y -> 1/2]]");
}

#[test]
fn r33_multinomial_with_a_symbolic_argument() {
    assert_eq!(ev("expand(multinomial(n, 1, 2) - (3 + (11/2)*n + 3*n^2 + (1/2)*n^3))"), "0");
}

#[test]
fn r33_trace_of_vectors_nonsquare_matrices_and_with_a_head_and_level() {
    assert_eq!(ev("[trace([1, 2, 3]), trace([[1, 2, 3], [4, 5, 6]], list), trace([[1, 2, 3], [4, 5, 6], [7, 8, 9]], add, 1), trace([[1, 2, 3], [0, 4, 5], [0, 0, 6]], mul), trace([[1, 2, 3], [4, 5, 6], [7, 8, 9]], f)]"), "[6, [1, 5], [12, 15, 18], 24, f(1, 5, 9)]");
    assert_eq!(ev("trace(array(a, [4, 3, 2]))"), "a(1, 1, 1) + a(2, 2, 2)");
}

#[test]
fn r33_fit_with_tikhonov_regularization() {
    // the normal equations plus lambda on the diagonal; numpy gives 0.499984875, 0.005499611, 5.0496022e-5
    assert_eq!(
        ev("data = [[0, 0], [0.001, 1], [0.01, 1]]; f = fit(data, [1, x, x^2], x, fit_regularization: [\"Tikhonov\", 1]); c = coefficient_list(f, x); [abs(c[1] - 0.4999848748) < 1e-9, abs(c[2] - 0.005499610866) < 1e-9, abs(c[3] - 5.0496022e-5) < 1e-12]"),
        "[true, true, true]"
    );
}

#[test]
fn r33_time_series_spaces_times_from_start_to_end() {
    assert_eq!(ev("ts = time_series([1, 2, 3], [0.1, 0.3]); ts(\"Path\")"), "[[0.1, 1], [0.2, 2], [0.3, 3]]");
    assert_eq!(ev("time_series([1, 2, 3], [10, 20])(\"Times\")"), "[10, 15, 20]");
}

#[test]
fn r33_dataset_queries_take_operator_forms() {
    let ds = "ds = dataset([assoc(\"a\" -> 1, \"b\" -> \"x\"), assoc(\"a\" -> 2, \"b\" -> \"y\"), assoc(\"a\" -> 3, \"b\" -> \"z\")]); ";
    assert_eq!(ev(&format!("{ds}ds[select(&(&1[\"a\"] < 3)), \"b\"]")), "[\"x\", \"y\"]");
    assert_eq!(ev(&format!("{ds}ds[sort_by(&(-&1[\"a\"])), \"a\"]")), "[3, 2, 1]");
    assert_eq!(ev(&format!("{ds}length(ds[select(&(&1[\"a\"] > 1))])")), "2");
}

#[test]
fn r33_discrete_plot3d_and_list_step_plot_read_their_data() {
    let image = |src: &str| ev(src).starts_with("image(");
    // several surfaces, wrapped functions, a one-bound axis, a list of values on an axis, a density by name
    assert!(image("discrete_plot3d([sin(t + u), cos(t + u)], [t, 0, 2*pi, pi/12], [u, 0, 2*pi, pi/12])"));
    assert!(image("discrete_plot3d(style([sin(t + u), style(cos(t + u), green)], red), [t, 0, 2*pi, pi/12], [u, 0, 2*pi, pi/12])"));
    assert!(image("discrete_plot3d(lcm(x, y), [x, 20], [y, 20])"));
    assert!(image("discrete_plot3d(tooltip(gcd(x, y), gcd(x, y)), [x, prime(range(10))], [y, prime(range(10))])"));
    assert!(image("discrete_plot3d(pdf(multivariate_poisson_distribution(3, [1, 1]), [t, u]), [t, 0, 10], [u, 0, 10])"));
    // gaps, associations, wrapped sets, quantities
    assert!(image("list_step_plot([1, 2, 3, none, 5, 6, missing(\"NotAvailable\"), 8, 9])"));
    assert!(image("list_step_plot(assoc(2 -> 1, 3 -> 2, 5 -> 3))"));
    assert!(image("list_step_plot(style([style([1, 2, 3], green), [4, 5, 6]], blue))"));
    assert!(image("list_step_plot(quantity(sqrt(range(10)), \"Meters\"))"));
    assert!(image("list_step_plot([legended(prime_pi(range(20)), prime_pi(n_u)), legended(euler_phi(range(20)), euler_phi(n_u))])"));
}

#[test]
fn r33_plots_take_infinite_ranges_and_region_domains() {
    let image = |src: &str| ev(src).starts_with("image(");
    assert!(image("plot(sinc(x), [x, -inf, inf])"));
    assert!(image("plot3d(atan(x)*atan(y), [x, -inf, inf], [y, -inf, inf])"));
    assert!(image("re_im_plot(exp(-(1/2 + im)*t), [t, 0, inf], plot_range: all)"));
    assert!(image("plot3d(x + y, element([x, y], disk()))"));
    assert!(image("contour_plot(x + y, element([x, y], disk()))"));
    assert!(image("plot(sin(x), element(x, interval([0, 3])))"));
    assert!(image("bar_chart(weighted_data([1, 2, 3], [0.5, 0.2, 0.1]))"));
}

#[test]
fn r33_check_arguments_reads_a_spread_call_without_running_it() {
    // `apply(f, args)` is how a rule with `args___` writes f(args...): the call must not run f
    assert_eq!(ev("rule f(x_, y_) := 2*x*y; rule f(args___) := null when check_arguments(apply(f, args), 2); f(2)"), "f(2)");
    assert_eq!(ev("rule f(x_, y_) := 2*x*y; rule f(args___) := null when check_arguments(apply(f, args), 2); f(3, 4)"), "24");
    assert_eq!(ev("rule g(args___) := h(args) when check_arguments(apply(g, args), [0, 1]); g(1, 2, 3)"), "g(1, 2, 3)");
}

#[test]
fn r33_dataset_part_with_a_list_of_column_names() {
    let ds = "ds = dataset([assoc(\"a\" -> 1, \"b\" -> \"x\", \"c\" -> 5), assoc(\"a\" -> 2, \"b\" -> \"y\", \"c\" -> 6)]); ";
    assert_eq!(ev(&format!("{ds}columns(ds[select(&(&1[\"a\"] < 2)), [\"b\", \"c\"]])")), "[\"b\", \"c\"]");
}

#[test]
fn r33_measures_of_a_map_with_ranges_and_coordinate_systems() {
    assert_eq!(ev("arc_length([sin(t), cos(t)], [t, 0, 2*pi])"), "2*pi");
    assert_eq!(ev("arc_length([1, t, t], [t, 0, 2*pi], \"Cylindrical\")"), "2*pi*2^(1/2)");
    assert_eq!(ev("area([r*sin(t), r*cos(t)], [r, 1, 2], [t, 0, 2*pi])"), "3*pi");
    assert_eq!(ev("area([1, t, z], [z, 0, 1], [t, 0, 2*pi], \"Cylindrical\")"), "2*pi");
    assert_eq!(ev("volume([s, t, u], [s, 0, a], [t, 0, b], [u, 0, c])"), "a*b*c");
    assert_eq!(ev("volume([r, t, p], [r, 0, 1], [t, 0, pi], [p, 0, 2*pi], \"Spherical\")"), "(4/3)*pi");
    // the seam of a full turn is inside the annulus: its boundary is the two circles (2 pi + 4 pi)
    assert_eq!(ev("perimeter([r*sin(t), r*cos(t)], [r, 1, 2], [t, 0, 2*pi])"), "6*pi");
    assert_eq!(ev("perimeter([r, t], [r, 0, 1], [t, 0, pi/3], \"Polar\")"), "2 + (1/3)*pi");
}

#[test]
fn r33_implicit_regions_moments_solids_and_nearest_points() {
    assert_eq!(ev("region_moment(implicit_region(x^2 + y^2 <= 1, [x, y]), [2, 2])"), "(1/24)*pi");
    // the solid cylinder: the integral of y^2 z^4 is (pi/4)(32/5)
    assert_eq!(ev("region_moment(implicit_region(x^2 + y^2 <= 1, [x, y, [z, 0, 2]]), [0, 2, 4])"), "(8/5)*pi");
    assert_eq!(ev("volume(implicit_region(x^2 + y^2 <= 1, [x, y, [z, 0, 2]]))"), "2*pi");
    // half the disk: the integral of x y^2 is 2/15
    assert_eq!(ev("region_integrate(x*y^2, [x, y], implicit_region(x^2 + y^2 <= 1 and x >= 0, [x, y]))"), "2/15");
    assert_eq!(ev("region_nearest(implicit_region(x^2 + y^2 <= 1, [x, y]), [3, 4])"), "[3/5, 4/5]");
    assert_eq!(ev("region_nearest(implicit_region(x^2 + y^2 <= 1, [x, y, [z, 0, 2]]), [3, 4, 5])"), "[3/5, 4/5, 2]");
    assert_eq!(ev("region_distance(implicit_region(x^2 + y^2 <= 1, [x, y]), [3, 4])"), "4");
}

#[test]
fn r33_conic_hulls_draw_and_several_closed_curves_have_a_winding_region() {
    let image = |src: &str| ev(src).starts_with("image(");
    assert!(image("graphics(conic_hull_region([[0, 0]], [[1, -1], [1, 1]]))"));
    assert!(image("graphics([pink, conic_hull_region([[0, 0]], [[1, 1], [-1, 1]])], plot_range: [[-2, 2], [-1, 2]], frame: true)"));
    assert!(image("graphics(conic_hull_region([1, 1], [[2, 1]]))"));
    // two overlapping squares: 4 + 4 - 1
    assert_eq!(ev("area(winding_polygon([[[0, 0], [2, 0], [2, 2], [0, 2]], [[1, 1], [3, 1], [3, 3], [1, 3]]]))"), "7");
}

#[test]
fn r33_rotate_about_a_position_and_the_area_of_an_annulus_sector() {
    let image = |src: &str| ev(src).starts_with("image(");
    assert!(image("graphics([circle(), rotate(rectangle(), pi/4, [right, bottom])], axes: true)"));
    assert!(image("graphics(rotate(circle(), pi/2, right), axes: true)"));
    assert!(image("graphics(scale(rectangle(), 2, left))"));
    // (3 pi / 2) / 2 (r^2 - r^2 / 4)
    assert_eq!(ev("area(annulus([0, 0], [r/2, r], [0, 3*(pi/2)]))"), "(9/16)*pi*r^2");
}

#[test]
fn r33_complex_sector_bijectivity_does_not_run_the_four_variable_decomposition() {
    // function_bijective: used to spend 12 s in a decomposition (and ignore an interrupt); now it answers at once
    let t = std::time::Instant::now();
    let v = ev("function_bijective([x^4, re(x) >= 0 and im_part(x) > 0 or x == 0], x, complexes)");
    assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());
    assert!(v.starts_with("function_bijective(") || v == "true", "{v}");
}

#[test]
fn r33_minimize_with_a_kink_in_several_variables() {
    // |x + 2y - 3| on the triangle x, y >= 0, x + y <= 2 vanishes on a segment; the gradient never does
    assert_eq!(ev("v = minimize([abs(x + 2*y - 3), x >= 0 and y >= 0 and x + y <= 2], [x, y]); v[1]"), "0");
    assert_eq!(ev("v = minimize([abs(x + 2*y - 3), x >= 0 and y >= 0 and x + y <= 2], [x, y]); p = v[2]; rewrite(abs(x + 2*y - 3) == 0 and x >= 0 and y >= 0 and x + y <= 2, p)"), "true");
}

#[test]
fn r33_power_expand_with_explicit_assumptions_keeps_the_branch_corrections() {
    // arg x and arg y in (pi/2, pi): arg(x y) in (pi, 2 pi) wraps once, so sqrt(x y) = -sqrt(x) sqrt(y)
    assert_eq!(
        ev("power_expand(sqrt(x*y), assumptions: pi/2 < arg(x) and arg(x) < pi and pi/2 < arg(y) and arg(y) < pi)"),
        "-1*x^(1/2)*y^(1/2)"
    );
    // checked at a point: x = y = i (arguments pi/2: boundary), x = y = -1 + i (3 pi/4)
    assert_eq!(ev("sqrt((-1 + im)*(-1 + im)) == -sqrt(-1 + im)*sqrt(-1 + im)"), "true");
    assert_eq!(ev("power_expand(sqrt(x^2), assumptions: x < 0)"), "-x");
    assert_eq!(ev("power_expand(sqrt(x^2), assumptions: x > 0)"), "x");
    assert_eq!(ev("power_expand(log((a*b)^c), assumptions: 3 < a and a < 5 and -2 < b and b < -1 and 7 < c and c < 9)"), "-8im*pi + c*(log(a) + log(b))");
    assert_eq!(ev("power_expand((exp(x))^y, assumptions: 4 < im_part(x) and im_part(x) < 5)"), "exp(x*y + -2im*pi*y)");
    // undecided: the winding stays as a floor
    assert_eq!(ev("power_expand(log(x*y), assumptions: true)"), "log(x) + log(y) + 2im*pi*floor(1/2 + (-1/2)*(arg(x) + arg(y))/pi)");
    // without the option the formal law, as before
    assert_eq!(ev("power_expand(sqrt(x*y))"), "x^(1/2)*y^(1/2)");
}

#[test]
fn r33_infinite_continued_fraction_k_with_constant_terms() {
    assert_eq!(ev("abs(n(continued_fraction_k(1, [n, 1, inf]) - (golden_ratio - 1), 30)) < 10^-25"), "true");
    // K = f/(g + K): 2/(3 + K)
    assert_eq!(ev("v = continued_fraction_k(2, 3, [n, 1, inf]); simplify(v - 2/(3 + v))"), "0");
    assert_eq!(ev("continued_fraction_k(n^2, [n, 1, 10])"), "13690825314204/17021645372933");
}

#[test]
#[ignore = "needs the pattern matcher of mc-eval (a flat sum matched by x_ + y_ takes the first term and the rest)"]
fn r33_flat_sum_matched_by_two_term_pattern() {
    assert_eq!(ev("rewrite(a + b + c, x_ + y_ -> [x, y])"), "[a, b + c]");
    assert_eq!(ev("replace(a + b + c, x_ + y_ :> [x, y])"), "[a, b + c]");
}

#[test]
fn r33_euler_numbers_beyond_600_by_the_beta_series() {
    // sum_j C(n, 2j) E_2j = 0 for even n >= 2: indices on both sides of the switch from the recurrence
    assert_eq!(
        ev("e = table(euler_e(j), j, range(0, 1000)); [total(table(binomial(1000, 2*k)*e[2*k + 1], k, range(0, 500))), total(table(binomial(700, 2*k)*e[2*k + 1], k, range(0, 350))), e[601] > 0, e[603] < 0]"),
        "[0, 0, true, true]"
    );
    let t = std::time::Instant::now();
    assert_eq!(ev("length(integer_digits(euler_e(10000)))"), "33699");
    assert!(t.elapsed().as_secs() < 10, "took {:?}", t.elapsed());
}

#[test]
fn r33_bell_polynomial_of_large_index_at_a_number_and_two_cubes_of_large_integers() {
    assert_eq!(ev("bell_b(50, 2) == total(table(stirling_s2(50, j)*2^j, j, range(0, 50)))"), "true");
    assert_eq!(ev("abs(bell_b(1000, 0.5) / (2.0330084852554126e1873~17) - 1) < 1e-12"), "true");
    assert_eq!(ev("powers_representations(48988659276962496, 2, 3)"), "[[38787, 365757], [107839, 362753], [205292, 342952], [221424, 336588], [231518, 331954]]");
    assert_eq!(ev("powers_representations(1000001, 2, 3) == pw_reps(1000001, 2, 3, 0)"), "true");
}

#[test]
fn r33_convolution_of_unit_steps_and_boxes_with_a_symbolic_argument() {
    assert_eq!(ev("convolve(exp(-x) * unit_step(x), exp(-x) * unit_step(x), x, y)"), "y*exp(-y)*unit_step(y)");
    assert_eq!(ev("convolve(unit_step(x), unit_step(x), x, y)"), "y*unit_step(y)");
    assert_eq!(ev("convolve(a^x * unit_step(x), unit_step(x), x, y)"), "(-1 + a^y)/log(a)*unit_step(y)");
    // the triangle: 1 - |y| on (-1, 1)
    assert_eq!(
        ev("c = convolve(unit_box(x), unit_box(x), x, y); [rewrite(c, y -> 1/4), rewrite(c, y -> -1/2), rewrite(c, y -> 0), rewrite(c, y -> 3/2), rewrite(c, y -> -3/2)]"),
        "[3/4, 1/2, 1, 0, 0]"
    );
    // a box and an exponential, against numeric quadrature at two points
    assert_eq!(
        ev("c = convolve(unit_box(x), unit_step(x)*exp(-x), x, y); [abs(rewrite(c, y -> 0.2) - n_integrate(exp(-(0.2 - t)), [t, -1/2, 0.2])) < 1e-9, abs(rewrite(c, y -> 2.) - n_integrate(exp(-(2. - t)), [t, -1/2, 1/2])) < 1e-9]"),
        "[true, true]"
    );
}

#[test]
fn r33_classical_wronskians_casoratian_and_difference_quotient_under_assumptions() {
    assert_eq!(ev("wronskian([airy_ai(x), airy_bi(x)], x)"), "pi^-1");
    assert_eq!(ev("abs(rewrite(wronskian([bessel_j(2, x), bessel_y(2, x)], x) - (bessel_j(2, x)*d(bessel_y(2, x), x) - d(bessel_j(2, x), x)*bessel_y(2, x)), x -> 1.3)) < 1e-12"), "true");
    assert_eq!(ev("wronskian([bessel_i(1, x), bessel_k(1, x)], x)"), "-x^-1");
    assert_eq!(ev("c = casoratian([fibonacci(n), lucas_l(n)], n); table(rewrite(c, n -> m) - (fibonacci(m)*lucas_l(m + 1) - fibonacci(m + 1)*lucas_l(m)), m, range(0, 8))"), "[0, 0, 0, 0, 0, 0, 0, 0, 0]");
    assert_eq!(ev("difference_quotient(abs(x), [x, h], assumptions: x > 0 and h > 0)"), "1");
    assert_eq!(ev("difference_quotient(abs(x), [x, h], assumptions: x < 0 and h < 0)"), "-1");
}

#[test]
fn r33_unit_step_refines_under_a_sign_and_power_expand_uses_the_principal_range_of_arg() {
    assert_eq!(ev("simplify(unit_step(x), x > 0)"), "1");
    assert_eq!(ev("refine(unit_step(-x), x > 0)"), "0");
    assert_eq!(ev("refine(unit_step(x))"), "unit_step(x)");
    assert_eq!(ev("expand(simplify(convolve(unit_step(t) / e^t, t^2 * unit_step(t), t, x), x > 0))"), "2 + -2*x + -2*exp(-x) + x^2");
    // p arg(z) lies in (-pi, pi) for 0 < p < 1: the winding is 0
    assert_eq!(ev("power_expand((z^p)^(1 / p), assumptions: 0 < p and p < 1)"), "z");
}
