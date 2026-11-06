//! Exports of generated notebooks: the HTML's elements and nesting, the PDF's pages, text (read back through the
//! fonts' `ToUnicode` maps) and pictures, and [`export`] by extension.
use super::*;
use std::collections::HashMap;
use std::path::PathBuf;

/// A fresh folder for one test.
fn folder(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mc-nbexport-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A 4 x 3 RGBA PNG with one transparent pixel.
fn png_bytes() -> Vec<u8> {
    let mut out = Vec::new();
    let mut e = png::Encoder::new(&mut out, 4, 3);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    let mut data = Vec::new();
    for i in 0..12u8 {
        data.extend_from_slice(&[i * 20, 100, 200, if i == 0 { 0 } else { 255 }]);
    }
    e.write_header().unwrap().write_image_data(&data).unwrap();
    out
}

/// A small notebook of every kind of part, with its picture `plot.png` in `dir`.
fn sample(dir: &Path) -> Notebook {
    std::fs::write(dir.join("plot.png"), png_bytes()).unwrap();
    let text = "#! mathcas 2 notebook\n\n\
        #% title\n# Sample <notes> & more\n\n\
        #% section\n# First steps\n\n\
        #% text\n# Some prose with `code` in it.\n\n\
        #% item\n# an item\n\n\
        #% itemnumbered\n# a numbered item\n\n\
        #% cell\nlet s = \"text\" # comment\nf(x_) := x^2\n#% output\n#| printed line\n#= x^2 + 1/3 + sqrt(y)\n\n\
        #% cell typeset\n((a)/(b)) + ((sqrt(c)))\n#% output\n#! failure(division_by_zero)\n#? a message\n\n\
        #% cell\nplot(x)\n#% output size=120x90\n#= image(\"plot.png\")\n\n\
        #% cell\nmissing()\n#% output\n#= image(\"gone.png\")\n";
    Notebook::parse(text).unwrap()
}

/// The element names of `html` balance (void elements aside) and nest properly.
fn assert_balanced(html: &str) {
    const VOID: [&str; 4] = ["meta", "img", "br", "mspace-void"];
    let mut stack: Vec<String> = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find('<') {
        rest = &rest[i + 1..];
        let end = rest.find('>').expect("a tag ends");
        let tag = &rest[..end];
        rest = &rest[end + 1..];
        if tag.starts_with('!') {
            continue;
        }
        if let Some(name) = tag.strip_prefix('/') {
            assert_eq!(stack.pop().as_deref(), Some(name), "closing </{name}>");
            continue;
        }
        let name = tag.split([' ', '/']).next().unwrap().to_string();
        assert!(!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric()), "tag <{tag}>");
        if !VOID.contains(&name.as_str()) && !tag.ends_with('/') {
            stack.push(name);
        }
    }
    assert!(stack.is_empty(), "unclosed: {stack:?}");
}

#[test]
fn html_shows_every_kind_of_part() {
    let dir = folder("html");
    let nb = sample(&dir);
    let h = to_html(&nb, &dir, &dir.join("out.html"), &Options::default()).unwrap();
    assert_balanced(&h);
    for needle in [
        "<title>Sample &lt;notes&gt; &amp; more</title>",
        "<h1>Sample &lt;notes&gt; &amp; more</h1>",
        "<h3>First steps</h3>",
        "<p>Some prose with <code>code</code> in it.</p>",
        "<ul class=\"items\">",
        "<span class=\"mark\">1.</span>a numbered item",
        "<pre class=\"input\"><span class=\"k\">let</span>",
        "<span class=\"s\">&quot;text&quot;</span>",
        "<div class=\"out print\">printed line</div>",
        "<div class=\"out value\">",
        "<msup>",
        "<mfrac><mrow><mn>1</mn></mrow><mrow><mn>3</mn></mrow></mfrac>",
        "<msqrt><mrow><mi mathvariant=\"normal\">y</mi></mrow></msqrt>",
        "<mfrac><mrow><mi mathvariant=\"normal\">a</mi></mrow><mrow><mi mathvariant=\"normal\">b</mi></mrow></mfrac>",
        "<div class=\"out failure\">failure(division_by_zero)</div>",
        "<div class=\"out note\">a message</div>",
        "<img src=\"data:image/png;base64,iVBORw0KGgo",
        "width=\"120\" height=\"90\"",
        "picture not found: gone.png",
    ] {
        assert!(h.contains(needle), "missing {needle:?} in\n{h}");
    }
    assert!(!h.contains("<script") && !h.contains("http:") && !h.contains("https:"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn html_pictures_can_go_into_a_folder() {
    let dir = folder("html-folder");
    let nb = sample(&dir);
    let out = dir.join("my notes.html");
    let opts = Options { images: Images::Folder, title: Some("Given".into()) };
    let h = to_html(&nb, &dir, &out, &opts).unwrap();
    assert!(h.contains("<title>Given</title>"));
    assert!(h.contains("<img src=\"my%20notes.files/plot.png\""), "{h}");
    assert_eq!(std::fs::read(dir.join("my notes.files").join("plot.png")).unwrap(), png_bytes());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The objects of a PDF file by number: the dictionary text and the decompressed stream, if any.
fn objects(pdf: &[u8]) -> HashMap<usize, (String, Option<Vec<u8>>)> {
    let mut out = HashMap::new();
    let find = |hay: &[u8], needle: &[u8], from: usize| hay[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from);
    let mut at = 0;
    while let Some(p) = find(pdf, b" 0 obj\n", at) {
        let start = pdf[..p].iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        let n: usize = std::str::from_utf8(&pdf[start..p]).unwrap().parse().unwrap();
        let body_start = p + 7;
        let end = find(pdf, b"\nendobj\n", body_start).unwrap();
        let body = &pdf[body_start..end];
        let (dict, data) = match find(body, b">>\nstream\n", 0) {
            Some(s) => {
                let data = &body[s + 10..body.len() - b"\nendstream".len()];
                let inflated = miniz_oxide::inflate::decompress_to_vec_zlib(data).expect("a Flate stream");
                (String::from_utf8_lossy(&body[..s + 2]).to_string(), Some(inflated))
            }
            None => (String::from_utf8_lossy(body).to_string(), None),
        };
        out.insert(n, (dict, data));
        at = end;
    }
    out
}

/// The object number after `key` in `dict` (`key N 0 R`).
fn reference(dict: &str, key: &str) -> Option<usize> {
    let i = dict.find(key)? + key.len();
    dict[i..].split_whitespace().next()?.parse().ok()
}

/// The text of each page, read through the fonts' `ToUnicode` maps (one line per text operation).
fn page_texts(pdf: &[u8]) -> Vec<String> {
    let objs = objects(pdf);
    let mut numbers: Vec<usize> = objs.keys().copied().collect();
    numbers.sort();
    let mut pages = Vec::new();
    for n in numbers {
        let (dict, _) = &objs[&n];
        if !dict.starts_with("<< /Type /Page /Parent") {
            continue;
        }
        let res = &objs[&reference(dict, "/Resources ").unwrap()].0;
        let mut maps: HashMap<String, HashMap<String, String>> = HashMap::new();
        if let Some(i) = res.find("/Font <<") {
            let fonts = &res[i + 8..];
            let fonts = &fonts[..fonts.find(">>").unwrap()];
            let words: Vec<&str> = fonts.split_whitespace().collect();
            for w in words.chunks(4) {
                let font = &objs[&w[1].parse::<usize>().unwrap()].0;
                let cmap = objs[&reference(font, "/ToUnicode ").unwrap()].1.clone().unwrap();
                let cmap = String::from_utf8(cmap).unwrap();
                let mut m = HashMap::new();
                for line in cmap.lines().filter(|l| l.starts_with('<') && l.len() >= 11 && l.matches('<').count() == 2) {
                    let (code, uni) = line.split_once(' ').unwrap();
                    let units: Vec<u16> = (1..uni.len() - 1).step_by(4).map(|k| u16::from_str_radix(&uni[k..k + 4], 16).unwrap()).collect();
                    m.insert(code[1..3].to_string(), String::from_utf16(&units).unwrap());
                }
                maps.insert(w[0].to_string(), m);
            }
        }
        let content = String::from_utf8(objs[&reference(dict, "/Contents ").unwrap()].1.clone().unwrap()).unwrap();
        let tokens: Vec<&str> = content.split_whitespace().collect();
        let mut text = String::new();
        let mut font = "";
        let mut baseline = "";
        for (i, t) in tokens.iter().enumerate() {
            match *t {
                "Tf" => font = tokens[i - 2],
                // a new line of text where the baseline changes (glyphs without a character, the pieces of a
                // radical sign, give nothing)
                "Tm" => {
                    if tokens[i - 1] != baseline && !text.is_empty() {
                        text.push('\n');
                    }
                    baseline = tokens[i - 1];
                }
                "Tj" => {
                    let hex = tokens[i - 1].trim_start_matches('<').trim_end_matches('>');
                    for k in (0..hex.len()).step_by(2) {
                        text.push_str(maps[font].get(&hex[k..k + 2]).map_or("", |s| s.as_str()));
                    }
                }
                _ => {}
            }
        }
        text.push('\n');
        pages.push(text);
    }
    pages
}

#[test]
fn pdf_has_text_pictures_and_page_numbers() {
    let dir = folder("pdf");
    let nb = sample(&dir);
    let pdf = to_pdf(&nb, &dir).unwrap();
    assert!(pdf.starts_with(b"%PDF-1.4\n") && pdf.ends_with(b"%%EOF\n"));
    let pages = page_texts(&pdf);
    assert_eq!(pages.len(), 1);
    let text = &pages[0];
    for needle in [
        "Sample <notes> & more",
        "First steps",
        "Some prose with ",
        "code",
        "a numbered item",
        "let s = \"text\" # comment",
        "printed line",
        "failure(division_by_zero)",
        "picture not found: gone.png",
        "1 / 1",
    ] {
        assert!(text.contains(needle), "missing {needle:?} in\n{text}");
    }
    let objs = objects(&pdf);
    let images: Vec<&String> = objs.values().map(|o| &o.0).filter(|d| d.contains("/Subtype /Image")).collect();
    // the picture and its soft mask (one pixel is transparent)
    assert_eq!(images.len(), 2, "{images:?}");
    assert!(images.iter().any(|d| d.contains("/Width 4 /Height 3 /ColorSpace /DeviceRGB") && d.contains("/SMask")));
    assert!(String::from_utf8_lossy(&pdf).contains("/Title (Sample <notes> & more)"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pdf_pages_follow_the_content() {
    // 100 one-line code cells: 25 to a page
    let mut text = String::from("#! mathcas 2 notebook\n\n");
    for i in 0..100 {
        text.push_str(&format!("#% cell\nx{i} = {i}\n\n"));
    }
    let pdf = to_pdf(&Notebook::parse(&text).unwrap(), Path::new(".")).unwrap();
    let pages = page_texts(&pdf);
    assert_eq!(pages.len(), 4);
    assert!(String::from_utf8_lossy(&pdf).contains("/Count 4 "));
    for (i, p) in pages.iter().enumerate() {
        assert!(p.contains(&format!("{} / 4", i + 1)), "{p}");
    }
    assert!(pages[0].contains("x0 = 0") && pages[3].contains("x99 = 99"));
    // one code cell of 200 lines continues over pages
    let source: Vec<String> = (0..200).map(|i| format!("line{i}")).collect();
    let text = format!("#! mathcas 2 notebook\n\n#% cell\n{}\n", source.join("\n"));
    let pages = page_texts(&to_pdf(&Notebook::parse(&text).unwrap(), Path::new(".")).unwrap());
    assert!(pages.len() >= 3, "{}", pages.len());
    assert!(pages[0].contains("line0\n") && !pages[0].contains("line199\n"));
    assert!(pages.last().unwrap().contains("line199\n"));
}

#[test]
fn pdf_wraps_long_lines_and_lays_out_boxes() {
    let long = "y".repeat(400);
    let text = format!("#! mathcas 2 notebook\n\n#% cell\n{long}\n#% output\n#= (a + b)/(c^2) + sqrt(2)\n");
    let pdf = to_pdf(&Notebook::parse(&text).unwrap(), Path::new(".")).unwrap();
    let page = &page_texts(&pdf)[0];
    let ys: Vec<&str> = page.lines().filter(|l| l.chars().all(|c| c == 'y') && !l.is_empty()).collect();
    assert!(ys.len() >= 4 && ys.iter().map(|l| l.len()).sum::<usize>() == 400, "{ys:?}");
    // the fraction bar and the radical's overbar are rectangles
    let objs = objects(&pdf);
    let content = objs.values().filter_map(|o| o.1.as_ref()).map(|d| String::from_utf8_lossy(d).to_string()).find(|c| c.contains("BT"));
    assert!(content.unwrap().matches(" re f").count() >= 3);
}

#[test]
fn export_chooses_the_format_by_extension() {
    let dir = folder("export");
    let nb = dir.join("nb.mcn");
    std::fs::write(&nb, "#! mathcas 2 notebook\n\n#% text\n# Hello\n\n#% cell\n1 + 1\n#% output\n#= 2\n").unwrap();
    export(&nb, &dir.join("a.html")).unwrap();
    assert!(std::fs::read_to_string(dir.join("a.html")).unwrap().contains("<p>Hello</p>"));
    export(&nb, &dir.join("b.HTM")).unwrap();
    assert!(dir.join("b.HTM").exists());
    export(&nb, &dir.join("c.pdf")).unwrap();
    assert!(std::fs::read(dir.join("c.pdf")).unwrap().starts_with(b"%PDF-"));
    let e = export(&nb, &dir.join("d.txt")).unwrap_err();
    assert!(e.contains("unknown export format"), "{e}");
    let e = export(&dir.join("none.mcn"), &dir.join("e.pdf")).unwrap_err();
    assert!(e.contains("none.mcn"), "{e}");
    std::fs::write(dir.join("bad.mcn"), "not a notebook\n").unwrap();
    let e = export(&dir.join("bad.mcn"), &dir.join("f.pdf")).unwrap_err();
    assert!(e.contains("line 1"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn base64_pads() {
    assert_eq!(base64(b""), "");
    assert_eq!(base64(b"f"), "Zg==");
    assert_eq!(base64(b"fo"), "Zm8=");
    assert_eq!(base64(b"foo"), "Zm9v");
    assert_eq!(base64(b"foobar"), "Zm9vYmFy");
}

#[test]
fn pdf_text_with_more_than_256_glyphs_of_one_font_reads_back() {
    // more distinct characters than one Type 3 font holds
    let chars: String = (0x21u32..0x250)
        .filter_map(char::from_u32)
        .filter(|c| c.is_alphanumeric() && pdf_has_prose_glyph(*c))
        .collect();
    assert!(chars.chars().count() > 256, "{}", chars.chars().count());
    let words: Vec<String> = chars.chars().collect::<Vec<_>>().chunks(30).map(|c| c.iter().collect()).collect();
    let text = format!("#! mathcas 2 notebook\n\n#% text\n# {}\n", words.join(" "));
    let pdf = to_pdf(&Notebook::parse(&text).unwrap(), Path::new(".")).unwrap();
    let page: String = page_texts(&pdf)[0].split_whitespace().collect();
    assert!(page.contains(&chars), "{page}");
    assert!(String::from_utf8_lossy(&pdf).contains("/F1 "));
}

fn pdf_has_prose_glyph(c: char) -> bool {
    crate::pdf::has_prose_glyph(c)
}
