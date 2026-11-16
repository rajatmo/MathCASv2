//! Rectangular box arithmetic for `centered_interval`: a centered interval with a complex centre c and a
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

/// sinh over [a, b] (increasing).
fn isinh(a: Iv) -> Option<Iv> {
    let r = widen((a.0.sinh(), a.1.sinh()));
    (r.0.is_finite() && r.1.is_finite()).then_some(r)
}

/// cosh over [a, b]: 1 at 0 when 0 is inside, else the end values.
fn icosh(a: Iv) -> Option<Iv> {
    let (p, q) = (a.0.cosh(), a.1.cosh());
    let lo = if a.0 <= 0.0 && a.1 >= 0.0 { 1.0 } else { p.min(q) };
    let r = widen((lo, p.max(q)));
    (r.1.is_finite()).then_some((r.0.max(1.0), r.1))
}

/// a / b for an interval b of one sign.
fn idiv(a: Iv, b: Iv) -> Option<Iv> {
    if !(b.0 > 0.0 || b.1 < 0.0) {
        return None;
    }
    Some(imul(a, widen((1.0 / b.1, 1.0 / b.0))))
}

fn bscale(z: CBox, k: f64) -> CBox {
    // multiplication by a power of two is exact
    let s = |a: Iv| if k > 0.0 { (a.0 * k, a.1 * k) } else { (a.1 * k, a.0 * k) };
    CBox { re: s(z.re), im: s(z.im) }
}

fn bneg(z: CBox) -> CBox {
    bscale(z, -1.0)
}

/// i z
fn btimes_i(z: CBox) -> CBox {
    CBox { re: (-z.im.1, -z.im.0), im: z.re }
}

fn bconst(re: f64, im: f64) -> CBox {
    CBox { re: (re, re), im: (im, im) }
}

/// The principal square root, exp(log(z)/2), for a box away from 0 and the negative real axis.
fn bsqrt(z: CBox) -> Option<CBox> {
    bexp(bscale(blog(z)?, 0.5))
}

/// The elementary functions of one complex box, each part enclosed by interval arithmetic on the parts:
/// sin(x + iy) = sin x cosh y + i cos x sinh y, cos(x + iy) = cos x cosh y - i sin x sinh y, sinh and cosh alike,
/// tan(x + iy) = (sin 2x + i sinh 2y)/(cos 2x + cosh 2y), tanh(x + iy) = (sinh 2x + i sin 2y)/(cosh 2x + cos 2y)
/// (DLMF 4.21.37-38, 4.35.34-35 and the quotient forms), and the inverse functions by their principal logarithmic
/// forms (DLMF 4.23.16-26 and 4.37.16-26: asin z = -i log(iz + sqrt(1 - z^2)), atan z = (i/2)(log(1 - iz) -
/// log(1 + iz)), asinh z = log(z + sqrt(z^2 + 1)), acosh z = log(z + sqrt(z + 1) sqrt(z - 1)), atanh z = (log(1 + z) -
/// log(1 - z))/2), each logarithm and square root over a box clear of its cut (else `None`); cot and coth by the
/// quotient forms (sin 2x - i sinh 2y)/(cosh 2y - cos 2x) and (sinh 2x - i sin 2y)/(cosh 2x - cos 2y), sec, csc, sech,
/// csch as reciprocals, and acot, asec, acsc, acoth, asech, acsch as atan, acos, asin, atanh, acosh, asinh of 1/z.
fn bfunction(name: &str, z: CBox) -> Option<CBox> {
    let (x, y) = (z.re, z.im);
    let one = bconst(1.0, 0.0);
    Some(match name {
        "sin" => CBox { re: imul(isin(x)?, icosh(y)?), im: imul(icos(x)?, isinh(y)?) },
        "cos" => {
            let im = imul(isin(x)?, isinh(y)?);
            CBox { re: imul(icos(x)?, icosh(y)?), im: (-im.1, -im.0) }
        }
        "sinh" => CBox { re: imul(isinh(x)?, icos(y)?), im: imul(icosh(x)?, isin(y)?) },
        "cosh" => CBox { re: imul(icosh(x)?, icos(y)?), im: imul(isinh(x)?, isin(y)?) },
        "exp" => bexp(z)?,
        "tan" => {
            let (x2, y2) = ((2.0 * x.0, 2.0 * x.1), (2.0 * y.0, 2.0 * y.1));
            let d = iadd(icos(x2)?, icosh(y2)?);
            CBox { re: idiv(isin(x2)?, d)?, im: idiv(isinh(y2)?, d)? }
        }
        "tanh" => {
            let (x2, y2) = ((2.0 * x.0, 2.0 * x.1), (2.0 * y.0, 2.0 * y.1));
            let d = iadd(icosh(x2)?, icos(y2)?);
            CBox { re: idiv(isinh(x2)?, d)?, im: idiv(isin(y2)?, d)? }
        }
        "log" => blog(z)?,
        "sqrt" => bsqrt(z)?,
        "asin" => {
            let s = bsqrt(badd(one, bneg(bmul(z, z))))?;
            let l = blog(badd(btimes_i(z), s))?;
            bneg(btimes_i(l))
        }
        "acos" => {
            let s = bsqrt(badd(one, bneg(bmul(z, z))))?;
            let a = bneg(btimes_i(blog(badd(btimes_i(z), s))?));
            let h = std::f64::consts::FRAC_PI_2;
            badd(CBox { re: widen((h, h)), im: (0.0, 0.0) }, bneg(a))
        }
        "atan" => {
            let iz = btimes_i(z);
            let d = badd(blog(badd(one, bneg(iz)))?, bneg(blog(badd(one, iz))?));
            bscale(btimes_i(d), 0.5)
        }
        "asinh" => blog(badd(z, bsqrt(badd(bmul(z, z), one))?))?,
        "acosh" => blog(badd(z, bmul(bsqrt(badd(z, one))?, bsqrt(badd(z, bneg(one)))?)))?,
        "atanh" => bscale(badd(blog(badd(one, z))?, bneg(blog(badd(one, bneg(z)))?)), 0.5),
        "cot" => {
            let (x2, y2) = ((2.0 * x.0, 2.0 * x.1), (2.0 * y.0, 2.0 * y.1));
            let d = isub(icosh(y2)?, icos(x2)?);
            let im = idiv(isinh(y2)?, d)?;
            CBox { re: idiv(isin(x2)?, d)?, im: (-im.1, -im.0) }
        }
        "coth" => {
            let (x2, y2) = ((2.0 * x.0, 2.0 * x.1), (2.0 * y.0, 2.0 * y.1));
            let d = isub(icosh(x2)?, icos(y2)?);
            let im = idiv(isin(y2)?, d)?;
            CBox { re: idiv(isinh(x2)?, d)?, im: (-im.1, -im.0) }
        }
        "sec" => brecip(bfunction("cos", z)?)?,
        "csc" => brecip(bfunction("sin", z)?)?,
        "sech" => brecip(bfunction("cosh", z)?)?,
        "csch" => brecip(bfunction("sinh", z)?)?,
        // the inverse reciprocal functions: F(1/z) (acot z = atan(1/z), ...)
        _ => bfunction(reciprocal_base(name)?, brecip(z)?)?,
    })
}

/// The inverse function F with acot z = F(1/z) and alike.
fn reciprocal_base(name: &str) -> Option<&'static str> {
    Some(match name {
        "acot" => "atan",
        "asec" => "acos",
        "acsc" => "asin",
        "acoth" => "atanh",
        "asech" => "acosh",
        "acsch" => "asinh",
        _ => return None,
    })
}

/// x^2 over [a, b] (0 when 0 is inside).
fn isqr(a: Iv) -> Iv {
    let (p, q) = (a.0 * a.0, a.1 * a.1);
    let lo = if a.0 <= 0.0 && a.1 >= 0.0 { 0.0 } else { p.min(q) };
    let (l, h) = widen((lo, p.max(q)));
    (l.max(0.0), h)
}

/// 1/z = conj(z)/|z|^2 for a box away from 0.
fn brecip(z: CBox) -> Option<CBox> {
    let n = iadd(isqr(z.re), isqr(z.im));
    let im = idiv(z.im, n)?;
    Some(CBox { re: idiv(z.re, n)?, im: (-im.1, -im.0) })
}

/// The derivative of `name` over a box (the forms of [`bfunction`]), `None` near a cut, a pole or for another name.
fn bderiv(name: &str, z: CBox) -> Option<CBox> {
    let one = bconst(1.0, 0.0);
    Some(match name {
        "sin" => bfunction("cos", z)?,
        "cos" => bneg(bfunction("sin", z)?),
        "sinh" => bfunction("cosh", z)?,
        "cosh" => bfunction("sinh", z)?,
        "exp" => bexp(z)?,
        "tan" => {
            let t = bfunction("tan", z)?;
            badd(one, bmul(t, t))
        }
        "tanh" => {
            let t = bfunction("tanh", z)?;
            badd(one, bneg(bmul(t, t)))
        }
        "log" => brecip(z)?,
        "sqrt" => brecip(bscale(bsqrt(z)?, 2.0))?,
        "asin" => brecip(bsqrt(badd(one, bneg(bmul(z, z))))?)?,
        "acos" => bneg(brecip(bsqrt(badd(one, bneg(bmul(z, z))))?)?),
        "atan" => brecip(badd(one, bmul(z, z)))?,
        "asinh" => brecip(bsqrt(badd(bmul(z, z), one))?)?,
        "acosh" => brecip(bmul(bsqrt(badd(z, one))?, bsqrt(badd(z, bneg(one)))?))?,
        "atanh" => brecip(badd(one, bneg(bmul(z, z))))?,
        "cot" => {
            let t = bfunction("cot", z)?;
            bneg(badd(one, bmul(t, t)))
        }
        "coth" => {
            let t = bfunction("coth", z)?;
            badd(one, bneg(bmul(t, t)))
        }
        "sec" => bmul(bfunction("sec", z)?, bfunction("tan", z)?),
        "csc" => bneg(bmul(bfunction("csc", z)?, bfunction("cot", z)?)),
        "sech" => bneg(bmul(bfunction("sech", z)?, bfunction("tanh", z)?)),
        "csch" => bneg(bmul(bfunction("csch", z)?, bfunction("coth", z)?)),
        // d/dz F(1/z) = -F'(1/z) / z^2
        _ => {
            let base = reciprocal_base(name)?;
            let w = brecip(z)?;
            bneg(bmul(bderiv(base, w)?, brecip(bmul(z, z))?))
        }
    })
}

/// The intersection of two enclosures of the same set.
fn bmeet(a: CBox, b: CBox) -> CBox {
    let meet = |p: Iv, q: Iv| {
        let r = (p.0.max(q.0), p.1.min(q.1));
        if r.0 <= r.1 {
            r
        } else {
            p
        }
    };
    CBox { re: meet(a.re, b.re), im: meet(a.im, b.im) }
}

/// The value over a box: the direct box evaluation (which also checks the cuts), met with the mean value form
/// f(c) + f'(box) (box - c) (for an analytic f on the convex box, f(z) - f(c) = (z - c) times an average of f' over
/// the segment from c to z, which lies in any convex enclosure of f' over the box; Moore, Kearfott and Cloud,
/// section 6.4): tight to first order where the direct form suffers from dependency (asin through log and sqrt).
fn bvalue(name: &str, z: CBox) -> Option<CBox> {
    let direct = bfunction(name, z)?;
    let mid = |a: Iv| 0.5 * (a.0 + a.1);
    let c = CBox { re: widen((mid(z.re), mid(z.re))), im: if is_real(z) { (0.0, 0.0) } else { widen((mid(z.im), mid(z.im))) } };
    let mvf = (|| {
        let fc = bfunction(name, c)?;
        let d = bderiv(name, z)?;
        let dz = badd(z, bneg(c));
        Some(badd(fc, bmul(d, dz)))
    })();
    Some(match mvf {
        Some(m) if [m.re.0, m.re.1, m.im.0, m.im.1].iter().all(|x| x.is_finite()) => bmeet(direct, m),
        _ => direct,
    })
}

/// A function of one centered interval with a complex part (the box of its centre and radius) among the elementary
/// functions of [`bfunction`]: the enclosing centered interval, `None` for anything else.
pub(crate) fn function<'h>(fx: &mut Fx<'_, '_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let View::App { head, .. } = fx.h.view(t) else { return Ok(None) };
    let View::Sym(s) = fx.h.view(head) else { return Ok(None) };
    let name = fx.h.sym_name(s);
    let [a] = args else { return Ok(None) };
    if fx.call_of(*a, "centered_interval").is_none() || !complex_parts(fx, *a)? {
        return Ok(None);
    }
    let Some(z) = to_box(fx, *a)? else { return Ok(None) };
    Ok(bvalue(&name, z).and_then(|b| to_term(fx, b)))
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

/// `re`, `im_part`, `conjugate`, `abs` and `arg` of one centered interval with a complex part (the box of its centre
/// and radius): the real part and the imaginary part are the box's sides, the conjugate the box mirrored; the modulus
/// lies between the nearest point of the box and its farthest corner, the argument between the arguments of the
/// corners (for a box away from 0 and from the negative real axis: the extreme rays from 0 that touch a convex set not
/// holding 0 pass through corners). `None` for anything else.
pub(crate) fn part<'h>(fx: &mut Fx<'_, '_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let View::App { head, .. } = fx.h.view(t) else { return Ok(None) };
    let View::Sym(s) = fx.h.view(head) else { return Ok(None) };
    let name = fx.h.sym_name(s);
    if !matches!(name.as_str(), "re" | "im_part" | "conjugate" | "abs" | "arg") {
        return Ok(None);
    }
    let [a] = args else { return Ok(None) };
    if fx.call_of(*a, "centered_interval").is_none() || !complex_parts(fx, *a)? {
        return Ok(None);
    }
    let Some(z) = to_box(fx, *a)? else { return Ok(None) };
    let real = |iv: Iv| CBox { re: iv, im: (0.0, 0.0) };
    let r = match name.as_str() {
        "re" => Some(real(z.re)),
        "im_part" => Some(real(z.im)),
        "conjugate" => Some(CBox { re: z.re, im: (-z.im.1, -z.im.0) }),
        "abs" => {
            let near = |a: Iv| if a.0 > 0.0 { a.0 } else if a.1 < 0.0 { -a.1 } else { 0.0 };
            let rmin = near(z.re).hypot(near(z.im));
            let rmax = z.re.0.abs().max(z.re.1.abs()).hypot(z.im.0.abs().max(z.im.1.abs()));
            Some(real((down(rmin).max(0.0), up(rmax))))
        }
        _ => blog(z).map(|l| real(l.im)),
    };
    Ok(r.and_then(|b| to_term(fx, b)))
}
