//! The SVG surface: a standalone SVG 1.1 document, one user unit per point. Glyphs are paths defined once (in
//! thousandths of an em) and placed with `<use>`, so the file shows the same letters everywhere without fonts;
//! each label carries its text in `aria-label`. Dots are zero-length subpaths with round caps.
use crate::color::Rgba;
use crate::raster::even;
use crate::surface::{Cap, Path, Pen, Seg, Surface, Text};
use mc_mathfont::PathCmd;
use std::collections::BTreeSet;
use std::fmt::Write as _;

/// A page being written as SVG.
pub struct Svg {
    width: f32,
    height: f32,
    body: String,
    glyphs: BTreeSet<u16>,
    clips: usize,
}

/// A coordinate or length with at most two decimals, without trailing zeros.
pub fn num(v: f32) -> String {
    let r = (v * 100.0).round() / 100.0;
    let s = format!("{:.2}", if r == 0.0 { 0.0 } else { r });
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Text escaped for an attribute value.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if (c as u32) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// The `d` attribute of a path.
fn data(path: &Path) -> String {
    let mut d = String::with_capacity(path.segs.len() * 12);
    for s in &path.segs {
        let _ = match *s {
            Seg::Move(x, y) => write!(d, "M{} {}", num(x), num(y)),
            Seg::Line(x, y) => write!(d, "L{} {}", num(x), num(y)),
            Seg::Cubic(a, b, c, e, x, y) => {
                write!(d, "C{} {} {} {} {} {}", num(a), num(b), num(c), num(e), num(x), num(y))
            }
            Seg::Close => write!(d, "Z"),
        };
    }
    d
}

/// The fill or stroke colour attributes.
fn colour(attr: &str, c: Rgba) -> String {
    let c = c.clamped();
    if c.a < 1.0 {
        format!(r#"{attr}="{}" {attr}-opacity="{}""#, c.hex(), num(c.a as f32))
    } else {
        format!(r#"{attr}="{}""#, c.hex())
    }
}

impl Svg {
    /// An empty page of `width` x `height` points.
    pub fn new(width: f32, height: f32) -> Svg {
        Svg { width, height, body: String::new(), glyphs: BTreeSet::new(), clips: 0 }
    }

    /// The document.
    pub fn finish(self) -> Vec<u8> {
        let font = mc_mathfont::font();
        let mut out = String::with_capacity(self.body.len() + 4096);
        let _ = writeln!(
            out,
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" version="1.1" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#,
            w = num(self.width),
            h = num(self.height)
        );
        if !self.glyphs.is_empty() {
            out.push_str("<defs>\n");
            for g in &self.glyphs {
                let _ = writeln!(out, r#"<path id="g{g}" d="{}"/>"#, glyph_data(&font.outline(*g)));
            }
            out.push_str("</defs>\n");
        }
        out.push_str(&self.body);
        out.push_str("</svg>\n");
        out.into_bytes()
    }
}

/// A glyph outline in thousandths of an em, y down.
fn glyph_data(cmds: &[PathCmd]) -> String {
    let u = |v: f32| format!("{}", (v * 1000.0).round() as i32);
    let mut d = String::new();
    for c in cmds {
        let _ = match *c {
            PathCmd::Move(x, y) => write!(d, "M{} {}", u(x), u(-y)),
            PathCmd::Line(x, y) => write!(d, "L{} {}", u(x), u(-y)),
            PathCmd::Quad(a, b, x, y) => write!(d, "Q{} {} {} {}", u(a), u(-b), u(x), u(-y)),
            PathCmd::Cubic(a, b, c, e, x, y) => {
                write!(d, "C{} {} {} {} {} {}", u(a), u(-b), u(c), u(-e), u(x), u(-y))
            }
            PathCmd::Close => write!(d, "Z"),
        };
    }
    d
}

impl Surface for Svg {
    fn fill(&mut self, path: &Path, color: Rgba) {
        if !path.is_empty() {
            let _ = writeln!(self.body, r#"<path d="{}" {}/>"#, data(path), colour("fill", color));
        }
    }

    fn stroke(&mut self, path: &Path, pen: &Pen) {
        if path.is_empty() {
            return;
        }
        let mut a = format!(r#"fill="none" {} stroke-width="{}" stroke-linejoin="round""#, colour("stroke", pen.color), num(pen.width));
        if pen.cap == Cap::Round {
            a.push_str(r#" stroke-linecap="round""#);
        }
        let dash = even(&pen.dash);
        if !dash.is_empty() {
            let d: Vec<String> = dash.iter().map(|v| num(*v)).collect();
            let _ = write!(a, r#" stroke-dasharray="{}""#, d.join(" "));
        }
        let _ = writeln!(self.body, r#"<path d="{}" {a}/>"#, data(path));
    }

    fn dots(&mut self, centres: &[(f32, f32)], d: f32, color: Rgba) {
        if centres.is_empty() {
            return;
        }
        let mut p = String::with_capacity(centres.len() * 14);
        for &(x, y) in centres {
            let _ = write!(p, "M{} {}h0", num(x), num(y));
        }
        let _ = writeln!(
            self.body,
            r#"<path d="{p}" fill="none" {} stroke-width="{}" stroke-linecap="round"/>"#,
            colour("stroke", color),
            num(d.max(0.5))
        );
    }

    fn text(&mut self, t: &Text<'_>) {
        let _ = writeln!(self.body, r#"<g {} aria-label="{}">"#, colour("fill", t.color), escape(t.text));
        for g in t.glyphs {
            self.glyphs.insert(g.id);
            let (x, y) = t.place(g.x, g.y);
            let turn = if t.up { " rotate(-90)" } else { "" };
            let _ = writeln!(
                self.body,
                r##"<use xlink:href="#g{}" transform="translate({} {}){turn} scale({})"/>"##,
                g.id,
                num(x),
                num(y),
                format!("{:.5}", g.size / 1000.0).trim_end_matches('0')
            );
        }
        for r in t.rules {
            let corners = [t.place(r[0], r[1]), t.place(r[0] + r[2], r[1]), t.place(r[0] + r[2], r[1] + r[3]), t.place(r[0], r[1] + r[3])];
            let _ = writeln!(self.body, r#"<path d="{}"/>"#, data(&Path::polyline(&corners, true)));
        }
        self.body.push_str("</g>\n");
    }

    fn image(&mut self, r: [f32; 4], cols: usize, rows: usize, rgba: &[u8]) {
        let Some(png) = crate::raster::pixmap(cols, rows, rgba).and_then(|pm| pm.encode_png().ok()) else { return };
        let _ = writeln!(
            self.body,
            r#"<image x="{}" y="{}" width="{}" height="{}" preserveAspectRatio="none" image-rendering="optimizeSpeed" style="image-rendering:pixelated" xlink:href="data:image/png;base64,{}"/>"#,
            num(r[0]),
            num(r[1]),
            num(r[2]),
            num(r[3]),
            base64(&png)
        );
    }

    fn clip(&mut self, r: [f32; 4]) {
        self.clips += 1;
        let _ = writeln!(
            self.body,
            r#"<clipPath id="c{n}"><rect x="{}" y="{}" width="{}" height="{}"/></clipPath><g clip-path="url(#c{n})">"#,
            num(r[0]),
            num(r[1]),
            num(r[2]),
            num(r[3]),
            n = self.clips
        );
    }

    fn unclip(&mut self) {
        self.body.push_str("</g>\n");
    }
}

/// Bytes in base64 (RFC 4648, with padding).
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= c.len() {
                out.push(ABC[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_pads() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn numbers_are_short() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(1.256), "1.26");
        assert_eq!(num(-0.001), "0");
        assert_eq!(num(10.5), "10.5");
        assert_eq!(escape("a<b & \"c\""), "a&lt;b &amp; &quot;c&quot;");
    }
}
