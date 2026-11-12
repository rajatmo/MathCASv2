//! Conway polynomials: the standard defining polynomials of the finite fields, and the fallbacks when one is out
//! of reach.
//!
//! The Conway polynomial C(p, n) is the least monic primitive polynomial of degree n over F_p, in the order below,
//! that is compatible with C(p, d) for every proper divisor d of n: if a is a root of C(p, n), then
//! a^((p^n - 1)/(p^d - 1)) is a root of C(p, d). Polynomials x^n - e1 x^(n-1) + e2 x^(n-2) - ... + (-1)^n en are
//! ordered by (e1, e2, ..., en) lexicographically, each e_k read as an integer 0..p-1 (Lidl and Niederreiter,
//! Finite Fields, notes to chapter 2; Heath and Loehr, "New algorithms for generating Conway polynomials over finite
//! fields", J. Symbolic Comput. 38 (2004); Luebeck's tables, which our values agree with where they overlap).
//! Compatibility with the maximal proper divisors implies it for all of them, and for d = 1 it fixes en: the norm
//! of a root is the least primitive root modulo p.
//!
//! Two methods compute them here:
//!
//! - **Search** (n prime, or n = 1): run through the polynomials in Conway order with en fixed and test each for
//!   primitivity (x has order p^n - 1 modulo it, which also proves irreducibility).
//! - **Coset enumeration** (n composite; Heath and Loehr's idea): in an auxiliary model of F_(p^n) with a primitive
//!   element g, a = g^k is compatible exactly when k lies in a residue class modulo L = lcm(p^d - 1) over the
//!   maximal divisors d, fixed by discrete logarithms of chosen roots of the C(p, d) (one root of the first fixed,
//!   by the Galois symmetry). The (p^n - 1)/L candidates of each class are run through with one multiplication
//!   each; the primitive ones (gcd(k, p^n - 1) = 1) with the least trace are kept, and among those the least
//!   characteristic polynomial wins (later coefficients by Newton's identities when p is large, else directly).
//!
//! Values for composite n whose enumeration is long are in [`crate::conway_table`] (written by this module's
//! generator test, see NOTES.md). When no Conway polynomial is available within the budget the field takes the
//! least primitive polynomial in the same order, and when p^n - 1 cannot be factored the least irreducible one.
use crate::field::{factor_capped, Capped, El, Field, Repr};
use crate::fp::{self, Fp, Poly};
use crate::fqpoly;
use mc_arith::num::Nat;
use mc_eval::Res;
use mc_numtheory::budget::Budget;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// The candidates a coset enumeration may run through when computing at run time.
pub const RUNTIME_STEPS: u64 = 1_000_000;

/// Computed Conway polynomials (None: out of reach) by (p, n).
type Cache = Mutex<HashMap<(u64, usize), Option<Poly>>>;

static CACHE: OnceLock<Cache> = OnceLock::new();

fn cache() -> &'static Cache {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The tabulated C(p, n), if listed.
pub fn tabulated(p: u64, n: usize) -> Option<Poly> {
    crate::conway_table::TABLE.iter().find(|e| e.0 == p && e.1 as usize == n).map(|e| e.2.to_vec())
}

/// C(p, n) (monic, little-endian), from the table or computed with at most `steps` candidates per coset
/// enumeration; None when it is out of reach.
pub fn conway(p: u64, n: usize, steps: u64, b: &mut dyn Budget) -> Res<Option<Poly>> {
    if let Some(v) = cache().lock().unwrap().get(&(p, n)) {
        return Ok(v.clone());
    }
    if let Some(t) = tabulated(p, n) {
        cache().lock().unwrap().insert((p, n), Some(t.clone()));
        return Ok(Some(t));
    }
    let r = compute(p, n, steps, b)?;
    // a result that the step limit prevented is not cached for larger limits
    if r.is_some() || steps >= RUNTIME_STEPS {
        cache().lock().unwrap().insert((p, n), r.clone());
    }
    Ok(r)
}

/// The Conway order key (e1, ..., en) of a monic polynomial of degree n.
pub fn key(f: Fp, a: &[u64]) -> Vec<u64> {
    let n = a.len() - 1;
    (1..=n).map(|k| if k % 2 == 1 { f.neg(a[n - k]) } else { a[n - k] }).collect()
}

/// The monic polynomial of the key (e1, ..., en).
pub fn from_key(f: Fp, e: &[u64]) -> Poly {
    let n = e.len();
    let mut a = vec![0u64; n + 1];
    a[n] = 1;
    for k in 1..=n {
        a[n - k] = if k % 2 == 1 { f.neg(e[k - 1]) } else { e[k - 1] };
    }
    a
}

fn compute(p: u64, n: usize, steps: u64, b: &mut dyn Budget) -> Res<Option<Poly>> {
    let f = Fp::new(p);
    let mut cb = Capped::new(b, 50_000_000 + steps / 4096 * (n * n) as u64 * 8);
    let r = compute_in(f, n, steps, &mut cb);
    Ok(cb.settle(r)?.flatten())
}

fn compute_in(f: Fp, n: usize, steps: u64, b: &mut dyn Budget) -> Res<Option<Poly>> {
    let p = f.p;
    let g = match mc_numtheory::modular::primitive_root(&Nat::from_u64(p), b)? {
        Some(g) => f.of_nat(&g),
        None => return Ok(None),
    };
    if n == 1 {
        return Ok(Some(vec![f.neg(g), 1]));
    }
    let rs = fp::prime_divisors(n as u64);
    if rs.len() == 1 && rs[0] == n as u64 {
        let big_n = Nat::pow(p, n as u64).sub_u64(1);
        let Some(fs) = factor_capped(&big_n, b)? else { return Ok(None) };
        let primes: Vec<Nat> = fs.iter().map(|x| x.0.clone()).collect();
        // n prime: the search with en = g
        return search(f, n, Some(g), &|a: &[u64], b: &mut dyn Budget| is_primitive_poly(f, a, &big_n, &primes, b), b);
    }
    // composite n: the cheaper of the search with compatibility tests and the coset enumeration
    let Some((per, lam)) = enumeration_size(p, n, u64::MAX, b)? else { return Ok(None) };
    let ds: Vec<usize> = rs.iter().map(|&r| n / r as usize).collect();
    let classes: f64 = ds.iter().skip(1).map(|&d| d as f64).product();
    let set = per as f64 * classes;
    let nf = n as f64;
    let tries = nf * (p as f64).powf(nf - 1.0) / (set * 0.5);
    let cost_search = tries * nf * nf * nf * (p as f64).log2().max(1.0);
    let cost_enum = set * 2.0 * nf * nf + if p as usize <= n { set / p as f64 * nf.powi(4) } else { 0.0 };
    if cost_search.min(cost_enum) > steps as f64 * 100.0 {
        return Ok(None);
    }
    let big_n = Nat::pow(p, n as u64).sub_u64(1);
    let Some(fs) = factor_capped(&big_n, b)? else { return Ok(None) };
    let primes: Vec<Nat> = fs.iter().map(|x| x.0.clone()).collect();
    // the subfield polynomials (recursively)
    let mut subs = vec![];
    for &d in &ds {
        match conway(p, d, steps, b)? {
            Some(c) => subs.push(c),
            None => return Ok(None),
        }
    }
    if cost_search < cost_enum {
        let exps: Vec<Nat> = ds.iter().map(|&d| big_n.div(&Nat::pow(p, d as u64).sub_u64(1))).collect();
        let test = |a: &[u64], b: &mut dyn Budget| -> Res<bool> {
            let x = vec![0u64, 1];
            let w = (a.len() * a.len()) as u64 / 8 + 1;
            for (c, e) in subs.iter().zip(&exps) {
                b.charge(w * e.bits().max(1) / 64 + 1)?;
                let beta = fp::powmod(f, &x, e, a);
                // c(beta) mod a by Horner
                let v = c.iter().rev().fold(vec![], |acc: Poly, &k| fp::add(f, &fp::mulmod(f, &acc, &beta, a), &[k]));
                if !fp::rem(f, &v, a).is_empty() {
                    return Ok(false);
                }
            }
            is_primitive_poly(f, a, &big_n, &primes, b)
        };
        return search(f, n, Some(g), &test, b);
    }
    enumerate(f, n, &big_n, &primes, &ds, &subs, per, &lam, b)
}

/// Whether a is irreducible and x has order exactly N = p^n - 1 modulo it (a is primitive).
pub fn is_primitive_poly(f: Fp, a: &[u64], big_n: &Nat, primes: &[Nat], b: &mut dyn Budget) -> Res<bool> {
    let one = vec![1u64];
    let x = vec![0u64, 1];
    // irreducible first (cheap on most reducible polynomials); then x^N = 1 holds
    if !fp::is_irreducible(f, a) {
        return Ok(false);
    }
    // one power of x per prime of N: charged each (a power takes about log2(N) polynomial products)
    let w = (a.len() * a.len()) as u64 / 8 + 1;
    for r in primes {
        b.charge(w * big_n.bits().max(1) / 64 + 1)?;
        if fp::powmod(f, &x, &big_n.div(r), a) == one {
            return Ok(false);
        }
    }
    Ok(true)
}

/// A candidate test of the search: it may charge the budget.
pub type Test<'a> = dyn Fn(&[u64], &mut dyn Budget) -> Res<bool> + 'a;

/// The least polynomial in Conway order (en fixed when given, else running over 1..p-1 as the last key) that passes
/// `test`; None past 10^7 candidates.
pub fn search(f: Fp, n: usize, last: Option<u64>, test: &Test, b: &mut dyn Budget) -> Res<Option<Poly>> {
    let free = if last.is_some() { n - 1 } else { n };
    let mut e = vec![0u64; n];
    if let Some(g) = last {
        e[n - 1] = g;
    }
    let mut tried = 0u64;
    loop {
        // e_n = 0 gives the root 0: never irreducible for n >= 1 (except x itself, which is not primitive)
        if e[n - 1] != 0 {
            let a = from_key(f, &e);
            if test(&a, b)? {
                return Ok(Some(a));
            }
        }
        tried += 1;
        // every candidate is charged: one test of a high-degree candidate takes milliseconds, so charging only every
        // 64th let finite_field(5, 300) run 5 s past a 2 s limit (X26-14)
        b.charge((n * n) as u64 / 32 + 1)?;
        if tried > 10_000_000 {
            return Ok(None);
        }
        // next key: odometer over the free positions, the last free one fastest
        let mut i = free;
        loop {
            if i == 0 {
                return Ok(None);
            }
            i -= 1;
            e[i] += 1;
            if e[i] < f.p {
                break;
            }
            e[i] = 0;
        }
    }
}

/// The least primitive polynomial of degree n in Conway order (no compatibility), or the least irreducible one when
/// p^n - 1 cannot be factored.
pub fn fallback(p: u64, n: usize, b: &mut dyn Budget) -> Res<Option<Poly>> {
    let f = Fp::new(p);
    let big_n = Nat::pow(p, n as u64).sub_u64(1);
    match factor_capped(&big_n, b)? {
        Some(fs) => {
            let primes: Vec<Nat> = fs.iter().map(|x| x.0.clone()).collect();
            search(f, n, None, &|a: &[u64], b: &mut dyn Budget| is_primitive_poly(f, a, &big_n, &primes, b), b)
        }
        None => search(f, n, None, &|a: &[u64], _: &mut dyn Budget| Ok(fp::is_irreducible(f, a)), b),
    }
}

/// The defining polynomial `finite_field(p, n)` takes: C(p, n) when available, else [`fallback`]; and whether it
/// is the Conway polynomial.
pub fn defining(p: u64, n: usize, b: &mut dyn Budget) -> Res<Option<(Poly, bool)>> {
    if let Some(c) = conway(p, n, RUNTIME_STEPS, b)? {
        return Ok(Some((c, true)));
    }
    Ok(fallback(p, n, b)?.map(|c| (c, false)))
}

/// The candidates per class, (p^n - 1)/L with L = lcm(p^d - 1) over the maximal divisors d, and L, when the
/// enumeration (times the number of classes) stays within `steps`.
pub fn enumeration_size(p: u64, n: usize, steps: u64, b: &mut dyn Budget) -> Res<Option<(u64, Nat)>> {
    let rs = fp::prime_divisors(n as u64);
    let ds: Vec<usize> = rs.iter().map(|&r| n / r as usize).collect();
    let mut lam = Nat::from_u64(1);
    for &d in &ds {
        let m = Nat::pow(p, d as u64).sub_u64(1);
        let g = mc_numtheory::gcd::gcd(&lam, &m, b)?;
        lam = lam.mul(&m).div(&g);
    }
    let per = Nat::pow(p, n as u64).sub_u64(1).div(&lam);
    let classes: u64 = ds.iter().skip(1).map(|&d| d as u64).product();
    let Some(per64) = per.to_u128().filter(|&v| v <= u64::MAX as u128).map(|v| v as u64) else { return Ok(None) };
    if per64.saturating_mul(classes) > steps {
        return Ok(None);
    }
    Ok(Some((per64, lam)))
}

/// Coset enumeration for composite n (see the module documentation).
#[allow(clippy::too_many_arguments)]
fn enumerate(
    f: Fp,
    n: usize,
    big_n: &Nat,
    primes: &[Nat],
    ds: &[usize],
    subs: &[Poly],
    per64: u64,
    lam: &Nat,
    b: &mut dyn Budget,
) -> Res<Option<Poly>> {
    let p = f.p;
    let mods: Vec<Nat> = ds.iter().map(|&d| Nat::pow(p, d as u64).sub_u64(1)).collect();
    // the auxiliary model of F_(p^n): the least primitive polynomial
    let Some(aux) = search(f, n, None, &|a: &[u64], b: &mut dyn Budget| is_primitive_poly(f, a, big_n, primes, b), b)? else { return Ok(None) };
    let k = Field::get(p, &aux, Repr::Polynomial);
    let gamma = k.gen();
    // for each maximal divisor: the logs of the roots of C(p, d) to the base gamma^((p^n-1)/(p^d-1)), mod p^d - 1
    let mut logs: Vec<Vec<Nat>> = vec![];
    for (i, c) in subs.iter().enumerate() {
        let lifted: fqpoly::QPoly = c.iter().map(|&x| k.constant(x)).collect();
        let roots = fqpoly::roots(&k, &lifted, b)?;
        let delta = k.pow(&gamma, &big_n.div(&mods[i]));
        let mut ls = vec![];
        for r in &roots {
            match k.log(r, &delta, b)? {
                Some(l) => ls.push(l),
                None => return Ok(None),
            }
        }
        if ls.is_empty() {
            return Ok(None);
        }
        if i == 0 {
            ls.truncate(1);
        }
        logs.push(ls);
    }
    // the residues of each prime of N for the primitivity test
    let small: Vec<u64> = match primes.iter().map(|r| r.to_u128().filter(|&v| v < 1 << 63).map(|v| v as u64)).collect() {
        Some(v) => v,
        None => return Ok(None),
    };
    let lam_res: Vec<u64> = small.iter().map(|&r| lam.divrem_u64(r).1).collect();
    let step_el = k.pow(&gamma, lam);
    let fast = DenseMul::new(&k, &step_el);
    let tv: Vec<u64> = k.trace_vector().to_vec();
    let mut best_trace = u64::MAX;
    let mut survivors: Vec<El> = vec![];
    let mut idx = vec![0usize; logs.len()];
    let mut acc = vec![0u64; 2 * n];
    let mut next = vec![0u64; n];
    loop {
        let parts: Vec<(Nat, Nat)> = idx.iter().enumerate().map(|(i, &j)| (logs[i][j].clone(), mods[i].clone())).collect();
        if let Some((k0, _)) = crate::field::crt(&parts, b)? {
            let mut res: Vec<u64> = small.iter().map(|&r| k0.divrem_u64(r).1).collect();
            let a0 = k.pow(&gamma, &k0);
            let mut a: Vec<u64> = (0..n).map(|i| a0.get(i).copied().unwrap_or(0)).collect();
            for j in 0..per64 {
                if res.iter().all(|&v| v != 0) {
                    let t = match &fast {
                        Some(_) => a.iter().zip(&tv).fold(0u64, |s, (&x, &y)| (s + x * y) % p),
                        None => k.trace(&k.reduce(&a)),
                    };
                    if t < best_trace {
                        best_trace = t;
                        survivors.clear();
                    }
                    if t == best_trace {
                        survivors.push(k.reduce(&a));
                        if survivors.len() > 4_000_000 {
                            return Ok(None);
                        }
                    }
                }
                match &fast {
                    Some(m) => {
                        m.apply(&a, &mut next, &mut acc);
                        std::mem::swap(&mut a, &mut next);
                    }
                    None => {
                        let v = k.mul(&k.reduce(&a), &step_el);
                        a = (0..n).map(|i| v.get(i).copied().unwrap_or(0)).collect();
                    }
                }
                for (v, (&l, &r)) in res.iter_mut().zip(lam_res.iter().zip(&small)) {
                    *v = (*v + l) % r;
                }
                if j % 4096 == 0 {
                    b.charge((n * n) as u64 * 4)?;
                }
            }
        }
        // next class
        let mut i = idx.len();
        loop {
            if i == 1 || i == 0 {
                // all classes done (the first divisor's root is fixed)
                return best_of(&k, f, n, survivors, b);
            }
            i -= 1;
            idx[i] += 1;
            if idx[i] < logs[i].len() {
                break;
            }
            idx[i] = 0;
        }
    }
}

/// Multiplication by a fixed element on dense coordinate vectors for p < 2^20: the products are summed in machine
/// words and reduced once per coefficient, x^(n+i) mod f precomputed.
struct DenseMul {
    p: u64,
    n: usize,
    by: Vec<u64>,
    red: Vec<Vec<u64>>,
}

impl DenseMul {
    fn new(k: &Field, by: &El) -> Option<DenseMul> {
        let (p, n) = (k.p(), k.n);
        if p >= 1 << 20 || n >= 1 << 16 {
            return None;
        }
        let dense = |e: &El| (0..n).map(|i| e.get(i).copied().unwrap_or(0)).collect::<Vec<u64>>();
        let x = k.gen();
        let mut pw = k.pow_u64(&x, n as u64);
        let mut red = vec![];
        for _ in 0..n.saturating_sub(1) {
            red.push(dense(&pw));
            pw = k.mul(&pw, &x);
        }
        Some(DenseMul { p, n, by: dense(by), red })
    }

    /// out = a by (a, out of length n; acc of length >= 2n - 1)
    fn apply(&self, a: &[u64], out: &mut [u64], acc: &mut [u64]) {
        let n = self.n;
        for v in acc.iter_mut() {
            *v = 0;
        }
        for (i, &x) in a.iter().enumerate() {
            if x != 0 {
                for (j, &y) in self.by.iter().enumerate() {
                    acc[i + j] += x * y;
                }
            }
        }
        for kk in n..2 * n - 1 {
            let c = acc[kk] % self.p;
            if c != 0 {
                for (t, &r) in self.red[kk - n].iter().enumerate() {
                    acc[t] += c * r;
                }
            }
        }
        for (o, &v) in out.iter_mut().zip(acc.iter()) {
            *o = v % self.p;
        }
    }
}

/// The least characteristic polynomial among the candidates (all with the same trace).
fn best_of(k: &Field, f: Fp, n: usize, mut cands: Vec<El>, b: &mut dyn Budget) -> Res<Option<Poly>> {
    if cands.is_empty() {
        return Ok(None);
    }
    // Newton's identities k e_k = sum_{i=1..k} (-1)^(i-1) e_(k-i) s_i, while k < p
    let mut es: Vec<Vec<u64>> = cands.iter().map(|a| vec![1, k.trace(a)]).collect();
    let mut powers: Vec<El> = cands.clone();
    let mut sums: Vec<Vec<u64>> = cands.iter().map(|a| vec![0, k.trace(a)]).collect();
    let mut level = 2;
    while cands.len() > 64 && level <= n && (level as u64) < f.p {
        for i in 0..cands.len() {
            powers[i] = k.mul(&powers[i], &cands[i]);
            sums[i].push(k.trace(&powers[i]));
            let mut v = 0u64;
            for j in 1..=level {
                let term = f.mul(es[i][level - j], sums[i][j]);
                v = if j % 2 == 1 { f.add(v, term) } else { f.sub(v, term) };
            }
            // invariant: level < p, so it is invertible
            es[i].push(f.mul(v, f.inv(level as u64 % f.p).expect("level < p")));
        }
        let best = es.iter().map(|e| e[level]).min().unwrap_or(0);
        let keep: Vec<bool> = es.iter().map(|e| e[level] == best).collect();
        let mut it = keep.iter();
        cands.retain(|_| *it.next().unwrap());
        let mut it = keep.iter();
        es.retain(|_| *it.next().unwrap());
        let mut it = keep.iter();
        powers.retain(|_| *it.next().unwrap());
        let mut it = keep.iter();
        sums.retain(|_| *it.next().unwrap());
        level += 1;
        b.charge(cands.len() as u64)?;
    }
    let mut best: Option<(Vec<u64>, Poly)> = None;
    for a in &cands {
        let m = k.minimal_polynomial(a);
        if m.len() != n + 1 {
            continue;
        }
        let kk = key(f, &m);
        if best.as_ref().is_none_or(|(bk, _)| kk < *bk) {
            best = Some((kk, m));
        }
        b.charge((n * n) as u64)?;
    }
    Ok(best.map(|x| x.1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_numtheory::budget::Unlimited;

    fn c(p: u64, n: usize) -> Poly {
        conway(p, n, 200_000_000, &mut Unlimited).unwrap().unwrap()
    }

    #[test]
    fn small_conway_polynomials() {
        // values agree with Luebeck's tables
        assert_eq!(c(5, 1), vec![3, 1]);
        assert_eq!(c(11, 1), vec![9, 1]);
        assert_eq!(c(7, 2), vec![3, 6, 1]);
        assert_eq!(c(2, 3), vec![1, 1, 0, 1]);
        assert_eq!(c(2, 4), vec![1, 1, 0, 0, 1]);
        assert_eq!(c(2, 6), vec![1, 1, 0, 1, 1, 0, 1]);
        assert_eq!(c(17, 3), vec![14, 1, 0, 1]);
        assert_eq!(c(7, 3), vec![4, 0, 6, 1]);
        assert_eq!(c(3, 5), vec![1, 2, 0, 0, 0, 1]);
        assert_eq!(c(3, 6), vec![2, 2, 1, 0, 2, 0, 1]);
        assert_eq!(c(17, 6), vec![3, 3, 10, 0, 2, 0, 1]);
        assert_eq!(c(29, 4), vec![2, 15, 2, 0, 1]);
        assert_eq!(c(1009, 3), vec![998, 3, 0, 1]);
    }

    /// Writes `src/conway_table.rs`: C(p, n) for primes p < 110 and composite n with p^n < 2^64 whose coset
    /// enumeration is longer than the run-time limit, up to `MC_FIELDS_STEPS` candidates (default 3e8). Progress goes
    /// to `$MC_STATUS_DIR/fields-conway-table.json`. Run with `cargo test --release -p mc-fields generate_table --
    /// --ignored`.
    #[test]
    #[ignore]
    fn generate_table() {
        let steps: u64 = std::env::var("MC_FIELDS_STEPS").ok().and_then(|s| s.parse().ok()).unwrap_or(300_000_000);
        let status = std::env::var("MC_STATUS_DIR").ok();
        let primes: Vec<u64> = (2..110u64).filter(|&p| mc_numtheory::word::is_prime_u64(p)).collect();
        let mut jobs = vec![];
        for &p in &primes {
            for n in 2..64usize {
                let comp = fp::prime_divisors(n as u64) != [n as u64];
                if comp && (n as f64) * (p as f64).log2() < 64.0 {
                    jobs.push((p, n));
                }
            }
        }
        let total = jobs.len();
        let mut lines = vec![];
        let started = std::time::Instant::now();
        for (i, &(p, n)) in jobs.iter().enumerate() {
            if let Some(dir) = &status {
                let msg = format!(
                    "{{\"job\": \"fields-conway-table\", \"owner\": \"mc-fields\", \"title\": \"Conway table\", \"done\": {i}, \"total\": {total}, \"unit\": \"polynomials\", \"current\": \"C({p},{n})\", \"state\": \"running\", \"message\": \"{} s\"}}",
                    started.elapsed().as_secs()
                );
                let _ = std::fs::write(format!("{dir}/fields-conway-table.json"), msg);
            }
            // computed within 50 ms at first use: not tabulated
            let t0 = std::time::Instant::now();
            let quick = compute(p, n, RUNTIME_STEPS, &mut Unlimited).unwrap();
            if quick.is_some() && t0.elapsed().as_millis() < 50 {
                cache().lock().unwrap().insert((p, n), quick);
                continue;
            }
            let t0 = std::time::Instant::now();
            let found = match quick {
                Some(c) => Some(c),
                None => compute(p, n, steps, &mut Unlimited).unwrap(),
            };
            if let Some(c) = found {
                cache().lock().unwrap().insert((p, n), Some(c.clone()));
                let cs: Vec<String> = c.iter().map(|x| x.to_string()).collect();
                lines.push(format!("    ({p}, {n}, &[{}]),", cs.join(", ")));
                eprintln!("C({p},{n}) in {:.1} s", t0.elapsed().as_secs_f64());
            } else {
                eprintln!("C({p},{n}) out of reach");
            }
        }
        let head = "//! Conway polynomials for composite degrees whose coset enumeration is too long to run at first use, computed by\n//! this crate's own algorithm (`conway::tests::generate_table`, see NOTES.md) and checked against Luebeck's tables\n//! where they overlap. Entries: (p, n, coefficients c0, c1, ..., cn = 1).\n/// The tabulated polynomials.\n#[rustfmt::skip]\npub const TABLE: &[(u64, u32, &[u64])] = &[\n";
        let body = format!("{head}{}\n];\n", lines.join("\n"));
        std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/src/conway_table.rs"), body).unwrap();
        if let Some(dir) = &status {
            let msg = format!(
                "{{\"job\": \"fields-conway-table\", \"owner\": \"mc-fields\", \"title\": \"Conway table\", \"done\": {total}, \"total\": {total}, \"unit\": \"polynomials\", \"current\": \"\", \"state\": \"done\", \"message\": \"{} entries\"}}",
                lines.len()
            );
            let _ = std::fs::write(format!("{dir}/fields-conway-table.json"), msg);
        }
    }

    #[test]
    fn key_order_round_trips() {
        let f = Fp::new(7);
        let a = vec![4, 0, 6, 1];
        assert_eq!(from_key(f, &key(f, &a)), a);
        assert_eq!(key(f, &a), vec![1, 0, 3]);
    }
}
