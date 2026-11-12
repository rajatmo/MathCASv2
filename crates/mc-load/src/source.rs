//! Where source text comes from: [`Origin`] (a name for messages and the directory imports are looked up in) and
//! [`SourceProvider`], the only way the loader reads files ([`FileSystem`] for the real thing, [`InMemory`] for
//! tests).
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The origin of a source text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    /// The name in messages: a file path as given, or `<repl>`, `<-e>`, `<input>`.
    pub name: String,
    /// The directory `import` looks for module files in first (decision F.5); the empty path is the current
    /// directory. `None` for embedded text (the prelude and the standard modules), whose imports go to the
    /// standard library only.
    pub dir: Option<PathBuf>,
}

impl Origin {
    /// An origin with this name and import directory.
    pub fn new(name: impl Into<String>, dir: Option<PathBuf>) -> Origin {
        Origin { name: name.into(), dir }
    }

    /// Input typed at the REPL: imports relative to the current directory.
    pub fn repl() -> Origin {
        Origin::new("<repl>", Some(PathBuf::new()))
    }

    /// Source given on the command line with `-e`: imports relative to the current directory.
    pub fn command_line() -> Origin {
        Origin::new("<-e>", Some(PathBuf::new()))
    }

    /// Source passed to `Engine::eval` (the benchmark runner): imports relative to the current directory.
    pub fn input() -> Origin {
        Origin::new("<input>", Some(PathBuf::new()))
    }

    /// Text embedded in the binary, named `name` in messages.
    pub fn embedded(name: impl Into<String>) -> Origin {
        Origin::new(name, None)
    }

    /// The file at `path`: named by the path as given, importing from its directory.
    pub fn file(path: &Path) -> Origin {
        Origin::new(path.display().to_string(), Some(path.parent().map(Path::to_path_buf).unwrap_or_default()))
    }
}

/// A source text and its origin.
#[derive(Clone, Debug)]
pub struct Source {
    /// where the text comes from
    pub origin: Origin,
    /// the text
    pub text: String,
}

/// Reads source files for the loader.
pub trait SourceProvider: Send + Sync {
    /// The source of module `name`: the file `name.mc` in `dir`, or `None` if there is no such file.
    fn module(&self, name: &str, dir: &Path) -> io::Result<Option<Source>>;

    /// The text of the file at `path`.
    fn file(&self, path: &Path) -> io::Result<String>;
}

/// The file name of module `name`.
fn module_file(name: &str, dir: &Path) -> PathBuf {
    dir.join(format!("{name}.mc"))
}

/// Text without a leading byte order mark (editors on some platforms write one).
fn without_bom(text: String) -> String {
    match text.strip_prefix('\u{feff}') {
        Some(rest) => rest.to_string(),
        None => text,
    }
}

/// The file system.
#[derive(Clone, Copy, Debug, Default)]
pub struct FileSystem;

impl SourceProvider for FileSystem {
    fn module(&self, name: &str, dir: &Path) -> io::Result<Option<Source>> {
        let path = module_file(name, dir);
        match std::fs::read_to_string(&path) {
            Ok(text) => Ok(Some(Source { origin: Origin::file(&path), text: without_bom(text) })),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn file(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path).map(without_bom)
    }
}

/// Files held in memory, by path (for tests). Clones share the files, so a test can change a file after giving
/// the provider to a loader.
#[derive(Clone, Debug, Default)]
pub struct InMemory {
    files: Arc<Mutex<HashMap<PathBuf, String>>>,
}

impl InMemory {
    /// No files.
    pub fn new() -> InMemory {
        InMemory::default()
    }

    /// These files plus one more.
    pub fn with(self, path: impl Into<PathBuf>, text: impl Into<String>) -> InMemory {
        self.insert(path, text);
        self
    }

    /// Add or replace a file.
    pub fn insert(&self, path: impl Into<PathBuf>, text: impl Into<String>) {
        self.lock().insert(path.into(), text.into());
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, String>> {
        // a panic while holding the lock leaves the map itself intact
        self.files.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl SourceProvider for InMemory {
    fn module(&self, name: &str, dir: &Path) -> io::Result<Option<Source>> {
        let path = module_file(name, dir);
        Ok(self.lock().get(&path).map(|text| Source { origin: Origin::file(&path), text: text.clone() }))
    }

    fn file(&self, path: &Path) -> io::Result<String> {
        self.lock().get(path).cloned().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such file"))
    }
}
