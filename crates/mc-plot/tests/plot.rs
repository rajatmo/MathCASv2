//! plot and list_plot through the engine: an image term naming a 640x400 PNG; failures for bad arguments.
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

/// The text of the file next to the PNG at `path` with extension `ext`.
fn sibling(path: &str, ext: &str) -> String {
    String::from_utf8_lossy(&std::fs::read(std::path::Path::new(path).with_extension(ext)).unwrap()).to_string()
}

/// The PNG named by `image("path")`: its width and height.
fn png_size(printed: &str) -> (u32, u32) {
    let path = image_path(printed);
    let b = std::fs::read(path).expect("the image file exists");
    assert_eq!(&b[..8], b"\x89PNG\r\n\x1a\n");
    (u32::from_be_bytes(b[16..20].try_into().unwrap()), u32::from_be_bytes(b[20..24].try_into().unwrap()))
}

#[test]
fn plots_are_png_images() {
    let mut e = CoreEngine::new();
    for src in [
        "plot(sin(x), [x, 0, 2*pi])",
        "plot([sin(x), cos(x)], [x, -pi, pi])",
        "plot(1/x, [x, -1, 1])",
        "plot(sqrt(x), [x, -1, 1])",
        "list_plot([1, 4, 9, 16])",
        "list_plot([[0, 1], [1, 0.5], [2, 0.25]], joined: true)",
        "list_plot(random_real(1.0, 1000))",
    ] {
        assert_eq!(png_size(&eval(&mut e, src)), (640, 400), "{src}");
    }
}

#[test]
fn bad_arguments_fail() {
    let mut e = CoreEngine::new();
    for (src, want) in [
        ("plot(x, [x, 1, 0])", "failure(argument, plot, 2)"),
        ("plot(x, [2, 0, 1])", "failure(argument, plot, 2)"),
        ("plot(x, [x, 0, a])", "failure(argument, plot, 2)"),
        ("list_plot(5)", "failure(argument, list_plot, 1)"),
        ("list_plot([a, b])", "failure(argument, list_plot, 1)"),
        ("list_plot([1, 2], sideways: 1)", "failure(argument, list_plot, 2)"),
    ] {
        assert_eq!(eval(&mut e, src), want, "{src}");
    }
}

#[test]
fn log_scale_plots_are_png_images() {
    let mut e = CoreEngine::new();
    for src in [
        "log_plot(exp(x), [x, -5, 20])",
        "log_plot(x, [x, 2, 5])",
        "log_log_plot([x^2, 1/x], [x, 0.01, 1.0e5])",
        "list_log_plot([1, 10, 100, 0, -5])",
        "list_log_log_plot(table([10.0^k, 2.0^k], k, 10), joined: true)",
    ] {
        assert_eq!(png_size(&eval(&mut e, src)), (640, 400), "{src}");
    }
    assert_eq!(eval(&mut e, "log_log_plot(x, [x, -1, 1])"), "failure(argument, log_log_plot, 2)");
    assert_eq!(eval(&mut e, "list_log_plot([0, -1])"), "failure(argument, list_log_plot, 1)");
}

#[test]
fn plots_take_axis_labels_and_legends() {
    let mut e = CoreEngine::new();
    for src in [
        "plot([sin(x), cos(x)], [x, 0, 2*pi], x_label: \"x\", y_label: \"\u{3c1} (GeV^4)\", legend: [\"sin\", \"cos\"])",
        "plot(exp(x), [x, 0, 5], y_scale: log, legend: [\"exp\"], legend_position: bottom_left)",
        // more entries than curves: the extra ones are left out; fewer: the rest have none
        "plot([x, x^2], [x, 0, 1], legend: [\"a\", \"b\", \"c\"], legend_position: top_left)",
        "plot([x, x^2], [x, 0, 1], legend: [\"a\"], legend_position: bottom_right)",
        "plot(x, [x, 0, 1], legend: [])",
        "list_plot([1, 4, 9], legend: [\"squares\"], x_label: \"k\")",
        "list_log_plot([1, 10, 100], joined: true, y_label: \"\u{3bc}_A/T\", legend: [\"powers\"])",
    ] {
        assert_eq!(png_size(&eval(&mut e, src)), (640, 400), "{src}");
    }
    // the SVG and PDF next to the PNG carry the labels
    let printed = eval(&mut e, "plot(x, [x, 0, 1], x_label: \"time (s)\", legend: [\"line\"])");
    let path = image_path(&printed);
    let svg = sibling(path, "svg");
    assert!(svg.contains(r#"aria-label="time (s)""#) && svg.contains(r#"aria-label="line""#), "{svg}");
    assert!(sibling(path, "pdf").starts_with("%PDF-1.4"));
    for (src, want) in [
        ("plot(x, [x, 0, 1], x_label: 5)", "failure(argument, plot, 3)"),
        ("plot(x, [x, 0, 1], legend: \"a\")", "failure(argument, plot, 3)"),
        ("plot(x, [x, 0, 1], legend: [a])", "failure(argument, plot, 3)"),
        ("plot(x, [x, 0, 1], legend_position: middle)", "failure(argument, plot, 3)"),
        ("list_plot([1, 2], y_label: [\"a\"])", "failure(argument, list_plot, 2)"),
    ] {
        assert_eq!(eval(&mut e, src), want, "{src}");
    }
}

#[test]
fn curves_take_colors_and_dash_patterns() {
    let mut e = CoreEngine::new();
    for src in [
        "plot([x, x^2, x^3, x^4], [x, 0, 1], line_style: [solid, dashed, dotted, dot_dashed])",
        "plot([x, x^2], [x, 0, 1], colors: [\"#560591\", \"#808000\"], line_style: [dashed], legend: [\"a\", \"b\"])",
        "plot([x, x^2, x^3], [x, 0, 1], colors: [\"#e0115f\"])",
        "list_plot([1, 4, 9], joined: true, colors: [\"#000000\"], line_style: [dotted])",
        "list_plot([1, 4, 9], colors: [\"#000000\"])",
    ] {
        assert_eq!(png_size(&eval(&mut e, src)), (640, 400), "{src}");
    }
    let printed = eval(&mut e, "plot(x, [x, 0, 1], colors: [\"#ff0000\"], line_style: [dashed])");
    let svg = sibling(image_path(&printed), "svg");
    assert!(svg.contains(r##"stroke="#ff0000" stroke-width="2" stroke-linejoin="round" stroke-linecap="round" stroke-dasharray="8 5""##), "{svg}");
    for src in [
        "plot(x, [x, 0, 1], colors: \"#ff0000\")",
        "plot(x, [x, 0, 1], colors: [\"red\"])",
        "plot(x, [x, 0, 1], colors: [\"#ff00\"])",
        "plot(x, [x, 0, 1], line_style: [wavy])",
        "plot(x, [x, 0, 1], line_style: dashed)",
    ] {
        assert_eq!(eval(&mut e, src), "failure(argument, plot, 3)", "{src}");
    }
}

#[test]
fn plots_have_svg_and_pdf_siblings_and_formatted_labels() {
    let mut e = CoreEngine::new();
    let printed = eval(&mut e, "plot(sin(x), [x, 0, 1], x_label: \"$E_\\\\gamma$ (GeV)\", legend: [\"$\\\\alpha^2$\"])");
    let path = image_path(&printed);
    let svg = sibling(path, "svg");
    assert!(svg.starts_with("<svg ") && svg.contains(r#"width="640" height="400""#));
    // the label's math: E, an index gamma
    assert!(svg.contains(r#"aria-label="$E_\gamma$ (GeV)""#), "{svg}");
    let pdf = std::fs::read(std::path::Path::new(path).with_extension("pdf")).unwrap();
    let pdf = String::from_utf8_lossy(&pdf);
    assert!(pdf.contains("/MediaBox [0 0 640 400]") && pdf.contains("/Subtype /Type3") && pdf.contains("/FlateDecode"));
}

#[test]
fn large_plots_are_quick() {
    let mut e = CoreEngine::new();
    for src in [
        "list_plot(random_real(1.0, 100000))",
        "list_plot(table([k, sin(k/1000.0)], k, 100000), joined: true)",
    ] {
        let start = std::time::Instant::now();
        let printed = eval(&mut e, src);
        let secs = start.elapsed().as_secs_f64();
        println!("{src}: {secs:.3} s");
        assert_eq!(png_size(&printed), (640, 400), "{src}");
        assert!(secs < 30.0, "{src}: {secs} s");
    }
}
