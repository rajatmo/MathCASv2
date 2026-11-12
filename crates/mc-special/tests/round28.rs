//! Round 28: the Scorer functions, the modified Lommel functions, Carlson's R_F and R_K, the
//! Coulomb wave functions and the Weierstrass functions: values (checked against independent high-precision
//! values), special values, and the precision contract (machine values agree with the 40-digit value of the exact
//! argument; 30 digits agree with 60).
mod common;
use common::Sys;

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

/// `n(e, 30)` agrees with `n(e, 60)` to 30 digits and its ball contains the 60-digit value; the machine value of
/// `m` (the expression at machine arguments) agrees with `n(e, 40)` to 4e-15 relative.
fn contract(cases: &[(&str, &str)]) {
    let mut s = Sys::new();
    let mut bad = vec![];
    for (e, m) in cases {
        let src = format!(
            "{{ let a = n({e}, 30); let b = n({e}, 60); let c = n({e}, 40); let d = {m}; \
             [precision(a) >= 29, abs(re(a) - re(b)) + abs(im_part(a) - im_part(b)) <= 10^-29*abs(b), \
              abs(set_precision(re(a), inf) - re(b)) <= 10^-min(accuracy(re(a)), 10000), \
              abs(set_precision(im_part(a), inf) - im_part(b)) <= 10^-min(accuracy(im_part(a)), 10000), \
              abs(d - c) <= 4*10^-15*abs(c)] }}"
        );
        let got = s.run(&src);
        if got != "[true, true, true, true, true]" {
            bad.push(format!("{e}: {got}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn scorer_values() {
    check(&[
        ("n(scorer_gi(1/3), 30)", "0.2384894214372833303739938866807654"),
        ("n(scorer_hi(1/3), 30)", "0.5301287993618319829747095496187327"),
        ("scorer_gi(0)", "(1/9)*3^(5/6)/gamma(2/3)"),
        ("scorer_hi(0)", "(2/9)*3^(5/6)/gamma(2/3)"),
        ("scorer_gi_prime(0)", "(1/3)*3^(1/6)/gamma(1/3)"),
        ("scorer_hi(inf)", "inf"),
        ("scorer_gi(-inf)", "0"),
        ("scorer_gi(1.7) + scorer_hi(1.7) - airy_bi(1.7)", "0.0"),
    ]);
    contract(&[
        ("scorer_gi(5)", "scorer_gi(5.0)"),
        ("scorer_gi(-7/2)", "scorer_gi(-3.5)"),
        ("scorer_hi(-6)", "scorer_hi(-6.0)"),
        ("scorer_hi(1/5 + im)", "scorer_hi(0.2 + 1.0*im)"),
        ("scorer_gi_prime(2)", "scorer_gi_prime(2.0)"),
        ("scorer_hi_prime(-2)", "scorer_hi_prime(-2.0)"),
        ("scorer_gi(12)", "scorer_gi(12.0)"),
    ]);
}

#[test]
fn lommel_t_values() {
    check(&[
        ("n(lommel_t1(2, 0, 4), 30)", "13.484612772632462293130137197599015"),
        ("n(lommel_t2(2, 3/2, 4), 30)", "3.7165183624360502655026928435275547"),
        ("lommel_t1(2, 5, z)", "complex_infinity"),
        ("lommel_t2(2, 5, z)", "z + -9216*z^-5 + -24/z + 576*z^-3"),
    ]);
    contract(&[
        ("lommel_t1(-1/4, 2/3, 1)", "lommel_t1(-0.25, 2/3, 1.0)"),
        ("lommel_t2(-1/4, 2/3, 1)", "lommel_t2(-0.25, 2/3, 1.0)"),
        ("lommel_t2(1/2, 1/3, 3/2)", "lommel_t2(0.5, 1/3, 1.5)"),
        ("lommel_t1(7/3 + im, 2*im, 9/2 - im)", "lommel_t1(7/3 + im, 2*im, 4.5 - im)"),
    ]);
}

#[test]
fn carlson_values() {
    check(&[
        ("n(carlson_rf(1, 4, 9), 30)", "0.5086446185700302382135790075023195"),
        ("n(carlson_rk(5, 7), 30)", "0.41040751430462836940653167883696572"),
        ("carlson_rf(x, x, x)", "x^(-1/2)"),
        ("carlson_rf(0, x, y)", "(1/2)*pi*carlson_rk(x, y)"),
        ("carlson_rk(x, 0)", "complex_infinity"),
        ("(pi/2)*carlson_rk(1, 1 - 0.3) - elliptic_k(0.3)", "0.0"),
    ]);
    contract(&[
        ("carlson_rf(1, 2, 3)", "carlson_rf(1.0, 2.0, 3.0)"),
        ("carlson_rf(4, -6, 8)", "carlson_rf(4.0, -6.0, 8.0)"),
        ("carlson_rf(im, 1 - 2*im, 3 + im)", "carlson_rf(im, 1 - 2*im, 3.0 + im)"),
        ("carlson_rk(2, 3)", "carlson_rk(2.0, 3.0)"),
    ]);
}

#[test]
fn coulomb_values() {
    check(&[
        ("n(coulomb_f(3, 2, 5), 30)", "0.43131593114087869012387960004177853"),
        ("n(coulomb_g(3, 12/100, 3), 30)", "1.625113876479269664022347040484569"),
        ("coulomb_f(l, 0, r)", "r*spherical_bessel_j(l, r)"),
        ("coulomb_h1(0, 0, r)", "exp(1im*r)"),
    ]);
    contract(&[
        ("coulomb_f(3, 12/100, 7)", "coulomb_f(3, 0.12, 7.0)"),
        ("coulomb_g(3, 12/100, 7)", "coulomb_g(3, 0.12, 7.0)"),
        ("coulomb_h1(2, 1/5, 3)", "coulomb_h1(2, 0.2, 3.0)"),
        ("coulomb_h2(1, 4/5, 5)", "coulomb_h2(1, 0.8, 5.0)"),
        ("coulomb_f(1/2, 7, 1)", "coulomb_f(0.5, 7.0, 1.0)"),
        ("coulomb_h1(-2/5, -3, 123/100)", "coulomb_h1(-0.4, -3.0, 1.23)"),
    ]);
}

#[test]
fn weierstrass_values() {
    check(&[
        ("n(weierstrass_p(1/3, [1, 2]), 30)", "9.0064386881227981952859598594593"),
        ("n(weierstrass_sigma(1/3, [1, 2]), 30)", "0.3333150975525210008724751746227234"),
        ("weierstrass_p(u, [0, 0])", "u^-2"),
        ("weierstrass_p(-x, [a, b])", "weierstrass_p(x, [a, b])"),
        ("weierstrass_sigma(-x, [a, b])", "-weierstrass_sigma(x, [a, b])"),
        ("inverse_weierstrass_p(7.0, [1, 2])", "2.2385322312096347"),
        ("inverse_weierstrass_p(5.0, [12, -7])", "2.1092205277288665"),
    ]);
    contract(&[
        ("weierstrass_p(5, [1, 2])", "weierstrass_p(5.0, [1, 2])"),
        ("weierstrass_p(12, [3, 2])", "weierstrass_p(12.0, [3, 2])"),
        ("weierstrass_p(2 + im, [im, im + 2])", "weierstrass_p(2.0 + im, [im, im + 2])"),
        ("weierstrass_p_prime(33/10 + 2*im, [12, -7])", "weierstrass_p_prime(3.3 + 2*im, [12, -7])"),
        ("weierstrass_zeta(73/10 - 41/10*im, [1, 2])", "weierstrass_zeta(7.3 - 4.1*im, [1, 2])"),
        ("weierstrass_sigma(5/2 + 3/2*im, [1, 2])", "weierstrass_sigma(2.5 + 1.5*im, [1, 2])"),
        ("inverse_weierstrass_p(3 + im, [im, im + 2])", "inverse_weierstrass_p(3.0 + im, [im, im + 2])"),
    ]);
}

#[test]
fn heun_values() {
    check(&[
        ("n(heun_g(12/10, 9/10, -14/10, 12/100, 3/100, -2/10, 1/10), 30)", "3.56908496265667212886240725085997"),
        ("n(heun_d(1/2, -1/3, 1/4, 1/5, -1/6, 1/7), 30)", "2.455982352072831559324842508338582"),
        ("heun_g(a, q, al, be, ga, de, 0)", "1"),
        ("heun_d(q, al, ga, de, ep, 1)", "1"),
    ]);
    contract(&[
        ("heun_g(43/10, -2/10, 13/10, 12/100, -14/100, 432/100, 12/100)", "heun_g(4.3, -0.2, 1.3, 0.12, -0.14, 4.32, 0.12)"),
        ("heun_g(12/10, 9/10, -14/10, 12/100, 3/100, -2/10, 1/10 + im)", "heun_g(1.2, 0.9, -1.4, 0.12, 0.03, -0.2, 0.1 + im)"),
        ("heun_c(4, -6/10, -7/10, -18/100, 3/10, 12/100)", "heun_c(4, -0.6, -0.7, -0.18, 0.3, 0.12)"),
        ("heun_b(12/10, -14/10, 12/100, 3/100, -2/10, 4)", "heun_b(1.2, -1.4, 0.12, 0.03, -0.2, 4.0)"),
        ("heun_d(4, -6/10, -7/10, -18/100, 3/10, 12/100)", "heun_d(4, -0.6, -0.7, -0.18, 0.3, 0.12)"),
        ("heun_t(12/10, -14/10, 12/100, 3/100, -2/10, 4)", "heun_t(1.2, -1.4, 0.12, 0.03, -0.2, 4.0)"),
    ]);
}

#[test]
fn elliptic_log_values() {
    check(&[
        ("elliptic_log([2, 5], [3, 2.5])", "-0.581447294993539"),
        ("n(elliptic_log([1, 3], [2, 6]), 25)", "-0.70985188008011573259779996833"),
        ("elliptic_log(elliptic_exp(0.7 + 0.2*im, [1, 2]), [1, 2])", "complex(0.7, 0.2)"),
    ]);
}

#[test]
fn appell_values() {
    check(&[
        ("appell_f1(a, b1, b2, c, 0, y)", "hypergeometric_2f1(a, b2, c, y)"),
        ("n(appell_f2(3, 2, 1, 2, 3, 1/3, 2/5), 20)", "8.437500000000000000000"),
    ]);
    contract(&[
        ("appell_f1(2, 1, 1, 3, 7/10, 3/10)", "appell_f1(2, 1, 1, 3, 0.7, 0.3)"),
        ("appell_f1(2, -1, 2, 5, 2/5, 13/10)", "appell_f1(2, -1, 2, 5, 0.4, 1.3)"),
        ("appell_f2(-2, -1, -2, 5, 2/3 + im, 2/5, 3/10)", "appell_f2(-2, -1, -2, 5, 2/3 + im, 0.4, 0.3)"),
        ("appell_f3(3, 2, 1, 2, 3, 1/3, 1/5)", "appell_f3(3, 2, 1, 2, 3, 1/3, 0.2)"),
        ("appell_f4(3, 2, 1, 2, 1/5, 1/10)", "appell_f4(3, 2, 1, 2, 1/5, 0.1)"),
    ]);
}

#[test]
fn zeros_values() {
    check(&[
        ("n(bessel_j_zero(0, 1))", "2.404825557695773"),
        ("n(airy_ai_zero(2, -500))", "-500.16973203561264"),
        ("bessel_j_zero(0, 1)", "bessel_j_zero(0, 1)"),
    ]);
    contract(&[
        ("bessel_j_zero(1, 20, 40)", "bessel_j_zero(1.0, 20, 40)"),
        ("bessel_y_zero(0, 1)", "bessel_y_zero(0.0, 1)"),
        ("bessel_y_zero(0, 4/3)", "bessel_y_zero(0.0, 4/3)"),
        ("bessel_j_zero(1/2, 1/3)", "bessel_j_zero(0.5, 1/3)"),
        ("airy_ai_zero(3)", "airy_ai_zero(3.0)"),
        ("zeta_zero(5)", "zeta_zero(5.0)"),
    ]);
}

#[test]
fn q_and_bilateral_values() {
    check(&[
        ("q_hypergeometric_pfq([0.7], [1/3], 0.2, 5)", "0.3720811428938267"),
        ("bilateral_hypergeometric_pfq([a1, a2], [b1, b2], 0)", "0"),
    ]);
    contract(&[
        ("q_hypergeometric_pfq([17, 1/3, 1/3], [1/3, 1/9], 1/9, 1/3)", "q_hypergeometric_pfq([17, 1/3, 1/3], [1/3, 1/9], 1/9, 1/3.0)"),
        ("q_hypergeometric_pfq([1/4], [1/3], 1/5, 5)", "q_hypergeometric_pfq([1/4], [1/3], 1/5, 5.0)"),
        ("bilateral_hypergeometric_pfq([1/2, 3/4], [1/4, 1/3], 5)", "bilateral_hypergeometric_pfq([1/2, 3/4], [1/4, 1/3], 5.0)"),
        ("bilateral_hypergeometric_pfq([1/2, 3/4], [1/4, 1/3], 3/10)", "bilateral_hypergeometric_pfq([1/2, 3/4], [1/4, 1/3], 0.3)"),
    ]);
}

#[test]
fn mathieu_values() {
    check(&[
        ("mathieu_c(a, 0, z)", "cos(z*a^(1/2))"),
        ("mathieu_s(2, 1, 0)", "0"),
    ]);
    contract(&[
        ("mathieu_c(2, 1, 16/5)", "mathieu_c(2, 1, 3.2)"),
        ("mathieu_s(2, 1, 37/10)", "mathieu_s(2, 1, 3.7)"),
        ("mathieu_s_prime(2, 1, 16/5)", "mathieu_s_prime(2, 1, 3.2)"),
        ("mathieu_characteristic_a(1, 1/2)", "mathieu_characteristic_a(1, 0.5)"),
        ("mathieu_characteristic_a(15/17, 5)", "mathieu_characteristic_a(15/17, 5.0)"),
    ]);
}
