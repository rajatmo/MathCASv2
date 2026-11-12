//! Round 36: fixed wrong answers in linear algebra, numerics, the core language, lists, statistics,
//! graphics and data, one test per cause. Every value is checked by a route other than the one that computes
//! it (a closed form, a brute-force sample, the defining property, a second algorithm).
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate on a detached thread with a big stack (one worker thread, fresh session); `TIMEOUT` after 120 s.
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
        .expect("spawn");
    rx.recv_timeout(std::time::Duration::from_secs(120)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

#[test]
fn centered_interval_powers_enclose_the_range() {
    // [1.99, 2.01]^[2.99, 3.01]: x^y is monotone in each argument, the range is [1.99^2.99, 2.01^3.01]; the first
    // order estimate gave the upper end 8.17704 below 2.01^3.01 = 8.17749...
    let out = ev("r = centered_interval(2, 1/100)^centered_interval(3, 1/100); lo = r[1] - r[2]; hi = r[1] + r[2]; \
                  [lo <= n(199/100, 30)^(299/100), hi >= n(201/100, 30)^(301/100), hi - n(201/100, 30)^(301/100) < 1e-12, n(199/100, 30)^(299/100) - lo < 1e-12]");
    assert_eq!(out, "[true, true, true, true]");
    // an exact base, an interval exponent: the ends pi^2 and pi^3, the machine end rounded outwards
    assert_eq!(ev("r = pi^interval([2, 3.]); [r[1][1], r[1][2] >= n(pi, 30)^3, r[1][2] - n(pi, 30)^3 < 1e-13]"), "[pi^2, true, true]");
}

#[test]
fn complex_centered_intervals_are_boxes() {
    // a complex radius rx + i ry is the box of the two parts; sums add the boxes
    let out = ev("r = centered_interval(0, 1 + im) + centered_interval(1, 1 + im); [abs(re(r[1]) - 1) < 1e-14, abs(im_part(r[1])) < 1e-14, abs(re(r[2]) - 2) < 1e-14, abs(im_part(r[2]) - 2) < 1e-14]");
    assert_eq!(out, "[true, true, true, true]");
    // an interval mixed in: [1, 2] is 3/2 +- 1/2
    let out = ev("r = interval([1, 2]) + centered_interval(2 + 3 * im, 1 + im); [abs(r[1] - (7/2 + 3 im)) < 1e-14, abs(r[2] - (3/2 + im)) < 1e-14]");
    assert_eq!(out, "[true, true]");
    // [-2.1, -1.9] (1.9..2.1 + i 2.9..3.1): the real part spans [-4.41, -3.61], the imaginary part [-6.51, -5.51]
    let out = ev("r = centered_interval(-2, 1 / 10) * centered_interval(2 + 3 * im, (1 + im) / 10); \
                  [abs(re(r[1] - r[2]) + 4.41) < 1e-13, abs(re(r[1] + r[2]) + 3.61) < 1e-13, abs(im_part(r[1] - r[2]) + 6.51) < 1e-13, abs(im_part(r[1] + r[2]) + 5.51) < 1e-13]");
    assert_eq!(out, "[true, true, true, true]");
    // [2, 3] [3.9, 4.1] = [7.8, 12.3]
    assert_eq!(ev("r = interval([2, 3]) * centered_interval(4, 1 / 10); [abs(r[1] - r[2] - 7.8) < 1e-13, abs(r[1] + r[2] - 12.3) < 1e-13]"), "[true, true]");
    // a complex power: every sampled value lies in the box
    let out = ev("r = centered_interval(2 + 3 * im, (1 + im) / 100)^centered_interval(3, (1 + im) / 100); c = r[1]; w = r[2]; \
                  v = flatten(table(n((2 + a/100 + (3 + b/100) im)^(3 + c1/100 + d/100 im)), [a, -1, 1, 1/2], [b, -1, 1, 1/2], [c1, -1, 1, 1/2], [d, -1, 1, 1/2])); \
                  [re(c) - re(w) <= min(re(v)), max(re(v)) <= re(c) + re(w), im_part(c) - im_part(w) <= min(im_part(v)), max(im_part(v)) <= im_part(c) + im_part(w), re(w) < 2, im_part(w) < 2]");
    assert_eq!(out, "[true, true, true, true, true, true]");
    // the mantissa form of a complex ball reads as the box
    let out = ev("r = centered_interval([[[1, 0, 536870912, -28], [0, 0, 536870912, -28]], 30]); [re(r[1]), im_part(r[1]), abs(re(r[2]) - 2) < 1e-14, abs(im_part(r[2]) - 2) < 1e-14]");
    assert_eq!(out, "[1.0, 0.0, true, true]");
}

#[test]
fn partition_and_block_map_of_deeper_arrays() {
    // element [b1, b2, b3, e1, e2, e3] is a[(b1 - 1) 2 + e1, b2 + e2 - 1, (b3 - 1) 2 + e3]
    let out = ev("a = array_reshape(range(48), [6, 2, 4]); p = partition(a, [2, 1, 2]); \
                  [dimensions(p), every(flatten(table(p[b1, b2, b3, e1, e2, e3] == a[(b1 - 1)*2 + e1, b2 + e2 - 1, (b3 - 1)*2 + e3], b1, 3, b2, 2, b3, 2, e1, 2, e2, 1, e3, 2)), t => t)]");
    assert_eq!(out, "[[3, 2, 2, 2, 1, 2], true]");
    assert_eq!(
        ev("block_map(f, array_reshape(range(48), [6, 2, 4]), [2, 1, 2])[1]"),
        "[[f([[[1, 2]], [[9, 10]]]), f([[[3, 4]], [[11, 12]]])], [f([[[5, 6]], [[13, 14]]]), f([[[7, 8]], [[15, 16]]])]]"
    );
}

#[test]
fn center_array_of_ragged_rows_and_quantity_bins() {
    assert_eq!(ev("center_array([[1, 2], [3]], 2)"), "[[1, 2], [3, 0]]");
    // the bins compare magnitudes in one unit: -1.541 and -0.849 g lie in [-2, 0) g, 0.226 and 1.4 g in [0, 2) g
    let out = ev("data = quantity([-1.541, -0.849, 0.226, 1.4], \"Grams\"); bin_lists(data, [[quantity(-2, \"Grams\"), quantity(0, \"Grams\"), quantity(2000, \"Milligrams\")]]) == [take(data, 2), drop(data, 2)]");
    assert_eq!(out, "true");
}

#[test]
fn circular_functions_of_angles_and_physical_constant_units() {
    // 60 degrees is pi/3
    assert_eq!(ev("[cos(quantity(60, \"AngularDegrees\")), sin(quantity(90, \"AngularDegrees\"))]"), "[1/2, 1]");
    // 1 m/s = 3125/1397 mph; the polar vector of length 2 mph at 45.8 degrees added
    let out = ev("v = angle_vector(quantity([1, 2], \"Meters\" / \"Seconds\"), [quantity(2, \"Miles\" / \"Hours\"), quantity(45.8, \"AngularDegrees\")]); \
                  abs(quantity_magnitude(v[1], \"Miles\"/\"Hours\") - (3125/1397 + 2*cos(45.8*pi/180))) < 1e-12");
    assert_eq!(out, "true");
    // G = 6.6743e-11 m^3/(kg s^2) (CODATA 2018)
    assert_eq!(ev("unit_convert(quantity(1., \"GravitationalConstant\"))"), "quantity(6.6743e-11, \"Kilograms\"^-1*\"Meters\"^3*\"Seconds\"^-2)");
}

#[test]
fn curl_of_rank_two_arrays() {
    // (1/2) eps_ijk d_i A_jk by hand: d_x(A23 - A32) + d_y(A31 - A13)
    let out = ev("a = [[x * y, x * y^2, x * y^3], [x^2 * y, x^2 * y^2, x^2 * y^3], [x^3 * y, x^3 * y^2, x^3 * y^3]]; \
                  expand(curl(a, [x, y, z]) - (d(a[2, 3] - a[3, 2], x) + d(a[3, 1] - a[1, 3], y) + d(a[1, 2] - a[2, 1], z))/2)");
    assert_eq!(out, "0");
    // the curl of a gradient (a symmetric Jacobian) vanishes; the vector case is unchanged
    assert_eq!(ev("curl(grad([f(x, y, z), g(x, y, z), h(x, y, z)], [x, y, z]), [x, y, z])"), "0");
    assert_eq!(ev("curl([y, -x, z], [x, y, z])"), "[0, 0, -2]");
}

#[test]
fn distance_matrix_of_associations_aligns_keys() {
    // the second row lists its keys in another order: |(32, 160) - (41, 183)|^2 = 81 + 529
    let out = ev("data = [assoc(\"age\" -> 32, \"height\" -> 160), assoc(\"height\" -> 183, \"age\" -> 41)]; distance_matrix(data)[1, 2]");
    assert_eq!(out, "610^(1/2)");
}

#[test]
fn linear_algebra_of_quantity_columns() {
    // m = [[1 m, 2 s], [3 m, 4 s]] is M diag(m, s); the solution of m x = b has entries b/m and b/s
    let out = ev("m = quantity_array([[1, 2], [3, 4]], [\"Meters\", \"Seconds\"]); b = quantity_array([[5, 6], [6, 7]], \"Meters\"); x = least_squares(m, b); [x, dot(m, x) == b]");
    assert_eq!(out, "[[[-4, -5], [quantity(9/2, \"Meters\"/\"Seconds\"), quantity(11/2, \"Meters\"/\"Seconds\")]], true]");
    // a singular one: rank 1, the null vector solves m x = 0
    let out = ev("m = quantity_array([[1, 2], [2, 4]], [\"Meters\", \"Seconds\"]); v = null_space(m); [row_reduce(m), v, dot(m, v[1]), rank(m)]");
    assert_eq!(out, "[[[1, quantity(2, \"Seconds\"/\"Meters\")], [0, 0]], [[quantity(-2, \"Seconds\"/\"Meters\"), 1]], [quantity(0, \"Seconds\"), quantity(0, \"Seconds\")], 1]");
    // one unit throughout: the minimal polynomial of u M is u^2 p(x/u), p(y) = y^2 - 5 y - 2
    let out = ev("p = matrix_minimal_polynomial(quantity_array([[1, 2], [3, 4]], [\"Meters\", \"Meters\"]), x); expand(rewrite(p, x -> quantity(y, \"Meters\")) - quantity(y^2 - 5*y - 2, \"Meters\"^2))");
    assert_eq!(out, "quantity(0, \"Meters\"^2)");
}

#[test]
fn symmetric_eigenvalues_at_the_input_precision() {
    // the 100 x 100 Hilbert matrix: lambda_min = 1.0e-150 (the machine solver gave 1e-19)
    assert_eq!(ev("real_exponent(eigenvalues(hilbert_matrix(100, working_precision: 200), -1))"), "[-150.23809463856605]");
    // a 30-digit matrix: m v = lambda v to 30 digits, the eigenvalues agree with the exact ones
    let out = ev("m = table(n(1 / (i + j + 1), 30), i, 4, j, 4); es = eigensystem(m); ex = n(eigenvalues(table(1 / (i + j + 1), i, 4, j, 4)), 40); \
                  [max(abs(flatten(n(dot(m, transpose(es[2])) - transpose(es[2]) * table(es[1], 4))))) < 1e-15, max(abs(n(es[1] - ex))) < 1e-15, precision(es[1][1]) > 25, abs(es[1][4] - ex[4]) < 1e-33]");
    assert_eq!(out, "[true, true, true, true]");
}

#[test]
fn cube_root_of_closed_reals_and_its_derivatives() {
    // the real branch at -1 is -(1 - u)^(1/3), u = x + 1: the coefficients -binomial(1/3, k) (-1)^k = -1, 1/3, 1/9,
    // 5/81, 10/243, 22/729 (by hand: binomial(1/3, 5) = 22/729)
    let out = ev("s = series(cube_root(x), x, -1, 5); expand(normal(s) - (-1 + (x + 1)/3 + (x + 1)^2/9 + 5*(x + 1)^3/81 + 10*(x + 1)^4/243 + 22*(x + 1)^5/729))");
    assert_eq!(out, "0");
    assert_eq!(ev("d(cube_root(x), x) === 1/(3*cube_root(x)^2)"), "true");
    // f(M)^3 == M for the real cube root of a matrix with real eigenvalues 1/2 +- sqrt(5/3)
    let out = ev("m = [[1 / 2, -1], [-5 / 3, 1 / 2]]; f = matrix_function(cube_root, m); max(abs(flatten(n(dot(f, dot(f, f)) - m)))) < 1e-12");
    assert_eq!(out, "true");
}

#[test]
fn matrix_functions_of_piecewise_functions() {
    // Sylvester: f(M) = (f(l1) (M - l2) - f(l2) (M - l1))/(l1 - l2), the eigenvalues 1/2 +- i sqrt(15)/3, parts rounded
    let check = |f: &str| {
        format!(
            "m = [[1 / 2, -1], [5 / 3, 1 / 2]]; l1 = 1/2 + im*sqrt(15)/3; l2 = 1/2 - im*sqrt(15)/3; \
             s = ({f}(l1)*(m - l2*identity_matrix(2)) - {f}(l2)*(m - l1*identity_matrix(2)))/(l1 - l2); \
             max(abs(flatten(n(matrix_function({f}, m) - s)))) < 1e-12"
        )
    };
    assert_eq!(ev(&check("fractional_part")), "true");
    assert_eq!(ev(&check("round")), "true");
    assert_eq!(ev("round(1/2 + im*sqrt(15)/3) == im"), "true");
    // a Jordan block: f(1/2) and f'(1/2) of the triangle, f'(x) = -(1 - 1/2) for 0 < x < 1
    assert_eq!(ev("full_simplify(matrix_function(&(unit_triangle(&1, 1 / 2)), [[1 / 2, -1], [0, 1 / 2]]))"), "[[1/4, 1/2], [0, 1/4]]");
}

#[test]
fn verified_julia_counts_near_the_boundary() {
    // |z0|^(2^k) > 4 first at k = 2055 for z0 = i + 10^-309 (log |z0| = 10^-618/2): machine arithmetic lost it
    assert_eq!(ev("julia_set_iteration_count(0, im + 10^(-309), max_iterations: 3000)"), "2055");
    assert_eq!(ev("[julia_set_iteration_count(0, im + 10^(-309)), julia_set_iteration_count(0, im + 10^(-309), max_iterations: 2000, working_precision: 1000)]"), "[1001, 2001]");
    // ordinary points agree with the machine counts
    assert_eq!(ev("julia_set_iteration_count(-1, [3/10, 0.3])"), "[5, 5]");
}

#[test]
fn digits_after_an_indeterminate_one_are_unknown() {
    assert_eq!(ev("r = from_digits([[6, 7, 0, 0, 0, indeterminate], 1]); [abs(r - 67/10) < 1e-6, precision(r) >= 5]"), "[true, true]");
}

#[test]
fn linear_optimization_with_machine_bounds() {
    assert_eq!(ev("linear_optimization(x, [x >= 2.], x)"), "[x -> 2.0]");
    assert_eq!(ev("linear_optimization(x, [x >= 2], x)"), "[x -> 2]");
}

#[test]
fn abstract_vectors_orthogonalize_and_regions() {
    // Gram-Schmidt with dot left unevaluated
    let out = ev("o = orthogonalize([b1, b2]); [o[1] === b1/sqrt(dot(b1, b1)), o[2] === (b2 - dot(o[1], b2)*o[1])/sqrt(dot(b2 - dot(o[1], b2)*o[1], b2 - dot(o[1], b2)*o[1]))]");
    assert_eq!(out, "[true, true]");
    // the lens of two unit disks at distance 1: 2 pi/3 - sqrt(3)/2 (the 256-gons gave 1.228159)
    assert_eq!(ev("reg = region_intersection(disk([0, 0], 1), disk([0, 1], 1)); expand(region_moment(reg, [0, 0]) - (4*pi - 3*sqrt(3))/6)"), "0");
    // the astroid traced twice over [0, 4 pi] has the boundary length 6 (one period counted)
    assert_eq!(ev("p = perimeter(parametric_region([r * cos(t)^3, r * sin(t)^3], [[r, 0, 1], [t, 0, 4 * pi]])); abs(n(p) - 6) < 1e-10"), "true");
    // a cell-size bound that is not honoured is a failure, not a coarse mesh
    assert_eq!(ev("discretize_region(disk(), max_cell_measure: 10^(-5))"), "failure(unsupported_option, max_cell_measure)");
}

#[test]
fn complex_plane_rotations_are_unitary() {
    // the plane of u, v with the Hermitian product: unitary, u/|u| to v/|v|
    let out = ev("u = [1 + im, 1 - im]; v = [-2 * im, 1 - 3 * im]; r = rotation_matrix([u, v]); [simplify(dot(r, conjugate_transpose(r))), max(abs(n(dot(r, u)/norm(u) - v/norm(v)))) < 1e-14]");
    assert_eq!(out, "[[[1, 0], [0, 1]], true]");
    // the rotation by pi/3 in the plane of [1, 0, 1], [1 + i, 0, 1] then in the swapped plane: entry [1, 1] is
    // (9 + i sqrt(3))/12, by the definition I + (cos t - 1)(a a^H + b b^H) + sin t (b a^H - a b^H) worked by hand
    let out = ev("p = simplify(dot(rotation_matrix(pi / 3, [[1, 0, 1], [1 + im, 0, 1]]), rotation_matrix(pi / 3, [[1 + im, 0, 1], [1, 0, 1]]))); [abs(n(p[1, 1] - (9 + im*sqrt(3))/12)) < 1e-14, p[2]]");
    assert_eq!(out, "[true, [0, 1, 0]]");
    // real vectors are unchanged
    assert_eq!(ev("rotation_matrix([[1, 0], [0, 1]])"), "[[0, -1], [1, 0]]");
}

#[test]
fn replace_part_reaches_heads_through_position_zero() {
    assert_eq!(ev("replace_part(f(g(x), y), [___, 0] -> list)"), "[[x], y]");
    // a pattern without 0 leaves the heads alone
    assert_eq!(ev("replace_part([a, b, c], [_] -> x)"), "[x, x, x]");
}

#[test]
fn dataset_queries_with_a_function_first() {
    // ds[f, c] is f of the column c
    let ds = "ds = dataset(assoc(\"a\" -> [1, 2, 3, 4, 5, 6], \"b\" -> [\"x\", \"y\", \"z\", \"x\", \"y\", \"z\"]));";
    assert_eq!(ev(&format!("{ds} [ds[total, \"a\"], ds[total, \"a\"] == total(ds[all, \"a\"]), ds[string_join, \"b\"]]")), "[21, true, \"xyzxyz\"]");
    assert_eq!(ev(&format!("{ds} total(ds[select(&(&1[\"b\"] == \"z\")), \"a\"])")), "9");
}

#[test]
fn interpolation_of_vectors_and_of_two_variables() {
    // four points: the not-a-knot spline is the cubic through them, so each component is the Lagrange cubic
    let out = ev("data = [[[0.], [1., 2.]], [[1.2], [3., 4.]], [[2.1], [5., 4.]], [[3.], [0., 4.]]]; fun = interpolation(data); \
                  lag = (ys, t) => total(table(ys[i]*product(if j == i then 1 else (t - [0., 1.2, 2.1, 3.][j])/([0., 1.2, 2.1, 3.][i] - [0., 1.2, 2.1, 3.][j]), [j, 4]), [i, 4])); \
                  [max(abs(fun(1.5) - [lag([1., 3., 5., 0.], 1.5), lag([2., 4., 4., 4.], 1.5)])) < 1e-12, fun(1.2)]");
    assert_eq!(out, "[true, [3.0, 4.0]]");
    // a grid: the nodes are reproduced, a bilinear function exactly (the tensor product of splines)
    let out = ev("data = flatten(table([[x, y], [2*x + 3*y + x*y, x - y]], x, [0, 1, 2, 3], y, [0, 1, 2]), 1); f = interpolation(data); \
                  [max(abs(f(1.5, 0.5) - [2*1.5 + 3*0.5 + 1.5*0.5, 1.0])) < 1e-12, f(2, 1)]");
    assert_eq!(out, "[true, [9.0, 1.0]]");
}

#[test]
fn unsupported_options_decline() {
    // find_root has no step monitor: a failure that says so, not a root
    assert_eq!(ev("find_root(cos(x) == x, [x, 1], step_monitor: sow(x))"), "failure(unknown_option, step_monitor)");
}
