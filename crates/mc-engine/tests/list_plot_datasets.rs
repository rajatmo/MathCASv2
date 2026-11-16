//! List plots of arrays and vector fields given several data sets at once.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn is_plot(s: &str) -> bool {
    !s.starts_with("failure") && !s.starts_with("error") && (s.starts_with("image(") || s.starts_with("graphics"))
}

#[test]
fn several_data_sets_are_drawn_together() {
    let mut e = CoreEngine::new();
    ev(&mut e, "m1 = table(sin(x)*cos(y), y, range(-2, 2, 0.5), x, range(-2, 2, 0.5))");
    ev(&mut e, "m2 = table(sin(x + y), y, range(-2, 2, 0.5), x, range(-2, 2, 0.5))");
    for src in [
        "list_density_plot([m1, m2])",
        "list_contour_plot([m1, m2])",
        // scattered values [x, y, f], two sets
        "list_density_plot([flatten(table([x, y, x*y], x, 4, y, 4), 1), flatten(table([x + 5, y, x - y], x, 4, y, 4), 1)])",
        "list_vector_plot([table([1, y], y, 4, x, 4), table([x, 1], y, 4, x, 4)])",
        "list_stream_plot([table([1, y], y, 4, x, 4), table([x, 1], y, 4, x, 4)])",
        // pairs [[x, y], [vx, vy]], two sets
        "list_vector_plot([[[[0, 0], [1, 0]], [[1, 0], [0, 1]], [[0, 1], [1, 1]]], [[[2, 2], [1, 0]], [[3, 2], [0, 1]], [[2, 3], [1, 1]]]])",
        "list_vector_plot3d([table([x, y, z], x, 3, y, 3, z, 3), table([-x, y, z], x, 3, y, 3, z, 3)])",
    ] {
        let got = ev(&mut e, src);
        assert!(is_plot(&got), "{src}: {got}");
    }
    // one data set still reads as one: a matrix of numbers, a matrix of vectors, an array of vectors
    for src in ["list_density_plot(m1)", "list_vector_plot(table([1, y], y, 4, x, 4))", "list_vector_plot3d(table([x, y, z], x, 3, y, 3, z, 3))"] {
        let got = ev(&mut e, src);
        assert!(is_plot(&got), "{src}: {got}");
    }
}
