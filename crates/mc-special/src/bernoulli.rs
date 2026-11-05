//! Exact Bernoulli numbers B_2n, cached process-wide, and their machine (double-double) forms.
//!
//! The numbers come from the tangent numbers T_n (Brent and Harvey's in-place recurrence on natural numbers,
//! O(n^2) additions and small multiplications): B_2n = (-1)^(n-1) 2n T_n / (2^(2n) (2^(2n) - 1)). The cache grows
//! by doubling and is shared by all threads; a computation stopped by the interrupt is discarded, never cached.
use crate::dd::DD;
use mc_arith::num::float::f64_to_dyadic;
use mc_arith::num::{stopped, Int, Nat, Rat};
use std::sync::{Mutex, OnceLock};

/// The largest index 2n served (beyond it the callers use other methods or stay unevaluated).
pub const MAX_INDEX: u64 = 20_000;

fn cache() -> &'static Mutex<Vec<Rat>> {
    static CACHE: OnceLock<Mutex<Vec<Rat>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Vec::new()))
}

/// B_2, B_4, ..., B_2n computed from scratch (`None` when the stop flag rises).
fn compute(n: usize) -> Option<Vec<Rat>> {
    // tangent numbers T_1..T_n
    let mut t: Vec<Nat> = vec![Nat::zero(); n + 1];
    if n >= 1 {
        t[1] = Nat::from_u64(1);
    }
    for k in 2..=n {
        t[k] = t[k - 1].mul_u64((k - 1) as u64);
    }
    for k in 2..=n {
        if stopped() {
            return None;
        }
        for j in k..=n {
            t[j] = t[j - 1].mul_u64((j - k) as u64).add(&t[j].mul_u64((j - k + 2) as u64));
        }
    }
    let mut out = Vec::with_capacity(n);
    for (i, ti) in t.iter().enumerate().skip(1) {
        let m = 2 * i as u64;
        let num = Int::new(i % 2 == 0, ti.mul_u64(m));
        let den = Nat::pow2(m).mul(&Nat::pow2(m).sub_u64(1));
        out.push(Rat::new(num, Int::from_nat(den)));
    }
    Some(out)
}

/// B_2n for 1 <= n (2n <= [`MAX_INDEX`]); `None` beyond the limit or when the computation was stopped.
pub fn b2n(n: u64) -> Option<Rat> {
    if n == 0 || 2 * n > MAX_INDEX {
        return None;
    }
    let n = n as usize;
    {
        let c = cache().lock().unwrap_or_else(|e| e.into_inner());
        if c.len() >= n {
            return Some(c[n - 1].clone());
        }
    }
    let want = n.max(2 * cache().lock().unwrap_or_else(|e| e.into_inner()).len()).max(32).min((MAX_INDEX / 2) as usize);
    let v = compute(want)?;
    let r = v[n - 1].clone();
    let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
    if c.len() < v.len() {
        *c = v;
    }
    Some(r)
}

/// B_k for any k >= 0 (B_1 = -1/2, odd k > 1 give 0).
pub fn bernoulli(k: u64) -> Option<Rat> {
    match k {
        0 => Some(Rat::from_i64(1)),
        1 => Some(Rat::new(Int::from_i64(-1), Int::from_i64(2))),
        k if k % 2 == 1 => Some(Rat::from_i64(0)),
        k => b2n(k / 2),
    }
}

/// The exact value of a double as a rational.
pub fn rat_of_f64(v: f64) -> Rat {
    let Some((neg, man, exp)) = f64_to_dyadic(v) else { return Rat::from_i64(0) };
    if exp >= 0 {
        Rat::from_int(Int::new(neg, man.shl(exp as u64)))
    } else {
        Rat::new(Int::new(neg, man), Int::from_nat(Nat::pow2((-exp) as u64)))
    }
}

/// The nearest double of a rational.
pub fn rat_to_f64(q: &Rat) -> f64 {
    mc_arith::num::float::ratio_to_f64(q.num.neg, &q.num.mag, &q.den)
}

/// A rational as a double-double (hi the nearest double, lo the nearest double to the rest).
pub fn rat_to_dd(q: &Rat) -> DD {
    let hi = rat_to_f64(q);
    if !hi.is_finite() || hi == 0.0 {
        return DD::new(hi);
    }
    let lo = rat_to_f64(&q.sub(&rat_of_f64(hi)));
    DD { hi, lo }
}

/// A table of double-doubles computed once from exact Bernoulli numbers: entry k-1 is f(k, B_2k) for k = 1..=len.
pub fn dd_table(cell: &'static OnceLock<Vec<DD>>, len: u64, f: fn(u64, &Rat) -> Rat) -> &'static [DD] {
    cell.get_or_init(|| {
        // computed with the stop flag off: a table is never half-built
        let _scope = mc_arith::num::StopScope::enter(None);
        (1..=len).map(|k| rat_to_dd(&f(k, &b2n(k).expect("small Bernoulli numbers")))).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(n: i64, d: i64) -> Rat {
        Rat::new(Int::from_i64(n), Int::from_i64(d))
    }

    #[test]
    fn known_values() {
        assert_eq!(bernoulli(2), Some(r(1, 6)));
        assert_eq!(bernoulli(4), Some(r(-1, 30)));
        assert_eq!(bernoulli(6), Some(r(1, 42)));
        assert_eq!(bernoulli(12), Some(r(-691, 2730)));
        assert_eq!(bernoulli(20), Some(r(-174611, 330)));
        assert_eq!(bernoulli(1), Some(r(-1, 2)));
        assert_eq!(bernoulli(7), Some(r(0, 1)));
        // B_60 = -1215233140483755572040304994079820246041491 / 56786730
        let b60 = b2n(30).unwrap();
        assert_eq!(b60.den, Nat::from_u64(56786730));
        assert_eq!(b60.num.mag.to_decimal(), "1215233140483755572040304994079820246041491");
        assert!(b60.num.neg);
        assert!(rat_to_dd(&r(1, 3)).mul_f(3.0).sub(DD::ONE).hi.abs() < 1e-32);
    }
}
