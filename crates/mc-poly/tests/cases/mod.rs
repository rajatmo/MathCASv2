//! The polynomial benchmark cases (bench/cases/P, R8) as sessions, with their properties checked on the result
//! term (the cases' own property lines use functions of other crates: `length`, `rewrite`, `binomial`).
#![allow(dead_code)]
use super::common::Sys;
use mc_api::{Store, View, T};
use mc_arith::Arith;
use mc_core::Heap;
use num_bigint::BigInt;

pub fn binomial(n: u64, k: u64) -> u64 {
    (0..k).fold(1u64, |acc, i| acc * (n - i) / (i + 1))
}

fn int_of(h: &Heap, t: T<'_>) -> BigInt {
    let x = Arith::new(h).num(t).expect("a number");
    let q = x.as_rat().expect("an exact number");
    assert!(q.is_int(), "an integer coefficient");
    let s = q.num.mag.to_decimal();
    let v: BigInt = s.parse().expect("decimal");
    if q.num.neg {
        -v
    } else {
        v
    }
}

/// (number of terms, sum of the coefficients) of a canonical sum of monomials with integer coefficients: the
/// value at 1 of every variable.
pub fn terms_and_value(h: &Heap, t: T<'_>) -> (usize, BigInt) {
    let ar = Arith::new(h);
    let add = h.sym_term(h.sym("add"));
    let mul = h.sym_term(h.sym("mul"));
    let args: Vec<T<'_>> = match h.view(t) {
        View::App { head, args } if head == add => args.to_vec(),
        _ => vec![t],
    };
    let mut sum = BigInt::from(0);
    for &a in &args {
        sum += if ar.num(a).is_some() {
            int_of(h, a)
        } else {
            match h.view(a) {
                View::App { head, args } if head == mul && ar.num(args[0]).is_some() => int_of(h, args[0]),
                _ => BigInt::from(1),
            }
        };
    }
    (args.len(), sum)
}

/// P1 / P2: f = (1 + x_1 + ... + x_v)^m, expand(f*(f + 1)); terms binomial(2m + v, v), value (v+1)^m ((v+1)^m + 1).
pub fn dense_product(s: &mut Sys, vars: &[&str], m: u64) {
    let base = format!("1 + {}", vars.join(" + "));
    s.run(&format!("f = expand(({base})^{m}); r = expand(f*(f + 1)); 1"));
    let (n, value) = s.inspect("r", terms_and_value);
    let v = vars.len() as u64;
    assert_eq!(n as u64, binomial(2 * m + v, v), "terms");
    let p = BigInt::from(v + 1).pow(m as u32);
    assert_eq!(value, &p * (&p + 1u32), "value at 1");
}

/// R8: expand((a + b + c + d)^m); terms binomial(m + 3, 3), value 4^m.
pub fn r8(s: &mut Sys, m: u64) {
    s.run(&format!("r = expand((a + b + c + d)^{m}); 1"));
    let (n, value) = s.inspect("r", terms_and_value);
    assert_eq!(n as u64, binomial(m + 3, 3));
    assert_eq!(value, BigInt::from(4).pow(m as u32));
}

/// P4: the gcd of two products sharing g = (1 + x + y + z)^m + 3 is g up to sign.
pub fn p4(s: &mut Sys, m: u64) {
    let src = format!(
        "g = expand((1 + x + y + z)^{m} + 3); p = expand(g*(x^2 + y*z + 1)^2); q = expand(g*(x*y - z + 5)^3); \
         r = gcd(p, q); expand(r - g) === 0 or expand(r + g) === 0"
    );
    assert_eq!(s.run(&src), "true");
}

/// P5: factor((x^d + 2x + 1)(x^(d/2) - 3x^2 + 7)(x^3 + x + 1)) multiplies back.
pub fn p5(s: &mut Sys, d: u64) {
    let src =
        format!("p = expand((x^{d} + 2x + 1)*(x^{} - 3x^2 + 7)*(x^3 + x + 1)); r = factor(p); expand(r) === p", d / 2);
    assert_eq!(s.run(&src), "true");
}

/// P7: resultant(f, f + 7, x) for f = (x + 2)^m - x.
pub fn p7(s: &mut Sys, m: u64) -> String {
    s.run(&format!("f = expand((x + 2)^{m} - x); resultant(f, f + 7, x)"))
}

/// P11: p = sum_(k=1..m) (k + 10^50) x^k (built directly: the case uses lambdas), expand(p*p); 2m - 1 terms,
/// value s^2 with s = p(1).
pub fn p11(s: &mut Sys, m: u64) {
    s.define("p", move |h| {
        let ar = Arith::new(h);
        let x = h.sym_term(h.sym("x"));
        let big = ar.int_from_decimal(&format!("1{}", "0".repeat(50))).expect("decimal");
        let terms: Vec<T<'_>> =
            (1..=m as i64).map(|k| ar.mul(&[ar.add(&[h.int(k), big]), ar.pow(x, h.int(k))])).collect();
        ar.add(&terms)
    });
    s.run("r = expand(p*p); 1");
    let (n, value) = s.inspect("r", terms_and_value);
    assert_eq!(n as u64, 2 * m - 1);
    let ten50 = BigInt::from(10).pow(50);
    let sum: BigInt = BigInt::from(m) * (m + 1) / 2u32 + ten50 * m;
    assert_eq!(value, &sum * &sum);
}

/// P12: the coefficients 3, 4, 6, 8 of normal(series(exp(sin(x)), x, 0, m)).
pub fn p12(s: &mut Sys, m: u64) {
    let src = format!(
        "r = normal(series(exp(sin(x)), x, 0, {m})); [coefficient(r, x, 3), coefficient(r, x, 4), coefficient(r, x, 6), coefficient(r, x, 8)]"
    );
    assert_eq!(s.run(&src), "[0, -1/8, -1/240, 31/5760]");
    assert_eq!(s.run("degree(r, x)"), m.to_string());
}
