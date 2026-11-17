//! The tidy forms of calls that stay unevaluated (semantics 3.2 and 7.4): orderless built-ins flattened and sorted,
//! even functions without a negative coefficient, whole periods dropped, `piecewise` without dead or repeated cases,
//! regions of the line as disjoint segments. Each form is checked against the call it came from by an independent
//! route: values substituted, or numeric values at points.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn orderless_builtins_are_flattened_and_sorted() {
    assert_eq!(ev("max(x, y, max(x, z))"), "max(x, y, z)");
    assert_eq!(ev("min(x, y, min(x, z))"), "min(x, y, z)");
    assert_eq!(ev("bit_or(y, x)"), "bit_or(x, y)");
    assert_eq!(ev("bit_and(y, x)"), "bit_and(x, y)");
    assert_eq!(ev("bit_xor(y, x)"), "bit_xor(x, y)");
    assert_eq!(ev("xor(xor(x, y), z)"), "xor(x, y, z)");
    assert_eq!(ev("xor(y, x)"), "xor(x, y)");
    assert_eq!(ev("gcd(y, x)"), "gcd(x, y)");
    assert_eq!(ev("lcm(x, lcm(y, 2))"), "lcm(2, x, y)");
    assert_eq!(ev("equivalent(b, a)"), "equivalent(a, b)");
    // the values do not depend on the order or the grouping
    for (x, y, z) in [(3, 7, 5), (-2, 9, 0), (12, 18, 30)] {
        let s = format!("[x -> {x}, y -> {y}, z -> {z}]");
        for f in ["max", "min", "bit_or", "bit_and", "bit_xor", "gcd", "lcm"] {
            assert_eq!(ev(&format!("rewrite({f}(x, {f}(z, y)), {s}) == {f}({x}, {y}, {z})")), "true", "{f} at {s}");
        }
    }
    assert_eq!(ev("rewrite(xor(xor(x, y), z), [x -> true, y -> false, z -> true])"), "false");
}

#[test]
fn towers_and_even_functions() {
    assert_eq!(ev("pow(x, y, z)"), "x^y^z");
    assert_eq!(ev("pow(2, 3, 2)"), "512");
    for f in ["unit_box", "unit_triangle", "dirac_delta", "heaviside_pi", "heaviside_lambda"] {
        assert_eq!(ev(&format!("{f}(-x)")), format!("{f}(x)"));
        // the same values at points
        for v in ["1/3", "3/4", "2"] {
            assert_eq!(ev(&format!("{f}(-{v}) == {f}({v})")), "true", "{f} at {v}");
        }
    }
    assert_eq!(ev("unit_box(-2*x)"), "unit_box(2*x)");
}

#[test]
fn whole_periods_drop() {
    assert_eq!(ev("jacobi_zn(u + 2*elliptic_k(m), m)"), "jacobi_zn(u, m)");
    assert_eq!(ev("modular_lambda(tau + 2)"), "modular_lambda(tau)");
    assert_eq!(ev("inverse_gudermannian(x + 2*pi)"), "inverse_gudermannian(x)");
    // numeric checks of the periods
    let close = |a: &str, b: &str| {
        let d = ev(&format!("abs(n({a}) - n({b})) < 1.0e-10"));
        assert_eq!(d, "true", "{a} against {b}");
    };
    close("jacobi_zn(0.3 + 2*elliptic_k(1/3), 1/3)", "jacobi_zn(0.3, 1/3)");
    close("modular_lambda(1/5 + im + 2)", "modular_lambda(1/5 + im)");
    close("inverse_gudermannian(0.4 + 2*pi)", "inverse_gudermannian(0.4)");
}

#[test]
fn piecewise_drops_dead_cases_and_joins_equal_ones() {
    assert_eq!(ev("piecewise([[e1, d1], [e2, d2], [e3, true], [e4, d4], [e5, d5]], eu)"), "piecewise([[e1, d1], [e2, d2]], e3)");
    assert_eq!(ev("piecewise([[e1, d1], [e2, d2], [e3, d2 and d3], [e4, d4]], eu)"), "piecewise([[e1, d1], [e2, d2], [e4, d4]], eu)");
    assert_eq!(ev("piecewise([[e1, d1], [e23, d2], [e23, d3], [e4, d4]], eu)"), "piecewise([[e1, d1], [e23, d2 or d3], [e4, d4]], eu)");
    // the tidy form has the same value for every truth assignment
    let src = "piecewise([[1, p], [2, q], [2, r], [3, q and r], [4, true], [5, s]], 6)";
    let tidy = ev(src);
    for p in ["true", "false"] {
        for q in ["true", "false"] {
            for r in ["true", "false"] {
                let s = format!("[p -> {p}, q -> {q}, r -> {r}, s -> true]");
                assert_eq!(ev(&format!("rewrite({src}, {s})")), ev(&format!("rewrite({tidy}, {s})")), "at {s}");
            }
        }
    }
}

#[test]
fn regions_of_the_line_and_other_forms() {
    assert_eq!(ev("region_union(line([[1], [2]]), line([[3], [4]]))"), "line([[[1], [2]], [[3], [4]]])");
    assert_eq!(ev("region_union(line([[1], [3]]), line([[2], [4]]))"), "line([[[1], [4]]])");
    assert_eq!(ev("region_difference(line([[1], [4]]), line([[2], [3]]))"), "line([[[1], [2]], [[3], [4]]])");
    assert_eq!(ev("region_measure(region_union(line([[1], [3]]), line([[2], [4]])))"), "3");
    assert_eq!(
        ev("implicit_region(x^2 + y^2 <= 1 or x == y, [[x, -2, 2], y])"),
        "implicit_region((x^2 + y^2 <= 1 or x == y) and -2 <= x and x <= 2, [x, y])"
    );
    assert_eq!(ev("indexed(x, 1)"), "indexed(x, [1])");
    assert_eq!(ev("indexed([a, b], 2)"), "b");
    assert_eq!(ev("dot([[1, 2], [3, 4]], [[1, 0], [0, 1]], v)"), "dot([[1, 2], [3, 4]], v)");
    assert_eq!(ev("around(4.1836, scaled(0.05))"), "around(4.1836, 0.20918000000000003)");
    assert_eq!(ev("around(3.56, scaled(quantity(30, \"Percent\")))"), "around(3.56, 1.068)");
    assert_eq!(ev("around(normal_distribution(5, 0.3))"), "around(5.0, 0.3)");
}
