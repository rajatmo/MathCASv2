//! Writing the PDF file: pages of drawing operations, the glyphs they use as Type 3 fonts (a subset by
//! construction: only those glyphs, drawn from the fonts' outlines) with `ToUnicode` maps so the text can be
//! searched and copied, pictures as image `XObject`s (RGB with a soft mask for transparency), Flate-compressed
//! streams, one shared resource dictionary.
//!
//! Operations use page coordinates in points from the top-left corner, y down; the writer turns them into PDF's
//! y-up space.
use super::fonts::{fonts, Font};
use mc_mathfont::PathCmd;
use std::collections::HashMap;
use std::fmt::Write as _;

/// An sRGB colour, 0xRRGGBB.
pub(crate) type Rgb = u32;

/// A glyph of a Type 3 font and the character it shows (none for pieces of math symbols).
type Coded = (u16, Option<char>);

/// Glyphs per Type 3 font (single-byte codes).
const PER_FONT: usize = 256;

/// One drawing operation on a page.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Op {
    /// `text` from its baseline origin (x, y) in `font` (missing glyphs from the fallback fonts)
    Text { x: f32, y: f32, size: f32, font: Font, color: Rgb, text: String },
    /// one glyph by id (math font pieces without a character)
    Glyph { x: f32, y: f32, size: f32, font: Font, glyph: u16, color: Rgb },
    /// a filled rectangle, top-left corner at (x, y)
    Rect { x: f32, y: f32, w: f32, h: f32, color: Rgb },
    /// a stroked rectangle
    Frame { x: f32, y: f32, w: f32, h: f32, color: Rgb },
    /// polylines stroked with `width`, their points relative to (x, y), y down (brackets drawn as strokes)
    Stroke { x: f32, y: f32, lines: Vec<Vec<[f32; 2]>>, width: f32, color: Rgb },
    /// picture `image` (an index from [`Writer::image`]) in the rectangle
    Image { x: f32, y: f32, w: f32, h: f32, image: usize },
    /// vector drawing `form` (an index from [`Writer::form`]) in the rectangle
    Form { x: f32, y: f32, w: f32, h: f32, form: usize },
}

impl Op {
    /// The operation moved down by `dy` and right by `dx`.
    pub(crate) fn shifted(mut self, dx: f32, dy: f32) -> Op {
        match &mut self {
            Op::Text { x, y, .. }
            | Op::Glyph { x, y, .. }
            | Op::Rect { x, y, .. }
            | Op::Frame { x, y, .. }
            | Op::Stroke { x, y, .. }
            | Op::Image { x, y, .. }
            | Op::Form { x, y, .. } => {
                *x += dx;
                *y += dy;
            }
        }
        self
    }
}

/// A decoded picture: RGB samples and, when some pixel is not opaque, the alpha samples.
pub(crate) struct Picture {
    pub(crate) cols: usize,
    pub(crate) rows: usize,
    pub(crate) rgb: Vec<u8>,
    pub(crate) alpha: Option<Vec<u8>>,
    /// smoothed when scaled (`/Interpolate`); cell images of colour bars and density plots are not
    pub(crate) smooth: bool,
}

/// A vector drawing as a form `XObject`: its content in its own units (y down), its size.
struct Form {
    content: String,
    size: (f32, f32),
}

/// A number for a content stream: at most two decimals, no trailing zeros.
pub(crate) fn num(v: f32) -> String {
    let v = if v.is_finite() { v } else { 0.0 };
    let s = format!("{:.2}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s.to_string()
    }
}

/// The document being written.
pub(crate) struct Writer {
    width: f32,
    height: f32,
    /// (font, glyph) to (Type 3 font, code)
    codes: HashMap<(Font, u16), (usize, u8)>,
    /// each Type 3 font: its source font and its glyphs by code, with the character each shows
    type3: Vec<(Font, Vec<Coded>)>,
    pictures: Vec<Picture>,
    forms: Vec<Form>,
    /// the opacities the forms' graphics states need (thousandths)
    alphas: Vec<u16>,
    /// the forms' shading dictionaries (`/Sh<n>`)
    shadings: Vec<String>,
    /// the forms' soft masks (`/GM<n>`): content, drawing size, and luminance (alpha otherwise)
    masks: Vec<(String, (f32, f32), bool)>,
    pages: Vec<String>,
}

impl Writer {
    /// An empty document of pages `width` x `height` points.
    pub(crate) fn new(width: f32, height: f32) -> Writer {
        Writer {
            width,
            height,
            codes: HashMap::new(),
            type3: vec![],
            pictures: vec![],
            forms: vec![],
            alphas: vec![],
            shadings: vec![],
            masks: vec![],
            pages: vec![],
        }
    }

    /// Add a vector drawing (its images become pictures of the document); gives its index for [`Op::Form`].
    pub(crate) fn form(&mut self, d: super::vector::Drawing) -> usize {
        // the drawing's names (counted from its own first image, shading and mask) become the document's
        let mut names: Vec<(String, String)> = vec![];
        for (k, p) in d.images.into_iter().enumerate() {
            names.push((format!("/Img{k} Do"), format!("/Im{} Do", self.image(p))));
        }
        for (k, sh) in d.shadings.into_iter().enumerate() {
            self.shadings.push(sh);
            names.push((format!("/Shd{k} sh"), format!("/Sh{} sh", self.shadings.len() - 1)));
        }
        let first_mask = self.masks.len();
        for k in 0..d.masks.len() {
            names.push((format!("/Msk{k} gs"), format!("/GM{} gs", first_mask + k)));
        }
        let rename = |mut c: String| {
            for (from, to) in &names {
                c = c.replace(from.as_str(), to);
            }
            c
        };
        for (c, luminance) in d.masks {
            self.masks.push((rename(c), d.size, luminance));
        }
        let content = rename(d.content);
        for a in d.alphas {
            if !self.alphas.contains(&a) {
                self.alphas.push(a);
            }
        }
        self.forms.push(Form { content, size: d.size });
        self.forms.len() - 1
    }

    /// Add a picture; gives its index for [`Op::Image`].
    pub(crate) fn image(&mut self, p: Picture) -> usize {
        self.pictures.push(p);
        self.pictures.len() - 1
    }

    /// The Type 3 font and code of a glyph, added to the subset on first use.
    fn code(&mut self, font: Font, id: u16, ch: Option<char>) -> (usize, u8) {
        if let Some(&fc) = self.codes.get(&(font, id)) {
            return fc;
        }
        let k = match self.type3.iter().rposition(|(f, gs)| *f == font && gs.len() < PER_FONT) {
            Some(k) => k,
            None => {
                self.type3.push((font, Vec::new()));
                self.type3.len() - 1
            }
        };
        let code = self.type3[k].1.len() as u8;
        self.type3[k].1.push((id, ch));
        self.codes.insert((font, id), (k, code));
        (k, code)
    }

    fn colour(out: &mut String, c: Rgb, op: &str) {
        let f = |s: u32| num(((c >> s) & 0xff) as f32 / 255.0);
        let _ = writeln!(out, "{} {} {} {op}", f(16), f(8), f(0));
    }

    /// Add a page with these operations.
    pub(crate) fn page(&mut self, ops: &[Op]) {
        let h = self.height;
        let mut c = String::new();
        for op in ops {
            match op {
                Op::Text { x, y, size, font, color, text } => {
                    if text.chars().all(char::is_control) {
                        continue;
                    }
                    Self::colour(&mut c, *color, "rg");
                    let _ = write!(c, "BT 1 0 0 1 {} {} Tm", num(*x), num(h - y));
                    let mut current = usize::MAX;
                    let mut open = false;
                    for ch in text.chars().filter(|ch| !ch.is_control()) {
                        let (f, g) = fonts().glyph(*font, ch);
                        let (k, code) = self.code(f, g, Some(ch));
                        if k != current {
                            if open {
                                c.push_str("> Tj");
                            }
                            let _ = write!(c, " /F{k} {} Tf <", num(*size));
                            current = k;
                            open = true;
                        }
                        let _ = write!(c, "{code:02X}");
                    }
                    if open {
                        c.push_str("> Tj");
                    }
                    c.push_str(" ET\n");
                }
                Op::Glyph { x, y, size, font, glyph, color } => {
                    Self::colour(&mut c, *color, "rg");
                    let (k, code) = self.code(*font, *glyph, None);
                    let _ = writeln!(c, "BT /F{k} {} Tf 1 0 0 1 {} {} Tm <{code:02X}> Tj ET", num(*size), num(*x), num(h - y));
                }
                Op::Rect { x, y, w, h: rh, color } => {
                    Self::colour(&mut c, *color, "rg");
                    let _ = writeln!(c, "{} {} {} {} re f", num(*x), num(h - y - rh), num(*w), num(*rh));
                }
                Op::Frame { x, y, w, h: rh, color } => {
                    Self::colour(&mut c, *color, "RG");
                    let _ = writeln!(c, "0.75 w {} {} {} {} re S", num(*x), num(h - y - rh), num(*w), num(*rh));
                }
                Op::Stroke { x, y, lines, width, color } => {
                    Self::colour(&mut c, *color, "RG");
                    let _ = write!(c, "q {} w 0 J 1 j", num(*width));
                    for l in lines {
                        for (i, p) in l.iter().enumerate() {
                            let _ = write!(c, " {} {} {}", num(x + p[0]), num(h - (y + p[1])), if i == 0 { "m" } else { "l" });
                        }
                    }
                    let _ = writeln!(c, " S Q");
                }
                Op::Image { x, y, w, h: ih, image } => {
                    let _ = writeln!(c, "q {} 0 0 {} {} {} cm /Im{image} Do Q", num(*w), num(*ih), num(*x), num(h - y - ih));
                }
                Op::Form { x, y, w, h: fh, form } => {
                    let (sw, sh) = self.forms[*form].size;
                    let (kx, ky) = (w / sw.max(1e-3), fh / sh.max(1e-3));
                    let _ = writeln!(c, "q {} 0 0 {} {} {} cm /Fm{form} Do Q", kx, ky, num(*x), num(h - y - fh));
                }
            }
        }
        self.pages.push(c);
    }

    /// The number of pages so far.
    pub(crate) fn pages(&self) -> usize {
        self.pages.len()
    }

    /// The file.
    pub(crate) fn finish(mut self, title: Option<&str>) -> Vec<u8> {
        let mut doc = Doc::default();
        let (catalog, pages, resources, info) = (doc.reserve(), doc.reserve(), doc.reserve(), doc.reserve());
        doc.set(catalog, format!("<< /Type /Catalog /Pages {pages} 0 R >>").into_bytes());
        let mut info_dict = String::from("<< /Producer (MathCAS) /Creator (MathCAS notebook export)");
        if let Some(t) = title {
            let _ = write!(info_dict, " /Title {}", text_string(t));
        }
        info_dict.push_str(" >>");
        doc.set(info, info_dict.into_bytes());
        let mut kids = String::new();
        let contents = std::mem::take(&mut self.pages);
        let count = contents.len();
        for content in contents {
            let stream_obj = doc.add(stream("", content.as_bytes()));
            let page = doc.add(
                format!(
                    "<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 {} {}] /Resources {resources} 0 R /Contents {stream_obj} 0 R >>",
                    num(self.width),
                    num(self.height)
                )
                .into_bytes(),
            );
            let _ = write!(kids, " {page} 0 R");
        }
        doc.set(pages, format!("<< /Type /Pages /Kids [{kids} ] /Count {count} >>").into_bytes());
        let mut font_refs = String::new();
        for (k, (font, glyphs)) in self.type3.iter().enumerate() {
            let f = type3(&mut doc, *font, glyphs);
            let _ = write!(font_refs, " /F{k} {f} 0 R");
        }
        let mut xobjects = String::new();
        for (k, p) in self.pictures.iter().enumerate() {
            let im = image(&mut doc, p);
            let _ = write!(xobjects, " /Im{k} {im} 0 R");
        }
        // vector drawings: forms whose content is y down (the matrix flips it), sharing the resources
        for (k, f) in self.forms.iter().enumerate() {
            let (w, h) = f.size;
            let dict = format!(
                " /Type /XObject /Subtype /Form /BBox [0 0 {} {}] /Matrix [1 0 0 -1 0 {}] /Resources {resources} 0 R",
                num(w),
                num(h),
                num(h)
            );
            let fm = doc.add(stream(&dict, f.content.as_bytes()));
            let _ = write!(xobjects, " /Fm{k} {fm} 0 R");
        }
        let mut gm = String::new();
        for (k, (content, (w, h), luminance)) in self.masks.iter().enumerate() {
            let dict = format!(
                " /Type /XObject /Subtype /Form /BBox [0 0 {} {}] /Group << /S /Transparency /CS /DeviceRGB >> /Resources {resources} 0 R",
                num(*w),
                num(*h)
            );
            let g = doc.add(stream(&dict, content.as_bytes()));
            let kind = if *luminance { "Luminosity" } else { "Alpha" };
            let _ = write!(gm, " /GM{k} << /SMask << /Type /Mask /S /{kind} /G {g} 0 R >> >>");
        }
        let mut res = String::from("<< /ProcSet [/PDF /Text /ImageC]");
        if !font_refs.is_empty() {
            let _ = write!(res, " /Font <<{font_refs} >>");
        }
        if !xobjects.is_empty() {
            let _ = write!(res, " /XObject <<{xobjects} >>");
        }
        if !self.alphas.is_empty() || !gm.is_empty() {
            res.push_str(" /ExtGState <<");
            for a in &self.alphas {
                let v = num(f32::from(*a) / 1000.0);
                let _ = write!(res, " /GS{a} << /ca {v} /CA {v} >>");
            }
            res.push_str(&gm);
            res.push_str(" >>");
        }
        if !self.shadings.is_empty() {
            res.push_str(" /Shading <<");
            for (k, sh) in self.shadings.iter().enumerate() {
                let _ = write!(res, " /Sh{k} {sh}");
            }
            res.push_str(" >>");
        }
        res.push_str(" >>");
        doc.set(resources, res.into_bytes());
        doc.write(catalog, info)
    }
}

/// A PDF text string: plain ASCII in parentheses (escaped), anything else as UTF-16BE with a byte order mark.
fn text_string(s: &str) -> String {
    if s.chars().all(|c| (' '..='~').contains(&c)) {
        let mut out = String::from("(");
        for c in s.chars() {
            if matches!(c, '(' | ')' | '\\') {
                out.push('\\');
            }
            out.push(c);
        }
        out.push(')');
        out
    } else {
        let mut out = String::from("<FEFF");
        for u in s.encode_utf16() {
            let _ = write!(out, "{u:04X}");
        }
        out.push('>');
        out
    }
}

/// A Flate-compressed stream object with extra dictionary entries `dict`.
fn stream(dict: &str, data: &[u8]) -> Vec<u8> {
    let z = miniz_oxide::deflate::compress_to_vec_zlib(data, 6);
    let mut out = format!("<< /Length {} /Filter /FlateDecode{dict} >>\nstream\n", z.len()).into_bytes();
    out.extend_from_slice(&z);
    out.extend_from_slice(b"\nendstream");
    out
}

/// An image `XObject` (with a soft mask when the picture has alpha); gives its object number.
fn image(doc: &mut Doc, p: &Picture) -> usize {
    let dict = |space: &str| {
        format!(
            " /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /{space} /BitsPerComponent 8 /Interpolate {}",
            p.cols, p.rows, p.smooth
        )
    };
    let mask = match &p.alpha {
        Some(a) => {
            let m = doc.add(stream(&dict("DeviceGray"), a));
            format!(" /SMask {m} 0 R")
        }
        None => String::new(),
    };
    doc.add(stream(&format!("{}{mask}", dict("DeviceRGB")), &p.rgb))
}

/// A Type 3 font of `glyphs` (by code) of `font`, with its glyph procedures and `ToUnicode` map; gives its object
/// number.
fn type3(doc: &mut Doc, font: Font, glyphs: &[Coded]) -> usize {
    let f = fonts();
    let u = |v: f32| (v * 1000.0).round() as i32;
    let mut procs = String::new();
    let mut widths = String::new();
    let mut names = String::new();
    let mut bbox = [0i32; 4];
    for (code, &(id, _)) in glyphs.iter().enumerate() {
        let b = f.bbox(font, id);
        let b = [u(b[0]), u(b[1]), u(b[2]), u(b[3])];
        bbox = [bbox[0].min(b[0]), bbox[1].min(b[1]), bbox[2].max(b[2]), bbox[3].max(b[3])];
        let adv = u(f.advance(font, id));
        let mut ops = format!("{adv} 0 {} {} {} {} d1\n", b[0], b[1], b[2], b[3]);
        let outline = f.outline(font, id);
        if !outline.is_empty() {
            glyph_ops(&outline, &mut ops);
            ops.push_str("f\n");
        }
        let obj = doc.add(stream("", ops.as_bytes()));
        let _ = write!(procs, " /g{code} {obj} 0 R");
        let _ = write!(widths, " {adv}");
        let _ = write!(names, " /g{code}");
    }
    let cmap = doc.add(stream("", to_unicode(glyphs).as_bytes()));
    doc.add(
        format!(
            "<< /Type /Font /Subtype /Type3 /Name /{} /FontBBox [{} {} {} {}] /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs <<{procs} >> /Encoding << /Type /Encoding /Differences [0{names}] >> /FirstChar 0 /LastChar {} /Widths [{widths} ] /Resources << >> /ToUnicode {cmap} 0 R >>",
            font.name(),
            bbox[0],
            bbox[1],
            bbox[2],
            bbox[3],
            glyphs.len().saturating_sub(1)
        )
        .into_bytes(),
    )
}

/// The path operators of a glyph outline in thousandths of an em (glyph space, y up); quadratic segments become
/// cubics.
fn glyph_ops(cmds: &[PathCmd], out: &mut String) {
    let u = |v: f32| (v * 1000.0).round() as i32;
    let mut cur = (0.0f32, 0.0f32);
    for c in cmds {
        let _ = match *c {
            PathCmd::Move(x, y) => {
                cur = (x, y);
                writeln!(out, "{} {} m", u(x), u(y))
            }
            PathCmd::Line(x, y) => {
                cur = (x, y);
                writeln!(out, "{} {} l", u(x), u(y))
            }
            PathCmd::Quad(a, b, x, y) => {
                let c1 = (cur.0 + 2.0 / 3.0 * (a - cur.0), cur.1 + 2.0 / 3.0 * (b - cur.1));
                let c2 = (x + 2.0 / 3.0 * (a - x), y + 2.0 / 3.0 * (b - y));
                cur = (x, y);
                writeln!(out, "{} {} {} {} {} {} c", u(c1.0), u(c1.1), u(c2.0), u(c2.1), u(x), u(y))
            }
            PathCmd::Cubic(a, b, c, d, x, y) => {
                cur = (x, y);
                writeln!(out, "{} {} {} {} {} {} c", u(a), u(b), u(c), u(d), u(x), u(y))
            }
            PathCmd::Close => writeln!(out, "h"),
        };
    }
}

/// The `ToUnicode` CMap of a font's codes (glyphs without a character are left out).
fn to_unicode(glyphs: &[Coded]) -> String {
    let pairs: Vec<(usize, char)> = glyphs.iter().enumerate().filter_map(|(k, g)| g.1.map(|c| (k, c))).collect();
    let mut s = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<00> <FF>\nendcodespacerange\n",
    );
    for chunk in pairs.chunks(100) {
        let _ = writeln!(s, "{} beginbfchar", chunk.len());
        for &(k, c) in chunk {
            let mut units = [0u16; 2];
            let hex: String = c.encode_utf16(&mut units).iter().map(|u| format!("{u:04X}")).collect();
            let _ = writeln!(s, "<{k:02X}> <{hex}>");
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s
}

/// The objects of a document, numbered from 1.
#[derive(Default)]
struct Doc {
    objects: Vec<Vec<u8>>,
}

impl Doc {
    fn reserve(&mut self) -> usize {
        self.objects.push(Vec::new());
        self.objects.len()
    }

    fn set(&mut self, n: usize, body: Vec<u8>) {
        self.objects[n - 1] = body;
    }

    fn add(&mut self, body: Vec<u8>) -> usize {
        self.objects.push(body);
        self.objects.len()
    }

    /// The file: header, objects, cross-reference table and trailer.
    fn write(self, root: usize, info: usize) -> Vec<u8> {
        let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets = Vec::with_capacity(self.objects.len());
        for (i, o) in self.objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            out.extend_from_slice(o);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref = out.len();
        let mut t = format!("xref\n0 {}\n0000000000 65535 f \n", self.objects.len() + 1);
        for o in offsets {
            let _ = writeln!(t, "{o:010} 00000 n ");
        }
        let _ = write!(
            t,
            "trailer\n<< /Size {} /Root {root} 0 R /Info {info} 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            self.objects.len() + 1
        );
        out.extend_from_slice(t.as_bytes());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_strings() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(-0.001), "0");
        assert_eq!(num(2.345), "2.35");
        assert_eq!(text_string("a (b)"), "(a \\(b\\))");
        assert_eq!(text_string("\u{3b1}"), "<FEFF03B1>");
    }

    #[test]
    fn to_unicode_maps_astral_characters_as_pairs() {
        let m = to_unicode(&[(10, Some('\u{3b1}')), (11, None), (12, Some('\u{1d465}'))]);
        assert!(m.contains("2 beginbfchar\n<00> <03B1>\n<02> <D835DC65>\n"), "{m}");
    }
}
