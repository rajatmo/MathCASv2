//! Regressions of the plot gaps found in the user's notebooks in round 13 (gap queue G83 to G90).
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
    printed.strip_prefix("image(\"").and_then(|s| s.strip_suffix("\")")).unwrap_or_else(|| panic!("{printed}"))
}

/// The data sidecar of a plot.
fn sidecar(printed: &str) -> serde_json::Value {
    let p = std::path::Path::new(image_path(printed)).with_extension("json");
    serde_json::from_str(&std::fs::read_to_string(p).expect("sidecar")).expect("json")
}

/// The graphics term a plot was drawn from (remembered for the session).
fn term(printed: &str) -> String {
    mc_graphics::compose::recall(image_path(printed)).expect("remembered")
}

/// G83: the colours of `line_legend([colours], [labels])` are the legend's, whatever the curves' colours.
#[test]
fn legends_keep_their_own_colours() {
    let mut e = CoreEngine::new();
    let p = eval(&mut e, "plot([x, 2*x], [x, 0, 1], plot_style: [blue, red], plot_legends: line_legend([red, blue], [\"a\", \"b\"]))");
    let t = term(&p);
    let legend = &t[t.find("legend: ").expect("a legend")..];
    let (a, b) = (legend.find("\"a\"").expect("entry a"), legend.find("\"b\"").expect("entry b"));
    // the last colour before each label is the legend's
    let last_colour = |s: &str| s.rfind("rgb_color(").map(|k| s[k..].split(')').next().unwrap_or("").to_string()).unwrap_or_default();
    let (ca, cb) = (last_colour(&legend[..a]), last_colour(&legend[a..b]));
    assert!(ca.starts_with("rgb_color(1.0, 0.0, 0.0") || ca.starts_with("rgb_color(1, 0, 0"), "a: {ca} in {legend}");
    assert!(cb.starts_with("rgb_color(0.0, 0.0, 1.0") || cb.starts_with("rgb_color(0, 0, 1"), "b: {cb} in {legend}");
    // the curves keep theirs: the first curve is blue
    let curves = &t[..t.find("legend: ").unwrap()];
    assert!(curves.find("rgb_color(0.0, 0.0, 1.0").or_else(|| curves.find("rgb_color(0, 0, 1")) < curves.find("rgb_color(1.0, 0.0, 0.0").or_else(|| curves.find("rgb_color(1, 0, 0")));
    // swatch legends take theirs as swatches
    let p = eval(&mut e, "plot([x, 2*x], [x, 0, 1], plot_legends: swatch_legend([green, orange], [\"a\", \"b\"]))");
    assert!(term(&p).contains("swatch"), "{}", term(&p));
}

/// G85: a framed column inset is fitted to its text.
#[test]
fn framed_columns_fit_their_text() {
    let mut e = CoreEngine::new();
    let printed = eval(&mut e, "to_string(framed(column([\"a\", \"bbbbbbbbbbbbbbbbbbbb\"]), frame_style: black))");
    let text = printed.trim_matches('"').replace("\\\"", "\"");
    let h = mc_core::Heap::new(mc_core::Config::default());
    let t = mc_syntax::read(&h, &text).expect("reads")[0];
    let (w, ht) = mc_graphics::page_size(&h, t).expect("a graphic");
    let label = mc_graphics::label::typeset(&mc_graphics::label::Label::text("bbbbbbbbbbbbbbbbbbbb"), 13.0);
    let margin = w - label.width;
    assert!((4.0..=20.0).contains(&margin), "width {w} for text {}", label.width);
    assert!(ht < 2.0 * 2.0 * label.height + 20.0, "height {ht}");
    // inside a plot near its left edge, the box stays inside the frame
    let p = eval(
        &mut e,
        "plot(x, [x, 0, 1], image_size: 360, font_size: 10, epilog: inset(framed(column([\"$\\\\lambda = 1.\\\\times 10^{-19}$\", \"$f = 5.18\\\\times 10^{11}$\"]), frame_style: black), scaled([0.15, 0.85])))",
    );
    assert!(p.starts_with("image(\""), "{p}");
}

/// G88: exact thresholds of signed-log axes; a curve crossing 0 is one line.
#[test]
fn signed_log_takes_exact_thresholds_and_crosses_zero() {
    let mut e = CoreEngine::new();
    let p = eval(&mut e, "plot(x^3 - x, [x, 0.001, 1000], x_scale: log, y_scale: signed_log(10^-8))");
    let j = sidecar(&p);
    assert_eq!(j["y"]["scale"], "signed_log");
    assert!((j["y"]["c"].as_f64().unwrap() - 1e-8).abs() < 1e-20);
    assert_eq!((j["y"]["a"].as_f64(), j["y"]["k"].as_f64()), (Some(32.0), Some(1e8)));
    // one entry: the zero crossing at x = 1 does not break the curve
    assert_eq!(j["series"].as_array().unwrap().len(), 1, "{}", j["series"]);
    let p = eval(&mut e, "plot(sin(x), [x, -5, 5], y_scale: signed_log(1/1000))");
    assert_eq!(sidecar(&p)["series"].as_array().unwrap().len(), 1);
    // the near-zero values are drawn just inside the threshold's level, not on the axis
    let pts = sidecar(&p)["series"][0]["points"].as_array().unwrap().clone();
    assert!(pts.iter().any(|q| q[1].as_f64().unwrap().abs() < 1e-4));
    // what is not a positive real is still refused
    for bad in ["signed_log(0)", "signed_log(-1)", "signed_log(a)"] {
        let p = eval(&mut e, &format!("plot(x, [x, 0, 1], y_scale: {bad})"));
        assert!(p.starts_with("failure(") || p.starts_with("image("), "{bad}: {p}");
    }
}

/// G90: log plots refine at steep ends and keep spikes in proportion.
#[test]
fn log_plots_refine_ends_and_cut_spikes() {
    let mut e = CoreEngine::new();
    // a curve that starts at 0 (log 0 has no place) rises from about 1e-3 at the first point inside
    let j = sidecar(&eval(&mut e, "log_plot(1 - exp(-1000*z), [z, 0, 54])"));
    let lo = j["y"]["min"].as_f64().unwrap();
    assert!(lo < 2e-3 && lo > 1e-5, "y from {lo}");
    // the spikes of 1/sin^2: a range of a few thousand, not the value next to a pole
    let j = sidecar(&eval(&mut e, "log_plot(1/sin(5*z)^2, [z, 1, 54])"));
    let hi = j["y"]["max"].as_f64().unwrap();
    assert!((1e3..1e5).contains(&hi), "y to {hi}");
    // a minimum at 0 inside: not the value of the sample nearest 0
    let j = sidecar(&eval(&mut e, "log_plot(x^2, [x, -1, 1])"));
    let lo = j["y"]["min"].as_f64().unwrap();
    assert!((1e-6..1e-3).contains(&lo), "y from {lo}");
    // smooth ends stay exact
    let j = sidecar(&eval(&mut e, "log_plot(exp(-x^2), [x, -5, 5])"));
    let lo = j["y"]["min"].as_f64().unwrap();
    assert!(lo <= (-25f64).exp() && lo > 1e-12, "y from {lo}");
}

/// G64: discrete_plot of functions of exact integers (gcd, mod, binomial) and of exact or symbolic values.
#[test]
fn discrete_plots_of_exact_values() {
    let mut e = CoreEngine::new();
    let j = sidecar(&eval(&mut e, "discrete_plot(gcd(12, n), [n, 100])"));
    let pts = j["series"][0]["points"].as_array().unwrap().clone();
    assert_eq!(pts.len(), 100);
    for q in &pts {
        let (n, y) = (q[0].as_f64().unwrap() as u64, q[1].as_f64().unwrap());
        let g = (1..=12u64).filter(|d| 12 % d == 0 && n % d == 0).max().unwrap();
        assert_eq!(y, g as f64, "gcd(12, {n})");
    }
    // exact rationals and roots
    let j = sidecar(&eval(&mut e, "discrete_plot([binomial(10, k)/2^10, sqrt(k)], [k, 0, 10])"));
    let s = j["series"].as_array().unwrap();
    assert!((s[0]["points"][5][1].as_f64().unwrap() - 252.0 / 1024.0).abs() < 1e-15);
    assert!((s[1]["points"][2][1].as_f64().unwrap() - 2f64.sqrt()).abs() < 1e-15);
}
