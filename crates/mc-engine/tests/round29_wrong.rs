//! Round 29: plot crashes and wrong values, one test per fix.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate on a detached thread with a big stack (one worker thread, fresh session); `TIMEOUT` after 60 s.
fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::spawn(move || {
        let stop = StopFlag::new();
        let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
        let out = match e.eval(&s, &stop) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        let _ = tx.send(out);
    });
    rx.recv_timeout(std::time::Duration::from_secs(60)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

#[test]
fn plot_of_a_curve_with_huge_values_does_not_overflow_the_stack() {
    // weierstrass_p has a pole in the interval: the polyline's far points are cut before stroking
    let out = ev("plot([weierstrass_p(x, [1, 2]), 2], [x, -6, 6])");
    assert!(out.starts_with("image("), "{out}");
}

#[test]
fn plots_of_a_two_dimensional_list_interpolation_with_four_nodes() {
    // `n` of the body turned the interpolation order into 3.0, which is no list length
    for f in ["plot3d", "contour_plot", "density_plot"] {
        let out = ev(&format!("g = list_interpolation([[1, 2, 3, 4], [2, 3, 4, 5]]); {f}(g(x, y), [x, 1, 2], [y, 1, 4])"));
        assert!(out.starts_with("image("), "{f}: {out}");
    }
}

#[test]
fn extract_evaluates_the_parts_of_a_held_term() {
    assert_eq!(ev("expr = quote([1 + 2, g(3 + 4, 2 * 3), f(1 + g(2 + 3))]); extract(expr, position(expr, _: Head[add]))"), "[3, 7, 5, 1 + g(5)]");
    assert_eq!(ev("extract(quote(f(1 + 2)), [1, 1])"), "3");
    assert_eq!(ev("extract(quote(f(1 + 2)), [1, 1], quote)"), "quote(1 + 2)");
}

#[test]
fn simplify_reduces_golden_ratio_expressions_to_numbers() {
    assert_eq!(ev("simplify(1 + 1 / golden_ratio - golden_ratio)"), "0");
    assert_eq!(ev("simplify(golden_ratio^2 - golden_ratio - 1)"), "0");
    assert_eq!(ev("simplify(golden_ratio + 1)"), "1 + golden_ratio");
}

#[test]
fn sqrt_of_a_square_is_the_base_for_positive_real_part() {
    assert_eq!(ev("simplify(sqrt(x^2), assumptions: re(x) > 0)"), "x");
    assert_eq!(ev("possible_zero_q(sqrt(x^2) - x, assumptions: re(x) > 0)"), "true");
    // not for exponents beyond 2, nor without the assumption
    assert_eq!(ev("refine((x^3)^(1/3), re(x) > 0)"), "(x^3)^(1/3)");
    assert_eq!(ev("simplify(sqrt(x^2))"), "(x^2)^(1/2)");
}

#[test]
fn color_convert_to_xyz_relative_to_a_white_point() {
    // the white of sRGB goes to the given white point (a named illuminant or an XYZ colour)
    let out = ev("color_convert(white, \"XYZ\", white_point: xyz_color(0.98, 1, 0.95))");
    assert!(out.starts_with("xyz_color(0.98, 1.0, 0.95"), "{out}");
    let out = ev("color_convert([1, 1, 1], \"XYZ\", white_point: \"F2\")");
    assert!(out.starts_with("xyz_color(0.99144661461802"), "{out}");
}

#[test]
fn names_own_values_and_echo_timing() {
    assert_eq!(ev("abc_one = 1; abc_two = 2; names(\"abc_*\")"), "[\"abc_one\", \"abc_two\"]");
    assert_eq!(ev("xv = 5; own_values(xv)"), "[xv :> 5]");
    assert_eq!(ev("own_values(never_assigned)"), "[]");
    assert_eq!(ev("echo_timing(2 + 3)"), "5");
}
