//! Integer relations and algebraic recognition: `find_integer_null_vector` and `root_approximant` (decision
//! R.25).
//!
//! The integer relation search is PSLQ (Ferguson, Bailey and Arno, *Analysis of PSLQ, an integer relation finding
//! algorithm*, Math. Comp. 68, 1999; the one-level form of Bailey and Broadhurst 2000) on floating-point numbers of
//! the inputs' precision (`mc-arith`'s `Float`, rounded to p bits at each operation), the matrices A and B in exact
//! 128-bit integers. A relation is reported when a component of y falls below 2^-(p - guard) relative to the inputs,
//! and only if it holds when checked again: |Σ c_i x_i| must be at the rounding level of the inputs. PSLQ's bound
//! (the relation norm is at least 1 / max |H_jj|) gives the message when none exists within the precision.
//!
//! `root_approximant(x, n)` runs the search on [1, x, ..., x^d] for d = 1, ..., n (n = 8 by default) and accepts the
//! first relation whose cost (digits of its largest coefficient times d + 1) is below the digits of x: the
//! polynomial is then far less likely to be a coincidence of the precision. The algebraic number is the root of
//! that polynomial nearest x, as `solve` writes it (radicals up to degree 2, `root(...)` beyond).
// index loops over several parallel arrays read more plainly than zipped iterators here
#![allow(clippy::needless_range_loop)]
use mc_arith::num::{Float, Nat};
use mc_arith::{Kind, Num, Real};
use std::cmp::Ordering;

/// The outcome of a search.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Relation {
    /// integers c with Σ c_i x_i = 0 to the precision
    Found(Vec<i128>),
    /// no relation of norm below the bound exists (the bound in decimal digits)
    None(f64),
}

/// Rounded arithmetic at p bits.
#[derive(Clone, Copy)]
struct P(u64);

impl P {
    fn add(self, a: &Float, b: &Float) -> Float {
        a.add(b, self.0).0
    }
    fn sub(self, a: &Float, b: &Float) -> Float {
        a.sub(b, self.0).0
    }
    fn mul(self, a: &Float, b: &Float) -> Float {
        a.mul(b, self.0).0
    }
    fn div(self, a: &Float, b: &Float) -> Float {
        a.div(b, self.0).0
    }
    fn sqrt(self, a: &Float) -> Float {
        a.sqrt(self.0).0
    }
}

/// An i128 as a Float.
fn fl(v: i128) -> Float {
    Float::new(v < 0, Nat::from_u128(v.unsigned_abs()), 0)
}

/// The nearest integer to x, if it fits in i128 comfortably.
fn nint(x: &Float) -> Option<i128> {
    if x.is_zero() {
        return Some(0);
    }
    if x.top() > 100 {
        return None;
    }
    let v = if x.exp >= 0 {
        x.man.shl(x.exp as u64).to_u128()? as i128
    } else {
        let k = (-x.exp) as u64;
        // (man + 2^(k-1)) >> k
        let half = Nat::pow2(k - 1);
        x.man.add(&half).shr(k).to_u128()? as i128
    };
    Some(if x.neg { -v } else { v })
}

/// log2 |x| (or -inf).
fn log2(x: &Float) -> f64 {
    if x.is_zero() {
        return f64::NEG_INFINITY;
    }
    // top - 1 + log2(leading fraction), good enough for thresholds
    x.top() as f64 - 1.0
}

/// A number as a Float of p bits (`None` when not real).
pub(crate) fn float_of(n: &Num, p: u64) -> Option<Float> {
    if let Some(Real::Machine(v)) = n.as_real() {
        let (neg, man, exp) = mc_arith::num::float::f64_to_dyadic(*v)?;
        return Some(Float::new(neg, man, exp));
    }
    match n.as_real()?.to_kind(Kind::Ball, p) {
        Real::Ball(b, _) => Some(b.mid),
        _ => None,
    }
}

/// PSLQ on x (p bits). `max_iter` bounds the work; `None` when it is reached or a coefficient overflows.
/// `norm_bits`: stop with `Relation::None` once no relation of norm below 2^norm_bits can exist.
pub(crate) fn pslq(x: &[Float], p: u64, norm_bits: f64, max_iter: usize, tick: &mut dyn FnMut() -> bool) -> Option<Relation> {
    let n = x.len();
    if n < 2 {
        return None;
    }
    let ar = P(p + 16);
    // a zero component is a relation by itself
    if let Some(k) = x.iter().position(|v| v.is_zero()) {
        let mut c = vec![0; n];
        c[k] = 1;
        return Some(Relation::Found(c));
    }
    let gamma = 2.0f64 / 3f64.sqrt() + 0.01;
    // partial norms s_k = sqrt(sum_{j>=k} x_j^2)
    let mut s = vec![Float::zero(); n];
    let mut acc = Float::zero();
    for k in (0..n).rev() {
        acc = ar.add(&acc, &ar.mul(&x[k], &x[k]));
        s[k] = ar.sqrt(&acc);
    }
    let t0 = s[0].clone();
    let mut y: Vec<Float> = x.iter().map(|v| ar.div(v, &t0)).collect();
    for v in s.iter_mut() {
        *v = ar.div(v, &t0);
    }
    // H: n by n-1
    let mut h = vec![vec![Float::zero(); n - 1]; n];
    for i in 0..n {
        for j in 0..(n - 1).min(i + 1) {
            if i == j {
                h[i][j] = ar.div(&s[j + 1], &s[j]);
            } else {
                let num = ar.mul(&y[i], &y[j]);
                let den = ar.mul(&s[j], &s[j + 1]);
                h[i][j] = ar.div(&num, &den).neg();
            }
        }
    }
    let mut a: Vec<Vec<i128>> = (0..n).map(|i| (0..n).map(|j| (i == j) as i128).collect()).collect();
    let mut b = a.clone();
    // reduction of rows i0.. of H
    let reduce = |h: &mut Vec<Vec<Float>>, y: &mut Vec<Float>, a: &mut Vec<Vec<i128>>, b: &mut Vec<Vec<i128>>, from: usize| -> Option<()> {
        for i in from.max(1)..n {
            for j in (0..(i).min(n - 1)).rev() {
                if h[j][j].is_zero() {
                    continue;
                }
                let q = ar.div(&h[i][j], &h[j][j]);
                let t = nint(&q)?;
                if t == 0 {
                    continue;
                }
                let tf = fl(t);
                y[j] = ar.add(&y[j], &ar.mul(&tf, &y[i]));
                for k in 0..=j {
                    h[i][k] = ar.sub(&h[i][k], &ar.mul(&tf, &h[j][k]));
                }
                for k in 0..n {
                    a[i][k] = a[i][k].checked_sub(t.checked_mul(a[j][k])?)?;
                    b[k][j] = b[k][j].checked_add(t.checked_mul(b[k][i])?)?;
                }
            }
        }
        Some(())
    };
    reduce(&mut h, &mut y, &mut a, &mut b, 1)?;
    // the detection threshold: the inputs' rounding level with guard bits for the dimension
    let thresh = -(p as f64) + 4.0 + 2.0 * n as f64;
    for _ in 0..max_iter {
        if !tick() {
            return None;
        }
        // the row to exchange
        let mut m = 0;
        let mut best = f64::NEG_INFINITY;
        for i in 0..n - 1 {
            let v = (i as f64 + 1.0) * gamma.log2() + log2(&h[i][i]) + frac_log2(&h[i][i]);
            if v > best {
                best = v;
                m = i;
            }
        }
        y.swap(m, m + 1);
        a.swap(m, m + 1);
        for row in b.iter_mut() {
            row.swap(m, m + 1);
        }
        h.swap(m, m + 1);
        if m + 1 < n - 1 {
            let (hmm, hm1) = (h[m][m].clone(), h[m][m + 1].clone());
            let t0 = ar.sqrt(&ar.add(&ar.mul(&hmm, &hmm), &ar.mul(&hm1, &hm1)));
            if t0.is_zero() {
                return None;
            }
            let (t1, t2) = (ar.div(&hmm, &t0), ar.div(&hm1, &t0));
            for row in h.iter_mut().skip(m) {
                let (t3, t4) = (row[m].clone(), row[m + 1].clone());
                row[m] = ar.add(&ar.mul(&t1, &t3), &ar.mul(&t2, &t4));
                row[m + 1] = ar.sub(&ar.mul(&t1, &t4), &ar.mul(&t2, &t3));
            }
        }
        reduce(&mut h, &mut y, &mut a, &mut b, m + 1)?;
        // a vanishing y_j: column j of B is a relation
        let (j, ymin) = y.iter().enumerate().map(|(j, v)| (j, log2(v) + frac_log2(v))).min_by(|a, b| a.1.total_cmp(&b.1)).expect("n >= 2");
        if ymin < thresh {
            let c: Vec<i128> = (0..n).map(|k| b[k][j]).collect();
            return Some(Relation::Found(normalise(c)));
        }
        // the bound: no relation of norm below 1 / max |H_jj|
        let hmax = (0..n - 1).map(|j| log2(&h[j][j]) + frac_log2(&h[j][j])).fold(f64::NEG_INFINITY, f64::max);
        // when the bound exceeds what the precision can show, stop
        if -hmax > norm_bits {
            return Some(Relation::None(-hmax * std::f64::consts::LOG10_2));
        }
    }
    None
}

/// log2 of the leading fraction of x (in [0, 1)).
fn frac_log2(x: &Float) -> f64 {
    if x.is_zero() {
        return 0.0;
    }
    let bits = x.man.bits();
    let lead = if bits > 53 { x.man.shr(bits - 53) } else { x.man.shl(53 - bits) };
    let f = lead.to_u128().unwrap_or(1u128 << 52) as f64 / (1u128 << 53) as f64;
    f.log2() + 1.0
}

/// The sign made positive on the first nonzero coefficient, the gcd divided out.
fn normalise(mut c: Vec<i128>) -> Vec<i128> {
    let g = c.iter().fold(0i128, |g, &v| gcd(g, v.abs()));
    if g > 1 {
        for v in c.iter_mut() {
            *v /= g;
        }
    }
    if c.iter().find(|&&v| v != 0).is_some_and(|&v| v < 0) {
        for v in c.iter_mut() {
            *v = -*v;
        }
    }
    c
}

fn gcd(a: i128, b: i128) -> i128 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// Does Σ c_i x_i vanish to the precision (relative to the largest |c_i x_i|)?
pub(crate) fn holds(c: &[i128], x: &[Float], p: u64) -> bool {
    let ar = P(p + 64);
    let mut sum = Float::zero();
    let mut big = f64::NEG_INFINITY;
    for (ci, xi) in c.iter().zip(x) {
        let t = ar.mul(&fl(*ci), xi);
        big = big.max(log2(&t));
        sum = ar.add(&sum, &t);
    }
    sum.is_zero() || log2(&sum) < big - p as f64 + (8.0 + 2.0 * c.len() as f64).min(0.25 * p as f64)
}

/// Compare by magnitude (for tests).
#[allow(dead_code)]
fn cmp_abs(a: &Float, b: &Float) -> Ordering {
    a.abs().cmp_exact(&b.abs())
}

// ---------------------------------------------------------------- natives

use crate::natives::{bad_argument, failure};
use mc_arith::Arith;
use mc_core::{Heap, Store, View, T};
use mc_eval::{Native, NativeCx, Reply, Res};

/// The most PSLQ iterations of one search.
const MAX_ITER: usize = 20_000;

/// A number argument and its precision in bits: machine reals 53, balls their own; exact and symbolic numbers
/// (pi, sqrt(2)) are evaluated at machine precision, as `n` does.
fn input<'h>(cx: &mut NativeCx<'_, '_, 'h>, ar: &Arith<'h, Heap>, t: T<'h>) -> Res<Option<(Num, u64)>> {
    let h = cx.heap();
    let v = match ar.num(t) {
        Some(Num::Real(Real::Exact(_))) | None => {
            let e = cx.eval(h.call(h.sym("n"), &[t]))?;
            match ar.num(e) {
                Some(v) => v,
                None => return Ok(None),
            }
        }
        Some(v) => v,
    };
    // a complex number: the precision of its parts
    let part = match &v {
        Num::Complex(a, b) => Some(if a.prec().unwrap_or(0) >= b.prec().unwrap_or(0) { a } else { b }),
        _ => v.as_real(),
    };
    let p = match part {
        Some(Real::Machine(_)) => 53,
        // `n(x, d)` works with 16 guard bits beyond the d digits asked for: the precision of the input is the one
        // asked for, not what the guard happens to add (a relation found in the guard bits would be an accident)
        // (kept for longer inputs: there the extra digits make a relation of a high degree findable)
        Some(Real::Ball(_, p)) if *p <= 80 => p.saturating_sub(16).max(6),
        Some(Real::Ball(_, p)) => *p,
        Some(Real::Exact(_)) => 133,
        None => return Ok(None),
    };
    Ok(Some((v, p)))
}

/// Print a message of `name`.
fn message<'h>(cx: &mut NativeCx<'_, '_, 'h>, name: &str, text: &str) -> Res<()> {
    let h = cx.heap();
    cx.eval(h.call(h.sym("message"), &[h.sym_term(h.sym(name)), h.string(text)]))?;
    Ok(())
}

/// `find_integer_null_vector([x1, ..., xn])`, `find_integer_null_vector(xs, d)`: integers c, not all zero, with
/// the sum of c_i x_i zero to the precision of the x_i (and |c| at most d).
pub(crate) struct FindIntegerNullVector;

impl Native for FindIntegerNullVector {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        const NAME: &str = "find_integer_null_vector";
        let h = cx.heap();
        let ar = Arith::new(h);
        let (xs, bound) = match args {
            [xs] => (*xs, None),
            [xs, d] => match ar.num(*d).and_then(|v| v.as_real().map(|r| r.to_f64())) {
                Some(d) if d > 0.0 => (*xs, Some(d)),
                _ => return Ok(bad_argument(h, NAME, h.int(2))),
            },
            _ => return Ok(Reply::Decline),
        };
        let Some(items) = cx.as_list(xs) else { return Ok(bad_argument(h, NAME, h.int(1))) };
        if items.len() < 2 {
            return Ok(bad_argument(h, NAME, h.int(1)));
        }
        let mut nums = Vec::new();
        let mut p = u64::MAX;
        for &it in items {
            let Some((v, q)) = input(cx, &ar, it)? else { return Ok(bad_argument(h, NAME, h.int(1))) };
            p = p.min(q);
            nums.push(v);
        }
        // complex numbers (round 37): integers c with sum c_i Re x_i = 0 and sum c_i Im x_i = 0 together. The search
        // runs on the real numbers Re x_i + theta Im x_i with theta = sqrt(2) (a relation of both parts is one of
        // these), and a relation found is kept only when it holds for the real and the imaginary parts separately
        let complex = nums.iter().any(|v| matches!(v, Num::Complex(..)));
        let parts: Option<(Vec<Float>, Vec<Float>)> = if complex {
            let part = |v: &Num, im: bool| match v {
                Num::Complex(a, b) => float_of(&Num::Real(if im { b.clone() } else { a.clone() }), p),
                Num::Real(_) if im => Some(Float::zero()),
                Num::Real(_) => float_of(v, p),
            };
            let re: Option<Vec<Float>> = nums.iter().map(|v| part(v, false)).collect();
            let im: Option<Vec<Float>> = nums.iter().map(|v| part(v, true)).collect();
            re.zip(im)
        } else {
            None
        };
        let fs: Option<Vec<Float>> = match &parts {
            Some((re, im)) => {
                let ar = P(p + 16);
                let theta = ar.sqrt(&fl(2));
                Some(re.iter().zip(im).map(|(a, b)| ar.add(a, &ar.mul(&theta, b))).collect())
            }
            None => nums.iter().map(|v| float_of(v, p)).collect(),
        };
        let Some(fs) = fs else { return Ok(bad_argument(h, NAME, h.int(1))) };
        let mut ticks = 0u64;
        let r = pslq(&fs, p, 0.5 * p as f64 + 4.0, MAX_ITER, &mut || {
            ticks += 1;
            true
        });
        cx.tick(ticks)?;
        let none = failure(h, "not_found", &[h.sym_term(h.sym(NAME))]);
        let verified = |c: &[i128]| match &parts {
            Some((re, im)) => holds(c, re, p) && holds(c, im, p),
            None => holds(c, &fs, p),
        };
        match r {
            Some(Relation::Found(c)) if verified(&c) => {
                let norm = c.iter().map(|&v| (v as f64).powi(2)).sum::<f64>().sqrt();
                if bound.is_some_and(|d| norm > d) {
                    message(cx, NAME, &format!("the relation found has norm {norm:.4}, above the bound"))?;
                    return Ok(Reply::Done(none));
                }
                // a relation as large as the precision allows for any numbers (Dirichlet: norm 2^(p / (n - 1)))
                // may be an artifact of the precision
                let typical = (p as f64 / (c.len() as f64 - 1.0)).exp2();
                if norm > 0.01 * typical {
                    message(cx, NAME, &format!("the relation has norm {norm:.4e}, as large as relations that hold for any numbers at this precision: it may be an artifact (raise the precision)"))?;
                }
                let ts: Vec<T<'h>> = c.iter().map(|&v| h.int(v as i64)).collect();
                Ok(Reply::Done(cx.list(&ts)))
            }
            Some(Relation::None(d)) => {
                let digits = p as f64 * std::f64::consts::LOG10_2;
                message(cx, NAME, &format!("no integer relation of norm below 10^{d:.1} exists at the precision of the input ({digits:.1} digits)"))?;
                Ok(Reply::Done(none))
            }
            _ => {
                message(cx, NAME, "no integer relation found within the precision of the input")?;
                Ok(Reply::Done(none))
            }
        }
    }
}

/// `root_approximant(x)`, `root_approximant(x, n)`: an algebraic number of degree at most n (8) close to x.
pub(crate) struct RootApproximant;

impl Native for RootApproximant {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        const NAME: &str = "root_approximant";
        let h = cx.heap();
        let ar = Arith::new(h);
        let (x, n, given) = match args {
            [x] => (*x, 8usize, false),
            [x, k] => match h.view(*k) {
                View::Int(k @ 1..=40) => (*x, k as usize, true),
                _ => return Ok(bad_argument(h, NAME, h.int(2))),
            },
            _ => return Ok(Reply::Decline),
        };
        if let Some(Num::Real(Real::Exact(_))) = ar.num(x) {
            return Ok(Reply::Done(x));
        }
        let Some((v, p)) = input(cx, &ar, x)? else { return Ok(bad_argument(h, NAME, h.int(1))) };
        let Some(xf) = float_of(&v, p + 32) else { return Ok(bad_argument(h, NAME, h.int(1))) };
        let digits = p as f64 * std::f64::consts::LOG10_2;
        let ar_p = P(p + 32);
        let mut chosen: Option<Vec<i128>> = None;
        let mut ticks = 0u64;
        for d in 1..=n {
            let mut pows = vec![Float::from_i64(1)];
            for k in 1..=d {
                let last = pows[k - 1].clone();
                pows.push(ar_p.mul(&last, &xf));
            }
            // the last degree asked for explicitly: the relation at full precision, however large
            let nb = if given && d == n { p as f64 } else { 0.5 * p as f64 + 4.0 };
            let found = pslq(&pows, p, nb, MAX_ITER, &mut || {
                ticks += 1;
                true
            });
            let Some(Relation::Found(c)) = found else { continue };
            if c[d] == 0 || !holds(&c, &pows, p) {
                continue;
            }
            let big = c.iter().map(|v| (v.unsigned_abs() as f64).max(1.0)).fold(1.0f64, f64::max);
            let cost = big.log10() * (d as f64 + 1.0);
            if cost < 0.9 * digits + 2.0 * (1.0 - digits / 10.0).max(0.0) {
                chosen = Some(c);
                break;
            }
            if given && d == n {
                chosen = Some(c);
            }
        }
        cx.tick(ticks)?;
        let Some(c) = chosen else {
            message(cx, NAME, "no algebraic number of small height found at this precision")?;
            return Ok(Reply::Done(failure(h, "not_found", &[h.sym_term(h.sym(NAME))])));
        };
        // the polynomial and its root nearest x
        let d = c.iter().rposition(|&v| v != 0).unwrap_or(0);
        if d == 1 {
            let q = ar.div(h.int(-(c[0] as i64)), h.int(c[1] as i64));
            return Ok(Reply::Done(cx.eval(q)?));
        }
        let var = h.sym_term(h.sym("ra·x"));
        let mut terms = Vec::new();
        for (k, &ck) in c.iter().enumerate() {
            if ck != 0 {
                terms.push(ar.mul(&[h.int(ck as i64), ar.pow(var, h.int(k as i64))]));
            }
        }
        let poly = cx.eval(ar.add(&terms))?;
        let eq = h.call(h.sym("equal"), &[poly, h.int(0)]);
        let sols = cx.eval(h.call(h.sym("solve"), &[eq, var]))?;
        let Some(list) = cx.as_list(sols) else { return Ok(Reply::Done(sols)) };
        let target = v.as_real().map(|r| r.to_f64()).unwrap_or(f64::NAN);
        let mut best: Option<(f64, T<'h>)> = None;
        for &s in list {
            let Some(&[r]) = cx.as_list(s) else { continue };
            let View::App { args: &[_, val], .. } = h.view(r) else { continue };
            let nv = cx.eval(h.call(h.sym("n"), &[val]))?;
            let Some(Num::Real(rv)) = ar.num(nv) else { continue };
            let dist = (rv.to_f64() - target).abs();
            if best.as_ref().is_none_or(|b| dist < b.0) {
                best = Some((dist, val));
            }
        }
        // the polynomial's variable is bound inside root(x => ...): written x
        let x_sym = h.sym_term(h.sym("x"));
        Ok(Reply::Done(match best {
            Some((_, val)) => mc_numfn::replace(h, val, &mut |u| (u == var).then_some(x_sym)),
            None => failure(h, "not_found", &[h.sym_term(h.sym(NAME))]),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(v: f64, p: u64) -> Float {
        float_of(&Num::Real(Real::Machine(v)), p).unwrap()
    }

    #[test]
    fn machine_relations() {
        // 1, sqrt(2), sqrt(2)^2: 2 - x^2 = 0
        let r = 2f64.sqrt();
        let xs = [f(1.0, 53), f(r, 53), f(r * r, 53)];
        let rel = pslq(&xs, 53, 30.0, 1000, &mut || true).unwrap();
        assert_eq!(rel, Relation::Found(vec![2, 0, -1]));
        // 1, x, x^2, x^3, x^4 for sqrt(2) + sqrt(3): 1 - 10 x^2 + x^4
        let x = 2f64.sqrt() + 3f64.sqrt();
        let xs: Vec<Float> = (0..5).map(|k| f(x.powi(k), 53)).collect();
        let rel = pslq(&xs, 53, 30.0, 1000, &mut || true).unwrap();
        assert_eq!(rel, Relation::Found(vec![1, 0, -10, 0, 1]));
    }
}
