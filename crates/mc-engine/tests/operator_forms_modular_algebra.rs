//! Operator forms of list and association functions, the `scaled` width of `min_max`, the symmetric coefficient
//! arrays, a Boolean function in a named form, `center_array` with an inherited size, ratio-unit quantities and the
//! light colour of `light_dark_switched`, and the modular and number-field options of `is_irreducible_polynomial`,
//! `matrix_minimal_polynomial` and `interpolating_polynomial`. Each new answer is checked by a second route (the
//! explicit-argument call, multiplying factors back, the truth table, annihilation of the matrix, values at the
//! nodes). Every test has a time bound.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn timed(what: &str, limit: u64, f: impl FnOnce()) {
    let t0 = Instant::now();
    f();
    let took = t0.elapsed();
    assert!(took < Duration::from_secs(limit), "{what} took {took:?}");
}

/// The operator form and the explicit call print the same, and as `want`.
fn same(e: &mut CoreEngine, op: &str, explicit: &str, want: &str) {
    assert_eq!(ev(e, op), want, "{op}");
    assert_eq!(ev(e, explicit), want, "{explicit}");
}

#[test]
fn operator_forms_agree_with_the_explicit_calls() {
    timed("operator forms", 60, || {
        let mut e = CoreEngine::new();
        same(&mut e, "fold_while(add, less_than(20))(range(10))", "fold_while(add, 1, range(2, 10), less_than(20))", "21");
        same(
            &mut e,
            "fold_while_list(subtract, is_positive)(12, [1, 2, 3, 4, 5, 6])",
            "fold_while_list(subtract, 12, [1, 2, 3, 4, 5, 6], is_positive)",
            "[12, 11, 9, 6, 2, -3]",
        );
        same(&mut e, "fold_while_list(add, less_than(20))(range(10))", "fold_while_list(add, 1, range(2, 10), less_than(20))", "[1, 3, 6, 10, 15, 21]");
        same(&mut e, "first_position(_: Int)([f(a), 2, 3, y])", "first_position([f(a), 2, 3, y], _: Int)", "[2]");
        same(&mut e, "select_keys(is_odd)(assoc(1 -> 2, 2 -> 3, 3 -> 4))", "select_keys(assoc(1 -> 2, 2 -> 3, 3 -> 4), is_odd)", "assoc(1 -> 2, 3 -> 4)");
        same(&mut e, "sequence_replace([a, e_u_] :> e_u)([a, b, x, x, a, c])", "sequence_replace([a, b, x, x, a, c], [a, e_u_] :> e_u)", "[b, x, x, c]");
        same(&mut e, "replace_repeated(f(x_) -> x)([f(f(x)), f(g(f(x)))])", "replace_repeated([f(f(x)), f(g(f(x)))], f(x_) -> x)", "[x, g(x)]");
        // the first n arguments reversed, the last -n for a negative n; a count beyond the arguments stays
        assert_eq!(ev(&mut e, "reverse_applied(f, 2)(a, b, c, d)"), "f(b, a, c, d)");
        assert_eq!(ev(&mut e, "reverse_applied(f, -2)(a, b, c, d)"), "f(a, b, d, c)");
        assert_eq!(ev(&mut e, "reverse_applied(f, 4)(a, b, c, d)"), ev(&mut e, "reverse_applied(f)(a, b, c, d)"));
        assert_eq!(ev(&mut e, "reverse_applied(f, 5)(a, b, c, d)"), "reverse_applied(f, 5)(a, b, c, d)");
    });
}

#[test]
fn min_max_scaled_width() {
    timed("min_max scaled", 60, || {
        let mut e = CoreEngine::new();
        // exact data: s times the range on each side
        assert_eq!(ev(&mut e, "min_max([1, 2, 3, 4], scaled(1/4))"), "[1/4, 19/4]");
        assert_eq!(ev(&mut e, "min_max([1, 2, 3, 4], scaled(1/4)) == min_max([1, 2, 3, 4], 3/4)"), "true");
        assert_eq!(ev(&mut e, "min_max([5, 1, 9], [scaled(1/2), 1])"), "[-3, 10]");
        // machine numbers
        let r = ev(&mut e, "let m = min_max([1.3, 2.9, 4.1], scaled(0.5)); abs(m[1] - (-0.1)) < 1e-12 and abs(m[2] - 5.5) < 1e-12");
        assert_eq!(r, "true");
    });
}

#[test]
fn symmetric_coefficient_arrays() {
    timed("symmetric coefficient arrays", 60, || {
        let mut e = CoreEngine::new();
        assert_eq!(
            ev(&mut e, "coefficient_arrays(a*x^2 + b*x*y + c*y^2 + d*x + e*y + f, [x, y], \"Symmetric\" -> true)"),
            "[f, [d, e], [[a, (1/2)*b], [(1/2)*b, c]]]"
        );
        // each array is symmetric, and contracting it with the vector of the variables k times gives the homogeneous
        // part of degree k of the polynomial
        let p = "4*x^2 + 9*y^2 + 12*x*y + y^3 + 3*x*y^2 - 5*x + 7";
        assert_eq!(ev(&mut e, &format!("cs = coefficient_arrays({p}, [x, y], symmetric: true); length(cs)")), "4");
        assert_eq!(ev(&mut e, "cs[3] == transpose(cs[3])"), "true");
        let contraction = "expand(cs[1] + dot(cs[2], [x, y]) + dot([x, y], dot(cs[3], [x, y])) + total(map(i => total(map(j => total(map(k => cs[4][i][j][k]*[x, y][i]*[x, y][j]*[x, y][k], range(2))), range(2))), range(2))))";
        assert_eq!(ev(&mut e, &format!("expand({p}) - {contraction}")), "0");
        // the upper-triangular form is unchanged without the option
        assert_eq!(ev(&mut e, "coefficient_arrays(x^2 + 2*x*y, [x, y])"), "[0, [0, 0], [[1, 2], [0, 0]]]");
    });
}

#[test]
fn boolean_function_in_a_named_form() {
    timed("boolean function forms", 60, || {
        let mut e = CoreEngine::new();
        assert_eq!(ev(&mut e, "boolean_function(33, [a, b, c], \"DNF\")"), "a and not b and c or not a and not b and not c");
        assert_eq!(ev(&mut e, "boolean_function(33, [a, b, c], \"CNF\")"), "(not a or c) and (a or not c) and not b");
        // both forms have the truth table of the index 33 (bit 4 a + 2 b + c, the first variable the highest)
        for form in ["DNF", "CNF"] {
            let check = format!(
                "let f = boolean_function(33, [a, b, c], \"{form}\"); every(tuples([true, false], 3), t => (rewrite(f, [a -> t[1], b -> t[2], c -> t[3]]) === boolean_function(33, 3)(t[1], t[2], t[3])))"
            );
            assert_eq!(ev(&mut e, &check), "true", "{form}");
        }
    });
}

#[test]
fn center_array_with_an_inherited_size() {
    timed("center_array inherited", 60, || {
        let mut e = CoreEngine::new();
        assert_eq!(ev(&mut e, "center_array([[9], [4, 5], [5, 5, 3]], [inherited, 3])"), "[[0, 9, 0], [4, 5, 0], [5, 5, 3]]");
        assert_eq!(ev(&mut e, "dimensions(center_array([[1, 2], [3, 4]], [inherited, 5]))"), "[2, 5]");
        assert_eq!(ev(&mut e, "center_array([1, 2], [inherited])"), "[1, 2]");
    });
}

#[test]
fn ratio_units_and_the_light_colour() {
    timed("ratio units", 60, || {
        let mut e = CoreEngine::new();
        assert_eq!(ev(&mut e, "normal(quantity(10, \"Percent\"))"), "1/10");
        assert_eq!(ev(&mut e, "normal(quantity(8, \"PartsPerMillion\"))"), "1/125000");
        // a unit with a dimension is left as it is
        assert_eq!(ev(&mut e, "normal(quantity(3, \"Meters\"))"), "quantity(3, \"Meters\")");
        assert_eq!(ev(&mut e, "input_form(normal(light_dark_switched(blue, green)))"), "rgb_color(0, 0, 1)");
        assert_eq!(ev(&mut e, "normal(light_dark_switched(orange, green))"), "rgb_color(1, 1/2, 0)");
        assert_eq!(ev(&mut e, "normal(light_dark_switched(rgb_color(1, 0, 0.5), blue))"), "rgb_color(1, 0, 0.5)");
        assert_eq!(ev(&mut e, "normal(light_dark_switched(foo, blue))"), "light_dark_switched(foo, blue)");
    });
}

#[test]
fn irreducibility_of_polynomials_in_several_variables() {
    timed("irreducibility", 120, || {
        let mut e = CoreEngine::new();
        // over F_3: a product written out is reducible, and the factor found multiplies back
        assert_eq!(ev(&mut e, "is_irreducible_polynomial(polynomial_mod(expand((x + y + 1)*(x + 2*y + 3)), 3), modulus: 3)"), "false");
        assert_eq!(ev(&mut e, "is_irreducible_polynomial([x*y + x + 1, 2 + 2*x + 2*x^2 + 2*y + x*y], modulus: 3)"), "[true, false]");
        // 2 + 2x + 2x^2 + 2y + xy = (x + 2)(2x + y + 1) modulo 3 (x + 2 divides both coefficients of y)
        assert_eq!(ev(&mut e, "polynomial_mod(expand((x + 2)*(2*x + y + 1)) - (2 + 2*x + 2*x^2 + 2*y + x*y), 3)"), "0");
        // x^2 + y^2 + 1 is monic in x and irreducible at y = 0 (x^2 + 1 over F_7): irreducible
        assert_eq!(ev(&mut e, "is_irreducible_polynomial(x^2 + y^2 + 1, modulus: 7)"), "true");
        assert_eq!(ev(&mut e, "is_irreducible_polynomial(x^2 + 1, modulus: 7)"), "true");
        // x^2 - 2 y^2 over F_7: 3^2 = 2, so (x - 3y)(x + 3y)
        assert_eq!(ev(&mut e, "is_irreducible_polynomial(x^2 - 2*y^2, modulus: 7)"), "false");
        assert_eq!(ev(&mut e, "polynomial_mod(expand((x - 3*y)*(x + 3*y)) - (x^2 - 2*y^2), 7)"), "0");
        // a binomial with coprime exponents has no factor over any field
        assert_eq!(ev(&mut e, "is_irreducible_polynomial(x^2 + y^3, modulus: 5)"), "true");
        assert_eq!(ev(&mut e, "is_irreducible_polynomial(x^2 + 4*y^4, modulus: 5)"), "false");
        // a number field: x^2 - 2 y^4 stays irreducible over Q(sqrt(3)); x^4 - 3 y^2 = (x^2 - sqrt(3) y)(x^2 + sqrt(3) y)
        assert_eq!(ev(&mut e, "is_irreducible_polynomial([x^2 - 2*y^4, x^4 - 3*y^2], extension: sqrt(3))"), "[true, false]");
        assert_eq!(ev(&mut e, "expand((x^2 - sqrt(3)*y)*(x^2 + sqrt(3)*y)) - (x^4 - 3*y^2)"), "0");
        // the algebraic closure: one variable needs degree 1; a conic y(2x) + x^2 - 7 is irreducible, a product is not
        assert_eq!(ev(&mut e, "is_irreducible_polynomial([x^3 - 3, x^2 + 2*x*y - 7, x^2 - y^2, x], extension: all)"), "[false, true, false, true]");
    });
}

#[test]
fn irreducibility_over_a_finite_field_extension() {
    timed("finite field irreducibility", 120, || {
        let mut e = CoreEngine::new();
        let f = "finite_field(17, &(14 + &1 + &1^3), polynomial)";
        let a = format!("finite_field_element({f}, [3])");
        let b = format!("finite_field_element({f}, [3, 2, 6])");
        // a x^2 + b y^3 (exponents 2 and 3): irreducible; a x^2 + b alone is a polynomial of one variable
        let r = ev(&mut e, &format!("is_irreducible_polynomial({a}*x^2 + {b}*y^3)"));
        assert_eq!(r, "true");
        let r = ev(&mut e, &format!("is_irreducible_polynomial({a}*x^2 + {b}*y^4)"));
        assert_ne!(r, "true", "x^2 + c y^4 factors when c is a square: no claim of irreducibility");
    });
}

#[test]
fn matrix_minimal_polynomial_modulo_a_prime() {
    timed("minimal polynomial mod p", 120, || {
        let mut e = CoreEngine::new();
        assert_eq!(ev(&mut e, "matrix_minimal_polynomial([[22, -16], [25, -18]], x, modulus: 19)"), "4 + 15*x + x^2");
        assert_eq!(ev(&mut e, "polynomial_mod(matrix_minimal_polynomial([[22, -16], [25, -18]], x) - matrix_minimal_polynomial([[22, -16], [25, -18]], x, modulus: 19), 19)"), "0");
        // the result annihilates the matrix modulo p, and the lower powers of the matrix are independent modulo p
        // (so no monic polynomial of lower degree does); one matrix has a degree below its size, one is a companion matrix
        for (m, p, degree) in [
            ("[[2, 0, 0], [0, 2, 0], [0, 0, 5]]", 7, 2),
            ("[[0, 1, 0], [0, 0, 1], [1, 1, 1]]", 2, 3),
            ("[[1, 2], [3, 4]]", 3, 2),
            ("[[4, 9, 2], [3, 5, 7], [8, 1, 6]]", 11, 3),
            ("[[3, 0], [0, 3]]", 5, 1),
        ] {
            let q = ev(&mut e, &format!("q = matrix_minimal_polynomial({m}, x, modulus: {p}); exponent(q, x)"));
            assert_eq!(q, degree.to_string(), "degree for {m} mod {p}");
            let n = if m.matches('[').count() == 3 { 2 } else { 3 };
            let annihilates = format!(
                "m = {m}; cl = coefficient_list(q, x); r = fold((acc, c) => dot(acc, m) + c*identity_matrix({n}), 0*identity_matrix({n}), reverse(cl)); every(flatten(r), v => mod(v, {p}) == 0)"
            );
            assert_eq!(ev(&mut e, &annihilates), "true", "annihilates {m}");
            let independent = format!(
                "pw = map(k => flatten(map(r => map(v => mod(v, {p}), r), matrix_power(m, k))), range(0, {degree} - 1)); null_space(transpose(pw), modulus: {p}) == []"
            );
            assert_eq!(ev(&mut e, &independent), "true", "lower powers of {m}");
            // monic, with coefficients in 0 .. p - 1
            assert_eq!(ev(&mut e, &format!("last(cl) == 1 and every(cl, c => c >= 0 and c < {p})")), "true");
        }
        // not a prime modulus or not an integer matrix: unevaluated
        assert_eq!(ev(&mut e, "matrix_minimal_polynomial([[1, 2], [3, 4]], x, modulus: 4)"), "matrix_minimal_polynomial([[1, 2], [3, 4]], x, modulus: 4)");
    });
}

#[test]
fn interpolating_polynomial_modulo_a_prime() {
    timed("interpolation mod p", 120, || {
        let mut e = CoreEngine::new();
        assert_eq!(
            ev(&mut e, "interpolating_polynomial([[0, 1], [4, 2], [9, 3], [16, 4]], x, modulus: 47)"),
            "1 + x*(12 + (-4 + x)*(6 + 31*(-9 + x)))"
        );
        // values at the nodes agree modulo p, for points and for values at 1, 2, ...
        for (data, nodes, p) in [
            ("[[0, 1], [4, 2], [9, 3], [16, 4]]", "[0, 4, 9, 16]", 47),
            ("[3, 5, 11, 2]", "[1, 2, 3, 4]", 13),
            ("[[-2, 7], [1, 0], [5, 12], [8, -3], [20, 4]]", "[-2, 1, 5, 8, 20]", 17),
        ] {
            let src = format!(
                "q = interpolating_polynomial({data}, x, modulus: {p}); d = {data}; ys = if is_list(d[1]) then map(last, d) else d; every(range(length(ys)), i => mod(rewrite(q, x -> {nodes}[i]) - ys[i], {p}) == 0)"
            );
            assert_eq!(ev(&mut e, &src), "true", "{data} mod {p}");
        }
        // two nodes equal modulo p: no interpolant, the call stays
        assert_eq!(
            ev(&mut e, "interpolating_polynomial([[0, 1], [13, 2]], x, modulus: 13)"),
            "interpolating_polynomial([[0, 1], [13, 2]], x, modulus: 13)"
        );
    });
}
