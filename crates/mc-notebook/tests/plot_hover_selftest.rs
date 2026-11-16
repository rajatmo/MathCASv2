//! Hovering over plots in the window, driven through the self-test: a picture made by `show` from a curve and a
//! list of points snaps to the points of both (a marker and a tooltip with the series' name and its coordinates),
//! and a single plot still snaps to its curve. It opens a window and starts the kernel, so it is ignored by
//! default: `cargo test -p mc-notebook --test plot_hover_selftest -- --ignored`.
use std::process::Command;

fn run(dir: &std::path::Path, script: &str) -> String {
    let path = dir.join("script.txt");
    std::fs::write(&path, script).unwrap();
    let _ = std::fs::remove_file(dir.join("selftest.log"));
    let status = Command::new(env!("CARGO_BIN_EXE_mc-notebook"))
        .env("MCNB_SELFTEST", &path)
        .env("MCNB_CONFIG_DIR", dir.join("config"))
        .env("MCUI_CLIPBOARD", "local")
        .env("MATHCAS_DOCS", "-")
        .status()
        .expect("start the notebook");
    assert!(status.success(), "{status}");
    std::fs::read_to_string(dir.join("selftest.log")).expect("the self-test log")
}

#[test]
#[ignore = "opens a window"]
fn a_shown_plot_snaps_to_the_points_of_every_plot() {
    let start = std::time::Instant::now();
    let dir = std::env::temp_dir().join(format!("mcnb-plot-hover-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("config")).unwrap();
    let nb = dir.join("hover.mcn");
    std::fs::write(
        &nb,
        "#! mathcas 2 notebook\n\n#% cell\nshow(plot(sin(x), [x, 0, 6]), list_plot([[1, 0.5], [2, 0.7]]))\n\n\
         #% cell\nplot(cos(x), [x, 0, 6])\n",
    )
    .unwrap();
    let out = dir.display();
    // the shown picture: the second point of the list, then a point of the curve; then the single plot's curve
    let log = run(
        &dir,
        &format!(
            "out {out}\nsize 1000 900\nppp 1\ntheme light\nopen {}\nwait 20\nselect 0 0 0\neval\nwait 30\n\
             plotpoint 1 1\nwait 8\nplottip\nshot shown-points\nplotpoint 0 20\nwait 8\nplottip\nshot shown-curve\n\
             select 2 0 0\neval\nwait 30\nplotpoint 0 20\nwait 8\nplottip\nshot single\nquit\n",
            nb.display()
        ),
    );
    println!("{log}");
    let tips: Vec<&str> = log.lines().filter_map(|l| l.strip_prefix("plottip: ")).collect();
    assert_eq!(tips.len(), 3, "{log}");
    // snapped (exact coordinates, `=`), named, on the list's point
    assert!(tips[0].contains("x = 2, y = 0.7") && tips[0].contains(": x"), "{log}");
    // on the curve of the other plot of the same picture: y = sin(x)
    let xy = |t: &str| -> (f64, f64) {
        let x = t.split("x = ").nth(1).and_then(|s| s.split(',').next()).and_then(|s| s.trim().parse().ok()).unwrap();
        let y = t.split("y = ").nth(1).and_then(|s| s.trim().parse().ok()).unwrap();
        (x, y)
    };
    let (x, y) = xy(tips[1]);
    assert!((y - x.sin()).abs() < 1e-3, "{log}");
    // a single plot: snapped to its curve, without a series name (one series)
    let (x, y) = xy(tips[2]);
    assert!((y - x.cos()).abs() < 1e-3 && !tips[2].contains(": x"), "{log}");
    assert!(start.elapsed().as_secs() < 300);
    let _ = std::fs::remove_dir_all(&dir);
}
