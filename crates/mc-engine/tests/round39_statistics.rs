//! Round 39: cumulants and moments of a symbolic order, covariance entries of multivariate distributions, dimensions
//! of regions given by symbols and curves, lengths and bounds of Bezier curves, the Karhunen-Loeve decomposition of
//! images and its inverse.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn cumulants_and_moments_of_a_symbolic_order() {
    assert_eq!(ev("cumulant(poisson_distribution(μ), r)"), "piecewise([[μ, r >= 1]], 0)");
    assert_eq!(ev("moment(poisson_distribution(μ), r)"), "piecewise([[bell_b(r, μ), r >= 0]], 0)");
    assert_eq!(ev("cumulant(normal_distribution(μ, σ), r)"), "piecewise([[μ, r == 1], [σ^2, r == 2]], 0)");
    assert_eq!(ev("central_moment(normal_distribution(μ, σ), r)"), "piecewise([[factorial2(-1 + r)*σ^r, mod(r, 2) == 0 and r >= 0]], 0)");
    assert_eq!(ev("cumulant(chi_square_distribution(ν), r)"), "piecewise([[ν*factorial(-1 + r)*2^(-1 + r), r >= 1]], 0)");
    // at numbers the forms agree with the moments of the distributions computed for that order
    assert_eq!(ev("expand(rewrite(moment(poisson_distribution(m), r), r -> 4) - moment(poisson_distribution(m), 4))"), "0");
    assert_eq!(ev("expand(rewrite(central_moment(normal_distribution(m, s), r), r -> 6) - central_moment(normal_distribution(m, s), 6))"), "0");
    assert_eq!(ev("rewrite(central_moment(normal_distribution(m, s), r), r -> 5)"), "0");
    // the cumulants as derivatives of the log of the moment generating function (1 - 2 t)^(-k/2) at 0
    assert_eq!(ev("simplify(rewrite(cumulant(chi_square_distribution(k), r), r -> 3) - rewrite(d(log((1 - 2*t)^(-k/2)), [t, 3]), t -> 0))"), "0");
    assert_eq!(ev("simplify(rewrite(cumulant(chi_square_distribution(k), r), r -> 4) - rewrite(d(log((1 - 2*t)^(-k/2)), [t, 4]), t -> 0))"), "0");
    assert_eq!(ev("simplify(rewrite(cumulant(poisson_distribution(m), r), r -> 5) - rewrite(d(m*(exp(t) - 1), [t, 5]), t -> 0))"), "0");
}

#[test]
fn covariance_entries() {
    assert_eq!(ev("covariance(multivariate_poisson_distribution(μ, [2, 3, 7]), 2, 3)"), "μ");
    assert_eq!(ev("covariance(multivariate_poisson_distribution(μ, [2, 3, 7]), 2, 2)"), "3 + μ");
    assert_eq!(ev("d = multivariate_poisson_distribution(1, [2, 5]); table(covariance(d, i, j), i, 2, j, 2) == covariance(d)"), "true");
}

#[test]
fn dimensions_of_regions_from_symbols_and_curves() {
    assert_eq!(
        ev("[region_dimension(regular_polygon(n)), region_embedding_dimension(regular_polygon(n)), region_dimension(regular_polygon(r, n))]"),
        "[2, 2, 2]"
    );
    let c = "b_spline_curve([[c1, c2], [c3, c4], [c5, c6]])";
    assert_eq!(ev(&format!("[region_dimension(filled_curve([{c}])), region_embedding_dimension(filled_curve([{c}]))]")), "[2, 2]");
    assert_eq!(ev(&format!("[region_dimension(joined_curve([{c}])), region_embedding_dimension(joined_curve([{c}]))]")), "[1, 2]");
    assert_eq!(ev("region_embedding_dimension(joined_curve([line([[0, 0, 0], [1, 2, 3]])]))"), "3");
}

#[test]
fn bezier_lengths_and_bounds() {
    let b = "bezier_curve([[0, 0], [1, 1], [2, -1]])";
    assert_eq!(ev(&format!("region_bounds({b})")), "[[0, 2], [-1, 1/3]]");
    // the exact length against the closed form with inverse hyperbolic sines and a numeric integral of the speed
    assert_eq!(ev(&format!("abs(n(arc_length({b}) - (asinh(1) + asinh(2) + 2*sqrt(5) + sqrt(2))/3, 30)) < 10^-25")), "true");
    assert_eq!(ev(&format!("abs(n(arc_length({b})) - n_integrate(sqrt(4 + (2 - 6*t)^2), [t, 0, 1])) < 10^-10")), "true");
    // a quadratic B-spline of three control points is the same curve
    assert_eq!(ev(&format!("arc_length(b_spline_curve([[0, 0], [1, 1], [2, -1]])) == arc_length({b})")), "true");
    // a cubic in space: x = 3 t, y = 9 t (1 - t) (1 - 2 t) with extremes -+ 3^(1/2)/2 at t = (3 +- 3^(1/2))/6, z = 3 t^2 (1 - t)
    // with its largest value 4/9 at t = 2/3
    assert_eq!(
        ev("b = region_bounds(bezier_curve([[0, 0, 0], [1, 3, 0], [2, -3, 1], [3, 0, 0]])); max(abs(n(flatten(b) - [0, 3, -sqrt(3)/2, sqrt(3)/2, 0, 4/9]))) < 10^-12"),
        "true"
    );
    // a straight segment: its length
    assert_eq!(ev("arc_length(bezier_curve([[0, 0], [3, 4]]))"), "5");
}

#[test]
fn karhunen_loeve_of_images() {
    let imgs = "imgs = [image([[0.2, 0.4], [0.6, 0.8]]), image([[0.8, 0.6], [0.4, 0.2]]), image([[0.5, 0.5], [0.1, 0.9]])]; ";
    // the transform of the images' samples, as images
    assert_eq!(ev(&format!("{imgs}r = karhunen_loeve_decomposition(imgs); [length(r[1]), dimensions(r[2])]")), "[3, [3, 3]]");
    // the basis is the one of the samples as rows
    assert_eq!(
        ev(&format!("{imgs}karhunen_loeve_decomposition(imgs)[2] == karhunen_loeve_decomposition(map(i => flatten(image_data(i)), imgs))[2]")),
        "true"
    );
    // the inverse with every component gives the data back
    assert_eq!(
        ev("a = [[1, 2, 3], [4, 6, 8], [2, 2, 5]]; r = karhunen_loeve_decomposition(a); max(abs(n(karhunen_loeve_decomposition(r[1], r[2])[1] - a))) < 10^-12"),
        "true"
    );
    // with a rotation: the rows in the rotated coordinates m^T . a, zero rows added up to the size of m
    assert_eq!(
        ev("c = 1/sqrt(2); karhunen_loeve_decomposition([[0, 0, 1], [0, 1, 1]], [[c, -c], [c, c]]) == [[[0, c, 2*c], [0, c, 0]], [[c, c], [-c, c]]]"),
        "true"
    );
    assert_eq!(ev("c = 1/sqrt(2); karhunen_loeve_decomposition([[0, 0, 1]], [[c, -c], [c, c]])[1] == [[0, 0, c], [0, 0, -c]]"), "true");
    assert_eq!(
        ev(&format!("{imgs}r = karhunen_loeve_decomposition(imgs); dimensions(image_data(first(first(karhunen_loeve_decomposition(r[1][1:2], r[2])))))")),
        "[2, 2, 1]"
    );
}

#[test]
fn arcs_of_circles() {
    assert_eq!(ev("arc_length(circle([0, 0], 1, [pi/6, 3*pi/4]))"), "(7/12)*pi");
    // the arc against the integral of the speed of its parametrisation
    assert_eq!(ev("abs(n(arc_length(circle([1, 2], 3, [0, 2]))) - n_integrate(norm(d([3*cos(t), 3*sin(t)], t)), [t, 0, 2])) < 10^-10"), "true");
    assert_eq!(ev("arc_length(circle([0, 0], 2, [0, 2*pi])) == arc_length(circle([0, 0], 2))"), "true");
}
