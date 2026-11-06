//! `export_data`: registered exporters first, then PNG, PDF and the text formats.

use crate::hook::{exporter_for, ExportOptions};
use crate::images::{self, image_dir, image_path, raster_of};
use crate::import::open_pdf;
use crate::pdf_read;
use crate::pdf_write::PdfOut;
use crate::raster::{Raster, MAX_SAMPLES};
use crate::{bad_argument, dataset, failure, format_named, split_options, tabular, target, Format, Opts};
use hayro::hayro_syntax::Pdf;
use mc_core::{Heap, Store, T};
use mc_eval::{Native, NativeCx, Reply, Res};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// `export_data(file, data)`, `export_data(file, data, format)`, options last.
pub struct Export;

/// An export's outcome: done, or the failure to give.
type Outcome<'h> = Result<(), T<'h>>;

impl Native for Export {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (args, o) = split_options(h, args);
        let (file, data, fmt) = match args {
            [f, d] => (*f, *d, None),
            [f, d, m] => match format_named(h, *m) {
                Some(fm) => (*f, *d, Some(fm)),
                None => return Ok(Reply::Done(failure(h, "unknown_format", &[*m]))),
            },
            _ => return Ok(Reply::Decline),
        };
        let (path, format) = match target(h, "export_data", file, fmt) {
            Ok(v) => v,
            Err(f) => return Ok(Reply::Done(f)),
        };
        let allowed: &[&str] = match format {
            Format::Png => &["dpi", "scale", "bit_depth"],
            Format::Pdf => &["dpi"],
            _ => &[],
        };
        let opts = match Opts::parse(h, o, allowed) {
            Ok(o) => o,
            Err(f) => return Ok(Reply::Done(f)),
        };
        let dpi = match opts.positive(h, "dpi") {
            Ok(d) => d,
            Err(f) => return Ok(Reply::Done(f)),
        };
        cx.effect()?;
        let io = |e: String| failure(h, "io", &[file, h.string(&e)]);
        let done: Outcome<'h> = if let Some(ex) = exporter_for(h, data, format.name()) {
            ex(h, data, Path::new(&path), format.name(), &ExportOptions { dpi }).map_err(io)
        } else {
            match format {
                Format::Png => png(cx, file, data, &path, &opts, dpi)?,
                Format::Pdf => pdf(cx, file, data, &path, dpi)?,
                // a plot's SVG sibling (graphics terms reach their exporter above)
                Format::Svg => match image_path(h, data).map(|p| Path::new(p).with_extension("svg")).filter(|p| p.is_file()) {
                    Some(svg) => std::fs::copy(&svg, &path).map(|_| ()).map_err(|e| e.to_string()).map_err(io),
                    None => Err(failure(h, "unsupported", &[file, h.string("svg export takes a plot or a graphics term")])),
                },
                Format::Gif => Err(failure(h, "unsupported", &[file, h.string("gif export takes a graphic or a list of graphics")])),
                Format::Json if dataset::is_dataset(h, data) => match dataset::json(cx, data)? {
                    Some(t) => write(&path, t).map_err(io),
                    None => Err(failure(h, "argument", &[h.sym_term(h.sym("dataset")), h.int(1)])),
                },
                Format::Json => {
                    let j = tabular::to_json(cx, data)?;
                    serde_json::to_string_pretty(&j).map_err(|e| e.to_string()).and_then(|s| write(&path, s)).map_err(io)
                }
                Format::Csv | Format::Tsv | Format::Table | Format::Lines => {
                    let sep = match format {
                        Format::Csv => ",",
                        Format::Tsv => "\t",
                        _ => " ",
                    };
                    let table = if dataset::is_dataset(h, data) {
                        dataset::delimited(cx, data, sep, format == Format::Csv)?
                    } else {
                        tabular::delimited(cx, data, sep, format == Format::Csv)?
                    };
                    match table {
                        Some(t) => write(&path, t).map_err(io),
                        None => Err(bad_argument(h, "export_data", 2)),
                    }
                }
            }
        };
        Ok(Reply::Done(match done {
            Ok(()) => file,
            Err(f) => f,
        }))
    }
}

fn write(path: &str, s: impl AsRef<[u8]>) -> Result<(), String> {
    std::fs::write(path, s).map_err(|e| e.to_string())
}

/// The vector twin of an image file: the PDF next to it with the same name (plots and imported PDF pages have one).
fn twin(png: &str) -> Option<std::path::PathBuf> {
    Some(Path::new(png).with_extension("pdf")).filter(|p| p.is_file())
}

/// PNG: an image value (copied as it is without options; re-rendered from its vector twin when `dpi` is given),
/// or an array of reals.
fn png<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    file: T<'h>,
    data: T<'h>,
    path: &str,
    opts: &Opts<'h>,
    dpi: Option<f64>,
) -> Res<Outcome<'h>> {
    let h = cx.heap();
    let io = |e: String| failure(h, "io", &[file, h.string(&e)]);
    let (scale, depth) = match (opts.positive(h, "scale"), opts.bit_depth(h)) {
        (Ok(s), Ok(d)) => (s, d),
        (Err(f), _) | (_, Err(f)) => return Ok(Err(f)),
    };
    let mut r = if let Some(src) = image_path(h, data) {
        let src_file = h.string(src);
        match (dpi, twin(src)) {
            (Some(d), Some(pdf)) => match vector_raster(h, src_file, &pdf, d) {
                Ok(r) => r,
                Err(f) => return Ok(Err(f)),
            },
            _ if opts.is_empty() => return Ok(std::fs::copy(src, path).map(|_| ()).map_err(|e| io(e.to_string()))),
            _ => match images::load(h, src_file, src) {
                Ok(r) => r,
                Err(f) => return Ok(Err(f)),
            },
        }
    } else {
        match raster_of(cx, data, depth.unwrap_or(8))? {
            Some(r) => r,
            None => return Ok(Err(bad_argument(h, "export_data", 2))),
        }
    };
    if let Some(s) = scale {
        let samples = (r.width as f64 * s).round() * (r.height as f64 * s).round() * r.channels as f64;
        if samples > MAX_SAMPLES as f64 {
            return Ok(Err(failure(h, "too_large", &[file, h.string("the scaled image is too large")])));
        }
        match r.resized(s, &|| cx.stopped()) {
            Some(x) => r = x,
            None => {
                cx.tick(1)?;
                return Ok(Err(failure(h, "stopped", &[])));
            }
        }
    }
    if let Some(d) = depth {
        r = r.with_depth(d);
    }
    if dpi.is_some() {
        r.dpi = dpi;
    }
    Ok(r.to_png().and_then(|b| write(path, b)).map_err(io))
}

/// The first page of the PDF file `pdf` rendered at `dpi`.
fn vector_raster<'h>(h: &'h Heap, file: T<'h>, pdf: &Path, dpi: f64) -> Result<Raster, T<'h>> {
    let (doc, _) = open_pdf(h, file, &pdf.to_string_lossy())?;
    let Some(page) = doc.pages().first() else {
        return Err(failure(h, "damaged", &[file, h.string("its vector form has no page")]));
    };
    pdf_read::render(page, dpi).map_err(|m| failure(h, "damaged", &[file, h.string(&m)]))
}

/// PDF: a plot or an imported page (their vector twin, copied), an image or an array (one raster page), or a list
/// of images, plots and values with a registered PDF exporter (a page each, every page of a vector twin).
fn pdf<'h>(cx: &mut NativeCx<'_, '_, 'h>, file: T<'h>, data: T<'h>, path: &str, dpi: Option<f64>) -> Res<Outcome<'h>> {
    let h = cx.heap();
    let io = |e: String| failure(h, "io", &[file, h.string(&e)]);
    let mut out = PdfOut::new();
    if let Some(src) = image_path(h, data) {
        if let Some(pdf) = twin(src) {
            return Ok(std::fs::copy(pdf, path).map(|_| ()).map_err(|e| io(e.to_string())));
        }
        if let Err(f) = add_page(cx, &mut out, data, dpi)? {
            return Ok(Err(f));
        }
    } else if let Some(items) = cx.as_list(data).filter(|a| !a.is_empty() && a.iter().all(|&t| is_page(h, t))) {
        for &t in items {
            cx.tick(1)?;
            if let Err(f) = add_page(cx, &mut out, t, dpi)? {
                return Ok(Err(f));
            }
        }
    } else {
        match raster_of(cx, data, 8)? {
            Some(r) => out.image_page(&r, dpi.unwrap_or(72.0)),
            None => return Ok(Err(bad_argument(h, "export_data", 2))),
        }
    }
    Ok(write(path, out.finish()).map_err(io))
}

/// Can `t` be a page of a multi-page PDF?
fn is_page<'h>(h: &'h Heap, t: T<'h>) -> bool {
    image_path(h, t).is_some() || exporter_for(h, t, "pdf").is_some()
}

/// Add `t` (an image value or a value with a PDF exporter) to `out`: the pages of its vector form, or its pixels.
fn add_page<'h>(cx: &mut NativeCx<'_, '_, 'h>, out: &mut PdfOut, t: T<'h>, dpi: Option<f64>) -> Res<Outcome<'h>> {
    let h = cx.heap();
    if let Some(src) = image_path(h, t) {
        let file = h.string(src);
        if let Some(pdf) = twin(src) {
            return Ok(match open_pdf(h, file, &pdf.to_string_lossy()) {
                Ok((doc, _)) => import_pages(cx, out, &doc, file)?,
                Err(f) => Err(f),
            });
        }
        return Ok(match images::load(h, file, src) {
            Ok(r) => {
                out.image_page(&r, dpi.or(r.dpi).unwrap_or(72.0));
                Ok(())
            }
            Err(f) => Err(f),
        });
    }
    let Some(ex) = exporter_for(h, t, "pdf") else { return Ok(Err(bad_argument(h, "export_data", 2))) };
    // the exporter writes a temporary PDF whose pages are copied in
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let dir = image_dir();
    let tmp = dir.join(format!("export-{}-{}.pdf", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed)));
    let tmp_name = tmp.to_string_lossy().into_owned();
    let made = std::fs::create_dir_all(&dir)
        .map_err(|e| e.to_string())
        .and_then(|()| ex(h, t, &tmp, "pdf", &ExportOptions { dpi }));
    let res = match made {
        Ok(()) => match open_pdf(h, h.string(&tmp_name), &tmp_name) {
            Ok((doc, _)) => import_pages(cx, out, &doc, h.string(&tmp_name))?,
            Err(f) => Err(f),
        },
        Err(e) => Err(failure(h, "io", &[h.string(&tmp_name), h.string(&e)])),
    };
    let _ = std::fs::remove_file(&tmp);
    Ok(res)
}

/// Every page of `doc` into `out`.
fn import_pages<'h>(cx: &mut NativeCx<'_, '_, 'h>, out: &mut PdfOut, doc: &Pdf, file: T<'h>) -> Res<Outcome<'h>> {
    let h = cx.heap();
    for page in doc.pages().iter() {
        cx.tick(1)?;
        let copied = {
            let cx = &*cx;
            pdf_read::guarded(|| out.import_page(page, &|| cx.stopped()))
        };
        match copied {
            Some(Ok(())) => {}
            Some(Err(m)) => {
                // an interrupted copy ends the evaluation; any other error is the file's
                cx.tick(1)?;
                return Ok(Err(failure(h, "damaged", &[file, h.string(&m)])));
            }
            None => return Ok(Err(failure(h, "damaged", &[file, h.string("a page could not be copied")]))),
        }
    }
    Ok(Ok(()))
}
