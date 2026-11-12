//! Sessions: identical results at 1 and 4 threads and under the stress collector; interrupts stop long exact
//! eliminations; the size target for machine LU and inverse.
mod common;
use common::Sys;
use std::time::{Duration, Instant};

const CASES: &[&str] = &[
    "det([[1, -5, 3], [-8, -7, 8], [-6, 2, 9]])",
    "inverse([[a, b], [c, d]])",
    "eigensystem([[1, 2], [3, 4]])",
    "eigenvalues([[0, 1, 0], [0, 0, 1], [1, 1, 0]])",
    "null_space([[4, 1, 1, 1, 3], [3, -3, -3, 0, 3], [10, -5, -5, 1, 9], [9, 6, 6, 3, 6]])",
    "seed(2); a = random_real(1, [70, 70]); total(dot(inverse(a), a))",
    "seed(2); b = random_real(1, [70, 70]); eigenvalues(b + transpose(b))[1]",
    "qr([[1, 2], [3, 4]])",
    "singular_values([[1.0, 2], [3, 4], [5, 6]])",
    "map(m => det(m), table(table(table(i^k + j, i, 3), j, 3), k, 30))",
];

#[test]
fn identical_across_threads_and_collectors() {
    let reference: Vec<String> = {
        let mut s = Sys::new();
        CASES.iter().map(|c| s.run(c)).collect()
    };
    for (threads, stress) in [(4, false), (1, true), (4, true)] {
        let mut s = Sys::with_threads(threads, stress);
        for (c, want) in CASES.iter().zip(&reference) {
            assert_eq!(&s.run(c), want, "{c} at {threads} threads, stress {stress}");
        }
    }
}

#[test]
fn interrupts_stop_long_eliminations() {
    let mut s = Sys::new();
    // an exact 220 x 220 determinant with big entries runs for many seconds
    s.run("seed(9); m = table(random_integer(10^30), i, 220, j, 220)");
    for src in ["det(m)", "inverse(m)", "det(table(random_integer(9)*x + random_integer(9)*y + random_integer(9)*z, i, 40, j, 40))"] {
        let (r, took) = s.interrupted(src, Duration::from_millis(300));
        assert!(took < Duration::from_secs(3), "{src}: took {took:?}");
        match r {
            Ok(out) => assert!(out.starts_with("failure(interrupted"), "{src}: {out}"),
            Err(e) => assert!(e.to_string().contains("interrupt") || e.to_string().contains("stopped"), "{src}: {e}"),
        }
        assert_eq!(s.run("det([[1, 2], [3, 4]])"), "-2");
    }
}

#[test]
fn machine_lu_and_inverse_at_500() {
    let mut s = Sys::new();
    s.run("seed(1); b = random_real(1, 500)");
    let mut best = Duration::MAX;
    for _ in 0..3 {
        // a fresh matrix each time, so that no result comes from the memo
        s.run("a = random_real(1, [500, 500])");
        let t = Instant::now();
        s.run("x = inverse(a); d = det(a); y = linear_solve(a, b)");
        best = best.min(t.elapsed());
    }
    println!("500 x 500 inverse + det + linear_solve: {best:?}");
    // the target is well under a second; the bound leaves room for a busy machine
    assert!(best < Duration::from_secs(2), "{best:?}");
    s.holds("norm(dot(a, y) - b, inf) < 1e-10");
}
