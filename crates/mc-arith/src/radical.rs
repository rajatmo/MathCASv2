//! Canonical radicals (semantics 2.4): exact powers of rationals with non-integer rational exponents.
//!
//! A product of such powers is reduced to `c * (-1)^u * m1^f1 * ... * mk^fk` (decision B.8):
//! - `c` is exact: a rational times 1, `i`, -1 or `-i`;
//! - `(-1)^u` with `u` in (0, 1/2), from negative bases, since `(-k)^f = (-1)^f k^f` for k > 0 holds for the
//!   principal branch. Any other exponent is reduced by `(-1)^(u + 1/2) = i (-1)^u`, so `(-1)^(2/3)` is
//!   `i (-1)^(1/6)` and `-(-1)^(1/3)` has only one form, which keeps like terms collectable in sums;
//! - the exponents `fi` are distinct, in (0, 1), and each `mi > 1` is the product of the prime factors whose total
//!   exponent has fractional part `fi`. So `pow(8, 1/2)` is `2 * pow(2, 1/2)`, `pow(12, 1/3)` is
//!   `pow(2, 2/3) * pow(3, 1/3)` and `pow(2, 1/2) * pow(3, 1/2)` is `pow(6, 1/2)`.
//!
//! The prime factors come from trial division by the primes below [`TRIAL_BOUND`]. The cofactors left over are
//! refined into pairwise coprime atoms by gcds and reduced when they are perfect powers, then treated as primes. The
//! form is unique when every prime factor is below the bound, or appears only inside one atom (a known gap: a large
//! prime appearing in two atoms with different multiplicities is not separated).
use crate::number::{Num, NumError};
use crate::num::gcd::{div_exact, gcd, is_one};
use crate::num::{Int, Nat, Rat};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Trial division by the primes below this bound.
pub const TRIAL_BOUND: u64 = 4096;

/// Atoms are tested for being perfect j-th powers for the primes j up to this (a known gap above it: an atom that is
/// a 67th or higher power of a number without small factors has over 800 bits).
pub const MAX_ROOT: u64 = 61;

fn small_primes() -> &'static [u64] {
    static P: OnceLock<Vec<u64>> = OnceLock::new();
    P.get_or_init(|| {
        let n = TRIAL_BOUND as usize;
        let mut composite = vec![false; n];
        let mut out = vec![];
        for i in 2..n {
            if !composite[i] {
                out.push(i as u64);
                for j in (i * i..n).step_by(i) {
                    composite[j] = true;
                }
            }
        }
        out
    })
}

/// The canonical form of a product of radicals.
#[derive(Clone, Debug)]
pub struct Radicals {
    /// The exact coefficient.
    pub coef: Num,
    /// The exponent u of a remaining factor (-1)^u.
    pub unit: Option<Rat>,
    /// The factors m^f, ordered by f.
    pub rads: Vec<(Nat, Rat)>,
}

fn rat(n: i64, d: i64) -> Rat {
    Rat::new(Int::from_i64(n), Int::from_i64(d))
}

/// Exponents accumulated per base, in a deterministic order.
#[derive(Default)]
struct Bases {
    primes: BTreeMap<Nat, Rat>,
    atoms: Vec<(Nat, Rat)>,
}

impl Bases {
    fn add_prime(&mut self, p: Nat, e: &Rat) {
        let cur = self.primes.remove(&p).unwrap_or_else(|| rat(0, 1));
        self.primes.insert(p, cur.add(e));
    }

    /// Splits k > 1 by trial division; what is left joins the atoms.
    fn add_int(&mut self, k: &Nat, e: &Rat) {
        let mut k = k.clone();
        for &p in small_primes() {
            if k.limbs.len() == 1 && k.limbs[0] < p * p {
                break;
            }
            let mut m = 0i64;
            loop {
                let (q, r) = k.divrem_u64(p);
                if r != 0 {
                    break;
                }
                k = q;
                m += 1;
            }
            if m > 0 {
                self.add_prime(Nat::from_u64(p), &e.mul(&rat(m, 1)));
            }
        }
        if is_one(&k) {
            return;
        }
        if k.limbs.len() == 1 && k.limbs[0] < TRIAL_BOUND * TRIAL_BOUND {
            // no factor below its square root
            self.add_prime(k, e);
        } else {
            self.atoms.push((k, e.clone()));
        }
    }

    /// Makes the atoms pairwise coprime: a pair with a common factor g becomes g, a/g and b/g. Every step lowers
    /// the total size of the atoms, which bounds the loop.
    fn refine(&mut self) {
        'again: loop {
            if crate::num::stopped() {
                return;
            }
            for i in 0..self.atoms.len() {
                for j in i + 1..self.atoms.len() {
                    let g = gcd(&self.atoms[i].0, &self.atoms[j].0);
                    if is_one(&g) {
                        continue;
                    }
                    let (b, eb) = self.atoms.swap_remove(j);
                    let (a, ea) = self.atoms.swap_remove(i);
                    self.atoms.push((g.clone(), ea.add(&eb)));
                    for (x, ex) in [(div_exact(&a, &g), ea), (div_exact(&b, &g), eb)] {
                        if !is_one(&x) {
                            self.atoms.push((x, ex));
                        }
                    }
                    continue 'again;
                }
            }
            return;
        }
    }

    /// Replaces atoms that are perfect powers m^j by m with j times the exponent, for prime j up to
    /// [`MAX_ROOT`]. An atom has no prime factor below the trial bound, so j is also at most bits / 12.
    fn reduce_powers(&mut self) {
        for (a, e) in &mut self.atoms {
            for &j in small_primes().iter().take_while(|&&j| j <= MAX_ROOT) {
                // bounded: every success divides the size of a by j
                while j <= a.bits() / 12 {
                    if crate::num::stopped() {
                        return;
                    }
                    let r = a.iroot(j);
                    if r.pow_u64(j) != *a {
                        break;
                    }
                    *a = r;
                    *e = e.mul(&rat(j as i64, 1));
                }
            }
        }
    }
}

/// The canonical form of `prod(b^e for (b, e) in pos) * prod((-1)^u for u in units)`, for positive rational bases.
/// Integer parts of the exponents become part of the coefficient; one larger than about `max_bits` bits is refused.
pub fn normalize(pos: &[(Rat, Rat)], units: &[Rat], max_bits: u64) -> Result<Radicals, NumError> {
    let mut coef = Rat::from_i64(1);
    let mut unit = rat(0, 1);
    for u in units {
        unit = unit.add(u);
    }
    let mut bases = Bases::default();
    for (b, e) in pos {
        debug_assert!(!b.num.neg && !b.is_zero());
        if !is_one(&b.num.mag) {
            bases.add_int(&b.num.mag, e);
        }
        if !is_one(&b.den) {
            bases.add_int(&b.den, &e.neg());
        }
    }
    bases.refine();
    bases.reduce_powers();
    let mut bits = 0u64;
    let mut by_frac: Vec<(Rat, Nat)> = vec![];
    let all = std::mem::take(&mut bases.primes).into_iter().chain(bases.atoms);
    for (base, e) in all {
        let (fl, fr) = e.floor_fract();
        if !fl.is_zero() {
            let n = fl.mag.to_u128().and_then(|v| u64::try_from(v).ok()).ok_or(NumError::TooLarge)?;
            bits = bits.saturating_add(base.bits().saturating_mul(n));
            if bits > max_bits {
                return Err(NumError::TooLarge);
            }
            let p = Rat::from_int(Int::from_nat(base.pow_u64(n)));
            coef = if fl.neg { coef.mul(&p.recip().expect("nonzero")) } else { coef.mul(&p) };
        }
        if !fr.is_zero() {
            match by_frac.iter_mut().find(|(f, _)| f.cmp_value(&fr).is_eq()) {
                Some((_, m)) => *m = m.mul(&base),
                None => by_frac.push((fr, base)),
            }
        }
    }
    by_frac.sort_by(|a, b| a.0.cmp_value(&b.0));
    // (-1)^u = i^k (-1)^v with k = floor(2u) and v = u - k/2 in [0, 1/2)
    let (k, _) = unit.mul(&rat(2, 1)).floor_fract();
    let v = unit.sub(&Rat::from_int(k.clone()).mul(&rat(1, 2)));
    let quarter = k.divrem_floor(&Nat::from_u64(4)).1.to_u128().expect("below 4");
    let mut c = Num::rat(coef);
    for _ in 0..quarter {
        c = c.mul(&Num::i());
    }
    let unit = (!v.is_zero()).then_some(v);
    let rads = by_frac.into_iter().map(|(f, m)| (m, f)).collect();
    Ok(Radicals { coef: c, unit, rads })
}

/// Splits an exact real base for [`normalize`]: a negative base contributes a sign factor (-1)^e.
pub fn split_base(b: &Rat, e: &Rat, pos: &mut Vec<(Rat, Rat)>, units: &mut Vec<Rat>) {
    if b.num.neg {
        units.push(e.clone());
        pos.push((b.neg(), e.clone()));
    } else {
        pos.push((b.clone(), e.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::number::Real;

    /// The real and imaginary parts of an exact coefficient.
    fn parts(n: &Num) -> (Rat, Rat) {
        match n {
            Num::Real(Real::Exact(q)) => (q.clone(), rat(0, 1)),
            Num::Complex(Real::Exact(a), Real::Exact(b)) => (a.clone(), b.clone()),
            _ => panic!("exact coefficient"),
        }
    }

    fn norm(items: &[(i64, i64, i64, i64)]) -> Radicals {
        let (mut pos, mut units) = (vec![], vec![]);
        for &(bn, bd, en, ed) in items {
            split_base(&rat(bn, bd), &rat(en, ed), &mut pos, &mut units);
        }
        normalize(&pos, &units, 1 << 20).unwrap()
    }

    fn rads(r: &Radicals) -> Vec<(u64, Rat)> {
        r.rads.iter().map(|(m, f)| (m.limbs[0], f.clone())).collect()
    }

    #[test]
    fn perfect_powers_are_extracted() {
        let r = norm(&[(8, 1, 1, 2)]);
        assert_eq!(parts(&r.coef), (rat(2, 1), rat(0, 1)));
        assert_eq!(rads(&r), vec![(2, rat(1, 2))]);
        let r = norm(&[(4, 1, 1, 2)]);
        assert_eq!(parts(&r.coef), (rat(2, 1), rat(0, 1)));
        assert!(r.rads.is_empty() && r.unit.is_none());
        let r = norm(&[(12, 1, 1, 3)]);
        assert_eq!(rads(&r), vec![(3, rat(1, 3)), (2, rat(2, 3))]);
        let r = norm(&[(2, 1, 1, 2), (3, 1, 1, 2)]);
        assert_eq!(rads(&r), vec![(6, rat(1, 2))]);
        // (1/2)^(1/2) = 2^(-1/2) = (1/2) 2^(1/2)
        let r = norm(&[(1, 2, 1, 2)]);
        assert_eq!(parts(&r.coef), (rat(1, 2), rat(0, 1)));
        assert_eq!(rads(&r), vec![(2, rat(1, 2))]);
    }

    #[test]
    fn negative_bases_give_units() {
        // (-1)^(1/2) = i, (-4)^(1/2) = 2i, (-8)^(1/3) = 2 (-1)^(1/3)
        assert_eq!(parts(&norm(&[(-1, 1, 1, 2)]).coef), (rat(0, 1), rat(1, 1)));
        assert_eq!(parts(&norm(&[(-4, 1, 1, 2)]).coef), (rat(0, 1), rat(2, 1)));
        let r = norm(&[(-8, 1, 1, 3)]);
        assert_eq!(parts(&r.coef), (rat(2, 1), rat(0, 1)));
        assert_eq!(r.unit, Some(rat(1, 3)));
        // (-1)^(2/3) (-1)^(2/3) = (-1)^(4/3) = -(-1)^(1/3)
        let r = norm(&[(-1, 1, 2, 3), (-1, 1, 2, 3)]);
        assert_eq!((parts(&r.coef), r.unit), ((rat(-1, 1), rat(0, 1)), Some(rat(1, 3))));
        // (-1)^(2/3) = i (-1)^(1/6), and (-1)^(-1/3) = -i (-1)^(1/6)
        let r = norm(&[(-1, 1, 2, 3)]);
        assert_eq!((parts(&r.coef), r.unit), ((rat(0, 1), rat(1, 1)), Some(rat(1, 6))));
        let r = norm(&[(-1, 1, -1, 3)]);
        assert_eq!((parts(&r.coef), r.unit), ((rat(0, 1), rat(-1, 1)), Some(rat(1, 6))));
        // (-1)^(3/2) = -i, (-1)^2 = 1
        assert_eq!(parts(&norm(&[(-1, 1, 3, 2)]).coef), (rat(0, 1), rat(-1, 1)));
        let r = norm(&[(-1, 1, 1, 1), (-1, 1, 1, 1)]);
        assert_eq!((parts(&r.coef), r.unit), ((rat(1, 1), rat(0, 1)), None));
    }

    #[test]
    fn large_cofactors_are_refined() {
        // two primes above the trial bound
        let p = Nat::from_u64(1_000_003);
        let q = Nat::from_u64(1_000_033);
        let pq = Rat::from_int(Int::from_nat(p.mul(&q)));
        let p2 = Rat::from_int(Int::from_nat(p.mul(&p).mul(&p)));
        // (p q)^(1/2) (p^3)^(1/2) = p^2 q^(1/2)
        let r = normalize(&[(pq, rat(1, 2)), (p2, rat(1, 2))], &[], 1 << 20).unwrap();
        assert_eq!(parts(&r.coef).0, Rat::from_int(Int::from_nat(p.mul(&p))));
        assert_eq!(r.rads, vec![(q, rat(1, 2))]);
        // a perfect power atom: (P^2)^(1/3) = P^(2/3)
        let big = Nat::from_u64(4_294_967_311); // a prime above 2^32
        let sq = Rat::from_int(Int::from_nat(big.mul(&big)));
        let r = normalize(&[(sq, rat(1, 3))], &[], 1 << 20).unwrap();
        assert_eq!(r.rads, vec![(big, rat(2, 3))]);
    }

    #[test]
    fn huge_integer_parts_are_refused() {
        assert_eq!(normalize(&[(rat(2, 1), rat(1 << 40, 3))], &[], 1 << 20).unwrap_err(), NumError::TooLarge);
    }
}
