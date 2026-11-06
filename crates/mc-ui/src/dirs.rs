//! The settings folder of the MathCAS windows, and atomic file writes for what is kept there.
//!
//! The folder is `MathCAS` in the platform's per-user settings place: `%APPDATA%` on Windows, `$XDG_CONFIG_HOME`
//! (else `~/.config`) on Linux and the BSDs, `~/Library/Application Support` on macOS. On Android there is no such
//! place in the environment: the program's entry point passes the activity's internal data folder to
//! [`set_app_dir`], which then is the folder (it is private to the app). Programs and tests can name a folder of their
//! own with `MCNB_CONFIG_DIR`; a self-test (`MCNB_SELFTEST`, `MCDOCS_SELFTEST`) keeps nothing.
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The name of the settings folder inside the platform's settings place.
pub const APP_FOLDER: &str = "MathCAS";

static APP_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Name the folder of the app's own files (Android: the activity's internal data path). It wins over the platform's
/// settings place. Only the first call has an effect; it returns whether this call set it.
pub fn set_app_dir(dir: PathBuf) -> bool {
    APP_DIR.set(dir).is_ok()
}

/// The folder of the settings, `gpu.json` and the single-instance file: `MCNB_CONFIG_DIR` when set, none in a
/// self-test, else [`user_config_dir`].
pub fn config_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("MCNB_CONFIG_DIR") {
        return Some(PathBuf::from(d));
    }
    if std::env::var_os("MCNB_SELFTEST").is_some() || std::env::var_os("MCDOCS_SELFTEST").is_some() {
        return None;
    }
    user_config_dir()
}

/// The user's MathCAS settings folder on this system (it may not exist yet), ignoring the overrides of
/// [`config_dir`].
pub fn user_config_dir() -> Option<PathBuf> {
    if let Some(d) = APP_DIR.get() {
        return Some(d.clone());
    }
    config_dir_for(std::env::consts::OS, |k| std::env::var_os(k))
}

/// The settings folder on operating system `os` (a `std::env::consts::OS` name) with the environment `env`.
pub fn config_dir_for(os: &str, env: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    // only absolute paths count: a relative one would depend on the working folder
    let var = |k: &str| env(k).map(PathBuf::from).filter(|p| p.is_absolute() || looks_absolute(p));
    let base = match os {
        "windows" => var("APPDATA").or_else(|| var("USERPROFILE").map(|h| h.join("AppData").join("Roaming"))),
        "macos" | "ios" => var("HOME").map(|h| h.join("Library").join("Application Support")),
        // the app folder comes from the activity ([`set_app_dir`])
        "android" => None,
        _ => var("XDG_CONFIG_HOME").or_else(|| var("HOME").map(|h| h.join(".config"))),
    }?;
    Some(base.join(APP_FOLDER))
}

/// Whether `p` is absolute on some system (`/home/x` on Windows, `C:\x` elsewhere), so the rule of
/// [`config_dir_for`] for another operating system can be tested on this one.
fn looks_absolute(p: &Path) -> bool {
    let s = p.to_string_lossy();
    let b = s.as_bytes();
    s.starts_with('/') || s.starts_with('\\') || (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':')
}

/// Write `text` to `path` through a temporary file in the same folder and a rename, so a crash or a full disk never
/// leaves a half-written file. Nothing is written when the file already holds `text`.
pub fn write_atomic(path: &Path, text: &str) -> io::Result<()> {
    if std::fs::read(path).is_ok_and(|old| old == text.as_bytes()) {
        return Ok(());
    }
    let name = path.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a file path"))?;
    let tmp = path.with_file_name(format!(".{}.{}.tmp", name.to_string_lossy(), std::process::id()));
    let written = std::fs::File::create(&tmp).and_then(|mut f| {
        f.write_all(text.as_bytes())?;
        f.sync_all()
    });
    match written.and_then(|()| std::fs::rename(&tmp, path)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |k| vars.iter().find(|(n, _)| *n == k).map(|(_, v)| OsString::from(v))
    }

    #[test]
    fn folders_per_system() {
        let win = config_dir_for("windows", env(&[("APPDATA", r"C:\Users\u\AppData\Roaming")])).unwrap();
        assert!(win.ends_with(APP_FOLDER));
        assert!(win.starts_with(r"C:\Users\u\AppData\Roaming"));
        let win = config_dir_for("windows", env(&[("USERPROFILE", r"C:\Users\u")])).unwrap();
        assert!(win.starts_with(r"C:\Users\u") && win.ends_with(Path::new("Roaming").join(APP_FOLDER)));
        let linux = config_dir_for("linux", env(&[("XDG_CONFIG_HOME", "/home/u/.cfg"), ("HOME", "/home/u")]));
        assert_eq!(linux, Some(Path::new("/home/u/.cfg").join(APP_FOLDER)));
        let linux = config_dir_for("linux", env(&[("HOME", "/home/u")]));
        assert_eq!(linux, Some(Path::new("/home/u").join(".config").join(APP_FOLDER)));
        // a relative XDG_CONFIG_HOME is ignored (the specification says so)
        let linux = config_dir_for("linux", env(&[("XDG_CONFIG_HOME", "cfg"), ("HOME", "/home/u")]));
        assert_eq!(linux, Some(Path::new("/home/u").join(".config").join(APP_FOLDER)));
        let mac = config_dir_for("macos", env(&[("HOME", "/Users/u")]));
        assert_eq!(mac, Some(Path::new("/Users/u").join("Library").join("Application Support").join(APP_FOLDER)));
        assert_eq!(config_dir_for("android", env(&[("HOME", "/data")])), None);
        assert_eq!(config_dir_for("linux", env(&[])), None);
    }

    #[test]
    fn atomic_writes() {
        let dir = std::env::temp_dir().join(format!("mcui-dirs-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.json");
        write_atomic(&f, "one").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "one");
        write_atomic(&f, "two").unwrap();
        write_atomic(&f, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "two");
        // no temporary file is left behind
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        assert!(write_atomic(&dir.join("missing").join("b.json"), "x").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
