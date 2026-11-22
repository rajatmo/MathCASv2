//! `cylindrical_decomposition` with a mode (closure, interior, exterior, boundary, components, the function form),
//! optimisation under quantified constraints, and complex instances of quantified conditions.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|v| v.printed).unwrap_or_else(|err| format!("error: {err}"))
}

/// The evaluation and its time.
fn timed(src: &str) -> (String, Duration) {
    let t = Instant::now();
    let r = ev(src);
    (r, t.elapsed())
}

#[test]
fn closure_interior_exterior_boundary_in_the_plane() {
    assert_eq!(
        ev(r#"cylindrical_decomposition(x^2 + y^2 < 1, [x, y], "Boundary")"#),
        "x == -1 and y == 0 or -1 < x and x < 1 and (y == -(1 - x^2)^(1/2) or y == (1 - x^2)^(1/2)) or x == 1 and y == 0"
    );
    assert_eq!(
        ev(r#"cylindrical_decomposition(x^4 > x^2 + y^2, [x, y], "Closure")"#),
        "x < -1 and -(-x^2 + x^4)^(1/2) <= y and y <= (-x^2 + x^4)^(1/2) or x == -1 and y == 0 or x == 1 and y == 0 or x > 1 and -(-x^2 + x^4)^(1/2) <= y and y <= (-x^2 + x^4)^(1/2)"
    );
    // the isolated point (0, 0) of x^4 >= x^2 + y^2 is on the boundary, not in the interior
    assert_eq!(
        ev(r#"cylindrical_decomposition(x^4 >= x^2 + y^2, [x, y], "Interior")"#),
        "x < -1 and -(-x^2 + x^4)^(1/2) < y and y < (-x^2 + x^4)^(1/2) or x > 1 and -(-x^2 + x^4)^(1/2) < y and y < (-x^2 + x^4)^(1/2)"
    );
    assert_eq!(
        ev(r#"cylindrical_decomposition(x^4 >= x^2 + y^2, [x, y], "Boundary")"#),
        "x < -1 and (y == -(-x^2 + x^4)^(1/2) or y == (-x^2 + x^4)^(1/2)) or x == -1 and y == 0 or x == 0 and y == 0 or x == 1 and y == 0 or x > 1 and (y == -(-x^2 + x^4)^(1/2) or y == (-x^2 + x^4)^(1/2))"
    );
    assert_eq!(
        ev(r#"cylindrical_decomposition(x^4 >= x^2 + y^2, [x, y], "Exterior")"#),
        "x < -1 and (y < -(-x^2 + x^4)^(1/2) or y > (-x^2 + x^4)^(1/2)) or x == -1 and (y < 0 or y > 0) or -1 < x and x < 0 or x == 0 and (y < 0 or y > 0) or 0 < x and x < 1 or x == 1 and (y < 0 or y > 0) or x > 1 and (y < -(-x^2 + x^4)^(1/2) or y > (-x^2 + x^4)^(1/2))"
    );
    // a section with a vertical asymptote: its closure is itself; a band between two hyperbolas closes at x = 0
    assert_eq!(ev(r#"cylindrical_decomposition(y*x^2 == 1, [x, y], "Closure")"#), "x < 0 and y == x^-2 or x > 0 and y == x^-2");
    assert_eq!(
        ev(r#"cylindrical_decomposition(x*y < 1 and x*y > -1, [x, y], "Closure")"#),
        "x < 0 and x^-1 <= y and y <= -x^-1 or x == 0 or x > 0 and -x^-1 <= y and y <= x^-1"
    );
    // a cusp has no interior; a quantified formula; the line
    assert_eq!(ev(r#"cylindrical_decomposition(y^2 == x^3 and x <= 1, [x, y], "Interior")"#), "false");
    assert_eq!(
        ev(r#"cylindrical_decomposition(exists(z, x^2 + y^2 + z^2 < 1), [x, y], "Boundary")"#),
        "x == -1 and y == 0 or -1 < x and x < 1 and (y == -(1 - x^2)^(1/2) or y == (1 - x^2)^(1/2)) or x == 1 and y == 0"
    );
    assert_eq!(ev(r#"cylindrical_decomposition(x^2 < 1, [x], "Closure")"#), "-1 <= x and x <= 1");
    assert_eq!(ev(r#"cylindrical_decomposition(x^2 < 1 or x == 3, [x], "Interior")"#), "-1 < x and x < 1");
}

#[test]
fn connected_components() {
    assert_eq!(
        ev(r#"cylindrical_decomposition(x^4 >= x^2 + y^2, [x, y], "Components")"#),
        "[x < -1 and -(-x^2 + x^4)^(1/2) <= y and y <= (-x^2 + x^4)^(1/2) or x == -1 and y == 0, x == 0 and y == 0, x == 1 and y == 0 or x > 1 and -(-x^2 + x^4)^(1/2) <= y and y <= (-x^2 + x^4)^(1/2)]"
    );
    assert_eq!(
        ev(r#"cylindrical_decomposition(y^2 == x*(-1 + x^4), [x, y], "Components")"#),
        "[x == -1 and y == 0 or -1 < x and x < 0 and (y == -(-x + x^5)^(1/2) or y == (-x + x^5)^(1/2)) or x == 0 and y == 0, x == 1 and y == 0 or x > 1 and (y == -(-x + x^5)^(1/2) or y == (-x + x^5)^(1/2))]"
    );
    assert_eq!(ev(r#"cylindrical_decomposition(x*y > 0, [x, y], "Components")"#), "[x < 0 and y < 0, x > 0 and y > 0]");
    assert_eq!(ev(r#"cylindrical_decomposition(x^2 + y^2 >= -1, [x, y], "Components")"#), "[true]");
    assert_eq!(ev(r#"cylindrical_decomposition(x^2 + y^2 < -1, [x, y], "Components")"#), "[]");
    assert_eq!(ev(r#"cylindrical_decomposition(x^2 + y^2 >= -1, [x, y], "Boundary")"#), "false");
}

#[test]
fn modes_outside_their_reach_stay() {
    for src in [
        r#"cylindrical_decomposition(x^2 + y^2 + z^2 < 1, [x, y, z], "Boundary")"#,
        r#"cylindrical_decomposition(x^2 + y^2 < a, [x, y], "Boundary")"#,
        r#"cylindrical_decomposition(x^2 + y^2 < 1, [x, y], "Unknown")"#,
        r#"cylindrical_decomposition(sin(x) < y, [x, y], "Closure")"#,
        r#"cylindrical_decomposition(interval([0, 1]) < y, [x, y], "Closure")"#,
        r#"cylindrical_decomposition(x^2 + y^2 < 1.5, [x, y], "Interior")"#,
    ] {
        let (r, t) = timed(src);
        assert!(r.starts_with("cylindrical_decomposition("), "{src}: {r}");
        assert!(t < Duration::from_secs(20), "{src}: {t:?}");
    }
}

#[test]
fn function_form_of_sets_that_are_everything_or_nothing() {
    assert_eq!(ev(r#"cylindrical_decomposition(for_all(z, x^2 + y^2 <= x*y*z and z^2 - x*y == 3), [x, y], "Function")"#), "false");
    assert_eq!(ev(r#"cylindrical_decomposition(x^2 + y^2 >= 0, [x, y], "Function")"#), "true");
    // a proper subset: no function object is built
    assert_eq!(
        ev(r#"cylindrical_decomposition(x^2 + y^2 <= 1, [x, y], "Function")"#),
        r#"cylindrical_decomposition(x^2 + y^2 <= 1, [x, y], "Function")"#
    );
    // the function form inside a formula stands for its set
    assert_eq!(ev(r#"cylindrical_decomposition(cylindrical_decomposition(x^2 + y^2 <= 1, [x, y], "Function") and x > 1, [x, y])"#), "false");
    // a heavy sentence decided by an instance (t = 0, z = 0 hold for every x and y)
    let (r, t) = timed(r#"cylindrical_decomposition(exists([z, t], cylindrical_decomposition(t*x*y <= 1 + t^3 and z <= t*z^2 - y*z + 2*t*x, [x, y, z, t], "Function")), [x, y], "Function")"#);
    assert_eq!(r, "true");
    assert!(t < Duration::from_secs(30), "{t:?}");
    // a heavy sentence stops at the bound on its work
    let src = r#"cylindrical_decomposition(exists(z, for_all(t, cylindrical_decomposition(t*x*y <= 1 + t^3 and z <= t*z^2 - y*z + 2*t*x, [x, y, z, t], "Function"))), [x, y], "Function")"#;
    let (r, t) = timed(src);
    assert!(r.starts_with("cylindrical_decomposition("), "{src}: {r}");
    assert!(t < Duration::from_secs(30), "{src}: {t:?}");
}

#[test]
fn optimisation_under_quantified_constraints() {
    // for_all z exists w of a quadratic in w: x^3 - x y + y^2 - 1 < 0 and 9 + 4 (x^3 - x y + y^2 - 1)(x + y) >= 0
    let c = "for_all(z, exists(w, 3*z^2*w + (x + y)*z^4 - 1 == (x^3 - x*y + y^2 - 1)*w^2))";
    let v = ev(&format!("n(min_value([(x + 7)^2 + (y - 8)^2, {c}], [x, y]), 20)"));
    assert!(v.starts_with("0.49276016994978"), "{v}");
    let p = ev(&format!("n(arg_max([-(x + 7)^2 - (y - 8)^2, {c}], [x, y]), 15)"));
    assert!(p.starts_with("[-7.4955573234262"), "{p}");
    assert_eq!(ev(&format!("resolve(for_all([x, y], implies({c}, x^3 - x*y + y^2 - 1 < 0 and 9 + 4*(x^3 - x*y + y^2 - 1)*(x + y) >= 0)), reals)")), "true");
}

#[test]
fn complex_instances_of_quantified_conditions() {
    assert_eq!(ev("find_instance(for_all(x, exists(y, a*x^2 + b*y^2 - 3*y == 1 and a*y != 0)), [a, b])"), "[[a -> -1, b -> -1]]");
    assert_eq!(ev("find_instance(exists(y, a*y^2 == 1), [a])"), "[[a -> -1]]");
    assert_eq!(ev("find_instance(for_all(x, exists(y, x*y == 1)), [a])"), "[]");
}
