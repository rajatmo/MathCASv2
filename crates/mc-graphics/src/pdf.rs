//! The PDF surface: a one-page PDF 1.4 document with vector paths, Flate-compressed streams, and the glyphs the
//! page uses embedded as Type 3 fonts (a subset: only those glyphs, drawn from the math font's outlines), with a
//! `ToUnicode` map so the text can be searched and copied. Opacity goes through `ExtGState` resources.
//!
//! The content stream starts by flipping the y axis (`1 0 0 -1 0 H cm`), so page coordinates are written as they
//! are (y down); text matrices flip glyphs back upright.
use crate::color::Rgba;
use crate::raster::even;
use crate::surface::{Cap, Path, Pen, Seg, Surface, Text};
use crate::svg::num;
use mc_mathfont::PathCmd;
use std::collections::HashMap;
use std::fmt::Write as _;

/// Glyphs per Type 3 font (single-byte codes).
const PER_FONT: usize = 256;

/// A page being written as PDF.
pub struct Pdf {
    width: f32,
    height: f32,
    content: String,
    /// opacity values in use, each an `ExtGState` named `/A<index>`
    alphas: Vec<u16>,
    /// the opacity in force (thousandths) and the ones saved by `q` for clips
    alpha: u16,
    saved: Vec<u16>,
    /// glyph id to (font, code)
    codes: HashMap<u16, (usize, u8)>,
    /// per font, its glyphs by code with the character each shows
    fonts: Vec<Vec<(u16, Option<char>)>>,
    /// images, each an `XObject` named `/Im<index>`: columns, rows, straight RGBA bytes (top row first)
    images: Vec<(usize, usize, Vec<u8>)>,
}

impl Pdf {
    /// An empty page of `width` x `height` points.
    pub fn new(width: f32, height: f32) -> Pdf {
        let mut content = String::new();
        let _ = writeln!(content, "1 0 0 -1 0 {} cm 1 j", num(height));
        Pdf { width, height, content, alphas: vec![], alpha: 1000, saved: vec![], codes: HashMap::new(), fonts: vec![], images: vec![] }
    }

    /// Use opacity `a` from here on.
    fn opacity(&mut self, a: f64) {
        let k = (a.clamp(0.0, 1.0) * 1000.0).round() as u16;
        if k == self.alpha {
            return;
        }
        let i = match self.alphas.iter().position(|&v| v == k) {
            Some(i) => i,
            None => {
                self.alphas.push(k);
                self.alphas.len() - 1
            }
        };
        let _ = writeln!(self.content, "/A{i} gs");
        self.alpha = k;
    }

    fn colour(&mut self, c: Rgba, op: &str) {
        let c = c.clamped();
        self.opacity(c.a);
        let f = |v: f64| num(v as f32);
        let _ = writeln!(self.content, "{} {} {} {op}", f(c.r), f(c.g), f(c.b));
    }

    fn path(&mut self, path: &Path) {
        let c = &mut self.content;
        for s in &path.segs {
            let _ = match *s {
                Seg::Move(x, y) => writeln!(c, "{} {} m", num(x), num(y)),
                Seg::Line(x, y) => writeln!(c, "{} {} l", num(x), num(y)),
                Seg::Cubic(a, b, d, e, x, y) => {
                    writeln!(c, "{} {} {} {} {} {} c", num(a), num(b), num(d), num(e), num(x), num(y))
                }
                Seg::Close => writeln!(c, "h"),
            };
        }
    }

    /// The font and code of a glyph, added to the subset on first use.
    fn code(&mut self, id: u16, ch: Option<char>) -> (usize, u8) {
        if let Some(&fc) = self.codes.get(&id) {
            return fc;
        }
        if self.fonts.last().is_none_or(|f| f.len() == PER_FONT) {
            self.fonts.push(Vec::new());
        }
        let f = self.fonts.len() - 1;
        let code = self.fonts[f].len() as u8;
        self.fonts[f].push((id, ch));
        self.codes.insert(id, (f, code));
        (f, code)
    }

    /// The document.
    pub fn finish(self) -> Vec<u8> {
        let mut doc = Doc::default();
        let (catalog, pages, page, content, info) = (doc.reserve(), doc.reserve(), doc.reserve(), doc.reserve(), doc.reserve());
        doc.set(catalog, format!("<< /Type /Catalog /Pages {pages} 0 R >>").into_bytes());
        doc.set(pages, format!("<< /Type /Pages /Kids [{page} 0 R] /Count 1 >>").into_bytes());
        doc.set(content, stream("", self.content.as_bytes()));
        doc.set(info, b"<< /Producer (MathCAS) >>".to_vec());
        let mut font_refs = String::new();
        for (k, glyphs) in self.fonts.iter().enumerate() {
            let f = type3(&mut doc, glyphs);
            let _ = write!(font_refs, " /F{k} {f} 0 R");
        }
        let mut gs = String::new();
        for (i, a) in self.alphas.iter().enumerate() {
            let a = num(*a as f32 / 1000.0);
            let _ = write!(gs, " /A{i} << /ca {a} /CA {a} >>");
        }
        let mut xobjects = String::new();
        for (k, (cols, rows, rgba)) in self.images.iter().enumerate() {
            let im = image(&mut doc, *cols, *rows, rgba);
            let _ = write!(xobjects, " /Im{k} {im} 0 R");
        }
        let mut res = String::from("<< /ProcSet [/PDF /Text /ImageC]");
        if !xobjects.is_empty() {
            let _ = write!(res, " /XObject <<{xobjects} >>");
        }
        if !font_refs.is_empty() {
            let _ = write!(res, " /Font <<{font_refs} >>");
        }
        if !gs.is_empty() {
            let _ = write!(res, " /ExtGState <<{gs} >>");
        }
        res.push_str(" >>");
        doc.set(
            page,
            format!(
                "<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 {} {}] /Resources {res} /Contents {content} 0 R >>",
                num(self.width),
                num(self.height)
            )
            .into_bytes(),
        );
        doc.write(catalog, info)
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

/// An image `XObject` of RGB cells, with a soft mask of their opacity when some cell is not opaque; gives its
/// object number. Cells are not smoothed (`/Interpolate false`).
fn image(doc: &mut Doc, cols: usize, rows: usize, rgba: &[u8]) -> usize {
    let rgb: Vec<u8> = rgba.as_chunks::<4>().0.iter().flat_map(|c| [c[0], c[1], c[2]]).collect();
    let dict = |space: &str| format!(" /Type /XObject /Subtype /Image /Width {cols} /Height {rows} /ColorSpace /{space} /BitsPerComponent 8 /Interpolate false");
    let mask = if rgba.as_chunks::<4>().0.iter().any(|c| c[3] < 255) {
        let alpha: Vec<u8> = rgba.as_chunks::<4>().0.iter().map(|c| c[3]).collect();
        let m = doc.add(stream(&dict("DeviceGray"), &alpha));
        format!(" /SMask {m} 0 R")
    } else {
        String::new()
    };
    doc.add(stream(&format!("{}{mask}", dict("DeviceRGB")), &rgb))
}

/// A Type 3 font of `glyphs` (by code), with its glyph procedures and `ToUnicode` map; gives its object number.
fn type3(doc: &mut Doc, glyphs: &[(u16, Option<char>)]) -> usize {
    let font = mc_mathfont::font();
    let mut procs = String::new();
    let mut widths = String::new();
    let mut names = String::new();
    let mut bbox = [0i32; 4];
    for (code, &(id, _)) in glyphs.iter().enumerate() {
        let m = font.metrics(id);
        let u = |v: f32| (v * 1000.0).round() as i32;
        let b = [u(m.x_min), u(-m.depth), u(m.x_max), u(m.height)];
        bbox = [bbox[0].min(b[0]), bbox[1].min(b[1]), bbox[2].max(b[2]), bbox[3].max(b[3])];
        let mut ops = format!("{} 0 {} {} {} {} d1\n", u(m.advance), b[0], b[1], b[2], b[3]);
        glyph_ops(&font.outline(id), &mut ops);
        ops.push_str("f\n");
        let obj = doc.add(stream("", ops.as_bytes()));
        let _ = write!(procs, " /g{code} {obj} 0 R");
        let _ = write!(widths, " {}", u(m.advance));
        let _ = write!(names, " /g{code}");
    }
    let cmap = doc.add(stream("", to_unicode(glyphs).as_bytes()));
    doc.add(
        format!(
            "<< /Type /Font /Subtype /Type3 /FontBBox [{} {} {} {}] /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs <<{procs} >> /Encoding << /Type /Encoding /Differences [0{names}] >> /FirstChar 0 /LastChar {} /Widths [{widths} ] /Resources << >> /ToUnicode {cmap} 0 R >>",
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
fn to_unicode(glyphs: &[(u16, Option<char>)]) -> String {
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

impl Surface for Pdf {
    fn fill(&mut self, path: &Path, color: Rgba) {
        if path.is_empty() {
            return;
        }
        self.colour(color, "rg");
        self.path(path);
        self.content.push_str("f\n");
    }

    fn stroke(&mut self, path: &Path, pen: &Pen) {
        if path.is_empty() {
            return;
        }
        self.colour(pen.color, "RG");
        let dash: Vec<String> = even(&pen.dash).iter().map(|v| num(*v)).collect();
        let cap = if pen.cap == Cap::Round { 1 } else { 0 };
        let _ = writeln!(self.content, "{} w {cap} J [{}] 0 d", num(pen.width), dash.join(" "));
        self.path(path);
        self.content.push_str("S\n");
    }

    fn dots(&mut self, centres: &[(f32, f32)], d: f32, color: Rgba) {
        if centres.is_empty() {
            return;
        }
        // zero-length subpaths with round caps are discs of the line width
        self.colour(color, "RG");
        let _ = writeln!(self.content, "{} w 1 J [] 0 d", num(d.max(0.5)));
        for &(x, y) in centres {
            let (x, y) = (num(x), num(y));
            let _ = writeln!(self.content, "{x} {y} m {x} {y} l");
        }
        self.content.push_str("S\n");
    }

    fn text(&mut self, t: &Text<'_>) {
        self.colour(t.color, "rg");
        if !t.glyphs.is_empty() {
            self.content.push_str("BT\n");
            let mut font = usize::MAX;
            let mut size = f32::NAN;
            for g in t.glyphs {
                let (f, code) = self.code(g.id, g.ch);
                if f != font || g.size != size {
                    let _ = writeln!(self.content, "/F{f} {} Tf", num(g.size));
                    (font, size) = (f, g.size);
                }
                let (x, y) = t.place(g.x, g.y);
                // glyph space is y up; the page here is y down
                let m = if t.up { "0 -1 -1 0" } else { "1 0 0 -1" };
                let _ = writeln!(self.content, "{m} {} {} Tm <{code:02X}> Tj", num(x), num(y));
            }
            self.content.push_str("ET\n");
        }
        for r in t.rules {
            let corners = [t.place(r[0], r[1]), t.place(r[0] + r[2], r[1]), t.place(r[0] + r[2], r[1] + r[3]), t.place(r[0], r[1] + r[3])];
            self.path(&Path::polyline(&corners, true));
            self.content.push_str("f\n");
        }
    }

    fn image(&mut self, r: [f32; 4], cols: usize, rows: usize, rgba: &[u8]) {
        if rgba.len() != cols * rows * 4 || cols == 0 || rows == 0 {
            return;
        }
        // the image's own opacity is its soft mask
        self.opacity(1.0);
        let k = self.images.len();
        self.images.push((cols, rows, rgba.to_vec()));
        // the unit square onto the rectangle, its top edge (the image's first row) at the top on this y-down page
        let _ = writeln!(self.content, "q {} 0 0 {} {} {} cm /Im{k} Do Q", num(r[2]), num(-r[3]), num(r[0]), num(r[1] + r[3]));
    }

    fn clip(&mut self, r: [f32; 4]) {
        self.saved.push(self.alpha);
        let _ = writeln!(self.content, "q {} {} {} {} re W n", num(r[0]), num(r[1]), num(r[2]), num(r[3]));
    }

    fn unclip(&mut self) {
        self.content.push_str("Q\n");
        if let Some(a) = self.saved.pop() {
            self.alpha = a;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_unicode_maps_astral_characters_as_pairs() {
        let m = to_unicode(&[(10, Some('\u{3b1}')), (11, None), (12, Some('\u{1d465}'))]);
        assert!(m.contains("2 beginbfchar\n<00> <03B1>\n<02> <D835DC65>\n"), "{m}");
    }
}
