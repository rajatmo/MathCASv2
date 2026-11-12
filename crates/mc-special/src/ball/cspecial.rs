//! More complex-ball enclosures (round 22): the Kummer and Gauss hypergeometric functions with any complex
//! parameters and argument, the exponential, sine, cosine and logarithmic integrals, the incomplete gamma and beta
//! functions, the beta function and the complete elliptic integrals K and E, all as sums of the series of
//! [`super::cseries`] (whose truncation bounds are rigorous) so that `n(f(z), 30)` at an exact complex argument
//! reaches its digits instead of failing with `precision_not_reached`.
//!
//! - **0F1, 1F1** are the series themselves (any argument). **2F1** picks the argument of smallest modulus among
//!   z, z/(z-1), 1/z, 1-z, 1/(1-z), 1-1/z and applies the matching Pfaff or connection formula (Abramowitz and
//!   Stegun 15.3.3, 15.3.7, 15.3.8; DLMF 15.8.2, 15.8.4, 15.8.7). The connection formulas are singular when b - a or
//!   c - a - b is an integer: there the formula is evaluated at four parameter shifts +-e, +-2e (e = 2^-(p/4+8))
//!   and combined by the fourth-order Richardson step (4 (f(e) + f(-e)) - (f(2e) + f(-2e)))/6, whose error is
//!   O(e^4) = 2^-(p+32); this limit is not rigorous (the radius adds 2^-p of the value), everything else is.
//!   A real argument above 1 is the limit from below (Im z = -0), as the principal logarithm of 1 - z.
//! - **Integrals** (DLMF 6.6.1-6.6.5 as series): Ei = gamma + log z + z 2F2(1, 1; 2, 2; z), Si = z 1F2(1/2; 3/2, 3/2;
//!   -z^2/4), Ci = gamma + log z - z^2/4 2F3(1, 1; 2, 2, 3/2; -z^2/4), Shi and Chi likewise; E_n by E_1 =
//!   -gamma - log z + z 2F2(1, 1; 2, 2; -z) and the recurrence n E_(n+1) = e^-z - z E_n; li(z) = Ei(log z).
//! - **Incomplete gamma** (DLMF 8.5.1, 8.4.15): Gamma(a, z) = Gamma(a) - z^a/a 1F1(a; a + 1; -z); at a nonpositive
//!   integer Gamma(-m, z) = z^-m E_(m+1)(z). Regularised: times 1/Gamma(a). The incomplete beta function by
//!   B_z(a, b) = z^a/a 2F1(a, 1 - b; a + 1; z).
//! - **Elliptic K, E** (DLMF 19.5.1, 19.5.2): pi/2 2F1(1/2, 1/2; 1; m) and pi/2 2F1(-1/2, 1/2; 1; m); beyond m = 1
//!   the value on the side Im m = -0.
use super::cseries::{guard_for, int_of, nonpos_int, pfq_sum, rgamma};
use super::{euler_gamma, pi, refine};
use mc_arith::num::{Ball, Mag};
use mc_numeric::cball::{self, CBall};

fn c(v: i64) -> CBall {
    CBall::from_i64(v)
}

fn rat(n: i64, d: i64, wp: u64) -> CBall {
    CBall::real(Ball::from_ratio(n, d, wp))
}

fn cgamma(z: &CBall, wp: u64) -> Option<CBall> {
    if nonpos_int(z).is_some() {
        return None;
    }
    if z.is_real() {
        super::gamma::gamma_ball(&z.re, wp).map(CBall::real)
    } else {
        super::complex::gamma(z, wp)
    }
}

fn eg(wp: u64) -> Option<CBall> {
    euler_gamma(wp).map(CBall::real)
}

// ------------------------------------------------------------------ 0F1, 1F1

/// 0F1(; b; z).
pub fn hyp0f1(b: &CBall, z: &CBall, p: u64) -> Option<CBall> {
    if nonpos_int(b).is_some() {
        return None;
    }
    let bb = [b.clone()];
    let g = guard_for(&[], &bb, z);
    refine(p, g, |wp| pfq_sum(&[], &bb, z, wp))
}

/// 1F1(a; b; z), also the regularised value's numerator.
pub fn hyp1f1(a: &CBall, b: &CBall, z: &CBall, p: u64) -> Option<CBall> {
    if nonpos_int(b).is_some() {
        return None;
    }
    let (aa, bb) = ([a.clone()], [b.clone()]);
    let g = guard_for(&aa, &bb, z);
    refine(p, g, |wp| pfq_sum(&aa, &bb, z, wp))
}

/// pFq(a; b; z) on complex balls by the series with its rigorous tail bound (round 37): any z for p <= q and the
/// polynomial cases; p = q + 1 inside the disk where the series converges (`pfq_sum` declines otherwise).
pub fn hyp_pfq(a: &[CBall], b: &[CBall], z: &CBall, p: u64) -> Option<CBall> {
    if b.iter().any(|x| nonpos_int(x).is_some()) {
        return None;
    }
    let g = guard_for(a, b, z);
    refine(p, g, |wp| pfq_sum(a, b, z, wp))
}

// ------------------------------------------------------------------ 2F1

fn dist_to_int(x: &CBall) -> bool {
    // does the ball contain an integer? (an exact integer, or a ball such as 0 +- 2^-116 from the exact difference
    // of two exact parameters converted to balls: B26-18)
    if int_of(x).is_some() {
        return true;
    }
    let (xr, xi) = super::cseries::approx(x);
    let rad = |b: &Ball| super::f64_of(&b.rad.to_float());
    (xr - xr.round()).abs() <= rad(&x.re) * 1.0001 + 1e-300 && xi.abs() <= rad(&x.im) * 1.0001 + 1e-300 && rad(&x.re) < 1e-6
}

/// The parameter shift for the degenerate connection formulas: e = 2^-(p/4 + 8).
fn shift(p: u64) -> Ball {
    Ball::exact(mc_arith::num::Float::new(false, mc_arith::num::Nat::from_u64(1), -((p / 4 + 8) as i64)))
}

/// f at four shifts combined to fourth order, f(e) the parameter shifted function.
fn extrapolate(p: u64, wp: u64, f: impl Fn(&CBall) -> Option<CBall>) -> Option<CBall> {
    let e = shift(p);
    let e1 = CBall::real(e.clone());
    let e2 = CBall::real(e.mul(&Ball::from_i64(2), wp));
    let f1 = f(&e1)?.add(&f(&e1.neg())?, wp);
    let f2 = f(&e2)?.add(&f(&e2.neg())?, wp);
    let g = f1.mul_real(&Ball::from_i64(4), wp).sub(&f2, wp).mul_real(&Ball::from_ratio(1, 6, wp), wp);
    // not rigorous: add 2^-(p + 8) of each part (the error of the step is O(e^4) = 2^-(p + 32); the old 2^-p of the
    // modulus left a small imaginary part with 8 bits fewer than asked and `refine` doubled to its cap, B26-18)
    let rel = |b: &Ball| Mag::pow2(-(p as i64) - 8).mul(&super::mag_up(b));
    let (mr, mi) = (rel(&g.re), rel(&g.im));
    Some(CBall::new(super::widen(g.re, &mr), super::widen(g.im, &mi)))
}

fn mag_of(z: &CBall) -> Mag {
    super::mag_up(&z.re).add(&super::mag_up(&z.im))
}

fn pw(z: &CBall, e: &CBall, wp: u64) -> Option<CBall> {
    match int_of(e) {
        Some(n) => cball::pow_int(z, &mc_arith::num::Int::from_i64(n), wp),
        None => {
            // a real negative base z = -x (2F1 at a real argument above 1: (-z)^-a and (1 - z)^-a): the argument is the
            // limit from below, i.e. arg = +pi and z^e = x^e e^(i pi e) (B26-18: the ball power refuses the cut)
            if z.im.mid.is_zero() && z.im.rad.is_zero() && z.re.mid.neg && !z.re.contains_zero() {
                let mag = cball::pow(&z.neg(), e, wp)?;
                let phase = cball::exp(&e.mul_i().mul_real(&pi(wp)?, wp), wp)?;
                return Some(mag.mul(&phase, wp));
            }
            cball::pow(z, e, wp)
        }
    }
}

/// The 1/z-type connection: G(c) G(b-a) / (G(b) G(c-a)) w^-a 2F1(a, a-c+1; a-b+1; 1/w') + (a <-> b); `fac` is the
/// power factor ((-z)^-a or (1-z)^-a) and `arg` the series argument, `third` the series parameter c' for the first
/// term as a function (a, b, c) -> (a1, a2, b1).
fn connection(a: &CBall, b: &CBall, cc: &CBall, base: &CBall, arg: &CBall, kind: u8, wp: u64) -> Option<CBall> {
    let one = c(1);
    let mut total: Option<CBall> = None;
    for (x, y) in [(a, b), (b, a)] {
        // term in x with partner y
        let amb = y.sub(x, wp);
        let g = cgamma(cc, wp)?.mul(&cgamma(&amb, wp)?, wp);
        let g = g.mul(&rgamma(y, wp)?, wp).mul(&rgamma(&cc.sub(x, wp), wp)?, wp);
        let powf = pw(base, &x.neg(), wp)?;
        let (s1, s2, t) = match kind {
            // 1/z: 2F1(x, x-c+1; x-y+1; 1/z)
            0 => (x.clone(), x.sub(cc, wp).add(&one, wp), x.sub(y, wp).add(&one, wp)),
            // 1/(1-z): 2F1(x, c-y; x-y+1; 1/(1-z))
            _ => (x.clone(), cc.sub(y, wp), x.sub(y, wp).add(&one, wp)),
        };
        let f = pfq_sum(&[s1, s2], &[t], arg, wp)?;
        let term = g.mul(&powf, wp).mul(&f, wp);
        total = Some(match total {
            Some(t0) => t0.add(&term, wp),
            None => term,
        });
    }
    total
}

/// The 1-z-type connection (kind 0: argument 1 - z; kind 1: 1 - 1/z).
fn connection_one_minus(a: &CBall, b: &CBall, cc: &CBall, z: &CBall, kind: u8, wp: u64) -> Option<CBall> {
    let one = c(1);
    let w = if kind == 0 { one.sub(z, wp) } else { one.sub(&one.div(z, wp)?, wp) };
    let cab = cc.sub(a, wp).sub(b, wp);
    let g1 = cgamma(cc, wp)?.mul(&cgamma(&cab, wp)?, wp).mul(&rgamma(&cc.sub(a, wp), wp)?, wp).mul(&rgamma(&cc.sub(b, wp), wp)?, wp);
    let g2 = cgamma(cc, wp)?.mul(&cgamma(&cab.neg(), wp)?, wp).mul(&rgamma(a, wp)?, wp).mul(&rgamma(b, wp)?, wp);
    let omz = one.sub(z, wp);
    let (t1, t2) = if kind == 0 {
        let f1 = pfq_sum(&[a.clone(), b.clone()], &[cab.neg().add(&one, wp)], &w, wp)?;
        let f2 = pfq_sum(&[cc.sub(a, wp), cc.sub(b, wp)], &[cab.add(&one, wp)], &w, wp)?;
        (f1, pw(&omz, &cab, wp)?.mul(&f2, wp))
    } else {
        let f1 = pfq_sum(&[a.clone(), a.sub(cc, wp).add(&one, wp)], &[cab.neg().add(&one, wp)], &w, wp)?;
        let f2 = pfq_sum(&[cc.sub(a, wp), one.sub(a, wp)], &[cab.add(&one, wp)], &w, wp)?;
        let pre1 = pw(z, &a.neg(), wp)?;
        let pre2 = pw(&omz, &cab, wp)?.mul(&pw(z, &a.sub(cc, wp), wp)?, wp);
        (pre1.mul(&f1, wp), pre2.mul(&f2, wp))
    };
    Some(g1.mul(&t1, wp).add(&g2.mul(&t2, wp), wp))
}

/// 2F1(a, b; c; z) on complex balls (principal branch; a real z above 1 is the value from below).
pub fn hyp2f1(a: &CBall, b: &CBall, cc: &CBall, z: &CBall, p: u64) -> Option<CBall> {
    if let Some(m) = nonpos_int(cc) {
        // a series that ends before the pole of c
        if ![a, b].iter().filter_map(|x| nonpos_int(x)).any(|n| n < m) {
            return None;
        }
    }
    let one = c(1);
    if z.is_exact_zero() {
        return Some(one);
    }
    // DLMF 15.4.6: F(a, b; a; z) = (1 - z)^-b (an integer b by products: the transformations at integer parameter
    // differences gave 2F1(5, 6; 5; 4.5) an imaginary part 2e-42 outside its radius)
    for (x, y) in [(a, b), (b, a)] {
        if x.sub(cc, p + 16).is_exact_zero() {
            let base = one.sub(z, p + 16);
            return match int_of(y) {
                Some(n) if n.abs() <= 1 << 20 => cball::pow_int(&base, &mc_arith::num::Int::from_i64(-n), p),
                _ => cball::pow(&base, &y.neg(), p),
            };
        }
    }
    // a terminating series is a polynomial: any z
    if nonpos_int(a).is_some() || nonpos_int(b).is_some() {
        let (aa, bb) = ([a.clone(), b.clone()], [cc.clone()]);
        let g = guard_for(&aa, &bb, z);
        return refine(p, g, |wp| pfq_sum(&aa, &bb, z, wp));
    }
    let (zr, zi) = super::cseries::approx(z);
    let zm = (zr * zr + zi * zi).sqrt();
    let om = ((1.0 - zr).powi(2) + zi * zi).sqrt();
    // candidates: (modulus, index)
    let m = [
        zm,
        if om > 0.0 { zm / om } else { f64::INFINITY },
        if zm > 0.0 { 1.0 / zm } else { f64::INFINITY },
        om,
        if om > 0.0 { 1.0 / om } else { f64::INFINITY },
        if zm > 0.0 { om / zm } else { f64::INFINITY },
    ];
    let (mut k, mut best) = (0usize, f64::INFINITY);
    for (i, v) in m.iter().enumerate() {
        if *v < best {
            best = *v;
            k = i;
        }
    }
    if best >= 0.99 || !best.is_finite() {
        return None;
    }
    // the transformed functions need b - a / c - a - b not special: shift when integral
    let amb_int = dist_to_int(&b.sub(a, 64));
    let cab_int = dist_to_int(&cc.sub(a, 64).sub(b, 64));
    let degenerate = match k {
        2 | 4 => amb_int,
        3 | 5 => cab_int,
        _ => false,
    };
    let guard = 32 + if degenerate { p / 2 + 32 } else { 0 } + guard_for(&[a.clone(), b.clone()], std::slice::from_ref(cc), &CBall::real(Ball::exact(super::float_of(best))));
    let eval = |wp: u64, bb: &CBall, c2: &CBall| -> Option<CBall> {
        let one = c(1);
        match k {
            0 => pfq_sum(&[a.clone(), bb.clone()], std::slice::from_ref(c2), z, wp),
            1 => {
                // Pfaff: (1-z)^-a 2F1(a, c-b; c; z/(z-1))
                let w = z.div(&z.sub(&one, wp), wp)?;
                let f = pfq_sum(&[a.clone(), c2.sub(bb, wp)], std::slice::from_ref(c2), &w, wp)?;
                Some(pw(&one.sub(z, wp), &a.neg(), wp)?.mul(&f, wp))
            }
            2 => connection(a, bb, c2, &z.neg(), &one.div(z, wp)?, 0, wp),
            3 => connection_one_minus(a, bb, c2, z, 0, wp),
            4 => connection(a, bb, c2, &one.sub(z, wp), &one.div(&one.sub(z, wp), wp)?, 1, wp),
            _ => connection_one_minus(a, bb, c2, z, 1, wp),
        }
    };
    if degenerate && (!z.re.rad.is_zero() || !z.im.rad.is_zero()) {
        // a ball z: the shifted functions are of size 1/e and cancel only at the midpoint, so the radius of z, carried
        // through them, came back multiplied by 1/e = 2^(p/4 + 8) and grew with the precision asked for (2^102 at 811
        // bits for 2F1(2, 2; 5; z), round 28 gap). The value at the midpoint, widened by twice |F'(z)| rad(z) with
        // F' = (a b/c) 2F1(a + 1, b + 1; c + 1; z) at the midpoint (first order, an estimate as below)
        let mid = |x: &CBall| CBall::new(Ball::exact(x.re.mid.clone()), Ball::exact(x.im.mid.clone()));
        let zm = mid(z);
        let v = hyp2f1(a, b, cc, &zm, p + 8)?;
        let one = c(1);
        let w = 64;
        let d = hyp2f1(&a.add(&one, w), &b.add(&one, w), &cc.add(&one, w), &zm, w)?.mul(&a.mul(b, w).div(cc, w)?, w);
        let rz = super::mag_up(&Ball { mid: mc_arith::num::Float::zero(), rad: z.re.rad }).add(&super::mag_up(&Ball { mid: mc_arith::num::Float::zero(), rad: z.im.rad }));
        let m = rz.mul(&mag_of(&d)).mul_2exp(1);
        return Some(CBall::new(super::widen(v.re, &m), super::widen(v.im, &m)).round(p));
    }
    if degenerate && [a, b, cc].iter().any(|x| !x.re.rad.is_zero() || !x.im.rad.is_zero()) {
        // exact parameters reach here as balls of radius 2^-p (n of an exact call): a ball difference b - a that
        // contains the integer cannot be shifted by an e of that size, so the function is evaluated at the midpoints
        // and widened by 16 times the sum of the parameter radii times its modulus (an estimate, as the limit itself)
        let mid = |x: &CBall| CBall::new(Ball::exact(x.re.mid.clone()), Ball::exact(x.im.mid.clone()));
        let rad = |x: &CBall| super::mag_up(&Ball { mid: mc_arith::num::Float::zero(), rad: x.re.rad }).add(&super::mag_up(&Ball { mid: mc_arith::num::Float::zero(), rad: x.im.rad }));
        let r_in = rad(a).add(&rad(b)).add(&rad(cc));
        let v = hyp2f1(&mid(a), &mid(b), &mid(cc), z, p + 8)?;
        let m = r_in.mul_2exp(4).mul(&mag_of(&v));
        return Some(CBall::new(super::widen(v.re, &m), super::widen(v.im, &m)).round(p));
    }
    if degenerate {
        refine(p, guard, |wp| {
            extrapolate(p, wp, |e| {
                if k == 2 || k == 4 {
                    eval(wp, &b.add(e, wp), cc)
                } else {
                    eval(wp, b, &cc.add(e, wp))
                }
            })
        })
    } else {
        refine(p, guard, |wp| eval(wp, b, cc))
    }
}

// ------------------------------------------------------------------ integrals

/// Ei(z) = gamma + log z + z 2F2(1, 1; 2, 2; z) (z not zero).
pub fn exp_integral_ei(z: &CBall, p: u64) -> Option<CBall> {
    if z.contains_zero() {
        return None;
    }
    let (a, b) = ([c(1), c(1)], [c(2), c(2)]);
    let g = guard_for(&a, &b, z);
    refine(p, g, |wp| {
        let s = z.mul(&pfq_sum(&a, &b, z, wp)?, wp);
        Some(eg(wp)?.add(&cball::log(z, wp)?, wp).add(&s, wp))
    })
}

/// li(z) = Ei(log z).
pub fn log_integral(z: &CBall, p: u64) -> Option<CBall> {
    let w = p + 32;
    let l = cball::log(z, w)?;
    exp_integral_ei(&l, p)
}

fn z2_over_4(z: &CBall, sign: i64, wp: u64) -> CBall {
    z.square(wp).mul_real(&Ball::from_ratio(sign, 4, wp), wp)
}

/// Si (kind 0), Ci (1), Shi (2), Chi (3).
pub fn trig_integral(z: &CBall, kind: u8, p: u64) -> Option<CBall> {
    let odd = kind == 0 || kind == 2;
    if !odd && z.contains_zero() {
        return None;
    }
    let sign = if kind < 2 { -1 } else { 1 };
    let h = rat(1, 2, 64);
    let (a, b): (Vec<CBall>, Vec<CBall>) = if odd {
        (vec![h.clone()], vec![rat(3, 2, 64), rat(3, 2, 64)])
    } else {
        (vec![c(1), c(1)], vec![c(2), c(2), rat(3, 2, 64)])
    };
    let w0 = z2_over_4(z, sign, 128);
    let g = guard_for(&a, &b, &w0);
    refine(p, g, |wp| {
        let w = z2_over_4(z, sign, wp);
        let s = pfq_sum(&a, &b, &w, wp)?;
        if odd {
            Some(z.mul(&s, wp))
        } else {
            let t = w.mul(&s, wp);
            Some(eg(wp)?.add(&cball::log(z, wp)?, wp).add(&t, wp))
        }
    })
}

fn expneg(z: &CBall, wp: u64) -> Option<CBall> {
    cball::exp(&z.neg(), wp)
}

/// E_n(z) for an integer n.
fn exp_integral_e_int(n: i64, z: &CBall, wp: u64) -> Option<CBall> {
    let e = expneg(z, wp)?;
    let e0 = e.div(z, wp)?;
    if n == 0 {
        return Some(e0);
    }
    if n > 0 {
        let (a, b) = ([c(1), c(1)], [c(2), c(2)]);
        let s = z.mul(&pfq_sum(&a, &b, &z.neg(), wp)?, wp);
        let mut cur = eg(wp)?.neg().sub(&cball::log(z, wp)?, wp).add(&s, wp);
        for k in 1..n {
            // E_(k+1) = (e^-z - z E_k)/k
            cur = e.sub(&z.mul(&cur, wp), wp).mul_real(&Ball::from_ratio(1, k, wp), wp);
        }
        return Some(cur);
    }
    // downward: E_k = (e^-z - k E_(k+1))/z
    let mut cur = e0;
    let mut k = -1;
    while k >= n {
        cur = e.sub(&c(k).mul(&cur, wp), wp).div(z, wp)?;
        k -= 1;
    }
    Some(cur)
}

/// E_n(z) on complex balls.
pub fn exp_integral_e(n: &CBall, z: &CBall, p: u64) -> Option<CBall> {
    if z.contains_zero() {
        return None;
    }
    if let Some(k) = int_of(n) {
        if k.unsigned_abs() > 5000 {
            return None;
        }
        let g = guard_for(&[c(1), c(1)], &[c(2), c(2)], z) + 2 * k.unsigned_abs();
        return refine(p, g, |wp| exp_integral_e_int(k, z, wp));
    }
    // E_n(z) = z^(n-1) Gamma(1 - n, z)
    let s = c(1).sub(n, 128);
    let g = gamma_inc(&s, z, p + 16)?;
    let w = p + 32;
    Some(pw(z, &n.sub(&c(1), w), w)?.mul(&g, w).round(p))
}

// ------------------------------------------------------------------ incomplete gamma and beta

/// Gamma(a, z).
pub fn gamma_inc(a: &CBall, z: &CBall, p: u64) -> Option<CBall> {
    if z.contains_zero() {
        return None;
    }
    if let Some(m) = nonpos_int(a) {
        // Gamma(-m, z) = z^-m E_(m+1)(z)
        let w = p + 32;
        let e = exp_integral_e(&c(m as i64 + 1), z, w)?;
        return Some(pw(z, &c(-(m as i64)), w)?.mul(&e, w).round(p));
    }
    let one_a = [a.clone()];
    let ba = [a.add(&c(1), 64)];
    let g = guard_for(&one_a, &ba, &z.neg()) + 16;
    refine(p, g, |wp| {
        let f = pfq_sum(&one_a, &[a.add(&c(1), wp)], &z.neg(), wp)?;
        let lower = pw(z, a, wp)?.div(a, wp)?.mul(&f, wp);
        Some(cgamma(a, wp)?.sub(&lower, wp))
    })
}

/// Gamma(a, z)/Gamma(a).
pub fn gamma_regularized(a: &CBall, z: &CBall, p: u64) -> Option<CBall> {
    if nonpos_int(a).is_some() {
        return Some(c(0));
    }
    let w = p + 32;
    let g = gamma_inc(a, z, w)?;
    Some(g.mul(&rgamma(a, w)?, w).round(p))
}

/// B(a, b) = Gamma(a) Gamma(b)/Gamma(a + b), with the finite limits at poles that cancel.
pub fn beta(a: &CBall, b: &CBall, p: u64) -> Option<CBall> {
    let w = p + 32;
    // a negative integer a and a positive integer b: B = Gamma(b)/(a)_b
    for (x, y) in [(a, b), (b, a)] {
        if nonpos_int(x).is_some() {
            if let Some(k) = int_of(y).filter(|k| *k >= 1 && *k <= 100_000) {
                let mut prod = c(1);
                for i in 0..k {
                    prod = prod.mul(&x.add(&c(i), w), w);
                }
                let mut fact = c(1);
                for i in 2..k {
                    fact = fact.mul(&c(i), w);
                }
                return fact.div(&prod, w).map(|v| v.round(p));
            }
            return None;
        }
    }
    refine(p, 32, |wp| Some(cgamma(a, wp)?.mul(&cgamma(b, wp)?, wp).mul(&rgamma(&a.add(b, wp), wp)?, wp)))
}

/// B_z(a, b) = z^a/a 2F1(a, 1 - b; a + 1; z).
pub fn beta_inc(z: &CBall, a: &CBall, b: &CBall, p: u64) -> Option<CBall> {
    if nonpos_int(a).is_some() {
        return None;
    }
    let w = p + 32;
    let f = hyp2f1(a, &c(1).sub(b, w), &a.add(&c(1), w), z, w)?;
    Some(pw(z, a, w)?.div(a, w)?.mul(&f, w).round(p))
}

/// I_z(a, b) = B_z(a, b)/B(a, b).
pub fn beta_regularized(z: &CBall, a: &CBall, b: &CBall, p: u64) -> Option<CBall> {
    let w = p + 32;
    let bz = beta_inc(z, a, b, w)?;
    let bb = beta(a, b, w)?;
    bz.div(&bb, w).map(|v| v.round(p))
}

// ------------------------------------------------------------------ complete elliptic integrals

/// K(m) = pi/2 2F1(1/2, 1/2; 1; m). A real m below 1 is the real function R_F(0, 1 - m, 1) of
/// [`super::elliptic`]: the connection formulas of 2F1 at c - a - b = 0 are evaluated at shifted parameters and
/// amplify the radius of a ball argument by 2^(p/4), so the radius grew with the working precision and every
/// Jacobi function of a ball took seconds (round 24).
pub fn elliptic_k(m: &CBall, p: u64) -> Option<CBall> {
    if let Some(r) = real_below_one(m) {
        return super::elliptic::elliptic_k(&mc_arith::Real::Ball(r, p + 16), p);
    }
    let w = p + 32;
    let f = hyp2f1(&rat(1, 2, w), &rat(1, 2, w), &c(1), m, w)?;
    Some(f.mul_real(&half_pi(w)?, w).round(p))
}

/// The real ball `m` when it is real with every point below 1.
fn real_below_one(m: &CBall) -> Option<Ball> {
    if !(m.im.rad.is_zero() && m.im.mid.is_zero()) {
        return None;
    }
    (super::upper(&m.re).cmp_exact(&mc_arith::num::Float::from_i64(1)) == std::cmp::Ordering::Less).then(|| m.re.clone())
}

/// E(m) = pi/2 2F1(-1/2, 1/2; 1; m) (real m below 1: the real function of [`super::elliptic`]).
pub fn elliptic_e(m: &CBall, p: u64) -> Option<CBall> {
    if let Some(r) = real_below_one(m) {
        return super::elliptic::elliptic_e(&mc_arith::Real::Ball(r, p + 16), p);
    }
    let w = p + 32;
    let f = hyp2f1(&rat(-1, 2, w), &rat(1, 2, w), &c(1), m, w)?;
    Some(f.mul_real(&half_pi(w)?, w).round(p))
}

fn half_pi(wp: u64) -> Option<Ball> {
    Some(pi(wp)?.mul(&Ball::from_ratio(1, 2, wp), wp))
}

// ------------------------------------------------------------------ Bessel Y and K of integer order

fn mag_up_c(z: &CBall) -> Mag {
    super::mag_up(&z.re).add(&super::mag_up(&z.im))
}

fn mag_f(v: f64) -> Mag {
    Mag::from_float_up(&super::float_of(v * (1.0 + 1e-9) + f64::MIN_POSITIVE))
}

/// The sum over k >= 0 of (H_k + H_(n+k) - 2 gamma) w^k/(k! (n+k)!) with a rigorous tail bound.
fn digamma_series(n: u64, w: &CBall, wp: u64) -> Option<CBall> {
    let two_g = eg(wp)?.mul_real(&Ball::from_i64(2), wp);
    let wa = super::cseries::up(w);
    let mut fact_n = Ball::from_i64(1);
    for i in 2..=n {
        fact_n = fact_n.mul(&Ball::from_i64(i as i64), wp);
    }
    // a_0 = 1/n!
    let mut a = c(1).div(&CBall::real(fact_n), wp)?;
    let mut hk = Ball::from_i64(0);
    let mut hnk = Ball::from_i64(0);
    for i in 1..=n {
        hnk = hnk.add(&Ball::from_ratio(1, i as i64, wp), wp);
    }
    let mut sum = c(0);
    let mut k: u64 = 0;
    loop {
        if mc_arith::num::stopped() || k > 1_000_000 {
            return None;
        }
        let hsum = CBall::real(hk.add(&hnk, wp)).sub(&two_g, wp);
        sum = sum.add(&a.mul(&hsum, wp), wp);
        // ratio to the next term
        let r = wa / (((k + 1) * (n + k + 1)) as f64);
        let next = a.mul(w, wp).mul_real(&Ball::from_ratio(1, ((k + 1) * (n + k + 1)) as i64, wp), wp);
        let log_at = super::cseries::log2_up(&a);
        if r <= 0.25 && log_at + r.log2() < super::cseries::log2_up(&sum).max(-(wp as f64)) - (wp as f64) - 4.0 {
            let hb = 4.0 * ((n + 2 * k + 10) as f64).ln() + 4.0;
            let m = mag_up_c(&a).mul(&mag_f(hb * r));
            return Some(CBall::new(super::widen(sum.re, &m), super::widen(sum.im, &m)));
        }
        a = next;
        k += 1;
        hk = hk.add(&Ball::from_ratio(1, k as i64, wp), wp);
        hnk = hnk.add(&Ball::from_ratio(1, (n + k) as i64, wp), wp);
    }
}

/// The finite sum over k < n of (n - k - 1)!/k! w^k.
fn finite_sum(n: u64, w: &CBall, wp: u64) -> CBall {
    let mut total = c(0);
    let mut wk = c(1);
    let mut fk = Ball::from_i64(1); // k!
    let mut fnk = Ball::from_i64(1); // (n-k-1)!
    for i in 2..n {
        fnk = fnk.mul(&Ball::from_i64(i as i64), wp);
    }
    for k in 0..n {
        let coef = fnk.div(&fk, wp).unwrap_or_else(|| Ball::from_i64(0));
        total = total.add(&wk.mul_real(&coef, wp), wp);
        wk = wk.mul(w, wp);
        fk = fk.mul(&Ball::from_i64((k + 1) as i64), wp);
        // (n-k-2)! = (n-k-1)!/(n-k-1)
        if n >= k + 2 {
            fnk = fnk.div(&Ball::from_i64((n - k - 1) as i64), wp).unwrap_or_else(|| Ball::from_i64(1));
        }
    }
    total
}

/// Y_n(z) (kind 0) or K_n(z) (kind 1) for an integer order, DLMF 10.8.1 and 10.31.1.
pub fn bessel_yk_int(kind: u8, order: i64, z: &CBall, p: u64) -> Option<CBall> {
    if z.contains_zero() {
        return None;
    }
    let n = order.unsigned_abs();
    if n > 2000 || super::cseries::up(z) > 1e4 {
        return None;
    }
    let sgn = if kind == 0 && order < 0 && n % 2 == 1 { -1 } else { 1 };
    let guard = 40 + 3 * super::cseries::up(z) as u64 + n;
    let r = refine(p, guard, |wp| {
        let hz = z.mul_real(&Ball::from_ratio(1, 2, wp), wp);
        let q = z.square(wp).mul_real(&Ball::from_ratio(1, 4, wp), wp);
        let lg = cball::log(&hz, wp)?;
        let pn = pw(&hz, &c(n as i64), wp)?;
        let mn = pw(&hz, &c(-(n as i64)), wp)?;
        let pi_w = pi(wp)?;
        if kind == 0 {
            let w = q.neg();
            let jn = super::cseries::bessel_j(&c(n as i64), z, wp)?;
            let t1 = mn.mul(&finite_sum(n, &q, wp), wp).neg();
            let t2 = lg.mul(&jn, wp).mul_real(&Ball::from_i64(2), wp);
            let t3 = pn.mul(&digamma_series(n, &w, wp)?, wp).neg();
            let tot = t1.add(&t2, wp).add(&t3, wp);
            let inv_pi = Ball::from_i64(1).div(&pi_w, wp)?;
            Some(tot.mul_real(&inv_pi, wp))
        } else {
            let w = q.clone();
            let inn = super::cseries::bessel_i(&c(n as i64), z, wp)?;
            let t1 = mn.mul(&finite_sum(n, &q.neg(), wp), wp).mul_real(&Ball::from_ratio(1, 2, wp), wp);
            let sgn_n = if n.is_multiple_of(2) { -1 } else { 1 };
            let t2 = lg.mul(&inn, wp).mul_real(&Ball::from_i64(sgn_n), wp);
            let sg2 = if n.is_multiple_of(2) { 1 } else { -1 };
            let t3 = pn.mul(&digamma_series(n, &w, wp)?, wp).mul_real(&Ball::from_ratio(sg2, 2, wp), wp);
            Some(t1.add(&t2, wp).add(&t3, wp))
        }
    })?;
    Some(if sgn < 0 { r.neg() } else { r })
}

// ------------------------------------------------------------------ polylogarithm

fn bernoulli_poly_at(s: u64, x: &CBall, wp: u64) -> Option<CBall> {
    // B_s(x) = sum_k C(s, k) B_k x^(s-k), B_1 = -1/2
    let mut total = c(0);
    let mut binom = Ball::from_i64(1);
    for k in 0..=s {
        let bk = if k == 1 { mc_arith::num::Rat::new(mc_arith::num::Int::from_i64(-1), mc_arith::num::Int::from_i64(2)) } else { crate::bernoulli::bernoulli(k)? };
        let bkb = super::rat_ball(&bk, wp);
        let term = pw(x, &c((s - k) as i64), wp)?.mul_real(&binom.mul(&bkb, wp), wp);
        total = total.add(&term, wp);
        // C(s, k + 1) = C(s, k) (s - k)/(k + 1)
        if k < s {
            binom = binom.mul(&Ball::from_ratio((s - k) as i64, (k + 1) as i64, wp), wp);
        }
    }
    Some(total)
}

/// The series sum_k z^k/k^s for |z| < 1 with the tail bound |z|^K / ((1 - |z|) K^(Re s)) (Re s >= 0).
fn polylog_series(s: &CBall, z: &CBall, wp: u64) -> Option<CBall> {
    let za = super::cseries::up(z);
    if za >= 0.95 {
        return None;
    }
    let (sr, _) = super::cseries::approx(s);
    if sr < 0.0 {
        return None;
    }
    let target = wp as f64 + 6.0;
    let mut zk = z.clone();
    let mut sum = c(0);
    let mut k: u64 = 1;
    loop {
        if mc_arith::num::stopped() || k > 200_000 {
            return None;
        }
        let ks = match int_of(s) {
            Some(n) => CBall::real(Ball::from_i64(k as i64)).pow_check(n, wp)?,
            None => cball::exp(&cball::log(&c(k as i64), wp)?.mul(s, wp).neg(), wp)?,
        };
        sum = sum.add(&zk.mul(&ks, wp), wp);
        // tail after k terms: |z|^(k+1)/(1 - |z|)
        let tail_log2 = za.log2() * (k as f64 + 1.0) - (1.0 - za).log2();
        if tail_log2 < super::cseries::log2_up(&sum).max(-(wp as f64)) - target {
            let m = Mag::pow2(tail_log2.ceil() as i64 + 1);
            return Some(CBall::new(super::widen(sum.re, &m), super::widen(sum.im, &m)));
        }
        zk = zk.mul(z, wp);
        k += 1;
    }
}

trait PowCheck {
    fn pow_check(&self, n: i64, wp: u64) -> Option<CBall>;
}
impl PowCheck for CBall {
    /// 1/self^n.
    fn pow_check(&self, n: i64, wp: u64) -> Option<CBall> {
        cball::pow_int(self, &mc_arith::num::Int::from_i64(-n), wp)
    }
}

/// Li_s(z) on complex balls: the series for |z| < 0.95; for an integer order s >= 1 and |z| > 1.1 the inversion
/// formula Li_s(z) + (-1)^s Li_s(1/z) = -(2 pi i)^s/s! B_s(1/2 + log(-z)/(2 pi i)) (DLMF 25.12.12 with the
/// Bernoulli polynomial of 25.12.13 forms; Abramowitz and Stegun 27.7). The region 0.95 <= |z| <= 1.1 is not covered.
pub fn polylog(s: &CBall, z: &CBall, p: u64) -> Option<CBall> {
    if z.is_exact_zero() {
        return Some(c(0));
    }
    let za = super::cseries::approx(z);
    let zm = za.0.hypot(za.1);
    let int_s = int_of(s);
    // a negative or zero integer order is the rational function z A_n(z)/(1 - z)^(n+1) for every z != 1
    if let Some(n) = int_s.filter(|n| *n <= 0) {
        return refine(p, 24 + 2 * n.unsigned_abs(), |wp| super::polylog::eulerian_c(n.unsigned_abs(), z, wp));
    }
    let sint = int_s.filter(|n| *n >= 1 && *n <= 60);
    // a non-integer real order: the series inside the unit disc, the expansion about z = 1 elsewhere (|log z| <= 4.5);
    // z real above 1 is the limit from below (as the integer orders)
    let frac = s.is_real() && int_s.is_none();
    let on_cut = z.im.rad.is_zero() && z.im.mid.is_zero() && za.0 > 1.0;
    let guard = 24 + if zm > 1.0 { 3 * sint.unwrap_or(1) as u64 } else { 0 } + if frac { 24 } else { 0 };
    refine(p, guard, |wp| {
        if zm < 0.9 {
            if let Some(v) = polylog_series(s, z, wp) {
                return Some(v);
            }
            return if frac && zm > 0.1 { super::polylog::frac_point_c(&s.re, z, on_cut, wp) } else { None };
        }
        if frac {
            return super::polylog::frac_point_c(&s.re, z, on_cut, wp);
        }
        let n = sint? as u64;
        if zm <= 1.1 {
            return None;
        }
        let inv = c(1).div(z, wp)?;
        let li = polylog_series(s, &inv, wp)?;
        let sign = if n.is_multiple_of(2) { 1 } else { -1 };
        let two_pi_i = pi(wp)?.mul(&Ball::from_i64(2), wp);
        let tpi = CBall::new(Ball::from_i64(0), two_pi_i);
        let x = c(0).sub(&cball::log(&z.neg(), wp)?.div(&tpi, wp)?.neg(), wp).add(&rat(1, 2, wp), wp);
        let bp = bernoulli_poly_at(n, &x, wp)?;
        let mut fact = Ball::from_i64(1);
        for i in 2..=n {
            fact = fact.mul(&Ball::from_i64(i as i64), wp);
        }
        let pre = pw(&tpi, &c(n as i64), wp)?.div(&CBall::real(fact), wp)?;
        let rhs = pre.mul(&bp, wp).neg();
        Some(rhs.sub(&li.mul_real(&Ball::from_i64(sign), wp), wp))
    })
}

// ------------------------------------------------------------------ Jacobi elliptic functions

/// The integer nearest to a float (halves up in magnitude).
pub(crate) fn nearest_int(f: &mc_arith::num::Float) -> mc_arith::num::Int {
    use mc_arith::num::{Int, Nat};
    if f.is_zero() {
        return Int::from_i64(0);
    }
    if f.exp >= 0 {
        return Int::new(f.neg, f.man.shl(f.exp as u64));
    }
    let s = (-f.exp) as u64;
    let half = f.man.shr(s - 1).low_bits_nonzero(1);
    let mut q = f.man.shr(s);
    if half {
        q = q.add(&Nat::from_u64(1));
    }
    Int::new(f.neg, q)
}

/// sn, cn, dn (kind 0, 1, 2) of u with parameter m on complex balls, through the theta functions of the nome
/// q = exp(-pi K'(m)/K(m)) (DLMF 22.2.4-6 with 20.2.1-4 and 22.2.1): sn = theta3(0) theta1(z)/(theta2(0) theta4(z)),
/// cn = theta4(0) theta2(z)/(theta2(0) theta4(z)), dn = theta4(0) theta3(z)/(theta3(0) theta4(z)), z = pi u/(2 K).
/// The theta series are summed with a rigorous geometric tail; `None` when |q| >= 0.9.
pub fn jacobi(kind: u8, u: &CBall, m: &CBall, p: u64) -> Option<CBall> {
    // the guard covers the cancellation of the theta sums (|Im u|) and, for a real part beyond the period, the bits
    // the reduction modulo 2K takes (log2 |Re u|); it was 3 |u|, which is 3e20 extra bits for u = 1e20 (D23-22)
    let ua = super::cseries::up(u);
    let guard = 40 + (3.0 * ua.min(64.0)) as u64 + ua.max(1.0).log2() as u64;
    refine(p, guard, |wp| {
        let (k, k1) = (elliptic_k(m, wp + 16)?, elliptic_k(&c(1).sub(m, wp + 16), wp + 16)?);
        // sn(u + 2K) = -sn(u), cn(u + 2K) = -cn(u), dn(u + 2K) = dn(u) (DLMF 22.4.1-3): reduce the real part of u
        // modulo 2K, with K at the bits the reduction consumes
        let mut flip = false;
        let mut u_eff = u.clone();
        let (ur, kr) = (super::cseries::approx(u).0, super::cseries::approx(&k).0);
        if k.is_real() && m.is_real() && kr > 0.0 && ur.is_finite() && ur.abs() > 4.0 * kr {
            let ratio = u.re.div(&k.re.mul(&Ball::from_i64(2), wp + 16), wp + 16)?;
            let n = nearest_int(&ratio.mid);
            let extra = n.mag.bits() + 16;
            let kb = elliptic_k(m, wp + 16 + extra)?;
            let nb = if n.neg { Ball::from_nat(n.mag.clone()).neg() } else { Ball::from_nat(n.mag.clone()) };
            let shift = kb.mul_real(&nb.mul(&Ball::from_i64(2), wp + 16 + extra), wp + 16 + extra);
            u_eff = u.sub(&shift, wp + 16 + extra).round(wp + 16);
            flip = n.mag.low_bits_nonzero(1);
        }
        let u = &u_eff;
        let pi_w = pi(wp)?;
        let logq = k1.div(&k, wp)?.mul_real(&pi_w, wp).neg();
        let (lq_re, _) = super::cseries::approx(&logq);
        if lq_re > -0.12 {
            return None; // |q| >= 0.89
        }
        let z = u.mul_real(&pi_w, wp).div(&k.mul_real(&Ball::from_i64(2), wp), wp)?;
        let (_, zim) = super::cseries::approx(&z);
        let zi_abs = zim.abs() + super::cseries::up(&z) * 1e-9;
        // number of terms: |q|^(n^2) e^(2 n |Im z|) < 2^-(wp + 8) and the ratio below 1/2
        let l2q = lq_re / std::f64::consts::LN_2; // log2 |q| (negative)
        let mut nmax = 1u64;
        loop {
            let nf = nmax as f64;
            let lt = l2q * nf * nf + 2.0 * nf * zi_abs / std::f64::consts::LN_2;
            let ratio = l2q * (2.0 * nf + 1.0) + 2.0 * zi_abs / std::f64::consts::LN_2;
            if lt < -(wp as f64) - 12.0 && ratio < -1.0 {
                break;
            }
            nmax += 1;
            if nmax > 100_000 {
                return None;
            }
        }
        nmax += 1;
        let tail_log2 = l2q * (nmax * nmax) as f64 + 2.0 * nmax as f64 * zi_abs / std::f64::consts::LN_2 + 2.0;
        let tail = Mag::pow2(tail_log2.ceil() as i64);
        let two = Ball::from_i64(2);
        let theta = |which: u8, zz: &CBall| -> Option<CBall> {
            // which: 1..4 at the argument zz
            let mut sum = if which >= 3 { c(1) } else { c(0) };
            for n in 0..=nmax {
                let nf = n as i64;
                let (expn, arg_mult) = if which <= 2 { ((2 * nf + 1) * (2 * nf + 1), 2 * nf + 1) } else { (nf * nf, 2 * nf) };
                if which >= 3 && n == 0 {
                    continue;
                }
                // q^(expn) for exponent expn (quarter for the half-integer series): q^((n+1/2)^2) = exp(expn/4 logq)
                let pe = if which <= 2 { logq.mul_real(&Ball::from_ratio(expn, 4, wp), wp) } else { logq.mul_real(&Ball::from_i64(expn), wp) };
                let qn = cball::exp(&pe, wp)?;
                let ang = zz.mul_real(&Ball::from_i64(arg_mult), wp);
                let tr = if which == 1 { cball::sin(&ang, wp)? } else { cball::cos(&ang, wp)? };
                let mut term = qn.mul(&tr, wp);
                if (which == 1 && n % 2 == 1) || (which == 4 && n % 2 == 1) {
                    term = term.neg();
                }
                sum = sum.add(&term.mul_real(&two, wp), wp);
            }
            Some(CBall::new(super::widen(sum.re, &tail), super::widen(sum.im, &tail)))
        };
        let zero = c(0);
        let t4z = theta(4, &z)?;
        let res = match kind {
            0 => {
                let r = theta(3, &zero)?.mul(&theta(1, &z)?, wp);
                r.div(&theta(2, &zero)?.mul(&t4z, wp), wp)
            }
            1 => {
                let r = theta(4, &zero)?.mul(&theta(2, &z)?, wp);
                r.div(&theta(2, &zero)?.mul(&t4z, wp), wp)
            }
            _ => {
                let r = theta(4, &zero)?.mul(&theta(3, &z)?, wp);
                r.div(&theta(3, &zero)?.mul(&t4z, wp), wp)
            }
        };
        res.map(|r| if flip && kind < 2 { r.neg() } else { r })
    })
}
