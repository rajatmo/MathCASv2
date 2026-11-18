//! `model::check` (what `graphics` and `graphics3d` evaluate to) gives the same verdict as `model::read`: the same
//! failure tag for content and options that cannot be drawn, success for the rest, in two and three dimensions.
use mc_core::{Config, Heap};

fn verdicts(src: &str) -> (Result<(), String>, Result<(), String>) {
    let h = Heap::new(Config::default());
    let t = mc_syntax::read(&h, src).expect("reads")[0];
    let checked = mc_graphics::model::check(&h, t, &|| false).map_err(|f| f.tag.to_string());
    let read = mc_graphics::model::read(&h, t, &|| false).map(|_| ()).map_err(|f| f.tag.to_string());
    (checked, read)
}

#[test]
fn check_agrees_with_read() {
    let cases = [
        "graphics3d([opacity(0.5), sphere([0, 0, 0])])",
        "graphics3d([rgb_color(1, 1, 0), sphere([0, 0, 0]), [rgb_color(0, 0, 1), sphere([1, 0, 0])], sphere([2, 0, 0])])",
        "graphics3d(cuboid([0, 0, 0]), options(axes -> true))",
        "graphics3d([line([[0, 0, 0], [1, 1, 1]]), point([0, 1, 0]), polygon([[0, 0, 0], [1, 0, 0], [0, 1, 0]])])",
        "graphics3d(sphere([a, 0, 0]))",
        "graphics3d(disk([0, 0]))",
        "graphics3d(17)",
        "graphics3d(sphere([0, 0, 0]), options(view_point -> foo))",
        "graphics3d(sphere([0, 0, 0]), options(plot_range -> [[0, 1], [0, 1], [0, 1]]))",
        "graphics3d(sphere([0, 0, 0]), 3)",
        "graphics([disk([0, 0]), text(\"a\", [1, 1])])",
        "graphics(point([x, 0]))",
        "graphics(sphere([0, 0, 0]))",
    ];
    for src in cases {
        let (checked, read) = verdicts(src);
        assert_eq!(checked, read, "{src}");
    }
}
