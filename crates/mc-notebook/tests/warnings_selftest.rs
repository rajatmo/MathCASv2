//! Round 20, through the window's self-test (`tests/selftest/warnings.txt`): `x(x - a)` typed in a
//! cell gets a warning, its quick fix inserts the `*`; a product and real calls get none; F1 in an option's value
//! asks for `plot#plot_range`. It opens a window, so it is ignored by default:
//! `cargo test -p mc-notebook --test warnings_selftest -- --ignored`.
use std::path::PathBuf;
use std::process::Command;

#[test]
#[ignore = "opens a window"]
fn calls_of_variables_are_marked_and_fixed() {
    let dir = std::env::temp_dir().join(format!("mcnb-warnings-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("config")).unwrap();
    let script = dir.join("warnings.txt");
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("selftest").join("warnings.txt");
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
    let w: Vec<&str> = log.lines().filter(|l| l.starts_with("warnings: ")).collect();
    assert_eq!(w.len(), 3, "{log}");
    assert_eq!(w[0], "warnings: 1 [`x`: x is called as a function; did you mean x*(x - a)?]", "{log}");
    assert_eq!(w[1], "warnings: 0 []", "the fix inserted the `*`: {log}");
    assert_eq!(w[2], "warnings: 0 []", "{log}");
    assert!(log.lines().any(|l| l == "fixwarning: true"), "{log}");
    let help = log.lines().find(|l| l.starts_with("help: ")).unwrap();
    assert!(help.contains("docs [\"plot#plot_range\"]"), "{log}");
    assert!(dir.join("warning.png").exists());
}
