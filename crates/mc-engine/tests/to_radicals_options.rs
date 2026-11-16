//! The options of to_radicals (cubics, quartics, assumptions) and eigenvalues in radicals. Each radical
//! is checked against the numeric value of the root it replaces, or by substitution into the polynomial.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let stop = StopFlag::new();
            let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
            let out = match e.eval(&s, &stop) {
                Ok(v) => v.printed,
                Err(err) => format!("error: {err}"),
            };
            let _ = tx.send(out);
        })
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(120)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

#[test]
fn cubics_and_quartics_false_keep_the_general_roots() {
    let general = "root(&(9 - 7*&1 - 5*&1^2 + &1^3), 1)";
    assert_eq!(ev(&format!("x = {general}; to_radicals(x, cubics: false) === x")), "true");
    assert_eq!(ev("to_radicals(root(&(&1^3 - 3), 1), cubics: false)"), "3^(1/3)");
    // a binomial cubic in radicals: the real root of x^3 + 7, and the default still uses Cardano's formula
    assert_eq!(ev("to_radicals(root(&(&1^3 + 7), 1), cubics: false)^3"), "-7");
    assert_eq!(ev(&format!("free_of(to_radicals({general}), root)")), "true");
    let quartic = "root(&(9 - 7*&1 - 5*&1^2 + 3*&1^3 + &1^4), 1)";
    assert_eq!(ev(&format!("x = {quartic}; to_radicals(x, quartics: false) === x")), "true");
    assert_eq!(ev("to_radicals(root(&(&1^4 - 7*&1^2 + 3), 1), quartics: false)"), "(-1/2)*2^(1/2)*(7 + 37^(1/2))^(1/2)");
    // the biquadratic x^4 - 5 x^2 + 2: its third root, checked against the root's numeric value
    assert_eq!(ev("abs(n(to_radicals(root(&(&1^4 - 5*&1^2 + 2), 3), quartics: false)) - n(root(&(&1^4 - 5*&1^2 + 2), 3))) < 1e-14"), "true");
    assert_eq!(ev("x = root(&(7 - 2*&1 + &1^3), 1); to_radicals([x, root(&(-5 + &1^3), 1)], cubics: false) === [x, 5^(1/3)]"), "true");
}

#[test]
fn assumptions_order_the_roots_of_a_parametric_polynomial() {
    let rt = "root(&((&1 - a)*(&1^2 - a)), 1)";
    assert_eq!(ev(&format!("to_radicals({rt}, assumptions: a >= 0)")), "-a^(1/2)");
    assert_eq!(ev(&format!("to_radicals({rt}, assumptions: a < 0)")), "a");
    assert_eq!(ev(&format!("x = {rt}; to_radicals(x, assumptions: true) === x")), "true");
    // the order of a and sqrt(a) changes at a = 1; checked at a = 4 and a = 1/4 against the numeric roots
    assert_eq!(ev("to_radicals(root(&((&1 - a)*(&1^2 - a)), 3), assumptions: a > 1)"), "a");
    assert_eq!(ev("to_radicals(root(&((&1 - a)*(&1^2 - a)), 3), assumptions: 0 < a and a < 1)"), "a^(1/2)");
    assert_eq!(ev("n(rewrite(root(&((&1 - a)*(&1^2 - a)), 3), a -> 4))"), "4.0");
    assert_eq!(ev("n(rewrite(root(&((&1 - a)*(&1^2 - a)), 3), a -> 1/4))"), "0.5");
    // a < 0: the second root is not real, so it stays
    assert_eq!(ev("head(to_radicals(root(&((&1 - a)*(&1^2 - a)), 2), assumptions: a < 0))"), "root");
    assert_eq!(ev("x = root(&(&1^3 - a), 1); to_radicals(x + root(&(&1^3 - 2), 1), assumptions: none) === 2^(1/3) + x"), "true");
}

#[test]
fn eigenvalues_with_radicals() {
    // each radical eigenvalue is a root of the characteristic polynomial
    assert_eq!(ev("m = [[1, 2, 1], [2, 1, 2], [3, 2, 1]]; free_of(eigenvalues(m, cubics: true), root)"), "true");
    assert_eq!(
        ev("m = [[1, 2, 1], [2, 1, 2], [3, 2, 1]]; max(abs(n(map(v => det(m - v*identity_matrix(3)), eigenvalues(m, cubics: true)), 30))) < 1e-20"),
        "true"
    );
    assert_eq!(ev("m = [[1, 2, 1, 4], [2, 1, 2, 2], [3, 2, 1, 2], [1, 3, 2, 1]]; abs(n(eigenvalues(m, 1, quartics: true)[1] - eigenvalues(m, 1)[1], 30)) < 1e-20"), "true");
}

/// Roots of degree above 4 whose roots are radicals (binomials, a polynomial in x^3 of degree 2) and algebraic
/// numbers are written with radicals; each radical is checked by its power (x^5 = 2) or against the numeric value
/// at 30 digits of the root or of c0 + c1 r + c2 r^2; a root that has no radical form from `roots` is kept.
#[test]
fn higher_degrees_and_algebraic_numbers_in_radicals() {
    let t0 = std::time::Instant::now();
    assert_eq!(ev("to_radicals(root(&(&1^5 - 2), 3))"), "(-1)^(4/5)*2^(1/5)");
    assert_eq!(ev("expand(to_radicals(root(&(&1^5 - 2), 3))^5)"), "2");
    assert_eq!(ev("abs(n(to_radicals(root(&(&1^5 - 2), 3)), 30) - n(root(&(&1^5 - 2), 3), 30)) < 10^-25"), "true");
    let s = ev("to_radicals(root(&(&1^3 + &1 + 11), 1) + root(&(&1^5 - 2), 3))");
    assert!(!s.contains("root("), "{s}");
    for k in 1..=6 {
        let r = format!("root(&(&1^6 + &1^3 + 1), {k})");
        let v = ev(&format!("to_radicals({r})"));
        assert!(!v.contains("root("), "{r}: {v}");
        assert_eq!(ev(&format!("abs(n(to_radicals({r}), 30) - n({r}, 30)) < 10^-25")), "true", "{r}");
    }
    let a = "algebraic_number(root(&(&1^3 - 11*&1 + 3), 1), [1, 2, 3])";
    let v = ev(&format!("to_radicals({a})"));
    assert!(!v.contains("root(") && !v.contains("algebraic_number"), "{v}");
    let direct = "1 + 2*root(&(&1^3 - 11*&1 + 3), 1) + 3*root(&(&1^3 - 11*&1 + 3), 1)^2";
    assert_eq!(ev(&format!("abs(n(to_radicals({a}), 30) - n({direct}, 30)) < 10^-25")), "true");
    assert_eq!(ev("to_radicals(root(&(&1^7 - 3*&1 + 1), 2))"), "root(&(1 - 3*&1 + &1^7), 2)");
    assert!(t0.elapsed() < std::time::Duration::from_secs(60), "{:?}", t0.elapsed());
}
