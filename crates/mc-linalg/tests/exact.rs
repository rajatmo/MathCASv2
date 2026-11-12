//! Exact and symbolic matrices: determinants, inverses, ranks, null spaces, reduced row echelon forms, linear
//! systems, pseudo-inverses, against expected values and identities over seeded random matrices.
mod common;
use common::{check, Sys};

/// A small deterministic generator (xorshift) for random exact matrices.
struct Rng(u64);

impl Rng {
    fn int(&mut self, lo: i64, hi: i64) -> i64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        lo + (self.0 % (hi - lo + 1) as u64) as i64
    }
    fn matrix(&mut self, r: usize, c: usize, lo: i64, hi: i64) -> String {
        let rows: Vec<String> =
            (0..r).map(|_| format!("[{}]", (0..c).map(|_| self.int(lo, hi).to_string()).collect::<Vec<_>>().join(", "))).collect();
        format!("[{}]", rows.join(", "))
    }
}

#[test]
fn reference_values() {
    check(&[
        ("det([[1, 2], [3, 4]])", "-2"),
        ("det([[1, -5, 3], [-8, -7, 8], [-6, 2, 9]])", "-373"),
        (
            "inverse([[1, -5, 3], [-8, -7, 8], [-6, 2, 9]])",
            "[[79/373, -51/373, 19/373], [-24/373, -27/373, 32/373], [58/373, -28/373, 47/373]]",
        ),
        ("det([[-8, 7, -3, -8], [-7, 4, 4, -7], [-2, -7, 8, 4], [-8, 9, -6, -2]])", "-2220"),
        (
            "inverse([[-8, 7, -3, -8], [-7, 4, 4, -7], [-2, -7, 8, 4], [-8, 9, -6, -2]])",
            "[[-11/74, 23/222, -59/444, -5/148], [-61/185, 47/185, -53/370, 53/370], [-44/185, 43/185, -17/370, 17/370], [-13/74, 7/222, 11/444, 21/148]]",
        ),
        ("det([[9, -8, 9, 9, 3], [-8, -2, -8, 8, -5], [0, 4, -5, 8, -6], [9, 0, 8, -4, -6], [9, 9, -3, 2, -6]])", "-55023"),
        (
            "inverse([[9, -8, 9, 9, 3], [-8, -2, -8, 8, -5], [0, 4, -5, 8, -6], [9, 0, 8, -4, -6], [9, 9, -3, 2, -6]])[1]",
            "[3076/55023, 2342/18341, -13600/55023, -1057/55023, 10340/55023]",
        ),
        (
            "det([[8, -7, 9, -8, -3, 6], [8, 4, 1, 5, 9, 5], [2, 0, -2, -4, -2, -7], [9, 0, 7, 6, 1, 5], [0, -7, -6, 7, 4, -4], [1, -5, 6, 4, -8, -7]])",
            "484404",
        ),
        (
            "inverse([[8, -7, 9, -8, -3, 6], [8, 4, 1, 5, 9, 5], [2, 0, -2, -4, -2, -7], [9, 0, 7, 6, 1, 5], [0, -7, -6, 7, 4, -4], [1, -5, 6, 4, -8, -7]])[6]",
            "[-1664/121101, -6598/40367, -1895/121101, 21404/121101, 2626/121101, -17182/121101]",
        ),
        ("rank([[4, 1, 1, 1, 3], [3, -3, -3, 0, 3], [10, -5, -5, 1, 9], [9, 6, 6, 3, 6]])", "2"),
        (
            "row_reduce([[4, 1, 1, 1, 3], [3, -3, -3, 0, 3], [10, -5, -5, 1, 9], [9, 6, 6, 3, 6]])",
            "[[1, 0, 0, 1/5, 4/5], [0, 1, 1, 1/5, -1/5], [0, 0, 0, 0, 0], [0, 0, 0, 0, 0]]",
        ),
        // one vector per free column, that entry one, from the last free column
        (
            "null_space([[4, 1, 1, 1, 3], [3, -3, -3, 0, 3], [10, -5, -5, 1, 9], [9, 6, 6, 3, 6]])",
            "[[-4/5, 1/5, 0, 0, 1], [-1/5, -1/5, 0, 1, 0], [0, -1, 1, 0, 0]]",
        ),
        ("null_space([[1, 2, 3], [2, 4, 6]])", "[[-3, 0, 1], [-2, 1, 0]]"),
        ("null_space([[1, 2], [3, 4]])", "[]"),
        ("row_reduce([[1, 2, 3, 4], [2, 4, 6, 8], [1, 0, 1, 0]])", "[[1, 0, 1, 0], [0, 1, 1, 2], [0, 0, 0, 0]]"),
        ("linear_solve([[1, 2, 3], [4, 5, 6], [7, 8, 10]], [1, 2, 3])", "[-1/3, 2/3, 0]"),
        ("linear_solve([[1, 2], [3, 4]], [[1, 0], [0, 1]])", "[[-2, 1], [3/2, -1/2]]"),
        // underdetermined: the free unknowns are zero
        ("linear_solve([[1, 2, 3]], [6])", "[6, 0, 0]"),
        ("linear_solve([[1, 2], [2, 4]], [1, 2])", "[1, 0]"),
        ("pseudo_inverse([[1, 2], [2, 4]])", "[[1/25, 2/25], [2/25, 4/25]]"),
        ("pseudo_inverse([[1, 0], [0, 0], [0, 1]])", "[[1, 0, 0], [0, 0, 1]]"),
        ("pseudo_inverse([[2, 0], [0, 4]])", "[[1/2, 0], [0, 1/4]]"),
        // rationals and the 6x6 Hilbert matrix
        ("det([[1/2, 1/3], [1/4, 1/5]])", "1/60"),
        ("det(table(1/(i + j - 1), i, 6, j, 6))", "1/186313420339200000"),
        ("inverse(table(1/(i + j - 1), i, 6, j, 6))[1]", "[36, -630, 3360, -7560, 7560, -2772]"),
        ("det(identity_matrix(5))", "1"),
        ("det([[0, 1], [1, 0]])", "-1"),
        ("det([[0, 0], [0, 0]])", "0"),
        ("det([[7]])", "7"),
        ("inverse([[7]])", "[[1/7]]"),
        ("rank([[0, 0], [0, 0]])", "0"),
    ]);
}

#[test]
fn symbolic_entries() {
    check(&[
        ("det([[a, b], [c, d]])", "a*d + -1*b*c"),
        (
            "inverse([[a, b], [c, d]])",
            "[[d/(a*d + -1*b*c), -1*b/(a*d + -1*b*c)], [-1*c/(a*d + -1*b*c), a/(a*d + -1*b*c)]]",
        ),
        ("det([[x, 1], [1, x]])", "-1 + x^2"),
        ("det([[1/x, 1], [1, x]])", "0"),
        ("rank([[1/x, 1], [1, x]])", "1"),
        ("null_space([[1/x, 1], [1, x]])", "[[-x, 1]]"),
        ("row_reduce([[a, b], [c, d]])", "[[1, 0], [0, 1]]"),
        ("linear_solve([[a, 1], [1, a]], [1, 0])", "[a/(-1 + a^2), -(-1 + a^2)^-1]"),
        ("det([[sin(t), cos(t)], [-cos(t), sin(t)]])", "cos(t)^2 + sin(t)^2"),
        ("inverse([[1, 0], [0, 0]])", "failure(singular, inverse)"),
        ("inverse([[a, a], [b, b]])", "failure(singular, inverse)"),
    ]);
}

#[test]
fn failures() {
    check(&[
        ("det([1, 2])", "failure(argument, det, 1)"),
        ("det([[1, 2, 3], [4, 5, 6]])", "failure(not_square, det)"),
        ("det([[1, 2], [3]])", "failure(argument, det, 1)"),
        ("det([[[1]]])", "failure(argument, det, 1)"),
        ("det([])", "failure(argument, det, 1)"),
        ("det(m)", "det(m)"),
        ("det([[1, 2], [3, 4]], 5)", "det([[1, 2], [3, 4]], 5)"),
        ("inverse([[1, 2], [2, 4]])", "failure(singular, inverse)"),
        ("linear_solve([[1, 2], [2, 4]], [1, 1])", "failure(no_solution, linear_solve)"),
        ("linear_solve([[1, 2], [3, 4]], [1, 2, 3])", "failure(shape_mismatch, linear_solve)"),
        ("rank([1, 2])", "failure(argument, rank, 1)"),
        ("null_space(5)", "null_space(5)"),
    ]);
}

#[test]
fn random_identities() {
    let mut s = Sys::new();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for round in 0..24 {
        let n = 2 + round % 5;
        let a = rng.matrix(n, n, -6, 6);
        let b = rng.matrix(n, n, -6, 6);
        s.run(&format!("a = {a}; b = {b}"));
        s.holds("det(dot(a, b)) == det(a)*det(b)");
        s.holds("det(transpose(a)) == det(a)");
        if s.run("det(a)") != "0" {
            s.holds(&format!("dot(inverse(a), a) === identity_matrix({n})"));
            s.holds(&format!("dot(a, inverse(a)) === identity_matrix({n})"));
            s.holds("det(inverse(a)) == 1/det(a)");
            s.holds("dot(a, linear_solve(a, b)) === b");
            s.holds("dot(a, linear_solve(a, b[1])) === b[1]");
            s.holds("pseudo_inverse(a) === inverse(a)");
        }
        // rank-deficient: the product of an n x k and a k x n matrix
        let k = 1 + round % (n - 1);
        let u = rng.matrix(n, k, -4, 4);
        let v = rng.matrix(k, n + 1, -4, 4);
        s.run(&format!("c = dot({u}, {v})"));
        s.holds(&format!("rank(c) + length(null_space(c)) == {}", n + 1));
        s.holds("rank(c) == rank(transpose(c))");
        s.holds("every(flatten(map(v => dot(c, v), null_space(c))), x => x == 0)");
        s.holds("rank(row_reduce(c)) == rank(c)");
        s.holds("row_reduce(row_reduce(c)) === row_reduce(c)");
        // the Moore-Penrose conditions
        s.run("p = pseudo_inverse(c)");
        s.holds("dot(c, p, c) === c");
        s.holds("dot(p, c, p) === p");
        s.holds("transpose(dot(c, p)) === dot(c, p)");
        s.holds("transpose(dot(p, c)) === dot(p, c)");
    }
}

#[test]
fn symbolic_identities() {
    let mut s = Sys::new();
    s.run("m = [[a, b, 0], [c, d, e], [0, f, g]]");
    s.holds("expand(det(m) - (a*d*g - a*e*f - b*c*g)) == 0");
    s.holds("map(r => map(together, r), dot(inverse(m), m)) === identity_matrix(3)");
    s.run("r = [[1, x], [x^2, 1/(1 + x)]]");
    s.holds("map(w => map(together, w), dot(r, inverse(r))) === identity_matrix(2)");
    s.holds("together(det(r)*det(inverse(r))) == 1");
}
