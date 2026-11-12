//! Products, constructors, norms and vector calculus.
mod common;
use common::{check, Sys};

#[test]
fn products_and_constructors() {
    check(&[
        ("dot([1, 2, 3], [4, 5, 6])", "32"),
        ("dot([a, b], [c, d])", "a*c + b*d"),
        ("dot([[1, 2], [3, 4]], [[5, 6], [7, 8]])", "[[19, 22], [43, 50]]"),
        ("dot([1, 2], [[1, 2], [3, 4]])", "[7, 10]"),
        ("dot([[1, 2], [3, 4]], [1, 2])", "[5, 11]"),
        ("dot([[1, 2], [3, 4]], [[1, 0], [0, 1]], [1, 1])", "[3, 7]"),
        ("dot([1.0, 2], [3, 4])", "11.0"),
        ("dot([1im, 2], [1im, 1])", "1"),
        ("dot([1/2, 1/3], [2, 3])", "2"),
        ("dot([], [])", "0"),
        ("dot([1, 2], [1, 2, 3])", "failure(shape_mismatch, dot)"),
        ("dot([1, 2], [1, 2], [1, 2])", "failure(shape_mismatch, dot)"),
        ("dot(u, v)", "dot(u, v)"),
        ("cross([1, 2, 3], [4, 5, 6])", "[-3, 6, -3]"),
        ("cross([a, b, c], [x, y, z])", "[b*z + -1*c*y, c*x + -1*a*z, a*y + -1*b*x]"),
        ("cross([1.0, 0, 0], [0, 1, 0])", "[0.0, 0.0, 1.0]"),
        ("cross([1, 2], [3, 4])", "failure(argument, cross, 1)"),
        ("kronecker_product([[1, 2], [3, 4]], [[0, 1], [1, 0]])", "[[0, 1, 0, 2], [1, 0, 2, 0], [0, 3, 0, 4], [3, 0, 4, 0]]"),
        ("kronecker_product([1, 2], [a, b])", "[a, b, 2*a, 2*b]"),
        ("kronecker_product([1, 2], [[1]])", "failure(shape_mismatch, kronecker_product)"),
        ("trace([[1, 2], [3, 4]])", "5"),
        ("trace([[a, b], [c, d]])", "a + d"),
        ("trace([[1, 2]])", "1"),
        ("identity_matrix(3)", "[[1, 0, 0], [0, 1, 0], [0, 0, 1]]"),
        ("identity_matrix(0)", "[]"),
        ("identity_matrix(-1)", "failure(argument, identity_matrix, 1)"),
        ("identity_matrix(x)", "failure(argument, identity_matrix, 1)"),
        ("diagonal_matrix([1, 2])", "[[1, 0], [0, 2]]"),
        ("diagonal_matrix([1, 2], 1)", "[[0, 1, 0], [0, 0, 2], [0, 0, 0]]"),
        ("diagonal_matrix([a, b], -1)", "[[0, 0, 0], [a, 0, 0], [0, b, 0]]"),
        ("diagonal_matrix([[1]])", "failure(argument, diagonal_matrix, 1)"),
    ]);
}

#[test]
fn norms() {
    check(&[
        ("norm([3, 4])", "5"),
        ("norm([1, 2])", "5^(1/2)"),
        ("norm([3.0, 4])", "5.0"),
        ("norm([1, -2, 3], 1)", "6"),
        ("norm([1, -2, 3], inf)", "3"),
        ("norm([1, 2, 2], 3)", "17^(1/3)"),
        ("norm([1im, 1])", "2^(1/2)"),
        ("norm([a, b])", "(abs(a)^2 + abs(b)^2)^(1/2)"),
        ("norm([[1, 2], [3, 4]], \"frobenius\")", "30^(1/2)"),
        ("norm([[1, -2], [3, 4]], 1)", "6"),
        ("norm([[1, -2], [3, 4]], inf)", "7"),
        ("norm([[3, 0], [0, 4]])", "4"),
        ("norm([1, 2], 0)", "failure(argument, norm, 2)"),
        ("norm([[1, 2], [3, 4]], 3)", "failure(argument, norm, 2)"),
    ]);
    // overflow-safe scaling of machine norms
    let mut s = Sys::new();
    s.holds("abs(norm([3e200, 4e200])/5e200 - 1) < 1e-15");
    s.holds("abs(norm([3e-200, 4e-200])/5e-200 - 1) < 1e-15");
    s.holds("abs(norm([[3e200, 0], [0, 4e200]], \"frobenius\")/5e200 - 1) < 1e-15");
}

#[test]
fn vector_calculus() {
    check(&[
        ("grad(x^2*y, [x, y])", "[2*x*y, x^2]"),
        ("grad([x*y, x + y], [x, y])", "[[y, x], [1, 1]]"),
        ("div([x^2, y^2, z^2], [x, y, z])", "2*x + 2*y + 2*z"),
        ("div([x, y], [x, y, z])", "failure(shape_mismatch, div)"),
        ("curl([y, -x, 0], [x, y, z])", "[0, 0, -2]"),
        ("curl([-y, x], [x, y])", "2"),
        ("curl([x*y*z, x^2, z], [x, y, z])", "[0, x*y, 2*x + -1*x*z]"),
        ("laplacian(x^2 + y^2 + z^2, [x, y, z])", "6"),
        ("laplacian(exp(x)*sin(y), [x, y])", "0"),
        ("laplacian([x^3, x*y], [x, y])", "[6*x, 0]"),
        ("grad(f, x)", "failure(argument, grad, 2)"),
    ]);
    let mut s = Sys::new();
    // identities: div curl = 0, curl grad = 0, div grad = laplacian
    s.run("v = [x*y^2, sin(y*z), exp(x*z)]; f = x^2*y*z + cos(x*y)");
    s.holds("div(curl(v, [x, y, z]), [x, y, z]) == 0");
    s.holds("expand(curl(grad(f, [x, y, z]), [x, y, z])) === [0, 0, 0]");
    s.holds("expand(div(grad(f, [x, y, z]), [x, y, z]) - laplacian(f, [x, y, z])) == 0");
}
