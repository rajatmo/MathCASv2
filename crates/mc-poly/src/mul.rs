//! Polynomial multiplication: three algorithms chosen by shape (decision K.3).
//!
//! - **Kronecker substitution** for dense products: the exponent vectors become indices in a mixed radix (one digit
//!   per variable, radix one more than the result's degree in it, so no digit overflows), the coefficients become
//!   the 64-bit-aligned slots of one big integer per operand, and a single big-integer product (`mc-arith`'s
//!   three-prime NTT for large operands) computes every coefficient at once. Signed coefficients use the balanced
//!   representation: each operand is `P - N` with `P` and `N` holding the positive and negative coefficients, and
//!   the product's slots are read back as balanced digits (a slot at or above half the radix is negative and
//!   borrows one from the next slot). This is the univariate dense multiplication of P11.
//! - **Heap multiplication** (Monagan and Pearce) for sparse products: exponent vectors are packed into one 64-bit
//!   word (bit fields sized for the result's degrees, variable 0 most significant, so integer order is the
//!   lexicographic order and a monomial product is one addition). A binary heap of at most `min(n, m)` entries
//!   merges the rows `a_i * b` in decreasing order, so the terms of the product come out sorted and like terms
//!   meet at the top of the heap; nothing of size `n m` is ever stored. Word-sized coefficients accumulate in
//!   128 bits (spilling to a big integer only on overflow).
//! - **Hashing** of full exponent vectors when the result's degrees do not fit in one word (rare: very many
//!   variables or huge degrees).
//!
//! The choice compares cost estimates (operation counts weighted by measured constants) and a memory cap on the
//! Kronecker integers.
use crate::poly::Poly;
use crate::work::{Halt, Meter, WorkResult};
use crate::z::Z;
use mc_arith::num::{Int, Nat};
use rustc_hash::FxHashMap;
use std::collections::binary_heap::PeekMut;
use std::collections::BinaryHeap;

/// The largest Kronecker operand, in limbs (2^24 limbs = 128 MB; the three-prime NTT of the product needs about
/// ten times that).
const KRON_MAX_LIMBS: u64 = 1 << 24;

/// The largest product attempted, in estimated bytes of terms (the process cap is 4 GB, and operands and
/// temporaries share it; a failure beats an abort).
pub(crate) const MAX_BYTES: u64 = 1 << 30;

/// The product of two polynomials in the same ring.
pub fn mul(a: &Poly, b: &Poly, m: &mut Meter) -> WorkResult<Poly> {
    debug_assert_eq!(a.nv, b.nv);
    let nv = a.nv;
    if a.is_zero() || b.is_zero() {
        return Ok(Poly::zero(nv));
    }
    if a.len() == 1 {
        return Ok(b.mul_term(a.exps(0), a.coef(0)));
    }
    if b.len() == 1 {
        return Ok(a.mul_term(b.exps(0), b.coef(0)));
    }
    let (da, db) = (a.degrees(), b.degrees());
    let d: Vec<u64> = da.iter().zip(&db).map(|(&x, &y)| x as u64 + y as u64).collect();
    // the memory of the result is checked as its terms are produced (the Kronecker integers are capped)
    let term_bytes = nv as u64 * 4 + (a.max_bits() + b.max_bits()).div_ceil(8) + 24;
    let max_terms = MAX_BYTES / term_bytes;
    let plan = Plan::new(a, b, &d);
    match plan {
        Plan::Kronecker { radix, slot } => kronecker(a, b, &radix, slot, m),
        Plan::Heap { bits } => heap(a, b, &bits, max_terms, m),
        Plan::Hash => hashed(a, b, max_terms, m),
    }
}

/// The algorithm for one product.
enum Plan {
    Kronecker { radix: Vec<u64>, slot: usize },
    Heap { bits: Vec<u32> },
    Hash,
}

fn bit_len(x: u64) -> u32 {
    64 - x.leading_zeros()
}

impl Plan {
    fn new(a: &Poly, b: &Poly, d: &[u64]) -> Plan {
        let (na, nb) = (a.len() as f64, b.len() as f64);
        let (ba, bb) = (a.max_bits(), b.max_bits());
        // a product coefficient is a sum of at most min(na, nb) products
        let cbits = ba + bb + bit_len(a.len().min(b.len()) as u64) as u64 + 1;
        let slot = cbits.div_ceil(64) as usize;
        let radix: Vec<u64> = d.iter().map(|&x| x + 1).collect();
        let span = radix.iter().try_fold(1u64, |acc, &r| acc.checked_mul(r));
        let bits: Vec<u32> = d.iter().map(|&x| bit_len(x)).collect();
        let packs = bits.iter().map(|&b| b as u64).sum::<u64>() <= 64;
        // word-sized coefficients multiply in a few cycles; big ones cost about one limb product per limb pair
        let wa = ba.div_ceil(64).max(1) as f64;
        let wb = bb.div_ceil(64).max(1) as f64;
        let heap_cost = na * nb * ((na.min(nb)).log2() + 2.0) * if wa * wb > 1.0 { 4.0 * wa * wb } else { 1.0 };
        if let Some(span) = span {
            let limbs = span.saturating_mul(slot as u64);
            if limbs <= KRON_MAX_LIMBS {
                let l = limbs as f64;
                // two operand transforms and one inverse over three primes, plus packing; the weights against the
                // heap's are measured (P1 and P2: about 2.6 ns per heap unit, 7 ns per limb unit here)
                let kron_cost = l * (l.log2() + 4.0) * 2.8;
                if kron_cost < heap_cost || !packs {
                    return Plan::Kronecker { radix, slot };
                }
            }
        }
        if packs {
            Plan::Heap { bits }
        } else {
            Plan::Hash
        }
    }
}

// ---------------------------------------------------------------------------------------------- Kronecker

/// The mixed-radix index of an exponent vector.
fn index(e: &[u32], radix: &[u64]) -> u64 {
    e.iter().zip(radix).fold(0u64, |acc, (&x, &r)| acc * r + x as u64)
}

/// The operand as a signed big integer: `sum c_i 2^(64 slot idx_i)`.
fn pack_int(p: &Poly, radix: &[u64], slot: usize, m: &mut Meter) -> WorkResult<Int> {
    let top = index(p.exps(0), radix) as usize;
    let len = (top + 1) * slot;
    let mut pos = vec![0u64; len];
    let mut neg: Option<Vec<u64>> = None;
    for (e, c) in p.terms() {
        m.step(slot as u64)?;
        let at = index(e, radix) as usize * slot;
        let dst = if c.is_neg() { neg.get_or_insert_with(|| vec![0u64; len]) } else { &mut pos };
        match c {
            Z::S(v) => dst[at] = v.unsigned_abs(),
            Z::B(i) => dst[at..at + i.mag.limbs.len()].copy_from_slice(&i.mag.limbs),
        }
    }
    let pos = Nat::from_limbs(pos);
    Ok(match neg {
        None => Int::from_nat(pos),
        Some(n) => Int::from_nat(pos).sub(&Int::from_nat(Nat::from_limbs(n))),
    })
}

fn kronecker(a: &Poly, b: &Poly, radix: &[u64], slot: usize, m: &mut Meter) -> WorkResult<Poly> {
    let nv = a.nv;
    let fa = pack_int(a, radix, slot, m)?;
    let h = if a == b {
        fa.mul(&fa)
    } else {
        let fb = pack_int(b, radix, slot, m)?;
        fa.mul(&fb)
    };
    // the big product, charged by its size (about l log l limb operations)
    let limbs = h.mag.limbs.len() as u64;
    m.step(limbs * (64 - limbs.leading_zeros() as u64))?;
    if mc_arith::num::stopped() {
        return Err(Halt::Stopped);
    }
    let digits = h.mag.limbs;
    let nslots = digits.len().div_ceil(slot);
    // the unpacked terms, from the lowest index up
    let mut found: Vec<(u64, Z)> = Vec::new();
    let mut carry = 0u64;
    let mut buf = vec![0u64; slot];
    for s in 0..nslots {
        m.step(slot as u64)?;
        let lo = s * slot;
        let hi = (lo + slot).min(digits.len());
        buf.fill(0);
        buf[..hi - lo].copy_from_slice(&digits[lo..hi]);
        // add the borrow of the previous slot
        let mut c = carry;
        for l in buf.iter_mut() {
            if c == 0 {
                break;
            }
            let (v, o) = l.overflowing_add(c);
            *l = v;
            c = o as u64;
        }
        if c == 1 {
            // the slot was the full radix: digit zero, borrow one
            carry = 1;
            continue;
        }
        let negative = buf[slot - 1] >> 63 == 1;
        let value = if negative {
            // digit - radix = -(two's complement of the digit)
            let mut mag: Vec<u64> = buf.iter().map(|&x| !x).collect();
            for l in mag.iter_mut() {
                let (v, o) = l.overflowing_add(1);
                *l = v;
                if !o {
                    break;
                }
            }
            carry = 1;
            Z::from_int(Int::new(true, Nat::from_limbs(mag)))
        } else {
            carry = 0;
            Z::from_nat(Nat::from_limbs(buf.clone()))
        };
        if !value.is_zero() {
            found.push((s as u64, if h.neg { value.neg() } else { value }));
        }
    }
    if carry == 1 {
        // a borrow out of the top slot: the leading coefficient is 1 (its slot reads 0 after the borrow)
        found.push((nslots as u64, if h.neg { Z::S(-1) } else { Z::ONE }));
    }
    // back to exponent vectors, in decreasing order
    let mut exps = Vec::with_capacity(found.len() * nv);
    let mut coefs = Vec::with_capacity(found.len());
    let mut e = vec![0u32; nv];
    for (idx, c) in found.into_iter().rev() {
        let mut x = idx;
        for v in (0..nv).rev() {
            e[v] = (x % radix[v]) as u32;
            x /= radix[v];
        }
        exps.extend_from_slice(&e);
        coefs.push(c);
    }
    Ok(Poly::from_sorted(nv, exps, coefs))
}

// ---------------------------------------------------------------------------------------------- heap

/// Bit-field packing of exponent vectors into one word (variable 0 in the top field).
struct Packing {
    shift: Vec<u32>,
    mask: Vec<u64>,
}

impl Packing {
    fn new(bits: &[u32]) -> Packing {
        let mut shift = vec![0; bits.len()];
        let mut s = 0;
        for v in (0..bits.len()).rev() {
            shift[v] = s;
            s += bits[v];
        }
        let mask = bits.iter().map(|&b| if b == 64 { u64::MAX } else { (1u64 << b) - 1 }).collect();
        Packing { shift, mask }
    }
    fn pack(&self, e: &[u32]) -> u64 {
        e.iter().zip(&self.shift).fold(0u64, |acc, (&x, &s)| acc | (x as u64) << s)
    }
    fn unpack(&self, k: u64, out: &mut Vec<u32>) {
        for (s, m) in self.shift.iter().zip(&self.mask) {
            out.push(((k >> s) & m) as u32);
        }
    }
}

/// The coefficient sum of one output term: 128-bit while it fits, then a big integer.
struct Acc {
    small: i128,
    big: Option<Z>,
}

impl Acc {
    #[inline]
    fn add_i128(&mut self, p: i128) {
        match self.small.checked_add(p) {
            Some(s) => self.small = s,
            None => {
                let spill = Z::from_i128(self.small);
                self.big = Some(match self.big.take() {
                    None => spill,
                    Some(b) => b.add(&spill),
                });
                self.small = p;
            }
        }
    }
    fn take(&mut self) -> Z {
        let s = Z::from_i128(std::mem::take(&mut self.small));
        match self.big.take() {
            None => s,
            Some(b) => b.add(&s),
        }
    }
}

fn heap(a: &Poly, b: &Poly, bits: &[u32], max_terms: u64, m: &mut Meter) -> WorkResult<Poly> {
    // the heap holds one entry per row of the shorter operand
    let (a, b) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let pk = Packing::new(bits);
    let ak: Vec<u64> = (0..a.len()).map(|i| pk.pack(a.exps(i))).collect();
    let bk: Vec<u64> = (0..b.len()).map(|i| pk.pack(b.exps(i))).collect();
    let small = |p: &Poly| p.coefs.iter().map(Z::as_i64).collect::<Option<Vec<i64>>>();
    let (keys, coefs) = match (small(a), small(b)) {
        (Some(ac), Some(bc)) => {
            heap_merge(&ak, &bk, max_terms, m, |i, j, acc: &mut Acc| acc.add_i128(ac[i] as i128 * bc[j] as i128))?
        }
        _ => heap_merge(&ak, &bk, max_terms, m, |i, j, acc: &mut Acc| {
            let p = a.coefs[i].mul(&b.coefs[j]);
            match p {
                Z::S(v) => acc.add_i128(v as i128),
                big => acc.big = Some(acc.big.take().map_or(big.clone(), |x| x.add(&big))),
            }
        })?,
    };
    let mut exps = Vec::with_capacity(keys.len() * a.nv);
    for k in keys {
        pk.unpack(k, &mut exps);
    }
    Ok(Poly::from_sorted(a.nv, exps, coefs))
}

/// The Monagan–Pearce merge of the rows `a_i * b` (keys decreasing); `madd(i, j, acc)` adds `a_i b_j` to `acc`.
fn heap_merge(
    ak: &[u64],
    bk: &[u64],
    max_terms: u64,
    m: &mut Meter,
    mut madd: impl FnMut(usize, usize, &mut Acc),
) -> WorkResult<(Vec<u64>, Vec<Z>)> {
    let (na, nb) = (ak.len(), bk.len());
    let mut heap: BinaryHeap<(u64, u32)> = BinaryHeap::with_capacity(na);
    // the column each row has reached
    let mut col = vec![0u32; na];
    heap.push((ak[0] + bk[0], 0));
    let mut keys = vec![];
    let mut coefs = vec![];
    let mut acc = Acc { small: 0, big: None };
    let mut steps = 0u64;
    while let Some(&(k, _)) = heap.peek() {
        loop {
            let start_next = {
                let Some(mut top) = heap.peek_mut() else {
                    break;
                };
                if top.0 != k {
                    break;
                }
                let i = top.1 as usize;
                let j = col[i] as usize;
                madd(i, j, &mut acc);
                steps += 1;
                if j + 1 < nb {
                    col[i] += 1;
                    top.0 = ak[i] + bk[j + 1];
                } else {
                    PeekMut::pop(top);
                }
                // the next row starts when this row takes its first step
                (j == 0 && i + 1 < na).then_some(i + 1)
            };
            if let Some(r) = start_next {
                heap.push((ak[r] + bk[0], r as u32));
            }
        }
        let c = acc.take();
        if !c.is_zero() {
            keys.push(k);
            coefs.push(c);
        }
        if steps >= 4096 {
            if keys.len() as u64 > max_terms {
                return Err(Halt::Memory);
            }
            m.step(steps * 4)?;
            steps = 0;
        }
    }
    Ok((keys, coefs))
}

// ---------------------------------------------------------------------------------------------- hashing

fn hashed(a: &Poly, b: &Poly, max_terms: u64, m: &mut Meter) -> WorkResult<Poly> {
    let nv = a.nv;
    let mut acc: FxHashMap<Vec<u32>, Z> = FxHashMap::default();
    for (ea, ca) in a.terms() {
        m.step(b.len() as u64 * 4)?;
        if acc.len() as u64 > max_terms {
            return Err(Halt::Memory);
        }
        for (eb, cb) in b.terms() {
            let e: Vec<u32> = ea.iter().zip(eb).map(|(x, y)| x + y).collect();
            let p = ca.mul(cb);
            let slot = acc.entry(e).or_insert(Z::ZERO);
            *slot = slot.add(&p);
        }
    }
    Ok(Poly::from_terms(nv, acc.into_iter().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::Unlimited;

    /// Schoolbook product as the reference.
    fn naive(a: &Poly, b: &Poly) -> Poly {
        let mut t = vec![];
        for (ea, ca) in a.terms() {
            for (eb, cb) in b.terms() {
                t.push((ea.iter().zip(eb).map(|(x, y)| x + y).collect(), ca.mul(cb)));
            }
        }
        Poly::from_terms(a.nv, t)
    }

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.0 >> 17
        }
    }

    fn random(rng: &mut Rng, nv: usize, n: usize, deg: u64, big: bool) -> Poly {
        let terms = (0..n)
            .map(|_| {
                let e = (0..nv).map(|_| (rng.next() % (deg + 1)) as u32).collect();
                let mut c = Z::S(rng.next() as i64 % 1000 - 500);
                if big && rng.next().is_multiple_of(3) {
                    c = c.mul(&Z::S(i64::MAX)).mul(&Z::S(rng.next() as i64));
                }
                (e, c)
            })
            .collect();
        Poly::from_terms(nv, terms)
    }

    #[test]
    fn all_algorithms_agree_with_schoolbook() {
        let mut rng = Rng(42);
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        for round in 0..200 {
            let nv = 1 + round % 4;
            let big = round % 2 == 1;
            let na = 1 + (rng.next() % 30) as usize;
            let a = random(&mut rng, nv, na, 6, big);
            let nb = 1 + (rng.next() % 30) as usize;
            let b = random(&mut rng, nv, nb, 6, big);
            if a.is_zero() || b.is_zero() {
                continue;
            }
            let want = naive(&a, &b);
            let (radix, slot, bits) = params(&a, &b);
            assert_eq!(kronecker(&a, &b, &radix, slot, &mut m).unwrap(), want, "kronecker {round}");
            assert_eq!(heap(&a, &b, &bits, u64::MAX, &mut m).unwrap(), want, "heap {round}");
            assert_eq!(hashed(&a, &b, u64::MAX, &mut m).unwrap(), want, "hash {round}");
            assert_eq!(mul(&a, &b, &mut m).unwrap(), want, "dispatch {round}");
            let (radix, slot, _) = params(&a, &a);
            assert_eq!(kronecker(&a, &a, &radix, slot, &mut m).unwrap(), naive(&a, &a), "square {round}");
        }
    }

    fn params(a: &Poly, b: &Poly) -> (Vec<u64>, usize, Vec<u32>) {
        let d: Vec<u64> = a.degrees().iter().zip(b.degrees()).map(|(&x, y)| x as u64 + y as u64).collect();
        let radix = d.iter().map(|x| x + 1).collect();
        let bits = d.iter().map(|&x| bit_len(x)).collect();
        let slot = (a.max_bits() + b.max_bits() + 8).div_ceil(64) as usize;
        (radix, slot, bits)
    }

    #[test]
    fn accumulator_spills() {
        // i64::MAX^2 summed many times overflows 128 bits
        let n = 40;
        let a = Poly::from_dense(1, 0, &vec![Z::S(i64::MAX); n]);
        let b = Poly::from_dense(1, 0, &vec![Z::S(i64::MAX); n]);
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let bits = vec![bit_len(2 * n as u64)];
        assert_eq!(heap(&a, &b, &bits, u64::MAX, &mut m).unwrap(), naive(&a, &b));
    }
}
