//! The graphics model through the engine: primitives and directives stay as they are (checked when numeric),
//! `graphics` makes numeric constants numbers and fails on what is not graphics, `show` combines, the colour
//! functions compute.
use mc_bench::{Engine, StopFlag};
use mc_engine::CoreEngine;

/// The value's printed term (a graphics value is shown as its picture, so the term is printed through `to_string`).
fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(&format!("to_string({src})"), &StopFlag::new()) {
        Ok(v) => match v.printed.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            Some(s) => s.replace("\\\"", "\"").replace("\\\\", "\\"),
            None => v.printed,
        },
        Err(err) => format!("error: {err}"),
    }
}

fn check(cases: &[(&str, &str)]) {
    let mut e = CoreEngine::new();
    for (src, want) in cases {
        assert_eq!(eval(&mut e, src), *want, "{src}");
    }
}

#[test]
fn primitives_stay_and_are_checked() {
    check(&[
        ("point([1, 2])", "point([1, 2])"),
        ("point([[1, 2], [3, 4.5]])", "point([[1, 2], [3, 4.5]])"),
        ("point([a, b])", "point([a, b])"),
        ("line([[0, 0], [1, 1]])", "line([[0, 0], [1, 1]])"),
        ("line([[[0, 0], [1, 1]], [[2, 2], [3, 1]]])", "line([[[0, 0], [1, 1]], [[2, 2], [3, 1]]])"),
        ("polygon([[0, 0], [1, 0], [0, 1]])", "polygon([[0, 0], [1, 0], [0, 1]])"),
        ("rectangle()", "rectangle()"),
        ("rectangle([0, 0], [1, 2])", "rectangle([0, 0], [1, 2])"),
        ("circle([0, 0], r)", "circle([0, 0], r)"),
        ("disk([0, 0], [1, 2], [0, pi])", "disk([0, 0], [1, 2], [0, pi])"),
        ("arrow([[0, 0], [1, 1]])", "arrow([[0, 0], [1, 1]])"),
        ("bezier_curve([[0, 0], [1, 1], [2, 0], [3, 1]])", "bezier_curve([[0, 0], [1, 1], [2, 0], [3, 1]])"),
        ("text(x^2, [0, 0])", "text(x^2, [0, 0])"),
        ("text(\"a\", scaled([0.5, 0.5]), [-1, 0])", "text(\"a\", scaled([0.5, 0.5]), [-1, 0])"),
        // wrong arguments: a failure naming the argument
        ("point(\"x\")", "failure(argument, point, 1)"),
        ("point([1, 2, 3])", "failure(argument, point, 1)"),
        ("circle([0, 0], -1)", "failure(argument, circle, 2)"),
        ("disk([0, 0], 1, [0])", "failure(argument, disk, 3)"),
        ("rectangle([0, 0], [1, 2, 3])", "failure(argument, rectangle, 2)"),
        ("rectangle([0, 0], scaled([1, 1]))", "failure(argument, rectangle, 2)"),
        ("line(5)", "failure(argument, line, 1)"),
        ("text(\"a\", [0, 0], [1, 2, 3])", "failure(argument, text, 3)"),
        ("point([scaled([1, 1]), [0, 0]])", "point([scaled([1, 1]), [0, 0]])"),
        // the wrong number of arguments leaves the call as it is
        ("point(1, 2)", "point(1, 2)"),
        ("circle(1, 2, 3, 4)", "circle(1, 2, 3, 4)"),
    ]);
}

#[test]
fn directives_and_colours_stay_and_are_checked() {
    check(&[
        ("thickness(0.01)", "thickness(0.01)"),
        ("absolute_dashing([4, 2])", "absolute_dashing([4, 2])"),
        ("dashing(dotted)", "dashing(dotted)"),
        ("opacity(0.5, red)", "opacity(0.5, red)"),
        ("edge_form([black, thickness(0.01)])", "edge_form([black, thickness(0.01)])"),
        ("face_form()", "face_form()"),
        ("rgb_color(1, 0, 0)", "rgb_color(1, 0, 0)"),
        ("rgb_color(a, 0, 0)", "rgb_color(a, 0, 0)"),
        ("rgb_color(\"#ff8000\")", "rgb_color(1.0, 0.5019607843137255, 0.0)"),
        ("thickness(-1)", "failure(argument, thickness, 1)"),
        ("dashing([0.1, \"a\"])", "failure(argument, dashing, 1)"),
        ("dashing(wavy)", "failure(argument, dashing, 1)"),
        ("opacity(0.5, 7)", "failure(argument, opacity, 2)"),
        ("font_size(0)", "failure(argument, font_size, 1)"),
        ("edge_form(7)", "failure(argument, edge_form, 1)"),
        ("rgb_color(1, \"a\", 0)", "failure(argument, rgb_color, 2)"),
        ("rgb_color(\"#ff80\")", "failure(argument, rgb_color, 1)"),
        ("hue(\"h\")", "failure(argument, hue, 1)"),
    ]);
}

#[test]
fn graphics_numericises_and_checks() {
    check(&[
        ("graphics(circle([0, 0], pi))", "graphics(circle([0, 0], 3.141592653589793))"),
        (
            "graphics([red, disk([0, 0], sqrt(2))], plot_range: [[0, pi], [-1, 1]])",
            "graphics([red, disk([0, 0], 1.4142135623730951)], plot_range: [[0, 3.141592653589793], [-1, 1]])",
        ),
        // labels are left as written
        ("graphics(text(pi, [pi, 0]))", "graphics(text(pi, [3.141592653589793, 0]))"),
        ("graphics(point([a, b]))", "graphics(point([a, b]))"),
        ("graphics(point([1, 2]), axes: true, frame: false)", "graphics(point([1, 2]), axes: true, frame: false)"),
        ("graphics([])", "graphics([])"),
        ("graphics(foo(1))", "failure(not_graphics, \"foo(1)\")"),
        ("graphics([red, 7])", "failure(not_graphics, \"7\")"),
        ("graphics(point([1, 2]), bogus: 1)", "failure(unknown_option, \"bogus\")"),
        ("graphics(point([1, 2]), axes: maybe)", "failure(option, \"axes: maybe\")"),
        ("graphics(point([1, 2]), plot_range: [2, 1])", "failure(option, \"plot_range: [2, 1]\")"),
        ("graphics(point([1, 2]), image_size: -5)", "failure(option, \"image_size: -5\")"),
        ("graphics(point([1, 2]), legend: [[7, \"a\"]])", "failure(option, \"legend: [[7, \\\"a\\\"]]\")"),
        // a graphic inside a graphic is its content
        ("graphics([graphics(point([1, 2]), frame: true)])", "graphics([graphics(point([1, 2]), frame: true)])"),
    ]);
}

#[test]
fn show_combines_graphics() {
    check(&[
        (
            "show(graphics(point([1, 2]), axes: true), graphics(line([[0, 0], [1, 1]]), frame: true, axes: false), plot_label: \"both\")",
            "graphics([point([1, 2]), line([[0, 0], [1, 1]])], plot_label: \"both\", axes: true, frame: true)",
        ),
        ("show([graphics(point([1, 2])), graphics(point([3, 4]))])", "graphics([point([1, 2]), point([3, 4])])"),
        ("show(graphics(point([1, 2])), 5)", "failure(argument, show, 2)"),
        ("show(graphics(point([1, 2])), bogus: 1)", "failure(unknown_option, \"bogus\")"),
    ]);
}

#[test]
fn colour_functions() {
    check(&[
        ("color_convert(red, hue)", "hue(0.0, 1.0, 1.0)"),
        ("color_convert(hue(1/3), rgb_color)", "rgb_color(0.0, 1.0, 0.0)"),
        ("color_convert(rgb_color(1, 0, 0, 0.5), gray_level)", "gray_level(0.299, 0.5)"),
        ("color_convert(gray_level(0.25), rgb_color)", "rgb_color(0.25, 0.25, 0.25)"),
        ("color_convert(x, hue)", "color_convert(x, hue)"),
        ("color_convert(red, cmyk)", "failure(argument, color_convert, 2)"),
        ("color_convert(\"red\", hue)", "failure(argument, color_convert, 1)"),
        ("blend([red, blue])", "rgb_color(0.5, 0.0, 0.5)"),
        ("blend([red, green, blue], 0.75)", "rgb_color(0.0, 0.5, 0.5)"),
        ("blend([red, blue], 2)", "rgb_color(0.0, 0.0, 1.0)"),
        ("blend([red], 0.3)", "rgb_color(1.0, 0.0, 0.0)"),
        ("blend([])", "failure(argument, blend, 1)"),
        ("blend([red, 5])", "failure(argument, blend, 1)"),
        ("blend([red, blue], \"t\")", "failure(argument, blend, 2)"),
        ("blend([red, blue], t)", "blend([red, blue], t)"),
        ("lighter(black)", "rgb_color(0.3333333333333333, 0.3333333333333333, 0.3333333333333333)"),
        ("darker(white, 0.5)", "rgb_color(0.5, 0.5, 0.5)"),
        ("lighter(rgb_color(0, 0, 0, 0.5), 1)", "rgb_color(1.0, 1.0, 1.0, 0.5)"),
        ("lighter(red, 2)", "failure(argument, lighter, 2)"),
    ]);
}
