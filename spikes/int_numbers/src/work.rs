//! Workloads. Each is written twice with the same algorithm: once through the store (every intermediate number is
//! a term, safe points between steps with the live handles as roots) and once on spike 3's bare numbers (`Int`,
//! `Rat`, `Ball` values). Both return a fingerprint computed by the same functions ([`crate::fp`]), so equal checks
//! mean equal values.
use crate::ball::{Ball, Float};
use crate::consts::{e_ratio, pi_fixed};
use crate::int::Int;
use crate::nat::Nat;
use crate::rat::Rat;
use crate::store::{Heap, IntRef, Term, View};
use crate::{fp, fp_mix, stopped, Rng};
use std::time::{Duration, Instant};

pub struct Out {
    pub phases: Vec<(&'static str, Duration)>,
    pub check: u64,
    pub notes: Vec<(&'static str, String)>,
}

fn out(phases: Vec<(&'static str, Duration)>, check: u64) -> Out {
    Out { phases, check, notes: vec![] }
}

/// Fingerprint of a sequence of fingerprints.
fn fold(v: impl IntoIterator<Item = u64>) -> u64 {
    v.into_iter().fold(0x77, fp_mix)
}

// ------------------------------------------------------------------------------------------------ N5 factorial

/// n! by a product tree, one level at a time; a safe point after each level.
pub fn n5_tree_store(h: &mut Heap, n: usize) -> Out {
    let t0 = Instant::now();
    let mut level: Vec<Term> = (1..=n as i64).map(|k| h.int(k)).collect();
    while level.len() > 1 && !stopped() {
        let next: Vec<Term> =
            level.chunks(2).map(|c| if c.len() == 2 { h.mul(c[0], c[1]) } else { c[0] }).collect();
        level = next;
        h.safe_point(&mut [&mut level], false);
    }
    let d = t0.elapsed();
    out(vec![("product", d)], h.fingerprint(level[0]))
}

pub fn n5_tree_bare(n: usize) -> Out {
    let t0 = Instant::now();
    let mut level: Vec<Nat> = (1..=n as u64).map(Nat::from_u64).collect();
    while level.len() > 1 && !stopped() {
        level = level.chunks(2).map(|c| if c.len() == 2 { c[0].mul(&c[1]) } else { c[0].clone() }).collect();
    }
    let d = t0.elapsed();
    out(vec![("product", d)], fp::int(&Int::from_nat(level.pop().unwrap())))
}

/// n! by the naive loop r = r k: n numbers of growing size are built and discarded (garbage-heavy).
pub fn n5_seq_store(h: &mut Heap, n: usize) -> Out {
    let t0 = Instant::now();
    let mut r = [h.int(1)];
    for k in 2..=n as i64 {
        if stopped() {
            break;
        }
        let kt = h.int(k);
        r[0] = h.mul(r[0], kt);
        if k % 64 == 0 {
            h.safe_point(&mut [&mut r], false);
        }
    }
    let d = t0.elapsed();
    out(vec![("loop", d)], h.fingerprint(r[0]))
}

pub fn n5_seq_bare(n: usize) -> Out {
    let t0 = Instant::now();
    let mut r = Nat::from_u64(1);
    for k in 2..=n as u64 {
        if stopped() {
            break;
        }
        r = r.mul_u64(k);
    }
    let d = t0.elapsed();
    out(vec![("loop", d)], fp::int(&Int::from_nat(r)))
}

// ------------------------------------------------------------------------------------------------ N10 harmonic

/// H(n) by binary splitting over canonical rationals (every partial sum reduced), one level at a time.
pub fn n10_split_store(h: &mut Heap, n: usize) -> Out {
    let t0 = Instant::now();
    let mut level: Vec<Term> = (1..=n as i64).map(|k| h.recip(k)).collect();
    while level.len() > 1 && !stopped() {
        let next: Vec<Term> =
            level.chunks(2).map(|c| if c.len() == 2 { h.add(c[0], c[1]) } else { c[0] }).collect();
        level = next;
        h.safe_point(&mut [&mut level], false);
    }
    let d = t0.elapsed();
    out(vec![("split", d)], h.fingerprint(level[0]))
}

pub fn n10_split_bare(n: usize) -> Out {
    let t0 = Instant::now();
    let mut level: Vec<Rat> = (1..=n as i64).map(|k| Rat::new(Int::from_i64(1), Int::from_i64(k))).collect();
    while level.len() > 1 && !stopped() {
        level = level.chunks(2).map(|c| if c.len() == 2 { c[0].add(&c[1]) } else { c[0].clone() }).collect();
    }
    let d = t0.elapsed();
    out(vec![("split", d)], fp::rat(&level[0]))
}

/// H(n) by the loop s = s + 1/k (garbage-heavy: every partial sum is a new canonical rational).
pub fn n10_seq_store(h: &mut Heap, n: usize) -> Out {
    let t0 = Instant::now();
    let mut s = [h.int(0)];
    for k in 1..=n as i64 {
        if stopped() {
            break;
        }
        let r = h.recip(k);
        s[0] = h.add(s[0], r);
        if k % 64 == 0 {
            h.safe_point(&mut [&mut s], false);
        }
    }
    let d = t0.elapsed();
    out(vec![("loop", d)], h.fingerprint(s[0]))
}

pub fn n10_seq_bare(n: usize) -> Out {
    let t0 = Instant::now();
    let mut s = Rat::from_i64(0);
    for k in 1..=n as i64 {
        if stopped() {
            break;
        }
        s = s.add(&Rat::new(Int::from_i64(1), Int::from_i64(k)));
    }
    let d = t0.elapsed();
    out(vec![("loop", d)], fp::rat(&s))
}

// ------------------------------------------------------------------------------------------------ expansion

/// (x + 2^100)^n expanded by n multiplications by (x + 2^100). Plain: every coefficient is a term and the
/// expanded sum plus(c0, times(c1, x), times(c2, power(x, 2)), ...) is built as a term after every step.
pub fn expand_plain_store(h: &mut Heap, n: usize) -> Out {
    let t0 = Instant::now();
    let a = h.int_parts(false, Nat::pow2(100).limbs);
    let one = h.int(1);
    // roots: [a, poly, c0, c1, ...]
    let mut live: Vec<Term> = vec![a, one, a, one];
    for _ in 1..n {
        if stopped() {
            break;
        }
        let a = live[0];
        let c = &live[2..];
        let mut next = Vec::with_capacity(c.len() + 1);
        next.push(h.mul(a, c[0]));
        for k in 1..c.len() {
            let ak = h.mul(a, c[k]);
            next.push(h.add(ak, c[k - 1]));
        }
        next.push(c[c.len() - 1]);
        let poly = build_poly(h, &next);
        live.truncate(1);
        live.push(poly);
        live.extend(next);
        h.safe_point(&mut [&mut live], false);
    }
    let d = t0.elapsed();
    let check = poly_check(h, live[1]);
    out(vec![("expand", d)], check)
}

/// Packed: the coefficients are bare integers during the expansion; the term is built once at the end.
pub fn expand_packed_store(h: &mut Heap, n: usize) -> Out {
    let t0 = Instant::now();
    let c = expand_bare_coeffs(n);
    let t1 = Instant::now();
    let terms: Vec<Term> = c.iter().map(|x| h.from_int(x)).collect();
    let mut poly = [build_poly(h, &terms)];
    h.safe_point(&mut [&mut poly], false);
    let t2 = Instant::now();
    out(vec![("expand", t1 - t0), ("to term", t2 - t1)], poly_check(h, poly[0]))
}

fn expand_bare_coeffs(n: usize) -> Vec<Int> {
    let a = Int::from_nat(Nat::pow2(100));
    let mut c = vec![a.clone(), Int::from_i64(1)];
    for _ in 1..n {
        if stopped() {
            break;
        }
        let mut next = Vec::with_capacity(c.len() + 1);
        next.push(a.mul(&c[0]));
        for k in 1..c.len() {
            next.push(a.mul(&c[k]).add(&c[k - 1]));
        }
        next.push(c[c.len() - 1].clone());
        c = next;
    }
    c
}

pub fn expand_bare(n: usize) -> Out {
    let t0 = Instant::now();
    let c = expand_bare_coeffs(n);
    let d = t0.elapsed();
    out(vec![("expand", d)], fold(c.iter().map(fp::int)))
}

fn build_poly(h: &mut Heap, c: &[Term]) -> Term {
    let (plus, times, power, x) = (h.sym("plus"), h.sym("times"), h.sym("power"), h.sym("x"));
    let mut args = Vec::with_capacity(c.len());
    args.push(c[0]);
    for (k, &ck) in c.iter().enumerate().skip(1) {
        let xk = if k == 1 {
            x
        } else {
            let kt = h.int(k as i64);
            h.app(power, &[x, kt])
        };
        args.push(h.app(times, &[ck, xk]));
    }
    h.app(plus, &args)
}

/// The fingerprint of the coefficients read back from the expanded term.
fn poly_check(h: &Heap, poly: Term) -> u64 {
    let View::App(_, args) = h.view(poly) else { panic!("not a sum") };
    let fps = args.iter().enumerate().map(|(k, &t)| {
        let c = if k == 0 {
            t
        } else {
            let View::App(_, ta) = h.view(t) else { panic!("not a product") };
            ta[0]
        };
        h.fingerprint(c)
    });
    fold(fps.collect::<Vec<_>>())
}

// ------------------------------------------------------------------------------------------------ balls

/// sum_{k<=n} 1/k^2 at p bits with balls: three ball terms per step through the store.
pub fn zeta_store(h: &mut Heap, n: usize, p: u64) -> Out {
    let t0 = Instant::now();
    let one = h.ball(&Ball::from_i64(1));
    let zero = h.ball(&Ball::from_i64(0));
    let mut live = [one, zero];
    for k in 1..=n as i64 {
        if stopped() {
            break;
        }
        let kk = h.ball(&Ball::from_i64(k * k));
        let t = h.ball_div(live[0], kk, p).expect("k^2 is not zero");
        live[1] = h.ball_add(live[1], t, p);
        if k % 100 == 0 {
            h.safe_point(&mut [&mut live], false);
        }
    }
    let d = t0.elapsed();
    let b = h.to_ball(live[1]);
    check_zeta(&b, n, p);
    out(vec![("sum", d)], h.fingerprint(live[1]))
}

pub fn zeta_bare(n: usize, p: u64) -> Out {
    let t0 = Instant::now();
    let one = Ball::from_i64(1);
    let mut s = Ball::from_i64(0);
    for k in 1..=n as i64 {
        if stopped() {
            break;
        }
        let t = one.div(&Ball::from_i64(k * k), p).expect("k^2 is not zero");
        s = s.add(&t, p);
    }
    let d = t0.elapsed();
    check_zeta(&s, n, p);
    out(vec![("sum", d)], fp::ball(&s))
}

/// pi^2/6 - S(n) lies in (1/(n+1), 1/n): both comparisons must be decided and true.
fn check_zeta(s: &Ball, n: usize, p: u64) {
    if stopped() {
        return;
    }
    let pi = pi_ball(p + 32);
    let z = pi.mul(&pi, p + 32).div(&Ball::from_i64(6), p + 32).unwrap();
    let d = z.sub(s, p + 32);
    let lo = Ball::from_ratio(1, n as i64 + 1, p + 32);
    let hi = Ball::from_ratio(1, n as i64, p + 32);
    assert_eq!(lo.lt(&d), Some(true), "zeta tail lower bound");
    assert_eq!(d.lt(&hi), Some(true), "zeta tail upper bound");
}

/// A ball containing pi at p bits (spike 3's construction).
pub fn pi_ball(p: u64) -> Ball {
    let bits = p + 8;
    let x: Int = pi_fixed::<Int>(bits);
    let (m, e) = Float::round(false, x.mag, -(bits as i64), p);
    Ball { mid: m, rad: crate::ball::Mag::pow2(1 - bits as i64).add(&e) }
}

/// A ball containing e at p bits: the factorial series' fraction num/den (within 10^-(d + 20) of e) divided at
/// p bits, widened by the series error.
pub fn e_ball(p: u64) -> Ball {
    let d = (p as f64 / std::f64::consts::LOG2_10).ceil() as u64;
    let (num, den) = e_ratio::<Int>(d, 20);
    let b = Ball::from_nat(num.mag).div(&Ball::from_nat(den.mag), p).unwrap();
    Ball { mid: b.mid, rad: b.rad.add(&crate::ball::Mag::pow2(-(p as i64) - 8)) }
}

pub const PI50: &str = "31415926535897932384626433832795028841971693993751";
pub const E50: &str = "27182818284590452353602874713526624977572470936999";

/// Kernel results for the constants case, computed once per process (the computation is the same with or without
/// a store; the store adds interning and reading back).
pub fn constants(digits: u64) -> &'static (Ball, Ball, Duration) {
    use std::sync::OnceLock;
    static C: OnceLock<(u64, (Ball, Ball, Duration))> = OnceLock::new();
    let c = C.get_or_init(|| {
        let p = (digits as f64 * std::f64::consts::LOG2_10).ceil() as u64 + 16;
        let t0 = Instant::now();
        let pi = pi_ball(p);
        let e = e_ball(p);
        (digits, (pi, e, t0.elapsed()))
    });
    assert_eq!(c.0, digits, "constants computed at another size");
    &c.1
}

fn check_digits(pi: &Ball, e: &Ball) {
    if stopped() {
        return;
    }
    assert_eq!(&pi.mid.to_sci(60).0[..50], PI50);
    assert_eq!(&e.mid.to_sci(60).0[..50], E50);
}

pub fn consts_store(h: &mut Heap, digits: usize) -> Out {
    let (pi, e, _) = constants(digits as u64);
    let t0 = Instant::now();
    let mut live = [h.ball(pi), h.ball(e)];
    h.safe_point(&mut [&mut live], true);
    let t1 = Instant::now();
    let (pb, eb) = (h.to_ball(live[0]), h.to_ball(live[1]));
    let t2 = Instant::now();
    // interning the same value again finds the stored term (or compares equal under Lazy)
    let again = h.ball(pi);
    let t3 = Instant::now();
    assert!(h.same(again, live[0]), "re-interned pi is not the same term");
    check_digits(&pb, &eb);
    out(vec![("intern+gc", t1 - t0), ("read back", t2 - t1), ("re-intern", t3 - t2)], fold([h.fingerprint(live[0]), h.fingerprint(live[1])]))
}

pub fn consts_bare(digits: usize) -> Out {
    let (pi, e, d) = constants(digits as u64);
    check_digits(pi, e);
    out(vec![("compute (once)", *d)], fold([fp::ball(pi), fp::ball(e)]))
}

// ------------------------------------------------------------------------------------------------ sessions

/// Session parameters: evaluations, and the digit range of the numbers built.
#[derive(Clone, Copy)]
pub struct Churn {
    pub evals: usize,
    pub lo_digits: f64,
    pub hi_digits: f64,
}

fn churn_exp(rng: &mut Rng, c: &Churn) -> u64 {
    let u = rng.unit();
    let d = 10f64.powf(c.lo_digits.log10() + (c.hi_digits.log10() - c.lo_digits.log10()) * u);
    (d * std::f64::consts::LOG2_10 / 3f64.log2()) as u64
}

fn pow_store(h: &mut Heap, base: i64, e: u64) -> Term {
    let b = h.int(base);
    let mut r = h.int(1);
    for i in (0..64 - e.leading_zeros()).rev() {
        r = h.mul(r, r);
        if (e >> i) & 1 == 1 {
            r = h.mul(r, b);
        }
    }
    r
}

fn pow_bare(base: i64, e: u64) -> Int {
    let b = Int::from_i64(base);
    let mut r = Int::from_i64(1);
    for i in (0..64 - e.leading_zeros()).rev() {
        r = r.mul(&r);
        if (e >> i) & 1 == 1 {
            r = r.mul(&b);
        }
    }
    r
}

const HIST: usize = 8;

/// LS-churn: each evaluation builds x = 3^k (10^3 to 10^5 digits, log-uniform) by squaring through the store,
/// y = x^2 + x, and keeps out(i, y mod 2^29); every 10th y is also kept in a history of the last 8.
pub fn churn_store(h: &mut Heap, c: Churn) -> Out {
    let mut rng = Rng(42);
    let t0 = Instant::now();
    let outs = h.sym("out");
    let mut results: Vec<Term> = vec![];
    let mut hist: Vec<Term> = vec![];
    for e in 0..c.evals {
        if stopped() {
            break;
        }
        let k = churn_exp(&mut rng, &c);
        let x = pow_store(h, 3, k);
        let x2 = h.mul(x, x);
        let y = h.add(x2, x);
        let low = match h.view(y) {
            View::Int(IntRef::Big { limbs, .. }) => limbs[0] & 0x1FFF_FFFF,
            View::Int(IntRef::Small(v)) => v as u64 & 0x1FFF_FFFF,
            _ => unreachable!(),
        };
        let (et, lt) = (h.int(e as i64), h.int(low as i64));
        results.push(h.app(outs, &[et, lt]));
        if e % 10 == 0 {
            hist.push(y);
            if hist.len() > HIST {
                hist.remove(0);
            }
        }
        h.safe_point(&mut [&mut results, &mut hist], false);
    }
    let d = t0.elapsed();
    let check = fold(results.iter().chain(&hist).map(|&t| h.fingerprint(t)).collect::<Vec<_>>());
    out(vec![("session", d)], check)
}

pub fn churn_bare(c: Churn) -> Out {
    let mut rng = Rng(42);
    let t0 = Instant::now();
    let mut results: Vec<(i64, i64)> = vec![];
    let mut hist: Vec<Int> = vec![];
    for e in 0..c.evals {
        if stopped() {
            break;
        }
        let k = churn_exp(&mut rng, &c);
        let x = pow_bare(3, k);
        let y = x.mul(&x).add(&x);
        let low = y.mag.limbs.first().copied().unwrap_or(0) & 0x1FFF_FFFF;
        results.push((e as i64, low as i64));
        if e % 10 == 0 {
            hist.push(y);
            if hist.len() > HIST {
                hist.remove(0);
            }
        }
    }
    let d = t0.elapsed();
    let so = fp::sym("out");
    let fps: Vec<u64> = results
        .iter()
        .map(|&(e, l)| fp::app(so, [fp::int(&Int::from_i64(e)), fp::int(&Int::from_i64(l))]))
        .chain(hist.iter().map(fp::int))
        .collect();
    out(vec![("session", d)], fold(fps))
}

/// LS-keep parameters.
#[derive(Clone, Copy)]
pub struct Keep {
    pub evals: usize,
    /// digits of the kept big numbers
    pub big_digits: f64,
    /// terms built per evaluation
    pub terms: usize,
}

const KEEP: usize = 4;

/// LS-keep: every 25th evaluation computes a new big number (3^k with `big_digits` digits) and keeps the last four;
/// every evaluation builds `terms` compound terms over small, word and 2-to-4-limb integers and rationals, and keeps
/// one of them. The kept big numbers sit above garbage, so the sliding passes move them (arena placement).
pub fn keep_store(h: &mut Heap, c: Keep) -> Out {
    let mut rng = Rng(7);
    let t0 = Instant::now();
    let (f, g, outs, list) = (h.sym("f"), h.sym("g"), h.sym("out"), h.sym("list"));
    let kbig = (c.big_digits * std::f64::consts::LOG2_10 / 3f64.log2()) as u64;
    let mut results: Vec<Term> = vec![];
    let mut big: Vec<Term> = vec![];
    for e in 0..c.evals {
        if stopped() {
            break;
        }
        if e % 25 == 0 {
            let b = pow_store(h, 3, kbig + e as u64);
            big.push(b);
            if big.len() > KEEP {
                big.remove(0);
            }
        }
        let mut ts = Vec::with_capacity(c.terms);
        for i in 0..c.terms {
            let r = rng.next_u64();
            let num = match r % 8 {
                0..=3 => h.int((r >> 8) as i64 % 1000),
                4 | 5 => h.int(((r >> 8) & ((1 << 40) - 1)) as i64),
                6 => {
                    let n = 2 + (r >> 8) as usize % 3;
                    let limbs: Vec<u64> = (0..n).map(|_| rng.next_u64() | 1).collect();
                    h.int_parts(r & 1 << 60 != 0, limbs)
                }
                _ => h.recip(2 + ((r >> 8) % 997) as i64),
            };
            let it = h.int(i as i64);
            let gt = h.app(g, &[num]);
            ts.push(h.app(f, &[it, gt]));
        }
        let all = h.app(list, &ts);
        let et = h.int(e as i64);
        results.push(h.app(outs, &[et, ts[c.terms / 2], all]));
        if results.len() > 50 {
            // older results become garbage, as a session history with a bounded length
            results.remove(0);
        }
        h.safe_point(&mut [&mut results, &mut big], false);
    }
    let d = t0.elapsed();
    let terms = fold(results.iter().map(|&t| h.fingerprint(t)).collect::<Vec<_>>());
    let check = fold(big.iter().map(|&t| h.fingerprint(t)).collect::<Vec<_>>());
    Out { phases: vec![("session", d)], check, notes: vec![("fp2", format!("{terms:016x}"))] }
}

/// The bare reference for LS-keep: the big numbers only (the compound terms have no bare counterpart; their
/// fingerprint, note `fp2`, is compared across the store configurations).
pub fn keep_bare(c: Keep) -> Out {
    let t0 = Instant::now();
    let kbig = (c.big_digits * std::f64::consts::LOG2_10 / 3f64.log2()) as u64;
    let mut big = vec![];
    for e in (0..c.evals).step_by(25) {
        big.push(pow_bare(3, kbig + e as u64));
        if big.len() > KEEP {
            big.remove(0);
        }
    }
    let d = t0.elapsed();
    out(vec![("big numbers only", d)], fold(big.iter().map(fp::int)))
}

// ------------------------------------------------------------------------------------------------ hashing hazard

/// Sampled hashing on structured input: k numbers of `limbs` limbs that differ only in one limb the sampled hash
/// does not read. Under sampled hashing they all share one hash value, so each interning compares against every
/// earlier one (quadratic); full hashing separates them.
pub fn hash_hazard_store(h: &mut Heap, k: usize, limbs: usize) -> Out {
    let t0 = Instant::now();
    let j = hazard_index(limbs);
    let mut kept = vec![];
    for i in 0..k {
        if stopped() {
            break;
        }
        let mut v = vec![0x0123_4567_89AB_CDEFu64; limbs];
        v[j] = i as u64;
        kept.push(h.int_parts(false, v));
    }
    h.safe_point(&mut [&mut kept], false);
    let d = t0.elapsed();
    out(vec![("intern", d)], fold(kept.iter().map(|&t| h.fingerprint(t)).collect::<Vec<_>>()))
}

pub fn hash_hazard_bare(k: usize, limbs: usize) -> Out {
    let t0 = Instant::now();
    let j = hazard_index(limbs);
    let mut kept = vec![];
    for i in 0..k {
        let mut v = vec![0x0123_4567_89AB_CDEFu64; limbs];
        v[j] = i as u64;
        kept.push(Int::from_nat(Nat::from_limbs(v)));
    }
    let d = t0.elapsed();
    out(vec![("build", d)], fold(kept.iter().map(fp::int)))
}

/// A limb index the sampled hash does not read (it reads 0..4, n-4..n and k (n-1) / 9 for k = 1..8).
fn hazard_index(n: usize) -> usize {
    let sampled = |i: usize| i < 4 || i >= n - 4 || (1..=8).any(|k| k * (n - 1) / 9 == i);
    (n / 2..n).find(|&i| !sampled(i)).expect("an unsampled limb")
}

// ------------------------------------------------------------------------------------------------ payload session

/// LS-big parameters.
#[derive(Clone, Copy)]
pub struct Big {
    pub evals: usize,
    /// limbs of each number
    pub limbs: usize,
    /// numbers kept (a ring)
    pub ring: usize,
    /// compound terms built per evaluation
    pub terms: usize,
}

/// LS-big: a payload-dominated session. Each evaluation creates a random number of `limbs` limbs, adds it to the
/// oldest kept one (the sum is kept, the operand dies), keeps the last `ring` sums, and builds a few compound terms.
/// The kept numbers sit above garbage, so every major collection slides them (arena placement) or only their
/// pointers (boxed).
pub fn big_store(h: &mut Heap, c: Big) -> Out {
    let mut rng = Rng(99);
    let t0 = Instant::now();
    let (f, outs) = (h.sym("f"), h.sym("out"));
    let mut ring: Vec<Term> = vec![];
    let mut results: Vec<Term> = vec![];
    for e in 0..c.evals {
        if stopped() {
            break;
        }
        let v: Vec<u64> = (0..c.limbs).map(|_| rng.next_u64() >> 2).collect();
        let x = h.int_parts(false, v);
        let s = if ring.len() == c.ring { h.add(ring.remove(0), x) } else { x };
        ring.push(s);
        let mut last = h.int(0);
        for i in 0..c.terms {
            let it = h.int((e * c.terms + i) as i64);
            last = h.app(f, &[it, last]);
        }
        let et = h.int(e as i64);
        results.push(h.app(outs, &[et, last]));
        if results.len() > 20 {
            results.remove(0);
        }
        h.safe_point(&mut [&mut ring, &mut results], false);
    }
    let d = t0.elapsed();
    let terms = fold(results.iter().map(|&t| h.fingerprint(t)).collect::<Vec<_>>());
    let check = fold(ring.iter().map(|&t| h.fingerprint(t)).collect::<Vec<_>>());
    Out { phases: vec![("session", d)], check, notes: vec![("fp2", format!("{terms:016x}"))] }
}

pub fn big_bare(c: Big) -> Out {
    let mut rng = Rng(99);
    let t0 = Instant::now();
    let mut ring: Vec<Int> = vec![];
    for _ in 0..c.evals {
        if stopped() {
            break;
        }
        let v: Vec<u64> = (0..c.limbs).map(|_| rng.next_u64() >> 2).collect();
        let x = Int::from_nat(Nat::from_limbs(v));
        let s = if ring.len() == c.ring { ring.remove(0).add(&x) } else { x };
        ring.push(s);
    }
    let d = t0.elapsed();
    out(vec![("big numbers only", d)], fold(ring.iter().map(fp::int)))
}
