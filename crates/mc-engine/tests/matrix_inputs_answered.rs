//! Matrix and optimization inputs that used to fail or stay unevaluated: dot products of tensors of rank three,
//! semidefinite programs in a vector variable, positive definiteness of a symbolic matrix whose Hermitian part is
//! numeric, the LU factors, pseudo-inverse and 2-norm of symbolic matrices, the tolerance of least squares and of
//! the pseudo-inverse, interpolation over a region, and the options of gaussian_matrix. Each value is checked by a
//! route that does not share the code that computed it (sums written out here, substitution back, the Penrose
//! conditions, machine evaluation at sample points, a hand solution).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn dot_contracts_tensors_of_rank_three() {
    // t[i][j][k] = i + 2 j + 3 k (1-based); t . v sums over k, w . t over i: both written out here
    let t: Vec<Vec<Vec<i64>>> = (1..=2).map(|i| (1..=3).map(|j| (1..=2).map(|k| i + 2 * j + 3 * k).collect()).collect()).collect();
    let src_t = format!("{t:?}");
    let v = [5i64, -7];
    let w = [2i64, 11];
    let tv: Vec<Vec<i64>> = t.iter().map(|m| m.iter().map(|r| r[0] * v[0] + r[1] * v[1]).collect()).collect();
    let wt: Vec<Vec<i64>> = (0..3).map(|j| (0..2).map(|k| w[0] * t[0][j][k] + w[1] * t[1][j][k]).collect()).collect();
    assert_eq!(ev(&format!("dot({src_t}, {v:?})")), format!("{tv:?}"));
    assert_eq!(ev(&format!("dot({w:?}, {src_t})")), format!("{wt:?}"));
    // a chain: t . v . u with u of length 3 is the matrix-vector product of the rows of t . v
    let u = [1i64, 0, -2];
    let tvu: Vec<i64> = tv.iter().map(|r| r.iter().zip(u).map(|(a, b)| a * b).sum()).collect();
    assert_eq!(ev(&format!("dot({src_t}, {v:?}, {u:?})")), format!("{tvu:?}"));
    // the polynomial of its coefficient arrays comes back
    assert_eq!(
        ev("v = [x, y]; c = coefficient_arrays(expand((2*x + 3*y)^2 + y^3), v); expand(c[1] + dot(c[2], v) + dot(c[3], v, v) + dot(c[4], v, v, v)) - expand((2*x + 3*y)^2 + y^3)"),
        "0"
    );
    // shapes that do not fit, and a symbolic vector, as for matrices
    assert_eq!(ev("dot([[[1, 2], [3, 4]], [[5, 6], [7, 8]]], [1, 2, 3])"), "failure(shape_mismatch, dot)");
    assert_eq!(ev("dot([[[1, 2], [3, 4]], [[5, 6], [7, 8]]], w)"), "dot([[[1, 2], [3, 4]], [[5, 6], [7, 8]]], w)");
}

#[test]
fn semidefinite_program_in_a_vector_variable() {
    // minimize v1 + 2 v2 with [[v1, 1], [1, v2]] positive semidefinite: v1, v2 >= 0 and v1 v2 >= 1, so the least
    // is v1 + 2/v1 at v1 = sqrt(2) (derivative 1 - 2/v1^2 = 0), v2 = 1/sqrt(2)
    let setup = "c = [1, 2]; a_0_s = [[0, 1], [1, 0]]; a_1_s = [[1, 0], [0, 0]]; a_2_s = [[0, 0], [0, 1]]; ";
    for cons in [
        "vector_greater_equal([dot([a_1_s, a_2_s], v), -a_0_s], [semidefinite_cone, 2])",
        "vector_greater_equal([inactive(add)(a_0_s, dot([a_1_s, a_2_s], v)), 0], [semidefinite_cone, 2])",
    ] {
        let got = ev(&format!("{setup}semidefinite_optimization(dot(c, v), {cons}, v)"));
        assert_eq!(got, "[v -> [2^(1/2), (1/2)*2^(1/2)]]", "{cons}");
    }
    let par = "semidefinite_optimization(dot([1, 2], v), [[a_0_s == [[0, 1], [1, 0]], a_1_s == [[1, 0], [0, 0]], a_2_s == [[0, 0], [0, 1]]], vector_greater_equal([inactive(add)(a_0_s, dot([a_1_s, a_2_s], v)), 0], [semidefinite_cone, 2])], v)";
    assert_eq!(ev(par), "[v -> [2^(1/2), (1/2)*2^(1/2)]]");
    // the point is feasible (the determinant is 0: on the boundary) and no point of a grid does better
    assert_eq!(ev("{ let p = [2^(1/2), (1/2)*2^(1/2)]; [simplify(p[1]*p[2] - 1), simplify(p[1] + 2*p[2] - 2*2^(1/2))] }"), "[0, 0]");
    let mut best = f64::INFINITY;
    for i in 1..400 {
        let v1 = i as f64 / 100.0;
        best = best.min(v1 + 2.0 / v1);
    }
    assert!(best >= 2.0 * 2f64.sqrt() - 1e-12);
}

#[test]
fn positive_definite_by_a_numeric_hermitian_part() {
    // (m + m^H)/2 = [[1, 0], [0, 2]] for b = -conjugate(a): positive definite for every a
    assert_eq!(ev("is_positive_definite_matrix([[1, a], [-conjugate(a), 2]])"), "true");
    // x^H m x = |x1|^2 + 2 |x2|^2 + 2 i Im(conj(x1) a x2), real part > 0: a sample
    assert_eq!(ev("{ let a = 3 - 5*im; let x = [1 + 2*im, -4 + im]; re(dot(conjugate(x), [[1, a], [-conjugate(a), 2]], x)) > 0 }"), "true");
    assert_eq!(ev("is_positive_definite_matrix([[1, a], [-conjugate(a), -2]])"), "false");
    // a Hermitian part that stays symbolic: no answer
    assert_eq!(ev("is_positive_definite_matrix([[1, a], [b, 2]])"), "is_positive_definite_matrix([[1, a], [b, 2]])");
}

#[test]
fn lu_of_a_symbolic_matrix() {
    let m = "[[a, b, c], [d_u, e_u, f], [g, h, i]]";
    // m[p] == l . u, checked by simplification and at a rational point (substitution back)
    assert_eq!(ev(&format!("{{ let r = lu({m}); simplify(dot(r[1], r[2]) - {m}[r[3]]) }}")), "[[0, 0, 0], [0, 0, 0], [0, 0, 0]]");
    assert_eq!(
        ev(&format!("{{ let r = lu({m}); let s = [a -> 2, b -> -1, c -> 3, d_u -> 5, e_u -> 7, f -> -2, g -> 1, h -> 4, i -> 9]; rewrite(dot(r[1], r[2]), s) == rewrite({m}, s) }}")),
        "true"
    );
    // a zero first pivot swaps rows; l stays unit lower triangular
    assert_eq!(ev("lu([[0, b], [c, d]])"), "[[[1, 0], [0, 1]], [[c, d], [0, b]], [2, 1]]");
    assert_eq!(ev("{ let r = lu([[x, 1, 2], [3, x, 0], [1, 1, x]]); [r[1][1][2], r[1][1][3], r[1][2][3], r[2][2][1], r[2][3][1], r[2][3][2]] }"), "[0, 0, 0, 0, 0, 0]");
    // numbers keep their factorisation
    assert_eq!(ev("lu([[1, 1, 1], [2, 4, 8], [3, 9, 27]])"), "[[[1, 0, 0], [2, 1, 0], [3, 3, 1]], [[1, 1, 1], [0, 2, 6], [0, 0, 6]], [1, 2, 3]]");
}

#[test]
fn pseudo_inverse_of_symbolic_matrices() {
    assert_eq!(
        ev("simplify(pseudo_inverse([[a, b, c]]), element([a, b, c], reals))"),
        "[[a/(a^2 + b^2 + c^2)], [b/(a^2 + b^2 + c^2)], [c/(a^2 + b^2 + c^2)]]"
    );
    // the Penrose conditions at a complex point, against the machine pseudo-inverse of the same matrix
    for m in ["[[a, b], [c, d_u], [1, 0]]", "[[a, b, 1], [c, d_u, im]]"] {
        let s = "[a -> 2 + im, b -> -1, c -> 3*im, d_u -> 1/2]";
        let t = format!(
            "{{ let p = rewrite(pseudo_inverse({m}), {s}); let q = rewrite({m}, {s}); \
             [simplify(dot(q, p, q) - q) == 0*q, simplify(dot(p, q, p) - p) == 0*p, \
             simplify(conjugate(transpose(dot(q, p))) - dot(q, p)) == 0*dot(q, p), \
             max(abs(flatten(n(p) - pseudo_inverse(n(q))))) < 10^-12] }}"
        );
        assert_eq!(ev(&t), "[true, true, true, true]", "{m}");
    }
}

#[test]
fn norm_of_a_symbolic_two_by_two_matrix() {
    // against the machine 2-norm (singular values) at sample points
    for x in ["7/3", "-5", "0", "1/9"] {
        let t = format!("abs(n(rewrite(norm([[1, x], [3, 4]]), x -> {x})) - norm(n([[1, {x}], [3, 4]]))) < 10^-12");
        assert_eq!(ev(&t), "true", "{x}");
    }
    let t = "abs(n(rewrite(norm([[a, b], [c, d]]), [a -> 1 + im, b -> -2, c -> 3*im, d -> 1/2])) - norm(n([[1 + im, -2], [3*im, 1/2]]))) < 10^-12";
    assert_eq!(ev(t), "true");
    // an antisymmetric matrix [[0, s], [-s, 0]] has both singular values |s|
    assert_eq!(ev("abs(n(rewrite(norm([[0, s], [-s, 0]]), s -> 2 - 3*im)) - abs(2 - 3*im)) < 10^-12"), "true");
}

#[test]
fn tolerance_of_least_squares_and_pseudo_inverse() {
    // diag(1, 1/1000): the singular value 1/1000 is below 0.01 times the largest, so it is dropped
    assert_eq!(ev("least_squares([[1.0, 0.0], [0.0, 0.001]], [1.0, 1.0], tolerance: 0.01)"), "[1.0, 0.0]");
    assert_eq!(ev("pseudo_inverse([[1.0, 0.0], [0.0, 0.001]], tolerance: 0.01)"), "[[1.0, 0.0], [0.0, 0.0]]");
    // a small tolerance on a well-conditioned matrix: the exact solution
    assert_eq!(ev("max(abs(least_squares([[2.0, 1.0], [1.0, 3.0]], [3.0, 5.0], tolerance: 10^-12) - [4/5, 7/5])) < 10^-14"), "true");
    // hilbert(20) at 1e-8: within 1e-9 of the truncated SVD solution computed at 60 digits (first entries)
    let t = "{ let m = hilbert_matrix(20); let b = dot(m, constant_array(1, 20)); let x = least_squares(n(m), b, tolerance: 10^(-8)); \
             abs(take(x, 3) - [0.99999979413635493, 1.00000790791573, 0.99993186373582242]) }";
    let got = ev(&format!("max({t}) < 10^-9"));
    assert_eq!(got, "true");
    // exact matrices keep their own pseudo-inverse; a negative tolerance, or one below the rounding level
    // max(r, c) eps, is not taken
    assert_eq!(ev("pseudo_inverse([[1, 2], [3, 4]], tolerance: 0.1)"), "pseudo_inverse([[1, 2], [3, 4]], tolerance: 0.1)");
    assert_eq!(ev("pseudo_inverse([[1.0, 2.0]], tolerance: -1)"), "pseudo_inverse([[1.0, 2.0]], tolerance: -1)");
    assert_eq!(ev("pseudo_inverse([[1.0, 2.0]], tolerance: 0)"), "pseudo_inverse([[1.0, 2.0]], tolerance: 0)");
}

#[test]
fn quantities_in_qr_unit_convert_and_divisible() {
    // q . r gives the matrix back with its unit; q has orthonormal columns
    assert_eq!(
        ev("{ let m = quantity_array([[1, 4], [3, 5]], \"Meters\"); let f = qr(m); [dot(f[1], f[2]) == m, simplify(dot(transpose(f[1]), f[1]))] }"),
        "[true, [[1, 0], [0, 1]]]"
    );
    // 1 yard = 0.9144 m, 1 inch = 0.0254 m (definitions)
    assert_eq!(ev("unit_convert([quantity(2, \"Yards\"), quantity(3, \"Inches\")])"), "[quantity(1143/625, \"Meters\"), quantity(381/5000, \"Meters\")]");
    assert_eq!(ev("unit_convert([quantity(2, \"Yards\"), quantity(3, \"Inches\")], \"Feet\")"), "[quantity(6, \"Feet\"), quantity(1/4, \"Feet\")]");
    // 8 feet = 96 inches: 48 times 2 inches, 96/5 not an integer
    assert_eq!(ev("[divisible(quantity(8, \"Feet\"), quantity(2, \"Inches\")), divisible(quantity(8, \"Feet\"), quantity(5, \"Inches\"))]"), "[true, false]");
}

#[test]
fn asech_of_a_centered_interval_on_the_cut() {
    // the machine values at the ends and the middle (pointwise principal values) lie in the box
    let t = "{ let ci = asech(centered_interval(-3, 1/100)); \
             map(x => { let v = asech(x); abs(re(v) - re(ci[1])) <= re(ci[2]) and abs(im_part(v) - im_part(ci[1])) <= im_part(ci[2]) }, [-3.01, -3.0, -2.99, -3.005]) }";
    assert_eq!(ev(t), "[true, true, true, true]");
    // an interval reaching -1 or above keeps the call
    assert_eq!(ev("asech(centered_interval(-1, 1/100))"), "asech(centered_interval(-1.0, 0.01000000000000001))");
}

#[test]
fn set_precision_keeps_integer_exponents() {
    assert_eq!(ev("set_precision(x^3 + 2*y, 20)"), "2.0000000000000000000*y + x^3");
    assert_eq!(ev("set_precision(f(x^2)^5, 30) === f(x^2)^5"), "true");
    // equations made 32-digit integrate at 32 digits: x'' + x = 0, x(0) = 1, x'(0) = 0 is cos(t)
    let t = "{ let e = set_precision([x''(t) + x(t) == 0, x(0) == 1, x'(0) == 0], 32); \
             let s = n_dsolve(e, x, [t, 2], working_precision: 32); abs(rewrite(x(2), s) - cos(2)) < 10^-20 }";
    assert_eq!(ev(t), "true");
}

#[test]
fn interpolation_over_a_region_and_gaussian_options() {
    let r = "region_difference(cuboid([0, 0, 0], [10, 10, 1]), cylinder([[5, 5, -1], [5, 5, 1]], 2))";
    // the interpolant against the function at points of the region (17 nodes per dimension over the box)
    let t = format!(
        "{{ let f = function_interpolation(sin(x)*y^2 + cos(z)*z, element([x, y, z], {r})); \
         max(map(p => abs(f(p[1], p[2], p[3]) - (sin(p[1])*p[2]^2 + cos(p[3])*p[3])), [[1, 2, 0.5], [9.1, 0.3, 0.9], [2.2, 8.8, 0.1]])) < 0.5 }}"
    );
    assert_eq!(ev(&t), "true");
    assert_eq!(
        ev("gaussian_matrix([[2, 4]], [1, 3], method: gaussian, standardized: true) == gaussian_matrix([[2, 4]], [1, 3])"),
        "true"
    );
    // the kernel of odd orders sums to 0
    assert_eq!(ev("abs(total(gaussian_matrix([[2, 4]], [1, 3], method: gaussian, standardized: true), 2)) < 10^-14"), "true");
}
