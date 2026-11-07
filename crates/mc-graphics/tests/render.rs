//! Rendering graphics terms read from source text (not evaluated): every primitive in every renderer (pixels of
//! the PNG probed, the SVG rasterised by an independent renderer and compared with the PNG, the PDF's structure
//! checked), labels, options, snapshots, and a large graphic within a time budget.
use mc_core::{Config, Heap, Store};
use mc_graphics::{is_graphics, page_size, render, render_formats, Format};
use std::time::Instant;

fn heap() -> Heap {
    Heap::new(Config::default())
}

/// The three renderings of a graphic.
struct Out {
    png: Vec<u8>,
    svg: String,
    pdf: Vec<u8>,
}

/// Render the graphic written `src` to PNG (scale 1), SVG and PDF.
fn all(src: &str) -> Out {
    let h = heap();
    let t = mc_syntax::read(&h, src).expect(src)[0];
    let v = render_formats(&h, t, &[Format::Png { scale: 1.0 }, Format::Svg, Format::Pdf], &|| false).expect(src);
    let [png, svg, pdf] = <[Vec<u8>; 3]>::try_from(v).unwrap();
    Out { png, svg: String::from_utf8(svg).unwrap(), pdf }
}

/// Write the files under the tests' temporary folder (to look at them).
fn keep(name: &str, o: &Out) {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("graphics");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(format!("{name}.png")), &o.png);
    let _ = std::fs::write(dir.join(format!("{name}.svg")), &o.svg);
    let _ = std::fs::write(dir.join(format!("{name}.pdf")), &o.pdf);
}

fn decode(png: &[u8]) -> tiny_skia::Pixmap {
    tiny_skia::Pixmap::decode_png(png).expect("a valid PNG")
}

/// The colour at a pixel (demultiplied).
fn pixel(pm: &tiny_skia::Pixmap, x: u32, y: u32) -> [u8; 4] {
    let c = pm.pixel(x, y).expect("inside").demultiply();
    [c.red(), c.green(), c.blue(), c.alpha()]
}

/// Whether two colours are within `tol` in every channel.
fn near(a: [u8; 4], b: [u8; 4], tol: u8) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= tol)
}

/// The SVG rasterised by resvg: its mean absolute difference from the PNG, per channel and pixel.
fn svg_difference(o: &Out) -> f64 {
    let ours = decode(&o.png);
    let tree = resvg::usvg::Tree::from_str(&o.svg, &resvg::usvg::Options::default()).expect("the SVG parses");
    let mut theirs = resvg::tiny_skia::Pixmap::new(ours.width(), ours.height()).unwrap();
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut theirs.as_mut());
    let (a, b) = (ours.data(), theirs.data());
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).map(|(x, y)| x.abs_diff(*y) as f64).sum::<f64>() / a.len() as f64
}

/// The PDF checked: header and end, the cross-reference table pointing at every object, every stream
/// Flate-compressed with its exact length and inflating, the page size. Gives the inflated streams.
fn pdf_streams(pdf: &[u8], w: &str, h: &str) -> Vec<String> {
    let text = String::from_utf8_lossy(pdf);
    assert!(pdf.starts_with(b"%PDF-1.4\n") && pdf.ends_with(b"%%EOF\n"));
    let find = |hay: &[u8], needle: &[u8], from: usize| -> Option<usize> {
        hay[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
    };
    // the xref table and its offsets
    let start: usize = text.rsplit("startxref\n").next().unwrap().lines().next().unwrap().parse().unwrap();
    assert!(pdf[start..].starts_with(b"xref\n0 "), "startxref points at the table");
    let table = String::from_utf8_lossy(&pdf[start..]).to_string();
    let count: usize = table.lines().nth(1).unwrap().split(' ').nth(1).unwrap().parse().unwrap();
    for (k, line) in table.lines().skip(3).take(count - 1).enumerate() {
        assert_eq!(line.len(), 19, "an xref entry is 20 bytes with its line end");
        let off: usize = line[..10].parse().unwrap();
        assert!(pdf[off..].starts_with(format!("{} 0 obj\n", k + 1).as_bytes()), "object {}", k + 1);
    }
    assert!(table.contains(&format!("trailer\n<< /Size {count} /Root 1 0 R")));
    // the page
    assert!(text.contains(&format!("/MediaBox [0 0 {w} {h}]")), "page size {w} x {h}");
    // every stream
    let mut streams = vec![];
    let mut at = 0;
    while let Some(p) = find(pdf, b"<< /Length ", at) {
        let rest = String::from_utf8_lossy(&pdf[p + 11..(p + 60).min(pdf.len())]).to_string();
        let len: usize = rest.split(' ').next().unwrap().parse().unwrap();
        assert!(rest.starts_with(&format!("{len} /Filter /FlateDecode")), "compressed");
        let body = find(pdf, b">>\nstream\n", p).unwrap() + 10;
        assert!(pdf[body + len..].starts_with(b"\nendstream"), "the length is exact");
        let data = miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[body..body + len]).expect("it inflates");
        streams.push(String::from_utf8_lossy(&data).into_owned());
        at = body + len;
    }
    assert!(!streams.is_empty());
    streams
}

/// A frame for probes: plot range [0, 10] on both axes, 100 x 100 points, no padding, so plot coordinate v is
/// pixel 4 + 9.2 v (x) and 4 + 9.2 (10 - v) (y).
const BOX: &str = "plot_range: [[0, 10], [0, 10]], image_size: [100, 100], plot_range_padding: none";

fn px(v: f64) -> u32 {
    (4.0 + 9.2 * v).round() as u32
}

fn py(v: f64) -> u32 {
    (4.0 + 9.2 * (10.0 - v)).round() as u32
}

const WHITE: [u8; 4] = [255, 255, 255, 255];
const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];

/// Render `content` in the probe frame: the PNG's pixels, after checking the SVG against the PNG and the PDF's
/// structure; `name` keeps the files.
fn probe(name: &str, content: &str) -> (tiny_skia::Pixmap, Out) {
    let o = all(&format!("graphics({content}, {BOX})"));
    keep(name, &o);
    let pm = decode(&o.png);
    assert_eq!((pm.width(), pm.height()), (100, 100), "{name}");
    let d = svg_difference(&o);
    assert!(d < 3.0, "{name}: the SVG differs from the PNG by {d} per channel");
    pdf_streams(&o.pdf, "100", "100");
    (pm, o)
}

#[test]
fn points() {
    let (pm, o) = probe("point", "[red, absolute_point_size(10), point([[5, 5], [2, 8]])]");
    assert_eq!(pixel(&pm, px(5.0), py(5.0)), RED);
    assert_eq!(pixel(&pm, px(2.0), py(8.0)), RED);
    assert_eq!(pixel(&pm, px(5.0) + 8, py(5.0)), WHITE);
    assert!(o.svg.contains(r##"stroke="#ff0000" stroke-width="10" stroke-linecap="round""##));
    assert!(pdf_streams(&o.pdf, "100", "100")[0].contains("10 w 1 J [] 0 d\n50 50 m 50 50 l\n"));
}

#[test]
fn lines() {
    let (pm, o) = probe("line", "[blue, absolute_thickness(4), line([[[0, 5], [10, 5]], [[5, 0], [5, 2]]])]");
    assert_eq!(pixel(&pm, px(2.0), py(5.0)), BLUE);
    assert_eq!(pixel(&pm, px(5.0), py(1.0)), BLUE);
    assert_eq!(pixel(&pm, px(2.0), py(5.0) - 4), WHITE);
    assert_eq!(pixel(&pm, px(5.0), py(3.0)), WHITE);
    // one data path for both runs; the background is chrome (round nb-svg)
    assert_eq!(o.svg.matches("<path d=\"M").count(), 1, "{}", o.svg);
    assert_eq!(o.svg.matches("<path class=\"mcf-bg\" d=\"M").count(), 1, "{}", o.svg);
    // dashes in each renderer
    let (pm, o) = probe("dashed", "[black, absolute_thickness(2), absolute_dashing([10, 10]), line([[0, 5], [10, 5]])]");
    assert!(o.svg.contains(r#"stroke-dasharray="10 10""#));
    assert!(pdf_streams(&o.pdf, "100", "100")[0].contains("[10 10] 0 d"));
    let on = (4..96).filter(|&x| pixel(&pm, x, py(5.0))[0] < 128).count();
    assert!((35..=60).contains(&on), "half the line is drawn: {on}");
}

#[test]
fn polygons_and_rectangles() {
    let (pm, _) = probe("polygon", "[rgb_color(0, 0.5, 0), polygon([[1, 1], [9, 1], [5, 9]])]");
    assert!(near(pixel(&pm, px(5.0), py(4.0)), [0, 128, 0, 255], 1));
    assert_eq!(pixel(&pm, px(1.5), py(8.0)), WHITE);
    let (pm, _) = probe("rectangle", "[black, rectangle([2, 2], [8, 8])]");
    assert_eq!(pixel(&pm, px(5.0), py(5.0)), BLACK);
    assert_eq!(pixel(&pm, px(1.0), py(5.0)), WHITE);
    // faces and edges
    let (pm, o) = probe("edges", "[face_form(none), edge_form([red, absolute_thickness(3)]), rectangle([2, 2], [8, 8])]");
    assert_eq!(pixel(&pm, px(5.0), py(5.0)), WHITE);
    assert_eq!(pixel(&pm, px(2.0), py(5.0)), RED);
    assert!(o.svg.contains(r##"fill="none" stroke="#ff0000" stroke-width="3""##));
    let (pm, _) = probe("face", "[face_form(blue), edge_form(black), polygon([[1, 1], [9, 1], [9, 9], [1, 9]])]");
    assert_eq!(pixel(&pm, px(5.0), py(5.0)), BLUE);
    assert!(pixel(&pm, px(1.0), py(5.0))[2] < 100);
    // a rectangle in scaled coordinates covers the area
    let (pm, _) = probe("scaled", "[gray, rectangle(scaled([0, 0]), scaled([1, 1]))]");
    assert!(near(pixel(&pm, 6, 6), [128, 128, 128, 255], 1));
}

#[test]
fn circles_and_disks() {
    let (pm, _) = probe("circle", "[red, absolute_thickness(3), circle([5, 5], 4)]");
    assert_eq!(pixel(&pm, px(9.0), py(5.0)), RED);
    assert_eq!(pixel(&pm, px(5.0), py(1.0)), RED);
    assert_eq!(pixel(&pm, px(5.0), py(5.0)), WHITE);
    let (pm, _) = probe("arc", "[red, absolute_thickness(3), circle([5, 5], 4, [0, 1.5707963267948966])]");
    assert_eq!(pixel(&pm, px(5.0), py(9.0)), RED);
    assert_eq!(pixel(&pm, px(1.0), py(5.0)), WHITE);
    let (pm, _) = probe("sector", "[blue, disk([5, 5], 4, [0, 1.5707963267948966])]");
    assert_eq!(pixel(&pm, px(6.5), py(6.5)), BLUE);
    assert_eq!(pixel(&pm, px(3.5), py(3.5)), WHITE);
    let (pm, _) = probe("ellipse", "[blue, disk([5, 5], [4, 2])]");
    assert_eq!(pixel(&pm, px(8.5), py(5.0)), BLUE);
    assert_eq!(pixel(&pm, px(5.0), py(7.5)), WHITE);
    // opacity blends with the background
    let (pm, o) = probe("opacity", "[opacity(0.5), red, disk([5, 5], 4)]");
    assert!(near(pixel(&pm, px(5.0), py(5.0)), [255, 128, 128, 255], 2), "{:?}", pixel(&pm, px(5.0), py(5.0)));
    assert!(o.svg.contains(r#"fill-opacity="0.5""#));
    let pdf = String::from_utf8_lossy(&o.pdf).to_string();
    assert!(pdf.contains("/ExtGState << /A0 << /ca 0.5 /CA 0.5 >> >>"));
    assert!(pdf_streams(&o.pdf, "100", "100")[0].contains("/A0 gs"));
}

#[test]
fn arrows_and_curves() {
    let (pm, _) = probe("arrow", "[black, absolute_thickness(1), arrow([[1, 5], [9, 5]])]");
    // the head is wider than the shaft
    assert!(pixel(&pm, px(8.7), py(5.0))[0] < 64);
    assert!(pixel(&pm, px(8.5), py(5.0) - 2)[0] < 100);
    assert_eq!(pixel(&pm, px(4.0), py(5.0) - 2), WHITE);
    let (pm, o) = probe("bezier", "[purple, absolute_thickness(3), bezier_curve([[0, 0], [0, 10], [10, 10], [10, 0]])]");
    // the cubic's midpoint is at (5, 7.5)
    assert!(near(pixel(&pm, px(5.0), py(7.5)), [128, 0, 128, 255], 2), "{:?}", pixel(&pm, px(5.0), py(7.5)));
    assert_eq!(pixel(&pm, px(5.0), py(5.0)), WHITE);
    assert!(o.svg.contains("C4 4 96 4 96 96"), "{}", o.svg);
    // a quadratic piece and a straight one after the cubic
    let _ = probe("bezier_tail", "[bezier_curve([[0, 0], [1, 2], [2, 0], [3, 2], [4, 4], [5, 0], [6, 1]])]");
}

#[test]
fn text_is_typeset_in_every_renderer() {
    let (pm, o) = probe("text", "[black, font_size(30), text(\"H$\\\\alpha_1^2$\", [5, 5])]");
    let dark = (0..100).flat_map(|x| (0..100).map(move |y| (x, y))).filter(|&(x, y)| pixel(&pm, x, y)[0] < 100).count();
    assert!(dark > 100, "the glyphs are drawn: {dark}");
    assert!(o.svg.contains(r#"aria-label="H$\alpha_1^2$""#), "{}", o.svg);
    assert_eq!(o.svg.matches("<use ").count(), 4);
    let pdf = String::from_utf8_lossy(&o.pdf).to_string();
    assert!(pdf.contains("/Subtype /Type3") && pdf.contains("/CharProcs") && pdf.contains("/ToUnicode"));
    let streams = pdf_streams(&o.pdf, "100", "100");
    // the ToUnicode map gives back the characters: H, alpha, 1, 2
    let cmap = streams.iter().find(|s| s.contains("beginbfchar")).expect("a ToUnicode map");
    for code in ["<0048>", "<03B1>", "<0031>", "<0032>"] {
        assert!(cmap.contains(code), "{code} in {cmap}");
    }
    // glyph procedures are paths in thousandths of an em
    assert!(streams.iter().any(|s| s.contains(" d1\n") && s.ends_with("f\n")));
    // the offset puts that corner of the box at the position
    let (pm, _) = probe("text_offset", "[black, font_size(30), text(\"H\", [5, 5], [-1, -1])]");
    assert_eq!(pixel(&pm, px(4.5), py(5.5)), WHITE);
    assert!((px(5.0)..px(8.0)).any(|x| pixel(&pm, x, py(6.0))[0] < 100));
}

#[test]
fn backgrounds_frames_axes_and_legends() {
    let o = all("graphics(disk(), background: none, image_size: 50)");
    let pm = decode(&o.png);
    assert_eq!(pixel(&pm, 0, 0)[3], 0, "transparent");
    assert!(!o.svg.contains("fill=\"#ffffff\""));
    let o = all("graphics(disk(), background: yellow, image_size: 50)");
    assert_eq!(pixel(&decode(&o.png), 0, 0), [255, 255, 0, 255]);
    let o = all(
        "graphics([red, line([[0, 0], [1, 1]])], frame: true, frame_label: [\"x\", \"y\"], plot_label: \"t\", \
         grid_lines: automatic, legend: [[red, \"a\"], [[blue, absolute_point_size(5)], \"b\", point], [green, \"c\", swatch], \"d\"], \
         legend_position: bottom_right)",
    );
    keep("decorations", &o);
    for l in ["\"x\"", "\"y\"", "\"t\"", "\"a\"", "\"b\"", "\"c\"", "\"d\"", "\"0.2\"", "\"1.0\""] {
        assert!(o.svg.contains(&format!("aria-label={l}")), "{l}");
    }
    assert!(svg_difference(&o) < 3.0);
    let o = all("graphics([circle()], axes: true, axes_label: [\"x\", \"y\"], ticks: [[-1, 1], none])");
    keep("axes", &o);
    assert!(o.svg.contains("aria-label=\"\u{2212}1\"") && o.svg.contains("aria-label=\"1\""));
    assert!(!o.svg.contains("aria-label=\"0.5\""), "no y ticks");
    assert!(svg_difference(&o) < 3.0);
    let o = all("graphics(line([[0, 0], [3.0e7, 2.0e-6]]), frame: true)");
    assert!(o.svg.contains("\u{d7}10^7"), "scientific tick labels");
}

#[test]
fn sizes_and_aspect_ratios() {
    let h = heap();
    let size = |src: &str| page_size(&h, mc_syntax::read(&h, src).unwrap()[0]).unwrap();
    // equal units: a 2 x 1 range is half as high as wide (the 4 point margins aside)
    assert_eq!(size("graphics(rectangle([0, 0], [2, 1]), plot_range_padding: none)"), (360.0, 184.0));
    assert_eq!(size("graphics(disk(), image_size: 200)"), (200.0, 200.0));
    assert_eq!(size("graphics(disk(), image_size: [300, 100])"), (300.0, 100.0));
    assert_eq!(size("graphics(disk(), aspect_ratio: 0.5, image_size: 208, plot_range_padding: none)"), (208.0, 108.0));
    assert_eq!(size("graphics(point([1, 1]))"), (360.0, 360.0));
    assert_eq!(size("graphics([])"), (360.0, 360.0));
    // high-DPI rendering scales the pixels
    let t = mc_syntax::read(&h, "graphics(disk(), image_size: 50)").unwrap()[0];
    let pm = decode(&render(&h, t, Format::Png { scale: 2.0 }).unwrap());
    assert_eq!((pm.width(), pm.height()), (100, 100));
    assert_eq!(render(&h, t, Format::Png { scale: 0.0 }).unwrap_err().tag, "argument");
    let big = mc_syntax::read(&h, "graphics(disk(), image_size: 20000)").unwrap()[0];
    assert_eq!(render(&h, big, Format::Png { scale: 16.0 }).unwrap_err().tag, "too_large");
}

#[test]
fn what_cannot_be_rendered_fails() {
    let h = heap();
    for (src, tag) in [
        ("graphics(point([a, 1]))", "not_numeric"),
        ("graphics(foo(1))", "not_graphics"),
        ("graphics([red, 7])", "not_graphics"),
        ("point([1, 2])", "not_graphics"),
        ("graphics(1, 2, 3)", "not_graphics"),
    ] {
        let t = mc_syntax::read(&h, src).unwrap()[0];
        for f in [Format::Png { scale: 1.0 }, Format::Svg, Format::Pdf] {
            assert_eq!(render(&h, t, f).unwrap_err().tag, tag, "{src}");
        }
    }
    let t = mc_syntax::read(&h, "graphics(disk())").unwrap()[0];
    assert!(is_graphics(&h, t));
    assert!(!is_graphics(&h, mc_syntax::read(&h, "disk()").unwrap()[0]));
    // a raised stop flag interrupts
    let many = h.call(h.sym("point"), &[h.real_array(&[10_000, 2], &vec![0.5; 20_000])]);
    let g = h.call(h.sym("graphics"), &[h.call(h.sym("list"), &[many; 10])]);
    assert_eq!(render_formats(&h, g, &[Format::Svg], &|| true).unwrap_err().tag, "interrupted");
}

/// FNV-1a over the pixels.
fn hash(png: &[u8]) -> u64 {
    decode(png).data().iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}

#[test]
fn png_snapshots() {
    // pixel hashes of small renderings; a change in drawing changes them (update deliberately)
    let cases = [
        ("graphics([red, disk([0, 0], 1)], image_size: 40)", 11038683754863902879u64),
        ("graphics([blue, absolute_thickness(2), line([[0, 0], [1, 1], [2, 0]])], image_size: 40)", 4573380468599864753),
        ("graphics(text(\"$x^2$\", [0, 0]), image_size: 40)", 8994584432840233460),
        ("graphics(circle(), frame: true, image_size: 120)", 3830055126281441207),
    ];
    let mut got = vec![];
    for (src, _) in &cases {
        got.push(hash(&all(src).png));
    }
    let want: Vec<u64> = cases.iter().map(|c| c.1).collect();
    assert_eq!(got, want, "snapshot hashes");
}

#[test]
fn svg_snapshot() {
    let o = all("graphics([red, disk([0, 0], 1), black, line([[0, 0], [1, 1]])], image_size: 40, plot_range_padding: none)");
    assert_eq!(
        o.svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" version=\"1.1\" class=\"mc-graphic\" width=\"40\" height=\"40\" viewBox=\"0 0 40 40\">\n\
         <path class=\"mcf-bg\" d=\"M0 0L40 0L40 40L0 40Z\" fill=\"#ffffff\"/>\n\
         <path d=\"M36 20C36 28.84 28.84 36 20 36C11.16 36 4 28.84 4 20C4 11.16 11.16 4 20 4C28.84 4 36 11.16 36 20Z\" fill=\"#ff0000\"/>\n\
         <path d=\"M20 20L36 4\" fill=\"none\" stroke=\"#000000\" stroke-width=\"1\" stroke-linejoin=\"round\"/>\n\
         </svg>\n"
    );
}

#[test]
fn a_sample_of_everything() {
    let src = r#"graphics([
        light_gray, rectangle([-1.5, -1.2], [1.5, 1.2]),
        [opacity(0.5), hue(0.6), disk([0.0, 0.0], 1.0)],
        red, thickness(0.01), circle([0.0, 0.0], 1.0),
        blue, dashing(dashed), line([[-1.0, -1.0], [1.0, 1.0]]),
        black, absolute_point_size(6.0), point([[0.5, 0.0], [-0.5, 0.0]]),
        rgb_color(0.0, 0.5, 0.0), arrow([[-1.0, 1.0], [0.0, 0.5]]),
        face_form(yellow), edge_form([black, absolute_thickness(2.0)]), polygon([[1.0, -1.0], [1.4, -0.4], [0.6, -0.4]]),
        purple, bezier_curve([[-1.4, -1.0], [-1.0, 0.0], [-0.6, -1.0], [-0.2, 0.0]]),
        black, text("$\\alpha_1^2 + \\frac{x}{2}$", [0.0, -0.8]),
        text("left", scaled([0.02, 0.95]), [-1.0, 1.0])
    ], frame: true, frame_label: ["$x$ (m)", "$\\rho_\\phi$"], plot_label: "Sample", image_size: 400)"#;
    let o = all(src);
    keep("sample", &o);
    assert!(svg_difference(&o) < 3.0);
    let h = heap();
    let w = page_size(&h, mc_syntax::read(&h, src).unwrap()[0]).unwrap();
    pdf_streams(&o.pdf, "400", &mc_graphics_num(w.1));
}

/// A page dimension as the PDF writes it.
fn mc_graphics_num(v: f32) -> String {
    let r = (v * 100.0).round() / 100.0;
    format!("{r:.2}").trim_end_matches('0').trim_end_matches('.').to_string()
}

#[test]
fn a_hundred_thousand_points_render_in_time() {
    let h = heap();
    let n = 100_000;
    let data: Vec<f64> = (0..n).flat_map(|k| {
        let x = k as f64 / n as f64;
        [x, (40.0 * x).sin()]
    }).collect();
    let arr = h.real_array(&[n, 2], &data);
    let content = h.call(
        h.sym("list"),
        &[h.call(h.sym("point"), &[arr]), h.call(h.sym("line"), &[arr])],
    );
    let g = h.call(h.sym("graphics"), &[content]);
    let start = Instant::now();
    let out = render_formats(&h, g, &[Format::Png { scale: 1.0 }, Format::Svg, Format::Pdf], &|| false).unwrap();
    let secs = start.elapsed().as_secs_f64();
    println!("10^5 points and a 10^5-point line, PNG + SVG + PDF: {secs:.3} s; sizes {} {} {} bytes", out[0].len(), out[1].len(), out[2].len());
    assert!(secs < 20.0, "{secs} s");
}

#[test]
fn arrowheads_at_either_end_or_none() {
    // heads at both ends, 0.1 of the plot width (9.2 points) long
    let (pm, _) = probe("arrowheads_both", "[black, absolute_thickness(1), arrowheads([0.1, 0.1]), arrow([[1, 5], [9, 5]])]");
    assert!(pixel(&pm, px(8.2), py(5.0) - 2)[0] < 100, "end head");
    assert!(pixel(&pm, px(1.8), py(5.0) - 2)[0] < 100, "start head");
    assert_eq!(pixel(&pm, px(5.0), py(5.0) - 2), WHITE);
    // no head: the line runs to its end, nothing wider
    let (pm, _) = probe("arrowheads_none", "[black, absolute_thickness(1), arrowheads(0), arrow([[1, 5], [9, 5]])]");
    assert_eq!(pixel(&pm, px(8.5), py(5.0) - 2), WHITE);
    assert!(pixel(&pm, px(8.9), py(5.0))[0] < 200);
}

#[test]
fn rasters_fill_their_rectangle_with_cells() {
    // two rows of two cells over [2, 8] x [2, 8]; the first row is the bottom one
    let (pm, o) = probe("raster", "[raster([[0, 1], [red, [0, 0, 1]]], [[2, 2], [8, 8]])]");
    assert_eq!(pixel(&pm, px(3.5), py(3.5)), BLACK);
    assert_eq!(pixel(&pm, px(6.5), py(3.5)), WHITE);
    assert_eq!(pixel(&pm, px(3.5), py(6.5)), RED);
    assert_eq!(pixel(&pm, px(6.5), py(6.5)), BLUE);
    // outside the rectangle: the background
    assert_eq!(pixel(&pm, px(1.0), py(1.0)), WHITE);
    assert!(o.svg.contains("<image ") && o.svg.contains("data:image/png;base64,"));
    let pdf = String::from_utf8_lossy(&o.pdf);
    assert!(pdf.contains("/Subtype /Image /Width 2 /Height 2") && pdf.contains("/XObject << /Im0"), "{pdf}");
    // numbers through a gradient, transparent cells, and a soft mask in the PDF
    let (pm, o) = probe("raster_gradient", "[blue, rectangle([0, 0], [10, 10]), raster([[0, none], [1, 0.5]], [[0, 0], [10, 10]], viridis)]");
    assert!(near(pixel(&pm, px(2.5), py(2.5)), [0x44, 0x01, 0x54, 255], 1));
    assert_eq!(pixel(&pm, px(7.5), py(2.5)), BLUE);
    assert!(near(pixel(&pm, px(2.5), py(7.5)), [0xfd, 0xe7, 0x25, 255], 1));
    assert!(String::from_utf8_lossy(&o.pdf).contains("/SMask"));
    // a packed array of colours
    let cells: Vec<String> = (0..20).map(|r| format!("[{}]", (0..30).map(|c| format!("[{}, 0.0, {}]", c as f64 / 29.0, r as f64 / 19.0)).collect::<Vec<_>>().join(", "))).collect();
    let (pm, _) = probe("raster_packed", &format!("[raster([{}], [[0, 0], [10, 10]])]", cells.join(", ")));
    assert!(near(pixel(&pm, px(9.9), py(0.1)), [255, 0, 0, 255], 2), "{:?}", pixel(&pm, px(9.9), py(0.1)));
    assert!(near(pixel(&pm, px(0.1), py(9.9)), [0, 0, 255, 255], 2));
}

#[test]
fn colour_bars_show_the_gradient_and_its_scale() {
    let o = all("graphics([point([0, 0]), point([1, 1])], frame: true, image_size: [300, 200], color_bar: [gray, [0, 10], \"level\"])");
    keep("color_bar", &o);
    let pm = decode(&o.png);
    assert_eq!((pm.width(), pm.height()), (300, 200));
    // the bar is right of the plot area: dark at its bottom, light at its top
    let column: Vec<u32> = (0..300).filter(|&x| {
        let (top, bottom) = (pixel(&pm, x, 30), pixel(&pm, x, 170));
        top[0] > 200 && top[0] < 255 && bottom[0] < 60
    }).collect();
    assert!(!column.is_empty() && column[0] > 200, "{column:?}");
    assert!(svg_difference(&o) < 3.0);
    assert!(o.svg.contains(r#"aria-label="level""#) && o.svg.contains(r#"aria-label="10""#));
    pdf_streams(&o.pdf, "300", "200");
}
