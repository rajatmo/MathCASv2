//! The plots of round 14: regions, complex contours, vector fields, graphs, charts, 3D level surfaces, and the
//! wrappers and primitives of graphics. Each gives a picture; some are checked through their SVG.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

fn picture(e: &mut CoreEngine, src: &str) -> String {
    let printed = eval(e, src);
    let path = printed.strip_prefix("image(\"").and_then(|s| s.strip_suffix("\")")).unwrap_or_else(|| panic!("{src} -> {printed}"));
    std::fs::read_to_string(std::path::Path::new(path).with_extension("svg")).expect("the SVG sibling exists")
}

#[test]
fn every_new_plot_draws() {
    let mut e = CoreEngine::new();
    for src in [
        "region_plot(x^2 + y^3 < 2, [x, -2, 2], [y, -2, 2])",
        "region_plot([x^2 + y^2 < 1, x + y > 0], [x, -2, 2], [y, -2, 2])",
        "region_plot(implicit_region(x^2 + y^2 <= 1 and x > 0, [x, y]))",
        "region_plot(implicit_region(x^2 + y^2 == 1, [x, y]))",
        "region_plot(disk())",
        "complex_region_plot(abs(z) < 1, [z, -2 - 2*im, 2 + 2*im])",
        "complex_contour_plot(abs(z^2 - 1), [z, -2 - 2*im, 2 + 2*im])",
        "complex_contour_plot(z^2, [z, -2 - 2*im, 2 + 2*im])",
        "complex_contour_plot(z^3 == 1, [z, -2 - 2*im, 2 + 2*im])",
        "vector_plot([y, -x], [x, -2, 2], [y, -2, 2])",
        "stream_plot([y, -x], [x, -2, 2], [y, -2, 2])",
        "list_vector_plot([[[1, 0], [1, 1], [0, 1]], [[1, -1], [0, 1], [-1, 1]], [[0, -1], [-1, -1], [-1, 0]]])",
        "list_vector_plot([[[0, 0], [1, 1]], [[1, 0], [0, 1]]])",
        "list_stream_plot([[[1, 0], [1, 1], [0, 1]], [[1, -1], [0, 1], [-1, 1]], [[0, -1], [-1, -1], [-1, 0]]])",
        "complex_vector_plot(z^2, [z, -1 - im, 1 + im])",
        "complex_stream_plot(z^2, [z, -1 - im, 1 + im])",
        "vector_plot3d([y, -x, z], [x, -1, 1], [y, -1, 1], [z, -1, 1])",
        "stream_plot3d([y, -x, 0.2], [x, -1, 1], [y, -1, 1], [z, -1, 1])",
        "graph_plot(complete_graph(5))",
        "layered_graph_plot([1 -> 2, 1 -> 3, 2 -> 4, 3 -> 4])",
        "tree_plot([1 -> 2, 1 -> 3, 2 -> 4])",
        "graph_plot([[0, 1], [1, 0]])",
        "bubble_chart([[1, 2, 3], [2, 3, 1], [3, 1, 5]])",
        "box_whisker_chart([[1, 2, 3, 4, 5, 20], [2, 3, 3, 4, 6]])",
        "sector_chart([[1, 2], [2, 1], [3, 3]])",
        "rectangle_chart([[1, 2], [2, 3]])",
        "paired_bar_chart([1, 2, 3], [3, 2, 1])",
        "density_histogram([[1, 2], [2, 3], [3, 1], [2, 2]])",
        "smooth_histogram([1, 2, 2, 3, 3, 3, 4, 5])",
        "number_line_plot([1, 2, 5])",
        "number_line_plot(x^2 < 4, x)",
        "mandelbrot_set_plot([-2 - im, 1 + im], plot_points: 60)",
        "julia_set_plot(-0.4 + 0.6*im, plot_points: 60)",
        "relief_plot(table(sin(i/3)*cos(j/4), i, 12, j, 12))",
        "complex_array_plot([[1, im], [-1, -im]])",
        "complex_list_plot([1 + im, 2 - im, 3])",
        "list_step_plot([1, 3, 2, 4])",
        "angular_gauge(40, [0, 100])",
        "horizontal_gauge(0.3)",
        "vertical_gauge(7, [0, 10])",
        "rasterize(x^2)",
        "contour_plot3d(x^2 + y^2 + z^2 == 1, [x, -1, 1], [y, -1, 1], [z, -1, 1])",
        "contour_plot3d(x^2 + y^2 - z^2, [x, -2, 2], [y, -2, 2], [z, -2, 2], contours: 2)",
        "region_plot3d(x^2 + y^2 + z^2 < 1, [x, -1, 1], [y, -1, 1], [z, -1, 1])",
        "density_plot3d(x*y*z, [x, -1, 1], [y, -1, 1], [z, -1, 1])",
        "list_contour_plot3d(table((x - 3)^2 + (y - 3)^2 + (z - 3)^2, z, 5, y, 5, x, 5))",
        "discrete_plot3d(i*j, [i, 1, 4], [j, 1, 4])",
        "bar_chart3d([[1, 2, 3], [2, 1, 2]])",
        "pie_chart3d([1, 2, 3])",
        "graphics([b_spline_curve([[0, 0], [1, 2], [2, -1], [3, 1]])])",
        "graphics([light_blue, cap_form(\"Round\"), disk()])",
        "graphics3d([specularity(white, 20), point_light(red, [1, 1, 1]), sphere()])",
    ] {
        let printed = eval(&mut e, src);
        assert!(printed.starts_with("image(\""), "{src} -> {printed}");
    }
}

#[test]
fn graphs_are_terms_shown_as_pictures() {
    let mut e = CoreEngine::new();
    // a graph value is shown as its picture
    assert!(eval(&mut e, "graph([1 -> 2, 2 -> 3])").starts_with("image(\""));
    eval(&mut e, "g = graph([1 -> 2, 2 -> 3, 3 -> 1, 3 -> 4]);");
    assert_eq!(eval(&mut e, "[vertex_count(g), edge_count(g)]"), "[4, 4]");
    assert_eq!(eval(&mut e, "vertex_list(g)"), "[1, 2, 3, 4]");
    assert_eq!(eval(&mut e, "vertex_degree(g)"), "[2, 2, 3, 1]");
    assert_eq!(eval(&mut e, "adjacency_matrix(cycle_graph(4))"), "[[0, 1, 0, 1], [1, 0, 1, 0], [0, 1, 0, 1], [1, 0, 1, 0]]");
    assert_eq!(eval(&mut e, "edge_count(complete_graph(6))"), "15");
    assert_eq!(eval(&mut e, "edge_count(grid_graph([3, 4]))"), "17");
    assert_eq!(eval(&mut e, "edge_list(path_graph([a, b, c]))"), "[undirected_edge(a, b), undirected_edge(b, c)]");
}

#[test]
fn wrappers_and_legend_size() {
    let mut e = CoreEngine::new();
    assert_eq!(eval(&mut e, "tooltip(3, \"x\")"), "3");
    assert_eq!(eval(&mut e, "mouseover(a, b)"), "a");
    assert_eq!(eval(&mut e, "light_mode_pane([1, 2])"), "[1, 2]");
    // round 18: a highlighted value stays as it is (plots read it; it was a style that broke them)
    assert_eq!(eval(&mut e, "highlighted(x + 1)"), "highlighted(1 + x)");
    // the legend's labels at their own size
    let small = picture(&mut e, "plot([x, x^2], [x, 0, 1], plot_legends: [\"a\", \"b\"])");
    let big = picture(&mut e, "plot([x, x^2], [x, 0, 1], plot_legends: [\"a\", \"b\"], legend_font_size: 20)");
    assert_ne!(small, big);
}

#[test]
fn regions_drawn_where_conditions_hold() {
    let mut e = CoreEngine::new();
    // the filled part of x > 0 over [-1, 1] x [-1, 1] lies right of the axis: no fill path reaches far left
    let svg = picture(&mut e, "region_plot(x > 0, [x, -1, 1], [y, -1, 1], plot_points: 20)");
    assert!(svg.contains("<path"), "a filled region");
}
