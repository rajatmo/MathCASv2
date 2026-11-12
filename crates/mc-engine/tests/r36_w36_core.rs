//! Round 36: regression tests of the wrong answers fixed in the core-language, strings, plots/graphics and
//! data/IO areas, each checked by a second route.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate on a detached thread with a big stack (one worker thread, fresh session); `TIMEOUT` after 60 s.
fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let stop = StopFlag::new();
            let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
            let out = match e.eval(&s, &stop) {
                Ok(v) => v.printed,
                Err(err) => format!("error: {err}"),
            };
            let _ = tx.send(out);
        })
        .expect("spawn");
    rx.recv_timeout(std::time::Duration::from_secs(60)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

#[test]
fn nested_and_or_are_flat() {
    assert_eq!(ev("full_form(x and (y and z))"), "`and`(x, y, z)");
    assert_eq!(ev("full_form(x or (y or (z or w)))"), "`or`(x, y, z, w)");
    assert_eq!(ev("full_form((x and y) and (z and w))"), "`and`(x, y, z, w)");
    // second route: the flat call and the nested one agree on every assignment of the truth values
    assert_eq!(
        ev("every(tuples([true, false], 3), v => ((v[1] and (v[2] and v[3])) === (v[1] and v[2] and v[3])))"),
        "true"
    );
    // a deciding or neutral value inside a nested call still decides or drops out
    assert_eq!(ev("x and (false and z)"), "false");
    assert_eq!(ev("x and (true and z)"), "x and z");
    assert_eq!(ev("x or (true or z)"), "true");
}

#[test]
fn composition_is_flat_and_drops_identity() {
    assert_eq!(ev("composition(f, composition(g, h), identity)"), "composition(f, g, h)");
    assert_eq!(ev("right_composition(identity, f, identity, g)"), "right_composition(f, g)");
    assert_eq!(ev("composition()"), "identity");
    assert_eq!(ev("right_composition(identity)"), "identity");
    assert_eq!(ev("composition(f)"), "f");
    assert_eq!(
        ev("fold_list(right_composition, identity, [a, b, c, d_u])"),
        "[identity, a, right_composition(a, b), right_composition(a, b, c), right_composition(a, b, c, d_u)]"
    );
    // second route: the flattened composition applies the functions in the same order
    assert_eq!(ev("composition(f, composition(g, h), identity)(x)"), "f(g(h(x)))");
    assert_eq!(ev("right_composition(f, right_composition(g, h))(x)"), "h(g(f(x)))");
    assert_eq!(ev("composition(sin, composition(cos, identity))(0)"), "sin(1)");
}

#[test]
fn operator_forms_of_nest_and_fold_while() {
    assert_eq!(ev("nest(sin, 3)(0)"), "0");
    assert_eq!(ev("nest(f, 3)(x)"), "f(f(f(x)))");
    assert_eq!(ev("nest_list(f, 2)(x)"), "[x, f(x), f(f(x))]");
    // second route: the three-argument form
    assert_eq!(ev("nest(sin, 3)(1) === nest(sin, 1, 3)"), "true");
    assert_eq!(ev("fold_while(subtract, is_positive)(12, [1, 2, 3, 4, 5, 6])"), "-3");
    assert_eq!(ev("fold_while(subtract, is_positive, 1, -1)(12, [1, 2, 3, 4, 5, 6])"), "2");
    // by hand: 12 - 1 - 2 - 3 - 4 = 2 is the last positive value, 2 - 5 = -3 the first that is not
    assert_eq!(ev("fold_while_list(subtract, 12, [1, 2, 3, 4, 5, 6], is_positive)"), "[12, 11, 9, 6, 2, -3]");
    assert_eq!(ev("fold_while(subtract, 12, [1, 2, 3, 4, 5, 6], is_positive)"), "-3");
}

#[test]
fn sequence_fold_with_a_window_as_wide_as_the_state() {
    // n = k: no element of the list is consumed by a step, so f is applied once per element and once more
    assert_eq!(ev("sequence_fold(f, [x], [a, b, c, d_u], 1)"), "f(f(f(f(f(x)))))");
    assert_eq!(ev("sequence_fold(f, [x], [a, b, c, d_u], 1) === nest(f, x, 5)"), "true");
    assert_eq!(
        ev("sequence_fold(f, [x, y, z], [a, b, c], 3)"),
        "f(f(x, y, z), f(y, z, f(x, y, z)), f(z, f(x, y, z), f(y, z, f(x, y, z))))"
    );
    // a window narrower than the state is not defined: the call stays
    assert_eq!(ev("sequence_fold(f, [x, y, z], [a, b, c], 2)"), "sequence_fold(f, [x, y, z], [a, b, c], 2)");
    // the default window (n = k + 1) is unchanged
    assert_eq!(ev("sequence_fold(f, [x], [a, b, c])"), "f(f(f(x, a), b), c)");
}

#[test]
fn polygons_in_space_and_canonical_index_order() {
    // a triangle in space is simple (and convex); a bow tie in a tilted plane is not simple
    assert_eq!(ev("is_simple_polygon(polygon([[1, 0, 0], [0, 1, 0], [0, 0, 1]]))"), "true");
    assert_eq!(ev("is_convex_polygon(polygon([[1, 0, 0], [0, 1, 0], [0, 0, 1]]))"), "true");
    assert_eq!(ev("is_simple_polygon(polygon([[0, 0, 0], [2, 2, 2], [2, 0, 0], [0, 2, 2]]))"), "false");
    assert_eq!(ev("is_simple_polygon(polygon([[0, 0, 0], [2, 0, 2], [2, 2, 2], [0, 2, 0]]))"), "true");
    // four points that do not lie in a plane do not form a polygon of one plane
    assert_eq!(ev("is_simple_polygon(polygon([[0, 0, 0], [1, 0, 0], [1, 1, 0], [0, 1, 1]]))"), "false");
    // second route: the same bow tie lying in the plane z = 0 is not simple either
    assert_eq!(ev("is_simple_polygon(polygon([[0, 0], [2, 2], [2, 0], [0, 2]]))"), "false");
    // the cycle of indices starts at its lowest index
    assert_eq!(
        ev("canonicalize_polygon(polygon([[1, 0], [0, sqrt(3)], [-1, 0]]))"),
        "polygon([[-1, 0], [0, 3^(1/2)], [1, 0]], [1, 3, 2])"
    );
}

#[test]
fn derivative_of_an_inactive_integral_is_leibniz() {
    // indefinite: the integrand
    assert_eq!(ev("d(inactive(integrate)(1 / (1 + x), x), x)"), "(1 + x)^-1");
    // definite, the variable of integration named like the outer one: the limits only
    assert_eq!(ev("d(inactive(integrate)(f(x), [x, a(x), b(x)]), x) === f(b(x)) * b'(x) - f(a(x)) * a'(x)"), "true");
    // limit and integrand both depend on x: the inert integral of the partial derivative stays inert
    assert_eq!(
        ev("d(inactive(integrate)((t + x)^2, [t, 0, x]), x)"),
        "4*x^2 + inactive(integrate)(2*(t + x), [t, 0, x])"
    );
    // second route: activating gives the derivative of the closed form 7 x^3 / 3
    assert_eq!(ev("activate(d(inactive(integrate)((t + x)^2, [t, 0, x]), x))"), "7*x^2");
    assert_eq!(ev("d(7 * x^3 / 3, x)"), "7*x^2");
    // the active integral with a bound variable named like the outer one follows the same rule
    assert_eq!(ev("d(integrate(f(x), [x, a(x), b(x)]), x) === f(b(x)) * b'(x) - f(a(x)) * a'(x)"), "true");
    // a closed form to compare with: d/dx of the integral of sin(x t) over t in [0, x] is (1 - cos x^2)/x differentiated
    assert_eq!(ev("simplify(d(integrate(sin(x * t), [t, 0, x]), x) - d((1 - cos(x^2)) / x, x))"), "0");
}

#[test]
fn accessors_of_an_uncertain_number_and_the_two_argument_operator_form() {
    assert_eq!(ev(r#"around(1.73, 0.012)("Value")"#), "1.73");
    assert_eq!(ev(r#"around(1.73, 0.012)("Uncertainty")"#), "0.012");
    assert_eq!(ev(r#"[order("a", "b"), operator_applied(alphabetic_order)("a", "b")]"#), "[1, -1]");
    assert_eq!(ev(r#"operator_applied(f)(x, y)"#), "f(y, x)");
    assert_eq!(ev(r#"operator_applied(f)(x)(y)"#), "f(y, x)");
}

#[test]
fn blend_of_lab_colours_is_mixed_in_lab() {
    let stops = "[lab_color(0.25, 0.4, -0.65), lab_color(0.55, 0.65, 0.65), lab_color(0.9, 0.05, 0.4)]";
    assert_eq!(ev(&format!("blend({stops}, 0)")), "lab_color(0.25, 0.4, -0.65)");
    assert_eq!(ev(&format!("blend({stops}, 1)")), "lab_color(0.9, 0.05, 0.4)");
    // the middle stop at t = 1/2
    assert_eq!(ev(&format!("blend({stops}, 1/2)")), "lab_color(0.55, 0.65, 0.65)");
    // a quarter of the first segment, by hand: 0.25 + (0.55 - 0.25) / 2 and so on
    assert_eq!(ev(&format!("blend({stops}, 1/4)")), "lab_color(0.4, 0.525, 0.0)");
    // mixed models still go through RGB
    assert_eq!(ev("head(blend([lab_color(0.25, 0.4, -0.65), red], 1/2))"), "rgb_color");
}

#[test]
fn alphabetic_order_folds_ligatures_and_block_map_cuts_every_dimension() {
    assert_eq!(ev(r#"alphabetic_sort(["A", "a", "æ", "Æ", "i", "I"], "English")"#), r#"["a", "A", "æ", "Æ", "i", "I"]"#);
    assert_eq!(
        ev(r#"alphabetic_sort(["A", "a", "æ", "Æ", "i", "I"], "English", case_ordering: "LowerFirst") === alphabetic_sort(["A", "a", "æ", "Æ", "i", "I"], "English")"#),
        "true"
    );
    assert_eq!(ev(r#"alphabetic_sort(["Encyclopædia", "Encyclopedia", "Encyclopaedia"])"#), r#"["Encyclopaedia", "Encyclopædia", "Encyclopedia"]"#);
    assert_eq!(ev("block_map(f, [[1, 2, 3, 4], [5, 6, 7, 8]], [1, 2])"), "[[f([[1, 2]]), f([[3, 4]])], [f([[5, 6]]), f([[7, 8]])]]");
    // second route: the blocks are those of partition, mapped at their depth
    assert_eq!(ev("block_map(f, [[1, 2, 3, 4], [5, 6, 7, 8]], [2, 2]) === map(f, partition([[1, 2, 3, 4], [5, 6, 7, 8]], [2, 2]), [2])"), "true");
}

#[test]
fn neighbouring_transformations_compose_into_one() {
    assert_eq!(
        ev("max(map(abs, n(composition(rotation_transform(0.3), rotation_transform(0.5))([1, 2]) - rotation_transform(0.3)(rotation_transform(0.5)([1, 2]))))) < 10^(-12)"),
        "true"
    );
    assert_eq!(
        ev("simplify(composition(translation_transform([2, 2]), f, rotation_transform(t), rotation_transform(-t)))"),
        "composition(transformation_function([[1, 0, 2], [0, 1, 2], [0, 0, 1]]), f, transformation_function([[1, 0, 0], [0, 1, 0], [0, 0, 1]]))"
    );
}
