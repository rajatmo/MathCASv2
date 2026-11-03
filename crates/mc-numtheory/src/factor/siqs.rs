//! The self-initialising quadratic sieve with the single large prime variation (decision J.11), for the balanced
//! composites of 30 to 100 digits that the other methods cannot split quickly (benchmark N7).
//!
//! With a multiplier k chosen by the Knuth-Schroeppel function, polynomials g(x) = ((a x + b)^2 - k n) / a are
//! sieved over |x| < M by the factor base (the primes p with (kn / p) != -1). Each a is a product of s factor-base
//! primes near (sqrt(2 k n) / M)^(1/s), and its 2^(s-1) values of b are visited in Gray-code order, so that a new
//! polynomial costs one addition per root. Values whose logarithms collect past a threshold are trial divided
//! (using the sieve roots, without big divisions); a cofactor below 64 times the largest factor-base prime is kept
//! as a partial relation and paired with another of the same large prime. Gaussian elimination over GF(2) on
//! bit-packed exponent vectors gives dependencies, and each gives X^2 = Y^2 (mod n) and a gcd.
use crate::budget::Budget;
use crate::prime::mod_u64;
use crate::sieve::small_primes;
use crate::word::{inverse_mod, jacobi, sqrt_mod};
use mc_arith::num::{Int, Nat};
use mc_eval::Res;
use std::collections::{HashMap, HashSet};

/// Factor-base size and half sieve interval by the number of decimal digits of n (tuned on the semiprimes of
/// benchmark N7).
const PARAMS: [(u32, usize, u32); 14] = [
    (20, 120, 1 << 12),
    (25, 180, 1 << 13),
    (30, 270, 1 << 13),
    (35, 420, 1 << 14),
    (40, 680, 1 << 14),
    (45, 1120, 1 << 14),
    (50, 1720, 1 << 15),
    (55, 3400, 1 << 15),
    (60, 4800, 1 << 15),
    (65, 6800, 3 << 14),
    (70, 9200, 1 << 16),
    (75, 12400, 1 << 16),
    (80, 16000, 3 << 15),
    (85, 20000, 3 << 15),
];

/// Bits below the threshold estimate that still count as candidates (sieve logarithms are rounded, and prime
/// powers are not sieved).
fn tolerance(digits: u32) -> f64 {
    if digits <= 52 {
        4.0
    } else {
        5.0
    }
}

/// Largest number SIQS is used for, in decimal digits: the dense elimination over GF(2) grows with the cube of
/// the factor base (decision J.11).
pub const MAX_DIGITS: u32 = 85;
/// Factor-base primes below this are not sieved (their logarithms are added to the threshold instead).
const SMALL: u32 = 30;
/// Relations beyond the number of columns, for dependencies.
const EXTRA: usize = 48;
/// Large primes are kept below this multiple of the largest factor-base prime.
const LARGE_MULT: u64 = 128;

/// Parameters for d digits, interpolated in the table.
fn params(d: u32) -> (usize, u32) {
    let d = d.clamp(PARAMS[0].0, PARAMS[PARAMS.len() - 1].0);
    let i = PARAMS.iter().position(|p| p.0 >= d).unwrap();
    if i == 0 || PARAMS[i].0 == d {
        return (PARAMS[i].1, PARAMS[i].2);
    }
    let (lo, hi) = (PARAMS[i - 1], PARAMS[i]);
    let f = (d - lo.0) as f64 / (hi.0 - lo.0) as f64;
    ((lo.1 as f64 + f * (hi.1 - lo.1) as f64) as usize, if f < 0.5 { lo.2 } else { hi.2 })
}

/// log2(x) for a natural number, approximately.
fn log2(x: &Nat) -> f64 {
    let b = x.bits();
    if b <= 64 {
        return (crate::prime::to_u64(x).unwrap() as f64).log2();
    }
    let top = x.shr(b - 64).limbs[0];
    (b - 64) as f64 + (top as f64).log2()
}

/// The Knuth-Schroeppel multiplier.
fn multiplier(n: &Nat) -> u64 {
    const KS: [u64; 30] = [
        1, 3, 5, 7, 11, 13, 15, 17, 19, 21, 23, 29, 31, 33, 35, 37, 39, 41, 43, 47, 51, 53, 55, 57, 59, 61, 65, 67, 69,
        71,
    ];
    let primes = &small_primes()[1..200];
    let ln2 = std::f64::consts::LN_2;
    let mut best = (f64::MIN, 1);
    for &k in &KS {
        let kn8 = (mod_u64(n, 8) * k) % 8;
        let mut f = -0.5 * (k as f64).ln()
            + match kn8 {
                1 => 2.0 * ln2,
                5 => ln2,
                3 | 7 => 0.5 * ln2,
                _ => 0.0,
            };
        for &p in primes {
            let p = p as u64;
            let lp = (p as f64).ln();
            if k % p == 0 {
                f += lp / p as f64;
            } else if jacobi((mod_u64(n, p) * (k % p)) % p, p) == 1 {
                f += 2.0 * lp / (p - 1) as f64;
            }
        }
        if f > best.0 {
            best = (f, k);
        }
    }
    best.1
}

/// A relation: Y^2 = (product of the factor-base primes, with the sign as index 0) (mod n), times the square of
/// `square` (paired large primes).
#[derive(Clone)]
struct Relation {
    y: Nat,
    factors: Vec<u32>,
    square: Nat,
}

/// Deterministic random numbers for polynomial choice.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

/// The factor base: index 0 is the sign, then the primes (2 first).
struct FactorBase {
    primes: Vec<u32>,
    /// sqrt(kn) mod p
    roots: Vec<u32>,
    logs: Vec<u8>,
    /// floor(2^40 / p) + 1: x mod p = x - p floor(x m / 2^40) exactly for x < 2^20
    magic: Vec<u64>,
}

fn factor_base(kn: &Nat, size: usize) -> FactorBase {
    let mut fb = FactorBase { primes: vec![1], roots: vec![0], logs: vec![0], magic: vec![] };
    let ps = small_primes();
    let mut i = 0;
    let mut extra: Vec<u32> = vec![];
    while fb.primes.len() < size + 1 {
        let p = if i < ps.len() {
            ps[i] as u64
        } else {
            // beyond 2^16: continue with odd numbers tested for primality
            let mut q = extra.last().copied().unwrap_or(ps[ps.len() - 1]) as u64 + 2;
            while !crate::word::is_prime_u64(q) {
                q += 2;
            }
            extra.push(q as u32);
            q
        };
        i += 1;
        let r = mod_u64(kn, p);
        if p == 2 {
            fb.primes.push(2);
            fb.roots.push(r as u32 & 1);
            fb.logs.push(1);
            continue;
        }
        if r == 0 {
            // p divides kn: one root (p divides k, or p divides n, which the caller excludes)
            fb.primes.push(p as u32);
            fb.roots.push(0);
            fb.logs.push((p as f64).log2().round() as u8);
            continue;
        }
        if jacobi(r, p) == 1 {
            fb.primes.push(p as u32);
            fb.roots.push(sqrt_mod(r, p).expect("a residue") as u32);
            fb.logs.push((p as f64).log2().round() as u8);
        }
    }
    fb.magic = fb.primes.iter().map(|&p| (1u64 << 40) / p as u64 + 1).collect();
    fb
}

/// A factor of the odd composite n (not a perfect power, no factor below 2^16), or `None` if the dependencies
/// gave only trivial factors.
pub fn siqs(n: &Nat, b: &mut dyn Budget) -> Res<Option<Nat>> {
    let digits = (log2(n) / std::f64::consts::LOG2_10) as u32 + 1;
    let (fb_size, m) = params(digits);
    let k = multiplier(n);
    let kn = n.mul(&Nat::from_u64(k));
    let fb = factor_base(&kn, fb_size);
    for &p in &fb.primes[2..] {
        if !k.is_multiple_of(p as u64) && mod_u64(n, p as u64) == 0 {
            return Ok(Some(Nat::from_u64(p as u64)));
        }
    }
    let nf = fb.primes.len();
    let pmax = *fb.primes.last().unwrap() as u64;
    let large_bound = pmax * LARGE_MULT;
    let len = 2 * m as usize;
    // threshold: log2 of the largest |g(x)|, about M sqrt(kn / 2), less the large prime and the unsieved primes
    let small_log: f64 =
        fb.primes[2..].iter().take_while(|&&p| p < SMALL).map(|&p| 2.0 * (p as f64).log2() / (p - 1) as f64).sum();
    let raw = (log2(&kn) / 2.0 - 0.5 + (m as f64).log2() - (large_bound as f64).log2() - small_log - tolerance(digits))
        .max(1.0);
    // logarithms are scaled so that the threshold stays below 128 (the sieve starts at 128 - threshold)
    let scale = (120.0 / raw).min(1.0);
    let thresh = (raw * scale) as u8;
    let logs: Vec<u8> = fb.primes.iter().map(|&p| ((p.max(1) as f64).log2() * scale).round() as u8).collect();
    let target = log2(&kn) / 2.0 + 0.5 - (m as f64).log2();
    let mut rels: Vec<Relation> = vec![];
    let mut partials: HashMap<u64, Relation> = HashMap::new();
    let mut seen: HashSet<Nat> = HashSet::new();
    let mut used_a: HashSet<Vec<usize>> = HashSet::new();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ n.limbs[0]);
    let mut sieve = vec![0u8; len];
    let need = nf + EXTRA;
    let mut tries = 0;
    while rels.len() < need {
        tries += 1;
        if tries > 1_000_000 {
            return Ok(None);
        }
        let Some((qs, a)) = choose_a(&fb, target, &mut rng, &mut used_a) else { return Ok(None) };
        let s = qs.len();
        // B_l = (a / q_l) * (t_l (a / q_l)^(-1) mod q_l), with the smaller of +-t_l
        let mut bl: Vec<Nat> = vec![];
        for &qi in &qs {
            let q = fb.primes[qi] as u64;
            let aq = a.divrem_u64(q).0;
            let inv = inverse_mod(mod_u64(&aq, q), q).expect("coprime");
            let mut g = (fb.roots[qi] as u64 * inv) % q;
            if g > q / 2 {
                g = q - g;
            }
            bl.push(aq.mul_u64(g));
        }
        let mut bv = Int::from_nat(bl.iter().fold(Nat::zero(), |acc, x| acc.add(x)));
        // per prime: a^(-1) mod p, 2 B_l a^(-1) mod p, and the two sieve starts
        let mut ainv = vec![0u32; nf];
        let mut delta: Vec<Vec<u32>> = vec![vec![0u32; nf]; s];
        let mut st1 = vec![u32::MAX; nf];
        let mut st2 = vec![u32::MAX; nf];
        for i in 2..nf {
            let p = fb.primes[i] as u64;
            let am = mod_u64(&a, p);
            if am == 0 {
                continue;
            }
            let ai = inverse_mod(am, p).expect("p does not divide a");
            ainv[i] = ai as u32;
            for (l, bln) in bl.iter().enumerate() {
                delta[l][i] = ((2 * mod_u64(bln, p) % p) * ai % p) as u32;
            }
            let bm = int_mod(&bv, p);
            let t = fb.roots[i] as u64;
            let r1 = (ai * ((t + p - bm) % p)) % p;
            let r2 = (ai * ((2 * p - t - bm) % p)) % p;
            st1[i] = ((r1 + m as u64) % p) as u32;
            st2[i] = ((r2 + m as u64) % p) as u32;
        }
        b.charge(nf as u64 * s as u64 / 64 + 1)?;
        for poly in 0..1usize << (s - 1) {
            if poly > 0 {
                // Gray code: flip the sign of B_v
                let v = poly.trailing_zeros() as usize;
                let gray = poly ^ (poly >> 1);
                let neg = gray >> v & 1 == 1;
                let two_b = Int::from_nat(bl[v].shl(1));
                bv = if neg { bv.sub(&two_b) } else { bv.add(&two_b) };
                for i in 2..nf {
                    if st1[i] == u32::MAX {
                        continue;
                    }
                    let p = fb.primes[i];
                    let d = delta[v][i];
                    // the roots move by -a^(-1) (change of b)
                    if neg {
                        st1[i] = add_mod(st1[i], d, p);
                        st2[i] = add_mod(st2[i], d, p);
                    } else {
                        st1[i] = add_mod(st1[i], p - d, p);
                        st2[i] = add_mod(st2[i], p - d, p);
                    }
                }
            }
            // start at 128 - threshold, so that a candidate is a byte with its top bit set
            sieve.fill(128 - thresh);
            for i in 2..nf {
                let p = fb.primes[i];
                if p < SMALL || st1[i] == u32::MAX {
                    continue;
                }
                let lg = logs[i];
                let pu = p as usize;
                let mut j = st1[i] as usize;
                while j < len {
                    sieve[j] = sieve[j].wrapping_add(lg);
                    j += pu;
                }
                if st2[i] != st1[i] {
                    let mut j = st2[i] as usize;
                    while j < len {
                        sieve[j] = sieve[j].wrapping_add(lg);
                        j += pu;
                    }
                }
            }
            b.charge(len as u64 / 256 + nf as u64 / 64 + 1)?;
            // c = (b^2 - kn) / a, exactly
            let b2 = bv.mul(&bv);
            let cv = b2.sub(&Int::from_nat(kn.clone()));
            let c = Int::new(cv.neg, cv.mag.divrem(&a).0);
            // candidates: bytes with the top bit set, found eight at a time
            let hits: Vec<usize> = sieve
                .as_chunks::<8>()
                .0
                .iter()
                .enumerate()
                .filter(|(_, w)| u64::from_le_bytes(**w) & 0x8080_8080_8080_8080 != 0)
                .flat_map(|(wi, w)| w.iter().enumerate().filter(|(_, &v)| v >= 128).map(move |(j, _)| 8 * wi + j))
                .collect();
            for idx in hits {
                let x = Int::from_i64(idx as i64 - m as i64);
                // g(x) = a x^2 + 2 b x + c
                let ax = Int::from_nat(a.clone()).mul(&x);
                let g = ax.add(&bv.shl(1)).mul(&x).add(&c);
                if g.is_zero() {
                    continue;
                }
                let Some((factors, cof)) = trial(&fb, &qs, &st1, &st2, idx, &g) else { continue };
                let yv = reduce(&ax.add(&bv), n);
                if !seen.insert(yv.clone()) {
                    continue;
                }
                if cof == 1 {
                    rels.push(Relation { y: yv, factors, square: Nat::from_u64(1) });
                } else if cof < large_bound {
                    match partials.remove(&cof) {
                        Some(other) => {
                            let mut f = factors;
                            f.extend_from_slice(&other.factors);
                            let y = yv.mul(&other.y).divrem(n).1;
                            rels.push(Relation { y, factors: f, square: Nat::from_u64(cof) });
                        }
                        None => {
                            partials.insert(cof, Relation { y: yv, factors, square: Nat::from_u64(1) });
                        }
                    }
                }
            }
            if rels.len() >= need {
                break;
            }
        }
    }
    solve(n, nf, &rels, &fb, b)
}

#[inline]
fn add_mod(a: u32, b: u32, p: u32) -> u32 {
    let s = a as u64 + b as u64;
    (if s >= p as u64 { s - p as u64 } else { s }) as u32
}

/// x mod p for a signed big integer.
fn int_mod(x: &Int, p: u64) -> u64 {
    let r = mod_u64(&x.mag, p);
    if x.neg && r != 0 {
        p - r
    } else {
        r
    }
}

/// x mod n, non-negative.
fn reduce(x: &Int, n: &Nat) -> Nat {
    x.divrem_floor(n).1
}

/// Choose a = product of s factor-base primes near the target size (log2), not used before.
fn choose_a(fb: &FactorBase, target: f64, rng: &mut Rng, used: &mut HashSet<Vec<usize>>) -> Option<(Vec<usize>, Nat)> {
    let nf = fb.primes.len();
    // primes of about 2^11 when the base reaches them, and never so large that the window leaves the base
    let max_each = (fb.primes[nf - 1] as f64).log2() - 1.5;
    let s = ((target / 11.0).round() as usize).max((target / max_each).ceil() as usize).clamp(2, 20);
    let each = target / s as f64;
    // the window of indices with primes in [2^(each - 1), 2^(each + 1)], not the small primes
    let lo_p = (each - 1.0).exp2().max(SMALL as f64 + 1.0);
    let hi_p = (each + 1.0).exp2();
    let lo = fb.primes.partition_point(|&p| (p as f64) < lo_p).max(3);
    let hi = fb.primes.partition_point(|&p| (p as f64) <= hi_p).min(nf);
    if hi <= lo + s {
        return None;
    }
    for _ in 0..1000 {
        let mut qs: Vec<usize> = vec![];
        while qs.len() < s - 1 {
            let i = lo + rng.below(hi - lo);
            if !qs.contains(&i) && fb.roots[i] != 0 {
                qs.push(i);
            }
        }
        let partial: f64 = qs.iter().map(|&i| (fb.primes[i] as f64).log2()).sum();
        let want = (target - partial).exp2();
        // the factor-base prime nearest the remaining size
        let j = fb.primes.partition_point(|&p| (p as f64) < want);
        let mut best = None;
        for cand in [j.saturating_sub(1), j, j + 1] {
            if cand >= 3 && cand < nf && !qs.contains(&cand) && fb.roots[cand] != 0 && fb.primes[cand] > SMALL {
                let err = ((fb.primes[cand] as f64).log2() + partial - target).abs();
                if best.is_none_or(|(_, e)| err < e) {
                    best = Some((cand, err));
                }
            }
        }
        let Some((last, err)) = best else { continue };
        if err > 1.0 {
            continue;
        }
        qs.push(last);
        qs.sort_unstable();
        if used.insert(qs.clone()) {
            let a = qs.iter().fold(Nat::from_u64(1), |acc, &i| acc.mul_u64(fb.primes[i] as u64));
            return Some((qs, a));
        }
    }
    None
}

/// Trial division of g(x) at sieve position `idx` by the factor base: the factor indices of a g(x) (a's primes
/// included, the sign as index 0) and the cofactor, if it fits a word.
fn trial(fb: &FactorBase, qs: &[usize], st1: &[u32], st2: &[u32], idx: usize, g: &Int) -> Option<(Vec<u32>, u64)> {
    let mut factors: Vec<u32> = qs.iter().map(|&i| i as u32).collect();
    if g.neg {
        factors.push(0);
    }
    let mut v = g.mag.clone();
    let tz = v.trailing_zeros();
    if tz > 0 {
        v = v.shr(tz);
        factors.extend(std::iter::repeat_n(1u32, tz as usize));
    }
    for i in 2..fb.primes.len() {
        let p = fb.primes[i];
        let hit = if st1[i] == u32::MAX {
            // a prime of a: test directly
            mod_u64(&v, p as u64) == 0
        } else {
            let x = idx as u64;
            let r = (x - ((x * fb.magic[i]) >> 40) * p as u64) as u32;
            r == st1[i] || r == st2[i]
        };
        if !hit {
            continue;
        }
        loop {
            let (q, r) = v.divrem_u64(p as u64);
            if r != 0 {
                break;
            }
            v = q;
            factors.push(i as u32);
        }
    }
    crate::prime::to_u64(&v).map(|c| (factors, c))
}

/// Dependencies by Gaussian elimination over GF(2), then a gcd per dependency.
fn solve(n: &Nat, nf: usize, rels: &[Relation], fb: &FactorBase, b: &mut dyn Budget) -> Res<Option<Nat>> {
    let r = rels.len();
    let cw = nf.div_ceil(64);
    let hw = r.div_ceil(64);
    let mut rows: Vec<Vec<u64>> = rels
        .iter()
        .map(|rel| {
            let mut row = vec![0u64; cw];
            for &f in &rel.factors {
                row[f as usize / 64] ^= 1 << (f % 64);
            }
            row
        })
        .collect();
    let mut hist: Vec<Vec<u64>> = (0..r)
        .map(|i| {
            let mut h = vec![0u64; hw];
            h[i / 64] |= 1 << (i % 64);
            h
        })
        .collect();
    let mut pivot = vec![false; r];
    for col in 0..nf {
        let (w, bit) = (col / 64, 1u64 << (col % 64));
        let Some(p) = (0..r).find(|&i| !pivot[i] && rows[i][w] & bit != 0) else { continue };
        pivot[p] = true;
        let (prow, phist) = (rows[p].clone(), hist[p].clone());
        for i in 0..r {
            if !pivot[i] && rows[i][w] & bit != 0 {
                for (x, y) in rows[i].iter_mut().zip(&prow) {
                    *x ^= y;
                }
                for (x, y) in hist[i].iter_mut().zip(&phist) {
                    *x ^= y;
                }
            }
        }
        if col % 64 == 0 {
            b.charge((r * (cw + hw)) as u64 / 256 + 1)?;
        }
    }
    let one = Nat::from_u64(1);
    for i in 0..r {
        if pivot[i] || rows[i].iter().any(|&w| w != 0) {
            continue;
        }
        let mut x = one.clone();
        let mut counts = vec![0u32; nf];
        let mut sq = one.clone();
        for (j, rel) in rels.iter().enumerate() {
            if hist[i][j / 64] >> (j % 64) & 1 == 1 {
                x = x.mul(&rel.y).divrem(n).1;
                sq = sq.mul(&rel.square).divrem(n).1;
                for &f in &rel.factors {
                    counts[f as usize] += 1;
                }
            }
        }
        let mut y = sq;
        for (f, &c) in counts.iter().enumerate().skip(1) {
            debug_assert!(c % 2 == 0);
            if c > 0 {
                let p = Nat::from_u64(fb.primes[f] as u64);
                y = y.mul(&p.pow_u64(c as u64 / 2)).divrem(n).1;
            }
        }
        b.charge(1)?;
        let d = if x >= y { x.sub(&y) } else { y.sub(&x) };
        let g = crate::gcd::gcd(&d, n, b)?;
        if g != one && g != *n {
            return Ok(Some(g));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::Unlimited;

    #[test]
    fn splits_semiprimes() {
        let np = |s: &str| crate::prime::next_prime(&Nat::from_decimal(s).unwrap(), &mut Unlimited).unwrap();
        for (p, q) in [
            ("1000000000", "1000000008"),
            ("100000000000", "1000000000000"),
            ("200000000000000000000", "500000000000000000000000"),
            ("20000000000000000000000", "5000000000000000000000000"),
        ] {
            let (p, q) = (np(p), np(q));
            let n = p.mul(&q);
            let f = siqs(&n, &mut Unlimited).unwrap().expect("a factor");
            assert!(f == p || f == q, "{}", f.to_decimal());
        }
    }
}
