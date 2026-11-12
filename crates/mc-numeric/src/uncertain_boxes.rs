//! Rectangular box arithmetic for `centered_interval` (round 36): a centered interval with a complex centre c and a
//! radius rx + i ry (a real radius r meaning r in both parts) is the box [re c -+ rx] x [im c -+ ry]. Sums and
//! products of boxes are interval arithmetic on the parts (re(zw) = re z re w - im z im w, ...); a power z^w is
//! exp(w log z), with log over a box that avoids 0 and the branch cut (the modulus between the nearest point and the
//! farthest corner, the argument between the corner arguments: the extreme rays from 0 touching a convex set that
//! does not contain 0 pass through corners), and exp(u + iv) = e^u (cos v + i sin v) with cos and sin over an
//! interval by their extremes at multiples of pi. A real box raised to a real box (base > 0) is spanned by the four
//! corners (x^y is monotone in each argument). Every operation widens its ends outwards by a few ulps, so the result
//! encloses the true range of values (Moore, Kearfott and Cloud, "Introduction to Interval Analysis", SIAM 2009,
//! chapters 2 and 5). Mixed `interval` arguments enter as the hull of their pieces.
use crate::formula::Fx;
use mc_api::{names, Store, View, T};
use mc_arith::Num;
use mc_eval::Res;

type Iv = (f64, f64);

#[derive(Clone, Copy, Debug)]
struct CBox {
    re: Iv,
    im: Iv,
}

fn down(x: f64) -> f64 {
    if x == 0.0 {
        -f64::from_bits(1)
    } else {
        x - x.abs() * (4.0 * f64::EPSILON) - f64::from_bits(1)
    }
}

fn up(x: f64) -> f64 {
    if x == 0.0 {
        f64::from_bits(1)
    } else {
        x + x.abs() * (4.0 * f64::EPSILON) + f64::from_bits(1)
    }
}

fn widen((a, b): Iv) -> Iv {
    (down(a), up(b))
}

fn exact_zero((a, b): Iv) -> bool {
    a == 0.0 && b == 0.0
}

fn iadd(a: Iv, b: Iv) -> Iv {
    if exact_zero(a) {
        return b;
    }
    if exact_zero(b) {
        return a;
    }
    widen((a.0 + b.0, a.1 + b.1))
}

fn isub(a: Iv, b: Iv) -> Iv {
    iadd(a, (-b.1, -b.0))
}

fn imul(a: Iv, b: Iv) -> Iv {
    if exact_zero(a) || exact_zero(b) {
        return (0.0, 0.0);
    }
    let p = [a.0 * b.0, a.0 * b.1, a.1 * b.0, a.1 * b.1];
    let lo = p.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = p.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    widen((lo, hi))
}

fn iexp(a: Iv) -> Iv {
    widen((a.0.exp(), a.1.exp()))
}

/// cos over [a, b]: the end values, and +-1 where an even or odd multiple of pi lies inside.
fn icos(a: Iv) -> Option<Iv> {
    if !(a.0.is_finite() && a.1.is_finite()) || a.1 - a.0 > 1e6 {
        return None;
    }
    let (mut lo, mut hi) = (a.0.cos().min(a.1.cos()), a.0.cos().max(a.1.cos()));
    let pi = std::f64::consts::PI;
    let k0 = (a.0 / pi).floor() as i64 - 1;
    let k1 = (a.1 / pi).ceil() as i64 + 1;
    for k in k0..=k1 {
        let x = k as f64 * pi;
        // a multiple of pi near an end counts as inside (the end values carry rounding)
        if x >= a.0 - 1e-12 && x <= a.1 + 1e-12 {
            if k.rem_euclid(2) == 0 {
                hi = 1.0;
            } else {
                lo = -1.0;
            }
        }
    }
    let (lo, hi) = widen((lo, hi));
    Some((lo.max(-1.0), hi.min(1.0)))
}

fn isin(a: Iv) -> Option<Iv> {
    let h = std::f64::consts::FRAC_PI_2;
    icos(widen((a.0 - h, a.1 - h)))
}

fn badd(a: CBox, b: CBox) -> CBox {
    CBox { re: iadd(a.re, b.re), im: iadd(a.im, b.im) }
}

fn bmul(a: CBox, b: CBox) -> CBox {
    CBox { re: isub(imul(a.re, b.re), imul(a.im, b.im)), im: iadd(imul(a.re, b.im), imul(a.im, b.re)) }
}

fn is_real(a: CBox) -> bool {
    exact_zero(a.im)
}

/// log over a box away from 0 and from the negative real axis.
fn blog(z: CBox) -> Option<CBox> {
    let (x, y) = (z.re, z.im);
    // the branch cut: the box meets x <= 0, y = 0
    if x.0 <= 0.0 && y.0 <= 0.0 && y.1 >= 0.0 {
        return None;
    }
    let near = |a: Iv| {
        if a.0 > 0.0 {
            a.0
        } else if a.1 < 0.0 {
            -a.1
        } else {
            0.0
        }
    };
    let rmin = near(x).hypot(near(y));
    let rmax = x.0.abs().max(x.1.abs()).hypot(y.0.abs().max(y.1.abs()));
    if rmin <= 0.0 || !rmax.is_finite() {
        return None;
    }
    let mut args = vec![];
    for a in [x.0, x.1] {
        for b in [y.0, y.1] {
            args.push(b.atan2(a));
        }
    }
    let alo = args.iter().copied().fold(f64::INFINITY, f64::min);
    let ahi = args.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let im = if is_real(z) { (0.0, 0.0) } else { widen((alo, ahi)) };
    Some(CBox { re: widen((rmin.ln(), rmax.ln())), im })
}

fn bexp(z: CBox) -> Option<CBox> {
    let m = iexp(z.re);
    if is_real(z) {
        return Some(CBox { re: m, im: (0.0, 0.0) });
    }
    Some(CBox { re: imul(m, icos(z.im)?), im: imul(m, isin(z.im)?) })
}

fn bpow(z: CBox, w: CBox, w_int: Option<i64>) -> Option<CBox> {
    // a small positive integer exponent: repeated products (no branch cut)
    if let Some(n) = w_int.filter(|n| (1..=64).contains(n)) {
        let mut acc = z;
        for _ in 1..n {
            acc = bmul(acc, z);
        }
        return Some(acc);
    }
    if is_real(z) && is_real(w) && z.re.0 > 0.0 {
        let mut vals = vec![];
        for a in [z.re.0, z.re.1] {
            for b in [w.re.0, w.re.1] {
                vals.push(a.powf(b));
            }
        }
        let lo = vals.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        return Some(CBox { re: widen((lo, hi)), im: (0.0, 0.0) });
    }
    bexp(bmul(w, blog(z)?))
}

fn parts(n: &Num) -> Option<(f64, f64)> {
    match n {
        Num::Real(r) => Some((r.to_f64(), 0.0)),
        Num::Complex(a, b) => Some((a.to_f64(), b.to_f64())),
    }
}

/// The real and imaginary parts of a number or a closed numeric term, as machine numbers.
fn number<'h>(fx: &mut Fx<'_, '_, '_, 'h>, t: T<'h>) -> Res<Option<(f64, f64)>> {
    if let Some(n) = fx.num(t) {
        return Ok(parts(&n));
    }
    let v = fx.eval(fx.call("n", &[t]))?;
    Ok(fx.num(v).as_ref().and_then(parts))
}

/// The hull [lo, hi] of an `interval` term's pieces.
fn hull<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<(T<'h>, T<'h>)> {
    let ps = fx.call_of(t, "interval")?;
    let end = |p: T<'h>, i: usize| fx.as_list(p).and_then(|l| if l.len() == 2 { Some(l[i]) } else { None });
    Some((end(*ps.first()?, 0)?, end(*ps.last()?, 1)?))
}

/// The box of an argument: a centered interval, the hull of an interval, a number.
fn to_box<'h>(fx: &mut Fx<'_, '_, '_, 'h>, t: T<'h>) -> Res<Option<CBox>> {
    if let Some(&[c, r]) = fx.call_of(t, "centered_interval") {
        let (Some(c), Some(r)) = (number(fx, c)?, number(fx, r)?) else { return Ok(None) };
        // a real radius with a complex centre: the same radius in both parts
        let (rx, ry) = if r.1 == 0.0 { (r.0.abs(), if c.1 == 0.0 { 0.0 } else { r.0.abs() }) } else { (r.0.abs(), r.1.abs()) };
        let side = |m: f64, d: f64| if d == 0.0 { widen((m, m)) } else { (down(m - d), up(m + d)) };
        let im = if c.1 == 0.0 && ry == 0.0 { (0.0, 0.0) } else { side(c.1, ry) };
        return Ok(Some(CBox { re: side(c.0, rx), im }));
    }
    if let Some((lo, hi)) = hull(fx, t) {
        let (Some(lo), Some(hi)) = (number(fx, lo)?, number(fx, hi)?) else { return Ok(None) };
        if lo.1 != 0.0 || hi.1 != 0.0 || !lo.0.is_finite() || !hi.0.is_finite() {
            return Ok(None);
        }
        return Ok(Some(CBox { re: (down(lo.0), up(hi.0)), im: (0.0, 0.0) }));
    }
    // a number: exact integers that fit are points, others widen by their rounding
    let exact = matches!(fx.h.view(t), View::Int(v) if v.unsigned_abs() < (1u64 << 53));
    Ok(number(fx, t)?.map(|(a, b)| {
        let side = |x: f64| if exact { (x, x) } else { widen((x, x)) };
        CBox { re: side(a), im: if b == 0.0 { (0.0, 0.0) } else { side(b) } }
    }))
}

fn mid_rad((a, b): Iv) -> (f64, f64) {
    let m = 0.5 * (a + b);
    let r = (b - m).max(m - a);
    (m, if r == 0.0 { 0.0 } else { up(r) })
}

fn to_term<'h>(fx: &Fx<'_, '_, '_, 'h>, b: CBox) -> Option<T<'h>> {
    if ![b.re.0, b.re.1, b.im.0, b.im.1].iter().all(|x| x.is_finite()) {
        return None;
    }
    let (mr, rr) = mid_rad(b.re);
    if is_real(b) {
        return Some(fx.call("centered_interval", &[fx.real(mr), fx.real(rr)]));
    }
    let (mi, ri) = mid_rad(b.im);
    let c = fx.ar.complex(fx.real(mr), fx.real(mi));
    let r = fx.ar.complex(fx.real(rr), fx.real(ri));
    Some(fx.call("centered_interval", &[c, r]))
}

/// Has the argument (a centered interval's centre or radius, or a number) a nonzero imaginary part?
fn complex_parts<'h>(fx: &mut Fx<'_, '_, '_, 'h>, t: T<'h>) -> Res<bool> {
    if fx.call_of(t, "interval").is_some() {
        return Ok(false);
    }
    let parts: Vec<T<'h>> = match fx.call_of(t, "centered_interval") {
        Some(a) => a.to_vec(),
        None => vec![t],
    };
    for p in parts {
        if let Some((_, b)) = number(fx, p)? {
            if b != 0.0 {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// `add`, `mul` or `pow` (the call `t`) with a centered interval among `args` (`kinds`: 1 interval, 2 centered
/// interval), when a part is complex, an `interval` is mixed in, or a power has an uncertain exponent: the enclosing
/// centered interval; `None` leaves the call to the other rules (or unevaluated).
pub(crate) fn arith<'h>(fx: &mut Fx<'_, '_, '_, 'h>, t: T<'h>, args: &[T<'h>], kinds: &[Option<usize>]) -> Res<Option<T<'h>>> {
    let View::App { head, .. } = fx.h.view(t) else { return Ok(None) };
    let View::Sym(s) = fx.h.view(head) else { return Ok(None) };
    let name = fx.h.sym_name(s);
    if name != names::ADD && name != names::MUL && name != names::POW {
        return Ok(None);
    }
    if !kinds.contains(&Some(2)) || kinds.iter().flatten().any(|&k| k != 1 && k != 2) {
        return Ok(None);
    }
    let mixed = kinds.contains(&Some(1));
    let uncertain_exponent = name == names::POW && args.len() == 2 && kinds[1].is_some();
    let mut complex = false;
    for &a in args {
        complex |= complex_parts(fx, a)?;
    }
    if !(mixed || uncertain_exponent || complex) {
        return Ok(None);
    }
    let mut bs = vec![];
    for &a in args {
        match to_box(fx, a)? {
            Some(b) => bs.push(b),
            None => return Ok(None),
        }
    }
    let r = if name == names::POW {
        let [z, w] = bs[..] else { return Ok(None) };
        let w_int = if kinds[1].is_none() { fx.int_of(args[1]) } else { None };
        bpow(z, w, w_int)
    } else {
        let op = if name == names::ADD { badd } else { bmul };
        Some(bs[1..].iter().fold(bs[0], |acc, &b| op(acc, b)))
    };
    Ok(r.and_then(|b| to_term(fx, b)))
}
