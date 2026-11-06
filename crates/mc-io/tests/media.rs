//! PNG and PDF import and export through the language: files written then read back, text extracted from PDF files
//! written here with known text, multi-page documents, the exporter hook, and damaged input (never a panic).
use mc_bench::{Engine, StopFlag};
use mc_core::{Heap, Store, View, T};
use mc_engine::CoreEngine;
use std::path::{Path, PathBuf};

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// A fresh folder for one test's files (forward slashes, so paths print without escapes).
fn scratch(name: &str) -> (PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("mc-io-media-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_string_lossy().replace('\\', "/");
    (dir, d)
}

/// A PDF file of `pages` pages, page k showing the text `texts[k]` in Helvetica at 24 points (written by hand, so
/// the test does not depend on the writer under test). `trailer_extra` goes into the trailer dictionary.
fn text_pdf(texts: &[&str], trailer_extra: &str) -> Vec<u8> {
    let n = texts.len();
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {n} >>",
            (0..n).map(|k| format!("{} 0 R ", 4 + 2 * k)).collect::<String>()
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
    ];
    for (k, t) in texts.iter().enumerate() {
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
            5 + 2 * k
        ));
        let content = format!("BT /F1 24 Tf 20 100 Td ({t}) Tj ET");
        objects.push(format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()));
    }
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = vec![];
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.push_str(&format!("{} 0 obj\n{o}\nendobj\n", i + 1));
    }
    let xref = out.len();
    out.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1));
    for o in offsets {
        out.push_str(&format!("{o:010} 00000 n \n"));
    }
    out.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R {trailer_extra} >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    ));
    out.into_bytes()
}

/// The expression for the dimensions of the rank-3 array `a`.
fn dims(a: &str) -> String {
    format!("{{ let d = {a}; [length(d), length(d[1]), length(d[1, 1])] }}")
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn png_round_trips() {
    let (dir, d) = scratch("png");
    let mut e = CoreEngine::new();
    // levels k/255 survive 8 bits exactly, in every channel layout
    for (c, px) in [(1, "x"), (2, "[x, y]"), (3, "[x, y, 102/255]"), (4, "[x, y, 102/255, 1]")] {
        eval(&mut e, &format!("a = n(table({px}, y, range(0, 20)/255, x, range(0, 40, 2)/255))"));
        eval(&mut e, "img = image(a)");
        let img = eval(&mut e, "img");
        assert!(img.starts_with("image(\""), "{c}: {img}");
        assert_eq!(eval(&mut e, "image_dimensions(img)"), "[21, 21]", "{c}");
        assert_eq!(eval(&mut e, "image_channels(img)"), c.to_string());
        let want = if c == 1 { "[21, 21, 1]" } else { &format!("[21, 21, {c}]") as &str };
        assert_eq!(eval(&mut e, &dims("image_data(img)")), want, "{c}");
        let same = if c == 1 { "image_data(img) === map(r => map(p => [p], r), a)" } else { "image_data(img) === a" };
        assert_eq!(eval(&mut e, same), "true", "{c}");
        // written out and read back
        let f = format!("{d}/c{c}.png");
        assert_eq!(eval(&mut e, &format!("export_data(\"{f}\", img)")), format!("\"{f}\""));
        assert_eq!(eval(&mut e, &format!("image_data(import_data(\"{f}\")) === image_data(img)")), "true");
        assert_eq!(eval(&mut e, &format!("import_data(\"{f}\", \"channels\")")), c.to_string());
    }
    // equal arrays give equal values (the file is named by its content)
    assert_eq!(eval(&mut e, "image([[0.0, 1.0], [1.0, 0.0]]) === image([[0, 1], [1, 0]])"), "true");
    // exact numbers go through n; values outside [0, 1] are clipped
    assert_eq!(eval(&mut e, "image_data(image([[1/2, 2, -1]]))"), "[[[0.5019607843137255], [1.0], [0.0]]]");
    // 16 bits keep 1/65535 steps
    eval(&mut e, "b = n(table([k/65535, 1 - k/65535, 0.5], j, 2, k, 300))");
    assert_eq!(eval(&mut e, "max(abs(image_data(image(b, bit_depth: 16)) - b)) < 1/131070"), "true");
    assert_eq!(eval(&mut e, "import_data(image(b, bit_depth: 16)[1], \"bit_depth\")"), "16");
    let f = format!("{d}/deep.png");
    eval(&mut e, &format!("export_data(\"{f}\", b, bit_depth: 16)"));
    assert_eq!(eval(&mut e, &format!("import_data(\"{f}\", \"bit_depth\")")), "16");
    assert_eq!(eval(&mut e, &format!("import_data(\"{f}\", \"dimensions\")")), "[300, 2]");
    // scaling, an array straight to PNG, dpi recorded
    let f = format!("{d}/big.png");
    eval(&mut e, &format!("export_data(\"{f}\", image([[0.0, 1.0], [1.0, 0.0]]), scale: 8)"));
    assert_eq!(eval(&mut e, &format!("import_data(\"{f}\", \"dimensions\")")), "[16, 16]");
    let f = format!("{d}/arr.png");
    eval(&mut e, &format!("export_data(\"{f}\", [[0.25, 0.5, 0.75]], dpi: 300)"));
    assert_eq!(eval(&mut e, &format!("import_data(\"{f}\", \"data\")")), "[[[0.25098039215686274], [0.5019607843137255], [0.7490196078431373]]]");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn plots_to_png_and_pdf() {
    let (dir, d) = scratch("plot");
    let mut e = CoreEngine::new();
    eval(&mut e, "p = plot(sin(x), [x, 0, 6])");
    // dpi re-renders the plot from its vector form
    let f = format!("{d}/p300.png");
    assert_eq!(eval(&mut e, &format!("export_data(\"{f}\", p, dpi: 216)")), format!("\"{f}\""));
    assert_eq!(eval(&mut e, &format!("import_data(\"{f}\", \"dimensions\")")), "[1920, 1200]");
    // the vector PDF, its page rendered at 72 dpi is the plot's size, and its text holds the tick labels
    let f = format!("{d}/p.pdf");
    eval(&mut e, &format!("export_data(\"{f}\", p)"));
    assert_eq!(eval(&mut e, &format!("import_data(\"{f}\", \"page_count\")")), "1");
    assert_eq!(eval(&mut e, &format!("image_dimensions(import_data(\"{f}\", \"page\", 1, dpi: 72))")), "[640, 400]");
    let text = eval(&mut e, &format!("import_data(\"{f}\", \"text\", 1)"));
    for label in ["0", "2", "4", "6", "0.5", "-0.5"] {
        assert!(text.contains(label), "{label} in {text}");
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn pdf_text_and_pages() {
    let (dir, d) = scratch("pdf");
    let mut e = CoreEngine::new();
    let f = format!("{d}/three.pdf");
    write(Path::new(&f), &text_pdf(&["First page here", "Second one", "Third and last"], ""));
    assert_eq!(eval(&mut e, &format!("import_data(\"{f}\", \"page_count\")")), "3");
    assert_eq!(
        eval(&mut e, &format!("import_data(\"{f}\", \"text\")")),
        r#"["First page here", "Second one", "Third and last"]"#
    );
    assert_eq!(eval(&mut e, &format!("import_data(\"{f}\", \"text\", -1)")), r#""Third and last""#);
    // the default: the pages as images, 300 x 200 points at 150 dpi
    eval(&mut e, &format!("ps = import_data(\"{f}\")"));
    assert_eq!(eval(&mut e, "length(ps)"), "3");
    assert_eq!(eval(&mut e, "map(image_dimensions, ps)"), "[[625, 416], [625, 416], [625, 416]]");
    // the page is white with dark text
    eval(&mut e, "px = image_data(ps[1])");
    assert_eq!(eval(&mut e, "px[1, 1]"), "[1.0, 1.0, 1.0]");
    assert_eq!(eval(&mut e, "min(px) < 0.2"), "true");
    // imported pages written back to PDF stay vector: their text survives
    let g = format!("{d}/back.pdf");
    assert_eq!(eval(&mut e, &format!("export_data(\"{g}\", reverse(ps))")), format!("\"{g}\""));
    assert_eq!(
        eval(&mut e, &format!("import_data(\"{g}\", \"text\")")),
        r#"["Third and last", "Second one", "First page here"]"#
    );
    // a multi-page document of a plot, an image and a page
    let m = format!("{d}/mixed.pdf");
    eval(&mut e, &format!("export_data(\"{m}\", [list_plot([1, 3, 2]), image([[0.0, 0.5], [0.5, 1.0]]), ps[2]])"));
    assert_eq!(eval(&mut e, &format!("import_data(\"{m}\", \"page_count\")")), "3");
    assert_eq!(eval(&mut e, &format!("import_data(\"{m}\", \"text\", 2)")), r#""""#);
    assert_eq!(eval(&mut e, &format!("import_data(\"{m}\", \"text\", 3)")), r#""Second one""#);
    assert_eq!(eval(&mut e, &format!("image_dimensions(import_data(\"{m}\", \"page\", 2, dpi: 72))")), "[2, 2]");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn raster_pages_are_lossless() {
    let (dir, d) = scratch("raster");
    let mut e = CoreEngine::new();
    // a 40 x 30 RGBA gradient on a page at 72 dpi (one pixel per point), rendered back at 72 dpi
    eval(&mut e, "a = n(table([x/39, y/29, 0.5, 1.0], y, range(0, 29), x, range(0, 39)))");
    let f = format!("{d}/one.pdf");
    assert_eq!(eval(&mut e, &format!("export_data(\"{f}\", image(a))")), format!("\"{f}\""));
    eval(&mut e, &format!("b = image_data(import_data(\"{f}\", \"page\", 1, dpi: 72))"));
    assert_eq!(eval(&mut e, &dims("b")), "[30, 40, 3]");
    assert_eq!(eval(&mut e, "max(abs(b - map(r => map(p => take(p, 3), r), a))) < 0.02"), "true");
    // a 16-bit gray image with dpi 144: half as many points as pixels
    let g = format!("{d}/two.pdf");
    eval(&mut e, &format!("export_data(\"{g}\", image(n(table(x/99, y, 50, x, range(0, 99))), bit_depth: 16), dpi: 144)"));
    assert_eq!(eval(&mut e, &format!("image_dimensions(import_data(\"{g}\", \"page\", 1, dpi: 144))")), "[100, 50]");
    std::fs::remove_dir_all(&dir).ok();
}

fn test_shape_exporter(h: &Heap, t: T<'_>, path: &Path, format: &str, _o: &mc_io::ExportOptions) -> Result<(), String> {
    let View::App { args: [k], .. } = h.view(t) else { return Err("test_shape takes one number".into()) };
    let View::Int(k) = h.view(*k) else { return Err("test_shape takes one number".into()) };
    match format {
        "pdf" => std::fs::write(path, text_pdf(&[&format!("Shape {k}")], "")).map_err(|e| e.to_string()),
        "png" => std::fs::write(path, b"made by the hook").map_err(|e| e.to_string()),
        _ => Err(format!("no {format}")),
    }
}

#[test]
fn exporter_hook() {
    mc_io::register_exporter("test_shape", &["png", "pdf"], test_shape_exporter);
    let (dir, d) = scratch("hook");
    let mut e = CoreEngine::new();
    let f = format!("{d}/s.png");
    assert_eq!(eval(&mut e, &format!("export_data(\"{f}\", test_shape(4))")), format!("\"{f}\""));
    assert_eq!(std::fs::read(&f).unwrap(), b"made by the hook");
    let f = format!("{d}/s.pdf");
    assert_eq!(eval(&mut e, &format!("export_data(\"{f}\", [test_shape(1), test_shape(2)])")), format!("\"{f}\""));
    assert_eq!(eval(&mut e, &format!("import_data(\"{f}\", \"text\")")), r#"["Shape 1", "Shape 2"]"#);
    // the exporter's own failure, and a format it does not take
    assert!(eval(&mut e, &format!("export_data(\"{d}/x.pdf\", test_shape(\"a\"))")).starts_with("failure(io, "));
    assert_eq!(eval(&mut e, &format!("export_data(\"{d}/x.json\", test_shape(1))")), format!("\"{d}/x.json\""));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn failures() {
    let (dir, d) = scratch("fail");
    let mut e = CoreEngine::new();
    let pdf = format!("{d}/ok.pdf");
    write(Path::new(&pdf), &text_pdf(&["a", "b"], ""));
    // elements, page numbers and options
    assert_eq!(
        eval(&mut e, &format!("import_data(\"{pdf}\", \"page\", 3)")),
        "failure(out_of_range, 3, [1, 2])"
    );
    assert_eq!(eval(&mut e, &format!("import_data(\"{pdf}\", \"page\", 0)")), "failure(out_of_range, 0, [1, 2])");
    assert_eq!(eval(&mut e, &format!("import_data(\"{pdf}\", \"page\")")), "failure(argument, import_data, 3)");
    assert_eq!(
        eval(&mut e, &format!("import_data(\"{pdf}\", \"words\")")),
        r#"failure(unknown_element, "words", ["pages", "page", "page_count", "text"])"#
    );
    assert_eq!(eval(&mut e, &format!("import_data(\"{pdf}\", \"pages\", dpi: 0)")), "failure(bad_option, dpi, 0)");
    assert_eq!(eval(&mut e, &format!("import_data(\"{pdf}\", \"pages\", size: 3)")), "failure(unknown_option, size)");
    assert_eq!(eval(&mut e, &format!("import_data(\"{pdf}\", \"page_count\", 1)")), "failure(argument, import_data, 3)");
    assert_eq!(eval(&mut e, &format!("import_data(\"{pdf}\", \"pages\", dpi: 10^6)")).split(',').next(), Some("[failure(damaged"));
    assert_eq!(eval(&mut e, "image(\"x.png\")"), "image(\"x.png\")");
    assert_eq!(eval(&mut e, "image([1, 2, \"a\"])"), "failure(argument, image, 1)");
    assert_eq!(eval(&mut e, "image([[[1, 2, 3, 4, 5]]])"), "failure(argument, image, 1)");
    assert_eq!(eval(&mut e, "image([[0.5]], bit_depth: 12)"), "failure(bad_option, bit_depth, 12)");
    assert_eq!(eval(&mut e, "image_data(5)"), "failure(argument, image_data, 1)");
    assert!(eval(&mut e, "image_data(image(\"no/such.png\"))").starts_with("failure(io, \"no/such.png\", "));
    assert_eq!(eval(&mut e, "export_data(\"a.png\", 5)"), "failure(argument, export_data, 2)");
    assert_eq!(eval(&mut e, "export_data(\"a.pdf\", [1, image(\"x.png\")])"), "failure(argument, export_data, 2)");
    assert_eq!(eval(&mut e, "export_data(\"a.png\", [[0.5]], quality: 3)"), "failure(unknown_option, quality)");
    assert_eq!(eval(&mut e, "export_data(\"a.csv\", [[1]], dpi: 3)"), "failure(unknown_option, dpi)");
    assert_eq!(eval(&mut e, "export_data(\"a.out\", [[1]], xyz)"), "failure(unknown_format, xyz)");
    // encrypted with an unknown password
    let enc = format!("{d}/enc.pdf");
    let id = "<0123456789abcdef0123456789abcdef>";
    let o = "<".to_string() + &"ab".repeat(32) + ">";
    write(
        Path::new(&enc),
        &text_pdf(&["secret"], &format!("/Encrypt << /Filter /Standard /V 1 /R 2 /O {o} /U {o} /P -4 >> /ID [{id} {id}]")),
    );
    assert_eq!(eval(&mut e, &format!("import_data(\"{enc}\", \"text\")")), format!("failure(encrypted, \"{enc}\")"));
    // not a PDF, not a PNG
    let bad = format!("{d}/bad.pdf");
    write(Path::new(&bad), b"hello");
    assert_eq!(
        eval(&mut e, &format!("import_data(\"{bad}\")")),
        format!("failure(damaged, \"{bad}\", \"not a readable PDF file\")")
    );
    let bad = format!("{d}/bad.png");
    write(Path::new(&bad), b"\x89PNG\r\n\x1a\nrest");
    assert!(eval(&mut e, &format!("import_data(\"{bad}\")")).starts_with(&format!("failure(damaged, \"{bad}\", ")));
    std::fs::remove_dir_all(&dir).ok();
}

/// Truncated and corrupted PNG and PDF files give values or failures, never a panic or an error.
#[test]
fn corrupt_input_never_panics() {
    let (dir, d) = scratch("corrupt");
    let mut e = CoreEngine::new();
    let f = format!("{d}/good.png");
    eval(&mut e, &format!("export_data(\"{f}\", n(table([x/9, y/9, 0.5], y, range(0, 9), x, range(0, 9))))"));
    let png = std::fs::read(&f).unwrap();
    let pdf = text_pdf(&["Some text", "More"], "");
    let mut seed = 12345u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let rounds = 60;
    for round in 0..rounds {
        if round % 20 == 0 {
            eprintln!("[{round}/{rounds} {}%]", round * 100 / rounds);
        }
        for (src, ext, reads) in [
            (&png, "png", &["import_data(F)", "import_data(F, \"data\")", "image_data(image(F))"][..]),
            (&pdf, "pdf", &["import_data(F, \"text\")", "import_data(F, \"pages\", dpi: 20)", "import_data(F, \"page_count\")"][..]),
        ] {
            let mut b = src.clone();
            if round % 2 == 0 {
                b.truncate(next() as usize % b.len());
            } else {
                for _ in 0..1 + next() % 8 {
                    let i = next() as usize % b.len();
                    b[i] = next() as u8;
                }
            }
            let p = format!("{d}/c{round}.{ext}");
            write(Path::new(&p), &b);
            for r in reads {
                let out = eval(&mut e, &r.replace('F', &format!("\"{p}\"")));
                assert!(!out.starts_with("error"), "{r} on round {round}: {out}");
            }
        }
    }
    std::fs::remove_dir_all(&dir).ok();
}
