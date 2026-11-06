//! The system clipboard (design 3.6): text, and on Windows the private format "MathCAS.Cells" carrying the `.mcn`
//! fragment of copied cells. Elsewhere the text flavour carries the fragment, recognised by its first line. A copied
//! picture (design 6) goes as PNG (the registered "PNG" format), as a device-independent bitmap, and as its
//! `image(...)` text.
use mc_nbmodel::Clip;

#[cfg(windows)]
mod sys {
    use super::Clip;
    use clipboard_win::{raw, with_clipboard_attempts};

    const PRIVATE: &str = "MathCAS.Cells";

    pub fn set(clip: &Clip) -> bool {
        let format = raw::register_format(PRIVATE);
        with_clipboard_attempts(10, || {
            let _ = raw::empty();
            if let Some(t) = &clip.text {
                let _ = raw::set_string_with(t, clipboard_win::options::NoClear);
            }
            if let (Some(c), Some(f)) = (&clip.cells, format) {
                let _ = raw::set_without_clear(f.get(), c.as_bytes());
            }
        })
        .is_ok()
    }

    /// A picture: its text, its PNG file's bytes, and a bitmap of its pixels (RGBA, not premultiplied), laid on
    /// white for programs that ignore transparency.
    pub fn set_picture(text: &str, png: Option<&[u8]>, rgba: &[u8], w: u32, h: u32) -> bool {
        // CF_DIB: a BITMAPINFOHEADER, then the rows bottom-up as BGRA
        const CF_DIB: u32 = 8;
        let mut dib = Vec::with_capacity(40 + rgba.len());
        for v in [40u32, w, h] {
            dib.extend(v.to_le_bytes());
        }
        dib.extend(1u16.to_le_bytes());
        dib.extend(32u16.to_le_bytes());
        dib.extend(0u32.to_le_bytes());
        dib.extend((w * h * 4).to_le_bytes());
        dib.extend([0u8; 16]);
        for row in rgba.chunks((w * 4) as usize).rev() {
            for px in row.chunks(4) {
                let a = u32::from(px[3]);
                let over = |c: u8| ((u32::from(c) * a + 255 * (255 - a) + 127) / 255) as u8;
                dib.extend([over(px[2]), over(px[1]), over(px[0]), 255]);
            }
        }
        let png_format = raw::register_format("PNG");
        with_clipboard_attempts(10, || {
            let _ = raw::empty();
            let _ = raw::set_string_with(text, clipboard_win::options::NoClear);
            let _ = raw::set_without_clear(CF_DIB, &dib);
            if let (Some(f), Some(png)) = (png_format, png) {
                let _ = raw::set_without_clear(f.get(), png);
            }
        })
        .is_ok()
    }

    pub fn get() -> Option<Clip> {
        let format = raw::register_format(PRIVATE);
        let mut clip = Clip::default();
        with_clipboard_attempts(10, || {
            let mut buf = vec![];
            if raw::get_string(&mut buf).is_ok() {
                clip.text = String::from_utf8(buf).ok().map(|t| t.replace("\r\n", "\n"));
            }
            if let Some(f) = format {
                let mut buf = vec![];
                if raw::is_format_avail(f.get()) && raw::get_vec(f.get(), &mut buf).is_ok() {
                    // a terminating zero may follow the text
                    while buf.last() == Some(&0) {
                        buf.pop();
                    }
                    clip.cells = String::from_utf8(buf).ok();
                }
            }
        })
        .ok()?;
        Some(clip)
    }
}

/// The clipboard: the system's, with an in-process copy as a fallback.
#[derive(Default)]
pub struct Clipboard {
    local: Clip,
    /// text that egui delivered with a paste event (used where the system clipboard cannot be read)
    pub pasted: Option<String>,
}

impl Clipboard {
    /// Put `clip` on the clipboard.
    pub fn set(&mut self, clip: Clip) {
        #[cfg(windows)]
        {
            sys::set(&clip);
        }
        self.local = clip;
    }

    /// Put a picture on the clipboard: its `image(...)` text, its PNG bytes when known, its pixels (RGBA, `w` x `h`).
    pub fn set_picture(&mut self, text: &str, png: Option<&[u8]>, rgba: &[u8], w: u32, h: u32) {
        #[cfg(windows)]
        {
            sys::set_picture(text, png, rgba, w, h);
        }
        #[cfg(not(windows))]
        let _ = (png, rgba, w, h);
        self.local = Clip { text: Some(text.to_string()), cells: None };
    }

    /// What the clipboard holds.
    pub fn get(&mut self) -> Clip {
        #[cfg(windows)]
        {
            if let Some(c) = sys::get() {
                return c;
            }
        }
        match self.pasted.take() {
            Some(t) if self.local.text.as_deref() != Some(&t) => Clip { text: Some(t), cells: None },
            _ => self.local.clone(),
        }
    }
}

/// RGBA pixels (not premultiplied) as a PNG file's bytes.
pub fn encode_png(rgba: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().ok()?.write_image_data(rgba).ok()?;
    Some(out)
}
