//! Round 38: a 3D picture of the default size gets a column twin as a 2D plot does, its projection laid out on a page
//! as wide as a 2D plot's twin (the notebook's PDF export draws it at the column's width); a 3D picture of its own size
//! and a 2D graphic shown as it is get none.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::path::Path;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// The PNG file an image value names.
fn png_of(value: &str) -> String {
    value.trim_start_matches("image(\"").trim_end_matches("\")").to_string()
}

/// The page size of an SVG of `mc-graphics` (its root element's width and height).
fn svg_size(path: &Path) -> (f64, f64) {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let attr = |name: &str| -> f64 {
        let at = text.find(&format!(" {name}=\"")).unwrap() + name.len() + 3;
        text[at..].split('"').next().unwrap().parse().unwrap()
    };
    (attr("width"), attr("height"))
}

/// The number of filled or stroked paths in an SVG (the same picture drawn on another page has as many).
fn paths(path: &Path) -> usize {
    std::fs::read_to_string(path).unwrap().matches("<path").count()
}

#[test]
fn default_3d_pictures_get_a_column_twin_and_sized_ones_none() {
    let mut e = CoreEngine::new();
    // the documented case and two variations: a parametric surface, a list of points in space
    for src in [
        "plot3d(sin(x*y), [x, 0, 3], [y, 0, 3])",
        "parametric_plot3d([cos(u)*sin(v), sin(u)*sin(v), cos(v)], [u, 0, 2*pi], [v, 0, pi])",
        "list_point_plot3d(table([k, k^2, sin(k)], [k, 1, 20]))",
    ] {
        let value = eval(&mut e, src);
        let png = png_of(&value);
        let png = Path::new(&png);
        assert!(png.is_file(), "{src}: {value}");
        assert!(png.with_extension("scene3d").is_file(), "{src}");
        let (sw, sh) = svg_size(&png.with_extension("svg"));
        let (cw, ch) = svg_size(&png.with_extension("column.svg"));
        // the screen picture keeps its width; the twin is a 2D twin's width, shaped like the picture (the margins
        // around the projection stay the same in points, so the shape is close to, not exactly, the picture's)
        assert_eq!(sw, 360.0, "{src}");
        assert_eq!(cw, 569.4, "{src}");
        let (a, b) = (sh / sw, ch / cw);
        assert!((a - b).abs() < 0.08 * a, "{src}: {sw} x {sh} against {cw} x {ch}");
        // the same picture: as many paths (faces, lines, the glyphs of the labels)
        assert_eq!(paths(&png.with_extension("svg")), paths(&png.with_extension("column.svg")), "{src}");
    }
    for src in [
        "plot3d(sin(x*y), [x, 0, 3], [y, 0, 3], image_size: 300)",
        "show(graphics([disk([0, 0], 1)]))",
    ] {
        let value = eval(&mut e, src);
        let png = png_of(&value);
        let png = Path::new(&png);
        assert!(png.is_file(), "{src}: {value}");
        assert!(!png.with_extension("column.svg").exists(), "{src}");
    }
}
