//! Pictures in notebooks (design `08-notebook.md` 6 and 7.5): the value text of an image (`image("path")`), the
//! picture formats a notebook shows, the `size=WxH` attribute of an output block, and the sidecar folder that holds
//! a saved notebook's pictures.
//!
//! While a session runs, plot files live in the system's temporary folder. Since round 13 saving embeds every
//! picture in the notebook's text ([`crate::embed`]); older notebooks name files in a `<notebook name>.files/`
//! folder next to them (a relative path is relative to the notebook's folder). A PNG may mark a pixel density
//! ([`png_density`]): it is shown at its pixel size divided by it.
use crate::Attrs;
use std::path::{Path, PathBuf};

/// A picture format the notebook shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    /// a PNG file
    Png,
    /// an SVG drawing (rasterised at the display's scale)
    Svg,
}

impl ImageFormat {
    /// The format of the file at `path`: by its extension (`.png`, `.svg`, `.svgz`), else by its first bytes.
    pub fn of(path: &Path) -> Option<ImageFormat> {
        let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
        match ext.as_deref() {
            Some("png") => return Some(ImageFormat::Png),
            Some("svg") | Some("svgz") => return Some(ImageFormat::Svg),
            _ => {}
        }
        use std::io::Read;
        let mut head = [0u8; 256];
        let n = std::fs::File::open(path).and_then(|mut f| f.read(&mut head)).ok()?;
        ImageFormat::sniff(&head[..n])
    }

    /// The format of a file from its first bytes: the PNG signature, or an XML text with an `<svg` element.
    pub fn sniff(head: &[u8]) -> Option<ImageFormat> {
        if head.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
            return Some(ImageFormat::Png);
        }
        let text = String::from_utf8_lossy(head);
        let t = text.trim_start_matches('\u{feff}').trim_start();
        (t.starts_with('<') && text.contains("<svg")).then_some(ImageFormat::Svg)
    }

    /// The protocol's name of the format (the `kind` of a `display` response).
    pub fn kind(self) -> &'static str {
        match self {
            ImageFormat::Png => "image",
            ImageFormat::Svg => "svg",
        }
    }
}

/// The size of an SVG drawing from its root element: `width` and `height` (in `px`, `pt`, `mm`, `cm`, `in` or
/// without a unit), else the `viewBox`'s, else the CSS default 300 x 150; `None` without an `<svg` element.
pub fn svg_size(text: &str) -> Option<(u32, u32)> {
    let start = text.find("<svg")?;
    let tag = &text[start..];
    let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
    let attr = |name: &str| -> Option<&str> {
        let mut rest = tag;
        loop {
            let at = rest.find(name)?;
            let before = rest[..at].chars().last();
            let after = rest[at + name.len()..].trim_start();
            rest = &rest[at + name.len()..];
            if before.is_some_and(char::is_whitespace) {
                if let Some(v) = after.strip_prefix('=') {
                    let v = v.trim_start();
                    let q = v.chars().next().filter(|c| *c == '"' || *c == '\'')?;
                    let v = &v[1..];
                    return v.find(q).map(|e| &v[..e]);
                }
            }
        }
    };
    let length = |v: &str| -> Option<f64> {
        let v = v.trim();
        let split = v.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e')).unwrap_or(v.len());
        let x: f64 = v[..split].parse().ok()?;
        let scale = match v[split..].trim() {
            "" | "px" => 1.0,
            "pt" => 4.0 / 3.0,
            "pc" => 16.0,
            "mm" => 96.0 / 25.4,
            "cm" => 96.0 / 2.54,
            "in" => 96.0,
            _ => return None,
        };
        Some(x * scale).filter(|x| x.is_finite() && *x > 0.0)
    };
    let view = attr("viewBox").and_then(|v| {
        let n: Vec<f64> = v.split([' ', ',']).filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).collect();
        (n.len() == 4 && n[2] > 0.0 && n[3] > 0.0).then(|| (n[2], n[3]))
    });
    let w = attr("width").and_then(length);
    let h = attr("height").and_then(length);
    let (w, h) = match (w, h, view) {
        (Some(w), Some(h), _) => (w, h),
        (Some(w), None, Some((vw, vh))) => (w, w * vh / vw),
        (None, Some(h), Some((vw, vh))) => (h * vw / vh, h),
        (None, None, Some(v)) => v,
        _ => (300.0, 150.0),
    };
    let px = |x: f64| x.round().clamp(1.0, 100_000.0) as u32;
    Some((px(w), px(h)))
}

/// The size in pixels of the picture at `path`: a PNG file's from its header, an SVG drawing's from its root
/// element ([`svg_size`]); `None` when it cannot be read or is neither.
pub fn picture_size(path: &Path) -> Option<(u32, u32)> {
    use std::io::Read;
    let mut head = vec![0u8; 8192];
    let n = std::fs::File::open(path).and_then(|mut f| f.read(&mut head)).ok()?;
    let head = &head[..n];
    match ImageFormat::sniff(head).or_else(|| ImageFormat::of(path))? {
        ImageFormat::Png => {
            let ihdr = head.get(12..24).filter(|h| h.starts_with(b"IHDR"))?;
            let be = |i: usize| u32::from_be_bytes([ihdr[i], ihdr[i + 1], ihdr[i + 2], ihdr[i + 3]]);
            // a picture of k pixels per point (the high-resolution PNG of a dense plot) is shown at its points
            let k = png_density(head);
            Some((be(4).div_ceil(k), be(8).div_ceil(k))).filter(|&(w, h)| w > 0 && h > 0)
        }
        ImageFormat::Svg => svg_size(&String::from_utf8_lossy(head)),
    }
}

/// Pixels per inch of the pixel density a PNG marks with `pHYs` as k pixels per point: 72 k (k = 2, 3, 4).
const POINT_PPM: f64 = 72.0 / 0.0254;

/// The pixel density of a PNG file from its first bytes: k when its `pHYs` chunk (before the image data) says 72 k
/// dots per inch for k = 2, 3 or 4 (within 1 %), else 1. Such a picture is drawn at its size divided by k: the PNG
/// [`crate::embed`] makes of a dense plot says 144 dpi, as high-resolution screenshots do.
pub fn png_density(head: &[u8]) -> u32 {
    let mut at = 8;
    while let Some(chunk) = head.get(at..at + 8) {
        let len = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as usize;
        let kind = &chunk[4..8];
        if kind == b"IDAT" || kind == b"IEND" {
            break;
        }
        if kind == b"pHYs" && len == 9 {
            let Some(d) = head.get(at + 8..at + 17) else { break };
            let (x, y, unit) = (u32::from_be_bytes([d[0], d[1], d[2], d[3]]), u32::from_be_bytes([d[4], d[5], d[6], d[7]]), d[8]);
            if unit == 1 && x == y {
                for k in 2..=4u32 {
                    if (x as f64 / (POINT_PPM * k as f64) - 1.0).abs() < 0.01 {
                        return k;
                    }
                }
            }
            break;
        }
        at += 12 + len;
    }
    1
}

/// A PNG file with a `pHYs` chunk saying `k` pixels per point (72 k dots per inch) right after its header chunk
/// (an existing `pHYs` is left out). Bytes that are not a PNG come back unchanged.
pub fn with_density(png: &[u8], k: u32) -> Vec<u8> {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if !png.starts_with(&SIG) || png.get(12..16) != Some(b"IHDR") {
        return png.to_vec();
    }
    let ppm = (POINT_PPM * k as f64).round() as u32;
    let mut data = Vec::with_capacity(9);
    data.extend_from_slice(&ppm.to_be_bytes());
    data.extend_from_slice(&ppm.to_be_bytes());
    data.push(1);
    let mut chunk = Vec::with_capacity(21);
    chunk.extend_from_slice(&9u32.to_be_bytes());
    chunk.extend_from_slice(b"pHYs");
    chunk.extend_from_slice(&data);
    chunk.extend_from_slice(&crc32(&chunk[4..]).to_be_bytes());
    let mut out = Vec::with_capacity(png.len() + chunk.len());
    out.extend_from_slice(&png[..33]);
    out.extend_from_slice(&chunk);
    let mut at = 33;
    while let Some(head) = png.get(at..at + 8) {
        let len = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        let end = (at + 12 + len).min(png.len());
        if &head[4..8] != b"pHYs" {
            out.extend_from_slice(&png[at..end]);
        }
        at = end;
    }
    out
}

/// CRC-32 (ISO-HDLC, as PNG chunks use).
fn crc32(bytes: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in bytes {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

/// The path an image value names: `value` is `image("path")` (white space around it allowed), the string in the
/// language's literal syntax (`\"`, `\\`, `\n`, `\t`, `\u{...}` escapes).
pub fn image_path(value: &str) -> Option<String> {
    let inner = value.trim().strip_prefix("image(")?.strip_suffix(')')?.trim();
    let body = inner.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return None,
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                'u' => {
                    if chars.next()? != '{' {
                        return None;
                    }
                    let hex: String = chars.by_ref().take_while(|&c| c != '}').collect();
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                _ => return None,
            },
            c => out.push(c),
        }
    }
    (!out.is_empty()).then_some(out)
}

/// The value text of an image naming `path` (the inverse of [`image_path`]).
pub fn image_value(path: &str) -> String {
    let mut out = String::from("image(\"");
    for c in path.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out.push_str("\")");
    out
}

/// The display size of a graphic (`size=WxH` on its output block, in points at 100 %).
pub fn size_of(attrs: &Attrs) -> Option<(u32, u32)> {
    let (w, h) = attrs.get("size")?.split_once('x')?;
    let (w, h) = (w.parse::<u32>().ok()?, h.parse::<u32>().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

/// The `size` attribute's value for a display size.
pub fn size_value(w: u32, h: u32) -> String {
    format!("{w}x{h}")
}

/// Where a path an output names is: absolute paths as they are, relative ones in `dir` (the notebook's folder).
pub fn resolve(path: &str, dir: Option<&Path>) -> PathBuf {
    let p = Path::new(path);
    match dir {
        Some(d) if p.is_relative() => d.join(p),
        _ => p.to_path_buf(),
    }
}

/// The embedded name of a picture ([`crate::embed`]): the first word of its file name (`plot` of `plot-1234-0.png`, else `image`), a
/// hash of its content and its extension, so equal pictures share a file and a name never changes on later saves.
pub(crate) fn content_name(source: &Path, bytes: &[u8]) -> String {
    let stem = source.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let word: String = stem.chars().take_while(|c| c.is_ascii_alphabetic()).take(24).collect();
    let word = if word.is_empty() { "image".to_string() } else { word };
    let ext = source
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .filter(|e| !e.is_empty() && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or_else(|| "png".into());
    format!("{word}-{:012x}.{ext}", fnv1a(bytes) & 0xffff_ffff_ffff)
}

/// FNV-1a, 64 bits: a content hash that is the same on every platform and Rust version.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_values_read_and_print() {
        assert_eq!(image_path("image(\"a.png\")").as_deref(), Some("a.png"));
        assert_eq!(image_path("  image( \"C:\\\\tmp\\\\p.png\" ) ").as_deref(), Some("C:\\tmp\\p.png"));
        assert_eq!(image_path("image(\"a\\\"b\\u{e9}.svg\")").as_deref(), Some("a\"bé.svg"));
        for bad in ["image(a.png)", "image(\"\")", "image(\"a\" \"b\")", "img(\"a\")", "image(\"a\\q\")", "image(\"a"] {
            assert_eq!(image_path(bad), None, "{bad}");
        }
        for p in ["a.png", "C:\\x y\\p.png", "q\"uote\n.svg", "ü/ß.png"] {
            assert_eq!(image_path(&image_value(p)).as_deref(), Some(p));
        }
    }

    #[test]
    fn formats_by_extension_and_content() {
        assert_eq!(ImageFormat::of(Path::new("x/y.PNG")), Some(ImageFormat::Png));
        assert_eq!(ImageFormat::of(Path::new("y.svg")), Some(ImageFormat::Svg));
        assert_eq!(ImageFormat::sniff(b"\x89PNG\r\n\x1a\n...."), Some(ImageFormat::Png));
        assert_eq!(ImageFormat::sniff(b"<?xml version=\"1.0\"?>\n<svg width=\"3\">"), Some(ImageFormat::Svg));
        assert_eq!(ImageFormat::sniff(b"GIF89a"), None);
        assert_eq!(ImageFormat::of(Path::new("no such file.dat")), None);
    }

    #[test]
    fn svg_sizes_from_the_root_element() {
        let svg = |attrs: &str| format!("<?xml version=\"1.0\"?>\n<!-- c -->\n<svg xmlns=\"x\" {attrs}><g/></svg>");
        assert_eq!(svg_size(&svg("width=\"640\" height=\"400\"")), Some((640, 400)));
        assert_eq!(svg_size(&svg("width='3in' height='72pt'")), Some((288, 96)));
        assert_eq!(svg_size(&svg("viewBox=\"0 0 50 25\"")), Some((50, 25)));
        assert_eq!(svg_size(&svg("height=\"100\" viewBox=\"0,0,50,25\"")), Some((200, 100)));
        // percentages are not sizes: the view box decides, else the CSS default
        assert_eq!(svg_size(&svg("width=\"100%\" viewBox=\"0 0 10 20\"")), Some((10, 20)));
        assert_eq!(svg_size(&svg("")), Some((300, 150)));
        // `stroke-width` is not `width`
        assert_eq!(svg_size(&svg("stroke-width=\"7\" viewBox=\"0 0 8 9\"")), Some((8, 9)));
        assert_eq!(svg_size("<html/>"), None);
    }

    #[test]
    fn picture_sizes_from_files() {
        let dir = std::env::temp_dir().join(format!("mc-nbfile-size-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("a.bin");
        let mut head = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        head.extend(640u32.to_be_bytes());
        head.extend(400u32.to_be_bytes());
        std::fs::write(&png, &head).unwrap();
        assert_eq!(picture_size(&png), Some((640, 400)));
        let svg = dir.join("b.svg");
        std::fs::write(&svg, "<svg width=\"12\" height=\"34\"/>").unwrap();
        assert_eq!(picture_size(&svg), Some((12, 34)));
        assert_eq!(picture_size(&dir.join("none.png")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sizes() {
        let mut a = Attrs::new();
        assert_eq!(size_of(&a), None);
        a.set("size", Some(size_value(360, 225)));
        assert_eq!(size_of(&a), Some((360, 225)));
        a.set("size", Some("0x5".into()));
        assert_eq!(size_of(&a), None);
        a.set("size", Some("12".into()));
        assert_eq!(size_of(&a), None);
    }

    #[test]
    fn content_names_are_stable() {
        let a = content_name(Path::new("/t/plot-123-4.png"), b"abc");
        assert!(a.starts_with("plot-") && a.ends_with(".png") && a.len() == "plot-".len() + 12 + 4, "{a}");
        assert_eq!(a, content_name(Path::new("/elsewhere/plot-9.png"), b"abc"));
        assert_ne!(a, content_name(Path::new("/t/plot-123-4.png"), b"abd"));
        assert!(content_name(Path::new("/t/42.SVG"), b"x").starts_with("image-"));
        assert!(content_name(Path::new("/t/42.SVG"), b"x").ends_with(".svg"));
    }
}
