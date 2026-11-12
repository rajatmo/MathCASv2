//! Round 20, through the window's self-test (`tests/selftest/sections.txt`): `mc-docs plot#plot_range`
//! (F1 on an option in the notebook) opens `plot` at the example giving `plot_range`, and a typeset value is
//! selected by a click and copied with Ctrl+C. It opens a window, so it is ignored by default:
//! `cargo test -p mc-docs --test sections_selftest -- --ignored`.
use std::path::PathBuf;
use std::process::Command;

#[test]
#[ignore = "opens a window"]
fn sections_and_copyable_values() {
    let dir = std::env::temp_dir().join(format!("mcdocs-sections-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("config")).unwrap();
    let script = dir.join("sections.txt");
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("selftest").join("sections.txt");
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
    assert!(line("place plot#plot_range: target").contains("Some(Input("), "{log}");
    assert!(line("place plot#plot_range: at").contains("page:plot"), "{log}");
    let value = line("value 1 of");
    let text = value.split(" is ").nth(1).unwrap();
    assert!(line("clipboard:").contains(&format!("Some({text})")), "{log}");
    assert!(dir.join("plot-range.png").exists() && dir.join("value-selected-dark.png").exists());
}
