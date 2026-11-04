//! Timings of package 1A's functions at large sizes (optimised builds; run under the guard with `--ignored`).
//! Each case prints its time and `[k/N pct%]` progress, and checks its result.
mod common;
use common::Env;
use std::time::Instant;

/// (setup, measured statement, expected printed result)
const CASES: &[(&str, &str, &str)] = &[
    ("", "length(permutations(range(9)))", "362880"),
    ("", "length(subsets(range(20)))", "1048576"),
    ("", "length(tuples(range(10), 6))", "1000000"),
    ("fn g(k) = imod(7919k, 1000)\nxs = array(g, 10^6)", "length(tally(xs))", "1000"),
    ("fn g(k) = imod(7919k, 1000)\nxs = array(g, 10^5)", "length(gather_by(xs, g))", "1000"),
    ("fn g(k) = imod(7919k, 1000)\nxs = array(g, 10^5)", "ordered(map(g, sort_by(xs, g)))", "true"),
    ("zs = reverse(range(10^6))", "ordering(zs)[1]", "1000000"),
    ("ys = range(10^6)", "length(position(ys, _: Int))", "1000000"),
    ("ys = range(10^6)", "free_of(ys, x)", "true"),
    ("ys = range(10^6)", "length(partition(ys, 2, 1))", "999999"),
    ("", "length(table(k^2, k, 10^5))", "100000"),
    ("", "length(table(i + j, i, 300, j, 300))", "300"),
    ("", "length(outer(imul, range(1000), range(1000)))", "1000"),
    ("fn g(i, j) = imod(i*j, 10)\nm = array(g, [100, 100])", "length(inner(mul, m, m))", "100"),
    ("", "length(constant_array(0, [1000, 1000]))", "1000"),
];

#[test]
#[ignore = "a measurement: run optimised under the guard"]
fn measure_package_1a() {
    let n = CASES.len();
    for (k, (setup, measure, want)) in CASES.iter().enumerate() {
        let mut env = Env::new();
        if !setup.is_empty() {
            let r = env.eval(setup);
            assert!(!r.starts_with("status") && !r.starts_with("failure"), "{setup}: {r}");
        }
        let t0 = Instant::now();
        let r = env.eval(measure);
        let dt = t0.elapsed();
        println!("[{}/{n} {}%] {measure}: {:.1} ms", k + 1, 100 * (k + 1) / n, dt.as_secs_f64() * 1e3);
        assert_eq!(r, *want, "{measure}");
    }
}
