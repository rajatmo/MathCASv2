//! Property tests of the canonicaliser on random polynomial expressions:
//! - canonical forms are unique: a polynomial built in many different ways is one handle;
//! - like-term collection matches a reference implementation (monomials collected in a map, the canonical structure
//!   then built directly in the store);
//! - the canonical form of a random nested expression (sums, products, powers, differences, division by numbers)
//!   has the same expansion as the expression, computed independently with num-bigint.
mod common;

use common::{big_of, Rng};
use mc_api::{RefStore, Store, View, T};
use mc_arith::Arith;
use num_bigint::BigInt;
use std::collections::BTreeMap;

const VARS: [&str; 3] = ["x", "y", "z"];

/// A monomial with a small rational coefficient.
#[derive(Clone)]
struct Mono {
    c: (i64, i64),
    e: [u32; 3],
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 {
        a.abs()
    } else {
        gcd(b, a % b)
    }
}

/// n/d reduced with a positive denominator.
fn qnorm(n: i64, d: i64) -> (i64, i64) {
    let g = gcd(n, d).max(1) * d.signum();
    (n / g, d / g)
}

/// a/b + c/d (small values).
fn qadd(x: (i64, i64), y: (i64, i64)) -> (i64, i64) {
    qnorm(x.0 * y.1 + y.0 * x.1, x.1 * y.1)
}

fn qmul(x: (i64, i64), y: (i64, i64)) -> (i64, i64) {
    qnorm(x.0 * y.0, x.1 * y.1)
}

fn rand_mono(rng: &mut Rng) -> Mono {
    let mut c = (rng.range(-6, 6), rng.range(1, 4));
    if c.0 == 0 {
        c.0 = 1;
    }
    let g = gcd(c.0, c.1);
    Mono { c: (c.0 / g, c.1 / g), e: [rng.below(4) as u32, rng.below(3) as u32, rng.below(3) as u32] }
}

/// Applies a binary operation over the items in a random tree shape.
fn tree<'h>(rng: &mut Rng, items: &[T<'h>], op: &dyn Fn(T<'h>, T<'h>) -> T<'h>, flat: &dyn Fn(&[T<'h>]) -> T<'h>) -> T<'h> {
    match items.len() {
        0 => flat(&[]),
        1 => items[0],
        n => {
            if rng.below(4) == 0 {
                return flat(items);
            }
            let k = 1 + rng.below(n as u64 - 1) as usize;
            let (l, r) = (tree(rng, &items[..k], op, flat), tree(rng, &items[k..], op, flat));
            op(l, r)
        }
    }
}

/// One monomial, built in a random way: the coefficient possibly split into two factors, each exponent split
/// into several powers, the factors shuffled and multiplied in a random tree.
fn build_mono<'h>(a: &Arith<'h, RefStore>, rng: &mut Rng, m: &Mono) -> T<'h> {
    let st = a.store();
    let mut f = vec![];
    if rng.below(2) == 0 {
        f.push(a.rational(a.int(m.c.0), a.int(m.c.1)));
    } else {
        let c1 = (rng.range(1, 3) * if rng.below(2) == 0 { 1 } else { -1 }, rng.range(1, 3));
        let c2 = qmul(m.c, (c1.1, c1.0));
        f.push(a.rational(a.int(c1.0), a.int(c1.1)));
        f.push(a.rational(a.int(c2.0), a.int(c2.1)));
    }
    for (v, &e) in VARS.iter().zip(&m.e) {
        let x = st.sym_term(st.sym(v));
        let mut left = e;
        while left > 0 {
            let part = 1 + rng.below(left as u64) as u32;
            f.push(if part == 1 { x } else { a.pow(x, a.int(part as i64)) });
            left -= part;
        }
    }
    rng.shuffle(&mut f);
    tree(rng, &f, &|l, r| a.mul(&[l, r]), &|xs| a.mul(xs))
}

/// A polynomial built in a random way: monomials possibly split into two with summing coefficients, cancelling
/// pairs added, terms shuffled and summed in a random tree, sometimes as a difference of a negation.
fn build_poly<'h>(a: &Arith<'h, RefStore>, rng: &mut Rng, monos: &[Mono]) -> T<'h> {
    let mut terms = vec![];
    for m in monos {
        if rng.below(3) == 0 {
            let c1 = (rng.range(-3, 3), rng.range(1, 3));
            let c2 = qadd(m.c, (-c1.0, c1.1));
            terms.push(build_mono(a, rng, &Mono { c: c1, e: m.e }));
            terms.push(build_mono(a, rng, &Mono { c: c2, e: m.e }));
        } else {
            terms.push(build_mono(a, rng, m));
        }
    }
    if rng.below(2) == 0 {
        let m = rand_mono(rng);
        let t = build_mono(a, rng, &m);
        terms.push(t);
        terms.push(a.neg(build_mono(a, rng, &m)));
    }
    rng.shuffle(&mut terms);
    tree(rng, &terms, &|l, r| if rng_free_choice(l, r) { a.sub(l, a.neg(r)) } else { a.add(&[l, r]) }, &|xs| a.add(xs))
}

/// A deterministic choice that does not need the generator inside the closure.
fn rng_free_choice(l: T<'_>, r: T<'_>) -> bool {
    (l.raw() ^ r.raw()) & 1 == 1
}

/// The reference: monomials collected in a map, zero coefficients dropped, then the canonical structure built
/// directly in the store (numbers first, factors and terms sorted by the canonical order).
fn reference<'h>(a: &Arith<'h, RefStore>, monos: &[Mono]) -> T<'h> {
    let st = a.store();
    let mut coll: BTreeMap<[u32; 3], (i64, i64)> = BTreeMap::new();
    for m in monos {
        let e = coll.entry(m.e).or_insert((0, 1));
        *e = qadd(*e, m.c);
    }
    let mut constant = None;
    let mut terms = vec![];
    for (e, c) in coll {
        if c.0 == 0 {
            continue;
        }
        let ct = a.rational(a.int(c.0), a.int(c.1));
        let mut f = vec![];
        for (v, &k) in VARS.iter().zip(&e) {
            let x = st.sym_term(st.sym(v));
            match k {
                0 => {}
                1 => f.push(x),
                _ => f.push(st.call(st.sym("pow"), &[x, st.int(k as i64)])),
            }
        }
        f.sort_by(|&p, &q| a.cmp(p, q));
        if f.is_empty() {
            constant = Some(ct);
            continue;
        }
        if c != (1, 1) {
            f.insert(0, ct);
        }
        terms.push(if f.len() == 1 { f[0] } else { st.call(st.sym("mul"), &f) });
    }
    terms.sort_by(|&p, &q| a.cmp(p, q));
    if let Some(c) = constant {
        terms.insert(0, c);
    }
    match terms.len() {
        0 => st.int(0),
        1 => terms[0],
        _ => st.call(st.sym("add"), &terms),
    }
}

#[test]
fn canonical_polynomials_are_unique_and_match_the_reference() {
    let st = RefStore::new();
    let a = Arith::new(&st);
    let mut rng = Rng(101);
    let cases = 400;
    for case in 0..cases {
        let monos: Vec<Mono> = (0..rng.range(0, 7)).map(|_| rand_mono(&mut rng)).collect();
        let want = reference(&a, &monos);
        for way in 0..4 {
            let got = build_poly(&a, &mut rng, &monos);
            assert_eq!(got, want, "case {case} way {way}: {} against {}", common::show(&st, got), common::show(&st, want));
        }
        if case % 100 == 99 {
            println!("[{}/{cases} {}%]", case + 1, (case + 1) * 100 / cases);
        }
    }
}

// ---------------------------------------------------------------- expansion against num-bigint

/// A random expression.
enum E {
    Var(usize),
    Num(i64, i64),
    Add(Vec<E>),
    Mul(Vec<E>),
    Sub(Box<E>, Box<E>),
    Neg(Box<E>),
    Pow(Box<E>, u32),
    DivNum(Box<E>, i64),
}

fn rand_expr(rng: &mut Rng, depth: u32) -> E {
    if depth == 0 || rng.below(4) == 0 {
        return if rng.below(2) == 0 { E::Var(rng.below(3) as usize) } else { E::Num(rng.range(-4, 4), rng.range(1, 3)) };
    }
    let d = depth - 1;
    match rng.below(6) {
        0 => E::Add((0..rng.range(1, 3)).map(|_| rand_expr(rng, d)).collect()),
        1 => E::Mul((0..rng.range(1, 3)).map(|_| rand_expr(rng, d)).collect()),
        2 => E::Sub(Box::new(rand_expr(rng, d)), Box::new(rand_expr(rng, d))),
        3 => E::Neg(Box::new(rand_expr(rng, d))),
        4 => E::Pow(Box::new(rand_expr(rng, d.min(1))), rng.below(4) as u32),
        _ => E::DivNum(Box::new(rand_expr(rng, d)), rng.range(1, 3) * if rng.below(2) == 0 { 1 } else { -1 }),
    }
}

fn build<'h>(a: &Arith<'h, RefStore>, e: &E) -> T<'h> {
    let st = a.store();
    match e {
        E::Var(i) => st.sym_term(st.sym(VARS[*i])),
        E::Num(n, d) => a.rational(a.int(*n), a.int(*d)),
        E::Add(xs) => a.add(&xs.iter().map(|x| build(a, x)).collect::<Vec<_>>()),
        E::Mul(xs) => a.mul(&xs.iter().map(|x| build(a, x)).collect::<Vec<_>>()),
        E::Sub(x, y) => a.sub(build(a, x), build(a, y)),
        E::Neg(x) => a.neg(build(a, x)),
        E::Pow(x, k) => a.pow(build(a, x), a.int(*k as i64)),
        E::DivNum(x, k) => a.div(build(a, x), a.int(*k)),
    }
}

/// A polynomial over the rationals: monomial exponents to an unreduced fraction.
type Poly = BTreeMap<[u32; 3], (BigInt, BigInt)>;

fn padd(p: &Poly, q: &Poly) -> Poly {
    let mut r = p.clone();
    for (k, (n, d)) in q {
        let e = r.entry(*k).or_insert((BigInt::from(0), BigInt::from(1)));
        *e = (&e.0 * d + n * &e.1, &e.1 * d);
    }
    r
}

fn pmul(p: &Poly, q: &Poly) -> Poly {
    let mut r = Poly::new();
    for (k1, (n1, d1)) in p {
        for (k2, (n2, d2)) in q {
            let k = [k1[0] + k2[0], k1[1] + k2[1], k1[2] + k2[2]];
            let e = r.entry(k).or_insert((BigInt::from(0), BigInt::from(1)));
            let (n, d) = (n1 * n2, d1 * d2);
            *e = (&e.0 * &d + n * &e.1, &e.1 * d);
        }
    }
    r
}

fn pconst(n: BigInt, d: BigInt) -> Poly {
    Poly::from([([0, 0, 0], (n, d))])
}

fn ppow(p: &Poly, k: u32) -> Poly {
    (0..k).fold(pconst(1.into(), 1.into()), |acc, _| pmul(&acc, p))
}

fn expand_expr(e: &E) -> Poly {
    match e {
        E::Var(i) => {
            let mut k = [0; 3];
            k[*i] = 1;
            Poly::from([(k, (BigInt::from(1), BigInt::from(1)))])
        }
        E::Num(n, d) => pconst((*n).into(), (*d).into()),
        E::Add(xs) => xs.iter().fold(Poly::new(), |acc, x| padd(&acc, &expand_expr(x))),
        E::Mul(xs) => xs.iter().fold(pconst(1.into(), 1.into()), |acc, x| pmul(&acc, &expand_expr(x))),
        E::Sub(x, y) => padd(&expand_expr(x), &pmul(&pconst((-1).into(), 1.into()), &expand_expr(y))),
        E::Neg(x) => pmul(&pconst((-1).into(), 1.into()), &expand_expr(x)),
        E::Pow(x, k) => ppow(&expand_expr(x), *k),
        E::DivNum(x, k) => pmul(&pconst(1.into(), (*k).into()), &expand_expr(x)),
    }
}

/// The expansion of a canonical term, read back from the store.
fn expand_term(st: &RefStore, t: T<'_>) -> Poly {
    if let Some(v) = big_of(st, t) {
        return pconst(v, 1.into());
    }
    match st.view(t) {
        View::Rational { num, den } => pconst(big_of(st, num).unwrap(), big_of(st, den).unwrap()),
        View::Sym(s) => {
            let i = VARS.iter().position(|v| *v == st.sym_name(s)).expect("a variable");
            let mut k = [0; 3];
            k[i] = 1;
            Poly::from([(k, (BigInt::from(1), BigInt::from(1)))])
        }
        View::App { head, args } => match st.sym_name(match st.view(head) {
            View::Sym(s) => s,
            _ => panic!("compound head"),
        })
        .as_str()
        {
            "add" => args.iter().fold(Poly::new(), |acc, &x| padd(&acc, &expand_term(st, x))),
            "mul" => args.iter().fold(pconst(1.into(), 1.into()), |acc, &x| pmul(&acc, &expand_term(st, x))),
            "pow" => {
                let k: u32 = big_of(st, args[1]).and_then(|k| k.try_into().ok()).expect("a natural exponent");
                ppow(&expand_term(st, args[0]), k)
            }
            h => panic!("unexpected head {h}"),
        },
        _ => panic!("unexpected term"),
    }
}

/// Equal polynomials: every coefficient equal by cross-multiplication (zero entries count as absent).
fn same(p: &Poly, q: &Poly) -> bool {
    let zero = (BigInt::from(0), BigInt::from(1));
    p.keys().chain(q.keys()).all(|k| {
        let (a, b) = p.get(k).unwrap_or(&zero);
        let (c, d) = q.get(k).unwrap_or(&zero);
        a * d == c * b
    })
}

#[test]
fn canonical_forms_expand_like_the_expression() {
    let st = RefStore::new();
    let a = Arith::new(&st);
    let mut rng = Rng(202);
    let cases = 2000;
    for case in 0..cases {
        let e = rand_expr(&mut rng, 4);
        let t = build(&a, &e);
        assert!(same(&expand_term(&st, t), &expand_expr(&e)), "case {case}: {}", common::show(&st, t));
        // canonicalising again changes nothing: the top level of a canonical term is a fixed point
        if let View::App { head, args } = st.view(t) {
            let again = match common::show(&st, head).as_str() {
                "add" => a.add(args),
                "mul" => a.mul(args),
                "pow" => a.pow(args[0], args[1]),
                _ => t,
            };
            assert_eq!(again, t, "case {case}: not a fixed point: {}", common::show(&st, t));
        }
        if case % 500 == 499 {
            println!("[{}/{cases} {}%]", case + 1, (case + 1) * 100 / cases);
        }
    }
}
