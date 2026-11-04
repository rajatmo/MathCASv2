//! Statistical smoke tests of the session's draws (fixed seeds, so they are deterministic; the tolerances are about
//! five standard errors) and draw rates.
mod common;
use common::Sys;
use std::time::Instant;

fn mean_var(xs: &[f64]) -> (f64, f64) {
    let n = xs.len() as f64;
    let m = xs.iter().sum::<f64>() / n;
    (m, xs.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (n - 1.0))
}

/// Pearson's chi-square statistic of observed counts against equal expected counts.
fn chi2(counts: &[u64]) -> f64 {
    let n: u64 = counts.iter().sum();
    let e = n as f64 / counts.len() as f64;
    counts.iter().map(|&c| (c as f64 - e) * (c as f64 - e) / e).sum()
}

#[test]
fn uniform_reals() {
    let mut s = Sys::new();
    for seed in [1, 2, 3] {
        let xs = s.numbers(&format!("seed({seed}); random_real([0, 1], 10^5)"));
        assert_eq!(xs.len(), 100_000);
        assert!(xs.iter().all(|x| (0.0..1.0).contains(x)));
        let (m, v) = mean_var(&xs);
        // standard errors: 1/sqrt(12 n) = 0.0009 for the mean, about 0.0003 for the variance
        assert!((m - 0.5).abs() < 0.0046, "seed {seed}: mean {m}");
        assert!((v - 1.0 / 12.0).abs() < 0.0015, "seed {seed}: variance {v}");
        // chi-square on 10 bins, 9 degrees of freedom: 99.9% quantile 27.9
        let mut bins = [0u64; 10];
        for x in &xs {
            bins[(x * 10.0) as usize] += 1;
        }
        let c = chi2(&bins);
        assert!(c < 27.9, "seed {seed}: chi-square {c} for {bins:?}");
        // successive pairs on a 4 x 4 grid, 15 degrees of freedom: 99.9% quantile 37.7
        let mut grid = [0u64; 16];
        for p in xs.as_chunks::<2>().0 {
            grid[(p[0] * 4.0) as usize * 4 + (p[1] * 4.0) as usize] += 1;
        }
        let c = chi2(&grid);
        assert!(c < 37.7, "seed {seed}: pair chi-square {c}");
    }
    // a scaled range
    let xs = s.numbers("seed(4); random_real([-3, 5], 10^5)");
    let (m, v) = mean_var(&xs);
    assert!((m - 1.0).abs() < 0.04 && (v - 64.0 / 12.0).abs() < 0.1, "{m} {v}");
}

#[test]
fn normal_reals() {
    let mut s = Sys::new();
    for seed in [1, 2] {
        let xs = s.numbers(&format!("seed({seed}); random_normal(0, 1, 10^5)"));
        let (m, v) = mean_var(&xs);
        // standard errors 0.0032 (mean) and 0.0045 (variance)
        assert!(m.abs() < 0.016, "seed {seed}: mean {m}");
        assert!((v - 1.0).abs() < 0.023, "seed {seed}: variance {v}");
        // the fraction within one standard deviation: 0.6827, standard error 0.0015
        let inside = xs.iter().filter(|x| x.abs() < 1.0).count() as f64 / xs.len() as f64;
        assert!((inside - 0.6827).abs() < 0.0075, "seed {seed}: {inside}");
        // fourth moment 3
        let k = xs.iter().map(|x| x.powi(4)).sum::<f64>() / xs.len() as f64;
        assert!((k - 3.0).abs() < 0.12, "seed {seed}: kurtosis {k}");
    }
    let xs = s.numbers("seed(3); random_normal(10, 2, 10^5)");
    let (m, v) = mean_var(&xs);
    assert!((m - 10.0).abs() < 0.032 && (v - 4.0).abs() < 0.09, "{m} {v}");
    // scalar draws follow the same distribution
    let xs = s.numbers("seed(4); table(random_normal(), 20000)");
    let (m, v) = mean_var(&xs);
    assert!(m.abs() < 0.036 && (v - 1.0).abs() < 0.05, "{m} {v}");
}

#[test]
fn dice() {
    let mut s = Sys::new();
    let xs = s.numbers("seed(6); random_integer([1, 6], 6*10^4)");
    let mut counts = [0u64; 6];
    for x in xs {
        counts[x as usize - 1] += 1;
    }
    // each count 10^4 with standard deviation 91
    for c in counts {
        assert!((c as i64 - 10_000).abs() < 460, "{counts:?}");
    }
    // 5 degrees of freedom: 99.9% quantile 20.5
    assert!(chi2(&counts) < 20.5, "{counts:?}");
    // a range that does not divide 2^64 evenly, far from small: [0, 3 * 2^62 - 1] split in 3 thirds
    let xs = s.numbers("seed(7); map(k => floor(k / 2^62), random_integer(3*2^62 - 1, 30000))");
    let mut thirds = [0u64; 3];
    for x in xs {
        thirds[x as usize] += 1;
    }
    assert!(chi2(&thirds) < 13.8, "{thirds:?}");
    // choices
    let xs = s.numbers("seed(8); random_choice([0, 1, 2, 3, 4], 50000)");
    let mut counts = [0u64; 5];
    for x in xs {
        counts[x as usize] += 1;
    }
    assert!(chi2(&counts) < 18.5, "{counts:?}");
}

#[test]
fn permutations_are_uniform() {
    // the 6 permutations of 3 elements, 6000 times
    let mut s = Sys::new();
    let out = s.run("seed(9); table(random_sample([0, 1, 2]), 6000)");
    let mut counts = std::collections::BTreeMap::new();
    for p in out.trim_start_matches("[[").trim_end_matches("]]").split("], [") {
        *counts.entry(p.to_string()).or_insert(0u64) += 1;
    }
    assert_eq!(counts.len(), 6, "{counts:?}");
    let c: Vec<u64> = counts.values().copied().collect();
    assert!(chi2(&c) < 20.5, "{counts:?}");
}

/// Draw rates (printed; run with `--nocapture`). The bounds only catch a gross regression.
#[test]
fn speed() {
    let mut s = Sys::new();
    s.run("seed(1)");
    let t = Instant::now();
    s.run("length(random_real([0, 1], 10^6))");
    let list = 1e6 / t.elapsed().as_secs_f64();
    let t = Instant::now();
    s.run("length(random_integer([1, 6], 10^6))");
    let ints = 1e6 / t.elapsed().as_secs_f64();
    let t = Instant::now();
    s.run("length(random_normal(0, 1, 10^6))");
    let normals = 1e6 / t.elapsed().as_secs_f64();
    let t = Instant::now();
    s.run("{ var x = 0.0; for k in range(10^5) { x = x + random_real() }; x }");
    let scalar = 1e5 / t.elapsed().as_secs_f64();
    let t = Instant::now();
    s.run("length(table(random_real(), 10^5))");
    let table = 1e5 / t.elapsed().as_secs_f64();
    // the generator alone
    let mut g = mc_random::Stream::from_key(1);
    let t = Instant::now();
    let mut acc = 0.0;
    for _ in 0..10_000_000 {
        acc += g.uniform();
    }
    let raw = 1e7 / t.elapsed().as_secs_f64();
    assert!(acc > 0.0);
    println!(
        "draws per second: random_real([0, 1], 10^6) {list:.3e}, random_integer([1, 6], 10^6) {ints:.3e}, \
         random_normal(0, 1, 10^6) {normals:.3e}, scalar loop {scalar:.3e}, table(random_real(), 10^5) {table:.3e}, \
         Stream::uniform {raw:.3e}"
    );
    assert!(list > 1e5 && scalar > 1e4 && raw > 1e7);
}
