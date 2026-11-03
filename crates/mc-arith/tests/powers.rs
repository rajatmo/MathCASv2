//! Property tests of products of powers (semantics 2.4 and 3): random products of symbolic and numeric powers with
//! integer and rational exponents, built three different ways (factor by factor in random order and grouping,
//! exponents of each base added first, positive composite bases split into prime bases), give one handle, and
//! the canonical form has the value of the product (principal branch, checked in complex machine arithmetic).
mod common;

use common::{show, Rng};
use common::TestStore;
use mc_api::{Store, View, T};
use mc_arith::Arith;

/// Bases: symbols and rationals (as numerator, denominator).
const BASES: [(&str, i64, i64); 12] = [
    ("x", 0, 0),
    ("y", 0, 0),
    ("", 2, 1),
    ("", 3, 1),
    ("", 6, 1),
    ("", 12, 1),
    ("", 1, 2),
    ("", 2, 3),
    ("", 8, 1),
    ("", -1, 1),
    ("", -2, 1),
    ("", -3, 1),
];
const EXPS: [(i64, i64); 11] = [(-2, 1), (-1, 1), (1, 1), (2, 1), (1, 2), (-1, 2), (1, 3), (2, 3), (3, 2), (-5, 6), (5, 4)];

/// Values substituted for the symbols in the numeric check.
const VALUES: [(f64, f64); 2] = [(0.7, 0.2), (1.3, -0.4)];

type C = (f64, f64);

fn cmul(a: C, b: C) -> C {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}

fn cadd(a: C, b: C) -> C {
    (a.0 + b.0, a.1 + b.1)
}

/// The principal power exp(w log z).
fn cpow(z: C, w: C) -> C {
    let (lr, th) = (z.0.hypot(z.1).ln(), z.1.atan2(z.0));
    let (x, y) = (w.0 * lr - w.1 * th, w.1 * lr + w.0 * th);
    let r = x.exp();
    (r * y.cos(), r * y.sin())
}

fn base_term<'h>(a: &Arith<'h, TestStore>, i: usize) -> T<'h> {
    let st = a.store();
    let (s, n, d) = BASES[i];
    if s.is_empty() {
        a.rational(a.int(n), a.int(d))
    } else {
        st.sym_term(st.sym(s))
    }
}

fn base_value(i: usize) -> C {
    match BASES[i] {
        ("x", ..) => VALUES[0],
        ("y", ..) => VALUES[1],
        (_, n, d) => (n as f64 / d as f64, 0.0),
    }
}

/// A term's value in complex machine arithmetic (recursion is fine on these small terms).
fn eval(st: &TestStore, t: T<'_>) -> C {
    match st.view(t) {
        View::Int(v) => (v as f64, 0.0),
        View::Rational { num, den } => (eval(st, num).0 / eval(st, den).0, 0.0),
        View::Sym(s) => match st.sym_name(s).as_str() {
            "x" => VALUES[0],
            "y" => VALUES[1],
            n => panic!("symbol {n}"),
        },
        View::App { head, args } => {
            let h = match st.view(head) {
                View::Sym(s) => st.sym_name(s),
                _ => panic!("compound head"),
            };
            let v: Vec<C> = args.iter().map(|&x| eval(st, x)).collect();
            match h.as_str() {
                "complex" => (v[0].0, v[1].0),
                "add" => v.into_iter().fold((0.0, 0.0), cadd),
                "mul" => v.into_iter().fold((1.0, 0.0), cmul),
                "pow" => cpow(v[0], v[1]),
                _ => panic!("head {h}"),
            }
        }
        _ => panic!("unexpected term {}", show(st, t)),
    }
}

fn tree<'h>(a: &Arith<'h, TestStore>, rng: &mut Rng, items: &[T<'h>]) -> T<'h> {
    if items.len() <= 1 || rng.below(3) == 0 {
        return a.mul(items);
    }
    let k = 1 + rng.below(items.len() as u64 - 1) as usize;
    let (l, r) = (tree(a, rng, &items[..k]), tree(a, rng, &items[k..]));
    a.mul(&[l, r])
}

#[test]
fn products_of_powers_are_unique_and_correct() {
    for st in TestStore::all() {
        let a = Arith::new(&st);
        let mut rng = Rng(707);
        let cases = 1500;
        for case in 0..cases {
            let factors: Vec<(usize, usize)> =
                (0..rng.range(1, 6)).map(|_| (rng.below(BASES.len() as u64) as usize, rng.below(EXPS.len() as u64) as usize)).collect();
            let q = |(n, d): (i64, i64)| a.rational(a.int(n), a.int(d));
            // one: factor by factor, shuffled, in a random tree
            let mut f: Vec<T<'_>> = factors.iter().map(|&(b, e)| a.pow(base_term(&a, b), q(EXPS[e]))).collect();
            rng.shuffle(&mut f);
            let one = tree(&a, &mut rng, &f);
            // two: the exponents of each base added first
            let mut by_base: Vec<(usize, T<'_>)> = vec![];
            for &(b, e) in &factors {
                match by_base.iter_mut().find(|(x, _)| *x == b) {
                    Some((_, s)) => *s = a.add(&[*s, q(EXPS[e])]),
                    None => by_base.push((b, q(EXPS[e]))),
                }
            }
            let mut g: Vec<T<'_>> = by_base.iter().map(|&(b, s)| a.pow(base_term(&a, b), s)).collect();
            rng.shuffle(&mut g);
            let two = tree(&a, &mut rng, &g);
            // three: positive composite bases split (6^e = 2^e 3^e, 12^e = 2^(2e) 3^e, (2/3)^e = 2^e 3^-e)
            let mut h = vec![];
            for &(b, e) in &factors {
                let ex = q(EXPS[e]);
                let two_e = a.mul(&[a.int(2), ex]);
                let three_e = a.mul(&[a.int(3), ex]);
                match BASES[b] {
                    ("", 6, 1) => h.extend([a.pow(a.int(2), ex), a.pow(a.int(3), ex)]),
                    ("", 12, 1) => h.extend([a.pow(a.int(2), two_e), a.pow(a.int(3), ex)]),
                    ("", 8, 1) => h.push(a.pow(a.int(2), three_e)),
                    ("", 2, 3) => h.extend([a.pow(a.int(2), ex), a.pow(a.int(3), a.neg(ex))]),
                    ("", 1, 2) => h.push(a.pow(a.int(2), a.neg(ex))),
                    _ => h.push(a.pow(base_term(&a, b), ex)),
                }
            }
            rng.shuffle(&mut h);
            let three = tree(&a, &mut rng, &h);
            assert_eq!(one, two, "case {case}: {} against {}", show(&st, one), show(&st, two));
            assert_eq!(one, three, "case {case}: {} against {}", show(&st, one), show(&st, three));
            // the value: the product of the principal powers
            let want = factors.iter().fold((1.0, 0.0), |acc, &(b, e)| {
                let (n, d) = EXPS[e];
                cmul(acc, cpow(base_value(b), (n as f64 / d as f64, 0.0)))
            });
            let got = eval(&st, one);
            let err = (got.0 - want.0).hypot(got.1 - want.1);
            assert!(err <= 1e-9 * want.0.hypot(want.1).max(1.0), "case {case}: {} is {got:?}, want {want:?}", show(&st, one));
            if case % 500 == 499 {
                println!("[{}/{cases} {}%]", case + 1, (case + 1) * 100 / cases);
            }
        }
    }
}
