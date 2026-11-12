//! Derivatives agree with central differences at random points, evaluated with balls: every rule of the table and
//! random compositions in two variables; mixed partials are symmetric.
//!
//! A central difference (F(p + h) - F(p - h)) / 2h with the exact step h = 10^-15 differs from F'(p) by about
//! h^2 F'''(p) / 6, so a derivative is accepted when it agrees to 20 digits relative to 1 + |F'(p)|. The difference
//! is evaluated as `n(1 + (D - C) 10^20, 25)`, a number near 1 whatever the cancellation inside, and compared
//! with the machine value of |D|.
mod common;
use common::{number, Lcg, Sys};
use mc_calculus::rules::{Rule, OPAQUE, RULES};

/// Check `d(f, var)` against the central difference at `x = px, y = py`; `Err` with the details on a mismatch.
fn agree(s: &mut Sys, f: &str, var: &str, px: &str, py: &str) -> Result<(), String> {
    let shift = |sign: &str| {
        let (x, y) = if var == "x" { (format!("{px} {sign} hh"), py.to_string()) } else { (px.to_string(), format!("{py} {sign} hh")) };
        format!("rewrite(ff, [x -> {x}, y -> {y}])")
    };
    let src = format!(
        "ff = {f}; dd = d(ff, {var}); hh = 10^-15; v = rewrite(dd, [x -> {px}, y -> {py}]); \
         c = ({} - {})/(2*hh); [n(1 + (v - c)*10^20, 25), n(abs(v))]",
        shift("+"),
        shift("-")
    );
    let out = s.run(&src);
    if within(&out) {
        Ok(())
    } else {
        Err(format!("d({f}, {var}) at x = {px}, y = {py}: {out}; derivative {}", s.run("dd")))
    }
}

/// Is `[r, m]` (the printed `[n(1 + (D - C) 10^20, 25), n(abs(D))]`) an agreement: |r - 1| < 1 + m?
fn within(out: &str) -> bool {
    let Some(inner) = out.strip_prefix('[').and_then(|o| o.strip_suffix(']')) else { return false };
    let Some((r, m)) = inner.split_once(", ") else { return false };
    let num = |t: &str| t.trim_end_matches('.').parse::<f64>().ok();
    matches!((num(r), num(m)), (Some(r), Some(m)) if (r - 1.0).abs() < 1.0 + m)
}

/// The argument expressions of the rule checks: values in (0.37, 0.9) for x in (0.1, 0.9), inside every domain.
const DEPENDENT: &str = "(1/3 + x*(1 + x)/3)";
const CONSTANTS: [&str; 3] = ["(5/7)", "(3/11)", "(2/9)"];

/// The functions defined by formulas.
const FORMULA: &[&str] = &[
    "sec", "csc", "cot", "sech", "csch", "coth", "asinh", "acosh", "atanh", "acoth", "acot", "asec", "acsc", "asech",
    "acsch", "gudermannian", "sinc", "product_log", "dawson_f", "fresnel_s", "fresnel_c", "harmonic_number", "surd",
    "ramp", "real_abs", "fractional_part", "chebyshev_t", "chebyshev_u", "legendre_p", "hermite_h", "laguerre_l",
    "gegenbauer_c", "spherical_bessel_j", "spherical_bessel_y", "hankel_h1", "hankel_h2", "jacobi_cd", "jacobi_cs",
    "jacobi_dc", "jacobi_ds", "jacobi_nc", "jacobi_nd", "jacobi_ns", "jacobi_sc", "jacobi_sd", "pochhammer",
    // numeric at machine precision or for integer orders only
    "kelvin_ber", "kelvin_bei", "kelvin_ker", "kelvin_kei", "legendre_q", "elliptic_nome_q", "hypergeometric_0f1_regularized", "hypergeometric_1f1_regularized",
    "hypergeometric_2f1_regularized", "factorial",
    // list parameters (checked in mc-special tests/meijer.rs, round 22)
    "meijer_g",
    // defined by formulas in mc-special's cover module (round 19 rows)
    "struve_h", "struve_l", "hypergeometric_u", "logistic_sigmoid", "fresnel_f", "fresnel_g", "jacobi_amplitude", "jacobi_epsilon", "jacobi_zn",
    // round 28 functions (rows added in round 29): their 25-digit ball values are not reliable yet; the rows are
    // checked by central differences in tests/round29.rs
    "scorer_gi", "scorer_hi", "scorer_gi_prime", "scorer_hi_prime", "weierstrass_p", "weierstrass_p_prime",
    "weierstrass_zeta", "weierstrass_sigma", "inverse_weierstrass_p", "elliptic_exp", "coulomb_f", "coulomb_g",
    "coulomb_h1", "coulomb_h2", "lommel_s1", "lommel_s2", "lommel_t1", "lommel_t2", "mathieu_s", "mathieu_s_prime",
    "appell_f1", "appell_f2", "appell_f3", "appell_f4",
    // round 31: list parameters and a series-defined value; the rows are checked by central differences in
    // tests/round31.rs
    "fox_h", "spheroidal_s1", "spheroidal_s1_prime",
    // round 36: the inverse Jacobi functions are real only inside their own domains (the random arguments
    // leave them), and the modular, nome and Bessel-zero rows need arguments in the upper half plane or real zeros; the
    // rows are checked by central differences in crates/mc-engine/tests/r36_w36_special.rs
    "inverse_jacobi_cn", "inverse_jacobi_dn", "inverse_jacobi_cd", "inverse_jacobi_cs", "inverse_jacobi_dc",
    "inverse_jacobi_ds", "inverse_jacobi_nc", "inverse_jacobi_nd", "inverse_jacobi_ns", "inverse_jacobi_sc",
    "inverse_jacobi_sd", "modular_lambda", "klein_invariant_j", "inverse_elliptic_nome_q", "bessel_j_zero",
    "bessel_y_zero", "inverse_gamma_regularized", "inverse_beta_regularized", "barnes_g",
    "spherical_hankel_h1", "spherical_hankel_h2",
    // round 37: formula-defined (anger_j, weber_e) and integration-defined (Mathieu) values; the rows
    // are checked by central differences at 30-40 digits in crates/mc-engine/tests/r37_f37_special.rs
    "anger_j", "weber_e", "mathieu_c", "mathieu_c_prime",
];

#[test]
fn every_rule_of_the_table() {
    let mut s = Sys::new();
    let mut rng = Lcg(2024);
    let mut checked = 0;
    let n = RULES.iter().filter(|r| matches!(r.2, Rule::Partials(_)) && !FORMULA.contains(&r.0)).count();
    for (k, &(name, arity, rule)) in RULES.iter().filter(|r| matches!(r.2, Rule::Partials(_))).enumerate() {
        let Rule::Partials(ps) = rule else { unreachable!() };
        // functions defined by formulas whose values are not refined to 25 digits (their rules are checked at
        // machine precision in `formula_functions_at_machine_precision`)
        if FORMULA.contains(&name) {
            continue;
        }
        assert_eq!(ps.len(), arity, "{name}: one partial per argument");
        for i in 0..arity {
            // a partial in an order argument is the operator itself (no value to compare)
            if OPAQUE.contains(&(name, arity, i)) {
                continue;
            }
            // polygamma's order is an integer
            let constant = |j: usize| if name == "polygamma" && j == 0 { "2" } else { CONSTANTS[j % 3] };
            // functions real only above 1 get their argument shifted there
            let above_one = format!("(1 + {DEPENDENT})");
            let dep = if matches!(name, "acosh" | "acoth" | "asec" | "acsc") { above_one.as_str() } else { DEPENDENT };
            let args: Vec<&str> = (0..arity).map(|j| if j == i { dep } else { constant(j) }).collect();
            // abs is also checked on the negative side
            let shifted = format!("({DEPENDENT} - 3/5)");
            let variants = if name == "abs" { vec![args.join(", "), shifted] } else { vec![args.join(", ")] };
            for a in variants {
                for _ in 0..3 {
                    let p = rng.point(0.1, 0.9);
                    agree(&mut s, &format!("{name}({a})"), "x", &p, "(1/2)").unwrap();
                    checked += 1;
                }
            }
        }
        println!("[{}/{n} {}%] {name}/{arity}", k + 1, (k + 1) * 100 / n);
    }
    assert!(checked >= 3 * n);
}

/// A random expression in x and y, as source text, defined and smooth near the points used (x, y in (0.1, 0.9)):
/// arguments of log, sqrt and powers with symbolic exponents are kept positive, those of asin, acos and tan small.
fn expr(rng: &mut Lcg, depth: u32) -> String {
    if depth == 0 || rng.below(5) == 0 {
        return match rng.below(5) {
            0 | 1 => "x".into(),
            2 | 3 => "y".into(),
            _ => format!("{}/{}", 1 + rng.below(5), 1 + rng.below(4)),
        };
    }
    let mut sub = || expr(rng, depth - 1);
    let (a, b) = (sub(), sub());
    let pos = |u: &str| format!("(1 + ({u})^2)");
    let small = |u: &str| format!("(({u})/(2 + ({u})^2))");
    match rng.below(20) {
        0 | 1 => format!("{a} + {b}"),
        2 | 3 => format!("({a})*({b})"),
        4 => format!("({a})/{}", pos(&b)),
        5 => format!("({a})^{}", 2 + rng.below(3)),
        6 => format!("{}^({b})", pos(&a)),
        7 => format!("{}^(-1/2)", pos(&a)),
        8 => format!("sin({a})"),
        9 => format!("cos({a})"),
        10 => format!("exp(({a})/3)"),
        11 => format!("log({})", pos(&a)),
        12 => format!("sqrt({})", pos(&a)),
        13 => format!("atan({a})"),
        14 => format!("asin({})", small(&a)),
        15 => format!("acos({})", small(&a)),
        16 => format!("tan({})", small(&a)),
        17 => format!("sinh({a}) - cosh({b})"),
        18 => format!("tanh({a})"),
        // (abs is checked in the rule test: n() of abs over a ball does not refine its precision)
        _ => format!("({a})*cosh({b})"),
    }
}

#[test]
fn random_compositions() {
    let mut s = Sys::new();
    let mut rng = Lcg(7);
    let n = 120;
    for k in 0..n {
        let f = expr(&mut rng, 3);
        let (px, py) = (rng.point(0.1, 0.9), rng.point(0.1, 0.9));
        for var in ["x", "y"] {
            agree(&mut s, &f, var, &px, &py).unwrap();
        }
        if (k + 1) % 20 == 0 {
            println!("[{}/{n} {}%]", k + 1, (k + 1) * 100 / n);
        }
    }
}

/// Mixed partials are symmetric: numerically always, and identical as terms for most inputs.
#[test]
fn mixed_partials_are_symmetric() {
    let mut s = Sys::new();
    let mut rng = Lcg(99);
    let n = 80;
    let mut identical = 0;
    for k in 0..n {
        let f = expr(&mut rng, 3);
        let (px, py) = (rng.point(0.1, 0.9), rng.point(0.1, 0.9));
        let src = format!(
            "ff = {f}; a = d(ff, x, y); b = d(ff, y, x); \
             [n(1 + rewrite(a - b, [x -> {px}, y -> {py}])*10^20, 25), n(abs(rewrite(a, [x -> {px}, y -> {py}])))]"
        );
        let out = s.run(&src);
        assert!(within(&out), "{f}: {out}");
        if s.run("a === b") == "true" {
            identical += 1;
        }
        if (k + 1) % 20 == 0 {
            println!("[{}/{n} {}%]", k + 1, (k + 1) * 100 / n);
        }
    }
    println!("{identical} of {n} mixed partials identical as terms");
    assert!(identical * 2 >= n, "only {identical} of {n} identical");
    // the third-order partials of a smooth function in every order
    let mut s = Sys::new();
    let f = "exp(x*y)*sin(x + y^2)/(1 + x^2)";
    let orders = [("x, x, y", "x, y, x"), ("x, y, x", "y, x, x"), ("y, y, x", "x, y, y")];
    for (o1, o2) in orders {
        let out = s.run(&format!(
            "ff = {f}; g = d(ff, {o1}) - d(ff, {o2}); n(1 + rewrite(g, [x -> 3/7, y -> 5/9])*10^20, 25)"
        ));
        assert!((number(&out) - 1.0).abs() < 1.0, "{o1} / {o2}: {out}");
    }
}

/// The rules of the functions defined by formulas, against central differences of machine values at a few points.
#[test]
fn formula_functions_at_machine_precision() {
    let mut s = Sys::new();
    let cases = [
        ("sec(x)", 0.4), ("csc(x)", 0.7), ("cot(x)", 0.7), ("sech(x)", 0.4), ("csch(x)", 0.7), ("coth(x)", 0.7),
        ("asinh(x)", 0.4), ("acosh(x)", 1.7), ("atanh(x)", 0.4), ("acoth(x)", 1.7), ("acot(x)", 0.4),
        ("asec(x)", 1.7), ("acsc(x)", 1.7), ("asech(x)", 0.4), ("acsch(x)", 0.7), ("gudermannian(x)", 0.4),
        ("sinc(x)", 0.7), ("product_log(x)", 0.7), ("dawson_f(x)", 0.7), ("fresnel_s(x)", 0.7), ("fresnel_c(x)", 0.7),
        ("surd(x, 3)", 0.7), ("chebyshev_t(5/2, x)", 0.4), ("chebyshev_u(3, x)", 0.4), ("legendre_p(4, x)", 0.4),
        ("hermite_h(3, x)", 0.4), ("laguerre_l(3, x)", 0.4), ("gegenbauer_c(3, 1/2, x)", 0.4),
        ("spherical_bessel_j(2, x)", 0.7), ("spherical_bessel_y(2, x)", 0.7), ("jacobi_sc(x, 1/3)", 0.4),
        ("jacobi_nd(x, 1/3)", 0.4), ("jacobi_cs(x, 1/3)", 0.4), ("pochhammer(1/3, x)", 0.7),
        ("legendre_q(2, x)", 0.4), ("elliptic_nome_q(x)", 0.3), ("hypergeometric_0f1_regularized(1/3, x)", 0.7),
        ("hypergeometric_1f1_regularized(1/3, 2/3, x)", 0.7), ("hypergeometric_2f1_regularized(1/3, 2/3, 5/7, x)", 0.3),
        ("laguerre_l(3, 1/2, x)", 0.4),
        ("struve_h(5/7, x)", 0.6), ("struve_l(5/7, x)", 0.6), ("hypergeometric_u(1/3, 2/7, x)", 0.7),
        ("logistic_sigmoid(x)", 0.4), ("fresnel_f(x)", 0.7), ("fresnel_g(x)", 0.7),
        ("jacobi_amplitude(x, 1/3)", 0.4), ("jacobi_epsilon(x, 1/3)", 0.4), ("jacobi_zn(x, 1/3)", 0.4),
    ];
    for (f, x) in cases {
        let h = 1e-5;
        let at = |s: &mut Sys, e: &str, v: f64| number(&s.run(&format!("n(rewrite({e}, x -> {v:?}))")));
        let d = at(&mut s, &format!("d({f}, x)"), x);
        let fd = (at(&mut s, f, x + h) - at(&mut s, f, x - h)) / (2.0 * h);
        assert!((d - fd).abs() <= 1e-6 * (1.0 + d.abs()), "{f} at {x}: {d} vs {fd}");
    }
}
