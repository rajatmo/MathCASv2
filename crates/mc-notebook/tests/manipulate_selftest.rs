//! Interactive controls (round 13) through the window and a real kernel (`mc-kernel` next to the notebook
//! binary): a `manipulate` cell is evaluated, its sliders are dragged, the frames stay short, and the saved file keeps
//! the positions and the last result (the plot embedded). It opens a window and starts a kernel, so it is ignored by
//! default: `cargo test -p mc-notebook --test manipulate_selftest -- --ignored`.
use std::process::Command;

#[test]
#[ignore = "opens a window and starts a kernel"]
fn sliders_evaluate_and_keep_their_positions() {
    let dir = std::env::temp_dir().join(format!("mcnb-manipulate-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("config")).unwrap();
    let nb = dir.join("m.mcn");
    std::fs::write(
        &nb,
        "#! mathcas 2 notebook\n\n#% title\n# Interactive controls\n\n#% cell\n\
         manipulate(plot(sin(a*x)*exp(-x/b), [x, 0, 12]), [a, 0.5, 4], [[b, 6], 1, 20, 1])\n",
    )
    .unwrap();
    let saved = dir.join("saved.mcn");
    let script = dir.join("script.txt");
    // the input is at y 257 and the sliders' knobs at (55, 296) and (134, 322) in a 1000 x 760 window at 100 %
    std::fs::write(
        &script,
        format!(
            "out {}\nsize 1000 760\nppp 1\ntheme light\nopen {}\nwait 10\nclick 300 257\nkeys Shift+Enter\nwait 150\n\
             probe drag\ndrag 55 296 300 296\nwait 3\ndrag 300 296 120 296\nwait 3\ndrag 134 322 300 322\nwait 120\n\
             save {}\nquit\n",
            dir.display(),
            nb.display(),
            saved.display()
        ),
    )
    .unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_mc-notebook"))
        .env("MCNB_SELFTEST", &script)
        .env("MCNB_CONFIG_DIR", dir.join("config"))
        .env("MCUI_CLIPBOARD", "local")
        .env("MATHCAS_DOCS", "-")
        .status()
        .expect("start the notebook");
    assert!(status.success(), "{status}");
    let log = std::fs::read_to_string(dir.join("selftest.log")).unwrap();
    println!("{log}");
    let file = std::fs::read_to_string(&saved).unwrap();
    let controls = file.lines().find(|l| l.contains(" controls=")).expect("the positions are kept");
    assert!(controls.ends_with(":17"), "b dragged to 17: {controls}");
    assert!(file.contains("#% output manipulated\n#= image(\"plot-"), "the last result is kept: {file}");
    // the window answered within a frame's time while the sliders moved and the kernel plotted
    let probe = log.lines().find(|l| l.starts_with("probe drag")).unwrap();
    let max: f64 = probe.split("max ").nth(1).and_then(|s| s.trim_end_matches(" ms").parse().ok()).unwrap();
    assert!(max < 16.0, "{probe}");
    let _ = std::fs::remove_dir_all(&dir);
}
