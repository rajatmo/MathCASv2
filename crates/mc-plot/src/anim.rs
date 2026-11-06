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
            let g = mc_graphics::compose::resolve(cx, f)?;
            if !mc_graphics::is_graphics(h, g) {
                return Ok(Reply::Done(failure(h, "argument", &[h.sym_term(h.sym(name)), h.int(2), h.int(k as i64 + 1)])));
            }
            match mc_graphics::render_rgba(h, g, 1.0, &|| stop.load(Ordering::Relaxed)) {
                Ok(p) => pictures.push(p),
                Err(f) => return Ok(Reply::Done(f.term(h))),
            }
        }
        // every frame on a canvas of the largest size, white around smaller ones
        let w = pictures.iter().map(|p| p.0).max().unwrap_or(1).min(u16::MAX as u32);
        let ht = pictures.iter().map(|p| p.1).max().unwrap_or(1).min(u16::MAX as u32);
        let write = || -> Result<(), String> {
            let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
            let mut enc = gif::Encoder::new(std::io::BufWriter::new(file), w as u16, ht as u16, &[]).map_err(|e| e.to_string())?;
            enc.set_repeat(gif::Repeat::Infinite).map_err(|e| e.to_string())?;
            let delay = (100.0 / fps).round().max(1.0) as u16;
            for (pw, ph, px) in &pictures {
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
        };
        Ok(Reply::Done(match write() {
            Ok(()) => h.string(&path),
            Err(e) => failure(h, "io", &[h.string(&e)]),
        }))
    }
}
