//! Pictures inside the notebook file (round 13, user decision: "keep everything in one file"). Saving no longer
//! writes a `<name>.files/` folder: each output block carries the files its image values name as `#~` lines, so a
//! notebook is one file that shows its plots, their hover read-outs and 3D rotation on any machine.
//!
//! ```text
//! #% output n=3
//! #= image("plot-3f2a91c04b7e.svg")
//! #~ @plot-3f2a91c04b7e.svg deflate
//! #~ eJzVvWuTHMeRJPj3+5...      (base64, 100 characters a line)
//! #~ @plot-3f2a91c04b7e.json deflate
//! #~ eJzNWE1v2zgQvedXCDqvC1Jf...
//! ```
//!
//! A `#~ @name [deflate]` line starts a file; the `#~` lines after it are its bytes in base64 (deflated first when
//! the header says so). Names are content hashes (`plot-<hash>.svg`), so a picture saved twice is the same text and
//! a diff shows only the pictures that changed. Its siblings with the same stem travel with it, deflated: the plot
//! description `.json` (hover read-outs) and the 3D scene `.scene3d` (rotation).
//!
//! **Which picture** (round nb-svg): a plot or graphic of this program (`plot-N.png` with its SVG sibling, or an
//! SVG written by `mc-graphics`, see [`is_ours`]) is embedded as its SVG, deflated, so the notebook draws it sharp
//! at every scale and in its theme. An SVG that deflates to more than [`SVG_LIMIT`] bytes (dense plots: some 10^5
//! points and more) is embedded as a PNG rendered from it at [`HIRES`] times its size, marked with a `pHYs` chunk
//! so readers draw it at its natural size ([`crate::images::picture_size`]). Other pictures (PNG files of other
//! programs, a plot whose SVG is gone) are embedded as they are. PDF siblings are never stored. Older notebooks
//! carrying PNGs read as before.
//!
//! **Shared copies** (round nb-svg): a picture several outputs show is embedded once, in the first block that
//! shows it; the later blocks only name it. [`extract`] reads the files of every block before it points the values
//! at them.
//!
//! [`extract`] writes the embedded files into a cache folder ([`cache_dir`]) when a notebook is opened and points
//! the values there; [`embed`] reads every picture a notebook's values name (a plot file of the session, a cached
//! file, or a picture of an older notebook's sidecar folder) back into its block when it is saved. The cache is only
//! a cache: no saved notebook names a file in it.
use crate::images::{content_name, image_path, image_value, resolve};
use crate::{Notebook, OutputBlock, OutputKind, OutputLine};
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

/// Base64 characters per `#~` line.
const LINE: usize = 100;
/// The siblings of a picture that are embedded with it (deflated).
pub const EMBEDDED_SIBLINGS: [&str; 2] = ["json", "scene3d"];
/// The extensions of files stored deflated: the siblings and SVG drawings.
const DEFLATED: [&str; 3] = ["json", "scene3d", "svg"];
/// The largest deflated SVG embedded as a drawing (bytes, before base64); a larger one is embedded as a [`HIRES`]
/// PNG.
pub const SVG_LIMIT: usize = 256 << 10;
/// The scale (pixels per point) of the PNG a dense plot's SVG is embedded as.
pub const HIRES: u32 = 2;
/// The class on the root element of every SVG `mc-graphics` writes: its chrome follows the notebook's theme.
pub const OUR_CLASS: &str = "mc-graphic";

/// A file with the bytes it is stored as (deflated or not, before base64).
type Stored = (Attachment, Vec<u8>);

/// One embedded file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    /// the file name (no folder)
    pub name: String,
    /// the file's bytes
    pub bytes: Vec<u8>,
}

/// What [`embed`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Embedded {
    /// pictures embedded (each block that shows one counts it)
    pub pictures: usize,
    /// bytes of the embedded files before encoding (each file once)
    pub bytes: usize,
    /// bytes of the embedded files as stored: deflated where they are, before base64
    pub stored: usize,
    /// value texts that changed: (before, after)
    pub renamed: Vec<(String, String)>,
    /// pictures whose file was not found and that the notebook did not carry already (left as they are)
    pub missing: Vec<String>,
}

/// The cache folder for the embedded files of opened notebooks: `mathcas-plots/embedded` in the temporary folder.
pub fn cache_dir() -> PathBuf {
    std::env::temp_dir().join("mathcas-plots").join("embedded")
}

/// Whether `name` is a plain file name an embedded file may have (no folders, no `..`, no hidden files).
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 120
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// The files a block carries, in order (malformed data is skipped).
pub fn attachments(block: &OutputBlock) -> Vec<Attachment> {
    let mut out = vec![];
    let mut current: Option<(String, bool, String)> = None;
    let finish = |cur: Option<(String, bool, String)>, out: &mut Vec<Attachment>| {
        let Some((name, deflated, data)) = cur else { return };
        let Some(raw) = base64_decode(&data) else { return };
        let bytes = if deflated {
            match miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&raw, 256 << 20) {
                Ok(b) => b,
                Err(_) => return,
            }
        } else {
            raw
        };
        out.push(Attachment { name, bytes });
    };
    for l in block.lines.iter().filter(|l| l.kind == OutputKind::Data) {
        if let Some(head) = l.text.strip_prefix('@') {
            finish(current.take(), &mut out);
            let mut words = head.split_whitespace();
            let name = words.next().unwrap_or("").to_string();
            let deflated = words.any(|w| w == "deflate");
            current = safe_name(&name).then_some((name, deflated, String::new()));
        } else if let Some((_, _, data)) = &mut current {
            data.push_str(l.text.trim());
        }
    }
    finish(current, &mut out);
    out
}

/// Whether a file of this name is stored deflated.
fn deflated(name: &str) -> bool {
    name.rsplit('.').next().is_some_and(|e| DEFLATED.contains(&e.to_ascii_lowercase().as_str()))
}

/// A file with the bytes it is stored as (deflated or not, before base64).
fn packed(a: Attachment) -> Stored {
    let p = if deflated(&a.name) { miniz_oxide::deflate::compress_to_vec_zlib(&a.bytes, 9) } else { a.bytes.clone() };
    (a, p)
}

/// The `#~` lines of a file whose stored bytes are `bytes` ([`packed`]).
fn data_lines(a: &Attachment, bytes: &[u8]) -> Vec<OutputLine> {
    let head = if deflated(&a.name) { format!("@{} deflate", a.name) } else { format!("@{}", a.name) };
    let text = base64_encode(bytes);
    let mut lines = vec![OutputLine { kind: OutputKind::Data, text: head }];
    let mut k = 0;
    while k < text.len() {
        let end = (k + LINE).min(text.len());
        lines.push(OutputLine { kind: OutputKind::Data, text: text[k..end].to_string() });
        k = end;
    }
    lines
}

/// Whether an SVG file was written by `mc-graphics` ([`OUR_CLASS`] on its root element).
pub fn is_ours(svg: &[u8]) -> bool {
    let text = String::from_utf8_lossy(&svg[..svg.len().min(1024)]);
    let Some(at) = text.find("<svg") else { return false };
    let tag = text[at..].split('>').next().unwrap_or("");
    tag.contains(&format!("class=\"{OUR_CLASS}\""))
}

/// The picture an output naming `source` embeds, as the module says: our SVG when it deflates small enough, else a
/// [`HIRES`] PNG made from it, else the file itself; with the bytes it is stored as.
fn choose(source: &Path) -> Option<Stored> {
    let ext = source.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let svg_path = if ext == "svg" { source.to_path_buf() } else { source.with_extension("svg") };
    let svg = if ext == "svg" || ext == "png" { std::fs::read(&svg_path).ok().filter(|b| is_ours(b)) } else { None };
    let Some(svg) = svg else {
        let bytes = std::fs::read(source).ok()?;
        return Some(packed(Attachment { name: content_name(source, &bytes), bytes }));
    };
    let stored = miniz_oxide::deflate::compress_to_vec_zlib(&svg, 9);
    if stored.len() <= SVG_LIMIT {
        return Some((Attachment { name: content_name(&svg_path, &svg), bytes: svg }, stored));
    }
    if let Some(png) = hires_png(&svg, HIRES) {
        return Some(packed(Attachment { name: content_name(&svg_path.with_extension("png"), &png), bytes: png }));
    }
    // a drawing the rasteriser refuses (too many pixels): the plot's own PNG, else the drawing after all
    if ext == "png" {
        if let Ok(bytes) = std::fs::read(source) {
            return Some(packed(Attachment { name: content_name(source, &bytes), bytes }));
        }
    }
    Some((Attachment { name: content_name(&svg_path, &svg), bytes: svg }, stored))
}

/// A picture and its embedded siblings read from disk under content-hash names, each with its stored bytes.
fn read_group(source: &Path) -> Option<Vec<Stored>> {
    let picture = choose(source)?;
    let name = picture.0.name.clone();
    let mut v = vec![picture];
    for ext in EMBEDDED_SIBLINGS {
        if let Ok(b) = std::fs::read(source.with_extension(ext)) {
            v.push(packed(Attachment { name: Path::new(&name).with_extension(ext).display().to_string(), bytes: b }));
        }
    }
    Some(v)
}

/// An SVG drawing rendered as a PNG at `scale` pixels per unit in the drawing's own (light) colours, with a `pHYs`
/// chunk of 72 x `scale` dots per inch ([`crate::images::with_density`]). `None` when it cannot be read or would
/// have more than 40 million pixels.
pub fn hires_png(svg: &[u8], scale: u32) -> Option<Vec<u8>> {
    use resvg::{tiny_skia, usvg};
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).ok()?;
    let s = scale as f32;
    let (w, h) = ((tree.size().width() * s).ceil() as u32, (tree.size().height() * s).ceil() as u32);
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 40_000_000 {
        return None;
    }
    let mut pm = tiny_skia::Pixmap::new(w, h)?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(s, s), &mut pm.as_mut());
    let png = pm.encode_png().ok()?;
    Some(crate::images::with_density(&png, scale))
}

/// Before a notebook is written: every image value of `nb` gets its picture (and the picture's siblings) embedded
/// under a content-hash name, and names that file; a file is stored once, in the first block that shows it.
/// Relative paths are looked up in `from` (the folder the notebook was in: pictures of an older notebook's sidecar
/// folder are embedded too, the folder is left alone). A value naming a file the notebook already carries keeps it.
/// Data lines no value names are dropped.
pub fn embed(nb: &mut Notebook, from: Option<&Path>) -> Embedded {
    let mut report = Embedded::default();
    // one read per source file, however many outputs name it
    let mut read: HashMap<PathBuf, Option<Vec<Stored>>> = HashMap::new();
    // what the file carries, from every block (a later block may name a file an earlier one carries)
    let carried: Vec<Attachment> = nb.cells.iter().flat_map(|c| c.outputs.iter()).flat_map(attachments).collect();
    // the files stored so far
    let mut stored: HashSet<String> = HashSet::new();
    for cell in &mut nb.cells {
        for block in &mut cell.outputs {
            block.lines.retain(|l| l.kind != OutputKind::Data);
            let mut files: Vec<Stored> = vec![];
            for line in &mut block.lines {
                if line.kind != OutputKind::Value {
                    continue;
                }
                let Some(path) = image_path(&line.text) else { continue };
                let source = resolve(&path, from);
                let found = read.entry(source.clone()).or_insert_with(|| read_group(&source)).clone();
                let group = match found {
                    Some(g) => g,
                    None => {
                        // not on disk: the file may carry it already (a notebook saved again without being opened)
                        let stem = Path::new(&path).file_stem().map(|s| s.to_os_string());
                        let mut had: Vec<Attachment> = vec![];
                        for a in &carried {
                            let same_stem = stem.as_deref().is_some_and(|s| Path::new(&a.name).file_stem() == Some(s));
                            if same_stem && !had.iter().any(|b| b.name == a.name) {
                                had.push(a.clone());
                            }
                        }
                        if !had.iter().any(|a| a.name == path) {
                            report.missing.push(path.clone());
                            continue;
                        }
                        // the picture first
                        had.sort_by_key(|a| a.name != path);
                        had.into_iter().map(packed).collect()
                    }
                };
                let after = image_value(&group[0].0.name);
                if after != line.text {
                    report.renamed.push((line.text.clone(), after.clone()));
                    line.text = after;
                }
                report.pictures += 1;
                for (a, p) in group {
                    if stored.insert(a.name.clone()) {
                        report.bytes += a.bytes.len();
                        report.stored += p.len();
                        files.push((a, p));
                    }
                }
            }
            for (a, p) in &files {
                block.lines.extend(data_lines(a, p));
            }
        }
    }
    report
}

/// After a notebook is read: write the files its blocks carry into `cache` (once; content-hash names) and point the
/// values that name them at the cached files (in any block: a picture shown twice is stored once), dropping the data
/// lines from the notebook in memory. Gives the number of files written.
pub fn extract(nb: &mut Notebook, cache: &Path) -> io::Result<usize> {
    let mut written = 0;
    let mut names: HashSet<String> = HashSet::new();
    for cell in &mut nb.cells {
        for block in &mut cell.outputs {
            if !block.lines.iter().any(|l| l.kind == OutputKind::Data) {
                continue;
            }
            let files = attachments(block);
            block.lines.retain(|l| l.kind != OutputKind::Data);
            for a in &files {
                names.insert(a.name.clone());
                let target = cache.join(&a.name);
                if std::fs::metadata(&target).map_or(true, |m| m.len() != a.bytes.len() as u64) {
                    std::fs::create_dir_all(cache)?;
                    // written under a temporary name first: another window may read the same name meanwhile
                    let tmp = cache.join(format!(".{}.{}", a.name, std::process::id()));
                    std::fs::write(&tmp, &a.bytes)?;
                    if std::fs::rename(&tmp, &target).is_err() {
                        let _ = std::fs::remove_file(&tmp);
                    }
                    written += 1;
                }
            }
        }
    }
    if names.is_empty() {
        return Ok(0);
    }
    for cell in &mut nb.cells {
        for block in &mut cell.outputs {
            for line in block.lines.iter_mut().filter(|l| l.kind == OutputKind::Value) {
                let Some(path) = image_path(&line.text) else { continue };
                if names.contains(&path) {
                    line.text = image_value(&cache.join(&path).display().to_string());
                }
            }
        }
    }
    Ok(written)
}

/// Delete cached files not used for `days` days (at start, so the cache does not grow without end). Gives the
/// number of files deleted.
pub fn sweep_cache(cache: &Path, days: u64) -> usize {
    let Ok(entries) = std::fs::read_dir(cache) else { return 0 };
    let limit = std::time::Duration::from_secs(days * 86_400);
    let mut removed = 0;
    for e in entries.flatten() {
        let old = e
            .metadata()
            .ok()
            .and_then(|m| m.accessed().or_else(|_| m.modified()).ok())
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > limit);
        if old && std::fs::remove_file(e.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding.
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(B64[((n >> (18 - 6 * k)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Standard base64 (padding optional, white space ignored); `None` for other characters.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b' ' | b'\t' | b'\r' | b'\n' => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips() {
        for n in 0..300usize {
            let bytes: Vec<u8> = (0..n).map(|k| (k * 37 % 256) as u8).collect();
            assert_eq!(base64_decode(&base64_encode(&bytes)).unwrap(), bytes);
        }
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert_eq!(base64_decode("TW E=\n").unwrap(), b"Ma");
        assert!(base64_decode("TW*E").is_none());
    }

    #[test]
    fn unsafe_names_are_refused() {
        for bad in ["", "../x.png", "a/b.png", ".hidden", "c:x.png", "a\\b.png"] {
            assert!(!safe_name(bad), "{bad}");
        }
        assert!(safe_name("plot-3f2a91c04b7e.png"));
    }
}
