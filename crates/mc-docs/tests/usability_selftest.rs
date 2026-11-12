//! The documentation window in use (round 19), through its self-test (`tests/selftest/usability.txt`):
//! forgiving search (underscores ignored, a typo, a misspelt word of a description) under a few milliseconds a
//! keystroke, the copy button of an example input, and a page with typeset outputs found by find-in-page, in both
//! themes. It opens a window, so it is ignored by default: `cargo test -p mc-docs --test usability_selftest -- --ignored`.
use std::path::PathBuf;
use std::process::Command;

#[test]
#[ignore = "opens a window"]
fn search_copy_and_typeset_outputs() {
    let dir = std::env::temp_dir().join(format!("mcdocs-usability-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("config")).unwrap();
    let script = dir.join("usability.txt");
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("selftest").join("usability.txt");
    std::fs::copy(&src, &script).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_mc-docs"))
        .env("MCDOCS_SELFTEST", &script)
        .env("MCNB_CONFIG_DIR", dir.join("config"))
        .env("MCUI_CLIPBOARD", "local")
        .status()
        .expect("start the documentation window");
    assert!(status.success(), "{status}");
    let log = std::fs::read_to_string(dir.join("selftest.log")).expect("the self-test log");
    let line = |start: &str| log.lines().find(|l| l.contains(start)).unwrap_or_else(|| panic!("no `{start}`:\n{log}"));
    assert!(line("typesearch nintegrate:").contains("first [\"n_integrate (Exact)\""), "{log}");
    assert!(line("typesearch intgrate:").contains("first [\"integrate (Similar)\""), "{log}");
    assert!(line("typesearch numericaly:").contains("results, first [\""), "{log}");
    assert!(!line("typesearch numericaly:").contains(": at search, 0 results"), "{log}"); // not `contains("0 results")`: "170 results" matched it (round 21)
    assert!(line("typesearch polynomial roots:").contains("(Summary)"), "{log}");
    // every keystroke's search well under a frame
    for l in log.lines().filter(|l| l.starts_with("typesearch `")) {
        let max: f64 = l.rsplit("max ").next().and_then(|m| m.trim_end_matches(" ms").parse().ok()).unwrap();
        assert!(max < 10.0, "{l}");
    }
    assert!(line("copy button 1 of").contains("copies \"table("), "{log}");
    assert!(line("clipboard:").contains("Some(\"table("), "{log}");
    assert!(line("find x^2:").contains("page:expand"), "{log}");
    assert!(log.lines().any(|l| l.contains("wait 3: at page:expand, find `x^2` 1/")), "{log}");
    assert!(dir.join("expand-find.png").exists() && dir.join("expand-dark.png").exists());
    let _ = std::fs::remove_dir_all(&dir);
}
