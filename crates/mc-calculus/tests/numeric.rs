//! Derivatives agree with central differences at random points, evaluated with balls: every rule of the table and
//! random compositions in two variables; mixed partials are symmetric.
//!
//! A central difference (F(p + h) - F(p - h)) / 2h with the exact step h = 10^-15 differs from F'(p) by about
//! h^2 F'''(p) / 6, so a derivative is accepted when it agrees to 20 digits relative to 1 + |F'(p)|. The difference
//! is evaluated as `n(1 + (D - C) 10^20, 25)`, a number near 1 whatever the cancellation inside, and compared
//! with the machine value of |D|.
mod common;
use common::{number, Lcg, Sys};
use mc_calculus::rules::{Rule, RULES};

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

#[test]
fn every_rule_of_the_table() {
    let mut s = Sys::new();
    let mut rng = Lcg(2024);
    let mut checked = 0;
    let n = RULES.iter().filter(|r| matches!(r.2, Rule::Partials(_))).count();
    for (k, &(name, arity, rule)) in RULES.iter().filter(|r| matches!(r.2, Rule::Partials(_))).enumerate() {
        let Rule::Partials(ps) = rule else { unreachable!() };
        assert_eq!(ps.len(), arity, "{name}: one partial per argument");
        for i in 0..arity {
            let args: Vec<&str> = (0..arity).map(|j| if j == i { DEPENDENT } else { CONSTANTS[j % 3] }).collect();
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
