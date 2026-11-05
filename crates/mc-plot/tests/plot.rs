//! plot and list_plot through the engine: an image term naming a 640x400 PNG; failures for bad arguments.
use mc_bench::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// The PNG named by `image("path")`: its width and height.
fn png_size(printed: &str) -> (u32, u32) {
    let path = printed.strip_prefix("image(\"").and_then(|s| s.strip_suffix("\")")).expect(printed);
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
    // the PDF next to the PNG carries the labels
    let printed = eval(&mut e, "plot(x, [x, 0, 1], x_label: \"time (s)\", legend: [\"line\"])");
    let path = printed.strip_prefix("image(\"").and_then(|s| s.strip_suffix("\")")).expect(&printed);
    let pdf = String::from_utf8_lossy(&std::fs::read(std::path::Path::new(path).with_extension("pdf")).unwrap()).to_string();
    assert!(pdf.contains(r"(time \(s\)) Tj") && pdf.contains("(line) Tj"), "{pdf}");
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
