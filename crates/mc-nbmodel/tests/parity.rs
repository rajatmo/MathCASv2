//! Plain-text agreement with the old editor (design 11, section 5): every key script in `tests/keys/*.keys` without boxes or aliases ends,
//! with the editor of design 11, in the notebook, selection and clipboard the editor of rounds 9 to 33 gave. Those
//! results were recorded from the old editor (round 34, before it was deleted) in `tests/parity/old-editor.txt`:
//! typing, word and line motions, automatic closers, indentation and comments in cells without boxes give the texts
//! the old editor gave.
use mc_nbmodel::script::{parse_scripts, run_final};

/// Whether a script involves boxes or aliases (its keys or notebooks): the old box editor parked its caret at the
/// text's end and an alias was a character of the text, so those results differ on purpose.
fn involves_boxes(text: &str) -> bool {
    ["typeset", "Ctrl+/", "Ctrl+6", "Ctrl+^", "Ctrl+2", "Ctrl+@", "Ctrl+5", "Ctrl+%", "Ctrl+Shift+V", "Esc", "TypesetOutputs"]
        .iter()
        .any(|k| text.contains(k))
}

/// Every compared script's result: `=== file / title`, the notebook, `--- clip`, the clipboard.
fn results() -> String {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/keys");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let mut out = String::new();
    for f in files.iter().filter(|f| f.extension().is_some_and(|e| e == "keys")) {
        let text = std::fs::read_to_string(f).unwrap();
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        for s in parse_scripts(&text).unwrap() {
            let all = format!("{}\n{}\n{}", s.before, s.keys, s.after.clone().unwrap_or_default());
            if involves_boxes(&all) {
                continue;
            }
            let (nb, clip) = run_final(&s).unwrap();
            out.push_str(&format!("=== {name} / {}\n{nb}--- clip\n{clip}\n", s.title));
        }
    }
    out
}

#[test]
fn plain_cells_behave_as_with_the_old_editor() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parity/old-editor.txt");
    let want = std::fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
    let got = results();
    let split = |t: &str| t.split("=== ").filter(|b| !b.is_empty()).map(str::to_string).collect::<Vec<_>>();
    let (w, g) = (split(&want), split(&got));
    assert!(w.len() > 150, "{}", w.len());
    let differ: Vec<String> = w.iter().zip(&g).filter(|(a, b)| a != b).map(|(a, b)| format!("old:\n{a}\nnew:\n{b}")).collect();
    assert_eq!(w.len(), g.len(), "the compared scripts changed");
    assert!(differ.is_empty(), "{} differ:\n\n{}", differ.len(), differ.join("\n\n"));
}

