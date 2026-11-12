//! `reduce`, `root` objects under `n`, and `n_solve`, against expected values.
mod common;
use common::{check, Sys};

#[test]
fn reduce_exact() {
    check(&[
        ("reduce(x^2 - 3*x + 2 > 0, x, reals)", "x < 1 or 2 < x"),
        ("reduce(x^2 < 2, x, reals)", "-2^(1/2) < x and x < 2^(1/2)"),
        ("reduce((x - 1)/(x + 2) >= 0, x, reals)", "x < -2 or 1 <= x"),
        (
            "reduce((x^3 - 2*x)/(x^2 - 1) >= 0, x, reals)",
            "-2^(1/2) <= x and x < -1 or 0 <= x and x < 1 or 2^(1/2) <= x",
        ),
        ("reduce(x^4 - 5*x^2 + 4 < 0, x, reals)", "-2 < x and x < -1 or 1 < x and x < 2"),
        ("reduce(x^2 + 1 > 0, x, reals)", "true"),
        ("reduce(x^2 + 1 < 0, x, reals)", "false"),
        ("reduce(x^3 - x - 1 > 0 and x < 3, x, reals)", "root(&(-1 - &1 + &1^3), 1) < x and x < 3"),
        ("reduce((x - 1)^2 <= 0, x)", "x == 1"),
        ("reduce(x^2 == 2, x, reals)", "x == -2^(1/2) or x == 2^(1/2)"),
        ("reduce(not(x > 1) or x == 5, x, reals)", "x <= 1 or x == 5"),
        ("reduce([x > 0, x < 1, x != 1/2], x, reals)", "0 < x and x < 1/2 or 1/2 < x and x < 1"),
        ("reduce(1/x > 0, x, reals)", "0 < x"),
        // a parameter: the cylindrical form from the decomposition (item 11A)
        ("reduce(x^2 > a, x, reals)", "a < 0 or a == 0 and (x < 0 or x > 0) or a > 0 and (x < -a^(1/2) or x > a^(1/2))"),
        ("reduce(x > 0, x, complexes)", "failure(argument, reduce, \"reduce works over the reals\")"),
    ]);
}

#[test]
fn reduce_numeric_boundaries() {
    // expected: -1.38879198440725 <= x <= 0.33473414194335 or x >= 1.21464804269846
    let mut s = Sys::new();
    let out = s.run("n(reduce(x^5 - 3*x + 1 >= 0, x, reals))");
    assert_eq!(out, "-1.3887919844072543 <= x and x <= 0.3347341419433527 or 1.2146480426984618 <= x");
}

#[test]
fn root_objects() {
    let mut s = Sys::new();
    // expected: the first root of x^3 - 2x - 5, 40 digits
    let v = s.run("n(root(x => x^3 - 2*x - 5, 1), 40)");
    assert!(v.starts_with("2.094551481542326591482386540579302963857"), "{v}");
    assert_eq!(s.run("n(root(x => x^5 - x - 1, 1))"), "1.1673039782614187");
    assert_eq!(s.run("n(root(x => x^5 - x - 1, 2))"), "complex(-0.7648844336005847, -0.3524715460317262)");
    assert_eq!(s.run("n(root(x => x^5 - x - 1, 5))"), "complex(0.18123244446987535, 1.0839541013177107)");
    // a polynomial in one variable as the function; degree one evaluates; exact objects stay
    assert_eq!(s.run("n(root(y^2 - 2, 2))"), "1.4142135623730951");
    assert_eq!(s.run("root(x => 3*x - 1, 1)"), "1/3");
    assert_eq!(s.run("root(x => x^3 - 2, 1)"), "root(&(-2 + &1^3), 1)");
    assert_eq!(s.run("root(x => x^3 - 2, 4)"), "failure(argument, root, \"the index of a root object is out of range\")");
    // multiple roots and the order: real ascending, then complex by real and imaginary part
    assert_eq!(
        s.run("map(k => n(root(x => (x - 1)^2*(x^2 + 1), k)), range(1, 4))"),
        "[1.0, 1.0, complex(0.0, -1.0), complex(0.0, 1.0)]"
    );
    // high precision: x^2 - 2 to 100 digits
    let v = s.run("n(root(x => x^2 - 2, 2), 100)");
    assert!(v.starts_with("1.41421356237309504880168872420969807856967187537694807317667973799073247846210703885038753432764157"), "{v}");
}

#[test]
fn n_solve_values() {
    let mut s = Sys::new();
    // expected (20 digits): -1.2492232925556733296, 0.33348623920829341660, 1.1331966782955089820,
    // -0.65094641823205501287 +- 1.0460344022428004580 i, 0.54221660575799047838 +- 1.0495215220975016165 i
    let out = s.run("n_solve(x^7 - 3*x + 1 == 0, x, options(digits -> 20))");
    for want in [
        "-1.24922329255567332958",
        "0.33348623920829341660",
        "1.13319667829550898197",
        "complex(-0.65094641823205501287",
        "1.04603440224280045796",
        "complex(0.54221660575799047837",
        "1.04952152209750161649",
    ] {
        assert!(out.contains(want), "{want} in {out}");
    }
    assert_eq!(
        s.run("n_solve(x^3 - 1 == 0, x)"),
        "[[x -> complex(-0.5, -0.8660254037844386)], [x -> complex(-0.5, 0.8660254037844386)], [x -> 1.0]]"
    );
    assert_eq!(s.run("n_solve(x^2 - 2.0 == 0, x)"), "[[x -> -1.4142135623730951], [x -> 1.4142135623730951]]");
    assert_eq!(s.run("n_solve([x + y == 3, x - y == 1], [x, y])"), "[[x -> 2.0, y -> 1.0]]");
    assert_eq!(
        s.run("n_solve(x^2 == 2, x, options(precision -> 3))"),
        "failure(argument, n_solve, \"n_solve does not take the option precision\")"
    );
}
