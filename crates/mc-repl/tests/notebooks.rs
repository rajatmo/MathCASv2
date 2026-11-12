//! `mc-repl notebook.mcn` and `mc-repl --update notebook.mcn` on the built binary: the sample notebook's output
//! and exit code, writing outputs back (idempotent, atomic, line ends kept), errors, and notebooks mixed with
//! `-e` sources in one session.
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).replace("\r\n", "\n")
}

/// Run `mc-repl --threads 2` with these arguments in `dir`: exit code, standard output, standard error.
fn run_in(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_mc-repl"))
        .arg("--threads")
        .arg("2")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("mc-repl runs");
    (o.status.code().unwrap_or(-1), text(&o.stdout), text(&o.stderr))
}

/// A fresh scratch directory for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mc-repl-notebooks-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_sample_notebook() {
    let (code, out, err) = run_in(&root(), &["examples/notebook.mcn"]);
    assert_eq!(code, 2, "the sample shows a failure");
    assert_eq!(
        out,
        "1/2\n1267650600228229401496703205376\nsteps: 111, highest: 9232\n[6, 3, 10, 5, 16, 8, 4, 2, 1]\n[1, 2]\n\
         [109/5, 22]\n"
    );
    assert_eq!(err, "examples/notebook.mcn: failure(division_by_zero)\n");
}

#[test]
fn update_writes_outputs_back_once() {
    let dir = scratch("update");
    let path = dir.join("n.mcn");
    let original = "#! mathcas 2 notebook\r\n\r\n#% cell\r\nk = 3\r\nsay(k^2)\r\n\r\n#% text\r\n# prose\r\n\r\n\
                    #% cell\r\nk + 1\r\n";
    std::fs::write(&path, original).unwrap();
    let (code, out, err) = run_in(&dir, &["--update", "n.mcn"]);
    assert_eq!((code, out.as_str(), err.as_str()), (0, "9\n4\n", ""));
    let written = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        written,
        "#! mathcas 2 notebook\r\n\r\n#% cell\r\nk = 3\r\nsay(k^2)\r\n#% output\r\n#| 9\r\n\r\n#% text\r\n# prose\r\n\r\n\
         #% cell\r\nk + 1\r\n#% output\r\n#= 4\r\n",
        "outputs below their cells, CR LF kept"
    );
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let (code, _, _) = run_in(&dir, &["--update", "n.mcn"]);
    assert_eq!(code, 0);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), written, "idempotent");
    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), modified, "an unchanged file is not written");
    // without --update the file is only read
    std::fs::write(&path, original).unwrap();
    let (code, out, _) = run_in(&dir, &["n.mcn"]);
    assert_eq!((code, out.as_str()), (0, "9\n4\n"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    let names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, ["n.mcn"], "no temporary file is left");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn errors_are_recorded_and_the_run_goes_on() {
    let dir = scratch("errors");
    let path = dir.join("e.mcn");
    std::fs::write(&path, "#! mathcas 2 notebook\n#% cell\n1/0\n#% cell\ny = (\n#% cell\n7\n#% output\n#= 7\n")
        .unwrap();
    let (code, out, err) = run_in(&dir, &["--update", "e.mcn"]);
    // a cell that does not read is an error, but the cells after it still run (gap G77)
    assert_eq!((code, out.as_str()), (1, "7\n"));
    assert!(err.starts_with("e.mcn: failure(division_by_zero)\ne.mcn:7:6: "), "{err}");
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.starts_with("#! mathcas 2 notebook\n#% cell\n1/0\n#% output\n#! failure(division_by_zero)\n"));
    assert!(written.contains("#% cell\ny = (\n#% output\n#! e.mcn:7:6: "), "{written}");
    assert!(written.ends_with("#% cell\n7\n#% output\n#= 7\n"), "the cell after the error ran: {written}");
    // the position is that of the file as written
    assert_eq!(written.lines().nth(6), Some("y = ("));
    // not a notebook, a missing file: nothing runs
    std::fs::write(dir.join("bad.mcn"), "#! mathcas 2\n1\n").unwrap();
    let (code, _, err) = run_in(&dir, &["bad.mcn", "-e", "print(1)"]);
    assert_eq!(code, 1);
    assert_eq!(err, "bad.mcn:1: a notebook starts with the line `#! mathcas 2 notebook`\n");
    let (code, _, err) = run_in(&dir, &["none.mcn"]);
    assert_eq!(code, 1);
    assert!(err.starts_with("none.mcn: cannot read the file"), "{err}");
    // --update needs a notebook
    let (code, _, err) = run_in(&dir, &["--update", "-e", "1"]);
    assert_eq!(code, 1);
    assert!(err.starts_with("mc-repl: --update needs a notebook"), "{err}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn notebooks_and_sources_share_the_session() {
    let dir = scratch("session");
    std::fs::write(dir.join("m.mc"), "#! mathcas 2\nmodule m { pub fn twice(x) = 2*x }\n").unwrap();
    std::fs::write(dir.join("s.mcn"), "#! mathcas 2 notebook\n#% cell\nimport m\ntwice(base)\n").unwrap();
    let (code, out, err) = run_in(&dir, &["-e", "base = 21", "s.mcn", "-e", "m.twice(1)"]);
    assert_eq!((code, out.as_str(), err.as_str()), (0, "42\n2\n", ""));
    std::fs::remove_dir_all(&dir).unwrap();
}
