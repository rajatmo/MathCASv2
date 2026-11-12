//! Round 18: colour models, exact colour arithmetic, new primitives (solids, tori, unbounded lines and
//! planes, mesh regions, B-spline surfaces, joined and filled curves, axis objects), transforms and the inert
//! interface wrappers.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

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

/// The PNG of a graphics term rendered through the engine (`export_data` to a temporary file), or the failure.
fn renders(e: &mut CoreEngine, src: &str) -> bool {
    let dir = std::env::temp_dir().join(format!("mc-round18-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("g.png");
    let p = path.to_string_lossy().replace('\\', "/");
    let r = e.eval(&format!("export_data(\"{p}\", {src})"), &StopFlag::new());
    let ok = matches!(&r, Ok(v) if !v.printed.starts_with("failure")) && std::fs::metadata(&path).is_ok_and(|m| m.len() > 100);
    let _ = std::fs::remove_file(&path);
    ok
}

#[test]
fn exact_colour_arithmetic() {
    check(&[
        ("blend([red, blue], 1/3)", "rgb_color(2/3, 0, 1/3)"),
        ("blend([red, green])", "rgb_color(1/2, 1/2, 0)"),
        ("blend([rgb_color(1, 0, 0), rgb_color(0, 1, 0), rgb_color(0, 0, 1)], [1/8, 1/2, 3/8])", "rgb_color(1/8, 1/2, 3/8)"),
        ("blend([blue, white], 1/3.)", "rgb_color(0.3333333333333333, 0.3333333333333333, 1.0)"),
        ("lighter(rgb_color(1, 0, 0))", "rgb_color(1, 1/3, 1/3)"),
        ("darker([red, green, blue], 1/2)", "[rgb_color(1/2, 0, 0), rgb_color(0, 1/2, 0), rgb_color(0, 0, 1/2)]"),
        ("darker(rgb_color(1, 0, 0, 0.4))", "rgb_color(2/3, 0, 0, 0.4)"),
        // past the ends: clamped
        ("darker(red, 5/4)", "rgb_color(0, 0, 0)"),
    ]);
}

#[test]
fn colour_models() {
    check(&[
        ("color_convert(rgb_color(1, 0, 0), \"CMYK\")", "cmyk_color(0.0, 1.0, 1.0, 0.0)"),
        ("color_convert([0.3], \"Grayscale\")", "gray_level(0.3)"),
        ("color_convert([1.5, 1.2, 0], \"RGB\")", "rgb_color(1.0, 1.0, 0.0)"),
        ("color_convert([red, blue], \"CMYK\")", "[cmyk_color(0.0, 1.0, 1.0, 0.0), cmyk_color(1.0, 1.0, 0.0, 0.0)]"),
        ("rgb_color(\"#f00\")", "rgb_color(1.0, 0.0, 0.0)"),
        ("is_color(lab_color(1, 0.5, 0.5))", "true"),
        ("is_color(1.2)", "false"),
        ("is_color(standard_blue)", "true"),
        ("color_negate(rgb_color(1, 0, 1/4))", "rgb_color(0, 1, 3/4)"),
        ("color_negate(gray_level(0.25))", "gray_level(0.75)"),
    ]);
    let mut e = CoreEngine::new();
    // the CIE 1976 difference: 0.5011 between middle gray and lab_color(0.5, 0.5, 0)
    let d: f64 = eval(&mut e, "color_distance(gray, lab_color(0.5, 0.5, 0))").parse().unwrap();
    assert!((d - 0.50115).abs() < 2e-4, "{d}");
    // L*a*b* round trip through the colour model
    let back = eval(&mut e, "color_convert(color_convert(rgb_color(0.2, 0.6, 0.4), \"LAB\"), \"RGB\")");
    assert!(back.starts_with("rgb_color(0.2000") || back.starts_with("rgb_color(0.1999"), "{back}");
}

#[test]
fn new_primitives_render() {
    let mut e = CoreEngine::new();
    for g in [
        "graphics3d(pyramid([[0, 0, 0], [2, 0, 0], [2, 2, 0], [0, 2, 0], [1, 1, 2]]))",
        "graphics3d([opacity(0.5), torus(), filled_torus([3, 0, 0], [1/2, 2])])",
        "graphics3d([tetrahedron(), prism(), hexahedron([[0, 0, 0], [1, 0, 0], [1, 1, 0], [0, 1, 0], [0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]])])",
        "graphics3d(parallelepiped([0, 0, 0], [[1, 0, 0], [1, 1, 0], [0, 0, 1]]))",
        "graphics3d([sphere(), infinite_line([0, 0, 0], [1, 1, 1]), infinite_plane([0, 0, 0], [[1, 0, 0], [0, 1, 0]])])",
        "graphics3d(half_plane([0, 0, 0], [1, 0, 0], [0, 0, 1]), plot_range: 2)",
        "graphics3d(b_spline_surface([[[0, 0, 0], [1, 0, 1], [2, 0, 0]], [[0, 1, 1], [1, 1, 2], [2, 1, 1]], [[0, 2, 0], [1, 2, 1], [2, 2, 0]]]))",
        "graphics3d(mesh_region([[0, 0, 0], [0, 0, 1], [0, 1, 0], [1, 0, 0]], [polygon([[1, 2, 3], [1, 2, 4], [1, 3, 4]])]))",
        "graphics([red, infinite_line([[0, 0], [1, 1]]), disk()])",
        "graphics([half_line([0, 0], [1, 2]), half_plane([[0, 0], [1, 0]], [0, 1])], plot_range: 2)",
        "graphics(mesh_region([[0, 0], [1, 0], [1, 1], [0, 1]], polygon([1, 2, 3, 4])))",
        "graphics(parallelogram([0, 0], [[1, 0], [1, 1]]))",
        "graphics(filled_curve([line([[0, 0], [1, 0]]), bezier_curve([[1, 0], [2, 1], [1, 2], [0, 1]])]))",
        "graphics(joined_curve([line([[0, 0], [1, 0]]), b_spline_curve([[1, 0], [2, 1], [1, 2]])]))",
        "graphics(axis_object(line([[0, 0], [10, 10]]), [0, 10], axis_label: \"hello\"))",
        "graphics(axis_object([\"horizontal\", 0]), plot_range: [[-2, 2], [-1, 1]])",
        "graphics([rotate(rectangle(), pi/6), translate(disk(), [[2, 0], [4, 0]]), scale(circle(), [2, 1])])",
        "graphics(geometric_transformation(polygon([[0, 0], [1, 0], [0, 1]]), [[1, 1/2], [0, 1]]))",
        "graphics([cmyk_color(0, 1, 1, 0.25), disk(), lch_color(0.6, 0.5, 0.3), disk([2, 0]), light_dark_switched(red, blue), disk([4, 0])])",
        "graphics([linear_gradient_filling([red, blue]), drop_shadowing(), disk(), haloing(), text(\"x\", offset([5, 5], [0, 0]))])",
        "graphics3d([gooch_shading(), sphere(), texture(1), cuboid()])",
    ] {
        assert!(renders(&mut e, g), "{g}");
    }
}

#[test]
fn interface_wrappers_are_inert() {
    check(&[
        ("tab_view([a, b, c, d])", "tab_view([a, b, c, d])"),
        ("slide_view([a, b], 2)", "slide_view([a, b], 2)"),
        ("overlay([2, 4])", "overlay([2, 4])"),
        ("popup_menu(a, [a, b])", "popup_menu(a, [a, b])"),
        ("short(x^2, 3)", "short(x^2, 3)"),
        ("highlighted(x + 1, \"Dropline\")", "highlighted(1 + x, \"Dropline\")"),
        ("list_animate([graphics(disk()), 2])", "graphics(disk())"),
    ]);
}
