//! The 3D fast path: a 3D picture's scene sidecar, opened once and drawn from other view points and zooms without
//! evaluating anything again (what the notebook uses to rotate pictures).
use mc_bench::{Engine, StopFlag};
use mc_core::{Config, Heap};
use mc_engine::CoreEngine;
use mc_graphics::three::orbit;
use mc_graphics::view3d::{orbit_drag, zoom_wheel, Scene3d, View3d, HEADER};
use std::path::{Path, PathBuf};

/// The PNG path of an `image("path")` value.
fn picture(e: &mut CoreEngine, src: &str) -> PathBuf {
    let v = e.eval(src, &StopFlag::new()).expect("evaluates");
    let p = v.printed.strip_prefix("image(\"").and_then(|s| s.strip_suffix("\")")).unwrap_or_else(|| panic!("{src} -> {}", v.printed));
    PathBuf::from(p)
}

fn png_pixels(path: &Path) -> (u32, u32, Vec<u8>) {
    mc_graphics::decode_png(&std::fs::read(path).expect("png")).expect("decodes")
}

/// The mean absolute difference per channel of two pictures of one size.
fn mean_diff(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).map(|(x, y)| (*x as f64 - *y as f64).abs()).sum::<f64>() / a.len() as f64
}

/// The picture of the term printed in `text` with options added (`, view_point: ...`).
fn static_render(text: &str, extra: &str) -> (u32, u32, Vec<u8>) {
    let h = Heap::new(Config::default());
    let text = text.split_once('\n').map_or(text, |(_, rest)| rest);
    let src = format!("{}{extra})", text.trim_end().strip_suffix(')').expect("a call"));
    let t = mc_syntax::read(&h, &src).expect("reads")[0];
    mc_graphics::render_rgba(&h, t, 1.0, &|| false).expect("renders")
}

#[test]
fn plots_and_graphics_in_3d_get_a_scene_and_2d_ones_do_not() {
    let mut e = CoreEngine::new();
    let p = picture(&mut e, "plot3d(x*y, [x, 0, 1], [y, 0, 1], plot_points: 9)");
    assert!(Scene3d::exists_for(&p), "{}", Scene3d::sidecar_of(&p).display());
    let g = picture(&mut e, "graphics3d([red, cuboid([0, 0, 0], [1, 2, 3])])");
    assert!(Scene3d::exists_for(&g));
    let s = picture(&mut e, "show(plot3d(x + y, [x, 0, 1], [y, 0, 1], plot_points: 5), graphics3d(sphere([0, 0, 3], 0.5)))");
    assert!(Scene3d::exists_for(&s));
    let flat = picture(&mut e, "plot(x, [x, 0, 1])");
    assert!(!Scene3d::exists_for(&flat));
    let flat = picture(&mut e, "graphics(disk())");
    assert!(!Scene3d::exists_for(&flat));
}

#[test]
fn the_default_view_redraws_the_picture() {
    let mut e = CoreEngine::new();
    for src in [
        "plot3d(sin(x*y), [x, -2, 2], [y, -2, 2])",
        "graphics3d([blue, sphere([0, 0, 0], 1), red, cuboid([1, 1, 1], [2, 2, 2]), black, line([[0, 0, 0], [2, 2, 2]])], axes: true, axes_label: [\"x\", \"y\", \"z\"])",
        "parametric_plot3d([cos(u)*(2 + cos(v)), sin(u)*(2 + cos(v)), sin(v)], [u, 0, 2*pi], [v, 0, 2*pi])",
    ] {
        let p = picture(&mut e, src);
        let scene = Scene3d::load(&Scene3d::sidecar_of(&p)).expect("opens");
        let (w, h, file) = png_pixels(&p);
        assert_eq!((w, h), (scene.page_size().0.ceil() as u32, scene.page_size().1.ceil() as u32), "{src}");
        let (w2, h2, px) = scene.render(&scene.view(), 1.0);
        assert_eq!((w, h), (w2, h2), "{src}");
        let d = mean_diff(&file, &px);
        assert!(d < 0.5, "{src}: mean difference {d}");
    }
}

#[test]
fn another_view_is_the_picture_with_that_view_point() {
    let mut e = CoreEngine::new();
    let src = "plot3d(exp(-(x^2 + y^2))*cos(2*x), [x, -2, 2], [y, -2, 2], plot_points: 31)";
    let p = picture(&mut e, src);
    let text = std::fs::read_to_string(Scene3d::sidecar_of(&p)).expect("sidecar");
    let scene = Scene3d::from_text(&text, &|| false).expect("prepares");
    let (pw, ph) = scene.page_size();
    let mut vp = scene.view().view_point;
    for k in 0..6 {
        vp = orbit(vp, [0.0, 0.0, 1.0], 0.9, if k % 2 == 0 { 0.35 } else { -0.5 });
        for zoom in [1.0, 1.7] {
            let view = View3d { view_point: vp, zoom, ..scene.view() };
            let fast = scene.render(&view, 1.0);
            if zoom == 1.0 {
                // the same picture as the graphic drawn from that view point on that page, from its term
                let extra = format!(", view_point: [{:?}, {:?}, {:?}], image_size: [{pw:?}, {ph:?}]", vp[0], vp[1], vp[2]);
                let slow = static_render(&text, &extra);
                assert_eq!((fast.0, fast.1), (slow.0, slow.1));
                let d = mean_diff(&fast.2, &slow.2);
                assert!(d < 0.05, "view {k}: mean difference {d}");
            }
            assert_eq!((fast.0 as f64, fast.1 as f64), (pw.ceil(), ph.ceil()), "the page keeps its size");
        }
    }
}

/// The colour at the page's centre.
fn centre(px: &(u32, u32, Vec<u8>)) -> [u8; 3] {
    let (w, h, d) = px;
    let i = ((h / 2) * w + w / 2) as usize * 4;
    [d[i], d[i + 1], d[i + 2]]
}

#[test]
fn the_nearer_face_hides_the_farther_one_from_every_side() {
    // a red square at y = -1 and a blue one at y = 1, both facing the y axis
    let h = Heap::new(Config::default());
    let src = "graphics3d([red, polygon([[-1, -1, -1], [1, -1, -1], [1, -1, 1], [-1, -1, 1]]), blue, polygon([[-1, 1, -1], [1, 1, -1], [1, 1, 1], [-1, 1, 1]])], boxed: false, view_point: [0, -3, 0.2])";
    let t = mc_syntax::read(&h, src).expect("reads")[0];
    let scene = Scene3d::from_term(&h, t, &|| false).expect("prepares");
    let front = scene.render(&scene.view(), 1.0);
    let c = centre(&front);
    assert!(c[0] > 150 && c[2] < 100, "from the front the red square: {c:?}");
    // half a turn about the vertical: the blue square is in front
    let back = View3d { view_point: orbit(scene.view().view_point, [0.0, 0.0, 1.0], std::f64::consts::PI, 0.0), ..scene.view() };
    let c = centre(&scene.render(&back, 1.0));
    assert!(c[2] > 150 && c[0] < 100, "from the back the blue square: {c:?}");
}

#[test]
fn zoom_magnifies_about_the_centre() {
    let h = Heap::new(Config::default());
    let t = mc_syntax::read(&h, "graphics3d([red, sphere([0, 0, 0], 0.25), gray, point([[-1, -1, -1], [1, 1, 1]])], boxed: false)").expect("reads")[0];
    let scene = Scene3d::from_term(&h, t, &|| false).expect("prepares");
    let coloured = |zoom: f64| {
        let (_, _, d) = scene.render(&View3d { zoom, ..scene.view() }, 1.0);
        d.chunks(4).filter(|p| p[0] > 100 && p[1] < 90).count()
    };
    let (one, two, half) = (coloured(1.0), coloured(2.0), coloured(0.5));
    assert!(two as f64 > 2.5 * one as f64, "zoom 2: {two} against {one}");
    assert!((half as f64) < 0.4 * one as f64, "zoom 1/2: {half} against {one}");
    assert_eq!(zoom_wheel(1.0, 0.0), 1.0);
    assert!(zoom_wheel(1.0, 3.0) > 1.3 && zoom_wheel(8.0, 5.0) == 8.0 && zoom_wheel(0.25, -5.0) == 0.25);
}

#[test]
fn dragging_orbits_at_the_same_distance() {
    let v = View3d { view_point: [1.3, -2.4, 2.0], view_vertical: [0.0, 0.0, 1.0], zoom: 1.5 };
    let r = |p: [f64; 3]| (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
    let w = orbit_drag(&v, 120.0, -40.0, 360.0, 300.0);
    assert!((r(w.view_point) - r(v.view_point)).abs() < 1e-12);
    assert_eq!((w.zoom, w.view_vertical), (1.5, v.view_vertical));
    // a whole turn in two drags across the page comes back, about any vertical
    for up in [[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [1.0, 1.0, 1.0]] {
        let v = View3d { view_vertical: up, ..v };
        let back = orbit_drag(&orbit_drag(&v, 360.0, 0.0, 360.0, 300.0), 360.0, 0.0, 360.0, 300.0);
        assert!((0..3).all(|k| (back.view_point[k] - v.view_point[k]).abs() < 1e-9), "{back:?}");
    }
    // turning to the right moves the eye to the left (clockwise seen from above)
    let t = orbit_drag(&View3d { view_point: [0.0, -3.0, 0.0], ..v }, 180.0, 0.0, 360.0, 300.0);
    assert!(t.view_point[0] < -2.9, "{t:?}");
    // tilting stops short of the pole
    let top = orbit_drag(&v, 0.0, 10_000.0, 360.0, 300.0).view_point;
    assert!(top[2] < r(v.view_point) && (top[0] * top[0] + top[1] * top[1]).sqrt() > 1e-3);
    // bad views give a blank page of the right size, and failures from try_render, never panics
    let h = Heap::new(Config::default());
    let t = mc_syntax::read(&h, "graphics3d(point([0, 0, 0]))").expect("reads")[0];
    let scene = Scene3d::from_term(&h, t, &|| false).expect("prepares");
    let good = scene.render(&scene.view(), 1.0);
    for view in [
        View3d { view_point: [0.0; 3], ..v },
        View3d { view_point: [f64::NAN, 1.0, 1.0], ..v },
        View3d { zoom: 0.0, ..v },
        View3d { view_vertical: [0.0; 3], ..v },
    ] {
        assert_eq!(scene.try_render(&view, 1.0, &|| false).unwrap_err().tag, "argument");
        let blank = scene.render(&view, 1.0);
        assert_eq!((blank.0, blank.1), (good.0, good.1));
    }
    assert_eq!(scene.try_render(&scene.view(), 0.0, &|| false).unwrap_err().tag, "argument");
    assert_eq!(Scene3d::from_text("1 + ", &|| false).unwrap_err().tag, "not_graphics");
    assert_eq!(Scene3d::from_text("graphics(disk())", &|| false).unwrap_err().tag, "not_graphics");
    assert_eq!(Scene3d::from_text("mathcas-scene3d 99\ngraphics3d(point([0, 0, 0]))", &|| false).unwrap_err().tag, "not_graphics");
    assert!(Scene3d::from_text(&format!("{HEADER}\ngraphics3d(point([0, 0, 0]))"), &|| false).is_ok());
    assert!(Scene3d::load(Path::new("no-such-dir/plot-0.scene3d")).is_err());
    // a raised stop flag interrupts
    let many = (0..20_000).map(|i| format!("point([{i}, 0, 0])")).collect::<Vec<_>>().join(", ");
    let t = mc_syntax::read(&h, &format!("graphics3d([{many}])")).expect("reads")[0];
    assert_eq!(Scene3d::from_term(&h, t, &|| true).unwrap_err().tag, "interrupted");
}

#[test]
fn turning_a_typical_plot_is_fast() {
    let mut e = CoreEngine::new();
    let p = picture(&mut e, "plot3d(sin(x)*cos(y), [x, -3, 3], [y, -3, 3])");
    let t0 = std::time::Instant::now();
    let scene = Scene3d::cached(&Scene3d::sidecar_of(&p)).expect("opens");
    let open = t0.elapsed();
    // the second time it comes from the cache
    let again = Scene3d::cached(&Scene3d::sidecar_of(&p)).expect("opens");
    assert!(std::sync::Arc::ptr_eq(&scene, &again));
    let mut view = scene.view();
    let frames = 20;
    let t1 = std::time::Instant::now();
    for _ in 0..frames {
        view = orbit_drag(&view, 12.0, 3.0, 360.0, 300.0);
        scene.try_render(&view, 1.5, &|| false).expect("renders");
    }
    let per = t1.elapsed() / frames;
    eprintln!("open {open:?}, {} primitives, {per:?} a frame", scene.len());
    // well under 50 ms in a release build; generous here, since tests run beside each other
    if !cfg!(debug_assertions) {
        assert!(per.as_millis() < 150, "{per:?} a frame");
    }
}

/// Where the time of showing a 3D plot goes (run with `--ignored --nocapture`; for the round's report).
#[test]
#[ignore]
fn three_d_stage_timings() {
    use mc_graphics::Format;
    let mut e = CoreEngine::new();
    let p = picture(&mut e, "plot3d(sin(x*y), [x, -2, 2], [y, -2, 2])");
    let text = std::fs::read_to_string(Scene3d::sidecar_of(&p)).expect("sidecar");
    let text = text.split_once('\n').map_or(text.as_str(), |(_, r)| r).to_string();
    let h = Heap::new(Config::default());
    let t0 = std::time::Instant::now();
    let t = mc_syntax::read(&h, &text).expect("reads")[0];
    let parse = t0.elapsed();
    let time = |f: &dyn Fn()| {
        let t = std::time::Instant::now();
        for _ in 0..5 {
            f();
        }
        t.elapsed() / 5
    };
    let read = time(&|| {
        mc_graphics::model::read(&h, t, &|| false).expect("reads");
    });
    let png = time(&|| {
        mc_graphics::render(&h, t, Format::Png { scale: 1.0 }).expect("png");
    });
    let svg = time(&|| {
        mc_graphics::render(&h, t, Format::Svg).expect("svg");
    });
    let pdf = time(&|| {
        mc_graphics::render(&h, t, Format::Pdf).expect("pdf");
    });
    let all = time(&|| {
        mc_graphics::render_read(&h, t, &[Format::Png { scale: 1.0 }, Format::Svg, Format::Pdf], &|| false).expect("all");
    });
    let print = time(&|| {
        mc_syntax::Printer::new(&h).expr(t);
    });
    eprintln!("term {} bytes: parse {parse:?}; read (prepare + draw) {read:?}; png {png:?}, svg {svg:?}, pdf {pdf:?} (each with a read); all three {all:?}; print {print:?}", text.len());
    let total = std::time::Instant::now();
    for a in 1..=5 {
        picture(&mut e, &format!("plot3d(sin({a}*x*y), [x, -2, 2], [y, -2, 2])"));
    }
    eprintln!("plot3d end to end {:?}", total.elapsed() / 5);
    // the notebook's case: a 640-point page drawn at 1.5 pixels a point while turning
    for (src, label) in [
        ("plot3d(sin(x*y), [x, -2, 2], [y, -2, 2], image_size: 640)", "plot3d 61 x 61"),
        ("plot3d(sin(x*y), [x, -2, 2], [y, -2, 2], image_size: 640, plot_points: 100)", "plot3d 100 x 100"),
        ("graphics3d(table(sphere([i, j, 0], 0.4), i, 1, 4, j, 1, 4), image_size: 640)", "16 spheres"),
    ] {
        let p = picture(&mut e, src);
        let t = std::time::Instant::now();
        let scene = Scene3d::load(&Scene3d::sidecar_of(&p)).expect("loads");
        let load = t.elapsed();
        let mut view = scene.view();
        let t = std::time::Instant::now();
        for _ in 0..10 {
            view = orbit_drag(&view, 15.0, 4.0, 640.0, 400.0);
            scene.try_render(&view, 1.5, &|| false).expect("renders");
        }
        eprintln!("{label}: {} primitives, load {load:?}, {:?} a frame at 1.5", scene.len(), t.elapsed() / 10);
    }
}
