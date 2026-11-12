//! Round 31: spherical Bessel at high order and small argument, and later items.
mod common;
use common::Sys;

fn val(s: &mut Sys, src: &str) -> f64 {
    s.run(src).parse::<f64>().unwrap_or_else(|_| panic!("{src}"))
}

#[test]
fn spherical_bessel_j_small_argument_high_order() {
    let mut s = Sys::new();
    for (k, x) in [(20, "0.0703"), (10, "0.5"), (30, "3.0"), (7, "-2.5"), (45, "12.0")] {
        let got = val(&mut s, &format!("spherical_bessel_j({k}, {x})"));
        // an independent route: the exact order and a 40-digit argument through the Bessel function
        let want = val(&mut s, &format!("n(spherical_bessel_j({k}, rationalize({x}, 0)), 40) * 1.0"));
        assert!(((got - want) / want).abs() < 1e-12, "j_{k}({x}): {got} vs {want}");
    }
}

#[test]
fn meijer_g_half_exponent_bessel() {
    let mut s = Sys::new();
    assert_eq!(s.run("activate(inactive(meijer_g)([[], []], [[1/2], [-1/2]], (1/2)*a*x, 1/2))"), "bessel_j(1, a*x)");
}

#[test]
fn timing_cells() {
    let mut s = Sys::new();
    let t = std::time::Instant::now();
    assert_eq!(s.run("partitions_q(10)"), "10");
    assert_eq!(s.run("partitions_q(100)"), "444793");
    s.run("table(partitions_q(k) / (e^(pi * sqrt(k / 3)) / (4 * 3^(1 / 4) * k^(3 / 4))), k, 1000);");
    eprintln!("partitions_q table {:?}", t.elapsed());
    let t = std::time::Instant::now();
    s.run("abs(fourier(cellular_automaton(30, [[1], 0], 50)));");
    eprintln!("fourier ca {:?}", t.elapsed());
}

#[test]
fn number_theory_lists() {
    let mut s = Sys::new();
    for (src, want) in [
        ("multiplicative_order(5, 7, [3, 11])", "2"),
        ("multiplicative_order(5, 7, [2, 3, 4])", "2"),
        ("multiplicative_order(5, 7, [4])", "2"),
        ("multiplicative_order(2, 15, [1, -1])", "4"),
        ("multiplicative_order(5, 7)", "6"),
        ("multiplicative_order([5, 3], 7)", "[6, 6]"),
        ("number_digit([pi, e, 99352, 11/7], [4, 0, -2])", "[[0, 3, 4], [0, 2, 1], [9, 2, 0], [0, 1, 7]]"),
    ] {
        assert_eq!(s.run(src), want, "{src}");
    }
}
