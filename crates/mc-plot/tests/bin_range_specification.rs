//! The bin specification [a, b, dx] means the same bins in every function that reads it: the bins
//! [a + k dx, a + (k + 1) dx) that lie within [a, b], so floor((b - a)/dx) of them. Checked for bin_counts, bin_lists
//! (values, labels and positions), the grids of bin_counts and bin_lists, histogram_list and histogram, on random
//! specifications whose step divides the range, does not divide it, or exceeds it, with negative data and data on
//! the edges, against counts made from the definition.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// The bars of a histogram (centre, height) from its data sidecar.
fn bars(e: &mut CoreEngine, src: &str) -> Vec<(f64, f64)> {
    let printed = eval(e, src);
    let path = printed.strip_prefix("image(\"").and_then(|s| s.strip_suffix("\")")).unwrap_or_else(|| panic!("{src} -> {printed}"));
    let json = std::path::Path::new(path).with_extension("json");
    let j: serde_json::Value = serde_json::from_slice(&std::fs::read(json).expect("the sidecar exists")).expect("JSON");
    j["series"][0]["points"].as_array().unwrap().iter().map(|p| (p[0].as_f64().unwrap(), p[1].as_f64().unwrap())).collect()
}

struct Lcg(u64);

impl Lcg {
    fn int(&mut self, lo: i64, hi: i64) -> i64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        lo + ((self.0 >> 33) % (hi - lo + 1) as u64) as i64
    }
}

fn list(v: &[String]) -> String {
    format!("[{}]", v.join(", "))
}

/// Halves written exactly: the numbers of a case are integers over 2.
fn half(x: i64) -> String {
    if x % 2 == 0 {
        format!("{}", x / 2)
    } else {
        format!("{x}/2")
    }
}

/// One case in doubled units: the spec [a, b, d] and the data xs; checks every reader against the definition.
fn check(e: &mut CoreEngine, a: i64, b: i64, d: i64, xs: &[i64]) {
    let nbins = ((b - a) / d) as usize;
    let bin = |x: i64| -> Option<usize> { (x >= a && x < a + nbins as i64 * d).then(|| ((x - a) / d) as usize) };
    let mut counts = vec![0usize; nbins];
    let mut members: Vec<Vec<usize>> = vec![vec![]; nbins];
    for (i, &x) in xs.iter().enumerate() {
        if let Some(k) = bin(x) {
            counts[k] += 1;
            members[k].push(i);
        }
    }
    let inside = xs.iter().filter(|&&x| bin(x).is_some()).count();
    assert_eq!(counts.iter().sum::<usize>(), inside);
    let spec = format!("[{}, {}, {}]", half(a), half(b), half(d));
    let data = list(&xs.iter().map(|&x| half(x)).collect::<Vec<_>>());
    let what = format!("{data} in {spec}");
    let want_counts = list(&counts.iter().map(|c| c.to_string()).collect::<Vec<_>>());
    assert_eq!(eval(e, &format!("bin_counts({data}, {spec})")), want_counts, "bin_counts {what}");
    let want_lists = list(&members.iter().map(|m| list(&m.iter().map(|&i| half(xs[i])).collect::<Vec<_>>())).collect::<Vec<_>>());
    assert_eq!(eval(e, &format!("bin_lists({data}, {spec})")), want_lists, "bin_lists {what}");
    let want_pos = list(&members.iter().map(|m| list(&m.iter().map(|&i| (i + 1).to_string()).collect::<Vec<_>>())).collect::<Vec<_>>());
    assert_eq!(eval(e, &format!("bin_lists({data} -> automatic, {spec})")), want_pos, "bin_lists positions {what}");
    // the grid readers with one bin in the second coordinate, [0, 1, 1]
    let pts = list(&xs.iter().map(|&x| format!("[{}, 1/2]", half(x))).collect::<Vec<_>>());
    let want_grid = list(&counts.iter().map(|c| format!("[{c}]")).collect::<Vec<_>>());
    assert_eq!(eval(e, &format!("bin_counts({pts}, {spec}, [0, 1, 1])")), want_grid, "bin_counts grid {what}");
    // histogram_list: the edges a + k d and the counts
    let edges = list(&(0..=nbins as i64).map(|k| half(a + k * d)).collect::<Vec<_>>());
    assert_eq!(eval(e, &format!("histogram_list({data}, {spec})")), format!("[{edges}, {want_counts}]"), "histogram_list {what}");
    // histogram: a bar over each occupied bin, its height the count
    let want_bars: Vec<(f64, f64)> =
        counts.iter().enumerate().filter(|(_, &c)| c > 0).map(|(k, &c)| ((a as f64 + (k as f64 + 0.5) * d as f64) / 2.0, c as f64)).collect();
    let got = bars(e, &format!("histogram({data}, {spec})"));
    assert_eq!(got.len(), want_bars.len(), "histogram {what}: {got:?}");
    for (g, w) in got.iter().zip(&want_bars) {
        assert!((g.0 - w.0).abs() < 1e-9 && g.1 == w.1, "histogram {what}: {got:?} vs {want_bars:?}");
    }
}

#[test]
fn a_step_that_does_not_divide_the_range_gives_only_the_bins_within_it() {
    let mut e = CoreEngine::new();
    assert_eq!(eval(&mut e, "bin_counts([1, 2, 5, 9, 9.5, 10], [0, 10, 3])"), "[2, 1, 0]");
    assert_eq!(eval(&mut e, "histogram_list([1, 2, 5, 9, 9.5, 10], [0, 10, 3])"), "[[0, 3, 6, 9], [2, 1, 0]]");
    assert_eq!(eval(&mut e, "bin_lists([1, 2, 5, 9, 9.5, 10], [0, 10, 3])"), "[[1, 2], [5], []]");
    // the step dividing the range: the upper end b is outside the last bin [b - dx, b)
    assert_eq!(eval(&mut e, "bin_counts([0, 5, 10], [0, 10, 5])"), "[1, 1]");
    // a step larger than the range: no bins
    assert_eq!(eval(&mut e, "bin_counts([1, 2], [0, 1, 5])"), "[]");
    assert_eq!(eval(&mut e, "histogram_list([1, 2], [0, 1, 5])"), "[[0], []]");
    // machine numbers: [0, 0.3, 0.1] has three bins, 0.3 itself outside them
    assert_eq!(eval(&mut e, "bin_counts([0.05, 0.15, 0.25, 0.3], [0, 0.3, 0.1])"), "[1, 1, 1]");
    assert_eq!(eval(&mut e, "histogram_list([0.05, 0.15, 0.25, 0.3], [0, 0.3, 0.1])"), "[[0.0, 0.1, 0.2, 0.3], [1, 1, 1]]");
    assert_eq!(bars(&mut e, "histogram([0.05, 0.15, 0.25, 0.3], [0, 0.3, 0.1])").len(), 3);
    assert_eq!(bars(&mut e, "histogram([0.05, 0.15, 0.25, 0.3], [0, 0.3, 0.1])").iter().map(|b| b.1).sum::<f64>(), 3.0);
}

#[test]
fn every_reader_of_a_bin_range_agrees_with_the_definition() {
    let mut e = CoreEngine::new();
    let mut g = Lcg(5);
    for case in 0..60 {
        let a = g.int(-12, 6);
        // dividing, not dividing, larger than the range
        let d = g.int(2, 7);
        let b = match case % 3 {
            0 => a + d * g.int(1, 5),
            1 => a + d * g.int(1, 4) + g.int(1, d - 1),
            _ => a + g.int(1, d - 1),
        };
        let n = g.int(0, 12) as usize;
        let mut xs: Vec<i64> = (0..n).map(|_| g.int(a - 6, b + 6)).collect();
        // data on the edges: a, b and an inner edge
        xs.extend([a, b, a + d]);
        check(&mut e, a, b, d, &xs);
    }
}
