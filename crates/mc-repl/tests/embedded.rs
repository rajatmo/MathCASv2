//! One file per notebook (round 13, user decision): `mc-repl --update` embeds the pictures of plots in the `.mcn`
//! itself, with their descriptions (hover) and 3D scenes (rotation); nothing next to the notebook, nothing in the
//! plots cache named; the file exports with its pictures; a second run gives the same text.
use std::path::PathBuf;
use std::process::Command;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mc-repl-embedded-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn update(nb: &std::path::Path) {
    let o = Command::new(env!("CARGO_BIN_EXE_mc-repl")).arg("--update").arg(nb).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}

#[test]
fn plots_are_embedded_in_the_notebook_file() {
    let dir = scratch("plots");
    let nb = dir.join("plots.mcn");
    std::fs::write(
        &nb,
        "#! mathcas 2 notebook\n\n#% cell\nplot(sin(x), [x, 0, 6])\n\n#% cell\nplot3d(x*y, [x, -1, 1], [y, -1, 1])\n",
    )
    .unwrap();
    update(&nb);
    // one file in the folder
    let names: Vec<String> =
        std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    assert_eq!(names, ["plots.mcn"]);
    let text = std::fs::read_to_string(&nb).unwrap();
    let temp = std::env::temp_dir().display().to_string();
    assert!(!text.contains(&temp) && !text.contains("mathcas-plots"), "no cache path is named");
    // each plot carries its drawing (round nb-svg: the SVG, deflated) and its description, the 3D plot also its
    // scene
    let heads: Vec<&str> = text.lines().filter(|l| l.starts_with("#~ @")).collect();
    assert_eq!(heads.len(), 5, "{heads:?}");
    assert!(heads[0].ends_with(".svg deflate") && heads[1].ends_with(".json deflate"), "{heads:?}");
    assert!(heads[2].ends_with(".svg deflate") && heads[4].ends_with(".scene3d deflate"), "{heads:?}");
    assert!(text.contains("#= image(\"plot-") && text.contains(".svg\")"), "the values name the drawings");
    // a plain plot stays small: the deflated drawing in base64 plus its deflated description
    let size = text.len();
    println!("embedded notebook: {size} bytes");
    assert!(size < 400_000, "{size}");
    // the file reads back, exports with its pictures, and a second run writes the same text
    let html = dir.join("out.html");
    let o = Command::new(env!("CARGO_BIN_EXE_mc-repl")).arg("--export").arg(&nb).arg(&html).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let page = std::fs::read_to_string(&html).unwrap();
    assert_eq!(page.matches("<svg ").count(), 2, "both pictures found, inline vector drawings");
    std::fs::remove_file(&html).unwrap();
    update(&nb);
    assert_eq!(std::fs::read_to_string(&nb).unwrap(), text, "plots are drawn the same way again");
    let _ = std::fs::remove_dir_all(&dir);
}
