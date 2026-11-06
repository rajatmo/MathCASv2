//! Graphics values combined across functions: `show` of 3D graphics with 3D plots, `show` of plots with graphics,
//! layouts of plots, labels and legends that wrap non-graphics (left for the plots), coordinates computed by other
//! functions.
use mc_bench::{Engine, StopFlag};
use mc_engine::CoreEngine;

/// The value's printed term (a graphics value is shown as its picture, so the term is printed through `to_string`).
fn term(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(&format!("to_string({src})"), &StopFlag::new()) {
        Ok(v) => match v.printed.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            Some(s) => s.replace("\\\"", "\"").replace("\\\\", "\\"),
            None => v.printed,
        },
        Err(err) => format!("error: {err}"),
    }
}

fn printed(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

#[test]
fn show_combines_three_dimensional_graphics() {
    let mut e = CoreEngine::new();
    let t = term(&mut e, "show(graphics3d(point([0, 0, 0])), graphics3d([red, point([1, 1, 2])]), axes: true)");
    assert_eq!(t, "graphics3d([point([0, 0, 0]), [red, point([1, 1, 2])]], axes: true)");
    // a 3D plot with a 3D graphic: one 3D picture whose box holds both
    let p = printed(&mut e, "show(plot3d(x*y, [x, 0, 1], [y, 0, 1], plot_points: 5), graphics3d([red, point([0, 0, 2])]))");
    assert!(p.starts_with("image(\""), "{p}");
    // 2D and 3D do not mix
    assert_eq!(printed(&mut e, "show(graphics(disk()), graphics3d(point([0, 0, 0])))"), "failure(argument, show, 2)");
}

#[test]
fn coordinates_from_other_functions_are_numbers() {
    let mut e = CoreEngine::new();
    // exact values, roots, and a complex number whose imaginary part is a rounding error
    assert_eq!(term(&mut e, "graphics(point([sqrt(2), 1/2]))"), "graphics(point([1.4142135623730951, 1/2]))");
    assert_eq!(term(&mut e, "graphics(point([x /. first(solve(x^2 == 2, x, reals)), 0]))"), "graphics(point([-1.4142135623730951, 0]))");
    assert_eq!(term(&mut e, "graphics(point([1, complex(2.0, 1.0e-17)]))"), "graphics(point([1, 2.0]))");
}

#[test]
fn labels_and_legends_of_non_graphics_stay() {
    let mut e = CoreEngine::new();
    // a function or data, to be read by a plot
    assert_eq!(printed(&mut e, "legended(sin(x), \"sine\")"), "legended(sin(x), \"sine\")");
    assert_eq!(printed(&mut e, "labeled(sin(x), \"sine\")"), "labeled(sin(x), \"sine\")");
    assert_eq!(printed(&mut e, "labeled([1, 2, 3], \"data\")"), "labeled([1, 2, 3], \"data\")");
    assert_eq!(printed(&mut e, "labeled(5, 5)"), "labeled(5, 5)");
    // graphics and text are laid out with the label
    assert!(term(&mut e, "labeled(graphics(disk()), \"disk\")").starts_with("graphics("));
    assert!(term(&mut e, "labeled(\"text\", \"label\")").starts_with("graphics("));
    assert!(term(&mut e, "legended(graphics(disk()), [\"a\"])").starts_with("graphics("));
}

#[test]
fn plots_and_graphics_lay_out_together() {
    let mut e = CoreEngine::new();
    for src in [
        "graphics_row([plot(x, [x, 0, 1]), graphics(disk()), plot3d(x + y, [x, 0, 1], [y, 0, 1], plot_points: 5)])",
        "graphics_grid([[plot(x, [x, 0, 1]), graphics(circle())], [histogram([1, 2, 2, 3]), bar_chart([1, 2])]])",
        "show(plot(x^2, [x, -1, 1]), list_plot([[0, 0], [1, 1]]), graphics([red, point([sqrt(1/2), 1/2])]))",
        "graphics([inset(plot(x, [x, 0, 1]), [0, 0]), circle([0, 0], 2)])",
    ] {
        let p = printed(&mut e, src);
        assert!(p.starts_with("image(\""), "{src} -> {p}");
    }
}
