//! Timing of simplify-heavy inputs (decision s-cost.4): `cargo test --release -p mc-simplify --test cost_bench --
//! --ignored --nocapture`. Each input runs in a fresh session (no memo hits), best of [`REPS`]; prints
//! `[k/N pct%] ms  result-size  input`.
mod common;
use common::Sys;
use std::time::Instant;

const REPS: usize = 3;

const CASES: &[&str] = &[
    "simplify(sin(x)^2 + cos(x)^2)",
    "simplify(sin(x)^6 + cos(x)^6)",
    "simplify(d(sin(x)/x, x))",
    "simplify(sin(x)*cos(y) + cos(x)*sin(y))",
    "simplify((x^3 - y^3)/(x - y) - (x^2 + x*y + y^2))",
    "simplify(1/(x + 1) + 1/(x - 1) + 1/(x + 2) + 1/(x - 2))",
    "simplify(expand((1 + x + y + z)^12))",
    "simplify(expand((1 + x + y + z)^20))",
    "simplify(expand((3 + 1234567*x)^15))",
    "simplify(expand((x - 1)^8*(x + 2)^5)/(x^2 + x - 2)^3)",
    "simplify(sum(k*x^k, [k, 1, 30])/(x - 1))",
    "simplify(trig_expand(sin(10*x)*cos(7*x)))",
    "simplify(tan(x)^4 + 2*tan(x)^2 + 1)",
    "simplify(d(atan(x/sqrt(1 - x^2)), x))",
    "simplify(d(d(log((1 + x)/(1 - x))*exp(x)/(1 + x^2), x), x))",
    "simplify(expand((1 + x + y + z)^30))",
    "simplify(sum(1/(x + k), [k, 1, 12]))",
    "simplify(trig_reduce(sin(x)^12*cos(x)^8))",
    "simplify(expand((a + b)^10*(a - b)^10))",
    "simplify(expand((123456789*x^2 + 987654321*y - 1)^8))",
    "simplify(integrate(x^3*exp(x)*sin(x), x))",
    "simplify(integrate(1/(x^4 + 1), x))",
    "simplify(integrate(x^2/(x^3 - 2*x + 1), x))",
    "integrate(x^5*exp(2*x)*cos(3*x), x)",
    "integrate((x^4 + 3*x^2 + 1)/(x^5 - x + 7), x)",
];

#[test]
#[ignore]
fn simplify_timings() {
    let n = CASES.len();
    let mut total = 0.0;
    for (k, src) in CASES.iter().enumerate() {
        let mut best = f64::INFINITY;
        let mut out = String::new();
        for _ in 0..REPS {
            let mut s = Sys::new();
            let t = Instant::now();
            out = s.run(src);
            best = best.min(t.elapsed().as_secs_f64() * 1e3);
        }
        total += best;
        let shown: String = out.chars().take(70).collect();
        println!("[{}/{} {:>3}%] {:>9.2} ms  {}  =>  {}", k + 1, n, (k + 1) * 100 / n, best, src, shown);
    }
    println!("total {:.1} ms", total);
}

/// The measure alone (decision s-cost.2): pricing 42 candidates of a 1,771-term polynomial (the size of
/// `expand((1 + x + y + z)^20)`, 14 moves times 3 levels) with a fresh memo per candidate, as the leaf count of
/// rounds before 15 did, against one cache for the whole call; each candidate shares all terms but a few with the
/// input.
#[test]
#[ignore]
fn measure_timings() {
    use mc_api::Store;
    use mc_simplify::cost::Cost;
    let h = mc_core::Heap::new(mc_core::Config::default());
    let (x, y, z) = (h.sym_term(h.sym("x")), h.sym_term(h.sym("y")), h.sym_term(h.sym("z")));
    let (add, mul, pow) = (h.sym(mc_api::names::ADD), h.sym(mc_api::names::MUL), h.sym(mc_api::names::POW));
    let mut terms = Vec::new();
    for i in 0..=20i64 {
        for j in 0..=20 - i {
            for k in 0..=20 - i - j {
                let c = h.int(1_000_003 * (i + 1) * (j + 2) * (k + 3));
                terms.push(h.call(
                    mul,
                    &[c, h.call(pow, &[x, h.int(i)]), h.call(pow, &[y, h.int(j)]), h.call(pow, &[z, h.int(k)])],
                ));
            }
        }
    }
    let input = h.call(add, &terms);
    let candidates: Vec<_> = (0..42)
        .map(|n| {
            let mut t = terms.clone();
            t[n % terms.len()] = h.call(h.sym("f"), &[x, h.int(n as i64)]);
            t[(7 * n + 3) % terms.len()] = y;
            h.call(add, &t)
        })
        .collect();
    let reps = 20;
    let fresh = Instant::now();
    let mut total = 0u64;
    for _ in 0..reps {
        total += Cost::new(&h).size(input);
        for &c in &candidates {
            total += Cost::new(&h).size(c);
        }
    }
    let fresh = fresh.elapsed().as_secs_f64() * 1e3 / reps as f64;
    let shared = Instant::now();
    let mut total2 = 0u64;
    let mut walked = 0;
    for _ in 0..reps {
        let mut cost = Cost::new(&h);
        total2 += cost.size(input);
        for &c in &candidates {
            total2 += cost.size(c);
        }
        walked = cost.walked;
    }
    let shared = shared.elapsed().as_secs_f64() * 1e3 / reps as f64;
    assert_eq!(total, total2);
    println!(
        "[1/1 100%] terms {}  fresh memo per candidate {:.3} ms  one cache per call {:.3} ms  nodes priced {}",
        terms.len(),
        fresh,
        shared,
        walked
    );
}
