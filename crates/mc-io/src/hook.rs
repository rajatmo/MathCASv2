//! Exporters supplied by other crates: `export_data` hands a value whose head has a registered exporter for the
//! requested format to that exporter before trying its own cases, and a multi-page PDF takes such a value's PDF
//! pages. A graphics crate registers its terms here (for example `graphics` to PNG, SVG and PDF) without this
//! crate depending on it.

use mc_core::{Heap, Store, View, T};
use std::path::Path;
use std::sync::RwLock;

/// The options of an export that an exporter may honour.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ExportOptions {
    /// `dpi`: the resolution of raster output, in dots per inch (vector sizes are in points, 72 per inch)
    pub dpi: Option<f64>,
}

/// An exporter: write `term` to the file `path` in `format` (a lowercase format name such as `"png"` or
/// `"pdf"`), or say why it cannot. It runs on the evaluating thread and must not keep `term`.
pub type Exporter = for<'h> fn(h: &'h Heap, term: T<'h>, path: &Path, format: &str, options: &ExportOptions) -> Result<(), String>;

struct Entry {
    head: String,
    formats: Vec<String>,
    exporter: Exporter,
}

static EXPORTERS: RwLock<Vec<Entry>> = RwLock::new(Vec::new());

/// Register `exporter` for values `head(...)` written as any of `formats` (lowercase names, matched without
/// regard to case). A later registration for the same head replaces the earlier one. Registration is process-wide:
/// call it once at start-up (the engine's installation does).
pub fn register_exporter(head: &str, formats: &[&str], exporter: Exporter) {
    let mut v = EXPORTERS.write().unwrap_or_else(|e| e.into_inner());
    v.retain(|e| e.head != head);
    v.push(Entry {
        head: head.to_string(),
        formats: formats.iter().map(|f| f.to_ascii_lowercase()).collect(),
        exporter,
    });
}

/// The exporter registered for `t`'s head and `format`, if any.
pub fn exporter_for<'h>(h: &'h Heap, t: T<'h>, format: &str) -> Option<Exporter> {
    let View::App { head, .. } = h.view(t) else { return None };
    let View::Sym(s) = h.view(head) else { return None };
    let name = h.sym_name(s);
    let v = EXPORTERS.read().unwrap_or_else(|e| e.into_inner());
    v.iter()
        .find(|e| e.head == name.as_str() && e.formats.iter().any(|f| f.eq_ignore_ascii_case(format)))
        .map(|e| e.exporter)
}

/// An importer: the value a file in a format holds, read on `h` (it is evaluated like any reply), or `None` when
/// the file is not one this importer reads (the built-in reader of the format then decides).
pub type Importer = for<'h> fn(h: &'h Heap, path: &Path) -> Option<T<'h>>;

static IMPORTERS: RwLock<Vec<(String, Importer)>> = RwLock::new(Vec::new());

/// Register `importer` for files of `formats` (lowercase names). `import_data(file)` of an SVG file and
/// `import_data(file, "graphics")` ask the importers of the file's format before the built-in readers (a graphics
/// crate reads back the pictures it wrote). A later registration for a format replaces the earlier one.
pub fn register_importer(formats: &[&str], importer: Importer) {
    let mut v = IMPORTERS.write().unwrap_or_else(|e| e.into_inner());
    for f in formats {
        let f = f.to_ascii_lowercase();
        v.retain(|(g, _)| *g != f);
        v.push((f, importer));
    }
}

/// The importer registered for `format`, if any.
pub(crate) fn importer_for(format: &str) -> Option<Importer> {
    let v = IMPORTERS.read().unwrap_or_else(|e| e.into_inner());
    v.iter().find(|(f, _)| f.eq_ignore_ascii_case(format)).map(|(_, i)| *i)
}
