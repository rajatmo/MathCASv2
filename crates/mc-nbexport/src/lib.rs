//! Exporting a saved `.mcn` notebook (design `08-notebook.md`, section 7) as a document to read, print or send:
//! [`to_html`] writes one self-contained HTML file, [`to_pdf`] a paginated PDF document, [`export`] picks one by
//! the output file's extension. Only the file as saved is converted; nothing is evaluated.
//!
//! Both formats show what the notebook shows: headings, text and items in the prose font, code cells in the code
//! font with syntax colouring, typeset inputs and output values with their square roots, fractions and powers
//! (`mc_boxes::from_text` and `mc_boxes::math_text`, the notebook's renderer rules), values right-justified and
//! `print` output, failures and messages left, pictures at their `size=` (or natural) size.
//!
//! ```no_run
//! mc_nbexport::export("notes.mcn".as_ref(), "notes.pdf".as_ref()).unwrap();
//! ```
mod gzip;
mod html;
mod model;
mod pdf;

use mc_nbfile::Notebook;
use std::io;
use std::path::Path;

/// Where the HTML export puts pictures.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Images {
    /// inside the HTML file as `data:` URIs (one file to send; the default)
    #[default]
    Embed,
    /// copied into a folder `<out name>.files/` next to the HTML file and linked relatively
    Folder,
}

/// Options of the HTML export.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// where pictures go
    pub images: Images,
    /// the document title (default: the first title cell, else the output file's name)
    pub title: Option<String>,
}

/// The notebook as one HTML document: inline CSS, no scripts, no external links. `base` is the notebook's folder
/// (relative picture paths are resolved in it); `out` is the file the HTML will be written to (its name gives the
/// picture folder of [`Images::Folder`], which this call creates and fills, and the default title).
pub fn to_html(nb: &Notebook, base: &Path, out: &Path, opts: &Options) -> io::Result<String> {
    html::document(nb, base, out, opts)
}

/// The notebook as a PDF document (A4 pages, page numbers, searchable text). `base` is the notebook's folder
/// (relative picture paths are resolved in it). A picture that cannot be read is drawn as a frame with its path.
pub fn to_pdf(nb: &Notebook, base: &Path) -> io::Result<Vec<u8>> {
    pdf::document(nb, base)
}

/// Export the notebook file `input_mcn` to `output`: HTML (pictures embedded) for `.html` and `.htm`, PDF for
/// `.pdf`. The error says what went wrong (an unreadable or malformed notebook, an unknown extension, a write
/// error).
pub fn export(input_mcn: &Path, output: &Path) -> Result<(), String> {
    let ext = output.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if !matches!(ext.as_str(), "html" | "htm" | "pdf") {
        return Err(format!("{}: unknown export format (use .html, .htm or .pdf)", output.display()));
    }
    let text = std::fs::read_to_string(input_mcn).map_err(|e| format!("{}: {e}", input_mcn.display()))?;
    let mut nb = Notebook::parse(&text).map_err(|e| format!("{}: {e}", input_mcn.display()))?;
    // pictures embedded in the notebook are read from the cache they are written to
    let cache = mc_nbfile::embed::cache_dir();
    mc_nbfile::embed::extract(&mut nb, &cache).map_err(|e| format!("{}: {e}", cache.display()))?;
    let base = input_mcn.parent().unwrap_or(Path::new(""));
    let bytes = if ext == "pdf" {
        to_pdf(&nb, base).map_err(|e| format!("{}: {e}", output.display()))?
    } else {
        to_html(&nb, base, output, &Options::default()).map_err(|e| format!("{}: {e}", output.display()))?.into_bytes()
    };
    std::fs::write(output, bytes).map_err(|e| format!("{}: {e}", output.display()))
}

/// Standard base64 with padding.
pub(crate) fn base64(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(A[(n >> (18 - 6 * k)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
