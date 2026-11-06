//! `import_data`: the text formats, PNG images and PDF documents with their elements.

use crate::images::{self, fnv, image_dir, image_term};
use crate::pdf_read::{self, OpenError};
use crate::pdf_write::PdfOut;
use crate::{bad_argument, dataset, failure, format_named, split_options, tabular, target, word, Format, Opts};
use hayro::hayro_syntax::Pdf;
use mc_api::names;
use mc_core::{Heap, Store, View, T};
use mc_eval::{Native, NativeCx, Reply, Res};
use std::path::{Path, PathBuf};

/// The resolution PDF pages are rendered at unless `dpi` is given.
pub const DEFAULT_DPI: f64 = 150.0;

const PNG_ELEMENTS: &[&str] = &["image", "data", "dimensions", "channels", "bit_depth"];
const PDF_ELEMENTS: &[&str] = &["pages", "page", "page_count", "text"];
const TEXT_ELEMENTS: &[&str] = &["dataset"];

/// `import_data(file)`, `import_data(file, format)`, `import_data(file, [format,] element [, k])`, options last.
pub struct Import;

impl Native for Import {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (args, o) = split_options(h, args);
        let Some((&file, rest)) = args.split_first() else { return Ok(Reply::Decline) };
        // an argument naming a format is the format; what follows is the element and its page number
        let (fmt, rest, at) = match rest.first() {
            Some(&m) if format_named(h, m).is_some() => (format_named(h, m), &rest[1..], 3),
            _ => (None, rest, 2),
        };
        if rest.len() > 2 {
            return Ok(Reply::Decline);
        }
        let (path, format) = match target(h, "import_data", file, fmt) {
            Ok(v) => v,
            Err(f) => return Ok(Reply::Done(f)),
        };
        let elem = match rest.first() {
            Some(&e) => match word(h, e) {
                Some(w) => Some((w, e)),
                None => return Ok(Reply::Done(bad_argument(h, "import_data", at))),
            },
            None => None,
        };
        let page = match rest.get(1).map(|&k| h.view(k)) {
            Some(View::Int(k)) => Some(k),
            Some(_) => return Ok(Reply::Done(bad_argument(h, "import_data", at + 1))),
            None => None,
        };
        // reading a file depends on the world: in program order, never memoised
        cx.effect()?;
        cx.volatile();
        let q = Query { file, path: &path, elem: elem.as_ref().map(|(w, e)| (w.as_str(), *e)), page, at };
        let v = match format {
            Format::Png => png(cx, &q, o)?,
            Format::Pdf => pdf(cx, &q, o)?,
            Format::Svg => failure(h, "io", &[q.file, h.string("reading svg files is not supported")]),
            _ => text(cx, &q, o, format)?,
        };
        Ok(Reply::Done(v))
    }
}

/// One import request.
struct Query<'q, 'h> {
    file: T<'h>,
    path: &'q str,
    /// the element's name and term
    elem: Option<(&'q str, T<'h>)>,
    /// the page number after the element
    page: Option<i64>,
    /// the position of the element argument
    at: i64,
}

fn unknown_element<'h>(h: &'h Heap, e: T<'h>, known: &[&str]) -> T<'h> {
    let names: Vec<T<'h>> = known.iter().map(|k| h.string(k)).collect();
    failure(h, "unknown_element", &[e, h.call(h.sym(names::LIST), &names)])
}

/// The text formats take no options; their one element is `"dataset"` (not for `lines`).
fn text<'h>(cx: &mut NativeCx<'_, '_, 'h>, q: &Query<'_, 'h>, o: Option<T<'h>>, format: Format) -> Res<T<'h>> {
    let h = cx.heap();
    let as_dataset = match q.elem {
        Some(("dataset", _)) if format != Format::Lines && q.page.is_none() => true,
        Some((_, e)) if format == Format::Lines => return Ok(unknown_element(h, e, &[])),
        Some((_, e)) => return Ok(unknown_element(h, e, TEXT_ELEMENTS)),
        None => false,
    };
    if let Err(f) = Opts::parse(h, o, &[]) {
        return Ok(f);
    }
    let text = match std::fs::read_to_string(q.path) {
        Ok(t) => t,
        Err(e) => return Ok(failure(h, "io", &[q.file, h.string(&e.to_string())])),
    };
    cx.tick(1 + text.len() as u64 / 4096)?;
    if as_dataset {
        return match format {
            Format::Json => dataset::import_json(cx, q.file, &text),
            _ => dataset::import_table(cx, q.file, &text, format),
        };
    }
    Ok(match format {
        Format::Csv => tabular::rows(h, &text, |l| tabular::split_csv(l, ',')),
        // TSV has no quoting: fields are the text between tabs
        Format::Tsv => tabular::rows(h, &text, |l| l.split('\t').map(str::to_string).collect()),
        Format::Table => tabular::rows(h, &text, |l| l.split_whitespace().map(str::to_string).collect()),
        Format::Lines => {
            let ls: Vec<T<'h>> = text.lines().map(|l| h.string(l)).collect();
            h.call(h.sym(names::LIST), &ls)
        }
        Format::Json => match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(j) => tabular::from_json(h, &j),
            Err(e) => failure(h, "io", &[q.file, h.string(&e.to_string())]),
        },
        Format::Png | Format::Pdf | Format::Svg => unreachable!("read by png, pdf and the svg refusal"),
    })
}

fn png<'h>(cx: &mut NativeCx<'_, '_, 'h>, q: &Query<'_, 'h>, o: Option<T<'h>>) -> Res<T<'h>> {
    let h = cx.heap();
    if let Err(f) = Opts::parse(h, o, &[]) {
        return Ok(f);
    }
    if q.page.is_some() {
        return Ok(bad_argument(h, "import_data", q.at + 1));
    }
    let (name, e) = q.elem.unwrap_or(("image", q.file));
    if name == "data" {
        return Ok(match images::load(h, q.file, q.path) {
            Ok(r) => images::to_array(cx, &r)?,
            Err(f) => f,
        });
    }
    let info = match images::header(h, q.file, q.path) {
        Ok(i) => i,
        Err(f) => return Ok(f),
    };
    Ok(match name {
        "image" => {
            let abs = std::fs::canonicalize(q.path).unwrap_or_else(|_| PathBuf::from(q.path));
            image_term(h, &abs)
        }
        "dimensions" => h.call(h.sym(names::LIST), &[h.int(info.width as i64), h.int(info.height as i64)]),
        "channels" => h.int(info.channels as i64),
        "bit_depth" => h.int(info.depth as i64),
        _ => unknown_element(h, e, PNG_ELEMENTS),
    })
}

/// Open the PDF file of `q`, or the failure for it.
pub(crate) fn open_pdf<'h>(h: &'h Heap, file: T<'h>, path: &str) -> Result<(Pdf, u64), T<'h>> {
    let bytes = images::read_file(h, file, path)?;
    let hash = fnv(&bytes, 0);
    match pdf_read::open(bytes) {
        Ok(p) => Ok((p, hash)),
        Err(OpenError::Encrypted) => Err(failure(h, "encrypted", &[file])),
        Err(OpenError::Damaged) => Err(failure(h, "damaged", &[file, h.string("not a readable PDF file")])),
    }
}

fn pdf<'h>(cx: &mut NativeCx<'_, '_, 'h>, q: &Query<'_, 'h>, o: Option<T<'h>>) -> Res<T<'h>> {
    let h = cx.heap();
    let opts = match Opts::parse(h, o, &["dpi"]) {
        Ok(o) => o,
        Err(f) => return Ok(f),
    };
    let dpi = match opts.positive(h, "dpi") {
        Ok(d) => d.unwrap_or(DEFAULT_DPI),
        Err(f) => return Ok(f),
    };
    let (name, e) = q.elem.unwrap_or(("pages", q.file));
    if !PDF_ELEMENTS.contains(&name) {
        return Ok(unknown_element(h, e, PDF_ELEMENTS));
    }
    let takes_page = matches!(name, "page" | "text");
    if q.page.is_some() && !takes_page {
        return Ok(bad_argument(h, "import_data", q.at + 1));
    }
    if name == "page" && q.page.is_none() {
        return Ok(bad_argument(h, "import_data", q.at + 1));
    }
    let (doc, hash) = match open_pdf(h, q.file, q.path) {
        Ok(d) => d,
        Err(f) => return Ok(f),
    };
    let pages = doc.pages();
    let n = pages.len();
    // the 0-based pages asked for
    let chosen: Vec<usize> = match q.page {
        Some(k) if k >= 1 && k as usize <= n => vec![k as usize - 1],
        Some(k) if k < 0 && k.unsigned_abs() as usize <= n => vec![n - k.unsigned_abs() as usize],
        Some(k) => {
            return Ok(failure(h, "out_of_range", &[h.int(k), h.call(h.sym(names::LIST), &[h.int(1), h.int(n as i64)])]))
        }
        None => (0..n).collect(),
    };
    if name == "page_count" {
        return Ok(h.int(n as i64));
    }
    let mut out = Vec::with_capacity(chosen.len());
    for i in chosen {
        cx.tick(1)?;
        let v = if name == "text" {
            match pdf_read::text(&pages[i]) {
                Some(s) => h.string(&s),
                None => failure(h, "damaged", &[q.file, h.string(&format!("the text of page {} could not be read", i + 1))]),
            }
        } else {
            match page_image(h, &doc, hash, i, dpi) {
                Ok(t) => t,
                Err(m) => failure(h, "damaged", &[q.file, h.string(&format!("page {}: {m}", i + 1))]),
            }
        };
        if q.page.is_some() {
            return Ok(v);
        }
        out.push(v);
    }
    Ok(h.call(h.sym(names::LIST), &out))
}

/// Page `i` of `doc` as an image value: rendered at `dpi` into a PNG file named after the document's content, the
/// page and the resolution (so importing the same page again reuses it), with the page alone as a PDF file next to
/// it (its vector twin, which `export_data` uses for PDF output).
fn page_image<'h>(h: &'h Heap, doc: &Pdf, hash: u64, i: usize, dpi: f64) -> Result<T<'h>, String> {
    let dir = image_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let png = dir.join(format!("pdf-{hash:016x}-p{}-{dpi}dpi.png", i + 1));
    let twin = png.with_extension("pdf");
    if !(png.is_file() && twin.is_file()) {
        let page = &doc.pages()[i];
        let r = pdf_read::render(page, dpi)?;
        write_twin(page, &twin);
        std::fs::write(&png, r.to_png()?).map_err(|e| e.to_string())?;
    }
    Ok(image_term(h, &png))
}

/// Write `page` alone as a PDF file at `path`; a page that cannot be copied gets no twin (its image then exports
/// as a raster page).
fn write_twin(page: &hayro::hayro_syntax::page::Page<'_>, path: &Path) {
    let mut out = PdfOut::new();
    if let Some(Ok(())) = pdf_read::guarded(|| out.import_page(page, &|| false)) {
        let _ = std::fs::write(path, out.finish());
    }
}
