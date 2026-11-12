//! Field axioms on random elements of several fields, and embeddings checked as homomorphisms.
use mc_arith::num::Nat;
use mc_fields::conway;
use mc_fields::embed;
use mc_fields::field::{El, Field, Repr};
use mc_fields::fqpoly;
use mc_numtheory::budget::Unlimited;
use std::sync::Arc;

/// A small deterministic generator (splitmix64).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn elem(&mut self, f: &Field) -> El {
        let v: Vec<u64> = (0..f.n).map(|_| self.next() % f.p()).collect();
        f.reduce(&v)
    }
}

fn standard(p: u64, n: usize) -> Arc<Field> {
    let (poly, _) = conway::defining(p, n, &mut Unlimited).unwrap().unwrap();
    Field::get(p, &poly, Repr::Polynomial)
}

fn fields() -> Vec<Arc<Field>> {
    vec![
        standard(2, 1),
        standard(5, 1),
        standard(2, 8),
        standard(3, 5),
        standard(17, 3),
        standard(97, 4),
        standard(1009, 3),
        // a defining polynomial that is not monic (2 x^4 + 57 x^3 + 43 x^2 + 17 x + 9 over F_509)
        Field::get(509, &[9, 17, 43, 57, 2], Repr::Polynomial),
        // a large characteristic
        Field::get((1 << 61) - 1, &[1, 0, 1], Repr::Polynomial),
    ]
}

#[test]
fn field_axioms_on_random_elements() {
    let mut r = Rng(42);
    for f in fields() {
        let zero: El = vec![];
        let one = f.one();
        for _ in 0..200 {
            let (a, b, c) = (r.elem(&f), r.elem(&f), r.elem(&f));
            assert_eq!(f.add(&a, &b), f.add(&b, &a));
            assert_eq!(f.mul(&a, &b), f.mul(&b, &a));
            assert_eq!(f.add(&f.add(&a, &b), &c), f.add(&a, &f.add(&b, &c)));
            assert_eq!(f.mul(&f.mul(&a, &b), &c), f.mul(&a, &f.mul(&b, &c)));
            assert_eq!(f.mul(&a, &f.add(&b, &c)), f.add(&f.mul(&a, &b), &f.mul(&a, &c)));
            assert_eq!(f.add(&a, &zero), a);
            assert_eq!(f.mul(&a, &one), a);
            assert_eq!(f.add(&a, &f.neg(&a)), zero);
            assert_eq!(f.sub(&a, &b), f.add(&a, &f.neg(&b)));
            if !a.is_empty() {
                let ia = f.inv(&a).unwrap();
                assert_eq!(f.mul(&a, &ia), one);
                // a^(q-1) = 1 (Lagrange) and the Frobenius is additive and multiplicative
                assert_eq!(f.pow(&a, &f.order), one);
            }
            let fr = |x: &El| f.frobenius(x, 1);
            assert_eq!(fr(&f.add(&a, &b)), f.add(&fr(&a), &fr(&b)));
            assert_eq!(fr(&f.mul(&a, &b)), f.mul(&fr(&a), &fr(&b)));
            // trace additive, norm multiplicative
            assert_eq!(f.trace(&f.add(&a, &b)), f.fp.add(f.trace(&a), f.trace(&b)));
            assert_eq!(f.norm(&f.mul(&a, &b)), f.fp.mul(f.norm(&a), f.norm(&b)));
            // the minimal polynomial vanishes at a
            let m = f.minimal_polynomial(&a);
            let mq: Vec<El> = m.iter().map(|&x| f.constant(x)).collect();
            assert!(fqpoly::eval(&f, &mq, &a).is_empty());
            // indices round-trip
            assert_eq!(f.from_index(&f.poly_index(&a)), a);
        }
    }
}

#[test]
fn discrete_logarithms_and_roots() {
    let mut r = Rng(7);
    for f in [standard(17, 3), standard(2, 10), standard(97, 4), standard(3, 6)] {
        let g = f.gen();
        assert_eq!(f.is_primitive(&g, &mut Unlimited).unwrap(), Some(true));
        for _ in 0..20 {
            let k = Nat::from_u64(r.next()).divrem(&f.order).1;
            let a = f.pow(&g, &k);
            assert_eq!(f.log(&a, &g, &mut Unlimited).unwrap(), Some(k));
        }
        // square roots: the roots of X^2 - a^2 are a and -a
        let a = r.elem(&f);
        let a2 = f.mul(&a, &a);
        let rs = fqpoly::roots(&f, &[f.neg(&a2), vec![], f.one()], &mut Unlimited).unwrap();
        assert!(rs.contains(&a) && rs.contains(&f.neg(&a)));
    }
}

#[test]
fn embeddings_are_homomorphisms() {
    let mut r = Rng(3);
    for (p, m, n) in [(17u64, 3usize, 6usize), (29, 4, 12), (2, 2, 6), (7, 3, 9), (5, 1, 4), (3, 2, 6)] {
        let k = standard(p, m);
        let f = standard(p, n);
        let img = embed::canonical(&k, &f, &mut Unlimited).unwrap().unwrap();
        let e = embed::with_image(&k, &f, img).unwrap();
        for _ in 0..50 {
            let (a, b) = (r.elem(&k), r.elem(&k));
            assert_eq!(e.apply(&k, &f, &k.add(&a, &b)), f.add(&e.apply(&k, &f, &a), &e.apply(&k, &f, &b)));
            assert_eq!(e.apply(&k, &f, &k.mul(&a, &b)), f.mul(&e.apply(&k, &f, &a), &e.apply(&k, &f, &b)));
            assert_eq!(e.project(&k, &f, &e.apply(&k, &f, &a)).unwrap(), a);
            // the projection is F_p-linear
            let (c, d) = (r.elem(&f), r.elem(&f));
            let lhs = e.project(&k, &f, &f.add(&f.scale(&c, 2), &f.scale(&d, 3))).unwrap();
            let rhs = k.add(&k.scale(&e.project(&k, &f, &c).unwrap(), 2), &k.scale(&e.project(&k, &f, &d).unwrap(), 3));
            assert_eq!(lhs, rhs);
        }
        assert_eq!(e.apply(&k, &f, &k.one()), f.one());
    }
}

#[test]
fn conway_embeddings_commute() {
    // F_2^2 -> F_2^4 -> F_2^8 is F_2^2 -> F_2^8
    let (a, b, c) = (standard(2, 2), standard(2, 4), standard(2, 8));
    let ab = embed::with_image(&a, &b, embed::canonical(&a, &b, &mut Unlimited).unwrap().unwrap()).unwrap();
    let bc = embed::with_image(&b, &c, embed::canonical(&b, &c, &mut Unlimited).unwrap().unwrap()).unwrap();
    let ac = embed::with_image(&a, &c, embed::canonical(&a, &c, &mut Unlimited).unwrap().unwrap()).unwrap();
    for x in [a.gen(), a.one(), a.add(&a.gen(), &a.one())] {
        assert_eq!(bc.apply(&b, &c, &ab.apply(&a, &b, &x)), ac.apply(&a, &c, &x));
    }
}
