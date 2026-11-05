//! Machine matrices: determinants, inverses and systems by LU, decompositions (LU, QR, Cholesky, SVD) with
//! reconstruction, ranks and null spaces by singular values, pseudo-inverses, conditioning, packed results, and
//! the exact decompositions.
mod common;
use common::{check, Sys};

#[test]
fn values() {
    check(&[
        ("det([[1.0, 2], [3, 4]])", "-2.0"),
        ("det([[1.0 + 2im, 3], [1, 1im]])", "complex(-5.0, 1.0)"),
        ("inverse([[2.0, 0], [0, 4]])", "[[0.5, 0.0], [0.0, 0.25]]"),
        ("inverse([[1.0, 2], [2, 4]])", "failure(singular, inverse)"),
        ("rank([[1.0, 2], [2, 4]])", "1"),
        ("rank([[1.0, 2, 3], [4, 5, 6], [7, 8, 9]])", "2"),
        ("row_reduce([[1.0, 2], [2, 4]])", "[[1.0, 2.0], [0.0, 0.0]]"),
        ("null_space([[1.0, 2], [2, 4]])", "[[0.8944271909999159, -0.4472135954999579]]"),
        ("null_space([[1.0, 0], [0, 1]])", "[]"),
        ("linear_solve([[2.0, 1], [1, 3]], [3, 5])", "[0.8, 1.4]"),
        ("linear_solve([[1.0, 2], [2, 4]], [1, 1])", "failure(no_solution, linear_solve)"),
        ("cholesky([[4.0, 2], [2, 3]])", "[[2.0, 1.0], [0.0, 1.4142135623730951]]"),
        ("cholesky([[1.0, 2], [2, 1]])", "failure(not_positive_definite, cholesky)"),
        ("cholesky([[1.0, 2], [0, 1]])", "failure(not_hermitian, cholesky)"),
        ("singular_values([[3.0, 2, 2], [2, 3, -2]])", "[4.999999999999999, 2.9999999999999996]"),
        ("singular_values([[1.0, 2], [2, 4]])", "[5.0]"),
        ("norm([[1.0, 2], [3, 4]])", "5.4649857042190435"),
        ("norm([[1.0, 2], [3, 4]], \"frobenius\")", "5.477225575051661"),
        ("norm([[1.0, -2], [3, 4]], 1)", "6.0"),
        ("norm([[1.0, -2], [3, 4]], inf)", "7.0"),
        ("pseudo_inverse([[1.0, 2], [2, 4]])", "[[0.04, 0.08], [0.08, 0.16]]"),
    ]);
}

#[test]
fn reconstruction() {
    let mut s = Sys::new();
    s.run("seed(11)");
    for (r, c) in [(4, 4), (6, 3), (3, 6), (30, 30), (40, 25)] {
        s.run(&format!("a = random_real([-1, 1], [{r}, {c}])"));
        s.run("f = lu(a); q = qr(a); v = svd(a)");
        s.holds("norm(a[f[3]] - dot(f[1], f[2]), inf) < 1e-13");
        s.holds("every(range(length(f[2])), i => every(range(min(i - 1, length(f[2][1]))), j => f[2][i][j] == 0))");
        s.holds("norm(dot(q[1], q[2]) - a, inf) < 1e-13");
        s.holds(&format!("norm(dot(transpose(q[1]), q[1]) - identity_matrix({}), inf) < 1e-13", r.min(c)));
        s.holds("norm(dot(v[1], v[2], transpose(v[3])) - a, inf) < 1e-13");
        s.holds(&format!("norm(dot(transpose(v[3]), v[3]) - identity_matrix({}), inf) < 1e-13", r.min(c)));
        // the Moore-Penrose conditions
        s.run("p = pseudo_inverse(a)");
        s.holds("norm(dot(a, p, a) - a, inf) < 1e-12*norm(a, inf)^2*norm(p, inf)");
        s.holds("norm(dot(p, a, p) - p, inf) < 1e-12*norm(p, inf)^2*norm(a, inf)");
    }
    // complex matrices
    s.run("z = [[1.0 + 1im, 2, 0], [0.5im, 3, 1 - 1im], [1, -1im, 2]]");
    s.run("f = lu(z); q = qr(z); v = svd(z)");
    s.holds("norm(z[f[3]] - dot(f[1], f[2]), inf) < 1e-14");
    s.holds("norm(dot(q[1], q[2]) - z, inf) < 1e-14");
    s.holds("norm(dot(v[1], v[2], conjugate(transpose(v[3]))) - z, inf) < 1e-14");
    s.holds("norm(dot(inverse(z), z) - identity_matrix(3), inf) < 1e-14");
    s.run("h = dot(conjugate(transpose(z)), z); u = cholesky(h)");
    s.holds("norm(dot(conjugate(transpose(u)), u) - h, inf) < 1e-13");
    // a rank-deficient matrix: the null space is orthonormal and annihilated
    s.run("b = dot(random_real(1, [8, 3]), random_real(1, [3, 6]))");
    s.holds("rank(b) == 3 and length(null_space(b)) == 3");
    s.holds("max(map(v => norm(dot(b, v)), null_space(b))) < 1e-13");
    s.holds("length(singular_values(b)) == 3");
}

#[test]
fn inverse_and_systems_at_size() {
    let mut s = Sys::new();
    s.run("seed(5); a = random_real(1, [200, 200]); b = random_real(1, 200)");
    s.holds("norm(dot(inverse(a), a) - identity_matrix(200), inf) < 1e-9");
    s.holds("norm(dot(a, linear_solve(a, b)) - b, inf) < 1e-11");
    s.holds("abs(det(a)*det(inverse(a)) - 1) < 1e-9");
    // packed in, packed out: the results are machine arrays of the right shape
    s.holds("length(inverse(a)) == 200 and length(inverse(a)[1]) == 200");
}

#[test]
fn conditioning() {
    let mut s = Sys::new();
    // the 8 x 8 Hilbert matrix (condition number about 1.5e10): the machine inverse loses about 10 digits
    s.run("h = table(1.0/(i + j - 1), i, 8, j, 8); x = table(1/(i + j - 1), i, 8, j, 8)");
    s.holds("norm(inverse(h) - n(inverse(x)), inf)/norm(n(inverse(x)), inf) < 1e-5");
    s.holds("abs(det(h)/n(det(x)) - 1) < 1e-6");
    // the exact one is exact
    s.holds("dot(inverse(x), x) === identity_matrix(8)");
    // singular to working precision
    s.run("w = [[1.0, 2, 3], [4, 5, 6], [7, 8, 9]]");
    assert_eq!(s.run("inverse(w)"), "failure(singular, inverse)");
    s.holds("abs(det(w)) < 1e-14");
    // the machine rank sees the numerical rank
    s.holds("rank([[1.0, 1], [1, 1 + 1e-17]]) == 1");
    s.holds("rank([[1.0, 1], [1, 1 + 1e-10]]) == 2");
}

#[test]
fn exact_decompositions() {
    check(&[
        ("lu([[1, 2], [3, 4]])", "[[[1, 0], [3, 1]], [[1, 2], [0, -2]], [1, 2]]"),
        ("lu([[0, 1], [1, 1]])", "[[[1, 0], [0, 1]], [[1, 1], [0, 1]], [2, 1]]"),
        (
            "qr([[1, 2], [3, 4]])",
            "[[[(1/10)*10^(1/2), (3/10)*10^(1/2)], [(3/10)*10^(1/2), (-1/10)*10^(1/2)]], [[10^(1/2), (7/5)*10^(1/2)], [0, (1/5)*10^(1/2)]]]",
        ),
        ("cholesky([[4, 2], [2, 3]])", "[[2, 1], [0, 2^(1/2)]]"),
        ("cholesky([[1, 2], [2, 1]])", "failure(not_positive_definite, cholesky)"),
        ("cholesky([[2, 1im], [-1im, 2]])", "[[2^(1/2), complex(0, 1/2)*2^(1/2)], [0, (1/2)*6^(1/2)]]"),
        ("singular_values([[1, 2], [3, 4]])", "[(15 + 221^(1/2))^(1/2), (15 - 221^(1/2))^(1/2)]"),
        ("singular_values([[1, 2], [2, 4]])", "[5]"),
        ("norm([[1, 2], [3, 4]])", "(15 + 221^(1/2))^(1/2)"),
        ("svd([[1, 2], [3, 4]])", "svd([[1, 2], [3, 4]])"),
    ]);
    let mut s = Sys::new();
    for m in ["[[2, -1, 0], [-1, 2, -1], [0, -1, 2]]", "[[4, 1, 2], [1, 5, 3], [2, 3, 6]]", "[[1, 1/2], [1/2, 1/3]]"] {
        s.run(&format!("m = {m}; u = cholesky(m); q = qr(m); f = lu(m)"));
        s.holds("expand(dot(transpose(u), u)) === m");
        s.holds("expand(dot(q[1], q[2])) === m");
        s.holds("expand(dot(transpose(q[1]), q[1])) === identity_matrix(length(m))");
        s.holds("m[f[3]] === dot(f[1], f[2])");
    }
}
