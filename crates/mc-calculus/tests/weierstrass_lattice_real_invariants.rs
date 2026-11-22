//! Singular sets of the Weierstrass lattice functions on the plane of real invariants (the roots, the half-period w1
//! and the quasi-period eta1), the invariants of a pair of half-periods, the real poles of p, p' and zeta, the
//! logarithmic point of an Appell F1 that reduces to a 2F1, and constants that have no value. Each claim is checked
//! against values: jumps across the lines, difference quotients across the discriminant, poles where the lattice meets
//! the line. Every call runs under a time limit.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::spawn(move || {
        let stop = StopFlag::new();
        let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
        let out = match e.eval(&s, &stop) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        let _ = tx.send(out);
    });
    rx.recv_timeout(std::time::Duration::from_secs(60)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

fn holds(src: &str) {
    assert_eq!(ev(src), "true", "{src}");
}

#[test]
fn roots_half_period_and_quasi_period_sets() {
    for f in ["weierstrass_e1", "weierstrass_eta1"] {
        for p in ["function_singularities", "function_discontinuities"] {
            assert_eq!(ev(&format!("{p}({f}([x, y]), [x, y])")), "x == 0 or y == 0", "{p} {f}");
        }
    }
    assert_eq!(ev("function_singularities(weierstrass_e2([x, y]), [x, y])"), "x == 0 or -27*y^2 + x^3 == 0");
    assert_eq!(ev("function_discontinuities(weierstrass_e2([x, y]), [x, y])"), "x == 0");
    assert_eq!(ev("function_discontinuities(weierstrass_e3([x, y]), [x, y])"), "x == 0 or y == 0");
    assert_eq!(ev("function_singularities(weierstrass_e3([x, y]), [x, y])"), "x == 0 or y == 0 or -27*y^2 + x^3 == 0");
    // in x with the parameter y: the jump across g3 = 0 is in the set too (the sets are joint in the parameters);
    // w1([1, 1e-8]) = 1.854075 against w1([1, -1e-8]) = -1.854075 i
    for p in ["function_singularities", "function_discontinuities"] {
        let s = format!("{p}(weierstrass_half_period_w1([x, y]), x)");
        holds(&format!("rewrite({s}, [x -> 1, y -> 0]) and rewrite({s}, [x -> 0, y -> 1])"));
        holds(&format!("not(rewrite({s}, [x -> 1, y -> 1]) or rewrite({s}, [x -> 3, y -> 1]) or rewrite({s}, [x -> -2, y -> -1]))"));
    }
    holds("abs(n(weierstrass_half_period_w1([1, 10^(-8)])) - n(weierstrass_half_period_w1([1, -10^(-8)]))) > 1");
    // e2 has no jump across g3 = 0, in x as well
    assert_eq!(ev("function_discontinuities(weierstrass_e2([x, y]), x)"), "x == 0");
    assert_eq!(ev("function_sign([weierstrass_half_period_w1([x, y]), x > 0 and y > 0], [x, y], strict_inequalities: true)"), "1");
    // jumps across g2 = 0 and g3 = 0
    for f in ["weierstrass_e1", "weierstrass_half_period_w1", "weierstrass_eta1", "weierstrass_e3"] {
        holds(&format!("abs(n({f}([1/1000, 1])) - n({f}([-1/1000, 1]))) > 1/2"));
        holds(&format!("abs(n({f}([1, 1/1000])) - n({f}([1, -1/1000]))) > 1/2"));
    }
    holds("abs(n(weierstrass_e2([1, 1/1000])) - n(weierstrass_e2([1, -1/1000]))) < 1/100");
    // across the discriminant at [3, 1]: the difference quotients of e1, w1, eta1 agree on both sides, those of e2 grow
    for f in ["weierstrass_e1", "weierstrass_half_period_w1", "weierstrass_eta1"] {
        let q = |s: &str| format!("({f}([3 + {s}, 1]) - {f}([3 + {s}/2, 1]))/({s}/2)");
        holds(&format!("abs(n({} - {}, 30)) < 10^(-6)", q("10^(-7)"), q("-10^(-7)")));
    }
    holds("abs(n((weierstrass_e2([3 + 10^(-8), 1]) - weierstrass_e2([3 + 10^(-8)/2, 1]))/(10^(-8)/2), 30)) > 100");
    // w1 positive on the quadrant
    holds("min(table(n(weierstrass_half_period_w1([t, 1])), [t, 1/8, 8, 1/8])) > 0");
    holds("min(table(n(weierstrass_half_period_w1([1, t])), [t, 1/8, 3, 1/8])) > 0");
    // and positive under that condition; not real off the closed quadrant
    assert_eq!(ev("function_sign([weierstrass_half_period_w1([x, y]), x > 0 and y > 0], [x, y], strict_inequalities: true)"), "1");
    assert_eq!(ev("function_sign(weierstrass_half_period_w1([x, y]), [x, y])"), "indeterminate");
    holds("im_part(n(weierstrass_half_period_w1([-1, 1]))) != 0");
    holds("im_part(n(weierstrass_half_period_w1([1, -1]))) != 0");
}

#[test]
fn invariants_of_half_periods() {
    assert_eq!(ev("function_singularities(weierstrass_invariant_g2([x, y]), x)"), "x == 0 or im_part(y/x) == 0");
    assert_eq!(ev("function_discontinuities(weierstrass_invariant_g3([x, y]), x)"), "x == 0 or im_part(y/x) == 0");
    // a pole of order 4 at w1 = 0: g2 grows like w1^-4
    holds("abs(n(weierstrass_invariant_g2([1/1000, 1 + im]))) > 10^12");
    holds("abs(n(weierstrass_invariant_g2([1, im])) - 11.817045008077) < 10^(-9)");
}

#[test]
fn real_poles_of_p_p_prime_and_zeta() {
    // g3 < 0 with three real roots: w1 imaginary, the poles 2 k w3 (not every x)
    // (the sets are stated for symbolic invariants; numeric ones keep the general route)
    let d = ev("function_domain(weierstrass_p(x, [g2, g3]), [x, g2, g3])");
    assert!(d.contains("weierstrass_half_period_w1([g2, g3])") && d.contains("weierstrass_half_period_w3([g2, g3])"), "{d}");
    let w3 = "weierstrass_half_period_w3([4, -1])";
    // the lattice coordinates of 2 w3 are (0, 1): m = x im(w3)/D, n = -x im(w1)/D
    let w1 = "weierstrass_half_period_w1([4, -1])";
    let dd = format!("(2*(re({w1})*im_part({w3}) - im_part({w1})*re({w3})))");
    holds(&format!("abs(n(2*{w3}*im_part({w3})/{dd})) < 10^(-12)"));
    holds(&format!("abs(n(-2*{w3}*im_part({w1})/{dd}) - 1) < 10^(-12)"));
    for f in ["weierstrass_p", "weierstrass_p_prime", "weierstrass_zeta"] {
        let dom = format!("function_domain({f}(x, [g2, g3]), [x, g2, g3])");
        holds(&format!("n(rewrite({dom}, [x -> 1, g2 -> 4, g3 -> -1]))"));
        holds(&format!("n(rewrite({dom}, [x -> 3/2, g2 -> 4, g3 -> -1]))"));        holds(&format!("abs(n({f}(2*{w3} + 10^(-4), [4, -1]))) > 10^3"));
        holds(&format!("abs(n({f}(1, [4, -1]))) < 100"));
    }
    // degenerate lattices: g3 > 0 poles 2 k w1, g3 < 0 the pole 0 only
    holds("rewrite(function_domain(weierstrass_p(x, [g2, g3]), [x, g2, g3]), [x -> 1, g2 -> 3, g3 -> -1])");
    holds("not(rewrite(function_domain(weierstrass_p(x, [g2, g3]), [x, g2, g3]), [x -> 0, g2 -> 3, g3 -> -1]))");
    holds("not(rewrite(function_domain(weierstrass_p(x, [g2, g3]), [x, g2, g3]), [x -> 2*weierstrass_half_period_w1([3, 1]), g2 -> 3, g3 -> 1]))");
    holds("rewrite(function_domain(weierstrass_p(x, [g2, g3]), [x, g2, g3]), [x -> 1, g2 -> 3, g3 -> 1])");
    holds("abs(n(weierstrass_p(1, [3, -1]) - (1/2 + 3/2*csch((3/2)^(1/2))^2), 30)) < 10^(-25)");
}

#[test]
fn appell_f1_logarithmic_point() {
    assert_eq!(ev("function_analytic(appell_f1(1, 1, 1, 2, 4, x), x)"), "false");
    // F1(1; 1, 1; 2; 4, y) = (log(1 - y) - log(-3))/(4 - y) near 0
    holds("abs(n(appell_f1(1, 1, 1, 2, 4, 1/2) - (log(1/2) - log(-3))/(7/2), 20)) < 10^(-15)");
    // not the reduction: a different upper parameter, no answer from this entry
    assert_ne!(ev("function_analytic(appell_f1(1, 1, 1, 3, 4, x), x)"), "true");
}

#[test]
fn constants_without_a_value() {
    assert_eq!(ev("function_domain(complex_infinity, x)"), "false");
    assert_eq!(ev("function_domain(-infinity, x)"), "false");
    assert_eq!(ev("function_domain(indeterminate, x, complexes)"), "false");
    assert_eq!(ev("function_domain(hypergeometric_2f1(1, 1, 0, z), z)"), "false");
    assert_eq!(ev("function_singularities(complex_infinity, x)"), "true");
    assert_eq!(ev("function_analytic(complex_infinity, x)"), "false");
    assert_eq!(ev("function_domain(3, x)"), "true");
    assert_eq!(ev("function_domain(arctan(infinity) + x, x)"), "true");
}
