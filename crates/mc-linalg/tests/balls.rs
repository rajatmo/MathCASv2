//! Ball matrices: elimination in balls encloses the exact results; balls containing zero as the only pivot
//! candidates are reported, not guessed.
mod common;
use common::Sys;

#[test]
fn ball_elimination_encloses_exact_results() {
    let mut s = Sys::new();
    s.run("m = [[4, -2, 1], [3, 6, -4], [2, 1, 8]]; b = [12, -25, 32]");
    s.run("mb = n(m, 40); bb = n(b, 40)");
    // the exact values lie inside the balls: |ball - exact| is a ball containing zero, so the comparison with a
    // tiny bound decides
    s.holds("abs(det(mb) - det(m)) < 1e-35");
    s.holds("every(flatten(inverse(mb) - inverse(m)), x => abs(x) < 1e-35)");
    s.holds("every(linear_solve(mb, bb) - linear_solve(m, b), x => abs(x) < 1e-35)");
    s.holds("rank(mb) == 3");
    s.holds("every(flatten(row_reduce(mb) - identity_matrix(3)), x => abs(x) < 1e-35)");
    // at 200 digits
    s.holds("abs(det(n(m, 200)) - det(m)) < 1e-190");
    // pi in a matrix
    s.run("p = [[n(pi, 30), 1], [1, 2]]");
    s.holds("abs(det(p) - (2*pi - 1)) < 1e-28");
    // the ball and exact pseudo-inverse, LU
    s.holds("every(flatten(pseudo_inverse(mb) - inverse(m)), x => abs(x) < 1e-35)");
    s.run("f = lu(mb)");
    s.holds("every(flatten(dot(f[1], f[2]) - mb[f[3]]), x => abs(x) < 1e-35)");
}

#[test]
fn undecided_pivots() {
    let mut s = Sys::new();
    // z: a ball around zero with a nonzero radius
    s.run("z = 3*n(1/3, 20) - 1");
    for (src, want) in [
        ("inverse([[z, 1], [z, 2]])", "failure(undecided, inverse)"),
        ("rank([[z, 0], [0, 0]])", "failure(undecided, rank)"),
        ("det([[z, 0], [0, 1]])", "failure(undecided, det)"),
        ("null_space([[z, 1], [z, 1]])", "failure(undecided, null_space)"),
        // an exact zero column is decided
        ("rank([[0, 1~20], [0, 2]])", "1"),
        ("det([[0, 1~20], [0, 2]])", "0"),
    ] {
        assert_eq!(s.run(src), want, "{src}");
    }
}
