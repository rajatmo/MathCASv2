//! Drawing a picture honours an interrupt: the sampling, the meshes and the writing of the PNG, SVG and PDF files
//! poll the stop flag, so a stop raised 200 ms into a heavy plot ends it promptly with the interrupt (not with a
//! picture, and not seconds later). Each input is first run without a stop to show that it is heavy (it takes longer
//! than the time at which the stop is raised), then with the stop.
use mc_api::StopFlag;
use mc_engine::{CoreEngine, EngineConfig, Origin};
use std::time::{Duration, Instant};

/// When the stop is raised.
const RAISE: Duration = Duration::from_millis(200);

/// The time factor of a slow test machine (`MC_TEST_TIME_FACTOR`, default 1).
fn factor() -> u32 {
    std::env::var("MC_TEST_TIME_FACTOR").ok().and_then(|s| s.parse().ok()).unwrap_or(1).max(1)
}

/// Evaluate `src` in a fresh engine with a stop raised after `raise` (none: no stop): the time taken and the last
/// value printed (or the error).
fn run(src: &str, raise: Option<Duration>) -> (Duration, String) {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 4, ..EngineConfig::default() });
    // the session's start (prelude, first plot's set-up) is not part of the measurement
    let _ = e.eval_in("plot(x, [x, 0, 1])", &Origin::input(), &StopFlag::new());
    let stop = StopFlag::new();
    let dog = raise.map(|d| {
        let s = stop.clone();
        std::thread::spawn(move || {
            std::thread::park_timeout(d);
            s.raise();
        })
    });
    let t0 = Instant::now();
    let out = match e.eval_in(src, &Origin::input(), &stop) {
        Ok(r) => r.last.printed,
        Err(err) => err.to_string(),
    };
    let took = t0.elapsed();
    if let Some(d) = dog {
        let _ = d.join();
    }
    (took, out)
}

/// Heavy pictures of every stage: many samples, fine meshes in 2D and 3D, contours, and graphics with many
/// primitives whose writing to the three formats dominates.
const HEAVY: &[&str] = &[
    "plot3d(sin(x*y) + bessel_j(0, x^2 + y^2), [x, -6, 6], [y, -6, 6], plot_points: 260, max_recursion: 2)",
    "contour_plot(sin(x^2 + y^3) * cos(x*y), [x, -4, 4], [y, -4, 4], plot_points: 200, contours: 40)",
    "graphics(table(disk([k, sin(k)], 0.3), k, range(100000)))",
    "list_plot(table([k, sin(k/100.0)], k, range(200000)))",
    "graphics3d(table(sphere([k, sin(k), cos(k)], 0.2), k, range(1200)))",
];

/// A stop raised 200 ms into a heavy plot ends it within a few hundred milliseconds, with the interrupt.
#[test]
fn heavy_pictures_stop_at_an_interrupt() {
    let slack = Duration::from_millis(600) * factor();
    let mut report = vec![];
    for src in HEAVY {
        let (full, _) = run(src, None);
        if full < RAISE * 2 {
            report.push(format!("not heavy enough ({full:?}): {src}"));
            continue;
        }
        let (took, out) = run(src, Some(RAISE));
        eprintln!("{full:?} without a stop, {took:?} with one: {src}");
        // the allowance grows with the run's own length (a loaded machine slows both runs alike); a stop that is
        // not polled ends with the picture (checked below) after about the full time, beyond this bound
        let allowed = slack.max(full.saturating_sub(RAISE) / 2);
        if took > RAISE + allowed {
            report.push(format!("stopped after {took:?} (without a stop {full:?}): {src}"));
        }
        // the engine reports an interrupted evaluation as stopped
        if !(out.contains("interrupted") || out.contains("stopped")) || out.starts_with("image(") {
            report.push(format!("ended with {} instead of the interrupt: {src}", out.chars().take(120).collect::<String>()));
        }
    }
    assert!(report.is_empty(), "{}", report.join("\n"));
}
