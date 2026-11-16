//! Round 46: the time of ordinary inputs.
//!
//! - Every cell was parsed and lowered on a thread started for it (two or three thread starts per cell, most of the
//!   time of a small input, and far more on a busy machine). The session's workers declare their large stack
//!   (`mc_api::stack`), and parsing inside an evaluation runs on them.
//! - A plot drew its PDF and two print twins with every evaluation; they are drawn when an export first reads them
//!   (`mc_graphics::deferred`), and the export gives the same files as before.
//! - Showing a plot opened its new PNG file again for its size; the size is kept when the file is written.
//! - Printing a packed array made a term and a task per element; it is written straight into the text, the same
//!   text (a plot's sidecar and memo print their whole term: a density plot's raster took longer than the plot).
//!
//! Each test has a time bound (scaled by `MC_TEST_TIME_FACTOR`).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn factor() -> f64 {
    std::env::var("MC_TEST_TIME_FACTOR").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0)
}

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// The path of a printed `image("path")`.
fn image_path(printed: &str) -> String {
    let p = printed.strip_prefix("image(\"").and_then(|s| s.strip_suffix("\")")).unwrap_or_else(|| panic!("{printed}"));
    p.to_string()
}

#[test]
fn evaluations_run_on_a_declared_stack() {
    let mut e = CoreEngine::new();
    let left = Arc::new(AtomicUsize::new(0));
    let l = left.clone();
    e.install(move |_, _| l.store(mc_api::stack::remaining(), Ordering::SeqCst));
    // the session's workers have 64 MB and say so: parsing inside an evaluation needs no thread of its own
    assert!(left.load(Ordering::SeqCst) >= 32 << 20, "{}", left.load(Ordering::SeqCst));
    // parsing the inputs of an evaluation still works, also deeply nested ones
    let deep = format!("{}1{}", "[".repeat(200), "]".repeat(200));
    assert_eq!(ev(&mut e, &format!("depth({deep})")), "201");
    assert_eq!(ev(&mut e, &format!("depth(to_expression(\"{deep}\"))")), "201");
    assert_eq!(ev(&mut e, "to_expression(\"1 + 2*3\")"), "7");
    // a thread that declared nothing still parses (on a thread of its own)
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(|| assert!(mc_syntax::parse("f(x) := [[[x]]]").errors.is_empty()))
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn many_small_cells_are_quick() {
    let mut e = CoreEngine::new();
    ev(&mut e, "1 + 1");
    let t = Instant::now();
    for k in 0..400 {
        assert_eq!(ev(&mut e, &format!("{k} + 1")), (k + 1).to_string());
    }
    let took = t.elapsed();
    // about 0.1 ms each on an idle machine; the bound leaves room for a loaded one
    assert!(took < Duration::from_secs_f64(8.0 * factor()), "{took:?}");
}

#[test]
fn plot_files_drawn_when_first_read() {
    let mut e = CoreEngine::new();
    let t = Instant::now();
    let png = image_path(&ev(&mut e, "plot(sin(x), [x, 0, 6])"));
    let png = Path::new(&png);
    // shown from the PNG and SVG; the column twin is there for a notebook's export
    for ext in ["png", "svg", "json", "column.svg"] {
        assert!(png.with_extension(ext).is_file(), "{ext}");
    }
    // the PDF and the print twins wait until something reads them
    for ext in ["pdf", "print.svg", "print.pdf"] {
        assert!(!png.with_extension(ext).exists(), "{ext}");
    }
    assert!(mc_graphics::deferred::is_pending(png));
    // export_data copies the print twin, drawn now
    let dir = std::env::temp_dir().join(format!("mc-speed-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("p.pdf").to_string_lossy().replace('\\', "/");
    let printed = ev(&mut e, &format!("export_data(\"{out}\", image(\"{}\"))", png.to_string_lossy().replace('\\', "/")));
    assert!(printed.contains("p.pdf"), "{printed}");
    let pdf = std::fs::read(&out).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    assert!(!mc_graphics::deferred::is_pending(png));
    for ext in ["pdf", "print.svg", "print.pdf"] {
        assert!(png.with_extension(ext).is_file(), "{ext}");
    }
    assert_eq!(std::fs::read(png.with_extension("print.pdf")).unwrap(), pdf);
    // a 3D plot: its PDF is drawn by an export too
    let png3 = image_path(&ev(&mut e, "plot3d(sin(x*y), [x, 0, 3], [y, 0, 3])"));
    let png3 = Path::new(&png3);
    assert!(png3.with_extension("scene3d").is_file() && !png3.with_extension("pdf").exists());
    let out3 = dir.join("p3.pdf").to_string_lossy().replace('\\', "/");
    ev(&mut e, &format!("export_data(\"{out3}\", image(\"{}\"))", png3.to_string_lossy().replace('\\', "/")));
    assert!(std::fs::read(&out3).unwrap().starts_with(b"%PDF-"));
    assert!(png3.with_extension("pdf").is_file());
    let _ = std::fs::remove_dir_all(&dir);
    assert!(t.elapsed() < Duration::from_secs_f64(20.0 * factor()), "{:?}", t.elapsed());
}

#[test]
fn a_shown_plot_has_its_size() {
    let mut e = CoreEngine::new();
    let png = image_path(&ev(&mut e, "plot(x, [x, 0, 1], image_size: [300, 200])"));
    assert_eq!(mc_plot::png_size(Path::new(&png)), Some((300, 200)));
    // a removed file has no size, remembered or not
    std::fs::remove_file(&png).unwrap();
    assert_eq!(mc_plot::png_size(Path::new(&png)), None);
}

#[test]
fn packed_arrays_print_as_lists_of_their_elements() {
    let mut e = CoreEngine::new();
    let t = Instant::now();
    // the same array with one element that is not a number is not packed: printed element by element, the general way
    for (def, at, last) in [
        ("n(table((-1)^i * i^3 / 7 * 10^(3*j - 9), [i, 1, 4], [j, 1, 5]))", "[4, 5]", 2),
        ("table((-1)^(i + j) * i * j * 10^k, [i, 3], [j, 2], [k, 0, 2])", "[3, 2, 3]", 3),
        ("[-0.0, 0.0, 1.0*10^-7, 1.0*10^300, -2.5, 3.0]", "[6]", 1),
        ("range(-20, 20)", "[41]", 1),
    ] {
        ev(&mut e, &format!("x = {def};"));
        let packed = ev(&mut e, "to_string(x)");
        let general = ev(&mut e, &format!("to_string(replace_part(x, {at} -> zz))"));
        let leaf = ev(&mut e, &format!("to_string(x{at})"));
        let close = "]".repeat(last);
        let (packed, general, leaf) = (packed.trim_matches('"'), general.trim_matches('"'), leaf.trim_matches('"'));
        let a = packed.strip_suffix(&format!("{leaf}{close}")).unwrap_or_else(|| panic!("{packed} / {leaf}"));
        let b = general.strip_suffix(&format!("zz{close}")).unwrap_or_else(|| panic!("{general}"));
        assert_eq!(a, b, "{def}");
        assert!(a.len() > 10, "{a}");
    }
    // a large raster prints quickly
    ev(&mut e, "r = n(table(sin(i*j/1000), [i, 300], [j, 300]));");
    let t2 = Instant::now();
    assert_eq!(ev(&mut e, "string_length(to_string(r)) > 90000"), "true");
    assert!(t2.elapsed() < Duration::from_secs_f64(2.0 * factor()), "{:?}", t2.elapsed());
    assert!(t.elapsed() < Duration::from_secs_f64(30.0 * factor()), "{:?}", t.elapsed());
}
