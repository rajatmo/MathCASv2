//! Round 39: data forms of plots and charts, primitives of pictures, fill rules of closed curves, arrays of shapes
//! with whole dimensions, tensor-product interpolants at symbolic points.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn picture(src: &str) {
    let r = ev(src);
    assert!(r.starts_with("image(") || r.starts_with("graphics("), "{src} -> {r}");
}

#[test]
fn fill_rules_of_a_closed_curve() {
    let star = "[[0.35, 0.2], [0.9, 0.75], [0.1, 0.55], [0.9, 0.35], [0.42, 0.9]]";
    // the even-odd rule named is the crossing polygon
    assert_eq!(ev(&format!("winding_polygon({star}, \"EvenOddRule\") == crossing_polygon({star})")), "true");
    assert_eq!(ev(&format!("winding_polygon({star}, \"NonzeroRule\") == winding_polygon({star})")), "true");
    // the curve turns anticlockwise: every enclosed point has a positive winding number, none a negative one
    assert_eq!(ev(&format!("abs(area(winding_polygon({star}, \"PositiveRule\")) - area(winding_polygon({star}))) < 10^-12")), "true");
    assert_eq!(ev(&format!("winding_polygon({star}, \"NegativeRule\")")), "empty_region(2)");
    // the same curve run clockwise: the negative rule gives the region, of the same area
    assert_eq!(ev(&format!("abs(area(winding_polygon(reverse({star}), \"NegativeRule\")) - area(winding_polygon({star}))) < 10^-12")), "true");
    // an unknown rule name stays unevaluated
    assert!(ev(&format!("winding_polygon({star}, \"NoRule\")")).starts_with("winding_polygon("));
}

#[test]
fn options_of_a_simple_curve_are_kept() {
    assert_eq!(
        ev("winding_polygon([[1, 0], [0, 2], [-1, 0]], vertex_colors: [red, green, blue])"),
        "polygon([[1, 0], [0, 2], [-1, 0]], vertex_colors: [red, green, blue])"
    );
    picture("graphics(crossing_polygon([[1, 0], [0, sqrt(3)], [-1, 0]], vertex_colors: [red, green, blue]))");
    // a self-crossing curve makes new corners: the options have no corners to go with, the call stays
    assert!(ev("winding_polygon([[0, 0], [1, 1], [1, 0], [0, 1]], vertex_colors: [red, green, blue, red])").starts_with("winding_polygon("));
}

#[test]
fn shapes_with_whole_dimensions() {
    assert_eq!(ev("box_matrix([1, 1], [5, all])"), "[[0, 0, 0], [1, 1, 1], [1, 1, 1], [1, 1, 1], [0, 0, 0]]");
    // a radius all reaches the edges: the whole array is ones, as box_matrix of the full radii
    assert_eq!(ev("box_matrix([2, all], [all, 6]) == constant_array(1, [5, 6])"), "true");
    assert_eq!(ev("box_matrix(all, [3, 4, 2]) == constant_array(1, [3, 4, 2])"), "true");
    // the cross of radius [1, all] in a 3 x 5 array: the middle row and the middle column
    assert_eq!(ev("cross_matrix([1, all], [all, 5])"), "[[0, 0, 1, 0, 0], [1, 1, 1, 1, 1], [0, 0, 1, 0, 0]]");
    assert_eq!(ev("cross_matrix([1, 2], [3, 5]) == cross_matrix([1, all], [all, 5])"), "true");
    picture("array_plot(cross_matrix([3, all], [all, 8]), mesh: all)");
}

#[test]
fn tensor_interpolants_at_symbolic_points() {
    let def = "fun = interpolation(flatten(outer(((t_args...) => [t_args, sin(pi * t_args[1]^2 * t_args[2])]), [0., 0.1, 0.25, 0.5, 1.], [-1., -0.5, 0., 0.5, 1.]), 1)); ";
    // at the nodes the interpolant is the data
    assert_eq!(ev(&format!("{def}abs(fun(0.5, 0.5) - sin(pi/8)) < 10^-12")), "true");
    assert_eq!(ev(&format!("{def}abs(fun(0.25, -1.) - sin(-pi/16)) < 10^-12")), "true");
    // at a symbolic point it waits, then gives the value at the number
    assert_eq!(ev(&format!("{def}g = fun(s, t); is_number(rewrite(g, [s -> 0.5, t -> 0.5]))")), "true");
    assert_eq!(ev(&format!("{def}abs(rewrite(fun(s, t), [s -> 0.5, t -> 0.5]) - fun(0.5, 0.5)) < 10^-15")), "true");
    picture(&format!("{def}contour_plot(fun(s, t), [s, 0, 1], [t, -1, 1])"));
    // malformed data still fails
    assert_eq!(ev("interpolation([[0, 0], [0, 1], [1, 2]])"), "failure(argument, interpolation, 1)");
    assert_eq!(ev("interpolation([a, b, c])"), "failure(argument, interpolation, 1)");
}

#[test]
fn data_forms_of_plots_and_charts() {
    picture("pie_chart(weighted_data([1, 2, 3, 4, 5], [0.5, 0.2, 0.1, 0.2, 0.3]))");
    picture("pie_chart(time_series([19, 16, 9, 3, 7, 2, 17], [\"May 24, 1982\"]))");
    picture("pie_chart3d(event_data([8, 3, 5, 4, 9], [0, 1, 1, 0, 0]))");
    picture("list_point_plot3d(table(around(sin(x) * cos(y), 1), x, range(-4, 4), y, range(-4, 4)))");
    picture("list_line_plot3d(table(around(sin(x) * cos(y), 0.25), x, range(-4, 4), y, range(-4, 4)))");
    picture("list_point_plot3d([[2 -> \"a\", 3 -> \"b\", 5 -> \"c\", 7 -> \"d\"]])");
    picture("list_point_plot3d([[2, 3, 5, 7, 11, 13, 17] -> [\"a\", \"b\", \"c\", \"d\", \"e\", \"f\", \"g\"]])");
    picture("list_line_plot3d([table(labeled(i, i), i, 10), table(labeled(i, i), i, 10)])");
    picture("list_plot3d(flatten(table([around(x, 1), around(y, 1), around(sin(x) * cos(y), 1)], x, range(-4, 4), y, range(-4, 4)), 1))");
}

#[test]
fn primitives_of_pictures() {
    picture("graphics(circle(scaled([-0.25, 0.25], [0, 0]), 1), frame: true)");
    picture("graphics(ellipsoid(scaled([0.5, 0.25], [0, 0]), [4, 2]), frame: true)");
    picture("graphics([text(\"ABCDEF\", [0, 0.5], automatic), circle()], axes: true, ticks: none)");
    picture("graphics([geometric_transformation(disk(), [translation_transform([0, 0]), translation_transform([6, 0]), translation_transform([12, 0])])])");
    picture("graphics(geometric_transformation(rectangle(), [rotation_matrix(pi/4), [[1, 0], [0, 2]]]))");
}

#[test]
fn images_of_typed_data() {
    let rgb = "i = image(numeric_array([[[240, 232, 230], [241, 233, 233]], [[100, 50, 25], [0, 255, 10]]], \"UnsignedInteger8\"), \"Byte\", color_space: \"RGB\", image_size: all, interleaving: true); ";
    // the bytes come back as fractions of 255
    assert_eq!(ev(&format!("{rgb}image_data(i) == [[[240, 232, 230], [241, 233, 233]], [[100, 50, 25], [0, 255, 10]]]/255.")), "true");
    // statistics per channel, against the same statistic of each channel's samples
    assert_eq!(ev(&format!("{rgb}mean(i) == [mean([240, 241, 100, 0]), mean([232, 233, 50, 255]), mean([230, 233, 25, 10])]/255.")), "true");
    assert_eq!(ev(&format!("{rgb}median(i) == [median([240, 241, 100, 0]), median([232, 233, 50, 255]), median([230, 233, 25, 10])]/255.")), "true");
    assert_eq!(ev(&format!("{rgb}max(abs(variance(i) - [variance([240, 241, 100, 0]), variance([232, 233, 50, 255]), variance([230, 233, 25, 10])]/255.^2)) < 10^-15")), "true");
    assert_eq!(ev(&format!("{rgb}max(abs(std_dev(i)^2 - variance(i))) < 10^-15")), "true");
    // a gray image gives one number
    let gray = "j = image(numeric_array([[234, 235, 237], [235, 236, 240]], \"UnsignedInteger8\"), \"Byte\", color_space: \"Grayscale\", interleaving: none); ";
    assert_eq!(ev(&format!("{gray}abs(mean(j) - 1417/(6*255)) < 10^-15")), "true");
    assert_eq!(ev(&format!("{gray}abs(median(j) - 235.5/255) < 10^-15")), "true");
    // bits are 0 and 1; channels first are read as channels last
    assert_eq!(ev("image_data(image([[0, 1], [1, 1]], \"Bit\")) == [[[0.], [1.]], [[1.], [1.]]]"), "true");
    assert_eq!(ev("image_data(image([[[255, 0], [0, 255]], [[0, 0], [255, 255]]], \"Byte\", interleaving: false)) == [[[1., 0.], [0., 0.]], [[0., 1.], [1., 1.]]]"), "true");
    assert_eq!(ev("image([[1, 2]], \"Nibble\")"), "failure(argument, image, 2)");
}

#[test]
fn colour_operations_on_images() {
    let gray = "j = image([[0., 0.6], [0.3, 1.]]); ";
    // the negative of the negative is the image again (the samples are bytes: equal to 1/255)
    assert_eq!(ev(&format!("{gray}max(abs(image_data(color_negate(color_negate(j))) - image_data(j))) < 1/255")), "true");
    assert_eq!(ev(&format!("{gray}max(abs(image_data(color_negate(j)) + image_data(j) - 1)) < 1/255")), "true");
    assert_eq!(ev(&format!("{gray}max(abs(image_data(darker(j, 1/2)) - image_data(j)/2)) < 1/255")), "true");
    assert_eq!(ev(&format!("{gray}max(abs(image_data(lighter(j)) - (image_data(j) + (1 - image_data(j))/3))) < 1/255")), "true");
    assert_eq!(ev(&format!("{gray}dimensions(image_data(color_convert(j, \"RGB\")))")), "[2, 2, 3]");
    // an alpha channel is kept
    assert_eq!(ev("image_data(color_negate(image([[[0.2, 1.], [1., 0.]]])))[1, 1, 2]"), "1.0");
    // colours are unchanged
    assert_eq!(ev("color_negate(rgb_color(1, 0, 0))"), "rgb_color(0, 1, 1)");
}

#[test]
fn symbolic_hues_in_rgb() {
    let c = "c = color_convert(hue(h, 0.3, b), rgb_color); ";
    // at numbers the symbolic conversion is the numeric one
    for (hh, bb) in [("0.37", "0.8"), ("0.91", "0.25"), ("0.05", "1.0")] {
        assert_eq!(
            ev(&format!("{c}d = rewrite(c, [h -> {hh}, b -> {bb}]); e = color_convert(hue({hh}, 0.3, {bb}), rgb_color); max(abs(apply(list, d) - apply(list, e))) < 10^-12")),
            "true"
        );
    }
    picture("parametric_plot3d(apply(list, color_convert(hue(h, 0.3, b), rgb_color)), [h, 0, 1], [b, 0, 1])");
}
