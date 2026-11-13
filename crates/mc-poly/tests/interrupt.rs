//! Long kernels stop promptly when the evaluation is interrupted, and the session goes on.
mod common;
use common::Sys;
use std::time::{Duration, Instant};

/// Run `src`, interrupting it from another thread after `after`; the printed result and the time it took.
fn interrupted(s: &mut Sys, src: &str, after: Duration) -> (String, Duration) {
    let ctl = s.control();
    let start = Instant::now();
    let t = std::thread::spawn(move || {
        while start.elapsed() < after {
            std::thread::yield_now();
        }
        ctl.interrupt();
    });
    let out = s.run(src);
    let took = start.elapsed();
    t.join().unwrap();
    s.control().clear_interrupt();
    (out, took)
}

#[test]
fn long_kernels_stop() {
    let mut s = Sys::new();
    let after = Duration::from_millis(300);
    for src in [
        // minutes of series arithmetic
        "normal(series(exp(sin(x)), x, 0, 20000))",
        // a large sparse product through the heap
        "expand((1 + x + y^3 + z^7 + t^13 + u^29 + v^31)^24)",
        // a long factorisation (many modular factors to recombine)
        "factor(expand((x^400 + 2x + 1)*(x^200 - 3x^2 + 7)*(x^100 + x + 1)*(x^50 + 3)))",
    ] {
        let (out, took) = interrupted(&mut s, src, after);
        assert!(took < Duration::from_secs(5), "{src} stopped after {took:?}");
        // a machine fast enough to finish before the interrupt was sent has nothing to stop
        if took < after {
            continue;
        }
        assert!(out.starts_with("failure(interrupted"), "{src}: {}", &out[..out.len().min(200)]);
    }
    // the session is usable afterwards
    assert_eq!(s.run("expand((x + 1)^2)"), "1 + 2*x + x^2");
}
