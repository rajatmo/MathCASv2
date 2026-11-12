//! Round 21, through the window's self-test (`tests/selftest/guides.txt`): the home page lists the
//! guides and a click on one opens it. It opens a window, so it is ignored by default:
//! `cargo test -p mc-docs --test guides_selftest -- --ignored`.
use std::path::PathBuf;
use std::process::Command;

#[test]
#[ignore = "opens a window"]
fn guides_are_one_click_from_the_start_page() {
    let dir = std::env::temp_dir().join(format!("mcdocs-guides-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("config")).unwrap();
    let script = dir.join("guides.txt");
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("selftest").join("guides.txt");
    std::fs::copy(&src, &script).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_mc-docs"))
        .env("MCDOCS_SELFTEST", &script)
        .env("MCNB_CONFIG_DIR", dir.join("config"))
        .status()
        .expect("start the documentation window");
    assert!(status.success(), "{status}");
    let log = std::fs::read_to_string(dir.join("selftest.log")).expect("the self-test log");
    assert!(log.contains("at topic:calculus"), "{log}");
    assert!(log.contains("at topic:plotting-and-graphics"), "{log}");
    // round 23: guides are searched
    assert!(log.contains("guides [\"transforms-and-series\""), "{log}");
    assert!(log.contains("at topic:transforms-and-series"), "{log}");
    assert!(dir.join("home.png").exists() && dir.join("plotting.png").exists());
    eprintln!("{log}\nscreenshots in {}", dir.display());
}
