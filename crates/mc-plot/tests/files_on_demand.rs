//! Pictures whose files are written on demand (`mc_plot::set_files_on_demand`, the kernel's `pictures`): nothing is
//! on disk when the picture is made, its size is known, and the files written when a reader asks are byte for byte
//! the ones written when the picture is made. One test in this file: the switch is the process's.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// The PNG file an image value names.
fn png_of(value: &str) -> PathBuf {
    assert!(value.starts_with("image(\""), "{value}");
    PathBuf::from(value.trim_start_matches("image(\"").trim_end_matches("\")"))
}

const FILES: [&str; 8] = ["png", "svg", "pdf", "json", "scene3d", "column.svg", "print.svg", "print.pdf"];

/// Every file of the picture at `png`, by extension (`None`: not on disk).
fn files(png: &Path) -> Vec<(&'static str, Option<Vec<u8>>)> {
    FILES.iter().map(|&e| (e, std::fs::read(png.with_extension(e)).ok())).collect()
}

#[test]
fn files_written_on_demand_are_the_files_written_at_once() {
    let started = Instant::now();
    let mut e = CoreEngine::new();
    let inputs = [
        "plot(sin(x), [x, 0, 6])",
        "plot(sin(x), [x, 0, 6], image_size: [500, 300])",
        "list_plot(table([k, k^2], [k, 1, 20]))",
        "graphics([disk(), text(\"a\", [0, 0])])",
        "graphics3d([sphere(), cuboid([1, 1, 1])])",
        "plot3d(sin(x*y), [x, 0, 3], [y, 0, 3], plot_points: 15)",
        "contour_plot(x^2 - y^2, [x, -1, 1], [y, -1, 1])",
    ];
    for src in inputs {
        mc_plot::set_files_on_demand(false);
        let now = png_of(&eval(&mut e, src));
        let size = mc_plot::png_size(&now);
        mc_graphics::deferred::ensure(&now);
        let at_once = files(&now);

        mc_plot::set_files_on_demand(true);
        let later = png_of(&eval(&mut e, src));
        assert_ne!(now, later, "{src}");
        assert!(FILES.iter().all(|x| !later.with_extension(x).exists()), "{src}: a file is on disk");
        assert!(mc_graphics::deferred::png_is_pending(&later), "{src}");
        // the size is the PNG's, though it is not drawn yet
        assert_eq!(mc_plot::png_size(&later), size, "{src}");
        // the files a front end reads now, the others when an export asks
        mc_graphics::deferred::ensure_only(&later, &["png", "json"]);
        assert!(later.is_file() && later.with_extension("json").is_file(), "{src}");
        assert!(!later.with_extension("svg").exists(), "{src}");
        mc_graphics::deferred::ensure(&later);
        assert!(!mc_graphics::deferred::is_pending(&later), "{src}");
        let on_demand = files(&later);
        for ((ext, a), (_, b)) in at_once.iter().zip(&on_demand) {
            assert_eq!(a.is_some(), b.is_some(), "{src}: .{ext} written in one way only");
            assert!(a == b, "{src}: .{ext} differs");
        }
    }
    // a reader in the session (an export of the picture) finds its files
    eval(&mut e, "p = plot(cos(x), [x, 0, 1])");
    let later = png_of(&eval(&mut e, "p"));
    assert!(!later.exists());
    let out = std::env::temp_dir().join(format!("files-on-demand-{}.png", std::process::id()));
    let src = format!("export_data(\"{}\", p)", out.to_string_lossy().replace('\\', "/"));
    eval(&mut e, &src);
    assert_eq!(std::fs::read(&out).ok(), std::fs::read(&later).ok());
    let _ = std::fs::remove_file(&out);
    mc_plot::set_files_on_demand(false);
    assert!(started.elapsed() < Duration::from_secs(60), "took {:?}", started.elapsed());
}
