//! Algebraic values of trigonometric constants, sums of arctangents of rationals, and logarithms of closed complex
//! numbers (phase 12C, the "trig-algebraic constants" family).
//!
//! - [`sin_cos`]: `sin(p pi/q)` and `cos(p pi/q)` in radicals for `q = 2^k m`, `m` in {1, 3, 5, 15}: the values at
//!   multiples of `pi/6` and `pi/5` (DLMF 4.17.1-4.17.4 and the regular pentagon), sums of a multiple of `pi/6` and
//!   one of `pi/10` by the addition theorems (DLMF 4.21.2), and halvings `2 cos(t/2) = sqrt(2 + 2 cos t)` for `t` in
//!   `[0, pi]` (DLMF 4.21.4), which give the nested radicals of `pi/2^k`.
//! - [`atan_sum`]: `sum c_k atan(q_k)` with integers `c_k` and rationals `q_k` is the argument of the Gaussian
//!   rational `prod (d_k + i n_k)^c_k` (`q_k = n_k/d_k`, `d_k > 0`) up to a multiple of `pi`, which a machine
//!   evaluation of the sum fixes (the spacing is `pi`, the error about `10^-15`): Machin's
//!   `4 atan(1/5) - atan(1/239)` is `pi/4`.
//! - [`closed_log`]: `log(z) = log(abs(z)) + i arg(z)` for a closed complex number `z`, with `arg` from the real and
//!   imaginary parts (signs decided in ball arithmetic): `log((1 + i sqrt(3))/2)` is `i pi/3`.
use crate::term::B;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};
use mc_numeric::numeval;
use std::cmp::Ordering;

/// Largest denominator `q` of an angle `p pi/q` this module writes in radicals.
pub const MAX_Q: i64 = 15 << 10;

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 {
        a.abs()
    } else {
        gcd(b, a % b)
    }
}

fn sqrt<'h>(b: &B<'h>, t: T<'h>) -> T<'h> {
    b.pow(t, b.rat(1, 2))
}

/// The value of `sin(p pi/q)` (`sin` true) or `cos(p pi/q)` in radicals, or `None`.
pub fn sin_cos<'h>(b: &B<'h>, sin: bool, p: i64, q: i64) -> Option<T<'h>> {
    if q <= 0 || q > MAX_Q {
        return None;
    }
    // to [0, 2 pi)
    let mut p = p.rem_euclid(2 * q);
    let mut neg = false;
    if sin {
        if p > q {
            p -= q;
            neg = true;
        }
        if 2 * p > q {
            p = q - p;
        }
    } else {
        if p > q {
            p = 2 * q - p;
        }
        if 2 * p > q {
            p = q - p;
            neg = true;
        }
    }
    // now 0 <= p/q <= 1/2
    let g = gcd(p, q).max(1);
    let (p, q) = (p / g, q / g);
    let v = first_quadrant(b, sin, p, q)?;
    Some(if neg { b.neg(v) } else { v })
}

/// `sin` or `cos` of `p pi/q` in `[0, pi/2]`, `p/q` in lowest terms.
fn first_quadrant<'h>(b: &B<'h>, sin: bool, p: i64, q: i64) -> Option<T<'h>> {
    let half = b.rat(1, 2);
    let s5 = sqrt(b, b.int(5));
    let s2 = sqrt(b, b.int(2));
    // sin at multiples of pi/6 and pi/4; cos(t) = sin(pi/2 - t)
    let six = |k: i64| -> T<'h> {
        match k {
            0 => b.int(0),
            1 => half,
            2 => b.mul(&[half, sqrt(b, b.int(3))]),
            _ => b.int(1),
        }
    };
    match q {
        1 | 2 | 3 | 6 => {
            let k = p * 6 / q;
            let k = if sin { k } else { 3 - k };
            Some(six(k))
        }
        4 => Some(b.mul(&[half, s2])),
        5 | 10 => {
            // cos(pi/5) = (1 + sqrt 5)/4, cos(2 pi/5) = (sqrt 5 - 1)/4, sin(pi/5) = sqrt(2) sqrt(5 - sqrt 5)/4,
            // sin(2 pi/5) = sqrt(2) sqrt(5 + sqrt 5)/4 (the regular pentagon); pi/10 and 3 pi/10 by complements
            let quarter = b.rat(1, 4);
            let c1 = b.mul(&[b.rat(-1, 4), b.add(&[b.int(-1), b.neg(s5)])]);
            let c2 = b.mul(&[quarter, b.add(&[b.int(-1), s5])]);
            let s1 = b.mul(&[quarter, s2, sqrt(b, b.add(&[b.int(5), b.neg(s5)]))]);
            let sn2 = b.mul(&[quarter, s2, sqrt(b, b.add(&[b.int(5), s5]))]);
            // the angle in tenths of pi: 1, 2, 3, 4 (5 is pi/2, handled above)
            let tenths = p * 10 / q;
            Some(match (sin, tenths) {
                (true, 2) | (false, 3) => s1,
                (true, 4) | (false, 1) => sn2,
                (false, 2) | (true, 3) => c1,
                (false, 4) | (true, 1) => c2,
                _ => return None,
            })
        }
        15 | 30 => {
            // p/q = k/6 + j/10: the addition theorem on a multiple of pi/6 and one of pi/10
            let n = p * 30 / q;
            let mut best: Option<(i64, i64)> = None;
            for k in -6i64..=6 {
                let rest = n - 5 * k;
                if rest % 3 != 0 {
                    continue;
                }
                let j = rest / 3;
                if j.abs() > 10 {
                    continue;
                }
                let better = match best {
                    None => true,
                    Some((bk, bj)) => (k.abs() + j.abs(), -k) < (bk.abs() + bj.abs(), -bk),
                };
                if better {
                    best = Some((k, j));
                }
            }
            let (k, j) = best?;
            let (sa, ca) = (sin_cos(b, true, k, 6)?, sin_cos(b, false, k, 6)?);
            let (sb, cb) = (sin_cos(b, true, j, 10)?, sin_cos(b, false, j, 10)?);
            Some(if sin {
                b.add(&[b.mul(&[sa, cb]), b.mul(&[ca, sb])])
            } else {
                b.add(&[b.mul(&[ca, cb]), b.neg(b.mul(&[sa, sb]))])
            })
        }
        _ if q % 2 == 0 => {
            // halving: cos(t) = sqrt(2 + 2 cos(2t))/2, sin(t) = sqrt(2 - 2 cos(2t))/2 for t in [0, pi/2]
            let c2 = sin_cos(b, false, 2 * p, q)?;
            let twice = b.mul(&[b.int(2), c2]);
            let inner = if sin { b.sub(b.int(2), twice) } else { b.add(&[b.int(2), twice]) };
            Some(b.mul(&[half, sqrt(b, inner)]))
        }
        _ => None,
    }
}

/// `(p, q)` when `t` is `p/q pi` (a rational multiple of pi).
pub fn pi_multiple<'h>(b: &B<'h>, t: T<'h>) -> Option<(i64, i64)> {
    let h = b.heap();
    let pi = h.sym_term(h.sym("pi"));
    if t == pi {
        return Some((1, 1));
    }
    let Some((m, &[c, x])) = b.call_of(t) else { return None };
    if m != b.hd.mul || x != pi {
        return None;
    }
    match h.view(c) {
        View::Int(k) => Some((k, 1)),
        View::Rational { num, den } => match (h.view(num), h.view(den)) {
            (View::Int(n), View::Int(d)) => Some((n, d)),
            _ => None,
        },
        _ => None,
    }
}

/// `(n, d)` of a rational with machine parts.
fn ratio<'h>(b: &B<'h>, t: T<'h>) -> Option<(i64, i64)> {
    let h = b.heap();
    match h.view(t) {
        View::Int(k) => Some((k, 1)),
        View::Rational { num, den } => match (h.view(num), h.view(den)) {
            (View::Int(n), View::Int(d)) => Some((n, d)),
            _ => None,
        },
        _ => None,
    }
}

/// Most arctangent terms, and the largest coefficient, of a sum combined by [`atan_sum`].
const MAX_ATANS: usize = 8;
const MAX_ATAN_COEFF: i64 = 64;

/// The arctangent terms `c atan(q)` of a sum combined into one (see the module documentation), with the other
/// terms kept; `None` when there are fewer than two such terms or the combination is not smaller (when `exact`
/// is set, only a rational multiple of `pi` is accepted).
pub fn atan_sum<'h>(b: &B<'h>, t: T<'h>, exact: bool) -> Option<T<'h>> {
    let h = b.heap();
    let atan = h.sym("atan");
    let pi = h.sym_term(h.sym("pi"));
    let terms = b.terms(t);
    if terms.len() < 2 {
        return None;
    }
    let mut rest = vec![];
    let mut found: Vec<(i64, i64, i64)> = vec![];
    for &u in &terms {
        let (c, a) = match b.call_of(u) {
            Some((m, &[c, a])) if m == b.hd.mul => match h.view(c) {
                View::Int(c) => (c, a),
                _ => (0, u),
            },
            _ => (1, u),
        };
        match b.unary(a, atan).and_then(|q| ratio(b, q)) {
            Some((n, d)) if c != 0 && c.abs() <= MAX_ATAN_COEFF && found.len() < MAX_ATANS => found.push((c, n, d)),
            _ => rest.push(u),
        }
    }
    if found.len() < 2 {
        return None;
    }
    // the Gaussian rational prod (d + i n)^c, and the machine value of the sum
    let mut z = b.int(1);
    let mut value = 0.0f64;
    for &(c, n, d) in &found {
        z = b.mul(&[z, b.powi(b.complex(b.int(d), b.int(n)), c)]);
        value += c as f64 * (n as f64 / d as f64).atan();
    }
    let (x, y) = match b.complex_parts(z) {
        Some((x, y)) => (x, y),
        None => (z, b.int(0)),
    };
    let xf = b.ar.num(x).map(|n| match n {
        mc_arith::number::Num::Real(r) => r.to_f64(),
        _ => f64::NAN,
    })?;
    let yf = b.ar.num(y).map(|n| match n {
        mc_arith::number::Num::Real(r) => r.to_f64(),
        _ => f64::NAN,
    })?;
    let zero = |u: T<'h>| matches!(h.view(u), View::Int(0));
    let combined = if zero(x) {
        // the sum is an odd multiple of pi/2
        let k = (value / std::f64::consts::FRAC_PI_2).round() as i64;
        if k % 2 == 0 {
            return None;
        }
        b.mul(&[b.rat(k, 2), pi])
    } else {
        let q = b.div(y, x);
        let base = (yf / xf).atan();
        let k = ((value - base) / std::f64::consts::PI).round() as i64;
        if ((value - base) / std::f64::consts::PI - k as f64).abs() > 1e-6 {
            return None;
        }
        let at = if zero(y) { b.int(0) } else { b.call(atan, &[q]) };
        if exact && !zero(y) && !(ratio(b, q).is_some_and(|(n, d)| n.abs() == d)) {
            return None;
        }
        b.add(&[at, b.mul(&[b.int(k), pi])])
    };
    rest.push(combined);
    Some(b.add(&rest))
}

/// Is `t` free of symbols other than numeric constants (a closed number)?
fn closed<'h>(b: &B<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    match h.view(t) {
        View::Sym(s) => matches!(h.sym_name(s).as_str(), "pi" | "e" | "euler_gamma" | "catalan" | "golden_ratio"),
        View::App { head, args } => b.sym_of(head).is_some() && args.iter().all(|&a| closed(b, a)),
        View::Str(_) => false,
        _ => true,
    }
}

/// `log(z)` for a closed complex `z` as `log(abs(z)) + i arg(z)` with `arg` written by `atan` (see the module
/// documentation); `None` when a part's sign is not decided or `z` is real.
pub fn closed_log<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, nm: &numeval::Names, z: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    if b.is_number(z) && b.complex_parts(z).is_none() {
        return Ok(None);
    }
    if !closed(b, z) || !mentions_complex(b, z) {
        return Ok(None);
    }
    let ce = cx.eval(b.call_named("complex_expand", &[z]))?;
    let x = cx.eval(b.call_named("re", &[ce]))?;
    let y = cx.eval(b.call_named("im_part", &[ce]))?;
    let unevaluated = |t: T<'h>| matches!(b.call_of(t), Some((s, _)) if matches!(h.sym_name(s).as_str(), "re" | "im_part"));
    if unevaluated(x) || unevaluated(y) {
        return Ok(None);
    }
    let sx = numeval::sign(h, &b.ar, nm, x);
    let sy = numeval::sign(h, &b.ar, nm, y);
    let (Some(sx), Some(sy)) = (sx, sy) else { return Ok(None) };
    if sy == Ordering::Equal {
        return Ok(None);
    }
    let pi = h.sym_term(h.sym("pi"));
    let atan = cx.eval(b.call_named("atan", &[b.div(y, x)]))?;
    let arg = match sx {
        Ordering::Greater => atan,
        Ordering::Equal => b.mul(&[b.rat(if sy == Ordering::Greater { 1 } else { -1 }, 2), pi]),
        Ordering::Less => b.add(&[atan, if sy == Ordering::Greater { pi } else { b.neg(pi) }]),
    };
    let modulus = cx.eval(b.call_named("expand", &[b.add(&[b.mul(&[x, x]), b.mul(&[y, y])])]))?;
    let modulus = cx.eval(b.pow(modulus, b.rat(1, 2)))?;
    let r = b.add(&[b.call(b.hd.log, &[modulus]), b.mul(&[b.i(), arg])]);
    Ok(Some(cx.eval(r)?))
}

fn mentions_complex<'h>(b: &B<'h>, t: T<'h>) -> bool {
    if b.complex_parts(t).is_some() {
        return true;
    }
    match b.heap().view(t) {
        View::App { args, .. } => args.iter().any(|&a| mentions_complex(b, a)),
        _ => false,
    }
}
