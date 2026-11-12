//! Round 29: the vector, matrix and number functions of `lib/lists.mc`. Expected values are
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
fn vectors() {
    assert_eq!(ev("unit_vector(4, 2)"), "[0, 1, 0, 0]");
    assert_eq!(ev("unit_vector(2)"), "[0, 1]");
    assert_eq!(ev("angle_vector([2, pi/3])"), "[1, 3^(1/2)]");
    assert_eq!(ev("angle_vector([1, 1], pi/2)"), "[1, 2]");
    assert_eq!(ev("vector_angle([1, 0], [1, 1])"), "(1/4)*pi");
    assert_eq!(ev("manhattan_distance([1, 2], [4, 6])"), "7");
    assert_eq!(ev("from_polar_coordinates([[1, 0], [2, pi/2]])"), "[[1, 0], [0, 2]]");
}

#[test]
fn matrix_predicates_and_numbers() {
    assert_eq!(ev("symmetric_matrix_q([[1, 2], [2, 3]])"), "true");
    assert_eq!(ev("symmetric_matrix_q([[1, 2], [3, 3]])"), "false");
    assert_eq!(ev("positive_definite_matrix_q([[2, 1], [1, 2]])"), "true");
    assert_eq!(ev("positive_definite_matrix_q([[1, 2], [2, 1]])"), "false");
    assert_eq!(ev("positive_definite_matrix_q([[2, 1 + im], [1 - im, 3]])"), "true");
    // a non-Hermitian matrix is tested through its Hermitian part
    assert_eq!(ev("positive_definite_matrix_q([[2.3, -1.2], [0.6, 3.7]])"), "true");
    assert_eq!(ev("coefficient_arrays([a + x - y - z == 0, b + x + z + 2*y == 0], [x, y, z])"), "[[a, b], [[1, -1, -1], [1, 2, 1]]]");
    assert_eq!(ev("permanent([[1, 2], [3, 4]])"), "10");
    assert_eq!(ev("permanent([[1, 1, 1], [1, 1, 1], [1, 1, 1]])"), "6");
    assert_eq!(ev("permanent([[1, 2, 3], [4, 5, 6]])"), "58");
}

#[test]
fn matrix_builders() {
    assert_eq!(ev("shearing_matrix(t, [1, 0], [0, 1])"), "[[1, tan(t)], [0, 1]]");
    assert_eq!(ev("scaling_matrix([a, b, c])"), "[[a, 0, 0], [0, b, 0], [0, 0, c]]");
    assert_eq!(ev("distance_matrix([1, 4, 9])"), "[[0, 3, 8], [3, 0, 5], [8, 5, 0]]");
    assert_eq!(ev("least_squares([[1, 0], [1, 1], [1, 2]], [1, 2, 2])"), "[7/6, 1/2]");
    // the defining equations of the Drazin inverse: m d = d m, d m d = d, m^(k+1) d = m^k
    assert_eq!(ev("let m = [[1, 1, 0], [0, 1, 1], [0, 0, 0]]; let d = drazin_inverse(m); [dot(m, d) == dot(d, m), dot(d, dot(m, d)) == d]"), "[true, true]");
    assert_eq!(ev("disk_matrix(2)"), "[[0, 0, 1, 0, 0], [0, 1, 1, 1, 0], [1, 1, 1, 1, 1], [0, 1, 1, 1, 0], [0, 0, 1, 0, 0]]");
    assert_eq!(ev("cross_matrix(1)"), "[[0, 1, 0], [1, 1, 1], [0, 1, 0]]");
    assert_eq!(ev("box_matrix(1, [5, 5])"), "[[0, 0, 0, 0, 0], [0, 1, 1, 1, 0], [0, 1, 1, 1, 0], [0, 1, 1, 1, 0], [0, 0, 0, 0, 0]]");
    assert_eq!(ev("diamond_matrix(1)"), "[[0, 1, 0], [1, 1, 1], [0, 1, 0]]");
    assert_eq!(ev("total(flatten(gaussian_matrix(2))) > 0.999999"), "true");
}

#[test]
fn number_functions() {
    assert_eq!(ev("mixed_fraction_parts([14/3, -5/3])"), "[[4, 2/3], [-1, -2/3]]");
    assert_eq!(ev("mixed_fraction_parts(17/(1 + 3*im))"), "[complex(1, -5), complex(7/10, -1/10)]");
    assert_eq!(ev("map(perfect_number, [1, 2, 3, 4])"), "[6, 28, 496, 8128]");
    assert_eq!(ev("map(rudin_shapiro, range(0, 9))"), "[1, 1, 1, -1, 1, 1, -1, 1, 1, 1]");
    assert_eq!(ev("quadratic_irrational_q([sqrt(2) + 1, sqrt(2) + sqrt(3), 3/2, pi])"), "[true, false, false, false]");
    assert_eq!(ev("discrete_indicator(a, a, [a, b, c])"), "1");
    assert_eq!(ev("discrete_indicator(x, a, [a, b, c])"), "discrete_indicator(x, a, [a, b, c])");
    assert_eq!(ev("cumulant([1, 2, 3, 4, 7], 2)"), "106/25");
    assert_eq!(ev("machine_epsilon"), "2.220446049250313e-16");
    assert_eq!(ev("accuracy(set_accuracy(1/3, 10)) >= 10"), "true");
    assert_eq!(ev("element([17, -17, 1, 9, 7/2], primes)"), "[true, false, false, false, false]");
}

#[test]
fn convolution_and_arrays() {
    assert_eq!(ev("list_convolve([x, y], [a, b, c])"), "[a*y + b*x, b*y + c*x]");
    assert_eq!(ev("list_convolve([x, y], [a, b, c], 1)"), "[a*x + c*y, a*y + b*x, b*y + c*x]");
    assert_eq!(ev("list_convolve([[1, 1], [1, 1]], [[1, 2, 3], [4, 5, 6], [7, 8, 9]])"), "[[12, 16], [24, 28]]");
    assert_eq!(ev("array_rules([[0, 1], [2, 0]])"), "[[1, 2] -> 1, [2, 1] -> 2, [_, _] -> 0]");
    assert_eq!(ev("sparse_array(array_rules([[0, 1], [2, 0]]))"), "[[0, 1], [2, 0]]");
    assert_eq!(ev("coefficient_arrays([x + 2*y - 1, 3*x*y], [x, y])"), "[[-1, 0], [[1, 2], [0, 0]], [[[0, 0], [0, 0]], [[0, 3], [0, 0]]]]");
}

#[test]
fn polynomials_and_decompositions() {
    assert_eq!(ev("matrix_minimal_polynomial([[2, 0, 0], [0, 3, 0], [0, 0, 2]], x)"), "6 + -5*x + x^2");
    assert_eq!(ev("matrix_minimal_polynomial([[1, 1], [0, 1]], x)"), "1 + -2*x + x^2");
    assert_eq!(ev("pfaffian_det([[0, 3, 2, 1], [-3, 0, 5, 6], [-2, -5, 0, 4], [-1, -6, -4, 0]])"), "5");
    assert_eq!(ev("pfaffian_det([[0, 1, 2], [-1, 0, 3], [-2, -3, 0]])"), "0");
    // the Pfaffian squared is the determinant
    assert_eq!(ev("let m = [[0, 3, 2, 1], [-3, 0, 5, 6], [-2, -5, 0, 4], [-1, -6, -4, 0]]; pfaffian_det(m)^2 == det(m)"), "true");
    // principal axes: the components are uncorrelated and carry the total variance
    assert_eq!(ev("let c = principal_components([[1., 2.], [2., 3.], [4., 10.]]); abs(covariance(c)[1][2]) < 1.0e-12"), "true");
    assert_eq!(ev("let r = karhunen_loeve_decomposition([[1., 2., 4.], [2., 3., 10.]]); abs(r[2][1][1]^2 + r[2][1][2]^2 - 1) < 1.0e-12"), "true");
}

#[test]
fn intervals_and_knapsack() {
    assert_eq!(ev("interval_member_q(interval([2, 5]), pi)"), "true");
    assert_eq!(ev("interval_member_q(interval([2, 5]), interval([4, 6]))"), "false");
    assert_eq!(ev("interval_member_q(centered_interval(2 + 3*im, 1 + im), e + pi*im)"), "true");
    assert_eq!(ev("interval_union(interval([1, 3]), interval([2, 4]))"), "interval([1, 4])");
    assert_eq!(ev("interval_union(centered_interval(-1, 1/2), centered_interval(1, 1/2))"), "interval([-3/2, -1/2], [1/2, 3/2])");
    assert_eq!(ev("knapsack_solve([[3, 4], [2, 5]], 10)"), "[2, 0]");
    assert_eq!(ev("knapsack_solve([[3, 2, 7], [2, 1, 5]], 10)"), "[3, 4]");
    assert_eq!(ev("knapsack_solve(assoc(\"apples\" -> [8, 3, 3], \"oranges\" -> [9, 7, 5]), 25)"), "assoc(\"apples\" -> 3, \"oranges\" -> 2)");
    assert_eq!(ev("knapsack_solve([4, 2, 3], 8)"), "[0, 4, 0]");
}

#[test]
fn band_positions_in_sparse_arrays() {
    assert_eq!(ev("sparse_array([band([1, 1]) -> x, band([2, 1]) -> y], [3, 3])"), "[[x, 0, 0], [y, x, 0], [0, y, x]]");
    assert_eq!(ev("sparse_array(band([1, 1]) -> [x, y], [3, 3])"), "[[x, 0, 0], [0, y, 0], [0, 0, 0]]");
    assert_eq!(ev("sparse_array(band([1, 1], [5, 5], 2) -> x, [5, 5])"), "[[x, 0, 0, 0, 0], [0, 0, 0, 0, 0], [0, 0, x, 0, 0], [0, 0, 0, 0, 0], [0, 0, 0, 0, x]]");
    assert_eq!(ev("sparse_array(band([1, 3], automatic, [1, -1]) -> 1, [3, 3])"), "[[0, 0, 1], [0, 1, 0], [1, 0, 0]]");
    assert_eq!(ev("sparse_array(band([1, 1]) -> [[[a, b], [c, d]], [[x]]], [4, 4])"), "[[a, b, 0, 0], [c, d, 0, 0], [0, 0, x, 0], [0, 0, 0, 0]]");
    assert_eq!(ev("sparse_array(band([1, 1]) -> [a, b, c])"), "[[a, 0, 0], [0, b, 0], [0, 0, c]]");
}

#[test]
fn convex_optimization_family() {
    assert_eq!(ev("quadratic_optimization(x^2 + y^2, [x + y == 2, 1 <= y, y <= 2], [x, y])"), "[x -> 1, y -> 1]");
    assert_eq!(ev("quadratic_optimization(x^2 + y^2, x + y >= 1, [x, y], \"PrimalMinimumValue\")"), "1/2");
    assert_eq!(ev("quadratic_optimization([[[2, 0], [0, 2]], [0, 0]], [[[1, 1]], [-1]])"), "[1/2, 1/2]");
    assert_eq!(ev("conic_optimization(x + 2*y, [x^2 + 2*y^2 <= 3, x + y == 2, x >= 1], [x, y])"), "[x -> 5/3, y -> 1/3]");
    assert_eq!(ev("second_order_cone_optimization(x + y, [norm([x, y]) <= 1, y >= 0], [x, y], \"PrimalMinimumValue\")"), "-1");
    assert_eq!(ev("semidefinite_optimization(x + y, vector_greater_equal([[[x, 1], [1, y]], 0], [\"SemidefiniteCone\", 2]), [x, y])"), "[x -> 1, y -> 1]");
    assert_eq!(ev("robust_convex_optimization(x + y, for_all(t, -1 <= t and t <= 1, [x + 2*y >= 3 + t, x >= -1]), [x, y])"), "[x -> -1, y -> 5/2]");
    assert_eq!(ev("parametric_convex_optimization((x - a)^2 + (y - b)^2, [x >= 0, y >= 0, x + 2*y <= 1], [x, y], [a, b], \"PrimalMinimumValue\")(0, 0)"), "0");
}

#[test]
fn distributions() {
    assert_eq!(ev("mean(empirical_distribution([1, 2, 2, 5]))"), "5/2");
    assert_eq!(ev("variance(empirical_distribution([1, 2, 2, 5]))"), "9/4");
    assert_eq!(ev("[pdf(empirical_distribution([1, 2, 2, 5]), 2), cdf(empirical_distribution([1, 2, 2, 5]), 3)]"), "[1/2, 3/4]");
    assert_eq!(ev("quantile(empirical_distribution([1, 2, 2, 5]), 3/4)"), "2");
    assert_eq!(ev("mean(empirical_distribution([1, 3] -> [10, 20]))"), "35/2");
    assert_eq!(ev("mean(multivariate_poisson_distribution(1, [2, 3]))"), "[3, 4]");
    assert_eq!(ev("covariance(multivariate_poisson_distribution(1, [2, 3]))"), "[[3, 1], [1, 4]]");
    // the density sums to 1 over the integer grid (checked on a truncated grid)
    assert_eq!(ev("n(sum(map(i => sum(map(j => pdf(multivariate_poisson_distribution(1, [1, 1]), [i, j]), range(0, 30))), range(0, 30)))) > 0.999999"), "true");
    assert_eq!(ev("mean(wishart_matrix_distribution(3, diagonal_matrix([2, 1, 3])))"), "[[6, 0, 0], [0, 3, 0], [0, 0, 9]]");
    assert_eq!(ev("abs(n(pdf(wishart_matrix_distribution(3, [[1, 0], [0, 1]]), [[1, 0], [0, 1]])) - n(1/(4*e*pi))) < 1.0e-12"), "true");
    assert_eq!(ev("let w = random_variate(wishart_matrix_distribution(10, [[1, 1/3], [1/3, 1]])); [symmetric_matrix_q(w), positive_definite_matrix_q(w)]"), "[true, true]");
}

#[test]
fn series_constants() {
    // Khinchin's constant 2.68545200106530644530971483548179569..., Champernowne's number 0.12345678910111213...
    assert_eq!(ev("abs(n(khinchin, 30) - n(26854520010653064453097148354818/10^31, 40)) < 10^(-29)"), "true");
    assert_eq!(ev("abs(n(khinchin) - 2.6854520010653064) < 1.0e-15"), "true");
    assert_eq!(ev("abs(n(champernowne_number(), 25) - n(123456789101112131415161718192/10^30, 40)) < 10^(-26)"), "true");
    // base 2: 0.1 10 11 100 101 ... in binary
    assert_eq!(ev("abs(n(champernowne_number(2), 20) - n(from_digits([1, 1, 0, 1, 1, 1, 0, 0, 1, 0, 1, 1, 1, 0, 1, 1, 1], 2)/2^17, 20)) < 2^(-17)"), "true");
}

#[test]
fn sparse_array_pattern_rules_are_not_defaults() {
    // `[i_, i_]` has named blanks: a rule for the diagonal, not the rule for all other positions
    assert_eq!(ev("sparse_array([[i_, i_] -> 2, [i_, j_] when abs(i - j) == 1 -> -1], [3, 3])"), "[[2, -1, 0], [-1, 2, -1], [0, -1, 2]]");
}
