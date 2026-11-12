//! Bernoulli, Euler and Bell numbers of integer index, as natives.
//!
//! Bernoulli and Euler numbers come from the tangent and secant number recurrences of Brent and Harvey (2011),
//! which use only integer multiplications by machine words (O(n^2) of them for B_(2n)); Bell numbers from Bell's
//! triangle (each row is the running sum of the previous one, starting with the previous row's last entry).
use mc_arith::num::{Int, Nat, Rat};
use mc_arith::{Arith, Num};
use mc_core::{Heap, Store, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};

/// The largest index (of the number itself) the natives compute: beyond it the call stays unevaluated.
const MAX_INDEX: u64 = 40_000;

/// The tangent numbers T_1..T_n (T_k = (2k)!-scaled coefficients of tan x): `t[k]` for k in 1..=n.
fn tangent(n: usize, cx: &mut NativeCx<'_, '_, '_>) -> Res<Vec<Nat>> {
    let mut t = vec![Nat::zero(); n + 1];
    if n == 0 {
        return Ok(t);
    }
    t[1] = Nat::from_u64(1);
    for k in 2..=n {
        t[k] = t[k - 1].mul_u64(k as u64 - 1);
    }
    for k in 2..=n {
        cx.tick(n as u64 / 64 + 1)?;
        for j in k..=n {
            let a = t[j - 1].mul_u64((j - k) as u64);
            let b = t[j].mul_u64((j - k + 2) as u64);
            t[j] = a.add(&b);
        }
    }
    Ok(t)
}

/// The secant numbers S_0..S_n (|E_2k|).
fn secant(n: usize, cx: &mut NativeCx<'_, '_, '_>) -> Res<Vec<Nat>> {
    let mut s = vec![Nat::zero(); n + 1];
    s[0] = Nat::from_u64(1);
    for k in 1..=n {
        s[k] = s[k - 1].mul_u64(k as u64);
    }
    for k in 1..=n {
        cx.tick(n as u64 / 64 + 1)?;
        for j in k + 1..=n {
            let a = s[j - 1].mul_u64((j - k) as u64);
            let b = s[j].mul_u64((j - k + 1) as u64);
            s[j] = a.add(&b);
        }
    }
    Ok(s)
}

/// B_n as an exact rational (B_1 = -1/2).
pub fn bernoulli(n: u64, cx: &mut NativeCx<'_, '_, '_>) -> Res<Rat> {
    match n {
        0 => return Ok(Rat::from_int(Int::from_i64(1))),
        1 => return Ok(Rat::new(Int::from_i64(-1), Int::from_i64(2))),
        _ if n % 2 == 1 => return Ok(Rat::from_int(Int::from_i64(0))),
        _ => {}
    }
    let m = (n / 2) as usize;
    let t = tangent(m, cx)?;
    // B_2m = (-1)^(m-1) 2m T_m / (2^2m (2^2m - 1))
    let num = t[m].mul_u64(2 * m as u64);
    let p = Nat::pow2(n);
    let den = p.mul(&p.sub_u64(1));
    Ok(Rat::new(Int::new(m.is_multiple_of(2), num), Int::from_nat(den)))
}

/// Euler numbers from this index on come from the Dirichlet beta series (see [`euler_beta`]); below it the secant
/// numbers are quicker.
const EULER_BETA_FROM: u64 = 600;

/// atan(1/x) * 2^w (Gregory's series, truncated; the error is a few units in the last place per term).
fn atan_inv(x: u64, w: u64, cx: &mut NativeCx<'_, '_, '_>) -> Res<Nat> {
    let x2 = x * x;
    let mut term = Nat::pow2(w).divrem_u64(x).0;
    let (mut pos, mut neg) = (term.clone(), Nat::zero());
    let mut k = 1u64;
    loop {
        cx.tick(term.len() as u64 / 32 + 1)?;
        term = term.divrem_u64(x2).0;
        if term.is_zero() {
            break;
        }
        let t = term.divrem_u64(2 * k + 1).0;
        if k % 2 == 1 {
            neg = neg.add(&t);
        } else {
            pos = pos.add(&t);
        }
        k += 1;
    }
    Ok(pos.sub(&neg))
}

/// floor(pi 2^q) up to a few units (Machin's formula, pi = 16 atan(1/5) - 4 atan(1/239), with 32 guard bits).
fn pi_fixed(q: u64, cx: &mut NativeCx<'_, '_, '_>) -> Res<Nat> {
    let w = q + 32;
    let a = atan_inv(5, w, cx)?.mul_u64(16);
    let b = atan_inv(239, w, cx)?.mul_u64(4);
    Ok(a.sub(&b).shr(32))
}

/// |E_n| for even n >= 2 from `|E_2m| = 2^(2m+2) (2m)! beta(2m+1) / pi^(2m+1)` (Euler; beta is the Dirichlet beta
/// function, `beta(s) = sum (-1)^k / (2k + 1)^s`): everything in fixed point with `q` fractional bits, `q` the bit
/// length of the result plus guard bits, the beta series cut when `(2k + 1)^(n+1)` exceeds `2^q` (its terms fall
/// like `3^-(n+1)`), then one division and rounding to the nearest integer. Quasi-quadratic in n against the
/// recurrence's cubic cost.
fn euler_beta(n: u64, cx: &mut NativeCx<'_, '_, '_>) -> Res<Nat> {
    let m = n / 2;
    let nf = n as f64;
    // ln n! (Stirling) and the size of the result in bits
    let ln_fact = (nf + 0.5) * nf.ln() - nf + 0.5 * (2.0 * std::f64::consts::PI).ln() + 1.0 / (12.0 * nf);
    let ln_e = ln_fact + (m + 1) as f64 * 4f64.ln() - (n + 1) as f64 * std::f64::consts::PI.ln();
    let bits_e = (ln_e / 2f64.ln()).ceil().max(1.0) as u64 + 8;
    let q = bits_e + 2 * (64 - (n + 1).leading_zeros() as u64) + 48;
    let pi = pi_fixed(q, cx)?;
    // pi^(n+1) in fixed point
    let mut pw = Nat::pow2(q);
    for i in (0..64 - (n + 1).leading_zeros()).rev() {
        cx.tick(pw.len() as u64 / 8 + 1)?;
        pw = pw.square().shr(q);
        if ((n + 1) >> i) & 1 == 1 {
            pw = pw.mul(&pi).shr(q);
        }
    }
    // beta(n+1) in fixed point
    let one = Nat::pow2(q);
    let (mut pos, mut neg) = (Nat::zero(), Nat::zero());
    let mut k = 0u64;
    loop {
        cx.tick(q / 256 + 1)?;
        let p = Nat::pow(2 * k + 1, n + 1);
        if p.bits() > q + 1 {
            break;
        }
        let t = one.div(&p);
        if k.is_multiple_of(2) {
            pos = pos.add(&t);
        } else {
            neg = neg.add(&t);
        }
        k += 1;
    }
    let beta = pos.sub(&neg);
    let mut fact = Nat::from_u64(1);
    for i in 2..=n {
        if i % 64 == 0 {
            cx.tick(fact.len() as u64 / 8 + 1)?;
        }
        fact = fact.mul_u64(i);
    }
    let num = fact.mul(&beta).shl(2 * (m + 1));
    let (qt, r) = num.divrem(&pw);
    Ok(if r.shl(1).cmp(&pw) != std::cmp::Ordering::Less { qt.add_u64(1) } else { qt })
}

/// E_n (zero for odd n).
pub fn euler(n: u64, cx: &mut NativeCx<'_, '_, '_>) -> Res<Int> {
    if n % 2 == 1 {
        return Ok(Int::from_i64(0));
    }
    if n >= EULER_BETA_FROM {
        return Ok(Int::new((n / 2) % 2 == 1, euler_beta(n, cx)?));
    }
    let m = (n / 2) as usize;
    let s = secant(m, cx)?;
    Ok(Int::new(m % 2 == 1, s[m].clone()))
}

/// The Bell number B_n by Bell's triangle.
pub fn bell(n: u64, cx: &mut NativeCx<'_, '_, '_>) -> Res<Nat> {
    let mut row = vec![Nat::from_u64(1)];
    for _ in 0..n {
        cx.tick(row.len() as u64 / 64 + 1)?;
        let mut next = Vec::with_capacity(row.len() + 1);
        next.push(row[row.len() - 1].clone());
        for x in &row {
            let s = next[next.len() - 1].add(x);
            next.push(s);
        }
        row = next;
    }
    Ok(row[0].clone())
}

fn index_of(h: &Heap, t: T<'_>) -> Option<u64> {
    match h.view(t) {
        View::Int(v) if v >= 0 && (v as u64) <= MAX_INDEX => Some(v as u64),
        _ => None,
    }
}

/// Which sequence a native computes.
#[derive(Clone, Copy)]
enum Which {
    Bernoulli,
    Euler,
    Bell,
}

struct Seq(Which);

/// The Stirling numbers S(m, 0..=m) of the second kind by the row recurrence S(n, k) = k S(n-1, k) + S(n-1, k-1).
fn stirling_row(m: usize, cx: &mut NativeCx<'_, '_, '_>) -> Res<Vec<Nat>> {
    let mut r = vec![Nat::zero(); m + 1];
    r[0] = Nat::from_u64(1);
    for i in 1..=m {
        cx.tick(i as u64 / 8 + 1)?;
        for k in (1..=i).rev() {
            r[k] = r[k].mul_u64(k as u64).add(&r[k - 1]);
        }
        r[0] = Nat::zero();
    }
    Ok(r)
}

/// `bell_b(m, x)` for an integer m >= 40 and a number x: the sum of `S(m, j) x^j` over the exact Stirling row, exactly
/// for an exact x; for an inexact x the exact value at its rational form, rounded to the precision of x (the powers
/// of x leave the machine range for large m).
fn bell_polynomial<'h>(cx: &mut NativeCx<'_, '_, 'h>, m: u64, x: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let ar = Arith::new(h);
    let (exact, prec) = match h.view(x) {
        View::Int(_) | View::Rational { .. } => (x, None),
        View::Real(_) => (cx.eval(h.call(h.sym("rationalize"), &[x, h.int(0)]))?, Some(16)),
        _ if ar.num(x).is_some_and(|n| matches!(n, Num::Real(_))) => {
            let e = cx.eval(h.call(h.sym("rationalize"), &[x, h.int(0)]))?;
            let p = cx.eval(h.call(h.sym("round"), &[h.call(h.sym("precision"), &[x])]))?;
            match h.view(p) {
                View::Int(p) if p >= 1 => (e, Some(p.max(16))),
                _ => return Ok(None),
            }
        }
        _ => return Ok(None),
    };
    if !matches!(h.view(exact), View::Int(_) | View::Rational { .. }) {
        return Ok(None);
    }
    let row = stirling_row(m as usize, cx)?;
    let mut terms = Vec::with_capacity(row.len());
    for (j, c) in row.into_iter().enumerate() {
        if j % 16 == 0 {
            cx.tick(1)?;
        }
        if c.is_zero() {
            continue;
        }
        let coef = ar.number(&Num::rat(Rat::from_int(Int::from_nat(c))));
        terms.push(ar.mul(&[coef, ar.pow(exact, h.int(j as i64))]));
    }
    let total = cx.eval(ar.add(&terms))?;
    Ok(Some(match prec {
        None => total,
        Some(p) => cx.eval(h.call(h.sym("n"), &[total, h.int(p)]))?,
    }))
}

impl Native for Seq {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        if let (Which::Bell, [a, x]) = (self.0, args) {
            return Ok(match index_of(h, *a) {
                Some(m) if m >= 40 => bell_polynomial(cx, m, *x)?.map_or(Reply::Decline, Reply::Done),
                _ => Reply::Decline,
            });
        }
        let [a] = args else { return Ok(Reply::Decline) };
        let Some(n) = index_of(h, *a) else { return Ok(Reply::Decline) };
        let ar = Arith::new(h);
        let v = match self.0 {
            Which::Bernoulli => ar.number(&Num::rat(bernoulli(n, cx)?)),
            Which::Euler => ar.number(&Num::rat(Rat::from_int(euler(n, cx)?))),
            Which::Bell => ar.number(&Num::rat(Rat::from_int(Int::from_nat(bell(n, cx)?)))),
        };
        Ok(Reply::Done(v))
    }
}

/// Register the natives.
pub fn install(k: &mut Kernel, h: &Heap) {
    for (name, w) in [("bernoulli_b", Which::Bernoulli), ("euler_e", Which::Euler), ("bell_b", Which::Bell)] {
        k.register(h.sym(name), Attributes::default(), Box::new(Seq(w)));
    }
}
