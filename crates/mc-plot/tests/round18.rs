//! The plots of round 18: chart data in every form the charts read, the new plot and chart types, date
//! plots, slices in space, colour scheme properties. Each new function gives a picture; some are checked through
//! their SVG, and undefined data stays a failure as it did.
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
    let d = "[[39.1, 18.7, 181, 3750], [39.5, 17.4, 186, 3800], [40.3, 18., 195, 3250], [36.7, 19.3, 193, 3450], [39.3, 20.6, 190, 3650]]";
    for src in [
        format!("pairwise_list_plot({d})"),
        format!("pairwise_density_histogram({d})"),
        format!("pairwise_smooth_density_histogram({d})"),
        format!("pairwise_probability_plot({d})"),
        format!("pairwise_quantile_plot({d} -> [\"a\", \"b\", \"c\", \"d\"])"),
        "parallel_axis_plot([[4, 5, 10, 7, 3], [9, 8, 2, 2, 5]])".into(),
        "radial_axis_plot([[9, 7, 9, 5, 5, 7], [6, 2, 3, 8, 8, 7]])".into(),
        "ternary_list_plot([[0.15, 0.3, 0.55], [0.4, 0.5, 0.1]])".into(),
        "stacked_list_plot([[3, 2, 1, 6], [7, 8, 8, 3]])".into(),
        "list_fit_plot([[1, 2.1], [2, 3.9], [3, 6.2], [4, 7.8]])".into(),
        "point_value_plot([[1, 1] -> 1, [2, 2] -> 2, [3, 3] -> 3])".into(),
        "point_value_plot(assoc([1, 1] -> 1, [2, 2] -> 2))".into(),
        "paired_histogram([1, 2, 2, 3], [2, 3, 3, 4], automatic, \"PDF\")".into(),
        "paired_smooth_histogram([1, 2, 2, 3], [2, 3, 3, 4])".into(),
        "smooth_density_histogram([[1, 2], [2, 3], [3, 3], [2, 1]])".into(),
        "distribution_chart([[1, 2, 2, 3, 3, 4], [2, 3, 3, 4, 5]])".into(),
        "candlestick_chart([[[2010, 1, 4], [626.95, 629.51, 624.24, 626.75]], [[2010, 1, 5], [627.18, 627.84, 621.54, 623.99]]])".into(),
        "renko_chart([10, 11, 13, 12, 15, 17, 16, 14, 11])".into(),
        "dendrogram([1, 2, 5, 9, 10, 2.5])".into(),
        "clustering_tree([\"apple\", \"apply\", \"ape\", \"band\"])".into(),
        "feature_space_plot([[1, 2, 3], [2, 3, 4], [5, 1, 2], [0, 0, 1]])".into(),
        "feature_space_plot3d([[1, 2, 3, 1], [2, 3, 4, 0], [5, 1, 2, 2], [0, 0, 1, 5]])".into(),
        "list_curve_path_plot([[0, 0], [1, 0], [1, 1], [0, 1]])".into(),
        "chromaticity_plot([\"sRGB\", \"Rec2020\"])".into(),
        "chromaticity_plot3d(\"sRGB\", \"LAB\")".into(),
        "slice_density_plot3d(exp(-(x^2 + y^2 + z^2)), \"CenterPlanes\", [x, -2, 2], [y, -2, 2], [z, -2, 2])".into(),
        "slice_contour_plot3d(x * y * z, x^2 + y^2 - z == 0, [x, -1, 1], [y, -1, 1], [z, -1, 1])".into(),
        "slice_vector_plot3d([y, -x, z], [x, -1, 1], [y, -1, 1], [z, -1, 1])".into(),
        "slice_density_plot3d(x + y + z, cylinder(), [x, -1, 1], [y, -1, 1], [z, -1, 1])".into(),
        "list_slice_density_plot3d(table(x + y - z, z, range(-1, 1, 0.5), y, range(-1, 1, 0.5), x, range(-1, 1, 0.5)), \"CenterPlanes\")".into(),
        "list_density_plot3d(table(x + y - z, z, range(-1, 1, 0.5), y, range(-1, 1, 0.5), x, range(-1, 1, 0.5)))".into(),
        "histogram3d([[1, 2], [2, 3], [3, 3], [2, 1]])".into(),
        "smooth_histogram3d([[1, 2], [2, 3], [3, 3], [2, 1]])".into(),
        "rectangle_chart3d([[1, 2, 3], [2, 1, 1]])".into(),
        "sector_chart3d([[1, 2, 3], [2, 1, 1]])".into(),
        "bubble_chart3d([[1, 2, 3, 1], [2, 1, 1, 2]])".into(),
        "array_plot3d([[[1, 0], [0, 1]], [[0, 1], [1, 0]]])".into(),
        "list_surface_plot3d(table([i, j, i * j], i, range(0, 3), j, range(0, 3)))".into(),
        "list_fit_plot3d([[1, 2, 3], [2, 1, 1], [1, 1, 2], [3, 3, 4]])".into(),
        "date_list_plot(sin(range(50) / 5), \"August 15, 2006\")".into(),
        "date_list_log_plot(range(50, 1000, 50), [2007, 1, 1])".into(),
        "date_list_step_plot([1, 2, 3, 4], [2000, 8])".into(),
        "stacked_date_list_plot([[1, 2, 3], [2, 2, 2]], [[2000, 1, 1], automatic, \"Month\"])".into(),
        "parametric_plot([r * cos(t), r * sin(t)], [t, 0, pi], [r, 1, 2])".into(),
        "color_data(\"DarkRainbow\", \"Panel\")".into(),
    ] {
        let printed = eval(&mut e, &src);
        assert!(printed.starts_with("image("), "{src} -> {printed}");
    }
}

#[test]
fn charts_read_associations_quantities_wrappers_and_gaps() {
    let mut e = CoreEngine::new();
    for src in [
        "sector_chart(assoc(\"a\" -> [2, 3], \"b\" -> [4, 6], \"c\" -> [5, 8]), plot_labels: automatic)",
        "sector_chart([[[1, 1], [1, 2]], [[7, 3], [5, 2]]])",
        "rectangle_chart([[[1, 1], missing(), [1, 1]], [[2, 2], 1 + im, [2, 2]]])",
        "rectangle_chart([[quantity(3, \"Seconds\"), quantity(2, \"Meters\")], [quantity(8, \"Seconds\"), quantity(5, \"Meters\")]])",
        "paired_bar_chart(assoc(\"a\" -> 1, \"b\" -> 2), assoc(\"a\" -> 3, \"b\" -> 1))",
        "paired_bar_chart([[1, 3, 5], [2, 4, 6]], [[2, 3, 6], [4, 5, 3]])",
        "bubble_chart([[1, 2, 2], style([2, 3, 3], red), [3, 4, 1]], bubble_sizes: [0.2, 0.7])",
        "box_whisker_chart([im, 1, 2, missing(), 3, 4, foo, 5])",
        "box_whisker_chart([[[1, 2, 3], [2, 3, 4]], [[3, 4, 5], [1, 5, 7]]])",
        "smooth_histogram([1, 2, 2, 3, 3, 3, 4, 7], automatic, \"CDF\")",
        "smooth_histogram(assoc(\"a\" -> 2, \"b\" -> 3, \"c\" -> 5, \"d\" -> 7))",
        "complex_list_plot([1 + im -> \"a\", 2 + 2 * im -> \"b\"])",
        "complex_list_plot(assoc(\"a\" -> 2 + im, \"b\" -> 3 - im))",
        "angular_gauge(quantity(68, \"Meters\"), [quantity(0, \"Meters\"), quantity(100, \"Meters\")])",
        "angular_gauge(42, [100, 0])",
        "bar_chart3d(assoc(\"one\" -> assoc(\"a\" -> 4, \"b\" -> 2), \"two\" -> assoc(\"a\" -> 2, \"b\" -> 6)))",
        "complex_vector_plot(z^2, [z, 2])",
        "complex_region_plot(labeled(abs(z) < 1, \"disk\", above), [z, 2])",
        "plot([sin(x), highlighted(cos(x), none)], [x, 0, 10])",
        "list_vector_plot(table([[x, y], [y, -x]], x, range(-1, 1, 0.5), y, range(-1, 1, 0.5)))",
        "revolution_plot3d(sqrt(t * sin(2 * u) + 1), [t, 0, 1], [u, 0, 2 * pi])",
    ] {
        let printed = eval(&mut e, src);
        assert!(printed.starts_with("image("), "{src} -> {printed}");
    }
    // a name with no value as the data stays a failure
    for src in ["box_whisker_chart(undefined_data)", "pairwise_list_plot(undefined_data -> [\"a\", \"b\"])", "histogram3d(undefined_data)"] {
        let printed = eval(&mut e, src);
        assert!(printed.starts_with("failure(argument"), "{src} -> {printed}");
    }
}

#[test]
fn date_axes_colour_schemes_and_css_colours() {
    let mut e = CoreEngine::new();
    let svg = picture(&mut e, "date_list_plot([[[2010, 1, 4], 1], [[2010, 6, 5], 2], [[2011, 3, 6], 3]])");
    assert!(svg.contains("Jan 2011") || svg.contains("2011"), "a date tick");
    assert_eq!(eval(&mut e, "color_data(\"HTML\", \"WhiteSmoke\")"), "rgb_color(0.9607843137254902, 0.9607843137254902, 0.9607843137254902)");
    assert_eq!(eval(&mut e, "color_data(\"GrayYellowTones\", \"Range\")"), "[0, 1]");
    assert_eq!(eval(&mut e, "color_data(\"Rainbow\", \"ParameterCount\")"), "1");
}
