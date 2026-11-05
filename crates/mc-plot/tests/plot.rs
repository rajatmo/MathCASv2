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
