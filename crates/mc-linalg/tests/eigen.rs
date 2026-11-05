//! Characteristic polynomials, eigenvalues and eigenvectors: exact (rational, radical and `root` eigenvalues,
//! defective matrices, parameters) and machine (symmetric, nonsymmetric, complex), with reconstruction checks.
mod common;
use common::{check, Sys};

#[test]
fn characteristic_polynomials() {
    check(&[
        ("characteristic_polynomial([[1, 2, 3], [4, 5, 6], [7, 8, 10]], x)", "3 + -16*x^2 + -12*x + x^3"),
        ("characteristic_polynomial([[a, b], [c, d]], x)", "a*d + -1*a*x + -1*b*c + -1*d*x + x^2"),
        ("characteristic_polynomial([[1/2, 0], [0, 1/3]], t)", "1/6 + (-5/6)*t + t^2"),
        ("characteristic_polynomial([[2.0, 0], [0, 3]], x)", "6.0 + -5.0*x + 1.0*x^2"),
        ("characteristic_polynomial([[1, 1im], [0, 1]], x)", "1 + -2*x + x^2"),
        ("characteristic_polynomial([[1, 2]], x)", "failure(not_square, characteristic_polynomial)"),
    ]);
}

#[test]
fn exact_eigenvalues() {
    check(&[
        ("eigenvalues([[2, 1], [1, 2]])", "[3, 1]"),
        ("eigenvectors([[2, 1], [1, 2]])", "[[1, 1], [-1, 1]]"),
        ("eigenvalues([[2, 0, 0], [0, 3, 4], [0, 4, 9]])", "[11, 2, 1]"),
        ("eigenvalues([[4, 1], [2, 3]])", "[5, 2]"),
        ("eigenvectors([[4, 1], [2, 3]])", "[[1, 1], [-1/2, 1]]"),
        ("eigenvalues([[1, 2], [3, 4]])", "[5/2 + (1/2)*33^(1/2), 5/2 + (-1/2)*33^(1/2)]"),
        ("eigenvectors([[1, 2], [3, 4]])", "[[-1/2 + (1/6)*33^(1/2), 1], [-1/2 + (-1/6)*33^(1/2), 1]]"),
        ("eigenvalues([[0, 1], [-1, 0]])", "[1im, -1im]"),
        ("eigenvectors([[0, 1], [-1, 0]])", "[[-1im, 1], [1im, 1]]"),
        ("eigenvalues([[1, 2, 0], [0, 1, 2], [2, 0, 1]])", "[3, 1im*3^(1/2), -1im*3^(1/2)]"),
        // defective: a zero vector for the missing eigenvector
        ("eigenvalues([[1, 1, 0], [0, 1, 0], [0, 0, 2]])", "[2, 1, 1]"),
        ("eigenvectors([[1, 1, 0], [0, 1, 0], [0, 0, 2]])", "[[0, 0, 1], [1, 0, 0], [0, 0, 0]]"),
        ("eigenvectors([[1, 1], [0, 1]])", "[[1, 0], [0, 0]]"),
        ("eigensystem([[2, 0], [0, 3]])", "[[3, 2], [[0, 1], [1, 0]]]"),
        // an irreducible cubic: root objects numbered real roots first
        ("eigenvalues([[0, 1, 0], [0, 0, 1], [1, 1, 0]])", "[root(x => -1 - x + x^3, 1), root(x => -1 - x + x^3, 2), root(x => -1 - x + x^3, 3)]"),
        // parameters
        ("eigenvalues([[a, b], [b, a]])", "[a - b, a + b]"),
        ("eigenvectors([[a, b], [b, a]])", "[[-1, 1], [1, 1]]"),
        ("eigenvalues([[a, 0], [0, a]])", "[a, a]"),
        ("eigenvectors([[a, 0], [0, a]])", "[[0, 1], [1, 0]]"),
        ("eigenvalues([[1, 2], [3]])", "failure(argument, eigenvalues, 1)"),
        ("eigenvalues([[1, 2]])", "failure(not_square, eigenvalues)"),
    ]);
}

#[test]
fn exact_reconstruction() {
    let mut s = Sys::new();
    for m in [
        "[[1, 2], [3, 4]]",
        "[[2, -1, 0], [-1, 2, -1], [0, -1, 2]]",
        "[[1, 2, 0], [0, 1, 2], [2, 0, 1]]",
        "[[5, 4, 2, 1], [0, 1, -1, -1], [-1, -1, 3, 0], [1, 1, -1, 2]]",
        "[[1/2, 1/3], [1/4, 1/5]]",
    ] {
        s.run(&format!("m = {m}; e = eigensystem(m)"));
        s.holds("length(e[1]) == length(m)");
        // m v - lambda v expands to zero for every pair
        s.holds("every(range(length(m)), k => every(expand(dot(m, e[2][k]) - e[1][k]*e[2][k]), x => x == 0))");
        s.holds("expand(total(e[1]) - trace(m)) == 0");
    }
    // root objects: the residual vanishes modulo the minimal polynomial
    s.run("m = [[0, 1, 0], [0, 0, 1], [1, 1, 0]]; e = eigensystem(m)");
    for k in 1..=3 {
        s.run(&format!("w = rewrite(expand(dot(m, e[2][{k}]) - e[1][{k}]*e[2][{k}]), root(f_, {k}) -> r)"));
        s.holds("every(w, p => polynomial_remainder(p, r^3 - r - 1, r) == 0)");
    }
    // a symbolic quadratic
    s.run("m = [[a, b], [c, d]]; e = [eigenvalues(m)]");
    // eigenvectors of a quadratic factor with parameters are a gap: the call stays
    assert_eq!(s.run("eigenvectors(m)"), "eigenvectors([[a, b], [c, d]])");
    s.holds("expand(e[1][1] + e[1][2] - a - d) == 0");
    s.holds("expand(expand(e[1][1]*e[1][2] - det(m))) == 0");
}

#[test]
fn machine_eigenvalues() {
    let mut s = Sys::new();
    let out = s.run("eigenvalues([[1.0, 2], [3, 4]])");
    assert_eq!(out, "[5.372281323269014, -0.3722813232690134]");
    s.run("r = eigenvalues([[0.0, 1], [-1, 0]])");
    s.holds("abs(r[1] - 1im) < 1e-15 and abs(r[2] + 1im) < 1e-15");
    // symmetric by Jacobi; nonsymmetric and complex by the Schur form
    for m in [
        "[[4.0, 1, 2], [1, 3, 0], [2, 0, 5]]",
        "[[1.0, 2, 3], [4, 5, 6], [7, 8, 10]]",
        "[[1.0, 2, 0], [0, 1, 2], [2, 0, 1]]",
        "[[1.0 + 1im, 2], [3, 4 - 2im]]",
        "[[2.0, 1, 0, 0], [1, 2, 1, 0], [0, 1, 2, 1], [0, 0, 1, 2]]",
    ] {
        s.run(&format!("m = {m}; e = eigensystem(m)"));
        s.holds("max(map(k => norm(dot(m, e[2][k]) - e[1][k]*e[2][k]), range(length(m)))) < 1e-13");
        s.holds("max(map(v => abs(norm(v) - 1), e[2])) < 1e-14");
        s.holds("abs(total(e[1]) - trace(m)) < 1e-13");
    }
    // the exact and machine results agree
    s.holds("max(abs(n(eigenvalues([[2, 0, 0], [0, 3, 4], [0, 4, 9]])) - eigenvalues([[2.0, 0, 0], [0, 3, 4], [0, 4, 9]]))) < 1e-14");
    // a large symmetric matrix: orthonormal eigenvectors reconstruct it
    s.run("seed(3); a = random_real(1, [60, 60]); b = a + transpose(a); e = eigensystem(b)");
    s.run("v = transpose(e[2]); r = dot(v, diagonal_matrix(e[1]), transpose(v))");
    s.holds("norm(r - b, inf) < 1e-12");
    s.holds("norm(dot(transpose(v), v) - identity_matrix(60), inf) < 1e-13");
    // a large nonsymmetric matrix: eigenpairs
    s.run("c = random_real(1, [40, 40]); e = eigensystem(c)");
    s.holds("max(map(k => norm(dot(c, e[2][k]) - e[1][k]*e[2][k]), range(40))) < 1e-11");
}
