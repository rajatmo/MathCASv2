//! `mc-repl --export` on the built binary: a saved notebook converted to HTML and PDF without evaluation, and the
//! exit code and message of a failed export.
use std::path::PathBuf;
use std::process::Command;

/// A fresh folder for one test.
fn folder(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mc-repl-export-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Exit code and standard error of `mc-repl` with these arguments.
fn run(args: &[&std::ffi::OsStr]) -> (i32, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_mc-repl")).args(args).output().expect("mc-repl runs");
    assert!(o.stdout.is_empty(), "{}", String::from_utf8_lossy(&o.stdout));
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stderr).to_string())
}

#[test]
fn export_writes_html_and_pdf_without_evaluating() {
    let dir = folder("ok");
    let nb = dir.join("nb.mcn");
    // the output on file is shown as saved: the cell is not run again
    std::fs::write(&nb, "#! mathcas 2 notebook\n\n#% title\n# Exported\n\n#% cell\n1 + 1\n#% output\n#= 3/4\n").unwrap();
    let html = dir.join("nb.html");
    let (code, err) = run(&["--export".as_ref(), nb.as_os_str(), html.as_os_str()]);
    assert_eq!((code, err.as_str()), (0, ""));
    let h = std::fs::read_to_string(&html).unwrap();
    assert!(h.contains("<h1>Exported</h1>") && h.contains("<mfrac>"), "{h}");
    let pdf = dir.join("nb.pdf");
    let (code, err) = run(&["--export".as_ref(), nb.as_os_str(), pdf.as_os_str()]);
    assert_eq!((code, err.as_str()), (0, ""));
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF-"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_export_says_why() {
    let dir = folder("bad");
    let nb = dir.join("nb.mcn");
    std::fs::write(&nb, "#! mathcas 2 notebook\n").unwrap();
    let (code, err) = run(&["--export".as_ref(), nb.as_os_str(), dir.join("nb.docx").as_os_str()]);
    assert_eq!(code, 1);
    assert!(err.contains("unknown export format"), "{err}");
    let (code, err) = run(&["--export".as_ref(), dir.join("none.mcn").as_os_str(), dir.join("x.pdf").as_os_str()]);
    assert_eq!(code, 1);
    assert!(err.starts_with("mc-repl: ") && err.contains("none.mcn"), "{err}");
    let (code, err) = run(&["--export".as_ref(), nb.as_os_str()]);
    assert_eq!(code, 1);
    assert!(err.contains("--export needs a notebook and an output file"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}
