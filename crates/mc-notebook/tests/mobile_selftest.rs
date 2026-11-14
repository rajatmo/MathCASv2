//! The layout for phones and tablets, driven through the window's self-test (`tests/selftest/mobile.txt`) on this
//! system with `MCNB_LAYOUT=mobile`: the Home screen, a new calculation evaluated with the kernel, the notebook
//! written by itself under `Calculation 1.mcn` when it is left, and opened again from Home. It opens a window and
//! needs `mc-kernel` built beside the notebook, so it is ignored by default:
//! `cargo test -p mc-notebook --test mobile_selftest -- --ignored`.
use std::path::PathBuf;
use std::process::Command;

#[test]
#[ignore = "opens a window"]
fn a_calculation_is_made_kept_and_opened_again() {
    let dir = std::env::temp_dir().join(format!("mcnb-mobile-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let docs = dir.join("notebooks");
    std::fs::create_dir_all(dir.join("config")).unwrap();
    std::fs::create_dir_all(&docs).unwrap();
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("selftest").join("mobile.txt");
    let text = std::fs::read_to_string(&src).unwrap().replace("@DOCS@", &docs.display().to_string().replace('\\', "/"));
    let script = dir.join("mobile.txt");
    std::fs::write(&script, text).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_mc-notebook"))
        .env("MCNB_LAYOUT", "mobile")
        .env("MCNB_DOCUMENTS", &docs)
        .env("MCNB_SELFTEST", &script)
        .env("MCNB_CONFIG_DIR", dir.join("config"))
        .env("MCUI_CLIPBOARD", "local")
        .env("MATHCAS_DOCS", "-")
        .status()
        .expect("start the notebook");
    assert!(status.success(), "{status}");
    let log = std::fs::read_to_string(dir.join("selftest.log")).expect("the self-test log");
    println!("{log}");
    let states: Vec<&str> = log.lines().filter_map(|l| l.strip_prefix("mobile: ")).collect();
    assert_eq!(
        states,
        [
            // the start screen
            "home",
            // a new calculation: no file yet, the keys show
            "notebook - keys",
            // evaluated: the file exists (written within two seconds or when the notebook is left)
            states[2],
            "home",
            // opened again from the list
            "notebook Calculation 1.mcn add",
        ],
        "{log}"
    );
    assert!(states[2] == "notebook - add" || states[2] == "notebook Calculation 1.mcn add", "{log}");
    assert!(log.contains("result mobile/integral: pass"), "{log}");
    let saved = std::fs::read_to_string(docs.join("Calculation 1.mcn")).expect("the notebook's file");
    assert!(saved.contains("integrate(") && saved.contains("#% output"), "{saved}");
    for shot in ["home", "typing", "result", "home-with-notebook", "reopened-dark", "tablet"] {
        assert!(dir.join(format!("{shot}.png")).is_file(), "screenshot {shot}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
