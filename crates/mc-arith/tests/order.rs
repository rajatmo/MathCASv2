//! Property tests of the canonical order (semantics 8) on random terms of every kind: total, antisymmetric,
//! transitive, consistent with identity and with decided numeric comparisons, and stable (the same terms built in
//! another store in another creation order sort the same way). Deep terms need no deep stack.
mod common;

use common::{show, Rng};
use mc_api::{RefStore, Store, T};
use mc_arith::{Arith, Cmp3};
use std::cmp::Ordering;

/// A term description, so that the same terms can be built in two stores.
#[derive(Clone, Debug)]
enum Spec {
    Int(i64),
    /// a big integer as a decimal string
    Big(String),
    Rat(i64, i64),
    Real(f64),
    Ball(i64, i64, u64),
    BallRad(i64, i64, u64, i64),
    Complex(Box<Spec>, Box<Spec>),
    /// `complex(re, im)` built raw, not normalised
    RawComplex(i64, i64),
    Str(String),
    Sym(String),
    App(Box<Spec>, Vec<Spec>),
}

fn rand_real(rng: &mut Rng) -> Spec {
    let n = rng.range(-5, 5);
    match rng.below(8) {
        0 => Spec::Int(n),
        1 => Spec::Int([i64::MIN, i64::MAX, 1 << 62, -(1 << 40)][rng.below(4) as usize]),
        2 => Spec::Big(format!("{}{}", if rng.below(2) == 0 { "-" } else { "" }, "9".repeat(20 + rng.below(30) as usize))),
        3 => Spec::Rat(n, rng.range(1, 6)),
        4 => Spec::Real([0.0, -0.0, 0.5, -1.5, 1e300, f64::INFINITY, f64::NEG_INFINITY, f64::NAN, n as f64][rng.below(9) as usize]),
        5 => Spec::Ball(n, rng.range(1, 6), [53, 64, 200][rng.below(3) as usize]),
        6 => Spec::BallRad(n, rng.range(1, 6), 64, rng.range(0, 3)),
        _ => Spec::Real(n as f64 / 4.0),
    }
}

fn rand_spec(rng: &mut Rng, depth: u32) -> Spec {
    let leaf = depth == 0 || rng.below(3) == 0;
    match rng.below(if leaf { 5 } else { 7 }) {
        0 => rand_real(rng),
        1 => match rng.below(3) {
            0 => Spec::Complex(Box::new(rand_real(rng)), Box::new(rand_real(rng))),
            1 => Spec::RawComplex(rng.range(-2, 2), rng.range(-1, 1)),
            _ => Spec::Complex(Box::new(Spec::Int(rng.range(-2, 2))), Box::new(Spec::Int(rng.range(-2, 2)))),
        },
        2 => Spec::Str(["", "a", "ab", "b", "\u{e9}", "Z", "\u{3b1}"][rng.below(7) as usize].to_string()),
        3 | 4 => Spec::Sym(["x", "y", "alg.x", "alg.gcd", "gcd", "core.add", "add", "mul", "f", "g"][rng.below(10) as usize].to_string()),
        5 => {
            let head = if rng.below(4) == 0 { rand_spec(rng, depth - 1) } else { Spec::Sym(["f", "g", "h"][rng.below(3) as usize].to_string()) };
            Spec::App(Box::new(head), (0..rng.below(4)).map(|_| rand_spec(rng, depth - 1)).collect())
        }
        _ => Spec::App(Box::new(Spec::Sym("f".to_string())), vec![rand_spec(rng, depth - 1)]),
    }
}

fn build<'h>(a: &Arith<'h, RefStore>, s: &Spec) -> T<'h> {
    let st = a.store();
    let q = |n: i64, d: i64| a.rational(a.int(n), a.int(d));
    match s {
        Spec::Int(v) => a.int(*v),
        Spec::Big(d) => a.int_from_decimal(d).unwrap(),
        Spec::Rat(n, d) => q(*n, *d),
        Spec::Real(v) => st.real(*v),
        Spec::Ball(n, d, p) => a.ball(q(*n, *d), *p).unwrap(),
        Spec::BallRad(n, d, p, r) => a.ball_with_radius(q(*n, *d), q(*r, 1000), *p).unwrap(),
        Spec::Complex(x, y) => a.complex(build(a, x), build(a, y)),
        Spec::RawComplex(x, y) => st.call(st.sym("complex"), &[a.int(*x), a.int(*y)]),
        Spec::Str(v) => st.string(v),
        Spec::Sym(v) => st.sym_term(st.sym(v)),
        Spec::App(h, xs) => {
            let args: Vec<T<'h>> = xs.iter().map(|x| build(a, x)).collect();
            st.app(build(a, h), &args)
        }
    }
}

#[test]
fn order_is_total_antisymmetric_transitive_and_stable() {
    let mut rng = Rng(606);
    let specs: Vec<Spec> = (0..300).map(|_| rand_spec(&mut rng, 3)).collect();

    let st = RefStore::new();
    let a = Arith::new(&st);
    let mut terms: Vec<T<'_>> = specs.iter().map(|s| build(&a, s)).collect();
    let n = terms.len();
    for (k, &x) in terms.iter().enumerate() {
        for &y in &terms {
            let o = a.cmp(x, y);
            assert_eq!(o == Ordering::Equal, x == y, "equal exactly for one term: {} {}", show(&st, x), show(&st, y));
            assert_eq!(a.cmp(y, x), o.reverse(), "antisymmetric: {} {}", show(&st, x), show(&st, y));
        }
        if k % 100 == 99 {
            println!("[{}/{n} {}%]", k + 1, (k + 1) * 100 / n);
        }
    }
    terms.sort_by(|&x, &y| a.cmp(x, y));
    terms.dedup();
    // a sorted list is ordered pairwise only if the order is transitive
    for i in 0..terms.len() {
        for j in i + 1..terms.len() {
            assert_eq!(a.cmp(terms[i], terms[j]), Ordering::Less, "transitive: {} {}", show(&st, terms[i]), show(&st, terms[j]));
        }
    }
    // consistent with decided comparisons of exact numbers and balls
    for &x in &terms {
        for &y in &terms {
            let machine = |t: T<'_>| matches!(st.view(t), mc_api::View::Real(_));
            if machine(x) || machine(y) {
                continue;
            }
            match a.compare(x, y) {
                Some(Cmp3::Less) => assert_eq!(a.cmp(x, y), Ordering::Less),
                Some(Cmp3::Greater) => assert_eq!(a.cmp(x, y), Ordering::Greater),
                _ => {}
            }
        }
    }
    let first: Vec<String> = terms.iter().map(|&t| show(&st, t)).collect();

    // stability: another store, other terms created first, the specs built in reverse order
    let st2 = RefStore::new();
    let a2 = Arith::new(&st2);
    for i in 0..100 {
        st2.call(st2.sym("junk"), &[st2.int(i)]);
        st2.sym(&format!("s{i}"));
    }
    let mut terms2: Vec<T<'_>> = specs.iter().rev().map(|s| build(&a2, s)).collect();
    terms2.sort_by(|&x, &y| a2.cmp(x, y));
    terms2.dedup();
    let second: Vec<String> = terms2.iter().map(|&t| show(&st2, t)).collect();
    assert_eq!(first, second, "the order does not depend on creation order");
}

#[test]
fn draft_order_rules() {
    let st = RefStore::new();
    let a = Arith::new(&st);
    let s = |n: &str| st.sym_term(st.sym(n));
    let f = st.sym("f");
    let seq = [
        a.int(-1),
        a.int(0),
        st.real(0.0),
        a.complex(a.int(0), a.int(1)),
        a.rational(a.int(1), a.int(2)),
        st.real(0.5),
        a.int(1),
        st.string("a"),
        st.string("b"),
        s("a"),
        s("alg.a"),
        s("core.a"),
        s("b"),
        st.call(f, &[]),
        st.call(f, &[a.int(1)]),
        st.call(f, &[a.int(2)]),
        st.call(f, &[a.int(1), a.int(1)]),
        st.call(st.sym("g"), &[]),
        st.app(st.call(f, &[]), &[]),
    ];
    for (i, &x) in seq.iter().enumerate() {
        for (j, &y) in seq.iter().enumerate() {
            assert_eq!(a.cmp(x, y), i.cmp(&j), "{} {}", show(&st, x), show(&st, y));
        }
    }
}

#[test]
fn deep_terms_need_no_deep_stack() {
    let st = RefStore::new();
    let a = Arith::new(&st);
    let g = st.sym("g");
    let (mut x, mut y) = (st.sym_term(st.sym("x")), st.sym_term(st.sym("y")));
    for _ in 0..200_000 {
        x = st.call(g, &[x]);
        y = st.call(g, &[y]);
    }
    assert_eq!(a.cmp(x, y), Ordering::Less);
    // a raw sum nested 200 000 deep flattens without recursion
    let mut s = a.int(1);
    let add = st.sym("add");
    for _ in 0..200_000 {
        s = st.call(add, &[a.int(1), s]);
    }
    assert_eq!(a.add(&[s]), a.int(200_001));
}
