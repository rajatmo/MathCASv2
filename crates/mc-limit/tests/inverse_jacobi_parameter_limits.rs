//! Limits of the inverse Jacobi functions in their parameter: the inverse circular function at m = 0 and 0 at m = inf.
//! Each limit is checked against the numeric values of the function at m = +-1e-6 (within 1e-5 of the limit) and at
//! m = 1e10 (below 1e-3), computed by the numeric inversion, which shares no code with the limit.
mod common;
use common::{number, Sys};

const TEN: [&str; 10] = ["sn", "cn", "cd", "sd", "dc", "nc", "sc", "ns", "ds", "cs"];
const POINTS: [&str; 4] = ["3/10", "2", "-7/10", "1/2 + im/2"];

/// The modulus of the difference of two printed numeric values, computed by the engine.
fn gap(s: &mut Sys, a: &str, b: &str) -> f64 {
    number(&s.run(&format!("n(abs(({a}) - ({b})))")))
}

#[test]
fn inverse_jacobi_at_parameter_zero_is_the_circular_inverse() {
    let mut s = Sys::new();
    for pq in TEN {
        for v in POINTS {
            let f = format!("inverse_jacobi_{pq}");
            let l = s.run(&format!("limit({f}({v}, m), m -> 0)"));
            assert!(!l.contains("limit("), "{f}({v}, m) at m -> 0: {l}");
            for m in ["10^-6", "-10^-6"] {
                let g = gap(&mut s, &l, &format!("n({f}({v}, {m}))"));
                assert!(g < 1e-5, "{f}({v}, m): limit {l} is {g} away from the value at m = {m}");
            }
        }
    }
    // dn and nd tend to 1 as m -> 0: their inverses have no limit there
    for pq in ["dn", "nd"] {
        let l = s.run(&format!("limit(inverse_jacobi_{pq}(3/10, m), m -> 0)"));
        assert!(l.starts_with("limit("), "inverse_jacobi_{pq}: {l}");
    }
}

#[test]
fn inverse_jacobi_at_parameter_infinity_is_zero() {
    let mut s = Sys::new();
    for pq in TEN.iter().chain(["dn", "nd"].iter()) {
        for v in POINTS {
            let f = format!("inverse_jacobi_{pq}");
            assert_eq!(s.run(&format!("limit({f}({v}, m), m -> inf)")), "0", "{f}({v}, m)");
            let a = number(&s.run(&format!("n(abs({f}({v}, 10^10)))")));
            assert!(a < 1e-3, "{f}({v}, 10^10) = {a}");
        }
    }
}
