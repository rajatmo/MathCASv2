//! The system clipboard (design 3.6) behind a small trait, [`SystemClipboard`], with one implementation per platform
//! and [`Clipboard`], the windows' clipboard over it with an in-process copy as the fallback.
//!
//! What each platform carries:
//!
//! | | text | the private formats (cells, boxes) | a picture |
//! |---|---|---|---|
//! | Windows (`clipboard-win`) | yes | the registered formats `MathCAS.Cells` and `MathCAS.Boxes` | the registered `PNG`, a bitmap (`CF_DIB`) and its text |
//! | Linux, BSD: X11 and Wayland (`arboard`), macOS | yes | in-process only (see below) | PNG (`image/png`) |
//! | Android, no display | in-process only | in-process only | in-process text only |
//!
//! Where the system clipboard has no private format, the cells or boxes of a copy are kept in-process and come back
//! with a paste as long as the system clipboard still holds the text of that copy (so copying and pasting cells
//! inside the program keeps their styles, and boxes stay boxes; between programs the text goes). The text of copied
//! cells is their fragment, so another MathCAS window recognises it by its first line.
//!
//! On Wayland, `arboard` uses the data-control protocol (KDE, wlroots compositors); where the compositor lacks it
//! (GNOME, WSLg), it uses X11 through Xwayland. With neither, a clipboard made [`Clipboard::for_window`] hands the
//! text of a copy to the window's toolkit (egui's copy command, which on Wayland goes through the window's own data
//! device), and egui's paste events bring text in: text still travels between programs.
use eframe::egui;

/// What the clipboard holds, in the flavours this program reads and writes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Contents {
    /// the text flavour
    pub text: Option<String>,
    /// the private flavour of cells: cells as written by `mc_nbfile::print_cells`
    pub cells: Option<String>,
    /// the private flavour of boxes: the canonical box text of a copied range holding a square root, fraction or
    /// power box (UTF-8; `text` then holds its linear text)
    pub boxes: Option<String>,
}

/// A picture to put on the clipboard.
#[derive(Clone, Copy, Debug)]
pub struct Picture<'a> {
    /// its text (the `image(...)` expression)
    pub text: &'a str,
    /// its PNG file's bytes, when known
    pub png: Option<&'a [u8]>,
    /// its pixels: RGBA, not premultiplied, row by row from the top
    pub rgba: &'a [u8],
    /// the width in pixels
    pub width: u32,
    /// the height in pixels
    pub height: u32,
}

/// A system clipboard. Every method returns whether the system took the request (`false`/`None`: the caller falls
/// back to its in-process copy).
pub trait SystemClipboard {
    /// Replace the clipboard's contents with `contents`.
    fn set(&mut self, contents: &Contents) -> bool;
    /// Replace the clipboard's contents with `picture`.
    fn set_picture(&mut self, picture: &Picture<'_>) -> bool;
    /// What the clipboard holds (`None`: it could not be read).
    fn get(&mut self) -> Option<Contents>;
}

/// The clipboard of this platform, or `None` where there is none (Android, no display, `MCUI_CLIPBOARD=local`).
pub fn system() -> Option<Box<dyn SystemClipboard>> {
    if std::env::var("MCUI_CLIPBOARD").is_ok_and(|v| v.eq_ignore_ascii_case("local")) {
        return None;
    }
    platform()
}

#[cfg(windows)]
fn platform() -> Option<Box<dyn SystemClipboard>> {
    Some(Box::new(windows::WindowsClipboard))
}

#[cfg(not(any(windows, target_os = "android", target_arch = "wasm32")))]
fn platform() -> Option<Box<dyn SystemClipboard>> {
    other::ArboardClipboard::new().map(|c| Box::new(c) as Box<dyn SystemClipboard>)
}

#[cfg(any(target_os = "android", target_arch = "wasm32"))]
fn platform() -> Option<Box<dyn SystemClipboard>> {
    None
}

/// A picture as a device-independent bitmap (`CF_DIB`): a `BITMAPINFOHEADER`, then the rows bottom-up as BGRA, laid
/// on white for programs that ignore transparency. `None` when `rgba` does not hold `width` x `height` pixels.
pub fn dib(rgba: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let size = (width as usize).checked_mul(height as usize)?.checked_mul(4)?;
    if width == 0 || height == 0 || rgba.len() != size || size > i32::MAX as usize {
        return None;
    }
    let mut out = Vec::with_capacity(40 + size);
    for v in [40u32, width, height] {
        out.extend(v.to_le_bytes());
    }
    out.extend(1u16.to_le_bytes());
    out.extend(32u16.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend((size as u32).to_le_bytes());
    out.extend([0u8; 16]);
    for row in rgba.chunks(width as usize * 4).rev() {
        for px in row.chunks(4) {
            let a = u32::from(px[3]);
            let over = |c: u8| ((u32::from(c) * a + 255 * (255 - a) + 127) / 255) as u8;
            out.extend([over(px[2]), over(px[1]), over(px[0]), 255]);
        }
    }
    Some(out)
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

#[cfg(windows)]
mod windows {
    use super::{Contents, Picture, SystemClipboard};
    use clipboard_win::{raw, with_clipboard_attempts};

    /// The registered name of the private cell format.
    const PRIVATE: &str = "MathCAS.Cells";
    /// The registered name of the private box format.
    const BOXES: &str = "MathCAS.Boxes";

    /// The Windows clipboard (opened for each request, retried while another program holds it).
    pub struct WindowsClipboard;

    impl SystemClipboard for WindowsClipboard {
        fn set(&mut self, clip: &Contents) -> bool {
            let format = raw::register_format(PRIVATE);
            let boxes = raw::register_format(BOXES);
            with_clipboard_attempts(10, || {
                let _ = raw::empty();
                if let Some(t) = &clip.text {
                    let _ = raw::set_string_with(t, clipboard_win::options::NoClear);
                }
                if let (Some(c), Some(f)) = (&clip.cells, format) {
                    let _ = raw::set_without_clear(f.get(), c.as_bytes());
                }
                if let (Some(b), Some(f)) = (&clip.boxes, boxes) {
                    let _ = raw::set_without_clear(f.get(), b.as_bytes());
                }
            })
            .is_ok()
        }

        fn set_picture(&mut self, p: &Picture<'_>) -> bool {
            const CF_DIB: u32 = 8;
            let dib = super::dib(p.rgba, p.width, p.height);
            let png_format = raw::register_format("PNG");
            with_clipboard_attempts(10, || {
                let _ = raw::empty();
                let _ = raw::set_string_with(p.text, clipboard_win::options::NoClear);
                if let Some(dib) = &dib {
                    let _ = raw::set_without_clear(CF_DIB, dib);
                }
                if let (Some(f), Some(png)) = (png_format, p.png) {
                    let _ = raw::set_without_clear(f.get(), png);
                }
            })
            .is_ok()
        }

        fn get(&mut self) -> Option<Contents> {
            let format = raw::register_format(PRIVATE);
            let boxes = raw::register_format(BOXES);
            let mut clip = Contents::default();
            // a private format's UTF-8 text (a terminating zero may follow it)
            let private = |f: Option<std::num::NonZeroU32>| -> Option<String> {
                let f = f?;
                let mut buf = vec![];
                if !raw::is_format_avail(f.get()) || raw::get_vec(f.get(), &mut buf).is_err() {
                    return None;
                }
                while buf.last() == Some(&0) {
                    buf.pop();
                }
                String::from_utf8(buf).ok()
            };
            with_clipboard_attempts(10, || {
                let mut buf = vec![];
                if raw::get_string(&mut buf).is_ok() {
                    clip.text = String::from_utf8(buf).ok().map(|t| t.replace("\r\n", "\n"));
                }
                clip.cells = private(format);
                clip.boxes = private(boxes);
            })
            .ok()?;
            Some(clip)
        }
    }
}

#[cfg(not(any(windows, target_os = "android", target_arch = "wasm32")))]
mod other {
    use super::{Contents, Picture, SystemClipboard};

    /// The clipboard through `arboard`: X11 (served by its own thread while this lives) or Wayland, and macOS.
    pub struct ArboardClipboard(arboard::Clipboard);

    impl ArboardClipboard {
        /// The clipboard, or `None` without a display.
        pub fn new() -> Option<Self> {
            arboard::Clipboard::new().ok().map(ArboardClipboard)
        }
    }

    impl SystemClipboard for ArboardClipboard {
        fn set(&mut self, clip: &Contents) -> bool {
            match &clip.text {
                Some(t) => self.0.set_text(t.as_str()).is_ok(),
                None => self.0.clear().is_ok(),
            }
        }

        fn set_picture(&mut self, p: &Picture<'_>) -> bool {
            let image = arboard::ImageData {
                width: p.width as usize,
                height: p.height as usize,
                bytes: std::borrow::Cow::Borrowed(p.rgba),
            };
            // one flavour per copy here: the picture, or its text when the picture cannot go
            self.0.set_image(image).is_ok() || self.0.set_text(p.text).is_ok()
        }

        fn get(&mut self) -> Option<Contents> {
            match self.0.get_text() {
                Ok(t) => Some(Contents { text: Some(t.replace("\r\n", "\n")), ..Contents::default() }),
                // empty, or holding something other than text (a picture)
                Err(arboard::Error::ContentNotAvailable) => Some(Contents::default()),
                Err(_) => None,
            }
        }
    }
}

/// The windows' clipboard: the system's ([`system`], opened on first use), with an in-process copy as the fallback
/// and as the carrier of the private cell format where the system has none.
#[derive(Default)]
pub struct Clipboard {
    /// the system clipboard: `None` until first used, then `Some(None)` where there is none
    system: Option<Option<Box<dyn SystemClipboard>>>,
    /// the window's toolkit, which takes the text of a copy the system clipboard refused
    window: Option<egui::Context>,
    /// what this program put on the clipboard last
    local: Contents,
    /// text that egui delivered with a paste event (used where the system clipboard cannot be read)
    pub pasted: Option<String>,
}

impl Clipboard {
    /// A clipboard over the given system clipboard (`None`: in-process only).
    pub fn with_system(system: Option<Box<dyn SystemClipboard>>) -> Self {
        Clipboard { system: Some(system), ..Clipboard::default() }
    }

    /// The clipboard of a window: the system's, and the window's toolkit (`ctx`) for text where the system clipboard
    /// cannot be reached.
    pub fn for_window(ctx: &egui::Context) -> Self {
        Clipboard { window: Some(ctx.clone()), ..Clipboard::default() }
    }

    fn system(&mut self) -> Option<&mut Box<dyn SystemClipboard>> {
        self.system.get_or_insert_with(system).as_mut()
    }

    /// The text of a copy the system clipboard did not take goes to the window's toolkit.
    fn fall_back(&self, took: bool, text: Option<&str>) {
        if let (false, Some(ctx), Some(t)) = (took, &self.window, text) {
            ctx.copy_text(t.to_string());
        }
    }

    /// Put `contents` on the clipboard.
    pub fn set(&mut self, contents: Contents) {
        let took = self.system().is_some_and(|s| s.set(&contents));
        self.fall_back(took, contents.text.as_deref());
        self.local = contents;
    }

    /// Put a picture on the clipboard.
    pub fn set_picture(&mut self, picture: &Picture<'_>) {
        let took = self.system().is_some_and(|s| s.set_picture(picture));
        self.fall_back(took, Some(picture.text));
        self.local = Contents { text: Some(picture.text.to_string()), ..Contents::default() };
    }

    /// What the clipboard holds.
    pub fn get(&mut self) -> Contents {
        let local = &self.local;
        if let Some(mut c) = self.system.get_or_insert_with(system).as_mut().and_then(|s| s.get()) {
            // the system has no private formats: the cells or boxes of our last copy, while its text is still there
            if c.cells.is_none() && c.boxes.is_none() && c.text.is_some() && c.text == local.text {
                c.cells = local.cells.clone();
                c.boxes = local.boxes.clone();
            }
            self.pasted = None;
            return c;
        }
        match self.pasted.take() {
            Some(t) if self.local.text.as_deref() != Some(&t) => Contents { text: Some(t), ..Contents::default() },
            _ => self.local.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A system clipboard holding text only, shared with the test.
    #[derive(Clone, Default)]
    struct TextOnly(Rc<RefCell<Option<String>>>);

    impl SystemClipboard for TextOnly {
        fn set(&mut self, c: &Contents) -> bool {
            *self.0.borrow_mut() = c.text.clone();
            true
        }
        fn set_picture(&mut self, p: &Picture<'_>) -> bool {
            *self.0.borrow_mut() = Some(p.text.to_string());
            true
        }
        fn get(&mut self) -> Option<Contents> {
            Some(Contents { text: self.0.borrow().clone(), ..Contents::default() })
        }
    }

    fn cells(text: &str, cells: &str) -> Contents {
        Contents { text: Some(text.into()), cells: Some(cells.into()), boxes: None }
    }

    fn boxes(text: &str, boxes: &str) -> Contents {
        Contents { text: Some(text.into()), cells: None, boxes: Some(boxes.into()) }
    }

    #[test]
    fn boxes_travel_with_their_linear_text() {
        // in-process only (what `MCUI_CLIPBOARD=local` selects, as the notebook's tests run): both flavours come
        // back as they went
        if std::env::var("MCUI_CLIPBOARD").is_ok_and(|v| v == "local") {
            assert!(system().is_none());
        }
        let mut c = Clipboard::with_system(None);
        c.set(boxes("a/b + sqrt(x)", "((a)/(b)) + ((sqrt(x)))"));
        assert_eq!(c.get(), boxes("a/b + sqrt(x)", "((a)/(b)) + ((sqrt(x)))"));
        assert_eq!(c.get().boxes.as_deref(), Some("((a)/(b)) + ((sqrt(x)))"));
        // a paste event with another program's text: text only, even when it looks like box text
        c.pasted = Some("((a)/(b))".into());
        assert_eq!(c.get(), Contents { text: Some("((a)/(b))".into()), ..Contents::default() });
        // a copy without boxes clears them
        c.set(Contents { text: Some("y".into()), ..Contents::default() });
        assert_eq!(c.get().boxes, None);
        // a text-only system clipboard: the boxes travel in-process while the text is ours
        let shared = TextOnly::default();
        let mut c = Clipboard::with_system(Some(Box::new(shared.clone())));
        c.set(boxes("x^2", "((x)^(2))"));
        assert_eq!(shared.0.borrow().as_deref(), Some("x^2"));
        assert_eq!(c.get(), boxes("x^2", "((x)^(2))"));
        *shared.0.borrow_mut() = Some("x^3".into());
        assert_eq!(c.get(), Contents { text: Some("x^3".into()), ..Contents::default() });
    }

    #[test]
    fn in_process_only() {
        let mut c = Clipboard::with_system(None);
        assert_eq!(c.get(), Contents::default());
        c.set(cells("x", "<cells>"));
        assert_eq!(c.get(), cells("x", "<cells>"));
        // a paste event from another program wins over our own copy
        c.pasted = Some("from elsewhere".into());
        assert_eq!(c.get().text.as_deref(), Some("from elsewhere"));
        assert_eq!(c.get(), cells("x", "<cells>"));
        // a paste event carrying our own text keeps the cells
        c.pasted = Some("x".into());
        assert_eq!(c.get(), cells("x", "<cells>"));
    }

    #[test]
    fn text_goes_to_the_window_without_a_system_clipboard() {
        let ctx = egui::Context::default();
        let mut c = Clipboard::for_window(&ctx);
        c.system = Some(None);
        let out = ctx.run_ui(egui::RawInput::default(), |_| c.set(cells("a + b", "<cells>")));
        assert!(out.platform_output.commands.contains(&egui::OutputCommand::CopyText("a + b".into())));
        assert_eq!(c.get(), cells("a + b", "<cells>"));
        // a system clipboard that takes the copy: the window is not asked
        let mut c = Clipboard::for_window(&ctx);
        c.system = Some(Some(Box::new(TextOnly::default())));
        let out = ctx.run_ui(egui::RawInput::default(), |_| c.set(cells("x", "<cells>")));
        assert!(!out.platform_output.commands.iter().any(|o| matches!(o, egui::OutputCommand::CopyText(_))));
    }

    #[test]
    fn cells_travel_in_process_while_the_text_is_ours() {
        let shared = TextOnly::default();
        let mut c = Clipboard::with_system(Some(Box::new(shared.clone())));
        c.set(cells("a + b", "<two cells>"));
        assert_eq!(shared.0.borrow().as_deref(), Some("a + b"));
        assert_eq!(c.get(), cells("a + b", "<two cells>"));
        // another program copied something: its text, without our cells
        *shared.0.borrow_mut() = Some("other".into());
        assert_eq!(c.get(), Contents { text: Some("other".into()), ..Contents::default() });
        // an empty clipboard is empty, not our old copy
        *shared.0.borrow_mut() = None;
        assert_eq!(c.get(), Contents::default());
    }

    #[test]
    fn pictures_leave_their_text() {
        let shared = TextOnly::default();
        let mut c = Clipboard::with_system(Some(Box::new(shared.clone())));
        let rgba = [255u8, 0, 0, 255, 0, 0, 255, 0];
        let p = Picture { text: "image(\"a.png\")", png: None, rgba: &rgba, width: 2, height: 1 };
        c.set_picture(&p);
        assert_eq!(c.get().text.as_deref(), Some("image(\"a.png\")"));
        let mut local = Clipboard::with_system(None);
        local.set_picture(&p);
        assert_eq!(local.get(), Contents { text: Some("image(\"a.png\")".into()), ..Contents::default() });
    }

    /// The real clipboard of this system (it replaces what the clipboard holds, so it runs only when asked:
    /// `cargo test -p mc-ui -- --ignored`; on Linux it needs a display).
    #[test]
    #[ignore]
    fn system_round_trip() {
        let mut c = Clipboard::default();
        let text = format!("mc-ui clipboard test {}", std::process::id());
        c.set(Contents { text: Some(text.clone()), cells: Some("<cells>".into()), boxes: None });
        let back = c.get();
        assert_eq!(back.text.as_deref(), Some(text.as_str()));
        assert_eq!(back.cells.as_deref(), Some("<cells>"));
        // the box flavour, read back through another clipboard object (as another window would)
        c.set(Contents { text: Some(text.clone()), cells: None, boxes: Some("((x)^(2))".into()) });
        let back = system().expect("a system clipboard").get().expect("readable");
        assert_eq!(back.boxes.as_deref(), Some("((x)^(2))"));
        assert_eq!(back.cells, None);
        // another clipboard object (as another window would) sees the text
        let other = system().expect("a system clipboard").get().expect("readable");
        assert_eq!(other.text.as_deref(), Some(text.as_str()));
        let rgba = [255u8, 0, 0, 255, 0, 0, 255, 255];
        let png = encode_png(&rgba, 2, 1);
        c.set_picture(&Picture { text: "image(\"t.png\")", png: png.as_deref(), rgba: &rgba, width: 2, height: 1 });
        c.set(Contents { text: Some(String::new()), ..Contents::default() });
    }

    #[test]
    fn bitmaps_are_bottom_up_bgra_on_white() {
        // 2 x 2: red, transparent / blue, half-transparent black
        let rgba = [255, 0, 0, 255, 9, 9, 9, 0, 0, 0, 255, 255, 0, 0, 0, 128];
        let d = dib(&rgba, 2, 2).unwrap();
        assert_eq!(d.len(), 40 + 16);
        assert_eq!(&d[0..4], &40u32.to_le_bytes());
        assert_eq!(&d[4..8], &2u32.to_le_bytes());
        assert_eq!(&d[14..16], &32u16.to_le_bytes());
        // bottom row first: blue, then grey (black at half alpha over white)
        assert_eq!(&d[40..48], &[255, 0, 0, 255, 127, 127, 127, 255]);
        // top row: red, then white (transparent)
        assert_eq!(&d[48..56], &[0, 0, 255, 255, 255, 255, 255, 255]);
        assert!(dib(&rgba, 3, 2).is_none());
        assert!(dib(&[], 0, 0).is_none());
    }

    #[test]
    fn png_round_trip() {
        let rgba = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let bytes = encode_png(&rgba, 2, 1).unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (2, 1));
        assert_eq!(&buf[..8], &rgba);
    }
}
