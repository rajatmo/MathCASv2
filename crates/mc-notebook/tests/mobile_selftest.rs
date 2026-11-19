//! The layout for phones and tablets, driven through the window's self-test (`tests/selftest/mobile.txt`) on this
//! system with `MCNB_LAYOUT=mobile`: the Home screen, a new calculation evaluated with the kernel, the notebook
//! written by itself under `Calculation 1.mcn` when it is left, and opened again from Home. It opens a window and
//! needs `mc-kernel` built beside the notebook, so it is ignored by default:
//! `cargo test -p mc-notebook --test mobile_selftest -- --ignored`.
//!
//! The second test puts fingers on a 3D picture (`tests/selftest/mobile-touch.txt`; the picture and its scene are
//! made here, no kernel needed): one finger turns it, two zoom it, and the page scrolls only when a finger moves
//! beside the picture or two move together.
use mc_graphics::view3d::{sidecar_text, Scene3d};
use std::path::PathBuf;
use std::process::Command;

/// The number after `key` in `line` (`offset 12.5`, `zoom=1.21`).
fn number(line: &str, key: &str) -> Option<f64> {
    let rest = &line[line.find(key)? + key.len()..];
    let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-')).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

#[test]
#[ignore = "opens a window"]
fn fingers_turn_and_zoom_a_3d_picture_and_the_page_stays() {
    let dir = std::env::temp_dir().join(format!("mcnb-mobile-touch-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let docs = dir.join("notebooks");
    std::fs::create_dir_all(dir.join("config")).unwrap();
    std::fs::create_dir_all(&docs).unwrap();
    // a scene and its picture, as a plot writes them, between two long cells (the page scrolls)
    let term = "graphics3d([rgb_color(0.9, 0.5, 0.2), cuboid([0, 0, 0], [1, 1, 1]), rgb_color(0.2, 0.5, 0.9), \
                sphere([2, 0, 0.5], 0.5)], axes: true)";
    let text = sidecar_text(term);
    let scene = Scene3d::from_text(&text, &|| false).expect("a scene");
    let png = docs.join("plot-1.png");
    std::fs::write(&png, scene.render_png(&scene.view(), 1.0, &|| false).unwrap()).unwrap();
    std::fs::write(docs.join("plot-1.scene3d"), &text).unwrap();
    let lines = |n: usize| (1..=n).map(|k| format!("a{k} = {k}")).collect::<Vec<_>>().join("\n");
    let value = mc_nbfile::images::image_value(&png.display().to_string());
    let nb = docs.join("Solids.mcn");
    let file = format!("#! mathcas 2 notebook\n\n#% cell\n{}\n\n#% cell\nsolids\n#% output\n#= {value}\n\n#% cell\n{}\n", lines(12), lines(60));
    std::fs::write(&nb, file).unwrap();
    let slashes = |p: &std::path::Path| p.display().to_string().replace('\\', "/");
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("selftest").join("mobile-touch.txt");
    let text = std::fs::read_to_string(&src).unwrap().replace("@NB@", &slashes(&nb)).replace("@OUT@", &slashes(&dir));
    let script = dir.join("mobile-touch.txt");
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
    // what the log says after each `log -- ...` line: the picture's kept view, the page's offset, the bottom bar
    let parts: Vec<&str> = log.split("\n-- ").collect();
    let part = |name: &str| *parts.iter().find(|p| p.starts_with(name)).unwrap_or_else(|| panic!("no part `{name}`: {log}"));
    let line = |name: &str, start: &str, k: usize| {
        let found = part(name).lines().filter(|l| l.starts_with(start)).nth(k);
        found.unwrap_or_else(|| panic!("no `{start}` line {k} in part `{name}`: {log}")).to_string()
    };
    let offset = |name: &str| number(&line(name, "anchor: ", 0), "offset ").expect("an offset");
    let view = |name: &str| line(name, "view3d: ", 0);
    let zoom = |name: &str| number(&view(name), "zoom=").unwrap_or(1.0);
    // the view point alone: `view=x:y:z`
    let turned = |name: &str| {
        let v = view(name);
        let from = v.find("view=").unwrap_or_else(|| panic!("no view in part `{name}`: {log}"));
        v[from..].split([' ', ']']).next().unwrap_or_default().to_string()
    };

    assert!(line("start", "hit ", 0).contains("Graphic("), "the script's point is on the picture: {log}");
    assert_eq!(offset("start"), 0.0, "{log}");
    assert!(line("start", "mobile: ", 0).ends_with(" add"), "{log}");

    // one finger, with nothing selected before: the picture turns and the page stays, although the bar with the
    // picture's actions comes up under the finger's first move
    assert!(view("one finger on the picture").contains("[view="), "the finger turned it: {log}");
    assert_eq!(offset("one finger on the picture"), 0.0, "a drag on the picture does not scroll the page: {log}");
    assert!(line("one finger on the picture", "mobile: ", 0).ends_with(" actions"), "{log}");

    // two fingers from 80 to 200 apart: closer by 200 / (80 + what they move before it is a zoom)
    let z = zoom("two fingers apart");
    assert!(z > 1.6 && z < 2.5, "zoomed in by the fingers' spread, got {z}: {log}");
    assert_eq!(turned("two fingers apart"), turned("one finger on the picture"), "a pinch does not turn: {log}");
    assert_eq!(offset("two fingers apart"), 0.0, "a pinch does not scroll the page: {log}");

    // beside the picture a finger scrolls the page by its travel
    let scrolled = offset("one finger beside the picture");
    assert!((scrolled - 100.0).abs() < 2.0, "the page follows the finger, got {scrolled}: {log}");
    assert!(line("one finger beside the picture", "hit ", 0).contains("Graphic("), "the picture moved with the page: {log}");

    // the picture turns where it is now, and the page stays
    assert_ne!(turned("one finger on the picture, the page scrolled"), turned("two fingers apart"), "{log}");
    assert!((zoom("one finger on the picture, the page scrolled") - z).abs() < 1e-6, "a turn keeps the zoom: {log}");
    assert_eq!(offset("one finger on the picture, the page scrolled"), scrolled, "{log}");

    // two fingers moving together scroll the page, on the picture too, and change nothing of the picture
    let panned = offset("two fingers together");
    assert!((panned - scrolled - 60.0).abs() < 2.0, "the page follows two fingers, got {panned}: {log}");
    assert_eq!(view("two fingers together").split(" renders").next(), view("one finger on the picture, the page scrolled").split(" renders").next(), "{log}");

    // ... and moving together (200 to 80 apart) they zoom out again
    let back = zoom("two fingers together again");
    assert!(back < z * 0.55 && back > z * 0.3, "zoomed out, got {back} from {z}: {log}");
    assert_eq!(offset("two fingers together again"), panned, "{log}");

    // a tap on the picture turns nothing and moves nothing
    let same = |a: &str, b: &str| assert_eq!(view(a).split(" renders").next(), view(b).split(" renders").next(), "{log}");
    same("a tap on the picture", "two fingers together again");
    assert_eq!(offset("a tap on the picture"), panned, "{log}");
    assert!(line("a tap on the picture", "mobile: ", 0).ends_with(" actions"), "{log}");
    for shot in ["start", "turned", "zoomed", "end"] {
        assert!(dir.join(format!("{shot}.png")).is_file(), "screenshot {shot}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

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
