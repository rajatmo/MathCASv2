//! Import and export of data files: lists as CSV, TSV, whitespace-separated text and JSON; PNG images; PDF
//! documents. Images are values `image("path")` naming a PNG file (decision IO.1), and come with `image`,
//! `image_data`, `image_dimensions` and `image_channels`.
//!
//! The format comes from the file extension, or from a format name after the file
//! (`import_data("data.out", csv)`). Paths are relative to the current directory. After the format, `import_data`
//! takes an element (a string or a symbol) and, for some elements, a page number; options come last
//! (`import_data("a.pdf", "page", 2, dpi: 300)`).
//!
//! | format | `import_data` gives | elements | `export_data` takes |
//! |---|---|---|---|
//! | `csv`, `tsv` | a list of rows, each a list of fields | `"dataset"` | a list of rows (lists), or of values (one per line); a dataset (header line, then rows) |
//! | `txt`, `dat` (`table`) | a list of lines, each a list of whitespace-separated fields | `"dataset"` | as csv, fields separated by spaces |
//! | `lines` | a list of strings, one per line | | a list of values, one per line |
//! | `json` | lists, numbers, strings, `true`/`false`, `null`; objects as lists of rules `key -> value` | `"dataset"` | the same; a dataset as an array of objects |
//! | `png` | `image(path)` (the file checked) | `"image"`, `"data"` (as `image_data`), `"dimensions"` (`[width, height]`), `"channels"`, `"bit_depth"` | an image value (plots too), or an array height x width (x channels) of reals in [0, 1]; options `dpi`, `scale`, `bit_depth` |
//! | `pdf` | the pages as images (`"pages"`) | `"pages"`, `"page", k`, `"page_count"`, `"text"` (a string per page), `"text", k`; option `dpi` (default 150) | an image, plot or array (one page), or a list of images and plots (a page each, or every page of a plot or imported page); option `dpi` for raster pages |
//!
//! **PNG.** All PNG layouts are read: gray, gray with alpha, RGB, RGBA, palette (expanded to RGB or RGBA), 1 to 16
//! bits; `image_data` gives height x width x channels reals in [0, 1] (8-bit samples over 255, 16-bit over 65535),
//! packed. Written files are 8-bit unless `bit_depth: 16`; `scale: s` resamples, `dpi: d` records the resolution
//! (and re-renders a plot from its vector form at that resolution).
//!
//! **PDF.** Pages are rendered by `hayro` (pure Rust) on white at `dpi` (default 150): each page becomes a PNG in the
//! system's temporary folder with, next to it, the page alone as a PDF file, so exporting imported pages (or plots,
//! which have the same vector twin) to PDF keeps them vector. Page numbers count from 1, negative ones from the end.
//! Encrypted files give `failure(encrypted, file)`, unreadable ones `failure(damaged, file, message)`. Exported
//! raster pages embed the pixels losslessly (Flate with PNG predictors, alpha as a soft mask), one pixel per point
//! unless `dpi` (or the PNG's own resolution) says otherwise.
//!
//! **Other packages' values.** [`register_exporter`] lets a crate supply `export_data` for its own terms (graphics
//! to PNG, SVG, PDF): a registered exporter is tried before this crate's cases, and its PDF pages join multi-page
//! documents.
//!
//! **Datasets** (`mc-assoc`): `import_data(file, "dataset")` reads CSV, TSV, a whitespace table or JSON as a
//! dataset: the first record names the columns, each column's type (integers, reals, `true`/`false` or strings) is
//! inferred from all its fields, and empty fields (JSON `null`) are `missing()`; numeric columns without missing
//! values are packed arrays. `export_data` writes a dataset to CSV, TSV or a table (a header line, missing values
//! empty) or to JSON (an array of objects, missing values `null`).
//!
//! Fields that read as integers become integers, then machine reals, else strings (quotes in CSV removed).
//! Exported values: integers and machine reals as numbers (reals keep a decimal point or exponent, so they read
//! back as reals), strings as text, anything else as its printed form. Both functions are effects (they run in
//! program order), and imported values are never memoised.
#![warn(missing_docs)]

mod dataset;
mod export;
mod files;
mod hook;
mod images;
mod import;
mod interp;
mod pdf_read;
mod pdf_write;
mod raster;
mod tabular;

pub use hook::{register_exporter, ExportOptions, Exporter};

use mc_api::names;
use mc_core::{Heap, Store, View, T};
use mc_eval::{Attributes, Kernel};

/// Every function of this package with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("temporary_directory", "temporary_directory(): the folder for temporary files, as a string ending in /"),
    (
        "import_data",
        "import_data(file), import_data(file, format), import_data(file, element), import_data(file, \"page\", k): the data of a file: csv, tsv, txt/dat (lists of rows; element dataset), lines, json (element dataset), png (image(path); elements data, dimensions, channels, bit_depth), pdf (the pages as images; elements pages, page k, page_count, text; option dpi); the format from the extension unless given",
    ),
    (
        "interpolation",
        "interpolation(data): a function through data [y1, ...] (at x = 1, 2, ...) or [[x1, y1], ...], a not-a-knot cubic spline (method: linear for straight segments); call it as f(x); outside the data range failure(out_of_range, x, [x_min, x_max])",
    ),
    (
        "export_data",
        "export_data(file, data), export_data(file, data, format): write data (lists of rows or values, datasets; json; png from an image, plot or array of reals with options dpi, scale, bit_depth; pdf from an image, plot or array, or a list of them as pages); gives the file name",
    ),
    (
        "image",
        "image(array): the image of an array height x width (gray) or height x width x channels (1 gray, 2 gray and alpha, 3 RGB, 4 RGBA) of reals in [0, 1], saved as a PNG file (bit_depth: 16 for 16 bits); gives image(path); image(\"path\") is an image value",
    ),
    (
        "image_data",
        "image_data(img): the pixels of an image as a packed array height x width x channels of reals in [0, 1]",
    ),
    ("file_exists", "file_exists(path): true when the file or directory exists"),
    ("file_names", "file_names(pattern), file_names(pattern, dir): the sorted names of the files matching a pattern with * and ?"),
    ("image_dimensions", "image_dimensions(img): the size of an image in pixels, [width, height]"),
    (
        "image_channels",
        "image_channels(img): the number of channels of an image: 1 gray, 2 gray and alpha, 3 RGB, 4 RGBA",
    ),
];

/// Register the natives with `k`.
pub fn install(k: &mut Kernel, h: &Heap) {
    k.register(h.sym("import_data"), Attributes::default(), Box::new(import::Import));
    k.register(h.sym("export_data"), Attributes::default(), Box::new(export::Export));
    k.register(h.sym("interpolation"), Attributes::default(), Box::new(interp::Interpolation));
    k.register(h.sym("interpolated"), Attributes::default(), Box::new(interp::Interpolated));
    k.register(h.sym(images::IMAGE), Attributes::default(), Box::new(images::ImageOf));
    k.register(h.sym("image_data"), Attributes::default(), Box::new(images::ImageQuery(images::Query::Data)));
    k.register(
        h.sym("image_dimensions"),
        Attributes::default(),
        Box::new(images::ImageQuery(images::Query::Dimensions)),
    );
    k.register(h.sym("image_channels"), Attributes::default(), Box::new(images::ImageQuery(images::Query::Channels)));
    k.register(h.sym("temporary_directory"), Attributes::default(), Box::new(TemporaryDirectory));
    mc_numeric::formula::install(k, h, files::DEFS);
}

/// `temporary_directory()`: the system's folder for temporary files, with forward slashes and a final `/`.
struct TemporaryDirectory;

impl mc_eval::Native for TemporaryDirectory {
    fn call<'h>(&self, cx: &mut mc_eval::NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> mc_eval::Res<mc_eval::Reply<'h>> {
        if !args.is_empty() {
            return Ok(mc_eval::Reply::Decline);
        }
        let mut d = std::env::temp_dir().to_string_lossy().replace('\\', "/");
        if !d.ends_with('/') {
            d.push('/');
        }
        Ok(mc_eval::Reply::Done(cx.heap().string(&d)))
    }
}

pub(crate) fn failure<'h>(h: &'h Heap, tag: &str, details: &[T<'h>]) -> T<'h> {
    let mut a = vec![h.sym_term(h.sym(tag))];
    a.extend_from_slice(details);
    h.call(h.sym(names::FAILURE), &a)
}

/// `failure(argument, f, k)`.
pub(crate) fn bad_argument<'h>(h: &'h Heap, f: &str, k: i64) -> T<'h> {
    failure(h, "argument", &[h.sym_term(h.sym(f)), h.int(k)])
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Format {
    Csv,
    Tsv,
    Table,
    Lines,
    Json,
    Png,
    Pdf,
    Svg,
    /// animated pictures: written by a registered exporter (a list of graphics)
    Gif,
}

impl Format {
    pub(crate) fn named(s: &str) -> Option<Format> {
        Some(match s.to_ascii_lowercase().as_str() {
            "csv" => Format::Csv,
            "tsv" => Format::Tsv,
            "txt" | "dat" | "table" => Format::Table,
            "lines" => Format::Lines,
            "json" => Format::Json,
            "png" => Format::Png,
            "pdf" => Format::Pdf,
            "svg" => Format::Svg,
            "gif" => Format::Gif,
            _ => return None,
        })
    }

    /// The lowercase name exporters are asked for.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Format::Csv => "csv",
            Format::Tsv => "tsv",
            Format::Table => "table",
            Format::Lines => "lines",
            Format::Json => "json",
            Format::Png => "png",
            Format::Pdf => "pdf",
            Format::Svg => "svg",
            Format::Gif => "gif",
        }
    }
}

/// A string or a symbol's name.
pub(crate) fn word<'h>(h: &'h Heap, t: T<'h>) -> Option<String> {
    match h.view(t) {
        View::Sym(s) => Some(h.sym_name(s).to_string()),
        View::Str(s) => Some(s.to_string()),
        _ => None,
    }
}

/// The format `t` names, if it names one.
pub(crate) fn format_named<'h>(h: &'h Heap, t: T<'h>) -> Option<Format> {
    word(h, t).and_then(|w| Format::named(&w))
}

/// The path `file` names and the format: `fmt` when given, else the file's extension; failures for a file that is
/// not a string (`failure(argument, f, 1)`) and an unknown format (`failure(unknown_format, file)`).
pub(crate) fn target<'h>(h: &'h Heap, f: &str, file: T<'h>, fmt: Option<Format>) -> Result<(String, Format), T<'h>> {
    let View::Str(path) = h.view(file) else { return Err(bad_argument(h, f, 1)) };
    let format = fmt.or_else(|| std::path::Path::new(path).extension().and_then(|e| e.to_str()).and_then(Format::named));
    match format {
        Some(fm) => Ok((path.to_string(), fm)),
        None => Err(failure(h, "unknown_format", &[file])),
    }
}

/// The trailing `options(k -> v, ...)` of `args`, split off.
pub(crate) fn split_options<'a, 'h>(h: &'h Heap, args: &'a [T<'h>]) -> (&'a [T<'h>], Option<T<'h>>) {
    match args.split_last() {
        Some((&last, rest)) => match h.view(last) {
            View::App { head, .. } if head == h.sym_term(h.sym("options")) => (rest, Some(last)),
            _ => (args, None),
        },
        None => (args, None),
    }
}

/// Named options, checked against the names a call accepts.
pub(crate) struct Opts<'h> {
    pairs: Vec<(String, T<'h>)>,
}

impl<'h> Opts<'h> {
    /// The rules of `options(...)` (none for `None`): `failure(unknown_option, name)` for a name not in
    /// `allowed`, `failure(argument, options, k)` for an entry that is not a rule with a symbol on the left.
    pub(crate) fn parse(h: &'h Heap, o: Option<T<'h>>, allowed: &[&str]) -> Result<Opts<'h>, T<'h>> {
        let mut pairs = Vec::new();
        let Some(o) = o else { return Ok(Opts { pairs }) };
        let View::App { args, .. } = h.view(o) else { return Ok(Opts { pairs }) };
        for (i, &r) in args.iter().enumerate() {
            let entry = match h.view(r) {
                View::App { head, args: &[k, v] } if head == h.sym_term(h.sym(names::RULE)) => match h.view(k) {
                    View::Sym(s) => Some((h.sym_name(s).to_string(), k, v)),
                    _ => None,
                },
                _ => None,
            };
            let Some((name, k, v)) = entry else {
                return Err(bad_argument(h, "options", i as i64 + 1));
            };
            if !allowed.contains(&name.as_str()) {
                return Err(failure(h, "unknown_option", &[k]));
            }
            pairs.push((name, v));
        }
        Ok(Opts { pairs })
    }

    /// Were no options given?
    pub(crate) fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    fn get(&self, k: &str) -> Option<T<'h>> {
        self.pairs.iter().rev().find(|(n, _)| n == k).map(|&(_, v)| v)
    }

    fn bad(h: &'h Heap, k: &str, v: T<'h>) -> T<'h> {
        failure(h, "bad_option", &[h.sym_term(h.sym(k)), v])
    }

    /// Option `k` as a positive finite real (`failure(bad_option, k, value)` otherwise).
    pub(crate) fn positive(&self, h: &'h Heap, k: &str) -> Result<Option<f64>, T<'h>> {
        let Some(v) = self.get(k) else { return Ok(None) };
        let x = match h.view(v) {
            View::Int(i) => i as f64,
            View::Real(x) => x,
            _ => f64::NAN,
        };
        if x.is_finite() && x > 0.0 {
            Ok(Some(x))
        } else {
            Err(Self::bad(h, k, v))
        }
    }

    /// Option `bit_depth`: 8 or 16.
    pub(crate) fn bit_depth(&self, h: &'h Heap) -> Result<Option<u8>, T<'h>> {
        let Some(v) = self.get("bit_depth") else { return Ok(None) };
        match h.view(v) {
            View::Int(8) => Ok(Some(8)),
            View::Int(16) => Ok(Some(16)),
            _ => Err(Self::bad(h, "bit_depth", v)),
        }
    }
}
