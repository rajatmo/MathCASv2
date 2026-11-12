//! The list producers (item P): `random_real`, `random_normal` and `random_integer` with a count give exactly
//! the draws of the session stream, in row-major order, whether the result is packed (at least 64 leaves) or not,
//! and the same at 1 and 4 threads and under the stress collector.
mod common;
use common::Sys;
use mc_arith::num::Int;
use mc_random::Generator;

/// The leaves of a printed (possibly nested) list of numbers, row-major.
fn leaves(s: &str) -> Vec<f64> {
    let flat: String = s.chars().filter(|&c| c != '[' && c != ']').collect();
    if flat.trim().is_empty() {
        return vec![];
    }
    flat.split(", ")
        .map(|x| x.trim().trim_end_matches('.').parse::<f64>().unwrap_or_else(|_| panic!("not a number: {x}")))
        .collect()
}

fn stream_of(seed: i64) -> Generator {
    Generator::seeded(Int::from_i64(seed))
}

fn bits(v: &[f64]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

const SIZES: &[usize] = &[0, 1, 2, 5, 63, 64, 65, 127, 200, 1001];

#[test]
fn uniform_lists_are_the_stream() {
    let mut s = Sys::new();
    for (k, &n) in SIZES.iter().enumerate() {
        let seed = 100 + k as i64;
        for (range, a, b) in [("1.0", 0.0, 1.0), ("[-3, 7.5]", -3.0, 7.5), ("2", 0.0, 2.0), ("[0, pi]", 0.0, std::f64::consts::PI)] {
            let got = leaves(&s.run(&format!("seed({seed}); random_real({range}, {n})")));
            let mut g = stream_of(seed);
            let want: Vec<f64> = (0..n).map(|_| g.stream().real_in(a, b - a)).collect();
            assert_eq!(bits(&got), bits(&want), "random_real({range}, {n})");
        }
    }
}

#[test]
fn normal_lists_are_the_stream() {
    let mut s = Sys::new();
    for (k, &n) in SIZES.iter().enumerate() {
        let seed = 200 + k as i64;
        for (mu, sigma) in [(0.0, 1.0), (10.0, 2.5), (-1.0, 0.0)] {
            let got = leaves(&s.run(&format!("seed({seed}); random_normal({mu:?}, {sigma:?}, {n})")));
            let mut g = stream_of(seed);
            let mut want = vec![];
            while want.len() < n {
                let (x, y) = g.stream().normal_pair();
                want.push(mu + sigma * x);
                if want.len() < n {
                    want.push(mu + sigma * y);
                }
            }
            assert_eq!(bits(&got), bits(&want), "random_normal({mu}, {sigma}, {n})");
        }
    }
}

#[test]
fn integer_lists_are_the_stream() {
    let mut s = Sys::new();
    for (k, &n) in SIZES.iter().enumerate() {
        let seed = 300 + k as i64;
        for (range, a, b) in [("[1, 6]", 1, 6), ("10", 0, 10), ("[-2^62, 2^62]", -(1i64 << 62), 1 << 62), ("[-2^63, 2^63 - 1]", i64::MIN, i64::MAX)] {
            let out = s.run(&format!("seed({seed}); random_integer({range}, {n})"));
            let flat: String = out.chars().filter(|&c| c != '[' && c != ']').collect();
            let got: Vec<i64> = if flat.is_empty() { vec![] } else { flat.split(", ").map(|x| x.parse().unwrap()).collect() };
            let mut g = stream_of(seed);
            let want: Vec<i64> = (0..n).map(|_| g.stream().int_in(a, b)).collect();
            assert_eq!(got, want, "random_integer({range}, {n})");
        }
    }
    // a range beyond machine integers still draws big integers
    let out = s.run("seed(5); random_integer([10^30, 10^30 + 5], 100)");
    assert_eq!(out.matches("1000000000000000000000000000").count(), 100, "{out}");
}

#[test]
fn shapes_are_row_major() {
    let mut s = Sys::new();
    for (dims, total) in [("[3, 4]", 12), ("[10, 20]", 200), ("[2, 3, 50]", 300), ("[1, 64]", 64), ("[64, 1]", 64)] {
        for f in ["random_real(1.0, {d})", "random_normal(0, 1, {d})", "random_integer([0, 9], {d})"] {
            let nested = s.run(&format!("seed(9); {}", f.replace("{d}", dims)));
            let flat = s.run(&format!("seed(9); {}", f.replace("{d}", &total.to_string())));
            assert_eq!(leaves(&nested).iter().map(|x| x.to_bits()).collect::<Vec<_>>(), bits(&leaves(&flat)), "{f} {dims}");
            let d: Vec<&str> = dims.trim_matches(['[', ']']).split(", ").collect();
            let x = format!("seed(9); x = {}; ", f.replace("{d}", dims));
            assert_eq!(s.run(&format!("{x}length(x)")), d[0], "{f} {dims}");
            assert_eq!(s.run(&format!("{x}length(x[1])")), d[1], "{f} {dims}");
        }
    }
    // empty dimensions
    assert_eq!(s.run("random_real(1.0, [2, 0])"), "[[], []]");
    assert_eq!(s.run("random_integer(3, [0, 5])"), "[]");
    assert_eq!(s.run("random_normal(0, 1, 0)"), "[]");
}

/// Packed results behave as the lists they are: element access, arithmetic, sums, equality with the same draws made
/// one at a time (scalar draws, `table`).
#[test]
fn packed_results_are_lists() {
    let mut s = Sys::new();
    assert_eq!(s.run("seed(4); a = random_real(1.0, 500); seed(4); b = table(random_real(), 500); a === b"), "true");
    assert_eq!(s.run("seed(4); a = random_integer([1, 6], 500); seed(4); b = table(random_integer([1, 6]), 500); a === b"), "true");
    assert_eq!(s.run("a[1] == b[1] and length(a + b) == 500 and total(a) == total(b)"), "true");
    assert_eq!(s.run("x = random_real([2, 3], 1000); min(x) >= 2 and max(x) < 3"), "true");
}

#[test]
fn identical_at_one_and_four_threads_and_under_stress() {
    let prog = "seed(77); [random_real(1.0, 300)[150], total(random_integer([1, 6], [20, 30])), \
                random_normal(0, 1, [7, 9]), length(random_real([1, 2], 10^5)), random_real(1.0, 70)]";
    let one = Sys::with_threads(1, false).run(prog);
    assert_eq!(Sys::with_threads(4, false).run(prog), one);
    assert_eq!(Sys::with_threads(1, true).run(prog), one);
    assert_eq!(Sys::with_threads(4, true).run(prog), one);
}
