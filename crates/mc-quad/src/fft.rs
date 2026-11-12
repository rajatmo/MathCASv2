//! The fast Fourier transform of a long vector of machine numbers (`nl_fft_machine(u, s)`), the engine of `fourier` and
//! `inverse_fourier` above a few hundred entries: radix-2 Cooley–Tukey for lengths that are powers of two
//! (Cooley and Tukey 1965), Bluestein's chirp-z transform through a power-of-two convolution for any other length
//! (Bluestein 1970; Rabiner, Schafer and Rader 1969), so the cost is O(n log n) for every n. The language-level
//! recursion of `lib/numerics.mc` stays in charge of short vectors, other precisions and non-integer parameters.
use mc_arith::{Arith, Num, Real};
use mc_core::{Store, View, T};
use mc_eval::{Native, NativeCx, Reply, Res};
use std::f64::consts::PI;

/// The name of the native (private to the numerics library).
pub(crate) const NAME: &str = "nl_fft_machine";

/// A complex machine number.
type C = (f64, f64);

fn mul(a: C, b: C) -> C {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}

/// exp(i sign 2 pi k / n) with the angle reduced exactly (k mod n) before the trigonometric functions.
fn twiddle(k: usize, n: usize, sign: f64) -> C {
    let a = 2.0 * PI * (k % n) as f64 / n as f64;
    (a.cos(), sign * a.sin())
}

/// In-place radix-2 transform of a power-of-two length vector with sign +1 (exp(+i ...)) or -1.
fn fft2(v: &mut [C], sign: f64) {
    let n = v.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            v.swap(i, j);
        }
    }
    let tw: Vec<C> = (0..n / 2).map(|k| twiddle(k, n, sign)).collect();
    let mut len = 2;
    while len <= n {
        let step = n / len;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let w = tw[k * step];
                let (a, b) = (v[start + k], mul(v[start + k + len / 2], w));
                v[start + k] = (a.0 + b.0, a.1 + b.1);
                v[start + k + len / 2] = (a.0 - b.0, a.1 - b.1);
            }
        }
        len <<= 1;
    }
}

/// The transform sum_r u[r] exp(i sign 2 pi r k / n) of any length.
pub(crate) fn dft(u: &[C], sign: f64) -> Vec<C> {
    let n = u.len();
    if n <= 1 {
        return u.to_vec();
    }
    if n.is_power_of_two() {
        let mut v = u.to_vec();
        fft2(&mut v, sign);
        return v;
    }
    // Bluestein: r k = (r^2 + k^2 - (k - r)^2)/2, so X_k = c_k sum_r (u_r c_r) conj(c_{k - r}) with the chirp
    // c_r = exp(i sign pi r^2 / n); r^2 is reduced mod 2n to keep the angle small
    let chirp: Vec<C> = (0..n)
        .map(|r| {
            let q = (r * r) % (2 * n);
            let a = PI * q as f64 / n as f64;
            (a.cos(), sign * a.sin())
        })
        .collect();
    let m = (2 * n - 1).next_power_of_two();
    let mut a = vec![(0.0, 0.0); m];
    for r in 0..n {
        a[r] = mul(u[r], chirp[r]);
    }
    let mut b = vec![(0.0, 0.0); m];
    b[0] = (chirp[0].0, -chirp[0].1);
    for r in 1..n {
        let c = (chirp[r].0, -chirp[r].1);
        b[r] = c;
        b[m - r] = c;
    }
    fft2(&mut a, 1.0);
    fft2(&mut b, 1.0);
    for (x, y) in a.iter_mut().zip(&b) {
        *x = mul(*x, *y);
    }
    fft2(&mut a, -1.0);
    let scale = 1.0 / m as f64;
    (0..n).map(|k| mul((a[k].0 * scale, a[k].1 * scale), chirp[k])).collect()
}

/// `nl_fft_machine(u, s)`: the machine-number vector `u` transformed with the exponent sign of the integer `s` and the
/// frequency step |s| (`F[k] = sum_r u[r] exp(2 pi i sgn(s) r (|s| k mod n) / n)`); the call stays when an entry is
/// not a machine real or complex number or `s` is not a nonzero machine integer.
pub(crate) struct Fft;

impl Native for Fft {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [u, s] = args else { return Ok(Reply::Decline) };
        let View::Int(s) = h.view(*s) else { return Ok(Reply::Decline) };
        let Some(items) = cx.as_list(*u).map(<[T<'h>]>::to_vec) else { return Ok(Reply::Decline) };
        if s == 0 || items.is_empty() {
            return Ok(Reply::Decline);
        }
        let ar = Arith::new(h);
        let mut v: Vec<C> = Vec::with_capacity(items.len());
        for &t in &items {
            match ar.num(t) {
                Some(Num::Real(Real::Machine(x))) if x.is_finite() => v.push((x, 0.0)),
                Some(Num::Complex(Real::Machine(a), Real::Machine(b))) if a.is_finite() && b.is_finite() => v.push((a, b)),
                _ => return Ok(Reply::Decline),
            }
        }
        cx.tick(v.len() as u64)?;
        let n = v.len();
        let f = dft(&v, if s > 0 { 1.0 } else { -1.0 });
        let step = (s.unsigned_abs() % n as u64) as usize;
        let out: Vec<T<'h>> = (0..n)
            .map(|k| {
                let (re, im) = f[(step * k) % n];
                ar.number(&Num::Complex(Real::Machine(re), Real::Machine(im)))
            })
            .collect();
        Ok(Reply::Done(cx.list(&out)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naive(u: &[C], sign: f64) -> Vec<C> {
        let n = u.len();
        (0..n)
            .map(|k| {
                u.iter().enumerate().fold((0.0, 0.0), |acc, (r, &x)| {
                    let w = twiddle(r * k, n, sign);
                    let p = mul(x, w);
                    (acc.0 + p.0, acc.1 + p.1)
                })
            })
            .collect()
    }

    #[test]
    fn matches_the_definition_at_power_of_two_and_other_lengths() {
        for n in [2usize, 3, 5, 8, 12, 17, 64, 100, 127] {
            let u: Vec<C> = (0..n).map(|i| ((i as f64 * 0.7).sin(), (i as f64 * 1.3).cos())).collect();
            for sign in [1.0, -1.0] {
                let (a, b) = (dft(&u, sign), naive(&u, sign));
                let err = a.iter().zip(&b).map(|(x, y)| (x.0 - y.0).hypot(x.1 - y.1)).fold(0.0, f64::max);
                assert!(err < 1e-11, "n = {n}, sign = {sign}: {err}");
            }
        }
    }
}
