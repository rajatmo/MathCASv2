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

    /// The pages in `dir` (`.mcn` files and the `.png` files of their `.files` folders).
    pub fn from_dir(dir: &Path) -> Library {
        let mut files = BTreeMap::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&d) else { continue };
            for e in entries.flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else if matches!(path.extension().and_then(|x| x.to_str()), Some("mcn" | "png")) {
                    if let (Ok(rel), Ok(bytes)) = (path.strip_prefix(dir), std::fs::read(&path)) {
                        files.insert(rel.to_string_lossy().replace('\\', "/"), bytes);
                    }
                }
            }
        }
        Library { files, dir: Some(dir.to_path_buf()) }
    }

    /// The folder named by `MCDOCS_DIR`, else the embedded pages.
    pub fn load() -> Library {
        match std::env::var_os("MCDOCS_DIR") {
            Some(d) if Path::new(&d).is_dir() => Library::from_dir(Path::new(&d)),
            _ => Library::embedded(),
        }
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
        assert!(dir.join("plot.files").join("plot-1.png").is_file());
        assert!(!dir.join("d.mcn").exists());
        let _ = std::fs::remove_dir_all(&dir);
        assert!(lib.names().contains(&"plot".to_string()));
        assert!(lib.file("plot.files/plot-1.png").is_some());
    }
}
