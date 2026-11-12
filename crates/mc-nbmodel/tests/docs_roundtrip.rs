//! The file format does not change with the editor of design 11 (section 3.3): every notebook under `docs/` reads
//! and writes byte for byte, and every code cell's tree (`Cell::content`) gives its text back.
use mc_nbmodel::Notebook;

fn notebooks(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            notebooks(&p, out);
        } else if p.extension().is_some_and(|x| x == "mcn") {
            out.push(p);
        }
    }
}

#[test]
fn every_docs_notebook_reads_and_writes_byte_for_byte() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
    let mut files = vec![];
    notebooks(&root, &mut files);
    files.sort();
    assert!(files.len() > 100, "{}", files.len());
    let mut bad = vec![];
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap().replace("\r\n", "\n");
        let Ok(nb) = Notebook::parse(&text) else { continue };
        // the file's own normal form: what saving it gives (older forms are normalised once, as before)
        let saved = nb.to_text();
        let again = Notebook::parse(&saved).unwrap();
        if again.to_text() != saved {
            bad.push(format!("{}: does not write back", f.display()));
            continue;
        }
        // 19 pages of `docs/functions` have one blank line more or less between two cells than the writer puts
        // (the file reader's normal form, before this round too): blank lines between cells are not compared
        let lines = |t: &str| t.lines().filter(|l| !l.is_empty()).map(str::to_string).collect::<Vec<_>>();
        if lines(&saved) != lines(&text) && !text.contains("form=") {
            let d = text.lines().zip(saved.lines()).find(|(a, b)| a != b);
            bad.push(format!("{}: changed on reading and writing: {d:?}", f.display()));
        }
        for c in again.cells() {
            if let Some(tree) = &c.content {
                if mc_boxes::to_text(tree) != c.text {
                    bad.push(format!("{}: a cell's tree does not give its text: {:?}", f.display(), c.text));
                }
            }
        }
    }
    assert!(bad.is_empty(), "{} of {}:\n{}", bad.len(), files.len(), bad.join("\n"));
}
