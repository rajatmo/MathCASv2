//! Round 38: the laws of the bitwise operations on symbolic integers, negative bit indices, and function
//! properties of the inverse Jacobi and Weierstrass functions. Every test has an example input, variations of its
//! own, and a check by an independent route (integer values substituted, numeric values at points).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// The value as a machine number (a complex value with a zero imaginary part counts as real).
fn real(src: &str) -> Option<f64> {
    let v = ev(&format!("chop(n({src}), 1.0e-12)"));
    v.parse::<f64>().ok()
}

#[test]
fn bitwise_laws_on_symbols() {
    assert_eq!(ev("bit_and(1, 2, x)"), "0");
    assert_eq!(ev("bit_and(x, x, x)"), "x");
    assert_eq!(ev("bit_or(-1, 2, x)"), "-1");
    assert_eq!(ev("bit_or(x, x)"), "x");
    assert_eq!(ev("bit_xor(x, y, y, x)"), "0");
    assert_eq!(ev("bit_xor(x, x, x, x, x)"), "x");
    assert_eq!(ev("bit_xor(-1, x)"), "bit_not(x)");
    assert_eq!(ev("bit_not(bit_not(x))"), "x");
    // variations: constants folded with a symbol left, a non-integer number declines
    assert_eq!(ev("bit_xor(5, x, 3, y)"), "bit_xor(6, x, y)");
    assert_eq!(ev("bit_and(x, 1.5)"), "bit_and(x, 1.5)");
    // the laws hold for every integer value of the symbols
    for (x, y) in [(13, -7), (0, 255), (-1, 1_i64 << 40)] {
        let s = format!("[x -> {x}, y -> {y}]");
        assert_eq!(ev(&format!("rewrite(bit_xor(x, y, y, x, 5), {s})")), "5");
        assert_eq!(ev(&format!("rewrite(bit_xor(-1, x), {s}) == bit_not({x})")), "true");
        assert_eq!(ev(&format!("rewrite(bit_and(x, x, 1, 2), {s})")), "0");
        assert_eq!(ev(&format!("bit_xor(5, {x}, 3, {y}) == bit_xor(6, {x}, {y})")), "true");
    }
}

#[test]
fn negative_bit_index_counts_from_the_top() {
    assert_eq!(ev("bit_flip(13, -2)"), "9");
    assert_eq!(ev("bit_flip(3, -3)"), "3");
    assert_eq!(ev("bit_shift_right(32)"), "16");
    // variations: the top bit of 13 = 1101 is bit 3; of 2^40 + 5 bit 40
    assert_eq!(ev("bit_flip(13, -1)"), "5");
    assert_eq!(ev("bit_flip(2^40 + 5, -1)"), "5");
    assert_eq!(ev("bit_flip(2^40 + 5, -41)"), "1099511627780");
    // by the digits: flipping the k-th digit from the left of the binary expansion
    assert_eq!(ev("from_digits(rewrite(integer_digits(13, 2), [1, 1, 0, 1] -> [1, 0, 0, 1]), 2)"), "9");
    assert_eq!(ev("bit_shift_right(1001) == quotient(1001, 2)"), "true");
}

#[test]
fn bounded_inverse_jacobi_functions_are_not_onto() {
    assert_eq!(ev("function_surjective(inverse_jacobi_sn(x, 1 / 3), x)"), "false");
    assert_eq!(ev("function_surjective(inverse_jacobi_cd(x, 3), x)"), "false");
    assert_eq!(ev("function_surjective(inverse_jacobi_ds(x, 3), x)"), "false");
    assert_eq!(ev("function_surjective(inverse_jacobi_dc(x, 1 / 3), x)"), "false");
    // variations: another parameter; the values stay within a quarter period K(m) (or 2 K for cd, dc)
    assert_eq!(ev("function_surjective(inverse_jacobi_sc(x, 5), x)"), "false");
    assert_eq!(ev("function_surjective(inverse_jacobi_nd(x, 7 / 2), x)"), "false");
    let k3 = real("2 * elliptic_k(1/3)").unwrap();
    for x in ["-50.", "-1.", "1.", "1.5", "40."] {
        let v = real(&format!("inverse_jacobi_cd({x}, 3)")).unwrap();
        assert!(v.abs() <= k3);
    }
    for x in ["-30.", "-0.5", "0.2", "100."] {
        let v = real(&format!("inverse_jacobi_ds({x}, 3)")).unwrap();
        assert!(v.abs() <= k3);
    }
}

#[test]
fn inverse_jacobi_for_m_above_one_real_domain_and_sign() {
    assert_eq!(ev("function_sign([inverse_jacobi_dc(x, 3), -1 <= x and x <= 1], x)"), "1");
    assert_eq!(ev("function_sign(inverse_jacobi_ds(x, 3), x)"), "indeterminate");
    assert_eq!(ev("function_domain(inverse_jacobi_cd(x, 3), x)"), "x <= -1 or x >= 1");
    assert_eq!(ev("function_domain(inverse_jacobi_dn(x, 5/2), x)"), "-1 <= x and x <= 1");
    // the numeric values are real exactly there, and of the sign claimed
    for x in ["-1.", "-0.3", "0.6", "0.99"] {
        assert!(real(&format!("inverse_jacobi_dc({x}, 3)")).unwrap() > 0.0);
        assert!(real(&format!("inverse_jacobi_dn({x}, 5/2)")).is_some());
    }
    assert!(real("inverse_jacobi_cd(0.5, 3)").is_none());
    assert!(real("inverse_jacobi_dn(1.5, 5/2)").is_none());
    assert!(real("inverse_jacobi_ds(-0.5, 3)").unwrap() < 0.0 && real("inverse_jacobi_ds(0.5, 3)").unwrap() > 0.0);
}

#[test]
fn weierstrass_functions_with_a_pole_on_the_line() {
    assert_eq!(ev("function_sign(weierstrass_p(x, [1, 2]), x)"), "indeterminate");
    assert_eq!(ev("function_monotonicity(weierstrass_p_prime(x, [1, 2]), x)"), "indeterminate");
    assert_eq!(ev("function_monotonicity(weierstrass_zeta(x, [1, 0]), x)"), "indeterminate");
    // variations: other invariants, the convexity
    assert_eq!(ev("function_sign(weierstrass_zeta(x, [2, -3]), x)"), "indeterminate");
    assert_eq!(ev("function_convexity(weierstrass_p(x, [3, 7]), x)"), "indeterminate");
    // the pole at 0: the values grow like 1/x^2 (DLMF 23.9.2)
    let a = real("weierstrass_p(0.001, [1, 2])").unwrap();
    assert!((a * 1e-6 - 1.0).abs() < 1e-6);
    // zeta jumps from -inf to +inf through its pole, so it is not monotonic on the line
    assert!(real("weierstrass_zeta(-0.01, [1, 0])").unwrap() < -50.0 && real("weierstrass_zeta(0.01, [1, 0])").unwrap() > 50.0);
}
