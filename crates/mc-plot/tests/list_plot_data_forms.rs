//! Data forms of the list plots and charts: data sets that are undefined symbols left out, `data_range` for lists
//! of numbers, weighted data, several fields of `vector_plot3d`, histogram bins in units, entries that are not
//! points in `density_histogram`, images combined with graphics in `show`.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// The SVG sibling of the PNG named by `image("path")`.
fn svg(printed: &str) -> String {
    let p = printed.strip_prefix("image(\"").and_then(|s| s.strip_suffix("\")")).expect(printed);
    String::from_utf8_lossy(&std::fs::read(std::path::Path::new(p).with_extension("svg")).unwrap()).to_string()
}

fn is_image(e: &mut CoreEngine, src: &str) -> String {
    let p = eval(e, src);
    assert!(p.starts_with("image(\""), "{src} -> {p}");
    p
}

#[test]
fn undefined_data_sets_are_left_out() {
    let mut e = CoreEngine::new();
    eval(&mut e, "ys = table(sin(x), x, range(0, 2*pi, 0.1)); pairs = table([i, i], i, 30); field = table([y, -x], x, range(-2, 2, 0.5), y, range(-2, 2, 0.5)); field3 = table([x, y, z], x, range(-1, 1, 0.5), y, range(-1, 1, 0.5), z, range(-1, 1, 0.5));");
    for src in [
        "list_plot([ys, undefined_set])",
        "list_line_plot([ys, undefined_set, other_set])",
        "list_polar_plot([pairs, undefined_set], joined: true)",
        "list_vector_plot([field, undefined_set])",
        "list_stream_plot([field, undefined_set])",
        "list_vector_plot3d([field3, undefined_set])",
    ] {
        is_image(&mut e, src);
    }
    // the same picture as the defined set alone
    assert_eq!(svg(&is_image(&mut e, "list_plot([ys, undefined_set])")), svg(&is_image(&mut e, "list_plot(ys)")));
    // only symbols: nothing to draw, still a failure
    assert_eq!(eval(&mut e, "list_plot([undefined_set, other_set])"), "failure(argument, list_plot, 1)");
}

#[test]
fn data_range_and_weighted_data() {
    let mut e = CoreEngine::new();
    // the values spread from a to b: the same points as written out
    let a = is_image(&mut e, "list_plot([1, 4, 9], data_range: [0, 2*pi])");
    let b = is_image(&mut e, "list_plot([[0, 1], [pi, 4], [2*pi, 9]])");
    assert_eq!(svg(&a), svg(&b));
    let a = is_image(&mut e, "list_line_plot([[1, 2, 3], [3, 2, 1]], data_range: [0, 1])");
    let b = is_image(&mut e, "list_line_plot([[[0, 1], [0.5, 2], [1, 3]], [[0, 3], [0.5, 2], [1, 1]]])");
    assert_eq!(svg(&a), svg(&b));
    // points with uncertainty: their centres; data -> labels in a polar plot: the data
    let a = is_image(&mut e, "list_plot([vector_around([1.8, 2.4], [[0.3, 0.4], 0.5]), [2, 3]])");
    let b = is_image(&mut e, "list_plot([[1.8, 2.4], [2, 3]])");
    assert_eq!(svg(&a), svg(&b));
    let a = is_image(&mut e, "list_polar_plot([1, 2, 3] -> [\"a\", \"b\", \"c\"])");
    let b = is_image(&mut e, "list_polar_plot([1, 2, 3])");
    assert_eq!(svg(&a), svg(&b));
    // weighted data: the values in order
    for f in ["list_plot", "list_log_linear_plot", "list_step_plot"] {
        let a = is_image(&mut e, &format!("{f}(weighted_data([2, 3, 5, 7], [1, 2, 3, 4]))"));
        let b = is_image(&mut e, &format!("{f}([2, 3, 5, 7])"));
        assert_eq!(svg(&a), svg(&b), "{f}");
    }
}

#[test]
fn several_fields_units_and_missing_points() {
    let mut e = CoreEngine::new();
    is_image(&mut e, "vector_plot3d([[y, -x, z], [x, y, z]], [x, -1, 1], [y, -1, 1], [z, -1, 1])");
    // bins in feet over data in metres: the data converted (6 m = 19.69 ft lies in the bin [16, 20])
    let a = is_image(&mut e, "histogram([quantity(6, \"Meters\"), quantity(1, \"Meters\")], [quantity(0, \"Feet\"), quantity(20, \"Feet\"), quantity(4, \"Feet\")])");
    let b = is_image(&mut e, "histogram([2500/127, 1250/381], [0, 20, 4])");
    assert_eq!(svg(&a), svg(&b));
    let a = is_image(&mut e, "density_histogram([[1, 1], none, [2, 2], missing(), [3, 3], foo])");
    let b = is_image(&mut e, "density_histogram([[1, 1], [2, 2], [3, 3]])");
    assert_eq!(svg(&a), svg(&b));
    // a rasterized picture shown with graphics: its pixels from [0, 0] to [width, height]
    is_image(&mut e, "show(graphics([cyan, rectangle(scaled([0, 0]), scaled([1, 1]))]), rasterize(graphics3d(cylinder())))");
}

#[test]
fn regions_in_show_and_surfaces_without_values() {
    let mut e = CoreEngine::new();
    // a discretization left as written is drawn as its region, in the graphics of its dimension
    is_image(&mut e, "show(graphics([yellow, rectangle([-1, -1], [1, 1])]), discretize_region(infinite_line([0, 0], [1, 1])))");
    is_image(&mut e, "show(graphics3d(sphere([3, 0, 0])), discretize_region(ball()))");
    // two and three dimensions do not mix
    assert_eq!(eval(&mut e, "show(graphics(circle()), discretize_region(ball()))"), "failure(argument, show, 2)");
    // a function without a value anywhere: the empty box, as `plot` draws nothing; a failure stays one
    let a = is_image(&mut e, "plot3d(abs(undefined_fn(x + im*y)), [x, -3, 3], [y, -1, 1])");
    let b = is_image(&mut e, "plot3d(other_fn(x, y), [x, -3, 3], [y, -1, 1])");
    assert_eq!(svg(&a), svg(&b));
    assert_eq!(eval(&mut e, "plot3d(failure(argument, f, 1), [x, -3, 3], [y, -1, 1])"), "failure(argument, plot3d, 1)");
    // edges with their labels, `[a -> b, label]`, and a side for the root
    let a = is_image(&mut e, "tree_plot([1 -> 4, [3 -> 8, \"38\"], 4 -> 5, 1 -> 3], left)");
    let b = is_image(&mut e, "tree_plot([1 -> 4, 3 -> 8, 4 -> 5, 1 -> 3], left)");
    assert_eq!(svg(&a), svg(&b));
}
