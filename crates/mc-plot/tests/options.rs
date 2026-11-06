//! Plot options in the common spellings, plots combined and laid out, 3D plots, and the regressions of gaps found
//! while porting notebooks.
use mc_bench::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// The path in a printed `image("path")`.
fn image_path(printed: &str) -> &str {
    printed.strip_prefix("image(\"").and_then(|s| s.strip_suffix("\")")).expect(printed)
}

/// The SVG sibling of the PNG named by `image("path")`.
fn svg(printed: &str) -> String {
    String::from_utf8_lossy(&std::fs::read(std::path::Path::new(image_path(printed)).with_extension("svg")).unwrap()).to_string()
}

/// Width and height of the PNG named by `image("path")`.
fn png_size(printed: &str) -> (u32, u32) {
    let b = std::fs::read(image_path(printed)).expect("the image file exists");
    assert_eq!(&b[..8], b"\x89PNG\r\n\x1a\n", "{printed}");
    (u32::from_be_bytes(b[16..20].try_into().unwrap()), u32::from_be_bytes(b[20..24].try_into().unwrap()))
}

fn is_image(e: &mut CoreEngine, src: &str) -> String {
    let p = eval(e, src);
    assert!(p.starts_with("image(\""), "{src} -> {p}");
    png_size(&p);
    p
}

#[test]
fn common_options_are_accepted() {
    let mut e = CoreEngine::new();
    for src in [
        "plot(sin(x), [x, 0, 10], plot_legends: [\"sine\"], plot_style: [red, thick], frame: true, mesh: all, plot_theme: \"plain\")",
        "plot([sin(x), cos(x)], [x, 0, 10], plot_style: [directive(red, dashed), directive(blue, thickness(0.01))], axes_label: [x, y])",
        "plot(x^2, [x, -1, 1], plot_legends: automatic, plot_labels: [\"p\"], frame: false, axes_origin: [0, 0])",
        "plot(x, [x, 0, 1], exclusions: none, plot_points: 50, max_recursion: 3, image_size: 300, aspect_ratio: 1)",
        "plot(x, [x, 0, 1], frame_ticks: none, ticks: [[0, 0.5, 1], automatic], background: light_gray, plot_range_padding: none)",
        "plot(x, [x, 0, 1], plot_range: all, plot_range_clipping: true, image_padding: 20, label_style: [bold, 14])",
        "list_plot([1, 2, 3], plot_style: point_size(large), plot_legends: placed([\"a\"], above))",
        "list_line_plot([[1, 2, 3], [3, 2, 1]], plot_legends: line_legend([red, blue], [\"up\", \"down\"]))",
        "density_plot(x*y, [x, 0, 1], [y, 0, 1], color_function: \"temperature scale\", plot_legends: automatic)",
        "contour_plot(x*y, [x, 0, 1], [y, 0, 1], contour_shading: none, contour_labels: automatic)",
        "bar_chart([1, 2, 3], chart_style: [red, blue], chart_labels: [\"a\", \"b\", \"c\"])",
    ] {
        is_image(&mut e, src);
    }
    // a styled legend entry that is an expression is typeset; a title shows
    let p = is_image(&mut e, "plot(sin(x), [x, 0, 1], plot_label: \"a title\", plot_legends: [sin(x)])");
    assert!(svg(&p).contains(r#"aria-label="a title""#));
}

/// Gap queue G19, G20, G21: titles, grid lines with styles, epilog on log plots.
#[test]
fn titles_grid_lines_and_epilog() {
    let mut e = CoreEngine::new();
    let p = is_image(&mut e, "log_plot(exp(x), [x, 0, 1], plot_label: \"abc\")");
    assert!(svg(&p).contains(r#"aria-label="abc""#));
    is_image(&mut e, "plot(sin(x), [x, 0, 1], frame: true)");
    let p = is_image(&mut e, "log_plot(exp(x), [x, 0, 10], grid_lines: [[[3, red]], none])");
    assert!(svg(&p).contains("#ff0000"), "a red grid line");
    let p = is_image(&mut e, "plot(sin(x), [x, 0, 1], epilog: [red, line([[0, 0], [1, 1]]), point_size(large), point([0.5, 0.5]), text(\"m\", [0.2, 0.8])])");
    let s = svg(&p);
    assert!(s.contains("#ff0000") && s.contains(r#"aria-label="m""#), "{s}");
}

/// Gap queue G18, G22: plots combined with show and laid out side by side.
#[test]
fn plots_combine_and_lay_out() {
    let mut e = CoreEngine::new();
    let p = is_image(&mut e, "show(log_plot(exp(x), [x, 0, 1], legend: [\"a\"]), log_plot(exp(2*x), [x, 0, 2], legend: [\"b\"]))");
    let s = svg(&p);
    assert!(s.contains(r#"aria-label="a""#) && s.contains(r#"aria-label="b""#), "both legends");
    let p = is_image(&mut e, "graphics_row([plot(sin(x), [x, 0, 1]), plot(cos(x), [x, 0, 1])])");
    let (w, h) = png_size(&p);
    assert!(w > 900 && h < 400, "{w} x {h}");
    let p = is_image(&mut e, "graphics_column([plot(sin(x), [x, 0, 1]), graphics(disk())])");
    let (w, h) = png_size(&p);
    assert!(h > w, "{w} x {h}");
    for src in [
        "grid([[1, x^2], [sqrt(y), \"text\"]], frame: all)",
        "grid([[plot(x, [x, 0, 1]), graphics(circle())], [graphics3d(sphere()), \"label\"]])",
        "framed(graphics(disk()))",
        "framed(x^2)",
        "labeled(plot(sin(x), [x, 0, 1]), \"$\\\\sin x$\", top)",
        "legended(graphics(disk()), line_legend([red, blue], [\"a\", \"b\"]))",
        "graphics([inset(plot(x, [x, 0, 1]), [0, 0], [0, 0], 2), circle([0, 0], 1)])",
    ] {
        is_image(&mut e, src);
    }
    // a row of expressions alone stays a row
    assert_eq!(eval(&mut e, "row([1, 2])"), "row([1, 2])");
}

/// Gap queue G23: contours of scattered values and of an equation.
#[test]
fn contours_of_scattered_values_and_equations() {
    let mut e = CoreEngine::new();
    is_image(&mut e, "list_contour_plot([[0, 0, 1], [1, 0, 2], [0, 1, 3], [1, 1, 4], [0.5, 0.5, 2.5]])");
    is_image(&mut e, "list_contour_plot(table(i*j, i, 5, j, 5))");
    is_image(&mut e, "list_density_plot([[0, 0, 1], [1, 0, 2], [0, 1, 3], [1, 1, 4], [0.5, 0.5, 2.5]])");
    let p = is_image(&mut e, "contour_plot(x^2 + y^2 == 1, [x, -1, 1], [y, -1, 1])");
    assert!(!svg(&p).contains("<image "), "an equation's curve has no raster or colour bar");
    is_image(&mut e, "contour_plot([x^2 + y^2 == 1, x == y], [x, -1, 1], [y, -1, 1])");
}

/// Gap queue G24, G25: colours as lists and values; per-curve thickness; the rule form of filling.
#[test]
fn colours_thickness_and_filling_rules() {
    let mut e = CoreEngine::new();
    assert_eq!(eval(&mut e, "rgb_color([0.1, 0.2, 0.3])"), "rgb_color(0.1, 0.2, 0.3)");
    assert_eq!(eval(&mut e, "rgb_color([224, 17, 95]/255) == rgb_color(224/255, 17/255, 95/255)"), "true");
    let p = is_image(&mut e, "plot(x, [x, 0, 1], colors: [rgb_color(1, 0, 0)])");
    assert!(svg(&p).contains("#ff0000"));
    let p = is_image(&mut e, "list_log_log_plot([[1, 2], [10, 20]], joined: true, line_thickness: [4])");
    assert!(svg(&p).contains(r#"stroke-width="4""#));
    let p = is_image(&mut e, "plot(x, [x, 0, 1], plot_style: [directive(dotted, red, absolute_thickness(3))])");
    assert!(svg(&p).contains(r##"stroke="#ff0000" stroke-width="3""##));
    is_image(&mut e, "log_plot([exp(x), exp(2*x)], [x, 0, 1], filling: [1 -> [2]])");
    is_image(&mut e, "plot([sin(x), cos(x)], [x, 0, 1], filling: [1 -> axis, 2 -> top])");
    is_image(&mut e, "plot(sin(x), [x, 0, 1], filling: top)");
}

/// Gap queue G30, G33, G34, G36: a legend title, filling one curve, the whole range of data, accents in labels.
#[test]
fn legend_titles_partial_filling_full_ranges_and_accents() {
    let mut e = CoreEngine::new();
    let p = is_image(&mut e, "plot([x, x^2], [x, 0, 1], legend: [\"a\", \"b\"], legend_title: \"T_c\")");
    assert!(svg(&p).contains(r#"aria-label="T_c""#));
    let p = is_image(&mut e, "plot([x, x^2], [x, 0, 1], plot_legends: placed(line_legend([\"a\", \"b\"], legend_label: \"title\"), above))");
    assert!(svg(&p).contains(r#"aria-label="title""#));
    let one = is_image(&mut e, "list_log_log_plot([[[1, 1], [10, 10]], [[1, 2], [10, 20]]], joined: true, filling: [1 -> top])");
    assert_eq!(svg(&one).matches("fill-opacity").count(), 1, "one region filled");
    // plot_range: all keeps every value; a vertical line of data is drawn (no jump breaks data)
    let p = is_image(&mut e, "list_line_plot([table([k, 1.0*floor(k/50)], k, 99), [[50, 0], [50, 1]]], plot_range: all)");
    let j: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&std::fs::read(std::path::Path::new(image_path(&p)).with_extension("json")).unwrap())).unwrap();
    assert!(j["y"]["max"].as_f64().unwrap() >= 1.0, "{}", j["y"]);
    assert_eq!(j["series"].as_array().unwrap().len(), 2, "both lines drawn");
    // G37: joined per data set
    let p = is_image(&mut e, "list_plot([[[1, 1], [2, 2], [3, 3]], [[1, 2], [2, 1], [3, 2]]], joined: [true, false])");
    let j: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&std::fs::read(std::path::Path::new(image_path(&p)).with_extension("json")).unwrap())).unwrap();
    let kinds: Vec<&str> = j["series"].as_array().unwrap().iter().map(|s| s["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds.len(), 2);
    assert_ne!(kinds[0], kinds[1], "{kinds:?}");
    // accents are combining marks over their letters (the label text keeps the TeX)
    let p = is_image(&mut e, "plot(x, [x, 0, 1], legend: [\"$H\\\\to \\\\bar{b}b h$\"])");
    assert!(!svg(&p).contains("\\barb"));
}

/// Gap queue G26: graphics as the frames of an animated GIF.
#[test]
fn animations_are_gif_files() {
    let mut e = CoreEngine::new();
    let path = std::env::temp_dir().join(format!("mathcas-anim-{}.gif", std::process::id()));
    let p = path.to_string_lossy().replace('\\', "/");
    let src = format!("export_animation(\"{p}\", table(graphics(circle([0, 0], r), plot_range: 3), r, [1, 2, 3]), frame_rate: 5)");
    assert_eq!(eval(&mut e, &src), format!("\"{p}\""));
    let b = std::fs::read(&path).unwrap();
    assert_eq!(&b[..6], b"GIF89a");
    // three image descriptors (0x2C after a graphic control extension)
    assert_eq!(b.windows(2).filter(|w| w == &[0x00, 0x2C]).count(), 3);
    let _ = std::fs::remove_file(&path);
    assert_eq!(eval(&mut e, "export_animation(\"a.png\", [graphics(disk())])"), "failure(argument, export_animation, 1)");
    assert_eq!(eval(&mut e, "export_animation(\"a.gif\", [5])"), "failure(argument, export_animation, 2, 1)");
}

#[test]
fn more_plots() {
    let mut e = CoreEngine::new();
    for src in [
        "discrete_plot(1/n, [n, 1, 10])",
        "discrete_plot([n, n^2/10], [n, 0, 10, 2])",
        "discrete_plot(sin(k), [k, [1, 2, 5]])",
        "list_polar_plot([1, 2, 3, 2, 1])",
        "list_polar_plot([[0, 1], [pi/2, 2], [pi, 1]], joined: true)",
        "re_im_plot(exp(im*x), [x, 0, 6])",
        "abs_arg_plot((x + im)^2, [x, -2, 2])",
        "complex_plot(z^2 - 1, [z, -2 - 2*im, 2 + 2*im])",
        "complex_plot3d(1/z, [z, -1 - im, 1 + im])",
        "array_plot([[1, 0, 1], [0, 1, 0]])",
        "array_plot([[red, blue], [true, false]], mesh: true)",
        "array_plot(table(i*j, i, 10, j, 10), color_function: viridis)",
        "matrix_plot([[1, -2], [3, 0.5]])",
        "pie_chart([1, 2, 3], chart_labels: [\"a\", \"b\", \"c\"], legend: [\"a\", \"b\", \"c\"])",
    ] {
        is_image(&mut e, src);
    }
    assert_eq!(eval(&mut e, "color_data(\"rainbow\", 0)"), eval(&mut e, "gradient_color(rainbow, 0)"));
    assert_eq!(eval(&mut e, "color_data(\"rainbow\")(1)"), eval(&mut e, "gradient_color(rainbow, 1)"));
    assert!(eval(&mut e, "color_data(1, 2)").starts_with("rgb_color("));
    assert_eq!(eval(&mut e, "length(color_data(viridis, \"ColorList\"))"), "10");
    assert_eq!(eval(&mut e, "discrete_plot(n, [n, 5, 1])"), "failure(argument, discrete_plot, 2)");
    // negative values have no sector; nothing left to draw fails
    assert_eq!(eval(&mut e, "pie_chart([-1, -2])"), "failure(argument, pie_chart, 1)");
}

#[test]
fn three_dimensional_plots() {
    let mut e = CoreEngine::new();
    for src in [
        "plot3d(sin(x)*cos(y), [x, 0, 2*pi], [y, 0, 2*pi])",
        "plot3d(x^2 + y^2, [x, -1, 1], [y, -1, 1], color_function: \"Rainbow\", mesh: none, axes_label: [\"$x$\", \"$y$\", \"$z$\"])",
        "plot3d([x + y, x - y], [x, 0, 1], [y, 0, 1], view_point: above)",
        "plot3d(1/(x^2 + y^2), [x, -1, 1], [y, -1, 1])",
        "parametric_plot3d([cos(u)*(2 + cos(v)), sin(u)*(2 + cos(v)), sin(v)], [u, 0, 2*pi], [v, 0, 2*pi])",
        "parametric_plot3d([cos(t), sin(t), t/5], [t, 0, 6*pi])",
        "spherical_plot3d(1, [t, 0, pi], [p, 0, 2*pi])",
        "revolution_plot3d(t^2, [t, 0, 1])",
        "list_plot3d(table(sin(i/3)*cos(j/4), i, 20, j, 20))",
        "list_plot3d(table([random_real(), random_real(), random_real()], 30))",
        "list_point_plot3d(table([cos(t), sin(t), t], t, range(0, 10, 0.2)))",
        "graphics3d([sphere(), red, cuboid([2, 0, 0]), blue, cylinder([[0, 3, 0], [0, 3, 2]], 0.5), cone([[3, 3, 0], [3, 3, 2]], 0.7)], axes: true)",
        "graphics3d([opacity(0.5), sphere([[0, 0, 0], [1, 1, 1]], 0.5), edge_form(), polygon([[0, 0, 0], [1, 0, 0], [0, 1, 1]])], boxed: false)",
        "graphics3d([thick, line([[0, 0, 0], [1, 1, 1]]), point([1, 0, 1]), arrow([[0, 0, 0], [0, 0, 1]]), text(\"$z$\", [0, 0, 1.2])])",
    ] {
        is_image(&mut e, src);
    }
    assert_eq!(eval(&mut e, "polygon([[1, 0, 0], [0, 1, 0], [0, 0, 1]])"), "polygon([[1, 0, 0], [0, 1, 0], [0, 0, 1]])");
    assert_eq!(eval(&mut e, "plot3d(x, [x, 1, 0], [y, 0, 1])"), "failure(argument, plot3d, 2)");
}

/// Gap queue G26: export_data of a list of graphics (or plots) writes an animated GIF.
#[test]
fn export_data_writes_animations() {
    let mut e = CoreEngine::new();
    let dir = eval(&mut e, "temporary_directory()");
    assert!(dir.ends_with("/\""), "{dir}");
    let p = format!("{}mathcas-export-{}.gif", dir.trim_matches('"'), std::process::id());
    eval(&mut e, &format!("export_data(\"{p}\", [graphics(disk()), plot(x, [x, 0, 1]), graphics3d(sphere())])"));
    let b = std::fs::read(&p).expect("the GIF is written");
    assert_eq!(&b[..6], b"GIF89a");
    let _ = std::fs::remove_file(&p);
    let q = format!("{}mathcas-export-{}.svg", dir.trim_matches('"'), std::process::id());
    eval(&mut e, &format!("export_data(\"{q}\", graphics3d(sphere()))"));
    assert!(std::fs::read_to_string(&q).expect("the SVG is written").starts_with("<svg"));
    let _ = std::fs::remove_file(&q);
}

/// Epilog positions on log plots are data values; show takes each axis label from the first plot that has one;
/// epilog text anchored outside the plot range is clipped, not dropped.
#[test]
fn epilog_in_data_values_and_labels_of_combined_plots() {
    let mut e = CoreEngine::new();
    // a point at x = 10, y = 100 lies on the curve y = x^2: the sidecar's mapping puts it at the curve's pixel
    let p = is_image(&mut e, "log_log_plot(x^2, [x, 1, 100], epilog: [red, point_size(large), point([10, 100])])");
    let s = svg(&p);
    assert!(s.contains("#ff0000"), "the point is drawn");
    // its centre in the SVG is where 10 and 100 are on the axes (logarithms 1 and 2), inside the plot area
    let j: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&std::fs::read(std::path::Path::new(image_path(&p)).with_extension("json")).unwrap())).unwrap();
    let (x0, x1) = (j["x"]["min"].as_f64().unwrap(), j["x"]["max"].as_f64().unwrap());
    assert!(x0 <= 10.0 && 10.0 <= x1);
    let p = is_image(&mut e, "show(plot(x, [x, 0, 1]), plot(x^2, [x, 0, 1], x_label: \"first x\", y_label: \"first y\"))");
    let s = svg(&p);
    assert!(s.contains(r#"aria-label="first x""#) && s.contains(r#"aria-label="first y""#), "labels of the second plot");
    // anchored right of the range, reaching into it
    let p = is_image(&mut e, "plot(x, [x, 0, 1], epilog: [text(\"reaching in\", [1.2, 0.5], [1, 0])])");
    assert!(svg(&p).contains(r#"aria-label="reaching in""#));
}

#[test]
fn callouts_and_placed_labels() {
    let mut e = CoreEngine::new();
    let p = is_image(&mut e, "plot([callout(sin(x), \"sine\"), cos(x)], [x, 0, 6])");
    assert!(svg(&p).contains(r#"aria-label="sine""#));
    let p = is_image(&mut e, "list_plot([1, callout(3, \"peak\"), 2])");
    assert!(svg(&p).contains(r#"aria-label="peak""#));
    let p = is_image(&mut e, "plot(x, [x, 0, 1], epilog: [placed(\"corner\", [0.9, 0.1]), placed(graphics(disk()), above)])");
    assert!(svg(&p).contains(r#"aria-label="corner""#));
    is_image(&mut e, "graphics([circle(), placed(\"$r = 1$\", below)])");
}

/// Gap queue G43, G44, G49: letter styles in labels, hex colours in grid lines and directives, epilog positions in
/// data values on log axes, and layouts of dense plots drawn from the remembered graphics.
#[test]
fn letter_styles_hex_grid_lines_and_dense_layouts() {
    let mut e = CoreEngine::new();
    let p = is_image(&mut e, r#"plot(x, [x, 0, 1], x_label: "$\\ln \\mathcal{A}$, $\\mathbb{R}$, $\\mathbf{v}$")"#);
    // the label's text keeps the TeX as written; the letters themselves are checked in mc-graphics' label tests
    assert!(svg(&p).contains("mathcal"));
    let p = is_image(&mut e, "plot(exp(x), [x, 0, 10], y_scale: log, epilog: [point([5, 5.0])], grid_lines: [[[3, \"#ff0000\"]], none])");
    assert!(svg(&p).contains("#ff0000"), "a red grid line");
    let p = is_image(&mut e, "graphics([\"#00ff00\", disk()])");
    assert!(svg(&p).contains("#00ff00"));
    let t = std::time::Instant::now();
    eval(&mut e, "p = list_line_plot(table(sin(k/1000.0), k, 100000));");
    let single = t.elapsed();
    let t = std::time::Instant::now();
    let p = is_image(&mut e, "graphics_row([p, p])");
    let row = t.elapsed();
    assert!(png_size(&p).0 > 900);
    assert!(row < single * 4 + std::time::Duration::from_secs(2), "row {row:?} vs one plot {single:?}");
}

/// The position and scale of the first glyph of the label `text` in an SVG: (x, y, scale).
fn label_at(svg: &str, text: &str) -> Option<(f64, f64, f64)> {
    let start = svg.find(&format!(r#"aria-label="{text}""#))?;
    let rest = &svg[start..];
    let t = &rest[rest.find("translate(")? + "translate(".len()..];
    let (xy, rest) = t.split_once(')')?;
    let (x, y) = xy.split_once(' ')?;
    let s = &rest[rest.find("scale(")? + "scale(".len()..];
    Some((x.parse().ok()?, y.parse().ok()?, s[..s.find(')')?].parse().ok()?))
}

/// The plot area (x, y, w, h): the first `rect` (the clip of the plot area) of an SVG.
fn area(svg: &str) -> (f64, f64, f64, f64) {
    let r = &svg[svg.find("<rect ").expect("a plot area")..];
    let attr = |k: &str| -> f64 {
        let s = &r[r.find(&format!(" {k}=\"")).unwrap() + k.len() + 3..];
        s[..s.find('"').unwrap()].parse().unwrap()
    };
    (attr("x"), attr("y"), attr("width"), attr("height"))
}

/// The glyphs drawn for the label `text` in an SVG.
fn glyph_count(svg: &str, text: &str) -> usize {
    let start = svg.find(&format!(r#"aria-label="{text}""#)).expect(text);
    let rest = &svg[start..];
    rest[..rest.find("</g>").unwrap()].matches("<use").count()
}

/// Vertical marks 2 points long (minor ticks) in an SVG's paths.
fn short_marks(svg: &str) -> usize {
    let p = |t: &str| t.trim().split_once(' ').and_then(|(a, b)| Some((a.parse::<f64>().ok()?, b.parse::<f64>().ok()?)));
    svg.split(['M', 'L', '"'])
        .collect::<Vec<_>>()
        .windows(2)
        .filter(|w| matches!((p(w[0]), p(w[1])), (Some((x0, y0)), Some((x1, y1))) if x0 == x1 && ((y0 - y1).abs() - 2.0).abs() < 1e-6))
        .count()
}

/// Gap queue G69: frame ticks per side, given ticks in data values on log axes next to the plot's own powers of
/// ten, TeX in tick labels, long tick lists with minor marks.
#[test]
fn given_ticks_in_data_values_frame_ticks_and_minor_marks() {
    let mut e = CoreEngine::new();
    // frame_ticks [[left, right], [bottom, top]]: the left ones given, the bottom ones automatic
    let s = svg(&is_image(&mut e, r#"plot(x^4, [x, 0.1, 1.1], frame_ticks: [[[[0, "0"], [1.0, "one"]], none], [automatic, none]])"#));
    assert!(s.contains(r#"aria-label="one""#) && s.contains(r#"aria-label="0.4""#), "given left ticks and automatic bottom ones");
    assert!(!s.contains(r#"aria-label="1.4""#), "no automatic left ticks");
    // a tick at 10 on a log axis from 1 to 100 is in the middle; the y axis keeps its automatic ticks
    let s = svg(&is_image(&mut e, r#"plot(x, [x, 1, 100], x_scale: log, ticks: [[[10, "ten"]], automatic])"#));
    let (ax, _, aw, _) = area(&s);
    let (x, _, _) = label_at(&s, "ten").expect("the given tick");
    assert!((x - (ax + aw / 2.0)).abs() < 0.04 * aw, "ten at {x}, area {ax} + {aw}");
    assert!(s.contains(r#"aria-label="20""#));
    // automatic ticks on a log axis are powers of ten, with minor marks (half as long) between them
    let s = svg(&is_image(&mut e, r#"plot(x, [x, 1, 1000], x_scale: log, ticks: [automatic, [[500, "half"]]])"#));
    assert!(s.contains(r#"aria-label="half""#));
    assert!(short_marks(&s) >= 16, "minor marks: {}", short_marks(&s));
    // TeX in a tick label: \times is a sign, not a tab
    let s = svg(&is_image(&mut e, r#"plot(x, [x, 0, 1], ticks: [[[0.5, r"$5.0\times 10^{18}$"]], automatic])"#));
    assert_eq!(glyph_count(&s, r"$5.0\times 10^{18}$"), 8, "5.0×10 and 18");
    // a long list of ticks with minor marks
    let s = svg(&is_image(
        &mut e,
        r#"plot(x, [x, 0.001, 100], x_scale: log, ticks: [join([[0.01, "a"], [1, "b"]], flatten(table([m*10.0^k, "", 0.5], k, range(-3, 2), m, range(2, 9)), 1)), automatic])"#,
    ));
    assert!(s.contains(r#"aria-label="a""#) && s.contains(r#"aria-label="b""#));
    assert!(short_marks(&s) >= 40, "minor marks: {}", short_marks(&s));
}

/// Gap queue G66, G72, G78: legends in a row with a rounded frame, placed in a corner, dashed styles given as lists.
#[test]
fn legend_layouts_frames_and_places() {
    let mut e = CoreEngine::new();
    let s = svg(&is_image(&mut e, r#"plot([x, x^2], [x, 0, 1], plot_legends: line_legend(["a", "b"], legend_layout: "Row", legend_function: frame))"#));
    let ((xa, ya, _), (xb, yb, _)) = (label_at(&s, "a").unwrap(), label_at(&s, "b").unwrap());
    assert!(ya == yb && xb > xa + 10.0, "side by side: {xa} {ya}, {xb} {yb}");
    assert!(s.split("<path d=").skip(1).any(|p| p[..p.find('>').unwrap()].contains('C') && p.contains("stroke=")), "a rounded frame");
    // placed(legend, [left, bottom]): the bottom left corner
    let s = svg(&is_image(&mut e, r#"plot([x, x^2], [x, 0, 1], plot_legends: placed(line_legend(["a", "b"]), [left, bottom]))"#));
    let (ax, ay, aw, ah) = area(&s);
    let (x, y, _) = label_at(&s, "a").unwrap();
    assert!(x < ax + aw / 4.0 && y > ay + ah * 0.75, "a at {x} {y}");
    // a list of directives is one curve's style: dashed, with the legend
    let s = svg(&is_image(&mut e, r#"plot([sin(x), cos(x)], [x, 0, 3], plot_style: [[blue, dashed], [red]], legend: ["a", "b"])"#));
    assert!(s.contains("stroke-dasharray") && s.contains(r#"aria-label="b""#));
}

/// Gap queue G74: label sizes (label_style, styled frame labels), framed columns inside graphics.
#[test]
fn label_sizes_and_framed_columns() {
    let mut e = CoreEngine::new();
    let s = svg(&is_image(&mut e, r#"plot(x, [x, 0, 1], frame_label: ["x", "y"], label_style: 16)"#));
    let (tick, label) = (label_at(&s, "0.2").unwrap().2, label_at(&s, "x").unwrap().2);
    assert!(label > tick * 1.2, "label {label} (16 pt) vs tick {tick}");
    let s = svg(&is_image(&mut e, r#"plot(x, [x, 0, 1], font_size: 10, frame_label: ["x", style(r"$\frac{h}{v}$", font_size(12))])"#));
    assert!(!s.contains("style("), "the style is applied, not printed");
    let (tick, label) = (label_at(&s, "0.2").unwrap().2, label_at(&s, r"$\frac{h}{v}$").unwrap().2);
    assert!(label > tick, "label {label} vs tick {tick}");
    let s = svg(&is_image(&mut e, r#"plot(x, [x, 0, 1], epilog: inset(framed(column(["a", "b"])), scaled([0.5, 0.5])))"#));
    assert!(s.contains(r#"aria-label="a""#) && s.contains(r#"aria-label="b""#) && !s.contains("column("));
}
