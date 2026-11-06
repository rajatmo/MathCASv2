//! Writing PDF files: a document of pages, each either a raster image (embedded losslessly, Flate-compressed with
//! PNG row predictors, alpha as a soft mask) or a page of another PDF file (copied as a form XObject with every
//! object its resources reach, so vector content stays vector).

use crate::raster::{Raster, Samples};
use hayro::hayro_syntax::object::{Array, Dict, MaybeRef, Name, Object, ObjectIdentifier, Stream};
use hayro::hayro_syntax::page::{Page, Rotation};
use hayro::hayro_syntax::xref::XRef;
use std::collections::HashMap;
use std::io::Write as _;

/// The most objects one imported page may pull in (a guard against reference cycles through broken files and
/// against runaway documents).
const MAX_OBJECTS: usize = 1_000_000;

/// The deepest nesting of direct objects copied (deeper values are written as `null`).
const MAX_DEPTH: usize = 64;

/// A PDF document being assembled. Objects 1 and 2 are the catalog and the page tree, written by [`PdfOut::finish`].
pub struct PdfOut {
    objects: Vec<Vec<u8>>,
    pages: Vec<usize>,
}

impl Default for PdfOut {
    fn default() -> Self {
        PdfOut { objects: vec![Vec::new(), Vec::new()], pages: Vec::new() }
    }
}

/// Flate-compress `data`.
fn deflate(data: &[u8]) -> Vec<u8> {
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    // writing to a Vec cannot fail
    z.write_all(data).expect("writing to memory");
    z.finish().expect("writing to memory")
}

/// A stream object: `dict` (the entries inside `<< >>`, without `/Length`) and its data.
fn stream(dict: &str, data: &[u8]) -> Vec<u8> {
    let mut v = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
    v.extend_from_slice(data);
    v.extend_from_slice(b"\nendstream");
    v
}

/// A number as PDF writes it: integers without a point, others in plain decimal notation.
fn number(x: f64) -> String {
    if !x.is_finite() {
        return "0".into();
    }
    if x.fract() == 0.0 && x.abs() < 1e15 {
        return format!("{}", x as i64);
    }
    let s = format!("{x:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

impl PdfOut {
    /// A new empty document.
    pub fn new() -> PdfOut {
        PdfOut::default()
    }

    /// Add an object; its number.
    fn add(&mut self, body: Vec<u8>) -> usize {
        self.objects.push(body);
        self.objects.len()
    }

    /// The image as one page of `width * 72 / dpi` by `height * 72 / dpi` points.
    pub fn image_page(&mut self, r: &Raster, dpi: f64) {
        let colors = if r.channels >= 3 { 3 } else { 1 };
        let c = r.channels as usize;
        let bytes = (r.depth() / 8) as usize;
        let (w, h) = (r.width as usize, r.height as usize);
        let sample = |i: usize, out: &mut Vec<u8>| match &r.samples {
            Samples::U8(v) => out.push(v[i]),
            Samples::U16(v) => out.extend_from_slice(&v[i].to_be_bytes()),
        };
        let mut color = Vec::with_capacity(w * h * colors * bytes);
        let mut alpha = Vec::new();
        for p in 0..w * h {
            for k in 0..colors {
                sample(p * c + k, &mut color);
            }
            if r.has_alpha() {
                sample(p * c + c - 1, &mut alpha);
            }
        }
        let image = |data: &[u8], n: usize, extra: &str| {
            let params = format!(
                "/DecodeParms << /Predictor 15 /Colors {n} /BitsPerComponent {} /Columns {w} >>",
                r.depth()
            );
            stream(
                &format!(
                    "/Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /{} /BitsPerComponent {} \
                     /Filter /FlateDecode {params}{extra}",
                    if n == 3 { "DeviceRGB" } else { "DeviceGray" },
                    r.depth()
                ),
                &deflate(&paeth_rows(data, w * n * bytes, n * bytes)),
            )
        };
        let smask = if r.has_alpha() {
            let a = image(&alpha, 1, "");
            format!(" /SMask {} 0 R", self.add(a))
        } else {
            String::new()
        };
        let im = image(&color, colors, &smask);
        let im = self.add(im);
        let s = 72.0 / if dpi.is_finite() && dpi > 0.0 { dpi } else { 72.0 };
        let (pw, ph) = (number(w as f64 * s), number(h as f64 * s));
        let content = format!("q {pw} 0 0 {ph} 0 0 cm /Im0 Do Q");
        let content = self.add(stream("", content.as_bytes()));
        let page = format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw} {ph}] /Resources << /XObject << /Im0 {im} 0 R >> >> \
             /Contents {content} 0 R >>"
        );
        let page = self.add(page.into_bytes());
        self.pages.push(page);
    }

    /// Copy `page` (of a parsed PDF file) as the next page: its content becomes a form XObject drawn on a page of
    /// the same size and rotation. `stopped` is polled once per copied object.
    pub fn import_page(&mut self, page: &Page<'_>, stopped: &dyn Fn() -> bool) -> Result<(), String> {
        let content = page.page_stream().unwrap_or(&[]);
        let r = page.resources();
        let mut copier = Copier { xref: page.xref(), map: HashMap::new(), queue: Vec::new(), copied: 0 };
        let mut res = b"<<".to_vec();
        for (key, d) in [
            ("ExtGState", &r.ext_g_states),
            ("Font", &r.fonts),
            ("Properties", &r.properties),
            ("ColorSpace", &r.color_spaces),
            ("XObject", &r.x_objects),
            ("Pattern", &r.patterns),
            ("Shading", &r.shadings),
        ] {
            if !d.is_empty() {
                let _ = write!(res, " /{key} ");
                copier.dict(self, d, &mut res, 0);
            }
        }
        res.extend_from_slice(b" >>");
        // the objects the resources reach, one at a time from a work list (no recursion through references)
        while let Some((id, at)) = copier.queue.pop() {
            if stopped() {
                return Err("stopped".into());
            }
            copier.copied += 1;
            if copier.copied > MAX_OBJECTS {
                return Err("the page refers to too many objects".into());
            }
            let body = match copier.xref.get::<Object<'_>>(id) {
                Some(Object::Stream(s)) => copier.stream(self, &s),
                Some(o) => {
                    let mut v = Vec::new();
                    copier.object(self, &o, &mut v, 0);
                    v
                }
                None => b"null".to_vec(),
            };
            self.objects[at - 1] = body;
        }
        let b = page.intersected_crop_box();
        let (x0, y0, x1, y1) = (b.x0, b.y0, b.x1, b.y1);
        let mut form = format!(
            "<< /Type /XObject /Subtype /Form /BBox [{} {} {} {}] /Resources ",
            number(x0),
            number(y0),
            number(x1),
            number(y1)
        )
        .into_bytes();
        form.extend_from_slice(&res);
        let packed = deflate(content);
        let _ = write!(form, " /Filter /FlateDecode /Length {} >>\nstream\n", packed.len());
        form.extend_from_slice(&packed);
        form.extend_from_slice(b"\nendstream");
        let form = self.add(form);
        let draw = format!("q 1 0 0 1 {} {} cm /Fm0 Do Q", number(-x0), number(-y0));
        let draw = self.add(stream("", draw.as_bytes()));
        let rotate = match page.rotation() {
            Rotation::None => 0,
            Rotation::Horizontal => 90,
            Rotation::Flipped => 180,
            Rotation::FlippedHorizontal => 270,
        };
        let page = format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Rotate {rotate} /Resources << /XObject << /Fm0 \
             {form} 0 R >> >> /Contents {draw} 0 R >>",
            number(x1 - x0),
            number(y1 - y0)
        );
        let page = self.add(page.into_bytes());
        self.pages.push(page);
        Ok(())
    }

    /// The finished file.
    pub fn finish(mut self) -> Vec<u8> {
        self.objects[0] = b"<< /Type /Catalog /Pages 2 0 R >>".to_vec();
        let kids: String = self.pages.iter().map(|p| format!("{p} 0 R ")).collect();
        self.objects[1] = format!("<< /Type /Pages /Kids [{kids}] /Count {} >>", self.pages.len()).into_bytes();
        let mut out = b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n".to_vec();
        let mut offsets = Vec::with_capacity(self.objects.len());
        for (i, o) in self.objects.iter().enumerate() {
            offsets.push(out.len());
            let _ = writeln!(out, "{} 0 obj", i + 1);
            out.extend_from_slice(o);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref = out.len();
        let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", self.objects.len() + 1);
        for o in offsets {
            let _ = writeln!(out, "{o:010} 00000 n ");
        }
        let _ = write!(
            out,
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            self.objects.len() + 1
        );
        out
    }
}

/// Rows of `stride` bytes, each prefixed with PNG filter type 4 (Paeth) and filtered with `bpp` bytes per pixel.
fn paeth_rows(data: &[u8], stride: usize, bpp: usize) -> Vec<u8> {
    let rows = data.len() / stride.max(1);
    let mut out = Vec::with_capacity(data.len() + rows);
    for y in 0..rows {
        out.push(4);
        let row = &data[y * stride..(y + 1) * stride];
        let up = (y > 0).then(|| &data[(y - 1) * stride..y * stride]);
        for x in 0..stride {
            let a = if x >= bpp { row[x - bpp] as i16 } else { 0 };
            let b = up.map_or(0, |u| u[x] as i16);
            let c = if x >= bpp { up.map_or(0, |u| u[x - bpp] as i16) } else { 0 };
            let p = a + b - c;
            let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
            let pred = if pa <= pb && pa <= pc {
                a
            } else if pb <= pc {
                b
            } else {
                c
            };
            out.push(row[x].wrapping_sub(pred as u8));
        }
    }
    out
}

/// Copies objects of a source file into a [`PdfOut`], renumbering references.
struct Copier<'a> {
    xref: &'a XRef,
    /// source object to its number in the output
    map: HashMap<ObjectIdentifier, usize>,
    /// referenced objects not yet copied: source and output number
    queue: Vec<(ObjectIdentifier, usize)>,
    copied: usize,
}

impl Copier<'_> {
    /// The output number of source object `id` (reserved and queued on first sight).
    fn target(&mut self, out: &mut PdfOut, id: ObjectIdentifier) -> usize {
        if let Some(&n) = self.map.get(&id) {
            return n;
        }
        let n = out.add(b"null".to_vec());
        self.map.insert(id, n);
        self.queue.push((id, n));
        n
    }

    fn reference(&mut self, out: &mut PdfOut, id: ObjectIdentifier, v: &mut Vec<u8>) {
        let n = self.target(out, id);
        let _ = write!(v, "{n} 0 R");
    }

    fn entry(&mut self, out: &mut PdfOut, e: MaybeRef<Object<'_>>, v: &mut Vec<u8>, depth: usize) {
        match e {
            MaybeRef::Ref(r) => self.reference(out, ObjectIdentifier::new(r.obj_number, r.gen_number), v),
            MaybeRef::NotRef(o) => self.object(out, &o, v, depth),
        }
    }

    fn dict(&mut self, out: &mut PdfOut, d: &Dict<'_>, v: &mut Vec<u8>, depth: usize) {
        v.extend_from_slice(b"<<");
        for (k, e) in d.entries() {
            v.push(b' ');
            name(&k, v);
            v.push(b' ');
            self.entry(out, e, v, depth + 1);
        }
        v.extend_from_slice(b" >>");
    }

    fn array(&mut self, out: &mut PdfOut, a: &Array<'_>, v: &mut Vec<u8>, depth: usize) {
        v.push(b'[');
        for e in a.raw_iter() {
            v.push(b' ');
            self.entry(out, e, v, depth + 1);
        }
        v.extend_from_slice(b" ]");
    }

    fn object(&mut self, out: &mut PdfOut, o: &Object<'_>, v: &mut Vec<u8>, depth: usize) {
        if depth > MAX_DEPTH {
            v.extend_from_slice(b"null");
            return;
        }
        match o {
            Object::Null(_) => v.extend_from_slice(b"null"),
            Object::Boolean(b) => v.extend_from_slice(if *b { b"true" } else { b"false" }),
            Object::Number(n) => v.extend_from_slice(number(n.as_f64()).as_bytes()),
            Object::String(s) => {
                v.push(b'<');
                for b in s.as_bytes() {
                    let _ = write!(v, "{b:02x}");
                }
                v.push(b'>');
            }
            Object::Name(n) => name(n, v),
            Object::Dict(d) => self.dict(out, d, v, depth),
            Object::Array(a) => self.array(out, a, v, depth),
            // a stream met as a direct value (a resolved reference): it becomes an indirect object of its own
            Object::Stream(s) => {
                let n = self.target(out, s.obj_id());
                let _ = write!(v, "{n} 0 R");
            }
        }
    }

    /// A stream object: its dictionary (with a new `/Length`) and its data, still encoded by its filters.
    fn stream(&mut self, out: &mut PdfOut, s: &Stream<'_>) -> Vec<u8> {
        let data = s.raw_data();
        let mut v = b"<<".to_vec();
        for (k, e) in s.dict().entries() {
            if &*k == b"Length" {
                continue;
            }
            v.push(b' ');
            name(&k, &mut v);
            v.push(b' ');
            self.entry(out, e, &mut v, 1);
        }
        let _ = write!(v, " /Length {} >>\nstream\n", data.len());
        v.extend_from_slice(&data);
        v.extend_from_slice(b"\nendstream");
        v
    }
}

/// A name object, with `#xx` escapes for delimiters, `#` and bytes outside the printable range.
fn name(n: &Name<'_>, v: &mut Vec<u8>) {
    v.push(b'/');
    for &b in n.iter() {
        if (0x21..=0x7e).contains(&b) && !b"()<>[]{}/%#".contains(&b) {
            v.push(b);
        } else {
            let _ = write!(v, "#{b:02X}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(number(3.0), "3");
        assert_eq!(number(-0.5), "-0.5");
        assert_eq!(number(1.0 / 3.0), "0.333333");
        assert_eq!(number(f64::NAN), "0");
    }

    #[test]
    fn paeth_filter_reverses() {
        // undo the filter as a reader does and compare
        let data: Vec<u8> = (0..60u32).map(|i| (i * 37 % 251) as u8).collect();
        let (stride, bpp) = (12, 3);
        let f = paeth_rows(&data, stride, bpp);
        let mut back = vec![0u8; data.len()];
        for y in 0..5 {
            assert_eq!(f[y * (stride + 1)], 4);
            for x in 0..stride {
                let a = if x >= bpp { back[y * stride + x - bpp] as i16 } else { 0 };
                let b = if y > 0 { back[(y - 1) * stride + x] as i16 } else { 0 };
                let c = if x >= bpp && y > 0 { back[(y - 1) * stride + x - bpp] as i16 } else { 0 };
                let p = a + b - c;
                let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
                let pred = if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c };
                back[y * stride + x] = f[y * (stride + 1) + 1 + x].wrapping_add(pred as u8);
            }
        }
        assert_eq!(back, data);
    }
}
