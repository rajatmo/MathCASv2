//! Modular functions at every point of the upper half-plane (reduction to the fundamental domain at machine
//! and arbitrary precision), theta functions near the unit circle, Mathieu functions and characteristic values at
//! machine and high precision, numeric linear algebra on matrices of arbitrary-precision numbers, and infinite
//! products of finite products. Each value is checked by a second route: an identity of the modular group, the value
//! at an exact argument at 30 or more digits, a closed form, or a second algorithm.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// `src` evaluates to `true`.
fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

/// The numbers `a` and `b` (terms) agree to a relative 10^-tol, each evaluated as it is.
fn close_rel(e: &mut CoreEngine, a: &str, b: &str, tol: u32) {
    holds(e, &format!("{{let x = {a}; let y = {b}; let d = x - y; abs(re(d)) <= 10^-{tol}*abs(y) and abs(im_part(d)) <= 10^-{tol}*abs(y)}}"));
}

/// The value of `src` is a number: the function `name` is not left in it.
fn numeric(e: &mut CoreEngine, src: &str, name: &str) {
    let s = ev(e, src);
    assert!(!s.contains(name) && !s.contains("failure"), "{src}: {s}");
}

#[test]
fn modular_functions_on_and_near_the_boundary_arcs() {
    let mut e = CoreEngine::new();
    // 1/2 + 0.3679 i maps onto the arc |t| = 1, where the reduction went back and forth
    for tau in ["0.5 + 0.3679*im", "1.5 + 0.3679*im", "-0.5 + 0.3679*im", "0.2977 + 0.9547*im", "-0.5 + 0.8660254037844*im"] {
        let s = ev(&mut e, &format!("klein_invariant_j({tau})"));
        assert!(!s.contains("klein_invariant_j"), "{tau}: {s}");
        let s = ev(&mut e, &format!("modular_lambda({tau})"));
        assert!(!s.contains("modular_lambda"), "{tau}: {s}");
        let s = ev(&mut e, &format!("dedekind_eta({tau})"));
        assert!(!s.contains("dedekind_eta"), "{tau}: {s}");
    }
    // J(tau) = J(-1/tau) = J(tau + 1) at machine points (the reduction takes different paths)
    for tau in ["0.5 + 0.3679*im", "0.31 + 0.02*im", "-0.77 + 0.4*im"] {
        close_rel(&mut e, &format!("klein_invariant_j({tau})"), &format!("klein_invariant_j(-1/({tau}))"), 11);
        close_rel(&mut e, &format!("klein_invariant_j({tau})"), &format!("klein_invariant_j({tau} + 3)"), 11);
    }
}

#[test]
fn modular_functions_at_high_precision_and_near_cusps() {
    let mut e = CoreEngine::new();
    // lambda(1 + 2 i) = lambda(2 i)/(lambda(2 i) - 1); lambda(2 i) = 1 - lambda(i/2)
    close_rel(&mut e, "n(modular_lambda(1 + 2*im), 50)", "n(modular_lambda(2*im)/(modular_lambda(2*im) - 1), 50)", 45);
    close_rel(&mut e, "n(modular_lambda(2*im), 50)", "n(1 - modular_lambda(im/2), 50)", 45);
    // J(2 i) = 66^3/1728 = 166.375 (DLMF 23.17)
    close_rel(&mut e, "n(klein_invariant_j(2*im), 40)", "n(66^3/1728, 40)", 35);
    // eta(-1/tau) = sqrt(-i tau) eta(tau) near the cusp 1/3, at 30 digits
    close_rel(&mut e, "n(dedekind_eta(-1/(1/3 + im/1000)), 30)", "n(sqrt(-im*(1/3 + im/1000))*dedekind_eta(1/3 + im/1000), 30)", 25);
    close_rel(&mut e, "n(dedekind_eta(im), 30)", "n(gamma(1/4)/(2*pi^(3/4)), 30)", 28);
    // a machine point is the exact dyadic number it is: its value agrees with the 30-digit value there
    let x = "5404319552844595/2^54";
    let y = "1152921504606847/2^60";
    for f in ["klein_invariant_j", "modular_lambda", "dedekind_eta"] {
        close_rel(&mut e, &format!("{f}(0.3 + 0.001*im)"), &format!("n({f}({x} + im*{y}), 30)"), 14);
    }
    // near a cusp the value leaves the machine range: no machine value, a value at 20 digits
    assert!(ev(&mut e, "modular_lambda(0.3 + 0.00001*im)").starts_with("modular_lambda("));
    numeric(&mut e, "n(klein_invariant_j(1/7 + im/10^8), 20)", "klein_invariant_j");
}

#[test]
fn theta_functions_near_the_unit_circle() {
    let mut e = CoreEngine::new();
    // theta_3(0, -q) = theta_4(0, q) (DLMF 20.2.3-4); the series cancels to 1e-106 at q = 0.99
    close_rel(&mut e, "n(elliptic_theta(3, 0, -99/100), 20)", "n(elliptic_theta(4, 0, 99/100), 20)", 15);
    holds(&mut e, "abs(elliptic_theta(3, 0, -0.99)/n(elliptic_theta(4, 0, 99/100), 20) - 1) < 10^-10");
    // theta_3(0, q) ~ sqrt(pi/(-log q)) as q -> 1 (Jacobi's imaginary transformation)
    close_rel(&mut e, "n(elliptic_theta(3, 0, 9999/10000), 20)", "n(sqrt(pi/(-log(9999/10000))), 20)", 18);
    // a complex nome near the circle at 20 and 40 digits agree, and the machine value is near
    close_rel(&mut e, "n(elliptic_theta(4, 13/10, 95/100*exp(3*im/10)), 20)", "n(elliptic_theta(4, 13/10, 95/100*exp(3*im/10)), 40)", 18);
    close_rel(&mut e, "elliptic_theta(4, 1.3, 0.95*exp(0.3*im))", "n(elliptic_theta(4, 13/10, 95/100*exp(3*im/10)), 40)", 12);
    // a machine value below the machine range is not given as 0 or as rounding noise
    assert!(ev(&mut e, "elliptic_theta(3, 0, -0.999)").contains("elliptic_theta"));
}

#[test]
fn mathieu_functions_by_their_floquet_series() {
    let mut e = CoreEngine::new();
    // far along the real axis and up the imaginary axis, against the 20-digit values of the exact arguments
    close_rel(&mut e, "mathieu_s_prime(8., 5., 77.3)", "n(mathieu_s_prime(8, 5, 773/10), 20)", 11);
    close_rel(&mut e, "mathieu_c(3., 2., 3.5*im)", "n(mathieu_c(3, 2, 7/2*im), 20)", 12);
    close_rel(&mut e, "mathieu_c(3. + im, 2., 1.5 - 0.5*im)", "n(mathieu_c(3 + im, 2, 3/2 - im/2), 20)", 13);
    // the equation: y'' + (a - 2 q cos 2z) y = 0 by central differences of the machine values
    holds(&mut e, "{let h = 1/1000.; let f = (z => mathieu_c(3.3, 1.7, z)); abs((f(2.1 + h) - 2*f(2.1) + f(2.1 - h))/h^2 + (3.3 - 3.4*cos(4.2))*f(2.1)) < 10^-5}");
    // a 40-digit root search over a characteristic value (it stopped with not_numeric)
    let s = ev(&mut e, "find_root(mathieu_s(mathieu_characteristic_b(5, q), q, im/2), [q, 30], working_precision: 40)");
    assert!(s.starts_with("[q -> 28.9455003886044669876403839783"), "{s}");
    // 25-digit parameters with a machine argument give the machine value, quickly
    close_rel(&mut e, "mathieu_c(38.680811859218614060802~25, 22.66671708059834602782345~25, 0.3)", "mathieu_c(38.680811859218614060802, 22.66671708059834602782345, 0.3)", 12);
}

#[test]
fn characteristic_values_of_non_integer_order_stay_in_their_band() {
    let mut e = CoreEngine::new();
    // 0 < nu < 1 lies between a_0 and b_1, 1 < nu < 2 between a_1 and b_2 (DLMF 28.12)
    holds(&mut e, "mathieu_characteristic_a(0, 1.) < mathieu_characteristic_a(0.99, 1.) and mathieu_characteristic_a(0.99, 1.) < mathieu_characteristic_b(1, 1.)");
    holds(&mut e, "mathieu_characteristic_a(1, 1.) < mathieu_characteristic_a(1.01, 1.) and mathieu_characteristic_a(1.01, 1.) < mathieu_characteristic_b(2, 1.)");
    holds(&mut e, "mathieu_characteristic_a(1, 3.) < mathieu_characteristic_a(1.5, 3.) and mathieu_characteristic_a(1.5, 3.) < mathieu_characteristic_b(2, 3.)");
    // the value at nu = 15/17, q = 5 at 25 digits, and the machine value
    close_rel(&mut e, "n(mathieu_characteristic_a(15/17, 5), 25)", "-5.790418614934502217370044", 22);
    close_rel(&mut e, "mathieu_characteristic_a(15/17, 5.)", "-5.790418614934502217370044", 13);
    // cos(pi nu) = w_I(pi) at the value (DLMF 28.2.16): the even solution with y(0) = 1 at pi
    holds(&mut e, "{let a = mathieu_characteristic_a(0.3, 2.); abs(mathieu_c(a, 2., pi*1.)/mathieu_c(a, 2., 0.) - cos(0.3*pi)) < 10^-10}");
}

#[test]
fn linear_algebra_of_arbitrary_precision_matrices() {
    let mut e = CoreEngine::new();
    // the 2-norm of [[1, 2], [3, 4]] is sqrt(15 + sqrt 221), its singular values sqrt(15 +- sqrt 221)
    close_rel(&mut e, "norm(n([[1, 2], [3, 4]], 30))", "n(sqrt(15 + sqrt(221)), 30)", 28);
    close_rel(&mut e, "norm([[1, 2], [3, n(4, 30)]])", "n(sqrt(15 + sqrt(221)), 30)", 28);
    close_rel(&mut e, "last(singular_values(n([[1, 2], [3, 4]], 30)))", "n(sqrt(15 - sqrt(221)), 30)", 28);
    // a complex matrix: the singular values are the square roots of the eigenvalues of m^H m
    close_rel(&mut e, "first(singular_values(n([[1, 2 + im], [3, 4]], 30)))", "n(sqrt(max(eigenvalues(dot(conjugate_transpose([[1, 2 + im], [3, 4]]), [[1, 2 + im], [3, 4]])))), 30)", 27);
    // a rank-one matrix has one nonzero singular value, its Frobenius norm 5
    close_rel(&mut e, "norm(n([[1, 2], [2, 4]], 30))", "n(5, 30)", 28);
    // eigenvalues of a matrix that is not symmetric: (5 +- sqrt 33)/2, and +- i
    close_rel(&mut e, "first(eigenvalues(n([[1, 2], [3, 4]], 30)))", "n((5 + sqrt(33))/2, 30)", 28);
    close_rel(&mut e, "first(eigenvalues(n([[0, 1], [-1, 0]], 30)))", "im", 28);
    // a 6 x 6 matrix at 40 digits against its exact eigenvalues at 40 digits
    holds(&mut e, "{let m = hilbert_matrix(6) + [[0, 1, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0]]; max(abs(eigenvalues(n(m, 40)) - n(eigenvalues(m), 40))) < 10^-30}");
}

#[test]
fn an_infinite_product_of_finite_products() {
    let mut e = CoreEngine::new();
    // the limit of the partial products, by Richardson extrapolation of log P(N) at N = 10^4, 2 10^4, 4 10^4 (even
    // N, an error c/N): 0.5629461736
    let s = ev(&mut e, "n_product(1 + (-1)^n_u*((2/n_u)^k/k^2), [n_u, 2, inf], [k, 1, n_u])");
    assert!(s.starts_with("0.56294617"), "{s}");
    holds(&mut e, "{let a = n_product(1 + (-1)^n_u*((2/n_u)^k/k^2), [n_u, 2, 2000], [k, 1, n_u]); let b = n_product(1 + (-1)^n_u*((2/n_u)^k/k^2), [n_u, 2, 4000], [k, 1, n_u]); abs(exp(2*log(b) - log(a)) - 0.5629461736) < 10^-6}");
    // the merged powers: a finite product whose factor overflows as 2^k n^-k is the compiled product
    close_rel(&mut e, "n_sum(n_product(1 + (-1)^n_u*((2/n_u)^k/k^2), [k, 1, n_u]), [n_u, 8194, 8194])", "n_product(1 + (2/8194)^k/k^2, [k, 1, 8194])", 13);
}
