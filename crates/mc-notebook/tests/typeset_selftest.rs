//! Typeset math in the window (round 19), driven through the self-test (`tests/selftest/typeset.txt`):
//! building a fraction, an n-th root and a power of a root with the box keys; clicks into a denominator, a radicand
//! and an index; a double-click and a drag across box boundaries; copying the boxes and pasting them into another
//! cell, undo and redo of the paste; another program's text pasted as text (Ctrl+V) and as math (Ctrl+Shift+V,
//! the identical structure); a documentation example pasted as math; Backspace and undo; screenshots in both
//! themes and at 150 %. It opens a window, so it is ignored by default:
//! `cargo test -p mc-notebook --test typeset_selftest -- --ignored`.
use std::path::PathBuf;
use std::process::Command;

#[test]
#[ignore = "opens a window"]
fn typeset_math_in_the_window() {
    let dir = std::env::temp_dir().join(format!("mcnb-typeset-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("config")).unwrap();
    let script = dir.join("typeset.txt");
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("selftest").join("typeset.txt");
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
    let got: Vec<&str> =
        log.lines().filter(|l| l.starts_with("boxes: ") || l.starts_with("clipboard: ")).collect();
    let want = [
        "boxes: input 1 y = (1/x) + √[3](x) + (e^√(t▮))",
        "boxes: input 1 y = (1/▮x) + √[3](x) + (e^√(t))",
        "boxes: input 1 y = (1/x) + √[3](x▮) + (e^√(t))",
        "boxes: input 1 y = (1/x) + √[▮3](x) + (e^√(t))",
        "boxes: input 1 y = (1/x) + √[3](⟦x⟧) + (e^√(t))",
        "boxes: input 1 y = ⟦(1/x) + √[3](x) ⟧+ (e^√(t))",
        "clipboard: Some(\"1/x + surd(x, 3) \") boxes \"((1)/(x)) + ((surd(x, 3))) \"",
        "boxes: input 2 z = (1/x) + √[3](x) ▮",
        "boxes: none",
        "boxes: input 2 z = (1/x) + √[3](x) ▮",
        "boxes: none",
        "boxes: input 4 w = ((x^2)/√(y)) + √[3](a)▮",
        "boxes: input 5 table((2^k), k, 10)▮",
        // Backspace after a box enters it (deleting nothing), again into the root, then deletes `t`; undo
        "boxes: input 1 y = (1/x) + √[3](x) + (e^√(t)▮)",
        "boxes: input 1 y = (1/x) + √[3](x) + (e^√(t▮))",
        "boxes: input 1 y = (1/x) + √[3](x) + (e^√(▮□))",
        "boxes: input 1 y = (1/x) + √[3](x) + (e^√(t▮))",
    ];
    assert_eq!(got, want, "log:\n{log}");
    let typed = std::fs::read_to_string(dir.join("typeset.mcn")).expect("the dump");
    assert!(typed.contains("w = ((((x)^(2)))/(((sqrt(y))))) + ((surd(a, 3)))"), "{typed}");
    for shot in ["t1-built", "t2-dragged", "t3-pasted", "t4-pasted-math", "t5-dark", "t6-dark-150"] {
        assert!(dir.join(format!("{shot}.png")).exists(), "{shot}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
