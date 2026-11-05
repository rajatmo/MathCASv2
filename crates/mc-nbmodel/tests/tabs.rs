//! Runs the tab scripts in `tests/tabs/*.tabs`: tests start with `=== title`, steps as in
//! `mc_nbmodel::tabs::run_tab_script`.
use mc_nbmodel::tabs::run_tab_script;

#[test]
fn tab_scripts() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/tabs");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let (mut passed, mut failed) = (0, vec![]);
    for f in files.iter().filter(|f| f.extension().is_some_and(|e| e == "tabs")) {
        let text = std::fs::read_to_string(f).unwrap();
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        for block in text.split("\n=== ").skip(1) {
            let (title, body) = block.split_once('\n').unwrap_or((block, ""));
            match run_tab_script(body) {
                Ok(()) => passed += 1,
                Err(e) => failed.push(format!("{name} / {title}: {e}")),
            }
        }
    }
    println!("tab scripts: {passed} passed, {} failed", failed.len());
    assert!(failed.is_empty(), "{} failed:\n{}", failed.len(), failed.join("\n"));
    assert!(passed > 0);
}
