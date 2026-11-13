//! Round 39: matrices modulo a prime (row reduction, null space, range space), positive definiteness of matrices of
//! exact constants, shape tests with a tolerance or a test, polynomial extrapolation of padded lists, parts of
//! complex centered intervals and complex values of uncertain numbers, run lengths of list patterns.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn matrices_modulo_a_prime() {
    let m = "m = [[1, 0, 4], [2, 0, 3], [2, 1, 2]]; ";
    assert_eq!(ev(&format!("{m}null_space(m, modulus: 5)")), "[[1, 1, 1]]");
    // the null vectors are annihilated mod 5
    assert_eq!(ev(&format!("{m}mod(dot(m, transpose(null_space(m, modulus: 5))), 5)")), "[[0], [0], [0]]");
    assert_eq!(ev(&format!("{m}row_reduce(m, modulus: 5)")), "[[1, 0, 4], [0, 1, 4], [0, 0, 0]]");
    assert_eq!(ev(&format!("{m}range_space(m, modulus: 5)")), "[[1, 2, 0], [0, 0, 1]]");
    // over the rationals the matrix is invertible
    assert_eq!(ev(&format!("{m}null_space(m)")), "[]");
    // a larger case mod 7: rank plus nullity is the number of columns, and every null vector is annihilated
    let a = "a = [[3, 1, 4, 1], [5, 2, 6, 5], [1, 0, 2, 4]]; ";
    assert_eq!(ev(&format!("{a}ns = null_space(a, modulus: 7); [length(ns) + length(range_space(transpose(a), modulus: 7)), mod(dot(a, transpose(ns)), 7) == 0*dot(a, transpose(ns))]")), "[4, true]");
    assert_eq!(ev(&format!("{m}range_space(m, tolerance: 10^(-2))")), "[[1, 0, 0], [0, 1, 0], [0, 0, 1]]");
    // a modulus that is not a prime stays unevaluated
    assert!(ev(&format!("{m}null_space(m, modulus: 6)")).starts_with("null_space("));
}

#[test]
fn definiteness_and_shape_tests() {
    // a diagonal entry -3: e_2 . m . e_2 < 0
    assert_eq!(ev("positive_definite_matrix_q([[pi, -5, 2], [e, -3, -3], [5, sqrt(2), 5]])"), "false");
    assert_eq!(ev("positive_definite_matrix_q([[pi, 1], [1, e]])"), "true");
    // pi e - 9 < 0: not definite, as the eigenvalues of the matrix say
    assert_eq!(ev("[positive_definite_matrix_q([[pi, 3], [3, e]]), min(n(eigenvalues([[pi, 3], [3, e]]))) > 0]"), "[false, false]");
    let m2 = "m2 = [[1, log(x^2)], [2*log(x), 2]]; ";
    assert_eq!(ev(&format!("{m2}symmetric_matrix_q(m2, same_test: &(simplify(&1 - &2, x > 0) == 0))")), "true");
    // log(x^2) - 2 log(x) is not 0 for negative x
    assert_eq!(ev(&format!("{m2}symmetric_matrix_q(m2, tolerance: 10^(-12))")), "false");
    assert_eq!(ev("symmetric_matrix_q([[1, 2.], [2. + 10^(-13), 1]], tolerance: 10^(-12))"), "true");
    assert_eq!(ev("symmetric_matrix_q([[1, 2.], [2. + 10^(-10), 1]], tolerance: 10^(-12))"), "false");
    assert_eq!(ev("diagonal_matrix_q([[1., 10^(-12), 0], [0, 2., 10^(-13)], [0, 0, 3.]], tolerance: 10^(-12))"), "true");
    assert_eq!(ev("diagonal_matrix_q([[1., 10^(-11), 0], [0, 2., 0], [0, 0, 3.]], tolerance: 10^(-12))"), "false");
    assert_eq!(ev("upper_triangular_matrix_q([[1., 2., 3.], [10^(-12), 4., 5.], [0, 10^(-13), 6.]], tolerance: 10^(-12))"), "true");
    assert_eq!(ev("lower_triangular_matrix_q([[1., 10^(-13)], [3., 4.]], tolerance: 10^(-12))"), "true");
}

#[test]
fn polynomial_extrapolation() {
    assert_eq!(ev("array_pad([1, 5, 7, 8], [0, 3], \"Extrapolated\", interpolation_order: 2)"), "[1, 5, 7, 8, 8, 7, 5]");
    assert_eq!(ev("array_pad([1, 5, 7, 8], [0, 3], \"Extrapolated\", interpolation_order: inf)"), "[1, 5, 7, 8, 9, 11, 15]");
    // the cubic through the four points, at the new places
    assert_eq!(ev("p = interpolating_polynomial([1, 5, 7, 8], t); array_pad([1, 5, 7, 8], [2, 3], \"Extrapolated\", interpolation_order: inf) == map(k => rewrite(p, t -> k), range(-1, 7))"), "true");
    // a quadratic is continued exactly by the order 2
    assert_eq!(ev("v = table(k^2 - 3*k, k, 5); array_pad(v, 3, \"Extrapolated\", interpolation_order: 2) == table(k^2 - 3*k, [k, -2, 8])"), "true");
    assert_eq!(ev("v = n(exp(range(0, 5))); w = array_pad(v, 5, \"Extrapolated\", interpolation_order: inf); p = interpolating_polynomial(v, t); max(abs(w - map(k => rewrite(p, t -> k), range(-4, 11)))) < 10^-9"), "true");
}

#[test]
fn parts_of_complex_centered_intervals() {
    // the sides of the box [-3, -1] x [1, 5]
    assert_eq!(ev("c = re(centered_interval(-2 + 3*im, 1 + 2*im)); abs(c[1] + 2) < 10^-14 and abs(c[2] - 1) < 10^-14"), "true");
    assert_eq!(ev("c = im_part(centered_interval(-2 + 3*im, 1 + 2*im)); abs(c[1] - 3) < 10^-14 and abs(c[2] - 2) < 10^-14"), "true");
    assert_eq!(ev("c = conjugate(centered_interval(-5 - 3*im, 1 + im)); abs(c[1] - (-5 + 3*im)) < 10^-14 and abs(c[2] - (1 + im)) < 10^-14"), "true");
    // the modulus over [1, 3] x [2, 4]: from |1 + 2 i| to |3 + 4 i|
    assert_eq!(ev("c = abs(centered_interval(2 + 3*im, 1 + im)); [c[1] - c[2] <= sqrt(5.) and sqrt(5.) - (c[1] - c[2]) < 10^-12, c[1] + c[2] >= 5 and c[1] + c[2] - 5 < 10^-12]"), "[true, true]");
    // the argument between the corner arguments
    assert_eq!(ev("c = arg(centered_interval(5 + 7*im, (1 + im)/100)); lo = atan(6.99/5.01); hi = atan(7.01/4.99); [c[1] - c[2] <= lo and lo - (c[1] - c[2]) < 10^-14, c[1] + c[2] >= hi and c[1] + c[2] - hi < 10^-14]"), "[true, true]");
    // a box over the negative real axis has no continuous argument
    assert!(ev("arg(centered_interval(-5 + 0.1*im, 1 + im))").starts_with("arg("));
}

#[test]
fn complex_values_of_uncertain_numbers() {
    assert_eq!(ev("a = conjugate(around(-1.1 + im, 0.01)); [is_number(a[1]), a[1] == -1.1 - 1.0*im, a[2]]"), "[true, true, 0.01]");
    // acoth(0.9) and the first-order spread |d acoth/dx| dx = 0.1/(1 - 0.81)
    assert_eq!(ev("a = acoth(around(0.9, 0.1)); abs(a[1] - n(acoth(9/10))) < 10^-14 and abs(a[2] - 0.1/0.19) < 10^-14"), "true");
    assert_eq!(ev("a = acoth(around(0.5, 0.01)); abs(a[1] - n(acoth(1/2))) < 10^-14 and abs(a[2] - 0.01/0.75) < 10^-14"), "true");
    // real values are as before
    assert_eq!(ev("a = acoth(around(2., 0.1)); abs(a[1] - n(acoth(2))) < 10^-14 and abs(a[2] - 0.1/3) < 10^-14"), "true");
}

#[test]
fn run_lengths_of_list_patterns() {
    let p = "[(t1_ when is_prime(t1)), _, (t2_ when is_prime(t2))]";
    // the twin primes p, p + 2 below 1000 with the number between them: 35 pairs, the first [3, 4, 5]
    assert_eq!(ev(&format!("r = sequence_cases(range(1000), {p}); [length(r), first(r)]")), "[34, [3, 4, 5]]");
    assert_eq!(ev(&format!("sequence_count(range(1000), {p}) == length(select(range(997), k => is_prime(k) and is_prime(k + 2))) - 1")), "true");
    assert_eq!(ev(&format!("take(sequence_position(range(5000), {p}), -1)")), "[[4967, 4969]]");
    // runs of other lengths are still found: sequences, repeats, optional elements
    assert_eq!(ev("sequence_cases([1, 2, 3, 4, 5, 6], [x__, 4, y___])"), "[[1, 2, 3, 4, 5, 6]]");
    assert_eq!(ev("sequence_cases([a, b, b, c, b], [b, repeated(b, [0, 2])])"), "[[b, b], [b]]");
    assert_eq!(ev("sequence_cases([1, 2, 3, 1, 3], [1, optional(z_, 0), 3])"), "[[1, 2, 3], [1, 3]]");
}

#[test]
fn interval_images_with_a_pole_at_an_end() {
    // csch decreases from inf at 0 to csch(log 3) = 3/4
    assert_eq!(ev("r = csch(interval([0, log(3)])); [r[1, 2], abs(n(r[1, 1]) - 0.75) < 10^-15]"), "[inf, true]");
    // acoth decreases from inf at 1 to acoth(coth(5)) = 5
    assert_eq!(ev("r = acoth(interval([1, coth(5)])); [r[1, 2], abs(n(r[1, 1], 30) - 5) < 10^-20]"), "[inf, true]");
    // on the other side of the pole: -inf up to the value at the end
    assert_eq!(ev("r = csch(interval([-log(3), 0])); [r[1, 1], abs(n(r[1, 2]) + 0.75) < 10^-15]"), "[-inf, true]");
    // a machine end at the pole
    assert_eq!(ev("r = csch(interval([0.0, 1.0])); [r[1, 2], abs(r[1, 1] - csch(1.0)) < 10^-15]"), "[inf, true]");
}


#[test]
fn arrays_of_blocks_and_other_forms() {
    assert_eq!(ev("dimensions(array_flatten(array(a, [1, 2, 3, 4, 5, 6]), 3))"), "[4, 10, 18]");
    // the entry (i, j, k) of block (b1, b2, b3) at ((b1 - 1) s1 + i, ...)
    assert_eq!(ev("t = array_flatten(array(a, [2, 2, 2, 3, 1, 2]), 3); [t[5, 2, 3], t[2, 1, 4]]"), "[a(2, 2, 2, 2, 1, 1), a(1, 1, 2, 2, 1, 2)]");
    // a number is a constant block; a block of ones and a zero block side by side
    assert_eq!(ev("array_flatten([[[constant_array(1, [1, 1, 2]), 0]]], 3)"), "[[[1, 1, 0, 0]]]");
    // rank 2 is unchanged
    assert_eq!(ev("array_flatten([[[[1, 2]], [[3]]]])"), "[[1, 2, 3]]");
    assert_eq!(ev("total(f(1, 2, 1), allowed_heads: inherited)"), "4");
    assert_eq!(ev("total(derivative(1, 2, 1), allowed_heads: inherited)"), "4");
    assert_eq!(ev("nearest(table(pauli_matrix(i), i, 3), [[1, -2], [3, -4]])"), "[[[1, 0], [0, -1]]]");
    // the distances checked by hand: 30, 32 and 22 under the square root
    assert_eq!(ev("map(x => norm(flatten(x - [[1, -2], [3, -4]]))^2, table(pauli_matrix(i), i, 3))"), "[30, 32, 22]");
    assert_eq!(ev("identity_matrix(3, working_precision: machine_precision)"), "[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]");
    assert_eq!(ev("x = identity_matrix(2, working_precision: 30); [is_inexact(x[1, 1]), x == identity_matrix(2)]"), "[true, true]");
    // the measure of a map with a list of ranges: a volume in cylindrical coordinates, an area of a plane map
    assert_eq!(ev("region_measure([r, t, z], [[r, 0, R], [t, 0, 2*pi], [z, 0, Z]], \"Cylindrical\") == pi*R^2*Z"), "true");
    assert_eq!(ev("region_measure([r*cos(x), r*sin(x)], [[x, 0, 2*pi], [r, 0, 2]])"), "4*pi");
}

#[test]
fn curls_of_scalars_and_inactive_gradients() {
    assert_eq!(ev("curl(f(x, y), [x, y])"), "[-derivative(0, 1)(f)(x, y), derivative(1, 0)(f)(x, y)]");
    // in space: the antisymmetric matrix eps_ijk f_k
    assert_eq!(ev("cur = curl(x*y*z, [x, y, z]); [transpose(cur) == -cur, cur[1, 2] == x*y, cur[1, 3] == -x*z, cur[2, 3] == y*z]"), "[true, true, true, true]");
    // the plane curl of the scalar is the gradient turned a quarter: orthogonal to it
    assert_eq!(ev("simplify(dot(curl(x^2*y^3, [x, y]), grad(x^2*y^3, [x, y])))"), "0");
    assert_eq!(ev("curl(inactive(grad)(f(x, y, z), [x, y, z]), [x, y, z])"), "[0, 0, 0]");
    assert_eq!(ev("curl(inactive(grad)([f(x, y, z), g(x, y, z), h(x, y, z)], [x, y, z]), [x, y, z])"), "0");
}

#[test]
fn boolean_functions_by_number_and_dataset_rows() {
    assert_eq!(ev("is_satisfiable(boolean_function(110, 3))"), "true");
    assert_eq!(ev("is_satisfiable(boolean_function(0, 20))"), "false");
    // the same as the truth table of the function says
    assert_eq!(ev("is_tautology(boolean_function(255, 3))"), "true");
    assert_eq!(ev("is_tautology(boolean_function(254, 3))"), "false");
    let ds = "d = dataset([assoc(\"a\" -> 1, \"b\" -> \"x\"), assoc(\"a\" -> 6, \"b\" -> \"z\"), assoc(\"a\" -> 7, \"b\" -> \"y\")]); ";
    assert_eq!(ev(&format!("{ds}d[select_first(&(&1[\"a\"] > 5)), \"b\"]")), "\"z\"");
    assert_eq!(ev(&format!("{ds}select_first(d, &(&1[\"a\"] > 6))")), "assoc(\"a\" -> 7, \"b\" -> \"y\")");
    assert_eq!(ev(&format!("{ds}select_first(d, &(&1[\"a\"] > 9), none)")), "none");
}

#[test]
fn measures_of_maps_that_fold() {
    // the figure eight [r cos x, r sin 2x]: its Jacobian has both signs, the area is the integral of its size
    assert_eq!(ev("region_measure([r*cos(x), r*sin(2*x)], [[x, 0, 2*pi], [r, 1/2, 1]])"), "2");
    assert_eq!(ev("abs(n_integrate(abs(r*(sin(x)*sin(2*x) + 2*cos(x)*cos(2*x))), [x, 0, 2*pi], [r, 1/2, 1]) - 2) < 10^-6"), "true");
    // a map of one sign is unchanged
    assert_eq!(ev("area(parametric_region([r*cos(t), r*sin(t)], [[r, 0, 1], [t, 0, 2*pi]]))"), "pi");
    assert_eq!(ev("first(n_dsolve([x''(t) + x(t) == 0, x(0) == 1, x'(0) == 0], x, [t, 0, 20], max_steps: inf))[2](20) - cos(20.) < 10^-5"), "true");
}

