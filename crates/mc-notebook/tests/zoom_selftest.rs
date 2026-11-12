//! Calm magnification (round 13), driven through the window's self-test (`tests/selftest/zoom.txt`): one rung of
//! the 10 % ladder per wheel notch, bursts and touchpad deltas in whole steps, Ctrl+= / Ctrl+- / Ctrl+0, and the cell
//! under the pointer kept under it. It opens a window, so it is ignored by default:
//! `cargo test -p mc-notebook --test zoom_selftest -- --ignored`.
use std::path::PathBuf;
use std::process::Command;

#[test]
#[ignore = "opens a window"]
fn ctrl_wheel_magnification_is_calm() {
    let dir = std::env::temp_dir().join(format!("mcnb-zoom-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("config")).unwrap();
    let script = dir.join("zoom.txt");
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("selftest").join("zoom.txt");
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
    println!("{log}");
    // "anchor: zoom 1.10 offset 3012.0 cell 57 at 0.412"
    let probes: Vec<(f32, usize, f32)> = log
        .lines()
        .filter_map(|l| l.strip_prefix("anchor: zoom "))
        .map(|l| {
            let w: Vec<&str> = l.split_whitespace().collect();
            (w[0].parse().unwrap(), w[4].parse().unwrap(), w[6].parse().unwrap())
        })
        .collect();
    let zooms: Vec<f32> = probes.iter().map(|p| p.0).collect();
    assert_eq!(zooms, [1.0, 1.1, 1.6, 1.8, 1.5, 1.0, 1.1, 1.0], "{log}");
    let (cell, frac) = (probes[0].1, probes[0].2);
    for &(z, c, f) in &probes {
        assert_eq!(c, cell, "the cell under the pointer at {z}: {log}");
        assert!((f - frac).abs() < 0.06, "the point in the cell at {z}: {f} against {frac}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
