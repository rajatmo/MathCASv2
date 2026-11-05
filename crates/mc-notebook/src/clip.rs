//! The system clipboard (design 3.6): text, and on Windows the private format "MathCAS.Cells" carrying the `.mcn`
//! fragment of copied cells. Elsewhere the text flavour carries the fragment, recognised by its first line.
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
