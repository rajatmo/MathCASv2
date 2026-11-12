//! Plots fed by the values other functions make: solve rules, n_dsolve solutions, interpolations, quantities, around
//! values, intervals, datasets, associations, distributions, sparse arrays, piecewise functions, and wrapped curves
//! (legended, labeled, callout, style). Each plot is checked through its data sidecar.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// The data sidecar of the plot `src` evaluates to (it must be a plot).
fn sidecar(e: &mut CoreEngine, src: &str) -> serde_json::Value {
    let printed = eval(e, src);
    let path = printed.strip_prefix("image(\"").and_then(|s| s.strip_suffix("\")")).unwrap_or_else(|| panic!("{src} -> {printed}"));
    let json = std::path::Path::new(path).with_extension("json");
    serde_json::from_slice(&std::fs::read(json).expect("the sidecar exists")).expect("the sidecar is JSON")
}

/// The series of a sidecar as (name, points).
fn series(j: &serde_json::Value) -> Vec<(String, Vec<(f64, f64)>)> {
    j["series"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let pts = s["points"].as_array().unwrap().iter().map(|p| (p[0].as_f64().unwrap(), p[1].as_f64().unwrap())).collect();
            (s["name"].as_str().unwrap_or("").to_string(), pts)
        })
        .collect()
}

/// The PNG of a plot as an SVG sibling's text.
fn svg(e: &mut CoreEngine, src: &str) -> String {
    let printed = eval(e, src);
    let path = printed.strip_prefix("image(\"").and_then(|s| s.strip_suffix("\")")).unwrap_or_else(|| panic!("{src} -> {printed}"));
    std::fs::read_to_string(std::path::Path::new(path).with_extension("svg")).unwrap()
}

#[test]
fn rules_and_solutions_are_plotted() {
    let mut e = CoreEngine::new();
    eval(&mut e, "sol = solve(x^2 - 9 == 0, x)");
    // x^2 /. sol: one curve per solution (both 9)
    let s = series(&sidecar(&mut e, "plot(t*x /. sol, [t, 0, 1])"));
    assert_eq!(s.len(), 2);
    let ends: Vec<f64> = s.iter().map(|c| c.1.last().unwrap().1).collect();
    assert!(ends.iter().any(|y| (y + 3.0).abs() < 1e-9) && ends.iter().any(|y| (y - 3.0).abs() < 1e-9), "{ends:?}");
    // an n_dsolve solution: the curve, named after the function
    let s = series(&sidecar(&mut e, "plot(y(t) /. n_dsolve([y'(t) == -y(t), y(0) == 1], y, [t, 0, 2]), [t, 0, 2])"));
    assert_eq!(s[0].0, "y(t)");
    let (x, y) = *s[0].1.last().unwrap();
    assert!((x - 2.0).abs() < 1e-12 && (y - (-2.0f64).exp()).abs() < 1e-4, "{x} {y}");
    // an interpolation
    eval(&mut e, "f = interpolation([[0, 0], [1, 1], [2, 4], [3, 9]])");
    let s = series(&sidecar(&mut e, "plot(f(x), [x, 0, 3])"));
    assert!((s[0].1.last().unwrap().1 - 9.0).abs() < 1e-9);
}

#[test]
fn quantities_put_their_units_on_the_axes() {
    let mut e = CoreEngine::new();
    // a list of quantities: magnitudes in the first one's unit, the unit as the y label
    let svg_text = svg(&mut e, "list_plot([quantity(1, \"m\"), quantity(30, \"cm\"), quantity(2, \"m\")])");
    assert!(svg_text.contains("aria-label=\"m\""), "unit label");
    let s = series(&sidecar(&mut e, "list_plot([quantity(1, \"m\"), quantity(30, \"cm\"), quantity(2, \"m\")])"));
    let ys: Vec<f64> = s[0].1.iter().map(|p| p.1).collect();
    assert!((ys[1] - 0.3).abs() < 1e-12, "{ys:?}");
    // quantity(list, unit) and a function with a unit, over an interval with a unit
    let s = series(&sidecar(&mut e, "list_line_plot(quantity([1, 4, 9], \"s\"))"));
    assert_eq!(s[0].1.len(), 3);
    let s = series(&sidecar(&mut e, "plot(quantity(x^2, \"m\"), [x, quantity(0, \"s\"), quantity(2, \"s\")])"));
    assert!((s[0].1.last().unwrap().1 - 4.0).abs() < 1e-12);
    // charts of quantities
    assert!(eval(&mut e, "bar_chart([quantity(1, \"m\"), quantity(2, \"m\")])").starts_with("image("));
    assert!(eval(&mut e, "histogram([quantity(1, \"m\"), quantity(3, \"m\"), quantity(2, \"m\")])").starts_with("image("));
}

#[test]
fn around_values_and_intervals_get_error_bars() {
    let mut e = CoreEngine::new();
    let s = series(&sidecar(&mut e, "list_plot([around(1, 0.1), around(2, 0.2), interval([2, 4])])"));
    let ys: Vec<f64> = s[0].1.iter().map(|p| p.1).collect();
    assert_eq!(ys, vec![1.0, 2.0, 3.0]);
    // the bars widen the range to their ends: from 0.9 to 4 before padding
    let j = sidecar(&mut e, "list_plot([around(1, 0.1), around(2, 0.2), interval([2, 4])])");
    let (lo, hi) = (j["y"]["min"].as_f64().unwrap(), j["y"]["max"].as_f64().unwrap());
    assert!(lo < 0.9 && hi > 4.0, "{lo} {hi}");
    // x error bars in pairs
    let j = sidecar(&mut e, "list_plot([[around(1, 0.5), 1], [2, 2]])");
    assert!(j["x"]["min"].as_f64().unwrap() < 0.5);
}

#[test]
fn datasets_and_associations_name_the_data() {
    let mut e = CoreEngine::new();
    // two columns: x and y, the column names as axis labels
    let text = svg(&mut e, "list_plot(dataset(assoc(\"t\" -> [1, 2, 3], \"u\" -> [2, 4, 5])))");
    assert!(text.contains("aria-label=\"t\"") && text.contains("aria-label=\"u\""));
    // three columns: two data sets against the first, named by their columns
    let s = series(&sidecar(&mut e, "list_plot(dataset(assoc(\"t\" -> [1, 2, 3], \"u\" -> [2, 4, 5], \"v\" -> [1, 1, 2])))"));
    assert_eq!(s.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), vec!["u", "v"]);
    // an association of lists: a data set per key, the keys as the legend
    let s = series(&sidecar(&mut e, "list_line_plot(assoc(\"a\" -> [1, 2, 3], \"b\" -> [3, 2, 1]))"));
    assert_eq!(s.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
    // bar charts and pie charts of associations: the keys as labels
    let text = svg(&mut e, "bar_chart(assoc(\"left\" -> 1, \"right\" -> 2))");
    assert!(text.contains("aria-label=\"left\"") && text.contains("aria-label=\"right\""));
    assert!(eval(&mut e, "pie_chart(assoc(\"a\" -> 1, \"b\" -> 2, \"c\" -> 5))").starts_with("image("));
    // histograms of an association of lists: one per key, in the legend
    let s = series(&sidecar(&mut e, "histogram(assoc(\"one\" -> [1, 2, 2, 3], \"two\" -> [3, 4, 4, 5]))"));
    assert_eq!(s.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), vec!["one", "two"]);
}

#[test]
fn distributions_overlay_histograms() {
    let mut e = CoreEngine::new();
    // a density histogram and the pdf on one plot
    eval(&mut e, "data = [1.2, 0.3, -0.5, 0.8, -1.1, 0.1, 0.4, -0.2, 1.6, -0.7]");
    let j = sidecar(&mut e, "histogram(data, [-2, 2, 1], \"PDF\")");
    let total: f64 = series(&j)[0].1.iter().map(|p| p.1).sum();
    assert!((total - 1.0).abs() < 1e-12, "density bars of width 1 sum to 1: {total}");
    let shown = eval(&mut e, "show(histogram(data, [-2, 2, 1], pdf), plot(pdf(normal_distribution(0, 1), x), [x, -2, 2]))");
    assert!(shown.starts_with("image("), "{shown}");
    // other heights; cumulative ends at 1
    let s = series(&sidecar(&mut e, "histogram(data, [-2, 2, 1], cdf)"));
    assert!((s[0].1.last().unwrap().1 - 1.0).abs() < 1e-12);
    assert_eq!(eval(&mut e, "histogram(data, automatic, \"bogus\")"), "failure(argument, histogram, 3)");
    // a discrete distribution at the integers
    let s = series(&sidecar(&mut e, "discrete_plot(pdf(binomial_distribution(4, 1/2), k), [k, 0, 4])"));
    assert!((s[0].1[2].1 - 0.375).abs() < 1e-12);
}

#[test]
fn integer_functions_in_discrete_plots() {
    let mut e = CoreEngine::new();
    let s = series(&sidecar(&mut e, "discrete_plot(prime(n), [n, 5])"));
    assert_eq!(s[0].1.iter().map(|p| p.1).collect::<Vec<_>>(), vec![2.0, 3.0, 5.0, 7.0, 11.0]);
    let s = series(&sidecar(&mut e, "discrete_plot(sum(1/k^2, [k, 1, n]), [n, 1, 3])"));
    assert!((s[0].1[2].1 - 49.0 / 36.0).abs() < 1e-12);
}

#[test]
fn wrapped_curves_give_legends_labels_and_styles() {
    let mut e = CoreEngine::new();
    // legended and labeled stay as they are around functions and data, for the plots
    assert_eq!(eval(&mut e, "legended(cos(x), \"c\")"), "legended(cos(x), \"c\")");
    assert_eq!(eval(&mut e, "labeled([1, 2], \"d\")"), "labeled([1, 2], \"d\")");
    let text = svg(&mut e, "plot([sin(x), legended(cos(x), \"cosine\")], [x, 0, 1])");
    assert!(text.contains("aria-label=\"cosine\""));
    let text = svg(&mut e, "list_plot([labeled([1, 2, 3], \"up\"), labeled([3, 2, 1], \"down\")])");
    assert!(text.contains("aria-label=\"up\"") && text.contains("aria-label=\"down\""));
    // style(f, colour): that curve in that colour
    let text = svg(&mut e, "plot([style(sin(x), red), cos(x)], [x, 0, 1])");
    assert!(text.contains("#ff0000"), "red curve");
    // automatic in a plot_style list keeps the curve's own style
    assert!(eval(&mut e, "plot([sin(x), 1], [x, 0, 1], plot_style: [automatic, dashed])").starts_with("image("));
    // callout around whole curves of polar, list and 3D plots
    assert!(eval(&mut e, "polar_plot(callout(1 + cos(t)), [t, 0, 2*pi])").starts_with("image("));
    assert!(eval(&mut e, "list_line_plot(callout(prime(range(10)), \"primes\", 4))").starts_with("image("));
    assert!(eval(&mut e, "parametric_plot3d(callout([cos(u), sin(u), u/5], \"helix\"), [u, 0, 6])").starts_with("image("));
    let s = series(&sidecar(&mut e, "list_polar_plot([range(5), 2*range(5)])"));
    assert_eq!(s.len(), 2);
}

#[test]
fn charts_skip_values_that_are_not_numbers() {
    let mut e = CoreEngine::new();
    let s = series(&sidecar(&mut e, "histogram([1, 2, 3, none, 3, missing(), foo, 2])"));
    assert!(!s[0].1.is_empty());
    assert!(eval(&mut e, "bar_chart([[1, missing(), 3], [4, 2, 1 + im]])").starts_with("image("));
    assert!(eval(&mut e, "pie_chart([1, legended(2, \"two\"), 3])").starts_with("image("));
    assert!(eval(&mut e, "pie_chart([[1, 2, 3], [1, 1, 1]])").starts_with("image("));
    assert_eq!(eval(&mut e, "bar_chart([missing(), foo])"), "failure(argument, bar_chart, 1)");
}

#[test]
fn epilog_points_may_be_exact() {
    let mut e = CoreEngine::new();
    // exact coordinates (a root of the plotted function) are made numbers, as graphics does
    let p = eval(&mut e, "plot(x^2 - 2, [x, 0, 2], epilog: [red, point([sqrt(2), 0]), text(\"root\", [sqrt(2), 1/2])])");
    assert!(p.starts_with("image("), "{p}");
    eval(&mut e, "r = solve(x^3 - 2 == 0, x, reals)");
    let p = eval(&mut e, "plot(x^3 - 2, [x, 0, 2], epilog: point([x /. first(r), 0]))");
    assert!(p.starts_with("image("), "{p}");
}

#[test]
fn sparse_arrays_and_piecewise_functions() {
    let mut e = CoreEngine::new();
    assert!(eval(&mut e, "matrix_plot(sparse_array([[1, 1] -> 1, [3, 3] -> 2], [3, 3]))").starts_with("image("));
    assert!(eval(&mut e, "array_plot(sparse_array([[1, 2] -> 1], [2, 2]))").starts_with("image("));
    // a jump of a piecewise function breaks the line
    let s = series(&sidecar(&mut e, "plot(piecewise([[1, x < 0]], 2), [x, -1, 1])"));
    assert_eq!(s.len(), 2);
}

#[test]
fn graphics_written_and_read_back() {
    let mut e = CoreEngine::new();
    let dir = std::env::temp_dir().join(format!("mc-plot-interop-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for ext in ["png", "svg"] {
        let path = dir.join(format!("g.{ext}")).to_string_lossy().replace('\\', "/");
        let out = eval(&mut e, &format!("export_data(\"{path}\", graphics([red, disk([0, 0], 1)], frame: true))"));
        assert_eq!(out, format!("\"{path}\""));
        let h = mc_core::Heap::default();
        let g = mc_plot::import_graphics(&h, std::path::Path::new(&path)).expect("the term is embedded");
        assert_eq!(mc_syntax::Printer::new(&h).expr(g), "graphics([red, disk([0, 0], 1)], frame: true)", "{ext}");
    }
    // a plot (an image of a remembered graphic) written as SVG carries its graphic too
    let path = dir.join("p.svg").to_string_lossy().replace('\\', "/");
    eval(&mut e, &format!("export_data(\"{path}\", show(plot(x, [x, 0, 1])))"));
    let h = mc_core::Heap::default();
    assert!(mc_plot::import_graphics(&h, std::path::Path::new(&path)).is_some());
    let _ = std::fs::remove_dir_all(&dir);
}
