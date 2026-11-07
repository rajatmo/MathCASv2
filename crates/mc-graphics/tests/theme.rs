//! Chrome marking (round nb-svg): the SVG marks background, frame, ticks, labels, grid lines, the legend box and
//! uncoloured primitives by class, data not at all; a style sheet made from a theme recolours exactly those when
//! an independent renderer (resvg) reads the SVG, and the raster surface with the same theme draws the same
//! picture.
use mc_core::{Config, Heap};
use mc_graphics::color::Rgba;
use mc_graphics::theme::Theme;
use mc_graphics::{render, render_rgba_themed, Format};

const SRC: &str = "graphics([[red, absolute_thickness(3), line([[0, 0], [10, 10]]), line([[0, 10], [10, 0]])], \
    text(\"hi\", [5, 2])], frame: true, frame_label: [\"x\", \"y\"], plot_label: \"title\", grid_lines: automatic, \
    legend: [[red, \"a\"]], image_size: 300)";

fn dark() -> Theme {
    Theme {
        background: None,
        text: Rgba::rgb(0.82, 0.84, 0.87),
        frame: Rgba::rgb(0.63, 0.66, 0.71),
        grid: Rgba::rgb(0.22, 0.24, 0.27),
        panel: Rgba::rgb(0.13, 0.15, 0.18),
    }
}

fn svg_of(src: &str) -> String {
    let h = Heap::new(Config::default());
    let t = mc_syntax::read(&h, src).unwrap()[0];
    String::from_utf8(render(&h, t, Format::Svg).unwrap()).unwrap()
}

/// The SVG rasterised by resvg at scale 1 with `css` injected: straight RGBA.
fn resvg_rgba(svg: &str, css: Option<String>) -> (u32, u32, Vec<u8>) {
    let opt = resvg::usvg::Options { style_sheet: css, ..resvg::usvg::Options::default() };
    let tree = resvg::usvg::Tree::from_str(svg, &opt).unwrap();
    let (w, h) = (tree.size().width().ceil() as u32, tree.size().height().ceil() as u32);
    let mut pm = resvg::tiny_skia::Pixmap::new(w, h).unwrap();
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pm.as_mut());
    let mut out = vec![];
    for p in pm.pixels() {
        let c = p.demultiply();
        out.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    (w, h, out)
}

#[test]
fn chrome_is_marked_and_data_is_not() {
    let svg = svg_of(SRC);
    assert!(svg.contains(r#"class="mc-graphic""#));
    for class in ["mcf-bg", "mcs-frame", "mcf-text", "mcs-grid", "mcf-panel", "mcf-ink"] {
        assert!(svg.contains(&format!(r#"class="{class}""#)), "{class} missing:\n{svg}");
    }
    // every red element is data: no class
    for line in svg.lines().filter(|l| l.contains("#ff0000")) {
        assert!(!line.contains("class="), "data marked as chrome: {line}");
    }
    // an uncoloured line is ink; a coloured one data
    let ink = svg_of("graphics([line([[0, 0], [1, 1]]), blue, line([[0, 1], [1, 0]])])");
    assert_eq!(ink.matches(r#"class="mcs-ink""#).count(), 1, "{ink}");
    assert_eq!(ink.lines().filter(|l| l.contains("#0000ff") && !l.contains("class=")).count(), 1, "{ink}");
    // a background given as an option is the user's colour: not themed
    let given = svg_of("graphics(circle(), background: white)");
    assert!(!given.contains("mcf-bg"), "{given}");
}

#[test]
fn a_theme_recolours_chrome_only_in_svg_and_raster_alike() {
    let svg = svg_of(SRC);
    let theme = dark();
    let (w, h, light) = resvg_rgba(&svg, None);
    let (_, _, themed) = resvg_rgba(&svg, Some(theme.css()));
    let at = |px: &[u8], x: u32, y: u32| -> [u8; 4] {
        let i = ((y * w + x) * 4) as usize;
        [px[i], px[i + 1], px[i + 2], px[i + 3]]
    };
    // the corner: white page in the file, transparent in the dark theme
    assert_eq!(at(&light, 1, 1), [255, 255, 255, 255]);
    assert_eq!(at(&themed, 1, 1)[3], 0);
    // pure red pixels (the curves) are the same in both
    let red = |px: &[u8]| (0..w * h).filter(|k| px[(*k * 4) as usize..(*k * 4 + 4) as usize] == [255, 0, 0, 255]).count();
    let (rl, rt) = (red(&light), red(&themed));
    assert!(rl > 200 && rl.abs_diff(rt) * 50 < rl, "red pixels {rl} light, {rt} themed");
    // the theme's frame colour appears, the light frame colour does not
    let has = |px: &[u8], c: [u8; 4]| px.chunks(4).any(|p| p == c);
    let [fr, fg, fb, _] = theme.frame.bytes();
    assert!(has(&themed, [fr, fg, fb, 255]));
    assert!(!has(&themed, [60, 60, 60, 255]));
    // the raster surface with the theme draws what resvg draws from the SVG with the style sheet
    let hp = Heap::new(Config::default());
    let t = mc_syntax::read(&hp, SRC).unwrap()[0];
    let (rw, rh, ours) = render_rgba_themed(&hp, t, 1.0, Some(&theme), &|| false).unwrap();
    assert_eq!((rw, rh), (w, h));
    // compared premultiplied over the dark page, as a front end shows them
    let over = |p: &[u8]| -> [f64; 3] {
        let a = p[3] as f64 / 255.0;
        let bg = [34.0, 38.0, 45.0];
        [0, 1, 2].map(|k| p[k] as f64 * a + bg[k] * (1.0 - a))
    };
    let diff: f64 = ours.chunks(4).zip(themed.chunks(4)).map(|(a, b)| {
        let (a, b) = (over(a), over(b));
        (0..3).map(|k| (a[k] - b[k]).abs()).sum::<f64>()
    }).sum::<f64>()
        / (w * h * 3) as f64;
    assert!(diff < 3.0, "mean difference {diff}");
}

#[test]
fn the_light_theme_is_what_the_files_show() {
    // a style sheet of the light theme changes nothing
    let svg = svg_of(SRC);
    let (_, _, plain) = resvg_rgba(&svg, None);
    let (_, _, light) = resvg_rgba(&svg, Some(Theme::LIGHT.css()));
    let diff = plain.iter().zip(&light).filter(|(a, b)| a.abs_diff(**b) > 1).count();
    assert!(diff * 1000 < plain.len(), "{diff} channels differ");
}
