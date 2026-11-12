//! The library on random inputs: the modular gcd against Euclid's algorithm, factorisations over number fields
//! checked by expansion and against degrees, minimal polynomials checked by evaluation in balls, signs of real
//! algebraic numbers.
use mc_algebra::algnum::{common_field, roots_of, AlgNum};
use mc_algebra::embed;
use mc_algebra::field::NumberField;
use mc_algebra::kpoly::{self, KPoly};
use mc_algebra::qpoly::{self, of_i64, q};
use mc_poly::work::{Meter, Unlimited};

/// A 64-bit linear congruential generator (reproducible inputs without a crate).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn small(&mut self, r: i64) -> i64 {
        (self.next() % (2 * r as u64 + 1)) as i64 - r
    }
}

fn random_kpoly(k: &NumberField, rng: &mut Lcg, deg: usize) -> KPoly {
    let d = k.degree();
    let mut p: KPoly = (0..deg).map(|_| qpoly::trim((0..d).map(|_| q(rng.small(5))).collect())).collect();
    p.push(k.one());
    kpoly::trim(p)
}

#[test]
fn modular_gcd_agrees_with_euclid() {
    let mut w = Unlimited;
    let mut m = Meter::new(&mut w);
    let mut rng = Lcg(7);
    let fields = [of_i64(&[-2, 0, 1]), of_i64(&[-1, -1, 0, 1]), of_i64(&[1, 0, 0, 0, 1]), of_i64(&[-3, 0, 1, 0, 0, 1])];
    let total = 40;
    for i in 0..total {
        let k = NumberField::new(fields[i % fields.len()].clone(), None);
        let g = random_kpoly(&k, &mut rng, 1 + i % 3);
        let a = kpoly::mul(&k, &g, &random_kpoly(&k, &mut rng, 2), &mut m).unwrap();
        let b = kpoly::mul(&k, &g, &random_kpoly(&k, &mut rng, 3), &mut m).unwrap();
        let modular = kpoly::gcd(&k, &a, &b, &mut m).unwrap();
        let euclid = kpoly::gcd_euclid(&k, &a, &b, &mut m).unwrap();
        assert_eq!(modular, euclid, "case {i}");
        // g divides the gcd
        assert!(kpoly::divrem(&k, &modular, &kpoly::monic(&k, &g), &mut m).unwrap().1.is_empty(), "case {i}");
    }
}

#[test]
fn factorisations_expand_back() {
    let mut w = Unlimited;
    let mut m = Meter::new(&mut w);
    let mut rng = Lcg(11);
    let fields = [of_i64(&[-2, 0, 1]), of_i64(&[1, 1, 1]), of_i64(&[-2, 0, 0, 1])];
    for i in 0..18 {
        let k = NumberField::new(fields[i % fields.len()].clone(), None);
        // a product of two random monic factors (and a square for some)
        let f1 = random_kpoly(&k, &mut rng, 1 + i % 3);
        let f2 = random_kpoly(&k, &mut rng, 2);
        let mut p = kpoly::mul(&k, &f1, &f2, &mut m).unwrap();
        if i % 4 == 0 {
            p = kpoly::mul(&k, &p, &f1, &mut m).unwrap();
        }
        let fs = kpoly::factor(&k, &p, &mut m).unwrap().expect("Trager's method applies");
        // the expansion check is inside factor; recheck here, and that there are at least two factors
        let mut prod = vec![fs.unit.clone()];
        let mut count = 0;
        for (f, e) in &fs.factors {
            for _ in 0..*e {
                prod = kpoly::mul(&k, &prod, f, &mut m).unwrap();
                count += 1;
            }
        }
        assert_eq!(prod, p, "case {i}");
        assert!(count >= 2, "case {i}: {count} factors");
    }
    // x^8 - 1 over Q(i): four linear factors (+-1, +-i) and two quadratics (x^2 +- i)
    let k = NumberField::new(of_i64(&[1, 0, 1]), None);
    let p = kpoly::lift(&of_i64(&[-1, 0, 0, 0, 0, 0, 0, 0, 1]));
    let fs = kpoly::factor(&k, &p, &mut m).unwrap().unwrap();
    let mut degs: Vec<usize> = fs.factors.iter().map(|(f, _)| f.len() - 1).collect();
    degs.sort();
    assert_eq!(degs, vec![1, 1, 1, 1, 2, 2]);
}

#[test]
fn minimal_polynomials_vanish_in_balls() {
    let mut w = Unlimited;
    let mut m = Meter::new(&mut w);
    let mut rng = Lcg(3);
    let gens = [of_i64(&[-2, 0, 1]), of_i64(&[-3, 0, 0, 1]), of_i64(&[-1, -1, 0, 0, 0, 1]), of_i64(&[1, 0, 1])];
    let roots: Vec<Vec<AlgNum>> = gens.iter().map(|f| roots_of(f, &mut m).unwrap().unwrap()).collect();
    for i in 0..12 {
        let a = roots[i % 4][rng.next() as usize % roots[i % 4].len()].clone();
        let b = roots[(i + 1) % 4][rng.next() as usize % roots[(i + 1) % 4].len()].clone();
        let (k, es) = common_field(&[a, b], &mut m).unwrap().unwrap();
        let x = k.add(&k.mul(&es[0], &k.of(&q(rng.small(3) | 1))), &k.mul(&es[1], &es[1]));
        let num = AlgNum { k: k.clone(), a: x.clone() };
        let mp = k.minpoly(&x, &mut m).unwrap();
        let z = num.enclose(256, &mut m).unwrap().unwrap();
        assert!(embed::eval_ball(&mp, &z, 300).contains_zero(), "case {i}");
        assert_eq!(qpoly::factor(&mp, &mut m).unwrap().len(), 1, "case {i}: irreducible");
    }
}

#[test]
fn signs_of_real_numbers() {
    let mut w = Unlimited;
    let mut m = Meter::new(&mut w);
    let s2 = roots_of(&of_i64(&[-2, 0, 1]), &mut m).unwrap().unwrap()[1].clone();
    let s3 = roots_of(&of_i64(&[-3, 0, 1]), &mut m).unwrap().unwrap()[1].clone();
    let (k, es) = common_field(&[s2, s3], &mut m).unwrap().unwrap();
    let sum = k.add(&es[0], &es[1]);
    // (sqrt 2 + sqrt 3)^2 - (5 + 2 sqrt 6) = 0 exactly
    let sq = k.mul(&sum, &sum);
    let six = k.mul(&es[0], &es[1]);
    let diff = k.sub(&sq, &k.add(&k.of(&q(5)), &k.scale(&six, &q(2))));
    assert!(diff.is_empty());
    // sqrt 2 + sqrt 3 = 3.14626436994...
    let x = AlgNum { k: k.clone(), a: k.sub(&sum, &k.of(&qpoly::qq(31462643699, 10000000000))) };
    assert_eq!(x.sign(&mut m).unwrap(), Some(1));
    let y = AlgNum { k: k.clone(), a: k.sub(&sum, &k.of(&qpoly::qq(31462643700, 10000000000))) };
    assert_eq!(y.sign(&mut m).unwrap(), Some(-1));
}

#[test]
fn real_numbers_in_complex_fields_and_comparisons() {
    let mut w = Unlimited;
    let mut m = Meter::new(&mut w);
    // zeta + 1/zeta for the fifth root of unity with the most negative real part: 2 cos(144 degrees) < 0
    let rs = roots_of(&of_i64(&[1, 1, 1, 1, 1]), &mut m).unwrap().unwrap();
    let z = rs[0].clone();
    let k = z.k.clone();
    let inv = k.inv(&z.a).unwrap();
    let x = AlgNum { k: k.clone(), a: k.add(&z.a, &inv) };
    assert_eq!(x.sign(&mut m).unwrap(), Some(-1));
    // its minimal polynomial is x^2 + x - 1
    assert_eq!(x.minpoly(&mut m).unwrap(), of_i64(&[-1, 1, 1]));
    let s2 = roots_of(&of_i64(&[-2, 0, 1]), &mut m).unwrap().unwrap()[1].clone();
    let half3 = AlgNum::rational(&qpoly::qq(3, 2));
    assert_eq!(mc_algebra::algnum::compare(&s2, &half3, &mut m).unwrap(), Some(-1));
    assert_eq!(mc_algebra::algnum::equal(&s2, &s2, &mut m).unwrap(), Some(true));
    assert_eq!(mc_algebra::algnum::equal(&s2, &half3, &mut m).unwrap(), Some(false));
}
