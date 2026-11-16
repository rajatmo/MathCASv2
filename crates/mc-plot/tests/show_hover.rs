//! Pictures that combine plots (`show`): their data sidecar carries the series of every plot, in the coordinates
//! of the combined picture, and their axes are marked afresh for the combined range. Plots draw no grid lines
//! unless asked.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use serde_json::Value;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// The data sidecar of the picture `src` evaluates to.
fn sidecar(e: &mut CoreEngine, src: &str) -> Value {
    let p = eval(e, src);
    let path = p.strip_prefix("image(\"").and_then(|s| s.strip_suffix("\")")).unwrap_or_else(|| panic!("{src} -> {p}"));
    let text = std::fs::read_to_string(std::path::Path::new(path).with_extension("json")).expect("a sidecar");
    serde_json::from_str(&text).expect("JSON")
}

/// The series entries of a sidecar: (name, kind, points).
fn series(v: &Value) -> Vec<(String, String, Vec<[f64; 2]>)> {
    v["series"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let pts = s["points"].as_array().unwrap().iter().map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()]).collect();
            (s["name"].as_str().unwrap().to_string(), s["kind"].as_str().unwrap().to_string(), pts)
        })
        .collect()
}

/// The value of option `key` of the graphic `src` evaluates to, printed (empty when it has none).
fn opt(e: &mut CoreEngine, src: &str, key: &str) -> String {
    let o = eval(e, &format!("input_form(last({src}))"));
    let Some(at) = o.find(&format!("{key} -> ")) else { return String::new() };
    let rest = &o[at + key.len() + 4..];
    let mut depth = 0i32;
    for (i, c) in rest.char_indices() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' if depth == 0 => return rest[..i].to_string(),
            ']' | ')' => depth -= 1,
            ',' if depth == 0 => return rest[..i].to_string(),
            _ => {}
        }
    }
    rest.to_string()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs().max(1.0)
}

fn bound(start: std::time::Instant) {
    assert!(start.elapsed().as_secs() < 60, "took {:?}", start.elapsed());
}

#[test]
fn a_shown_plot_carries_the_series_of_every_plot() {
    let t = std::time::Instant::now();
    let mut e = CoreEngine::new();
    let v = sidecar(&mut e, "show(plot(sin(x), [x, 0, 6]), list_plot([[1, 0.5], [2, 0.7]]))");
    let s = series(&v);
    let line = s.iter().find(|e| e.1 == "line").expect("the curve");
    assert!(line.2.len() > 20 && line.2.iter().all(|p| p[0] >= 0.0 && p[0] <= 6.0 && close(p[1], p[0].sin())), "{:?}", &line.2[..3]);
    let pts = s.iter().find(|e| e.1 == "points").expect("the points");
    assert_eq!(pts.2, vec![[1.0, 0.5], [2.0, 0.7]]);
    assert_eq!(v["x"]["scale"], "linear");
    // the combined range holds both
    assert!(v["x"]["min"].as_f64().unwrap() <= 0.0 && v["x"]["max"].as_f64().unwrap() >= 6.0);
    bound(t);
}

#[test]
fn a_shown_log_plot_keeps_its_scale_and_data_values() {
    let t = std::time::Instant::now();
    let mut e = CoreEngine::new();
    let v = sidecar(&mut e, "show(log_plot(exp(x), [x, 0, 3]), list_log_plot([[1, 10], [2, 100]]))");
    assert_eq!(v["y"]["scale"], "log");
    let s = series(&v);
    let line = s.iter().find(|e| e.1 == "line").expect("the curve");
    assert!(line.2.iter().all(|p| close(p[1], p[0].exp())), "{:?}", &line.2[..3]);
    let pts = s.iter().find(|e| e.1 == "points").expect("the points");
    assert!(pts.2.len() == 2 && close(pts.2[0][1], 10.0) && close(pts.2[1][1], 100.0), "{:?}", pts.2);
    bound(t);
}

#[test]
fn a_shown_plot_with_its_own_range_hovers_in_that_range() {
    let t = std::time::Instant::now();
    let mut e = CoreEngine::new();
    let v = sidecar(&mut e, "show(plot(sin(x), [x, 0, 6]), plot_range: [[1, 3], [-2, 2]])");
    assert_eq!((v["x"]["min"].as_f64(), v["x"]["max"].as_f64()), (Some(1.0), Some(3.0)));
    assert_eq!((v["y"]["min"].as_f64(), v["y"]["max"].as_f64()), (Some(-2.0), Some(2.0)));
    assert!(!series(&v).is_empty());
    // a log plot shown with a range in data values: the range placed on the log axis, the sidecar in data values
    let v = sidecar(&mut e, "show(log_plot(exp(x), [x, 0, 3]), plot_range: [[0, 3], [1, 1000]])");
    assert!(close(v["y"]["min"].as_f64().unwrap(), 1.0) && close(v["y"]["max"].as_f64().unwrap(), 1000.0), "{}", v["y"]);
    bound(t);
}

#[test]
fn a_single_plot_hovers_as_before() {
    let t = std::time::Instant::now();
    let mut e = CoreEngine::new();
    let v = sidecar(&mut e, "plot(sin(x), [x, 0, 6])");
    let s = series(&v);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].0, "sin(x)");
    assert!(s[0].2.iter().all(|p| close(p[1], p[0].sin())));
    bound(t);
}

#[test]
fn shown_plots_are_ticked_for_the_combined_range() {
    let t = std::time::Instant::now();
    let mut e = CoreEngine::new();
    eval(&mut e, "small = log_plot(exp(x), [x, 0, 3]); large = log_plot(exp(x), [x, 0, 20]);");
    // the larger plot's range holds the smaller one's: the combined picture is ticked as the larger plot alone,
    // with no tick left from the smaller plot's range
    let both = opt(&mut e, "show(small, large)", "ticks");
    let alone = opt(&mut e, "show(large)", "ticks");
    assert_eq!(both, alone);
    // explicit ticks of a plot survive
    let o = opt(&mut e, "show(plot(x, [x, 0, 1], ticks: [[0, 0.5, 1], automatic]), plot(2 x, [x, 0, 3]))", "ticks");
    assert!(o.contains("0.5"), "{o}");
    // a range given to show: the log axis ticked for it (10^0 to 10^3)
    let o = opt(&mut e, "show(small, plot_range: [[0, 3], [1, 1000]])", "ticks");
    assert!(o.contains("superscript(\"10\", \"3\")") && !o.contains("superscript(\"10\", \"4\")"), "{o}");
    bound(t);
}

#[test]
fn plots_have_no_grid_lines_unless_asked() {
    let t = std::time::Instant::now();
    let mut e = CoreEngine::new();
    let none = opt(&mut e, "show(plot(sin(x), [x, 0, 6]))", "grid_lines");
    assert!(none.is_empty(), "{none}");
    let auto = opt(&mut e, "show(plot(sin(x), [x, 0, 6], grid_lines: automatic))", "grid_lines");
    assert!(auto.contains("[automatic, automatic]"), "{auto}");
    // on a log axis `automatic` gives the powers of ten, as the default did before
    let log = opt(&mut e, "show(log_plot(exp(x), [x, 0, 10], grid_lines: automatic))", "grid_lines");
    assert!(log.contains("opacity(0.5)"), "{log}");
    // a plot that asks for a grid gives the combined picture one
    let both = opt(&mut e, "show(plot(sin(x), [x, 0, 6], grid_lines: automatic), plot(x, [x, 0, 9]))", "grid_lines");
    assert!(both.contains("[automatic, automatic]"), "{both}");
    bound(t);
}
