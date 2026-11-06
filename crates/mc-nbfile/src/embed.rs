//! Pictures inside the notebook file (round 13, user decision: "keep everything in one file"). Saving no longer
//! writes a `<name>.files/` folder: each output block carries the files its image values name as `#~` lines, so a
//! notebook is one file that shows its plots, their hover read-outs and 3D rotation on any machine.
//!
//! ```text
//! #% output n=3
//! #= image("plot-3f2a91c04b7e.png")
//! #~ @plot-3f2a91c04b7e.png
//! #~ iVBORw0KGgoAAAANSUhEUgAAAoAAAAGQCAYAAAA...      (base64, 100 characters a line)
//! #~ @plot-3f2a91c04b7e.json deflate
//! #~ eJzNWE1v2zgQvedXCDqvC1Jf...
//! ```
//!
//! A `#~ @name [deflate]` line starts a file; the `#~` lines after it are its bytes in base64 (deflated first when
//! the header says so). Names are content hashes (`plot-<hash>.png`), so a picture saved twice is the same text and a
//! diff shows only the pictures that changed. The picture is the PNG only (already compressed); its siblings with the
//! same stem travel with it, deflated: the plot description `.json` (hover read-outs) and the 3D scene `.scene3d`
//! (rotation). SVG and PDF are not stored: they are made again from the scene or the term the PNG carries.
//!
//! [`extract`] writes the embedded files into a cache folder ([`cache_dir`]) when a notebook is opened and points
//! the values there; [`embed`] reads every picture a notebook's values name (a plot file of the session, a cached
//! file, or a picture of an older notebook's sidecar folder) back into its block when it is saved. The cache is only
//! a cache: no saved notebook names a file in it.
use crate::images::{content_name, image_path, image_value, resolve};
use crate::{Notebook, OutputBlock, OutputKind, OutputLine};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

/// Base64 characters per `#~` line.
const LINE: usize = 100;
/// The siblings of a picture that are embedded with it (deflated).
pub const EMBEDDED_SIBLINGS: [&str; 2] = ["json", "scene3d"];

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
    /// pictures embedded (each once per block that shows it)
    pub pictures: usize,
    /// bytes of the embedded files before encoding
    pub bytes: usize,
    /// value texts that changed: (before, after)
    pub renamed: Vec<(String, String)>,
    /// pictures whose file was not found and that the block did not carry already (left as they are)
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

/// The `#~` lines of a file.
fn data_lines(a: &Attachment) -> Vec<OutputLine> {
    let deflate = a.name.rsplit('.').next().is_some_and(|e| EMBEDDED_SIBLINGS.contains(&e));
    let (head, bytes) = if deflate {
        (format!("@{} deflate", a.name), miniz_oxide::deflate::compress_to_vec_zlib(&a.bytes, 9))
    } else {
        (format!("@{}", a.name), a.bytes.clone())
    };
    let text = base64_encode(&bytes);
    let mut lines = vec![OutputLine { kind: OutputKind::Data, text: head }];
    let mut k = 0;
    while k < text.len() {
        let end = (k + LINE).min(text.len());
        lines.push(OutputLine { kind: OutputKind::Data, text: text[k..end].to_string() });
        k = end;
    }
    lines
}

/// A picture file and its embedded siblings, read from disk under content-hash names.
fn read_group(source: &Path) -> Option<Vec<Attachment>> {
    let bytes = std::fs::read(source).ok()?;
    let name = content_name(source, &bytes);
    let mut v = vec![Attachment { name: name.clone(), bytes }];
    for ext in EMBEDDED_SIBLINGS {
        if let Ok(b) = std::fs::read(source.with_extension(ext)) {
            v.push(Attachment { name: Path::new(&name).with_extension(ext).display().to_string(), bytes: b });
        }
    }
    Some(v)
}

/// Before a notebook is written: every image value of `nb` gets its picture (and the picture's siblings) embedded
/// in its block under a content-hash name, and names that file. Relative paths are looked up in `from` (the folder
/// the notebook was in: pictures of an older notebook's sidecar folder are embedded too, the folder is left alone).
/// A value naming a file the block already carries keeps it. Data lines no value names are dropped.
pub fn embed(nb: &mut Notebook, from: Option<&Path>) -> Embedded {
    let mut report = Embedded::default();
    // one read per source file, however many outputs name it
    let mut read: HashMap<PathBuf, Option<Vec<Attachment>>> = HashMap::new();
    for cell in &mut nb.cells {
        for block in &mut cell.outputs {
            let carried = attachments(block);
            block.lines.retain(|l| l.kind != OutputKind::Data);
            let mut files: Vec<Attachment> = vec![];
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
                        // not on disk: the block may carry it already (a notebook saved again without being opened)
                        let stem = Path::new(&path).file_stem().map(|s| s.to_os_string());
                        let had: Vec<Attachment> = carried
                            .iter()
                            .filter(|a| stem.as_deref().is_some_and(|s| Path::new(&a.name).file_stem() == Some(s)))
                            .cloned()
                            .collect();
                        if !had.iter().any(|a| a.name == path) {
                            report.missing.push(path.clone());
                            continue;
                        }
                        // the picture first
                        let mut had = had;
                        had.sort_by_key(|a| a.name != path);
                        had
                    }
                };
                let after = image_value(&group[0].name);
                if after != line.text {
                    report.renamed.push((line.text.clone(), after.clone()));
                    line.text = after;
                }
                report.pictures += 1;
                for a in group {
                    if !files.iter().any(|f| f.name == a.name) {
                        report.bytes += a.bytes.len();
                        files.push(a);
                    }
                }
            }
            for a in &files {
                block.lines.extend(data_lines(a));
            }
        }
    }
    report
}

/// After a notebook is read: write the files its blocks carry into `cache` (once; content-hash names) and point the
/// values that name them at the cached files, dropping the data lines from the notebook in memory. Gives the number
/// of files written.
pub fn extract(nb: &mut Notebook, cache: &Path) -> io::Result<usize> {
    let mut written = 0;
    for cell in &mut nb.cells {
        for block in &mut cell.outputs {
            if !block.lines.iter().any(|l| l.kind == OutputKind::Data) {
                continue;
            }
            let files = attachments(block);
            block.lines.retain(|l| l.kind != OutputKind::Data);
            for a in &files {
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
            for line in block.lines.iter_mut().filter(|l| l.kind == OutputKind::Value) {
                let Some(path) = image_path(&line.text) else { continue };
                if files.iter().any(|a| a.name == path) {
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
