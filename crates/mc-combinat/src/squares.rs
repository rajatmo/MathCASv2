//! `squares_r(2, k)` and `squares_r(4, k)` of a machine integer k >= 1 as a native: the divisor sums of the library
//! rules (Jacobi's two- and four-square theorems: r_2(k) = 4 sum over the divisors d of k of chi_4(d), r_4(k) = 8
//! sum over the divisors d of k with 4 not dividing d of d; Grosswald, *Representations of Integers as Sums of
//! Squares*, 1985, chapters 2 and 3), worked out from the factorization of k by trial division. Other calls go to
//! the rules.
use mc_core::{Heap, Store, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};

/// The prime factorization of k (1 <= k <= 10^12) by trial division.
fn factor(mut k: u64) -> Vec<(u64, u32)> {
    let mut out = vec![];
    let mut p = 2u64;
    while p * p <= k {
        if k.is_multiple_of(p) {
            let mut e = 0;
            while k.is_multiple_of(p) {
                k /= p;
                e += 1;
            }
            out.push((p, e));
        }
        p += if p == 2 { 1 } else { 2 };
    }
    if k > 1 {
        out.push((k, 1));
    }
    out
}

/// r_2(k) = 4 prod over p^e || k of: 1 for p = 2, e + 1 for p = 1 mod 4, (e + 1) mod 2 for p = 3 mod 4 (the
/// divisor sum of chi_4 is multiplicative).
fn r2(k: u64) -> u128 {
    let mut v = 4u128;
    for (p, e) in factor(k) {
        v *= match p % 4 {
            1 => u128::from(e) + 1,
            3 => u128::from(e % 2 == 0),
            _ => 1,
        };
    }
    v
}

/// r_4(k) = 8 sigma*(k), sigma* the sum of the divisors not divisible by 4: multiplicative, 1 + 2 = 3 for any power
/// of 2 (the divisors 1 and 2), (p^(e+1) - 1)/(p - 1) for an odd p.
fn r4(k: u64) -> Option<u128> {
    let mut v = 8u128;
    for (p, e) in factor(k) {
        let f = if p == 2 {
            3
        } else {
            let pe = u128::from(p).checked_pow(e + 1)?;
            (pe - 1) / (u128::from(p) - 1)
        };
        v = v.checked_mul(f)?;
    }
    Some(v)
}

struct Squares;

impl Native for Squares {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let &[d, k] = args else { return Ok(Reply::Decline) };
        let (View::Int(d), View::Int(k)) = (h.view(d), h.view(k)) else { return Ok(Reply::Decline) };
        if !(1..=1_000_000_000_000).contains(&k) {
            return Ok(Reply::Decline);
        }
        cx.tick(((k as f64).sqrt() as u64) / 64 + 1)?;
        let v = match d {
            2 => r2(k as u64),
            4 => match r4(k as u64) {
                Some(v) => v,
                None => return Ok(Reply::Decline),
            },
            _ => return Ok(Reply::Decline),
        };
        Ok(match i64::try_from(v) {
            Ok(v) => Reply::Done(h.int(v)),
            Err(_) => Reply::Decline,
        })
    }
}

/// Register the native (before the library rules, which take every other call).
pub fn install(k: &mut Kernel, h: &Heap) {
    k.register(h.sym("squares_r"), Attributes::default(), Box::new(Squares));
}

#[cfg(test)]
mod tests {
    use super::{r2, r4};

    /// The counts by enumeration of the integer points.
    fn count(d: usize, k: i64) -> u128 {
        let r = (k as f64).sqrt() as i64 + 1;
        let mut c = 0u128;
        let mut x = vec![-r; d];
        loop {
            if x.iter().map(|v| v * v).sum::<i64>() == k {
                c += 1;
            }
            let mut i = 0;
            loop {
                if i == d {
                    return c;
                }
                x[i] += 1;
                if x[i] <= r {
                    break;
                }
                x[i] = -r;
                i += 1;
            }
        }
    }

    #[test]
    fn counts_agree_with_enumeration() {
        for k in 1..=60 {
            assert_eq!(r2(k as u64), count(2, k), "r2({k})");
            assert_eq!(r4(k as u64), Some(count(4, k)), "r4({k})");
        }
    }
}
