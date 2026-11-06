//! Turning 3D pictures (round 13), driven through the window's self-test: a `graphics3d` picture with its scene
//! (made here, no kernel needed) is dragged (it turns), zoomed with the wheel while selected, saved (the view is kept
//! on its output and the picture embedded with its scene), reopened (the view comes back) and double-clicked (its
//! own view again). It opens a window, so it is ignored by default:
//! `cargo test -p mc-notebook --test rotate3d_selftest -- --ignored`.
use mc_graphics::view3d::{sidecar_text, Scene3d};
use std::path::PathBuf;
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
fn a_3d_picture_turns_zooms_and_keeps_its_view() {
    let dir = std::env::temp_dir().join(format!("mcnb-rotate3d-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("config")).unwrap();
    // a scene and its picture, as a plot writes them
    let term = "graphics3d([rgb_color(0.9, 0.5, 0.2), cuboid([0, 0, 0], [1, 1, 1]), rgb_color(0.2, 0.5, 0.9), \
                sphere([2, 0, 0.5], 0.5)], axes: true)";
    let text = sidecar_text(term);
    let scene = Scene3d::from_text(&text, &|| false).expect("a scene");
    let png = dir.join("plot-1.png");
    std::fs::write(&png, scene.render_png(&scene.view(), 1.0, &|| false).unwrap()).unwrap();
    std::fs::write(dir.join("plot-1.scene3d"), &text).unwrap();
    let nb = dir.join("solids.mcn");
    let value = mc_nbfile::images::image_value(&png.display().to_string());
    std::fs::write(&nb, format!("#! mathcas 2 notebook\n\n#% cell\nsolids\n#% output\n#= {value}\n")).unwrap();
    let out = dir.display();
    let saved = dir.join("saved.mcn");
    // the picture's top-left corner is at about (33, 60) in a 1000 x 700 window: points well inside it
    let log = run(
        &dir,
        &format!(
            "out {out}\nsize 1000 700\nppp 1\ntheme light\nopen {}\nwait 20\nclick 150 200\nwait 40\n\
             drag 150 200 260 160\nwait 20\nview3d\nplainwheel 1\nwait 2\nplainwheel 1\nwait 60\nview3d\nshot turned\n\
             save {}\nquit\n",
            nb.display(),
            saved.display()
        ),
    );
    println!("{log}");
    let views: Vec<&str> = log.lines().filter(|l| l.starts_with("view3d: ")).collect();
    assert!(views[0].contains("[view="), "the drag turned it: {log}");
    assert!(views[1].contains("zoom=1.21"), "two wheel notches zoom by 1.1^2: {log}");
    let file = std::fs::read_to_string(&saved).unwrap();
    assert!(file.contains("#% output view=") && file.contains(" zoom=1.21"), "{file}");
    assert!(file.contains(".scene3d deflate\n"), "the scene travels inside the file");
    // reopened: the view is there; a double-click goes back to the picture's own view
    let log = run(
        &dir,
        &format!("out {out}\nsize 1000 700\nppp 1\ntheme light\nopen {}\nwait 30\nview3d\nshot reopened\ndblclick 150 200\nwait 20\nview3d\nquit\n", saved.display()),
    );
    println!("{log}");
    let views: Vec<&str> = log.lines().filter(|l| l.starts_with("view3d: ")).collect();
    assert!(views[0].contains("zoom=1.21") && !views[0].contains("renders: 0 "), "rendered from the file: {log}");
    assert!(!views[1].contains('['), "the double-click dropped the view: {log}");
    let _ = std::fs::remove_dir_all(PathBuf::from(&dir));
}
