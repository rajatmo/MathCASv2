//! Exact sign of a univariate polynomial with rational coefficients on an interval (tier 4 of the decision service).
//!
//! The polynomial is split into its square-free parts by Yun's algorithm (`e = c ∏ s_k^k`); the parts of odd
//! multiplicity decide where `e` changes sign, those of even multiplicity only where it touches zero. The real roots
//! of each part in the interval are counted with a Sturm sequence (Sturm 1829; Basu, Pollack and Roy, *Algorithms in
//! Real Algebraic Geometry*, ch. 2): `V(a+) - V(b)` sign variations count the distinct roots in `(a, b)`, with the
//! endpoints tested exactly and infinite endpoints read from the leading coefficients. When no part of odd
//! multiplicity has a root inside, the sign is the sign at one rational sample point; when no part has a root
//! inside, it is strict. All arithmetic is exact (no root is approximated), so every answer is a proof.
use mc_arith::num::{Int, Rat};
use std::cmp::Ordering;

/// A dense polynomial over Q, the coefficient of `x^i` at index `i`, without trailing zeros (zero is empty).
pub type QP = Vec<Rat>;

fn zero() -> Rat {
    Rat::from_i64(0)
}

/// The sign of a rational (-1, 0, 1).
pub fn rsign(r: &Rat) -> i32 {
    match r.cmp_value(&zero()) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

fn trim(mut p: QP) -> QP {
    while p.last().is_some_and(|c| c.is_zero()) {
        p.pop();
    }
    p
}

fn deg(p: &QP) -> Option<usize> {
    p.len().checked_sub(1)
}

/// `p(x)` (Horner).
pub fn eval(p: &QP, x: &Rat) -> Rat {
    let mut acc = zero();
    for c in p.iter().rev() {
        acc = acc.mul(x).add(c);
    }
    acc
}

fn derivative(p: &QP) -> QP {
    trim(p.iter().enumerate().skip(1).map(|(i, c)| c.mul(&Rat::from_i64(i as i64))).collect())
}

/// Quotient and remainder of `a / b` (`b` nonzero).
fn divrem(a: &QP, b: &QP) -> (QP, QP) {
    let db = deg(b).expect("a nonzero divisor");
    let lb = b[db].recip().expect("a nonzero leading coefficient");
    let mut r = a.clone();
    let mut q = vec![zero(); a.len().saturating_sub(db).max(1)];
    while let Some(dr) = deg(&r) {
        if dr < db {
            break;
        }
        let c = r[dr].mul(&lb);
        let s = dr - db;
        for (i, bc) in b.iter().enumerate() {
            r[s + i] = r[s + i].sub(&c.mul(bc));
        }
        q[s] = c;
        r = trim(r);
    }
    (trim(q), r)
}

fn monic(p: &QP) -> QP {
    match p.last() {
        Some(l) => {
            let inv = l.recip().expect("a nonzero leading coefficient");
            p.iter().map(|c| c.mul(&inv)).collect()
        }
        None => vec![],
    }
}

/// The monic gcd.
fn gcd(a: &QP, b: &QP) -> QP {
    let (mut a, mut b) = (a.clone(), b.clone());
    while !b.is_empty() {
        let (_, r) = divrem(&a, &b);
        a = b;
        b = r;
    }
    monic(&a)
}

fn sub(a: &QP, b: &QP) -> QP {
    trim((0..a.len().max(b.len())).map(|i| a.get(i).cloned().unwrap_or_else(zero).sub(&b.get(i).cloned().unwrap_or_else(zero))).collect())
}

/// Yun's square-free decomposition of a nonconstant `p`: `(s_k, k)` with `p = lc ∏ s_k^k`, each `s_k` monic and
/// square-free, the `s_k` pairwise coprime.
pub fn squarefree(p: &QP, steps: &mut u64) -> Vec<(QP, u32)> {
    let mut out = vec![];
    let dp = derivative(p);
    let a0 = gcd(p, &dp);
    let mut b = divrem(p, &a0).0;
    let c = divrem(&dp, &a0).0;
    let mut d = sub(&c, &derivative(&b));
    let mut k = 1u32;
    while deg(&b).unwrap_or(0) > 0 && k <= 4096 {
        *steps += 1;
        let a = gcd(&b, &d);
        if deg(&a).unwrap_or(0) > 0 {
            out.push((a.clone(), k));
        }
        b = divrem(&b, &a).0;
        let c = divrem(&d, &a).0;
        d = sub(&c, &derivative(&b));
        k += 1;
    }
    out
}

/// An endpoint of an interval: a rational (included or not) or an infinity.
#[derive(Clone, Debug)]
pub enum End {
    /// minus or plus infinity
    Inf,
    /// a rational endpoint, `true` when it belongs to the interval
    At(Rat, bool),
}

/// The Sturm sequence of a square-free `p`.
fn sequence(p: &QP, steps: &mut u64) -> Vec<QP> {
    let mut seq = vec![p.clone(), derivative(p)];
    while let Some(last) = seq.last() {
        *steps += 1;
        if last.is_empty() {
            seq.pop();
            break;
        }
        let n = seq.len();
        let (_, r) = divrem(&seq[n - 2], &seq[n - 1]);
        if r.is_empty() {
            break;
        }
        seq.push(r.iter().map(|c| c.neg()).collect());
    }
    seq
}

fn variations(signs: impl Iterator<Item = i32>) -> usize {
    let mut last = 0;
    let mut v = 0;
    for s in signs {
        if s != 0 {
            if last != 0 && s != last {
                v += 1;
            }
            last = s;
        }
    }
    v
}

/// The sign of `p` at minus infinity (`lo`) or plus infinity.
fn sign_inf(p: &QP, lo: bool) -> i32 {
    let Some(d) = deg(p) else { return 0 };
    let s = rsign(&p[d]);
    if lo && d % 2 == 1 {
        -s
    } else {
        s
    }
}

/// The number of distinct real roots of the square-free `p` in the interval `(lo, hi)` with the endpoints that
/// belong to it.
fn count(p: &QP, lo: &End, hi: &End, steps: &mut u64) -> usize {
    if deg(p).unwrap_or(0) == 0 {
        return 0;
    }
    let seq = sequence(p, steps);
    // V(a+): at a root a of p the sign just right of a is the sign of p'(a), so p itself is dropped
    let (v_lo, at_lo) = match lo {
        End::Inf => (variations(seq.iter().map(|q| sign_inf(q, true))), 0),
        End::At(a, closed) => {
            let p0 = rsign(&eval(p, a));
            let v = if p0 == 0 { variations(seq[1..].iter().map(|q| rsign(&eval(q, a)))) } else { variations(seq.iter().map(|q| rsign(&eval(q, a)))) };
            (v, usize::from(p0 == 0 && *closed))
        }
    };
    let (v_hi, at_hi, open_hi_root) = match hi {
        End::Inf => (variations(seq.iter().map(|q| sign_inf(q, false))), 0, 0),
        End::At(b, closed) => {
            let p0 = rsign(&eval(p, b));
            // V(a+) - V(b) counts the roots in (a, b]
            (variations(seq.iter().map(|q| rsign(&eval(q, b)))), usize::from(p0 == 0 && *closed), usize::from(p0 == 0))
        }
    };
    (v_lo.saturating_sub(v_hi)).saturating_sub(open_hi_root) + at_lo + at_hi
}

/// A rational point of the interval (which must not be empty).
fn sample(lo: &End, hi: &End) -> Rat {
    let one = Rat::from_i64(1);
    match (lo, hi) {
        (End::Inf, End::Inf) => zero(),
        (End::At(a, _), End::Inf) => a.add(&one),
        (End::Inf, End::At(b, _)) => b.sub(&one),
        (End::At(a, _), End::At(b, _)) => a.add(b).mul(&Rat::new(Int::from_i64(1), Int::from_i64(2))),
    }
}

/// What is proved of the sign of `p` on the interval: `(sign, strict)` with `sign` -1, 0 or 1 (0 only for the zero
/// polynomial), or `None` when `p` changes sign inside.
pub fn sign_on(p: &QP, lo: &End, hi: &End, steps: &mut u64) -> Option<(i32, bool)> {
    let p = trim(p.clone());
    let Some(d) = deg(&p) else { return Some((0, false)) };
    if let (End::At(a, ca), End::At(b, cb)) = (lo, hi) {
        // an empty or one-point interval proves nothing useful
        match a.cmp_value(b) {
            Ordering::Greater => return None,
            Ordering::Equal => {
                return (*ca && *cb).then(|| {
                    let v = rsign(&eval(&p, a));
                    (v, v != 0)
                });
            }
            Ordering::Less => {}
        }
    }
    if d == 0 {
        return Some((rsign(&p[0]), true));
    }
    let parts = squarefree(&p, steps);
    let mut odd: QP = vec![Rat::from_i64(1)];
    let mut touches = false;
    let open = |e: &End| match e {
        End::Inf => End::Inf,
        End::At(a, _) => End::At(a.clone(), false),
    };
    let (olo, ohi) = (open(lo), open(hi));
    let root_at = |s: &QP, e: &End| matches!(e, End::At(a, true) if eval(s, a).is_zero());
    for (s, k) in &parts {
        // roots inside change the sign of an odd part; roots at an included endpoint only make p vanish there
        let inside = count(s, &olo, &ohi, steps);
        let ends = root_at(s, lo) || root_at(s, hi);
        if k % 2 == 1 && inside > 0 {
            return None;
        }
        if k % 2 == 1 {
            odd = mul(&odd, s);
        }
        touches |= inside > 0 || ends;
    }
    // the sign of lc * (odd part) at a sample point, where the odd part has no root
    let x = sample(lo, hi);
    let v = rsign(&p[d]) * rsign(&eval(&odd, &x));
    if v == 0 {
        return None;
    }
    Some((v, !touches))
}

fn mul(a: &QP, b: &QP) -> QP {
    if a.is_empty() || b.is_empty() {
        return vec![];
    }
    let mut out = vec![zero(); a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            out[i + j] = out[i + j].add(&x.mul(y));
        }
    }
    trim(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(v: &[i64]) -> QP {
        v.iter().map(|&c| Rat::from_i64(c)).collect()
    }
    fn at(v: i64, closed: bool) -> End {
        End::At(Rat::from_i64(v), closed)
    }

    #[test]
    fn counts_and_signs() {
        let mut s = 0;
        // x^2 - 2 has roots ±sqrt 2
        let p = q(&[-2, 0, 1]);
        assert_eq!(count(&p, &End::Inf, &End::Inf, &mut s), 2);
        assert_eq!(count(&p, &at(0, true), &End::Inf, &mut s), 1);
        assert_eq!(sign_on(&p, &at(2, false), &End::Inf, &mut s), Some((1, true)));
        assert_eq!(sign_on(&p, &at(-1, true), &at(1, true), &mut s), Some((-1, true)));
        assert_eq!(sign_on(&p, &at(0, true), &at(3, true), &mut s), None);
        // (x - 1)^2 touches zero at 1: nonnegative, not strict
        let p = q(&[1, -2, 1]);
        assert_eq!(sign_on(&p, &End::Inf, &End::Inf, &mut s), Some((1, false)));
        assert_eq!(sign_on(&p, &at(1, false), &End::Inf, &mut s), Some((1, true)));
        // x^3 - x on (0, 1) is negative; on [0, 1] it is nonpositive only through its roots at the ends
        let p = q(&[0, -1, 0, 1]);
        assert_eq!(sign_on(&p, &at(0, false), &at(1, false), &mut s), Some((-1, true)));
        assert_eq!(sign_on(&p, &at(0, true), &at(1, true), &mut s), Some((-1, false)));
        assert_eq!(sign_on(&p, &at(-1, true), &at(1, true), &mut s), None);
        // (x-1)^2 (x-2) on (-inf, 2): nonpositive
        let p = mul(&q(&[1, -2, 1]), &q(&[-2, 1]));
        assert_eq!(sign_on(&p, &End::Inf, &at(2, false), &mut s), Some((-1, false)));
        assert_eq!(squarefree(&p, &mut s).len(), 2);
    }
}
