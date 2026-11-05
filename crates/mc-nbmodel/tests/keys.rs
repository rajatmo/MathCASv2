//! Runs every key script in `tests/keys/*.keys` (design 9.8) and reports each failing test with the difference.
use mc_nbmodel::script::{parse_scripts, run_script};

#[test]
fn key_scripts() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/keys");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let (mut passed, mut failed) = (0, vec![]);
    for f in files.iter().filter(|f| f.extension().is_some_and(|e| e == "keys")) {
        let text = std::fs::read_to_string(f).unwrap();
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let scripts = parse_scripts(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        for s in scripts {
            match run_script(&s) {
                Ok(()) => passed += 1,
                Err(e) => failed.push(format!("{name} / {}:\n{e}", s.title)),
            }
        }
    }
    println!("key scripts: {passed} passed, {} failed", failed.len());
    assert!(failed.is_empty(), "{} failed:\n\n{}", failed.len(), failed.join("\n\n"));
    assert!(passed > 0);
}
