//! Round 37: a plot of the default size gets a column twin, the figure laid out for the text column of a
//! notebook's PDF export (`mc-nbexport` draws it at the column's width); a plot of its own size gets none.
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

#[test]
fn default_plots_get_a_column_twin_and_sized_ones_none() {
    let mut e = CoreEngine::new();
    // the documented case and two variations: another function family, a log plot, a list plot
    for src in ["plot(sin(x), [x, 0, 6])", "log_plot(exp(x), [x, 0, 5])", "list_plot(table([k, k^2], [k, 1, 20]))"] {
        let png = png_of(&eval(&mut e, src));
        let png = Path::new(&png);
        assert!(png.is_file(), "{src}: {}", png.display());
        let (w, h) = svg_size(&png.with_extension("column.svg"));
        assert_eq!((w, h), (569.4, 355.9), "{src}");
        // the screen drawing and the print twin keep their sizes
        assert_eq!(svg_size(&png.with_extension("svg")), (640.0, 400.0), "{src}");
        assert_eq!(svg_size(&png.with_extension("print.svg")), (360.0, 225.0), "{src}");
    }
    for src in ["plot(sin(x), [x, 0, 6], image_size: [500, 300])", "plot(sin(x), [x, 0, 6], aspect_ratio: 1)"] {
        let png = png_of(&eval(&mut e, src));
        let png = Path::new(&png);
        assert!(png.is_file(), "{src}");
        for twin in ["column.svg", "print.svg", "print.pdf"] {
            assert!(!png.with_extension(twin).exists(), "{src}: {twin}");
        }
    }
}
