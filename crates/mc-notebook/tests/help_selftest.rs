//! The notebook's help while writing code, driven through the window's self-test (`tests/selftest/help.txt`):
//! completion (opening, accepting with the bracket, cancelling, Enter not stolen), the usage hint (argument
//! highlighted, Esc), F1, hover and Ctrl+click, in plain text and inside a square root box. It opens a window, so it
//! is ignored by default: `cargo test -p mc-notebook --test help_selftest -- --ignored`.
use std::path::PathBuf;
use std::process::Command;

#[test]
#[ignore = "opens a window"]
fn help_while_writing_code() {
    let dir = std::env::temp_dir().join(format!("mcnb-help-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("config")).unwrap();
    let script = dir.join("help.txt");
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("selftest").join("help.txt");
    std::fs::copy(&src, &script).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_mc-notebook"))
        .env("MCNB_SELFTEST", &script)
        .env("MCNB_CONFIG_DIR", dir.join("config"))
        .env("MCUI_CLIPBOARD", "local")
        .env("MATHCAS_DOCS", "-")
        .status()
        .expect("start the notebook");
    assert!(status.success(), "{status}");
    let log = std::fs::read_to_string(dir.join("selftest.log")).expect("the self-test log");
    let help: Vec<&str> = log.lines().filter(|l| l.starts_with("help: ")).collect();
    let want: [&[&str]; 12] = [
        &["completion open", "first [\"n_integrate\"", "hint none"],
        &["completion closed", "hint n_integrate(f, [x, a, b]) argument 1 `f`"],
        &["hint n_integrate(f, [x, a, b]) argument 2 `[x, a, b]`"],
        &["completion closed; hint none"],
        &["docs [\"n_integrate\"]"],
        &["completion open", "selected `abs`"],
        &["completion closed; hint none"],
        &["completion closed; hint table("],
        &["hover n_integrate(f, [x, a, b]): gives a numerical approximation"],
        &["docs [\"n_integrate\", \"n_integrate\"]"],
        &["completion open", "first [\"n_integrate\"]"],
        &["hint n_integrate(f, [x, a, b]) argument 2 `[x, a, b]`"],
    ];
    assert_eq!(help.len(), 12, "help lines:\n{}", help.join("\n"));
    for (k, (line, parts)) in help.iter().zip(want).enumerate() {
        for p in parts {
            assert!(line.contains(p), "help line {} lacks `{p}`:\n{line}", k + 1);
        }
    }
    let typed = std::fs::read_to_string(dir.join("typed.mcn")).expect("the dump");
    assert!(typed.contains("y = n_integrate(sin(x)^2, )\nab\ntable()"), "{typed}");
    let _ = std::fs::remove_dir_all(&dir);
}
