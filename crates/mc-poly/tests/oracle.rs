//! Property tests of the kernels against `num-bigint` oracles: products, powers, gcds, factorisations and
//! resultants of random polynomials.
use mc_poly::work::{Meter, Unlimited};
use mc_poly::{Poly, Z};
use num_bigint::BigInt;
use num_traits::{One, Zero};
use std::collections::BTreeMap;

/// A reproducible generator.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    /// A coefficient: small, or (sometimes) a few hundred bits.
    fn coef(&mut self, big: bool) -> Z {
        let mut c = Z::S(self.below(41) as i64 - 20);
        if big && self.below(3) == 0 {
            for _ in 0..1 + self.below(4) {
                c = c.mul(&Z::S(self.next() as i64)).add(&Z::S(self.below(1000) as i64));
            }
        }
        c
    }
    fn poly(&mut self, nv: usize, terms: usize, deg: u64, big: bool) -> Poly {
        let t = (0..terms).map(|_| ((0..nv).map(|_| self.below(deg + 1) as u32).collect(), self.coef(big))).collect();
        Poly::from_terms(nv, t)
    }
}

/// The oracle form: exponents -> nonzero BigInt.
type Dense = BTreeMap<Vec<u32>, BigInt>;

fn big(z: &Z) -> BigInt {
    z.to_string().parse().expect("a decimal integer")
}

fn oracle(p: &Poly) -> Dense {
    p.terms().map(|(e, c)| (e.to_vec(), big(c))).collect()
}

fn omul(a: &Dense, b: &Dense) -> Dense {
    let mut out = Dense::new();
    for (ea, ca) in a {
        for (eb, cb) in b {
            let e: Vec<u32> = ea.iter().zip(eb).map(|(x, y)| x + y).collect();
            *out.entry(e).or_insert_with(BigInt::zero) += ca * cb;
        }
    }
    out.retain(|_, c| !c.is_zero());
    out
}

fn meter() -> Unlimited {
    Unlimited
}

#[test]
fn products_and_powers_match_the_oracle() {
    let mut rng = Rng(0x1234_5678);
    let mut w = meter();
    let mut m = Meter::new(&mut w);
    for round in 0..300 {
        let nv = 1 + round % 5;
        let big = round % 3 == 0;
        // dense univariate operands reach the Kronecker path, sparse multivariate ones the heap
        let (ta, tb) = if nv == 1 {
            (1 + rng.below(80) as usize, 1 + rng.below(80) as usize)
        } else {
            (1 + rng.below(40) as usize, 1 + rng.below(40) as usize)
        };
        let a = rng.poly(nv, ta, if nv == 1 { 60 } else { 8 }, big);
        let b = rng.poly(nv, tb, if nv == 1 { 60 } else { 8 }, big);
        let p = a.mul(&b, &mut m).unwrap();
        assert!(p.is_canonical());
        assert_eq!(oracle(&p), omul(&oracle(&a), &oracle(&b)), "round {round}");
        if round % 10 == 0 {
            let e = 1 + rng.below(4);
            let mut want: Dense = [(vec![0; nv], BigInt::one())].into_iter().collect();
            for _ in 0..e {
                want = omul(&want, &oracle(&a));
            }
            assert_eq!(oracle(&a.pow(e, &mut m).unwrap()), want, "power round {round}");
        }
    }
}

#[test]
fn gcds_divide_and_are_greatest() {
    let mut rng = Rng(0x9abc_def1);
    let mut w = meter();
    let mut m = Meter::new(&mut w);
    for round in 0..60 {
        let nv = 1 + round % 3;
        let g = {
            let t = 1 + rng.below(5) as usize;
            let d = 3;
            rng.poly(nv, t, d, round % 2 == 0)
        };
        let a = {
            let t = 1 + rng.below(5) as usize;
            let d = 3;
            rng.poly(nv, t, d, false)
        };
        let b = {
            let t = 1 + rng.below(5) as usize;
            let d = 3;
            rng.poly(nv, t, d, false)
        };
        if g.is_zero() || a.is_zero() || b.is_zero() {
            continue;
        }
        let (ga, gb) = (g.mul(&a, &mut m).unwrap(), g.mul(&b, &mut m).unwrap());
        let d = mc_poly::gcd::gcd(&ga, &gb, &mut m).unwrap();
        // the gcd divides both, and g divides the gcd
        assert!(ga.div_exact(&d, &mut m).unwrap().is_some(), "round {round}");
        assert!(gb.div_exact(&d, &mut m).unwrap().is_some(), "round {round}");
        assert!(d.div_exact(&g.primitive().1, &mut m).unwrap().is_some(), "round {round}");
        // with coprime cofactors it is g up to the content
        let ab = mc_poly::gcd::gcd(&a, &b, &mut m).unwrap();
        if ab.is_constant() && ab.as_constant() == Some(Z::ONE) && a.content().is_one() && b.content().is_one() {
            assert_eq!(d.primitive().1, g.primitive().1, "round {round}");
        }
    }
}

#[test]
fn factorisations_multiply_back() {
    let mut rng = Rng(0x5555_aaaa);
    let mut w = meter();
    let mut m = Meter::new(&mut w);
    for round in 0..40 {
        // a product of a few random univariate factors, some repeated
        let mut p = Poly::constant(1, Z::S(1 + rng.below(6) as i64));
        let mut parts = 0;
        for _ in 0..1 + rng.below(4) {
            let f = {
                let t = 2 + rng.below(4) as usize;
                let d = 1 + rng.below(6);
                rng.poly(1, t, d, false)
            };
            if f.is_constant() {
                continue;
            }
            parts += 1;
            p = p.mul(&f.pow(1 + rng.below(2), &mut m).unwrap(), &mut m).unwrap();
        }
        let f = mc_poly::factor::factor(&p, false, &mut m).unwrap();
        let mut back: Dense = [(vec![0], big(&f.unit))].into_iter().collect();
        let mut count = 0;
        for (g, e) in &f.factors {
            assert!(!g.is_constant());
            assert!(g.lc().signum() > 0);
            for _ in 0..*e {
                back = omul(&back, &oracle(g));
                count += 1;
            }
        }
        assert_eq!(back, oracle(&p), "round {round}");
        // at least as many factors as were multiplied in
        assert!(count >= parts, "round {round}");
    }
}

/// The determinant by Bareiss's fraction-free elimination.
fn det(mut a: Vec<Vec<BigInt>>) -> BigInt {
    let n = a.len();
    let mut sign = BigInt::one();
    let mut prev = BigInt::one();
    for k in 0..n {
        if a[k][k].is_zero() {
            match (k + 1..n).find(|&i| !a[i][k].is_zero()) {
                Some(i) => {
                    a.swap(i, k);
                    sign = -sign;
                }
                None => return BigInt::zero(),
            }
        }
        for i in k + 1..n {
            for j in k + 1..n {
                a[i][j] = (&a[i][j] * &a[k][k] - &a[i][k] * &a[k][j]) / &prev;
            }
        }
        prev = a[k][k].clone();
    }
    sign * &a[n - 1][n - 1]
}

/// The Sylvester matrix of two dense polynomials (index = degree).
fn sylvester(f: &[BigInt], g: &[BigInt]) -> Vec<Vec<BigInt>> {
    let (n, k) = (f.len() - 1, g.len() - 1);
    let size = n + k;
    let mut rows = vec![];
    for i in 0..k {
        let mut r = vec![BigInt::zero(); size];
        for (j, c) in f.iter().rev().enumerate() {
            r[i + j] = c.clone();
        }
        rows.push(r);
    }
    for i in 0..n {
        let mut r = vec![BigInt::zero(); size];
        for (j, c) in g.iter().rev().enumerate() {
            r[i + j] = c.clone();
        }
        rows.push(r);
    }
    rows
}

#[test]
fn resultants_are_sylvester_determinants() {
    let mut rng = Rng(0x0f0f_f0f0);
    let mut w = meter();
    let mut m = Meter::new(&mut w);
    for round in 0..40 {
        let a = {
            let t = 2 + rng.below(8) as usize;
            let d = 1 + rng.below(9);
            rng.poly(1, t, d, round % 2 == 0)
        };
        let b = {
            let t = 2 + rng.below(8) as usize;
            let d = 1 + rng.below(9);
            rng.poly(1, t, d, false)
        };
        if a.degree(0).unwrap_or(0) == 0 || b.degree(0).unwrap_or(0) == 0 {
            continue;
        }
        let dense = |p: &Poly| p.to_dense(0).iter().map(big).collect::<Vec<_>>();
        let want = det(sylvester(&dense(&a), &dense(&b)));
        let r = mc_poly::resultant::resultant(&a, &b, 0, &mut m).unwrap();
        let got = r.as_constant().map(|c| big(&c)).unwrap();
        assert_eq!(got, want, "round {round}");
        // the subresultant path (one extra variable that does not occur in b) agrees on a substitution
        let y = Poly::var(2, 1);
        let a2 = a.embed(2, &[0]).add(&y);
        let b2 = b.embed(2, &[0]);
        let r2 = mc_poly::resultant::resultant(&a2, &b2, 0, &mut m).unwrap();
        let at0 = r2.eval_var(1, &Z::ZERO).as_constant().unwrap();
        assert_eq!(big(&at0), want, "subresultant round {round}");
    }
}

#[test]
fn multivariate_factorisations_multiply_back() {
    let mut rng = Rng(0x3141_5926);
    let mut w = meter();
    let mut m = Meter::new(&mut w);
    for round in 0..30 {
        let nv = 2 + round % 2;
        let mut p = Poly::constant(nv, Z::ONE);
        let mut parts = 0;
        for _ in 0..1 + rng.below(3) {
            let t = 2 + rng.below(3) as usize;
            let f = rng.poly(nv, t, 2, false);
            if f.is_constant() {
                continue;
            }
            parts += 1;
            p = p.mul(&f, &mut m).unwrap();
        }
        let f = mc_poly::factor::factor(&p, false, &mut m).unwrap();
        let mut back: Dense = [(vec![0; nv], big(&f.unit))].into_iter().collect();
        let mut count = 0;
        for (g, e) in &f.factors {
            for _ in 0..*e {
                back = omul(&back, &oracle(g));
                count += 1;
            }
        }
        assert_eq!(back, oracle(&p), "round {round}");
        assert!(count >= parts, "round {round}: {count} factors of {parts}");
    }
}
