//! Round 20: the properties of `rasterize(x, p)` are values, never an image term naming a file in
//! the temporary folder.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

#[test]
fn rasterize_gives_its_properties() {
    let mut e = CoreEngine::new();
    eval(
        &mut e,
        "g = graphics([red, annotation(disk(), \"Red\", \"Region\"), blue, annotation(disk([3, 0]), \"Blue\", \"Region\")], image_size: 100)",
    );
    let size = eval(&mut e, "rasterize(g, \"RasterSize\")");
    let [w, h]: [i64; 2] = serde_json::from_str::<Vec<i64>>(&size).unwrap().try_into().unwrap();
    assert_eq!(w, 100, "{size}");
    assert!(h > 20 && h < 100, "{size}");
    assert_eq!(eval(&mut e, "rasterize(g, \"ImageSize\")"), size);
    assert_eq!(eval(&mut e, "rasterize(g, \"BoundingBox\")"), format!("[{w}, {h}, {h}]"), "the baseline is the bottom edge");
    assert_eq!(eval(&mut e, "rasterize(g, [\"RasterSize\", \"ImageSize\"])"), format!("[{size}, {size}]"));
    // the pixels: h rows of w [r, g, b] triples, red on the left, blue on the right
    assert_eq!(eval(&mut e, "dimensions(rasterize(g, \"Data\"))"), format!("[{h}, {w}, 3]"));
    let mid = h / 2 + 1;
    assert_eq!(eval(&mut e, &format!("rasterize(g, \"Data\")[{mid}, 20]")), "[255, 0, 0]");
    assert_eq!(eval(&mut e, &format!("rasterize(g, \"Data\")[{mid}, 80]")), "[0, 0, 255]");
    // the regions: [left, top] and [right, bottom], y up from the bottom edge, around each disk
    let regions = eval(&mut e, "rasterize(g, \"Regions\")");
    assert!(regions.starts_with("[[\"Red\", \"Region\"] -> [["), "{regions}");
    assert!(regions.contains("[\"Blue\", \"Region\"] -> [["), "{regions}");
    assert_eq!(eval(&mut e, "length(rasterize(g, \"Regions\"))"), "2");
    let red = eval(&mut e, "rasterize(g, \"Regions\")[1, 2]");
    let [[l, t], [r, b]]: [[i64; 2]; 2] = serde_json::from_str(&red).unwrap();
    assert!(l < r && b < t && l >= 0 && r <= w / 2 && t <= h && b >= 0, "{red}");
    assert!(r - l > 25 && t - b > 25, "the disk is most of the half: {red}");
    // the rest
    assert_eq!(eval(&mut e, "rasterize(x^2, \"Regions\")"), "[]");
    // annotated parts of a layout: not located, said so (not an empty list)
    let col = eval(&mut e, "rasterize(column([annotation(\"a\", \"A\", \"Region\"), x]), \"Regions\")");
    assert!(col.starts_with("failure(unsupported"), "{col}");
    assert!(eval(&mut e, "rasterize(x^2, \"RasterSize\")").starts_with('['));
    assert!(eval(&mut e, "rasterize(g, \"Nope\")").starts_with("failure(unknown_property, \"Nope\""));
    assert!(eval(&mut e, "rasterize(g, \"Image\")").starts_with("image("));
    assert!(eval(&mut e, "rasterize(g, \"Properties\")").contains("\"Regions\""));
    // a picture already drawn: its own size
    eval(&mut e, "p = rasterize(g)");
    assert_eq!(eval(&mut e, "rasterize(p, \"RasterSize\")"), size);
    assert_eq!(eval(&mut e, "dimensions(rasterize(p, \"Data\"))"), format!("[{h}, {w}, 3]"));
    // no property is an image term naming a temporary file
    for p in ["RasterSize", "ImageSize", "BoundingBox", "Regions", "Data", "Properties"] {
        assert!(!eval(&mut e, &format!("rasterize(g, \"{p}\")")).contains("mathcas-plots"), "{p}");
    }
}
