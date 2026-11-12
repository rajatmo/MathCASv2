//! Where the pages come from: the copy embedded in the program at build time (`docs/functions/` of the
//! repository), or a folder on disk (`MCDOCS_DIR`, for page authors, who then see their edits without a rebuild).
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/embedded.rs"));
}

/// The page files: paths relative to the pages folder (with `/`) and their contents.
#[derive(Clone, Debug, Default)]
pub struct Library {
    files: BTreeMap<String, Vec<u8>>,
    /// the folder the files were read from (`None`: embedded)
    pub dir: Option<PathBuf>,
}

impl Library {
    /// The pages embedded in the program.
    pub fn embedded() -> Library {
        let files = embedded::FILES.iter().map(|(p, b)| (p.to_string(), b.to_vec())).collect();
        Library { files, dir: None }
    }

    /// The pages in `dir` (`.mcn` files and the pictures of their `.files` folders: `.svgz` drawings and `.png` raster images).
    pub fn from_dir(dir: &Path) -> Library {
        let mut files = BTreeMap::new();
        read_into(&mut files, dir, "");
        Library { files, dir: Some(dir.to_path_buf()) }
    }

    /// Add the guides in `dir` (stored under `guides/`).
    pub fn with_guides_from(mut self, dir: &Path) -> Library {
        self.files.retain(|p, _| !p.starts_with("guides/"));
        read_into(&mut self.files, dir, "guides/");
        self
    }

    /// The folder named by `MCDOCS_DIR`, else the embedded pages. The guides come from `MCDOCS_GUIDES`, else from
    /// the `guides` folder beside `MCDOCS_DIR`, else stay the embedded ones.
    pub fn load() -> Library {
        let Some(d) = std::env::var_os("MCDOCS_DIR").filter(|d| Path::new(d).is_dir()) else { return Library::embedded() };
        let mut lib = Library::from_dir(Path::new(&d));
        let beside = Path::new(&d).join("..").join("guides");
        let guides = std::env::var_os("MCDOCS_GUIDES").map(PathBuf::from).unwrap_or(beside);
        if guides.is_dir() {
            lib = lib.with_guides_from(&guides);
        } else {
            for (p, b) in embedded::FILES.iter().filter(|(p, _)| p.starts_with("guides/")) {
                lib.files.insert(p.to_string(), b.to_vec());
            }
        }
        lib
    }

    /// The stems (`<NN>-<slug>`) of the topic guides, in order.
    pub fn guide_stems(&self) -> Vec<String> {
        self.files
            .keys()
            .filter_map(|p| p.strip_prefix("guides/"))
            .filter(|p| !p.contains('/'))
            .filter_map(|p| p.strip_suffix(".mcn"))
            .map(str::to_string)
            .collect()
    }

    /// The text of the guide with file stem `stem`.
    pub fn guide_text(&self, stem: &str) -> Option<String> {
        self.files.get(&format!("guides/{stem}.mcn")).map(|b| String::from_utf8_lossy(b).into_owned())
    }

    /// The names of the pages, sorted.
    pub fn names(&self) -> Vec<String> {
        self.files
            .keys()
            .filter(|p| !p.contains('/'))
            .filter_map(|p| p.strip_suffix(".mcn"))
            .map(str::to_string)
            .collect()
    }

    /// The text of page `name`.
    pub fn page_text(&self, name: &str) -> Option<String> {
        self.files.get(&format!("{name}.mcn")).map(|b| String::from_utf8_lossy(b).into_owned())
    }

    /// The bytes of a file by its relative path.
    pub fn file(&self, rel: &str) -> Option<&[u8]> {
        self.files.get(rel).map(Vec::as_slice)
    }

    /// The files of page `name`: the page and its images, as (relative path, contents).
    pub fn page_files(&self, name: &str) -> Vec<(&str, &[u8])> {
        let page = format!("{name}.mcn");
        let folder = format!("{name}.files/");
        self.files
            .iter()
            .filter(|(p, _)| **p == page || p.starts_with(&folder))
            .map(|(p, b)| (p.as_str(), b.as_slice()))
            .collect()
    }

    /// Write page `name` and its images into `dir` (images in `<name>.files/` beside the page, where its outputs
    /// name them); the path of the page written.
    pub fn copy_page(&self, name: &str, dir: &Path) -> std::io::Result<PathBuf> {
        for (rel, bytes) in self.page_files(name) {
            let path = dir.join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, bytes)?;
        }
        Ok(dir.join(format!("{name}.mcn")))
    }
}

/// Gzip data, decompressed.
pub(crate) fn gunzip(data: &[u8]) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(data).read_to_end(&mut out).ok()?;
    Some(out)
}

/// Read the `.mcn`, `.svgz` and `.png` files under `dir` into `files`, their relative paths prefixed with `prefix`.
fn read_into(files: &mut BTreeMap<String, Vec<u8>>, dir: &Path, prefix: &str) {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else if matches!(path.extension().and_then(|x| x.to_str()), Some("mcn" | "png" | "svgz")) {
                if let (Ok(rel), Ok(bytes)) = (path.strip_prefix(dir), std::fs::read(&path)) {
                    files.insert(format!("{prefix}{}", rel.to_string_lossy().replace('\\', "/")), bytes);
                }
            }
        }
    }
}

/// The repository's topic guides folder (`docs/guides/`).
pub fn guides_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("docs").join("guides")
}

/// The repository's pages folder seen from this crate (for the tests and the page tool run from the repository).
pub fn source_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("docs").join("functions")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_copied_with_its_images() {
        let lib = Library::from_dir(&source_dir());
        let dir = std::env::temp_dir().join(format!("mcdocs-copy-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let page = lib.copy_page("plot", &dir).expect("copied");
        assert_eq!(std::fs::read_to_string(&page).ok(), lib.page_text("plot"));
        // a plot is its drawing, as stored: no PNG, no second copy
        assert_eq!(std::fs::read(dir.join("plot.files").join("plot-1.svgz")).ok().as_deref(), lib.file("plot.files/plot-1.svgz"));
        assert!(!dir.join("plot.files").join("plot-1.png").exists());
        assert!(crate::library::gunzip(lib.file("plot.files/plot-1.svgz").unwrap()).is_some_and(|s| s.starts_with(b"<svg") || s.starts_with(b"<?xml")));
        assert!(!dir.join("d.mcn").exists());
        let _ = std::fs::remove_dir_all(&dir);
        assert!(lib.names().contains(&"plot".to_string()));
        assert!(lib.file("plot.files/plot-1.svgz").is_some());
    }
}
