//! What a phone's system does for an app and a desktop does not, behind one set of functions: handing a file or a
//! text to another app (the share sheet), the system's document picker, files other apps hand in, and the parts of
//! the window the system covers.
//!
//! On Android these go to the activity ([`crate::android`]). Elsewhere there is no share sheet and no picker: a
//! shared file is written to `mathcas-shared` in the temporary folder, [`pick_document`] gives the file named by
//! `MCNB_PICK` (for trying the phone layout out on a desktop and for its tests), and nothing covers the window.
use eframe::egui;
use std::path::PathBuf;

/// The answer of [`pick_document`].
#[derive(Clone, Debug, PartialEq)]
pub enum Picked {
    /// this file
    Path(PathBuf),
    /// the system's picker is open: the file arrives through [`take_incoming`]
    Later,
    /// nothing
    None,
}

/// Ask for a notebook from elsewhere (other apps' storage).
pub fn pick_document() -> Picked {
    #[cfg(target_os = "android")]
    {
        if crate::android::pick_document() {
            Picked::Later
        } else {
            Picked::None
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        std::env::var_os("MCNB_PICK").map(PathBuf::from).filter(|p| p.is_file()).map_or(Picked::None, Picked::Path)
    }
}

/// A file the system handed to the app since the last call (the picker's answer, "Open with" in another app), as a
/// copy in the app's cache.
pub fn take_incoming() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        crate::android::take_incoming()
    }
    #[cfg(not(target_os = "android"))]
    {
        None
    }
}

/// Hand `bytes` to another app as a file named `name` of media type `mime` (the user chooses the app).
pub fn share_bytes(name: &str, mime: &str, bytes: &[u8]) -> Result<(), String> {
    let dir = shared_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(name);
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    #[cfg(target_os = "android")]
    {
        crate::android::share_file(&path, mime)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = mime;
        Ok(())
    }
}

/// Hand `text` to another app.
pub fn share_text(text: &str) {
    #[cfg(target_os = "android")]
    crate::android::share_text(text);
    #[cfg(not(target_os = "android"))]
    {
        let _ = std::fs::create_dir_all(shared_dir());
        let _ = std::fs::write(shared_dir().join("shared.txt"), text);
    }
}

/// Where shared files are written: the app's cache (`TMPDIR` on Android), a folder the share sheet's content
/// provider serves.
pub fn shared_dir() -> PathBuf {
    std::env::temp_dir().join("mathcas-shared")
}

/// Whether the system shows its dark theme, where the window cannot tell (Android: asked of the activity, at most
/// once a second; `now` is the frame clock in seconds). `None` elsewhere: the window knows.
pub fn system_dark(now: f64) -> Option<bool> {
    #[cfg(target_os = "android")]
    {
        use std::sync::Mutex;
        static LAST: Mutex<Option<(f64, bool)>> = Mutex::new(None);
        let mut last = LAST.lock().ok()?;
        match *last {
            Some((at, dark)) if (now - at).abs() < 1.0 => Some(dark),
            _ => {
                let dark = crate::android::night_mode()?;
                *last = Some((now, dark));
                Some(dark)
            }
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = now;
        None
    }
}

/// Tell the system whether the page under its status and navigation bars is light, so it draws its clock and icons
/// dark on it (and light on a dark page). Only Android has such bars over the window.
pub fn light_bars(light: bool) {
    #[cfg(target_os = "android")]
    crate::android::light_bars(light);
    #[cfg(not(target_os = "android"))]
    let _ = light;
}

/// How much of the window's top and bottom the system covers (its bars, the on-screen keyboard), in points.
pub fn bars(ctx: &egui::Context) -> (f32, f32) {
    #[cfg(target_os = "android")]
    {
        let (top, bottom) = crate::android::bars();
        let scale = ctx.pixels_per_point();
        (top / scale, bottom / scale)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = ctx;
        (0.0, 0.0)
    }
}
