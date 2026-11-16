//! Centered intervals through complex-ball enclosures: a function of `centered_interval` arguments whose centre
//! or radius is complex (or whose value is complex, as elliptic_k beyond 1) is the function of the rectangular boxes
//! [re c -+ rx] x [im c -+ ry] (a real radius r of a complex centre is r in both parts, a real centre with a real
//! radius has the exact imaginary part 0), each box a complex ball with a midpoint and a radius per part.
//! Functions with an enclosure on such balls register it here ([`set_hook`]; the special-function crate does for its
//! functions): ball arithmetic is inclusion-isotone, so the result contains the value at every point of the boxes
//! (Moore, Kearfott and Cloud, "Introduction to Interval Analysis", SIAM 2009, ch. 5; Johansson, "Arb: efficient
//! arbitrary-precision midpoint-radius interval arithmetic", IEEE Trans. Comput. 66 (2017)). The resulting ball is
//! written back as a centered interval of machine numbers rounded outwards.
use crate::cball::CBall;
use crate::formula::Fx;
use mc_api::{Store, View, T};
use mc_arith::num::{Ball, Float, Mag};
use mc_arith::{Kind, Num, Real};
use mc_eval::Res;
use std::sync::OnceLock;

/// An enclosure of the named function (`None` as the third argument) or of its derivative in the given argument on
/// complex balls at a precision in bits; `None` for an unknown name or where none is proved (a pole or a branch cut
/// in a box).
pub type Hook = fn(&str, &[CBall], Option<usize>, u64) -> Option<CBall>;

static HOOK: OnceLock<Hook> = OnceLock::new();

/// Install the enclosure provider (once; later calls are ignored).
pub fn set_hook(f: Hook) {
    let _ = HOOK.set(f);
}

/// The working precision of the enclosures.
const P: u64 = 128;
/// The largest radius (in each part) taken: a wide box gives a loose enclosure, and the truncation bounds of some
/// kernels are computed at the midpoint with a margin meant for small radii.
const MAX_RADIUS: f64 = 0.25;

fn float_of(v: f64) -> Option<Float> {
    mc_arith::num::float::f64_to_dyadic(v).map(|(neg, man, exp)| Float::new(neg, man, exp))
}

fn f64_of(f: &Float) -> f64 {
    mc_arith::num::float::dyadic_to_f64(f.neg, &f.man, f.exp)
}

/// A real number as a ball (a machine number stands for its exact binary value).
fn ball_of(r: &Real) -> Option<Ball> {
    match r {
        Real::Machine(v) => v.is_finite().then(|| float_of(*v).map(Ball::exact)).flatten(),
        _ => match r.to_kind(Kind::Ball, P) {
            Real::Ball(b, _) => Some(b),
            _ => None,
        },
    }
}

/// An upper bound of |r| as a magnitude.
fn mag_of(r: &Real) -> Option<Mag> {
    let b = ball_of(r)?;
    Some(Mag::from_float_up(&b.mid).add(&b.rad))
}

/// The parts of a number.
fn parts(n: Num) -> (Real, Option<Real>) {
    match n {
        Num::Real(r) => (r, None),
        Num::Complex(a, b) => (a, Some(b)),
    }
}

/// A number as a complex ball.
fn num_ball(n: Num) -> Option<CBall> {
    let (re, im) = parts(n);
    let im = match im {
        Some(i) => ball_of(&i)?,
        None => Ball::from_i64(0),
    };
    Some(CBall::new(ball_of(&re)?, im))
}

/// An argument as a complex ball (a centered interval as its box, a number as itself), its centre as a ball, whether
/// it carries a complex part and whether it has a spread; `None` for anything else.
fn arg_ball<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<(CBall, CBall, bool, bool)> {
    let Some(&[c, r]) = fx.call_of(t, "centered_interval") else {
        let n = fx.num(t)?;
        let complex = matches!(n, Num::Complex(..));
        let b = num_ball(n)?;
        return Some((b.clone(), b, complex, false));
    };
    let centre = num_ball(fx.num(c)?)?;
    let (cre, cim) = parts(fx.num(c)?);
    let (rre, rim) = parts(fx.num(r)?);
    let complex = cim.is_some() || rim.is_some();
    let rx = mag_of(&rre)?;
    // a real radius of a complex centre is the radius of both parts
    let ry = match (&rim, &cim) {
        (Some(i), _) => mag_of(i)?,
        (None, Some(_)) => rx,
        (None, None) => Mag::ZERO,
    };
    let small = |m: &Mag| m.is_zero() || m.log2() <= MAX_RADIUS.log2();
    if !(small(&rx) && small(&ry)) {
        return None;
    }
    let mut re = ball_of(&cre)?;
    re.rad = re.rad.add(&rx);
    let mut im = match cim {
        Some(i) => ball_of(&i)?,
        None => Ball::from_i64(0),
    };
    im.rad = im.rad.add(&ry);
    let spread = !(rx.is_zero() && ry.is_zero());
    Some((CBall::new(re, im), centre, complex, spread))
}

/// The ends m -+ r of a ball (exact).
fn ends(b: &Ball) -> (Float, Float) {
    let r = b.rad.to_float();
    (b.mid.sub_exact(&r), b.mid.add_exact(&r))
}

/// The ball of [lo, hi] (exact midpoint, radius rounded up).
fn from_ends(lo: &Float, hi: &Float) -> Ball {
    let s = lo.add_exact(hi);
    let mid = Float::new(s.neg, s.man, s.exp - 1);
    let d = hi.sub_exact(lo);
    Ball { mid, rad: Mag::from_float_up(&Float::new(false, d.man, d.exp - 1)) }
}

/// The intersection of two enclosures of the same real number (the first when they do not meet: rounding).
fn meet(a: &Ball, b: &Ball) -> Ball {
    let ((a0, a1), (b0, b1)) = (ends(a), ends(b));
    let lo = if a0.cmp_exact(&b0).is_ge() { a0 } else { b0 };
    let hi = if a1.cmp_exact(&b1).is_le() { a1 } else { b1 };
    if lo.cmp_exact(&hi).is_gt() {
        return a.clone();
    }
    from_ends(&lo, &hi)
}

/// A ball as a machine midpoint and a radius rounded up so that [m - r, m + r] contains it.
fn mid_rad(b: &Ball) -> Option<(f64, f64)> {
    let m = f64_of(&b.mid);
    let fm = float_of(m)?;
    let shift = f64_of(&b.mid.sub_exact(&fm).abs());
    let r = (f64_of(&b.rad.to_float()) + shift) * (1.0 + 1e-15) + f64::from_bits(1);
    (m.is_finite() && r.is_finite()).then_some((m, r))
}

/// f(args) for the call `t` with centered-interval arguments by the registered complex-ball enclosure: the
/// enclosing centered interval, or `None` (no hook, an unknown function, an argument that is neither a number nor a
/// centered interval, a radius above [`MAX_RADIUS`], no enclosure). With `need_complex` only calls with a complex
/// centre or radius are taken.
pub(crate) fn apply<'h>(fx: &mut Fx<'_, '_, '_, 'h>, t: T<'h>, args: &[T<'h>], need_complex: bool) -> Res<Option<T<'h>>> {
    let Some(hook) = HOOK.get() else { return Ok(None) };
    let View::App { head, .. } = fx.h.view(t) else { return Ok(None) };
    let View::Sym(s) = fx.h.view(head) else { return Ok(None) };
    let name = fx.h.sym_name(s);
    let mut balls = Vec::with_capacity(args.len());
    let mut centres = Vec::with_capacity(args.len());
    let mut spread = vec![];
    let mut complex = false;
    for (i, &a) in args.iter().enumerate() {
        let Some((b, c, cx, sp)) = arg_ball(fx, a) else { return Ok(None) };
        complex |= cx;
        if sp {
            spread.push(i);
        }
        balls.push(b);
        centres.push(c);
    }
    if need_complex && !complex {
        return Ok(None);
    }
    fx.tick(4)?;
    // the natural enclosure over the boxes, met with the mean value form f(c) + f'(B) (B - c) when one argument is
    // spread (the segment from c to any point of the convex box B stays in B, so f(z) - f(c) is z - c times an
    // average of f' over it, inside the convex enclosure of f'(B)): the natural form widens by the dependency of
    // the terms of a series, the mean value form only to second order in the radius
    let natural = hook(&name, &balls, None, P);
    let mvf = match spread[..] {
        [i] => (|| {
            let fc = hook(&name, &centres, None, P)?;
            let d = hook(&name, &balls, Some(i), P)?;
            Some(fc.add(&d.mul(&balls[i].sub(&centres[i], P), P), P))
        })(),
        _ => None,
    };
    let v = match (natural, mvf) {
        (Some(a), Some(b)) => CBall::new(meet(&a.re, &b.re), meet(&a.im, &b.im)),
        (Some(a), None) => a,
        // no natural enclosure: a pole or a cut may be inside, where the mean value form does not hold
        (None, _) => return Ok(None),
    };
    let Some((mr, rr)) = mid_rad(&v.re) else { return Ok(None) };
    if v.im.mid.is_zero() && v.im.rad.is_zero() {
        return Ok(Some(fx.call("centered_interval", &[fx.real(mr), fx.real(rr)])));
    }
    let Some((mi, ri)) = mid_rad(&v.im) else { return Ok(None) };
    let c = fx.ar.complex(fx.real(mr), fx.real(mi));
    let r = fx.ar.complex(fx.real(rr), fx.real(ri));
    Ok(Some(fx.call("centered_interval", &[c, r])))
}
