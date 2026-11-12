//! Round 29: the ball 2F1 at a ball argument in the degenerate connection cases, the machine
//! fast paths of the round 28 functions, and the new functions.
mod common;
use common::Sys;
use mc_arith::num::Ball;
use mc_numeric::cball::CBall;

fn check(cases: &[(&str, &str)]) {
    let mut s = Sys::new();
    let mut bad = vec![];
    for (src, want) in cases {
        let got = s.run(src);
        if got != *want {
            bad.push(format!("{src}\n    got  {got}\n    want {want}"));
        }
    }
    assert!(bad.is_empty(), "{} mismatches:\n{}", bad.len(), bad.join("\n"));
}

/// A ball of radius 2^-r around n/d.
fn ball(n: i64, d: i64, r: i64) -> Ball {
    let mut b = Ball::from_ratio(n, d, 200);
    b = b.add(&Ball { mid: mc_arith::num::Float::zero(), rad: mc_arith::num::Mag::pow2(-r) }, 200);
    b
}

#[test]
fn ball_2f1_degenerate_radius() {
    // 2F1(2, 2; 5; z) beyond the unit disk (c - a - b = 1): the radius of z must not be amplified by the shift
    let z = CBall::new(ball(3, 2, 139), ball(1, 3, 139));
    for p in [139u64, 400, 811] {
        let t = std::time::Instant::now();
        let v = mc_special::ball::cspecial::hyp2f1(&CBall::real(Ball::from_i64(2)), &CBall::real(Ball::from_i64(2)), &CBall::real(Ball::from_i64(5)), &z, p).unwrap();
        let acc = mc_special::ball::accuracy(&v.re).min(mc_special::ball::accuracy(&v.im));
        assert!(acc > 125.0, "p {p}: accuracy {acc}");
        assert!(t.elapsed().as_secs_f64() < 2.0, "p {p}: {:?}", t.elapsed());
    }
}

#[test]
fn elliptic_k_complex_ball() {
    // K(1 - m) at m = 1.19 i/(1.35 + 0.60 i) with a radius (hung in round 28)
    let m = CBall::new(ball(1, 1, 110).sub(&Ball::from_ratio(1428, 4365, 200), 200), ball(-3213, 4365, 110));
    let t = std::time::Instant::now();
    let v = mc_special::ball::cspecial::elliptic_k(&m, 110);
    assert!(v.is_some(), "no value");
    assert!(t.elapsed().as_secs_f64() < 2.0, "{:?}", t.elapsed());
    let mut s = Sys::new();
    let t = std::time::Instant::now();
    let got = s.run("n(elliptic_k(1 - 119/100*im/(135/100 + 60/100*im)), 30)");
    assert!(t.elapsed().as_secs_f64() < 3.0 && got.starts_with("complex(1.62585866549723766359779658858"), "{got} {:?}", t.elapsed());
}

#[test]
fn appell_digits_without_retries() {
    // the retry loop tested its own widening: 6 s at 22 digits
    let mut s = Sys::new();
    let t = std::time::Instant::now();
    let got = s.run("n(appell_f1(1, 1, 3/5, 3, 2/3, 2/5), 22)");
    assert_eq!(got, "1.50804530668358548664314");
    assert!(t.elapsed().as_secs_f64() < 2.0, "{:?}", t.elapsed());
    check(&[("n(appell_f1(1, 1, 3/5, 3, 2/3, 2/5), 25)", "1.508045306683585486643143805")]);
}

/// The machine value `m` agrees with `n(e, 40)` to 4e-15 relative (the precision contract), for the fast paths.
fn machine_contract(cases: &[(&str, &str)]) {
    let mut s = Sys::new();
    let mut bad = vec![];
    for (e, m) in cases {
        let src = format!("{{ let c = n({e}, 40); let d = {m}; abs(d - c) <= 4*10^-15*abs(c) }}");
        let got = s.run(&src);
        if got != "true" {
            bad.push(format!("{e} vs {m}: {got} ({})", s.run(m)));
        }
    }
    assert!(bad.is_empty(), "{} failures:\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn fast_paths_keep_the_contract() {
    machine_contract(&[
        ("scorer_gi(1/2)", "scorer_gi(0.5)"),
        ("scorer_hi(-37/10)", "scorer_hi(-3.7)"),
        ("scorer_gi_prime(9)", "scorer_gi_prime(9.0)"),
        ("scorer_hi_prime(2 + 3/2*im)", "scorer_hi_prime(2.0 + 1.5*im)"),
        ("scorer_gi(16)", "scorer_gi(16.0)"),
        ("weierstrass_p(6/5, [1, 2])", "weierstrass_p(1.2, [1, 2])"),
        ("weierstrass_zeta(17/10 + 2/5*im, [3, 2])", "weierstrass_zeta(1.7 + 0.4*im, [3, 2])"),
        ("weierstrass_sigma(-5/2, [1/2, 1/2])", "weierstrass_sigma(-2.5, [0.5, 0.5])"),
        ("weierstrass_p_prime(33/10 - 29/10*im, [2 + im, 1])", "weierstrass_p_prime(3.3 - 2.9*im, [2.0 + im, 1.0])"),
        ("inverse_weierstrass_p(5, [12, -7])", "inverse_weierstrass_p(5.0, [12, -7])"),
        ("carlson_rf(1 + im, 2, 3)", "carlson_rf(1.0 + im, 2, 3)"),
        ("carlson_rk(1, 2)", "carlson_rk(1.0, 2)"),
        ("mathieu_c(3, 2, 13/10)", "mathieu_c(3, 2, 1.3)"),
        ("mathieu_s_prime(3, 2, 13/10)", "mathieu_s_prime(3, 2, 1.3)"),
        ("mathieu_s(3, 1, 1/2 + 7/10*im)", "mathieu_s(3.0, 1, 0.5 + 0.7*im)"),
        ("mathieu_characteristic_a(3, 2)", "mathieu_characteristic_a(3, 2.0)"),
        ("mathieu_characteristic_a(1/3, 7/10)", "mathieu_characteristic_a(1/3, 0.7)"),
        ("appell_f1(2, 1, 1, 3, 7/10, 3/10)", "appell_f1(2, 1, 1, 3, 0.7, 0.3)"),
        ("appell_f2(2, 1, 1/2, 1/3, 2, 1/3, -3/4)", "appell_f2(2, 1, 1/2, 1/3, 2, 1/3, -0.75)"),
        ("appell_f3(1, 2, 3, 6, 1/5, -1/3, 1/10)", "appell_f3(1, 2, 3, 6, 1/5, -1/3, 0.1)"),
        ("appell_f4(1/2, 3/2, 1/3, 4, 1/10, 1/5)", "appell_f4(1/2, 3/2, 1/3, 4, 0.1, 0.2)"),
        ("heun_g(2, 1/2, 1, 2, 3, 1/3, 2/5)", "heun_g(2, 1/2, 1, 2, 3, 1/3, 0.4)"),
        ("heun_c(1/2, 1, 2, 3, 1/3, 5/2)", "heun_c(1/2, 1, 2, 3, 1/3, 2.5)"),
        ("heun_b(1/2, 1, 2, 3, 1/3, 7/5)", "heun_b(1/2, 1, 2, 3, 1/3, 1.4)"),
        ("heun_d(1/2, 1, 2, 3, 1/3, 7/5)", "heun_d(1/2, 1, 2, 3, 1/3, 1.4)"),
        ("heun_t(1/2, 1, 2, 3, 1/3, 7/5)", "heun_t(1/2, 1, 2, 3, 1/3, 1.4)"),
        ("lommel_t1(4/3, 1/2, -3/2 + 2*im)", "lommel_t1(4/3, 1/2, -1.5 + 2.0*im)"),
    ]);
}

#[test]
fn fast_paths_are_fast() {
    // 400 values each well under a second (each took 0.05-0.5 s before)
    let mut s = Sys::new();
    for e in [
        "table(scorer_gi(x + 0.3*im), [x, -4, 4, 0.02])",
        "table(weierstrass_p(x + 0.3*im, [1, 2]), [x, -4, 4, 0.02])",
        "table(mathieu_c(3, 2, x), [x, -4, 4, 0.02])",
        "table(appell_f4(1/2, 3/2, 1/3, 4, 0.1, y), [y, -1/2, 1/3, 0.002])",
        "table(heun_b(4 + im, -0.6 - 0.3*im, -0.7, -0.18, 0.3, z), [z, -5, 1, 0.015])",
    ] {
        let t = std::time::Instant::now();
        let got = s.run(&format!("length({e})"));
        assert!(got.parse::<usize>().is_ok_and(|n| n >= 400), "{e}: {got}");
        assert!(t.elapsed().as_secs_f64() < 3.0, "{e}: {:?}", t.elapsed());
    }
}

#[test]
fn lattice_functions() {
    // the reduced half-periods and the conventions on the boundaries (half-periods of [2, 0], [-2, 0],
    // [-3, 1], [1, 2], [0, -1], [1, 2 i]), the roots and quasi-periods
    check(&[
        ("weierstrass_half_periods([11.0, 7.0])", "[0.9270373386506859, complex(0.0, 1.8540746773013719)]"),
        ("weierstrass_half_periods([8.0, -4.0])", "[complex(0.0, -1.0094529099892116), 1.4844124734223865]"),
        ("weierstrass_half_periods([1.0, 2.0])", "[1.308364762637097, complex(0.6541823813185484, 1.229368751632915)]"),
        ("weierstrass_half_periods([-3.0, 1.0])", "[complex(0.8472351391740537, 1.0500353672047946), complex(-0.8472351391740537, 1.0500353672047946)]"),
        ("weierstrass_half_periods([-2.0, 0.0])", "[complex(1.1024393989965828, -1.1024393989965828), complex(1.1024393989965828, 1.1024393989965828)]"),
        ("weierstrass_half_periods([0.0, -1.0])", "[complex(1.3249790627140874, -0.7649770185285965), complex(0.0, 1.529954037057193)]"),
        ("weierstrass_half_periods([1.0, 2.0*im])", "[complex(1.3013924813494882, -0.29912656788745645), complex(-0.29912656788745645, 1.3013924813494882)]"),
        ("[weierstrass_e1([11.0, 7.0]), weierstrass_e2([11.0, 7.0]), weierstrass_e3([11.0, 7.0])]", "[1.9142135623730951, -0.914213562373095, -1.0]"),
        ("weierstrass_eta2([0.5 + im, 1.0])", "complex(0.3026048271252669, 0.5282469551368528)"),
        ("weierstrass_half_periods([1, 0])", "[(1/4)*pi^(-1/2)*gamma(1/4)^2, complex(0, 1/4)*pi^(-1/2)*gamma(1/4)^2]"),
        ("weierstrass_eta1([12, 8])", "(1/6)*pi*3^(1/2)"),
        ("weierstrass_invariants([1, im])", "[(1/256)*pi^-2*gamma(1/4)^8, 0]"),
        ("n(weierstrass_invariants([1, 2*im]), 30)", "[8.124218443053017090717481882359900, 4.4430520449459499906285496780843793]"),
        ("weierstrass_half_periods(weierstrass_invariants([2.0 + im, -1 + 3.0*im]))", "[complex(2.0, 1.0), complex(-0.9999999999999999, 3.0)]"),
        ("elliptic_exp_prime(0.7, [1.0, 2.0])", "[-6.149550200488952, 12.515970582511383]"),
        ("weierstrass_e1([11, 7])", "weierstrass_e1([11, 7])"),
    ]);
}

#[test]
fn new_functions_values() {
    // values checked against independent high-precision evaluations (see NOTES.md, round 29)
    check(&[
        ("n(airy_bi_zero(1), 25)", "-1.17371322270912792491997996247"),
        ("n(airy_bi_zero(2, -500))", "-500.23996579701316"),
        ("stieltjes_gamma(0)", "euler_gamma"),
        ("n(stieltjes_gamma(3), 20)", "0.00205383442030334586616005"),
        ("n(stieltjes_gamma(2, 1/2), 30)", "0.968864475220290711421711062323781"),
        ("n(stieltjes_gamma(100), 10)", "-4.253401571708027e17"),
        ("n(siegel_theta([[1/2, 0], [0, 1/2]], [[im, 0], [0, im]], [1/10 + im, 0]), 30)", "complex(14.849894009124256064967747430155416, -10.78907953400080065490368998112683)"),
        ("siegel_theta([[0.5 + im]], [2])", "complex(1.0000069746847124, 0.08642783652859559)"),
        ("siegel_theta([[im, 1], [0, 2*im]], [1, 2.0])", "siegel_theta([[1im, 1], [0, 2im]], [1, 2.0])"),
        ("n(spheroidal_eigenvalue(117, 5, 1), 30)", "13805.4991058412031617406729670552"),
        ("spheroidal_eigenvalue(2, 1, 0.5)", "5.856900130405332"),
        ("n(spheroidal_ps(7, 5, 4, 0), 30)", "5197.949191793180304847731343835593"),
        ("n(spheroidal_ps(2, 1, 1/3, 1/2), 20)", "-1.300875733572396152328754"),
        // a series whose factors are all exact (the term bound of polar_step stays at -inf; it once allocated 2^60 bytes)
        ("n(bessel_k(1, 2 + im), 20)", "complex(0.0362915924004270455710960, -0.1240638345728347622360960)"),
    ]);
}
