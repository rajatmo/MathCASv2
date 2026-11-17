//! `reduce` over the complex numbers with order relations and with `re`, `im_part`, `conjugate`: the answer in
//! `re(z)` and `im_part(z)` is checked at a grid of points against the engine's own evaluation of the condition.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn ev(src: &str) -> String {
    let stop = StopFlag::new();
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    match e.eval(src, &stop) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// The reduction of `cond` in `z`, and at every grid point the answer's truth against `truth(a + b i)`, a
/// condition on the point written with `Z` for the number.
fn check_grid(cond: &str, truth: &str) {
    let src = format!(
        "ans = reduce({cond}, z, complexes); \
         pts = flatten(table(table([a, b], [a, -5/2, 5/2, 1/2]), [b, -5/2, 5/2, 1/2]), 1); \
         pts = select(pts, &(&1 != [0, 0]));          select(pts, &(rewrite(ans, [re(z) -> &1[1], im_part(z) -> &1[2]]) != rewrite({truth}, [Z -> &1[1] + &1[2]*im])))"
    );
    // the grid has points: 11 x 11
    assert_eq!(ev("length(flatten(table(table([a, b], [a, -5/2, 5/2, 1/2]), [b, -5/2, 5/2, 1/2]), 1))"), "121");
    let r = ev(&src);
    assert_eq!(r, "[]", "{cond}: points where the answer and the condition differ: {r}");
}

#[test]
fn inequality_of_a_polynomial() {
    let r = ev("reduce(x^2 < 1, x, complexes)");
    assert!(r.contains("re(x)") && r.contains("im_part(x)") && !r.contains("failure"), "{r}");
    check_grid("z^2 < 1", "im_part(expand(Z^2)) == 0 and re(expand(Z^2)) < 1");
    check_grid("z^4 <= 1", "im_part(expand(Z^4)) == 0 and re(expand(Z^4)) <= 1");
}

#[test]
fn conjugate_and_parts() {
    check_grid("z*conjugate(z) < 1", "re(expand(Z*conjugate(Z))) < 1");
    check_grid(
        "re(z^2) == 0 and im_part(z^2) >= 1",
        "re(expand(Z^2)) == 0 and im_part(expand(Z^2)) >= 1",
    );
}

#[test]
fn equations_with_a_condition_on_a_part() {
    assert_eq!(ev("reduce(x^2 == -1 and re(x) >= 0, x)"), ev("x == -im or x == im"));
    assert_eq!(ev("reduce(x^2 == -1 and im_part(x) > 0, x)"), ev("x == im"));
}

#[test]
fn rational_condition() {
    check_grid("1/z < 1", "im_part(expand(1/Z)) == 0 and re(expand(1/Z)) < 1");
    // the pole is outside the domain
    assert_eq!(ev("rewrite(reduce(1/z < 1, z, complexes), [re(z) -> 0, im_part(z) -> 0])"), "false");
}

#[test]
fn quantified_over_complex_variables() {
    // a Hermitian form: positive definite or not
    assert_eq!(
        ev("reduce(for_all([w, z], w != 0 or z != 0, 5*w*conjugate(w) + (2+im)*w*conjugate(z) + (2-im)*z*conjugate(w) + 3*z*conjugate(z) > 0), [w, z], complexes)"),
        "true"
    );
    assert_eq!(
        ev("reduce(for_all([w, z], w != 0 or z != 0, w*conjugate(w) + (2+im)*w*conjugate(z) + (2-im)*z*conjugate(w) + z*conjugate(z) > 0), [w, z], complexes)"),
        "false"
    );
}

#[test]
fn max_and_min_of_three_arguments() {
    let b = "0 <= r and r <= 1 and 0 <= g and g <= 1 and 0 <= b and b <= 1";
    let one = ev(&format!("reduce(max(r, g, b) == 0 and {b}, [r, g, b])"));
    assert_eq!(one, "r == 0 and g == 0 and b == 0");
    // the answer against the definition at a grid of points
    let src = format!(
        "ans = reduce(max(r, g, b) - min(r, g, b) == 1/2 and {b}, [r, g, b], backsubstitution: true); \
         pts = flatten(table(table(table([r0, g0, b0], [r0, 0, 1, 1/4]), [g0, 0, 1, 1/4]), [b0, 0, 1, 1/4]), 2); \
         select(pts, &(rewrite(ans, [r -> &1[1], g -> &1[2], b -> &1[3]]) != (max(&1[1], &1[2], &1[3]) - min(&1[1], &1[2], &1[3]) == 1/2)))"
    );
    assert_eq!(ev(&src), "[]");
}

#[test]
fn several_variable_answers_are_disjunctions_of_conjunctions() {
    let b = "0 <= r and r <= 1 and 0 <= g and g <= 1 and 0 <= b and b <= 1";
    let got = ev(&format!("reduce((max(r, g, b) - min(r, g, b)) / max(r, g, b) == 1 and {b}, [r, g, b])"));
    let want = ev("r == 0 and g == 0 and 0 < b and b <= 1 or r == 0 and 0 < g and g <= 1 and 0 <= b and b <= 1 \
                   or 0 < r and r <= 1 and g == 0 and 0 <= b and b <= 1 or 0 < r and r <= 1 and 0 < g and g <= 1 and b == 0");
    assert_eq!(got, want);
    // alternatives that are single atoms stay together after their condition
    let r = ev("reduce(x^4 <= 1, x, complexes)");
    assert!(r.contains("re(x) < -1 and (im_part(x) == re(x) or im_part(x) == -re(x))"), "{r}");
}
