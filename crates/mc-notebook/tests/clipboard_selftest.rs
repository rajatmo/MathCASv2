//! Clipboard with boxes (round 13), driven through the window's self-test (`tests/selftest/clipboard.txt`): a
//! fraction copied as linear text and boxes, Copy as plain text, pasting into a plain cell of another tab, into a cell
//! with boxes and at an insertion point, cutting and pasting a power. It opens a window, so it is ignored by default:
//! `cargo test -p mc-notebook --test clipboard_selftest -- --ignored`.
use std::path::PathBuf;
use std::process::Command;

#[test]
#[ignore = "opens a window"]
fn clipboard_with_boxes() {
    let dir = std::env::temp_dir().join(format!("mcnb-clipboard-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("config")).unwrap();
    let script = dir.join("clipboard.txt");
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("selftest").join("clipboard.txt");
    std::fs::copy(&src, &script).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_mc-notebook"))
        .env("MCNB_SELFTEST", &script)
        .env("MCNB_CONFIG_DIR", dir.join("config"))
        .env("MCUI_CLIPBOARD", "local")
        .status()
        .expect("start the notebook");
    assert!(status.success(), "{status}");
    let log = std::fs::read_to_string(dir.join("selftest.log")).expect("the self-test log");
    println!("{log}");
    let lines: Vec<&str> = log.lines().filter(|l| l.starts_with("clipboard: ") || l.starts_with("boxes: ")).collect();
    let want = [
        "clipboard: Some(\"a/b\") boxes \"((a)/(b))\"",
        "clipboard: Some(\"a/b\")",
        "boxes: input * x+(a/b)▮",
        "boxes: input * x+(a/b)*(a/b)▮",
        "boxes: input * (a/b)▮",
        "clipboard: Some(\"y^2\") boxes \"((y)^(2))\"",
        "boxes: input * z+(y^2)▮",
    ];
    assert_eq!(lines.len(), want.len(), "lines:\n{}", lines.join("\n"));
    for (got, want) in lines.iter().zip(want) {
        // `*` stands for the cell id
        let (head, tail) = want.split_once('*').map_or((want, ""), |(h, t)| (h, t.trim_start()));
        let ok = got.starts_with(head) && got.ends_with(tail);
        assert!(ok, "expected `{want}`, got `{got}`");
    }
    let pasted = std::fs::read_to_string(dir.join("pasted.mcn")).expect("the dump");
    assert!(pasted.contains("#% cell typeset\nx+((a)/(b))*((a)/(b))\n"), "{pasted}");
    assert!(pasted.contains("#% cell typeset\n((a)/(b))\n"), "{pasted}");
    let cut = std::fs::read_to_string(dir.join("cut.mcn")).expect("the dump");
    assert!(cut.contains("#% cell typeset\nz+((y)^(2))\n"), "{cut}");
    let _ = std::fs::remove_dir_all(&dir);
}
