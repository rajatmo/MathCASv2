//! Graphics written by `export_data` carry their term, so they can be read back as graphics: a PNG file gets an
//! `iTXt` chunk and an SVG file a `<metadata>` element, each holding the printed `graphics(...)` term under the key
//! [`KEY`]. [`import_graphics`] reads it back (PDF files are written without it).
use mc_core::{Heap, T};

/// The key of the embedded term.
pub const KEY: &str = "mathcas-graphics";
/// The largest term embedded, in bytes of its printed form (larger graphics are written without it).
const MAX_TERM: usize = 16 << 20;

/// `bytes` of a rendered file in `format` with the printed term `text` embedded (unchanged when the format holds
/// no term or the text is too long).
pub(crate) fn embed(bytes: Vec<u8>, format: &str, text: &str) -> Vec<u8> {
    if text.len() > MAX_TERM {
        return bytes;
    }
    match format {
        "png" => embed_png(bytes, text),
        "svg" => embed_svg(bytes, text),
        _ => bytes,
    }
}

/// The PNG with an `iTXt` chunk (uncompressed UTF-8) after `IHDR`.
fn embed_png(bytes: Vec<u8>, text: &str) -> Vec<u8> {
    // signature (8) and IHDR (4 + 4 + 13 + 4)
    const AFTER_IHDR: usize = 8 + 25;
    if bytes.len() < AFTER_IHDR || &bytes[12..16] != b"IHDR" {
        return bytes;
    }
    let mut data = Vec::with_capacity(KEY.len() + 5 + text.len());
    data.extend_from_slice(KEY.as_bytes());
    // keyword end, compression flag and method, empty language tag and translated keyword
    data.extend_from_slice(&[0, 0, 0, 0, 0]);
    data.extend_from_slice(text.as_bytes());
    let mut chunk = Vec::with_capacity(data.len() + 12);
    chunk.extend_from_slice(&(data.len() as u32).to_be_bytes());
    chunk.extend_from_slice(b"iTXt");
    chunk.extend_from_slice(&data);
    let crc = crc32(&chunk[4..]);
    chunk.extend_from_slice(&crc.to_be_bytes());
    let mut out = Vec::with_capacity(bytes.len() + chunk.len());
    out.extend_from_slice(&bytes[..AFTER_IHDR]);
    out.extend_from_slice(&chunk);
    out.extend_from_slice(&bytes[AFTER_IHDR..]);
    out
}

/// The SVG with a `<metadata>` element holding the term (XML-escaped) right after the opening `<svg ...>` tag.
fn embed_svg(bytes: Vec<u8>, text: &str) -> Vec<u8> {
    let Ok(s) = String::from_utf8(bytes) else { return Vec::new() };
    let Some(open) = s.find("<svg") else { return s.into_bytes() };
    let Some(end) = s[open..].find('>').map(|e| open + e + 1) else { return s.into_bytes() };
    let escaped = text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!("{}<metadata id=\"{KEY}\">{escaped}</metadata>{}", &s[..end], &s[end..]).into_bytes()
}

/// The printed term embedded in a PNG or SVG file's bytes, if there is one.
pub fn embedded_text(bytes: &[u8]) -> Option<String> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        let mut i = 8;
        while i + 12 <= bytes.len() {
            let len = u32::from_be_bytes(bytes[i..i + 4].try_into().ok()?) as usize;
            let kind = &bytes[i + 4..i + 8];
            let data = bytes.get(i + 8..i + 8 + len)?;
            if kind == b"iTXt" && data.starts_with(KEY.as_bytes()) && data.get(KEY.len()) == Some(&0) {
                // keyword, NUL, flag, method, language NUL, translated keyword NUL, text
                let rest = &data[KEY.len() + 3..];
                let lang_end = rest.iter().position(|&b| b == 0)?;
                let rest = &rest[lang_end + 1..];
                let tk_end = rest.iter().position(|&b| b == 0)?;
                return String::from_utf8(rest[tk_end + 1..].to_vec()).ok();
            }
            if kind == b"IDAT" || kind == b"IEND" {
                return None;
            }
            i += 12 + len;
        }
        return None;
    }
    let s = std::str::from_utf8(bytes).ok()?;
    let open = format!("<metadata id=\"{KEY}\">");
    let start = s.find(&open)? + open.len();
    let end = start + s[start..].find("</metadata>")?;
    Some(s[start..end].replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&"))
}

/// The graphics term a PNG or SVG file written by `export_data` holds, read on `h` (not evaluated); `None` for a file
/// without one. For an importer of `import_data`.
pub fn import_graphics<'h>(h: &'h Heap, path: &std::path::Path) -> Option<T<'h>> {
    let bytes = std::fs::read(path).ok()?;
    let text = embedded_text(&bytes)?;
    match mc_syntax::read(h, &text) {
        Ok(v) if v.len() == 1 && mc_graphics::is_graphics(h, v[0]) => Some(v[0]),
        _ => None,
    }
}

/// CRC-32 (IEEE, as PNG uses) of `data`.
fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_of_iend() {
        // the CRC every PNG file ends with
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
    }

    #[test]
    fn svg_text_round_trips() {
        let svg = b"<?xml version=\"1.0\"?><svg width=\"1\"><g/></svg>".to_vec();
        let t = "graphics([text(\"a<b & c\", [0, 0])])";
        let out = embed(svg, "svg", t);
        assert_eq!(embedded_text(&out).as_deref(), Some(t));
    }
}
