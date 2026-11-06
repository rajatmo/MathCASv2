//! plot and list_plot through the engine: an image term naming a 640x400 PNG; failures for bad arguments.
use mc_bench::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// `src` evaluates to `want`; `"image"` stands for any image (a plot drawn).
fn expect(e: &mut CoreEngine, src: &str, want: &str) {
    let got = eval(e, src);
    if want == "image" {
        assert!(got.starts_with("image(\""), "{src} -> {got}");
    } else {
        assert_eq!(got, want, "{src}");
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
    ] {
        expect(&mut e, src, want);
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
        ("plot(x, [x, 0, 1], x_label: 5)", "image"),
        ("plot(x, [x, 0, 1], legend: \"a\")", "image"),
        ("plot(x, [x, 0, 1], legend: [a])", "image"),
        ("plot(x, [x, 0, 1], legend_position: middle)", "image"),
        ("list_plot([1, 2], y_label: [\"a\"])", "image"),
    ] {
        expect(&mut e, src, want);
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
        // values an option does not take are left out (named in a message): the plot is drawn
        expect(&mut e, src, "image");
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

/// The data sidecar `plot-N.json` of a printed `image("path")`.
fn sidecar(printed: &str) -> serde_json::Value {
    let path = std::path::Path::new(image_path(printed)).with_extension("json");
    serde_json::from_slice(&std::fs::read(path).expect("the sidecar exists")).expect("the sidecar is JSON")
}

/// The PNG pixel of data point (x, y), from the sidecar's plot area, ranges and scales.
fn pixel(j: &serde_json::Value, x: f64, y: f64) -> (f64, f64) {
    let s = |axis: &serde_json::Value, v: f64| match axis["scale"].as_str().unwrap() {
        "linear" => v,
        "log" => v.log10(),
        "signed_log" => {
            let c = axis["c"].as_f64().unwrap();
            v.signum() * (v.abs() / c).ln_1p() / std::f64::consts::LN_10
        }
        other => panic!("scale {other}"),
    };
    let a: Vec<f64> = j["plot_area"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    let frac = |axis: &serde_json::Value, v: f64| {
        let (lo, hi) = (axis["min"].as_f64().unwrap(), axis["max"].as_f64().unwrap());
        (s(axis, v) - s(axis, lo)) / (s(axis, hi) - s(axis, lo))
    };
    (a[0] + frac(&j["x"], x) * (a[2] - a[0]), a[3] - frac(&j["y"], y) * (a[3] - a[1]))
}

/// Whether the PNG has a pixel within 2 pixels of (px, py) close to the colour `#rrggbb`.
fn has_colour_near(printed: &str, (px, py): (f64, f64), hex: &str) -> bool {
    let pm = tiny_skia::Pixmap::load_png(image_path(printed)).unwrap();
    let want: Vec<i32> = (0..3).map(|i| i32::from_str_radix(&hex[1 + 2 * i..3 + 2 * i], 16).unwrap()).collect();
    for dy in -2..=2 {
        for dx in -2..=2 {
            let (x, y) = ((px.round() as i32 + dx) as u32, (py.round() as i32 + dy) as u32);
            let Some(c) = pm.pixel(x, y) else { continue };
            let got = [c.red() as i32, c.green() as i32, c.blue() as i32];
            if got.iter().zip(&want).all(|(g, w)| (g - w).abs() <= 24) {
                return true;
            }
        }
    }
    false
}

#[test]
fn every_plot_has_a_data_sidecar_that_maps_to_its_pixels() {
    let mut e = CoreEngine::new();
    // a curve: the point (pi/2, 1) is drawn in the curve's colour where the sidecar says
    let printed = eval(&mut e, "plot([sin(x), x/10], [x, 0, 2*pi], legend: [\"sine\"])");
    let j = sidecar(&printed);
    assert_eq!(j["version"], 1);
    assert_eq!((j["width"].as_f64(), j["height"].as_f64()), (Some(640.0), Some(400.0)));
    let series = j["series"].as_array().unwrap();
    assert_eq!(series.len(), 2);
    assert_eq!(series[0]["name"], "sine");
    assert_eq!(series[1]["name"], "(1/10)*x");
    assert_eq!(series[0]["kind"], "line");
    let color = series[0]["color"].as_str().unwrap();
    assert!(has_colour_near(&printed, pixel(&j, std::f64::consts::FRAC_PI_2, 1.0), color));
    // ... and a point off the curve is not
    assert!(!has_colour_near(&printed, pixel(&j, std::f64::consts::FRAC_PI_2, 0.5), color));
    let pts = series[0]["points"].as_array().unwrap();
    assert!(pts.len() > 60 && pts.iter().all(|p| p[1].as_f64().unwrap().abs() <= 1.0));
    // a logarithmic axis: data coordinates in the sidecar, the scale named
    let printed = eval(&mut e, "log_plot(exp(x), [x, 0, 10], colors: [\"#c0102e\"])");
    let j = sidecar(&printed);
    assert_eq!(j["y"]["scale"], "log");
    assert_eq!(j["series"][0]["color"], "#c0102e");
    assert!(has_colour_near(&printed, pixel(&j, 5.0, 5f64.exp()), "#c0102e"));
    let last = j["series"][0]["points"].as_array().unwrap().last().unwrap().clone();
    assert!((last[1].as_f64().unwrap() - 10f64.exp()).abs() < 1e-6 * 10f64.exp());
    // points, on a signed-log axis
    let printed = eval(&mut e, "list_plot([[1, -100], [2, 0], [3, 1000]], y_scale: signed_log)");
    let j = sidecar(&printed);
    assert_eq!((j["y"]["scale"].as_str(), j["y"]["c"].as_f64()), (Some("signed_log"), Some(1.0)));
    let s = &j["series"][0];
    assert_eq!((s["kind"].as_str(), s["name"].as_str()), (Some("points"), Some("data")));
    assert_eq!(s["points"][2][1].as_f64(), Some(1000.0));
    let color = s["color"].as_str().unwrap();
    assert!(has_colour_near(&printed, pixel(&j, 3.0, 1000.0), color));
    // a pole splits the line into entries of one name
    let j = sidecar(&eval(&mut e, "plot(tan(x), [x, 0, 3])"));
    let names: Vec<&str> = j["series"].as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["tan(x)", "tan(x)"]);
}

/// Copy the plot's PNG under the tests' temporary folder as `name.png` (to look at).
fn keep(printed: &str, name: &str) {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("plots");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::copy(image_path(printed), dir.join(format!("{name}.png")));
}

/// Evaluate `src`, which must give a plot; keep it as `name`; give the printed value.
fn picture(e: &mut CoreEngine, src: &str, name: &str) -> String {
    let printed = eval(e, src);
    assert!(printed.starts_with("image(\""), "{src}: {printed}");
    keep(&printed, name);
    printed
}

#[test]
fn plane_curves() {
    let mut e = CoreEngine::new();
    let p = picture(&mut e, "parametric_plot([cos(t), sin(t)], [t, 0, 2*pi])", "circle");
    // equal units: the page is about square
    let (w, h) = png_size(&p);
    assert!((w as i32 - h as i32).abs() < 60, "{w} x {h}");
    let j = sidecar(&p);
    let color = j["series"][0]["color"].as_str().unwrap().to_string();
    assert!(has_colour_near(&p, pixel(&j, 0.0, 1.0), &color));
    assert!(!has_colour_near(&p, pixel(&j, 0.0, 0.0), &color));
    picture(&mut e, "parametric_plot([[sin(2t), sin(3t)], [cos(t)/2, sin(t)/2]], [t, 0, 2*pi], legend: [\"a\", \"b\"])", "lissajous");
    let p = picture(&mut e, "polar_plot(1 + cos(theta), [theta, 0, 2*pi])", "cardioid");
    let j = sidecar(&p);
    assert!(has_colour_near(&p, pixel(&j, 2.0, 0.0), j["series"][0]["color"].as_str().unwrap()));
    picture(&mut e, "polar_plot([1, theta/(2*pi)], [theta, 0, 4*pi])", "polar_two");
    for (src, want) in [
        ("parametric_plot([t, t], [t, 1, 0])", "failure(argument, parametric_plot, 2)"),
        ("parametric_plot(t, [t, 0, 1])", "failure(argument, parametric_plot, 1)"),
        ("parametric_plot([[t]], [t, 0, 1])", "failure(argument, parametric_plot, 1)"),
        ("parametric_plot([a, b], [t, 0, 1])", "failure(argument, parametric_plot, 1)"),
        ("polar_plot(1, [t, 0, 1], filling: axis)", "image"),
    ] {
        expect(&mut e, src, want);
    }
}

#[test]
fn lines_of_data_and_fillings() {
    let mut e = CoreEngine::new();
    let p = picture(&mut e, "list_line_plot([1, 3, 2, 5, 4])", "list_line");
    let j = sidecar(&p);
    assert_eq!(j["series"][0]["kind"], "line");
    let p = picture(&mut e, "list_line_plot([[1, 2, 3, 2], [2, 1, 0, 1]], legend: [\"up\", \"down\"])", "list_lines");
    assert_eq!(sidecar(&p)["series"].as_array().unwrap().len(), 2);
    picture(&mut e, "list_plot([[[0, 1], [1, 2]], [[0, 2], [1, 0]]])", "list_two_sets");
    let p = picture(&mut e, "plot(sin(x), [x, 0, 2*pi], filling: axis, colors: [\"#1b5e63\"])", "filling_axis");
    // the filled region is a faint shade of the curve's colour between the curve and the axis
    let j = sidecar(&p);
    let pm = tiny_skia::Pixmap::load_png(image_path(&p)).unwrap();
    let (x, y) = pixel(&j, 1.5, 0.5);
    let c = pm.pixel(x as u32, y as u32).unwrap();
    assert!(c.red() < 235 && c.red() > 150 && c.blue() > c.red(), "{c:?}");
    let (x, y) = pixel(&j, 1.3, -0.6);
    assert_eq!(pm.pixel(x as u32, y as u32).unwrap().red(), 255);
    picture(&mut e, "plot([sin(x), cos(x)], [x, 0, 2*pi], filling: [1, 2])", "filling_between");
    picture(&mut e, "plot(exp(x), [x, 0, 3], filling: bottom, plot_range: [0, 10])", "filling_bottom");
    picture(&mut e, "list_line_plot([1, 3, 2, 5, 4], filling: 2)", "filling_value");
    expect(&mut e, "plot(x, [x, 0, 1], filling: [1, 2])", "failure(argument, plot, 3)");
    // options in rule form and of other plots are accepted
    expect(&mut e, "list_line_plot([1, 2], joined: false, plot_range: [1, 0])", "image");
}

#[test]
fn plot_ranges_clip_and_jumps_break_the_line() {
    let mut e = CoreEngine::new();
    let p = picture(&mut e, "plot(x^2, [x, -2, 2], plot_range: [0, 1])", "plot_range");
    let j = sidecar(&p);
    assert_eq!((j["y"]["min"].as_f64(), j["y"]["max"].as_f64()), (Some(0.0), Some(1.0)));
    let p = picture(&mut e, "plot(x, [x, 0, 10], plot_range: [[2, 4], [automatic, 5]])", "plot_range_both");
    let j = sidecar(&p);
    assert_eq!((j["x"]["min"].as_f64(), j["x"]["max"].as_f64(), j["y"]["max"].as_f64()), (Some(2.0), Some(4.0), Some(5.0)));
    // floor has a jump at every integer: the line is broken there, one piece per step
    let p = picture(&mut e, "plot(floor(x), [x, 0.5, 3.5])", "floor");
    assert_eq!(sidecar(&p)["series"].as_array().unwrap().len(), 4);
    // a steep but continuous curve is not broken
    let p = picture(&mut e, "plot(atan(50*x), [x, -1, 1])", "steep");
    assert_eq!(sidecar(&p)["series"].as_array().unwrap().len(), 1);
}

#[test]
fn density_and_contour_plots() {
    let mut e = CoreEngine::new();
    let p = picture(&mut e, "density_plot(sin(x)*cos(y), [x, 0, 2*pi], [y, 0, 2*pi])", "density");
    assert_eq!(png_size(&p), (540, 440));
    let j = sidecar(&p);
    // the colour at (pi/2, 0) is the top of viridis (yellow), at (3 pi/2, 0) its bottom (violet)
    let pm = tiny_skia::Pixmap::load_png(image_path(&p)).unwrap();
    let probe = |x: f64, y: f64| {
        let (px, py) = pixel(&j, x, y);
        let c = pm.pixel(px as u32, py as u32).unwrap();
        [c.red(), c.green(), c.blue()]
    };
    let top = probe(std::f64::consts::FRAC_PI_2, 0.1);
    assert!(top[0] > 200 && top[1] > 200 && top[2] < 90, "{top:?}");
    let bottom = probe(3.0 * std::f64::consts::FRAC_PI_2, 0.1);
    assert!(bottom[0] < 90 && bottom[1] < 30 && bottom[2] > 60, "{bottom:?}");
    let svg = sibling(image_path(&p), "svg");
    assert!(svg.contains("<image ") && svg.contains(r#"aria-label="0.4""#), "a raster and a colour bar");
    picture(&mut e, "density_plot(x^2 - y^2, [x, -1, 1], [y, -1, 1], color_function: cool_warm, color_bar: false)", "density_cool");
    picture(&mut e, "density_plot(sqrt(1 - x^2 - y^2), [x, -1, 1], [y, -1, 1], color_function: thermal, plot_points: 60)", "density_holes");
    let p = picture(&mut e, "contour_plot(x^2 + y^2, [x, -2, 2], [y, -2, 2])", "contour");
    let pm = tiny_skia::Pixmap::load_png(image_path(&p)).unwrap();
    let j = sidecar(&p);
    let (cx, cy) = pixel(&j, 0.0, 0.0);
    let (ex, ey) = pixel(&j, 1.9, 1.9);
    let (c0, c1) = (pm.pixel(cx as u32, cy as u32).unwrap(), pm.pixel(ex as u32, ey as u32).unwrap());
    // low values violet in the middle, high ones yellow in the corners
    assert!(c0.blue() > c0.green() && c1.green() > c1.blue(), "{c0:?} {c1:?}");
    picture(&mut e, "contour_plot(sin(x)*sin(y), [x, 0, 2*pi], [y, 0, 2*pi], contours: [-0.5, 0, 0.5], color_function: ocean)", "contour_levels");
    picture(&mut e, "contour_plot(x*y, [x, -1, 1], [y, -1, 1], contours: 6, color_function: [white, \"#1b5e63\"])", "contour_saddle");
    let p = picture(&mut e, "list_density_plot([[1, 2, 3], [2, 4, 2], [3, 2, 1]])", "list_density");
    let j = sidecar(&p);
    assert_eq!((j["x"]["min"].as_f64(), j["x"]["max"].as_f64()), (Some(1.0), Some(3.0)));
    let p = picture(&mut e, "list_density_plot(table(sin(i/3)*cos(k/5), i, 30, k, 40), data_range: [[0, 1], [0, 2]])", "list_density_table");
    assert_eq!(sidecar(&p)["y"]["max"].as_f64(), Some(2.0));
    for (src, want) in [
        ("density_plot(x, [x, 0, 1], [x, 0, 1])", "failure(argument, density_plot, 3)"),
        ("density_plot(x, [x, 0, 1], [y, 1, 0])", "failure(argument, density_plot, 3)"),
        ("density_plot(a, [x, 0, 1], [y, 0, 1])", "failure(argument, density_plot, 1)"),
        ("density_plot(x, [x, 0, 1], [y, 0, 1], contours: 3)", "image"),
        ("contour_plot(x, [x, 0, 1], [y, 0, 1], contours: 0)", "image"),
        ("contour_plot(x, [x, 0, 1], [y, 0, 1], plot_points: 1)", "image"),
        ("list_density_plot([1, 2])", "failure(argument, list_density_plot, 1)"),
        ("list_density_plot([[1, 2], [3]])", "failure(argument, list_density_plot, 1)"),
        ("list_density_plot([[1, 2], [3, 4]], data_range: [[1, 0], [0, 1]])", "image"),
    ] {
        expect(&mut e, src, want);
    }
}

#[test]
fn histograms_and_bar_charts() {
    let mut e = CoreEngine::new();
    eval(&mut e, "random_seed(7)");
    let p = picture(&mut e, "histogram(random_variate(normal_distribution(0, 1), 2000))", "histogram");
    let j = sidecar(&p);
    let bars = j["series"][0]["points"].as_array().unwrap();
    let total: f64 = bars.iter().map(|b| b[1].as_f64().unwrap()).sum();
    assert_eq!(total, 2000.0);
    assert!((8..=40).contains(&bars.len()), "{}", bars.len());
    let p = picture(&mut e, "histogram([1, 2, 2, 3, 3, 3], bins: [0.5, 3.5, 1])", "histogram_explicit");
    let heights: Vec<f64> = sidecar(&p)["series"][0]["points"].as_array().unwrap().iter().map(|b| b[1].as_f64().unwrap()).collect();
    assert_eq!(heights, [1.0, 2.0, 3.0]);
    let p = picture(&mut e, "histogram([1, 2, 2, 3, 3, 3], bins: [1], normalized: true)", "histogram_density");
    let area: f64 = sidecar(&p)["series"][0]["points"].as_array().unwrap().iter().map(|b| b[1].as_f64().unwrap()).sum();
    assert!((area - 1.0).abs() < 1e-12);
    picture(&mut e, "histogram([random_variate(normal_distribution(0, 1), 500), random_variate(normal_distribution(2, 1), 500)], bins: 20, legend: [\"a\", \"b\"])", "histogram_two");
    let p = picture(&mut e, "bar_chart([3, 1, 4, 1, 5], labels: [\"a\", \"b\", \"c\", \"d\", \"e\"])", "bar_chart");
    let j = sidecar(&p);
    assert_eq!((j["series"][0]["points"][4][0].as_f64(), j["series"][0]["points"][4][1].as_f64()), (Some(5.0), Some(5.0)));
    let color = j["series"][0]["color"].as_str().unwrap().to_string();
    assert!(has_colour_near(&p, pixel(&j, 3.0, 2.0), &color));
    assert!(sibling(image_path(&p), "svg").contains(r#"aria-label="c""#));
    picture(&mut e, "bar_chart([[1, 2, 3], [2, 3, 1], [3, 1, 2], [-1, 2, 0.5]], legend: [\"x\", \"y\", \"z\"])", "bar_groups");
    for (src, want) in [
        ("histogram([])", "failure(argument, histogram, 1)"),
        ("histogram([1, a])", "failure(argument, histogram, 1)"),
        ("histogram([1, 2], bins: 0)", "image"),
        ("histogram([1, 2], bins: [1.0e-9])", "failure(argument, histogram, 2)"),
        ("bar_chart(5)", "failure(argument, bar_chart, 1)"),
        ("bar_chart([])", "failure(argument, bar_chart, 1)"),
        ("bar_chart([1, 2], bins: 3)", "image"),
    ] {
        expect(&mut e, src, want);
    }
}
