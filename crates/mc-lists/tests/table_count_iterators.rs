//! table with a count iterator that depends on an earlier iterator.
//! A test that still fails is kept ignored with the reason in its `ignore`; removing the `ignore` is the proof of a fix.
#![allow(dead_code)]
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate on a detached thread (one worker thread, fresh session) and give up after `secs` seconds: a hang that
/// ignores the stop flag cannot block the test run. `None` when the evaluation did not finish in time.
fn ev_within(src: &str, secs: u64) -> Option<String> {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::spawn(move || {
        let stop = StopFlag::new();
        let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
        let out = match e.eval(&s, &stop) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        let _ = tx.send(out);
    });
    rx.recv_timeout(std::time::Duration::from_secs(secs)).ok()
}

fn ev(src: &str) -> String {
    ev_within(src, 30).unwrap_or_else(|| "TIMEOUT".to_string())
}

#[test]
fn trailing_count_bound_by_an_earlier_iterator() {
    // the last argument repeats the body as many times as the bound value of j
    assert_eq!(ev("table(x, j, 3, j)"), "[[x], [x, x], [x, x, x]]");
    assert_eq!(ev("table(i, j, 2, i, [-1, 3], j)"), "[[[-1], [3]], [[-1, -1], [3, 3]]]");
    // a symbol at the end that no iterator binds stays a non-iterator
    assert_eq!(ev("table(x, k)"), "table(x, k)");
}

#[test]
fn ulam_spiral_from_counted_steps() {
    // the steps of the square spiral: +-1 and +-n alternating with run lengths 1, 1, 2, 2, ...; the spiral of side 3
    // starts at the centre 5 and winds outwards (checked by hand)
    let def = "ulam(n_) := partition(permute(range(n^2), accumulate(take(flatten([[n^2 + 1] / 2, \
               table((-1)^j * i, j, n, i, [-1, n], j)]), n^2))), n)";
    assert_eq!(ev(&format!("{def}; ulam(3)")), "[[5, 4, 3], [6, 1, 2], [7, 8, 9]]");
    assert_eq!(ev(&format!("{def}; dimensions(ulam(31))")), "[31, 31]");
}
