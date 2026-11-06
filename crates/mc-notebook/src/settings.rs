//! Settings kept between sessions (design 8.3): `notebook.json` in the settings folder of `mc_ui::dirs`
//! (`%APPDATA%/MathCAS` on Windows, `$XDG_CONFIG_HOME/MathCAS` or `~/.config/MathCAS` on Linux, the app folder on
//! Android).
use crate::theme::ThemeChoice;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// How many recent files are remembered.
const RECENT: usize = 10;

/// A tab of the last session.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionTab {
    /// the notebook
    pub path: PathBuf,
    /// the scroll offset (points)
    pub offset: f32,
    /// the caret: cell index and byte position
    pub caret: Option<(usize, usize)>,
}

/// The tabs of the last session (only tabs with a file are kept).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Session {
    /// the tabs in order
    pub tabs: Vec<SessionTab>,
    /// the active one
    pub active: usize,
}

/// The settings.
#[derive(Clone, Debug)]
pub struct Settings {
    /// the theme
    pub theme: ThemeChoice,
    /// the notebook magnification
    pub magnification: f32,
    /// recently opened or saved notebooks, newest first
    pub recent: Vec<PathBuf>,
    /// save without output cells and labels
    pub save_without_outputs: bool,
    /// show the toolbar
    pub show_toolbar: bool,
    /// reopen the last session's tabs at start
    pub restore_session: bool,
    /// the tabs of the last session
    pub session: Session,
    /// the caret blinks (off: a steady caret)
    pub caret_blink: bool,
    /// typing an opening bracket or quote inserts its closer
    pub auto_close: bool,
    /// outputs end at the content's right edge (View > Output alignment > Right); off (the default, user
    /// decision of round 13): they start at the left edge like inputs
    pub outputs_right: bool,
    /// only the bundled fonts, never the system's (the same look on every system)
    pub bundled_fonts: bool,
    /// the window's last position and inner size: x, y, width, height (points)
    pub window: Option<[f32; 4]>,
    /// where the settings are read from and written to (`None`: not saved, as in the self-test)
    file: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: ThemeChoice::System,
            magnification: 1.0,
            recent: vec![],
            save_without_outputs: false,
            show_toolbar: true,
            restore_session: true,
            session: Session::default(),
            caret_blink: true,
            auto_close: true,
            outputs_right: false,
            bundled_fonts: false,
            window: None,
            file: None,
        }
    }
}

/// The folder of the settings and of the single-instance file (`mc_ui::dirs`): `MCNB_CONFIG_DIR` when set, else none
/// in the self-test (which keeps nothing), else the user's settings folder.
pub use mc_ui::dirs::config_dir;

fn settings_file() -> Option<PathBuf> {
    Some(config_dir()?.join("notebook.json"))
}

impl Settings {
    /// The saved settings, or the defaults.
    pub fn load() -> Settings {
        let file = settings_file();
        let mut s = Settings { file: file.clone(), ..Settings::default() };
        let Some(v) =
            file.and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| serde_json::from_str::<Value>(&t).ok())
        else {
            return s;
        };
        if let Some(t) = v.get("theme").and_then(Value::as_str) {
            s.theme = ThemeChoice::from_name(t);
        }
        if let Some(m) = v.get("magnification").and_then(Value::as_f64) {
            s.magnification = (m as f32).clamp(0.5, 3.0);
        }
        if let Some(r) = v.get("recent").and_then(Value::as_array) {
            s.recent = r.iter().filter_map(Value::as_str).map(PathBuf::from).collect();
        }
        s.save_without_outputs = v.get("save_without_outputs").and_then(Value::as_bool).unwrap_or(false);
        s.show_toolbar = v.get("show_toolbar").and_then(Value::as_bool).unwrap_or(true);
        s.restore_session = v.get("restore_session").and_then(Value::as_bool).unwrap_or(true);
        s.caret_blink = v.get("caret_blink").and_then(Value::as_bool).unwrap_or(true);
        s.auto_close = v.get("auto_close").and_then(Value::as_bool).unwrap_or(true);
        s.outputs_right = v.get("output_alignment").and_then(Value::as_str) == Some("right");
        s.bundled_fonts = v.get("bundled_fonts").and_then(Value::as_bool).unwrap_or(false);
        if let Some(w) = v.get("window").and_then(Value::as_array) {
            let w: Vec<f32> = w.iter().filter_map(Value::as_f64).map(|x| x as f32).collect();
            // a place off every screen (a minimized window's -32000) or absurdly large is not restored
            let sane = |x: f32| x.is_finite() && (-10000.0..=30000.0).contains(&x);
            if w.len() == 4 && w.iter().all(|&x| sane(x)) && w[2] >= 200.0 && w[3] >= 150.0 {
                s.window = Some([w[0], w[1], w[2], w[3]]);
            }
        }
        if let Some(sess) = v.get("session") {
            s.session.active = sess.get("active").and_then(Value::as_u64).unwrap_or(0) as usize;
            for t in sess.get("tabs").and_then(Value::as_array).into_iter().flatten() {
                let Some(path) = t.get("path").and_then(Value::as_str) else {
                    continue;
                };
                let caret = t
                    .get("caret")
                    .and_then(Value::as_array)
                    .and_then(|c| Some((c.first()?.as_u64()? as usize, c.get(1)?.as_u64()? as usize)));
                s.session.tabs.push(SessionTab {
                    path: PathBuf::from(path),
                    offset: t.get("offset").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                    caret,
                });
            }
        }
        s
    }

    /// Write the settings.
    pub fn save(&self) {
        let Some(f) = &self.file else { return };
        let v = json!({
            "theme": self.theme.name(),
            "magnification": self.magnification,
            "recent": self.recent.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "save_without_outputs": self.save_without_outputs,
            "show_toolbar": self.show_toolbar,
            "restore_session": self.restore_session,
            "caret_blink": self.caret_blink,
            "auto_close": self.auto_close,
            "output_alignment": if self.outputs_right { "right" } else { "left" },
            "bundled_fonts": self.bundled_fonts,
            "window": self.window,
            "session": {
                "active": self.session.active,
                "tabs": self.session.tabs.iter().map(|t| json!({
                    "path": t.path.display().to_string(),
                    "offset": t.offset,
                    "caret": t.caret.map(|(i, p)| vec![i, p]),
                })).collect::<Vec<_>>(),
            },
        });
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = mc_nbfile::write_atomic(f, &format!("{v:#}\n"));
    }

    /// Remember a notebook as recently used.
    pub fn add_recent(&mut self, p: &Path) {
        let p = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        // `C:\x`, not the verbatim `\\?\C:\x` of canonicalisation on Windows
        let p = p.to_str().and_then(|s| s.strip_prefix(r"\\?\")).map_or(p.clone(), PathBuf::from);
        self.recent.retain(|q| q != &p);
        self.recent.insert(0, p);
        self.recent.truncate(RECENT);
        self.save();
    }
}
