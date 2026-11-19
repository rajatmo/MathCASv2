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

/// A folder for a run with a notebooks folder that holds a scene and its picture, as a plot writes them; the
/// picture's path with forward slashes.
fn run_folder(name: &str) -> (PathBuf, PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("mcnb-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let docs = dir.join("notebooks");
    std::fs::create_dir_all(dir.join("config")).unwrap();
    std::fs::create_dir_all(&docs).unwrap();
    let term = "graphics3d([rgb_color(0.9, 0.5, 0.2), cuboid([0, 0, 0], [1, 1, 1]), rgb_color(0.2, 0.5, 0.9), \
                sphere([2, 0, 0.5], 0.5)], axes: true)";
    let text = sidecar_text(term);
    let scene = Scene3d::from_text(&text, &|| false).expect("a scene");
    let png = docs.join("plot-1.png");
    std::fs::write(&png, scene.render_png(&scene.view(), 1.0, &|| false).unwrap()).unwrap();
    std::fs::write(docs.join("plot-1.scene3d"), &text).unwrap();
    let value = mc_nbfile::images::image_value(&png.display().to_string());
    (dir, docs, value)
}

/// Run the script `name` of `tests/selftest` in the phone layout with `@NB@` and `@OUT@` filled in; the log.
fn run_script(name: &str, dir: &std::path::Path, docs: &std::path::Path, nb: &std::path::Path) -> String {
    let slashes = |p: &std::path::Path| p.display().to_string().replace('\\', "/");
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("selftest").join(name);
    let text = std::fs::read_to_string(&src).unwrap().replace("@NB@", &slashes(nb)).replace("@OUT@", &slashes(dir));
    let script = dir.join(name);
    std::fs::write(&script, text).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_mc-notebook"))
        .env("MCNB_LAYOUT", "mobile")
        .env("MCNB_DOCUMENTS", docs)
        .env("MCNB_SELFTEST", &script)
        .env("MCNB_CONFIG_DIR", dir.join("config"))
        .env("MCUI_CLIPBOARD", "local")
        .env("MATHCAS_DOCS", "-")
        .status()
        .expect("start the notebook");
    assert!(status.success(), "{status}");
    let log = std::fs::read_to_string(dir.join("selftest.log")).expect("the self-test log");
    println!("{log}");
    log
}

#[test]
#[ignore = "opens a window"]
fn the_editor_under_fingers() {
    let (dir, docs, picture) = run_folder("mobile-editor-selftest");
    let nb = docs.join("Editor.mcn");
    let file = format!(
        "#! mathcas 2 notebook\n\n#% cell\nfirst = 12\nsecond = first + 30\n\n#% cell\nx + y\n#% output\n#= x + y\n\n\
         #% cell\nsolids\n#% output\n#= {picture}\n\n#% cell\nlast = 99\n"
    );
    std::fs::write(&nb, file).unwrap();
    let log = run_script("mobile-editor.txt", &dir, &docs, &nb);
    assert!(!log.contains("no control") && !log.contains("no such place") && !log.contains("unknown step"), "{log}");
    // the lines of the part after `log -- <name>` that start with `start`, without it
    let lines = |name: &str, start: &str| -> Vec<String> {
        let all = format!("\n{log}");
        let part = all.split("\n-- ").find(|p| p.lines().next() == Some(name)).unwrap_or_else(|| panic!("no part `{name}`: {log}"));
        part.lines().filter_map(|l| l.strip_prefix(start)).map(str::to_string).collect()
    };
    let over = |name: &str| lines(name, "overlays: ");
    let caret = |cell: u64, anchor: usize, pos: usize| format!("Caret {{ cell: CellId({cell}), anchor: {anchor}, pos: {pos} }}");

    // a new calculation with one cell: a long press on a word selects it and brings the tools; their Cell opens
    // the cell's sheet; Delete cell leaves an empty input and the line with Undo; Undo brings the text back
    assert_eq!(lines("the only cell", "selection: "), [caret(1, 0, 5)], "{log}");
    assert_eq!(
        over("the only cell"),
        [
            "sheet none, tools true, snack none, cells 1, caret shown",
            "sheet cell, tools true, snack none, cells 1, caret shown",
            "sheet none, tools false, snack Cell deleted, cells 1, caret shown",
            "sheet none, tools false, snack none, cells 1, caret none",
        ],
        "{log}"
    );
    assert_eq!(lines("the only cell", "show canon: "), ["\"\"", "\"seven = 7\""], "{log}");

    // the keys of the key row act at the caret: `+` at the end, the left arrow twice, then typing
    assert_eq!(lines("the key row", "show canon: "), ["\"seven = 7*3+1\""], "{log}");
    // a fraction, a power and a root from the key row, each left with the right arrow
    assert_eq!(lines("typeset boxes from the key row", "show linear: "), ["\"seven = 7*3+1+1/2+x^2+sqrt(y)\""], "{log}");
    assert!(lines("typeset boxes from the key row", "show boxes: ")[0].contains("(1/2)+(x^2)+√(y)"), "{log}");
    // Run evaluates the cell (an assignment: it is numbered and has no result to show) and the round + for the
    // next input shows
    assert!(lines("Run", "mobile: ")[0].ends_with(" add"), "{log}");
    let ran = std::fs::read_to_string(dir.join("after-run.mcn")).expect("the notebook after Run");
    assert!(ran.contains("#% cell in=1 typeset\nseven = 7*3+1+((1)/(2))+((x)^(2))+((sqrt(y)))"), "{ran}");

    // in the notebook: a long press on `second` selects the word; Copy puts it on the clipboard, the tools stay
    assert_eq!(lines("a long press on a word", "selection: "), [caret(1, 11, 17)], "{log}");
    assert_eq!(lines("a long press on a word", "clipboard: "), ["Some(\"second\")"], "{log}");
    assert!(lines("a long press on a word", "mobile: ")[0].ends_with(" keys"), "{log}");
    assert!(over("a long press on a word").iter().all(|o| o.starts_with("sheet none, tools true, snack none, cells 6")), "{log}");
    // Paste over the word `99`; the tools go
    assert_eq!(lines("paste over a word", "selection: "), [caret(6, 7, 9)], "{log}");
    assert_eq!(lines("paste over a word", "show canon: "), ["\"last = second\""], "{log}");
    assert!(over("paste over a word")[0].contains("tools false"), "{log}");
    // Cut takes the word out and onto the clipboard
    assert_eq!(lines("cut", "show canon: "), ["\" = 12\\nsecond = first + 30\""], "{log}");
    assert_eq!(lines("cut", "clipboard: "), ["Some(\"first\")"], "{log}");
    // Select all: the whole cell
    assert_eq!(lines("select all", "selection: "), [caret(6, 0, 13)], "{log}");
    // a long press that moves selects from the word on (here to the end of `x + y`)
    assert_eq!(lines("a long press that moves", "selection: "), [caret(2, 0, 5)], "{log}");

    // a long press on a result: its sheet; Copy copies its text and the sheet goes
    assert!(over("a long press on a result")[0].starts_with("sheet cell, tools false"), "{log}");
    assert_eq!(lines("a long press on a result", "clipboard: "), ["Some(\"x + y\")"], "{log}");
    assert!(over("a long press on a result")[1].starts_with("sheet none"), "{log}");
    // a long press on a picture: selected, its sheet; Delete result takes it away, Undo brings it back
    assert!(lines("a long press on a picture", "mobile: ")[0].ends_with(" actions"), "{log}");
    assert_eq!(
        over("a long press on a picture").iter().map(|o| o.split(", caret").next().unwrap_or_default()).collect::<Vec<_>>(),
        [
            "sheet cell, tools false, snack none, cells 6",
            "sheet none, tools false, snack Result deleted, cells 5",
            "sheet none, tools false, snack none, cells 6",
        ],
        "{log}"
    );
    // Insert input below: a new empty cell after the picture, the caret in it; Backspace there removes it again
    // and the caret is at the end of `solids`; Undo brings the cell back
    let o = over("insert below, Backspace in the empty cell");
    assert!(o[0].contains("cells 7") && o[1].contains("snack Cell deleted, cells 6") && o[2].contains("snack none, cells 7"), "{log}");
    assert_eq!(lines("insert below, Backspace in the empty cell", "selection: "), [caret(7, 0, 0), caret(4, 6, 6)], "{log}");
    // an input is deleted with its result (two cells), and both come back
    let o = over("an input goes with its result");
    assert!(o[0].contains("snack Cell deleted, cells 4") && o[1].contains("snack none, cells 6"), "{log}");
    // the canvas gets shorter (as when the on-screen keyboard comes up): the caret is brought back into view
    assert!(over("the caret stays in view").iter().all(|o| o.ends_with("caret shown")), "{log}");
    for shot in ["sheet-input", "deleted-last", "keys", "word", "sheet-result", "sheet-picture", "snack", "short"] {
        assert!(dir.join(format!("{shot}.png")).is_file(), "screenshot {shot}");
    }
    let _ = std::fs::remove_dir_all(&dir);
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
