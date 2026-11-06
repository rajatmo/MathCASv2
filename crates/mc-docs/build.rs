// Embeds the documentation pages (`docs/functions/` at the repository root: every `.mcn` page and the images in
// its `<name>.files/` folder) into the program, so the documentation window needs no files next to it, and the
// application icon into the Windows executable.
use std::path::{Path, PathBuf};

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/functions");
    println!("cargo:rerun-if-changed=../../docs/functions");
    let mut files = vec![];
    collect(&root, &root, &mut files);
    files.sort();
    let mut out = String::from(
        "/// The embedded files of `docs/functions/`: their paths relative to it (with `/`) and contents.\n",
    );
    out.push_str("pub static FILES: &[(&str, &[u8])] = &[\n");
    for (rel, abs) in &files {
        out.push_str(&format!("    ({rel:?}, include_bytes!({:?})),\n", abs.to_string_lossy()));
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR is set by cargo")).join("embedded.rs");
    std::fs::write(dest, out).expect("the build folder is writable");

    println!("cargo:rerun-if-changed=../../assets/app/icon.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/app/icon.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=application icon not embedded: {e}");
        }
    }
}

/// The pages (`*.mcn`) and images (`*.png`) under `dir`, as (path relative to `root`, absolute path).
fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            collect(root, &path, out);
            continue;
        }
        let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("");
        if !matches!(ext, "mcn" | "png") {
            continue;
        }
        if let Ok(rel) = path.strip_prefix(root) {
            let rel = rel.to_string_lossy().replace('\\', "/");
            out.push((rel, path.clone()));
        }
    }
}
