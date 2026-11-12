//! `export_animation(path, frames)`: graphics (or plots) as the frames of an animated GIF file.
use crate::{bad, failure};
use mc_core::{Heap, Store, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use std::sync::atomic::Ordering;

/// Register the native.
pub(crate) fn install(k: &mut Kernel, h: &Heap) {
    k.register(h.sym("export_animation"), Attributes::default(), Box::new(ExportAnimation));
}

/// The most frames of an animation.
const MAX_FRAMES: usize = 2000;

struct ExportAnimation;

impl Native for ExportAnimation {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = "export_animation";
        let (path, frames, opts) = match args {
            [p, f] => (*p, *f, None),
            [p, f, o] => (*p, *f, Some(*o)),
            _ => return Ok(Reply::Decline),
        };
        let View::Str(path) = h.view(path) else { return Ok(bad(h, name, 1)) };
        let path = path.to_string();
        if !path.to_ascii_lowercase().ends_with(".gif") {
            return Ok(bad(h, name, 1));
        }
        let Some(items) = cx.as_list(frames).filter(|v| !v.is_empty() && v.len() <= MAX_FRAMES) else { return Ok(bad(h, name, 2)) };
        let items = items.to_vec();
        // frame_rate: frames per second (default 10)
        let mut fps = 10.0;
        if let Some(o) = opts {
            if let View::App { args: rs, .. } = h.view(o) {
                for &r in rs {
                    if let View::App { args: &[k, v], .. } = h.view(r) {
                        if matches!(h.view(k), View::Sym(s) if matches!(h.sym_name(s).as_str(), "frame_rate" | "fps")) {
                            match mc_graphics::num::number(h, v) {
                                Some(f) if f > 0.0 && f <= 100.0 => fps = f,
                                _ => return Ok(bad(h, name, 3)),
                            }
                        }
                    }
                }
            }
        }
        cx.effect()?;
        let stop = cx.stop_flag().clone();
        let mut pictures = Vec::with_capacity(items.len());
        for (k, f) in items.into_iter().enumerate() {
            cx.tick(1)?;
            let g = mc_graphics_kernel::compose::resolve(cx, f)?;
            if !mc_graphics::is_graphics(h, g) {
                return Ok(Reply::Done(failure(h, "argument", &[h.sym_term(h.sym(name)), h.int(2), h.int(k as i64 + 1)])));
            }
            match mc_graphics::render_rgba(h, g, 1.0, &|| stop.load(Ordering::Relaxed)) {
                Ok(p) => pictures.push(p),
                Err(f) => return Ok(Reply::Done(f.term(h))),
            }
        }
        let write = || write_gif(&pictures, std::path::Path::new(&path), fps);
        Ok(Reply::Done(match write() {
            Ok(()) => h.string(&path),
            Err(e) => failure(h, "io", &[h.string(&e)]),
        }))
    }
}

/// RGBA frames (width, height, straight RGBA bytes) as an animated GIF at `fps` frames per second, looping; every
/// frame on a canvas of the largest size, white around smaller ones.
pub fn write_gif(pictures: &[(u32, u32, Vec<u8>)], path: &std::path::Path, fps: f64) -> Result<(), String> {
    let w = pictures.iter().map(|p| p.0).max().unwrap_or(1).min(u16::MAX as u32);
    let ht = pictures.iter().map(|p| p.1).max().unwrap_or(1).min(u16::MAX as u32);
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = gif::Encoder::new(std::io::BufWriter::new(file), w as u16, ht as u16, &[]).map_err(|e| e.to_string())?;
    enc.set_repeat(gif::Repeat::Infinite).map_err(|e| e.to_string())?;
    let delay = (100.0 / fps).round().max(1.0) as u16;
    for (pw, ph, px) in pictures {
        let mut canvas = vec![255u8; (w * ht * 4) as usize];
        for y in 0..(*ph).min(ht) {
            for x in 0..(*pw).min(w) {
                let s = ((y * pw + x) * 4) as usize;
                let d = ((y * w + x) * 4) as usize;
                // over white
                let a = px[s + 3] as u32;
                for c in 0..3 {
                    canvas[d + c] = ((px[s + c] as u32 * a + 255 * (255 - a)) / 255) as u8;
                }
                canvas[d + 3] = 255;
            }
        }
        let mut frame = gif::Frame::from_rgba_speed(w as u16, ht as u16, &mut canvas, 10);
        frame.delay = delay;
        enc.write_frame(&frame).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The pixels of one frame: a graphics term rendered, or the PNG file of a plot (`image(path)`).
fn frame_pixels<'h>(h: &'h Heap, t: T<'h>) -> Result<(u32, u32, Vec<u8>), String> {
    if mc_graphics::is_graphics(h, t) {
        return mc_graphics::render_rgba(h, t, 1.0, &|| false).map_err(|f| f.to_string());
    }
    if let View::App { head, args: &[p] } = h.view(t) {
        if let (View::Sym(s), View::Str(path)) = (h.view(head), h.view(p)) {
            if h.sym_name(s) == "image" {
                let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
                return mc_graphics::decode_png(&bytes).ok_or_else(|| format!("{path} is not a PNG file"));
            }
        }
    }
    Err(format!("{} is not a graphic", mc_syntax::Printer::new(h).expr(t)))
}

/// The exporter of graphics for `export_data`: a `graphics` or `graphics3d` term as PNG (at `dpi`/72 pixels per
/// point), SVG, PDF or a one-frame GIF; a list of graphics (or plots) as an animated GIF at 10 frames a second.
pub fn export<'h>(h: &'h Heap, t: T<'h>, path: &std::path::Path, format: &str, options: &mc_io::ExportOptions) -> Result<(), String> {
    let f = match format {
        "png" => mc_graphics::Format::Png { scale: options.dpi.map_or(1.0, |d| (d / 72.0) as f32) },
        "svg" => mc_graphics::Format::Svg,
        "pdf" => mc_graphics::Format::Pdf,
        "gif" => {
            let frames: Vec<T<'h>> = match h.view(t) {
                View::App { head, args } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == mc_api::names::LIST) => args.to_vec(),
                _ => vec![t],
            };
            if frames.is_empty() || frames.len() > MAX_FRAMES {
                return Err("an animation takes 1 to 2000 frames".into());
            }
            let pictures = frames.into_iter().map(|g| frame_pixels(h, g)).collect::<Result<Vec<_>, _>>()?;
            return write_gif(&pictures, path, 10.0);
        }
        other => return Err(format!("graphics cannot be written as {other}")),
    };
    // labeled(x, l) and legended(x, l) around values that are not pictures: the value with its label
    let t = if mc_graphics::is_graphics(h, t) { t } else { mc_graphics_kernel::compose::shown_layout(h, t).unwrap_or(t) };
    let bytes = mc_graphics::render(h, t, f).map_err(|e| e.to_string())?;
    // PNG and SVG files carry the term, so import_data can read them back as graphics
    let bytes = crate::roundtrip::embed(bytes, format, &mc_syntax::Printer::new(h).expr(t));
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}
