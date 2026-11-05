//! Settings kept between sessions (design 8.3): `%APPDATA%/MathCAS/notebook.json` (on other systems the config
//! folder named by `XDG_CONFIG_HOME` or `~/.config`).
use crate::theme::ThemeChoice;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// How many recent files are remembered.
const RECENT: usize = 10;

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
            file: None,
        }
    }
}

fn settings_file() -> Option<PathBuf> {
    if std::env::var_os("MCNB_SELFTEST").is_some() {
        return None;
    }
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".config")))?;
    Some(base.join("MathCAS").join("notebook.json"))
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
        });
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = mc_nbfile::write_atomic(f, &format!("{v:#}\n"));
    }

    /// Remember a notebook as recently used.
    pub fn add_recent(&mut self, p: &Path) {
        let p = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        self.recent.retain(|q| q != &p);
        self.recent.insert(0, p);
        self.recent.truncate(RECENT);
        self.save();
    }
}
