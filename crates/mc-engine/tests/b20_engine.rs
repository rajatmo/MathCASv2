//! Round 20 engine fixes of bugs found in round 19 (gcd of symbols,
//! variables of lists, attributes of AC heads, minus, numerator of complex rationals, periodic indefinite sums,
//! purely imaginary inverse hyperbolic values, unsigned zeros in machine complex numbers).
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn check(cases: &[(&str, &str)]) {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() });
    let mut bad = vec![];
    for (src, want) in cases {
        let got = match e.eval(src, &StopFlag::new()) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        if got != *want {
            bad.push(format!("{src}\n   got  {got}\n   want {want}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn gcd_and_lcm_of_symbols_stay() {
    check(&[
        ("gcd(a, m)", "gcd(a, m)"),
        ("lcm(a, m)", "lcm(a, m)"),
        ("gcd(x^2 - 1, x - 1)", "-1 + x"),
        ("gcd(6, 4)", "2"),
        ("polynomial_gcd(a, m)", "1"),
    ]);
}

#[test]
fn variables_of_lists_and_equations() {
    check(&[("variables([p, q])", "[p, q]"), ("variables([p, q^2 + r, p])", "[p, q, r]"), ("variables(x^2 == y)", "[x, y]")]);
}

#[test]
fn attributes_and_operator_functions() {
    check(&[
        ("attributes(add)", "[flat, listable, orderless]"),
        ("minus(3)", "-3"),
        ("map(minus, [1, a])", "[-1, -a]"),
        ("key_sort_by(assoc(1 -> x, 2 -> y, 3 -> z), minus)", "assoc(3 -> z, 2 -> y, 1 -> x)"),
    ]);
}

#[test]
fn periodic_indefinite_sums_by_differencing() {
    for f in ["mod(k^2, 2)", "mod(k, 3)", "mod(k^2 + k + 1, 5)*mod(k, 2)"] {
        let src = format!(
            "{{ let s = sum({f}, k); count(table(rewrite(s, k -> j + 1) - rewrite(s, k -> j) == rewrite({f}, k -> j), j, range(-7, 12)), true) }}"
        );
        check(&[(&src, "20")]);
    }
}

#[test]
fn imaginary_values_and_signed_zeros() {
    check(&[
        // a zero ball without radius (the parts of a complex number share their kind)
        ("{ let r = re(n(acosh(1/2), 30)); [r == 0, set_precision(r, inf)] }", "[true, 0]"),
        ("abs(im_part(n(acosh(1/2), 30)) - pi/3) < 10^-29", "true"),
        ("denominator(1/(a^b*x))", "x*a^b"),
    ]);
}
